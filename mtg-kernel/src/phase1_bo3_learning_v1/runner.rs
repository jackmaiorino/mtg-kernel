//! Finite synchronous BO3 match-return batches. No BO1 dispatch is changed.
use super::continuation::{
    ValidatedBo3TipV1, recorded_bo3_tip_v1, validate_bo3_tip_v1, validate_recorded_bo3_result_v1,
};
use super::preparation::{physical_match_identity, strict_json};
use super::*;
use crate::durable_publication_v1::{
    DurableFileExpectationV1, capture_existing_publication_parent_v1, publish_new_file_v1,
    verify_existing_publication_v1,
};
use crate::expanded_deck_training_v1::{ExpandedSeatBehaviorV1, PinnedFileV1};
use crate::phase1_agent_v1::CompleteAgentPackageV1;
use crate::phase1_bo3_collection_v1::Bo3CollectionConfigV1;
use crate::rl::PlayerSeatV1;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const NATIVE_BO3_RUN_SCHEMA_V1: &str = "mtg-kernel-native-bo3-training-run/v1";
pub const MAX_NATIVE_BO3_RUN_REQUEST_BYTES_V1: usize = 16 * 1024 * 1024;
const SMALL: u64 = 16 * 1024 * 1024;
const INPUT: u64 = 512 * 1024 * 1024;
const LEDGER: usize = 65_536;
const STACK: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedBo3OpponentV1 {
    pub id: String,
    pub package: CompleteAgentPackageV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Bo3RunOpponentV1 {
    Fixed { id: String },
    Initial,
    Current,
    CompletedBatch { index: usize },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledBo3MatchV1 {
    pub config: Bo3CollectionConfigV1,
    pub learner_seat: PlayerSeatV1,
    pub opponent: Bo3RunOpponentV1,
    pub capture_limits: Bo3NativeCaptureLimitsV1,
    /// Serialized file limit, separate from game/native record capture limits.
    /// An overflow is an execution error, never a replacement match or target.
    pub max_result_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBo3RunBatchV1 {
    pub matches: Vec<ScheduledBo3MatchV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBo3TrainingRunV1 {
    pub schema: String,
    pub initial_learner: CompleteAgentPackageV1,
    pub initial_input: Bo3GameplayUpdateInputV1,
    pub previous_progress: Option<PinnedFileV1>,
    pub learning_rate_bits: u32,
    pub value_coefficient_bits: u32,
    pub opponents: Vec<NamedBo3OpponentV1>,
    pub batches: Vec<NativeBo3RunBatchV1>,
    pub preparation_limits: Bo3PreparationLimitsV1,
    pub collection_workers: usize,
    pub output_directory: PathBuf,
}
impl NativeBo3TrainingRunV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        require(
            text.len() <= MAX_NATIVE_BO3_RUN_REQUEST_BYTES_V1,
            "BO3 run exceeds 16 MiB",
        )?;
        let run: Self = strict_json(text.as_bytes())?;
        run.validate_v1()?;
        Ok(run)
    }
    pub fn validate_v1(&self) -> Result<(), String> {
        require(
            self.schema == NATIVE_BO3_RUN_SCHEMA_V1 && self.output_directory.is_absolute(),
            "BO3 run schema or absolute output differs",
        )?;
        require(
            self.initial_learner.gameplay == *self.initial_input.learner_v1(),
            "initial package differs from explicit optimizer input",
        )?;
        require(
            self.previous_progress.is_some()
                || matches!(
                    self.initial_input,
                    Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition { .. }
                ),
            "initial BO3 source requires latest attempted progress",
        )?;
        require(
            (1..=32).contains(&self.collection_workers)
                && (1..=4096).contains(&self.batches.len())
                && self.opponents.len() <= 128,
            "BO3 run dimensions exceed bounds",
        )?;
        require(
            (1..=INPUT).contains(&self.preparation_limits.max_input_bytes)
                && (1..=256 * 1024 * 1024)
                    .contains(&self.preparation_limits.max_prepared_payload_bytes),
            "BO3 preparation bounds differ",
        )?;
        for bits in [self.learning_rate_bits, self.value_coefficient_bits] {
            require(
                f32::from_bits(bits).is_finite() && f32::from_bits(bits) > 0.0,
                "BO3 optimizer scalar must be positive finite",
            )?;
        }
        self.initial_learner.validate_metadata_v1()?;
        let mut opponents = BTreeSet::new();
        for item in &self.opponents {
            require(
                !item.id.is_empty() && item.id.len() <= 128 && opponents.insert(item.id.as_str()),
                "duplicate or invalid opponent id",
            )?;
            item.package.validate_metadata_v1()?;
        }
        let mut ids = BTreeSet::new();
        let mut total = 0usize;
        for (index, batch) in self.batches.iter().enumerate() {
            require(
                (1..=32).contains(&batch.matches.len()),
                "BO3 batch needs 1..32 attempted matches",
            )?;
            total = total
                .checked_add(batch.matches.len())
                .ok_or("schedule count overflow")?;
            let mut result_bytes = 0u64;
            for item in &batch.matches {
                require(
                    ids.insert(item.config.match_id.as_str()),
                    "duplicate scheduled match id",
                )?;
                require(
                    (1..=INPUT).contains(&item.max_result_bytes),
                    "BO3 serialized result bound differs",
                )?;
                result_bytes = result_bytes
                    .checked_add(item.max_result_bytes)
                    .ok_or("declared results size overflow")?;
                item.capture_limits.validate()?;
                match &item.opponent {
                    Bo3RunOpponentV1::Fixed { id } => {
                        require(opponents.contains(id.as_str()), "unknown fixed opponent")?
                    }
                    Bo3RunOpponentV1::CompletedBatch { index: before } => {
                        require(*before < index, "future historical opponent")?
                    }
                    _ => {}
                }
                // Configuration legality is independent of future learned weights.
                crate::phase1_bo3_collection_v1::validate_configuration(
                    &item.config,
                    [&self.initial_learner, &self.initial_learner],
                )?;
            }
            // Even future model path sizes cannot make an already excessive
            // result-only declaration valid. Exact resolved request sizes are
            // added again before that batch's collection.
            require(
                result_bytes < self.preparation_limits.max_input_bytes,
                "scheduled result caps already exhaust a batch input bound",
            )?;
        }
        require(total <= LEDGER, "BO3 schedule exceeds attempt ledger")?;
        super::canonical_size(self, SMALL)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBo3RunResultV1 {
    pub schema: String,
    pub complete: bool,
    pub completed_batches: usize,
    pub planned_batches: usize,
    pub learner: ExpandedSeatBehaviorV1,
    pub previous_progress: Option<PinnedFileV1>,
    pub completed_bo3_updates: u64,
    pub attempted_batches: u64,
    pub attempted_matches: usize,
    pub batch_receipts: Vec<PinnedFileV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunManifest {
    schema: String,
    config: NativeBo3TrainingRunV1,
    git_commit: String,
    tracked_tree_sha256: String,
    implementation_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotIntent {
    schema: String,
    request: PinnedFileV1,
    result: PinnedFileV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchReceipt {
    schema: String,
    batch: usize,
    manifest: PinnedFileV1,
    input: Bo3GameplayUpdateInputV1,
    previous_progress: Option<PinnedFileV1>,
    attempts: Vec<Bo3AttemptInputV1>,
    preparation_request: PinnedFileV1,
    update_request: PinnedFileV1,
    result: Bo3GameplayUpdateResultV1,
}
#[derive(Clone)]
struct ResolvedMatch {
    request: TrainableBo3RequestV1,
    learner_seat: PlayerSeatV1,
    max_result_bytes: u64,
    physical_key: String,
}
#[derive(Serialize)]
struct WorkerTiming {
    worker: usize,
    assigned_slots: usize,
    completed_slots: usize,
    failed_slots: usize,
    busy_seconds: f64,
}
struct PoolReport<T> {
    outputs: Vec<(usize, T)>,
    timings: Vec<WorkerTiming>,
    failure: Option<String>,
    started_workers: usize,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn pin_bytes(path: PathBuf, bytes: &[u8]) -> PinnedFileV1 {
    PinnedFileV1 {
        path,
        sha256: digest(bytes),
    }
}
fn bounded_json<T: Serialize>(value: &T, maximum: u64) -> Result<Vec<u8>, String> {
    struct Buffer {
        bytes: Vec<u8>,
        maximum: u64,
    }
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let length = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .filter(|n| *n as u64 <= self.maximum)
                .ok_or_else(|| std::io::Error::other("BO3 run artifact byte bound exceeded"))?;
            self.bytes
                .try_reserve(length - self.bytes.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut buffer, value).map_err(err)?;
    Ok(buffer.bytes)
}
fn read_json<T: DeserializeOwned + Serialize>(path: &Path, maximum: u64) -> Result<T, String> {
    let file = fs::File::open(path).map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    require(
        meta.is_file() && meta.len() <= maximum,
        "run artifact is not a bounded file",
    )?;
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    require(
        bytes.len() as u64 <= maximum,
        "run artifact grew past bound",
    )?;
    strict_json(&bytes)
}
fn publish_bytes(directory: &Path, name: &str, bytes: &[u8]) -> Result<PinnedFileV1, String> {
    let parent = capture_existing_publication_parent_v1(directory).map_err(err)?;
    let expected = DurableFileExpectationV1::from_bytes(bytes).map_err(err)?;
    if directory.join(name).try_exists().map_err(err)? {
        verify_existing_publication_v1(&parent, name, expected).map_err(err)?;
    } else {
        // Never overwrite an orphan stage. A fresh stage can finish an absent final.
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(err)?
            .as_nanos();
        let stage = format!(".{name}.stage-{}-{nonce}", std::process::id());
        publish_new_file_v1(&parent, stage, name, bytes, expected).map_err(err)?;
    }
    Ok(pin_bytes(directory.join(name), bytes))
}
fn publish<T: Serialize>(
    directory: &Path,
    name: &str,
    value: &T,
    maximum: u64,
) -> Result<PinnedFileV1, String> {
    publish_bytes(directory, name, &bounded_json(value, maximum)?)
}
#[derive(Default)]
struct VerifiedFiles(BTreeMap<PathBuf, (String, u64)>);
impl VerifiedFiles {
    /// Stream payloads once per invocation; never deserialize historical tensors.
    fn verify(&mut self, pin: &PinnedFileV1, maximum: u64) -> Result<(), String> {
        require(
            pin.path.is_absolute()
                && pin.sha256.len() == 64
                && pin
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "run pin needs absolute path and lowercase SHA256",
        )?;
        if let Some((sha, size)) = self.0.get(&pin.path) {
            return require(
                sha == &pin.sha256 && *size <= maximum,
                "conflicting cached artifact pin or bound",
            );
        }
        let mut file = fs::File::open(&pin.path).map_err(err)?;
        let meta = file.metadata().map_err(err)?;
        require(
            meta.is_file() && meta.len() <= maximum,
            "pinned run file exceeds bound",
        )?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let n = file.read(&mut buffer).map_err(err)?;
            if n == 0 {
                break;
            }
            size = size.checked_add(n as u64).ok_or("file size overflow")?;
            require(size <= maximum, "pinned run file grew past bound")?;
            hasher.update(&buffer[..n]);
        }
        require(
            format!("{:x}", hasher.finalize()) == pin.sha256,
            "pinned run file hash differs",
        )?;
        self.0.insert(pin.path.clone(), (pin.sha256.clone(), size));
        Ok(())
    }
    fn bytes(&mut self, pin: &PinnedFileV1, maximum: u64) -> Result<Vec<u8>, String> {
        require(pin.path.is_absolute(), "run pin path is relative")?;
        let file = fs::File::open(&pin.path).map_err(err)?;
        let meta = file.metadata().map_err(err)?;
        require(
            meta.is_file() && meta.len() <= maximum,
            "pinned run metadata exceeds bound",
        )?;
        let mut bytes = Vec::new();
        file.take(maximum + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        require(
            bytes.len() as u64 <= maximum,
            "pinned metadata grew past bound",
        )?;
        if let Some((sha, size)) = self.0.get(&pin.path) {
            require(
                sha == &pin.sha256 && *size == bytes.len() as u64,
                "conflicting cached metadata pin",
            )?;
        } else {
            require(digest(&bytes) == pin.sha256, "pinned metadata hash differs")?;
            self.0
                .insert(pin.path.clone(), (pin.sha256.clone(), bytes.len() as u64));
        }
        Ok(bytes)
    }
}

/// A mutex protects only dispatch, never policies/inference. Cancellation and
/// assignment share the same lock, so no new slot is assigned after cancellation.
/// Already active engines are joined; the outer process cap handles a hung engine.
fn ordered_jobs<T: Send, F: Fn(usize) -> Result<T, String> + Sync>(
    workers: usize,
    jobs: usize,
    work: F,
) -> Result<PoolReport<T>, String> {
    require(
        (1..=32).contains(&workers) && workers <= jobs && jobs <= 32,
        "invalid BO3 worker dimensions",
    )?;
    let dispatch = Mutex::new((0usize, false));
    thread::scope(|scope| {
        let mut handles = Vec::new();
        let mut failures = Vec::new();
        for worker in 0..workers {
            let dispatch = &dispatch;
            let work = &work;
            match thread::Builder::new()
                .name(format!("bo3-collector-{worker}"))
                .stack_size(STACK)
                .spawn_scoped(scope, move || {
                    let mut output = Vec::new();
                    let mut timing = WorkerTiming {
                        worker,
                        assigned_slots: 0,
                        completed_slots: 0,
                        failed_slots: 0,
                        busy_seconds: 0.0,
                    };
                    let mut failure = None;
                    loop {
                        let index = {
                            let mut state = dispatch.lock().unwrap_or_else(|e| e.into_inner());
                            if state.1 || state.0 == jobs {
                                break;
                            }
                            let index = state.0;
                            state.0 += 1;
                            index
                        };
                        timing.assigned_slots += 1;
                        let started = Instant::now();
                        let result = catch_unwind(AssertUnwindSafe(|| work(index)));
                        timing.busy_seconds += started.elapsed().as_secs_f64();
                        match result {
                            Ok(Ok(value)) => {
                                output.push((index, value));
                                timing.completed_slots += 1;
                            }
                            other => {
                                dispatch.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
                                timing.failed_slots += 1;
                                failure = Some((
                                    index,
                                    match other {
                                        Ok(Err(error)) => error,
                                        Err(_) => "collector panicked; see panic diagnostic".into(),
                                        _ => unreachable!(),
                                    },
                                ));
                                break;
                            }
                        }
                    }
                    (output, timing, failure)
                }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    dispatch.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
                    failures.push((
                        usize::MAX,
                        format!("could not spawn collector {worker}: {error}"),
                    ));
                    break;
                }
            }
        }
        let started_workers = handles.len();
        let mut outputs = Vec::new();
        let mut timings = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok((output, timing, failure)) => {
                    outputs.extend(output);
                    timings.push(timing);
                    if let Some(error) = failure {
                        failures.push(error);
                    }
                }
                Err(_) => {
                    failures.push((usize::MAX, "collector thread panicked outside job".into()))
                }
            }
        }
        outputs.sort_by_key(|(index, _)| *index);
        failures.sort_by_key(|(index, _)| *index);
        if failures.is_empty()
            && (outputs.len() != jobs || outputs.iter().enumerate().any(|(i, (j, _))| i != *j))
        {
            failures.push((
                usize::MAX,
                "collector result indices are incomplete or duplicated".into(),
            ));
        }
        Ok(PoolReport {
            outputs,
            timings,
            started_workers,
            failure: failures
                .into_iter()
                .next()
                .map(|(index, error)| format!("BO3 slot {index}: {error}")),
        })
    })
}

fn learner_package(
    config: &NativeBo3TrainingRunV1,
    learner: &ExpandedSeatBehaviorV1,
) -> CompleteAgentPackageV1 {
    let mut package = config.initial_learner.clone();
    package.gameplay = learner.clone();
    package
}
fn resolve_batch(
    config: &NativeBo3TrainingRunV1,
    index: usize,
    learner: &ExpandedSeatBehaviorV1,
    history: &[ExpandedSeatBehaviorV1],
) -> Result<Vec<ResolvedMatch>, String> {
    let current = learner_package(config, learner);
    let mut physical = BTreeSet::new();
    let mut declared_bytes = 0u64;
    let mut resolved = Vec::new();
    for item in &config.batches[index].matches {
        let opponent = match &item.opponent {
            Bo3RunOpponentV1::Fixed { id } => config
                .opponents
                .iter()
                .find(|p| &p.id == id)
                .ok_or("missing fixed opponent")?
                .package
                .clone(),
            Bo3RunOpponentV1::Initial => config.initial_learner.clone(),
            Bo3RunOpponentV1::Current => current.clone(),
            Bo3RunOpponentV1::CompletedBatch { index } => learner_package(
                config,
                history
                    .get(*index)
                    .ok_or("historical batch has not completed")?,
            ),
        };
        let packages = if item.learner_seat == PlayerSeatV1::P0 {
            [current.clone(), opponent]
        } else {
            [opponent, current.clone()]
        };
        let request = TrainableBo3RequestV1 {
            schema: TRAINABLE_BO3_REQUEST_SCHEMA_V1.into(),
            config: item.config.clone(),
            packages,
            capture_limits: item.capture_limits.clone(),
        };
        request.validate()?;
        let request_bytes = super::canonical_size(&request, 4 * 1024 * 1024)?;
        declared_bytes = declared_bytes
            .checked_add(request_bytes)
            .and_then(|n| n.checked_add(item.max_result_bytes))
            .ok_or("declared batch size overflow")?;
        let physical_key = physical_match_identity(&request.config, request.packages.each_ref())?;
        require(
            physical.insert(physical_key.clone()),
            "duplicate physical match in resolved batch",
        )?;
        resolved.push(ResolvedMatch {
            request,
            learner_seat: item.learner_seat,
            max_result_bytes: item.max_result_bytes,
            physical_key,
        });
    }
    require(
        declared_bytes <= config.preparation_limits.max_input_bytes,
        "declared serialized batch inputs exceed preparation bound",
    )?;
    Ok(resolved)
}
fn next_input(result: &Bo3GameplayUpdateResultV1) -> Bo3GameplayUpdateInputV1 {
    if result.completed_bo3_updates == 0 {
        Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition {
            learner: result.learner.clone(),
        }
    } else {
        Bo3GameplayUpdateInputV1::Bo3Checkpoint {
            learner: result.learner.clone(),
        }
    }
}
fn attempt_directories(directory: &Path, prefix: &str) -> Result<Vec<(usize, PathBuf)>, String> {
    let mut found = Vec::new();
    for entry in fs::read_dir(directory).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name = entry.file_name();
        let text = name.to_str().ok_or("non-UTF8 attempt name")?;
        if let Some(suffix) = text.strip_prefix(prefix) {
            require(
                suffix.len() == 6
                    && suffix.bytes().all(|b| b.is_ascii_digit())
                    && entry.file_type().map_err(err)?.is_dir(),
                "invalid attempt directory",
            )?;
            found.push((suffix.parse::<usize>().map_err(err)?, entry.path()));
        }
    }
    found.sort_by_key(|(index, _)| *index);
    Ok(found)
}
fn new_attempt(
    directory: &Path,
    prefix: &str,
    existing: &[(usize, PathBuf)],
) -> Result<PathBuf, String> {
    let number = existing.last().map_or(Ok(0), |(n, _)| {
        n.checked_add(1).ok_or("attempt index overflow")
    })?;
    require(number <= 999_999, "execution attempt directories exhausted")?;
    let path = directory.join(format!("{prefix}{number:06}"));
    fs::create_dir(&path).map_err(err)?;
    Ok(path)
}
fn slot_recovery(
    directory: &Path,
    resolved: &ResolvedMatch,
    verified: &mut VerifiedFiles,
) -> Result<
    (
        Option<Bo3AttemptInputV1>,
        Option<(PathBuf, PinnedFileV1)>,
        usize,
    ),
    String,
> {
    fs::create_dir_all(directory).map_err(err)?;
    let existing = attempt_directories(directory, "attempt-")?;
    let expected = bounded_json(&resolved.request, 4 * 1024 * 1024)?;
    let mut complete = None;
    let mut interrupted = 0usize;
    for (_, attempt) in &existing {
        let request_path = attempt.join("request.json");
        let intent_path = attempt.join("intent.json");
        let result_path = attempt.join("result.json");
        let request_pin = pin_bytes(request_path.clone(), &expected);
        if request_path.try_exists().map_err(err)? {
            verified.verify(&request_pin, 4 * 1024 * 1024)?;
        }
        if intent_path.try_exists().map_err(err)? {
            require(
                request_path.try_exists().map_err(err)?,
                "slot intent lacks published request",
            )?;
            let intent: SlotIntent = read_json(&intent_path, SMALL)?;
            require(
                intent.schema == "mtg-kernel-bo3-slot-publication/v1"
                    && intent.request == request_pin
                    && intent.result.path == result_path,
                "slot publication intent differs from exact request/location",
            )?;
            if result_path.try_exists().map_err(err)? {
                verified.verify(&intent.result, resolved.max_result_bytes)?;
                let candidate = Bo3AttemptInputV1 {
                    request: request_pin,
                    result: intent.result,
                    learner_seat: resolved.learner_seat,
                };
                if let Some(prior) = &complete {
                    let prior: &Bo3AttemptInputV1 = prior;
                    require(
                        prior.result.sha256 == candidate.result.sha256,
                        "multiple published slot attempts disagree",
                    )?;
                } else {
                    complete = Some(candidate);
                }
            } else {
                interrupted += 1;
            }
        } else {
            require(
                !result_path.try_exists().map_err(err)?,
                "unbound final capture is corrupt run state",
            )?;
            interrupted += 1;
        }
    }
    if complete.is_some() {
        return Ok((complete, None, interrupted));
    }
    let attempt = new_attempt(directory, "attempt-", &existing)?;
    let request = publish_bytes(&attempt, "request.json", &expected)?;
    Ok((None, Some((attempt, request)), interrupted))
}
fn collect_slot(
    resolved: &ResolvedMatch,
    directory: &Path,
    request: &PinnedFileV1,
) -> Result<Bo3AttemptInputV1, String> {
    let captured = collect_trainable_bo3_v1(resolved.request.clone())?;
    let bytes = bounded_json(&captured, resolved.max_result_bytes)?;
    drop(captured);
    publish_slot_bytes(directory, request, resolved.learner_seat, bytes)
}
fn publish_slot_bytes(
    directory: &Path,
    request: &PinnedFileV1,
    learner_seat: PlayerSeatV1,
    bytes: Vec<u8>,
) -> Result<Bo3AttemptInputV1, String> {
    let result = pin_bytes(directory.join("result.json"), &bytes);
    // This durable hash intent precedes the final payload. There is no final
    // capture without a previously published exact request/result binding.
    publish(
        directory,
        "intent.json",
        &SlotIntent {
            schema: "mtg-kernel-bo3-slot-publication/v1".into(),
            request: request.clone(),
            result: result.clone(),
        },
        SMALL,
    )?;
    require(
        publish_bytes(directory, "result.json", &bytes)? == result,
        "result publication pin differs",
    )?;
    drop(bytes);
    Ok(Bo3AttemptInputV1 {
        request: request.clone(),
        result,
        learner_seat,
    })
}
fn preparation_request(
    config: &NativeBo3TrainingRunV1,
    learner: &ExpandedSeatBehaviorV1,
    attempts: Vec<Bo3AttemptInputV1>,
) -> Bo3GameplayPreparationRequestV1 {
    Bo3GameplayPreparationRequestV1 {
        schema: BO3_PREPARATION_REQUEST_SCHEMA_V1.into(),
        learner: learner.clone(),
        attempts,
        limits: config.preparation_limits.clone(),
    }
}
fn update_request(
    config: &NativeBo3TrainingRunV1,
    input: &Bo3GameplayUpdateInputV1,
    previous: Option<PinnedFileV1>,
    preparation: PinnedFileV1,
    output: PathBuf,
) -> Bo3GameplayUpdateRequestV1 {
    Bo3GameplayUpdateRequestV1 {
        schema: BO3_GAMEPLAY_UPDATE_SCHEMA_V1.into(),
        input: input.clone(),
        preparation_request: preparation,
        learning_rate_bits: config.learning_rate_bits,
        value_coefficient_bits: config.value_coefficient_bits,
        previous_progress: previous,
        output_directory: output,
    }
}
fn update_recovery(
    config: &NativeBo3TrainingRunV1,
    directory: &Path,
    input: &Bo3GameplayUpdateInputV1,
    previous: &Option<PinnedFileV1>,
    preparation: &PinnedFileV1,
    verified: &mut VerifiedFiles,
) -> Result<(Bo3GameplayUpdateRequestV1, PinnedFileV1, bool, usize), String> {
    let existing = attempt_directories(directory, "update-attempt-")?;
    let mut recovery = None;
    let mut interrupted = 0usize;
    for (_, attempt) in &existing {
        let saved = attempt.join("update-request.json");
        let operation = attempt.join("operation");
        let request = update_request(
            config,
            input,
            previous.clone(),
            preparation.clone(),
            operation.clone(),
        );
        let pin = pin_bytes(saved.clone(), &bounded_json(&request, 1024 * 1024)?);
        if saved.try_exists().map_err(err)? {
            verified.verify(&pin, 1024 * 1024)?;
        }
        let has_progress = operation.join("progress.json").try_exists().map_err(err)?;
        let has_checkpoint = operation
            .join("checkpoint.json")
            .try_exists()
            .map_err(err)?;
        if has_progress || has_checkpoint {
            require(
                saved.try_exists().map_err(err)? && recovery.is_none(),
                "committed update lacks its exact request or has conflicting committed attempts",
            )?;
            recovery = Some((request, pin));
        } else {
            interrupted += 1;
            if operation.join("request.json").try_exists().map_err(err)? {
                let operation_pin = pin_bytes(
                    operation.join("request.json"),
                    &bounded_json(&request, 1024 * 1024)?,
                );
                verified.verify(&operation_pin, 1024 * 1024)?;
            }
        }
    }
    if let Some((request, pin)) = recovery {
        return Ok((request, pin, true, interrupted));
    }
    // No committed state exists. Preserve the incomplete operation and restart
    // from the same batch inputs, with a fresh directory for its stage files.
    let attempt = new_attempt(directory, "update-attempt-", &existing)?;
    let request = update_request(
        config,
        input,
        previous.clone(),
        preparation.clone(),
        attempt.join("operation"),
    );
    let pin = publish(&attempt, "update-request.json", &request, 1024 * 1024)?;
    Ok((request, pin, false, interrupted))
}

fn prefix_length(present: &[bool]) -> Result<usize, String> {
    let prefix = present.iter().take_while(|&&x| x).count();
    require(
        present[prefix..].iter().all(|x| !x),
        "completed BO3 batches are not a contiguous prefix",
    )?;
    Ok(prefix)
}
fn verify_behavior_files(
    verified: &mut VerifiedFiles,
    behavior: &ExpandedSeatBehaviorV1,
) -> Result<(), String> {
    verified.verify(&behavior.source.play_import, INPUT)?;
    if let Some(checkpoint) = &behavior.source.checkpoint {
        verified.verify(checkpoint, INPUT)?;
    }
    Ok(())
}
fn validate_result_progress(
    result: &Bo3GameplayUpdateResultV1,
    before: &ValidatedBo3TipV1,
    attempts: usize,
) -> Result<(), String> {
    validate_count_delta(
        (
            before.completed_bo3_updates,
            before.attempted_batches,
            before.attempted_matches,
        ),
        (
            result.completed_bo3_updates,
            result.attempted_batches,
            result.attempted_matches,
        ),
        result.optimizer_updated,
        attempts,
    )
}
fn validate_count_delta(
    before: (u64, u64, usize),
    after: (u64, u64, usize),
    updated: bool,
    attempts: usize,
) -> Result<(), String> {
    require(
        after.0
            == before
                .0
                .checked_add(u64::from(updated))
                .ok_or("update counter overflow")?
            && after.1 == before.1.checked_add(1).ok_or("batch counter overflow")?
            && after.2
                == before
                    .2
                    .checked_add(attempts)
                    .ok_or("match counter overflow")?,
        "result counters differ from one declared attempted batch",
    )
}

/// Run only this finite, explicitly match-counted CPU schedule. The invocation
/// limit pauses at completed batch boundaries and never changes the saved plan.
pub fn run_native_bo3_training_v1(
    config: &NativeBo3TrainingRunV1,
    max_new_batches: Option<usize>,
) -> Result<NativeBo3RunResultV1, String> {
    config.validate_v1()?;
    require(
        max_new_batches != Some(0),
        "max-new-batches must be positive",
    )?;
    let invocation = Instant::now();
    fs::create_dir_all(&config.output_directory).map_err(err)?;
    let root = &config.output_directory;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("run.lock"))
        .map_err(err)?;
    lock.try_lock()
        .map_err(|e| format!("BO3 run already has a writer or cannot lock: {e}"))?;
    let manifest = RunManifest {
        schema: "mtg-kernel-native-bo3-run-manifest/v1".into(),
        config: config.clone(),
        git_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").into(),
        tracked_tree_sha256: env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256").into(),
        implementation_sha256: digest(include_bytes!("runner.rs")),
    };
    let batches_root = root.join("batches");
    if !root.join("run.json").try_exists().map_err(err)? {
        require(
            !batches_root.try_exists().map_err(err)?
                && !root.join("completion.json").try_exists().map_err(err)?,
            "run artifacts lack their original manifest",
        )?;
    }
    let manifest_pin = publish(root, "run.json", &manifest, SMALL)?;
    fs::create_dir_all(&batches_root).map_err(err)?;
    // Runtime identity is this actual executable, not an old collector label.
    config.initial_learner.runtime.verify_current_runtime_v1()?;
    let prefix = prefix_length(
        &(0..config.batches.len())
            .map(|index| {
                batches_root
                    .join(format!("{index:06}"))
                    .join("complete.json")
                    .try_exists()
                    .map_err(err)
            })
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    require(
        !root.join("completion.json").try_exists().map_err(err)? || prefix == config.batches.len(),
        "run completion exists without its whole batch prefix",
    )?;
    let mut verified = VerifiedFiles::default();
    let mut input = config.initial_input.clone();
    let mut previous = config.previous_progress.clone();
    let mut history = Vec::new();
    let mut receipts = Vec::new();
    let prefix_started = Instant::now();
    let mut prefix_ledger = Vec::new();
    let mut before_counts = if let Some(pin) = &previous {
        let (saved, operation, ledger) = recorded_bo3_tip_v1(
            &input,
            pin,
            &verified.bytes(pin, 32 * 1024 * 1024)?,
            config.learning_rate_bits,
            config.value_coefficient_bits,
        )?;
        verified.verify(
            &pin_bytes(
                operation.output_directory.join("request.json"),
                &bounded_json(&operation, 1024 * 1024)?,
            ),
            1024 * 1024,
        )?;
        prefix_ledger = ledger;
        (
            saved.completed_bo3_updates,
            saved.attempted_batches,
            saved.attempted_matches,
        )
    } else {
        (0, 0, 0)
    };
    let mut last_result: Option<Bo3GameplayUpdateResultV1> = None;
    let mut prefix_keys: BTreeSet<_> = prefix_ledger
        .iter()
        .map(|triple| triple[2].clone())
        .collect();
    for index in 0..prefix {
        let directory = batches_root.join(format!("{index:06}"));
        let path = directory.join("complete.json");
        let receipt: BatchReceipt = read_json(&path, SMALL)?;
        let receipt_pin = pin_bytes(path, &bounded_json(&receipt, SMALL)?);
        verified.verify(&receipt_pin, SMALL)?;
        require(
            receipt.schema == "mtg-kernel-native-bo3-batch/v1"
                && receipt.batch == index
                && receipt.manifest == manifest_pin
                && receipt.input == input
                && receipt.previous_progress == previous,
            "completed batch differs from exact schedule/input/progress",
        )?;
        let resolved = resolve_batch(config, index, input.learner_v1(), &history)?;
        require(
            receipt.attempts.len() == resolved.len(),
            "completed batch attempt count differs",
        )?;
        for (slot, (attempt, item)) in receipt.attempts.iter().zip(&resolved).enumerate() {
            let slot_root = directory.join("slots").join(format!("{slot:03}"));
            let parent = attempt
                .request
                .path
                .parent()
                .ok_or("attempt request has no parent")?;
            require(
                parent.parent() == Some(slot_root.as_path())
                    && parent
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(valid_attempt_name)
                    && attempt.request.path == parent.join("request.json")
                    && attempt.result.path == parent.join("result.json")
                    && attempt.learner_seat == item.learner_seat,
                "completed slot paths or learner seat differ",
            )?;
            require(
                attempt.request
                    == pin_bytes(
                        attempt.request.path.clone(),
                        &bounded_json(&item.request, 4 * 1024 * 1024)?,
                    ),
                "completed slot does not use resolved current/frozen packages",
            )?;
            verified.verify(&attempt.request, 4 * 1024 * 1024)?;
            let intent: SlotIntent = read_json(&parent.join("intent.json"), SMALL)?;
            require(
                intent
                    == SlotIntent {
                        schema: "mtg-kernel-bo3-slot-publication/v1".into(),
                        request: attempt.request.clone(),
                        result: attempt.result.clone(),
                    },
                "completed slot intent differs",
            )?;
            verified.verify(&attempt.result, item.max_result_bytes)?;
            require(
                prefix_keys.insert(item.physical_key.clone()),
                "duplicate physical sample in completed schedule",
            )?;
            prefix_ledger.push([
                attempt.request.sha256.clone(),
                attempt.result.sha256.clone(),
                item.physical_key.clone(),
            ]);
        }
        let preparation = preparation_request(config, input.learner_v1(), receipt.attempts.clone());
        require(
            receipt.preparation_request
                == pin_bytes(
                    directory.join("preparation.json"),
                    &bounded_json(&preparation, 1024 * 1024)?,
                ),
            "completed preparation input differs",
        )?;
        verified.verify(&receipt.preparation_request, 1024 * 1024)?;
        let operation_parent = receipt
            .update_request
            .path
            .parent()
            .ok_or("update request has no parent")?;
        require(
            operation_parent.parent() == Some(directory.as_path())
                && operation_parent
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(valid_update_attempt_name)
                && receipt.update_request.path == operation_parent.join("update-request.json"),
            "completed update request path differs",
        )?;
        let request = update_request(
            config,
            &input,
            previous.clone(),
            receipt.preparation_request.clone(),
            operation_parent.join("operation"),
        );
        require(
            receipt.update_request
                == pin_bytes(
                    receipt.update_request.path.clone(),
                    &bounded_json(&request, 1024 * 1024)?,
                ),
            "completed update request differs",
        )?;
        verified.verify(&receipt.update_request, 1024 * 1024)?;
        verified.verify(
            &pin_bytes(
                request.output_directory.join("request.json"),
                &bounded_json(&request, 1024 * 1024)?,
            ),
            1024 * 1024,
        )?;
        validate_recorded_bo3_result_v1(
            &request,
            &receipt.result,
            &verified.bytes(&receipt.result.progress, 32 * 1024 * 1024)?,
            Some(&prefix_ledger),
        )?;
        let after_counts = (
            receipt.result.completed_bo3_updates,
            receipt.result.attempted_batches,
            receipt.result.attempted_matches,
        );
        validate_count_delta(
            before_counts,
            after_counts,
            receipt.result.optimizer_updated,
            resolved.len(),
        )?;
        before_counts = after_counts;
        require(
            receipt.result.learner.identity.adam_step
                == input
                    .learner_v1()
                    .identity
                    .adam_step
                    .checked_add(u64::from(receipt.result.optimizer_updated))
                    .ok_or("Adam counter overflow")?
                && (receipt.result.optimizer_updated
                    || receipt.result.learner == *input.learner_v1()),
            "completed batch changed state outside its declared optimizer disposition",
        )?;
        verify_behavior_files(&mut verified, &receipt.result.learner)?;
        input = next_input(&receipt.result);
        previous = Some(receipt.result.progress.clone());
        history.push(receipt.result.learner.clone());
        last_result = Some(receipt.result);
        receipts.push(receipt_pin);
    }
    let prefix_io_seconds = prefix_started.elapsed().as_secs_f64();
    // Exactly the reconstructed latest tip gets full model/optimizer loading.
    // Historical tensor files above are streamed against immutable pins only.
    let tip_started = Instant::now();
    let mut tip = validate_bo3_tip_v1(
        &input,
        previous.as_ref(),
        config.learning_rate_bits,
        config.value_coefficient_bits,
    )?;
    let initial_tip_validation_seconds = tip_started.elapsed().as_secs_f64();
    if let Some(saved) = &last_result {
        require(
            saved.completed_bo3_updates == tip.completed_bo3_updates
                && saved.attempted_batches == tip.attempted_batches
                && saved.attempted_matches == tip.attempted_matches
                && prefix_keys == tip.consumed_physical_keys,
            "latest validated tip differs from completed prefix",
        )?;
    }
    require(
        before_counts
            == (
                tip.completed_bo3_updates,
                tip.attempted_batches,
                tip.attempted_matches,
            )
            && prefix_keys == tip.consumed_physical_keys,
        "validated tip differs from initial and completed ledger prefix",
    )?;
    let remaining_matches = config.batches[prefix..]
        .iter()
        .map(|b| b.matches.len())
        .sum::<usize>();
    require(
        tip.attempted_matches
            .checked_add(remaining_matches)
            .is_some_and(|n| n <= LEDGER),
        "finite schedule exceeds remaining exact attempt ledger",
    )?;
    // Keep only one loaded fixed package at a time. Collection itself owns each
    // match's independent runtime/policies; there is no shared inference mutex.
    if prefix < config.batches.len() {
        for opponent in &config.opponents {
            drop(opponent.package.load_supported_components_v1()?);
        }
    }
    let invocation_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(err)?
            .as_nanos()
    );
    let telemetry_root = root.join("telemetry");
    fs::create_dir_all(&telemetry_root).map_err(err)?;
    let mut new_batches = 0usize;
    for index in prefix..config.batches.len() {
        if max_new_batches.is_some_and(|limit| new_batches >= limit) {
            break;
        }
        let batch_started = Instant::now();
        let resolved = resolve_batch(config, index, input.learner_v1(), &history)?;
        for item in &resolved {
            require(
                !tip.consumed_physical_keys.contains(&item.physical_key),
                "scheduled physical sample is already consumed; no renaming or backfill",
            )?;
        }
        let directory = batches_root.join(format!("{index:06}"));
        fs::create_dir_all(&directory).map_err(err)?;
        let collection_started = Instant::now();
        let mut attempts: Vec<Option<Bo3AttemptInputV1>> = vec![None; resolved.len()];
        let mut pending = Vec::new();
        let mut interrupted_slots = 0usize;
        // Persist every exact request before dispatching any slot in this batch.
        for (slot, item) in resolved.iter().enumerate() {
            let slot_root = directory.join("slots").join(format!("{slot:03}"));
            let (complete, work, interrupted) = slot_recovery(&slot_root, item, &mut verified)?;
            interrupted_slots += interrupted;
            attempts[slot] = complete;
            if let Some((path, pin)) = work {
                pending.push((slot, path, pin));
            }
        }
        let reused = attempts.iter().filter(|v| v.is_some()).count();
        let pool = if pending.is_empty() {
            PoolReport {
                outputs: Vec::new(),
                timings: Vec::new(),
                failure: None,
                started_workers: 0,
            }
        } else {
            ordered_jobs(
                config.collection_workers.min(pending.len()),
                pending.len(),
                |index| {
                    let (slot, directory, pin) = &pending[index];
                    collect_slot(&resolved[*slot], directory, pin)
                },
            )?
        };
        for (index, value) in pool.outputs {
            attempts[pending[index].0] = Some(value);
        }
        let collection_seconds = collection_started.elapsed().as_secs_f64();
        let collection_telemetry = serde_json::json!({
            "schema":"mtg-kernel-bo3-collection-wall/v1", "batch":index,
            "requested_workers":config.collection_workers, "started_workers":pool.started_workers,
            "reused_slots":reused, "completed_slots":attempts.iter().filter(|v|v.is_some()).count(),
            "preserved_interrupted_slot_attempts":interrupted_slots,
            "collection_recovery_and_publication_wall_seconds":collection_seconds,
            "workers":pool.timings, "error":pool.failure,
            "unused_capacity_reason":"bounded finite pending match count; no speculative next batch",
            "cpu_utilization_claim":false, "resource_caps_owner":"outer execution supervisor"
        });
        publish(
            &telemetry_root,
            &format!("{invocation_id}-batch-{index:06}-collection.json"),
            &collection_telemetry,
            SMALL,
        )?;
        if let Some(error) = pool.failure {
            return Err(error);
        }
        let attempts = attempts
            .into_iter()
            .enumerate()
            .map(|(slot, value)| value.ok_or_else(|| format!("missing slot {slot}")))
            .collect::<Result<Vec<_>, _>>()?;
        let preparation = preparation_request(config, input.learner_v1(), attempts.clone());
        let preparation_pin = publish(&directory, "preparation.json", &preparation, 1024 * 1024)?;
        let update_started = Instant::now();
        let (request, request_pin, recovery, interrupted_updates) = update_recovery(
            config,
            &directory,
            &input,
            &previous,
            &preparation_pin,
            &mut verified,
        )?;
        // The existing updater prepares/replays once, applies at most one Adam
        // step, or resumes a committed checkpoint without preparing/updating.
        let result = match update_bo3_gameplay_v1(request) {
            Ok(result) => result,
            Err(error) => {
                publish(
                    &telemetry_root,
                    &format!("{invocation_id}-batch-{index:06}-update-error.json"),
                    &serde_json::json!({"batch":index,"committed_recovery":recovery,"error":error,
                        "preserved_interrupted_update_attempts":interrupted_updates,
                        "combined_update_wall_seconds":update_started.elapsed().as_secs_f64()}),
                    SMALL,
                )?;
                return Err(error);
            }
        };
        let update_seconds = update_started.elapsed().as_secs_f64();
        validate_result_progress(&result, &tip, resolved.len())?;
        require(
            result.learner.identity.adam_step
                == input
                    .learner_v1()
                    .identity
                    .adam_step
                    .checked_add(u64::from(result.optimizer_updated))
                    .ok_or("Adam counter overflow")?
                && (result.optimizer_updated || result.learner == *input.learner_v1()),
            "updated batch changed state outside declared disposition",
        )?;
        let receipt = BatchReceipt {
            schema: "mtg-kernel-native-bo3-batch/v1".into(),
            batch: index,
            manifest: manifest_pin.clone(),
            input: input.clone(),
            previous_progress: previous.clone(),
            attempts,
            preparation_request: preparation_pin,
            update_request: request_pin,
            result: result.clone(),
        };
        receipts.push(publish(&directory, "complete.json", &receipt, SMALL)?);
        for item in &resolved {
            tip.consumed_physical_keys.insert(item.physical_key.clone());
        }
        tip.completed_bo3_updates = result.completed_bo3_updates;
        tip.attempted_batches = result.attempted_batches;
        tip.attempted_matches = result.attempted_matches;
        input = next_input(&result);
        previous = Some(result.progress.clone());
        history.push(result.learner.clone());
        new_batches += 1;
        let telemetry = serde_json::json!({"schema":"mtg-kernel-bo3-batch-wall/v1","batch":index,
            "batch_wall_seconds":batch_started.elapsed().as_secs_f64(),"collection_wall_seconds":collection_seconds,
            "preparation_update_checkpoint_readback_combined_seconds":update_seconds,"committed_recovery":recovery,
            "preserved_interrupted_update_attempts":interrupted_updates,
            "optimizer_updated_for_recorded_operation":result.optimizer_updated,
            "completed_bo3_updates":result.completed_bo3_updates,"attempted_batches":result.attempted_batches,
            "attempted_matches":result.attempted_matches,"separate_update_checkpoint_timing_available":false});
        publish(
            &telemetry_root,
            &format!("{invocation_id}-batch-{index:06}.json"),
            &telemetry,
            SMALL,
        )?;
    }
    let result = NativeBo3RunResultV1 {
        schema: "mtg-kernel-native-bo3-run-result/v1".into(),
        complete: receipts.len() == config.batches.len(),
        completed_batches: receipts.len(),
        planned_batches: config.batches.len(),
        learner: input.learner_v1().clone(),
        previous_progress: previous,
        completed_bo3_updates: tip.completed_bo3_updates,
        attempted_batches: tip.attempted_batches,
        attempted_matches: tip.attempted_matches,
        batch_receipts: receipts,
    };
    if result.complete {
        publish(root, "completion.json", &result, SMALL)?;
    }
    publish(
        &telemetry_root,
        &format!("{invocation_id}-invocation.json"),
        &serde_json::json!({
            "schema":"mtg-kernel-bo3-run-wall/v1", "invocation_wall_seconds":invocation.elapsed().as_secs_f64(),
            "completed_prefix_batches":prefix,"newly_completed_batches":new_batches,
            "prefix_stream_hash_and_metadata_seconds":prefix_io_seconds,
            "latest_tip_validation_seconds":initial_tip_validation_seconds,
            "complete":result.complete,"strength_claim":false,"acceleration_claim":false
        }),
        SMALL,
    )?;
    Ok(result)
}
fn valid_attempt_name(name: &str) -> bool {
    name.strip_prefix("attempt-")
        .is_some_and(|n| n.len() == 6 && n.bytes().all(|b| b.is_ascii_digit()))
}
fn valid_update_attempt_name(name: &str) -> bool {
    name.strip_prefix("update-attempt-")
        .is_some_and(|n| n.len() == 6 && n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
