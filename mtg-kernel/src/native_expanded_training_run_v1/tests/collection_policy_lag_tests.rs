//! `collection_policy_lag_v1` (docs/design/collection_policy_lag_v1.md): the
//! wire and scope of the flag, which state each batch is played on from a
//! start, a fork and a segment boundary, run-to-run and worker-count identity,
//! a stop at a declared update, and a crash at any publication.

use super::*;
use crate::durable_publication_v1::crash_points_for_test_v1::{
    with_crash_at_path_v1, with_crash_point_v1, CrashPointV1,
};
use crate::native_expanded_training_run_v1::follow_v1::check_continuation_v1;

/// Documents that carry timing telemetry, or the digest of one.
const TIMED: [&str; 5] = [
    "collection.json",
    "update.json",
    "update-input.json",
    "complete.json",
    "completion.json",
];

/// `parallel_two_iteration_config_v1` (two games an iteration, each against
/// the current policy, parallel collection and preparation, the CPU GAE loss)
/// with `iterations` iterations, under `collection_policy_lag_v1`.
fn lag_config_v1(label: &str, iterations: usize) -> NativeExpandedTrainingRunV1 {
    let mut config = parallel_two_iteration_config_v1(label);
    while config.iterations.len() < iterations {
        let index = config.iterations.len();
        let mut next = config.iterations[1].clone();
        for (slot, scheduled) in next.episodes.iter_mut().enumerate() {
            scheduled.episode.id = format!("{label}-iter-{index}-slot-{slot}");
            scheduled.episode.seed += 100 * index as u64;
        }
        config.iterations.push(next);
    }
    config.iterations.truncate(iterations);
    config.collection_policy_lag_v1 = 1;
    config
}

/// A fresh output directory beside the config's own, named `name`.
fn fresh_directory_v1(config: &NativeExpandedTrainingRunV1, name: &str) -> PathBuf {
    let directory = config.output_directory.with_file_name(name);
    let _ = fs::remove_dir_all(&directory);
    directory
}

/// Iteration `index`'s receipt, collection and update documents.
fn documents_v1(result: &Value, index: usize) -> (Value, Value, Value) {
    let receipt = iteration_receipt_v1(result, index);
    let collection = read_pin(&parse_pin(&receipt, "collection").unwrap()).unwrap();
    let update = read_pin(&parse_pin(&receipt, "update").unwrap()).unwrap();
    (receipt, collection, update)
}

/// The source iteration `index` started from.
fn source_v1(result: &Value, index: usize) -> ExpandedModelSourceV1 {
    serde_json::from_value(iteration_receipt_v1(result, index)["source"].clone()).unwrap()
}

/// Every iteration's after-state digest.
fn after_states_v1(result: &Value) -> Vec<Value> {
    (0..result["completed_iterations"].as_u64().unwrap() as usize)
        .map(|index| iteration_update_v1(result, index)["after_state_sha256"].clone())
        .collect()
}

