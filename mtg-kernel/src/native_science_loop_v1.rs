//! One-command native science loop: trainer, runner, and evaluator over one
//! durable Store.
//!
//! This library entry point bootstraps or reopens a Store, trains to the
//! run's exact target through the resume orchestration (every window on a
//! reconstructed executor), fully validates the Store, loads the update-zero
//! and latest trained boundaries through the complete decode chain, runs the
//! checkpoint-backed runner for both, and evaluates the seat-swapped uniform
//! reward delta. It is a development workflow product: it publishes no
//! experiment manifest and claims no experiment authority; the authoritative
//! one-command experiment product remains gated on the joint Store/CLI
//! freeze. On non-Windows platforms the loop fails with the stable
//! unsupported-platform classification before any path-backed mutation.

use crate::native_checkpoint_evaluator_v1::{
    evaluate_native_checkpoint_uniform_delta_v1, NativeCheckpointUniformDeltaEvaluationV1,
};
use crate::native_checkpoint_runner_v1::{
    run_native_checkpoint_v1, NativeCheckpointRunResultV1, NativeCheckpointRunnerConfigV1,
};
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
use crate::native_training_executor_v1::{
    NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
};
use crate::native_training_store_bootstrap_v2::{
    bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapV2ErrorKind,
};
use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
use crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3;
use crate::native_training_store_prepared_segment_v2::prepare_segment_v2;
use crate::native_training_store_reference_latest_v2::{
    build_checkpoint_reference_v2, build_latest_v2,
};
use crate::native_training_store_resume_v2::{
    load_native_training_boundary_v2, resume_native_training_store_v2,
    validate_native_training_store_v2, NativeTrainingStoreResumeV2,
};
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
use crate::native_training_store_update_group_v1::validate_prepared_execution_config_v1;
use crate::native_training_store_v2::{
    publish_genesis_generation_v2, NativeTrainingStorePublisherV2ErrorKind,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeScienceLoopV1ErrorKind {
    UnsupportedPlatform,
    StoreBusy,
    InputInvalid,
    BootstrapFailed,
    GenesisFailed,
    TrainFailed,
    ValidateFailed,
    LoadFailed,
    RunFailed,
    EvaluateFailed,
}

impl NativeScienceLoopV1ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "native-training-store-v2-unsupported-platform",
            Self::StoreBusy => "native-training-store-busy",
            Self::InputInvalid => "native-science-loop-input-invalid",
            Self::BootstrapFailed => "native-science-loop-bootstrap-failed",
            Self::GenesisFailed => "native-science-loop-genesis-failed",
            Self::TrainFailed => "native-science-loop-train-failed",
            Self::ValidateFailed => "native-science-loop-validate-failed",
            Self::LoadFailed => "native-science-loop-load-failed",
            Self::RunFailed => "native-science-loop-run-failed",
            Self::EvaluateFailed => "native-science-loop-evaluate-failed",
        }
    }
}

/// Redacted science-loop error carrying only its phase classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeScienceLoopV1Error {
    kind: NativeScienceLoopV1ErrorKind,
}

impl NativeScienceLoopV1Error {
    pub const fn kind(self) -> NativeScienceLoopV1ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeScienceLoopV1Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeScienceLoopV1Error {}

type Result<T> = std::result::Result<T, NativeScienceLoopV1Error>;

const fn loop_error_v1(kind: NativeScienceLoopV1ErrorKind) -> NativeScienceLoopV1Error {
    NativeScienceLoopV1Error { kind }
}

fn map_busy_v1<K>(
    kind: NativeScienceLoopV1ErrorKind,
    busy: impl Fn(&K) -> bool,
    unsupported: impl Fn(&K) -> bool,
) -> impl Fn(K) -> NativeScienceLoopV1Error {
    move |error| {
        loop_error_v1(if busy(&error) {
            NativeScienceLoopV1ErrorKind::StoreBusy
        } else if unsupported(&error) {
            NativeScienceLoopV1ErrorKind::UnsupportedPlatform
        } else {
            kind
        })
    }
}

const fn map_genesis_publisher_error_kind_v1(
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> NativeScienceLoopV1Error {
    loop_error_v1(match kind {
        NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform => {
            NativeScienceLoopV1ErrorKind::UnsupportedPlatform
        }
        NativeTrainingStorePublisherV2ErrorKind::StoreBusy => {
            NativeScienceLoopV1ErrorKind::StoreBusy
        }
        _ => NativeScienceLoopV1ErrorKind::GenesisFailed,
    })
}

/// Move-only report of one complete science-loop invocation.
#[derive(Debug)]
pub struct NativeScienceLoopReportV1 {
    latest_generation_index: u64,
    reference_run: NativeCheckpointRunResultV1,
    candidate_run: NativeCheckpointRunResultV1,
    evaluation: NativeCheckpointUniformDeltaEvaluationV1,
}

impl NativeScienceLoopReportV1 {
    pub const fn latest_generation_index(&self) -> u64 {
        self.latest_generation_index
    }

    /// The update-zero reference run.
    pub const fn reference_run(&self) -> &NativeCheckpointRunResultV1 {
        &self.reference_run
    }

    /// The latest trained candidate run.
    pub const fn candidate_run(&self) -> &NativeCheckpointRunResultV1 {
        &self.candidate_run
    }

    /// The seat-swapped uniform reward-delta evaluation of candidate minus
    /// reference.
    pub const fn evaluation(&self) -> &NativeCheckpointUniformDeltaEvaluationV1 {
        &self.evaluation
    }
}

/// Run the complete one-command science loop.
///
/// Bootstrap or reopen the Store under `parent/root_basename`, publish the
/// genesis generation whenever no `latest.json` final exists, train to the
/// run's target entirely through resume-reconstructed executors, validate the
/// complete Store, then run and evaluate the update-zero and latest boundaries.
///
/// `ladder_opponent` is the Self-Play Ladder Design Contract S2 opponent
/// engine (Section 5): `None` reproduces today's uniform-opponent behavior
/// exactly and is what every caller outside the ladder pilot integration
/// passes; `Some(engine)` is threaded onto every window's reconstructed
/// executor before that window trains.
///
/// `LadderOpponentEngineV1` is deliberately `pub(crate)` (see its module
/// docs): only this crate can ever construct `Some(engine)`, so this stays a
/// sealed capability parameter for external callers, who can still call this
/// function with `None`.
#[allow(private_interfaces)]
pub fn run_native_science_loop_v1(
    parent: impl AsRef<Path>,
    root_basename: &str,
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
    runner_config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
) -> Result<NativeScienceLoopReportV1> {
    use crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2ErrorKind;

    validate_prepared_execution_config_v1(run, &execution_config)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid))?;

    // Bootstrap admits only the frozen B0 through B8 states.
    let bootstrapped = bootstrap_native_training_store_v2(parent.as_ref(), root_basename)
        .map_err(map_busy_v1(
        NativeScienceLoopV1ErrorKind::BootstrapFailed,
        |error: &crate::native_training_store_bootstrap_v2::NativeTrainingStoreBootstrapV2Error| {
            error.kind() == NativeTrainingStoreBootstrapV2ErrorKind::StoreBusy
        },
        |error| error.kind() == NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform,
    ))?;
    // A missing latest final includes both a fresh skeleton and interrupted
    // bootstrap after exact run authority (and possibly candidate-equal
    // generation-zero finals or a latest stage). The publisher revalidates
    // the complete state under its own exclusive lock before any mutation.
    let genesis_required = !bootstrapped.latest_final_present();
    let root: ValidatedNativeTrainingStoreRootV2 = bootstrapped.into_root();

    // Train-new bootstrap and interrupted-bootstrap recovery both reconstruct
    // the exact genesis candidate from the independently attested common
    // snapshot. The receipt witnesses publication of exactly generation zero.
    if genesis_required {
        let genesis_error = loop_error_v1(NativeScienceLoopV1ErrorKind::GenesisFailed);
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            snapshot_manifest_path,
            snapshot_payload_path,
        )
        .map_err(|_| genesis_error)?;
        let candidate = executor
            .checkpoint_candidate_v1()
            .map_err(|_| genesis_error)?;
        let payload = candidate.payload().to_vec();
        let checkpoint =
            build_genesis_checkpoint_manifest_v3(run, &payload).map_err(|_| genesis_error)?;
        let segment =
            build_genesis_segment_manifest_v2(run, &checkpoint).map_err(|_| genesis_error)?;
        let boundary = build_genesis_native_training_boundary_v2(run, &segment, &checkpoint)
            .map_err(|_| genesis_error)?;
        let reference = build_checkpoint_reference_v2(run, &boundary).map_err(|_| genesis_error)?;
        let latest = build_latest_v2(&boundary, &reference).map_err(|_| genesis_error)?;
        let receipt = publish_genesis_generation_v2(
            &root,
            run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .map_err(|error| map_genesis_publisher_error_kind_v1(error.kind()))?;
        if receipt.generation_index() != 0 {
            return Err(genesis_error);
        }
    }

