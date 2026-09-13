//! Resumable training with CPU V3 collection and an explicit update backend.
//! This successor owns its checkpoints; it does not reinterpret legacy Stores.

use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, publish_new_file_v1, DurableFileExpectationV1,
};
use crate::expanded_deck_training_v1::{
    execute_v1, load_expanded_inference_v1, ExpandedEpisodeV1, ExpandedInferenceIdentityV1,
    ExpandedModelSourceV1, ExpandedTrainingCommandV1, ExpandedUpdateBackendV1, PinnedFileV1,
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

const SCHEMA: &str = "mtg-kernel-native-expanded-training-run/v1";
const SMALL_CAP: u64 = 16 * 1024 * 1024;
const ARTIFACT_CAP: u64 = 512 * 1024 * 1024;

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
    pub output_directory: PathBuf,
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
    pub fn validate_v1(&self) -> Result<(), String> {
        check(self.schema == SCHEMA, "unknown successor run schema")?;
        self.update_backend.validate_v1()?;
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
    let trajectories: Vec<PinnedFileV1> =
        serde_json::from_value(document["trajectories"].clone()).map_err(err)?;
    check(
        trajectories.len() == episodes.len(),
        "collection episode count differs",
    )?;
    for (trajectory, episode) in trajectories.iter().zip(episodes) {
        verify_pin(trajectory)?;
        let saved = read_json(&trajectory.path, ARTIFACT_CAP)?;
        check(
            saved["episode"] == value(episode)?,
            "collection episode assignment differs",
        )?;
    }
    Ok(trajectories)
}

fn validate_update(
    update: &PinnedFileV1,
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    before: &ExpandedInferenceIdentityV1,
    learning_rate: f32,
    value_coefficient: f32,
    update_backend: ExpandedUpdateBackendV1,
) -> Result<(ExpandedModelSourceV1, ExpandedInferenceIdentityV1), String> {
    let document = read_pin(update)?;
    update_backend.validate_update_execution_v1(&document)?;
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
            let collect_command = ExpandedTrainingCommandV1::Collect {
                source: current.clone(),
                episodes: episodes.clone(),
                output_directory: attempt.join("collect"),
            };
            check(
                read_json(&saved_command, SMALL_CAP)? == value(&collect_command)?,
                "interrupted attempt collection command differs",
            )?;
            let path = attempt.join("collect/collection.json");
            if path.exists() {
                let candidate = pin(&path)?;
                validate_collection(&candidate, &current, &episodes, &current_identity)?;
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
                let trajectories = validate_collection(
                    &referenced_collection,
                    &current,
                    &episodes,
                    &current_identity,
                )?;
                validate_update(
                    &candidate,
                    &current,
                    &trajectories,
                    &current_identity,
                    config.learning_rate,
                    config.value_coefficient,
                    config.update_backend,
                )?;
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
            let collect_command = ExpandedTrainingCommandV1::Collect {
                source: current.clone(),
                episodes: episodes.clone(),
                output_directory: attempt.join("collect"),
            };
            publish(&attempt, "collect-command.json", &collect_command)?;
            if collection_pin.is_none() {
                execute_v1(collect_command)?;
                collection_pin = Some(pin(&attempt.join("collect/collection.json"))?);
            }
            let collection = collection_pin.as_ref().unwrap();
            let trajectories =
                validate_collection(collection, &current, &episodes, &current_identity)?;
            publish(
                &attempt,
                "update-input.json",
                &json!({"collection":collection}),
            )?;
            let update_command = ExpandedTrainingCommandV1::Update {
                source: current.clone(),
                trajectories,
                learning_rate: config.learning_rate,
                value_coefficient: config.value_coefficient,
                update_backend: config.update_backend,
                output_directory: attempt.join("update"),
            };
            publish(&attempt, "update-command.json", &update_command)?;
            execute_v1(update_command)?;
            update_pin = Some(pin(&attempt.join("update/update.json"))?);
        }
        let collection = collection_pin.unwrap();
        let update = update_pin.unwrap();
        let trajectories =
            validate_collection(&collection, &current, &episodes, &current_identity)?;
        let (next, after) = validate_update(
            &update,
            &current,
            &trajectories,
            &current_identity,
            config.learning_rate,
            config.value_coefficient,
            config.update_backend,
        )?;
        let receipt = json!({"schema":"mtg-kernel-native-expanded-iteration/v1", "iteration":index,
            "source":current,"episodes_sha256":episode_digest,"collection":collection,"update":update,
            "output_identity":after});
        receipts.push(publish(&directory, "complete.json", &receipt)?);
        current = next;
        current_identity = after;
        history.push(current.clone());
        newly_completed += 1;
        println!(
            "{}",
            json!({"completed_iteration":index,"adam_step":current_identity.adam_step,
            "checkpoint":current.checkpoint})
        );
    }
    let complete = receipts.len() == config.iterations.len();
    let mut result = json!({"schema":SCHEMA,"complete":complete,"completed_iterations":receipts.len(),
        "planned_iterations":config.iterations.len(),"iterations":receipts,"source":current,
        "actual_identity":current_identity,"device":"cpu","gpu_ordinal":null,
        "loss_identity":"terminal_reinforce_value/v3","strength_claim":false});
    config.update_backend.record_run_execution_v1(&mut result);
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
            output_directory: std::env::temp_dir().join("native-expanded-validation-only"),
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
}