/// The flag's wire: absent and 0 are today's recipe and share bytes (never
/// serialized), 1 and a carry are serialized, and the config is refused
/// outside the flag's scope before any work.
#[test]
fn collection_policy_lag_keeps_today_wire_and_validates_its_scope_v1() {
    let today = schedule();
    let today_value = value(&today).unwrap();
    assert!(today_value.get("collection_policy_lag_v1").is_none());
    assert!(today_value.get("collection_policy_lag_carry_v1").is_none());
    let mut explicit_zero = today_value.clone();
    explicit_zero["collection_policy_lag_v1"] = json!(0);
    let parsed: NativeExpandedTrainingRunV1 = serde_json::from_value(explicit_zero).unwrap();
    assert_eq!(parsed.collection_policy_lag_v1, 0);
    assert_eq!(
        serde_json::to_vec(&parsed).unwrap(),
        serde_json::to_vec(&today).unwrap()
    );
    assert_eq!(identity(&parsed).unwrap(), identity(&today).unwrap());
    // A run.json and result of today's recipe name no lag.
    let mut manifest = json!({});
    record_collection_execution(&today, &mut manifest);
    assert!(manifest.get("collection_policy_lag_v1").is_none());

    let mut lag = real_two_iteration_config_v1(
        "lag-wire",
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.9,
            entropy_coefficient: 0.0,
            pregame_advantage: crate::expanded_deck_training_v1::PregameAdvantageV1::default(),
            pregame_announcement_step_scale: None,
        },
        ExpandedUpdateBackendV1::Cpu,
    );
    lag.collection_policy_lag_v1 = 1;
    lag.validate_v1().unwrap();
    let lag_value = value(&lag).unwrap();
    assert_eq!(lag_value["collection_policy_lag_v1"], 1);
    assert!(lag_value.get("collection_policy_lag_carry_v1").is_none());
    let mut manifest = json!({});
    record_collection_execution(&lag, &mut manifest);
    assert_eq!(manifest["collection_policy_lag_v1"], 1);
    let refused = |config: NativeExpandedTrainingRunV1, expected: &str| {
        let error = config.validate_v1().unwrap_err();
        assert!(error.contains(expected), "{error}");
    };
    let mut two = lag.clone();
    two.collection_policy_lag_v1 = 2;
    refused(two, "collection_policy_lag_v1 must be 0 or 1");
    let mut terminal = lag.clone();
    terminal.loss_selection = ExpandedLossSelectionV1::TerminalReinforceValueV3;
    refused(
        terminal,
        "collection_policy_lag_v1 requires the plain CPU GAE update",
    );
    let mut cuda = lag.clone();
    cuda.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 0 };
    refused(
        cuda,
        "collection_policy_lag_v1 requires the plain CPU GAE update",
    );
    let mut ex1 = lag.clone();
    ex1.preparation_workers = 1;
    ex1.ex1 = Some(Ex1RunOptionsV1 {
        run: "lag-wire".into(),
        update_offset: 0,
        coefficient: 0.5,
        cap: Default::default(),
        w5: None,
    });
    refused(
        ex1,
        "collection_policy_lag_v1 requires the plain CPU GAE update",
    );
    // A completed-iteration opponent must be two iterations old: batch 1 is
    // collected while update 0 runs.
    let mut recent = lag.clone();
    recent.iterations[1].episodes[0].opponent =
        ExpandedOpponentAssignmentV1::CompletedIteration { index: 0 };
    refused(recent.clone(), "at least two iterations old");
    recent.collection_policy_lag_v1 = 0;
    recent.validate_v1().unwrap();
    // A carry needs the flag and an initial checkpoint.
    let mut carried = lag.clone();
    carried.collection_policy_lag_carry_v1 = Some(lag.initial_source.clone());
    refused(
        carried.clone(),
        "collection_policy_lag_carry_v1 requires an initial checkpoint",
    );
    carried.collection_policy_lag_v1 = 0;
    refused(
        carried,
        "collection_policy_lag_carry_v1 requires collection_policy_lag_v1",
    );
}

