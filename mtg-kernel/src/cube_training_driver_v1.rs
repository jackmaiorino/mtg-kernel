//! Training segments, checkpoints, league snapshots and durable resume for M4.

pub use crate::cube_draft_build_v1::card_index::CubeTableV1;
use crate::cube_draft_build_v1::model::training::{HeadUpdates, TrainingStatistics};
use crate::cube_draft_build_v1::model::{training::Optimizer, ModelCardRowsV1, ModelV1};
#[path = "cube_training_driver_model_v1.rs"]
mod model;
use model::update_model;
#[path = "cube_training_tripwire_v1.rs"]
mod tripwire;
use tripwire::{tripwire_window_sizes, CensusRates, TripwireTracker};
pub use tripwire::{CensusBaselineFiles, CensusBaselinePin};
#[path = "cube_training_resume_v1.rs"]
mod resume;
pub use resume::ResumeAuthorization;
use resume::{check_resume, RunLock, RunnerMarker};

use crate::cube_draft_v1::{uniform_index, CubePool, SEATS};
use crate::cube_learned_seat_v1::{
    LearnedSeatBuilder, LearnedSeatPicker, ModelSeatForward, SeatForward,
};
use crate::cube_pod_runner_v1::{
    is_watchdog_end, pod_league_seed, training_roles_with_older, PilotInfo, PilotKind, PodPins,
    PodRecord, PodRunnerError, PodStatus, PodUse, SeatKind, SeatRef, StopAttemptReport,
    TrainingChair, TrainingPodConfig, CLIENT_NAMESPACE,
};
use crate::cube_read_plan_v1::sha256_hex;
use crate::cube_seat_v1::{
    GamePilot, UniformPilot, UniformSeatPicker, LEARNED_SAMPLER_ID, M4_INTERFACES_VERSION,
};
use crate::cube_worker_liveness_v1::LIVENESS;
use crate::durable_publication_v1::{capture_existing_publication_parent_v1, publish_new_bytes_v1};
use crate::state::SplitMix64;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const RUN_SCHEMA: &str = "mtg-kernel-cube-draft-build-run/v1";
pub const CHECKPOINT_SCHEMA: &str = "mtg-kernel-cube-draft-build-checkpoint/v1";
pub const STOP_SCHEMA: &str = "mtg-kernel-cube-training-stop/v1";
pub const PODS_PER_UPDATE: u64 = 16;
pub const TRIPWIRE_WINDOW_GAMES: u64 = 2_000;
pub const TRIPWIRE_TAIL_MERGE_GAMES: u64 = 500;

static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, PartialEq, Eq)]
pub enum TrainingDriverError {
    Io(String),
    Pod(PodRunnerError),
    Invalid(String),
    Update(String),
    StoppedWithoutResolution,
    StoppedRequiresRestart,
    RunAlreadyActive,
    ResumeAuthorizationRequired,
    PinnedCheckpoint,
}

impl fmt::Display for TrainingDriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "training store I/O error: {message}"),
            Self::Pod(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid training configuration: {message}"),
            Self::Update(message) => write!(f, "update failed closed: {message}"),
            Self::StoppedWithoutResolution => {
                write!(f, "the stop ledger has no resolution record")
            }
            Self::RunAlreadyActive => write!(f, "another driver holds the training run lock"),
            Self::ResumeAuthorizationRequired => {
                write!(
                    f,
                    "an unrecorded exit requires a pinned shakedown manifest resume entry"
                )
            }
            Self::PinnedCheckpoint => write!(f, "cannot prune a pinned checkpoint"),
            Self::StoppedRequiresRestart => {
                write!(
                    f,
                    "a stopped run id requires a resolved restart under a new run id"
                )
            }
        }
    }
}

impl std::error::Error for TrainingDriverError {}

impl From<PodRunnerError> for TrainingDriverError {
    fn from(error: PodRunnerError) -> Self {
        Self::Pod(error)
    }
}

