//! Trainer-compatible in-memory runner for one validated native checkpoint.
//!
//! The caller selects only a fresh evaluation seed, a complete even/odd seat
//! pair range, and deadline diagnostics. Decks, limits, and execution topology
//! come from the validated training run. The existing native trainer schedule
//! derives environment, learner-seat, opponent, and learner-action seeds; the
//! existing checkpoint adapter supplies immutable model inference. This module
//! adds no artifact schema, filesystem access, evaluator statistic, or seed
//! derivation.

use crate::async_flat_scored_rollout_v2::{
    run_async_flat_scored_rollout_native_observed_v2, AsyncFlatScoredObservedRunErrorV2,
    AsyncFlatScoredRolloutErrorV2, AsyncFlatScoredRolloutResultV2, FlatScoredSelectedEventV2,
    FlatScoredTerminalEventV2, FlatScoredTrajectoryObserverV2,
};
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::native_checkpoint_inference_v1::{
    load_native_checkpoint_inference_v1, NativeCheckpointInferenceErrorV1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, parse_lower_hex_raw32_v1};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::rl::PlayerSeatV1;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::time::{Duration, Instant};

/// Runtime deadline ceiling for one in-process runner call. Harder or longer
/// enforcement belongs at the process boundary, not inside this cooperative
/// scheduler.
pub const NATIVE_CHECKPOINT_RUNNER_MAX_TIMEOUT_V1: Duration = Duration::from_secs(86_400);

/// Runtime-only evaluation inputs not already frozen by the training run.
///
/// `first_episode_index` and `episode_count` must both be even, and the count
/// must be positive. This admits only complete native schedule seat pairs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCheckpointRunnerConfigV1 {
    pub evaluation_base_seed: u64,
    pub first_episode_index: u64,
    pub episode_count: u64,
    pub scheduler_timeout: Duration,
    pub measure_broker_service_time: bool,
}

/// Native-schedule facts observed from one completed engine trajectory.
///
/// These are runtime facts, not an artifact authority. Private fields prevent
/// callers from accidentally presenting hand-built values as runner output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCheckpointRunnerEpisodeV1 {
    episode_index: u64,
    environment_seed: u64,
    deck_hashes: [u64; 2],
    learner_seat: PlayerSeatV1,
    trajectory_sha256: [u8; 32],
    policy_step_count: u64,
    physical_decision_count: u64,
    learner_policy_step_count: u64,
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_physical_decision_count: u64,
}

impl NativeCheckpointRunnerEpisodeV1 {
    pub const fn episode_index(&self) -> u64 {
        self.episode_index
    }

    pub const fn environment_seed(&self) -> u64 {
        self.environment_seed
    }

    pub const fn deck_hashes(&self) -> [u64; 2] {
        self.deck_hashes
    }

    pub const fn learner_seat(&self) -> PlayerSeatV1 {
        self.learner_seat
    }

    pub const fn trajectory_sha256(&self) -> [u8; 32] {
        self.trajectory_sha256
    }

    pub const fn policy_step_count(&self) -> u64 {
        self.policy_step_count
    }

    pub const fn physical_decision_count(&self) -> u64 {
        self.physical_decision_count
    }

    pub const fn learner_policy_step_count(&self) -> u64 {
        self.learner_policy_step_count
    }

    pub const fn opponent_policy_step_count(&self) -> u64 {
        self.opponent_policy_step_count
    }

    pub const fn learner_physical_decision_count(&self) -> u64 {
        self.learner_physical_decision_count
    }

    pub const fn opponent_physical_decision_count(&self) -> u64 {
        self.opponent_physical_decision_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeCheckpointRunnerErrorV1 {
    InvalidConfig,
    Inference(NativeCheckpointInferenceErrorV1),
    Rollout(AsyncFlatScoredRolloutErrorV2),
    Protocol,
}

impl NativeCheckpointRunnerErrorV1 {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig => "native_checkpoint_runner_v1_invalid_config",
            Self::Inference(error) => error.code(),
            Self::Rollout(_) => "native_checkpoint_runner_v1_rollout",
            Self::Protocol => "native_checkpoint_runner_v1_protocol",
        }
    }
}

impl Display for NativeCheckpointRunnerErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeCheckpointRunnerErrorV1 {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Inference(error) => Some(error),
            Self::Rollout(error) => Some(error),
            Self::InvalidConfig | Self::Protocol => None,
        }
    }
}

