//! Schema-neutral comparison of same-run native checkpoint evaluations.
//!
//! Both checkpoints independently play the frozen native uniform opponent under
//! one identical trainer schedule. The statistical unit retained here is the
//! complete even/odd learner-seat pair: two reference games and two candidate
//! games sharing one environment seed. This module derives exact integer facts
//! only. It adds no artifact schema, filesystem access, seed derivation,
//! confidence interval, hypothesis test, or direct checkpoint head-to-head play.

use crate::async_rollout::{AsyncRolloutEpisodeV1, AsyncRolloutTerminalV1};
use crate::native_checkpoint_runner_v1::{
    NativeCheckpointRunResultV1, NativeCheckpointRunnerConfigV1, NativeCheckpointRunnerEpisodeV1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::rl::{
    terminal_tuple_is_valid_v1, PlayerSeatV1, TerminalClassificationV1, TerminalSafeCodeV2,
};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCheckpointEvaluatorErrorV1 {
    AuthorityMismatch,
    RuntimeMismatch,
    EpisodeBindingMismatch,
    TerminalTupleMismatch,
    Arithmetic,
    Allocation,
}

impl NativeCheckpointEvaluatorErrorV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::AuthorityMismatch => "native_checkpoint_evaluator_v1_authority_mismatch",
            Self::RuntimeMismatch => "native_checkpoint_evaluator_v1_runtime_mismatch",
            Self::EpisodeBindingMismatch => {
                "native_checkpoint_evaluator_v1_episode_binding_mismatch"
            }
            Self::TerminalTupleMismatch => "native_checkpoint_evaluator_v1_terminal_tuple_mismatch",
            Self::Arithmetic => "native_checkpoint_evaluator_v1_arithmetic",
            Self::Allocation => "native_checkpoint_evaluator_v1_allocation",
        }
    }
}

impl Display for NativeCheckpointEvaluatorErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeCheckpointEvaluatorErrorV1 {}

/// Learner-perspective natural outcomes from one checkpoint against uniform.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeLearnerOutcomeCountsV1 {
    wins: u64,
    losses: u64,
    draws: u64,
}

impl NativeLearnerOutcomeCountsV1 {
    pub const fn wins(self) -> u64 {
        self.wins
    }

    pub const fn losses(self) -> u64 {
        self.losses
    }

    pub const fn draws(self) -> u64 {
        self.draws
    }

    pub const fn total(self) -> u64 {
        // Construction checks the same sum before publishing this value.
        self.wins + self.losses + self.draws
    }

    pub const fn learner_reward_sum(self) -> i128 {
        self.wins as i128 - self.losses as i128
    }
}

/// One complete common-random-number block.
///
/// Reward arrays are ordered by learner seat: index zero is the even P0-learner
/// leg and index one is the odd P1-learner leg. Every reward is canonical
/// `-1`, `0`, or `1` from a natural terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCheckpointUniformRewardPairV1 {
    pair_index: u64,
    environment_seed: u64,
    reference_rewards_by_learner_seat: [i8; 2],
    candidate_rewards_by_learner_seat: [i8; 2],
}

impl NativeCheckpointUniformRewardPairV1 {
    pub const fn pair_index(self) -> u64 {
        self.pair_index
    }

    pub const fn environment_seed(self) -> u64 {
        self.environment_seed
    }

    pub const fn reference_rewards_by_learner_seat(self) -> [i8; 2] {
        self.reference_rewards_by_learner_seat
    }

    pub const fn candidate_rewards_by_learner_seat(self) -> [i8; 2] {
        self.candidate_rewards_by_learner_seat
    }

    pub const fn reward_deltas_by_learner_seat(self) -> [i8; 2] {
        [
            self.candidate_rewards_by_learner_seat[0] - self.reference_rewards_by_learner_seat[0],
            self.candidate_rewards_by_learner_seat[1] - self.reference_rewards_by_learner_seat[1],
        ]
    }

    pub const fn reference_pair_reward(self) -> i8 {
        self.reference_rewards_by_learner_seat[0] + self.reference_rewards_by_learner_seat[1]
    }

    pub const fn candidate_pair_reward(self) -> i8 {
        self.candidate_rewards_by_learner_seat[0] + self.candidate_rewards_by_learner_seat[1]
    }

    pub const fn candidate_minus_reference_reward_delta(self) -> i8 {
        self.candidate_pair_reward() - self.reference_pair_reward()
    }
}

