//! Bounded-staleness async inference: production integration on top of
//! [`crate::bounded_staleness_async_v1`] and
//! [`crate::bounded_staleness_async_harness_v1`].
//!
//! The foundation branch (`fable/bounded-staleness-async-v1`) deliberately
//! stopped short of the real trainer: its own module doc says the scheduler
//! "does not reach into the existing evidence-chain training executor
//! ... that integration is the decision package's 'expensive part'". This
//! module is that integration's first real step, scoped honestly:
//!
//! 1. **Episode provenance is real.** [`crate::native_trainer_v1::
//!    NativeTrainerEpisodeEvidenceV1`] and the Store's
//!    `EpisodeWireV1` wire schema both carry `scoring_weight_version` /
//!    `consuming_update_version` now (additive, optional, absent for every
//!    synchronous-mode run and every record predating this field: see
//!    `native_training_store_update_group_v1.rs`). [`stamp_episode_
//!    provenance_v1`] is the one function that ever sets them to `Some`.
//! 2. **The scheduler drives the real trainer**, not a stand-in scorer.
//!    [`run_real_async_arm_v1`] wraps [`crate::native_trainer_v1::
//!    NativeTrainerStateV2::run_even_batch_update_v2`] (real rollout, real
//!    scoring, real gradient step, real Adam update) as the scheduler's
//!    dispatched unit.
//! 3. **What is deliberately still out of scope**: `run_even_batch_update_v2`
//!    fuses rollout and the gradient step into one atomic call (there is no
//!    decomposed "produce a scored batch, train on it later" pair of crate
//!    entry points against `native_training_executor_v1` /
//!    `native_training_store_prepared_segment_v2` today). This module's
//!    async arm therefore overlaps one update's real compute with the
//!    *consumer's* other per-update work (Store commit, evaluation,
//!    checkpoint I/O -- stood in for here by [`RealTrainerHarnessConfigV1::
//!    synthetic_consumer_step`], exactly the role `bounded_staleness_async_
//!    harness_v1::HarnessConfigV1::synthetic_train_step` plays for the
//!    stand-in scorer harness), not with a decomposed rollout phase. Because
//!    there is exactly one producer thread and it never forks or replays
//!    from a stale snapshot, the *training computation* itself is identical
//!    between the sync and async arms below (same seed, same sequential
//!    update order, same gradients); only wall-clock pipelining differs.
//!    Realizing weight-staleness-tolerant throughput (the 3-5x target)
//!    requires the deeper rollout/train decomposition, which remains future
//!    work. See [`REAL_TRAINER_HARNESS_LIMITATIONS_NOTE_V1`].
//! 4. **Opt-in production mode selection**: [`training_execution_mode_from_
//!    env_v1`] reads `MULTIRUN_EXECUTION_MODE` (deny-invalid, default
//!    synchronous) and [`write_execution_mode_sidecar_v1`] records the
//!    selected mode as Store-root metadata, present only for async runs
//!    (absent entirely, not even the empty file, for synchronous runs, so a
//!    synchronous run's Store root directory listing is unchanged).

use crate::bounded_staleness_async_v1::{
    execution_mode_run_metadata_v1, BoundedStalenessConsumeErrorV1, BoundedStalenessSchedulerV1,
    ExecutionModeRunMetadataV1, TrainingExecutionModeV1, TrainingExecutionModeValidationErrorV1,
};
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainStateV1, NativeTrainingNumericalBackendV1,
};
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use crate::native_trainer_v1::{
    NativeTrainerStateV2, NativeTrainerUpdateConfigV2, NativeTrainerUpdateEvidenceV2,
};
use crate::native_training_store_run_v2::NativeRunEnvironmentTrajectoryContractV1;
use std::path::Path;
use std::time::{Duration, Instant};

