//! Training segments, checkpoints, league snapshots and durable resume for M4.

pub use crate::cube_draft_build_v1::model::training::HeadUpdates;
use crate::cube_draft_build_v1::model::{
    training::{
        fixture::{halt_cases, HaltCase},
        Optimizer,
    },
    ModelCardRowsV1, ModelV1,
};
#[path = "cube_training_driver_model_v1.rs"]
mod model;
use model::TrainingModel;

use crate::cube_draft_v1::{uniform_index, CubePool, GameEnd, SEATS};
use crate::cube_learned_seat_v1::{LearnedSeatBuilder, LearnedSeatPicker, SeatForward};
use crate::cube_pod_runner_v1::{
    collect_halts_and_stop, is_watchdog_end, play_training_game_for_driver, pod_league_seed,
    training_roles_with_older, GameRole, PilotInfo, PilotKind, PodPins, PodRecord, PodRunnerError,
    PodStatus, PodUse, SeatKind, SeatRef, SeatRule, StopAttemptReport, TrainingChair,
    TrainingPodConfig, CLIENT_NAMESPACE, POD_RECORD_SCHEMA,
};
use crate::cube_read_plan_v1::sha256_hex;
use crate::cube_seat_v1::{
    AttemptOutcome, AttemptSpec, GamePilot, UniformPilot, UniformSeatPicker, LEARNED_SAMPLER_ID,
    M4_INTERFACES_VERSION,
};
use crate::durable_publication_v1::{capture_existing_publication_parent_v1, publish_new_bytes_v1};
use crate::state::SplitMix64;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
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
        match (&self.switch, &resumed.switch) {
            (None, None) => true,
            (Some(before), Some(after)) => before.gate == after.gate || before.gate.is_none(),
            _ => false,
        }
    }
}

pub trait PilotProvider {
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
        max_games: Option<u64>,
        max_rate_bits: Option<u64>,
    },
    Card {
        card: u16,
        max_games: Option<u64>,
        max_rate_bits: Option<u64>,
    },
}

impl TripwireRule {
    pub fn stop_kind_count(stop_kind: impl Into<String>, max_games: u64) -> Self {
        Self::StopKind {
            stop_kind: stop_kind.into(),
            max_games: Some(max_games),
            max_rate_bits: None,
        }
    }

    pub fn stop_kind_rate(stop_kind: impl Into<String>, max_rate: f64) -> Self {
        Self::StopKind {
            stop_kind: stop_kind.into(),
            max_games: None,
            max_rate_bits: Some(max_rate.to_bits()),
        }
    }

