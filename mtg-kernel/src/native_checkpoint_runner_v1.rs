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
    run_async_flat_scored_rollout_native_environment_randomization_with_population_v1,
    run_async_flat_scored_rollout_native_environment_randomization_v2,
    run_async_flat_scored_rollout_native_observed_with_population_v1,
    run_async_flat_scored_rollout_native_observed_v2, AsyncFlatScoredObservedRunErrorV2,
    AsyncFlatScoredRolloutErrorV2, AsyncFlatScoredRolloutResultV2, FlatScoredSelectedEventV2,
    FlatScoredTerminalEventV2, FlatScoredTrajectoryObserverV2,
};
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::ids::PlayerId;
use crate::native_checkpoint_inference_v1::{
    load_native_checkpoint_inference_v1, load_native_checkpoint_inference_wide_v1,
    NativeCheckpointInferenceErrorV1,
};
use crate::native_full_episode_trajectory_v2::{
    preflight_native_environment_window_v2, NativeEnvironmentWindowPreflightAuthorityV2,
};
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
use crate::native_population_opponent_v1::PopulationOpponentEngineV1;
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, parse_lower_hex_raw32_v1};
use crate::native_training_store_run_v2::{
    NativeRunEnvironmentTrajectoryContractV1, ValidatedTrainRunV2,
};
use crate::rl::PlayerSeatV1;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
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
    /// The opt-in starting-player authority (`P1-METAMORPHIC-AUDIT-DESIGN-V4.md`
    /// Section 1.2), threaded verbatim into the rollout config below. `None`
    /// (every existing caller) reproduces the exact legacy per-episode reset
    /// path; `Some` is the `ladder_head_to_head_eval_v1` native test's
    /// `H2H_STARTING_PLAYER` environment binding, forcing the named physical
    /// seat to be the starting player of every rolled-out episode.
    pub starting_player: Option<PlayerId>,
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
    outer_trajectory_sha256_v2: Option<[u8; 32]>,
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

    /// The frozen 34-atom V2 outer envelope digest observed for an
    /// environment randomization V2 run; `None` for a legacy run. Ephemeral
    /// runtime evidence only, never persisted by any store byte stream.
    pub const fn outer_trajectory_sha256_v2(&self) -> Option<[u8; 32]> {
        self.outer_trajectory_sha256_v2
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
    expected_deck_ids: [String; 2],
    expected_deck_hashes: [u64; 2],
    expected_environment: NativeRunEnvironmentTrajectoryContractV1,
    expected_episode_count: usize,
    episode_bindings: Vec<NativeCheckpointRunnerEpisodeV1>,
}

// Zero-side-effect ordering instrumentation: proves whether the runner
// observer was constructed before a rejection. Test-only.
#[cfg(test)]
thread_local! {
    static RUNNER_OBSERVER_CONSTRUCTION_COUNT_V2: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// Run-local RAII counting scope; drop restores the saved value on every
/// exit path, including panics.
#[cfg(test)]
pub(crate) struct RunnerObserverConstructionCountScopeV2 {
    saved: u64,
    thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl RunnerObserverConstructionCountScopeV2 {
    pub(crate) fn count(&self) -> u64 {
        RUNNER_OBSERVER_CONSTRUCTION_COUNT_V2.with(std::cell::Cell::get)
    }
}

#[cfg(test)]
impl Drop for RunnerObserverConstructionCountScopeV2 {
    fn drop(&mut self) {
        RUNNER_OBSERVER_CONSTRUCTION_COUNT_V2.with(|count| count.set(self.saved));
    }
}

#[cfg(test)]
pub(crate) fn runner_observer_construction_count_scope_v2() -> RunnerObserverConstructionCountScopeV2
{
    let saved = RUNNER_OBSERVER_CONSTRUCTION_COUNT_V2.with(|count| count.replace(0));
    RunnerObserverConstructionCountScopeV2 {
        saved,
        thread_bound: std::marker::PhantomData,
    }
}

impl NativeCheckpointRunnerObserverV1 {
    fn new_v1(
        evaluation_base_seed: u64,
        first_episode_index: u64,
        end_episode_index_exclusive: u64,
        expected_deck_ids: [String; 2],
        expected_deck_hashes: [u64; 2],
        expected_environment: NativeRunEnvironmentTrajectoryContractV1,
        expected_episode_count: usize,
    ) -> Result<Self, NativeCheckpointRunnerErrorV1> {
        #[cfg(test)]
        RUNNER_OBSERVER_CONSTRUCTION_COUNT_V2.with(|count| count.set(count.get() + 1));
        let mut episode_bindings = Vec::new();
        episode_bindings
            .try_reserve_exact(expected_episode_count)
            .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?;
        Ok(Self {
            evaluation_base_seed,
            first_episode_index,
            end_episode_index_exclusive,
            expected_deck_ids,
            expected_deck_hashes,
            expected_environment,
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
        // The receipt variant must match the validated run's sealed contract
        // before any common accessor is trusted. Exhaustive on purpose: a
        // future third mode variant must fail compilation here rather than
        // silently map to Legacy.
        let expected_v2 = match self.expected_environment {
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1 => false,
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2 => true,
        };
        if receipt.is_environment_randomization_v2() != expected_v2 {
            return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant);
        }
        // V2-only facts are validated, not merely present: the pair index
        // must be the episode's own pair and the catalog-resolved physical
        // bindings must equal the ordered run deck IDs, while a legacy
        // receipt must project no V2-only fact at all.
        if expected_v2 {
            if receipt.pair_index_v2() != Some(receipt.episode_index() / 2) {
                return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant);
            }
            match receipt.deck_ids_v2() {
                Some(receipt_deck_ids) => {
                    if receipt_deck_ids[0] != self.expected_deck_ids[0]
                        || receipt_deck_ids[1] != self.expected_deck_ids[1]
                    {
                        return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant);
                    }
                }
                None => return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant),
            }
        } else if receipt.pair_index_v2().is_some() || receipt.deck_ids_v2().is_some() {
            return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant);
        }
        let schedule =
            native_trainer_episode_schedule_v1(self.evaluation_base_seed, receipt.episode_index())
                .map_err(|_| NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch)?;
        if schedule.environment_seed != receipt.environment_seed()
            || schedule.learner_seat != receipt.learner_seat()
            || !(self.first_episode_index..self.end_episode_index_exclusive)
                .contains(&receipt.episode_index())
            || receipt.deck_hashes() != self.expected_deck_hashes
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch);
        }
        if event.terminal.episode_id != receipt.episode_index()
            || event.terminal.policy_step_count != receipt.policy_step_count()
            || event.terminal.physical_decision_count != receipt.physical_decision_count()
            || event.learner_action_count != receipt.learner_policy_step_count()
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::TerminalMismatch);
        }
        if receipt
            .learner_policy_step_count()
            .checked_add(receipt.opponent_policy_step_count())
            != Some(receipt.policy_step_count())
            || receipt
                .learner_physical_decision_count()
                .checked_add(receipt.opponent_physical_decision_count())
                != Some(receipt.physical_decision_count())
            || self.episode_bindings.len() >= self.expected_episode_count
        {
            return Err(NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant);
        }
        self.episode_bindings.push(NativeCheckpointRunnerEpisodeV1 {
            episode_index: receipt.episode_index(),
            environment_seed: receipt.environment_seed(),
            deck_hashes: receipt.deck_hashes(),
            learner_seat: receipt.learner_seat(),
            trajectory_sha256: receipt.trajectory_sha256(),
            outer_trajectory_sha256_v2: receipt.outer_trajectory_sha256_v2(),
            policy_step_count: receipt.policy_step_count(),
            physical_decision_count: receipt.physical_decision_count(),
            learner_policy_step_count: receipt.learner_policy_step_count(),
            opponent_policy_step_count: receipt.opponent_policy_step_count(),
            learner_physical_decision_count: receipt.learner_physical_decision_count(),
            opponent_physical_decision_count: receipt.opponent_physical_decision_count(),
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
///
/// Thin wrapper over [`run_native_checkpoint_core_v1`] with the opponent
/// seat's ladder engine hardcoded to `None`, i.e. today's frozen uniform
/// opponent -- this function's behavior for every existing caller is
/// unchanged by that factoring (see the core's docs).
pub fn run_native_checkpoint_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    run_native_checkpoint_core_v1(run, checkpoint, checkpoint_payload, config, None, None)
}

/// EVAL-ONLY (Self-Play Ladder Design Contract S2, Deliverable 2 head-to-head
/// evaluator). The SAME rollout invocation as [`run_native_checkpoint_v1`],
/// with one added parameter threading an optional ladder opponent engine
/// into the rollout's opponent seat instead of the hardcoded `None`
/// `run_native_checkpoint_v1` passes.
///
/// Why this exists instead of a parameter on `run_native_checkpoint_v1`
/// itself: that would be a production-path signature change reaching every
/// caller of the frozen uniform-opponent eval path (the science loop, the
/// panel/v0 saturation probes, and any future panel/v1 tooling), none of
/// which should ever be able to pass anything but `None`. Checked first per
/// the task's own instruction: `run_native_checkpoint_v1`'s only opponent
/// hook is the hardcoded `None` third argument to
/// `run_async_flat_scored_rollout_native_observed_v2` a few lines below;
/// the executor-side ladder threading from commit da8e486
/// (`NativeTrainingExecutorV1::set_ladder_opponent_v1`) reaches only the
/// TRAINING loop's executor, not this runner. This function is the
/// documented fallback: a copy of the rollout invocation with the engine
/// threaded, factored as a shared private core (rather than a hand-copied
/// duplicate of the ~100-line body) so the two entry points can never drift
/// out of sync, and `run_native_checkpoint_v1`'s own behavior is provably
/// unchanged (it is now a one-line wrapper over the identical core with
/// `None`, covered by this module's existing test suite).
///
/// `cfg(test)`-gated: does not exist in a non-test build, so it can never be
/// reached from a training run record or any non-test/non-eval-tooling
/// caller.
#[cfg(test)]
pub(crate) fn run_native_checkpoint_with_ladder_opponent_eval_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    run_native_checkpoint_core_v1(
        run,
        checkpoint,
        checkpoint_payload,
        config,
        ladder_opponent,
        None,
    )
}