impl From<NativeCheckpointInferenceErrorV1> for NativeCheckpointRunnerErrorV1 {
    fn from(error: NativeCheckpointInferenceErrorV1) -> Self {
        Self::Inference(error)
    }
}

/// A successful natural rollout inseparably bound to its validated run,
/// checkpoint, evaluation seed range, and execution topology.
///
/// Fields are private and the value is deliberately not serializable. A later
/// artifact layer may encode these facts only after defining and validating an
/// explicit runner-record contract.
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_runner_v1::NativeCheckpointRunResultV1;
/// let _ = NativeCheckpointRunResultV1 {};
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_runner_v1::NativeCheckpointRunResultV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<NativeCheckpointRunResultV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_runner_v1::NativeCheckpointRunResultV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<NativeCheckpointRunResultV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_runner_v1::NativeCheckpointRunResultV1;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<NativeCheckpointRunResultV1>();
/// ```
pub struct NativeCheckpointRunResultV1 {
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    logical_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    config: NativeCheckpointRunnerConfigV1,
    worker_count: usize,
    sessions_per_worker: usize,
    broker_batch_target: usize,
    episode_bindings: Vec<NativeCheckpointRunnerEpisodeV1>,
    rollout: AsyncFlatScoredRolloutResultV2,
}

impl Debug for NativeCheckpointRunResultV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointRunResultV1")
            .field("run_sha256", &lower_hex_raw32_v1(self.run_sha256))
            .field(
                "checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.checkpoint_manifest_sha256),
            )
            .field("generation_index", &self.generation_index)
            .field("evaluation_base_seed", &self.config.evaluation_base_seed)
            .field("first_episode_index", &self.config.first_episode_index)
            .field("episode_count", &self.config.episode_count)
            .field("worker_count", &self.worker_count)
            .field("sessions_per_worker", &self.sessions_per_worker)
            .field("broker_batch_target", &self.broker_batch_target)
            .finish_non_exhaustive()
    }
}

impl NativeCheckpointRunResultV1 {
    pub const fn run_sha256(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub const fn identity_bundle_sha256(&self) -> [u8; 32] {
        self.identity_bundle_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }

    pub const fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.checkpoint_payload_sha256
    }

    pub const fn logical_state_sha256(&self) -> [u8; 32] {
        self.logical_state_sha256
    }