    fn validate(&self) -> Result<(), TrainingDriverError> {
        let (max_games, max_rate) = match self {
            Self::StopKind {
                stop_kind,
                max_games,
                max_rate_bits,
            } => {
                if stop_kind.is_empty() {
                    return Err(invalid("a tripwire stop kind cannot be empty"));
                }
                (max_games, max_rate_bits)
            }
            Self::Card {
                max_games,
                max_rate_bits,
                ..
            } => (max_games, max_rate_bits),
        };
        if max_games.is_none() && max_rate.is_none() {
            return Err(invalid("a tripwire rule needs a count or rate threshold"));
        }
        if max_games == &Some(0) || max_rate.is_some_and(|bits| !f64::from_bits(bits).is_finite()) {
            return Err(invalid("invalid tripwire threshold"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TripwireMode {
    Q9,
    Full { rules: Vec<TripwireRule> },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
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
    #[serde(default)]
    pub tripwire_window_games: Vec<u64>,
    pub pins: PodPins,
    pub metadata: CheckpointMetadata,
    pub caps: crate::cube_pod_runner_v1::PodCaps,
    pub workers: usize,
}

impl RunConfig {
    pub fn shakedown(root: impl Into<PathBuf>, run: impl Into<String>, segment: Segment) -> Self {
        let train_sets = segment.train_sets();
        Self {
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
            tripwire_window_games: Vec::new(),
            pins: PodPins::default(),
            metadata: CheckpointMetadata::default(),
            caps: crate::cube_pod_runner_v1::PodCaps::training_a2(),
            workers: 1,
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
            || self.workers == 0
        {
            return Err(invalid("invalid update plan or trained sets"));
        }
        if self.label != "shakedown" {
            if matches!(self.pilot.initial, PilotSelection::Uniform) {
                return Err(invalid(
                    "a non-shakedown run cannot start from a uniform-pilot lineage",
                ));
            }
        }
        if let TripwireMode::Full { rules } = &self.tripwire {
            for rule in rules {
                rule.validate()?;
            }
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

    fn append_canonical(&self, bytes: &mut Vec<u8>) {
        for values in [&self.parameters, &self.first_moment, &self.second_moment] {
            bytes.extend_from_slice(&(values.len() as u64).to_le_bytes());
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&self.adam_step.to_le_bytes());
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
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.build.append_canonical(&mut bytes);
        self.pick.append_canonical(&mut bytes);
        self.value.append_canonical(&mut bytes);
        bytes
    }

    fn state_sha256(&self) -> String {
        sha256_hex(&self.canonical_bytes())
    }

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
        let state_sha256 = model.state_sha256();
        Ok(Self {
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
            state_sha256,
            head_updates,
        })
    }

    fn validate(&self) -> Result<(), TrainingDriverError> {
        if self.schema != CHECKPOINT_SCHEMA || self.state_sha256 != self.model.state_sha256() {
            return Err(invalid("checkpoint schema or state digest differs"));
        }
        self.model.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateReceipt {
    pub before_state_sha256: String,
    pub after_state_sha256: String,
    pub segment: Segment,
    pub update: u64,
    pub sets_trained: TrainSets,
    pub pilot: PilotInfo,
    pub pods: Vec<u64>,
    pub league_entries: Vec<String>,
    pub league_trained_pick_entries: usize,
    pub optimizer: OptimizerOutput,
    pub head_updates: HeadUpdates,
    pub workers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitManifest {
    pub update: u64,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TripwireFiring {
    pub window: u64,
    pub rule: TripwireRule,
    pub games: u64,
    pub denominator: u64,
    pub observed_rate_bits: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecord {
    pub schema: String,
    pub kind: String,
    pub run: String,
    pub update: u64,
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
    pub batch: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Published { update: u64 },
    Stopped { update: u64, kind: String },
    Paused { update: u64 },
    Completed,
}

#[derive(Debug, Clone, Default)]
struct TripwireWindow {
    games: u64,
    stop_games: BTreeMap<String, u64>,
    card_games: BTreeMap<u16, u64>,
    card_denominator: BTreeMap<u16, u64>,
}

#[derive(Debug, Clone)]
struct TripwireTracker {
    window_sizes: Vec<u64>,
    closed_games: u64,
    closed_windows: usize,
    open: TripwireWindow,
    firing: Option<TripwireFiring>,
}

fn tripwire_window_sizes(games: u64) -> Vec<u64> {
    let windows = games / TRIPWIRE_WINDOW_GAMES;
    let tail = games % TRIPWIRE_WINDOW_GAMES;
    let mut sizes = vec![TRIPWIRE_WINDOW_GAMES; windows as usize];
    if tail > 0 {
        match sizes.last_mut() {
            Some(last) if tail < TRIPWIRE_TAIL_MERGE_GAMES => *last += tail,
            _ => sizes.push(tail),
        }
    }
    sizes
}

impl TripwireTracker {
    fn new(window_sizes: Vec<u64>) -> Self {
        Self {
            window_sizes,
            closed_games: 0,
            closed_windows: 0,
            open: TripwireWindow::default(),
            firing: None,
        }
    }

    fn rule_match(
        rules: &[TripwireRule],
        window: &TripwireWindow,
        window_index: u64,
        count_rules: bool,
        rate_rules: bool,
    ) -> Option<TripwireFiring> {
        for rule in rules {
            let (games, denominator) = match rule {
                TripwireRule::StopKind { stop_kind, .. } => (
                    window.stop_games.get(stop_kind).copied().unwrap_or(0),
                    window.games,
                ),
                TripwireRule::Card { card, .. } => (
                    window.card_games.get(card).copied().unwrap_or(0),
                    window.card_denominator.get(card).copied().unwrap_or(0),
                ),
            };
            let (max_games, max_rate_bits) = match rule {
                TripwireRule::StopKind {
                    max_games,
                    max_rate_bits,
                    ..
                }
                | TripwireRule::Card {
                    max_games,
                    max_rate_bits,
                    ..
                } => (max_games, max_rate_bits),
            };
            let count_fired = count_rules && max_games.is_some_and(|limit| games >= limit);
            let rate = if denominator == 0 {
                0.0
            } else {
                games as f64 / denominator as f64
            };
            let rate_fired =
                rate_rules && max_rate_bits.is_some_and(|bits| rate > f64::from_bits(bits));
            if count_fired || rate_fired {
                return Some(TripwireFiring {
                    window: window_index,
                    rule: rule.clone(),
                    games,
                    denominator,
                    observed_rate_bits: Some(rate.to_bits()),
                });
            }
        }
        None
    }

    fn add_game(
        &mut self,
        decks: &[Vec<u16>],
        halted: &[(String, Option<u16>)],
        rules: &[TripwireRule],
    ) {
        let stop_kinds: BTreeSet<String> = halted.iter().map(|(kind, _)| kind.clone()).collect();
        let cards: BTreeSet<u16> = halted.iter().filter_map(|(_, card)| *card).collect();
        let present: BTreeSet<u16> = decks.iter().flatten().copied().collect();
        self.open.games += 1;
        for kind in stop_kinds {
            *self.open.stop_games.entry(kind).or_default() += 1;
        }
        for card in cards {
            *self.open.card_games.entry(card).or_default() += 1;
        }
        for card in present {
            *self.open.card_denominator.entry(card).or_default() += 1;
        }
        if self.firing.is_none() {
            let index = self.closed_windows as u64;
            self.firing = Self::rule_match(rules, &self.open, index, true, false);
        }
        if self.window_sizes.get(self.closed_windows) == Some(&self.open.games) {
            if self.firing.is_none() {
                let index = self.closed_windows as u64;
                self.firing = Self::rule_match(rules, &self.open, index, false, true);
            }
            self.closed_windows += 1;
            self.closed_games += std::mem::take(&mut self.open).games;
        }
    }

    fn add_pod(&mut self, pod: &PodRecord, rules: &[TripwireRule]) {
        for game in &pod.games {
            let decks: Vec<Vec<u16>> = game
                .decks
                .iter()
                .filter_map(|id| pod.decks.get(*id as usize).map(|deck| deck.cards.clone()))
                .collect();
            let halted: Vec<(String, Option<u16>)> = game
                .attempts
                .iter()
                .filter(|attempt| attempt.halted)
                .map(|attempt| {
                    (
                        attempt
                            .stop_kind
                            .clone()
                            .unwrap_or_else(|| "unknown".to_owned()),
                        attempt.last_action_source,
                    )
                })
                .collect();
            self.add_game(&decks, &halted, rules);
        }
    }

    fn add_pod_json(
        &mut self,
        pod: &Value,
        rules: &[TripwireRule],
    ) -> Result<(), TrainingDriverError> {
        let empty = Vec::new();
        let decks = pod["decks"].as_array().unwrap_or(&empty);
        let games = pod["games"].as_array().unwrap_or(&empty);
        for game in games {
            let mut game_decks = Vec::new();
            for id in game["decks"].as_array().unwrap_or(&empty) {
                let index = id.as_u64().ok_or_else(|| invalid("invalid deck id"))? as usize;
                let cards = decks
                    .get(index)
                    .and_then(|deck| deck["cards"].as_array())
                    .ok_or_else(|| invalid("game names a missing deck"))?
                    .iter()
                    .map(|card| {
                        card.as_u64()
                            .and_then(|id| u16::try_from(id).ok())
                            .ok_or_else(|| invalid("invalid card id"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                game_decks.push(cards);
            }
            let mut halted = Vec::new();
            for attempt in game["attempts"].as_array().unwrap_or(&empty) {
                if attempt["halted"].as_bool() == Some(true) {
                    halted.push((
                        attempt["stop_kind"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_owned(),
                        attempt["last_action_source"]
                            .as_u64()
                            .and_then(|id| u16::try_from(id).ok()),
                    ));
                }
            }
            self.add_game(&game_decks, &halted, rules);
        }
        Ok(())
    }
}

fn rules_for(mode: &TripwireMode) -> &[TripwireRule] {
    match mode {
        TripwireMode::Q9 => &[],
        TripwireMode::Full { rules } => rules,
    }
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
        if stop.update == update {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn ledger_stop(root: &Path, stop: &StopRecord) -> Result<PathBuf, TrainingDriverError> {
    fs::create_dir_all(stop_directory(root))?;
    let path = if let Some(path) = ledger_path_for_update(root, stop.update)? {
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

fn read_commit(path: &Path) -> Result<Option<CommitManifest>, TrainingDriverError> {
    let commit_path = path.join("commit.json");
    if !commit_path.exists() {
        return Ok(None);
    }
    let commit: CommitManifest = read_json(&commit_path)?;
    for (name, digest) in &commit.files {
        let bytes = fs::read(path.join(name))?;
        if sha256_hex(&bytes) != *digest {
            return Ok(None);
        }
    }
    Ok(Some(commit))
}

fn recover(root: &Path, run: &str) -> Result<(), TrainingDriverError> {
    let pending = root.join("pending");
    let updates = root.join("updates");
    fs::create_dir_all(&pending)?;
    fs::create_dir_all(&updates)?;
    let mut directories = Vec::new();
    for entry in fs::read_dir(&pending)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    for directory in directories {
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid pending update name"))?;
        let update = name
            .strip_prefix("update-")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| invalid("invalid pending update name"))?;
        if read_commit(&directory)?.is_some() {
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
            let stop_path = destination.join("stop.json");
            if stop_path.exists() {
                let stop: StopRecord = read_json(&stop_path)?;
                ledger_stop(root, &stop)?;
            }
            continue;
        }
        let mut batch = Vec::new();
        let partial = directory.join("partial");
        if partial.exists() {
            let mut files: Vec<PathBuf> = fs::read_dir(&partial)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<_, _>>()?;
            files.sort();
            for path in files {
                batch.push(serde_json::from_slice(&fs::read(path)?)?);
            }
        }
        if ledger_path_for_update(root, update)?.is_none() {
            ledger_stop(
                root,
                &StopRecord {
                    schema: STOP_SCHEMA.to_owned(),
                    kind: "process_death".to_owned(),
                    run: run.to_owned(),
                    update,
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
                    batch,
                },
            )?;
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
        let stop_path = directory.join("stop.json");
        if stop_path.exists() {
            let stop: StopRecord = read_json(&stop_path)?;
            ledger_stop(root, &stop)?;
        }
    }
    Ok(())
}

fn published_update_indices(root: &Path) -> Result<Vec<u64>, TrainingDriverError> {
    let updates = root.join("updates");
    if !updates.exists() {
        return Ok(Vec::new());
    }
    let mut indices = Vec::new();
    for entry in fs::read_dir(&updates)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("invalid update name"))?;
        let update = name
            .strip_prefix("update-")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| invalid("invalid update name"))?;
        if read_commit(&entry.path())?.is_none() {
            return Err(invalid("published update has an invalid commit"));
        }
        indices.push(update);
    }
    indices.sort_unstable();
    if indices
        .iter()
        .enumerate()
        .any(|(index, update)| index as u64 != *update)
    {
        return Err(invalid("published updates are not contiguous from zero"));
    }
    Ok(indices)
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
    mode: &TripwireMode,
    window_sizes: Vec<u64>,
) -> Result<TripwireTracker, TrainingDriverError> {
    let mut tracker = TripwireTracker::new(window_sizes);
    let rules = rules_for(mode);
    for update in published_update_indices(root)? {
        for pod in read_pods(
            &root
                .join("updates")
                .join(update_name(update))
                .join("pods.json"),
        )? {
            tracker.add_pod_json(&pod, rules)?;
        }
    }
    Ok(tracker)
}

fn pin_reasons(config: &RunConfig, checkpoint: u64) -> Vec<CheckpointPinReason> {
    let mut reasons = Vec::new();
    if checkpoint % 16 == 0 {
        reasons.push(CheckpointPinReason::League);
    }
    if checkpoint == config.total_updates() {
        reasons.push(CheckpointPinReason::SegmentEnd);
    }
    if config.segment == Segment::BuilderOnly && matches!(checkpoint, 32 | 64 | 96 | 128) {
        reasons.push(CheckpointPinReason::DoseRead);
        reasons.push(CheckpointPinReason::OwnerView);
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
    let checkpoint: Checkpoint = read_json(&update.join("checkpoint.json"))?;
    let entry = league_entry(root, &checkpoint)?;
    publish_bytes(&root.join(entry.path), &fs::read(snapshot)?)
}

pub fn prune_checkpoint(path: &Path) -> Result<(), TrainingDriverError> {
    let checkpoint: Checkpoint = read_json(path)?;
    if !checkpoint.pin_reasons.is_empty() {
        return Err(invalid("a pinned checkpoint cannot be pruned"));
    }
    fs::remove_file(path)?;
    Ok(())
}

struct BatchFailure {
    pod: PodRecord,
    completed: Vec<PodRecord>,
}

enum CollectFailure {
    Stop(BatchFailure),
    Error(TrainingDriverError),
}

fn stop_from_failure(run: &str, update: u64, failure: BatchFailure) -> StopRecord {
    let pod = failure.pod;
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
        update,
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
        batch: batch
            .iter()
            .map(|pod| serde_json::to_value(pod).expect("pod json"))
            .collect(),
    }
}

pub struct TrainingDriver {
    config: RunConfig,
    model: Arc<TrainingModel>,
    optimizer: Optimizer,
    rows: Arc<ModelCardRowsV1>,
    next_update: u64,
    lineage: Vec<LineageSegment>,
    league: Vec<LeagueEntry>,
    current_pin: String,
    tripwire: TripwireTracker,
}

impl TrainingDriver {
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
        let model = ModelV1::initialise_v1(
            rows.rows(),
            rows.table_width(),
            rows.dense_width(),
            init_seed,
        )
        .map_err(|error| TrainingDriverError::Update(error.to_string()))?;
        let optimizer = Optimizer::new(&model);
        Self::open_from_state(config, rows, model, optimizer, lineage, Vec::new())
    }

    fn open_from_state(
        mut config: RunConfig,
        rows: Arc<ModelCardRowsV1>,
        initial_model: ModelV1,
        initial_optimizer: Optimizer,
        initial_lineage: Vec<LineageSegment>,
        initial_league: Vec<LeagueEntry>,
    ) -> Result<Self, TrainingDriverError> {
        let layout = initial_model
            .parameter_layout_sha256()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if config.metadata.parameter_layout_sha256.is_empty() {
            config.metadata.parameter_layout_sha256 = layout.clone();
        } else if config.metadata.parameter_layout_sha256 != layout {
            return Err(invalid(
                "configured parameter layout differs from the model",
            ));
        }
        config.validate()?;
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
        fs::create_dir_all(&config.root)?;
        recover(&config.root, &config.run)?;
        check_stop_ledger(&config.root)?;
        let run_path = config.root.join("run.json");
        if run_path.exists() {
            let saved: RunConfig = read_json(&run_path)?;
            if saved.namespace != config.namespace
                || saved.run != config.run
                || saved.segment != config.segment
                || saved.update_range != config.update_range
                || saved.label != config.label
                || saved.train_sets != config.train_sets
                || saved.pods_per_update != config.pods_per_update
                || saved.caps != config.caps
                || saved.pins != config.pins
                || saved.metadata != config.metadata
                || saved.tripwire_window_games != config.tripwire_window_games
                || saved.tripwire != config.tripwire
                || !saved.pilot.compatible_resume(&config.pilot)
            {
                return Err(invalid("resume changed a run pin, plan or tripwire mode"));
            }
        } else {
            publish_bytes(&run_path, &json_bytes(&config)?)?;
        }

        let indices = published_update_indices(&config.root)?;
        let initial_path = config.root.join("initial").join("checkpoint.json");
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
                    vec![CheckpointPinReason::SegmentStart],
                    initial_optimizer.head_updates,
                )?,
                true,
            )
        };
        checkpoint.validate()?;
        if checkpoint.run != config.run
            || checkpoint.segment != config.segment
            || checkpoint.parameter_layout_sha256 != layout
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
            }
            publish_bytes(&initial_path, &json_bytes(&checkpoint)?)?;
        }
        let next_update = indices.last().map_or(0, |update| update + 1);
        let tripwire = rebuild_tripwire(
            &config.root,
            &config.tripwire,
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
        Ok(Self {
            current_pin: checkpoint.state_sha256,
            model: Arc::new(
                TrainingModel::new(model, rows.clone()).map_err(TrainingDriverError::Update)?,
            ),
            optimizer,
            rows,
            next_update,
            lineage: checkpoint.lineage.clone(),
            league: checkpoint.league.clone(),
            tripwire,
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
        if config.namespace != previous.config.namespace || config.run == previous.config.run {
            return Err(invalid(
                "the pick phase needs a new run id in the same namespace",
            ));
        }
        if config.metadata.parameter_layout_sha256.is_empty() {
            config.metadata.parameter_layout_sha256 =
                previous.config.metadata.parameter_layout_sha256.clone();
        }
        if config.pins != previous.config.pins || config.metadata != previous.config.metadata {
            return Err(invalid("the segment boundary changed model or input pins"));
        }
        for entry in &previous.league {
            let source = previous.config.root.join(&entry.path);
            let destination = config.root.join(&entry.path);
            if source != destination {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                publish_bytes(&destination, &fs::read(source)?)?;
            }
        }
        let mut model = previous.model.parameters().clone();
        model.copy_build_trunk_to_pick_trunk();
        let mut state = CubeModelState::from_model(&model, &previous.optimizer);
        state.pick.reset_optimizer();
        state.value.reset_optimizer();
        let (model, optimizer) = state
            .restore(model, previous.head_updates())
            .map_err(TrainingDriverError::Update)?;
        let lineage = if previous.lineage.iter().any(|entry| {
            entry.segment == previous.config.segment && entry.run == previous.config.run
        }) {
            previous.lineage.clone()
        } else {
            let mut lineage = previous.lineage.clone();
            lineage.push(LineageSegment {
                segment: previous.config.segment,
                run: previous.config.run.clone(),
                pilots: vec![PilotKind::Uniform],
            });
            lineage
        };
        config.label = previous.config.label.clone();
        Self::open_from_state(
            config,
            previous.rows.clone(),
            model,
            optimizer,
            lineage,
            previous.league.clone(),
        )
    }

    pub fn checkpoint(&self) -> Result<Checkpoint, TrainingDriverError> {
        Checkpoint::new(
            &self.config,
            CubeModelState::from_model(self.model.parameters(), &self.optimizer),
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

    pub fn set_gate_pin(&mut self, pin: PilotPin) -> Result<(), TrainingDriverError> {
        let switch = self
            .config
            .pilot
            .switch
            .as_mut()
            .ok_or_else(|| invalid("the run has no pilot switch"))?;
        if let Some(existing) = &switch.gate {
            if existing != &pin {
                return Err(invalid("the gate pin is already fixed"));
            }
        }
        switch.gate = Some(pin);
        write_synced(
            &self.config.root.join("run.json"),
            &json_bytes(&self.config)?,
        )
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
        snapshots: &BTreeMap<String, Arc<TrainingModel>>,
    ) -> Result<Vec<PodRecord>, CollectFailure> {
        let current: Arc<dyn SeatForward> = self.model.clone();
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
                    |pin| snapshots[pin].clone() as Arc<dyn SeatForward>,
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
                        pod: *record,
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
            PilotSelection::Uniform => Arc::new(UniformPilot),
            PilotSelection::Checkpoint { pin } => pilots.load_checkpoint(pin)?,
        };
        let snapshots = self
            .league
            .iter()
            .map(|entry| {
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
                    .restore(self.model.parameters().clone(), checkpoint.head_updates)
                    .map_err(TrainingDriverError::Update)?;
                let snapshot = TrainingModel::new(model, self.rows.clone())
                    .map_err(TrainingDriverError::Update)?;
                Ok((entry.pin.clone(), Arc::new(snapshot)))
            })
            .collect::<Result<BTreeMap<_, _>, TrainingDriverError>>()?;
        let pending = self.config.root.join("pending").join(update_name(update));
        if pending.exists() {
            return Err(invalid("an unrecovered pending update remains"));
        }
        fs::create_dir_all(&pending)?;
        sync_directory(&pending)?;
        sync_directory(&self.config.root.join("pending"))?;

        let mut first_watchdog = None;
        let pods = loop {
            match self.collect_batch(pool, update, &selection, pilot.as_ref(), &snapshots) {
                Ok(pods) => break pods,
                Err(CollectFailure::Error(error)) => return Err(error),
                Err(CollectFailure::Stop(failure)) => {
                    let watchdog_game = failure.pod.games.iter().find_map(|game| {
                        game.attempts
                            .iter()
                            .any(|attempt| is_watchdog_end(&attempt.end))
                            .then_some((failure.pod.pod_index, game.game))
                    });
                    if first_watchdog.is_none() && watchdog_game.is_some() {
                        first_watchdog = watchdog_game;
                        fs::remove_dir_all(&pending)?;
                        fs::create_dir_all(&pending)?;
                        continue;
                    }
                    let mut stop = stop_from_failure(&self.config.run, update, failure);
                    if let Some((second_pod, second_game)) = watchdog_game {
                        stop.hang = Some(first_watchdog == Some((second_pod, second_game)));
                        stop.first_watchdog_pod = first_watchdog.map(|key| key.0);
                        stop.first_watchdog_game = first_watchdog.map(|key| key.1);
                        stop.second_watchdog_pod = Some(second_pod);
                        stop.second_watchdog_game = Some(second_game);
                    }
                    ledger_stop(&self.config.root, &stop)?;
                    fs::remove_dir_all(&pending)?;
                    return Ok(RunOutcome::Stopped {
                        update,
                        kind: stop.kind,
                    });
                }
            }
        };

        let mut tripwire = self.tripwire.clone();
        for pod in &pods {
            tripwire.add_pod(pod, rules_for(&self.config.tripwire));
        }
        let firing = tripwire.firing.clone();
        let before_state = self.current_pin.clone();
        let (model, optimizer, optimizer_output) = self
            .model
            .update(
                self.optimizer.clone(),
                self.config.segment,
                &pods,
                &snapshots,
            )
            .map_err(TrainingDriverError::Update)?;
        let head_updates = optimizer.head_updates;
        let checkpoint_number = update + 1;
        let model_state = CubeModelState::from_model(model.parameters(), &optimizer);
        let state_pin = model_state.state_sha256();
        let mut league = self.league.clone();
        let reasons = pin_reasons(&self.config, checkpoint_number);
        let mut league_snapshot_bytes = None;
        if reasons.contains(&CheckpointPinReason::League) {
            let provisional = Checkpoint::new(
                &self.config,
                model_state.clone(),
                league.clone(),
                checkpoint_number,
                self.current_lineage(&selection),
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
            self.current_lineage(&selection),
            reasons,
            head_updates,
        )?;
        let pilot_info = match &selection {
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
            update: checkpoint_number,
            sets_trained: self.config.train_sets,
            pilot: pilot_info,
            pods: pods.iter().map(|pod| pod.pod_index).collect(),
            league_entries: league.iter().map(|entry| entry.pin.clone()).collect(),
            league_trained_pick_entries: league
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
        if let Some(bytes) = &league_snapshot_bytes {
            write_synced(&pending.join("league.json"), bytes)?;
        }
        if let Some(firing) = &firing {
            let stop = StopRecord {
                schema: STOP_SCHEMA.to_owned(),
                kind: "tripwire".to_owned(),
                run: self.config.run.clone(),
                update,
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
                firing: Some(firing.clone()),
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
            .chain(league_snapshot_bytes.is_some().then_some("league.json"));
        let files = names
            .map(|name| {
                let bytes = fs::read(pending.join(name))?;
                Ok((name.to_owned(), sha256_hex(&bytes)))
            })
            .collect::<Result<BTreeMap<_, _>, TrainingDriverError>>()?;
        write_synced(
            &pending.join("commit.json"),
            &json_bytes(&CommitManifest { update, files })?,
        )?;
        sync_directory(&pending)?;
        sync_directory(&self.config.root.join("pending"))?;
        let published = self.config.root.join("updates").join(update_name(update));
        fs::rename(&pending, &published)?;
        sync_directory(&self.config.root.join("pending"))?;
        sync_directory(&self.config.root.join("updates"))?;
        restore_update_league_snapshot(&self.config.root, &published)?;
        if published.join("stop.json").exists() {
            let stop: StopRecord = read_json(&published.join("stop.json"))?;
            ledger_stop(&self.config.root, &stop)?;
        }
        self.next_update += 1;
        self.model = Arc::new(model);
        self.optimizer = optimizer;
        self.league = league;
        self.current_pin = state_pin;
        self.tripwire = tripwire;
        self.lineage = checkpoint.lineage;
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

struct TwoActionPilot {
    case: HaltCase,
    deterministic_halt: bool,
    game_seed: u64,
}

impl GamePilot for TwoActionPilot {
    fn play(
        &self,
        spec: &AttemptSpec<'_>,
        _caps: &crate::cube_draft_v1::GameCaps,
    ) -> AttemptOutcome {
        let attempt = (0..=3u8)
            .find(|attempt| {
                crate::cube_pod_runner_v1::attempt_seeds(self.game_seed, *attempt).2
                    == spec.action_seed
            })
            .expect("two-action attempt seed");
        let halt = if self.deterministic_halt {
            self.case.deterministic_halt()
        } else {
            self.case.halts(usize::from(attempt))
        };
        if halt {
            return AttemptOutcome {
                decisions: 1,
                physical_decisions: Some(1),
                policy_steps: Some(1),
                end: GameEnd::DecisionCap,
                last_action_source: None,
            };
        }
        let result = self.case.actions[0] as i8 - self.case.actions[1] as i8;
        AttemptOutcome {
            decisions: 1,
            physical_decisions: Some(1),
            policy_steps: Some(1),
            end: GameEnd::Natural {
                winner_seat: match result {
                    1 => Some(spec.seats[0]),
                    -1 => Some(spec.seats[1]),
                    _ => None,
                },
                reason: "fixture".to_owned(),
            },
            last_action_source: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TwoActionPart3Report {
    pub published_mean: f64,
    pub published_batches: usize,
    pub stopped_batches: usize,
}

pub fn run_two_action_part3(
    root: &Path,
    deterministic_halt: bool,
) -> Result<TwoActionPart3Report, TrainingDriverError> {
    fs::create_dir_all(root)?;
    let decks = [
        crate::cube_draft_v1::Deck::default(),
        crate::cube_draft_v1::Deck::default(),
    ];
    let mut derivative_sum = 0.0;
    let mut published = 0usize;
    let mut stopped = 0usize;
    for (index, case) in halt_cases().enumerate() {
        if deterministic_halt && case.pattern != 15 {
            continue;
        }
        let game_seed = index as u64;
        let pilot = TwoActionPilot {
            case,
            deterministic_halt,
            game_seed,
        };
        let task = crate::cube_pod_runner_v1::GameTask {
            game: 0,
            group: 0,
            role: GameRole::Reference,
            pairing: Some(0),
            decks: [0, 1],
            chairs: [Some(0), Some(1)],
            seats: [0, 1],
            seat_rule: SeatRule::Schedule,
            game_seed,
            episode: 0,
        };
        let caps = crate::cube_pod_runner_v1::PodCaps::training_a2();
        let game = play_training_game_for_driver(&task, &decks, &pilot, &caps)?;
        let (halts, status, stop) = collect_halts_and_stop(std::slice::from_ref(&game), true);
        let pod = PodRecord {
            schema: POD_RECORD_SCHEMA,
            interfaces: M4_INTERFACES_VERSION,
            pod_use: PodUse::Test,
            namespace: "test".to_owned(),
            run: "v1".to_owned(),
            pod_index: 0,
            seeds: Default::default(),
            pins: PodPins::default(),
            pilot: PilotInfo::uniform(),
            caps,
            draft: None,
            chairs: Vec::new(),
            decks: Vec::new(),
            games: vec![game],
            paired_builds: Vec::new(),
            halts,
            status,
            stop,
        };
        if pod.status == PodStatus::Stopped {
            stopped += 1;
            let record = stop_from_failure(
                "v1",
                index as u64,
                BatchFailure {
                    pod,
                    completed: Vec::new(),
                },
            );
            ledger_stop(root, &record)?;
            if check_stop_ledger(root) != Err(TrainingDriverError::StoppedWithoutResolution) {
                return Err(invalid("a stop without resolution did not refuse resume"));
            }
        } else {
            published += 1;
            let expected = case.actions[0] as i8 - case.actions[1] as i8;
            if pod.games[0].scores != Some([expected, -expected]) {
                return Err(invalid("two-action replay changed the completed score"));
            }
            derivative_sum += case.derivative;
        }
    }
    let total = if deterministic_halt { 4.0 } else { 64.0 };
    let published_mass = published as f64 / total;
    Ok(TwoActionPart3Report {
        published_mean: if published == 0 {
            f64::NAN
        } else {
            derivative_sum / total / published_mass
        },
        published_batches: published,
        stopped_batches: stopped,
    })
}

pub fn two_action_dropped_mean() -> f64 {
    let (sum, count) = halt_cases()
        .filter(|case| !case.halts(0))
        .fold((0.0, 0), |(sum, count), case| {
            (sum + case.derivative, count + 1)
        });
    sum / f64::from(count)
}

pub fn tripwire_games_for_test(
    root: &Path,
    mode: &TripwireMode,
) -> Result<u64, TrainingDriverError> {
    let config: RunConfig = read_json(&root.join("run.json"))?;
    let tracker = rebuild_tripwire(root, mode, config.tripwire_window_games)?;
    Ok(tracker.closed_games + tracker.open.games)
}
