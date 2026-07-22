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
use crate::native_training_executor_v1::{
    NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
};
use crate::native_training_store_bootstrap_v2::{
    bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapOutcomeV2,
    NativeTrainingStoreBootstrapV2ErrorKind,
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
use crate::native_training_store_v2::publish_genesis_generation_v2;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

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
/// genesis generation when the skeleton is fresh, train to the run's target
/// entirely through resume-reconstructed executors, validate the complete
/// Store, then run and evaluate the update-zero and latest boundaries.
pub fn run_native_science_loop_v1(
    parent: impl AsRef<Path>,
    root_basename: &str,
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
    runner_config: NativeCheckpointRunnerConfigV1,
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
    let fresh_skeleton = matches!(
        bootstrapped.outcome(),
        NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
    );
    let root: ValidatedNativeTrainingStoreRootV2 = bootstrapped.into_root();

    // Fresh bootstrap publishes run.json and the genesis generation from the
    // pristine snapshot-built executor; the receipt witnesses publication of
    // exactly generation zero.
    if fresh_skeleton {
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
        .map_err(|_| genesis_error)?;
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
    use crate::native_training_store_resume_v2::test_execution_config_v2;
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
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
                    println!("K={batch_episodes}: run record rejected: {error}");
                    continue;
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
                    println!("K={batch_episodes}: training failed after {wall:.3}s: {message}");
                }
            }
        }
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
}
