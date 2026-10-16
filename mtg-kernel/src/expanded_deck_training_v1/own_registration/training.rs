//! One synchronous ordinary-GAE update per frozen collection batch.
use super::*;
use crate::experimental_burn_net8_packed_v1::own_registration_training::{
    self as device_training, ProjectionSnapshot, RegistrationDeviceTrainState,
    RegistrationTrainingGroup, RegistrationTrainingStep,
};
use crate::native_flat_tensorizer_v4::encoded_decision_view_v4;
use crate::own_registration_v1::{contract_identity_v1, RegistrationExpectationV1};

const CHECKPOINT_SCHEMA: &str = "mtg-kernel-own-registration-checkpoint/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    Own,
    Mean,
    Baseline,
}
impl Arm {
    fn mode(self) -> RegistrationModeV1 {
        match self {
            Self::Mean => RegistrationModeV1::Mean,
            Self::Own | Self::Baseline => RegistrationModeV1::Own,
        }
    }
    fn enabled(self) -> bool {
        self != Self::Baseline
    }
    fn initial(self) -> Result<ProjectionSnapshot, String> {
        if self.enabled() {
            ProjectionSnapshot::zero(self.mode()).map_err(err)
        } else {
            ProjectionSnapshot::baseline().map_err(err)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub source: ExpandedModelSourceV1,
    pub updates: Vec<Vec<ExpandedEpisodeV1>>,
    pub arm: Arm,
    pub learning_rate: f32,
    pub value_coefficient: f32,
    pub gamma: f32,
    pub lambda: f32,
    pub entropy_coefficient: f32,
    pub training_math: TrainingMathV1,
    pub gpu_ordinal: usize,
    pub max_chunk_substeps: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub config: Config,
    pub output_directory: PathBuf,
    pub resume: Option<PinnedFileV1>,
    pub stop_after: Option<usize>,
    pub collector_workers: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    config_sha256: String,
    initial_state_sha256: String,
    next_update: usize,
    optimizer_file: String,
    optimizer_sha256: String,
    trajectory_sha256: Vec<String>,
}

fn validate_parent_recipe(config: &Config, saved: &Value) -> Result<(), String> {
    ensure(
        saved["schema"] == FRESH_CHECKPOINT_SCHEMA
            && saved["loss_identity"] == GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
        "registration continuation requires an ordinary fresh-lineage GAE checkpoint",
    )?;
    for (key, expected) in [
        ("learning_rate_bits", config.learning_rate),
        ("value_coefficient_bits", config.value_coefficient),
        ("gamma_bits", config.gamma),
        ("gae_lambda_bits", config.lambda),
        ("entropy_coefficient_bits", config.entropy_coefficient),
    ] {
        ensure(
            saved[key].as_u64() == Some(u64::from(expected.to_bits())),
            &format!("registration continuation changes inherited {key}"),
        )?;
    }
    let math: TrainingMathV1 = match saved.get("training_math") {
        None => TrainingMathV1::default(),
        Some(value) => serde_json::from_value(value.clone()).map_err(err)?,
    };
    ensure(
        math == config.training_math,
        "registration continuation changes training math",
    )
}

fn validate_config(config: &Config) -> Result<(), String> {
    ensure(
        config.learning_rate.to_bits() == 0.0001f32.to_bits()
            && config.value_coefficient.to_bits() == 0.5f32.to_bits()
            && config.gamma.to_bits() == 1f32.to_bits()
            && config.lambda.to_bits() == 0.9f32.to_bits()
            && config.entropy_coefficient.to_bits() == 0f32.to_bits()
            && config.training_math == TrainingMathV1::PlatformLibm,
        "registration successor preserves the ordinary GAE recipe",
    )?;
    ensure(
        config.gpu_ordinal == 1 && (1..=512).contains(&config.max_chunk_substeps),
        "registration GPU ordinal or chunk bound differs",
    )?;
    ensure(
        !config.updates.is_empty() && config.updates.len() <= 256,
        "registration update count outside 1..256",
    )?;
    let mut ids = BTreeSet::new();
    for episodes in &config.updates {
        ensure(
            !episodes.is_empty() && episodes.len() <= 64,
            "registration batch outside 1..64",
        )?;
        for episode in episodes {
            episode.configurations()?;
            ensure(
                ids.insert(&episode.id),
                "registration training repeats an episode id",
            )?;
        }
    }
    let parent = config
        .source
        .checkpoint
        .as_ref()
        .ok_or("registration continuation requires full parent Adam state")?;
    let saved: Value = read_pinned(parent)?;
    validate_parent_recipe(config, &saved)
}

fn validate_projection(config: &Config, projection: &ProjectionSnapshot) -> Result<(), String> {
    projection.validate().map_err(err)?;
    ensure(
        projection.mode == config.arm.mode()
            && projection.projection_enabled == config.arm.enabled(),
        "registration snapshot arm differs from config",
    )
}

fn validate_ages(
    config: &Config,
    legacy: &NativePolicyValueTrainSnapshotV1,
    projection: &ProjectionSnapshot,
    initial_step: u64,
    completed: usize,
) -> Result<(), String> {
    validate_projection(config, projection)?;
    let expected_base = initial_step
        .checked_add(completed as u64)
        .ok_or("registration base age overflow")?;
    ensure(
        legacy.adam_step == expected_base
            && projection.adam_step
                == if config.arm.enabled() {
                    completed as u64
                } else {
                    0
                },
        "registration optimizer ages differ",
    )
}

fn restore(
    config: &Config,
    config_sha256: &str,
    pin: &PinnedFileV1,
    initial: &NativePolicyValueTrainSnapshotV1,
) -> Result<(NativePolicyValueTrainSnapshotV1, ProjectionSnapshot, usize), String> {
    let checkpoint: Checkpoint = read_pinned(pin)?;
    ensure(
        checkpoint.schema == CHECKPOINT_SCHEMA
            && checkpoint.config_sha256 == config_sha256
            && checkpoint.initial_state_sha256 == hex(&initial.state_sha256_v1().map_err(err)?)
            && checkpoint.optimizer_file == "optimizer.json"
            && checkpoint.next_update > 0
            && checkpoint.next_update <= config.updates.len()
            && checkpoint.trajectory_sha256.len()
                == config.updates[checkpoint.next_update - 1].len(),
        "registration resume checkpoint binding differs",
    )?;
    let optimizer_pin = PinnedFileV1 {
        path: pin
            .path
            .parent()
            .ok_or("registration checkpoint has no parent")?
            .join("optimizer.json"),
        sha256: checkpoint.optimizer_sha256,
    };
    let (legacy, projection) =
        device_training::snapshot::decode(&read_pinned_bytes(&optimizer_pin)?).map_err(err)?;
    validate_ages(
        config,
        &legacy,
        &projection,
        initial.adam_step,
        checkpoint.next_update,
    )?;
    Ok((legacy, projection, checkpoint.next_update))
}

fn policy(
    base: FrozenPlayPolicyV1,
    legacy: &NativePolicyValueTrainSnapshotV1,
    projection: &ProjectionSnapshot,
) -> Result<OwnRegistrationPlayPolicyV1, String> {
    let mut policy = if projection.projection_enabled {
        OwnRegistrationPlayPolicyV1::new(
            base,
            projection.input_weights().map_err(err)?,
            projection.mode,
        )?
    } else {
        OwnRegistrationPlayPolicyV1::new_legacy(base)?
    };
    policy.install(&legacy.parameters, projection.input_weights().map_err(err)?)?;
    Ok(policy)
}

/// This loader uses only CPU data and policy construction, even though the
/// checkpoint was produced by the CUDA optimizer.
pub(super) fn load_for_evaluation(
    config_pin: &PinnedFileV1,
    checkpoint_pin: &PinnedFileV1,
) -> Result<(OwnRegistrationPlayPolicyV1, Value, RegistrationModeV1, bool), String> {
    let config: Config = read_pinned(config_pin)?;
    validate_config(&config)?;
    let config_sha256 = sha(&serde_json::to_vec(&config).map_err(err)?);
    let (base, initial) = initialize(&config.source)?;
    let initial = initial.snapshot_v1().map_err(err)?;
    let (legacy, projection, completed) =
        restore(&config, &config_sha256, checkpoint_pin, &initial)?;
    let identity = json!({
        "schema":"own-registration-evaluation-model/v1",
        "architecture":crate::native_policy_value_net_v1::own_registration::ARCHITECTURE,
        "config_sha256":config_sha256,
        "checkpoint_sha256":checkpoint_pin.sha256,
        "optimizer_sha256":sha(&device_training::snapshot::encode(&legacy, &projection).map_err(err)?),
        "feature_contract":contract_identity_v1()?,
        "arm":config.arm,"completed_updates":completed,
        "base_adam_step":legacy.adam_step,"projection_adam_step":projection.adam_step,
        "training_math":config.training_math,
    });
    Ok((
        policy(base, &legacy, &projection)?,
        identity,
        config.arm.mode(),
        config.arm.enabled(),
    ))
}

pub fn run(command: Command) -> Result<Value, String> {
    validate_config(&command.config)?;
    validate_collection_workers_v1(command.collector_workers)?;
    ensure(
        !command.output_directory.exists(),
        "registration training requires a fresh output directory",
    )?;
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "command.json", &command)?;
    publish_json(&command.output_directory, "config.json", &command.config)?;
    let result = run_updates(&command);
    if let Err(error) = &result {
        publish_json(
            &command.output_directory,
            "failure.json",
            &json!({
                "schema":"own-registration-training-failure/v1", "error":error,
            }),
        )?;
    }
    result
}

fn run_updates(command: &Command) -> Result<Value, String> {
    let config = &command.config;
    let config_sha256 = sha(&serde_json::to_vec(config).map_err(err)?);
    let (base, initial) = initialize(&config.source)?;
    ensure(
        base.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
        "registration source must be V4",
    )?;
    ensure(
        base.training_math_v1() == config.training_math,
        "registration loaded math differs",
    )?;
    let initial = initial.snapshot_v1().map_err(err)?;
    let initial_state_sha256 = hex(&initial.state_sha256_v1().map_err(err)?);
    let (mut legacy, mut projection, first) = match &command.resume {
        Some(pin) => restore(config, &config_sha256, pin, &initial)?,
        None => (initial.clone(), config.arm.initial()?, 0),
    };
    let last = command.stop_after.unwrap_or(config.updates.len());
    ensure(
        first < last && last <= config.updates.len(),
        "registration update range invalid",
    )?;
    validate_ages(config, &legacy, &projection, initial.adam_step, first)?;
    let mut policy = policy(base, &legacy, &projection)?;
    let mut device = RegistrationDeviceTrainState::import(
        &legacy,
        &projection,
        &burn_cuda::CudaDevice::new(config.gpu_ordinal),
    )
    .map_err(err)?;
    let mut checkpoint_hashes = Vec::new();
    for update in first..last {
        let started = std::time::Instant::now();
        let directory = command.output_directory.join(format!("{update:04}"));
        fs::create_dir(&directory).map_err(err)?;
        let before = sha(&device_training::snapshot::encode(&legacy, &projection).map_err(err)?);
        let trajectories = collect_parallel(
            &policy,
            &config.updates[update],
            &config_sha256,
            &before,
            config.arm.mode(),
            config.arm.enabled(),
            command.collector_workers,
        )?;
        let collection_seconds = started.elapsed().as_secs_f64();
        let trajectory_sha256 = super::super::public_features::ordered_parallel_map(
            &trajectories,
            command.collector_workers,
            |index, trajectory| {
                Ok(
                    publish_json(&directory, &format!("episode-{index:03}.json"), trajectory)?
                        .sha256,
                )
            },
        )?;
        let replayed = super::super::public_features::ordered_parallel_map(
            &trajectories,
            command.collector_workers,
            |_, trajectory| {
                trajectory.validate_records()?;
                let configs = trajectory.episode.configurations()?;
                let groups = learner_groups(trajectory)?;
                for group in &groups {
                    for (row, tensor, auxiliary) in group {
                        let expected = RegistrationExpectationV1 {
                            external_episode_id: &trajectory.episode.id,
                            actor: if row.actor == 0 {
                                PlayerSeatV1::P0
                            } else {
                                PlayerSeatV1::P1
                            },
                            selected_mainboard_sha256: configs[row.actor as usize]
                                .mainboard_sha256_v1(),
                        };
                        let output = policy.replay(tensor, auxiliary, expected)?;
                        ensure(
                            bits(&output.logits) == row.logits
                                && output.value.to_bits() == row.value,
                            "registration stored-row replay differs from frozen learner",
                        )?;
                    }
                }
                Ok(groups)
            },
        )?;
        let mut records = Vec::new();
        let mut expectations = Vec::new();
        let mut raw_advantages = Vec::new();
        let mut value_targets = Vec::new();
        for (trajectory, groups) in trajectories.iter().zip(replayed) {
            ensure(
                !groups.is_empty(),
                "registration episode has no learner groups",
            )?;
            let configs = trajectory.episode.configurations()?;
            let values: Vec<_> = groups
                .iter()
                .enumerate()
                .map(|(i, group)| (f32::from_bits(group[0].0.value), i + 1 == groups.len()))
                .collect();
            for (advantage, target) in gae_episode_advantages_v1(
                &values,
                trajectory.terminal.terminal_reward[trajectory.episode.learner_seat as usize]
                    as f32,
                config.gamma,
                config.lambda,
            )? {
                raw_advantages.push(advantage);
                value_targets.push(target);
            }
            for group in groups {
                let actor = group[0].0.actor;
                expectations.push(RegistrationExpectationV1 {
                    external_episode_id: &trajectory.episode.id,
                    actor: if actor == 0 {
                        PlayerSeatV1::P0
                    } else {
                        PlayerSeatV1::P1
                    },
                    selected_mainboard_sha256: configs[actor as usize].mainboard_sha256_v1(),
                });
                records.push(group);
            }
        }
        let (advantages, statistics) = normalize_advantages_v1(&raw_advantages, 1e-6)?;
        let steps: Vec<Vec<_>> = records
            .iter()
            .zip(&expectations)
            .map(|(group, expected)| {
                group
                    .iter()
                    .map(|(row, tensor, auxiliary)| RegistrationTrainingStep {
                        view: encoded_decision_view_v4(tensor),
                        auxiliary,
                        expected: *expected,
                        selected: row.selected as usize,
                        expected_logits: &row.logits,
                        expected_value: row.value,
                    })
                    .collect()
            })
            .collect();
        let groups: Vec<_> = steps
            .iter()
            .map(|substeps| RegistrationTrainingGroup { substeps })
            .collect();
        let preparation_seconds = started.elapsed().as_secs_f64() - collection_seconds;
        device
            .update_groups(
                &groups,
                &value_targets,
                &advantages,
                config.learning_rate,
                config.value_coefficient,
                config.entropy_coefficient,
                config.max_chunk_substeps,
            )
            .map_err(err)?;
        (legacy, projection) = device.snapshot().map_err(err)?;
        validate_ages(config, &legacy, &projection, initial.adam_step, update + 1)?;
        policy.install(&legacy.parameters, projection.input_weights().map_err(err)?)?;
        let optimizer = device_training::snapshot::encode(&legacy, &projection).map_err(err)?;
        publish_json_bytes_v1(&directory, "optimizer.json", &optimizer)?;
        let checkpoint = Checkpoint {
            schema: CHECKPOINT_SCHEMA.into(),
            config_sha256: config_sha256.clone(),
            initial_state_sha256: initial_state_sha256.clone(),
            next_update: update + 1,
            optimizer_file: "optimizer.json".into(),
            optimizer_sha256: sha(&optimizer),
            trajectory_sha256,
        };
        let pin = publish_json(&directory, "checkpoint.json", &checkpoint)?;
        checkpoint_hashes.push(pin.sha256);
        publish_json(
            &directory,
            "receipt.json",
            &json!({
                "schema":"own-registration-update-receipt/v1","update":update,
                "before_optimizer_sha256":before,"optimizer_sha256":checkpoint.optimizer_sha256,
                "learner_groups":groups.len(),"advantages":statistics,
                "base_adam_step":legacy.adam_step,"projection_adam_step":projection.adam_step,
            }),
        )?;
        publish_json(
            &directory,
            "timing.json",
            &json!({
                "collection_seconds":collection_seconds,"preparation_seconds":preparation_seconds,
                "wall_seconds":started.elapsed().as_secs_f64(),"collector_workers":command.collector_workers,
            }),
        )?;
    }
    let result = json!({
        "schema":"own-registration-training-completion/v1","config_sha256":config_sha256,
        "first_update":first,"next_update":last,"checkpoint_sha256":checkpoint_hashes,
        "base_adam_step":legacy.adam_step,"projection_adam_step":projection.adam_step,
    });
    publish_json(&command.output_directory, "completion.json", &result)?;
    Ok(result)
}