impl From<std::io::Error> for TrainingDriverError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for TrainingDriverError {
    fn from(error: serde_json::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

fn io_error(error: impl fmt::Display) -> TrainingDriverError {
    TrainingDriverError::Io(error.to_string())
}

fn invalid(error: impl Into<String>) -> TrainingDriverError {
    TrainingDriverError::Invalid(error.into())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainSets {
    pub build: bool,
    pub pick: bool,
    pub value: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Segment {
    BuilderOnly,
    PickPhase,
}

impl Segment {
    pub const fn train_sets(self) -> TrainSets {
        match self {
            Self::BuilderOnly => TrainSets {
                build: true,
                pick: false,
                value: false,
            },
            Self::PickPhase => TrainSets {
                build: true,
                pick: true,
                value: true,
            },
        }
    }

    pub const fn run_name(self) -> &'static str {
        match self {
            Self::BuilderOnly => "b1",
            Self::PickPhase => "k1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageSegment {
    pub segment: Segment,
    pub run: String,
    pub pilots: Vec<PilotKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotPin {
    pub checkpoint_sha256: String,
    pub after_state_sha256: String,
    pub card_database_sha256: String,
    pub table_sha256: String,
    pub training_math: String,
    pub source_binary_sha256: String,
    pub decoding: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PilotSelection {
    Uniform,
    Checkpoint { pin: PilotPin },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotSwitch {
    pub update: u64,
    pub gate: Option<PilotPin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotSchedule {
    pub initial: PilotSelection,
    pub switch: Option<PilotSwitch>,
}

impl PilotSchedule {
    pub fn uniform() -> Self {
        Self {
            initial: PilotSelection::Uniform,
            switch: None,
        }
    }

    fn at(&self, update: u64) -> Option<PilotSelection> {
        if let Some(switch) = &self.switch {
            if update >= switch.update {
                return switch
                    .gate
                    .as_ref()
                    .map(|pin| PilotSelection::Checkpoint { pin: pin.clone() });
            }
        }
        Some(self.initial.clone())
    }

    fn compatible_resume(&self, resumed: &Self) -> bool {
        if self.initial != resumed.initial
            || self.switch.as_ref().map(|s| s.update) != resumed.switch.as_ref().map(|s| s.update)
        {
            return false;
        }
        true
    }
}

pub trait PilotProvider {
    /// A launcher can supervise the same uniform policy in a worker process.
    fn uniform_pilot(&self) -> Arc<dyn GamePilot + Sync> {
        Arc::new(UniformPilot)
    }

    fn load_checkpoint(
        &self,
        pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError>;
}

pub struct UniformOnlyPilotProvider;

impl PilotProvider for UniformOnlyPilotProvider {
    fn load_checkpoint(
        &self,
        _pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        Err(invalid("a checkpoint pilot requires the D9 adapter"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case", deny_unknown_fields)]
pub enum TripwireRule {
    StopKind {
        stop_kind: String,
        min_games: u64,
        rate_threshold_bits: Option<u64>,
    },
    Card {
        card: u16,
        min_games: u64,
        rate_threshold_bits: Option<u64>,
    },
    ProcessDeath,
    Hang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TripwireMode {
    Q9,
    Full { baseline: CensusBaselinePin },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointMetadata {
    pub model_identity: String,
    pub parameter_layout_sha256: String,
    pub card_database_sha256: String,
    pub column_scales: Vec<u32>,
    pub dense_fields: Vec<String>,
    pub training_math: String,
    pub sampler: String,
    pub constants: BTreeMap<String, u64>,
}

impl Default for CheckpointMetadata {
    fn default() -> Self {
        Self {
            model_identity: "m4-cube-draft-build/v1".to_owned(),
            parameter_layout_sha256: String::new(),
            card_database_sha256: String::new(),
            column_scales: Vec::new(),
            dense_fields: Vec::new(),
            training_math: "portable-math-v1+m4-ln-atanh23-f64-v1".to_owned(),
            sampler: LEARNED_SAMPLER_ID.to_owned(),
            constants: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointMarks {
    pub dose_reads: Vec<u64>,
    pub owner_views: Vec<u64>,
    pub read_candidates: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PickBoundary {
    pub builder_run: String,
    pub builder_final_state_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundaryRecord {
    config: RunConfig,
    checkpoint: Checkpoint,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenIntent {
    #[default]
    Run,
    PrepareFresh,
    ReadCompletedBuilder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    #[serde(skip)]
    pub open_intent: OpenIntent,
    #[serde(skip)]
    pub root: PathBuf,
    pub schema: String,
    pub interfaces: String,
    pub label: String,
    pub namespace: String,
    pub run: String,
    pub segment: Segment,
    pub update_range: [u64; 2],
    pub pods_per_update: u64,
    pub train_sets: TrainSets,
    pub pilot: PilotSchedule,
    pub tripwire: TripwireMode,
    /// Local evidence for the census pins recorded in run.json.
    #[serde(skip)]
    pub tripwire_files: Option<CensusBaselineFiles>,
    /// Launcher-provided D0.3b proof for this open, outside the immutable run plan.
    #[serde(skip)]
    pub resume: Option<ResumeAuthorization>,
    #[serde(default)]
    pub tripwire_window_games: Vec<u64>,
    pub pins: PodPins,
    /// Immutable launcher input identities which have no field in the pod wire format.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub launch_pins: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_prefix_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_profile: Option<String>,
    pub metadata: CheckpointMetadata,
    pub caps: crate::cube_pod_runner_v1::PodCaps,
    pub workers: usize,
    pub liveness: String,
    pub marks: CheckpointMarks,
    pub boundary: Option<PickBoundary>,
}

impl RunConfig {
    /// Read-only launcher preflight. The open intent is rechecked under runner.lock.
    pub fn preflight_open(&self) -> Result<(), TrainingDriverError> {
        self.check_open_intent(false)
    }
    /// A prepare never recovers, consumes a resume entry or records a death.
    fn check_open_intent(&self, locked: bool) -> Result<(), TrainingDriverError> {
        match self.open_intent {
            OpenIntent::PrepareFresh if self.root.try_exists()? => {
                for entry in fs::read_dir(&self.root)? {
                    if !locked || entry?.file_name() != "runner.lock" {
                        return Err(invalid("prepare requires a fresh segment directory"));
                    }
                }
            }
            OpenIntent::ReadCompletedBuilder => {
                if !self.root.join("run.json").is_file()
                    || !self.root.join("runner.lock").is_file()
                    || self.root.join("runner-active.json").try_exists()?
                    || !update_directories(&self.root.join("pending"))?.is_empty()
                {
                    return Err(invalid("builder is missing, active or interrupted"));
                }
                check_stop_ledger(&self.root)?;
                // D6 validates every commit once the clean predecessor is open.
                let end = self
                    .total_updates()
                    .checked_sub(1)
                    .ok_or_else(|| invalid("empty builder segment"))?;
                if !self
                    .root
                    .join("updates")
                    .join(update_name(end))
                    .join("commit.json")
                    .is_file()
                {
                    return Err(invalid(
                        "the pick phase requires a completed builder segment",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn shakedown(root: impl Into<PathBuf>, run: impl Into<String>, segment: Segment) -> Self {
        let train_sets = segment.train_sets();
        Self {
            open_intent: OpenIntent::Run,
            root: root.into(),
            schema: RUN_SCHEMA.to_owned(),
            interfaces: M4_INTERFACES_VERSION.to_owned(),
            label: "shakedown".to_owned(),
            namespace: "shakedown-pods".to_owned(),
            run: run.into(),
            segment,
            update_range: [0, 128],
            pods_per_update: PODS_PER_UPDATE,
            train_sets,
            pilot: PilotSchedule::uniform(),
            tripwire: TripwireMode::Q9,
            tripwire_files: None,
            resume: None,
            tripwire_window_games: Vec::new(),
            pins: PodPins::default(),
            launch_pins: BTreeMap::new(),
            manifest_prefix_sha256: None,
            platform: None,
            host: None,
            build_profile: None,
            metadata: CheckpointMetadata::default(),
            caps: crate::cube_pod_runner_v1::PodCaps::training_a2(),
            workers: 1,
            liveness: LIVENESS.to_owned(),
            // A6.17's four b1 dose marks and D0.21's six owner views.
            marks: match segment {
                Segment::BuilderOnly => CheckpointMarks {
                    dose_reads: vec![32, 64, 96, 128],
                    owner_views: vec![32, 64, 96, 128],
                    read_candidates: Vec::new(),
                },
                Segment::PickPhase => CheckpointMarks {
                    owner_views: vec![64, 128],
                    ..CheckpointMarks::default()
                },
            },
            boundary: None,
        }
    }

    fn validate(&self) -> Result<(), TrainingDriverError> {
        if self.schema != RUN_SCHEMA || self.interfaces != M4_INTERFACES_VERSION {
            return Err(invalid("run schema or interface version differs"));
        }
        if self.namespace == CLIENT_NAMESPACE
            || !crate::cube_pod_runner_v1::is_valid_token(&self.namespace)
            || !crate::cube_pod_runner_v1::is_valid_token(&self.run)
        {
            return Err(invalid("invalid training namespace or run id"));
        }
        if self.update_range != [0, self.update_range[1]]
            || self.update_range[1] == 0
            || self.pods_per_update != PODS_PER_UPDATE
            || self.train_sets != self.segment.train_sets()
            || Some(self.caps.max_stack_objects)
                != crate::cube_b_census_v1::A2_CAPS.max_stack_objects
            || self.workers == 0
        {
            return Err(invalid("invalid update plan or trained sets"));
        }
        if (self.label == "shakedown") != (self.namespace == "shakedown-pods") {
            return Err(invalid("the shakedown label and namespace must agree"));
        }
        if self.liveness != LIVENESS
            || self
                .pilot
                .switch
                .as_ref()
                .is_some_and(|switch| switch.update != 0)
        {
            return Err(invalid(
                "invalid watchdog liveness or nonboundary pilot switch",
            ));
        }
        if self.label != "shakedown" && matches!(self.pilot.initial, PilotSelection::Uniform) {
            return Err(invalid(
                "a non-shakedown run cannot start from a uniform-pilot lineage",
            ));
        }
        Ok(())
    }

    fn total_updates(&self) -> u64 {
        self.update_range[1] - self.update_range[0]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterSetState {
    pub parameters: Vec<u32>,
    pub first_moment: Vec<u32>,
    pub second_moment: Vec<u32>,
    pub adam_step: u64,
}

impl ParameterSetState {
    fn reset_optimizer(&mut self) {
        self.first_moment.fill(0);
        self.second_moment.fill(0);
        self.adam_step = 0;
    }

    fn validate(&self) -> Result<(), TrainingDriverError> {
        if self.parameters.len() != self.first_moment.len()
            || self.parameters.len() != self.second_moment.len()
        {
            return Err(invalid("parameter and Adam tensor lengths differ"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CubeModelState {
    pub build: ParameterSetState,
    pub pick: ParameterSetState,
    pub value: ParameterSetState,
}

impl CubeModelState {
    fn validate(&self) -> Result<(), TrainingDriverError> {
        self.build.validate()?;
        self.pick.validate()?;
        self.value.validate()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizerOutput {
    pub losses: BTreeMap<String, f64>,
    pub gradient_norms: BTreeMap<String, f64>,
    pub statistics: TrainingStatistics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckpointPinReason {
    League,
    SegmentStart,
    SegmentEnd,
    DoseRead,
    OwnerView,
    ReadCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeagueEntry {
    pub pin: String,
    pub path: String,
    pub segment: Segment,
    pub update: u64,
    pub head_updates: HeadUpdates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema: String,
    pub model_identity: String,
    pub parameter_layout_sha256: String,
    pub card_index_sha256: Option<String>,
    pub cube_table_sha256: Option<String>,
    pub card_database_sha256: String,
    pub column_scales: Vec<u32>,
    pub dense_fields: Vec<String>,
    pub training_math: String,
    pub sampler: String,
    pub model: CubeModelState,
    pub league: Vec<LeagueEntry>,
    pub segment: Segment,
    pub run: String,
    pub update: u64,
    pub constants: BTreeMap<String, u64>,
    pub lineage: Vec<LineageSegment>,
    pub pin_reasons: Vec<CheckpointPinReason>,
    pub boundary: Option<PickBoundary>,
    pub state_sha256: String,
    pub head_updates: HeadUpdates,
}

impl Checkpoint {
    fn new(
        config: &RunConfig,
        model: CubeModelState,
        league: Vec<LeagueEntry>,
        update: u64,
        lineage: Vec<LineageSegment>,
        pin_reasons: Vec<CheckpointPinReason>,
        head_updates: HeadUpdates,
    ) -> Result<Self, TrainingDriverError> {
        model.validate()?;
        let mut checkpoint = Self {
            schema: CHECKPOINT_SCHEMA.to_owned(),
            model_identity: config.metadata.model_identity.clone(),
            parameter_layout_sha256: config.metadata.parameter_layout_sha256.clone(),
            card_index_sha256: config.pins.card_index.clone(),
            cube_table_sha256: config.pins.cube_table.clone(),
            card_database_sha256: config.metadata.card_database_sha256.clone(),
            column_scales: config.metadata.column_scales.clone(),
            dense_fields: config.metadata.dense_fields.clone(),
            training_math: config.metadata.training_math.clone(),
            sampler: config.metadata.sampler.clone(),
            model,
            league,
            segment: config.segment,
            run: config.run.clone(),
            update,
            constants: config.metadata.constants.clone(),
            lineage,
            pin_reasons,
            boundary: config.boundary.clone(),
            state_sha256: String::new(),
            head_updates,
        };
        checkpoint.refresh_pin()?;
        Ok(checkpoint)
    }

    // The canonical compact JSON has the Rust field order, sorted map keys,
    // and an empty state_sha256 field. All model floats are encoded as integer bits.
    fn calculate_pin(&self) -> Result<String, TrainingDriverError> {
        let mut checkpoint = self.clone();
        checkpoint.state_sha256.clear();
        let mut bytes = b"mtg-kernel-cube-checkpoint-state/v1\0".to_vec();
        serde_json::to_writer(&mut bytes, &checkpoint)?;
        Ok(sha256_hex(&bytes))
    }

    fn refresh_pin(&mut self) -> Result<(), TrainingDriverError> {
        self.state_sha256 = self.calculate_pin()?;
        Ok(())
    }

    fn validate(&self) -> Result<(), TrainingDriverError> {
        if self.schema != CHECKPOINT_SCHEMA || self.state_sha256 != self.calculate_pin()? {
            return Err(invalid("checkpoint schema or state digest differs"));
        }
        self.model.validate()
    }
}

pub type LoadedCheckpointV1 = (Checkpoint, Arc<ModelV1>, Arc<ModelCardRowsV1>);

/// Loads an owner-view checkpoint from raw JSON bytes against external rows from a verified D1 table.
pub fn load_checkpoint_read_only_bytes_v1(
    bytes: &[u8],
    table: &CubeTableV1,
) -> Result<LoadedCheckpointV1, TrainingDriverError> {
    let checkpoint: Checkpoint = serde_json::from_slice(bytes)?;
    checkpoint.validate()?;
    if checkpoint.card_index_sha256.as_deref() != Some(table.index_pin()) {
        return Err(invalid(
            "checkpoint card index pin differs from the supplied table",
        ));
    }
    if checkpoint.cube_table_sha256.as_deref() != Some(table.file_sha256()) {
        return Err(invalid(
            "checkpoint cube table pin differs from the supplied table",
        ));
    }
    // D1 checks the table's database identity against this same compiled registry.
    if checkpoint.card_database_sha256 != format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH) {
        return Err(invalid(
            "checkpoint card database pin differs from the compiled registry",
        ));
    }
    let rows = ModelCardRowsV1::with_registry_basics_v1(
        table.index().ids().to_vec(),
        table.vector_width(),
        table.scaled_rows().to_vec(),
        table.dense_columns().len(),
        table.dense_rows().to_vec(),
    )
    .map_err(|error| TrainingDriverError::Update(error.to_string()))?;
    let model = ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 0)
        .map_err(|error| TrainingDriverError::Update(error.to_string()))?;
    if checkpoint.parameter_layout_sha256 != model_layout_pin(&model) {
        return Err(invalid(
            "checkpoint parameter layout differs from the supplied rows",
        ));
    }
    let (model, _) = checkpoint
        .model
        .restore(model, checkpoint.head_updates)
        .map_err(TrainingDriverError::Update)?;
    Ok((checkpoint, Arc::new(model), Arc::new(rows)))
}

/// Loads an owner-view checkpoint against external rows from a verified D1 table.
pub fn load_checkpoint_read_only_v1(
    path: &Path,
    table: &CubeTableV1,
) -> Result<LoadedCheckpointV1, TrainingDriverError> {
    let bytes = fs::read(path)?;
    load_checkpoint_read_only_bytes_v1(&bytes, table)
}

#[cfg(feature = "eternal-fixtures")]
/// Writes a copy of `checkpoint` with updated `segment`, `run`, `update`, and
/// `head_updates`, re-sealed with D6's own state digest, for test fixtures.
pub fn write_marked_test_checkpoint_v1(
    path: &Path,
    checkpoint: &Checkpoint,
    segment: Segment,
    run: &str,
    update: u64,
    head_updates: HeadUpdates,
) -> Result<Checkpoint, TrainingDriverError> {
    let mut marked = checkpoint.clone();
    marked.segment = segment;
    marked.run = run.to_owned();
    marked.update = update;
    marked.head_updates = head_updates;
    marked.refresh_pin()?;
    fs::write(path, json_bytes(&marked)?)?;
    Ok(marked)
}

fn model_layout_pin(model: &ModelV1) -> String {
    model
        .parameter_layout_sha256()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateReceipt {
    pub before_state_sha256: String,
    pub after_state_sha256: String,
    pub segment: Segment,
    pub checkpoint_number: u64,
    pub sets_trained: TrainSets,
    pub pilot: PilotInfo,
    pub pods: Vec<u64>,
    pub execution: u8,
    pub void_execution_sha256: Option<String>,
    pub game_statistics: GameStatistics,
    pub first_watchdog_pod: Option<u64>,
    pub first_watchdog_game: Option<u32>,
    pub league_entries: Vec<String>,
    pub league_trained_pick_entries: usize,
    pub optimizer: OptimizerOutput,
    pub head_updates: HeadUpdates,
    pub workers: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameStatistics {
    pub halts_by_stop_kind_and_attempt: BTreeMap<String, BTreeMap<u8, u64>>,
    pub cap_hits_by_stop_kind_and_basics_band: BTreeMap<String, BTreeMap<String, u64>>,
    pub replays: u64,
}

impl GameStatistics {
    fn from_pods(pods: &[PodRecord]) -> Self {
        let mut statistics = Self::default();
        for pod in pods {
            for game in &pod.games {
                for attempt in &game.attempts {
                    statistics.replays += u64::from(attempt.attempt > 0);
                    let Some(kind) = crate::cube_b_census_v1::stop_kind(&attempt.end) else {
                        continue;
                    };
                    *statistics
                        .halts_by_stop_kind_and_attempt
                        .entry(kind.clone())
                        .or_default()
                        .entry(attempt.attempt)
                        .or_default() += 1;
                    if kind == "driver_decision_cap"
                        || kind.starts_with("physical_decision_cap_reached:")
                        || kind.starts_with("policy_step_cap_reached:")
                        || kind.starts_with("engine_halted:StackSizeCapExceeded:")
                    {
                        for &deck in &game.decks {
                            let basics = pod.decks[deck as usize]
                                .build_rows
                                .as_ref()
                                .expect("training decks have learned count rows")
                                .count
                                .chosen;
                            let band = match basics {
                                0 => "0",
                                1..=9 => "1-9",
                                10..=14 => "10-14",
                                15..=18 => "15-18",
                                19..=25 => "19-25",
                                26..=35 => "26-35",
                                _ => "36-40",
                            };
                            *statistics
                                .cap_hits_by_stop_kind_and_basics_band
                                .entry(kind.clone())
                                .or_default()
                                .entry(band.to_owned())
                                .or_default() += 1;
                        }
                    }
                }
            }
        }
        statistics
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitManifest {
    pub batch_index: u64,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TripwireFiring {
    pub window: Option<u64>,
    pub rule: TripwireRule,
    pub games: u64,
    pub denominator: u64,
    pub observed_rate_bits: Option<u64>,
}

impl TripwireFiring {
    fn immediate(rule: TripwireRule) -> Self {
        Self {
            window: None,
            rule,
            games: 1,
            denominator: 0,
            observed_rate_bits: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessDeathEvidence {
    /// None when the runner exited before the first batch committed.
    pub last_committed_batch: Option<u64>,
    pub found_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecord {
    pub schema: String,
    pub kind: String,
    pub run: String,
    pub batch_index: u64,
    pub pod: Option<u64>,
    pub pairing: Option<u8>,
    pub game: Option<u32>,
    pub decks: Option<[u32; 2]>,
    pub attempts: Vec<StopAttemptReport>,
    pub hang: Option<bool>,
    pub first_watchdog_pod: Option<u64>,
    pub first_watchdog_game: Option<u32>,
    pub second_watchdog_pod: Option<u64>,
    pub second_watchdog_game: Option<u32>,
    pub firing: Option<TripwireFiring>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_death: Option<ProcessDeathEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub batch: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Published { update: u64 },
    Stopped { update: u64, kind: String },
    Paused { update: u64 },
    Completed,
}

fn update_name(update: u64) -> String {
    format!("update-{update:04}")
}

fn sync_directory(path: &Path) -> Result<(), TrainingDriverError> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), TrainingDriverError> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn publish_bytes(path: &Path, bytes: &[u8]) -> Result<(), TrainingDriverError> {
    if path.exists() {
        if fs::read(path)? == bytes {
            return Ok(());
        }
        return Err(invalid(format!(
            "immutable file differs: {}",
            path.display()
        )));
    }
    publish_fresh_bytes(path, bytes)
}

fn publish_fresh_bytes(path: &Path, bytes: &[u8]) -> Result<(), TrainingDriverError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("published file has no parent"))?;
    fs::create_dir_all(parent)?;
    let captured = capture_existing_publication_parent_v1(parent).map_err(io_error)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("invalid published file name"))?;
    let stage = format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed)
    );
    publish_new_bytes_v1(&captured, stage, name, bytes).map_err(io_error)?;
    Ok(())
}

fn replace_bytes(path: &Path, bytes: &[u8]) -> Result<(), TrainingDriverError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("replacement has no parent"))?;
    let stage = parent.join(format!(
        ".replace-{}-{}",
        std::process::id(),
        TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    write_synced(&stage, bytes)?;
    fs::rename(&stage, path)?;
    sync_directory(parent)
}

fn persist_gate(
    config: &mut RunConfig,
    root: &Path,
    pin: &PilotPin,
) -> Result<(), TrainingDriverError> {
    let switch = config
        .pilot
        .switch
        .as_mut()
        .ok_or_else(|| invalid("the run has no pilot switch"))?;
    if switch.gate.as_ref().is_some_and(|saved| saved != pin) {
        return Err(invalid("the gate pin is already fixed"));
    }
    publish_bytes(&root.join("gate-pin.json"), &json_bytes(pin)?)?;
    if switch.gate.is_none() {
        switch.gate = Some(pin.clone());
        if let Err(error) = replace_bytes(&root.join("run.json"), &json_bytes(config)?) {
            config.pilot.switch.as_mut().unwrap().gate = None;
            return Err(error);
        }
    }
    Ok(())
}

fn check_published_pilot(
    schedule: &PilotSchedule,
    update: u64,
    recorded: &PilotInfo,
) -> Result<(), TrainingDriverError> {
    let expected = match schedule.at(update) {
        Some(PilotSelection::Uniform) => PilotInfo::uniform(),
        Some(PilotSelection::Checkpoint { pin }) => PilotInfo {
            kind: PilotKind::Checkpoint,
            pin: Some(pin.checkpoint_sha256),
            decoding: pin.decoding,
        },
        None => return Err(invalid("published update has no pinned pilot")),
    };
    if expected != *recorded {
        return Err(invalid("published update pilot differs from the run plan"));
    }
    Ok(())
}

fn check_run_plan(saved: &RunConfig, proposed: &RunConfig) -> Result<(), TrainingDriverError> {
    if saved.namespace != proposed.namespace
        || saved.run != proposed.run
        || saved.segment != proposed.segment
        || saved.update_range != proposed.update_range
        || saved.label != proposed.label
        || saved.train_sets != proposed.train_sets
        || saved.pods_per_update != proposed.pods_per_update
        || saved.caps != proposed.caps
        || saved.pins != proposed.pins
        || saved.launch_pins != proposed.launch_pins
        || saved.manifest_prefix_sha256 != proposed.manifest_prefix_sha256
        || saved.platform != proposed.platform
        || saved.host != proposed.host
        || saved.build_profile != proposed.build_profile
        || (proposed.host.is_some() && saved.workers != proposed.workers)
        || saved.metadata != proposed.metadata
        || saved.tripwire_window_games != proposed.tripwire_window_games
        || saved.tripwire != proposed.tripwire
        || saved.marks != proposed.marks
        || saved.liveness != proposed.liveness
        || saved.boundary != proposed.boundary
        || !saved.pilot.compatible_resume(&proposed.pilot)
    {
        return Err(invalid("resume changed a run pin, plan or tripwire mode"));
    }
    Ok(())
}

fn json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, TrainingDriverError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, TrainingDriverError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn stop_directory(root: &Path) -> PathBuf {
    root.join("stops")
}

fn stop_paths(root: &Path) -> Result<Vec<PathBuf>, TrainingDriverError> {
    let directory = stop_directory(root);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with("stop-")
            && name.ends_with(".json")
            && !name.ends_with(".resolution.json")
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn check_stop_ledger(root: &Path) -> Result<(), TrainingDriverError> {
    let stops = stop_paths(root)?;
    if stops.is_empty() {
        return Ok(());
    }
    for stop in stops {
        let resolution = stop.with_file_name(format!(
            "{}.resolution.json",
            stop.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("")
        ));
        if !resolution.exists() {
            return Err(TrainingDriverError::StoppedWithoutResolution);
        }
    }
    Err(TrainingDriverError::StoppedRequiresRestart)
}

fn ledger_path_for_update(
    root: &Path,
    update: u64,
) -> Result<Option<PathBuf>, TrainingDriverError> {
    for path in stop_paths(root)? {
        let stop: StopRecord = read_json(&path)?;
        if stop.batch_index == update {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn batch_has_stop(root: &Path, update: u64, pending: &Path) -> Result<bool, TrainingDriverError> {
    if ledger_path_for_update(root, update)?.is_some() {
        return Ok(true);
    }
    // A committed stop is authoritative even if publishing the batch or its
    // ledger copy failed. Recovery will finish publishing the original stop.
    let published = root.join("updates").join(update_name(update));
    for directory in [pending, published.as_path()] {
        if directory.join("stop.json").is_file()
            && read_commit(directory, update, false)?
                .is_some_and(|commit| commit.files.contains_key("stop.json"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ledger_path_for_kind(
    root: &Path,
    stop: &StopRecord,
) -> Result<Option<PathBuf>, TrainingDriverError> {
    for path in stop_paths(root)? {
        if read_json::<StopRecord>(&path)
            .is_ok_and(|saved| saved.batch_index == stop.batch_index && saved.kind == stop.kind)
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn ledger_stop(root: &Path, stop: &StopRecord) -> Result<PathBuf, TrainingDriverError> {
    fs::create_dir_all(stop_directory(root))?;
    let path = if let Some(path) = ledger_path_for_kind(root, stop)? {
        path
    } else {
        stop_directory(root).join(format!("stop-{:04}.json", stop_paths(root)?.len()))
    };
    let bytes = json_bytes(stop)?;
    publish_bytes(&path, &bytes)?;
    if fs::read(&path)? != bytes {
        return Err(invalid("stop ledger readback differs"));
    }
    Ok(path)
}

fn published_stop(root: &Path, directory: &Path, update: u64) -> Result<(), TrainingDriverError> {
    let path = directory.join("stop.json");
    if !path.exists() {
        return Ok(());
    }
    let commit: CommitManifest = read_json(&directory.join("commit.json"))?;
    let bytes = fs::read(path)?;
    let digest = sha256_hex(&bytes);
    let record: StopRecord = serde_json::from_slice(&bytes)?;
    if commit.batch_index != update
        || record.batch_index != update
        || commit.files.get("stop.json") != Some(&digest)
    {
        return Err(invalid("published stop differs from commit.json"));
    }
    let paths = stop_paths(root)?;
    let target = ledger_path_for_kind(root, &record)?.or_else(|| {
        paths
            .iter()
            .find(|path| read_json::<StopRecord>(path).is_err())
            .cloned()
    });
    let target = if let Some(path) = target {
        if fs::read(&path)? != bytes {
            replace_bytes(&path, &bytes)?;
        }
        path
    } else {
        let path = stop_directory(root).join(format!("stop-{:04}.json", paths.len()));
        publish_bytes(&path, &bytes)?;
        path
    };
    if sha256_hex(&fs::read(target)?) != digest {
        return Err(invalid("stop ledger copy differs from commit.json"));
    }
    Ok(())
}

fn watchdog_path(root: &Path, update: u64) -> PathBuf {
    root.join("watchdog")
        .join(format!("{}.json", update_name(update)))
}

fn first_watchdog(root: &Path, update: u64) -> Result<Option<(u64, u32)>, TrainingDriverError> {
    let path = watchdog_path(root, update);
    if !path.exists() {
        return Ok(None);
    }
    let record: StopRecord = read_json(&path)?;
    if record.kind != "watchdog_void" || record.batch_index != update {
        return Err(invalid("invalid void watchdog execution"));
    }
    record
        .first_watchdog_pod
        .zip(record.first_watchdog_game)
        .map(Some)
        .ok_or_else(|| invalid("void execution missing expired game"))
}

fn apply_first_watchdog(stop: &mut StopRecord, first: Option<(u64, u32)>) {
    stop.first_watchdog_pod = first.map(|key| key.0);
    stop.first_watchdog_game = first.map(|key| key.1);
}

fn read_commit(
    path: &Path,
    update: u64,
    allow_pruned: bool,
) -> Result<Option<CommitManifest>, TrainingDriverError> {
    let bytes = match fs::read(path.join("commit.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Ok(commit) = serde_json::from_slice::<CommitManifest>(&bytes) else {
        return Ok(None);
    };
    let required = ["pods.json", "checkpoint.json", "update.json"];
    let optional = ["stop.json", "league.json", "execution-0.json"];
    if commit.batch_index != update
        || required
            .iter()
            .any(|name| !commit.files.contains_key(*name))
        || optional
            .iter()
            .any(|name| path.join(name).exists() != commit.files.contains_key(*name))
    {
        return Ok(None);
    }
    for (name, digest) in &commit.files {
        if !required.contains(&name.as_str()) && !optional.contains(&name.as_str()) {
            return Ok(None);
        }
        let bytes = match fs::read(path.join(name)) {
            Ok(bytes) => bytes,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && allow_pruned
                    && name == "checkpoint.json"
                    && fs::read_to_string(path.join("checkpoint.pruned"))
                        .is_ok_and(|saved| saved == *digest) =>
            {
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if sha256_hex(&bytes) != *digest {
            return Ok(None);
        }
    }
    Ok(Some(commit))
}

fn read_partial_pods(directory: &Path) -> Result<Vec<Value>, TrainingDriverError> {
    let mut batch = Vec::new();
    let partial = directory.join("partial");
    if partial.exists() {
        let mut files: Vec<PathBuf> = fs::read_dir(&partial)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<_, _>>()?;
        files.sort();
        for path in files {
            // The runner can die while writing the last pod. Keep the complete rows.
            if let Ok(pod) = serde_json::from_slice(&fs::read(path)?) {
                batch.push(pod);
            }
        }
    }
    Ok(batch)
}

fn update_directories(directory: &Path) -> Result<Vec<PathBuf>, TrainingDriverError> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut directories = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn death_evidence(
    last_committed_batch: Option<u64>,
) -> Result<ProcessDeathEvidence, TrainingDriverError> {
    let found_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io_error)?
        .as_millis()
        .try_into()
        .map_err(io_error)?;
    Ok(ProcessDeathEvidence {
        last_committed_batch,
        found_unix_ms,
    })
}

fn record_process_death(
    config: &RunConfig,
    last_committed_batch: Option<u64>,
    abandoned: Option<(u64, PathBuf)>,
) -> Result<(), TrainingDriverError> {
    let root = &config.root;
    let (batch_index, batch) = if let Some((update, directory)) = abandoned {
        let mut batch = read_partial_pods(&directory)?;
        if batch.is_empty() && directory.join("pods.json").exists() {
            batch = read_pods(&directory.join("pods.json")).unwrap_or_default();
        }
        (update, batch)
    } else {
        (
            last_committed_batch.map_or(0, |update| update + 1),
            Vec::new(),
        )
    };
    let mut stop = StopRecord {
        schema: STOP_SCHEMA.to_owned(),
        kind: "process_death".to_owned(),
        run: config.run.clone(),
        batch_index,
        pod: None,
        pairing: None,
        game: None,
        decks: None,
        attempts: Vec::new(),
        hang: None,
        first_watchdog_pod: None,
        first_watchdog_game: None,
        second_watchdog_pod: None,
        second_watchdog_game: None,
        firing: Some(TripwireFiring::immediate(TripwireRule::ProcessDeath)),
        process_death: Some(death_evidence(last_committed_batch)?),
        batch,
        error: None,
    };
    apply_first_watchdog(&mut stop, first_watchdog(root, batch_index)?);
    ledger_stop(root, &stop)?;
    Ok(())
}

fn recover(root: &Path) -> Result<(), TrainingDriverError> {
    let pending = root.join("pending");
    let updates = root.join("updates");
    fs::create_dir_all(&pending)?;
    fs::create_dir_all(&updates)?;
    for directory in update_directories(&pending)? {
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid pending update name"))?;
        let update = name
            .strip_prefix("update-")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| invalid("invalid pending update name"))?;
        if read_commit(&directory, update, false)?.is_some() {
            let destination = updates.join(name);
            if destination.exists() {
                if fs::read(destination.join("commit.json"))?
                    != fs::read(directory.join("commit.json"))?
                {
                    return Err(invalid("pending and published commits differ"));
                }
                fs::remove_dir_all(&directory)?;
            } else {
                fs::rename(&directory, &destination)?;
            }
            sync_directory(&pending)?;
            sync_directory(&updates)?;
            restore_update_league_snapshot(root, &destination)?;
            published_stop(root, &destination, update)?;
            continue;
        }
        if ledger_path_for_update(root, update)?.is_none() {
            return Err(invalid("unfinished batch has no stop ledger record"));
        }
        fs::remove_dir_all(&directory)?;
        sync_directory(&pending)?;
    }
    for entry in fs::read_dir(&updates)? {
        let directory = entry?.path();
        if !directory.is_dir() {
            continue;
        }
        restore_update_league_snapshot(root, &directory)?;
        if directory.join("stop.json").exists() {
            let update = index_from_update_path(&directory)?;
            published_stop(root, &directory, update)?;
        }
    }
    Ok(())
}

fn index_from_update_path(path: &Path) -> Result<u64, TrainingDriverError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("update-"))
        .and_then(|index| index.parse().ok())
        .ok_or_else(|| invalid("invalid update name"))
}

fn published_update_indices(root: &Path) -> Result<Vec<u64>, TrainingDriverError> {
    let updates = root.join("updates");
    if !updates.exists() {
        return Ok(Vec::new());
    }
    let mut entries = update_directories(&updates)?
        .into_iter()
        .map(|path| Ok((index_from_update_path(&path)?, path)))
        .collect::<Result<Vec<_>, TrainingDriverError>>()?;
    entries.sort_by_key(|(index, _)| *index);
    for (position, (index, path)) in entries.iter().enumerate() {
        if *index != position as u64 {
            return Err(invalid("published updates are not contiguous from zero"));
        }
        if read_commit(path, *index, position + 1 < entries.len())?.is_none() {
            return Err(invalid("published update has an invalid commit"));
        }
    }
    Ok(entries.into_iter().map(|(index, _)| index).collect())
}

fn read_pods(path: &Path) -> Result<Vec<Value>, TrainingDriverError> {
    let file = File::open(path)?;
    let mut decoder = flate2::read::GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn rebuild_tripwire(
    root: &Path,
    baseline: Option<&CensusRates>,
    window_sizes: Vec<u64>,
) -> Result<TripwireTracker, TrainingDriverError> {
    let mut tracker = TripwireTracker::new(window_sizes);
    for update in published_update_indices(root)? {
        let pods = read_pods(
            &root
                .join("updates")
                .join(update_name(update))
                .join("pods.json"),
        )?;
        if pods.len() != PODS_PER_UPDATE as usize {
            return Err(invalid("published update does not contain exactly 16 pods"));
        }
        for (offset, pod) in pods.into_iter().enumerate() {
            if pod["pod_index"].as_u64() != Some(update * PODS_PER_UPDATE + offset as u64) {
                return Err(invalid(
                    "published pod index differs from the batch schedule",
                ));
            }
            tracker.add_pod_json(pod, baseline)?;
        }
    }
    Ok(tracker)
}

fn pin_reasons(config: &RunConfig, checkpoint: u64) -> Vec<CheckpointPinReason> {
    let mut reasons = Vec::new();
    if checkpoint == 0 {
        reasons.push(CheckpointPinReason::SegmentStart);
    }
    if (checkpoint == 0 && config.segment == Segment::BuilderOnly)
        || (checkpoint != 0 && checkpoint.is_multiple_of(16))
    {
        reasons.push(CheckpointPinReason::League);
    }
    if checkpoint == config.total_updates() {
        reasons.push(CheckpointPinReason::SegmentEnd);
    }
    if config.marks.dose_reads.contains(&checkpoint) {
        reasons.push(CheckpointPinReason::DoseRead);
    }
    if config.marks.owner_views.contains(&checkpoint) {
        reasons.push(CheckpointPinReason::OwnerView);
    }
    if config.marks.read_candidates.contains(&checkpoint) {
        reasons.push(CheckpointPinReason::ReadCandidate);
    }
    reasons
}

fn league_entry(root: &Path, checkpoint: &Checkpoint) -> Result<LeagueEntry, TrainingDriverError> {
    let path = root
        .join("league")
        .join(format!("{}.json", checkpoint.state_sha256));
    Ok(LeagueEntry {
        pin: checkpoint.state_sha256.clone(),
        path: path
            .strip_prefix(root)
            .map_err(|error| invalid(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/"),
        segment: checkpoint.segment,
        update: checkpoint.update,
        head_updates: checkpoint.head_updates,
    })
}

fn write_league_snapshot(
    root: &Path,
    checkpoint: &Checkpoint,
) -> Result<LeagueEntry, TrainingDriverError> {
    let entry = league_entry(root, checkpoint)?;
    publish_bytes(&root.join(&entry.path), &json_bytes(checkpoint)?)?;
    Ok(entry)
}

fn restore_update_league_snapshot(root: &Path, update: &Path) -> Result<(), TrainingDriverError> {
    let snapshot = update.join("league.json");
    if !snapshot.exists() {
        return Ok(());
    }
    let bytes = fs::read(snapshot)?;
    let commit: CommitManifest = read_json(&update.join("commit.json"))?;
    if commit.files.get("league.json") != Some(&sha256_hex(&bytes)) {
        return Err(invalid("staged league snapshot differs from commit.json"));
    }
    let checkpoint: Checkpoint = serde_json::from_slice(&bytes)?;
    checkpoint.validate()?;
    if !checkpoint
        .pin_reasons
        .contains(&CheckpointPinReason::League)
        || checkpoint.update != commit.batch_index + 1
    {
        return Err(invalid(
            "staged league snapshot has incorrect checkpoint pins",
        ));
    }
    let entry = league_entry(root, &checkpoint)?;
    publish_bytes(&root.join(entry.path), &bytes)
}

pub fn prune_checkpoint(path: &Path) -> Result<(), TrainingDriverError> {
    let bytes = fs::read(path)?;
    let checkpoint: Checkpoint = serde_json::from_slice(&bytes)?;
    checkpoint.validate()?;
    if !checkpoint.pin_reasons.is_empty() {
        return Err(TrainingDriverError::PinnedCheckpoint);
    }
    let directory = path
        .parent()
        .ok_or_else(|| invalid("checkpoint has no update directory"))?;
    let update = index_from_update_path(directory)?;
    let updates = directory
        .parent()
        .ok_or_else(|| invalid("missing updates directory"))?;
    if path.file_name().and_then(|name| name.to_str()) != Some("checkpoint.json")
        || updates.file_name().and_then(|name| name.to_str()) != Some("updates")
        || !updates
            .join(update_name(update + 1))
            .join("commit.json")
            .is_file()
    {
        return Err(invalid("can only prune an earlier published checkpoint"));
    }
    let commit = read_commit(directory, update, false)?
        .ok_or_else(|| invalid("checkpoint has no valid commit"))?;
    let digest = sha256_hex(&bytes);
    if commit.files.get("checkpoint.json") != Some(&digest) {
        return Err(invalid("checkpoint digest changed during pruning"));
    }
    publish_bytes(&directory.join("checkpoint.pruned"), digest.as_bytes())?;
    fs::remove_file(path)?;
    sync_directory(directory)
}

struct BatchFailure {
    pod: Box<PodRecord>,
    completed: Vec<PodRecord>,
}

enum CollectFailure {
    Stop(BatchFailure),
    Error(TrainingDriverError),
}

fn stop_from_failure(run: &str, update: u64, failure: BatchFailure) -> StopRecord {
    let pod = *failure.pod;
    let stop = pod.stop.clone();
    let attempts = stop
        .as_ref()
        .map_or_else(Vec::new, |stop| stop.attempts.clone());
    let mut batch = failure.completed;
    batch.push(pod.clone());
    StopRecord {
        schema: STOP_SCHEMA.to_owned(),
        kind: if batch.iter().any(|pod| {
            pod.games.iter().any(|game| {
                game.attempts
                    .iter()
                    .any(|attempt| is_watchdog_end(&attempt.end))
            })
        }) {
            "watchdog_stop".to_owned()
        } else {
            "retry_exhausted".to_owned()
        },
        run: run.to_owned(),
        batch_index: update,
        pod: Some(pod.pod_index),
        pairing: stop.as_ref().and_then(|stop| stop.pairing),
        game: stop.as_ref().map(|stop| stop.game),
        decks: stop.as_ref().map(|stop| stop.decks),
        attempts,
        hang: None,
        first_watchdog_pod: None,
        first_watchdog_game: None,
        second_watchdog_pod: None,
        second_watchdog_game: None,
        firing: None,
        process_death: None,
        error: None,
        batch: batch
            .iter()
            .map(|pod| serde_json::to_value(pod).expect("pod json"))
            .collect(),
    }
}

struct Snapshot {
    model: Arc<ModelV1>,
    seat: Arc<ModelSeatForward>,
}

impl Snapshot {
    fn new(model: ModelV1, rows: Arc<ModelCardRowsV1>) -> Result<Self, String> {
        let model = Arc::new(model);
        let seat =
            ModelSeatForward::new(model.clone(), rows, 1).map_err(|error| error.to_string())?;
        Ok(Self {
            model,
            seat: Arc::new(seat),
        })
    }
}

/// Owns the exclusive run lock and active runner marker. The launcher must
/// drop the driver before a clean exit, including before `std::process::exit`.
/// Without a host-loss resume entry, the next open records a process-death
/// stop for an exit that skips cleanup.
pub struct TrainingDriver {
    config: RunConfig,
    model: Snapshot,
    optimizer: Optimizer,
    rows: Arc<ModelCardRowsV1>,
    next_update: u64,
    lineage: Vec<LineageSegment>,
    league: Vec<LeagueEntry>,
    current_pin: String,
    tripwire: TripwireTracker,
    census_rates: Option<CensusRates>,
    snapshots: BTreeMap<String, Snapshot>,
    runner: RunnerMarker,
    // The marker must be dropped before releasing the run lock.
    _run_lock: RunLock,
}

impl TrainingDriver {
    /// Derive the builder's checkpoint zero including its initial league entry, without IO.
    pub fn initial_state_sha256(
        config: &RunConfig,
        rows: &ModelCardRowsV1,
        init_seed: u64,
    ) -> Result<String, TrainingDriverError> {
        if config.segment != Segment::BuilderOnly {
            return Err(invalid("initial state requires a builder configuration"));
        }
        let mut config = config.clone();
        let model = Self::initial_model(rows, init_seed)?;
        let layout = model_layout_pin(&model);
        if !config.metadata.parameter_layout_sha256.is_empty()
            && config.metadata.parameter_layout_sha256 != layout
        {
            return Err(invalid(
                "configured parameter layout differs from the model",
            ));
        }
        config.metadata.parameter_layout_sha256 = layout;
        let optimizer = Optimizer::new(&model);
        let mut checkpoint = Checkpoint::new(
            &config,
            CubeModelState::from_model(&model, &optimizer),
            Vec::new(),
            0,
            Vec::new(),
            pin_reasons(&config, 0),
            optimizer.head_updates,
        )?;
        checkpoint
            .league
            .push(league_entry(&config.root, &checkpoint)?);
        checkpoint.refresh_pin()?;
        Ok(checkpoint.state_sha256)
    }

    fn initial_model(
        rows: &ModelCardRowsV1,
        init_seed: u64,
    ) -> Result<ModelV1, TrainingDriverError> {
        ModelV1::initialise_v1(
            rows.rows(),
            rows.table_width(),
            rows.dense_width(),
            init_seed,
        )
        .map_err(|error| TrainingDriverError::Update(error.to_string()))
    }
    pub fn open(
        config: RunConfig,
        rows: Arc<ModelCardRowsV1>,
        init_seed: u64,
    ) -> Result<Self, TrainingDriverError> {
        Self::open_with_lineage(config, rows, init_seed, Vec::new())
    }

    pub fn open_with_lineage(
        config: RunConfig,
        rows: Arc<ModelCardRowsV1>,
        init_seed: u64,
        lineage: Vec<LineageSegment>,
    ) -> Result<Self, TrainingDriverError> {
        let model = Self::initial_model(&rows, init_seed)?;
        let optimizer = Optimizer::new(&model);
        Self::open_from_state(config, rows, model, optimizer, lineage, Vec::new(), None)
    }

    /// Open a restarted builder-only run by loading an archived pinned initial
    /// checkpoint (D0.12 / D0.18) without drawing the initialiser seed again.
    pub fn open_from_initial_checkpoint(
        config: RunConfig,
        rows: Arc<ModelCardRowsV1>,
        source_checkpoint: &Checkpoint,
        expected_source_state_sha256: &str,
    ) -> Result<Self, TrainingDriverError> {
        if config.segment != Segment::BuilderOnly {
            return Err(invalid(
                "initial checkpoint restart requires a builder configuration",
            ));
        }
        source_checkpoint.validate()?;
        if source_checkpoint.state_sha256 != expected_source_state_sha256 {
            return Err(invalid(
                "archived initial checkpoint state sha256 differs from pin",
            ));
        }
        if source_checkpoint.segment != Segment::BuilderOnly
            || source_checkpoint.update != 0
            || source_checkpoint.head_updates != HeadUpdates::default()
            || source_checkpoint.boundary.is_some()
            || !source_checkpoint.lineage.is_empty()
        {
            return Err(invalid(
                "archived initial checkpoint is not an untrained builder checkpoint 0",
            ));
        }
        if source_checkpoint.card_index_sha256 != config.pins.card_index
            || source_checkpoint.cube_table_sha256 != config.pins.cube_table
            || source_checkpoint.card_database_sha256 != config.metadata.card_database_sha256
            || source_checkpoint.column_scales != config.metadata.column_scales
            || source_checkpoint.dense_fields != config.metadata.dense_fields
            || source_checkpoint.training_math != config.metadata.training_math
            || source_checkpoint.sampler != config.metadata.sampler
            || source_checkpoint.constants != config.metadata.constants
            || source_checkpoint.model_identity != config.metadata.model_identity
        {
            return Err(invalid(
                "archived initial checkpoint metadata differs from run configuration",
            ));
        }
        let template =
            ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 0)
                .map_err(|error| TrainingDriverError::Update(error.to_string()))?;
        if source_checkpoint.parameter_layout_sha256 != model_layout_pin(&template) {
            return Err(invalid(
                "archived initial checkpoint parameter layout differs from model",
            ));
        }
        let (model, optimizer) = source_checkpoint
            .model
            .restore(template, source_checkpoint.head_updates)
            .map_err(TrainingDriverError::Update)?;
        Self::open_from_state(config, rows, model, optimizer, Vec::new(), Vec::new(), None)
    }

    /// Open an already-prepared pick-phase run from its staged initial checkpoint
    /// without drawing any initialiser seed.
    pub fn open_existing_pick_phase(
        config: RunConfig,
        rows: Arc<ModelCardRowsV1>,
    ) -> Result<Self, TrainingDriverError> {
        if config.segment != Segment::PickPhase || config.open_intent != OpenIntent::Run {
            return Err(invalid(
                "open_existing_pick_phase requires a prepared pick-phase run",
            ));
        }
        let initial_path = config.root.join("initial/checkpoint.json");
        let checkpoint: Checkpoint = read_json(&initial_path)?;
        checkpoint.validate()?;
        let template =
            ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 0)
                .map_err(|error| TrainingDriverError::Update(error.to_string()))?;
        if checkpoint.parameter_layout_sha256 != model_layout_pin(&template) {
            return Err(invalid(
                "pick-phase initial checkpoint parameter layout differs from model",
            ));
        }
        let (model, optimizer) = checkpoint
            .model
            .restore(template, checkpoint.head_updates)
            .map_err(TrainingDriverError::Update)?;
        Self::open_from_state(config, rows, model, optimizer, Vec::new(), Vec::new(), None)
    }

    fn open_from_state(
        mut config: RunConfig,
        rows: Arc<ModelCardRowsV1>,
        initial_model: ModelV1,
        initial_optimizer: Optimizer,
        initial_lineage: Vec<LineageSegment>,
        initial_league: Vec<LeagueEntry>,
        builder_boundary: Option<(PickBoundary, &Path)>,
    ) -> Result<Self, TrainingDriverError> {
        let layout = model_layout_pin(&initial_model);
        if config.metadata.parameter_layout_sha256.is_empty() {
            config.metadata.parameter_layout_sha256 = layout.clone();
        } else if config.metadata.parameter_layout_sha256 != layout {
            return Err(invalid(
                "configured parameter layout differs from the model",
            ));
        }
        config.validate()?;
        let census_rates = match &config.tripwire {
            TripwireMode::Full { baseline } if baseline.matches(&config.pins) => {
                let files = config.tripwire_files.as_ref().ok_or_else(|| {
                    invalid("matching census-b baseline needs its pinned evidence")
                })?;
                Some(baseline.load(files)?)
            }
            _ => {
                config.tripwire = TripwireMode::Q9;
                None
            }
        };
        let scheduled_games = config.update_range[1]
            .checked_mul(PODS_PER_UPDATE * crate::cube_pod_runner_v1::TRAINING_POD_GAMES as u64)
            .ok_or_else(|| invalid("scheduled game count overflow"))?;
        let window_sizes = tripwire_window_sizes(scheduled_games);
        if !config.tripwire_window_games.is_empty() && config.tripwire_window_games != window_sizes
        {
            return Err(invalid(
                "tripwire window layout differs from the update range",
            ));
        }
        config.tripwire_window_games = window_sizes;
        if config.label != "shakedown"
            && initial_lineage
                .iter()
                .any(|entry| entry.pilots.contains(&PilotKind::Uniform))
        {
            return Err(invalid(
                "a non-shakedown run cannot start from a uniform-pilot lineage",
            ));
        }
        config.check_open_intent(false)?;
        let run_lock = RunLock::acquire(&config)?;
        config.check_open_intent(true)?;
        let resume_receipt = check_resume(&config, &run_lock)?;
        let run_path = config.root.join("run.json");
        let mut runner = None;
        if let Some(receipt) = resume_receipt {
            let saved: RunConfig = read_json(&run_path)?;
            if config.segment == Segment::PickPhase && config.boundary.is_none() {
                config.boundary = saved.boundary.clone();
            }
            check_run_plan(&saved, &config)?;
            runner = Some(RunnerMarker::open(&config.root, Some(receipt))?);
        }
        recover(&config.root)?;
        check_stop_ledger(&config.root)?;
        let indices = published_update_indices(&config.root)?;
        let initial_path = config.root.join("initial/checkpoint.json");
        if config.segment == Segment::PickPhase {
            let boundary_source = if let Some((boundary, builder_root)) = builder_boundary {
                for entry in &initial_league {
                    let source = builder_root.join(&entry.path);
                    let destination = config.root.join(&entry.path);
                    if source != destination {
                        publish_bytes(&destination, &fs::read(source)?)?;
                    }
                }
                Some(boundary)
            } else {
                None
            };
            let journal = config.root.join("pick-boundary.json");
            let record = if journal.exists() {
                read_json::<BoundaryRecord>(&journal)?
            } else {
                let source = boundary_source
                    .clone()
                    .ok_or_else(|| invalid("pick phase requires a staged builder boundary"))?;
                config.boundary = Some(source);
                let checkpoint = Checkpoint::new(
                    &config,
                    CubeModelState::from_model(&initial_model, &initial_optimizer),
                    initial_league.clone(),
                    0,
                    initial_lineage.clone(),
                    pin_reasons(&config, 0),
                    initial_optimizer.head_updates,
                )?;
                let record = BoundaryRecord {
                    config: config.clone(),
                    checkpoint,
                };
                publish_bytes(&journal, &json_bytes(&record)?)?;
                record
            };
            record.checkpoint.validate()?;
            if record.config.boundary.is_none()
                || record.checkpoint.boundary != record.config.boundary
                || record.checkpoint.segment != Segment::PickPhase
                || record.checkpoint.run != config.run
                || record.checkpoint.update != 0
                || config
                    .boundary
                    .as_ref()
                    .or(boundary_source.as_ref())
                    .is_some_and(|requested| Some(requested) != record.config.boundary.as_ref())
            {
                return Err(invalid(
                    "pick phase boundary differs from its staged checkpoint",
                ));
            }
            config.boundary = record.config.boundary.clone();
            check_run_plan(&record.config, &config)?;
            publish_bytes(&initial_path, &json_bytes(&record.checkpoint)?)?;
            if !run_path.exists() {
                publish_bytes(&run_path, &json_bytes(&record.config)?)?;
            }
        } else if config.boundary.is_some() {
            return Err(invalid("builder-only run has a pick phase boundary"));
        }
        if run_path.exists() {
            let mut saved: RunConfig = read_json(&run_path)?;
            check_run_plan(&saved, &config)?;
            let gate_path = config.root.join("gate-pin.json");
            let durable_gate = if gate_path.exists() {
                Some(read_json::<PilotPin>(&gate_path)?)
            } else {
                None
            };
            let saved_gate = saved
                .pilot
                .switch
                .as_ref()
                .and_then(|switch| switch.gate.as_ref());
            let proposed = config
                .pilot
                .switch
                .as_ref()
                .and_then(|switch| switch.gate.as_ref());
            let gate = durable_gate
                .clone()
                .or_else(|| saved_gate.cloned())
                .or_else(|| proposed.cloned());
            if let Some(gate) = gate {
                if config.pilot.switch.is_none()
                    || [saved_gate, proposed]
                        .into_iter()
                        .flatten()
                        .any(|pin| pin != &gate)
                {
                    return Err(invalid("resume changed the fixed gate pin"));
                }
                config.pilot.switch.as_mut().unwrap().gate = Some(gate.clone());
                if let Some(&last) = indices.last() {
                    let receipt: UpdateReceipt = read_json(
                        &config
                            .root
                            .join("updates")
                            .join(update_name(last))
                            .join("update.json"),
                    )?;
                    check_published_pilot(&config.pilot, last, &receipt.pilot)?;
                }
                persist_gate(&mut saved, &config.root, &gate)?;
            }
        } else {
            publish_bytes(&run_path, &json_bytes(&config)?)?;
            if let Some(gate) = config
                .pilot
                .switch
                .as_ref()
                .and_then(|switch| switch.gate.as_ref())
            {
                publish_bytes(&config.root.join("gate-pin.json"), &json_bytes(gate)?)?;
            }
        }

        let (mut checkpoint, new_initial) = if let Some(update) = indices.last() {
            (
                read_json::<Checkpoint>(
                    &config
                        .root
                        .join("updates")
                        .join(update_name(*update))
                        .join("checkpoint.json"),
                )?,
                false,
            )
        } else if initial_path.exists() {
            (read_json::<Checkpoint>(&initial_path)?, false)
        } else {
            (
                Checkpoint::new(
                    &config,
                    CubeModelState::from_model(&initial_model, &initial_optimizer),
                    initial_league.clone(),
                    0,
                    initial_lineage,
                    pin_reasons(&config, 0),
                    initial_optimizer.head_updates,
                )?,
                true,
            )
        };
        checkpoint.validate()?;
        if checkpoint.run != config.run
            || checkpoint.segment != config.segment
            || checkpoint.parameter_layout_sha256 != layout
            || checkpoint.boundary != config.boundary
        {
            return Err(invalid("checkpoint belongs to another segment"));
        }
        if config.label != "shakedown"
            && checkpoint
                .lineage
                .iter()
                .any(|entry| entry.pilots.contains(&PilotKind::Uniform))
        {
            return Err(invalid(
                "a non-shakedown run cannot start from a uniform-pilot lineage",
            ));
        }
        if new_initial {
            if checkpoint.league.is_empty() {
                let entry = write_league_snapshot(&config.root, &checkpoint)?;
                checkpoint.league.push(entry);
                checkpoint.refresh_pin()?;
            }
            publish_bytes(&initial_path, &json_bytes(&checkpoint)?)?;
        }
        if let Some(&last) = indices.last() {
            let receipt: UpdateReceipt = read_json(
                &config
                    .root
                    .join("updates")
                    .join(update_name(last))
                    .join("update.json"),
            )?;
            if receipt.after_state_sha256 != checkpoint.state_sha256
                || receipt.checkpoint_number != checkpoint.update
                || checkpoint.update != last + 1
                || receipt.segment != checkpoint.segment
                || receipt.head_updates != checkpoint.head_updates
            {
                return Err(invalid(
                    "checkpoint differs from its published update receipt",
                ));
            }
            check_published_pilot(&config.pilot, last, &receipt.pilot)?;
        }
        let next_update = indices.last().map_or(0, |update| update + 1);
        let tripwire = rebuild_tripwire(
            &config.root,
            census_rates.as_ref(),
            config.tripwire_window_games.clone(),
        )?;
        if tripwire.firing.is_some() && !indices.is_empty() {
            let last = *indices.last().unwrap();
            if !config
                .root
                .join("updates")
                .join(update_name(last))
                .join("stop.json")
                .exists()
            {
                return Err(invalid("a published tripwire firing has no stop record"));
            }
        }
        let (model, optimizer) = checkpoint
            .model
            .restore(initial_model, checkpoint.head_updates)
            .map_err(TrainingDriverError::Update)?;
        let model = Snapshot::new(model, rows.clone()).map_err(TrainingDriverError::Update)?;
        let runner = match runner {
            Some(mut runner) => {
                runner.finish_batch();
                runner
            }
            None => RunnerMarker::open(&config.root, None)?,
        };
        Ok(Self {
            current_pin: checkpoint.state_sha256,
            model,
            optimizer,
            rows,
            next_update,
            lineage: checkpoint.lineage.clone(),
            league: checkpoint.league.clone(),
            tripwire,
            census_rates,
            snapshots: BTreeMap::new(),
            runner,
            _run_lock: run_lock,
            config,
        })
    }

    pub fn start_pick_phase(
        previous: &Self,
        mut config: RunConfig,
    ) -> Result<Self, TrainingDriverError> {
        check_stop_ledger(&previous.config.root)?;
        if previous.config.segment != Segment::BuilderOnly
            || config.segment != Segment::PickPhase
            || previous.next_update != previous.config.total_updates()
        {
            return Err(invalid(
                "the pick phase requires a completed builder segment",
            ));
        }
        if config.namespace != previous.config.namespace
            || config.label != previous.config.label
            || config.run == previous.config.run
        {
            return Err(invalid(
                "the pick phase needs a new run id in the same namespace and run mode",
            ));
        }
        if config.metadata.parameter_layout_sha256.is_empty() {
            config.metadata.parameter_layout_sha256 =
                previous.config.metadata.parameter_layout_sha256.clone();
        }
        if config.pins != previous.config.pins
            || config.platform != previous.config.platform
            || config.metadata != previous.config.metadata
            || config.launch_pins != previous.config.launch_pins
        {
            return Err(invalid("the segment boundary changed model or input pins"));
        }
        let mut model = previous.model.model.as_ref().clone();
        model.copy_build_trunk_to_pick_trunk();
        let mut state = CubeModelState::from_model(&model, &previous.optimizer);
        state.pick.reset_optimizer();
        state.value.reset_optimizer();
        let (model, optimizer) = state
            .restore(model, previous.head_updates())
            .map_err(TrainingDriverError::Update)?;
        Self::open_from_state(
            config,
            previous.rows.clone(),
            model,
            optimizer,
            previous.lineage.clone(),
            previous.league.clone(),
            Some((
                PickBoundary {
                    builder_run: previous.config.run.clone(),
                    builder_final_state_sha256: previous.current_pin.clone(),
                },
                &previous.config.root,
            )),
        )
    }

    pub fn checkpoint(&self) -> Result<Checkpoint, TrainingDriverError> {
        Checkpoint::new(
            &self.config,
            CubeModelState::from_model(&self.model.model, &self.optimizer),
            self.league.clone(),
            self.next_update,
            self.lineage.clone(),
            pin_reasons(&self.config, self.next_update),
            self.head_updates(),
        )
    }

    pub fn head_updates(&self) -> HeadUpdates {
        self.optimizer.head_updates
    }

    pub fn league(&self) -> &[LeagueEntry] {
        &self.league
    }

    pub fn next_update(&self) -> u64 {
        self.next_update
    }

    /// Pin of the currently published state, also available after an abandoned batch.
    pub fn checkpoint_state_sha256(&self) -> &str {
        &self.current_pin
    }

    pub fn set_gate_pin(&mut self, pin: PilotPin) -> Result<(), TrainingDriverError> {
        let root = self.config.root.clone();
        persist_gate(&mut self.config, &root, &pin)
    }

    fn current_lineage(&self, selection: &PilotSelection) -> Vec<LineageSegment> {
        let pilot = match selection {
            PilotSelection::Uniform => PilotKind::Uniform,
            PilotSelection::Checkpoint { .. } => PilotKind::Checkpoint,
        };
        let mut lineage = self.lineage.clone();
        if let Some(current) = lineage
            .last_mut()
            .filter(|entry| entry.segment == self.config.segment && entry.run == self.config.run)
        {
            if !current.pilots.contains(&pilot) {
                current.pilots.push(pilot);
            }
        } else {
            lineage.push(LineageSegment {
                segment: self.config.segment,
                run: self.config.run.clone(),
                pilots: vec![pilot],
            });
        }
        lineage
    }

    fn pod_config(&self, pod_index: u64, pilot_info: PilotInfo) -> TrainingPodConfig {
        TrainingPodConfig {
            pod_use: if self.config.label == "shakedown" {
                PodUse::Shakedown
            } else {
                PodUse::Training
            },
            namespace: self.config.namespace.clone(),
            run: self.config.run.clone(),
            pod_index,
            pins: self.config.pins.clone(),
            pilot_info,
            caps: self.config.caps,
            workers: self.config.workers,
        }
    }

    fn collect_batch(
        &self,
        pool: &CubePool,
        update: u64,
        selection: &PilotSelection,
        pilot: &(dyn GamePilot + Sync),
        snapshots: &BTreeMap<String, Snapshot>,
    ) -> Result<Vec<PodRecord>, CollectFailure> {
        let current: Arc<dyn SeatForward> = self.model.seat.clone();
        let mut completed = Vec::new();
        for offset in 0..PODS_PER_UPDATE {
            let pod_index = update * PODS_PER_UPDATE + offset;
            let league_seed = pod_league_seed(&self.config.namespace, &self.config.run, pod_index)
                .map_err(TrainingDriverError::from)
                .map_err(CollectFailure::Error)?;
            let mut stream = SplitMix64::seed(league_seed);
            let first = uniform_index(&mut stream, SEATS);
            let draw = uniform_index(&mut stream, SEATS - 1);
            let second = if draw < first { draw } else { draw + 1 };
            let older = [first.min(second), first.max(second)];
            let roles = training_roles_with_older(older)
                .map_err(TrainingDriverError::from)
                .map_err(CollectFailure::Error)?;
            let mut assignments = BTreeMap::new();
            for chair in older {
                let index = uniform_index(&mut stream, self.league.len());
                assignments.insert(chair, self.league[index].pin.clone());
            }
            let pilot_info = match selection {
                PilotSelection::Uniform => PilotInfo::uniform(),
                PilotSelection::Checkpoint { pin } => PilotInfo {
                    kind: PilotKind::Checkpoint,
                    pin: Some(pin.checkpoint_sha256.clone()),
                    decoding: pin.decoding.clone(),
                },
            };
            let config = self.pod_config(pod_index, pilot_info);
            let mut chairs: [TrainingChair; SEATS] = std::array::from_fn(|chair| {
                let older_pin = assignments.get(&chair);
                let forward = older_pin.map_or_else(
                    || current.clone(),
                    |pin| snapshots[pin].seat.clone() as Arc<dyn SeatForward>,
                );
                let picker: Box<dyn crate::cube_seat_v1::SeatPicker> =
                    if self.config.segment == Segment::BuilderOnly {
                        Box::new(UniformSeatPicker)
                    } else {
                        Box::new(LearnedSeatPicker::new(forward.clone()))
                    };
                let picker_ref = if self.config.segment == Segment::BuilderOnly {
                    SeatRef::uniform()
                } else {
                    SeatRef {
                        kind: SeatKind::Learned,
                        pin: Some(older_pin.unwrap_or(&self.current_pin).clone()),
                    }
                };
                TrainingChair {
                    role: roles[chair],
                    picker_ref,
                    picker,
                    builder_ref: SeatRef {
                        kind: SeatKind::Learned,
                        pin: Some(older_pin.unwrap_or(&self.current_pin).clone()),
                    },
                    builder: Box::new(LearnedSeatBuilder::new(forward)),
                }
            });
            match crate::cube_pod_runner_v1::run_training_pod(pool, &config, &mut chairs, pilot) {
                Ok(record) => {
                    let pending = self
                        .config
                        .root
                        .join("pending")
                        .join(update_name(update))
                        .join("partial");
                    fs::create_dir_all(&pending)
                        .map_err(TrainingDriverError::from)
                        .map_err(CollectFailure::Error)?;
                    let bytes = json_bytes(&record).map_err(CollectFailure::Error)?;
                    write_synced(&pending.join(format!("pod-{pod_index:06}.json")), &bytes)
                        .map_err(CollectFailure::Error)?;
                    completed.push(record);
                }
                Err(PodRunnerError::Stopped(record)) => {
                    return Err(CollectFailure::Stop(BatchFailure {
                        pod: record,
                        completed,
                    }));
                }
                Err(error) => {
                    return Err(CollectFailure::Error(TrainingDriverError::from(error)));
                }
            }
        }
        Ok(completed)
    }

    fn abandon_error(
        &self,
        update: u64,
        pending: &Path,
        first: Option<(u64, u32)>,
        error: &TrainingDriverError,
        pods: Option<&[PodRecord]>,
    ) -> Result<(), TrainingDriverError> {
        let kind = match error {
            TrainingDriverError::Pod(PodRunnerError::ProcessDeath { .. }) => "process_death",
            TrainingDriverError::Update(_) => "update_failed",
            TrainingDriverError::Pod(PodRunnerError::Seat(_)) => "seat_error",
            TrainingDriverError::Io(_) => "io_error",
            TrainingDriverError::Invalid(_) => "invalid",
            _ => "collection_failed",
        };
        let batch = pods.map_or_else(
            || {
                let published = self.config.root.join("updates").join(update_name(update));
                let evidence = if pending.exists() {
                    pending.to_owned()
                } else {
                    published
                };
                Ok(read_pods(&evidence.join("pods.json"))
                    .or_else(|_| read_partial_pods(&evidence))
                    .unwrap_or_default())
            },
            |pods| {
                pods.iter()
                    .map(|pod| serde_json::to_value(pod).map_err(TrainingDriverError::from))
                    .collect()
            },
        )?;
        let mut stop = StopRecord {
            schema: STOP_SCHEMA.to_owned(),
            kind: kind.to_owned(),
            run: self.config.run.clone(),
            batch_index: update,
            pod: None,
            pairing: None,
            game: None,
            decks: None,
            attempts: Vec::new(),
            hang: None,
            first_watchdog_pod: None,
            first_watchdog_game: None,
            second_watchdog_pod: None,
            second_watchdog_game: None,
            firing: None,
            process_death: None,
            error: Some(error.to_string()),
            batch,
        };
        if let TrainingDriverError::Pod(PodRunnerError::ProcessDeath { pod, game, .. }) = error {
            stop.pod = Some(*pod);
            stop.game = Some(*game);
            stop.firing = Some(TripwireFiring::immediate(TripwireRule::ProcessDeath));
            stop.process_death = Some(death_evidence(update.checked_sub(1))?);
        }
        apply_first_watchdog(&mut stop, first);
        ledger_stop(&self.config.root, &stop)?;
        // A fully staged commit can still be published by recovery if the
        // rename failed. Preserve it alongside the error ledger.
        if pending.exists() && matches!(read_commit(pending, update, false), Ok(None)) {
            fs::remove_dir_all(pending)?;
            sync_directory(&self.config.root.join("pending"))?;
        }
        Ok(())
    }

    pub fn run_next_update(
        &mut self,
        pool: &CubePool,
        pilots: &dyn PilotProvider,
    ) -> Result<RunOutcome, TrainingDriverError> {
        check_stop_ledger(&self.config.root)?;
        if self.next_update >= self.config.total_updates() {
            return Ok(RunOutcome::Completed);
        }
        let update = self.next_update;
        let Some(selection) = self.config.pilot.at(update) else {
            let pause = self.config.root.join("pause.json");
            publish_bytes(
                &pause,
                &json_bytes(&serde_json::json!({"update": update, "reason": "missing_gate_pin"}))?,
            )?;
            return Ok(RunOutcome::Paused { update });
        };
        let pause = self.config.root.join("pause.json");
        if pause.exists() {
            fs::remove_file(pause)?;
        }
        let pilot: Arc<dyn GamePilot + Sync> = match &selection {
            PilotSelection::Uniform => pilots.uniform_pilot(),
            PilotSelection::Checkpoint { pin } => pilots.load_checkpoint(pin)?,
        };
        for entry in &self.league {
            if self.snapshots.contains_key(&entry.pin) {
                continue;
            }
            let checkpoint: Checkpoint = read_json(&self.config.root.join(&entry.path))?;
            checkpoint.validate()?;
            if checkpoint.state_sha256 != entry.pin
                || checkpoint.parameter_layout_sha256
                    != self.config.metadata.parameter_layout_sha256
                || checkpoint.head_updates != entry.head_updates
            {
                return Err(invalid("league snapshot differs from its pin"));
            }
            let (model, _) = checkpoint
                .model
                .restore(self.model.model.as_ref().clone(), checkpoint.head_updates)
                .map_err(TrainingDriverError::Update)?;
            let snapshot =
                Snapshot::new(model, self.rows.clone()).map_err(TrainingDriverError::Update)?;
            self.snapshots.insert(entry.pin.clone(), snapshot);
        }
        let pending = self.config.root.join("pending").join(update_name(update));
        if pending.exists() {
            return Err(invalid("an unrecovered pending update remains"));
        }
        self.runner.start_batch();
        let result = self.run_started_batch(pool, update, &selection, pilot.as_ref(), &pending);
        if let Err(error) = &result {
            if !batch_has_stop(&self.config.root, update, &pending)? {
                let first = first_watchdog(&self.config.root, update).unwrap_or_default();
                self.abandon_error(update, &pending, first, error, None)?;
            }
        }
        result
    }

    fn run_started_batch(
        &mut self,
        pool: &CubePool,
        update: u64,
        selection: &PilotSelection,
        pilot: &(dyn GamePilot + Sync),
        pending: &Path,
    ) -> Result<RunOutcome, TrainingDriverError> {
        fs::create_dir_all(pending)?;
        sync_directory(pending)?;
        sync_directory(&self.config.root.join("pending"))?;

        let mut first_watchdog = None;
        let mut void_execution = None;
        let partial = pending.join("partial");
        let pods = loop {
            match self.collect_batch(pool, update, selection, pilot, &self.snapshots) {
                Ok(pods) => break pods,
                Err(CollectFailure::Error(error)) => {
                    self.abandon_error(update, pending, first_watchdog, &error, None)?;
                    if matches!(
                        error,
                        TrainingDriverError::Pod(PodRunnerError::ProcessDeath { .. })
                    ) {
                        return Ok(RunOutcome::Stopped {
                            update,
                            kind: "process_death".into(),
                        });
                    }
                    return Err(error);
                }
                Err(CollectFailure::Stop(failure)) => {
                    let watchdog_game = failure.pod.games.iter().find_map(|game| {
                        game.attempts
                            .iter()
                            .any(|attempt| is_watchdog_end(&attempt.end))
                            .then_some((failure.pod.pod_index, game.game))
                    });
                    if first_watchdog.is_none() && watchdog_game.is_some() {
                        first_watchdog = watchdog_game;
                        let mut record = stop_from_failure(&self.config.run, update, failure);
                        record.kind = "watchdog_void".to_owned();
                        apply_first_watchdog(&mut record, first_watchdog);
                        let bytes = json_bytes(&record)?;
                        publish_bytes(&watchdog_path(&self.config.root, update), &bytes)?;
                        if partial.exists() {
                            fs::remove_dir_all(&partial)?;
                            sync_directory(pending)?;
                        }
                        void_execution = Some(bytes);
                        continue;
                    }
                    let mut stop = stop_from_failure(&self.config.run, update, failure);
                    apply_first_watchdog(&mut stop, first_watchdog);
                    if let Some((second_pod, second_game)) = watchdog_game {
                        let hang = first_watchdog == Some((second_pod, second_game));
                        stop.hang = Some(hang);
                        if hang {
                            stop.firing = Some(TripwireFiring::immediate(TripwireRule::Hang));
                        }
                        stop.second_watchdog_pod = Some(second_pod);
                        stop.second_watchdog_game = Some(second_game);
                    }
                    ledger_stop(&self.config.root, &stop)?;
                    fs::remove_dir_all(pending)?;
                    return Ok(RunOutcome::Stopped {
                        update,
                        kind: stop.kind,
                    });
                }
            }
        };

        let mut tripwire = self.tripwire.clone();
        for pod in &pods {
            tripwire.add_pod(pod, self.census_rates.as_ref());
        }
        let firing = tripwire.firing.clone();
        let before_state = self.current_pin.clone();
        let updated = update_model(
            &self.model.model,
            &self.rows,
            self.optimizer.clone(),
            self.config.segment,
            &pods,
            &self.snapshots,
        )
        .and_then(|(model, optimizer, output)| {
            Ok((Snapshot::new(model, self.rows.clone())?, optimizer, output))
        });
        let (model, optimizer, optimizer_output) = match updated {
            Ok(result) => result,
            Err(message) => {
                let error = TrainingDriverError::Update(message);
                self.abandon_error(update, pending, first_watchdog, &error, Some(&pods))?;
                return Err(error);
            }
        };
        let head_updates = optimizer.head_updates;
        let checkpoint_number = update + 1;
        let model_state = CubeModelState::from_model(&model.model, &optimizer);
        let mut league = self.league.clone();
        let reasons = pin_reasons(&self.config, checkpoint_number);
        let mut league_snapshot_bytes = None;
        if reasons.contains(&CheckpointPinReason::League) {
            let provisional = Checkpoint::new(
                &self.config,
                model_state.clone(),
                league.clone(),
                checkpoint_number,
                self.current_lineage(selection),
                reasons.clone(),
                head_updates,
            )?;
            league_snapshot_bytes = Some(json_bytes(&provisional)?);
            league.push(league_entry(&self.config.root, &provisional)?);
        }
        let checkpoint = Checkpoint::new(
            &self.config,
            model_state,
            league.clone(),
            checkpoint_number,
            self.current_lineage(selection),
            reasons,
            head_updates,
        )?;
        let state_pin = checkpoint.state_sha256.clone();
        let pilot_info = match selection {
            PilotSelection::Uniform => PilotInfo::uniform(),
            PilotSelection::Checkpoint { pin } => PilotInfo {
                kind: PilotKind::Checkpoint,
                pin: Some(pin.checkpoint_sha256.clone()),
                decoding: pin.decoding.clone(),
            },
        };
        let receipt = UpdateReceipt {
            before_state_sha256: before_state,
            after_state_sha256: state_pin.clone(),
            segment: self.config.segment,
            checkpoint_number,
            sets_trained: self.config.train_sets,
            pilot: pilot_info,
            pods: pods.iter().map(|pod| pod.pod_index).collect(),
            execution: u8::from(void_execution.is_some()),
            void_execution_sha256: void_execution.as_deref().map(sha256_hex),
            game_statistics: GameStatistics::from_pods(&pods),
            first_watchdog_pod: first_watchdog.map(|key| key.0),
            first_watchdog_game: first_watchdog.map(|key| key.1),
            league_entries: self.league.iter().map(|entry| entry.pin.clone()).collect(),
            league_trained_pick_entries: self
                .league
                .iter()
                .filter(|entry| entry.head_updates.pick > 0)
                .count(),
            optimizer: optimizer_output,
            head_updates,
            workers: self.config.workers,
        };
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        serde_json::to_writer(&mut encoder, &pods)?;
        let pod_bytes = encoder.finish()?;
        write_synced(&pending.join("pods.json"), &pod_bytes)?;
        write_synced(&pending.join("checkpoint.json"), &json_bytes(&checkpoint)?)?;
        write_synced(&pending.join("update.json"), &json_bytes(&receipt)?)?;
        if let Some(bytes) = &void_execution {
            write_synced(&pending.join("execution-0.json"), bytes)?;
        }
        if partial.exists() {
            fs::remove_dir_all(&partial)?;
        }
        if let Some(bytes) = &league_snapshot_bytes {
            write_synced(&pending.join("league.json"), bytes)?;
        }
        if let Some(firing) = &firing {
            let stop = StopRecord {
                schema: STOP_SCHEMA.to_owned(),
                kind: "tripwire".to_owned(),
                run: self.config.run.clone(),
                batch_index: update,
                pod: None,
                pairing: None,
                game: None,
                decks: None,
                attempts: Vec::new(),
                hang: None,
                first_watchdog_pod: first_watchdog.map(|key| key.0),
                first_watchdog_game: first_watchdog.map(|key| key.1),
                second_watchdog_pod: None,
                second_watchdog_game: None,
                firing: Some(firing.clone()),
                process_death: None,
                error: None,
                batch: pods
                    .iter()
                    .map(|pod| serde_json::to_value(pod).expect("pod json"))
                    .collect(),
            };
            write_synced(&pending.join("stop.json"), &json_bytes(&stop)?)?;
        }
        let names = ["pods.json", "checkpoint.json", "update.json"]
            .into_iter()
            .chain(firing.is_some().then_some("stop.json"))
            .chain(league_snapshot_bytes.is_some().then_some("league.json"))
            .chain(void_execution.is_some().then_some("execution-0.json"));
        let files = names
            .map(|name| {
                let bytes = fs::read(pending.join(name))?;
                Ok((name.to_owned(), sha256_hex(&bytes)))
            })
            .collect::<Result<BTreeMap<_, _>, TrainingDriverError>>()?;
        write_synced(
            &pending.join("commit.json"),
            &json_bytes(&CommitManifest {
                batch_index: update,
                files,
            })?,
        )?;
        sync_directory(pending)?;
        sync_directory(&self.config.root.join("pending"))?;
        let published = self.config.root.join("updates").join(update_name(update));
        fs::rename(pending, &published)?;
        sync_directory(&self.config.root.join("pending"))?;
        sync_directory(&self.config.root.join("updates"))?;
        restore_update_league_snapshot(&self.config.root, &published)?;
        published_stop(&self.config.root, &published, update)?;
        self.next_update += 1;
        self.model = model;
        self.optimizer = optimizer;
        self.league = league;
        self.current_pin = state_pin;
        self.tripwire = tripwire;
        self.lineage = checkpoint.lineage;
        self.runner.finish_batch();
        if firing.is_some() {
            Ok(RunOutcome::Stopped {
                update,
                kind: "tripwire".to_owned(),
            })
        } else {
            Ok(RunOutcome::Published { update })
        }
    }
}

#[path = "cube_training_driver_fixtures_v1.rs"]
pub mod fixtures;

pub fn tripwire_games_for_test(root: &Path) -> Result<u64, TrainingDriverError> {
    let config: RunConfig = read_json(&root.join("run.json"))?;
    Ok(rebuild_tripwire(root, None, config.tripwire_window_games)?.games())
}