    pub const fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub const fn train_state_sha256(&self) -> [u8; 32] {
        self.train_state_sha256
    }

    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub const fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub const fn checkpoint_segment_updates(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub const fn config(&self) -> NativeCheckpointRunnerConfigV1 {
        self.config
    }

    pub const fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub const fn sessions_per_worker(&self) -> usize {
        self.sessions_per_worker
    }

    pub const fn broker_batch_target(&self) -> usize {
        self.broker_batch_target
    }

    pub fn episode_bindings(&self) -> &[NativeCheckpointRunnerEpisodeV1] {
        &self.episode_bindings
    }

    pub const fn rollout(&self) -> &AsyncFlatScoredRolloutResultV2 {
        &self.rollout
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeCheckpointRunnerObserverErrorV1 {
    MissingNativeReceipt,
    ScheduleMismatch,
    TerminalMismatch,
    ReceiptInvariant,
    DuplicateEpisode,
}

struct NativeCheckpointRunnerObserverV1 {
    evaluation_base_seed: u64,
    first_episode_index: u64,
    end_episode_index_exclusive: u64,
    expected_deck_hashes: [u64; 2],
    expected_episode_count: usize,
    episode_bindings: Vec<NativeCheckpointRunnerEpisodeV1>,
}

impl NativeCheckpointRunnerObserverV1 {
    fn new_v1(
        evaluation_base_seed: u64,
        first_episode_index: u64,
        end_episode_index_exclusive: u64,
        expected_deck_hashes: [u64; 2],
        expected_episode_count: usize,
    ) -> Result<Self, NativeCheckpointRunnerErrorV1> {
        let mut episode_bindings = Vec::new();
        episode_bindings
            .try_reserve_exact(expected_episode_count)
            .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?;
        Ok(Self {
            evaluation_base_seed,
            first_episode_index,
            end_episode_index_exclusive,
            expected_deck_hashes,
            expected_episode_count,
            episode_bindings,
        })
    }
}

impl FlatScoredTrajectoryObserverV2 for NativeCheckpointRunnerObserverV1 {
    type Error = NativeCheckpointRunnerObserverErrorV1;
    type Output = Vec<NativeCheckpointRunnerEpisodeV1>;

    const OBSERVES_TRAJECTORY: bool = true;

    fn observe_selected_v2(
        &mut self,
        _event: FlatScoredSelectedEventV2<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn observe_terminal_v2(&mut self, event: FlatScoredTerminalEventV2) -> Result<(), Self::Error> {
        let receipt = event
            .native_full_trajectory_receipt
            .ok_or(NativeCheckpointRunnerObserverErrorV1::MissingNativeReceipt)?;
        let schedule =
            native_trainer_episode_schedule_v1(self.evaluation_base_seed, receipt.episode_index)
                .map_err(|_| NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch)?;
        if schedule.environment_seed != receipt.environment_seed
            || schedule.learner_seat != receipt.learner_seat
            || !(self.first_episode_index..self.end_episode_index_exclusive)
                .contains(&receipt.episode_index)
            || receipt.deck_hashes != self.expected_deck_hashes
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch);
        }
        if event.terminal.episode_id != receipt.episode_index
            || event.terminal.policy_step_count != receipt.policy_step_count
            || event.terminal.physical_decision_count != receipt.physical_decision_count
            || event.learner_action_count != receipt.learner_policy_step_count
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::TerminalMismatch);
        }
        if receipt
            .learner_policy_step_count
            .checked_add(receipt.opponent_policy_step_count)
            != Some(receipt.policy_step_count)
            || receipt
                .learner_physical_decision_count
                .checked_add(receipt.opponent_physical_decision_count)
                != Some(receipt.physical_decision_count)
            || self.episode_bindings.len() >= self.expected_episode_count
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant);
        }
        self.episode_bindings.push(NativeCheckpointRunnerEpisodeV1 {
            episode_index: receipt.episode_index,
            environment_seed: receipt.environment_seed,
            deck_hashes: receipt.deck_hashes,
            learner_seat: receipt.learner_seat,
            trajectory_sha256: receipt.trajectory_sha256,
            policy_step_count: receipt.policy_step_count,
            physical_decision_count: receipt.physical_decision_count,
            learner_policy_step_count: receipt.learner_policy_step_count,
            opponent_policy_step_count: receipt.opponent_policy_step_count,
            learner_physical_decision_count: receipt.learner_physical_decision_count,
            opponent_physical_decision_count: receipt.opponent_physical_decision_count,
        });
        Ok(())
    }

    fn finish_v2(mut self) -> Result<Self::Output, Self::Error> {
        self.episode_bindings
            .sort_unstable_by_key(|binding| binding.episode_index);
        if self
            .episode_bindings
            .windows(2)
            .any(|pair| pair[0].episode_index == pair[1].episode_index)
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::DuplicateEpisode);
        }
        Ok(self.episode_bindings)
    }
}