/// Exact comparison of two same-run checkpoint rollouts against uniform.
///
/// Fields are private and this authority is deliberately move-only and not
/// serializable. A future artifact layer must define and validate its own
/// versioned record before encoding these facts.
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_evaluator_v1::NativeCheckpointUniformDeltaEvaluationV1;
/// let _ = NativeCheckpointUniformDeltaEvaluationV1 {};
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_evaluator_v1::NativeCheckpointUniformDeltaEvaluationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<NativeCheckpointUniformDeltaEvaluationV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_evaluator_v1::NativeCheckpointUniformDeltaEvaluationV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<NativeCheckpointUniformDeltaEvaluationV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_evaluator_v1::NativeCheckpointUniformDeltaEvaluationV1;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<NativeCheckpointUniformDeltaEvaluationV1>();
/// ```
#[derive(Eq, PartialEq)]
pub struct NativeCheckpointUniformDeltaEvaluationV1 {
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    config: NativeCheckpointRunnerConfigV1,
    worker_count: usize,
    sessions_per_worker: usize,
    broker_batch_target: usize,
    deck_hashes: [u64; 2],
    reference_checkpoint_manifest_sha256: [u8; 32],
    reference_generation_index: u64,
    candidate_checkpoint_manifest_sha256: [u8; 32],
    candidate_generation_index: u64,
    reference_learner_outcomes: NativeLearnerOutcomeCountsV1,
    candidate_learner_outcomes: NativeLearnerOutcomeCountsV1,
    pair_count: u64,
    leg_count: u64,
    total_candidate_minus_reference_reward_delta: i128,
    reward_pairs: Vec<NativeCheckpointUniformRewardPairV1>,
}

impl Debug for NativeCheckpointUniformDeltaEvaluationV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointUniformDeltaEvaluationV1")
            .field("run_sha256", &lower_hex_raw32_v1(self.run_sha256))
            .field(
                "reference_checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.reference_checkpoint_manifest_sha256),
            )
            .field(
                "candidate_checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.candidate_checkpoint_manifest_sha256),
            )
            .field(
                "reference_generation_index",
                &self.reference_generation_index,
            )
            .field(
                "candidate_generation_index",
                &self.candidate_generation_index,
            )
            .field("pair_count", &self.pair_count)
            .field("leg_count", &self.leg_count)
            .field(
                "total_candidate_minus_reference_reward_delta",
                &self.total_candidate_minus_reference_reward_delta,
            )
            .finish_non_exhaustive()
    }
}

impl NativeCheckpointUniformDeltaEvaluationV1 {
    pub const fn run_sha256(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub const fn identity_bundle_sha256(&self) -> [u8; 32] {
        self.identity_bundle_sha256
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

    pub const fn deck_hashes(&self) -> [u64; 2] {
        self.deck_hashes
    }

    pub const fn reference_checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.reference_checkpoint_manifest_sha256
    }

    pub const fn reference_generation_index(&self) -> u64 {
        self.reference_generation_index
    }

    pub const fn candidate_checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.candidate_checkpoint_manifest_sha256
    }

    pub const fn candidate_generation_index(&self) -> u64 {
        self.candidate_generation_index
    }

    pub const fn reference_learner_outcomes(&self) -> NativeLearnerOutcomeCountsV1 {
        self.reference_learner_outcomes
    }

    pub const fn candidate_learner_outcomes(&self) -> NativeLearnerOutcomeCountsV1 {
        self.candidate_learner_outcomes
    }

    pub const fn pair_count(&self) -> u64 {
        self.pair_count
    }

    pub const fn leg_count(&self) -> u64 {
        self.leg_count
    }

    pub const fn total_candidate_minus_reference_reward_delta(&self) -> i128 {
        self.total_candidate_minus_reference_reward_delta
    }

    pub fn reward_pairs(&self) -> &[NativeCheckpointUniformRewardPairV1] {
        &self.reward_pairs
    }
}

/// Compare two same-run checkpoint rollouts against the frozen uniform opponent.
///
/// The only public admission boundary consumes the nonforgeable runner result;
/// cloneable raw rollout results, episode slices, and models are not accepted.
/// Deltas are positional and always mean `candidate - reference`.
///
/// ```compile_fail
/// use mtg_kernel::async_flat_scored_rollout_v2::AsyncFlatScoredRolloutResultV2;
/// use mtg_kernel::native_checkpoint_evaluator_v1::evaluate_native_checkpoint_uniform_delta_v1;
/// fn forged(reference: &AsyncFlatScoredRolloutResultV2, candidate: &AsyncFlatScoredRolloutResultV2) {
///     let _ = evaluate_native_checkpoint_uniform_delta_v1(reference, candidate);
/// }
/// ```
pub fn evaluate_native_checkpoint_uniform_delta_v1(
    reference: &NativeCheckpointRunResultV1,
    candidate: &NativeCheckpointRunResultV1,
) -> Result<NativeCheckpointUniformDeltaEvaluationV1, NativeCheckpointEvaluatorErrorV1> {
    evaluate_views_v1(
        CheckpointEvaluationViewV1::from_runner_result_v1(reference),
        CheckpointEvaluationViewV1::from_runner_result_v1(candidate),
        PairAllocationV1::Fallible,
    )
}

trait EpisodeBindingFactsV1 {
    fn episode_index_v1(&self) -> u64;
    fn environment_seed_v1(&self) -> u64;
    fn deck_hashes_v1(&self) -> [u64; 2];
    fn learner_seat_v1(&self) -> PlayerSeatV1;
}

impl EpisodeBindingFactsV1 for NativeCheckpointRunnerEpisodeV1 {
    fn episode_index_v1(&self) -> u64 {
        self.episode_index()
    }

    fn environment_seed_v1(&self) -> u64 {
        self.environment_seed()
    }

    fn deck_hashes_v1(&self) -> [u64; 2] {
        self.deck_hashes()
    }

    fn learner_seat_v1(&self) -> PlayerSeatV1 {
        self.learner_seat()
    }
}