    // Train to the exact target: every window runs on a reconstructed
    // executor and commits only through the durable receipt.
    let latest_generation_index = loop {
        let resumed = resume_native_training_store_v2(&root, run, execution_config.clone())
            .map_err(map_busy_v1(
            NativeScienceLoopV1ErrorKind::TrainFailed,
            |error: &crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2Error| {
                error.kind() == NativeTrainingStoreResumeV2ErrorKind::StoreBusy
            },
            |error| error.kind() == NativeTrainingStoreResumeV2ErrorKind::UnsupportedPlatform,
        ))?;
        match resumed {
            NativeTrainingStoreResumeV2::Complete {
                latest_generation_index,
            } => break latest_generation_index,
            NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                let train_error = loop_error_v1(NativeScienceLoopV1ErrorKind::TrainFailed);
                // Self-Play Ladder Design Contract S2, Section 5. Every
                // window trains on a freshly reconstructed executor
                // (`resume_native_training_store_v2`'s own design); the
                // ladder engine (when configured) is threaded onto each one
                // here, before that window's update runs.
                continuation
                    .executor
                    .set_ladder_opponent_v1(ladder_opponent.clone());
                let prepared = prepare_segment_v2(
                    &mut continuation.executor,
                    run,
                    &continuation.parent_boundary,
                    &continuation.parent_checkpoint,
                )
                .map_err(|_| train_error)?;
                let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                    &root,
                    run,
                    &continuation.parent_boundary,
                    &continuation.parent_checkpoint,
                    &prepared,
                )
                .map_err(|_| train_error)?;
                prepared.commit_v2(receipt).map_err(|_| train_error)?;
            }
        }
    };

    // Full-store currentness validation after training.
    let state = validate_native_training_store_v2(&root, run)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::ValidateFailed))?;
    if state.latest_generation_index() != latest_generation_index {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::ValidateFailed));
    }

    // Load the update-zero and latest boundaries through the complete decode
    // chain, then run both through the checkpoint-backed runner.
    let load_error = loop_error_v1(NativeScienceLoopV1ErrorKind::LoadFailed);
    let reference_boundary =
        load_native_training_boundary_v2(&root, run, 0).map_err(|_| load_error)?;
    let candidate_boundary = load_native_training_boundary_v2(&root, run, latest_generation_index)
        .map_err(|_| load_error)?;

    let run_error = loop_error_v1(NativeScienceLoopV1ErrorKind::RunFailed);
    let reference_run = run_native_checkpoint_v1(
        run,
        reference_boundary.checkpoint(),
        reference_boundary.payload(),
        runner_config,
    )
    .map_err(|_| run_error)?;
    let candidate_run = run_native_checkpoint_v1(
        run,
        candidate_boundary.checkpoint(),
        candidate_boundary.payload(),
        runner_config,
    )
    .map_err(|_| run_error)?;

    let evaluation = evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::EvaluateFailed))?;

    Ok(NativeScienceLoopReportV1 {
        latest_generation_index,
        reference_run,
        candidate_run,
        evaluation,
    })
}

