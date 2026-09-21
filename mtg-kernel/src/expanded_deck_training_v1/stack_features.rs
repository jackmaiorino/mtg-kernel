//! Bounded synchronous stack-feature learning, with distinct trajectory and
//! checkpoint identities. No legacy trajectory is relabeled as a successor.
use super::*;
use crate::experimental_burn_net8_packed_v1::stack_training::{
    self, StackDeviceTrainState, StackProjectionSnapshot, StackTrainingGroup, StackTrainingStep,
};
use crate::native_policy_value_net_v1::stack_inputs_v1::StackInputWeightsV1;
use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, PairedBo1PolicyV1};
use crate::public_stack_features_v1::{
    StackEncodedDecisionV1, StackFeatureRowsV1, StackInputModeV1, CONTRACT, PERMUTATION_CONTRACT,
};
use crate::sideboard_play_policy_v1::stack_inputs::StackInputPlayPolicyV1;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub stack_contract_sha256: String,
    pub permutation_contract_sha256: String,
    pub source: ExpandedModelSourceV1,
    pub updates: Vec<Vec<ExpandedEpisodeV1>>,
    pub input_mode: StackInputModeV1,
    pub learning_rate: f32,
    pub value_coefficient: f32,
    pub gamma: f32,
    pub lambda: f32,
    pub gpu_ordinal: usize,
    pub max_chunk_substeps: usize,
    /// A learning-only opt-in, omitted from the serialized config when zero.
    #[serde(default, skip_serializing_if = "entropy_is_zero")]
    pub entropy_coefficient: f32,
}