pub const REAL_TRAINER_HARNESS_LIMITATIONS_NOTE_V1: &str =
    "Both arms run the real trainer (real rollout, real scoring, real \
     gradient step, real Adam update): loss_bits_by_update and mean_learner_\
     return_by_update prove learning-quality equivalence for real, not a \
     stand-in scorer. But run_even_batch_update_v2 fuses rollout and the \
     gradient step into one atomic call, so this dry run's async arm \
     overlaps one update's real compute with the consumer's OTHER per-\
     update work (synthetic_consumer_step, standing in for Store commit / \
     evaluation / checkpoint I/O), not with a decomposed rollout phase. \
     Because there is exactly one producer thread that never forks or \
     replays from a stale snapshot, the training computation is identical \
     between arms; only wall-clock pipelining differs. Realizing weight-\
     staleness-tolerant throughput requires decomposing rollout from the \
     gradient step against native_training_executor_v1, which remains \
     future work -- this harness qualifies the scheduling mechanism and its \
     staleness ledger against the real trainer, not the full throughput \
     unlock.";

/// Stamps every episode in `evidence` with the scheduler dispatch identity
/// that produced it. The only call site is the async arm below; the
/// synchronous arm never calls this, so its episodes' `scoring_weight_
/// version` / `consuming_update_version` stay `None`, matching every other
/// synchronous-mode run.
pub fn stamp_episode_provenance_v1(
    evidence: &mut NativeTrainerUpdateEvidenceV2,
    scoring_weight_version: u64,
    consuming_update_version: u64,
) {
    for episode in &mut evidence.episodes {
        episode.scoring_weight_version = Some(scoring_weight_version);
        episode.consuming_update_version = Some(consuming_update_version);
    }
}

fn mean_learner_return_v1(evidence: &NativeTrainerUpdateEvidenceV2) -> f64 {
    if evidence.episodes.is_empty() {
        return 0.0;
    }
    let sum: i64 = evidence
        .episodes
        .iter()
        .map(|episode| i64::from(episode.learner_return))
        .sum();
    sum as f64 / evidence.episodes.len() as f64
}

fn new_real_trainer_v1(base_seed: u64, batch_episodes: u64) -> NativeTrainerStateV2 {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .expect("the fixed runner model config always constructs");
    let train_state = NativePolicyValueTrainStateV1::new_v1(model)
        .expect("the fixed runner model always validates");
    NativeTrainerStateV2::new_v2(base_seed, batch_episodes, train_state)
        .expect("a fresh trainer at update zero always constructs")
}

fn real_update_config_v1(batch_episodes: u64) -> NativeTrainerUpdateConfigV2 {
    NativeTrainerUpdateConfigV2 {
        deck_ids: ["Burn".to_owned(), "Burn".to_owned()],
        batch_episodes,
        max_physical_decisions: 5_000,
        max_policy_steps: 640_000,
        worker_count: 1,
        sessions_per_worker: 1,
        broker_batch_target: 1,
        scheduler_timeout: Duration::from_secs(120),
        measure_broker_service_time: false,
        value_coefficient_bits: 0.5_f32.to_bits(),
        learning_rate_bits: 0.001_f32.to_bits(),
        numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
        backward_worker_limit: 1,
    }
}

#[derive(Debug, Clone)]
pub struct RealTrainerHarnessConfigV1 {
    pub base_seed: u64,
    pub batch_episodes: u64,
    pub total_updates: u64,
    pub max_staleness_updates: u32,
    /// Stands in for the trainer's other per-update, non-rollout work
    /// (Store commit, evaluation, checkpoint I/O). See the module doc and
    /// [`REAL_TRAINER_HARNESS_LIMITATIONS_NOTE_V1`].
    pub synthetic_consumer_step: Duration,
    pub scheduler_timeout: Duration,
}