/// Evaluation-only population-opponent sibling. Existing fixed and ladder
/// callers cannot supply a population engine and preserve their exact path.
#[cfg(test)]
pub(crate) fn run_native_checkpoint_with_population_opponent_eval_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
    population_opponent: Arc<PopulationOpponentEngineV1>,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    run_native_checkpoint_core_v1(
        run,
        checkpoint,
        checkpoint_payload,
        config,
        None,
        Some(population_opponent),
    )
}

fn run_native_checkpoint_core_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
    population_opponent: Option<Arc<PopulationOpponentEngineV1>>,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    if ladder_opponent.is_some() && population_opponent.is_some() {
        return Err(NativeCheckpointRunnerErrorV1::InvalidConfig);
    }
    let validated = validate_runner_config_v1(run, config)?;
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
        [
            run.record().environment().deck_ids()[0].clone(),
            run.record().environment().deck_ids()[1].clone(),
        ],
        expected_deck_hashes,
        validated.environment,
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
        worker_count: validated.worker_count,
        sessions_per_worker: validated.sessions_per_worker,
        broker_batch_target: validated.broker_batch_target,
        first_episode_id: config.first_episode_index,
        episode_count: config.episode_count,
        scheduler_timeout: config.scheduler_timeout,
        measure_broker_service_time: config.measure_broker_service_time,
        starting_player: config.starting_player,
    };
    let mut scorer = inference.batch_scorer_v1();
    // Exhaustive rollout dispatch by the sealed contract: neither core can
    // default to Legacy, and the V2 arm surrenders the consumed authority
    // minted by the first validator from the evaluation seed.
    let observed = match (
        validated.environment,
        validated.environment_authority,
        population_opponent,
    ) {
        (NativeRunEnvironmentTrajectoryContractV1::LegacyV1, None, Some(population)) => {
            run_async_flat_scored_rollout_native_observed_with_population_v1(
                rollout_config,
                config.evaluation_base_seed,
                ladder_opponent,
                Some(population),
                &mut scorer,
                observer,
            )
        }
        (NativeRunEnvironmentTrajectoryContractV1::LegacyV1, None, None) => {
            run_async_flat_scored_rollout_native_observed_v2(
                rollout_config,
                config.evaluation_base_seed,
                ladder_opponent,
                &mut scorer,
                observer,
            )
        }
        (
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
            Some(environment_authority),
            Some(population),
        ) => run_async_flat_scored_rollout_native_environment_randomization_with_population_v1(
            rollout_config,
            config.evaluation_base_seed,
            environment_authority,
            ladder_opponent,
            Some(population),
            &mut scorer,
            observer,
        ),
        (
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
            Some(environment_authority),
            None,
        ) => run_async_flat_scored_rollout_native_environment_randomization_v2(
            rollout_config,
            config.evaluation_base_seed,
            environment_authority,
            ladder_opponent,
            &mut scorer,
            observer,
        ),
        (NativeRunEnvironmentTrajectoryContractV1::LegacyV1, Some(_), _)
        | (NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2, None, _) => {
            return Err(NativeCheckpointRunnerErrorV1::Protocol);
        }
    };
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
        worker_count: validated.worker_count,
        sessions_per_worker: validated.sessions_per_worker,
        broker_batch_target: validated.broker_batch_target,
        episode_bindings,
        rollout,
    })
}