/// From a start (DESIGN 1): batches 0 and 1 are played on the initial source
/// and batch k + 1 on update k - 1's output, as every collection, trajectory,
/// update, receipt and checkpoint records; the result names the source a
/// continuation's first batch is played on; the first update is today's
/// update bit for bit, and the lagged batches make the run differ from
/// today's. An update refuses a behaviour other than the input its source
/// checkpoint records, and a batch played on another state.
#[test]
fn a_lag_run_plays_each_batch_on_the_input_of_the_update_before_v1() {
    let mut config = lag_config_v1("lag-sources", 3);
    config.output_directory = fresh_directory_v1(&config, "run-lag");
    config.validate_v1().unwrap();
    let result = run_native_expanded_training_v1(&config, None).unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(result["collection_policy_lag_v1"], 1);
    assert_eq!(result["actual_identity"]["adam_step"], 3);
    let (_, initial) =
        load_expanded_inference_bound_v1(&config.initial_source, config.training_math).unwrap();
    // θ_j: the state iteration j started from.
    let sources: Vec<ExpandedModelSourceV1> =
        (0..3).map(|index| source_v1(&result, index)).collect();
    let states: Vec<Value> = (0..3)
        .map(|index| iteration_update_v1(&result, index)["before_state_sha256"].clone())
        .collect();
    assert_eq!(sources[0], config.initial_source);
    assert_eq!(states[0], initial.state_sha256);
    // Batch j's behaviour: θ_0 for j = 0 and 1 (no lag on the first batch),
    // θ_{j-1} after.
    for (index, behaviour) in [(0usize, 0usize), (1, 0), (2, 1)] {
        let (receipt, collection, update) = documents_v1(&result, index);
        let case = format!("batch {index}");
        assert_eq!(
            collection["source"],
            value(&sources[behaviour]).unwrap(),
            "{case}"
        );
        assert_eq!(
            collection["behavior_state_sha256"], states[behaviour],
            "{case}"
        );
        let trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collection["trajectories"].clone()).unwrap();
        assert_eq!(trajectories.len(), 2, "{case}");
        for trajectory in &trajectories {
            let document = read_pin_bounded(trajectory, ARTIFACT_CAP).unwrap();
            assert_eq!(
                document["behavior_state_sha256"], states[behaviour],
                "{case}"
            );
        }
        let expected = json!({"lag": 1, "behaviour": sources[behaviour],
            "behaviour_state_sha256": states[behaviour]});
        assert_eq!(update["collection_policy_lag_v1"], expected, "{case}");
        assert_eq!(receipt["collection_policy_lag_v1"], expected, "{case}");
        assert_eq!(update["source"], value(&sources[index]).unwrap(), "{case}");
        // The checkpoint names its update's input and this batch's behaviour.
        let record = checkpoint_collection_policy_lag_v1(&iteration_checkpoint_v1(&result, index))
            .unwrap()
            .unwrap();
        assert_eq!(record.lag, 1, "{case}");
        assert_eq!(record.input, sources[index], "{case}");
        assert_eq!(json!(record.input_state_sha256), states[index], "{case}");
        assert_eq!(record.behaviour, sources[behaviour], "{case}");
        assert_eq!(
            json!(record.behaviour_state_sha256),
            states[behaviour],
            "{case}"
        );
    }
    // Batch 2 was played on update 0's output checkpoint.
    let (_, collection, _) = documents_v1(&result, 2);
    assert_eq!(
        collection["source"]["checkpoint"],
        json!(iteration_checkpoint_v1(&result, 0))
    );
    assert_eq!(
        collection["behavior_state_sha256"],
        iteration_update_v1(&result, 0)["after_state_sha256"]
    );
    // A continuation plays its first batch on the last update's input.
    assert_eq!(
        result["collection_policy_lag_next_behaviour_v1"],
        value(&sources[2]).unwrap()
    );

    // Today's recipe from the same start: the same first update, then other
    // batches (batch 1 on update 0's output), so another final state.
    let mut today = config.clone();
    today.collection_policy_lag_v1 = 0;
    today.output_directory = fresh_directory_v1(&config, "run-today");
    let today = run_native_expanded_training_v1(&today, None).unwrap();
    assert!(today.get("collection_policy_lag_v1").is_none());
    assert!(today
        .get("collection_policy_lag_next_behaviour_v1")
        .is_none());
    let (today_receipt, today_batch_1, today_update) = documents_v1(&today, 1);
    assert!(today_receipt.get("collection_policy_lag_v1").is_none());
    assert!(today_update.get("collection_policy_lag_v1").is_none());
    assert!(
        checkpoint_collection_policy_lag_v1(&iteration_checkpoint_v1(&today, 1))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        today_batch_1["behavior_state_sha256"],
        iteration_update_v1(&today, 0)["after_state_sha256"]
    );
    assert_eq!(after_states_v1(&today)[0], after_states_v1(&result)[0]);
    assert_ne!(
        today["actual_identity"]["state_sha256"],
        result["actual_identity"]["state_sha256"]
    );

    // An update of θ_2 refuses a behaviour that is not the input its
    // checkpoint records (θ_0 instead of θ_1), and a batch played on another
    // state than the behaviour (batch 1, played on θ_0, with θ_1).
    let update_of = |batch: usize, behaviour: &ExpandedModelSourceV1, name: &str| {
        let (_, collection, _) = documents_v1(&result, batch);
        execute_v1(ExpandedTrainingCommandV1::UpdatePrepared {
            line_b: None,
            source: sources[2].clone(),
            trajectories: serde_json::from_value(collection["trajectories"].clone()).unwrap(),
            learning_rate: config.learning_rate,
            value_coefficient: config.value_coefficient,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution: UpdateBackwardExecutionV1::Sequential,
            loss_selection: config.loss_selection,
            preparation_workers: 2,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            max_trajectory_batch_mebibytes: DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
            training_math: config.training_math,
            record_format: Default::default(),
            record_envelope: Default::default(),
            collection_policy_lag_v1: Some(CollectionPolicyLagUpdateV1 {
                behaviour: behaviour.clone(),
            }),
            output_directory: fresh_directory_v1(&config, name),
        })
        .unwrap_err()
    };
    let error = update_of(2, &sources[0], "update-wrong-behaviour");
    assert!(
        error.contains("is not the input of the update that wrote the source checkpoint"),
        "{error}"
    );
    let error = update_of(1, &sources[1], "update-stale-batch");
    assert!(error.contains("stale trajectory"), "{error}");
}