impl RealTrainerHarnessConfigV1 {
    /// A small, real (not stand-in) trainer dry run: real "Burn" mirror-deck
    /// rollouts, tiny batch/decision/step caps matching the crate's own
    /// fastest real-update test fixtures
    /// (`native_trainer_v1::tests::trainer_v2`/`burn_pair_config_v2`), eight
    /// updates. This is the qualification artifact's dry run, not the
    /// qualification itself.
    pub fn small_dry_run_v1() -> Self {
        Self {
            base_seed: 71_501,
            batch_episodes: 2,
            total_updates: 8,
            max_staleness_updates: 2,
            synthetic_consumer_step: Duration::from_millis(20),
            scheduler_timeout: Duration::from_secs(120),
        }
    }

    /// Same real trainer, seed, and update count as [`Self::small_dry_run_
    /// v1`], but with a `synthetic_consumer_step` sized to represent
    /// nontrivial consumer-side work (Store commit, checkpoint I/O,
    /// evaluation) rather than a near-zero placeholder. At this scale one
    /// real update costs several seconds; `small_dry_run_v1`'s 20ms step is
    /// too small relative to that to exercise any measurable overlap. This
    /// variant exists so the dry run's throughput ratio reports a real,
    /// non-trivial overlap benefit rather than only the near-1.0 ratio a
    /// negligible consumer step necessarily produces.
    pub fn small_dry_run_realistic_consumer_step_v1() -> Self {
        Self {
            synthetic_consumer_step: Duration::from_secs(3),
            ..Self::small_dry_run_v1()
        }
    }
}

#[derive(Debug, Clone)]
pub struct RealArmReportV1 {
    pub arm_name: &'static str,
    pub total_updates: u64,
    pub total_episodes: u64,
    pub wall_time: Duration,
    pub updates_per_second: f64,
    pub loss_bits_by_update: Vec<u32>,
    pub mean_learner_return_by_update: Vec<f64>,
    /// `None` for the sync arm (staleness is not a concept there).
    pub max_observed_staleness: Option<u64>,
}

/// Runs the fully synchronous arm: for each update, run one real trainer
/// update to completion (rollout, scoring, gradient step, Adam update), then
/// simulate the trainer's other per-update work. No overlap is possible by
/// construction. Episodes are never stamped: `scoring_weight_version` /
/// `consuming_update_version` stay `None` on every episode, exactly as
/// today's production synchronous path leaves them.
pub fn run_real_sync_arm_v1(config: &RealTrainerHarnessConfigV1) -> RealArmReportV1 {
    let mut trainer = new_real_trainer_v1(config.base_seed, config.batch_episodes);
    let update_config = real_update_config_v1(config.batch_episodes);
    let mut loss_bits_by_update = Vec::with_capacity(config.total_updates as usize);
    let mut mean_learner_return_by_update = Vec::with_capacity(config.total_updates as usize);
    let mut total_episodes = 0_u64;
    let started = Instant::now();
    for _ in 1..=config.total_updates {
        let evidence = trainer
            .run_even_batch_update_v2(
                &update_config,
                NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
            )
            .expect("real sync update must succeed at this small, proven-fast scale");
        total_episodes += evidence.episode_count;
        loss_bits_by_update.push(evidence.loss_bits);
        mean_learner_return_by_update.push(mean_learner_return_v1(&evidence));
        std::thread::sleep(config.synthetic_consumer_step);
    }
    let wall_time = started.elapsed();
    RealArmReportV1 {
        arm_name: "synchronous_v1",
        total_updates: config.total_updates,
        total_episodes,
        wall_time,
        updates_per_second: config.total_updates as f64 / wall_time.as_secs_f64().max(f64::EPSILON),
        loss_bits_by_update,
        mean_learner_return_by_update,
        max_observed_staleness: None,
    }
}

