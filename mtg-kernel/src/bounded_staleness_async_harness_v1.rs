//! A/B validation harness and throughput self-check for bounded-staleness
//! async inference ([`crate::bounded_staleness_async_v1`]).
//!
//! Condition (d) of CLAUDE #191 requires "a matched A/B validation harness
//! ... so a campaign can prove learning-quality preservation before adopting
//! the async path." This module is that harness's mechanism: it runs two
//! matched arms (sync, async) over the same deterministic seed schedule and
//! reports throughput and a per-update reward-curve proxy for each.
//!
//! ## What this harness does and does not prove
//!
//! Both arms use [`SmokeScorerV1`], a fixed, weight-independent stand-in
//! scorer (it never reads the `weights` argument the scheduler hands it).
//! That makes the harness's rollout content, index for index, provably
//! identical between arms under matched seeds regardless of staleness
//! timing (see `matched_seeds_yield_identical_episode_content_regardless_of_
//! arm` below) -- a real, useful proof that the scheduling mechanism does
//! not corrupt or reorder episode content. It does **not** prove
//! learning-quality equivalence: a real trainer's scorer output depends on
//! the weight version it reads, and this harness's does not. A future
//! campaign qualifying the async path for production must rerun this same
//! harness shape against the real trainer/model, not this stand-in. This
//! module implements the harness only; per the task, no full validation
//! campaign is run here.
//!
//! The example binary `examples/bounded_staleness_async_smoke_v1.rs` is a
//! runnable script wrapping [`run_matched_ab_v1`] at "a few minutes" scale,
//! for the throughput self-check.

use crate::async_flat_scored_rollout_v1::{
    run_async_flat_scored_rollout_v1, AsyncFlatScoredRolloutResultV1, FlatBatchScorerErrorV1,
    FlatBatchScorerV1, FlatScoringBatchViewV1,
};
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::bounded_staleness_async_v1::{BoundedStalenessConsumeErrorV1, BoundedStalenessSchedulerV1};
use crate::rl::PlayerSeatV1;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const HARNESS_LIMITATIONS_NOTE_V1: &str =
    "Both arms score episodes with a fixed, weight-independent stand-in \
     scorer and simulate the trainer's non-rollout wall (gradient step plus \
     Store commit) with a configured sleep rather than a real train step. \
     Reward-curve equality between arms here demonstrates the scheduler does \
     not corrupt or reorder episode content, not that learning quality is \
     preserved under a real model: qualifying the async path for production \
     requires rerunning this harness shape against the real trainer, per \
     CLAUDE #191 condition (d).";

/// Weight-independent stand-in "model": carries only a version identity, no
/// parameters. Real integration plugs in the trainer's actual weight
/// snapshot type here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmokeWeightsV1 {
    pub version: u64,
}

/// Always emits zero logits/values, so selection is uniform-random and the
/// scorer never reads `weights`: rollout content depends only on the
/// deterministic seed schedule, never on which weight version "scored" it.
/// Modeled directly on the crate's own `ZeroScorer` test fixture (async_
/// flat_scored_rollout_v1.rs) so its shape validation matches production.
#[derive(Debug, Default)]
pub struct SmokeScorerV1;

