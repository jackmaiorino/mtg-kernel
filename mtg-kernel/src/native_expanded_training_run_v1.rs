//! Resumable training with CPU V3 collection and an explicit update backend.
//! This successor owns its checkpoints; it does not reinterpret legacy Stores.

use crate::durable_publication_v1::{capture_existing_publication_parent_v1, publish_new_bytes_v1};
use crate::expanded_deck_training_v1::e_supported_teacher_v1::{
    continuation_identity as e_supported_continuation_identity_v1, ESupportedSlotV1,
    ESupportedTeacherOptionsV1,
};
use crate::expanded_deck_training_v1::no_state_digest_v1::{
    is_converted_source_v1, policy_contract_record_v1,
};
use crate::expanded_deck_training_v1::search_targets_v1::{
    continuation_identity as search_targets_continuation_identity_v1, SearchTargetsOpponentV1,
    SearchTargetsOptionsV1,
};
use crate::expanded_deck_training_v1::soft_q_teacher_v1::{
    continuation_identity as soft_q_continuation_identity_v1, SoftQArmV1, SoftQTeacherOptionsV1,
};
use crate::expanded_deck_training_v1::{
    execute_v1, line_b_frozen_sha256_v1, load_expanded_inference_bound_v1,
    load_expanded_update_inference_v1, take_collection_fields_v1, CollectionSamplerV1,
    CollectionTrajectoryFieldsV1, ExpandedEpisodeV1, ExpandedInferenceIdentityV1,
    ExpandedLossSelectionV1, ExpandedModelSourceV1, ExpandedTrainingCommandV1,
    ExpandedUpdateBackendV1, LineBUpdateOptionsV1, PinnedFileV1, UpdateBackwardExecutionV1,
    DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
};
use crate::native_flat_tensorizer_v3::{FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3};
use crate::sideboard::RegisteredDeckV1;
use crate::training_math_v1::TrainingMathV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const SCHEMA: &str = "mtg-kernel-native-expanded-training-run/v1";
const SMALL_CAP: u64 = 16 * 1024 * 1024;
const ARTIFACT_CAP: u64 = 512 * 1024 * 1024;

fn default_collection_workers() -> usize {
    1
}

fn is_serial_collection(workers: &usize) -> bool {
    *workers == 1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedExpandedOpponentV1 {
    pub id: String,
    pub source: ExpandedModelSourceV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpandedOpponentAssignmentV1 {
    Current,
    Initial,
    Fixed { id: String },
    CompletedIteration { index: usize },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledExpandedEpisodeV1 {
    pub episode: ExpandedEpisodeV1,
    pub opponent: ExpandedOpponentAssignmentV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedRunIterationV1 {
    pub episodes: Vec<ScheduledExpandedEpisodeV1>,
}

/// Frozen E supported-choice teacher for every update of a run
/// (`e_supported_update`). `slots[i]` is update i's schedule; an empty entry is
/// a declared ordinary update that still preserves the E continuation identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ESupportedRunOptionsV1 {
    pub teacher: ExpandedModelSourceV1,
    pub treatment: bool,
    pub prior_deck_indices: Vec<usize>,
    pub selection_transition_cap: u64,
    pub slots: Vec<Vec<ESupportedSlotV1>>,
}

impl ESupportedRunOptionsV1 {
    fn teacher_options_v1(&self, index: usize) -> ESupportedTeacherOptionsV1 {
        ESupportedTeacherOptionsV1 {
            teacher: self.teacher.clone(),
            treatment: self.treatment,
            slots: self.slots[index].clone(),
            prior_deck_indices: self.prior_deck_indices.clone(),
            selection_transition_cap: self.selection_transition_cap,
        }
    }
}

/// SQ1 soft rollout-Q teaching for every update of a run (collab
/// LANES/soft-q-teaching-20261018): every episode of update i is a slot, its
/// case `{case_namespace}/u{i:06}/slot{j}`, keyed without the arm so paired
/// arms share worlds and streams.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftQRunOptionsV1 {
    pub arm: SoftQArmV1,
    pub case_namespace: String,
    pub prior_deck_indices: Vec<usize>,
    pub teacher_workers: usize,
    /// Engineering check only: the teacher runs, no target is applied.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub engineering_zero_treatment: bool,
}

impl SoftQRunOptionsV1 {
    fn teacher_options_v1(&self, index: usize) -> SoftQTeacherOptionsV1 {
        SoftQTeacherOptionsV1 {
            arm: self.arm,
            case_prefix: format!("{}/u{index:06}", self.case_namespace),
            prior_deck_indices: self.prior_deck_indices.clone(),
            teacher_workers: self.teacher_workers,
            engineering_zero_treatment: self.engineering_zero_treatment,
        }
    }
}

/// TP1 Stage B searched-game targets (collab LANES/teacher-price-20261019,
/// STAGE-B.md). Before update `index`, the run waits for the lane
/// orchestrator's packet `{packets_directory}/u{update_offset + index:06}.json`
/// (built from this update's start checkpoint), pins it and trains on it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchTargetsRunOptionsV1 {
    pub packets_directory: PathBuf,
    pub bank_spec: PinnedFileV1,
    pub opponents: Vec<SearchTargetsOpponentV1>,
    pub coefficient: f64,
    pub denominator: usize,
    pub run: String,
    pub arm: String,
    pub seed: u64,
    pub update_offset: usize,
    pub wait_seconds: u64,
    /// The search allocation every searched game must record (E4).
    pub allocation: Value,
    /// STAGE-B2.md: the target family and the per-update cap (defaults:
    /// Stage B's kept set, no cap).
    #[serde(default)]
    pub target: crate::expanded_deck_training_v1::search_targets_v1::SearchTargetKindV1,
    #[serde(default)]
    pub cap: crate::expanded_deck_training_v1::search_targets_v1::SearchTargetCapV1,
}

impl SearchTargetsRunOptionsV1 {
    fn packet_path_v1(&self, index: usize) -> PathBuf {
        self.packets_directory
            .join(format!("u{:06}.json", self.update_offset + index))
    }

    fn options_v1(&self, index: usize, packet: PinnedFileV1) -> SearchTargetsOptionsV1 {
        SearchTargetsOptionsV1 {
            packet,
            bank_spec: self.bank_spec.clone(),
            opponents: self.opponents.clone(),
            coefficient: self.coefficient,
            denominator: self.denominator,
            allocation: self.allocation.clone(),
            expected: json!({"run":self.run,"arm":self.arm,"seed":self.seed,
                "update":self.update_offset + index,"allocation":self.allocation}),
            target: self.target,
            cap: self.cap,
        }
    }

    /// Waits for the packet (one-second polls, `STOP` in the packets
    /// directory or the timeout ends the wait with an error) and pins it.
    fn wait_for_packet_v1(&self, index: usize) -> Result<PinnedFileV1, String> {
        let path = self.packet_path_v1(index);
        let started = Instant::now();
        loop {
            check(
                !self.packets_directory.join("STOP").exists(),
                "search-target packet wait stopped",
            )?;
            if path.exists() {
                return pin(&path);
            }
            check(
                started.elapsed().as_secs() < self.wait_seconds,
                "search-target packet did not arrive in time",
            )?;
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExpandedTrainingRunV1 {
    pub schema: String,
    pub initial_source: ExpandedModelSourceV1,
    pub opponents: Vec<NamedExpandedOpponentV1>,
    pub iterations: Vec<ExpandedRunIterationV1>,
    pub learning_rate: f32,
    pub value_coefficient: f32,
    #[serde(default, skip_serializing_if = "ExpandedUpdateBackendV1::is_cpu")]
    pub update_backend: ExpandedUpdateBackendV1,
    /// Config-driven; default `TerminalReinforceValueV3` keeps every
    /// existing config, its wire bytes, and its run identity byte-identical.
    /// See `ExpandedLossSelectionV1`.
    #[serde(
        default,
        skip_serializing_if = "ExpandedLossSelectionV1::is_terminal_reinforce_value_v3"
    )]
    pub loss_selection: ExpandedLossSelectionV1,
    /// Config-driven; default `Sequential` keeps every existing config and
    /// the V3 lineage byte-identical. See `UpdateBackwardExecutionV1`.
    #[serde(
        default,
        skip_serializing_if = "UpdateBackwardExecutionV1::is_sequential"
    )]
    pub update_backward_execution: UpdateBackwardExecutionV1,
    #[serde(
        default = "default_collection_workers",
        skip_serializing_if = "is_serial_collection"
    )]
    pub collection_workers: usize,
    #[serde(
        default = "default_collection_workers",
        skip_serializing_if = "is_serial_collection"
    )]
    pub preparation_workers: usize,
    /// Config-driven; default `0.0` (fatal on the first non-Natural
    /// terminal) keeps every existing config byte-identical on the wire and
    /// its collection behavior unchanged. See
    /// `expanded_deck_training_v1::collect_episode_tolerant_v1`.
    #[serde(default, skip_serializing_if = "is_zero_non_natural_fraction")]
    pub max_non_natural_episode_fraction: f32,
    /// Config-driven bound (MiB) on the decoded tensor payload of one
    /// prepared update; default 256 keeps every existing config byte-identical
    /// on the wire and every prepared update's bound unchanged. Raising it
    /// admits updates of more than about fifty games. Recorded in run.json
    /// when non-default and re-verified against each update's preparation
    /// telemetry. See `ordered_update_preparation`.
    #[serde(
        default = "default_max_prepared_tensor_mebibytes",
        skip_serializing_if = "is_default_max_prepared_tensor_mebibytes"
    )]
    pub max_prepared_tensor_mebibytes: usize,
    /// Learner collection sampler (g115 line (b)); the legacy default is
    /// omitted, so every existing config and run identity is unchanged.
    #[serde(default, skip_serializing_if = "CollectionSamplerV1::is_legacy")]
    pub collection_sampler: CollectionSamplerV1,
    /// The math of every forward, loss and gradient of the run
    /// (`training_math_v1`): the platform's libm by default, omitted so
    /// every existing config and run identity is unchanged, or
    /// `portable-math-v1`, which trains the same bits on every x86-64 system.
    #[serde(default, skip_serializing_if = "TrainingMathV1::is_platform_libm")]
    pub training_math: TrainingMathV1,
    /// G115 line (b) update options (head-only optimizer mask); omitted
    /// when absent, so every existing config and run identity is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_b: Option<LineBUpdateOptionsV1>,
    /// E supported-choice policy supervision; omitted when absent, so every
    /// existing config and run identity is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e_supported: Option<ESupportedRunOptionsV1>,
    /// SQ1 soft rollout-Q teaching; omitted when absent, so every existing
    /// config and run identity is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_q: Option<SoftQRunOptionsV1>,
    /// TP1 Stage B searched-game targets; omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_targets: Option<SearchTargetsRunOptionsV1>,
    /// TP1 Stage B's CPU ledger: each iteration receipt records this driver
    /// process's CPU seconds at the iteration boundary. Off by default, so
    /// every existing config and receipt is unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_process_cpu: bool,
    pub output_directory: PathBuf,
}

fn is_zero_non_natural_fraction(value: &f32) -> bool {
    *value == 0.0
}

fn default_max_prepared_tensor_mebibytes() -> usize {
    DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES
}

fn is_default_max_prepared_tensor_mebibytes(value: &usize) -> bool {
    *value == DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES
}

