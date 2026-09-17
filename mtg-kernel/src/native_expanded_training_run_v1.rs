//! Resumable training with CPU V3 collection and an explicit update backend.
//! This successor owns its checkpoints; it does not reinterpret legacy Stores.

use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, publish_new_file_v1, DurableFileExpectationV1,
};
use crate::expanded_deck_training_v1::{
    execute_v1, load_expanded_inference_v1, ExpandedEpisodeV1, ExpandedInferenceIdentityV1,
    ExpandedLossSelectionV1, ExpandedModelSourceV1, ExpandedTrainingCommandV1,
    ExpandedUpdateBackendV1, PinnedFileV1, UpdateBackwardExecutionV1,
    DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
};
use crate::native_flat_tensorizer_v3::{FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3};
use crate::sideboard::RegisteredDeckV1;
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
        self.update_backend.validate_v1()?;
        self.loss_selection.validate_v1()?;
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
    verify_pin(pin)?;
    read_json(&pin.path, SMALL_CAP)
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
    publish_new_file_v1(
        &parent,
        format!(".{name}.stage-{}-{nonce}", std::process::id()),
        name,
        &bytes,
        DurableFileExpectationV1::from_bytes(&bytes).map_err(err)?,
    )
    .map_err(err)?;
    Ok(PinnedFileV1 {
        path: directory.join(name),
        sha256: digest(&bytes),
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

fn validate_collection(
    collection: &PinnedFileV1,
    source: &ExpandedModelSourceV1,
    episodes: &[ExpandedEpisodeV1],
    before: &ExpandedInferenceIdentityV1,
) -> Result<Vec<PinnedFileV1>, String> {
    let document = read_pin(collection)?;
    check(
        document["complete"] == true
            && document["source"] == value(source)?
            && document["behavior_state_sha256"] == before.state_sha256,
        "collection source/state does not match iteration",
    )?;
    validate_collection_episodes(&document, episodes)
}

/// The schedule half of `validate_collection`: every published trajectory
/// must carry exactly its scheduled episode. Under tolerant collection
/// (`max_non_natural_episode_fraction > 0.0`) a slot whose ledgered attempts
/// all failed legitimately published its trajectory from the derived retry
/// seed, so for that slot alone the scheduled seed is replaced by the seed
/// the ledger re-derives (`ledgered_retry_seed_overrides_v1`, which first
/// re-verifies the whole ledger against the schedule); every other episode
/// field, and every unledgered slot, must still match the schedule exactly.
fn validate_collection_episodes(
    document: &Value,
    episodes: &[ExpandedEpisodeV1],
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
    let saved: Vec<Result<Value, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = trajectories
            .iter()
            .map(|trajectory| {
                scope.spawn(move || -> Result<Value, String> {
                    verify_pin(trajectory)?;
                    read_json(&trajectory.path, ARTIFACT_CAP)
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
            saved["episode"] == value(&expected)?,
            "collection episode assignment differs",
        )?;
    }
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
) -> Result<(ExpandedModelSourceV1, ExpandedInferenceIdentityV1), String> {
    let document = read_pin(update)?;
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
    let (_, after) = load_expanded_inference_v1(&next)?;
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
    let saved = read_json(&checkpoint.path, ARTIFACT_CAP)?;
    check(
        saved["trajectories"] == value(&trajectories)?
            && saved["learning_rate_bits"] == learning_rate.to_bits()
            && saved["value_coefficient_bits"] == value_coefficient.to_bits(),
        "checkpoint inputs or optimizer settings differ",
    )?;
    Ok((next, after))
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
            output_directory,
        }
    } else {
        ExpandedTrainingCommandV1::CollectParallel {
            source: source.clone(),
            episodes: episodes.to_vec(),
            workers: config.collection_workers,
            max_non_natural_episode_fraction: config.max_non_natural_episode_fraction,
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
    source: &ExpandedModelSourceV1,
    trajectories: Vec<PinnedFileV1>,
    output_directory: PathBuf,
) -> ExpandedTrainingCommandV1 {
    if config.preparation_workers == 1 {
        ExpandedTrainingCommandV1::Update {
            source: source.clone(),
            trajectories,
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            update_backend: config.update_backend,
            update_backward_execution: config.update_backward_execution,
            loss_selection: config.loss_selection,
            output_directory,
        }
    } else {
        ExpandedTrainingCommandV1::UpdatePrepared {
            source: source.clone(),
            trajectories,
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            update_backend: config.update_backend,
            update_backward_execution: config.update_backward_execution,
            loss_selection: config.loss_selection,
            preparation_workers: config.preparation_workers,
            max_prepared_tensor_mebibytes: config.max_prepared_tensor_mebibytes,
            output_directory,
        }
    }
}

/// Runs or resumes a declared schedule. The optional invocation limit pauses
/// only at completed iteration boundaries; it never changes the saved plan.
pub fn run_native_expanded_training_v1(
    config: &NativeExpandedTrainingRunV1,
    max_new_iterations: Option<usize>,
) -> Result<Value, String> {
    config.validate_v1()?;
    config.update_backend.require_compiled_v1()?;
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
    let mut manifest = json!({"schema":SCHEMA, "config":config,
        "git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
        "feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3,
        "device":"cpu", "gpu_ordinal":null,
        "implementation_sha256":digest(include_bytes!("native_expanded_training_run_v1.rs"))});
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
    let (_, mut current_identity) = load_expanded_inference_v1(&config.initial_source)?;
    // Validate the complete fixed roster before spending episode compute.
    for opponent in &config.opponents {
        load_expanded_inference_v1(&opponent.source)?;
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
        let episodes = resolve_episodes(config, index, &current, &history)?;
        let episode_digest = identity(&episodes)?;
        if complete_path.exists() {
            let receipt = read_json(&complete_path, SMALL_CAP)?;
            check(
                receipt["iteration"] == index
                    && receipt["source"] == value(&current)?
                    && receipt["episodes_sha256"] == episode_digest,
                "completed iteration schedule differs",
            )?;
            let collection = parse_pin(&receipt, "collection")?;
            let trajectories =
                validate_collection(&collection, &current, &episodes, &current_identity)?;
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
                validate_collection(&candidate, &current, &episodes, &current_identity)?;
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
            let trajectories =
                validate_collection(collection, &current, &episodes, &current_identity)?;
            collection_validation_seconds += started.elapsed().as_secs_f64();
            publish(
                &attempt,
                "update-input.json",
                &json!({"collection":collection}),
            )?;
            let update_command =
                update_command(config, &current, trajectories, attempt.join("update"));
            publish(&attempt, "update-command.json", &update_command)?;
            let started = Instant::now();
            execute_v1(update_command)?;
            update_execution_seconds += started.elapsed().as_secs_f64();
            update_pin = Some(pin(&attempt.join("update/update.json"))?);
        }
        let collection = collection_pin.unwrap();
        let update = update_pin.unwrap();
        let started = Instant::now();
        let trajectories =
            validate_collection(&collection, &current, &episodes, &current_identity)?;
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
        )?;
        update_validation_seconds += started.elapsed().as_secs_f64();
        let mut receipt = json!({"schema":"mtg-kernel-native-expanded-iteration/v1", "iteration":index,
            "source":current,"episodes_sha256":episode_digest,"collection":collection,"update":update,
            "output_identity":after});
        if let Some(ledger_pin) = collection_non_natural_ledger_pin(&collection)? {
            receipt["non_natural_ledger"] = value(&ledger_pin)?;
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
        assert!(out_of_range.validate_v1().unwrap_err().contains("64..=4096"));
        let mut manifest = json!({});
        record_collection_execution(&raised, &mut manifest);
        assert_eq!(manifest["max_prepared_tensor_mebibytes"], json!(1024));
        let mut default_manifest = json!({});
        record_collection_execution(&schedule(), &mut default_manifest);
        assert!(default_manifest
            .get("max_prepared_tensor_mebibytes")
            .is_none());
        match update_command(&raised, &raised.initial_source, Vec::new(), PathBuf::new()) {
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
        assert_eq!(gae_manifest["loss_selection"]["kind"], "gae_advantage_value_v1");
        assert_eq!(gae_manifest["loss_selection"]["gamma"], 1.0);
        assert_eq!(gae_manifest["loss_selection"]["lambda"], 0.9_f32 as f64);
        assert_eq!(gae_manifest["loss_selection"]["entropy_coefficient"], 0.0);

        // The forwarded selection reaches the actual Update command.
        let legacy_command = update_command(
            &legacy,
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
                        registered: decks.clone(),
                        selected: decks.clone(),
                        postboard: false,
                        max_physical_decisions: 100_000,
                        max_policy_steps: 1_000_000,
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
        let manifest: Value = read_json(&config.output_directory.join("run.json"), SMALL_CAP)
            .unwrap();
        assert_eq!(manifest["loss_selection"]["kind"], "gae_advantage_value_v1");
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