/// Across runs (DESIGN 1, 3): two segments, the second carrying the lag,
/// train exactly the unsegmented chain with every batch on the same state; a
/// followed continuation must carry the previous run's next behaviour; a
/// carry that is not the recorded update input is refused before any work;
/// and a fork (no carry) plays its first two batches on the fork state, its
/// first update being today's update from that state.
#[test]
fn a_lag_chain_carries_across_segments_and_a_fork_starts_unlagged_v1() {
    let mut whole = lag_config_v1("lag-chain", 4);
    whole.output_directory = fresh_directory_v1(&whole, "run-whole");
    whole.validate_v1().unwrap();
    let whole_result = run_native_expanded_training_v1(&whole, None).unwrap();
    assert_eq!(whole_result["complete"], true);

    let mut first = whole.clone();
    first.iterations.truncate(2);
    first.output_directory = fresh_directory_v1(&whole, "seg-00000");
    let first_result = run_native_expanded_training_v1(&first, None).unwrap();
    let last_checkpoint: PinnedFileV1 =
        serde_json::from_value(first_result["source"]["checkpoint"].clone()).unwrap();
    let carry: ExpandedModelSourceV1 =
        serde_json::from_value(first_result["collection_policy_lag_next_behaviour_v1"].clone())
            .unwrap();
    assert_eq!(carry, source_v1(&first_result, 1));
    let mut second = whole.clone();
    second.iterations = whole.iterations[2..].to_vec();
    second.output_directory = fresh_directory_v1(&whole, "seg-00001");
    second.initial_source.checkpoint = Some(last_checkpoint);
    second.collection_policy_lag_carry_v1 = Some(carry);
    second.validate_v1().unwrap();
    check_continuation_v1(&first, &first_result, &second).unwrap();
    let second_result = run_native_expanded_training_v1(&second, None).unwrap();
    for field in ["state_sha256", "adam_step"] {
        assert_eq!(
            second_result["actual_identity"][field], whole_result["actual_identity"][field],
            "{field}"
        );
    }
    let mut segmented = after_states_v1(&first_result);
    segmented.extend(after_states_v1(&second_result));
    assert_eq!(segmented, after_states_v1(&whole_result));
    for (segment_index, whole_index) in [(0usize, 2usize), (1, 3)] {
        let (_, collection, _) = documents_v1(&second_result, segment_index);
        let (_, whole_collection, _) = documents_v1(&whole_result, whole_index);
        assert_eq!(
            collection["behavior_state_sha256"], whole_collection["behavior_state_sha256"],
            "global batch {whole_index}"
        );
    }

    // A followed continuation without the carry, or with another one, is
    // refused; so is a run whose carry is not the recorded update input.
    let mut fork = second.clone();
    fork.collection_policy_lag_carry_v1 = None;
    fork.output_directory = fresh_directory_v1(&whole, "fork");
    let error = check_continuation_v1(&first, &first_result, &fork).unwrap_err();
    assert!(error.contains("collection_policy_lag_carry_v1"), "{error}");
    let mut wrong = second.clone();
    wrong.collection_policy_lag_carry_v1 = Some(whole.initial_source.clone());
    wrong.output_directory = fresh_directory_v1(&whole, "wrong-carry");
    wrong.validate_v1().unwrap();
    let error = check_continuation_v1(&first, &first_result, &wrong).unwrap_err();
    assert!(error.contains("collection_policy_lag_carry_v1"), "{error}");
    let error = run_native_expanded_training_v1(&wrong, None).unwrap_err();
    assert!(
        error.contains("collection_policy_lag_carry_v1 is not the update input"),
        "{error}"
    );
    assert_eq!(
        fs::read_dir(wrong.output_directory.join("iterations"))
            .unwrap()
            .count(),
        0
    );

    // The fork: its first two batches on the fork state, which differs from
    // the input the fork checkpoint records.
    fork.validate_v1().unwrap();
    let fork_result = run_native_expanded_training_v1(&fork, None).unwrap();
    let fork_state = first_result["actual_identity"]["state_sha256"].clone();
    assert_ne!(
        fork_state,
        iteration_update_v1(&first_result, 1)["before_state_sha256"]
    );
    for index in 0..2 {
        let (_, collection, update) = documents_v1(&fork_result, index);
        assert_eq!(collection["source"], value(&fork.initial_source).unwrap());
        assert_eq!(collection["behavior_state_sha256"], fork_state);
        assert_eq!(
            update["collection_policy_lag_v1"]["behaviour_state_sha256"],
            fork_state
        );
    }
    assert_ne!(
        fork_result["actual_identity"]["state_sha256"],
        whole_result["actual_identity"]["state_sha256"]
    );
    let mut today = fork.clone();
    today.collection_policy_lag_v1 = 0;
    today.output_directory = fresh_directory_v1(&whole, "fork-today");
    let today_result = run_native_expanded_training_v1(&today, None).unwrap();
    assert_eq!(
        after_states_v1(&today_result)[0],
        after_states_v1(&fork_result)[0]
    );
}