/// Capacity-experiment wide-net sibling of [`run_native_checkpoint_v1`]
/// (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3). EVALUATION ONLY: loads
/// through [`load_native_checkpoint_inference_wide_v1`] instead, which
/// itself fails closed unless `run`'s record carries
/// `contracts.wide_model_experiment_v1`.
///
/// Thin wrapper over [`run_native_checkpoint_wide_core_v1`] with the
/// opponent seat's ladder engine hardcoded to `None` (the uniform opponent),
/// mirroring exactly the frozen-path factoring of
/// [`run_native_checkpoint_v1`] over [`run_native_checkpoint_core_v1`]. An
/// earlier revision of this doc claimed no ladder-opponent-threaded wide
/// variant was needed; that claim was WRONG and was caught in hostile
/// review: the capacity contract's DECISIVE read (Section 5) is a true
/// head-to-head of the wide candidate against promoted frozen checkpoints,
/// which requires the ladder engine in the opponent seat. The correction is
/// [`run_native_checkpoint_wide_with_ladder_opponent_eval_v1`] below.
pub fn run_native_checkpoint_wide_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    run_native_checkpoint_wide_core_v1(run, checkpoint, checkpoint_payload, config, None)
}

/// EVAL-ONLY wide-net twin of
/// [`run_native_checkpoint_with_ladder_opponent_eval_v1`] (capacity contract
/// Section 5 decisive read: wide candidate vs a frozen promoted checkpoint's
/// actual weights). The CANDIDATE seat loads through the wide loader; the
/// opponent seat's [`LadderOpponentEngineV1`] holds frozen-identity
/// inference handles built by the caller, which is correct BY PROTOCOL: every
/// opponent in the capacity experiment (promoted(1), promoted(2), pool
/// members) is a frozen Net8 checkpoint, and the frozen loader those handles
/// come from fails closed on any wide-length payload, so a wide opponent can
/// never be substituted silently.
///
/// `cfg(test)`-gated for the same production-safety argument as the frozen
/// twin: it does not exist in a non-test build and can never be reached from
/// a training run record.
#[cfg(test)]
pub(crate) fn run_native_checkpoint_wide_with_ladder_opponent_eval_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    run_native_checkpoint_wide_core_v1(run, checkpoint, checkpoint_payload, config, ladder_opponent)
}

fn run_native_checkpoint_wide_core_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    checkpoint_payload: &[u8],
    config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
) -> Result<NativeCheckpointRunResultV1, NativeCheckpointRunnerErrorV1> {
    let validated = validate_runner_config_v1(run, config)?;
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
        [
            run.record().environment().deck_ids()[0].clone(),
            run.record().environment().deck_ids()[1].clone(),
        ],
        expected_deck_hashes,
        validated.environment,
        expected_episode_count,
    )?;
    let inference = load_native_checkpoint_inference_wide_v1(run, checkpoint, checkpoint_payload)?;
    let identity_bundle_sha256 = parse_lower_hex_raw32_v1(run.identity_bundle_sha256())
        .map_err(|_| NativeCheckpointRunnerErrorV1::Protocol)?;
    let rollout_config = AsyncRolloutConfigV2 {
        deck_ids: [
            run.record().environment().deck_ids()[0].clone(),
            run.record().environment().deck_ids()[1].clone(),
        ],
        learner_seat: PlayerSeatV1::P0,
        environment_seed: config.evaluation_base_seed,
        opponent_policy_seed: config.evaluation_base_seed,
        learner_policy_seed: config.evaluation_base_seed,
        max_physical_decisions: run.record().limits().max_physical_decisions(),
        max_policy_steps: run.record().limits().max_policy_steps(),
        worker_count: validated.worker_count,
        sessions_per_worker: validated.sessions_per_worker,
        broker_batch_target: validated.broker_batch_target,
        first_episode_id: config.first_episode_index,
        episode_count: config.episode_count,
        scheduler_timeout: config.scheduler_timeout,
        measure_broker_service_time: config.measure_broker_service_time,
        starting_player: config.starting_player,
    };
    let mut scorer = inference.batch_scorer_v1();
    // Exhaustive rollout dispatch by the sealed contract: neither core can
    // default to Legacy, and the V2 arm surrenders the consumed authority
    // minted by the first validator from the evaluation seed.
    let observed = match (validated.environment, validated.environment_authority) {
        (NativeRunEnvironmentTrajectoryContractV1::LegacyV1, None) => {
            run_async_flat_scored_rollout_native_observed_v2(
                rollout_config,
                config.evaluation_base_seed,
                ladder_opponent,
                &mut scorer,
                observer,
            )
        }
        (
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
            Some(environment_authority),
        ) => run_async_flat_scored_rollout_native_environment_randomization_v2(
            rollout_config,
            config.evaluation_base_seed,
            environment_authority,
            ladder_opponent,
            &mut scorer,
            observer,
        ),
        (NativeRunEnvironmentTrajectoryContractV1::LegacyV1, Some(_))
        | (NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2, None) => {
            return Err(NativeCheckpointRunnerErrorV1::Protocol);
        }
    };
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
        worker_count: validated.worker_count,
        sessions_per_worker: validated.sessions_per_worker,
        broker_batch_target: validated.broker_batch_target,
        episode_bindings,
        rollout,
    })
}

/// Private validated runner bundle: the run topology plus the sealed run
/// trajectory contract and, for an environment randomization V2 run, the
/// consumed whole-window preflight authority derived from the caller's
/// `evaluation_base_seed`. Both cores consume exactly this bundle, so neither
/// wide nor narrow evaluation can ever default to Legacy.
struct NativeCheckpointRunnerValidatedConfigV1 {
    worker_count: usize,
    sessions_per_worker: usize,
    broker_batch_target: usize,
    environment: NativeRunEnvironmentTrajectoryContractV1,
    environment_authority: Option<NativeEnvironmentWindowPreflightAuthorityV2>,
}