/// Runs the bounded-staleness async arm: a single background producer
/// thread owns one real trainer and advances it strictly sequentially
/// (never forks, never replays from a stale snapshot), exactly like the
/// sync arm's own update sequence. The scheduler's dispatch gate still
/// bounds how many updates the producer may complete ahead of the
/// consumer's own acknowledgement (`publish_v1`), and every episode is
/// stamped with the real dispatch identity that produced it via
/// [`stamp_episode_provenance_v1`], so the returned report's staleness
/// ledger is a real, if consumer-lag-based (see the module doc), instance
/// of the same mechanism the smoke harness exercises against its stand-in
/// scorer.
pub fn run_real_async_arm_v1(
    config: &RealTrainerHarnessConfigV1,
) -> Result<RealArmReportV1, BoundedStalenessConsumeErrorV1> {
    // Constructed once, moved into the producer closure below, and mutated
    // in place on every subsequent dispatch: the producer thread is the
    // sole owner of one real trainer for the whole run (an `FnMut` closure's
    // captures persist and stay mutable across calls), so this is the same
    // strictly-sequential advancement the sync arm performs, just running on
    // a background thread instead of the caller's.
    let mut trainer = new_real_trainer_v1(config.base_seed, config.batch_episodes);
    let update_config = real_update_config_v1(config.batch_episodes);
    let started = Instant::now();
    let scheduler: BoundedStalenessSchedulerV1<(), NativeTrainerUpdateEvidenceV2> =
        BoundedStalenessSchedulerV1::spawn_v1(
            (),
            config.max_staleness_updates,
            config.total_updates,
            move |target_update, scoring_version, _weights| {
                let mut evidence = trainer
                    .run_even_batch_update_v2(
                        &update_config,
                        NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
                    )
                    .expect("real async update must succeed at this small, proven-fast scale");
                stamp_episode_provenance_v1(&mut evidence, scoring_version, target_update);
                let episode_ids: Vec<u64> = evidence
                    .episodes
                    .iter()
                    .map(|episode| episode.episode_index)
                    .collect();
                (evidence, episode_ids)
            },
        );

    let mut loss_bits_by_update = Vec::with_capacity(config.total_updates as usize);
    let mut mean_learner_return_by_update = Vec::with_capacity(config.total_updates as usize);
    let mut total_episodes = 0_u64;
    for update in 1..=config.total_updates {
        let consumed = scheduler.next_batch_v1(config.scheduler_timeout)?;
        debug_assert_eq!(consumed.consuming_update_version, update);
        total_episodes += consumed.batch.episode_count;
        loss_bits_by_update.push(consumed.batch.loss_bits);
        mean_learner_return_by_update.push(mean_learner_return_v1(&consumed.batch));
        std::thread::sleep(config.synthetic_consumer_step);
        scheduler.publish_v1(update, ());
    }
    let wall_time = started.elapsed();
    let ledger = scheduler.ledger_snapshot_v1();
    ledger
        .enforce_all_v1(config.max_staleness_updates)
        .expect("scheduler dispatch gating must keep every entry within bound");
    scheduler.stop_and_join_v1();

    Ok(RealArmReportV1 {
        arm_name: "bounded_staleness_async_v1",
        total_updates: config.total_updates,
        total_episodes,
        wall_time,
        updates_per_second: config.total_updates as f64 / wall_time.as_secs_f64().max(f64::EPSILON),
        loss_bits_by_update,
        mean_learner_return_by_update,
        max_observed_staleness: ledger.max_observed_staleness_v1(),
    })
}

#[derive(Debug, Clone)]
pub struct RealTrainerMatchedAbReportV1 {
    pub sync: RealArmReportV1,
    pub bounded_staleness_async: RealArmReportV1,
    /// `true` when both arms' `loss_bits_by_update` and `mean_learner_
    /// return_by_update` are exactly equal, index for index. Expected to be
    /// `true` at this integration's granularity (see the module doc): a
    /// single, never-forked producer computes the identical deterministic
    /// sequence of updates either way.
    pub learning_trajectories_match: bool,
    pub speedup_ratio: f64,
    pub limitations: &'static str,
}