/// The same lag run trains the same states at one, two and twelve collection
/// and preparation workers, and run twice in one directory it publishes the
/// same bytes apart from timing telemetry.
#[test]
fn a_lag_run_is_identical_run_to_run_and_across_worker_counts_v1() {
    let base = lag_config_v1("lag-workers", 3);
    let mut reference: Option<Vec<Value>> = None;
    for workers in [1usize, 2, 12] {
        let mut config = base.clone();
        config.collection_workers = workers;
        config.preparation_workers = workers;
        config.output_directory = fresh_directory_v1(&base, &format!("run-{workers}"));
        config.validate_v1().unwrap();
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["complete"], true);
        let states = after_states_v1(&result);
        match &reference {
            None => reference = Some(states),
            Some(expected) => assert_eq!(&states, expected, "{workers} workers"),
        }
    }
    let mut config = base.clone();
    config.output_directory = fresh_directory_v1(&base, "run-again");
    let first_directory = fresh_directory_v1(&base, "run-again-first");
    let first = run_native_expanded_training_v1(&config, None).unwrap();
    fs::rename(&config.output_directory, &first_directory).unwrap();
    let second = run_native_expanded_training_v1(&config, None).unwrap();
    assert_eq!(first["actual_identity"], second["actual_identity"]);
    assert_eq!(Some(after_states_v1(&second)), reference);
    let before = files_under_v1(&first_directory);
    let after = files_under_v1(&config.output_directory);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in &after {
        let name = path.file_name().unwrap().to_str().unwrap();
        if name != "run.lock" && !TIMED.contains(&name) {
            assert!(
                *bytes == before[path],
                "{} differs run to run",
                path.display()
            );
        }
    }
}