/// Runs one validated checkpoint against the frozen native uniform opponent.
///
/// Validation of the cheap runtime/range inputs precedes the 14 MiB payload
/// decode. No model or rollout is constructed on an invalid configuration.
pub fn run_native_checkpoint_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    let topology = validate_runner_config_v1(run, config)?;
    let expected_episode_count = usize::try_from(config.episode_count)
        .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    let end_episode_index_exclusive = config
        .first_episode_index
        .checked_add(config.episode_count)
        .ok_or(NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    let deck_hashes_hex = run.record().environment().deck_hashes_u64_hex();
    let expected_deck_hashes = [
        u64::from_str_radix(&deck_hashes_hex[0], 16)
            .map_err(|_| NativeCheckpointRunnerErrorV1::Protocol)?,
        u64::from_str_radix(&deck_hashes_hex[1], 16)
            .map_err(|_| NativeCheckpointRunnerErrorV1::Protocol)?,
    ];
    let observer = NativeCheckpointRunnerObserverV1::new_v1(
        config.evaluation_base_seed,
        config.first_episode_index,
        end_episode_index_exclusive,
        expected_deck_hashes,
        expected_episode_count,
    )?;
    let inference = load_native_checkpoint_inference_v1(run, checkpoint, checkpoint_payload)?;
    let identity_bundle_sha256 = parse_lower_hex_raw32_v1(run.identity_bundle_sha256())
        .map_err(|_| NativeCheckpointRunnerErrorV1::Protocol)?;
    let rollout_config = AsyncRolloutConfigV2 {
        deck_ids: [
            run.record().environment().deck_ids()[0].clone(),
            run.record().environment().deck_ids()[1].clone(),
        ],
        // The native schedule replaces this placeholder on every episode.
        learner_seat: PlayerSeatV1::P0,
        // The native schedule is the only consumer of these seed roles. Keep
        // all legacy placeholders equal to the one explicit evaluation seed,
        // matching the existing trainer construction.
        environment_seed: config.evaluation_base_seed,
        opponent_policy_seed: config.evaluation_base_seed,
        learner_policy_seed: config.evaluation_base_seed,
        max_physical_decisions: run.record().limits().max_physical_decisions(),
        max_policy_steps: run.record().limits().max_policy_steps(),
        worker_count: topology.0,
        sessions_per_worker: topology.1,
        broker_batch_target: topology.2,
        first_episode_id: config.first_episode_index,
        episode_count: config.episode_count,
        scheduler_timeout: config.scheduler_timeout,
        measure_broker_service_time: config.measure_broker_service_time,
    };
    let mut scorer = inference.batch_scorer_v1();
    let observed = run_async_flat_scored_rollout_native_observed_v2(
        rollout_config,
        config.evaluation_base_seed,
        &mut scorer,
        observer,
    );
    drop(scorer);
    let (rollout, episode_bindings) = match observed {
        Ok((rollout, episode_bindings)) => (rollout, episode_bindings),
        Err(AsyncFlatScoredObservedRunErrorV2::Rollout(error)) => {
            return Err(NativeCheckpointRunnerErrorV1::Rollout(error));
        }
        Err(AsyncFlatScoredObservedRunErrorV2::ObserverFailed { .. }) => {
            return Err(NativeCheckpointRunnerErrorV1::Protocol);
        }
        Err(AsyncFlatScoredObservedRunErrorV2::ObserverPanicked { .. }) => {
            return Err(NativeCheckpointRunnerErrorV1::Protocol);
        }
    };
    if rollout.episodes.len() != expected_episode_count
        || episode_bindings.len() != expected_episode_count
        || !rollout.all_natural()
        || rollout
            .episodes
            .iter()
            .zip(&episode_bindings)
            .any(|(episode, binding)| episode.terminal.episode_id != binding.episode_index)
        || episode_bindings
            .iter()
            .enumerate()
            .any(|(offset, binding)| {
                u64::try_from(offset)
                    .ok()
                    .and_then(|offset| config.first_episode_index.checked_add(offset))
                    != Some(binding.episode_index)
            })
    {
        return Err(NativeCheckpointRunnerErrorV1::Protocol);
    }
    Ok(NativeCheckpointRunResultV1 {
        run_sha256: inference.run_sha256(),
        identity_bundle_sha256,
        checkpoint_manifest_sha256: inference.checkpoint_manifest_sha256(),
        checkpoint_payload_sha256: inference.checkpoint_payload_sha256(),
        logical_state_sha256: checkpoint.logical_state_sha256(),
        model_parameter_sha256: inference.model_parameter_sha256(),
        train_state_sha256: inference.train_state_sha256(),
        generation_index: inference.generation_index(),
        batch_episodes: checkpoint.batch_episodes(),
        checkpoint_segment_updates: checkpoint.checkpoint_segment_updates(),
        config,
        worker_count: topology.0,
        sessions_per_worker: topology.1,
        broker_batch_target: topology.2,
        episode_bindings,
        rollout,
    })
}