fn entropy_is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn validate_entropy(config: &Config) -> Result<(), String> {
    ensure(
        config.schema == "mtg-kernel-stack-training-config/v1"
            && config.stack_contract_sha256 == sha(CONTRACT)
            && config.permutation_contract_sha256 == sha(PERMUTATION_CONTRACT),
        "stack config/input contract differs",
    )?;
    ensure(
        config.entropy_coefficient.is_finite() && (0.0..=1.0).contains(&config.entropy_coefficient),
        "stack entropy coefficient must be finite in [0,1]",
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub config: Config,
    pub output_directory: PathBuf,
    pub resume: Option<PinnedFileV1>,
    pub stop_after: Option<usize>,
    /// Execution placement only; excluded from the scientific config hash.
    #[serde(default = "default_collector_workers")]
    pub collector_workers: usize,
    /// Explicit execution placement; preserves the frozen scientific config.
    /// Omission retains its original device assignment.
    #[serde(default)]
    pub execution_gpu_ordinal: Option<usize>,
}

fn default_collector_workers() -> usize {
    1
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trajectory {
    schema: String,
    config_sha256: String,
    optimizer_state_sha256: String,
    input_mode: StackInputModeV1,
    episode: ExpandedEpisodeV1,
    opponent: ExpandedInferenceIdentityV1,
    configuration_sha256: [String; 2],
    decisions: Vec<DecisionRecordV1>,
    auxiliary: Vec<Option<StackFeatureRowsV1>>,
    terminal: RlSessionTerminalV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    config_sha256: String,
    next_update: usize,
    optimizer_file: String,
    optimizer_sha256: String,
    trajectory_sha256: Vec<String>,
}

fn weights(stack: &StackProjectionSnapshot) -> Result<StackInputWeightsV1, String> {
    StackInputWeightsV1::new(floats(&stack.weight)).map_err(err)
}

fn validate_projection_mode(
    config: &Config,
    stack: &StackProjectionSnapshot,
) -> Result<(), String> {
    if config.input_mode == StackInputModeV1::Disabled {
        ensure(
            stack
                .weight
                .iter()
                .chain(&stack.first)
                .chain(&stack.second)
                .all(|v| f32::from_bits(*v) == 0.0),
            "disabled stack projection acquired weights or moments",
        )?;
    }
    Ok(())
}

/// CPU inference load only. Reading a CUDA-produced checkpoint creates no GPU
/// device and cannot advance either optimizer.
pub(crate) fn load_for_evaluation(
    config_pin: &PinnedFileV1,
    checkpoint_pin: &PinnedFileV1,
) -> Result<(StackInputPlayPolicyV1, Value), String> {
    let config: Config = serde_json::from_slice(&read_pinned_bytes(config_pin)?).map_err(err)?;
    validate_entropy(&config)?;
    let checkpoint: Checkpoint =
        serde_json::from_slice(&read_pinned_bytes(checkpoint_pin)?).map_err(err)?;
    let config_hash = sha(&serde_json::to_vec(&config).map_err(err)?);
    ensure(
        checkpoint.schema == "mtg-kernel-public-stack-checkpoint/v1"
            && checkpoint.config_sha256 == config_hash
            && checkpoint.optimizer_file == "optimizer.json"
            && checkpoint.next_update > 0
            && checkpoint.next_update <= config.updates.len(),
        "stack evaluation checkpoint/config identity differs",
    )?;
    let optimizer_path = checkpoint_pin
        .path
        .parent()
        .ok_or("checkpoint has no parent")?
        .join("optimizer.json");
    let saved = read_pinned_bytes(&PinnedFileV1 {
        path: optimizer_path,
        sha256: checkpoint.optimizer_sha256.clone(),
    })?;
    let (legacy, stack) = stack_training::snapshot::decode(&saved).map_err(err)?;
    validate_projection_mode(&config, &stack)?;
    let (mut base, initial) = initialize(&config.source)?;
    ensure(
        legacy.adam_step == initial.adam_step_v1() + checkpoint.next_update as u64
            && stack.adam_step == checkpoint.next_update as u64,
        "stack evaluation ages differ",
    )?;
    base.replace_training_parameters_v3(&legacy.parameters)?;
    let base_model = base.actual_model_identity_v1();
    let weights_sha256 = sha(&serde_json::to_vec(&(
        crate::native_policy_value_net_v1::stack_inputs_v1::ARCHITECTURE,
        &base_model.weights_sha256,
        &stack.weight,
        config.input_mode,
    ))
    .map_err(err)?);
    let identity = json!({"schema":"public-stack-evaluation-model/v1",
        "architecture":crate::native_policy_value_net_v1::stack_inputs_v1::ARCHITECTURE,
        "config_pin":config_pin,"checkpoint_pin":checkpoint_pin,"config_sha256":config_hash,
        "optimizer_sha256":checkpoint.optimizer_sha256,"weights_sha256":weights_sha256,
        "actual_base_model":base_model,"input_mode":config.input_mode,
        "legacy_adam_step":legacy.adam_step,"stack_adam_step":stack.adam_step});
    Ok((
        StackInputPlayPolicyV1::new(base, weights(&stack)?)?.with_mode(config.input_mode),
        identity,
    ))
}

fn collect(
    policy: &mut StackInputPlayPolicyV1,
    episode: &ExpandedEpisodeV1,
    config_hash: &str,
    state_hash: &str,
    mode: StackInputModeV1,
) -> Result<Trajectory, String> {
    let configs = episode.configurations()?;
    let (mut opponent, identity) = load_expanded_inference_v1(
        episode
            .opponent
            .as_ref()
            .ok_or("explicit opponent required")?,
    )?;
    ensure(
        opponent.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
        "stack collection requires V4 opponent",
    )?;
    let mut session=FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        1,episode.seed,episode.max_physical_decisions,episode.max_policy_steps,episode.selected.each_ref().map(|d|d.label.clone()),configs.each_ref().map(|c|c.mainboard().to_vec()),PlayerId(episode.starting_player)).map_err(err)?;
    let seeds = paired_policy_seeds_v1(episode.seed);
    policy.reset_for_game_v1(seeds).map_err(err)?;
    opponent.reset_sampling_v1(seeds);
    let mut decisions = Vec::new();
    let mut auxiliary = Vec::new();
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                let hashes = configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1()));
                validate_episode_records_v1(episode, &hashes, &decisions, &terminal)?;
                return Ok(Trajectory {
                    schema: "mtg-kernel-public-stack-trajectory/v1".into(),
                    config_sha256: config_hash.into(),
                    optimizer_state_sha256: state_hash.into(),
                    input_mode: mode,
                    episode: episode.clone(),
                    opponent: identity,
                    configuration_sha256: hashes,
                    decisions,
                    auxiliary,
                    terminal,
                });
            }
            FastActorResponseV1::Decision(d) => {
                ensure(
                    decisions.len() < episode.max_policy_steps as usize,
                    "stack collection step limit",
                )?;
                let learner = seat(d.acting_player) == episode.learner_seat;
                let (selected, scores, tensor) = if learner {
                    let input = PairedBo1PolicyInputV1::new(&session, d);
                    let (selected, scores) = policy.select_with_scores(&input).map_err(err)?;
                    let captured = policy.captured()?;
                    auxiliary.push(Some(captured.stack.clone()));
                    (
                        selected,
                        scores,
                        TensorBitsV1::from_tensor(&captured.legacy.common),
                    )
                } else {
                    let (selected, scores, tensor) =
                        opponent.select_with_training_tensor_v4(&session)?;
                    auxiliary.push(None);
                    (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                };
                decisions.push(DecisionRecordV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    substep_index: d.substep_index,
                    substep_count: d.substep_count,
                    actor: seat(d.acting_player),
                    selected,
                    logits: bits(&scores.logits),
                    value: scores.value.to_bits(),
                    tensor,
                    sampler_identity: decision_sampler_identity_v1(scores.logits.len())
                        .map(str::to_owned),
                });
                session.step(d.episode_id, d.step, selected).map_err(err)?;
            }
        }
    }
}