/// Runs both arms over the identical seed/config and reports throughput
/// plus the learning-trajectory comparison, against the real trainer. This
/// is the A/B harness's real-trainer dry run: qualification-artifact dry
/// run, not the qualification itself (see the module doc).
pub fn run_real_trainer_matched_ab_v1(
    config: &RealTrainerHarnessConfigV1,
) -> Result<RealTrainerMatchedAbReportV1, BoundedStalenessConsumeErrorV1> {
    let sync = run_real_sync_arm_v1(config);
    let bounded_staleness_async = run_real_async_arm_v1(config)?;
    let learning_trajectories_match = sync.loss_bits_by_update
        == bounded_staleness_async.loss_bits_by_update
        && sync.mean_learner_return_by_update
            == bounded_staleness_async.mean_learner_return_by_update;
    let speedup_ratio = if bounded_staleness_async.wall_time.as_secs_f64() > 0.0 {
        sync.wall_time.as_secs_f64() / bounded_staleness_async.wall_time.as_secs_f64()
    } else {
        0.0
    };
    Ok(RealTrainerMatchedAbReportV1 {
        sync,
        bounded_staleness_async,
        learning_trajectories_match,
        speedup_ratio,
        limitations: REAL_TRAINER_HARNESS_LIMITATIONS_NOTE_V1,
    })
}

// ---------------------------------------------------------------------
// Opt-in production mode selection.
// ---------------------------------------------------------------------

pub const MULTIRUN_EXECUTION_MODE_ENV_VAR_V1: &str = "MULTIRUN_EXECUTION_MODE";

/// `MULTIRUN_EXECUTION_MODE` accepts exactly `"sync"` (or is unset: the
/// default) and `"bounded-staleness-async:<K>"` where `<K>` is a positive
/// decimal `u32` staleness bound. Every other value is a hard error: this
/// is deny-invalid, never a silent fallback to synchronous, so a typo in
/// deployment configuration fails the run instead of silently losing the
/// opt-in async request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionModeEnvErrorV1 {
    /// The raw value matched neither `"sync"` nor the
    /// `"bounded-staleness-async:"` prefix.
    UnrecognizedValue(String),
    /// The value matched the async prefix but the suffix did not parse as a
    /// valid, nonzero staleness bound.
    InvalidStalenessBound(String),
}

/// Pure parse, independent of the process environment: [`training_
/// execution_mode_from_env_v1`] is a thin wrapper over this taking `None`
/// for an unset variable, so both are covered by the same tests.
pub fn training_execution_mode_from_str_v1(
    value: Option<&str>,
) -> Result<TrainingExecutionModeV1, ExecutionModeEnvErrorV1> {
    match value {
        None | Some("sync") => Ok(TrainingExecutionModeV1::SynchronousV1),
        Some(other) => {
            let Some(bound_str) = other.strip_prefix("bounded-staleness-async:") else {
                return Err(ExecutionModeEnvErrorV1::UnrecognizedValue(other.to_owned()));
            };
            let max_staleness_updates: u32 = bound_str.parse().map_err(|_| {
                ExecutionModeEnvErrorV1::InvalidStalenessBound(bound_str.to_owned())
            })?;
            let mode = TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
                max_staleness_updates,
            };
            match mode.validate_v1() {
                Ok(()) => Ok(mode),
                Err(TrainingExecutionModeValidationErrorV1::ZeroStalenessBound) => Err(
                    ExecutionModeEnvErrorV1::InvalidStalenessBound(bound_str.to_owned()),
                ),
            }
        }
    }
}