fn validate_runner_config_v1(
    run: &ValidatedTrainRunV2,
    config: NativeCheckpointRunnerConfigV1,
) -> Result<(usize, usize, usize), NativeCheckpointRunnerErrorV1> {
    let end = config
        .first_episode_index
        .checked_add(config.episode_count)
        .ok_or(NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    if config.scheduler_timeout.is_zero()
        || config.scheduler_timeout > NATIVE_CHECKPOINT_RUNNER_MAX_TIMEOUT_V1
        || Instant::now()
            .checked_add(config.scheduler_timeout)
            .is_none()
        || !config.first_episode_index.is_multiple_of(2)
        || config.episode_count == 0
        || !config.episode_count.is_multiple_of(2)
        || native_trainer_episode_schedule_v1(
            config.evaluation_base_seed,
            config.first_episode_index,
        )
        .is_err()
        || native_trainer_episode_schedule_v1(config.evaluation_base_seed, end - 1).is_err()
    {
        return Err(NativeCheckpointRunnerErrorV1::InvalidConfig);
    }
    let topology = run.record().topology();
    let worker_count = usize::try_from(topology.worker_count())
        .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    let sessions_per_worker = usize::try_from(topology.sessions_per_worker())
        .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    let broker_batch_target = usize::try_from(topology.broker_batch_target())
        .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    let logical_actor_count = worker_count
        .checked_mul(sessions_per_worker)
        .ok_or(NativeCheckpointRunnerErrorV1::InvalidConfig)?;
    if worker_count == 0
        || sessions_per_worker == 0
        || !(1..=logical_actor_count).contains(&broker_batch_target)
        || u64::try_from(logical_actor_count).ok() != Some(topology.logical_actor_count())
    {
        return Err(NativeCheckpointRunnerErrorV1::InvalidConfig);
    }
    Ok((worker_count, sessions_per_worker, broker_batch_target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_checkpoint_inference_v1::NativeCheckpointInferenceErrorKindV1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
        decode_genesis_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1,
    };
    use std::sync::OnceLock;

    struct RunnerFixtureV1 {
        run_bytes: Vec<u8>,
        checkpoint_bytes: Vec<u8>,
        payload: Vec<u8>,
    }

    struct TrainedRunnerFixtureV1 {
        checkpoint: CheckpointManifestV3,
        payload: Vec<u8>,
    }

    static RUNNER_FIXTURE_V1: OnceLock<RunnerFixtureV1> = OnceLock::new();
    static TRAINED_RUNNER_FIXTURE_V1: OnceLock<TrainedRunnerFixtureV1> = OnceLock::new();

    fn execution_config_v1(run: &ValidatedTrainRunV2) -> NativeTrainingExecutionConfigV1 {
        NativeTrainingExecutionConfigV1 {
            run_base_seed: run.record().schedule().base_seed(),
            batch_episodes: run.batch_episodes(),
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            max_physical_decisions: run.record().limits().max_physical_decisions(),
            max_policy_steps: run.record().limits().max_policy_steps(),
            worker_count: usize::try_from(run.record().topology().worker_count()).unwrap(),
            sessions_per_worker: usize::try_from(run.record().topology().sessions_per_worker())
                .unwrap(),
            broker_batch_target: usize::try_from(run.record().topology().broker_batch_target())
                .unwrap(),
            scheduler_timeout: Duration::from_secs(30),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5_f32.to_bits(),
            learning_rate_bits: 0.001_f32.to_bits(),
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    fn fresh_executor_v1(run: &ValidatedTrainRunV2) -> NativeTrainingExecutorV1 {
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap()
    }

    fn fixture_v1() -> &'static RunnerFixtureV1 {
        RUNNER_FIXTURE_V1.get_or_init(|| {
            let run_bytes = test_fixture_bytes_v2();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let executor = fresh_executor_v1(&run);
            let candidate = executor.checkpoint_candidate_v1().unwrap();
            let payload = candidate.payload().to_vec();
            let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
            RunnerFixtureV1 {
                run_bytes,
                checkpoint_bytes: checkpoint.canonical_bytes().to_vec(),
                payload,
            }
        })
    }

    fn trained_fixture_v1() -> &'static TrainedRunnerFixtureV1 {
        TRAINED_RUNNER_FIXTURE_V1.get_or_init(|| {
            let genesis_fixture = fixture_v1();
            let run = decode_train_run_v2(&genesis_fixture.run_bytes).unwrap();
            let genesis = decode_genesis_checkpoint_manifest_v3(
                &genesis_fixture.checkpoint_bytes,
                &genesis_fixture.payload,
                &run,
            )
            .unwrap();
            let mut context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
            let mut executor = fresh_executor_v1(&run);
            let update_count = usize::try_from(run.checkpoint_segment_updates()).unwrap();
            let mut final_candidate = None;
            for update_ordinal in 0..update_count {
                let prepared = executor.prepare_update_v2().unwrap();
                let built = build_update_group_v1(&run, context, &prepared).unwrap();
                final_candidate = Some(prepared.checkpoint_candidate().clone());
                context = built.into_parts().1;
                drop(prepared);
                if update_ordinal + 1 < update_count {
                    executor.run_update_v2().unwrap();
                }
            }
            let final_candidate = final_candidate.unwrap();
            let checkpoint =
                build_trained_checkpoint_manifest_v3(&run, &context, &final_candidate).unwrap();
            TrainedRunnerFixtureV1 {
                checkpoint,
                payload: final_candidate.payload().to_vec(),
            }
        })
    }

    fn authorities_v1() -> (ValidatedTrainRunV2, CheckpointManifestV3) {
        let fixture = fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let checkpoint = decode_genesis_checkpoint_manifest_v3(
            &fixture.checkpoint_bytes,
            &fixture.payload,
            &run,
        )
        .unwrap();
        (run, checkpoint)
    }

    fn runner_config_v1() -> NativeCheckpointRunnerConfigV1 {
        NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 91_501,
            first_episode_index: 2,
            episode_count: 2,
            scheduler_timeout: Duration::from_secs(60),
            measure_broker_service_time: false,
        }
    }

    #[test]
    fn genuine_checkpoint_runs_complete_paired_native_schedule_repeatably() {
        let fixture = fixture_v1();
        let (first_run, first_checkpoint) = authorities_v1();
        let first = run_native_checkpoint_v1(
            &first_run,
            &first_checkpoint,
            &fixture.payload,
            runner_config_v1(),
        )
        .unwrap();
        let (second_run, second_checkpoint) = authorities_v1();
        let second = run_native_checkpoint_v1(
            &second_run,
            &second_checkpoint,
            &fixture.payload,
            runner_config_v1(),
        )
        .unwrap();

        assert_eq!(
            first.run_sha256(),
            parse_lower_hex_raw32_v1(first_run.run_sha256()).unwrap()
        );
        assert_eq!(
            first.identity_bundle_sha256(),
            parse_lower_hex_raw32_v1(first_run.identity_bundle_sha256()).unwrap()
        );
        assert_eq!(
            first.checkpoint_manifest_sha256(),
            first_checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(
            first.checkpoint_payload_sha256(),
            first_checkpoint.checkpoint_payload_sha256()
        );
        assert_eq!(
            first.logical_state_sha256(),
            first_checkpoint.logical_state_sha256()
        );
        assert_eq!(
            first.model_parameter_sha256(),
            first_checkpoint.model_parameter_sha256()
        );
        assert_eq!(
            first.train_state_sha256(),
            first_checkpoint.train_state_sha256()
        );
        assert_eq!(first.generation_index(), 0);
        assert_eq!(first.config(), runner_config_v1());
        assert_eq!(first.rollout().episodes.len(), 2);
        assert_eq!(first.episode_bindings().len(), 2);
        assert_eq!(first.episode_bindings()[0].episode_index(), 2);
        assert_eq!(first.episode_bindings()[1].episode_index(), 3);
        assert_eq!(first.episode_bindings()[0].learner_seat(), PlayerSeatV1::P0);
        assert_eq!(first.episode_bindings()[1].learner_seat(), PlayerSeatV1::P1);
        assert_eq!(
            first.episode_bindings()[0].environment_seed(),
            first.episode_bindings()[1].environment_seed()
        );
        let expected_deck_hashes = first_run
            .record()
            .environment()
            .deck_hashes_u64_hex()
            .each_ref()
            .map(|value| u64::from_str_radix(value, 16).unwrap());
        for binding in first.episode_bindings() {
            let expected = native_trainer_episode_schedule_v1(
                runner_config_v1().evaluation_base_seed,
                binding.episode_index(),
            )
            .unwrap();
            assert_eq!(binding.environment_seed(), expected.environment_seed);
            assert_eq!(binding.learner_seat(), expected.learner_seat);
            assert_eq!(binding.deck_hashes(), expected_deck_hashes);
            assert_ne!(binding.trajectory_sha256(), [0; 32]);
            assert_eq!(
                binding.policy_step_count(),
                binding
                    .learner_policy_step_count()
                    .checked_add(binding.opponent_policy_step_count())
                    .unwrap()
            );
            assert_eq!(
                binding.physical_decision_count(),
                binding
                    .learner_physical_decision_count()
                    .checked_add(binding.opponent_physical_decision_count())
                    .unwrap()
            );
        }
        assert!(first.rollout().all_natural());
        assert!(first.rollout().metrics.scorer_batch_count > 1);
        assert!(first.rollout().metrics.scored_decision_count > 1);
        assert_eq!(first.rollout().episodes, second.rollout().episodes);
        assert_eq!(
            first.rollout().policy_step_count,
            second.rollout().policy_step_count
        );
        assert_eq!(
            first.rollout().physical_decision_count,
            second.rollout().physical_decision_count
        );
        assert_eq!(
            first.rollout().metrics.batch_membership_digest,
            second.rollout().metrics.batch_membership_digest
        );
        assert!(!format!("{first:?}").contains("payload"));
    }

    #[test]
    fn config_preflight_precedes_payload_decode_and_rejects_incomplete_pairs() {
        let (run, checkpoint) = authorities_v1();
        let invalid = NativeCheckpointRunnerConfigV1 {
            first_episode_index: 1,
            ..runner_config_v1()
        };
        assert_eq!(
            run_native_checkpoint_v1(&run, &checkpoint, &[], invalid).unwrap_err(),
            NativeCheckpointRunnerErrorV1::InvalidConfig
        );

        let invalid = NativeCheckpointRunnerConfigV1 {
            first_episode_index: 0,
            episode_count: 1,
            ..runner_config_v1()
        };
        assert_eq!(
            run_native_checkpoint_v1(&run, &checkpoint, &[], invalid).unwrap_err(),
            NativeCheckpointRunnerErrorV1::InvalidConfig
        );

        let invalid = NativeCheckpointRunnerConfigV1 {
            scheduler_timeout: Duration::MAX,
            ..runner_config_v1()
        };
        assert_eq!(
            run_native_checkpoint_v1(&run, &checkpoint, &[], invalid).unwrap_err(),
            NativeCheckpointRunnerErrorV1::InvalidConfig
        );
    }

    #[test]
    fn real_k2_s4_trained_checkpoint_runs_and_retains_all_digest_bindings() {
        let genesis_fixture = fixture_v1();
        let trained_fixture = trained_fixture_v1();
        let run = decode_train_run_v2(&genesis_fixture.run_bytes).unwrap();
        let result = run_native_checkpoint_v1(
            &run,
            &trained_fixture.checkpoint,
            &trained_fixture.payload,
            runner_config_v1(),
        )
        .unwrap();

        assert_eq!(run.batch_episodes(), 2);
        assert_eq!(run.checkpoint_segment_updates(), 4);
        assert_eq!(result.generation_index(), 4);
        assert_eq!(
            result.checkpoint_manifest_sha256(),
            trained_fixture.checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(
            result.checkpoint_payload_sha256(),
            trained_fixture.checkpoint.checkpoint_payload_sha256()
        );
        assert_eq!(
            result.logical_state_sha256(),
            trained_fixture.checkpoint.logical_state_sha256()
        );
        assert_eq!(
            result.model_parameter_sha256(),
            trained_fixture.checkpoint.model_parameter_sha256()
        );
        assert_eq!(
            result.train_state_sha256(),
            trained_fixture.checkpoint.train_state_sha256()
        );
        assert_ne!(
            result.model_parameter_sha256(),
            parse_lower_hex_raw32_v1(&run.record().model_snapshot().named_parameter_stream_sha256)
                .unwrap()
        );
        assert_eq!(result.episode_bindings().len(), 2);
        assert_eq!(
            result.episode_bindings()[0].learner_seat(),
            PlayerSeatV1::P0
        );
        assert_eq!(
            result.episode_bindings()[1].learner_seat(),
            PlayerSeatV1::P1
        );
        assert!(result.rollout().all_natural());
    }

    #[test]
    fn valid_range_rejects_corrupt_payload_before_any_rollout() {
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        let mut corrupt = fixture.payload.clone();
        corrupt[0] ^= 1;
        let error =
            run_native_checkpoint_v1(&run, &checkpoint, &corrupt, runner_config_v1()).unwrap_err();
        match error {
            NativeCheckpointRunnerErrorV1::Inference(error) => assert_eq!(
                error.kind(),
                NativeCheckpointInferenceErrorKindV1::PayloadDigestMismatch
            ),
            other => panic!("unexpected runner error: {other:?}"),
        }
    }
}