struct CheckpointEvaluationViewV1<'a, B> {
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    config: NativeCheckpointRunnerConfigV1,
    worker_count: usize,
    sessions_per_worker: usize,
    broker_batch_target: usize,
    bindings: &'a [B],
    episodes: &'a [AsyncRolloutEpisodeV1],
}

impl<'a> CheckpointEvaluationViewV1<'a, NativeCheckpointRunnerEpisodeV1> {
    fn from_runner_result_v1(result: &'a NativeCheckpointRunResultV1) -> Self {
        Self {
            run_sha256: result.run_sha256(),
            identity_bundle_sha256: result.identity_bundle_sha256(),
            checkpoint_manifest_sha256: result.checkpoint_manifest_sha256(),
            generation_index: result.generation_index(),
            batch_episodes: result.batch_episodes(),
            checkpoint_segment_updates: result.checkpoint_segment_updates(),
            config: result.config(),
            worker_count: result.worker_count(),
            sessions_per_worker: result.sessions_per_worker(),
            broker_batch_target: result.broker_batch_target(),
            bindings: result.episode_bindings(),
            episodes: &result.rollout().episodes,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PairAllocationV1 {
    Fallible,
    #[cfg(test)]
    InjectFailure,
}

fn reserve_pair_rows_v1(
    rows: &mut Vec<NativeCheckpointUniformRewardPairV1>,
    pair_count: usize,
    allocation: PairAllocationV1,
) -> Result<(), NativeCheckpointEvaluatorErrorV1> {
    match allocation {
        PairAllocationV1::Fallible => rows
            .try_reserve_exact(pair_count)
            .map_err(|_| NativeCheckpointEvaluatorErrorV1::Allocation),
        #[cfg(test)]
        PairAllocationV1::InjectFailure => Err(NativeCheckpointEvaluatorErrorV1::Allocation),
    }
}

fn learner_reward_v1(
    terminal: &AsyncRolloutTerminalV1,
    learner_seat: PlayerSeatV1,
) -> Result<i8, NativeCheckpointEvaluatorErrorV1> {
    if terminal.terminal_classification != TerminalClassificationV1::Natural
        || terminal.terminal_code != TerminalSafeCodeV2::NaturalGameOver
        || !terminal_tuple_is_valid_v1(
            terminal.terminal_outcome,
            terminal.terminal_classification,
            terminal.winner,
            terminal.terminal_reward,
        )
    {
        return Err(NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch);
    }
    let reward_index = match learner_seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    };
    i8::try_from(terminal.terminal_reward[reward_index])
        .ok()
        .filter(|reward| matches!(*reward, -1..=1))
        .ok_or(NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch)
}

fn record_reward_v1(
    outcomes: &mut NativeLearnerOutcomeCountsV1,
    reward: i8,
) -> Result<(), NativeCheckpointEvaluatorErrorV1> {
    let counter = match reward {
        1 => &mut outcomes.wins,
        -1 => &mut outcomes.losses,
        0 => &mut outcomes.draws,
        _ => return Err(NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch),
    };
    *counter = counter
        .checked_add(1)
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    Ok(())
}

fn checked_outcome_total_v1(
    outcomes: NativeLearnerOutcomeCountsV1,
) -> Result<u64, NativeCheckpointEvaluatorErrorV1> {
    outcomes
        .wins
        .checked_add(outcomes.losses)
        .and_then(|total| total.checked_add(outcomes.draws))
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)
}

fn checked_add_i128_v1(left: i128, right: i128) -> Result<i128, NativeCheckpointEvaluatorErrorV1> {
    left.checked_add(right)
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)
}