fn publish_bytes(directory: &Path, name: &str, bytes: &[u8]) -> Result<String, String> {
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "stack output size exceeds bound",
    )?;
    let parent = capture_existing_publication_parent_v1(directory).map_err(err)?;
    publish_new_file_v1(
        &parent,
        format!(".{name}.stage"),
        name,
        bytes,
        DurableFileExpectationV1::from_bytes(bytes).map_err(err)?,
    )
    .map_err(err)?;
    Ok(sha(bytes))
}

/// All collectors use the current batch's parameters. Results are ordered by
/// the original schedule before publication and the single learning update.
fn collect_parallel(
    policy: &StackInputPlayPolicyV1,
    episodes: &[ExpandedEpisodeV1],
    config_hash: &str,
    state_hash: &str,
    mode: StackInputModeV1,
    workers: usize,
    fork_seconds: &mut f64,
) -> Result<Vec<Trajectory>, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let fork_started = std::time::Instant::now();
    let policies = (0..workers.min(episodes.len()))
        .map(|_| policy.fork_for_collection())
        .collect::<Result<Vec<_>, _>>()?;
    *fork_seconds = fork_started.elapsed().as_secs_f64();
    let mut results = std::thread::scope(|scope| -> Result<Vec<(usize, Trajectory)>, String> {
        let mut handles = Vec::new();
        for (worker, mut policy) in policies.into_iter().enumerate() {
            let next = &next;
            handles.push(
                std::thread::Builder::new()
                    .name(format!("stack-collector-{worker}"))
                    .stack_size(16 * 1024 * 1024)
                    .spawn_scoped(
                        scope,
                        move || -> Result<Vec<(usize, Trajectory)>, String> {
                            let mut completed = Vec::new();
                            loop {
                                let index = next.fetch_add(1, Ordering::Relaxed);
                                let Some(episode) = episodes.get(index) else {
                                    break;
                                };
                                eprintln!(
                                    "stack collector {worker}: episode {index} {}",
                                    episode.id
                                );
                                completed.push((
                                    index,
                                    collect(&mut policy, episode, config_hash, state_hash, mode)?,
                                ));
                            }
                            Ok(completed)
                        },
                    )
                    .map_err(err)?,
            );
        }
        let mut results = Vec::new();
        // Join every worker before returning an error. No collector survives
        // into an optimizer update or starts a speculative next batch.
        let mut failure = None;
        for handle in handles {
            match handle.join() {
                Ok(Ok(mut rows)) => results.append(&mut rows),
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert("stack collector panicked".into());
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(results)
    })?;
    results.sort_by_key(|(index, _)| *index);
    ensure(
        results.len() == episodes.len()
            && results
                .iter()
                .enumerate()
                .all(|(i, (index, _))| i == *index),
        "parallel stack collection lost or duplicated an episode",
    )?;
    Ok(results
        .into_iter()
        .map(|(_, trajectory)| trajectory)
        .collect())
}

pub fn run(command: Command) -> Result<Value, String> {
    let config = &command.config;
    validate_entropy(config)?;
    let execution_gpu_ordinal = command.execution_gpu_ordinal.unwrap_or(config.gpu_ordinal);
    ensure(
        execution_gpu_ordinal < 16,
        "execution GPU ordinal outside bounds",
    )?;
    ensure(
        (1..=64).contains(&command.collector_workers),
        "stack collector count outside bounds",
    )?;
    ensure(
        !config.updates.is_empty() && config.updates.len() <= 256,
        "stack update count outside bounds",
    )?;
    ensure(
        config.learning_rate.to_bits() == 0.0001f32.to_bits()
            && config.value_coefficient.to_bits() == 0.5f32.to_bits()
            && config.gamma.to_bits() == 1f32.to_bits()
            && config.lambda.to_bits() == 0.9f32.to_bits(),
        "stack learning uses fixed g115 GAE/loss scalars",
    )?;
    ensure(
        config.gpu_ordinal == 1 && (1..=512).contains(&config.max_chunk_substeps),
        "stack GPU or chunk bound differs",
    )?;
    let mut ids = BTreeSet::new();
    for episodes in &config.updates {
        ensure(
            !episodes.is_empty() && episodes.len() <= 64,
            "stack episode batch outside bounds",
        )?;
        for episode in episodes {
            episode.configurations()?;
            ensure(
                episode.opponent.is_some() && ids.insert(episode.id.clone()),
                "missing opponent or duplicate episode",
            )?;
        }
    }
    let config_hash = sha(&serde_json::to_vec(config).map_err(err)?);
    let (base, state) = initialize(&config.source)?;
    ensure(
        base.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
        "stack initial source must be V4",
    )?;
    let initial_step = state.adam_step_v1();
    let (mut legacy, mut stack, first_update) = if let Some(pin) = &command.resume {
        let checkpoint: Checkpoint =
            serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        ensure(
            checkpoint.schema == "mtg-kernel-public-stack-checkpoint/v1"
                && checkpoint.config_sha256 == config_hash
                && checkpoint.optimizer_file == "optimizer.json",
            "stack resume identity differs",
        )?;
        let path = pin
            .path
            .parent()
            .ok_or("resume path has no parent")?
            .join("optimizer.json");
        let saved = read_pinned_bytes(&PinnedFileV1 {
            path,
            sha256: checkpoint.optimizer_sha256,
        })?;
        let (legacy, stack) = stack_training::snapshot::decode(&saved).map_err(err)?;
        ensure(
            legacy.adam_step == initial_step + checkpoint.next_update as u64
                && stack.adam_step == checkpoint.next_update as u64,
            "resume optimizer ages differ",
        )?;
        (legacy, stack, checkpoint.next_update)
    } else {
        (
            state.snapshot_v1().map_err(err)?,
            StackProjectionSnapshot::zero(),
            0,
        )
    };
    let last = command.stop_after.unwrap_or(config.updates.len());
    validate_projection_mode(config, &stack)?;
    ensure(
        first_update < last && last <= config.updates.len(),
        "invalid stack start/stop iteration",
    )?;
    let mut policy =
        StackInputPlayPolicyV1::new(base, weights(&stack)?)?.with_mode(config.input_mode);
    policy.install(&legacy.parameters, weights(&stack)?)?;
    let mut device = StackDeviceTrainState::import(
        &legacy,
        &stack,
        &burn_cuda::CudaDevice::new(execution_gpu_ordinal),
    )
    .map_err(err)?;
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "config.json", config)?;
    let mut receipts = Vec::new();
    for update in first_update..last {
        let started = std::time::Instant::now();
        // Observational host wall times only. CUDA work may finish at a later
        // synchronization boundary, so these are not GPU kernel timings.
        // This map never enters the config, trajectory, optimizer or checkpoint.
        let mut stage_seconds = std::collections::BTreeMap::new();
        let directory = command.output_directory.join(format!("{update:04}"));
        fs::create_dir(&directory).map_err(err)?;
        let before_bytes = stack_training::snapshot::encode(&legacy, &stack).map_err(err)?;
        let before = sha(&before_bytes);
        stage_seconds.insert("prepare_and_before_state", started.elapsed().as_secs_f64());
        let mut trajectories = Vec::new();
        let mut trajectory_hashes = Vec::new();
        let mut fork_seconds = 0.0;
        let mut rollout_seconds = 0.0;
        let mut trajectory_publish_seconds = 0.0;
        if command.collector_workers > 1 {
            let collection_started = std::time::Instant::now();
            trajectories = collect_parallel(
                &policy,
                &config.updates[update],
                &config_hash,
                &before,
                config.input_mode,
                command.collector_workers,
                &mut fork_seconds,
            )?;
            rollout_seconds = collection_started.elapsed().as_secs_f64() - fork_seconds;
            let publish_started = std::time::Instant::now();
            for (index, trajectory) in trajectories.iter().enumerate() {
                trajectory_hashes.push(
                    publish_json(&directory, &format!("episode-{index:03}.json"), trajectory)?
                        .sha256,
                );
            }
            trajectory_publish_seconds = publish_started.elapsed().as_secs_f64();
        } else {
            for (index, episode) in config.updates[update].iter().enumerate() {
                let collection_started = std::time::Instant::now();
                eprintln!(
                    "stack update {update}: collect {}/{} {}",
                    index + 1,
                    config.updates[update].len(),
                    episode.id
                );
                let trajectory = collect(
                    &mut policy,
                    episode,
                    &config_hash,
                    &before,
                    config.input_mode,
                )?;
                rollout_seconds += collection_started.elapsed().as_secs_f64();
                let publish_started = std::time::Instant::now();
                trajectory_hashes.push(
                    publish_json(&directory, &format!("episode-{index:03}.json"), &trajectory)?
                        .sha256,
                );
                trajectories.push(trajectory);
                trajectory_publish_seconds += publish_started.elapsed().as_secs_f64();
            }
        }
        let collection_seconds = started.elapsed().as_secs_f64();
        stage_seconds.insert("collector_forks", fork_seconds);
        stage_seconds.insert("rollout_and_join", rollout_seconds);
        stage_seconds.insert("trajectory_publication", trajectory_publish_seconds);
        let replay_started = std::time::Instant::now();
        let mut records: Vec<Vec<(&DecisionRecordV1, StackEncodedDecisionV1)>> = Vec::new();
        let mut raw_advantages = Vec::new();
        let mut value_targets = Vec::new();
        for trajectory in &trajectories {
            let group_start = records.len();
            let mut index = 0;
            while index < trajectory.decisions.len() {
                let first = &trajectory.decisions[index];
                let end = index + first.substep_count as usize;
                if first.actor == trajectory.episode.learner_seat {
                    let mut group = Vec::new();
                    for i in index..end {
                        let row = &trajectory.decisions[i];
                        let auxiliary = trajectory.auxiliary[i]
                            .as_ref()
                            .ok_or("missing learner stack row")?;
                        let tensor = NativeFlatDecisionTensorV4 {
                            common: row.tensor.tensor(),
                        };
                        let captured = StackEncodedDecisionV1 {
                            legacy: tensor,
                            stack: auxiliary.clone(),
                        };
                        let output = policy.replay(&captured)?;
                        ensure(
                            bits(&output.logits) == row.logits
                                && output.value.to_bits() == row.value,
                            "stack rollout replay differs from current learner",
                        )?;
                        group.push((row, captured));
                    }
                    records.push(group);
                }
                index = end;
            }
            ensure(
                records.len() > group_start,
                "episode has no learner physical decisions",
            )?;
            let values: Vec<_> = records[group_start..]
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    (
                        f32::from_bits(g[0].0.value),
                        group_start + i == records.len() - 1,
                    )
                })
                .collect();
            let targets = gae_episode_advantages_v1(
                &values,
                trajectory.terminal.terminal_reward[trajectory.episode.learner_seat as usize]
                    as f32,
                config.gamma,
                config.lambda,
            )?;
            for (advantage, target) in targets {
                raw_advantages.push(advantage);
                value_targets.push(target);
            }
        }
        let (advantages, statistics) = normalize_advantages_v1(&raw_advantages, 1e-6)?;
        let steps: Vec<Vec<_>> = records
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|(row, captured)| StackTrainingStep {
                        decision: captured,
                        selected: row.selected as usize,
                        expected_logits: &row.logits,
                        expected_value: row.value,
                    })
                    .collect()
            })
            .collect();
        let groups: Vec<_> = steps
            .iter()
            .map(|substeps| StackTrainingGroup { substeps })
            .collect();
        stage_seconds.insert(
            "replay_targets_and_grouping",
            replay_started.elapsed().as_secs_f64(),
        );
        let update_started = std::time::Instant::now();
        device
            .update_groups(
                &groups,
                &value_targets,
                &advantages,
                config.learning_rate,
                config.value_coefficient,
                config.entropy_coefficient,
                config.input_mode != StackInputModeV1::Disabled,
                config.max_chunk_substeps,
            )
            .map_err(err)?;
        stage_seconds.insert("device_update_call", update_started.elapsed().as_secs_f64());
        let snapshot_started = std::time::Instant::now();
        (legacy, stack) = device.snapshot().map_err(err)?;
        stage_seconds.insert(
            "device_snapshot_call",
            snapshot_started.elapsed().as_secs_f64(),
        );
        let install_started = std::time::Instant::now();
        validate_projection_mode(config, &stack)?;
        ensure(
            legacy.adam_step == initial_step + update as u64 + 1
                && stack.adam_step == update as u64 + 1,
            "updated optimizer ages differ",
        )?;
        policy.install(&legacy.parameters, weights(&stack)?)?;
        stage_seconds.insert(
            "validate_and_install",
            install_started.elapsed().as_secs_f64(),
        );
        let encode_started = std::time::Instant::now();
        let optimizer = stack_training::snapshot::encode(&legacy, &stack).map_err(err)?;
        stage_seconds.insert("optimizer_encoding", encode_started.elapsed().as_secs_f64());
        let publish_started = std::time::Instant::now();
        let optimizer_hash = publish_bytes(&directory, "optimizer.json", &optimizer)?;
        stage_seconds.insert(
            "optimizer_publication",
            publish_started.elapsed().as_secs_f64(),
        );
        let checkpoint_started = std::time::Instant::now();
        let checkpoint = Checkpoint {
            schema: "mtg-kernel-public-stack-checkpoint/v1".into(),
            config_sha256: config_hash.clone(),
            next_update: update + 1,
            optimizer_file: "optimizer.json".into(),
            optimizer_sha256: optimizer_hash.clone(),
            trajectory_sha256: trajectory_hashes,
        };
        publish_json(&directory, "checkpoint.json", &checkpoint)?;
        stage_seconds.insert(
            "checkpoint_publication",
            checkpoint_started.elapsed().as_secs_f64(),
        );
        let receipt = json!({"update":update,"execution_gpu_ordinal":execution_gpu_ordinal,"collector_workers":command.collector_workers,"episodes":trajectories.len(),"natural_games":trajectories.len(),"learner_groups":groups.len(),"learner_substeps":steps.iter().map(Vec::len).sum::<usize>(),
            "physical_decisions":trajectories.iter().map(|t|t.terminal.physical_decision_count).sum::<u64>(),"before_state_sha256":before,"after_state_sha256":optimizer_hash,
            "legacy_adam_step":legacy.adam_step,"stack_adam_step":stack.adam_step,"advantage_statistics":statistics,"collection_seconds":collection_seconds,"seconds":started.elapsed().as_secs_f64(),
            "stage_seconds":stage_seconds,"stage_timing_semantics":"host_wall/v1; CUDA calls may synchronize later; receipt publication excluded"});
        publish_json(&directory, "receipt.json", &receipt)?;
        receipts.push(receipt);
    }
    let result = json!({"schema":"mtg-kernel-public-stack-run/v1","execution_gpu_ordinal":execution_gpu_ordinal,"collector_workers":command.collector_workers,"config_sha256":config_hash,"first_update":first_update,"next_update":last,"receipts":receipts,
        "non_claim":"Bounded local continuation only; no evaluation, promotion or human-strength claim."});
    publish_json(&command.output_directory, "completion.json", &result)?;
    Ok(result)
}