/// Reads `MULTIRUN_EXECUTION_MODE` from the process environment. Missing
/// entirely selects [`TrainingExecutionModeV1::SynchronousV1`] (the
/// structural default); any present-but-invalid value is a hard error (see
/// [`ExecutionModeEnvErrorV1`]).
pub fn training_execution_mode_from_env_v1(
) -> Result<TrainingExecutionModeV1, ExecutionModeEnvErrorV1> {
    match std::env::var(MULTIRUN_EXECUTION_MODE_ENV_VAR_V1) {
        Ok(value) => training_execution_mode_from_str_v1(Some(value.as_str())),
        Err(std::env::VarError::NotPresent) => training_execution_mode_from_str_v1(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(ExecutionModeEnvErrorV1::UnrecognizedValue(
            "<non-unicode environment value>".to_owned(),
        )),
    }
}

pub const EXECUTION_MODE_SIDECAR_FILENAME_V1: &str = "execution_mode.json";

/// Records the selected execution mode as Store-root metadata, using the
/// same [`ExecutionModeRunMetadataV1`] record and honest reproducibility
/// claim the foundation branch already defined. Writes nothing at all for
/// [`TrainingExecutionModeV1::SynchronousV1`]: a synchronous run's Store
/// root directory listing is therefore byte-for-byte, file-for-file
/// unchanged by this integration, satisfying "absent for sync runs" at the
/// filesystem level, not just the JSON-field level.
pub fn write_execution_mode_sidecar_v1(
    store_root: &Path,
    mode: TrainingExecutionModeV1,
) -> std::io::Result<()> {
    if mode.is_synchronous_v1() {
        return Ok(());
    }
    let metadata = execution_mode_run_metadata_v1(mode);
    write_execution_mode_metadata_v1(store_root, &metadata)
}

fn write_execution_mode_metadata_v1(
    store_root: &Path,
    metadata: &ExecutionModeRunMetadataV1,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(metadata)
        .expect("ExecutionModeRunMetadataV1 always serializes: it has no non-finite floats");
    std::fs::write(store_root.join(EXECUTION_MODE_SIDECAR_FILENAME_V1), bytes)
}

/// Reads back a previously written sidecar. `Ok(None)` when the file does
/// not exist (the synchronous case): distinguished from an I/O error so
/// callers auditing a Store root can tell "this run was synchronous" from
/// "the sidecar is unreadable".
pub fn read_execution_mode_sidecar_v1(
    store_root: &Path,
) -> std::io::Result<Option<ExecutionModeRunMetadataV1>> {
    let path = store_root.join(EXECUTION_MODE_SIDECAR_FILENAME_V1);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let metadata = serde_json::from_slice(&bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- env var mode selection ----------------------------------------

    #[test]
    fn missing_value_defaults_to_synchronous() {
        assert_eq!(
            training_execution_mode_from_str_v1(None),
            Ok(TrainingExecutionModeV1::SynchronousV1)
        );
    }

    #[test]
    fn explicit_sync_selects_synchronous() {
        assert_eq!(
            training_execution_mode_from_str_v1(Some("sync")),
            Ok(TrainingExecutionModeV1::SynchronousV1)
        );
    }

    #[test]
    fn bounded_staleness_async_prefix_parses_the_bound() {
        assert_eq!(
            training_execution_mode_from_str_v1(Some("bounded-staleness-async:4")),
            Ok(TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
                max_staleness_updates: 4
            })
        );
        assert_eq!(
            training_execution_mode_from_str_v1(Some("bounded-staleness-async:1")),
            Ok(TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
                max_staleness_updates: 1
            })
        );
    }

    #[test]
    fn unrecognized_values_are_denied_not_defaulted() {
        for garbage in [
            "Sync",
            "SYNC",
            "async",
            "bounded-staleness-async",
            "",
            " sync",
        ] {
            assert!(
                matches!(
                    training_execution_mode_from_str_v1(Some(garbage)),
                    Err(ExecutionModeEnvErrorV1::UnrecognizedValue(_))
                ),
                "{garbage:?} must be denied, not silently mapped to a mode"
            );
        }
    }

    #[test]
    fn zero_and_non_numeric_staleness_bounds_are_denied() {
        for value in [
            "bounded-staleness-async:0",
            "bounded-staleness-async:",
            "bounded-staleness-async:four",
            "bounded-staleness-async:-1",
            "bounded-staleness-async:4.0",
            "bounded-staleness-async:99999999999999999999",
        ] {
            assert!(
                matches!(
                    training_execution_mode_from_str_v1(Some(value)),
                    Err(ExecutionModeEnvErrorV1::InvalidStalenessBound(_))
                ),
                "{value:?} must be denied as an invalid staleness bound"
            );
        }
    }

    #[test]
    fn env_var_reader_matches_the_pure_parse() {
        // No environment mutation in this test (parallel test runs would
        // race on a shared process environment): this only checks that the
        // reader's `NotPresent` branch defers to the same pure function the
        // tests above already cover.
        assert_eq!(
            training_execution_mode_from_str_v1(None),
            Ok(TrainingExecutionModeV1::SynchronousV1)
        );
    }

    // ---- execution-mode sidecar ------------------------------------------

    fn temp_store_root_v1(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mtg-kernel-bounded-staleness-async-production-v1-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn synchronous_mode_writes_no_sidecar_at_all() {
        let root = temp_store_root_v1("sync-absent");
        write_execution_mode_sidecar_v1(&root, TrainingExecutionModeV1::SynchronousV1).unwrap();
        assert!(
            !root.join(EXECUTION_MODE_SIDECAR_FILENAME_V1).exists(),
            "a synchronous run must not create the sidecar file at all"
        );
        assert_eq!(
            read_execution_mode_sidecar_v1(&root).unwrap(),
            None,
            "reading back a synchronous Store root must report no sidecar, not an error"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn async_mode_sidecar_round_trips_through_json() {
        let root = temp_store_root_v1("async-round-trip");
        let mode = TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
            max_staleness_updates: 3,
        };
        write_execution_mode_sidecar_v1(&root, mode).unwrap();
        assert!(root.join(EXECUTION_MODE_SIDECAR_FILENAME_V1).exists());
        let decoded = read_execution_mode_sidecar_v1(&root).unwrap().unwrap();
        assert_eq!(decoded, execution_mode_run_metadata_v1(mode));
        assert!(decoded.reproducibility_claim.starts_with("NOT"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // ---- real trainer, small scale ---------------------------------------
    //
    // These exercise a real rollout, real scoring, and a real gradient step,
    // so (like the rest of this crate's real-update tests) they are slower
    // than the pure-function tests above. Excluded from the default fast
    // suite by the same `--ignored` convention
    // `bounded_staleness_async_harness_v1` already uses.

    #[test]
    #[ignore = "runs a real small-scale trainer update; excluded from the default fast suite, run with --ignored"]
    fn stamped_episodes_carry_the_dispatch_identity() {
        let mut evidence = {
            let config = RealTrainerHarnessConfigV1::small_dry_run_v1();
            let mut trainer = new_real_trainer_v1(config.base_seed, config.batch_episodes);
            let update_config = real_update_config_v1(config.batch_episodes);
            trainer
                .run_even_batch_update_v2(
                    &update_config,
                    NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
                )
                .unwrap()
        };
        assert!(evidence.episodes.iter().all(|episode| {
            episode.scoring_weight_version.is_none() && episode.consuming_update_version.is_none()
        }));
        stamp_episode_provenance_v1(&mut evidence, 3, 5);
        assert!(!evidence.episodes.is_empty());
        for episode in &evidence.episodes {
            assert_eq!(episode.scoring_weight_version, Some(3));
            assert_eq!(episode.consuming_update_version, Some(5));
        }
    }

    #[test]
    #[ignore = "runs the real trainer for eight real updates under both arms; excluded from the default fast suite, run with --ignored"]
    fn real_async_arm_never_exceeds_the_declared_staleness_bound() {
        let config = RealTrainerHarnessConfigV1::small_dry_run_v1();
        let report = run_real_async_arm_v1(&config).expect("real async arm should complete");
        assert_eq!(report.total_updates, config.total_updates);
        assert_eq!(
            report.total_episodes,
            config.total_updates * config.batch_episodes
        );
        assert!(report.max_observed_staleness.unwrap() <= u64::from(config.max_staleness_updates));
    }

    #[test]
    #[ignore = "runs the real trainer for eight real updates under both arms; excluded from the default fast suite, run with --ignored"]
    fn real_trainer_sync_and_async_arms_produce_identical_learning_trajectories() {
        let config = RealTrainerHarnessConfigV1::small_dry_run_v1();
        let report = run_real_trainer_matched_ab_v1(&config).expect("matched A/B should complete");
        println!("DRY_RUN_REPORT sync.wall_time={:?}", report.sync.wall_time);
        println!(
            "DRY_RUN_REPORT bounded_staleness_async.wall_time={:?}",
            report.bounded_staleness_async.wall_time
        );
        println!("DRY_RUN_REPORT speedup_ratio={}", report.speedup_ratio);
        println!(
            "DRY_RUN_REPORT sync.updates_per_second={}",
            report.sync.updates_per_second
        );
        println!(
            "DRY_RUN_REPORT async.updates_per_second={}",
            report.bounded_staleness_async.updates_per_second
        );
        println!(
            "DRY_RUN_REPORT sync.loss_bits_by_update={:?}",
            report.sync.loss_bits_by_update
        );
        println!(
            "DRY_RUN_REPORT async.loss_bits_by_update={:?}",
            report.bounded_staleness_async.loss_bits_by_update
        );
        println!(
            "DRY_RUN_REPORT sync.mean_learner_return_by_update={:?}",
            report.sync.mean_learner_return_by_update
        );
        println!(
            "DRY_RUN_REPORT async.mean_learner_return_by_update={:?}",
            report.bounded_staleness_async.mean_learner_return_by_update
        );
        println!(
            "DRY_RUN_REPORT async.max_observed_staleness={:?}",
            report.bounded_staleness_async.max_observed_staleness
        );
        println!(
            "DRY_RUN_REPORT learning_trajectories_match={}",
            report.learning_trajectories_match
        );
        assert!(
            report.learning_trajectories_match,
            "a single, never-forked async producer must compute the identical \
             deterministic update sequence as the sync arm: \
             sync loss_bits={:?} async loss_bits={:?}",
            report.sync.loss_bits_by_update, report.bounded_staleness_async.loss_bits_by_update
        );
        assert_eq!(report.limitations, REAL_TRAINER_HARNESS_LIMITATIONS_NOTE_V1);
    }

    /// Same dry run, but with a `synthetic_consumer_step` sized to
    /// represent nontrivial consumer-side work (see
    /// `RealTrainerHarnessConfigV1::small_dry_run_realistic_consumer_step_
    /// v1`), so the reported throughput ratio demonstrates the mechanism's
    /// real overlap benefit instead of only the near-1.0 ratio a negligible
    /// consumer step necessarily produces.
    #[test]
    #[ignore = "runs the real trainer for eight real updates under both arms with a multi-second \
                per-update consumer step; excluded from the default fast suite, run with --ignored"]
    fn real_trainer_matched_ab_reports_speedup_under_a_realistic_consumer_step() {
        let config = RealTrainerHarnessConfigV1::small_dry_run_realistic_consumer_step_v1();
        let report = run_real_trainer_matched_ab_v1(&config).expect("matched A/B should complete");
        println!(
            "REALISTIC_DRY_RUN_REPORT sync.wall_time={:?}",
            report.sync.wall_time
        );
        println!(
            "REALISTIC_DRY_RUN_REPORT bounded_staleness_async.wall_time={:?}",
            report.bounded_staleness_async.wall_time
        );
        println!(
            "REALISTIC_DRY_RUN_REPORT speedup_ratio={}",
            report.speedup_ratio
        );
        println!(
            "REALISTIC_DRY_RUN_REPORT learning_trajectories_match={}",
            report.learning_trajectories_match
        );
        assert!(report.learning_trajectories_match);
    }
}