fn evaluate_views_v1<B: EpisodeBindingFactsV1>(
    reference: CheckpointEvaluationViewV1<'_, B>,
    candidate: CheckpointEvaluationViewV1<'_, B>,
    allocation: PairAllocationV1,
) -> Result<NativeCheckpointUniformDeltaEvaluationV1, NativeCheckpointEvaluatorErrorV1> {
    if reference.run_sha256 != candidate.run_sha256
        || reference.identity_bundle_sha256 != candidate.identity_bundle_sha256
    {
        return Err(NativeCheckpointEvaluatorErrorV1::AuthorityMismatch);
    }
    if reference.config != candidate.config
        || reference.batch_episodes != candidate.batch_episodes
        || reference.checkpoint_segment_updates != candidate.checkpoint_segment_updates
        || reference.worker_count != candidate.worker_count
        || reference.sessions_per_worker != candidate.sessions_per_worker
        || reference.broker_batch_target != candidate.broker_batch_target
    {
        return Err(NativeCheckpointEvaluatorErrorV1::RuntimeMismatch);
    }

    let config = reference.config;
    if config.episode_count == 0
        || !config.episode_count.is_multiple_of(2)
        || !config.first_episode_index.is_multiple_of(2)
    {
        return Err(NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch);
    }
    config
        .first_episode_index
        .checked_add(config.episode_count)
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    let expected_leg_count = usize::try_from(config.episode_count)
        .map_err(|_| NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    if reference.bindings.len() != expected_leg_count
        || candidate.bindings.len() != expected_leg_count
        || reference.episodes.len() != expected_leg_count
        || candidate.episodes.len() != expected_leg_count
    {
        return Err(NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch);
    }
    if reference.batch_episodes == 0 || reference.checkpoint_segment_updates == 0 {
        return Err(NativeCheckpointEvaluatorErrorV1::RuntimeMismatch);
    }
    let logical_actor_count = reference
        .worker_count
        .checked_mul(reference.sessions_per_worker)
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    if reference.worker_count == 0
        || reference.sessions_per_worker == 0
        || !(1..=logical_actor_count).contains(&reference.broker_batch_target)
    {
        return Err(NativeCheckpointEvaluatorErrorV1::RuntimeMismatch);
    }

    let pair_count_usize = expected_leg_count / 2;
    let pair_count = u64::try_from(pair_count_usize)
        .map_err(|_| NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    let leg_count = u64::try_from(expected_leg_count)
        .map_err(|_| NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    if pair_count
        .checked_mul(2)
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?
        != leg_count
        || leg_count != config.episode_count
    {
        return Err(NativeCheckpointEvaluatorErrorV1::Arithmetic);
    }

    let deck_hashes = reference.bindings[0].deck_hashes_v1();
    let mut reference_learner_outcomes = NativeLearnerOutcomeCountsV1::default();
    let mut candidate_learner_outcomes = NativeLearnerOutcomeCountsV1::default();
    let mut reward_pairs = Vec::new();
    reserve_pair_rows_v1(&mut reward_pairs, pair_count_usize, allocation)?;
    let mut total_candidate_minus_reference_reward_delta = 0_i128;

    for pair_offset in 0..pair_count_usize {
        let mut pair_index = None;
        let mut environment_seed = None;
        let mut reference_rewards = [0_i8; 2];
        let mut candidate_rewards = [0_i8; 2];
        for leg_index in 0..2_usize {
            let offset_usize = pair_offset
                .checked_mul(2)
                .and_then(|offset| offset.checked_add(leg_index))
                .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
            let offset = u64::try_from(offset_usize)
                .map_err(|_| NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
            let expected_episode_index = config
                .first_episode_index
                .checked_add(offset)
                .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
            let schedule = native_trainer_episode_schedule_v1(
                config.evaluation_base_seed,
                expected_episode_index,
            )
            .map_err(|_| NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch)?;
            let expected_learner_seat = if leg_index == 0 {
                PlayerSeatV1::P0
            } else {
                PlayerSeatV1::P1
            };
            let reference_binding = &reference.bindings[offset_usize];
            let candidate_binding = &candidate.bindings[offset_usize];
            if schedule.episode_index != expected_episode_index
                || schedule.learner_seat != expected_learner_seat
                || reference_binding.episode_index_v1() != expected_episode_index
                || candidate_binding.episode_index_v1() != expected_episode_index
                || reference_binding.environment_seed_v1() != schedule.environment_seed
                || candidate_binding.environment_seed_v1() != schedule.environment_seed
                || reference_binding.deck_hashes_v1() != deck_hashes
                || candidate_binding.deck_hashes_v1() != deck_hashes
                || reference_binding.learner_seat_v1() != schedule.learner_seat
                || candidate_binding.learner_seat_v1() != schedule.learner_seat
            {
                return Err(NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch);
            }
            match (pair_index, environment_seed) {
                (None, None) => {
                    pair_index = Some(schedule.pair_index);
                    environment_seed = Some(schedule.environment_seed);
                }
                (Some(first_pair_index), Some(first_environment_seed))
                    if first_pair_index == schedule.pair_index
                        && first_environment_seed == schedule.environment_seed => {}
                _ => return Err(NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch),
            }

            let reference_terminal = &reference.episodes[offset_usize].terminal;
            let candidate_terminal = &candidate.episodes[offset_usize].terminal;
            if reference_terminal.episode_id != expected_episode_index
                || candidate_terminal.episode_id != expected_episode_index
            {
                return Err(NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch);
            }
            let reference_reward = learner_reward_v1(reference_terminal, schedule.learner_seat)?;
            let candidate_reward = learner_reward_v1(candidate_terminal, schedule.learner_seat)?;
            record_reward_v1(&mut reference_learner_outcomes, reference_reward)?;
            record_reward_v1(&mut candidate_learner_outcomes, candidate_reward)?;
            reference_rewards[leg_index] = reference_reward;
            candidate_rewards[leg_index] = candidate_reward;
        }

        let row = NativeCheckpointUniformRewardPairV1 {
            pair_index: pair_index.ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?,
            environment_seed: environment_seed
                .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?,
            reference_rewards_by_learner_seat: reference_rewards,
            candidate_rewards_by_learner_seat: candidate_rewards,
        };
        total_candidate_minus_reference_reward_delta = checked_add_i128_v1(
            total_candidate_minus_reference_reward_delta,
            i128::from(row.candidate_minus_reference_reward_delta()),
        )?;
        reward_pairs.push(row);
    }

    if checked_outcome_total_v1(reference_learner_outcomes)? != leg_count
        || checked_outcome_total_v1(candidate_learner_outcomes)? != leg_count
        || u64::try_from(reward_pairs.len())
            .map_err(|_| NativeCheckpointEvaluatorErrorV1::Arithmetic)?
            != pair_count
    {
        return Err(NativeCheckpointEvaluatorErrorV1::Arithmetic);
    }
    let summary_delta = candidate_learner_outcomes
        .learner_reward_sum()
        .checked_sub(reference_learner_outcomes.learner_reward_sum())
        .ok_or(NativeCheckpointEvaluatorErrorV1::Arithmetic)?;
    if summary_delta != total_candidate_minus_reference_reward_delta {
        return Err(NativeCheckpointEvaluatorErrorV1::Arithmetic);
    }

    Ok(NativeCheckpointUniformDeltaEvaluationV1 {
        run_sha256: reference.run_sha256,
        identity_bundle_sha256: reference.identity_bundle_sha256,
        config,
        worker_count: reference.worker_count,
        sessions_per_worker: reference.sessions_per_worker,
        broker_batch_target: reference.broker_batch_target,
        deck_hashes,
        reference_checkpoint_manifest_sha256: reference.checkpoint_manifest_sha256,
        reference_generation_index: reference.generation_index,
        candidate_checkpoint_manifest_sha256: candidate.checkpoint_manifest_sha256,
        candidate_generation_index: candidate.generation_index,
        reference_learner_outcomes,
        candidate_learner_outcomes,
        pair_count,
        leg_count,
        total_candidate_minus_reference_reward_delta,
        reward_pairs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_checkpoint_runner_v1::run_native_checkpoint_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
        decode_genesis_checkpoint_manifest_v3, CheckpointManifestV3,
    };
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_v2, ValidatedTrainRunV2,
    };
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1,
    };
    use crate::rl::TerminalOutcomeV1;
    use std::sync::OnceLock;
    use std::time::Duration;

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
            episode_count: 4,
            scheduler_timeout: Duration::from_secs(60),
            measure_broker_service_time: false,
        }
    }

    #[test]
    fn genuine_same_checkpoint_repeat_has_exact_zero_deltas() {
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        let reference =
            run_native_checkpoint_v1(&run, &checkpoint, &fixture.payload, runner_config_v1())
                .unwrap();
        let candidate =
            run_native_checkpoint_v1(&run, &checkpoint, &fixture.payload, runner_config_v1())
                .unwrap();

        let evaluated =
            evaluate_native_checkpoint_uniform_delta_v1(&reference, &candidate).unwrap();
        assert_eq!(evaluated.run_sha256(), reference.run_sha256());
        assert_eq!(
            evaluated.identity_bundle_sha256(),
            reference.identity_bundle_sha256()
        );
        assert_eq!(
            evaluated.reference_checkpoint_manifest_sha256(),
            checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(
            evaluated.candidate_checkpoint_manifest_sha256(),
            checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!((evaluated.pair_count(), evaluated.leg_count()), (2, 4));
        assert_eq!(evaluated.reference_learner_outcomes().total(), 4);
        assert_eq!(evaluated.candidate_learner_outcomes().total(), 4);
        assert_eq!(
            evaluated.reference_learner_outcomes(),
            evaluated.candidate_learner_outcomes()
        );
        assert_eq!(evaluated.total_candidate_minus_reference_reward_delta(), 0);
        assert_eq!(
            evaluated
                .reward_pairs()
                .iter()
                .map(|pair| pair.pair_index())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        for pair in evaluated.reward_pairs() {
            assert_eq!(
                pair.reference_rewards_by_learner_seat(),
                pair.candidate_rewards_by_learner_seat()
            );
            assert_eq!(pair.reward_deltas_by_learner_seat(), [0, 0]);
            assert_eq!(pair.candidate_minus_reference_reward_delta(), 0);
        }
    }

    #[test]
    fn genuine_genesis_and_k2_s4_generation_four_compare_and_swap_exactly() {
        let fixture = fixture_v1();
        let trained = trained_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let genesis = decode_genesis_checkpoint_manifest_v3(
            &fixture.checkpoint_bytes,
            &fixture.payload,
            &run,
        )
        .unwrap();
        let reference =
            run_native_checkpoint_v1(&run, &genesis, &fixture.payload, runner_config_v1()).unwrap();
        let candidate = run_native_checkpoint_v1(
            &run,
            &trained.checkpoint,
            &trained.payload,
            runner_config_v1(),
        )
        .unwrap();
        assert_eq!(
            (
                candidate.batch_episodes(),
                candidate.checkpoint_segment_updates()
            ),
            (2, 4)
        );
        assert_eq!(
            (reference.generation_index(), candidate.generation_index()),
            (0, 4)
        );
        assert_ne!(
            reference.model_parameter_sha256(),
            candidate.model_parameter_sha256()
        );
        assert!(reference
            .episode_bindings()
            .iter()
            .zip(candidate.episode_bindings())
            .any(|(left, right)| {
                left.trajectory_sha256() != right.trajectory_sha256()
                    || left.policy_step_count() != right.policy_step_count()
                    || left.physical_decision_count() != right.physical_decision_count()
            }));

        let forward = evaluate_native_checkpoint_uniform_delta_v1(&reference, &candidate).unwrap();
        let reverse = evaluate_native_checkpoint_uniform_delta_v1(&candidate, &reference).unwrap();
        assert_eq!(forward.reference_generation_index(), 0);
        assert_eq!(forward.candidate_generation_index(), 4);
        assert_eq!(
            forward.reference_learner_outcomes().total(),
            forward.leg_count()
        );
        assert_eq!(
            forward.candidate_learner_outcomes().total(),
            forward.leg_count()
        );
        assert_eq!(
            forward
                .reward_pairs()
                .iter()
                .map(|pair| i128::from(pair.candidate_minus_reference_reward_delta()))
                .sum::<i128>(),
            forward.total_candidate_minus_reference_reward_delta()
        );
        assert_eq!(
            forward.candidate_learner_outcomes().learner_reward_sum()
                - forward.reference_learner_outcomes().learner_reward_sum(),
            forward.total_candidate_minus_reference_reward_delta()
        );
        assert_eq!(
            forward.reference_learner_outcomes(),
            reverse.candidate_learner_outcomes()
        );
        assert_eq!(
            forward.candidate_learner_outcomes(),
            reverse.reference_learner_outcomes()
        );
        assert_eq!(
            forward.total_candidate_minus_reference_reward_delta(),
            -reverse.total_candidate_minus_reference_reward_delta()
        );
        assert_eq!(forward.reward_pairs().len(), reverse.reward_pairs().len());
        for (forward_pair, reverse_pair) in
            forward.reward_pairs().iter().zip(reverse.reward_pairs())
        {
            assert_eq!(forward_pair.pair_index(), reverse_pair.pair_index());
            assert_eq!(
                forward_pair.environment_seed(),
                reverse_pair.environment_seed()
            );
            assert_eq!(
                forward_pair.reference_rewards_by_learner_seat(),
                reverse_pair.candidate_rewards_by_learner_seat()
            );
            assert_eq!(
                forward_pair.candidate_rewards_by_learner_seat(),
                reverse_pair.reference_rewards_by_learner_seat()
            );
            assert_eq!(
                forward_pair.candidate_minus_reference_reward_delta(),
                -reverse_pair.candidate_minus_reference_reward_delta()
            );
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct SyntheticBindingV1 {
        episode_index: u64,
        environment_seed: u64,
        deck_hashes: [u64; 2],
        learner_seat: PlayerSeatV1,
        // Deliberately absent from the evaluator projection.
        trajectory_sha256: [u8; 32],
        policy_step_count: u64,
        physical_decision_count: u64,
    }

    impl EpisodeBindingFactsV1 for SyntheticBindingV1 {
        fn episode_index_v1(&self) -> u64 {
            self.episode_index
        }

        fn environment_seed_v1(&self) -> u64 {
            self.environment_seed
        }

        fn deck_hashes_v1(&self) -> [u64; 2] {
            self.deck_hashes
        }

        fn learner_seat_v1(&self) -> PlayerSeatV1 {
            self.learner_seat
        }
    }

    #[derive(Clone)]
    struct SyntheticInputV1 {
        run_sha256: [u8; 32],
        identity_bundle_sha256: [u8; 32],
        checkpoint_manifest_sha256: [u8; 32],
        generation_index: u64,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        config: NativeCheckpointRunnerConfigV1,
        worker_count: usize,
        sessions_per_worker: usize,
        broker_batch_target: usize,
        bindings: Vec<SyntheticBindingV1>,
        episodes: Vec<AsyncRolloutEpisodeV1>,
    }

    impl SyntheticInputV1 {
        fn view_v1(&self) -> CheckpointEvaluationViewV1<'_, SyntheticBindingV1> {
            CheckpointEvaluationViewV1 {
                run_sha256: self.run_sha256,
                identity_bundle_sha256: self.identity_bundle_sha256,
                checkpoint_manifest_sha256: self.checkpoint_manifest_sha256,
                generation_index: self.generation_index,
                batch_episodes: self.batch_episodes,
                checkpoint_segment_updates: self.checkpoint_segment_updates,
                config: self.config,
                worker_count: self.worker_count,
                sessions_per_worker: self.sessions_per_worker,
                broker_batch_target: self.broker_batch_target,
                bindings: &self.bindings,
                episodes: &self.episodes,
            }
        }
    }

    fn natural_terminal_v1(episode_id: u64, outcome: TerminalOutcomeV1) -> AsyncRolloutEpisodeV1 {
        let (winner, terminal_reward) = match outcome {
            TerminalOutcomeV1::P0Win => (Some(PlayerSeatV1::P0), [1, -1]),
            TerminalOutcomeV1::P1Win => (Some(PlayerSeatV1::P1), [-1, 1]),
            TerminalOutcomeV1::Draw => (None, [0, 0]),
            TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
                panic!("synthetic natural terminal requires a natural outcome")
            }
        };
        AsyncRolloutEpisodeV1 {
            terminal: AsyncRolloutTerminalV1 {
                episode_id,
                terminal_outcome: outcome,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner,
                terminal_reward,
                policy_step_count: 5,
                physical_decision_count: 3,
            },
            learner_action_count: 2,
            learner_trace_hash: episode_id ^ 0x55aa,
        }
    }

    fn synthetic_input_v1(outcomes: &[TerminalOutcomeV1]) -> SyntheticInputV1 {
        assert!(!outcomes.is_empty() && outcomes.len().is_multiple_of(2));
        let first_episode_index = 2_u64;
        let evaluation_base_seed = 91_501_u64;
        let deck_hashes = [11_u64, 11_u64];
        let mut bindings = Vec::new();
        let mut episodes = Vec::new();
        for (offset, outcome) in outcomes.iter().copied().enumerate() {
            let episode_index = first_episode_index + u64::try_from(offset).unwrap();
            let schedule =
                native_trainer_episode_schedule_v1(evaluation_base_seed, episode_index).unwrap();
            bindings.push(SyntheticBindingV1 {
                episode_index,
                environment_seed: schedule.environment_seed,
                deck_hashes,
                learner_seat: schedule.learner_seat,
                trajectory_sha256: [u8::try_from(offset + 1).unwrap(); 32],
                policy_step_count: 5,
                physical_decision_count: 3,
            });
            episodes.push(natural_terminal_v1(episode_index, outcome));
        }
        SyntheticInputV1 {
            run_sha256: [1; 32],
            identity_bundle_sha256: [2; 32],
            checkpoint_manifest_sha256: [3; 32],
            generation_index: 0,
            batch_episodes: 2,
            checkpoint_segment_updates: 4,
            config: NativeCheckpointRunnerConfigV1 {
                evaluation_base_seed,
                first_episode_index,
                episode_count: u64::try_from(outcomes.len()).unwrap(),
                scheduler_timeout: Duration::from_secs(60),
                measure_broker_service_time: false,
            },
            worker_count: 1,
            sessions_per_worker: 2,
            broker_batch_target: 2,
            bindings,
            episodes,
        }
    }

    fn evaluate_synthetic_v1(
        reference: &SyntheticInputV1,
        candidate: &SyntheticInputV1,
    ) -> Result<NativeCheckpointUniformDeltaEvaluationV1, NativeCheckpointEvaluatorErrorV1> {
        evaluate_views_v1(
            reference.view_v1(),
            candidate.view_v1(),
            PairAllocationV1::Fallible,
        )
    }

    fn assert_synthetic_error_v1(
        reference: &SyntheticInputV1,
        candidate: &SyntheticInputV1,
        expected: NativeCheckpointEvaluatorErrorV1,
    ) {
        assert_eq!(
            evaluate_synthetic_v1(reference, candidate).unwrap_err(),
            expected
        );
    }

    #[test]
    fn synthetic_outcomes_are_learner_centric_for_both_seats_and_draws() {
        let outcomes = [
            TerminalOutcomeV1::P0Win,
            TerminalOutcomeV1::P0Win,
            TerminalOutcomeV1::P1Win,
            TerminalOutcomeV1::P1Win,
            TerminalOutcomeV1::Draw,
            TerminalOutcomeV1::Draw,
        ];
        let reference = synthetic_input_v1(&outcomes);
        let mut candidate = reference.clone();
        candidate.checkpoint_manifest_sha256 = [4; 32];
        candidate.generation_index = 4;
        let evaluated = evaluate_synthetic_v1(&reference, &candidate).unwrap();

        assert_eq!(
            evaluated.reference_learner_outcomes(),
            NativeLearnerOutcomeCountsV1 {
                wins: 2,
                losses: 2,
                draws: 2,
            }
        );
        assert_eq!(
            evaluated.reference_learner_outcomes(),
            evaluated.candidate_learner_outcomes()
        );
        assert_eq!(
            evaluated.reward_pairs()[0].reference_rewards_by_learner_seat(),
            [1, -1]
        );
        assert_eq!(
            evaluated.reward_pairs()[1].reference_rewards_by_learner_seat(),
            [-1, 1]
        );
        assert_eq!(
            evaluated.reward_pairs()[2].reference_rewards_by_learner_seat(),
            [0, 0]
        );
        assert_eq!(evaluated.total_candidate_minus_reference_reward_delta(), 0);
    }

    #[test]
    fn trajectory_and_decision_count_facts_may_differ() {
        let reference = synthetic_input_v1(&[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P1Win]);
        let mut candidate = reference.clone();
        candidate.checkpoint_manifest_sha256 = [4; 32];
        candidate.generation_index = 4;
        for binding in &mut candidate.bindings {
            binding.trajectory_sha256 = [99; 32];
            binding.policy_step_count = 9_999;
            binding.physical_decision_count = 7_777;
        }
        let evaluated = evaluate_synthetic_v1(&reference, &candidate).unwrap();
        assert_eq!(evaluated.total_candidate_minus_reference_reward_delta(), 0);
    }

    #[test]
    fn synthetic_authority_config_k_s_and_topology_mismatch_matrix_fails_closed() {
        let base = synthetic_input_v1(&[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P1Win]);

        let mut changed = base.clone();
        changed.run_sha256 = [9; 32];
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::AuthorityMismatch,
        );
        let mut changed = base.clone();
        changed.identity_bundle_sha256 = [9; 32];
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::AuthorityMismatch,
        );

        let mut changed = base.clone();
        changed.config.evaluation_base_seed += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.config.first_episode_index += 2;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.config.episode_count += 2;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.config.scheduler_timeout = Duration::from_secs(61);
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.config.measure_broker_service_time = true;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );

        let mut changed = base.clone();
        changed.batch_episodes += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.checkpoint_segment_updates += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.worker_count += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.sessions_per_worker += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
        let mut changed = base.clone();
        changed.broker_batch_target -= 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::RuntimeMismatch,
        );
    }

    #[test]
    fn synthetic_range_binding_and_pair_mismatch_matrix_fails_closed() {
        let base = synthetic_input_v1(&[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P1Win]);

        let mut left = base.clone();
        let mut right = base.clone();
        left.config.first_episode_index = 3;
        right.config.first_episode_index = 3;
        assert_synthetic_error_v1(
            &left,
            &right,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut left = base.clone();
        let mut right = base.clone();
        left.config.episode_count = 3;
        right.config.episode_count = 3;
        assert_synthetic_error_v1(
            &left,
            &right,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut left = base.clone();
        let mut right = base.clone();
        left.config.episode_count = 0;
        right.config.episode_count = 0;
        assert_synthetic_error_v1(
            &left,
            &right,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut left = base.clone();
        let mut right = base.clone();
        left.config.first_episode_index = u64::MAX - 1;
        right.config.first_episode_index = u64::MAX - 1;
        assert_synthetic_error_v1(&left, &right, NativeCheckpointEvaluatorErrorV1::Arithmetic);
        let mut left = base.clone();
        let mut right = base.clone();
        left.config.evaluation_base_seed = 1_u64 << 63;
        right.config.evaluation_base_seed = 1_u64 << 63;
        assert_synthetic_error_v1(
            &left,
            &right,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );

        let mut changed = base.clone();
        changed.bindings.pop();
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.episodes.pop();
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.bindings.swap(0, 1);
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.episodes.swap(0, 1);
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );

        let mut changed = base.clone();
        changed.bindings[0].episode_index += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.bindings[0].environment_seed ^= 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.bindings[0].deck_hashes[0] ^= 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.bindings[0].learner_seat = PlayerSeatV1::P1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.bindings[1].environment_seed ^= 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
    }

    #[test]
    fn synthetic_terminal_tuple_mismatch_matrix_fails_closed() {
        let base = synthetic_input_v1(&[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P1Win]);

        let mut changed = base.clone();
        changed.episodes[0].terminal.episode_id += 1;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::EpisodeBindingMismatch,
        );
        let mut changed = base.clone();
        changed.episodes[0].terminal.terminal_outcome = TerminalOutcomeV1::P1Win;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch,
        );
        let mut changed = base.clone();
        changed.episodes[0].terminal.terminal_classification = TerminalClassificationV1::Truncated;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch,
        );
        let mut changed = base.clone();
        changed.episodes[0].terminal.terminal_code = TerminalSafeCodeV2::DecisionCap;
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch,
        );
        let mut changed = base.clone();
        changed.episodes[0].terminal.winner = Some(PlayerSeatV1::P1);
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch,
        );
        let mut changed = base.clone();
        changed.episodes[0].terminal.terminal_reward = [0, 0];
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch,
        );
        let mut changed = base.clone();
        changed.episodes[0].terminal.terminal_outcome = TerminalOutcomeV1::Truncated;
        changed.episodes[0].terminal.terminal_classification = TerminalClassificationV1::Truncated;
        changed.episodes[0].terminal.terminal_code = TerminalSafeCodeV2::DecisionCap;
        changed.episodes[0].terminal.winner = None;
        changed.episodes[0].terminal.terminal_reward = [0, 0];
        assert_synthetic_error_v1(
            &base,
            &changed,
            NativeCheckpointEvaluatorErrorV1::TerminalTupleMismatch,
        );
    }

    #[test]
    fn invalid_runtime_arithmetic_and_allocation_fail_closed() {
        let base = synthetic_input_v1(&[TerminalOutcomeV1::P0Win, TerminalOutcomeV1::P1Win]);
        let mut left = base.clone();
        let mut right = base.clone();
        left.worker_count = usize::MAX;
        left.sessions_per_worker = 2;
        right.worker_count = usize::MAX;
        right.sessions_per_worker = 2;
        assert_synthetic_error_v1(&left, &right, NativeCheckpointEvaluatorErrorV1::Arithmetic);

        assert_eq!(
            evaluate_views_v1(
                base.view_v1(),
                base.view_v1(),
                PairAllocationV1::InjectFailure,
            )
            .unwrap_err(),
            NativeCheckpointEvaluatorErrorV1::Allocation
        );
        assert_eq!(
            checked_add_i128_v1(i128::MAX, 1).unwrap_err(),
            NativeCheckpointEvaluatorErrorV1::Arithmetic
        );
        let mut outcomes = NativeLearnerOutcomeCountsV1 {
            wins: u64::MAX,
            losses: 0,
            draws: 0,
        };
        assert_eq!(
            record_reward_v1(&mut outcomes, 1).unwrap_err(),
            NativeCheckpointEvaluatorErrorV1::Arithmetic
        );
        assert_eq!(
            checked_outcome_total_v1(NativeLearnerOutcomeCountsV1 {
                wins: u64::MAX,
                losses: 1,
                draws: 0,
            })
            .unwrap_err(),
            NativeCheckpointEvaluatorErrorV1::Arithmetic
        );
    }
}