#[cfg(all(test, windows))]
mod windows_science_loop_tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    use crate::native_training_store_bootstrap_v2::{
        bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapOutcomeV2,
    };
    use crate::native_training_store_resume_v2::test_execution_config_v2;
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_v2, test_fixture_bytes_with_schedule_v2,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    struct TestParentV1 {
        parent: PathBuf,
    }

    impl TestParentV1 {
        fn new(label: &str) -> Self {
            static ORDINAL: AtomicU64 = AtomicU64::new(0);
            let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
            let parent = std::env::temp_dir().join(format!(
                "mtg-kernel-science-loop-v1-{}-{label}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&parent).expect("create test parent");
            Self { parent }
        }
    }

    impl Drop for TestParentV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.parent);
        }
    }

    fn runner_config_v1() -> NativeCheckpointRunnerConfigV1 {
        NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 2,
            scheduler_timeout: Duration::from_secs(300),
            measure_broker_service_time: false,
        }
    }

    fn one_segment_run_v1() -> ValidatedTrainRunV2 {
        decode_train_run_v2(&test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            4,
            2,
            4,
            8,
            32_768,
            65_536,
        ))
        .unwrap()
    }

    fn establish_exact_run_only_v1(parent: &TestParentV1, run: &ValidatedTrainRunV2) -> PathBuf {
        let bootstrapped = bootstrap_native_training_store_v2(&parent.parent, "store").unwrap();
        assert_eq!(
            bootstrapped.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        );
        assert!(!bootstrapped.latest_final_present());
        drop(bootstrapped);

        let root_path = parent.parent.join("store");
        fs::write(root_path.join("run.json"), run.canonical_bytes()).unwrap();
        root_path
    }

    fn assert_generation_directories_empty_v1(root_path: &Path) {
        for directory in ["segments", "checkpoints", "heads", "refs"] {
            assert_eq!(
                fs::read_dir(root_path.join(directory)).unwrap().count(),
                0,
                "{directory} must remain empty"
            );
        }
    }

    /// Learning smoke at the settled K=64 operating point: train a real
    /// episode count through the one-command loop on the CudaBurnDense
    /// backend, then report the seat-swapped uniform reward delta of the
    /// trained boundary against update-zero over enough evaluation pairs to
    /// see a direction. Diagnostic only: prints the delta and outcome
    /// counts, makes no learning-quality gate claim (the ratified gate has
    /// its own frozen estimands, seeds, and thresholds).
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn learning_smoke_k64_uniform_delta_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let updates = 128_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("smoke run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 256,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };

        let parent = TestParentV1::new("learning-smoke");
        let started = std::time::Instant::now();
        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            execution_config,
            &snapshot_manifest,
            &snapshot_payload,
            runner_config,
            None,
        )
        .expect("learning smoke loop");
        let wall = started.elapsed().as_secs_f64();

        let evaluation = report.evaluation();
        let reference = evaluation.reference_learner_outcomes();
        let candidate = evaluation.candidate_learner_outcomes();
        println!(
            "learning smoke: K=64 updates={updates} episodes={} wall={wall:.1}s",
            64 * updates
        );
        println!(
            "reference (update-zero) W/L/D: {}/{}/{} of {}",
            reference.wins(),
            reference.losses(),
            reference.draws(),
            reference.total()
        );
        println!(
            "candidate (gen {}) W/L/D: {}/{}/{} of {}",
            report.latest_generation_index(),
            candidate.wins(),
            candidate.losses(),
            candidate.draws(),
            candidate.total()
        );
        println!(
            "pairs={} total candidate-minus-reference reward delta = {}",
            evaluation.pair_count(),
            evaluation.total_candidate_minus_reference_reward_delta()
        );
    }

    /// Qualification measurement at 256-update depth: the diagnostic loop
    /// with the bridge's test-only measurement mode enabled, so the
    /// transported-logit gate records per-row f64 promotion metrics (delta
    /// range with worst-row identity and gauge-invariant row scale,
    /// selected-log-probability delta, per-decision cumulative joint error)
    /// instead of failing at the unratified bound. Measurement evidence for
    /// the gate qualification campaign; banks nothing.
    /// RAII guard for the bridge's measurement flag: arming sets it, and the
    /// drop runs on every exit path including panics, so no early failure
    /// can leak record-only mode into later same-process tests.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    struct MeasurementModeGuardV1;

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    impl MeasurementModeGuardV1 {
        fn arm() -> Self {
            crate::experimental_burn_net8_packed_v1::bridge::TOLERANCE_MEASUREMENT_MODE_V1
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Self
        }
    }

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    impl Drop for MeasurementModeGuardV1 {
        fn drop(&mut self) {
            crate::experimental_burn_net8_packed_v1::bridge::TOLERANCE_MEASUREMENT_MODE_V1
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Multirun orchestration pilot: N concurrent science loops in one
    /// process, each with a distinct held-out base seed via the combined
    /// fixture helper, a distinct store root, and the full durable
    /// pipeline. Proves same-process isolation (per-root store locks,
    /// content-keyed resident device slot, per-run rollout state) and
    /// measures real aggregate throughput under contention. Non-banked
    /// diagnostic; wall-clock numbers are environment-dependent.
    ///
    /// Sweep knobs arrive via env so one binary serves every point:
    /// MULTIRUN_RUNS, MULTIRUN_UPDATES, MULTIRUN_WORKERS,
    /// MULTIRUN_SESSIONS, MULTIRUN_BROKER_TARGET. The broker target is
    /// the contention knob: the trainer sizes its forward pool as
    /// min(broker_batch_target, actors, cores), so N runs partition the
    /// machine's scoring cores only when N * target <= cores.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn multirun_pilot_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_v2;

        fn env_knob_v1(name: &str, default: u64) -> u64 {
            match std::env::var(name) {
                Ok(raw) => raw.parse().expect("multirun knob must be a u64"),
                Err(_) => default,
            }
        }

        let run_count = usize::try_from(env_knob_v1("MULTIRUN_RUNS", 4)).unwrap();
        let updates = env_knob_v1("MULTIRUN_UPDATES", 16);
        let workers = env_knob_v1("MULTIRUN_WORKERS", 2);
        let sessions = env_knob_v1("MULTIRUN_SESSIONS", 32);
        let broker_target = env_knob_v1("MULTIRUN_BROKER_TARGET", 16);
        // S1 mirror-validation knobs (test-harness only; no model or
        // schedule semantics change): MULTIRUN_BASE_SEED replaces the
        // hardcoded 424_242, MULTIRUN_SEED_OFFSET disambiguates ordinals
        // across concurrent device processes so N seeds stay globally
        // distinct, and MULTIRUN_STORE_PARENT retains per-run stores under
        // a durable directory instead of the auto-deleting temp parent.
        let base_seed = env_knob_v1("MULTIRUN_BASE_SEED", 424_242);
        let seed_offset = env_knob_v1("MULTIRUN_SEED_OFFSET", 0);
        let durable_parent = std::env::var("MULTIRUN_STORE_PARENT").ok();
        // MULTIRUN_RECORD_ONLY=1 arms the bridge's record-only measurement
        // mode for the whole pilot, same guard as pathfinding_run_k64_deep_v1
        // (process-wide flag; the guard drops on every exit path). Without
        // it, depths past ~128 updates risk a mid-run fail-closed abort on
        // the transported-logit hard tolerance, exactly as measured in the
        // depth-drift characterization; the S1 mirror-validation plan
        // predeclares record-only for this reason.
        let record_only = env_knob_v1("MULTIRUN_RECORD_ONLY", 0) != 0;
        let _measurement_mode = if record_only {
            Some(MeasurementModeGuardV1::arm())
        } else {
            None
        };
        // Self-Play Ladder Design Contract S2, Section 6 (pilot). MULTIRUN_LADDER=1
        // builds a ladder opponent engine from MULTIRUN_LADDER_POOL_DIR (a
        // directory with three subdirectories -- primary/, pred-a/, pred-b/,
        // each a complete Store root for that pool member's source run; see
        // `native_ladder_pool_resolution_v1`'s module docs for why a full
        // Store root is required for a nonzero-generation checkpoint -- plus
        // pool.json carrying the `OpponentLadderPoolContractV1` JSON) and
        // threads the same engine into every spawned run. Each run's record
        // then carries the ladder identity + pool + schedule V2 sections
        // (`test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`)
        // instead of the uniform identity; the uniform path (MULTIRUN_LADDER
        // unset) is completely untouched.
        let ladder_enabled = env_knob_v1("MULTIRUN_LADDER", 0) != 0;
        let ladder_pool: Option<crate::native_training_store_run_v2::OpponentLadderPoolContractV1> =
            if ladder_enabled {
                let pool_dir = std::env::var("MULTIRUN_LADDER_POOL_DIR")
                    .expect("MULTIRUN_LADDER=1 requires MULTIRUN_LADDER_POOL_DIR");
                let pool_dir = std::path::PathBuf::from(pool_dir);
                let pool_bytes = fs::read(pool_dir.join("pool.json"))
                    .expect("MULTIRUN_LADDER_POOL_DIR/pool.json");
                let pool = serde_json::from_slice(&pool_bytes)
                    .expect("pool.json must decode as OpponentLadderPoolContractV1");
                Some(pool)
            } else {
                None
            };
        let ladder_engine: Option<Arc<LadderOpponentEngineV1>> = match &ladder_pool {
            Some(pool) => {
                let pool_dir = std::path::PathBuf::from(
                    std::env::var("MULTIRUN_LADDER_POOL_DIR")
                        .expect("MULTIRUN_LADDER=1 requires MULTIRUN_LADDER_POOL_DIR"),
                );
                let (primary, predecessor_a, predecessor_b) =
                    crate::native_ladder_pool_resolution_v1::resolve_ladder_pool_v1(
                        pool,
                        &pool_dir.join("primary"),
                        &pool_dir.join("pred-a"),
                        &pool_dir.join("pred-b"),
                    )
                    .expect("ladder pool members must resolve to validated checkpoint handles");
                let engine = LadderOpponentEngineV1::new_v1(
                    pool.clone(),
                    primary,
                    predecessor_a,
                    predecessor_b,
                )
                .expect("MULTIRUN_LADDER_POOL_DIR/pool.json must match the frozen pool literals");
                Some(Arc::new(engine))
            }
            None => None,
        };
        println!(
            "MULTIRUN CONFIG runs={run_count} updates={updates} topology={workers}x{sessions} \
             broker_target={broker_target} base_seed={base_seed} seed_offset={seed_offset} \
             record_only={record_only} ladder={ladder_enabled}"
        );
        let started = std::time::Instant::now();
        let handles: Vec<_> = (0..run_count)
            .map(|ordinal| {
                let durable_parent = durable_parent.clone();
                let ladder_pool = ladder_pool.clone();
                let ladder_engine = ladder_engine.clone();
                std::thread::spawn(move || {
                    let run_seed = base_seed + seed_offset + ordinal as u64;
                    let patched = match &ladder_pool {
                        Some(pool) => {
                            use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_v2;
                            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
                                NativeTrainingNumericalBackendV1::CudaBurnDense,
                                64,
                                4,
                                updates,
                                workers,
                                sessions,
                                broker_target,
                                1_024,
                                2_048,
                                run_seed,
                                pool.clone(),
                            )
                        }
                        None => test_fixture_bytes_with_schedule_and_base_seed_v2(
                            NativeTrainingNumericalBackendV1::CudaBurnDense,
                            64,
                            4,
                            updates,
                            workers,
                            sessions,
                            broker_target,
                            1_024,
                            2_048,
                            run_seed,
                        ),
                    };
                    let run = decode_train_run_v2(&patched).expect("pilot run record");
                    let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
                    let mut execution_config = test_execution_config_v2(&run);
                    execution_config.numerical_backend =
                        NativeTrainingNumericalBackendV1::CudaBurnDense;
                    let (parent_path, _ephemeral_guard) = match durable_parent {
                        Some(dir) => {
                            let path = std::path::PathBuf::from(dir)
                                .join(format!("run-{ordinal}"));
                            fs::create_dir_all(&path).expect("durable multirun parent");
                            (path, None)
                        }
                        None => {
                            let guard =
                                TestParentV1::new(&format!("multirun-pilot-{ordinal}"));
                            (guard.parent.clone(), Some(guard))
                        }
                    };
                    println!(
                        "MULTIRUN run={ordinal} seed={run_seed} store_root={}",
                        parent_path.join("store").display()
                    );
                    let runner_config = NativeCheckpointRunnerConfigV1 {
                        evaluation_base_seed: 7_777,
                        first_episode_index: 0,
                        episode_count: 32,
                        scheduler_timeout: Duration::from_secs(3_600),
                        measure_broker_service_time: false,
                    };
                    let run_started = std::time::Instant::now();
                    let report = run_native_science_loop_v1(
                        &parent_path,
                        "store",
                        &run,
                        execution_config,
                        &snapshot_manifest,
                        &snapshot_payload,
                        runner_config,
                        ladder_engine.clone(),
                    )
                    .expect("pilot loop");
                    let wall = run_started.elapsed().as_secs_f64();
                    let outcomes = report.evaluation().candidate_learner_outcomes();
                    (
                        ordinal,
                        report.latest_generation_index(),
                        wall,
                        outcomes.wins(),
                        outcomes.losses(),
                    )
                })
            })
            .collect();
        let mut total_episodes = 0_u64;
        for handle in handles {
            let (ordinal, generation, wall, wins, losses) = handle.join().expect("pilot thread");
            assert_eq!(generation, updates);
            total_episodes += 64 * updates;
            println!(
                "MULTIRUN run={ordinal} gen={generation} wall={wall:.1}s eval W/L {wins}/{losses}"
            );
        }
        let aggregate_wall = started.elapsed().as_secs_f64();
        println!(
            "MULTIRUN AGGREGATE runs={run_count} episodes={total_episodes} \
             wall={aggregate_wall:.1}s eps_per_s={:.2} (non-evidence)",
            total_episodes as f64 / aggregate_wall
        );
    }

    /// Saturation-curve evaluation over a surviving pathfinding store:
    /// loads the boundary at each requested generation and runs the
    /// checkpoint runner against uniform opponents, printing win/loss/draw
    /// S1 mirror-validation checkpoint evaluation: identical body shape to
    /// pathfinding_saturation_eval_v1 but reconstructs the run record with
    /// the S1 training schedule (256 updates, workers=2, sessions=32,
    /// broker target 16) and the store's own base seed, since boundary
    /// loading validates the run identity the store was created under.
    /// Env: S1_STORE_ROOT, S1_EVAL_GENS (comma-separated), S1_BASE_SEED.
    /// 256 seat-swapped pairs per generation at evaluation_base_seed 7_777,
    /// the predeclared S1 estimator.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn s1_mirror_saturation_eval_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_v2;

        let root_path = std::env::var("S1_STORE_ROOT")
            .expect("S1_STORE_ROOT must name the S1 per-seed store root");
        let generations: Vec<u64> = std::env::var("S1_EVAL_GENS")
            .expect("S1_EVAL_GENS must list generations")
            .split(',')
            .map(|token| token.trim().parse().expect("generation index"))
            .collect();
        let base_seed: u64 = std::env::var("S1_BASE_SEED")
            .expect("S1_BASE_SEED must give the store's base seed")
            .parse()
            .expect("base seed u64");
        let eval_pairs: u64 = std::env::var("S1_EVAL_PAIRS")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("eval pair count");

        let patched = test_fixture_bytes_with_schedule_and_base_seed_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            256,
            2,
            32,
            16,
            1_024,
            2_048,
            base_seed,
        );
        let run = decode_train_run_v2(&patched).expect("s1 run record");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: eval_pairs,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };

        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_native_checkpoint_v1(
            &run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config,
        )
        .unwrap();
        for generation in generations {
            let boundary = load_native_training_boundary_v2(&root, &run, generation).unwrap();
            let candidate_run = run_native_checkpoint_v1(
                &run,
                boundary.checkpoint(),
                boundary.payload(),
                runner_config,
            )
            .unwrap();
            let evaluation =
                evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
                    .unwrap();
            let outcomes = evaluation.candidate_learner_outcomes();
            println!(
                "S1_SATURATION seed={base_seed} gen={generation} W/L/D {}/{}/{} of {} (delta vs gen0 {})",
                outcomes.wins(),
                outcomes.losses(),
                outcomes.draws(),
                outcomes.total(),
                evaluation.total_candidate_minus_reference_reward_delta()
            );
        }
    }

    /// Ladder-store checkpoint evaluation (Self-Play Ladder Design Contract
    /// S2 pilot, Deliverable 1): identical body shape to
    /// `s1_mirror_saturation_eval_v1`, but reconstructs the run record with
    /// the LADDER fixture builder (the SAME builder the pilot used to train
    /// the store, `test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`
    /// -- commit 3ce8f48/ca58b58's fixture, with the SAME K=64/S=4 topology
    /// 2/32/16 the pilot's `pilot-rung1.sh` used) instead of the uniform
    /// fixture, so `load_native_training_boundary_v2` accepts the ladder
    /// store's boundaries (boundary loading validates the run identity the
    /// store was created under, and a ladder run's `run_sha256` differs from
    /// a uniform run's at the same schedule/topology/base-seed because the
    /// opponent contracts and pool section are hashed into it). The pool
    /// contract embedded in the reconstructed record is parsed byte-for-byte
    /// from `LADDER_POOL_JSON` (`OpponentLadderPoolContractV1`'s `Deserialize`
    /// impl, the SAME decode `multirun_pilot_v1`'s `MULTIRUN_LADDER_POOL_DIR`
    /// branch already uses), not hand-reconstructed, so the identity matches
    /// exactly.
    ///
    /// This is the panel/v0 (uniform-anchor) curve only (stopping policy
    /// 126fd81a..., Section 3's interim clause: "until [the policy-driven
    /// opponent] seat exists, an interim panel/v0 = uniform only applies");
    /// the frozen uniform opponent is the SAME opponent every uniform
    /// evaluation in this file already uses -- nothing about the OPPONENT
    /// side of this probe is ladder-specific, only the candidate/reference
    /// checkpoints' run-record identity is.
    ///
    /// The checkpoint runner (`run_native_checkpoint_v1`) needs no CUDA
    /// feature: it is the CPU dense inference path
    /// (`NativeCheckpointInferenceV1::score_decision_v1`), verified
    /// feature-gate-free in `native_checkpoint_runner_v1.rs` and
    /// `native_checkpoint_inference_v1.rs`; only the run record's DECLARED
    /// backend identity is `CudaBurnDense` (matching what the pilot actually
    /// trained under), which decodes and validates without the CUDA feature
    /// compiled in (see `cuda_backend_records_validate_and_mismatched_pairs_reject`,
    /// itself not feature-gated). This probe is therefore deliberately NOT
    /// gated behind `experimental-burn-net8-packed-cuda-v1`, unlike its
    /// sibling GPU-training probes in this file.
    ///
    /// Env: LADDER_STORE_ROOT, LADDER_EVAL_GENS (comma-separated),
    /// LADDER_BASE_SEED, LADDER_POOL_JSON (path to the pool.json whose
    /// contents the run record embeds). LADDER_EVAL_PAIRS (default 256,
    /// S1_EVAL_PAIRS-style knob, scoped to this probe only) sets
    /// `episode_count` directly, matching `s1_mirror_saturation_eval_v1`'s
    /// own `S1_EVAL_PAIRS` wiring convention byte-for-byte (that probe's
    /// knob is likewise a direct `episode_count` pass-through despite its
    /// name; mirrored here rather than "corrected" so this probe stays
    /// behaviorally identical to its S1 twin, as instructed). 256
    /// seat-swapped pairs at evaluation_base_seed 7_777 by default, the
    /// predeclared S1 estimator.
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn ladder_saturation_eval_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2, OpponentLadderPoolContractV1,
        };

        let root_path = std::env::var("LADDER_STORE_ROOT")
            .expect("LADDER_STORE_ROOT must name the ladder per-seed store root");
        let generations: Vec<u64> = std::env::var("LADDER_EVAL_GENS")
            .expect("LADDER_EVAL_GENS must list generations")
            .split(',')
            .map(|token| token.trim().parse().expect("generation index"))
            .collect();
        let base_seed: u64 = std::env::var("LADDER_BASE_SEED")
            .expect("LADDER_BASE_SEED must give the store's base seed")
            .parse()
            .expect("base seed u64");
        let pool_json_path = std::env::var("LADDER_POOL_JSON")
            .expect("LADDER_POOL_JSON must name the pool.json path embedded in the run record");
        let eval_pairs: u64 = std::env::var("LADDER_EVAL_PAIRS")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("eval pair count");
        // The reconstructed run identity must match the store's actual
        // schedule; the retry rungs train 512 updates (contract v4.1 8B).
        let ladder_updates: u64 = std::env::var("LADDER_UPDATES")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("ladder updates");

        let pool_bytes =
            fs::read(&pool_json_path).expect("LADDER_POOL_JSON must be a readable file");
        let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
            .expect("pool.json must decode as OpponentLadderPoolContractV1");

        let patched = test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            ladder_updates,
            2,
            32,
            16,
            1_024,
            2_048,
            base_seed,
            pool,
        );
        let run = decode_train_run_v2(&patched).expect("ladder run record");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: eval_pairs,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };

        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_native_checkpoint_v1(
            &run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config,
        )
        .unwrap();
        for generation in generations {
            let boundary = load_native_training_boundary_v2(&root, &run, generation).unwrap();
            let candidate_run = run_native_checkpoint_v1(
                &run,
                boundary.checkpoint(),
                boundary.payload(),
                runner_config,
            )
            .unwrap();
            let evaluation =
                evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
                    .unwrap();
            let outcomes = evaluation.candidate_learner_outcomes();
            println!(
                "LADDER_SATURATION seed={base_seed} gen={generation} W/L/D {}/{}/{} of {} (delta vs gen0 {})",
                outcomes.wins(),
                outcomes.losses(),
                outcomes.draws(),
                outcomes.total(),
                evaluation.total_candidate_minus_reference_reward_delta()
            );
        }
    }

    /// Head-to-head evaluator (Self-Play Ladder Design Contract S2 pilot,
    /// Deliverable 2): runs a candidate ladder checkpoint through the
    /// checkpoint runner with the LADDER ENGINE standing in the opponent
    /// seat, where the engine's three policy-driven pool slots are ALL
    /// independently loaded handles onto ONE fixed opponent checkpoint and
    /// the per-episode pool-choice is forced to always select a policy
    /// member (`LadderOpponentEngineV1::head_to_head_eval_v1`, EVAL-ONLY,
    /// `cfg(test)`-gated -- see its docs in `native_ladder_opponent_v1` for
    /// the full production-safety argument). Uses the EVAL-ONLY
    /// `run_native_checkpoint_with_ladder_opponent_eval_v1` runner twin (see
    /// `native_checkpoint_runner_v1` docs for why `run_native_checkpoint_v1`
    /// itself cannot take an engine without production-path surgery: its
    /// only opponent hook is a hardcoded `None`, and the executor-side
    /// ladder threading from da8e486 reaches only the training loop).
    ///
    /// Seat-swapped CRN pairing matches the S1 estimator convention
    /// automatically: the checkpoint runner's native schedule
    /// (`native_trainer_episode_schedule_v1`) already alternates the
    /// LEARNER's seat every consecutive episode while pairing episodes
    /// `2k`/`2k+1` onto the SAME environment seed, for ANY opponent
    /// (uniform or ladder-engine) -- this probe changes only who scores the
    /// opponent seat's decisions, not the schedule that decides seeding or
    /// seat alternation.
    ///
    /// The candidate side is reconstructed exactly like
    /// `ladder_saturation_eval_v1` (same fixture builder, same fixed
    /// K=64/S=4/topology-2-32-16 schedule, pool parsed from
    /// `H2H_CANDIDATE_POOL_JSON`). The opponent is loaded DIRECTLY from
    /// `H2H_OPPONENT_STORE_ROOT` (a full Store root: its own `run.json` plus
    /// the validated walk to that store's own latest generation) rather
    /// than through `native_ladder_pool_resolution_v1`'s ref-digest gate --
    /// permitted explicitly by the task ("digest optional here since this
    /// is eval-only"), and three independent
    /// `NativeCheckpointInferenceV1` handles are loaded from that ONE
    /// validated checkpoint (the type is deliberately not `Clone`) for the
    /// engine's three policy-driven slots.
    ///
    /// `H2H_PAIRS` (default 1_024) is the CRN PAIR count. This probe doubles
    /// `H2H_PAIRS` into `episode_count` so the run plays `2 * H2H_PAIRS`
    /// GAMES, matching the promotion gate's own literal (Amendment 1 /
    /// Section 8A point 1: `PROMOTION_GATE_GAME_COUNT_V1 = 2_048`, "the
    /// candidate plays 1,024 seat-swapped CRN pairs = 2,048 games... gate
    /// quantity = wins / 2,048"). This doubling also reproduces the
    /// contract's own predeclared "~8 minutes" head-to-head economics
    /// estimate (Section 4) far more closely than a non-doubled 1,024-game
    /// run would, which independently corroborates the doubling as the
    /// correct reading; the discrepancy with the panel-probe convention is
    /// disclosed rather than silently resolved.
    ///
    /// GATE FEED (Amendment 1 / Section 8A point 1): the gate's win-rate
    /// sub-check is fed the RAW GAME win count (`wins`, tabulated at leg
    /// granularity) over the raw game total, never the pair-level
    /// `pair_wins` this probe also tabulates below -- feeding pair wins into
    /// a game-denominated gate (or vice versa) silently misstates the win
    /// rate. `pair_wins` is retained ONLY as a labeled diagnostic (the
    /// contract's own "net-positive-per-pair" reading, REJECTED as the gate
    /// quantity: "at true parity under CRN seat symmetry, winning both legs
    /// of a pair is structurally rare, and that metric misrepresents parity
    /// as collapse").
    ///
    /// Env: H2H_CANDIDATE_STORE_ROOT, H2H_CANDIDATE_GEN,
    /// H2H_CANDIDATE_BASE_SEED, H2H_CANDIDATE_POOL_JSON,
    /// H2H_OPPONENT_STORE_ROOT, H2H_PAIRS (default 1_024), H2H_EVAL_SEED
    /// (default 7_777). Prints `H2H candidate_gen=<g> W/L/D x/y/z of N` plus
    /// the promotion gate's win-rate sub-check verdict computed by actually
    /// calling `native_ladder_promotion_v1::promotion_gate_win_rate_passes_v1`
    /// on the tabulated raw game wins/games (Deliverable 4's "feed the
    /// results through the gate function").
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn ladder_head_to_head_eval_v1() {
        use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
        use crate::native_checkpoint_runner_v1::run_native_checkpoint_with_ladder_opponent_eval_v1;
        use crate::native_ladder_promotion_v1::promotion_gate_win_rate_passes_v1;
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2, OpponentLadderPoolContractV1,
        };
        use crate::rl::PlayerSeatV1;

        fn required_env_v1(name: &str) -> String {
            std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
        }

        let candidate_store_root = required_env_v1("H2H_CANDIDATE_STORE_ROOT");
        let candidate_gen: u64 = required_env_v1("H2H_CANDIDATE_GEN")
            .parse()
            .expect("candidate generation");
        let candidate_base_seed: u64 = required_env_v1("H2H_CANDIDATE_BASE_SEED")
            .parse()
            .expect("candidate base seed");
        let candidate_pool_json = required_env_v1("H2H_CANDIDATE_POOL_JSON");
        let opponent_store_root = required_env_v1("H2H_OPPONENT_STORE_ROOT");
        let pairs: u64 = std::env::var("H2H_PAIRS")
            .unwrap_or_else(|_| "1024".to_owned())
            .parse()
            .expect("H2H_PAIRS");
        let eval_seed: u64 = std::env::var("H2H_EVAL_SEED")
            .unwrap_or_else(|_| "7777".to_owned())
            .parse()
            .expect("H2H_EVAL_SEED");
        let ladder_updates: u64 = std::env::var("H2H_UPDATES")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("h2h updates");
        let episode_count = pairs.checked_mul(2).expect("H2H_PAIRS overflow");

        // Candidate: the SAME ladder run-record reconstruction as
        // ladder_saturation_eval_v1.
        let pool_bytes = fs::read(&candidate_pool_json)
            .expect("H2H_CANDIDATE_POOL_JSON must be a readable file");
        let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
            .expect("pool.json must decode as OpponentLadderPoolContractV1");
        let candidate_run_bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            ladder_updates,
            2,
            32,
            16,
            1_024,
            2_048,
            candidate_base_seed,
            pool,
        );
        let candidate_run =
            decode_train_run_v2(&candidate_run_bytes).expect("candidate ladder run record");
        let candidate_root =
            ValidatedNativeTrainingStoreRootV2::open_v2(&candidate_store_root).unwrap();
        let candidate_boundary =
            load_native_training_boundary_v2(&candidate_root, &candidate_run, candidate_gen)
                .unwrap();

        // Opponent: loaded DIRECTLY from a full Store root (digest
        // re-validation optional, eval-only tooling -- see docs above).
        let opponent_run_bytes =
            fs::read(std::path::Path::new(&opponent_store_root).join("run.json"))
                .expect("H2H_OPPONENT_STORE_ROOT/run.json must be readable");
        let opponent_run = decode_train_run_v2(&opponent_run_bytes).expect("opponent run record");
        let opponent_root =
            ValidatedNativeTrainingStoreRootV2::open_v2(&opponent_store_root).unwrap();
        let opponent_state =
            validate_native_training_store_v2(&opponent_root, &opponent_run).unwrap();
        let opponent_checkpoint = opponent_state.latest_checkpoint();
        let opponent_payload = opponent_state.latest_payload();
        let primary = load_native_checkpoint_inference_v1(
            &opponent_run,
            opponent_checkpoint,
            opponent_payload,
        )
        .unwrap();
        let predecessor_a = load_native_checkpoint_inference_v1(
            &opponent_run,
            opponent_checkpoint,
            opponent_payload,
        )
        .unwrap();
        let predecessor_b = load_native_checkpoint_inference_v1(
            &opponent_run,
            opponent_checkpoint,
            opponent_payload,
        )
        .unwrap();
        let engine = Arc::new(LadderOpponentEngineV1::head_to_head_eval_v1(
            primary,
            predecessor_a,
            predecessor_b,
        ));

        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: eval_seed,
            first_episode_index: 0,
            episode_count,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };
        let result = run_native_checkpoint_with_ladder_opponent_eval_v1(
            &candidate_run,
            candidate_boundary.checkpoint(),
            candidate_boundary.payload(),
            runner_config,
            Some(engine),
        )
        .unwrap();

        // Leg-level W/L/D (2 * pairs games total), matching this file's
        // established print convention for every other saturation probe.
        let episodes = &result.rollout().episodes;
        let bindings = result.episode_bindings();
        assert_eq!(episodes.len(), bindings.len());
        assert_eq!(episodes.len() as u64, episode_count);
        let mut wins = 0_u64;
        let mut losses = 0_u64;
        let mut draws = 0_u64;
        // PAIR-level win count: Amendment 1 / Section 8A point 1 REJECTS
        // this net-positive-per-pair metric as the gate quantity ("at true
        // parity under CRN seat symmetry, winning both legs of a pair is
        // structurally rare, and that metric misrepresents parity as
        // collapse"); the gate's real denominator is the raw GAME count
        // (`wins` above, tabulated at leg granularity), fed to the gate
        // function below. `pair_wins` is kept ONLY as a labeled diagnostic.
        // A pair is a "diagnostic win" iff the learner's summed reward
        // across its two seat-swapped legs (sharing one environment seed)
        // is net positive; net-zero (a true draw on both legs, or a win
        // cancelled by a loss across the seat swap) and net-negative pairs
        // both fall into "not a diagnostic win".
        let mut pair_wins = 0_u64;
        for pair_offset in 0..pairs {
            let mut pair_reward = 0_i32;
            for leg in 0..2_u64 {
                let index = usize::try_from(pair_offset * 2 + leg).unwrap();
                let binding = &bindings[index];
                let episode = &episodes[index];
                let seat_index = match binding.learner_seat() {
                    PlayerSeatV1::P0 => 0,
                    PlayerSeatV1::P1 => 1,
                };
                let reward = episode.terminal.terminal_reward[seat_index];
                match reward {
                    1 => wins += 1,
                    -1 => losses += 1,
                    0 => draws += 1,
                    other => panic!("unexpected learner reward {other} at a natural terminal"),
                }
                pair_reward += reward;
            }
            if pair_reward > 0 {
                pair_wins += 1;
            }
        }
        let total = wins + losses + draws;
        assert_eq!(total, episode_count);
        println!("H2H candidate_gen={candidate_gen} W/L/D {wins}/{losses}/{draws} of {total}");
        // Labeled diagnostic only (Amendment 1 / Section 8A point 1 rejects
        // this as the gate quantity); NOT fed to the gate function below.
        println!(
            "H2H candidate_gen={candidate_gen} pair_diagnostic_net_positive pair_wins={pair_wins}/{pairs}={:.4}",
            pair_wins as f64 / pairs as f64
        );

        // Amendment 1 / Section 8A point 1: feed the tabulated RAW GAME
        // win count through the actual gate function (win-rate sub-check
        // only; the regression sub-check needs the previous rung's panel
        // mean, computed by the caller from multiple invocations of the
        // panel probe, not from one head-to-head run alone).
        let win_rate_passes = promotion_gate_win_rate_passes_v1(wins, total);
        println!(
            "H2H candidate_gen={candidate_gen} win_rate_sub_check game_wins={wins}/{total}={:.4} passes={win_rate_passes}",
            wins as f64 / total as f64
        );
    }

    /// per generation. Non-banked diagnostic; the store root arrives via
    /// PATHFINDING_STORE_ROOT and generations via PATHFINDING_EVAL_GENS
    /// (comma-separated).
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn pathfinding_saturation_eval_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let root_path = std::env::var("PATHFINDING_STORE_ROOT")
            .expect("PATHFINDING_STORE_ROOT must name the surviving store root");
        let generations: Vec<u64> = std::env::var("PATHFINDING_EVAL_GENS")
            .expect("PATHFINDING_EVAL_GENS must list generations")
            .split(',')
            .map(|token| token.trim().parse().expect("generation index"))
            .collect();

        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            8_192,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("pathfinding run record");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 256,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };

        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_native_checkpoint_v1(
            &run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config,
        )
        .unwrap();
        for generation in generations {
            let boundary = load_native_training_boundary_v2(&root, &run, generation).unwrap();
            let candidate_run = run_native_checkpoint_v1(
                &run,
                boundary.checkpoint(),
                boundary.payload(),
                runner_config,
            )
            .unwrap();
            let evaluation =
                evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
                    .unwrap();
            let outcomes = evaluation.candidate_learner_outcomes();
            println!(
                "SATURATION gen={generation} W/L/D {}/{}/{} of {} (delta vs gen0 {})",
                outcomes.wins(),
                outcomes.losses(),
                outcomes.draws(),
                outcomes.total(),
                evaluation.total_candidate_minus_reference_reward_delta()
            );
        }
    }

    /// Overnight non-banked pathfinding run: K=64 vs uniform on the
    /// CudaBurnDense backend in record-only measurement mode to thousands of
    /// updates. Purpose: uniform-opponent saturation depth, durable-store
    /// behavior at real depth, and drift trajectory far past the measured
    /// 256-update horizon. Wall-clock and throughput from this probe are
    /// explicitly non-evidence (it runs concurrent with other lanes); the
    /// per-update QUALIFICATION_JSONL drift series and the store artifacts
    /// are its products. Direct the store under a roomy drive via TMP.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn pathfinding_run_k64_deep_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let _measurement_mode = MeasurementModeGuardV1::arm();

        let updates = 8_192_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("pathfinding run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;

        let parent = TestParentV1::new("pathfinding-deep");
        println!("pathfinding store parent: {}", parent.parent.display());
        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        let root = bootstrapped.into_root();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);

        let started = std::time::Instant::now();
        let mut committed_segments = 0_u64;
        loop {
            match resume_native_training_store_v2(&root, &run, execution_config.clone()) {
                Ok(NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                }) => {
                    println!(
                        "pathfinding: COMPLETE at generation {latest_generation_index} \
                         after {:.0}s",
                        started.elapsed().as_secs_f64()
                    );
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut continuation)) => {
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .expect("pathfinding prepare");
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    committed_segments += 1;
                    if committed_segments.is_multiple_of(64) {
                        println!(
                            "pathfinding: {} updates committed, {:.0}s elapsed, \
                             {:.2} eps/s cumulative (non-evidence)",
                            committed_segments * 4,
                            started.elapsed().as_secs_f64(),
                            (committed_segments * 4 * 64) as f64 / started.elapsed().as_secs_f64()
                        );
                    }
                }
                Err(error) => panic!("pathfinding resume: {error:?}"),
            }
        }
    }

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn qualification_measurement_k64_depth256_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let _measurement_mode = MeasurementModeGuardV1::arm();

        let updates = 256_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("measurement run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;

        let parent = TestParentV1::new("qualification-depth256");
        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        let root = bootstrapped.into_root();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);

        loop {
            match resume_native_training_store_v2(&root, &run, execution_config.clone()) {
                Ok(NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                }) => {
                    println!("measurement: COMPLETE at generation {latest_generation_index}");
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut continuation)) => {
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .expect("measurement prepare");
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                }
                Err(error) => panic!("measurement resume: {error:?}"),
            }
        }
    }

    /// Diagnostic twin of the learning smoke: same K=64 x 128-update run
    /// driven through the manual bootstrap/genesis/resume loop so the
    /// underlying trainer error is printed with full detail instead of the
    /// loop's redacted phase classification. Reports the failing update
    /// window by counting successful segment commits.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn learning_smoke_k64_failure_diagnostic_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let updates = 128_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("diagnostic run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;

        let parent = TestParentV1::new("learning-smoke-diag");
        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        let root = bootstrapped.into_root();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);

        let mut committed_segments = 0_u64;
        loop {
            match resume_native_training_store_v2(&root, &run, execution_config.clone()) {
                Ok(NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                }) => {
                    println!(
                        "diagnostic: COMPLETE at generation {latest_generation_index} \
                         ({committed_segments} segments committed this process)"
                    );
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut continuation)) => {
                    let prepared = match prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    ) {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            panic!(
                                "diagnostic: prepare FAILED after {committed_segments} committed \
                                 segments (updates {}..{}): {error:?}",
                                committed_segments * 4,
                                committed_segments * 4 + 4,
                            );
                        }
                    };
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    committed_segments += 1;
                    if committed_segments.is_multiple_of(8) {
                        println!(
                            "diagnostic: {committed_segments} segments \
                             ({} updates) committed",
                            committed_segments * 4
                        );
                    }
                }
                Err(error) => {
                    panic!(
                        "diagnostic: resume FAILED after {committed_segments} committed \
                         segments: {error:?}"
                    );
                }
            }
        }
    }

    /// Temporary GPU K-scaling measurement probe: end-to-end durable training
    /// throughput at K = 2/16/64/256 with the CudaBurnDense train-step backend
    /// and the same scaled topology grid as the CPU probe, one segment
    /// (S=4, N=4) per configuration, cold-start inclusive.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn timing_probe_gpu_k_scaling_throughput() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_v2, ValidatedTrainRunV2,
        };

        fn cuda_execution_config_v2(
            run: &ValidatedTrainRunV2,
        ) -> crate::native_training_executor_v1::NativeTrainingExecutionConfigV1 {
            let mut config = test_execution_config_v2(run);
            config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;
            config
        }

        let configurations: [(u64, u64, u64, u64, u64, u64); 4] = [
            // (K, workers, sessions, broker, max_physical, max_policy)
            (2, 0, 0, 0, 32_768, 65_536),
            (16, 4, 4, 8, 2_048, 4_096),
            (64, 8, 8, 32, 1_024, 2_048),
            (256, 16, 16, 128, 2_048, 4_096),
        ];
        let updates = 4_u64;

        for (batch_episodes, workers, sessions, broker, max_physical, max_policy) in configurations
        {
            let patched = if workers == 0 {
                test_fixture_bytes_with_schedule_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    batch_episodes,
                    4,
                    updates,
                    1,
                    2,
                    2,
                    max_physical,
                    max_policy,
                )
            } else {
                test_fixture_bytes_with_schedule_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    batch_episodes,
                    4,
                    updates,
                    workers,
                    sessions,
                    broker,
                    max_physical,
                    max_policy,
                )
            };
            let run = match decode_train_run_v2(&patched) {
                Ok(run) => run,
                Err(error) => {
                    panic!("K={batch_episodes}: run record rejected: {error}");
                }
            };
            let episodes = batch_episodes * updates;
            let target = run.requested_successful_updates();

            let parent = TestParentV1::new("gpu-kscale");
            let started = std::time::Instant::now();
            let bootstrapped =
                crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                    &parent.parent,
                    "store",
                )
                .unwrap();
            let root = bootstrapped.into_root();
            let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
            let executor = crate::native_training_executor_v1::NativeTrainingExecutorV1::
                from_common_model_snapshot_v1(
                    cuda_execution_config_v2(&run),
                    &snapshot_manifest,
                    &snapshot_payload,
                )
                .unwrap();
            let candidate = executor.checkpoint_candidate_v1().unwrap();
            let payload = candidate.payload().to_vec();
            let checkpoint =
                crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3(
                    &run, &payload,
                )
                .unwrap();
            let segment = crate::native_training_store_segment_manifest_v2::
                build_genesis_segment_manifest_v2(&run, &checkpoint)
            .unwrap();
            let boundary = crate::native_training_store_boundary_v2::
                build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint)
            .unwrap();
            let reference =
                crate::native_training_store_reference_latest_v2::build_checkpoint_reference_v2(
                    &run, &boundary,
                )
                .unwrap();
            let latest = crate::native_training_store_reference_latest_v2::build_latest_v2(
                &boundary, &reference,
            )
            .unwrap();
            let _ = crate::native_training_store_v2::publish_genesis_generation_v2(
                &root,
                &run,
                &payload,
                &checkpoint,
                &segment,
                &boundary,
                &reference,
                &latest,
            )
            .unwrap();
            let genesis_done = started.elapsed().as_secs_f64();

            let mut train_result = Ok(());
            loop {
                match crate::native_training_store_resume_v2::resume_native_training_store_v2(
                    &root,
                    &run,
                    cuda_execution_config_v2(&run),
                ) {
                    Ok(
                        crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2::Complete {
                            latest_generation_index,
                        },
                    ) => {
                        assert_eq!(latest_generation_index, target);
                        break;
                    }
                    Ok(
                        crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2::Continue(
                            mut continuation,
                        ),
                    ) => {
                        let prepared = match crate::native_training_store_prepared_segment_v2::
                            prepare_segment_v2(
                                &mut continuation.executor,
                                &run,
                                &continuation.parent_boundary,
                                &continuation.parent_checkpoint,
                            ) {
                            Ok(prepared) => prepared,
                            Err(error) => {
                                train_result = Err(format!("prepare: {error}"));
                                break;
                            }
                        };
                        let receipt = crate::native_training_store_v2::
                            publish_prepared_segment_v2(
                                &root,
                                &run,
                                &continuation.parent_boundary,
                                &continuation.parent_checkpoint,
                                &prepared,
                            )
                            .unwrap();
                        prepared.commit_v2(receipt).unwrap();
                    }
                    Err(error) => {
                        train_result = Err(format!("resume: {error}"));
                        break;
                    }
                }
            }
            let wall = started.elapsed().as_secs_f64();
            match train_result {
                Ok(()) => {
                    let rate = episodes as f64 / wall;
                    println!(
                        "K={batch_episodes}: {episodes} episodes over {wall:.3}s \
                         (genesis {genesis_done:.3}s) = {rate:.4} eps/s \
                         [vs floor 0.2925: {:.1}x]",
                        rate / 0.2925
                    );
                }
                Err(message) => {
                    panic!("K={batch_episodes}: training failed after {wall:.3}s: {message}");
                }
            }
        }
    }

    /// A crash between run-authority and latest publication leaves run.json
    /// present with latest.json never written (bootstrap B8). The loop must
    /// recover by retrying genesis and then train to the exact target, per
    /// the frozen draft's train-new recovery clause. The state is
    /// constructed literally: bootstrap the empty skeleton, then write the
    /// run's exact canonical bytes as run.json, which is byte-identical to
    /// what the interrupted publisher leaves behind.
    #[test]
    fn science_loop_recovers_interrupted_genesis() {
        let parent = TestParentV1::new("genesis-recovery");
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let target = run.requested_successful_updates();

        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        assert!(matches!(
            bootstrapped.outcome(),
            crate::native_training_store_bootstrap_v2::NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        ));
        let root = bootstrapped.into_root();
        fs::write(root.root_path().join("run.json"), run.canonical_bytes()).unwrap();
        drop(root);

        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .unwrap();
        assert_eq!(report.latest_generation_index(), target);
        assert_eq!(report.candidate_run().generation_index(), target);
    }

    #[test]
    fn genesis_publisher_mapping_preserves_global_busy_and_platform_errors() {
        assert_eq!(
            map_genesis_publisher_error_kind_v1(
                NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform
            )
            .kind(),
            NativeScienceLoopV1ErrorKind::UnsupportedPlatform
        );
        assert_eq!(
            map_genesis_publisher_error_kind_v1(NativeTrainingStorePublisherV2ErrorKind::StoreBusy)
                .kind(),
            NativeScienceLoopV1ErrorKind::StoreBusy
        );
        for kind in [
            NativeTrainingStorePublisherV2ErrorKind::RootInvalid,
            NativeTrainingStorePublisherV2ErrorKind::InputInvalid,
            NativeTrainingStorePublisherV2ErrorKind::StageCorruption,
            NativeTrainingStorePublisherV2ErrorKind::PublicationFailed,
            NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid,
            NativeTrainingStorePublisherV2ErrorKind::LatestInvalid,
        ] {
            assert_eq!(
                map_genesis_publisher_error_kind_v1(kind).kind(),
                NativeScienceLoopV1ErrorKind::GenesisFailed
            );
        }
    }

    #[test]
    fn one_command_recovers_exact_run_only_and_candidate_equal_partial_genesis() {
        let parent = TestParentV1::new("run-only-recovery");
        let run = one_segment_run_v1();
        let root_path = establish_exact_run_only_v1(&parent, &run);
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();

        // Reproduce a real interrupted publisher state: exact run authority,
        // the first candidate-equal generation-zero immutable, a recognized
        // latest stage, and no latest final.
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let partial_payload = executor
            .checkpoint_candidate_v1()
            .unwrap()
            .payload()
            .to_vec();
        let partial_payload_path = root_path
            .join("checkpoints")
            .join("update-00000000.state.f32le");
        fs::write(&partial_payload_path, &partial_payload).unwrap();
        let latest_stage_path = root_path.join(".latest.json.stage-v2");
        fs::write(&latest_stage_path, b"interrupted-latest-stage").unwrap();

        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .unwrap();
        assert_eq!(report.latest_generation_index(), 4);
        assert_eq!(report.reference_run().generation_index(), 0);
        assert_eq!(report.candidate_run().generation_index(), 4);
        assert_eq!(fs::read(&partial_payload_path).unwrap(), partial_payload);
        assert!(fs::symlink_metadata(&latest_stage_path).is_err());

        let reopened = bootstrap_native_training_store_v2(&parent.parent, "store").unwrap();
        assert_eq!(
            reopened.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::RunAuthorityPresent
        );
        assert!(reopened.latest_final_present());
        let state = validate_native_training_store_v2(reopened.root(), &run).unwrap();
        assert_eq!(state.latest_generation_index(), 4);
    }

    #[test]
    fn run_only_mismatching_run_fails_as_genesis_without_mutation() {
        let parent = TestParentV1::new("run-only-mismatching-run");
        let run = one_segment_run_v1();
        let root_path = establish_exact_run_only_v1(&parent, &run);
        let run_path = root_path.join("run.json");
        let mut mismatching_run = run.canonical_bytes().to_vec();
        let flip = mismatching_run.len() / 2;
        mismatching_run[flip] ^= 0x01;
        fs::write(&run_path, &mismatching_run).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();

        let error = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeScienceLoopV1ErrorKind::GenesisFailed);
        assert_eq!(fs::read(run_path).unwrap(), mismatching_run);
        assert!(fs::symlink_metadata(root_path.join("latest.json")).is_err());
        assert_generation_directories_empty_v1(&root_path);
    }

    #[test]
    fn present_but_invalid_latest_is_not_misclassified_as_recoverable_genesis() {
        let parent = TestParentV1::new("invalid-latest");
        let run = one_segment_run_v1();
        let root_path = establish_exact_run_only_v1(&parent, &run);
        let latest_path = root_path.join("latest.json");
        fs::write(&latest_path, b"{}").unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();

        let error = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeScienceLoopV1ErrorKind::TrainFailed);
        assert_eq!(fs::read(latest_path).unwrap(), b"{}");
        assert_generation_directories_empty_v1(&root_path);
    }

    #[test]
    fn one_command_science_loop_trains_runs_evaluates_and_reruns_deterministically() {
        let parent = TestParentV1::new("smoke");
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let target = run.requested_successful_updates();

        let first = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .unwrap();
        assert_eq!(first.latest_generation_index(), target);
        assert_eq!(first.reference_run().generation_index(), 0);
        assert_eq!(first.candidate_run().generation_index(), target);
        let evaluation = first.evaluation();
        assert_eq!(evaluation.reference_generation_index(), 0);
        assert_eq!(evaluation.candidate_generation_index(), target);
        assert_eq!(evaluation.pair_count(), 1);
        assert_eq!(evaluation.leg_count(), 2);
        let reference_outcomes = evaluation.reference_learner_outcomes();
        let candidate_outcomes = evaluation.candidate_learner_outcomes();
        assert_eq!(
            reference_outcomes.wins() + reference_outcomes.losses() + reference_outcomes.draws(),
            reference_outcomes.total()
        );
        assert_eq!(
            candidate_outcomes.wins() + candidate_outcomes.losses() + candidate_outcomes.draws(),
            candidate_outcomes.total()
        );

        // The second invocation resumes the completed store as the exact
        // no-op and must reproduce the identical evaluation: same pairs, same
        // rewards, same delta. This is the deterministic science guarantee.
        let second = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .unwrap();
        assert_eq!(second.latest_generation_index(), target);
        assert_eq!(
            second
                .evaluation()
                .total_candidate_minus_reference_reward_delta(),
            evaluation.total_candidate_minus_reference_reward_delta()
        );
        assert_eq!(
            second.evaluation().reward_pairs().len(),
            evaluation.reward_pairs().len()
        );
        for (second_pair, first_pair) in second
            .evaluation()
            .reward_pairs()
            .iter()
            .zip(evaluation.reward_pairs())
        {
            assert_eq!(second_pair.pair_index(), first_pair.pair_index());
            assert_eq!(
                second_pair.environment_seed(),
                first_pair.environment_seed()
            );
            assert_eq!(
                second_pair.reference_rewards_by_learner_seat(),
                first_pair.reference_rewards_by_learner_seat()
            );
            assert_eq!(
                second_pair.candidate_rewards_by_learner_seat(),
                first_pair.candidate_rewards_by_learner_seat()
            );
        }
    }

    /// GEN-0 AS SELECTION CANDIDATE (Self-Play Ladder Design Contract S2,
    /// Amendment 1 / Section 8A point 2, Deliverable 4): under continual
    /// initialization, generation 0 -- the inherited checkpoint itself --
    /// joins the evaluated selection schedule as a genuine candidate, not
    /// merely the delta reference, so best-panel selection structurally
    /// cannot return worse than the initialization. This proves the
    /// mechanism `ladder_saturation_eval_v1`'s LADDER_EVAL_GENS loop relies
    /// on when `0` is one of the requested generations: loading generation 0
    /// as BOTH the reference and the candidate runs it through the exact
    /// same `run_native_checkpoint_v1` / `evaluate_native_checkpoint_uniform_delta_v1`
    /// path with no special-casing, trips no assert, and produces an exact
    /// zero delta by construction. Exercised against a real (trained)
    /// LADDER-identity store -- the ladder fixture builder, not the uniform
    /// one -- since the probe itself takes its stores from env vars this
    /// test cannot depend on. Uses the real ladder pilot pool.json
    /// (read-only) for a structurally valid pool section; the opponent
    /// engine passed to the trainer is `None` (uniform fallback), since
    /// this test's subject is checkpoint SELECTION machinery, not the
    /// opponent engine.
    #[test]
    fn ladder_gen_zero_as_both_reference_and_candidate_runs_cleanly() {
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2, OpponentLadderPoolContractV1,
        };

        let pool_bytes = fs::read(r"D:\mtg-kernel-ladder-pilot-20260725\pool\pool.json")
            .expect("real ladder pilot pool.json must be readable");
        let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
            .expect("pool.json must decode as OpponentLadderPoolContractV1");

        let patched = test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            4,
            2,
            4,
            8,
            32_768,
            65_536,
            555_002,
            pool,
        );
        let run = decode_train_run_v2(&patched).expect("ladder run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let parent = TestParentV1::new("ladder-gen-zero-candidate");

        run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
        )
        .expect("ladder science loop");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&parent.parent.join("store"))
            .expect("validated store root");

        // LADDER_EVAL_GENS's own loop body: gen 0 as the reference AND, here,
        // gen 0 again in place of a later `generation` drawn from the
        // requested list.
        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_native_checkpoint_v1(
            &run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config_v1(),
        )
        .unwrap();
        let candidate_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let candidate_run = run_native_checkpoint_v1(
            &run,
            candidate_boundary.checkpoint(),
            candidate_boundary.payload(),
            runner_config_v1(),
        )
        .unwrap();
        let evaluation =
            evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run).unwrap();
        assert_eq!(evaluation.reference_generation_index(), 0);
        assert_eq!(evaluation.candidate_generation_index(), 0);
        assert_eq!(evaluation.total_candidate_minus_reference_reward_delta(), 0);
        assert_eq!(
            evaluation.reference_learner_outcomes(),
            evaluation.candidate_learner_outcomes()
        );
    }
}