/// A stop at a declared update (DESIGN 1): an iteration limit stops after the
/// update and starts no next collection; each relaunch resumes from committed
/// receipts, and the run reaches the uninterrupted run's states.
#[test]
fn a_lag_run_stopped_at_an_update_resumes_to_the_uninterrupted_result_v1() {
    let mut config = lag_config_v1("lag-stop", 3);
    config.output_directory = fresh_directory_v1(&config, "run-uninterrupted");
    let uninterrupted = run_native_expanded_training_v1(&config, None).unwrap();
    config.output_directory = fresh_directory_v1(&config, "run-stopped");
    let stopped = run_native_expanded_training_v1(&config, Some(1)).unwrap();
    assert_eq!(stopped["complete"], false);
    assert_eq!(stopped["completed_iterations"], 1);
    // The next batch (on θ_0) is named for a continuation, and was not
    // collected beside the last update.
    assert_eq!(
        stopped["collection_policy_lag_next_behaviour_v1"],
        value(&config.initial_source).unwrap()
    );
    assert!(!config.output_directory.join("iterations/000001").exists());
    let stopped = run_native_expanded_training_v1(&config, Some(1)).unwrap();
    assert_eq!(stopped["completed_iterations"], 2);
    assert!(!config.output_directory.join("iterations/000002").exists());
    let resumed = run_native_expanded_training_v1(&config, None).unwrap();
    assert_eq!(resumed["complete"], true);
    assert_eq!(after_states_v1(&resumed), after_states_v1(&uninterrupted));
    for field in ["state_sha256", "adam_step"] {
        assert_eq!(
            resumed["actual_identity"][field], uninterrupted["actual_identity"][field],
            "{field}"
        );
    }
}

/// The lag run's crash-state check: a crash just before or just after any
/// publication of a three-iteration lag run (each next batch publishing
/// beside its update) leaves only files the uninterrupted run publishes, with
/// the same bytes apart from timing telemetry, plus staging debris; and
/// resuming the crashed run reaches the uninterrupted run's final state.
/// Crash points are named by path, since the two threads interleave their
/// publications. Every case runs in the same output directory, so documents
/// that name it compare equal.
#[test]
#[ignore = "slow: one crashed run and its resume per crash point; run before shipping a lag change"]
fn a_lag_run_crashed_at_any_publication_resumes_to_the_uninterrupted_result_v1() {
    let config = lag_config_v1("lag-crash-points", 3);
    config.validate_v1().unwrap();
    let directory = config.output_directory.clone();
    let reference_directory = directory.with_file_name("uninterrupted");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference_directory);
    let (reference, publications, crashed) = with_crash_point_v1(&directory, None, || {
        run_native_expanded_training_v1(&config, None)
    });
    let reference = reference.unwrap();
    assert!(!crashed);
    assert_eq!(reference["complete"], true);
    assert_eq!(reference["actual_identity"]["adam_step"], 3);
    fs::rename(&directory, &reference_directory).unwrap();
    let reference_files = files_under_v1(&reference_directory);
    let published: Vec<PathBuf> = reference_files
        .keys()
        .filter(|path| path.file_name().unwrap() != "run.lock")
        .cloned()
        .collect();
    // Every file but the lock is a publication, so every one is a case.
    assert_eq!(published.len(), publications);
    for relative in &published {
        for point in [CrashPointV1::BeforeFinal, CrashPointV1::AfterFinal] {
            let case = format!("{} {point:?}", relative.display());
            let (crashed_run, _, crashed) =
                with_crash_at_path_v1(&directory, &directory.join(relative), point, || {
                    run_native_expanded_training_v1(&config, None)
                });
            assert!(crashed, "{case}: never reached");
            let error = crashed_run.unwrap_err();
            assert!(error.contains("injected crash"), "{case}: {error}");
            for (path, bytes) in files_under_v1(&directory) {
                let name = path.file_name().unwrap().to_str().unwrap();
                if name.starts_with('.') && name.contains(".stage") {
                    continue;
                }
                let expected = reference_files
                    .get(&path)
                    .unwrap_or_else(|| panic!("{case}: unexpected file {}", path.display()));
                if name != "run.lock" && !TIMED.contains(&name) {
                    assert!(
                        bytes == *expected,
                        "{case}: {} differs from the uninterrupted run",
                        path.display()
                    );
                }
            }
            let resumed = run_native_expanded_training_v1(&config, None)
                .unwrap_or_else(|error| panic!("{case}: resume failed: {error}"));
            assert_eq!(resumed["complete"], true, "{case}");
            for field in ["state_sha256", "adam_step"] {
                assert_eq!(
                    resumed["actual_identity"][field], reference["actual_identity"][field],
                    "{case}: {field}"
                );
            }
            fs::remove_dir_all(&directory).unwrap();
        }
    }
    fs::remove_dir_all(&reference_directory).unwrap();
}