fn check(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
fn value(value: &impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(err)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identity(value: &impl Serialize) -> Result<String, String> {
    Ok(digest(&serde_json::to_vec(value).map_err(err)?))
}

impl NativeExpandedTrainingRunV1 {
    /// Read-only strict decoding for explicit schedule preparation. This does
    /// not load any model, open a device or create a run directory.
    pub fn from_json_v1(input: &str) -> Result<Self, String> {
        check(input.len() as u64 <= SMALL_CAP, "run config exceeds 16 MiB")?;
        let value = crate::rl::parse_strict_json_value(input).map_err(err)?;
        serde_json::from_value(value).map_err(err)
    }

    pub fn validate_v1(&self) -> Result<(), String> {
        check(self.schema == SCHEMA, "unknown successor run schema")?;
        // Line (b) teacher seeds are explicit per update and come from the
        // launcher's manifest (CODEX #572); this driver invents no schedule.
        check(
            self.line_b
                .as_ref()
                .is_none_or(|options| options.teacher.is_none()),
            "the training-run driver takes no line (b) teacher; the launcher passes per-update seeds to Update",
        )?;
        self.update_backend.validate_v1()?;
        self.loss_selection.validate_v1()?;
        if !self.training_math.is_platform_libm() {
            check(
                self.update_backend.is_cpu()
                    && self.line_b.is_none()
                    && self.e_supported.is_none()
                    && self.soft_q.is_none()
                    && self.search_targets.is_none()
                    && !matches!(
                        self.loss_selection,
                        ExpandedLossSelectionV1::GaeUniformKlRecoveryV1 { .. }
                    ),
                "portable training math requires the CPU update without the recovery loss, line (b) or E",
            )?;
        }
        crate::expanded_deck_training_v1::validate_collection_workers_v1(self.collection_workers)?;
        crate::expanded_deck_training_v1::validate_preparation_workers_v1(
            self.preparation_workers,
        )?;
        crate::expanded_deck_training_v1::validate_max_non_natural_episode_fraction_v1(
            self.max_non_natural_episode_fraction,
        )?;
        crate::expanded_deck_training_v1::validate_max_prepared_tensor_mebibytes_v1(
            self.max_prepared_tensor_mebibytes,
        )?;
        check(
            self.preparation_workers > 1
                || self.max_prepared_tensor_mebibytes == DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            "max_prepared_tensor_mebibytes applies to prepared updates only (preparation_workers > 1)",
        )?;
        check(
            self.output_directory.is_absolute(),
            "absolute run directory required",
        )?;
        check(
            !self.iterations.is_empty() && self.iterations.len() <= 4096,
            "run requires 1..4096 iterations",
        )?;
        check(
            self.learning_rate.is_finite()
                && self.learning_rate > 0.0
                && self.value_coefficient.is_finite()
                && self.value_coefficient > 0.0,
            "invalid optimizer configuration",
        )?;
        check(
            self.opponents.len() <= 128,
            "opponent roster exceeds 128 entries",
        )?;
        validate_source_paths(&self.initial_source)?;
        if let Some(options) = &self.e_supported {
            // The E command is the sequential CPU GAE update with full Adam and
            // the unchanged deployment sampler; nothing else may be combined.
            check(
                self.line_b.is_none()
                    && self.update_backend.is_cpu()
                    && self.update_backward_execution.is_sequential()
                    && self.preparation_workers == 1
                    && self.collection_sampler.is_legacy()
                    && matches!(
                        self.loss_selection,
                        ExpandedLossSelectionV1::GaeAdvantageValueV1 { .. }
                    ),
                "E requires ordinary sequential CPU GAE, serial preparation and the legacy sampler",
            )?;
            validate_source_paths(&options.teacher)?;
            check(
                options.slots.len() == self.iterations.len(),
                "E schedule needs one slot list per iteration",
            )?;
            let mut cases = BTreeSet::new();
            for (slots, iteration) in options.slots.iter().zip(&self.iterations) {
                for slot in slots {
                    check(
                        slot.trajectory_index < iteration.episodes.len()
                            && !slot.case_id.is_empty()
                            && slot.case_id.len() <= 256
                            && cases.insert(slot.case_id.as_str()),
                        "E slot outside its batch, or empty or repeated case id",
                    )?;
                }
            }
        }
        if let Some(options) = &self.search_targets {
            check(
                self.line_b.is_none()
                    && self.e_supported.is_none()
                    && self.soft_q.is_none()
                    && self.update_backend.is_cpu()
                    && self.update_backward_execution.is_sequential()
                    && self.preparation_workers == 1
                    && self.collection_sampler.is_legacy()
                    && matches!(
                        self.loss_selection,
                        ExpandedLossSelectionV1::GaeAdvantageValueV1 { .. }
                    ),
                "search targets require ordinary sequential CPU GAE, serial preparation and the legacy sampler",
            )?;
            check(
                (1..=172_800).contains(&options.wait_seconds)
                    && !options.run.is_empty()
                    && !options.arm.is_empty(),
                "search-target wait, run or arm invalid",
            )?;
            options
                .options_v1(0, options.bank_spec.clone())
                .validate_v1()?;
        }
        if let Some(options) = &self.soft_q {
            // The SQ1 command is the sequential CPU GAE update with full Adam,
            // serial preparation and the unchanged deployment sampler.
            check(
                self.line_b.is_none()
                    && self.e_supported.is_none()
                    && self.update_backend.is_cpu()
                    && self.update_backward_execution.is_sequential()
                    && self.preparation_workers == 1
                    && self.collection_sampler.is_legacy()
                    && matches!(
                        self.loss_selection,
                        ExpandedLossSelectionV1::GaeAdvantageValueV1 { .. }
                    ),
                "SQ1 requires ordinary sequential CPU GAE, serial preparation and the legacy sampler",
            )?;
            check(
                !options.case_namespace.is_empty() && options.case_namespace.len() <= 128,
                "SQ1 case namespace must be 1 to 128 bytes",
            )?;
            options.teacher_options_v1(0).validate_v1()?;
        }
        let mut roster = BTreeSet::new();
        for opponent in &self.opponents {
            check(
                !opponent.id.is_empty()
                    && opponent.id.len() <= 128
                    && roster.insert(opponent.id.as_str()),
                "empty or duplicate opponent id",
            )?;
            validate_source_paths(&opponent.source)?;
        }
        let mut episode_ids = BTreeSet::new();
        for (index, iteration) in self.iterations.iter().enumerate() {
            check(
                !iteration.episodes.is_empty() && iteration.episodes.len() <= 1024,
                "iteration requires 1..1024 episodes",
            )?;
            for scheduled in &iteration.episodes {
                let episode = &scheduled.episode;
                check(
                    episode.opponent.is_none(),
                    "schedule owns opponent source; embedded opponent is not allowed",
                )?;
                check(
                    !episode.id.is_empty()
                        && episode.id.len() <= 128
                        && episode_ids.insert(episode.id.as_str()),
                    "empty or repeated run episode id",
                )?;
                check(
                    episode.learner_seat < 2
                        && episode.starting_player < 2
                        && (1..=100_000).contains(&episode.max_physical_decisions)
                        && (1..=1_000_000).contains(&episode.max_policy_steps),
                    "invalid episode bounds or seat",
                )?;
                for seat in 0..2 {
                    let list = &episode.registered[seat];
                    let registered = RegisteredDeckV1::new_executable_v1(
                        &list.label,
                        list.mainboard.clone(),
                        list.sideboard.clone(),
                    )
                    .map_err(err)?;
                    let list = &episode.selected[seat];
                    let selected = RegisteredDeckV1::new_executable_v1(
                        &list.label,
                        list.mainboard.clone(),
                        list.sideboard.clone(),
                    )
                    .map_err(err)?;
                    check(
                        registered
                            .registered_configuration()
                            .combined_card_counts_v1()
                            == selected
                                .registered_configuration()
                                .combined_card_counts_v1(),
                        "selected configuration changes registered 75",
                    )?;
                    check(
                        episode.postboard
                            || registered.registered_configuration()
                                == selected.registered_configuration(),
                        "game-one selection differs from registration",
                    )?;
                }
                match &scheduled.opponent {
                    ExpandedOpponentAssignmentV1::Fixed { id } => check(
                        roster.contains(id.as_str()),
                        "scheduled opponent absent from roster",
                    )?,
                    ExpandedOpponentAssignmentV1::CompletedIteration { index: previous } => check(
                        *previous < index,
                        "opponent references an unfinished or future iteration",
                    )?,
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

fn validate_source_paths(source: &ExpandedModelSourceV1) -> Result<(), String> {
    for pin in std::iter::once(&source.play_import).chain(source.checkpoint.iter()) {
        check(
            pin.path.is_absolute()
                && pin.sha256.len() == 64
                && pin.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "model source requires absolute SHA-pinned inputs",
        )?;
    }
    Ok(())
}

fn read_json(path: &Path, cap: u64) -> Result<Value, String> {
    let meta = fs::metadata(path).map_err(err)?;
    check(
        meta.is_file() && meta.len() <= cap,
        "artifact is not a bounded regular file",
    )?;
    let bytes = fs::read(path).map_err(err)?;
    check(bytes.len() as u64 <= cap, "artifact grew beyond bound")?;
    serde_json::from_slice(&bytes).map_err(err)
}
fn verify_pin(pin: &PinnedFileV1) -> Result<(), String> {
    let mut file = File::open(&pin.path).map_err(err)?;
    check(
        file.metadata().map_err(err)?.is_file(),
        "pinned artifact is not a file",
    )?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let count = file.read(&mut buffer).map_err(err)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        check(size <= ARTIFACT_CAP, "pinned artifact exceeds 512 MiB")?;
        hasher.update(&buffer[..count]);
    }
    check(
        format!("{:x}", hasher.finalize()) == pin.sha256,
        "pinned artifact hash differs",
    )
}
fn pin(path: &Path) -> Result<PinnedFileV1, String> {
    check(
        fs::metadata(path).map_err(err)?.len() <= SMALL_CAP,
        "receipt exceeds 16 MiB",
    )?;
    Ok(PinnedFileV1 {
        path: path.to_path_buf(),
        sha256: digest(&fs::read(path).map_err(err)?),
    })
}
fn read_pin(pin: &PinnedFileV1) -> Result<Value, String> {
    read_pin_bounded(pin, SMALL_CAP)
}
fn read_pin_bounded(pin: &PinnedFileV1, cap: u64) -> Result<Value, String> {
    let bytes = read_pin_bounded_bytes(pin, cap)?;
    serde_json::from_slice(&bytes).map_err(err)
}
/// `read_pin_bounded`'s checks, returning the verified bytes.
fn read_pin_bounded_bytes(pin: &PinnedFileV1, cap: u64) -> Result<Vec<u8>, String> {
    let file = File::open(&pin.path).map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    check(
        meta.is_file() && meta.len() <= cap,
        "artifact is not a bounded regular file",
    )?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(cap + 1).read_to_end(&mut bytes).map_err(err)?;
    check(bytes.len() as u64 <= cap, "artifact grew beyond bound")?;
    check(digest(&bytes) == pin.sha256, "pinned artifact hash differs")?;
    Ok(bytes)
}
fn publish(
    directory: &Path,
    name: &str,
    document: &impl Serialize,
) -> Result<PinnedFileV1, String> {
    let bytes = serde_json::to_vec(document).map_err(err)?;
    check(
        bytes.len() as u64 <= SMALL_CAP,
        "run receipt exceeds 16 MiB",
    )?;
    let parent = capture_existing_publication_parent_v1(directory).map_err(err)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_nanos();
    // A crashed writer may leave any previous stage. A new stage preserves
    // those bytes and allows the absent final receipt to publish on resume.
    let receipt = publish_new_bytes_v1(
        &parent,
        format!(".{name}.stage-{}-{nonce}", std::process::id()),
        name,
        &bytes,
    )
    .map_err(err)?;
    Ok(PinnedFileV1 {
        path: directory.join(name),
        sha256: receipt
            .sha256()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    })
}
fn parse_pin(document: &Value, field: &str) -> Result<PinnedFileV1, String> {
    serde_json::from_value(
        document
            .get(field)
            .ok_or_else(|| format!("missing {field}"))?
            .clone(),
    )
    .map_err(err)
}

fn resolve_episodes(
    config: &NativeExpandedTrainingRunV1,
    index: usize,
    current: &ExpandedModelSourceV1,
    history: &[ExpandedModelSourceV1],
) -> Result<Vec<ExpandedEpisodeV1>, String> {
    config.iterations[index]
        .episodes
        .iter()
        .map(|scheduled| {
            let source =
                match &scheduled.opponent {
                    ExpandedOpponentAssignmentV1::Current => current,
                    ExpandedOpponentAssignmentV1::Initial => &config.initial_source,
                    ExpandedOpponentAssignmentV1::Fixed { id } => {
                        &config
                            .opponents
                            .iter()
                            .find(|entry| entry.id == *id)
                            .ok_or("unknown opponent")?
                            .source
                    }
                    ExpandedOpponentAssignmentV1::CompletedIteration { index } => history
                        .get(*index)
                        .ok_or("opponent checkpoint has not completed")?,
                };
            let mut episode = scheduled.episode.clone();
            episode.opponent = Some(source.clone());
            Ok(episode)
        })
        .collect()
}

/// Auditability for the config-driven tolerant-collection ledger (see
/// `expanded_deck_training_v1::collect_episode_tolerant_v1`): `collect`/
/// `collect_parallel` stamp `non_natural_ledger` on their own
/// `collection.json` receipt only when `max_non_natural_episode_fraction >
/// 0.0`, so an iteration's ledger (if any) is surfaced here, from the same
/// pinned collection receipt `validate_collection` already re-verified, for
/// the caller to record directly on the iteration's own `complete.json`
/// receipt rather than leaving it reachable only by chasing the collection
/// pin by hand.
fn collection_non_natural_ledger_pin(
    collection: &PinnedFileV1,
) -> Result<Option<PinnedFileV1>, String> {
    let document = read_pin(collection)?;
    match document.get("non_natural_ledger") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(serde_json::from_value(value.clone()).map_err(err)?)),
    }
}

/// Reuse semantic checks only while all their pinned bytes still match.
/// This is invocation-local evidence, never a persisted or metadata cache.
fn reverify_validated_collection(
    collection: &PinnedFileV1,
    trajectories: &[PinnedFileV1],
) -> Result<(), String> {
    if let Some(ledger) = collection_non_natural_ledger_pin(collection)? {
        verify_pin(&ledger)?;
    }
    let checked: Vec<Result<(), String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = trajectories
            .iter()
            .map(|trajectory| scope.spawn(move || verify_pin(trajectory)))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("collection verification worker panicked".to_string()))
            })
            .collect()
    });
    for result in checked {
        result?;
    }
    Ok(())
}

/// `input_contract` is the run's no-state-digest contract member
/// (`no_state_digest_v1`), None for an unconverted run; the receipt and
/// every trajectory must carry exactly it.
fn validate_collection(
    collection: &PinnedFileV1,
    source: &ExpandedModelSourceV1,
    episodes: &[ExpandedEpisodeV1],
    before: &ExpandedInferenceIdentityV1,
    collection_sampler: CollectionSamplerV1,
    training_math: TrainingMathV1,
    input_contract: Option<&Value>,
) -> Result<Vec<PinnedFileV1>, String> {
    let document = read_pin(collection)?;
    check(
        document["complete"] == true
            && document["source"] == value(source)?
            && document["behavior_state_sha256"] == before.state_sha256,
        "collection source/state does not match iteration",
    )?;
    check(
        document.get("collection_sampler").and_then(Value::as_str)
            == collection_sampler.identity_v1(),
        "collection sampler differs from the run config",
    )?;
    check(
        document.get("training_math").and_then(Value::as_str) == training_math.identity_v1(),
        "collection training math differs from the run config",
    )?;
    check(
        document.get("input_contract") == input_contract,
        "collection no-state-digest contract differs from the run's",
    )?;
    validate_collection_episodes_with_sampler_v1(
        &document,
        episodes,
        collection_sampler.identity_v1(),
        training_math,
        input_contract,
    )
}

/// The schedule half of `validate_collection`: every published trajectory
/// must carry exactly its scheduled episode. Under tolerant collection
/// (`max_non_natural_episode_fraction > 0.0`) a slot whose ledgered attempts
/// all failed legitimately published its trajectory from the derived retry
/// seed, so for that slot alone the scheduled seed is replaced by the seed
/// the ledger re-derives (`ledgered_retry_seed_overrides_v1`, which first
/// re-verifies the whole ledger against the schedule); every other episode
/// field, and every unledgered slot, must still match the schedule exactly.
#[cfg(test)]
fn validate_collection_episodes(
    document: &Value,
    episodes: &[ExpandedEpisodeV1],
) -> Result<Vec<PinnedFileV1>, String> {
    validate_collection_episodes_with_sampler_v1(
        document,
        episodes,
        None,
        TrainingMathV1::PlatformLibm,
        None,
    )
}