fn validate_runner_config_v1(
    run: &ValidatedTrainRunV2,
    config: NativeCheckpointRunnerConfigV1,
) -> Result<NativeCheckpointRunnerValidatedConfigV1, NativeCheckpointRunnerErrorV1> {
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
    // Sealed-mode classification plus, for a V2 run, the whole-window pair
    // preflight, still inside this first validator and therefore before
    // observer construction and before any checkpoint payload decode in both
    // cores. The window root is the caller's evaluation seed, never the run
    // schedule seed: evaluation derives its own pair roots exactly as it
    // derives its own episode schedule. Exhaustive, no wildcard.
    let environment = run.environment_trajectory_contract_v1();
    let environment_authority = match environment {
        NativeRunEnvironmentTrajectoryContractV1::LegacyV1 => None,
        NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2 => {
            let record_environment = &run.record().environment;
            let deck_hashes_hex = record_environment.deck_hashes_u64_hex();
            let deck_hashes = [
                u64::from_str_radix(&deck_hashes_hex[0], 16)
                    .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?,
                u64::from_str_radix(&deck_hashes_hex[1], 16)
                    .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?,
            ];
            Some(
                preflight_native_environment_window_v2(
                    config.evaluation_base_seed,
                    config.first_episode_index,
                    config.episode_count,
                    record_environment.deck_ids(),
                    deck_hashes,
                )
                .map_err(|_| NativeCheckpointRunnerErrorV1::InvalidConfig)?,
            )
        }
    };
    Ok(NativeCheckpointRunnerValidatedConfigV1 {
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        environment,
        environment_authority,
    })
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
            starting_player: None,
        }
    }

    /// LEGACY BIT-IDENTITY GATE (`P1-METAMORPHIC-AUDIT-DESIGN-V4.md` Section
    /// 1.2). This is the golden-regression half of the two-part proof
    /// required before any starting-player authority is trusted: with the
    /// authority unset (`runner_config_v1()`'s `starting_player: None`), the
    /// exact same fixed-seed episode batch, run through this crate's real
    /// checkpoint-eval path (`run_native_checkpoint_v1`, the same function
    /// `ladder_head_to_head_eval_v1` and `run_native_checkpoint_core_v1`
    /// share), must produce byte-identical results to the parent commit this
    /// branch forked from (`cc8e20f8080bd8a46ccee69b1a434bd89822d06b`,
    /// `fable/response-exploiter-v2-campaign`), before the starting-player
    /// authority existed at all.
    ///
    /// Every constant below was captured by running this exact scenario
    /// (same fixture helpers, same seeds, same episode window) on that
    /// parent commit, via a temporary extraction test added there only for
    /// this capture and discarded afterward (never committed): checked out
    /// read-only into a scratch worktree, `run_native_checkpoint_v1` invoked
    /// with the pre-existing `NativeCheckpointRunnerConfigV1` shape (no
    /// `starting_player` field existed on that commit), and the resulting
    /// hashes/counts printed. Every value asserted here reproduces that
    /// printed output verbatim.
    #[test]
    fn starting_player_unset_reproduces_parent_commit_checkpoint_eval_bytes_v1() {
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        let result =
            run_native_checkpoint_v1(&run, &checkpoint, &fixture.payload, runner_config_v1())
                .unwrap();

        // Re-baselined once per the owner ruling on record (collab CLAUDE
        // #236, 2026-08-14): logical_state_sha256 is derived from the
        // observation, so it alone (of the three digests in this test)
        // moves with the two accepted 603.10-family observation fixes; the
        // parent-commit comparison's premise (bit-identical to pre-fix
        // behavior) is superseded by the epoch, not violated by it --
        // model_parameter_sha256/train_state_sha256/deck_hashes below are
        // unaffected and still verify against the original parent-commit
        // capture unchanged.
        assert_eq!(
            lower_hex_raw32_v1(result.logical_state_sha256()),
            "69e6a7d0fdbccd6013bd1d2a4f49baa42ef30e8f3218d8076c9388020bfad974"
        );
        assert_eq!(
            lower_hex_raw32_v1(result.model_parameter_sha256()),
            "36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54"
        );
        assert_eq!(
            lower_hex_raw32_v1(result.train_state_sha256()),
            "5854b477e2ce22dda199b5c9442824a339acd15d7eb8666f19895aa0d7c53c26"
        );

        let bindings = result.episode_bindings();
        assert_eq!(bindings.len(), 2);

        assert_eq!(bindings[0].episode_index(), 2);
        assert_eq!(bindings[0].environment_seed(), 3_233_989_599_464_222_885);
        assert_eq!(
            bindings[0].deck_hashes(),
            [909_447_583_901_160_127, 909_447_583_901_160_127]
        );
        assert_eq!(bindings[0].learner_seat(), PlayerSeatV1::P0);
        // Re-baselined once per the owner ruling on record (collab CLAUDE
        // #236, 2026-08-14): observation-derived, see logical_state_sha256
        // above for the full rationale.
        assert_eq!(
            lower_hex_raw32_v1(bindings[0].trajectory_sha256()),
            "f6a0be9ced1bceb1628965d2597e7c3cc7adeaa5ae8de24aa017d52a481b6985"
        );
        assert_eq!(bindings[0].outer_trajectory_sha256_v2(), None);
        assert_eq!(bindings[0].policy_step_count(), 151);
        assert_eq!(bindings[0].physical_decision_count(), 141);
        assert_eq!(bindings[0].learner_policy_step_count(), 63);
        assert_eq!(bindings[0].opponent_policy_step_count(), 88);
        assert_eq!(bindings[0].learner_physical_decision_count(), 53);
        assert_eq!(bindings[0].opponent_physical_decision_count(), 88);

        assert_eq!(bindings[1].episode_index(), 3);
        assert_eq!(bindings[1].environment_seed(), 3_233_989_599_464_222_885);
        assert_eq!(
            bindings[1].deck_hashes(),
            [909_447_583_901_160_127, 909_447_583_901_160_127]
        );
        assert_eq!(bindings[1].learner_seat(), PlayerSeatV1::P1);
        // Re-baselined once per the owner ruling on record (collab CLAUDE
        // #236, 2026-08-14): observation-derived, see logical_state_sha256
        // above for the full rationale.
        assert_eq!(
            lower_hex_raw32_v1(bindings[1].trajectory_sha256()),
            "2253bd914bb47db25ab403b212680272cec399a9e4459286b5a6bbcfb2d17b90"
        );
        assert_eq!(bindings[1].outer_trajectory_sha256_v2(), None);
        // Re-baselined once per the owner ruling on record (collab CLAUDE
        // #236, 2026-08-14): these step/decision counts are derived from a
        // live replay whose legal-action availability can shift under the
        // two accepted 603.10-family observation fixes (e.g. reset-scope
        // widening changes which permanents are summoning-sick/tapped after
        // a zone change), so the exact game length is not guaranteed
        // invariant across the epoch even though it was deterministic
        // before and after it. Values are this test's own live-computed
        // counts, read directly from failing runs (never hand-typed).
        assert_eq!(bindings[1].policy_step_count(), 200);
        assert_eq!(bindings[1].physical_decision_count(), 167);
        assert_eq!(bindings[1].learner_policy_step_count(), 101);
        assert_eq!(bindings[1].opponent_policy_step_count(), 99);
        assert_eq!(bindings[1].learner_physical_decision_count(), 100);
        assert_eq!(bindings[1].opponent_physical_decision_count(), 67);
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

    // ------------------------------------------------------------------
    // Capacity-experiment wide-net runner
    // (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3). EVALUATION ONLY.
    // ------------------------------------------------------------------

    struct WideRunnerFixtureV1 {
        run_bytes: Vec<u8>,
        checkpoint_bytes: Vec<u8>,
        payload: Vec<u8>,
    }

    static WIDE_RUNNER_FIXTURE_V1: OnceLock<WideRunnerFixtureV1> = OnceLock::new();

    fn wide_zero_moment_payload_v1() -> Vec<u8> {
        use crate::native_policy_train_step_v1::NativePolicyValueTrainSnapshotV1;
        use crate::native_policy_value_net_v1::NativeNamedParameterV1;
        use crate::native_train_state_payload_v1::encode_native_train_state_payload_wide_v1;
        let (manifest_path, payload_path) =
            crate::common_model_snapshot_v1::wide_model_snapshot_paths_v1();
        let (model, _record) = crate::common_model_snapshot_v1::build_wide_model_candidate_v1(
            &manifest_path,
            &payload_path,
        )
        .expect("real wide snapshot must load");
        let parameters = model.parameter_snapshot_wide_v1();
        let zero_moments: Vec<_> = parameters
            .iter()
            .map(|parameter| NativeNamedParameterV1 {
                name: parameter.name,
                shape: parameter.shape.clone(),
                values: vec![0.0; parameter.values.len()],
            })
            .collect();
        let scorer_bias_anchor_bits = parameters
            .iter()
            .find(|parameter| parameter.name == "scorer.2.bias")
            .expect("scorer.2.bias tensor present")
            .values[0]
            .to_bits();
        let snapshot = NativePolicyValueTrainSnapshotV1 {
            adam_step: 0,
            scorer_bias_anchor_bits,
            parameters,
            first_moments: zero_moments.clone(),
            second_moments: zero_moments,
        };
        encode_native_train_state_payload_wide_v1(&snapshot)
            .unwrap()
            .bytes
    }

    fn wide_fixture_v1() -> &'static WideRunnerFixtureV1 {
        WIDE_RUNNER_FIXTURE_V1.get_or_init(|| {
            use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_v2;
            let run_bytes = test_fixture_bytes_with_schedule_and_base_seed_wide_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                2,
                4,
                4,
                2,
                4,
                8,
                32_768,
                65_536,
                71501,
            );
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let payload = wide_zero_moment_payload_v1();
            let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
            WideRunnerFixtureV1 {
                run_bytes,
                checkpoint_bytes: checkpoint.canonical_bytes().to_vec(),
                payload,
            }
        })
    }

    fn wide_authorities_v1() -> (ValidatedTrainRunV2, CheckpointManifestV3) {
        let fixture = wide_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let checkpoint = decode_genesis_checkpoint_manifest_v3(
            &fixture.checkpoint_bytes,
            &fixture.payload,
            &run,
        )
        .unwrap();
        (run, checkpoint)
    }

    /// The end-to-end proof (contract Section 5 freeze gate): a real wide
    /// genesis checkpoint decodes, authority-binds, and plays one ACTUAL
    /// evaluation game (a genuine seat-swapped pair via the real rollout
    /// engine, not a synthetic decision), exactly like
    /// `genuine_checkpoint_runs_complete_paired_native_schedule_repeatably`
    /// proves for the frozen net.
    #[test]
    fn wide_checkpoint_runs_a_genuine_evaluation_game_end_to_end() {
        let fixture = wide_fixture_v1();
        let (run, checkpoint) = wide_authorities_v1();
        let result =
            run_native_checkpoint_wide_v1(&run, &checkpoint, &fixture.payload, runner_config_v1())
                .expect("a real wide checkpoint must play a genuine evaluation game");

        assert_eq!(result.generation_index(), 0);
        assert_eq!(
            result.run_sha256(),
            parse_lower_hex_raw32_v1(run.run_sha256()).unwrap()
        );
        assert_eq!(result.rollout().episodes.len(), 2);
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
        assert!(result.rollout().metrics.scorer_batch_count > 1);
        assert!(result.rollout().metrics.scored_decision_count > 1);
        for binding in result.episode_bindings() {
            assert_ne!(binding.trajectory_sha256(), [0; 32]);
        }

        // Fail-closed direction 1: the frozen runner rejects the wide-length
        // payload against a real frozen checkpoint outright.
        let (frozen_run, frozen_checkpoint) = authorities_v1();
        match run_native_checkpoint_v1(
            &frozen_run,
            &frozen_checkpoint,
            &fixture.payload,
            runner_config_v1(),
        )
        .unwrap_err()
        {
            NativeCheckpointRunnerErrorV1::Inference(error) => assert_eq!(
                error.kind(),
                NativeCheckpointInferenceErrorKindV1::PayloadExactLength
            ),
            other => panic!("expected Inference(PayloadExactLength), got {other:?}"),
        }

        // Fail-closed direction 2: the wide runner rejects a real
        // frozen-length payload against the wide checkpoint outright.
        let frozen_fixture = fixture_v1();
        match run_native_checkpoint_wide_v1(
            &run,
            &checkpoint,
            &frozen_fixture.payload,
            runner_config_v1(),
        )
        .unwrap_err()
        {
            NativeCheckpointRunnerErrorV1::Inference(error) => assert_eq!(
                error.kind(),
                NativeCheckpointInferenceErrorKindV1::PayloadExactLength
            ),
            other => panic!("expected Inference(PayloadExactLength), got {other:?}"),
        }
    }

    /// The head-to-head e2e proof the hostile review demanded (capacity
    /// contract Section 5 decisive read): a real wide genesis checkpoint
    /// plays a genuine seat-swapped pair with the LADDER ENGINE standing in
    /// the opponent seat, where the engine's three policy-driven slots are
    /// independently loaded FROZEN-identity handles onto one real frozen
    /// checkpoint. This is the exact runner + engine combination
    /// `ladder_head_to_head_eval_v1` (WIDE=1) invokes, exercised end to end
    /// through the real rollout engine, not compile-checked.
    #[test]
    fn wide_checkpoint_plays_head_to_head_against_frozen_ladder_opponent_end_to_end() {
        use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;

        let fixture = wide_fixture_v1();
        let (run, checkpoint) = wide_authorities_v1();
        let frozen_fixture = fixture_v1();
        let (frozen_run, frozen_checkpoint) = authorities_v1();
        let engine = Arc::new(LadderOpponentEngineV1::head_to_head_eval_v1(
            load_native_checkpoint_inference_v1(
                &frozen_run,
                &frozen_checkpoint,
                &frozen_fixture.payload,
            )
            .unwrap(),
            load_native_checkpoint_inference_v1(
                &frozen_run,
                &frozen_checkpoint,
                &frozen_fixture.payload,
            )
            .unwrap(),
            load_native_checkpoint_inference_v1(
                &frozen_run,
                &frozen_checkpoint,
                &frozen_fixture.payload,
            )
            .unwrap(),
        ));

        let result = run_native_checkpoint_wide_with_ladder_opponent_eval_v1(
            &run,
            &checkpoint,
            &fixture.payload,
            runner_config_v1(),
            Some(engine.clone()),
        )
        .expect("a real wide candidate must complete a head-to-head against a frozen opponent");

        assert_eq!(result.generation_index(), 0);
        assert_eq!(result.rollout().episodes.len(), 2);
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
        assert!(result.rollout().metrics.scorer_batch_count > 1);
        assert!(result.rollout().metrics.scored_decision_count > 1);
        // The opponent seat's decisions must actually route through the
        // ladder engine, not the uniform fallback: with the engine present,
        // opponent policy steps are engine-scored and counted per binding.
        for binding in result.episode_bindings() {
            assert_ne!(binding.trajectory_sha256(), [0; 32]);
            assert!(binding.opponent_policy_step_count() > 0);
        }

        // Fail-closed: the wide eval twin rejects a frozen-length candidate
        // payload outright, engine present or not.
        match run_native_checkpoint_wide_with_ladder_opponent_eval_v1(
            &run,
            &checkpoint,
            &frozen_fixture.payload,
            runner_config_v1(),
            Some(engine),
        )
        .unwrap_err()
        {
            NativeCheckpointRunnerErrorV1::Inference(error) => assert_eq!(
                error.kind(),
                NativeCheckpointInferenceErrorKindV1::PayloadExactLength
            ),
            other => panic!("expected Inference(PayloadExactLength), got {other:?}"),
        }
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

    /// Live C2 acceptance and ordering for both runner cores under the
    /// environment randomization V2 contract.
    ///
    /// Ordering: the first validator performs the V2 whole-window pair
    /// preflight, so an armed interior pair corruption returns
    /// `InvalidConfig` with zero runner-observer constructions even though
    /// the payload is deliberately malformed; the legacy control proves the
    /// same malformed bytes otherwise reach the payload decoder. Acceptance:
    /// the V2 run's own valid payload evaluates end to end in the frozen and
    /// wide cores, every binding carries the V2 outer digest beside the
    /// inner compatibility digest, and the pair roots follow
    /// `config.evaluation_base_seed`, never the run schedule seed.
    #[test]
    fn v2_checkpoint_evaluation_preflights_the_window_then_evaluates_live() {
        use crate::native_full_episode_trajectory_v2::{
            arm_window_pair_corruption_for_test_v2, NativeWindowPairCorruptionForTestV2,
        };
        use crate::native_training_store_run_v2::test_fixture_bytes_environment_randomization_v2;

        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let (legacy_run, legacy_checkpoint) = authorities_v1();
        assert_eq!(
            legacy_run.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1
        );
        let v2_run = decode_train_run_v2(&test_fixture_bytes_environment_randomization_v2())
            .expect("the coherent V2 fixture decodes");
        assert_eq!(
            v2_run.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );

        let v2_executor = fresh_executor_v1(&v2_run);
        let v2_candidate = v2_executor.checkpoint_candidate_v1().unwrap();
        let v2_payload = v2_candidate.payload().to_vec();
        let v2_checkpoint = build_genesis_checkpoint_manifest_v3(&v2_run, &v2_payload).unwrap();
        let malformed_v2 = v2_payload[..7].to_vec();

        // Nonvacuity control: with a legacy run the malformed payload reaches
        // the payload decoder and is rejected there, not by config preflight.
        let malformed_legacy = fixture_v1().payload[..7].to_vec();
        match run_native_checkpoint_v1(
            &legacy_run,
            &legacy_checkpoint,
            &malformed_legacy,
            runner_config_v1(),
        )
        .unwrap_err()
        {
            NativeCheckpointRunnerErrorV1::Inference(error) => assert_eq!(
                error.kind(),
                NativeCheckpointInferenceErrorKindV1::PayloadExactLength
            ),
            other => panic!("expected the malformed payload to reach decode, got {other:?}"),
        }

        // Armed interior pair corruption: the V2 window preflight inside the
        // first validator rejects before observer construction and before
        // payload decode, in the frozen and wide cores alike. The runner
        // window is one pair per config below, so offset zero is the whole
        // table here; the pure window-preflight suite and the
        // prepared-segment interior-pair oracle cover the deeper K=2, S=4
        // offsets.
        for wide in [false, true] {
            let scope = runner_observer_construction_count_scope_v2();
            let _corruption = arm_window_pair_corruption_for_test_v2(
                0,
                NativeWindowPairCorruptionForTestV2::PairRootDrift,
            );
            let error = if wide {
                run_native_checkpoint_wide_v1(
                    &v2_run,
                    &v2_checkpoint,
                    &malformed_v2,
                    runner_config_v1(),
                )
                .unwrap_err()
            } else {
                run_native_checkpoint_v1(&v2_run, &v2_checkpoint, &malformed_v2, runner_config_v1())
                    .unwrap_err()
            };
            assert_eq!(error, NativeCheckpointRunnerErrorV1::InvalidConfig);
            assert_eq!(
                scope.count(),
                0,
                "a failed window preflight must construct zero runner observers"
            );
        }

        // Live V2 acceptance, genuinely in both cores: the narrow core runs
        // the narrow V2 fixture's own payload, and the wide core runs a
        // genuinely wide V2 run with a real wide payload. In each, pair
        // roots follow the evaluation seed, outer digests ride beside the
        // inner compatibility digest, and the seat pair shares one exact
        // root that is not the run schedule root.
        let wide_v2_run = decode_train_run_v2(
            &crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_environment_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                2,
                4,
                4,
                2,
                4,
                8,
                32_768,
                65_536,
                71_501,
            ),
        )
        .expect("the wide V2 fixture decodes");
        assert_eq!(
            wide_v2_run.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        let wide_payload = wide_zero_moment_payload_v1();
        let wide_v2_checkpoint =
            build_genesis_checkpoint_manifest_v3(&wide_v2_run, &wide_payload).unwrap();
        let acceptance_arms: [(u64, bool); 3] = [(7_777, false), (9_291, false), (7_777, true)];
        for (evaluation_base_seed, wide) in acceptance_arms {
            let config = NativeCheckpointRunnerConfigV1 {
                evaluation_base_seed,
                ..runner_config_v1()
            };
            let observer_scope = runner_observer_construction_count_scope_v2();
            let (result, run_schedule_seed) = if wide {
                (
                    run_native_checkpoint_wide_v1(
                        &wide_v2_run,
                        &wide_v2_checkpoint,
                        &wide_payload,
                        config,
                    )
                    .expect("the genuinely wide V2 evaluation must run live"),
                    wide_v2_run.record().schedule.base_seed,
                )
            } else {
                (
                    run_native_checkpoint_v1(&v2_run, &v2_checkpoint, &v2_payload, config).unwrap(),
                    v2_run.record().schedule.base_seed,
                )
            };
            assert_eq!(
                observer_scope.count(),
                1,
                "a live evaluation constructs exactly one runner observer"
            );
            let bindings = result.episode_bindings();
            assert_eq!(bindings.len(), 2);
            for binding in bindings {
                let schedule =
                    crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1(
                        evaluation_base_seed,
                        binding.episode_index(),
                    )
                    .unwrap();
                assert_eq!(
                    binding.environment_seed(),
                    schedule.environment_seed,
                    "V2 pair roots must follow the evaluation seed"
                );
                let outer = binding
                    .outer_trajectory_sha256_v2()
                    .expect("a V2 evaluation binding carries the outer digest");
                assert_ne!(outer, binding.trajectory_sha256());
            }
            assert_eq!(
                bindings[0].environment_seed(),
                bindings[1].environment_seed(),
                "the even/odd pair shares one exact schedule root"
            );
            assert_ne!(
                bindings[0].environment_seed(),
                crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1(
                    run_schedule_seed,
                    0,
                )
                .unwrap()
                .environment_seed,
                "the evaluation root must not be the run schedule root"
            );
        }

        // Legacy bindings carry no outer digest.
        let legacy_result = run_native_checkpoint_v1(
            &legacy_run,
            &legacy_checkpoint,
            fixture_v1().payload.as_slice(),
            runner_config_v1(),
        )
        .unwrap();
        for binding in legacy_result.episode_bindings() {
            assert_eq!(binding.outer_trajectory_sha256_v2(), None);
        }
    }

    /// Live C2 runner-observer receipt battery with genuinely distinct
    /// ordered decks: a coherent crafted V2 terminal is admitted, and then
    /// wrong-variant, pair-index, and each ordered deck-ID mutation rejects,
    /// so deleting any one of the observer's V2 receipt checks fails here.
    /// CPU-only and allocation-light: the observer is driven directly.
    #[test]
    fn runner_observer_rejects_each_v2_receipt_fact_mutation() {
        use crate::async_flat_scored_rollout_v2::FlatScoredTerminalEventV2;
        use crate::async_rollout::AsyncRolloutTerminalV1;
        use crate::native_full_episode_trajectory_v1::NativeFullEpisodeTrajectoryReceiptV1;
        use crate::native_full_episode_trajectory_v2::{
            envelope_probe_receipt_for_test_v2, NativeTrainingTrajectoryReceiptV2,
            NativeV2ReceiptFactMutationForTestV2,
        };
        use crate::rl::{TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
        use crate::runtime_decks::runtime_deck_by_id;

        let evaluation_base_seed = 7_777_u64;
        let rally = runtime_deck_by_id("Rally").unwrap();
        let burn = runtime_deck_by_id("Burn").unwrap();
        let schedule = native_trainer_episode_schedule_v1(evaluation_base_seed, 0).unwrap();
        let deck_ids = ["Rally".to_owned(), "Burn".to_owned()];
        let deck_hashes = [rally.runtime_deck_hash, burn.runtime_deck_hash];
        let genuine = envelope_probe_receipt_for_test_v2(
            0,
            schedule.environment_seed,
            &deck_ids,
            deck_hashes,
        );
        let event_with = |receipt: NativeTrainingTrajectoryReceiptV2| FlatScoredTerminalEventV2 {
            terminal: AsyncRolloutTerminalV1 {
                episode_id: receipt.episode_index(),
                terminal_outcome: TerminalOutcomeV1::P0Win,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner: Some(PlayerSeatV1::P0),
                terminal_reward: [1, -1],
                policy_step_count: receipt.policy_step_count(),
                physical_decision_count: receipt.physical_decision_count(),
            },
            learner_action_count: receipt.learner_policy_step_count(),
            learner_trace_hash: 0,
            native_full_trajectory_receipt: Some(receipt),
        };
        let fresh_observer = || {
            NativeCheckpointRunnerObserverV1::new_v1(
                evaluation_base_seed,
                0,
                2,
                deck_ids.clone(),
                deck_hashes,
                NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
                2,
            )
            .unwrap()
        };

        // Positive control: the coherent crafted event is admitted and
        // retained as exactly one binding carrying the outer evidence.
        use crate::async_flat_scored_rollout_v2::FlatScoredTrajectoryObserverV2;
        let mut observer = fresh_observer();
        observer
            .observe_terminal_v2(event_with(genuine))
            .expect("the coherent crafted V2 terminal must be admitted");
        let retained = observer.finish_v2().unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(
            retained[0].outer_trajectory_sha256_v2(),
            genuine.outer_trajectory_sha256_v2()
        );

        // Wrong variant: a legacy receipt with the same common facts.
        let legacy = NativeTrainingTrajectoryReceiptV2::from_legacy_v1(
            NativeFullEpisodeTrajectoryReceiptV1 {
                episode_index: genuine.episode_index(),
                environment_seed: genuine.environment_seed(),
                deck_hashes: genuine.deck_hashes(),
                learner_seat: genuine.learner_seat(),
                trajectory_sha256: genuine.trajectory_sha256(),
                policy_step_count: genuine.policy_step_count(),
                physical_decision_count: genuine.physical_decision_count(),
                learner_policy_step_count: genuine.learner_policy_step_count(),
                opponent_policy_step_count: genuine.opponent_policy_step_count(),
                learner_physical_decision_count: genuine.learner_physical_decision_count(),
                opponent_physical_decision_count: genuine.opponent_physical_decision_count(),
            },
        );
        let mut variant_observer = fresh_observer();
        assert_eq!(
            variant_observer
                .observe_terminal_v2(event_with(legacy))
                .unwrap_err(),
            NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant,
            "the wrong receipt variant must reject"
        );
        assert!(variant_observer.finish_v2().unwrap().is_empty());

        for mutation in [
            NativeV2ReceiptFactMutationForTestV2::PairIndex,
            NativeV2ReceiptFactMutationForTestV2::DeckId0,
            NativeV2ReceiptFactMutationForTestV2::DeckId1,
        ] {
            let mut corrupted = genuine;
            corrupted.mutate_environment_fact_for_test_v2(mutation);
            let mut mutation_observer = fresh_observer();
            assert_eq!(
                mutation_observer
                    .observe_terminal_v2(event_with(corrupted))
                    .unwrap_err(),
                NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant,
                "mutation {mutation:?} must reject at the runner observer"
            );
            assert!(
                mutation_observer.finish_v2().unwrap().is_empty(),
                "a rejected terminal must retain no binding"
            );
        }

        // Fixed-baseline extension over the schedule, terminal, and split
        // blocks: the event stays the genuine baseline and only the receipt
        // is replaced, so each drifted field isolates exactly one reachable
        // check and its error class.
        let baseline_event = event_with(genuine);
        let fixed_event_case = |receipt: NativeTrainingTrajectoryReceiptV2| {
            let mut event = baseline_event;
            event.native_full_trajectory_receipt = Some(receipt);
            let mut observer = fresh_observer();
            let error = observer.observe_terminal_v2(event).unwrap_err();
            assert!(
                observer.finish_v2().unwrap().is_empty(),
                "a rejected terminal must retain no binding"
            );
            error
        };
        for (mutation, expected) in [
            (
                NativeV2ReceiptFactMutationForTestV2::EpisodeIndex,
                NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::PairRoot,
                NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::DeckHash0,
                NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::DeckHash1,
                NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::LearnerSeat,
                NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::PolicyStepCount,
                NativeCheckpointRunnerObserverErrorV1::TerminalMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::PhysicalDecisionCount,
                NativeCheckpointRunnerObserverErrorV1::TerminalMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::LearnerPolicyStepCount,
                NativeCheckpointRunnerObserverErrorV1::TerminalMismatch,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::LearnerPhysicalDecisionCount,
                NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::OpponentPolicyStepCount,
                NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant,
            ),
            (
                NativeV2ReceiptFactMutationForTestV2::OpponentPhysicalDecisionCount,
                NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant,
            ),
        ] {
            let mut corrupted = genuine;
            corrupted.mutate_environment_fact_for_test_v2(mutation);
            assert_eq!(
                fixed_event_case(corrupted),
                expected,
                "fixed-baseline mutation {mutation:?}"
            );
        }

        // Range predicate isolation: a fully coherent episode-2 receipt and
        // event, on the correct schedule root for the evaluation seed, is
        // outside the observer's admitted [0, 2) window, so only the range
        // clause can reject it.
        let episode_two = envelope_probe_receipt_for_test_v2(
            2,
            native_trainer_episode_schedule_v1(evaluation_base_seed, 2)
                .unwrap()
                .environment_seed,
            &deck_ids,
            deck_hashes,
        );
        let mut range_observer = fresh_observer();
        assert_eq!(
            range_observer
                .observe_terminal_v2(event_with(episode_two))
                .unwrap_err(),
            NativeCheckpointRunnerObserverErrorV1::ScheduleMismatch,
            "a coherent out-of-window episode must reject on the range clause"
        );
        assert!(range_observer.finish_v2().unwrap().is_empty());

        // Terminal episode equality isolation: only the event's terminal
        // episode id drifts while the receipt stays the genuine baseline.
        let mut terminal_id_event = event_with(genuine);
        terminal_id_event.terminal.episode_id = 1;
        let mut terminal_observer = fresh_observer();
        assert_eq!(
            terminal_observer
                .observe_terminal_v2(terminal_id_event)
                .unwrap_err(),
            NativeCheckpointRunnerObserverErrorV1::TerminalMismatch,
            "a drifted terminal episode id must reject on the terminal clause"
        );
        assert!(terminal_observer.finish_v2().unwrap().is_empty());

        // Capacity isolation: the admitted range is [0, 4) but the expected
        // count is two, so a third fully coherent episode can only be
        // rejected by the capacity clause, and exactly the first two
        // bindings survive.
        let mut capacity_observer = NativeCheckpointRunnerObserverV1::new_v1(
            evaluation_base_seed,
            0,
            4,
            deck_ids.clone(),
            deck_hashes,
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
            2,
        )
        .unwrap();
        let episode_one = envelope_probe_receipt_for_test_v2(
            1,
            native_trainer_episode_schedule_v1(evaluation_base_seed, 1)
                .unwrap()
                .environment_seed,
            &deck_ids,
            deck_hashes,
        );
        capacity_observer
            .observe_terminal_v2(event_with(genuine))
            .expect("episode zero must be admitted");
        capacity_observer
            .observe_terminal_v2(event_with(episode_one))
            .expect("episode one must be admitted");
        assert_eq!(
            capacity_observer
                .observe_terminal_v2(event_with(episode_two))
                .unwrap_err(),
            NativeCheckpointRunnerObserverErrorV1::ReceiptInvariant,
            "the third coherent episode must reject on the capacity clause"
        );
        let retained = capacity_observer.finish_v2().unwrap();
        assert_eq!(retained.len(), 2, "exactly the first two bindings remain");
        assert_eq!(
            (retained[0].episode_index(), retained[1].episode_index()),
            (0, 1)
        );
    }
}