impl FlatBatchScorerV1 for SmokeScorerV1 {
    fn score_batch_v1(
        &mut self,
        batch: &FlatScoringBatchViewV1<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV1> {
        debug_assert_eq!(values.len(), batch.decision_count());
        debug_assert_eq!(action_logits.len(), batch.total_action_count());
        action_logits.fill(0.0);
        values.fill(0.0);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HarnessConfigV1 {
    pub deck_ids: [String; 2],
    pub base_environment_seed: u64,
    pub opponent_policy_seed: u64,
    pub learner_policy_seed: u64,
    pub batch_episodes: u64,
    pub total_updates: u64,
    pub max_staleness_updates: u32,
    pub worker_count: usize,
    pub sessions_per_worker: usize,
    pub broker_batch_target: usize,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub scheduler_timeout: Duration,
    /// Stands in for the trainer's non-rollout wall (gradient step + Store
    /// commit). See [`HARNESS_LIMITATIONS_NOTE_V1`].
    pub synthetic_train_step: Duration,
}

impl HarnessConfigV1 {
    /// A config sized for a fast, deterministic `cargo test` (well under a
    /// second of rollout work); used by this module's own `#[test]`s.
    pub fn small_for_tests_v1() -> Self {
        Self {
            deck_ids: ["Rally".to_string(), "Rally".to_string()],
            base_environment_seed: 0x5EED_0000_0001,
            opponent_policy_seed: 0xAA55_0000_0002,
            learner_policy_seed: 0x1234_0000_0003,
            batch_episodes: 4,
            total_updates: 6,
            max_staleness_updates: 2,
            worker_count: 2,
            sessions_per_worker: 2,
            broker_batch_target: 4,
            max_physical_decisions: 2_000,
            max_policy_steps: 200_000,
            scheduler_timeout: Duration::from_secs(30),
            synthetic_train_step: Duration::from_millis(5),
        }
    }

    fn rollout_config_for_update_v1(&self, update: u64) -> AsyncRolloutConfigV2 {
        AsyncRolloutConfigV2 {
            deck_ids: self.deck_ids.clone(),
            learner_seat: PlayerSeatV1::P0,
            // Deterministic function of the logical update index, not of
            // wall-clock arrival order: the seed schedule itself stays fully
            // reproducible even though async mode does not guarantee which
            // weight version consumes which update. See
            // BOUNDED_STALENESS_ASYNC_REPRODUCIBILITY_CLAIM_V1.
            environment_seed: self.base_environment_seed ^ update.wrapping_mul(0x9E37_79B9),
            opponent_policy_seed: self.opponent_policy_seed,
            learner_policy_seed: self.learner_policy_seed,
            max_physical_decisions: self.max_physical_decisions,
            max_policy_steps: self.max_policy_steps,
            worker_count: self.worker_count,
            sessions_per_worker: self.sessions_per_worker,
            broker_batch_target: self.broker_batch_target,
            first_episode_id: (update - 1) * self.batch_episodes,
            episode_count: self.batch_episodes,
            scheduler_timeout: self.scheduler_timeout,
            measure_broker_service_time: false,
        }
    }
}

fn mean_p0_terminal_reward_v1(result: &AsyncFlatScoredRolloutResultV1) -> f64 {
    if result.episodes.is_empty() {
        return 0.0;
    }
    let sum: i64 = result
        .episodes
        .iter()
        .map(|episode| i64::from(episode.terminal.terminal_reward[0]))
        .sum();
    sum as f64 / result.episodes.len() as f64
}

fn episode_ids_v1(result: &AsyncFlatScoredRolloutResultV1) -> Vec<u64> {
    result
        .episodes
        .iter()
        .map(|episode| episode.terminal.episode_id)
        .collect()
}

#[derive(Debug, Clone)]
pub struct ArmReportV1 {
    pub arm_name: &'static str,
    pub total_updates: u64,
    pub total_episodes: u64,
    pub wall_time: Duration,
    pub episodes_per_second: f64,
    pub mean_terminal_reward_by_update: Vec<f64>,
    /// `None` for the sync arm (staleness is not a concept there).
    pub max_observed_staleness: Option<u64>,
}

/// Runs the fully synchronous arm: for each update, block on one rollout
/// call using the current weights, then simulate the train step. No overlap
/// is possible by construction, matching the pre-existing trainer's own
/// shape (score, train, commit, repeat).
pub fn run_sync_arm_v1(config: &HarnessConfigV1) -> ArmReportV1 {
    let mut scorer = SmokeScorerV1;
    let mut mean_terminal_reward_by_update = Vec::with_capacity(config.total_updates as usize);
    let mut total_episodes = 0_u64;
    let started = Instant::now();
    for update in 1..=config.total_updates {
        let rollout_config = config.rollout_config_for_update_v1(update);
        let result = run_async_flat_scored_rollout_v1(rollout_config, &mut scorer)
            .expect("smoke scorer never rejects a valid batch");
        total_episodes += result.episodes.len() as u64;
        mean_terminal_reward_by_update.push(mean_p0_terminal_reward_v1(&result));
        std::thread::sleep(config.synthetic_train_step);
    }
    let wall_time = started.elapsed();
    ArmReportV1 {
        arm_name: "synchronous_v1",
        total_updates: config.total_updates,
        total_episodes,
        wall_time,
        episodes_per_second: total_episodes as f64 / wall_time.as_secs_f64().max(f64::EPSILON),
        mean_terminal_reward_by_update,
        max_observed_staleness: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessErrorV1 {
    Consume(BoundedStalenessConsumeErrorV1),
}

/// Runs the bounded-staleness async arm over the identical seed schedule
/// [`HarnessConfigV1::rollout_config_for_update_v1`] produces for the sync
/// arm, using [`BoundedStalenessSchedulerV1`] to overlap rollout production
/// for update `u+1` with the (simulated) train step for update `u`.
pub fn run_async_arm_v1(config: &HarnessConfigV1) -> Result<ArmReportV1, HarnessErrorV1> {
    let config_for_producer = config.clone();
    let started = Instant::now();
    let scheduler: BoundedStalenessSchedulerV1<SmokeWeightsV1, AsyncFlatScoredRolloutResultV1> =
        BoundedStalenessSchedulerV1::spawn_v1(
            SmokeWeightsV1 { version: 0 },
            config.max_staleness_updates,
            config.total_updates,
            move |target_update, _scoring_version, _weights| {
                let rollout_config = config_for_producer.rollout_config_for_update_v1(target_update);
                let mut scorer = SmokeScorerV1;
                let result = run_async_flat_scored_rollout_v1(rollout_config, &mut scorer)
                    .expect("smoke scorer never rejects a valid batch");
                let ids = episode_ids_v1(&result);
                (result, ids)
            },
        );

    let mut mean_terminal_reward_by_update = Vec::with_capacity(config.total_updates as usize);
    let mut total_episodes = 0_u64;
    for update in 1..=config.total_updates {
        let consumed = scheduler
            .next_batch_v1(config.scheduler_timeout)
            .map_err(HarnessErrorV1::Consume)?;
        debug_assert_eq!(consumed.consuming_update_version, update);
        total_episodes += consumed.batch.episodes.len() as u64;
        mean_terminal_reward_by_update.push(mean_p0_terminal_reward_v1(&consumed.batch));
        std::thread::sleep(config.synthetic_train_step);
        scheduler.publish_v1(update, SmokeWeightsV1 { version: update });
    }
    let wall_time = started.elapsed();
    let ledger = scheduler.ledger_snapshot_v1();
    ledger
        .enforce_all_v1(config.max_staleness_updates)
        .expect("scheduler dispatch gating must keep every entry within bound");

    Ok(ArmReportV1 {
        arm_name: "bounded_staleness_async_v1",
        total_updates: config.total_updates,
        total_episodes,
        wall_time,
        episodes_per_second: total_episodes as f64 / wall_time.as_secs_f64().max(f64::EPSILON),
        mean_terminal_reward_by_update,
        max_observed_staleness: ledger.max_observed_staleness_v1(),
    })
}

#[derive(Debug, Clone)]
pub struct ABHarnessReportV1 {
    pub sync: ArmReportV1,
    pub bounded_staleness_async: ArmReportV1,
    pub speedup_ratio: f64,
    pub reward_curves_match: bool,
    pub limitations: &'static str,
}

/// Runs both arms over the identical seed schedule and reports throughput
/// plus the reward-curve proxy for each. This is the harness a future
/// campaign runs at real scale against the real trainer to qualify the
/// async path; here it is exercised at whatever scale `config` declares.
pub fn run_matched_ab_v1(config: &HarnessConfigV1) -> Result<ABHarnessReportV1, HarnessErrorV1> {
    let sync = run_sync_arm_v1(config);
    let bounded_staleness_async = run_async_arm_v1(config)?;
    let reward_curves_match = sync.mean_terminal_reward_by_update
        == bounded_staleness_async.mean_terminal_reward_by_update;
    let speedup_ratio = if bounded_staleness_async.wall_time.as_secs_f64() > 0.0 {
        sync.wall_time.as_secs_f64() / bounded_staleness_async.wall_time.as_secs_f64()
    } else {
        0.0
    };
    Ok(ABHarnessReportV1 {
        sync,
        bounded_staleness_async,
        speedup_ratio,
        reward_curves_match,
        limitations: HARNESS_LIMITATIONS_NOTE_V1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_arm_runs_the_declared_number_of_updates_and_episodes() {
        let config = HarnessConfigV1::small_for_tests_v1();
        let report = run_sync_arm_v1(&config);
        assert_eq!(report.total_updates, config.total_updates);
        assert_eq!(
            report.total_episodes,
            config.total_updates * config.batch_episodes
        );
        assert_eq!(
            report.mean_terminal_reward_by_update.len(),
            config.total_updates as usize
        );
        assert!(report.max_observed_staleness.is_none());
    }

    #[test]
    fn async_arm_never_exceeds_the_declared_staleness_bound() {
        let config = HarnessConfigV1::small_for_tests_v1();
        let report = run_async_arm_v1(&config).expect("harness arm should complete");
        assert_eq!(report.total_updates, config.total_updates);
        assert_eq!(
            report.total_episodes,
            config.total_updates * config.batch_episodes
        );
        assert!(
            report.max_observed_staleness.unwrap() <= u64::from(config.max_staleness_updates)
        );
    }

    #[test]
    #[ignore = "runs real (small) rollouts under both arms; excluded from the default fast suite, run with --ignored"]
    fn matched_seeds_yield_identical_episode_content_regardless_of_arm() {
        let config = HarnessConfigV1::small_for_tests_v1();
        let report = run_matched_ab_v1(&config).expect("harness should complete");
        // The stand-in scorer never reads the weight version, so with
        // matched seeds the two arms' rollouts are identical index for
        // index despite the async arm's staleness/timing nondeterminism:
        // the scheduling mechanism does not corrupt or reorder content.
        assert!(report.reward_curves_match);
        assert_eq!(report.sync.total_episodes, report.bounded_staleness_async.total_episodes);
    }

    #[test]
    #[ignore = "runs real (small) rollouts under both arms; excluded from the default fast suite, run with --ignored"]
    fn matched_ab_harness_reports_a_complete_structured_comparison() {
        let config = HarnessConfigV1::small_for_tests_v1();
        let report = run_matched_ab_v1(&config).expect("harness should complete");
        assert!(report.sync.episodes_per_second > 0.0);
        assert!(report.bounded_staleness_async.episodes_per_second > 0.0);
        assert_eq!(report.limitations, HARNESS_LIMITATIONS_NOTE_V1);
    }
}