/// Also binds every trajectory's `learner_sampler` to the configured
/// collection sampler (absent for the legacy samplers), its
/// `training_math` to the run's, and its `input_contract` to the run's.
fn validate_collection_episodes_with_sampler_v1(
    document: &Value,
    episodes: &[ExpandedEpisodeV1],
    learner_sampler: Option<&str>,
    training_math: TrainingMathV1,
    input_contract: Option<&Value>,
) -> Result<Vec<PinnedFileV1>, String> {
    let trajectories: Vec<PinnedFileV1> =
        serde_json::from_value(document["trajectories"].clone()).map_err(err)?;
    check(
        trajectories.len() == episodes.len(),
        "collection episode count differs",
    )?;
    let seed_overrides = match document.get("non_natural_ledger") {
        None | Some(Value::Null) => vec![None; episodes.len()],
        Some(pin) => {
            let pin: PinnedFileV1 = serde_json::from_value(pin.clone()).map_err(err)?;
            crate::expanded_deck_training_v1::ledgered_retry_seed_overrides_v1(&pin, episodes)?
        }
    };
    // Hash and parse the trajectory files concurrently (one short-lived thread
    // per file, ten to sixty per update); the checks below still run in order
    // and report the first failing trajectory exactly as the serial loop did.
    // A file parsed ahead by an idle collection worker yields that parse of
    // exactly these bytes instead of a second one.
    let parsed_ahead = std::sync::atomic::AtomicUsize::new(0);
    let saved: Vec<Result<CollectionTrajectoryFieldsV1, String>> = std::thread::scope(|scope| {
        let parsed_ahead = &parsed_ahead;
        let handles: Vec<_> = trajectories
            .iter()
            .map(|trajectory| {
                scope.spawn(move || -> Result<CollectionTrajectoryFieldsV1, String> {
                    let bytes = read_pin_bounded_bytes(trajectory, ARTIFACT_CAP)?;
                    match take_collection_fields_v1(&trajectory.sha256, &bytes) {
                        Some(fields) => {
                            parsed_ahead.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            Ok(fields)
                        }
                        None => {
                            let document: Value = serde_json::from_slice(&bytes).map_err(err)?;
                            Ok(CollectionTrajectoryFieldsV1::of_document_v1(&document))
                        }
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("collection validation worker panicked".to_string()))
            })
            .collect()
    });
    for ((episode, seed_override), saved) in episodes.iter().zip(&seed_overrides).zip(saved) {
        let saved = saved?;
        let mut expected = episode.clone();
        if let Some(seed) = seed_override {
            expected.seed = *seed;
        }
        check(
            saved.episode == value(&expected)?,
            "collection episode assignment differs",
        )?;
        check(
            saved.learner_sampler.as_deref() == learner_sampler,
            "collection learner sampler differs from the run config",
        )?;
        check(
            saved.training_math.as_deref() == training_math.identity_v1(),
            "collection trajectory training math differs from the run config",
        )?;
        check(
            saved.input_contract.as_ref() == input_contract,
            "collection trajectory no-state-digest contract differs from the run's",
        )?;
    }
    eprintln!(
        "collection validation: {} of {} trajectories parsed ahead",
        parsed_ahead.into_inner(),
        trajectories.len()
    );
    Ok(trajectories)
}

fn validate_preparation_execution(
    document: &Value,
    workers: usize,
    max_prepared_tensor_mebibytes: usize,
) -> Result<(), String> {
    let telemetry = document.get("update_preparation");
    if workers == 1 {
        return check(
            telemetry.is_none(),
            "serial run recovered a prepared update",
        );
    }
    let telemetry =
        telemetry.ok_or("prepared run recovered an update without preparation telemetry")?;
    let jobs = telemetry["physical_group_jobs"]
        .as_u64()
        .ok_or("missing preparation job count")?;
    check(
        telemetry["schema"] == "mtg-kernel-ordered-update-preparation/v1"
            && telemetry["requested_workers"] == workers
            && (1..=65_536).contains(&jobs)
            && telemetry["started_workers"] == (workers as u64).min(jobs),
        "recovered update preparation differs from run configuration",
    )?;
    // Receipts written before the bound became config-driven carry no
    // `max_decoded_tensor_bytes`; they are valid only at the historical
    // default. A receipt that records the bound must record this run's.
    let expected_bytes = (max_prepared_tensor_mebibytes as u64) * 1024 * 1024;
    check(
        match telemetry.get("max_decoded_tensor_bytes") {
            None | Some(Value::Null) => {
                max_prepared_tensor_mebibytes == DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES
            }
            Some(recorded) => recorded == &json!(expected_bytes),
        },
        "recovered update preparation bound differs from run configuration",
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_update(
    update: &PinnedFileV1,
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    before: &ExpandedInferenceIdentityV1,
    learning_rate: f32,
    value_coefficient: f32,
    update_backend: ExpandedUpdateBackendV1,
    preparation_workers: usize,
    max_prepared_tensor_mebibytes: usize,
    line_b_frozen_sha256: Option<&str>,
    e_supported: Option<&ESupportedTeacherOptionsV1>,
    soft_q: Option<&SoftQTeacherOptionsV1>,
    search_targets: Option<(&SearchTargetsRunOptionsV1, usize)>,
    training_math: TrainingMathV1,
    input_contract: Option<&Value>,
) -> Result<(ExpandedModelSourceV1, ExpandedInferenceIdentityV1), String> {
    let document = read_pin(update)?;
    check(
        document.get("input_contract") == input_contract,
        "update no-state-digest contract differs from the run's",
    )?;
    validate_e_supported_update(&document, e_supported)?;
    validate_soft_q_update(&document, soft_q)?;
    validate_search_targets_update(&document, search_targets)?;
    check(
        document.get("training_math").and_then(Value::as_str) == training_math.identity_v1(),
        "update training math differs from the run config",
    )?;
    // A masked run's every update repeats the initial source's frozen digest.
    check(
        document
            .get("line_b")
            .and_then(|line_b| line_b.get("frozen_tensor_sha256"))
            .and_then(Value::as_str)
            == line_b_frozen_sha256,
        "line (b) frozen tensors differ from the run's initial source",
    )?;
    update_backend.validate_update_execution_v1(&document)?;
    validate_preparation_execution(
        &document,
        preparation_workers,
        max_prepared_tensor_mebibytes,
    )?;
    check(
        document["complete"] == true
            && document["source"] == value(source)?
            && document["trajectories"] == value(&trajectories)?
            && document["before_state_sha256"] == before.state_sha256,
        "completed update does not belong to this iteration",
    )?;
    let checkpoint = parse_pin(&document, "checkpoint")?;
    let next = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint.clone()),
        ..source.clone()
    };
    let after =
        load_expanded_update_inference_v1(&next, trajectories, learning_rate, value_coefficient)?;
    check(
        after.adam_step
            == before
                .adam_step
                .checked_add(1)
                .ok_or("Adam step overflow")?
            && document["adam_step"] == after.adam_step
            && document["after_state_sha256"] == after.state_sha256,
        "update does not advance exactly one optimizer step",
    )?;
    Ok((next, after))
}

/// An E run's update must carry its packet, bound into the new checkpoint with
/// this run's exact continuation identity; an ordinary run's must carry neither.
fn validate_e_supported_update(
    document: &Value,
    e_supported: Option<&ESupportedTeacherOptionsV1>,
) -> Result<(), String> {
    let Some(options) = e_supported else {
        return check(
            document.get("e_supported").is_none(),
            "ordinary run recovered an E update",
        );
    };
    let checkpoint = read_pin_bounded(&parse_pin(document, "checkpoint")?, ARTIFACT_CAP)?;
    let receipt = document
        .get("e_supported")
        .ok_or("E run update lacks its teacher receipt")?;
    let packet_pin = parse_pin(receipt, "packet")?;
    let packet = read_pin_bounded(&packet_pin, ARTIFACT_CAP)?;
    check(
        checkpoint["e_supported"]["continuation"] == e_supported_continuation_identity_v1(options)
            && checkpoint["e_supported"]["packet"] == value(&packet_pin)?
            && packet["options"] == value(options)?
            && packet["scheduled"] == options.slots.len()
            && receipt["scheduled"] == options.slots.len(),
        "E update does not bind this run's teacher, schedule and packet",
    )
}

/// An SQ1 run's update must carry its packet, bound into the new checkpoint
/// with this run's exact continuation identity; any other run's must not.
fn validate_soft_q_update(
    document: &Value,
    soft_q: Option<&SoftQTeacherOptionsV1>,
) -> Result<(), String> {
    let Some(options) = soft_q else {
        return check(
            document.get("soft_q").is_none(),
            "a run without SQ1 recovered an SQ1 update",
        );
    };
    let checkpoint = read_pin_bounded(&parse_pin(document, "checkpoint")?, ARTIFACT_CAP)?;
    let receipt = document
        .get("soft_q")
        .ok_or("SQ1 run update lacks its teacher receipt")?;
    let packet_pin = parse_pin(receipt, "packet")?;
    let packet = read_pin_bounded(&packet_pin, ARTIFACT_CAP)?;
    check(
        checkpoint["soft_q"]["continuation"] == soft_q_continuation_identity_v1(options)
            && checkpoint["soft_q"]["packet"] == value(&packet_pin)?
            && packet["options"] == value(options)?
            && receipt["scheduled"] == packet["scheduled"],
        "SQ1 update does not bind this run's teacher options and packet",
    )
}

/// A search-target run's update must carry the packet at this update's
/// expected path, unchanged since, bound into the checkpoint with this run's
/// continuation identity; any other run's update must not.
fn validate_search_targets_update(
    document: &Value,
    search_targets: Option<(&SearchTargetsRunOptionsV1, usize)>,
) -> Result<(), String> {
    let Some((options, index)) = search_targets else {
        return check(
            document.get("search_targets").is_none(),
            "a run without search targets recovered a search-target update",
        );
    };
    let receipt = document
        .get("search_targets")
        .ok_or("search-target run update lacks its receipt")?;
    let packet_pin = parse_pin(receipt, "packet")?;
    check(
        Path::new(&packet_pin.path) == options.packet_path_v1(index),
        "search-target update read another packet path",
    )?;
    read_pin_bounded(&packet_pin, ARTIFACT_CAP)?;
    crate::expanded_deck_training_v1::search_targets_v1::revalidate_packet_v1(&packet_pin)?;
    let checkpoint = read_pin_bounded(&parse_pin(document, "checkpoint")?, ARTIFACT_CAP)?;
    let expected = options.options_v1(index, packet_pin.clone());
    check(
        checkpoint["search_targets"]["continuation"]
            == search_targets_continuation_identity_v1(&expected)
            && checkpoint["search_targets"]["packet"] == value(&packet_pin)?
            && receipt["packet"] == value(&packet_pin)?,
        "search-target update does not bind this run's packet and options",
    )
}

fn collection_command(
    config: &NativeExpandedTrainingRunV1,
    source: &ExpandedModelSourceV1,
    episodes: &[ExpandedEpisodeV1],
    output_directory: PathBuf,
) -> ExpandedTrainingCommandV1 {
    if config.collection_workers == 1 {
        ExpandedTrainingCommandV1::Collect {
            source: source.clone(),
            episodes: episodes.to_vec(),
            max_non_natural_episode_fraction: config.max_non_natural_episode_fraction,
            collection_sampler: config.collection_sampler,
            training_math: config.training_math,
            output_directory,
        }
    } else {
        ExpandedTrainingCommandV1::CollectParallel {
            source: source.clone(),
            episodes: episodes.to_vec(),
            workers: config.collection_workers,
            max_non_natural_episode_fraction: config.max_non_natural_episode_fraction,
            collection_sampler: config.collection_sampler,
            training_math: config.training_math,
            output_directory,
        }
    }
}

/// Writes `loss_selection` into `document` next to `update_backend`,
/// exactly when it is not the default v3 identity, so a manifest or result
/// for a legacy config gains no new key, and a gae_advantage_value/v1 run's
/// receipt proves which loss ran without needing to parse the nested
/// `config` object.
fn record_loss_selection_execution(config: &NativeExpandedTrainingRunV1, document: &mut Value) {
    if !config.loss_selection.is_terminal_reinforce_value_v3() {
        document["loss_selection"] = json!(config.loss_selection);
    }
}

fn record_collection_execution(config: &NativeExpandedTrainingRunV1, document: &mut Value) {
    config.collection_sampler.record_v1(document);
    config.training_math.record_v1(document);
    if let Some(line_b) = &config.line_b {
        document["line_b"] = json!(line_b);
    }
    if config.collection_workers > 1 {
        document["collection_backend"] = json!("native-cpu-parallel-episodes-v1");
        document["collection_workers_requested"] = json!(config.collection_workers);
    }
    if config.preparation_workers > 1 {
        document["preparation_backend"] = json!("native-cpu-ordered-physical-groups-v1");
        document["preparation_workers_requested"] = json!(config.preparation_workers);
    }
    if config.max_prepared_tensor_mebibytes != DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES {
        document["max_prepared_tensor_mebibytes"] = json!(config.max_prepared_tensor_mebibytes);
    }
}

fn update_command(
    config: &NativeExpandedTrainingRunV1,
    index: usize,
    source: &ExpandedModelSourceV1,
    trajectories: Vec<PinnedFileV1>,
    output_directory: PathBuf,
) -> ExpandedTrainingCommandV1 {
    if let Some(options) = &config.soft_q {
        ExpandedTrainingCommandV1::SoftQUpdate {
            source: source.clone(),
            trajectories,
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            loss_selection: config.loss_selection,
            teacher: options.teacher_options_v1(index),
            output_directory,
        }
    } else if let Some(options) = &config.e_supported {
        ExpandedTrainingCommandV1::ESupportedUpdate {
            source: source.clone(),
            trajectories,
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            loss_selection: config.loss_selection,
            teacher: options.teacher_options_v1(index),
            output_directory,
        }
    } else if config.preparation_workers == 1 {
        ExpandedTrainingCommandV1::Update {
            line_b: config.line_b.clone(),
            source: source.clone(),
            trajectories,
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            update_backend: config.update_backend,
            update_backward_execution: config.update_backward_execution,
            loss_selection: config.loss_selection,
            training_math: config.training_math,
            output_directory,
        }
    } else {
        ExpandedTrainingCommandV1::UpdatePrepared {
            line_b: config.line_b.clone(),
            source: source.clone(),
            trajectories,
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            update_backend: config.update_backend,
            update_backward_execution: config.update_backward_execution,
            loss_selection: config.loss_selection,
            preparation_workers: config.preparation_workers,
            max_prepared_tensor_mebibytes: config.max_prepared_tensor_mebibytes,
            training_math: config.training_math,
            output_directory,
        }
    }
}

/// A converted (no-state-digest) run's initial source, validated: its
/// inference identity, its `input_contract` member and its converted digest.
struct ConvertedInitialV1 {
    identity: ExpandedInferenceIdentityV1,
    record: Value,
    contract_digest: String,
}

/// Never inlined: the loaded policy stays out of the driver's frame.
#[inline(never)]
fn load_converted_initial_v1(
    config: &NativeExpandedTrainingRunV1,
) -> Result<ConvertedInitialV1, String> {
    let (policy, identity) =
        load_expanded_inference_bound_v1(&config.initial_source, config.training_math)?;
    let record = policy_contract_record_v1(&policy)?
        .ok_or("a converted initial source loaded without its contract")?;
    Ok(ConvertedInitialV1 {
        identity,
        record: value(&record)?,
        contract_digest: policy
            .input_contract_v1()
            .ok_or("a converted initial source loaded without its contract")?
            .to_owned(),
    })
}

/// A fixed opponent's full load and its converted digest, if any. Never
/// inlined: the loaded policy stays out of the driver's frame.
#[inline(never)]
fn load_opponent_contract_v1(
    source: &ExpandedModelSourceV1,
    training_math: TrainingMathV1,
) -> Result<Option<String>, String> {
    let (policy, _) = load_expanded_inference_bound_v1(source, training_math)?;
    Ok(policy.input_contract_v1().map(str::to_owned))
}

/// Runs or resumes a declared schedule. The optional invocation limit pauses
/// only at completed iteration boundaries; it never changes the saved plan.
pub fn run_native_expanded_training_v1(
    config: &NativeExpandedTrainingRunV1,
    max_new_iterations: Option<usize>,
) -> Result<Value, String> {
    config.validate_v1()?;
    config.update_backend.require_compiled_v1()?;
    // Masked runs compare every update's frozen digest with the initial source's.
    let line_b_frozen_sha256 = config
        .line_b
        .as_ref()
        .map(|_| line_b_frozen_sha256_v1(&config.initial_source))
        .transpose()?;
    check(
        max_new_iterations != Some(0),
        "iteration limit must be positive",
    )?;
    let root = &config.output_directory;
    fs::create_dir_all(root).map_err(err)?;
    // OS lock lifetime, not a persisted PID/marker, determines writer ownership.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("run.lock"))
        .map_err(err)?;
    lock.try_lock()
        .map_err(|e| format!("run already has an active writer or cannot lock: {e}"))?;
    // A converted (no-state-digest) run loads its initial source before the
    // plan, which records its validated effective identity and contract. An
    // unconverted run keeps its plan bytes and loads afterwards, as before.
    let converted_initial = is_converted_source_v1(&config.initial_source)
        .then(|| load_converted_initial_v1(config))
        .transpose()?;
    let input_contract = converted_initial
        .as_ref()
        .map(|initial| initial.record.clone());
    let mut manifest = json!({"schema":SCHEMA, "config":config,
        "git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
        "feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3,
        "device":"cpu", "gpu_ordinal":null,
        "implementation_sha256":digest(include_bytes!("native_expanded_training_run_v1.rs"))});
    if let Some(initial) = &converted_initial {
        manifest["feature_contract_digest"] = json!(initial.identity.model.feature_contract_digest);
        manifest["feature_encoding_digest"] = json!(initial.identity.model.feature_encoding_digest);
        manifest["input_contract"] = initial.record.clone();
    }
    config.update_backend.record_run_execution_v1(&mut manifest);
    record_loss_selection_execution(config, &mut manifest);
    record_collection_execution(config, &mut manifest);
    let manifest_path = root.join("run.json");
    if manifest_path.exists() {
        check(
            read_json(&manifest_path, SMALL_CAP)? == manifest,
            "resume plan or implementation differs",
        )?;
    } else {
        check(
            !root.join("iterations").exists() && !root.join("completion.json").exists(),
            "run artifacts exist without their original plan",
        )?;
        publish(root, "run.json", &manifest)?;
    }
    let iterations_root = root.join("iterations");
    fs::create_dir_all(&iterations_root).map_err(err)?;
    let (mut current_identity, initial_contract) = match converted_initial {
        Some(initial) => (initial.identity, Some(initial.contract_digest)),
        None => {
            let (_, identity) =
                load_expanded_inference_bound_v1(&config.initial_source, config.training_math)?;
            (identity, None)
        }
    };
    // Validate the complete fixed roster before spending episode compute.
    for opponent in &config.opponents {
        check(
            load_opponent_contract_v1(&opponent.source, config.training_math)? == initial_contract,
            "fixed opponent and learner use different no-state-digest contracts",
        )?;
    }
    let mut current = config.initial_source.clone();
    let mut history = Vec::new();
    let mut receipts = Vec::new();
    let mut newly_completed = 0usize;
    let mut missing_seen = false;
    for index in 0..config.iterations.len() {
        let directory = iterations_root.join(format!("{index:06}"));
        if !directory.join("complete.json").exists() {
            missing_seen = true;
        } else {
            check(
                !missing_seen,
                "completed iterations are not a contiguous prefix",
            )?;
        }
    }
    check(
        !root.join("completion.json").exists() || !missing_seen,
        "run completion exists with a missing iteration prefix",
    )?;
    for index in 0..config.iterations.len() {
        let directory = iterations_root.join(format!("{index:06}"));
        let complete_path = directory.join("complete.json");
        if let Some(options) = &config.search_targets {
            check(
                complete_path.exists() || !options.packets_directory.join("STOP").exists(),
                "search-target run stopped before its next iteration",
            )?;
        }
        let episodes = resolve_episodes(config, index, &current, &history)?;
        let episode_digest = identity(&episodes)?;
        let e_options = config
            .e_supported
            .as_ref()
            .map(|options| options.teacher_options_v1(index));
        let sq_options = config
            .soft_q
            .as_ref()
            .map(|options| options.teacher_options_v1(index));
        if complete_path.exists() {
            let receipt = read_json(&complete_path, SMALL_CAP)?;
            check(
                receipt["iteration"] == index
                    && receipt["source"] == value(&current)?
                    && receipt["episodes_sha256"] == episode_digest,
                "completed iteration schedule differs",
            )?;
            let collection = parse_pin(&receipt, "collection")?;
            let trajectories = validate_collection(
                &collection,
                &current,
                &episodes,
                &current_identity,
                config.collection_sampler,
                config.training_math,
                input_contract.as_ref(),
            )?;
            let update = parse_pin(&receipt, "update")?;
            let (next, after) = validate_update(
                &update,
                &current,
                &trajectories,
                &current_identity,
                config.learning_rate,
                config.value_coefficient,
                config.update_backend,
                config.preparation_workers,
                config.max_prepared_tensor_mebibytes,
                line_b_frozen_sha256.as_deref(),
                e_options.as_ref(),
                sq_options.as_ref(),
                config.search_targets.as_ref().map(|o| (o, index)),
                config.training_math,
                input_contract.as_ref(),
            )?;
            check(
                receipt["output_identity"] == value(&after)?,
                "iteration checkpoint identity differs",
            )?;
            current = next;
            current_identity = after;
            history.push(current.clone());
            receipts.push(pin(&complete_path)?);
            continue;
        }
        if max_new_iterations.is_some_and(|limit| newly_completed >= limit) {
            break;
        }
        // Invocation telemetry stays outside immutable iteration receipts. It
        // measures wall time, including verification, and never claims CPU use.
        let iteration_started = Instant::now();
        let mut collection_validation_seconds = 0.0;
        let mut update_validation_seconds = 0.0;
        let mut collection_execution_seconds = 0.0;
        let mut update_execution_seconds = 0.0;
        fs::create_dir_all(&directory).map_err(err)?;
        // Reuse a fully published phase. A checkpoint without update.json is
        // an incomplete attempt and never becomes the next learner source.
        let mut attempts = Vec::new();
        for entry in fs::read_dir(&directory).map_err(err)? {
            let entry = entry.map_err(err)?;
            if let Some(number) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("attempt-"))
                .and_then(|number| number.parse::<usize>().ok())
            {
                check(
                    entry.file_type().map_err(err)?.is_dir(),
                    "attempt path is not a directory",
                )?;
                attempts.push((number, entry.path()));
            }
        }
        attempts.sort_by_key(|entry| entry.0);
        let mut collection_pin = None;
        let mut update_pin = None;
        // Only a collection fully validated in this invocation may reuse its
        // parsed schedule checks. Its bytes are still reverified after update.
        let mut validated_trajectories = None;
        for (_, attempt) in &attempts {
            let saved_command = attempt.join("collect-command.json");
            if !saved_command.exists() {
                continue;
            }
            let collect_command =
                collection_command(config, &current, &episodes, attempt.join("collect"));
            check(
                read_json(&saved_command, SMALL_CAP)? == value(&collect_command)?,
                "interrupted attempt collection command differs",
            )?;
            let path = attempt.join("collect/collection.json");
            if path.exists() {
                let candidate = pin(&path)?;
                let started = Instant::now();
                validate_collection(
                    &candidate,
                    &current,
                    &episodes,
                    &current_identity,
                    config.collection_sampler,
                    config.training_math,
                    input_contract.as_ref(),
                )?;
                collection_validation_seconds += started.elapsed().as_secs_f64();
                collection_pin = Some(candidate);
            }
            let path = attempt.join("update/update.json");
            if path.exists() {
                let candidate = pin(&path)?;
                let document = read_pin(&candidate)?;
                let referenced_collection = parse_pin(
                    &read_json(&attempt.join("update-input.json"), SMALL_CAP)?,
                    "collection",
                )?;
                let started = Instant::now();
                let trajectories = validate_collection(
                    &referenced_collection,
                    &current,
                    &episodes,
                    &current_identity,
                    config.collection_sampler,
                    config.training_math,
                    input_contract.as_ref(),
                )?;
                collection_validation_seconds += started.elapsed().as_secs_f64();
                let started = Instant::now();
                validate_update(
                    &candidate,
                    &current,
                    &trajectories,
                    &current_identity,
                    config.learning_rate,
                    config.value_coefficient,
                    config.update_backend,
                    config.preparation_workers,
                    config.max_prepared_tensor_mebibytes,
                    line_b_frozen_sha256.as_deref(),
                    e_options.as_ref(),
                    sq_options.as_ref(),
                    config.search_targets.as_ref().map(|o| (o, index)),
                    config.training_math,
                    input_contract.as_ref(),
                )?;
                update_validation_seconds += started.elapsed().as_secs_f64();
                check(document["complete"] == true, "incomplete update receipt")?;
                collection_pin = Some(referenced_collection);
                update_pin = Some(candidate);
                break;
            }
        }
        if update_pin.is_none() {
            let number = attempts.last().map_or(Ok(0usize), |entry| {
                entry.0.checked_add(1).ok_or("attempt overflow")
            })?;
            let attempt = directory.join(format!("attempt-{number:06}"));
            fs::create_dir(&attempt).map_err(err)?;
            if !attempts.is_empty() {
                let mut log = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("restarts.log"))
                    .map_err(err)?;
                writeln!(log, "iteration {index}: restart incomplete phase in attempt {number} from the same input state").map_err(err)?;
                log.sync_all().map_err(err)?;
            }
            let collect_command =
                collection_command(config, &current, &episodes, attempt.join("collect"));
            publish(&attempt, "collect-command.json", &collect_command)?;
            if collection_pin.is_none() {
                let started = Instant::now();
                execute_v1(collect_command)?;
                collection_execution_seconds += started.elapsed().as_secs_f64();
                collection_pin = Some(pin(&attempt.join("collect/collection.json"))?);
            }
            let collection = collection_pin.as_ref().unwrap();
            let started = Instant::now();
            let trajectories = validate_collection(
                collection,
                &current,
                &episodes,
                &current_identity,
                config.collection_sampler,
                config.training_math,
                input_contract.as_ref(),
            )?;
            collection_validation_seconds += started.elapsed().as_secs_f64();
            validated_trajectories = Some(trajectories.clone());
            publish(
                &attempt,
                "update-input.json",
                &json!({"collection":collection}),
            )?;
            let update_command = match &config.search_targets {
                Some(options) => ExpandedTrainingCommandV1::SearchTargetsUpdate {
                    source: current.clone(),
                    trajectories,
                    learning_rate: config.learning_rate,
                    value_coefficient: config.value_coefficient,
                    loss_selection: config.loss_selection,
                    targets: options.options_v1(index, options.wait_for_packet_v1(index)?),
                    output_directory: attempt.join("update"),
                },
                None => update_command(
                    config,
                    index,
                    &current,
                    trajectories,
                    attempt.join("update"),
                ),
            };
            publish(&attempt, "update-command.json", &update_command)?;
            let started = Instant::now();
            execute_v1(update_command)?;
            update_execution_seconds += started.elapsed().as_secs_f64();
            update_pin = Some(pin(&attempt.join("update/update.json"))?);
        }
        let collection = collection_pin.unwrap();
        let update = update_pin.unwrap();
        let started = Instant::now();
        let trajectories = if let Some(trajectories) = validated_trajectories {
            // Hash every referenced file again to reject post-update changes,
            // including any tolerant-collection ledger. Never trust mtime.
            reverify_validated_collection(&collection, &trajectories)?;
            trajectories
        } else {
            validate_collection(
                &collection,
                &current,
                &episodes,
                &current_identity,
                config.collection_sampler,
                config.training_math,
                input_contract.as_ref(),
            )?
        };
        collection_validation_seconds += started.elapsed().as_secs_f64();
        let started = Instant::now();
        let (next, after) = validate_update(
            &update,
            &current,
            &trajectories,
            &current_identity,
            config.learning_rate,
            config.value_coefficient,
            config.update_backend,
            config.preparation_workers,
            config.max_prepared_tensor_mebibytes,
            line_b_frozen_sha256.as_deref(),
            e_options.as_ref(),
            sq_options.as_ref(),
            config.search_targets.as_ref().map(|o| (o, index)),
            config.training_math,
            input_contract.as_ref(),
        )?;
        update_validation_seconds += started.elapsed().as_secs_f64();
        let mut receipt = json!({"schema":"mtg-kernel-native-expanded-iteration/v1", "iteration":index,
            "source":current,"episodes_sha256":episode_digest,"collection":collection,"update":update,
            "output_identity":after});
        if config.record_process_cpu {
            receipt["driver_process_cpu_seconds"] =
                json!(crate::regret_census_v1::stage4a::process_cpu::seconds()?);
        }
        if let Some(ledger_pin) = collection_non_natural_ledger_pin(&collection)? {
            receipt["non_natural_ledger"] = value(&ledger_pin)?;
        }
        if e_options.is_some() {
            let document = read_pin(&update)?;
            let e = &document["e_supported"];
            receipt["e_supported"] =
                json!({"packet":e["packet"],"scheduled":e["scheduled"],"valid":e["valid"]});
        }
        if sq_options.is_some() {
            let document = read_pin(&update)?;
            let sq = &document["soft_q"];
            receipt["soft_q"] = json!({"packet":sq["packet"],"scheduled":sq["scheduled"],
                "available":sq["available"]});
        }
        receipts.push(publish(&directory, "complete.json", &receipt)?);
        current = next;
        current_identity = after;
        history.push(current.clone());
        newly_completed += 1;
        println!(
            "{}",
            json!({"completed_iteration":index,"adam_step":current_identity.adam_step,
            "checkpoint":current.checkpoint,
            "scheduler_timing":{"schema":"phase1-scheduler-wall/v1",
                "scope":"newly_completed_iteration_excludes_initialization_and_completed_prefix_validation",
                "iteration_wall_seconds":iteration_started.elapsed().as_secs_f64(),
                "collection_execution_seconds":collection_execution_seconds,
                "update_execution_seconds":update_execution_seconds,
                "collection_validation_seconds":collection_validation_seconds,
                "update_validation_seconds":update_validation_seconds,
                "cpu_utilization_claim":false}})
        );
    }
    let complete = receipts.len() == config.iterations.len();
    let mut result = json!({"schema":SCHEMA,"complete":complete,"completed_iterations":receipts.len(),
        "planned_iterations":config.iterations.len(),"iterations":receipts,"source":current,
        "actual_identity":current_identity,"device":"cpu","gpu_ordinal":null,
        "loss_identity":config.loss_selection.loss_identity_v1(),"strength_claim":false});
    config.update_backend.record_run_execution_v1(&mut result);
    record_loss_selection_execution(config, &mut result);
    record_collection_execution(config, &mut result);
    let final_path = root.join("completion.json");
    if complete {
        if final_path.exists() {
            check(
                read_json(&final_path, SMALL_CAP)? == result,
                "saved run completion differs",
            )?;
        } else {
            publish(root, "completion.json", &result)?;
        }
    } else {
        check(
            !final_path.exists(),
            "completion exists for an incomplete prefix",
        )?;
    }
    drop(lock);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The final state of `portable_math_run_trains_one_state_on_every_system_and_worker_count`,
    /// the same on every system. Native236 and CI39598628050 observe this CPU
    /// regression state after the CR 117.3c/117.5 caster-priority correction.
    /// Paired inputs first differ at an off-turn Mystic trigger in iteration1;
    /// iteration0 and initialization match. Frozen experiment artifacts remain separate.
    const PORTABLE_PARALLEL_RUN_STATE_SHA256_V1: &str =
        "af94495eac0f2c8bd390173327afda86fd007cb81c69ac03cb2a945e80d29439";

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "native-expanded-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }

    #[test]
    fn bounded_pinned_parse_and_reverification_reject_changed_bytes() {
        let root = temporary_directory("reverify");
        let trajectory = publish(&root, "trajectory.json", &json!({"episode":1})).unwrap();
        let ledger = publish(&root, "ledger.json", &json!({"entries":[]})).unwrap();
        let collection = publish(
            &root,
            "collection.json",
            &json!({"trajectories":[trajectory], "non_natural_ledger":ledger}),
        )
        .unwrap();
        assert!(read_pin_bounded(&trajectory, 1).is_err());
        assert_eq!(
            read_pin_bounded(&trajectory, ARTIFACT_CAP).unwrap(),
            json!({"episode":1})
        );
        reverify_validated_collection(&collection, std::slice::from_ref(&trajectory)).unwrap();
        fs::write(&trajectory.path, b"{\"episode\":2}").unwrap();
        assert!(read_pin_bounded(&trajectory, ARTIFACT_CAP).is_err());
        assert!(
            reverify_validated_collection(&collection, std::slice::from_ref(&trajectory)).is_err()
        );
        fs::write(&trajectory.path, b"{\"episode\":1}").unwrap();
        fs::write(&ledger.path, b"{\"entries\":[1]}").unwrap();
        assert!(
            reverify_validated_collection(&collection, std::slice::from_ref(&trajectory)).is_err()
        );
        fs::write(&ledger.path, b"{\"entries\":[]}").unwrap();
        fs::write(&collection.path, b"{\"trajectories\":[]}").unwrap();
        assert!(reverify_validated_collection(&collection, &[trajectory]).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_stage_is_preserved_and_final_receipt_can_publish() {
        let directory = temporary_directory("stage");
        for name in ["run.json", "complete.json", "completion.json"] {
            let orphan = directory.join(format!(".{name}.stage"));
            fs::write(&orphan, b"interrupted partial bytes").unwrap();
            let expected = json!({"complete":true,"name":name});
            let receipt = publish(&directory, name, &expected).unwrap();
            assert_eq!(read_pin(&receipt).unwrap(), expected);
            assert_eq!(fs::read(&orphan).unwrap(), b"interrupted partial bytes");
            assert!(publish(&directory, name, &json!({"replacement":true})).is_err());
            assert_eq!(read_pin(&receipt).unwrap(), expected);
        }
    }

    #[test]
    fn run_lock_is_exclusive_and_released_by_handle_lifetime() {
        let directory = temporary_directory("lock");
        let first = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.join("run.lock"))
            .unwrap();
        first.try_lock().unwrap();
        let second = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("run.lock"))
            .unwrap();
        assert!(second.try_lock().is_err());
        drop(first);
        second.try_lock().unwrap();
    }

    fn schedule() -> NativeExpandedTrainingRunV1 {
        let source: ExpandedModelSourceV1 = serde_json::from_value(json!({
            "play_import":{"path":std::env::temp_dir().join("input.json"),"sha256":"a".repeat(64)},
            "feature_transfer":{"expected_feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
                "expected_feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3},"checkpoint":null
        }))
        .unwrap();
        let list = |id: &str| {
            let registration =
                crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
            let cards = registration.registered_configuration();
            crate::expanded_deck_training_v1::ExpandedDeckListV1 {
                label: id.into(),
                mainboard: cards.mainboard().to_vec(),
                sideboard: cards.sideboard().to_vec(),
            }
        };
        let registered = [list("Affinity"), list("Elves")];
        NativeExpandedTrainingRunV1 {
            schema: SCHEMA.into(),
            initial_source: source.clone(),
            opponents: vec![NamedExpandedOpponentV1 {
                id: "archive".into(),
                source,
            }],
            iterations: vec![ExpandedRunIterationV1 {
                episodes: vec![ScheduledExpandedEpisodeV1 {
                    episode: ExpandedEpisodeV1 {
                        id: "first".into(),
                        seed: 1,
                        starting_player: 0,
                        learner_seat: 0,
                        registered: registered.clone(),
                        selected: registered,
                        postboard: false,
                        max_physical_decisions: 4000,
                        max_policy_steps: 40000,
                        opponent: None,
                        opponent_kind: None,
                        opponent_search: None,
                        runtime_rules: None,
                        reset: None,
                    },
                    opponent: ExpandedOpponentAssignmentV1::Fixed {
                        id: "archive".into(),
                    },
                }],
            }],
            learning_rate: 0.00001,
            value_coefficient: 0.5,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            loss_selection: ExpandedLossSelectionV1::default(),
            update_backward_execution: UpdateBackwardExecutionV1::Sequential,
            collection_workers: 1,
            preparation_workers: 1,
            max_non_natural_episode_fraction: 0.0,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            line_b: None,
            e_supported: None,
            soft_q: None,
            search_targets: None,
            record_process_cpu: false,
            output_directory: std::env::temp_dir().join("native-expanded-validation-only"),
        }
    }

    #[test]
    fn collection_workers_preserve_serial_defaults_and_bind_parallel_resume() {
        let serial = schedule();
        let serial_value = value(&serial).unwrap();
        assert!(serial_value.get("collection_workers").is_none());
        let restored: NativeExpandedTrainingRunV1 =
            serde_json::from_value(serial_value.clone()).unwrap();
        assert_eq!(restored.collection_workers, 1);
        assert_eq!(value(&restored).unwrap(), serial_value);
        let mut execution = json!({"device":"cpu"});
        record_collection_execution(&serial, &mut execution);
        assert_eq!(execution, json!({"device":"cpu"}));

        let episodes = resolve_episodes(&serial, 0, &serial.initial_source, &[]).unwrap();
        let output = serial.output_directory.join("collect");
        let serial_command = value(&collection_command(
            &serial,
            &serial.initial_source,
            &episodes,
            output.clone(),
        ))
        .unwrap();
        assert_eq!(serial_command["mode"], "collect");

        let mut parallel = serial.clone();
        parallel.collection_workers = 4;
        parallel.validate_v1().unwrap();
        assert_ne!(identity(&serial).unwrap(), identity(&parallel).unwrap());
        let parallel_command = value(&collection_command(
            &parallel,
            &parallel.initial_source,
            &episodes,
            output,
        ))
        .unwrap();
        assert_eq!(parallel_command["mode"], "collect_parallel");
        assert_eq!(parallel_command["workers"], 4);
        assert_eq!(parallel_command["source"], serial_command["source"]);
        assert_eq!(parallel_command["episodes"], serial_command["episodes"]);
        record_collection_execution(&parallel, &mut execution);
        assert_eq!(
            execution["collection_backend"],
            "native-cpu-parallel-episodes-v1"
        );
        assert_eq!(execution["collection_workers_requested"], 4);
        parallel.collection_workers = 0;
        assert!(parallel.validate_v1().is_err());
        parallel.collection_workers = 1025;
        assert!(parallel.validate_v1().is_err());
    }

    #[test]
    fn preparation_workers_preserve_serial_wire_and_bind_parallel_resume() {
        let serial = schedule();
        let serial_value = value(&serial).unwrap();
        assert!(serial_value.get("preparation_workers").is_none());
        let restored: NativeExpandedTrainingRunV1 =
            serde_json::from_value(serial_value.clone()).unwrap();
        assert_eq!(restored.preparation_workers, 1);
        assert_eq!(value(&restored).unwrap(), serial_value);
        let trajectories = vec![PinnedFileV1 {
            path: serial.output_directory.join("trajectory.json"),
            sha256: "b".repeat(64),
        }];
        let output = serial.output_directory.join("update");
        let serial_command = value(&update_command(
            &serial,
            0,
            &serial.initial_source,
            trajectories.clone(),
            output.clone(),
        ))
        .unwrap();
        assert_eq!(serial_command["mode"], "update");
        let mut parallel = serial.clone();
        parallel.preparation_workers = 4;
        parallel.validate_v1().unwrap();
        assert_ne!(identity(&serial).unwrap(), identity(&parallel).unwrap());
        let mut parallel_command = value(&update_command(
            &parallel,
            0,
            &parallel.initial_source,
            trajectories,
            output,
        ))
        .unwrap();
        assert_eq!(parallel_command["mode"], "update_prepared");
        assert_eq!(
            parallel_command
                .as_object_mut()
                .unwrap()
                .remove("preparation_workers"),
            Some(json!(4))
        );
        parallel_command["mode"] = json!("update");
        assert_eq!(parallel_command, serial_command);
        for invalid in [0, 33, usize::MAX] {
            parallel.preparation_workers = invalid;
            assert!(parallel.validate_v1().is_err());
        }
    }

    #[test]
    fn max_prepared_tensor_mebibytes_preserves_wire_and_validates() {
        let plan = schedule();
        let wire = value(&plan).unwrap();
        assert!(wire.get("max_prepared_tensor_mebibytes").is_none());
        let decoded: NativeExpandedTrainingRunV1 = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded.max_prepared_tensor_mebibytes, 256);
        let mut raised = schedule();
        raised.preparation_workers = 4;
        raised.max_prepared_tensor_mebibytes = 1024;
        let wire = value(&raised).unwrap();
        assert_eq!(wire["max_prepared_tensor_mebibytes"], json!(1024));
        raised.validate_v1().unwrap();
        let mut serial = schedule();
        serial.max_prepared_tensor_mebibytes = 1024;
        assert!(serial
            .validate_v1()
            .unwrap_err()
            .contains("prepared updates only"));
        let mut out_of_range = schedule();
        out_of_range.preparation_workers = 4;
        out_of_range.max_prepared_tensor_mebibytes = 8192;
        assert!(out_of_range
            .validate_v1()
            .unwrap_err()
            .contains("64..=4096"));
        let mut manifest = json!({});
        record_collection_execution(&raised, &mut manifest);
        assert_eq!(manifest["max_prepared_tensor_mebibytes"], json!(1024));
        let mut default_manifest = json!({});
        record_collection_execution(&schedule(), &mut default_manifest);
        assert!(default_manifest
            .get("max_prepared_tensor_mebibytes")
            .is_none());
        match update_command(
            &raised,
            0,
            &raised.initial_source,
            Vec::new(),
            PathBuf::new(),
        ) {
            ExpandedTrainingCommandV1::UpdatePrepared {
                max_prepared_tensor_mebibytes,
                ..
            } => assert_eq!(max_prepared_tensor_mebibytes, 1024),
            _ => panic!("prepared run must issue a prepared update"),
        }
        let legacy = json!({"update_preparation": {"schema": "mtg-kernel-ordered-update-preparation/v1",
            "requested_workers": 4, "physical_group_jobs": 10, "started_workers": 4}});
        validate_preparation_execution(&legacy, 4, 256).unwrap();
        assert!(validate_preparation_execution(&legacy, 4, 1024)
            .unwrap_err()
            .contains("preparation bound differs"));
        let recorded = json!({"update_preparation": {"schema": "mtg-kernel-ordered-update-preparation/v1",
            "requested_workers": 4, "physical_group_jobs": 10, "started_workers": 4,
            "max_decoded_tensor_bytes": 1024u64 * 1024 * 1024}});
        validate_preparation_execution(&recorded, 4, 1024).unwrap();
        assert!(validate_preparation_execution(&recorded, 4, 256).is_err());
    }

    #[test]
    fn max_non_natural_episode_fraction_preserves_serial_wire_and_validates_range() {
        let serial = schedule();
        let serial_value = value(&serial).unwrap();
        assert!(serial_value
            .get("max_non_natural_episode_fraction")
            .is_none());
        let restored: NativeExpandedTrainingRunV1 =
            serde_json::from_value(serial_value.clone()).unwrap();
        assert_eq!(restored.max_non_natural_episode_fraction, 0.0);
        assert_eq!(value(&restored).unwrap(), serial_value);

        let mut tolerant = serial.clone();
        tolerant.max_non_natural_episode_fraction = 0.2;
        tolerant.validate_v1().unwrap();
        assert_ne!(identity(&serial).unwrap(), identity(&tolerant).unwrap());
        let tolerant_value = value(&tolerant).unwrap();
        assert_eq!(
            tolerant_value["max_non_natural_episode_fraction"],
            json!(0.2f32)
        );

        for invalid in [
            -0.1f32,
            1.0,
            1.5,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            tolerant.max_non_natural_episode_fraction = invalid;
            assert!(tolerant.validate_v1().is_err(), "must reject {invalid}");
        }

        let episodes = resolve_episodes(&serial, 0, &serial.initial_source, &[]).unwrap();
        tolerant.max_non_natural_episode_fraction = 0.2;
        let command = value(&collection_command(
            &tolerant,
            &tolerant.initial_source,
            &episodes,
            tolerant.output_directory.join("collect"),
        ))
        .unwrap();
        assert_eq!(command["max_non_natural_episode_fraction"], json!(0.2f32));
    }

    #[test]
    fn collection_non_natural_ledger_pin_extracts_or_omits_the_ledger_reference() {
        let directory = temporary_directory("ledger-pin");
        let write = |name: &str, document: &Value| -> PinnedFileV1 {
            let bytes = serde_json::to_vec(document).unwrap();
            let path = directory.join(name);
            fs::write(&path, &bytes).unwrap();
            PinnedFileV1 {
                path,
                sha256: digest(&bytes),
            }
        };
        let without = write(
            "collection-without.json",
            &json!({"complete": true, "trajectories": []}),
        );
        assert_eq!(collection_non_natural_ledger_pin(&without).unwrap(), None);

        let with_null = write(
            "collection-null.json",
            &json!({"complete": true, "non_natural_ledger": null}),
        );
        assert_eq!(collection_non_natural_ledger_pin(&with_null).unwrap(), None);

        let ledger_pin = PinnedFileV1 {
            path: directory.join("non-natural.json"),
            sha256: "c".repeat(64),
        };
        let with_ledger = write(
            "collection-with.json",
            &json!({"complete": true, "non_natural_ledger": value(&ledger_pin).unwrap()}),
        );
        assert_eq!(
            collection_non_natural_ledger_pin(&with_ledger).unwrap(),
            Some(ledger_pin)
        );
    }

    #[test]
    fn collection_schedule_check_admits_a_ledgered_retry_seed_and_nothing_else() {
        let directory = temporary_directory("ledgered-schedule");
        let write = |name: &str, document: &Value| -> PinnedFileV1 {
            let bytes = serde_json::to_vec(document).unwrap();
            let path = directory.join(name);
            fs::write(&path, &bytes).unwrap();
            PinnedFileV1 {
                path,
                sha256: digest(&bytes),
            }
        };
        let plan = schedule();
        let episode = plan.iterations[0].episodes[0].episode.clone();
        let retry_seed =
            crate::expanded_deck_training_v1::derived_retry_seed_v1(&episode.id, episode.seed, 1);
        let mut retried = episode.clone();
        retried.seed = retry_seed;
        let scheduled_trajectory = write(
            "scheduled.json",
            &json!({"episode": value(&episode).unwrap()}),
        );
        let retried_trajectory = write(
            "retried.json",
            &json!({"episode": value(&retried).unwrap()}),
        );
        let ledger = write(
            "non-natural.json",
            &json!({"schema": "mtg-kernel-non-natural-collection-ledger/v1", "entries": [{
                "slot": 0, "attempt": 1, "episode_id": episode.id.clone(), "seed": episode.seed,
                "decks": [episode.selected[0].label.clone(), episode.selected[1].label.clone()],
                "starting_player": episode.starting_player,
                "terminal_classification": "halted",
                "terminal_reason": "engine_halted:test", "error_text": "test", "step": 7}]}),
        );
        let empty_ledger = write(
            "empty-ledger.json",
            &json!({"schema": "mtg-kernel-non-natural-collection-ledger/v1", "entries": []}),
        );
        let collection = |trajectory: &PinnedFileV1, ledger: Option<&PinnedFileV1>| {
            let mut document = json!({"trajectories": [value(trajectory).unwrap()]});
            if let Some(ledger) = ledger {
                document["non_natural_ledger"] = value(ledger).unwrap();
            }
            document
        };
        let episodes = vec![episode.clone()];

        // Today's behavior, byte for byte: no ledger, the scheduled seed passes
        // and any other seed is rejected.
        validate_collection_episodes(&collection(&scheduled_trajectory, None), &episodes).unwrap();
        assert_eq!(
            validate_collection_episodes(&collection(&retried_trajectory, None), &episodes)
                .unwrap_err(),
            "collection episode assignment differs"
        );
        // An empty ledger (every tolerant iteration writes one) changes nothing.
        validate_collection_episodes(
            &collection(&scheduled_trajectory, Some(&empty_ledger)),
            &episodes,
        )
        .unwrap();
        assert_eq!(
            validate_collection_episodes(
                &collection(&retried_trajectory, Some(&empty_ledger)),
                &episodes
            )
            .unwrap_err(),
            "collection episode assignment differs"
        );
        // One ledgered failure: only the re-derived retry seed is admitted for
        // that slot; the scheduled seed is now the wrong one.
        validate_collection_episodes(&collection(&retried_trajectory, Some(&ledger)), &episodes)
            .unwrap();
        assert_eq!(
            validate_collection_episodes(
                &collection(&scheduled_trajectory, Some(&ledger)),
                &episodes
            )
            .unwrap_err(),
            "collection episode assignment differs"
        );
    }

    #[test]
    fn collection_learner_sampler_must_match_the_run_config() {
        let directory = temporary_directory("learner-sampler-binding");
        let write = |name: &str, document: &Value| -> PinnedFileV1 {
            let bytes = serde_json::to_vec(document).unwrap();
            let path = directory.join(name);
            fs::write(&path, &bytes).unwrap();
            PinnedFileV1 {
                path,
                sha256: digest(&bytes),
            }
        };
        let episode = schedule().iterations[0].episodes[0].episode.clone();
        let identity = CollectionSamplerV1::UnclampedSoftmaxF64IcdfU53V1
            .identity_v1()
            .unwrap();
        let legacy = write("legacy.json", &json!({"episode": value(&episode).unwrap()}));
        let unclamped = write(
            "unclamped.json",
            &json!({"episode": value(&episode).unwrap(), "learner_sampler": identity}),
        );
        let collection =
            |trajectory: &PinnedFileV1| json!({"trajectories": [value(trajectory).unwrap()]});
        let episodes = vec![episode];
        validate_collection_episodes_with_sampler_v1(
            &collection(&legacy),
            &episodes,
            None,
            TrainingMathV1::PlatformLibm,
            None,
        )
        .unwrap();
        validate_collection_episodes_with_sampler_v1(
            &collection(&unclamped),
            &episodes,
            Some(identity),
            TrainingMathV1::PlatformLibm,
            None,
        )
        .unwrap();
        for (trajectory, expected) in [(&legacy, Some(identity)), (&unclamped, None)] {
            assert_eq!(
                validate_collection_episodes_with_sampler_v1(
                    &collection(trajectory),
                    &episodes,
                    expected,
                    TrainingMathV1::PlatformLibm,
                    None,
                )
                .unwrap_err(),
                "collection learner sampler differs from the run config"
            );
        }
    }

    /// The run's no-state-digest contract member binds every trajectory: an
    /// unconverted run admits only its absence, and an explicit `null`, which
    /// the typed trajectory reader refuses, is not absence here either.
    #[test]
    fn collection_trajectory_contract_member_must_match_the_run() {
        let directory = temporary_directory("input-contract-binding");
        let write = |name: &str, document: &Value| -> PinnedFileV1 {
            let bytes = serde_json::to_vec(document).unwrap();
            let path = directory.join(name);
            fs::write(&path, &bytes).unwrap();
            PinnedFileV1 {
                path,
                sha256: digest(&bytes),
            }
        };
        let episode = schedule().iterations[0].episodes[0].episode.clone();
        let record = json!({"contract": "no-state-digest-v1", "state_digest_columns": [123, 219],
            "conversion": {"path": "conversion.json", "sha256": "c".repeat(64)}});
        let absent = write("absent.json", &json!({"episode": value(&episode).unwrap()}));
        let null = write(
            "null.json",
            &json!({"episode": value(&episode).unwrap(), "input_contract": null}),
        );
        let converted = write(
            "converted.json",
            &json!({"episode": value(&episode).unwrap(), "input_contract": record}),
        );
        let collection =
            |trajectory: &PinnedFileV1| json!({"trajectories": [value(trajectory).unwrap()]});
        let episodes = vec![episode];
        let validate = |trajectory: &PinnedFileV1, expected: Option<&Value>| {
            validate_collection_episodes_with_sampler_v1(
                &collection(trajectory),
                &episodes,
                None,
                TrainingMathV1::PlatformLibm,
                expected,
            )
        };
        validate(&absent, None).unwrap();
        validate(&converted, Some(&record)).unwrap();
        for (trajectory, expected) in [
            (&null, None),
            (&null, Some(&record)),
            (&converted, None),
            (&absent, Some(&record)),
        ] {
            assert_eq!(
                validate(trajectory, expected).unwrap_err(),
                "collection trajectory no-state-digest contract differs from the run's"
            );
        }
    }

    #[test]
    fn recovered_update_must_match_actual_preparation_mode() {
        let serial = json!({});
        validate_preparation_execution(&serial, 1, 256).unwrap();
        assert!(validate_preparation_execution(&serial, 4, 256).is_err());
        let prepared = json!({"update_preparation":{
            "schema":"mtg-kernel-ordered-update-preparation/v1",
            "requested_workers":4,"started_workers":3,"physical_group_jobs":3}});
        validate_preparation_execution(&prepared, 4, 256).unwrap();
        assert!(validate_preparation_execution(&prepared, 1, 256).is_err());
        assert!(validate_preparation_execution(&prepared, 2, 256).is_err());
        for (field, bad) in [
            ("schema", json!("unknown")),
            ("started_workers", json!(4)),
            ("physical_group_jobs", json!(0)),
            ("physical_group_jobs", json!(65_537)),
        ] {
            let mut changed = prepared.clone();
            changed["update_preparation"][field] = bad;
            assert!(validate_preparation_execution(&changed, 4, 256).is_err());
        }
    }

    #[test]
    fn update_backend_is_explicit_in_run_identity_and_legacy_cpu_is_unchanged() {
        let cpu = schedule();
        let cpu_value = value(&cpu).unwrap();
        assert!(cpu_value.get("update_backend").is_none());
        let restored: NativeExpandedTrainingRunV1 = serde_json::from_value(cpu_value).unwrap();
        assert_eq!(restored.update_backend, ExpandedUpdateBackendV1::Cpu);
        let mut cuda = cpu.clone();
        cuda.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 1 };
        cuda.validate_v1().unwrap();
        assert_ne!(identity(&cpu).unwrap(), identity(&cuda).unwrap());
        let mut another_device = cuda.clone();
        another_device.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 2 };
        assert_ne!(identity(&cuda).unwrap(), identity(&another_device).unwrap());
        let mut execution = json!({"device":"cpu", "gpu_ordinal":null});
        cpu.update_backend.record_run_execution_v1(&mut execution);
        assert_eq!(execution, json!({"device":"cpu", "gpu_ordinal":null}));
        cuda.update_backend.record_run_execution_v1(&mut execution);
        assert_eq!(execution["device"], "cpu-collection-cuda-update");
        assert_eq!(execution["collection_backend"], "native-cpu-sequential");
        assert_eq!(execution["update_backend"]["device_ordinal"], 1);
        assert_eq!(execution["gpu_ordinal"], 1);
    }

    #[cfg(not(feature = "experimental-burn-net8-packed-cuda-v1"))]
    #[test]
    fn unavailable_cuda_run_rejects_before_artifacts_or_collection() {
        let mut config = schedule();
        config.output_directory =
            std::env::temp_dir().join(format!("native-expanded-no-cuda-{}", std::process::id()));
        assert!(!config.output_directory.exists());
        config.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 1 };
        let error = run_native_expanded_training_v1(&config, None).unwrap_err();
        assert!(
            error.contains("CUDA update backend was not compiled"),
            "{error}"
        );
        assert!(!config.output_directory.exists());
    }

    #[test]
    fn schedule_rejects_missing_future_and_competing_opponent_sources() {
        let config = schedule();
        config.validate_v1().unwrap();
        let mut missing = config.clone();
        missing.iterations[0].episodes[0].opponent = ExpandedOpponentAssignmentV1::Fixed {
            id: "missing".into(),
        };
        assert!(missing
            .validate_v1()
            .unwrap_err()
            .contains("absent from roster"));
        let mut future = config.clone();
        future.iterations[0].episodes[0].opponent =
            ExpandedOpponentAssignmentV1::CompletedIteration { index: 0 };
        assert!(future
            .validate_v1()
            .unwrap_err()
            .contains("future iteration"));
        let mut conflicting = config.clone();
        conflicting.iterations[0].episodes[0].episode.opponent =
            Some(config.initial_source.clone());
        assert!(conflicting
            .validate_v1()
            .unwrap_err()
            .contains("schedule owns opponent"));
    }

    #[test]
    fn historical_assignment_resolves_the_declared_completed_checkpoint() {
        let mut config = schedule();
        let mut iteration = config.iterations[0].clone();
        iteration.episodes[0].episode.id = "second".into();
        iteration.episodes[0].opponent =
            ExpandedOpponentAssignmentV1::CompletedIteration { index: 0 };
        config.iterations.push(iteration);
        config.validate_v1().unwrap();
        let mut previous = config.initial_source.clone();
        previous.checkpoint = Some(PinnedFileV1 {
            path: std::env::temp_dir().join("previous.json"),
            sha256: "b".repeat(64),
        });
        let mut current = previous.clone();
        current.checkpoint.as_mut().unwrap().sha256 = "c".repeat(64);
        let episodes = resolve_episodes(&config, 1, &current, &[previous.clone()]).unwrap();
        assert_eq!(
            value(episodes[0].opponent.as_ref().unwrap()).unwrap(),
            value(&previous).unwrap()
        );
        assert_ne!(
            value(episodes[0].opponent.as_ref().unwrap()).unwrap(),
            value(&current).unwrap()
        );
    }

    #[test]
    fn a_named_opponent_holding_the_initial_source_keeps_playing_it_from_another_start() {
        // Spy archive distillation phase 2 (collab DESIGN.md section 2): O's
        // `Initial` units become a fixed "parent" entry holding O's initial
        // source, and the run starts from a phase-1 checkpoint. Those units
        // must still play the parent, which `Initial` would no longer do.
        let mut config = schedule();
        config.iterations[0].episodes[0].opponent = ExpandedOpponentAssignmentV1::Initial;
        config.validate_v1().unwrap();
        let parent = config.initial_source.clone();
        let as_o = resolve_episodes(&config, 0, &parent, &[]).unwrap();
        let mut distilled = parent.clone();
        distilled.checkpoint = Some(PinnedFileV1 {
            path: std::env::temp_dir().join("phase1-step-1024.json"),
            sha256: "d".repeat(64),
        });
        config.initial_source = distilled.clone();
        let unnamed = resolve_episodes(&config, 0, &distilled, &[]).unwrap();
        assert_eq!(
            value(unnamed[0].opponent.as_ref().unwrap()).unwrap(),
            value(&distilled).unwrap()
        );
        config.opponents.push(NamedExpandedOpponentV1 {
            id: "parent".into(),
            source: parent.clone(),
        });
        config.iterations[0].episodes[0].opponent = ExpandedOpponentAssignmentV1::Fixed {
            id: "parent".into(),
        };
        config.validate_v1().unwrap();
        let named = resolve_episodes(&config, 0, &distilled, &[]).unwrap();
        let opponent = value(named[0].opponent.as_ref().unwrap()).unwrap();
        assert_eq!(opponent, value(as_o[0].opponent.as_ref().unwrap()).unwrap());
        assert_eq!(opponent, value(&parent).unwrap());
    }

    // --- Follow-up: the run harness must carry `loss_selection`, or arm g
    // cannot launch. Mirrors `update_backend_is_explicit_in_run_identity_and_legacy_cpu_is_unchanged`'s
    // structural style, plus a real end-to-end run for the forwarding proof.

    #[test]
    fn loss_selection_preserves_legacy_wire_and_is_explicit_in_run_identity() {
        let legacy = schedule();
        let legacy_value = value(&legacy).unwrap();
        assert!(legacy_value.get("loss_selection").is_none());
        let restored: NativeExpandedTrainingRunV1 =
            serde_json::from_value(legacy_value.clone()).unwrap();
        assert_eq!(restored.loss_selection, ExpandedLossSelectionV1::default());
        assert_eq!(value(&restored).unwrap(), legacy_value);
        let mut manifest = json!({});
        record_loss_selection_execution(&legacy, &mut manifest);
        assert_eq!(manifest, json!({}));
        let mut result = json!({});
        record_loss_selection_execution(&legacy, &mut result);
        assert_eq!(result, json!({}));

        let mut gae = legacy.clone();
        gae.loss_selection = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        };
        gae.validate_v1().unwrap();
        assert_ne!(identity(&legacy).unwrap(), identity(&gae).unwrap());
        let mut gae_manifest = json!({});
        record_loss_selection_execution(&gae, &mut gae_manifest);
        assert_eq!(
            gae_manifest["loss_selection"]["kind"],
            "gae_advantage_value_v1"
        );
        assert_eq!(gae_manifest["loss_selection"]["gamma"], 1.0);
        assert_eq!(gae_manifest["loss_selection"]["lambda"], 0.9_f32 as f64);
        assert_eq!(gae_manifest["loss_selection"]["entropy_coefficient"], 0.0);

        // The forwarded selection reaches the actual Update command.
        let legacy_command = update_command(
            &legacy,
            0,
            &legacy.initial_source,
            vec![],
            legacy.output_directory.clone(),
        );
        match legacy_command {
            ExpandedTrainingCommandV1::Update { loss_selection, .. } => {
                assert_eq!(loss_selection, ExpandedLossSelectionV1::default());
            }
            _ => panic!("expected an Update command"),
        }
        let gae_command = update_command(
            &gae,
            0,
            &gae.initial_source,
            vec![],
            gae.output_directory.clone(),
        );
        match gae_command {
            ExpandedTrainingCommandV1::Update { loss_selection, .. } => {
                assert_eq!(loss_selection, gae.loss_selection);
            }
            _ => panic!("expected an Update command"),
        }
    }

    #[test]
    fn unknown_loss_selection_kind_is_rejected_with_a_readable_error() {
        let mut legacy_value = value(&schedule()).unwrap();
        legacy_value["loss_selection"] = json!({"kind": "not_a_real_loss"});
        let error = serde_json::from_value::<NativeExpandedTrainingRunV1>(legacy_value)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("unknown variant") && error.contains("not_a_real_loss"),
            "{error}"
        );
    }

    /// A real fresh V4 source plus a real two-iteration, self-play (`Current`
    /// opponent) schedule over real constructed decks, run end to end
    /// through the actual `run_native_expanded_training_v1` production path
    /// (not the lower-level `execute_v1` calls
    /// `expanded_deck_training_v1::tests::run_ordinary_two_iteration_fixture_v1`
    /// makes directly): this is what proves `update_command`'s forwarding,
    /// not just the structural match above.
    fn real_two_iteration_config_v1(
        label: &str,
        loss_selection: ExpandedLossSelectionV1,
        update_backend: ExpandedUpdateBackendV1,
    ) -> NativeExpandedTrainingRunV1 {
        let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
        let root = std::env::temp_dir().join(format!(
            "native-expanded-loss-selection-{label}-{}",
            std::process::id()
        ));
        let source_struct =
            crate::expanded_deck_training_v1::write_synthetic_fresh_source_with_parameters_v1(
                &root.join("source"),
                feature_identity,
                |_parameters| {},
            );
        let descriptor_path = root.join("descriptor.json");
        let descriptor_bytes = serde_json::to_vec(&source_struct).unwrap();
        fs::write(&descriptor_path, &descriptor_bytes).unwrap();
        let play_import = PinnedFileV1 {
            path: descriptor_path.canonicalize().unwrap(),
            sha256: digest(&descriptor_bytes),
        };
        let feature_transfer = crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        };
        let initial_source = ExpandedModelSourceV1 {
            play_import,
            feature_transfer,
            checkpoint: None,
        };
        let list = |id: &str| {
            let registration =
                crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
            let cards = registration.registered_configuration();
            crate::expanded_deck_training_v1::ExpandedDeckListV1 {
                label: id.into(),
                mainboard: cards.mainboard().to_vec(),
                sideboard: cards.sideboard().to_vec(),
            }
        };
        let decks = [list("Affinity"), list("Terror")];
        let iterations = (0..2u64)
            .map(|iteration| ExpandedRunIterationV1 {
                episodes: vec![ScheduledExpandedEpisodeV1 {
                    episode: ExpandedEpisodeV1 {
                        id: format!("{label}-iter-{iteration}"),
                        seed: 2_026_091_601 + iteration,
                        starting_player: 0,
                        learner_seat: 0,
                        opponent: None,
                        opponent_kind: None,
                        opponent_search: None,
                        registered: decks.clone(),
                        selected: decks.clone(),
                        postboard: false,
                        max_physical_decisions: 100_000,
                        max_policy_steps: 1_000_000,
                        runtime_rules: None,
                        reset: None,
                    },
                    opponent: ExpandedOpponentAssignmentV1::Current,
                }],
            })
            .collect();
        NativeExpandedTrainingRunV1 {
            schema: SCHEMA.into(),
            initial_source,
            opponents: vec![],
            iterations,
            learning_rate: 0.0003,
            value_coefficient: 0.5,
            update_backend,
            loss_selection,
            update_backward_execution: UpdateBackwardExecutionV1::Sequential,
            collection_workers: 1,
            preparation_workers: 1,
            max_non_natural_episode_fraction: 0.0,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            line_b: None,
            e_supported: None,
            soft_q: None,
            search_targets: None,
            record_process_cpu: false,
            output_directory: root.join("run"),
        }
    }

    /// `result["iterations"]` is a `Vec<PinnedFileV1>`, one pin per
    /// iteration's own `complete.json` (see `receipts.push` at both the
    /// resume-path and the newly-completed-path call sites); each
    /// `complete.json` in turn carries its own `update` field as a second
    /// `PinnedFileV1` pin to that iteration's `update.json`. Two levels of
    /// indirection to read back, not one.
    fn last_update_loss_identity_v1(result: &Value) -> String {
        let complete_pin: PinnedFileV1 =
            serde_json::from_value(result["iterations"][1].clone()).unwrap();
        let complete_document: Value =
            serde_json::from_str(&fs::read_to_string(&complete_pin.path).unwrap()).unwrap();
        let update_pin: PinnedFileV1 =
            serde_json::from_value(complete_document["update"].clone()).unwrap();
        let document: Value =
            serde_json::from_str(&fs::read_to_string(&update_pin.path).unwrap()).unwrap();
        document["loss_identity"].as_str().unwrap().to_owned()
    }

    #[test]
    fn loss_selection_reaches_a_real_two_iteration_cpu_run_receipt() {
        let config = real_two_iteration_config_v1(
            "cpu",
            ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            ExpandedUpdateBackendV1::Cpu,
        );
        config.validate_v1().unwrap();
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["loss_identity"], "gae_advantage_value/v1");
        assert_eq!(
            last_update_loss_identity_v1(&result),
            "gae_advantage_value/v1"
        );
        let manifest: Value =
            read_json(&config.output_directory.join("run.json"), SMALL_CAP).unwrap();
        assert_eq!(manifest["loss_selection"]["kind"], "gae_advantage_value_v1");
        // An unconverted run keeps its plan's historical digests and carries
        // no no-state-digest contract member.
        assert_eq!(
            manifest["feature_contract_digest"],
            FEATURE_CONTRACT_DIGEST_V3
        );
        assert_eq!(
            manifest["feature_encoding_digest"],
            FEATURE_ENCODING_DIGEST_V3
        );
        assert!(manifest.get("input_contract").is_none());
    }

    fn e_supported_run_config_v1(label: &str) -> NativeExpandedTrainingRunV1 {
        let mut config = real_two_iteration_config_v1(
            label,
            ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            ExpandedUpdateBackendV1::Cpu,
        );
        config.learning_rate = 0.0001;
        config.e_supported = Some(ESupportedRunOptionsV1 {
            teacher: config.initial_source.clone(),
            treatment: true,
            prior_deck_indices: (0..9).collect(),
            selection_transition_cap: 128,
            // Update 0 is supervised; update 1 is a declared ordinary update.
            slots: vec![
                vec![ESupportedSlotV1 {
                    trajectory_index: 0,
                    case_id: format!("{label}/main/update/0/episode/0"),
                }],
                vec![],
            ],
        });
        config
    }

    fn iteration_receipt_v1(result: &Value, index: usize) -> Value {
        let pin: PinnedFileV1 =
            serde_json::from_value(result["iterations"][index].clone()).unwrap();
        read_pin(&pin).unwrap()
    }

    #[test]
    fn e_supported_run_binds_packets_resumes_and_replays() {
        let config = e_supported_run_config_v1("e-run-a");
        config.validate_v1().unwrap();
        let first = run_native_expanded_training_v1(&config, Some(1)).unwrap();
        assert_eq!(first["completed_iterations"], 1);
        // The second invocation re-validates the E prefix before its new update.
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["complete"], true);
        let supervised = iteration_receipt_v1(&result, 0);
        let ordinary = iteration_receipt_v1(&result, 1);
        assert_eq!(supervised["e_supported"]["scheduled"], 1);
        assert_eq!(ordinary["e_supported"]["scheduled"], 0);
        assert_eq!(ordinary["e_supported"]["valid"], 0);
        let update: PinnedFileV1 = serde_json::from_value(ordinary["update"].clone()).unwrap();
        let checkpoint = parse_pin(&read_pin(&update).unwrap(), "checkpoint").unwrap();
        let checkpoint = read_pin_bounded(&checkpoint, ARTIFACT_CAP).unwrap();
        assert_eq!(checkpoint["e_supported"]["continuation"]["treatment"], true);
        assert_eq!(result["actual_identity"]["adam_step"], 2);
        // A complete rerun only verifies; a fresh directory replays identically.
        assert_eq!(
            run_native_expanded_training_v1(&config, None).unwrap(),
            result
        );
        let mut replay = config.clone();
        replay.output_directory = config.output_directory.with_file_name("run-replay");
        let again = run_native_expanded_training_v1(&replay, None).unwrap();
        assert_eq!(
            again["actual_identity"]["state_sha256"],
            result["actual_identity"]["state_sha256"]
        );
        assert_eq!(
            iteration_receipt_v1(&again, 0)["e_supported"]["packet"]["sha256"],
            supervised["e_supported"]["packet"]["sha256"]
        );
        // The plan, including the treatment bit, is bound for resume.
        let mut changed = config.clone();
        changed.e_supported.as_mut().unwrap().treatment = false;
        let error = run_native_expanded_training_v1(&changed, None).unwrap_err();
        assert!(error.contains("resume plan"), "{error}");
    }

    fn soft_q_run_config_v1(
        label: &str,
        arm: SoftQArmV1,
        workers: usize,
    ) -> NativeExpandedTrainingRunV1 {
        let mut config = real_two_iteration_config_v1(
            label,
            ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            ExpandedUpdateBackendV1::Cpu,
        );
        config.learning_rate = 0.0001;
        config.soft_q = Some(SoftQRunOptionsV1 {
            arm,
            case_namespace: "sq1-test/r0/s0".into(),
            prior_deck_indices: (0..9).collect(),
            teacher_workers: workers,
            engineering_zero_treatment: false,
        });
        config
    }

    /// TP1 Stage B: a stand-in orchestrator writes each update's packet, bound
    /// to that update's start checkpoint, once the previous iteration ends.
    fn spawn_packet_writer_v1(
        config: &NativeExpandedTrainingRunV1,
        updates: usize,
        tamper_update: Option<usize>,
    ) -> std::thread::JoinHandle<()> {
        let options = config.search_targets.clone().unwrap();
        let run = config.output_directory.clone();
        let initial = config.initial_source.checkpoint.clone();
        std::thread::spawn(move || {
            std::fs::create_dir_all(&options.packets_directory).unwrap();
            for u in 0..updates {
                let start = if u == 0 {
                    json!(initial)
                } else {
                    let complete = run
                        .join("iterations")
                        .join(format!("{:06}", u - 1))
                        .join("complete.json");
                    while !complete.exists() {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    let receipt = read_json(&complete, SMALL_CAP).unwrap();
                    let update = read_pin(&parse_pin(&receipt, "update").unwrap()).unwrap();
                    update["checkpoint"].clone()
                };
                let bound = if tamper_update == Some(u) { u + 7 } else { u };
                let game = options.packets_directory.join(format!("game-{u}.jsonl"));
                std::fs::write(
                    &game,
                    format!(
                        "{{\"game\":{u}}}
"
                    ),
                )
                .unwrap();
                let packet = json!({"schema":"tp1b-search-targets/v1","binding":{
                    "run":options.run,"arm":options.arm,"seed":options.seed,"update":bound,
                    "start_checkpoint":start,"case":null,"game":pin(&game).unwrap(),
                    "allocation":{"initial":32000,"refresh":16000,"total":512000}},"rows":[]});
                std::fs::write(
                    options.packet_path_v1(u),
                    serde_json::to_vec(&packet).unwrap(),
                )
                .unwrap();
            }
        })
    }

    fn search_targets_run_config_v1(label: &str) -> NativeExpandedTrainingRunV1 {
        let mut config = soft_q_run_config_v1(label, SoftQArmV1::Treatment, 2);
        config.soft_q = None;
        let packets = config.output_directory.with_extension("packets");
        let _ = std::fs::remove_dir_all(&packets);
        std::fs::create_dir_all(&packets).unwrap();
        let spec = packets.with_extension("spec.json");
        std::fs::write(&spec, b"{}").unwrap();
        config.search_targets = Some(SearchTargetsRunOptionsV1 {
            packets_directory: packets,
            bank_spec: pin(&spec).unwrap(),
            opponents: vec![SearchTargetsOpponentV1 {
                label: "T1".into(),
                source: config.initial_source.clone(),
            }],
            coefficient: 1.0,
            denominator: 32,
            run: label.into(),
            arm: "T".into(),
            seed: 1,
            update_offset: 0,
            wait_seconds: 600,
            allocation: json!({"initial":32000,"refresh":16000,"total":512000}),
            target: Default::default(),
            cap: Default::default(),
        });
        config
    }

    /// TP1 Stage B: a packet is refused before its update when its searched
    /// game is not an authenticated record of its case (even with no rows),
    /// when it is bound to another update, or when STOP exists, even if the
    /// packet is already there.
    #[test]
    fn search_target_runs_refuse_unbound_packets_and_honor_stop() {
        let config = search_targets_run_config_v1("st-run-fake-game");
        config.validate_v1().unwrap();
        let writer = spawn_packet_writer_v1(&config, 1, None);
        let error = run_native_expanded_training_v1(&config, None).unwrap_err();
        writer.join().unwrap();
        assert!(!error.is_empty());
        assert!(!config
            .output_directory
            .join("iterations/000000/complete.json")
            .exists());
        let tampered = search_targets_run_config_v1("st-run-tampered");
        let writer = spawn_packet_writer_v1(&tampered, 1, Some(0));
        let error = run_native_expanded_training_v1(&tampered, None).unwrap_err();
        writer.join().unwrap();
        assert!(
            error.contains("another run, arm, seed or update"),
            "{error}"
        );
        let stopped = search_targets_run_config_v1("st-run-stopped");
        let writer = spawn_packet_writer_v1(&stopped, 1, None);
        writer.join().unwrap();
        let options = stopped.search_targets.as_ref().unwrap();
        std::fs::write(options.packets_directory.join("STOP"), b"").unwrap();
        let error = run_native_expanded_training_v1(&stopped, None).unwrap_err();
        assert!(error.contains("stopped"), "{error}");
    }

    /// TP1 Stage B's ledger: with `record_process_cpu`, every iteration
    /// receipt records the driver's own process CPU at its boundary,
    /// nondecreasing; without it receipts are unchanged.
    #[test]
    fn record_process_cpu_marks_every_iteration_boundary() {
        let mut config = soft_q_run_config_v1("cpu-marks", SoftQArmV1::Treatment, 1);
        config.soft_q = None;
        config.record_process_cpu = true;
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        let marks: Vec<f64> = (0..2)
            .map(|i| {
                iteration_receipt_v1(&result, i)["driver_process_cpu_seconds"]
                    .as_f64()
                    .unwrap()
            })
            .collect();
        assert!(marks[0] > 0.0 && marks[1] >= marks[0], "{marks:?}");
        let mut plain = soft_q_run_config_v1("cpu-marks-off", SoftQArmV1::Treatment, 1);
        plain.soft_q = None;
        let result = run_native_expanded_training_v1(&plain, None).unwrap();
        assert!(iteration_receipt_v1(&result, 0)
            .get("driver_process_cpu_seconds")
            .is_none());
    }

    /// Design section 6.2: with the whole teacher run but no target applied,
    /// an SQ1 update equals the ordinary update bit for bit.
    #[test]
    fn soft_q_zero_treatment_equals_the_ordinary_update() {
        let mut zero = soft_q_run_config_v1("sq-run-zero", SoftQArmV1::Treatment, 2);
        zero.soft_q.as_mut().unwrap().engineering_zero_treatment = true;
        let mut ordinary = soft_q_run_config_v1("sq-run-ordinary", SoftQArmV1::Treatment, 2);
        ordinary.soft_q = None;
        let z = run_native_expanded_training_v1(&zero, Some(1)).unwrap();
        let o = run_native_expanded_training_v1(&ordinary, Some(1)).unwrap();
        let state = |r: &Value| {
            let d = read_pin(
                &serde_json::from_value(iteration_receipt_v1(r, 0)["update"].clone()).unwrap(),
            )
            .unwrap();
            (d["after_state_sha256"].clone(), d["adam_step"].clone())
        };
        assert_eq!(state(&z), state(&o));
        let d = read_pin(
            &serde_json::from_value(iteration_receipt_v1(&z, 0)["update"].clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(d["soft_q"]["available"], 1, "the teacher ran");
        assert!(
            d["soft_q"]["gradient_exposure"].is_null(),
            "no target applied"
        );
    }

    #[test]
    fn soft_q_run_binds_packets_resumes_replays_and_diagnoses() {
        let config = soft_q_run_config_v1("sq-run-a", SoftQArmV1::Treatment, 3);
        config.validate_v1().unwrap();
        let first = run_native_expanded_training_v1(&config, Some(1)).unwrap();
        assert_eq!(first["completed_iterations"], 1);
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["complete"], true);
        let receipt = iteration_receipt_v1(&result, 0);
        assert_eq!(receipt["soft_q"]["scheduled"], 1);
        assert_eq!(receipt["soft_q"]["available"], 1);
        let update: PinnedFileV1 = serde_json::from_value(receipt["update"].clone()).unwrap();
        let document = read_pin(&update).unwrap();
        let checkpoint = parse_pin(&document, "checkpoint").unwrap();
        let checkpoint = read_pin_bounded(&checkpoint, ARTIFACT_CAP).unwrap();
        assert_eq!(checkpoint["soft_q"]["continuation"]["arm"], "treatment");
        let packet = read_pin_bounded(
            &parse_pin(&document["soft_q"], "packet").unwrap(),
            ARTIFACT_CAP,
        )
        .unwrap();
        let slot = &packet["slots"][0];
        assert_eq!(slot["status"], "available");
        assert_eq!(slot["ordinals"].as_array().unwrap().len(), 16);
        assert!(
            document["soft_q"]["gradient_exposure"]["auxiliary_loss"]
                .as_f64()
                .unwrap()
                >= 0.0
        );
        let movement = document["soft_q"]["movement"].as_array().unwrap();
        assert_eq!(movement.len(), 1);
        assert!(movement[0]["deployed_tv"].as_f64().unwrap() >= 0.0);
        // A complete rerun only verifies; a fresh directory replays identically,
        // and the teacher's worker count changes no record.
        assert_eq!(
            run_native_expanded_training_v1(&config, None).unwrap(),
            result
        );
        let serial = soft_q_run_config_v1("sq-run-serial", SoftQArmV1::Treatment, 1);
        let again = run_native_expanded_training_v1(&serial, None).unwrap();
        assert_eq!(
            again["actual_identity"]["state_sha256"],
            result["actual_identity"]["state_sha256"]
        );
        let again_document = read_pin(
            &serde_json::from_value(iteration_receipt_v1(&again, 0)["update"].clone()).unwrap(),
        )
        .unwrap();
        let again_packet = read_pin_bounded(
            &parse_pin(&again_document["soft_q"], "packet").unwrap(),
            ARTIFACT_CAP,
        )
        .unwrap();
        // Every slot of every update is the same apart from execution metadata
        // (teacher_workers sits in the options) and the label-dependent
        // opponent identity of these synthetic sources.
        for iteration in 0..2 {
            let packet_of = |r: &Value| {
                let d = read_pin(
                    &serde_json::from_value(iteration_receipt_v1(r, iteration)["update"].clone())
                        .unwrap(),
                )
                .unwrap();
                let mut p =
                    read_pin_bounded(&parse_pin(&d["soft_q"], "packet").unwrap(), ARTIFACT_CAP)
                        .unwrap();
                for slot in p["slots"].as_array_mut().unwrap() {
                    slot.as_object_mut().unwrap().remove("opponent_identity");
                }
                p["slots"].clone()
            };
            assert_eq!(
                packet_of(&again),
                packet_of(&result),
                "iteration {iteration}"
            );
        }
        assert!(again_packet["slots"].is_array());
        // The arm is bound for resume.
        let mut changed = config.clone();
        changed.soft_q.as_mut().unwrap().arm = SoftQArmV1::ShuffledQ;
        let error = run_native_expanded_training_v1(&changed, None).unwrap_err();
        assert!(error.contains("resume plan"), "{error}");
        // The shuffled arm shares the treatment's worlds: same census, and its
        // target is built from the permuted means.
        let shuffled = soft_q_run_config_v1("sq-run-shuffled", SoftQArmV1::ShuffledQ, 3);
        let s = run_native_expanded_training_v1(&shuffled, Some(1)).unwrap();
        let s_document = read_pin(
            &serde_json::from_value(iteration_receipt_v1(&s, 0)["update"].clone()).unwrap(),
        )
        .unwrap();
        let s_packet = read_pin_bounded(
            &parse_pin(&s_document["soft_q"], "packet").unwrap(),
            ARTIFACT_CAP,
        )
        .unwrap();
        assert_eq!(s_packet["slots"][0]["ordinals"], slot["ordinals"]);
        assert_eq!(s_packet["slots"][0]["mean_returns"], slot["mean_returns"]);
        assert!(s_packet["slots"][0]["permutation"].is_array());
        // Diagnostic: construction equals the run's teaching census at the
        // same case; assessment uses independent worlds.
        let collection = read_pin(&parse_pin(&receipt, "collection").unwrap()).unwrap();
        let trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collection["trajectories"].clone()).unwrap();
        let output = config.output_directory.with_file_name("diagnostic");
        let diagnostic = crate::expanded_deck_training_v1::execute_v1(
            ExpandedTrainingCommandV1::SoftQDiagnostic {
                parent: config.initial_source.clone(),
                prior_deck_indices: (0..9).collect(),
                teacher_workers: 2,
                cases: vec![
                    crate::expanded_deck_training_v1::soft_q_teacher_v1::SoftQDiagnosticCaseV1 {
                        case: "sq1-test/r0/s0/u000000/slot0".into(),
                        trajectory: trajectories[0].clone(),
                    },
                ],
                output_directory: output,
            },
        )
        .unwrap();
        let case = &diagnostic["cases"][0];
        assert_eq!(case["construction"], slot["ordinals"]);
        assert_eq!(case["assessment"].as_array().unwrap().len(), 16);
        assert_ne!(case["assessment"], case["construction"]);
        assert_eq!(case["target"], slot["target"]);
    }

    #[test]
    fn e_supported_run_rejects_incompatible_execution_and_schedules() {
        let base = e_supported_run_config_v1("e-run-validation");
        base.validate_v1().unwrap();
        let mut prepared = base.clone();
        prepared.preparation_workers = 2;
        assert!(prepared.validate_v1().is_err());
        let mut short = base.clone();
        short.e_supported.as_mut().unwrap().slots.pop();
        assert!(short.validate_v1().is_err());
        let mut outside = base.clone();
        outside.e_supported.as_mut().unwrap().slots[1] = vec![ESupportedSlotV1 {
            trajectory_index: 1,
            case_id: "outside".into(),
        }];
        assert!(outside.validate_v1().is_err());
        let mut repeated = base.clone();
        let slot = repeated.e_supported.as_ref().unwrap().slots[0][0].clone();
        repeated.e_supported.as_mut().unwrap().slots[1] = vec![slot];
        assert!(repeated.validate_v1().is_err());
        let mut legacy_loss = base.clone();
        legacy_loss.loss_selection = ExpandedLossSelectionV1::TerminalReinforceValueV3;
        assert!(legacy_loss.validate_v1().is_err());
        // Absent E keeps the existing wire bytes.
        let mut ordinary = base;
        ordinary.e_supported = None;
        assert!(value(&ordinary).unwrap().get("e_supported").is_none());
    }

    /// `real_two_iteration_config_v1` with two games per iteration, collected
    /// and prepared on two workers, so the parallel collection, its idle-worker
    /// parse-ahead and the parallel update preparation all run.
    fn parallel_two_iteration_config_v1(label: &str) -> NativeExpandedTrainingRunV1 {
        let mut config = real_two_iteration_config_v1(
            label,
            ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            ExpandedUpdateBackendV1::Cpu,
        );
        for (iteration, scheduled) in config.iterations.iter_mut().enumerate() {
            let mut second = scheduled.episodes[0].clone();
            second.episode.id = format!("{label}-iter-{iteration}-second");
            second.episode.seed += 1_000;
            second.episode.starting_player = 1;
            scheduled.episodes.push(second);
        }
        config.collection_workers = 2;
        config.preparation_workers = 2;
        config
    }

    /// B5 through the run driver: a run under `portable-math-v1` with
    /// parallel collection and preparation trains the same bits on two
    /// workers as on one, and the same bits on every system (one pin, which
    /// CI checks on Windows and Linux). Its plan and each iteration's
    /// collection, trajectories, update and checkpoint name the math; the
    /// same run under the default math names none and trains other bits.
    #[test]
    fn portable_math_run_trains_one_state_on_every_system_and_worker_count() {
        let mut config = parallel_two_iteration_config_v1("portable-math");
        config.training_math = TrainingMathV1::PortableV1;
        config.validate_v1().unwrap();
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["complete"], true);
        assert_eq!(result["actual_identity"]["adam_step"], 2);
        let manifest: Value =
            read_json(&config.output_directory.join("run.json"), SMALL_CAP).unwrap();
        assert_eq!(manifest["training_math"], "portable-math-v1");
        assert_eq!(manifest["config"]["training_math"], "portable-math-v1");
        for index in 0..2 {
            let receipt = iteration_receipt_v1(&result, index);
            let collection = read_pin(&parse_pin(&receipt, "collection").unwrap()).unwrap();
            assert_eq!(collection["training_math"], "portable-math-v1");
            let trajectories: Vec<PinnedFileV1> =
                serde_json::from_value(collection["trajectories"].clone()).unwrap();
            assert_eq!(trajectories.len(), 2);
            for trajectory in &trajectories {
                let document = read_pin_bounded(trajectory, ARTIFACT_CAP).unwrap();
                assert_eq!(document["training_math"], "portable-math-v1");
            }
            let update = read_pin(&parse_pin(&receipt, "update").unwrap()).unwrap();
            assert_eq!(update["training_math"], "portable-math-v1");
            let checkpoint = parse_pin(&update, "checkpoint").unwrap();
            let checkpoint = read_pin_bounded(&checkpoint, ARTIFACT_CAP).unwrap();
            assert_eq!(checkpoint["training_math"], "portable-math-v1");
        }
        let mut serial = config.clone();
        serial.collection_workers = 1;
        serial.preparation_workers = 1;
        serial.output_directory = config.output_directory.with_file_name("run-serial");
        let serial = run_native_expanded_training_v1(&serial, None).unwrap();
        assert_eq!(
            serial["actual_identity"]["state_sha256"],
            result["actual_identity"]["state_sha256"]
        );
        let mut platform = config.clone();
        platform.training_math = TrainingMathV1::PlatformLibm;
        platform.output_directory = config.output_directory.with_file_name("run-platform");
        let platform_result = run_native_expanded_training_v1(&platform, None).unwrap();
        assert_ne!(
            platform_result["actual_identity"]["state_sha256"],
            result["actual_identity"]["state_sha256"]
        );
        let manifest = fs::read_to_string(platform.output_directory.join("run.json")).unwrap();
        assert!(!manifest.contains("training_math"), "{manifest}");
        assert_eq!(
            result["actual_identity"]["state_sha256"],
            PORTABLE_PARALLEL_RUN_STATE_SHA256_V1
        );
    }

    /// The portable math covers the CPU update without the recovery loss,
    /// line (b) or E; a config that asks for more is refused before any work.
    #[test]
    fn portable_math_run_refuses_paths_it_does_not_cover() {
        let mut base = real_two_iteration_config_v1(
            "portable-math-refusals",
            ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            ExpandedUpdateBackendV1::Cpu,
        );
        base.training_math = TrainingMathV1::PortableV1;
        base.validate_v1().unwrap();
        let refused = |config: NativeExpandedTrainingRunV1| {
            let error = config.validate_v1().unwrap_err();
            assert!(error.contains("portable training math"), "{error}");
        };
        let mut cuda = base.clone();
        cuda.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 0 };
        refused(cuda);
        let mut recovery = base.clone();
        recovery.loss_selection = ExpandedLossSelectionV1::GaeUniformKlRecoveryV1 {
            gamma: 1.0,
            lambda: 0.9,
            initial_adam_step: 32400,
            recovery: true,
            diagnostics: false,
        };
        refused(recovery);
        let mut e_supported = e_supported_run_config_v1("portable-math-refusals-e");
        e_supported.training_math = TrainingMathV1::PortableV1;
        refused(e_supported);
    }

    /// Every regular file under `directory`, by its path relative to it.
    fn files_under_v1(directory: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut files = std::collections::BTreeMap::new();
        let mut pending = vec![directory.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in fs::read_dir(&next).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else {
                    let relative = entry.path().strip_prefix(directory).unwrap().to_path_buf();
                    files.insert(relative, fs::read(entry.path()).unwrap());
                }
            }
        }
        files
    }

    /// Stage 1's crash-state check (collab actor-learner design v1, section
    /// 2.6): a crash just before or just after any publication of a
    /// two-iteration run with parallel collection and preparation leaves only
    /// files the uninterrupted run publishes, with the same bytes apart from
    /// timing telemetry, plus staging debris; and resuming the crashed run
    /// reaches the uninterrupted run's final train state. Every case runs in
    /// the same output directory, so documents that name it compare equal.
    #[test]
    #[ignore = "slow: one crashed run and its resume per crash point; run before shipping a stage 1 change"]
    fn a_crash_at_any_publication_resumes_to_the_uninterrupted_result() {
        use crate::durable_publication_v1::crash_points_for_test_v1::{
            with_crash_point_v1, CrashPointV1,
        };
        let config = parallel_two_iteration_config_v1("crash-points");
        config.validate_v1().unwrap();
        let directory = config.output_directory.clone();
        let reference_directory = directory.with_file_name("uninterrupted");
        let _ = fs::remove_dir_all(&directory);
        let _ = fs::remove_dir_all(&reference_directory);
        let (reference, publications, crashed) = with_crash_point_v1(&directory, None, || {
            run_native_expanded_training_v1(&config, None)
        });
        let reference = reference.unwrap();
        assert!(!crashed);
        assert_eq!(reference["complete"], true);
        assert_eq!(reference["actual_identity"]["adam_step"], 2);
        fs::rename(&directory, &reference_directory).unwrap();
        let reference_files = files_under_v1(&reference_directory);
        // Each iteration publishes its two commands, two trajectories, the
        // collection, the update input, the checkpoint, the update and its
        // receipt; the run adds its plan and completion.
        assert!(publications >= 20, "{publications} publications");
        // Documents that carry timing telemetry, or the digest of one.
        let timed = [
            "collection.json",
            "update.json",
            "update-input.json",
            "complete.json",
            "completion.json",
        ];
        for index in 0..publications {
            for point in [CrashPointV1::BeforeFinal, CrashPointV1::AfterFinal] {
                let case = format!("publication {index} {point:?}");
                let (crashed_run, _, crashed) =
                    with_crash_point_v1(&directory, Some((index, point)), || {
                        run_native_expanded_training_v1(&config, None)
                    });
                assert!(crashed, "{case}: never reached");
                let error = crashed_run.unwrap_err();
                assert!(error.contains("injected crash"), "{case}: {error}");
                for (path, bytes) in files_under_v1(&directory) {
                    let name = path.file_name().unwrap().to_str().unwrap();
                    if name.starts_with('.') && name.contains(".stage") {
                        continue;
                    }
                    let expected = reference_files
                        .get(&path)
                        .unwrap_or_else(|| panic!("{case}: unexpected file {}", path.display()));
                    if name != "run.lock" && !timed.contains(&name) {
                        assert!(
                            bytes == *expected,
                            "{case}: {} differs from the uninterrupted run",
                            path.display()
                        );
                    }
                }
                let resumed = run_native_expanded_training_v1(&config, None)
                    .unwrap_or_else(|error| panic!("{case}: resume failed: {error}"));
                assert_eq!(resumed["complete"], true, "{case}");
                for field in ["state_sha256", "adam_step"] {
                    assert_eq!(
                        resumed["actual_identity"][field], reference["actual_identity"][field],
                        "{case}: {field}"
                    );
                }
                fs::remove_dir_all(&directory).unwrap();
            }
        }
        fs::remove_dir_all(&reference_directory).unwrap();
    }

    #[test]
    #[ignore = "requires a real GPU; explicit GPU execution only"]
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    fn loss_selection_reaches_a_real_two_iteration_cuda_run_receipt() {
        let config = real_two_iteration_config_v1(
            "cuda",
            ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            ExpandedUpdateBackendV1::Cuda { device_ordinal: 0 },
        );
        config.validate_v1().unwrap();
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["loss_identity"], "gae_advantage_value/v1");
        assert_eq!(
            last_update_loss_identity_v1(&result),
            "gae_advantage_value/v1"
        );
    }
}
