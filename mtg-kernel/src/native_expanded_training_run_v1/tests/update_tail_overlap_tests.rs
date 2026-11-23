//! Update tail overlap (`update_tail_overlap_v1`): the key's wire form, and
//! that a run with it publishes the bytes, makes the commits, fails and stops
//! as the same run without it does.
//!
//! Every run of a comparison uses the same output directory (the reference
//! tree is moved aside), so documents that name it compare equal.

use super::follow_tests::{fresh_run_v1, without_telemetry_v1, TIMED};
use super::*;
use crate::durable_publication_v1::crash_points_for_test_v1::{with_crash_point_v1, CrashPointV1};
use crate::native_expanded_training_run_v1::update_tail_overlap_v1::collected_for_test_v1;

/// `parallel_two_iteration_config_v1` (two games an iteration, collected and
/// prepared on two workers, every opponent the current model) with `updates`
/// iterations, two or more.
fn overlap_config_v1(label: &str, updates: usize) -> NativeExpandedTrainingRunV1 {
    let mut config = parallel_two_iteration_config_v1(label);
    while config.iterations.len() < updates {
        let index = config.iterations.len();
        let mut next = config.iterations[index - 1].clone();
        for (slot, scheduled) in next.episodes.iter_mut().enumerate() {
            scheduled.episode.id = format!("{label}-iter-{index}-slot-{slot}");
            scheduled.episode.seed += 17;
        }
        config.iterations.push(next);
    }
    config.validate_v1().unwrap();
    config
}

/// `config` with the key on.
fn overlapped_v1(config: &NativeExpandedTrainingRunV1) -> NativeExpandedTrainingRunV1 {
    let mut overlapped = config.clone();
    overlapped.update_tail_overlap_v1 = true;
    overlapped.validate_v1().unwrap();
    overlapped
}

/// Runs `config` on a new thread, which starts with empty thread-local caches
/// as a new process does, after `before` on that thread. The result, and how
/// many of the run's collections its driver took from a prefetch.
fn run_counted_v1(
    config: &NativeExpandedTrainingRunV1,
    limit: Option<usize>,
    before: impl FnOnce() + Send + 'static,
) -> (Result<Value, String>, usize) {
    let config = config.clone();
    std::thread::spawn(move || {
        before();
        let result = run_native_expanded_training_v1(&config, limit);
        (result, collected_for_test_v1())
    })
    .join()
    .unwrap()
}

/// A run's plan without the key: the one member an overlapped run's plan
/// has more.
fn plan_without_key_v1(bytes: &[u8]) -> Value {
    let mut plan: Value = serde_json::from_slice(bytes).unwrap();
    plan["config"]
        .as_object_mut()
        .unwrap()
        .remove("update_tail_overlap_v1");
    plan
}

fn staging_debris_v1(path: &Path) -> bool {
    let name = path.file_name().unwrap().to_str().unwrap();
    name.starts_with('.') && name.contains(".stage")
}

/// Every file of `got` is a file of `want` with the same bytes, apart from
/// staging debris, the lock, timed documents (equal without their telemetry)
/// and the plan (equal without the key); with `same_files`, `got` also has
/// every file of `want`.
fn assert_same_tree_v1(case: &str, got: &Path, want: &Path, same_files: bool) {
    let wanted = files_under_v1(want);
    let found = files_under_v1(got);
    for (path, bytes) in &found {
        if staging_debris_v1(path) {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let expected = wanted
            .get(path)
            .unwrap_or_else(|| panic!("{case}: unexpected file {}", path.display()));
        if name == "run.json" {
            assert_eq!(
                plan_without_key_v1(bytes),
                plan_without_key_v1(expected),
                "{case}: the plan differs beyond the key"
            );
        } else if TIMED.contains(&name) {
            let parse =
                |bytes: &[u8]| without_telemetry_v1(&serde_json::from_slice(bytes).unwrap());
            assert_eq!(
                parse(bytes),
                parse(expected),
                "{case}: {} differs outside its telemetry",
                path.display()
            );
        } else if name != "run.lock" {
            assert!(bytes == expected, "{case}: {} differs", path.display());
        }
    }
    if same_files {
        for path in wanted.keys() {
            assert!(
                staging_debris_v1(path) || found.contains_key(path),
                "{case}: {} missing",
                path.display()
            );
        }
    }
}

/// The after-state of every iteration of a run result, from its update
/// records.
fn after_states_v1(result: &Value) -> Vec<Value> {
    (0..result["iterations"].as_array().unwrap().len())
        .map(|index| {
            let update = parse_pin(&iteration_receipt_v1(result, index), "update").unwrap();
            read_pin(&update).unwrap()["after_state_sha256"].clone()
        })
        .collect()
}

/// How many iterations of the run in `directory` are committed.
fn committed_v1(directory: &Path) -> usize {
    let iterations = directory.join("iterations");
    if !iterations.exists() {
        return 0;
    }
    fs::read_dir(iterations)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .join("complete.json")
                .exists()
        })
        .count()
}

/// Whether any attempt of iteration `index` of the run in `directory` holds a
/// published `update.json`.
fn update_published_v1(directory: &Path, index: usize) -> bool {
    let iteration = directory.join("iterations").join(format!("{index:06}"));
    iteration.exists()
        && fs::read_dir(iteration).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .join("update")
                .join("update.json")
                .exists()
        })
}

/// The root publications of the run in `directory` are counted under: its
/// canonical path, which exists before the run only up to its parent.
fn publication_root_v1(directory: &Path) -> PathBuf {
    directory
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join(directory.file_name().unwrap())
}

/// Invariant: with the key absent the config has its bytes and identity (the
/// key is never serialized, also when written as false); true is serialized;
/// no command carries it; and only a run with parallel collection and a
/// prepared CPU update takes it.
#[test]
fn update_tail_overlap_default_preserves_the_config_wire_and_validates() {
    let default = schedule();
    assert!(!default.update_tail_overlap_v1);
    let default_value = value(&default).unwrap();
    assert!(default_value.get("update_tail_overlap_v1").is_none());
    let default_bytes = serde_json::to_vec(&default).unwrap();
    assert!(!String::from_utf8_lossy(&default_bytes).contains("update_tail_overlap"));
    let reparsed: NativeExpandedTrainingRunV1 = serde_json::from_slice(&default_bytes).unwrap();
    assert!(!reparsed.update_tail_overlap_v1);
    assert_eq!(serde_json::to_vec(&reparsed).unwrap(), default_bytes);
    // An explicit false is the absent key: the same bytes and identity.
    let mut explicit_false = default_value.clone();
    explicit_false["update_tail_overlap_v1"] = json!(false);
    let parsed: NativeExpandedTrainingRunV1 = serde_json::from_value(explicit_false).unwrap();
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), default_bytes);
    assert_eq!(identity(&parsed).unwrap(), identity(&default).unwrap());
    // True is serialized and is another config; without the member it is the
    // default's bytes again.
    let mut on = default.clone();
    on.update_tail_overlap_v1 = true;
    let on_value = value(&on).unwrap();
    assert_eq!(on_value["update_tail_overlap_v1"], true);
    assert_ne!(identity(&on).unwrap(), identity(&default).unwrap());
    let mut stripped = on_value.clone();
    stripped
        .as_object_mut()
        .unwrap()
        .remove("update_tail_overlap_v1");
    assert_eq!(stripped, default_value);
    let back: NativeExpandedTrainingRunV1 = serde_json::from_value(on_value.clone()).unwrap();
    assert!(back.update_tail_overlap_v1);
    let mut not_a_flag = on_value;
    not_a_flag["update_tail_overlap_v1"] = json!("on");
    assert!(serde_json::from_value::<NativeExpandedTrainingRunV1>(not_a_flag).is_err());
    // The key is the run's alone: its commands are the commands without it.
    let mut parallel = parallel_two_iteration_config_v1("overlap-validation");
    let overlapped = overlapped_v1(&parallel);
    let episodes = resolve_episodes(&parallel, 0, &parallel.initial_source, &[]).unwrap();
    let output = parallel.output_directory.join("command");
    assert_eq!(
        value(&collection_command(
            &overlapped,
            0,
            &overlapped.initial_source,
            &episodes,
            output.clone()
        ))
        .unwrap(),
        value(&collection_command(
            &parallel,
            0,
            &parallel.initial_source,
            &episodes,
            output.clone()
        ))
        .unwrap()
    );
    assert_eq!(
        value(&update_command(
            &overlapped,
            0,
            &overlapped.initial_source,
            Vec::new(),
            output.clone()
        ))
        .unwrap(),
        value(&update_command(
            &parallel,
            0,
            &parallel.initial_source,
            Vec::new(),
            output
        ))
        .unwrap()
    );
    // Serial collection, a serial update or another kind of update keeps
    // its serial order: the key is refused there.
    let refused = |config: &NativeExpandedTrainingRunV1| {
        let error = config.validate_v1().unwrap_err();
        assert!(
            error.contains("update_tail_overlap_v1 requires parallel collection"),
            "{error}"
        );
    };
    for (collection, preparation) in [(1, 2), (2, 1)] {
        let mut serial = overlapped.clone();
        serial.collection_workers = collection;
        serial.preparation_workers = preparation;
        refused(&serial);
    }
    let mut masked = overlapped.clone();
    masked.line_b = Some(crate::expanded_deck_training_v1::LineBUpdateOptionsV1 {
        optimizer_mask: crate::expanded_deck_training_v1::OptimizerMaskV1::LineBHeadOnlyV1,
        teacher: None,
    });
    refused(&masked);
    // The same configs without the key are accepted.
    parallel.collection_workers = 1;
    parallel.preparation_workers = 1;
    parallel.validate_v1().unwrap();
}

/// Invariant 1 (bytes): a three-update run with the key publishes the files
/// of the same run without it, with the same bytes: every command,
/// trajectory and checkpoint byte for byte, every receipt apart from its
/// timing telemetry, and the plan apart from the key. Every update but the
/// last hands its successor's collection over (so the comparison is of an
/// overlapped run), every iteration stays in its first attempt, and the
/// finished run revalidates as complete.
#[test]
fn update_tail_overlap_run_publishes_the_serial_runs_bytes() {
    let serial = overlap_config_v1("overlap-bytes", 3);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let (serial_result, serial_prefetched) = run_counted_v1(&serial, None, || {});
    let serial_result = serial_result.unwrap();
    assert_eq!(serial_result["complete"], true);
    assert_eq!(serial_prefetched, 0, "a run without the key overlapped");
    // Read through the receipts' pins, so before the tree moves.
    let serial_states = after_states_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let (result, prefetched) = run_counted_v1(&overlapped, None, || {});
    let result = result.unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(prefetched, 2, "updates 0 and 1 hand their successor over");
    assert_eq!(result["actual_identity"], serial_result["actual_identity"]);
    assert_eq!(result["source"], serial_result["source"]);
    assert_eq!(after_states_v1(&result), serial_states);
    assert_same_tree_v1("overlapped against serial", &directory, &reference, true);
    assert!(!directory.join("restarts.log").exists());
    for index in 0..3 {
        let iteration = directory.join("iterations").join(format!("{index:06}"));
        let mut names: Vec<String> = fs::read_dir(iteration)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["attempt-000000", "complete.json"]);
    }
    // A completed overlapped run only verifies: the same result, no prefetch.
    let (again, prefetched) = run_counted_v1(&overlapped, None, || {});
    assert_eq!(again.unwrap(), result);
    assert_eq!(prefetched, 0);
    // The key is part of the plan: the run without it is another run.
    let error = fresh_run_v1(&serial).unwrap_err();
    assert!(error.contains("resume plan"), "{error}");
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// Runs `serial` and then, in the same output directory, the same run with
/// the key, and asserts that every update but the last handed its successor's
/// collection over, that the two runs reach the same states, and that their
/// trees hold the same bytes (`assert_same_tree_v1`). `before_move` sees the
/// serial result while its tree is still in place. Leaves neither tree.
fn assert_overlapped_run_matches_v1(
    case: &str,
    serial: &NativeExpandedTrainingRunV1,
    before_move: impl FnOnce(&Value),
) {
    serial.validate_v1().unwrap();
    let overlapped = overlapped_v1(serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let serial_result =
        fresh_run_v1(serial).unwrap_or_else(|error| panic!("{case}: serial run: {error}"));
    assert_eq!(serial_result["complete"], true, "{case}");
    before_move(&serial_result);
    // Read through the receipts' pins, so before the tree moves.
    let serial_states = after_states_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let (result, prefetched) = run_counted_v1(&overlapped, None, || {});
    let result = result.unwrap_or_else(|error| panic!("{case}: overlapped run: {error}"));
    assert_eq!(result["complete"], true, "{case}");
    assert_eq!(
        prefetched,
        serial.iterations.len() - 1,
        "{case}: every update but the last hands its successor over"
    );
    assert_eq!(
        result["actual_identity"], serial_result["actual_identity"],
        "{case}"
    );
    assert_eq!(after_states_v1(&result), serial_states, "{case}");
    assert_same_tree_v1(case, &directory, &reference, true);
    assert!(!directory.join("restarts.log").exists(), "{case}");
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// Invariant 1 (bytes) for the sources the RT1 chains train: a FlatV7 run
/// (from the V4 import, then from its own FlatV7 checkpoints) and a FlatV8
/// run (from a converted V8 checkpoint, with the fixed-partition backward),
/// each of three updates with London openings, parallel collection and
/// ordered preparation, publish with the key the bytes they publish without
/// it. Their loader first probes the new checkpoint's metadata and then
/// reads its record: both come from memory while the checkpoint is being
/// published.
#[test]
fn update_tail_overlap_flat_v7_and_v8_runs_publish_the_serial_runs_bytes() {
    use crate::native_flat_tensorizer_v8::{
        FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8,
    };
    // `london_v7_config_v1` with a second game an iteration, a third
    // iteration and two workers, as `overlap_config_v1` makes them.
    let flat = |label: &str| {
        let mut config = london_v7_config_v1(
            label,
            gae_loss_v1(crate::expanded_deck_training_v1::PregameAdvantageV1::default()),
            None,
        );
        for (iteration, scheduled) in config.iterations.iter_mut().enumerate() {
            let mut second = scheduled.episodes[0].clone();
            second.episode.id = format!("{label}-iter-{iteration}-second");
            second.episode.seed += 1_000;
            second.episode.starting_player = 1;
            scheduled.episodes.push(second);
        }
        let mut third = config.iterations[1].clone();
        for (slot, scheduled) in third.episodes.iter_mut().enumerate() {
            scheduled.episode.id = format!("{label}-iter-2-slot-{slot}");
            scheduled.episode.seed += 17;
        }
        config.iterations.push(third);
        config.collection_workers = 2;
        config.preparation_workers = 2;
        config
    };
    let v7 = flat("overlap-flat-v7");
    // The V8 run starts from the FlatV7 run's first checkpoint, converted.
    let v8_start = v7.output_directory.with_file_name("v8-start.json");
    let _ = fs::remove_file(&v8_start);
    let mut converted = None;
    assert_overlapped_run_matches_v1("FlatV7", &v7, |serial_result| {
        converted = Some(
            crate::expanded_deck_training_v1::v8_inclusion_conversion_v1::write_v8_checkpoint_v1(
                &iteration_checkpoint_v1(serial_result, 0),
                &v8_start,
                0x0008_0000_0000_0007,
            )
            .unwrap()
            .pin,
        );
    });
    let mut v8 = flat("overlap-flat-v8");
    v8.initial_source = ExpandedModelSourceV1 {
        play_import: v7.initial_source.play_import.clone(),
        feature_transfer: crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V8.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V8.into(),
        },
        checkpoint: converted,
        s_learner: None,
    };
    v8.update_backward_execution = UpdateBackwardExecutionV1::FixedPartition4;
    assert_overlapped_run_matches_v1("FlatV8", &v8, |_| {});
    fs::remove_file(&v8_start).unwrap();
}

/// Invariant 4 (the invocation limit): a limited overlapped run stops at the
/// update the serial run stops at, hands over only up to that update, starts
/// no collection past it, and resumes to the serial run's tree.
#[test]
fn update_tail_overlap_stops_at_the_invocation_limit_like_the_serial_run() {
    let serial = overlap_config_v1("overlap-limit", 3);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let serial_result = fresh_run_v1(&serial).unwrap();
    fs::rename(&directory, &reference).unwrap();
    let third = directory.join("iterations").join("000002");
    // Two updates: the first hands the second's collection over, the second
    // hands nothing over.
    let (limited, prefetched) = run_counted_v1(&overlapped, Some(2), || {});
    let limited = limited.unwrap();
    assert_eq!(limited["completed_iterations"], 2);
    assert_eq!(limited["complete"], false);
    assert_eq!(prefetched, 1);
    assert_eq!(committed_v1(&directory), 2);
    assert!(!third.exists(), "a collection started past the limit");
    assert_same_tree_v1("at the limit", &directory, &reference, false);
    let (resumed, prefetched) = run_counted_v1(&overlapped, None, || {});
    let resumed = resumed.unwrap();
    assert_eq!(resumed["complete"], true);
    assert_eq!(prefetched, 0, "the last update has no successor");
    assert_eq!(resumed["actual_identity"], serial_result["actual_identity"]);
    assert_same_tree_v1("resumed after the limit", &directory, &reference, true);
    assert!(!directory.join("restarts.log").exists());
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// Invariant 4 (the chain ledger's stop rule): an overlapped run whose
/// ledger reaches its stop value at a boundary stops at that update, as the
/// serial run does, with no collection started past it, does no new work
/// when relaunched, and once the stop is lifted resumes to the serial run's
/// tree. Its receipts record the ledger and the process CPU read at each
/// handoff.
#[test]
fn update_tail_overlap_stops_at_the_chain_ledger_like_the_serial_run() {
    let mut serial = overlap_config_v1("overlap-ledger", 3);
    let root = serial.output_directory.parent().unwrap().to_path_buf();
    let stat = root.join("cpu.stat");
    fs::write(&stat, "usage_usec 1000\nuser_usec 600\nsystem_usec 400\n").unwrap();
    let source = root.join("ledger-source.json");
    let write_source = |stop: Option<u64>| {
        let mut document = json!({"schema":CHAIN_LEDGER_SOURCE_SCHEMA_V1,
            "chain":"overlap/chain","closed_usec":5_000,"cpu_stat":stat,"instance":2});
        if let Some(stop) = stop {
            document["stop_at_usec"] = json!(stop);
        }
        fs::write(&source, serde_json::to_vec(&document).unwrap()).unwrap();
    };
    write_source(None);
    serial.chain_ledger = Some(ChainLedgerOptionsV1 {
        source: source.clone(),
    });
    serial.record_process_cpu = true;
    serial.validate_v1().unwrap();
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let serial_result = fresh_run_v1(&serial).unwrap();
    assert_eq!(serial_result["complete"], true);
    fs::rename(&directory, &reference).unwrap();
    let second = directory.join("iterations").join("000001");
    // Every boundary reads 5,000 + 1,000: a stop value of 6,000 stops the run
    // at its first boundary.
    write_source(Some(6_000));
    let (stopped, prefetched) = run_counted_v1(&overlapped, None, || {});
    let stopped = stopped.unwrap();
    assert_eq!(stopped["completed_iterations"], 1);
    assert_eq!(stopped["complete"], false);
    assert_eq!(prefetched, 0);
    assert_eq!(committed_v1(&directory), 1);
    assert!(
        !second.exists(),
        "a collection started past the ledger's stop"
    );
    assert_same_tree_v1("stopped at the ledger", &directory, &reference, false);
    // A relaunch with the same stop value does no new work.
    let (again, prefetched) = run_counted_v1(&overlapped, None, || {});
    assert_eq!(again.unwrap()["completed_iterations"], 1);
    assert_eq!(prefetched, 0);
    assert!(!second.exists());
    write_source(None);
    let (resumed, prefetched) = run_counted_v1(&overlapped, None, || {});
    let resumed = resumed.unwrap();
    assert_eq!(resumed["complete"], true);
    assert_eq!(prefetched, 1, "update 1 hands update 2's collection over");
    assert_eq!(resumed["actual_identity"], serial_result["actual_identity"]);
    assert_same_tree_v1("resumed past the stop", &directory, &reference, true);
    for index in 0..3 {
        let receipt = iteration_receipt_v1(&resumed, index);
        assert_eq!(receipt["chain_ledger"]["usec"], 6_000);
        assert!(receipt["driver_process_cpu_seconds"].as_f64().unwrap() > 0.0);
    }
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// Invariant 3 (failures of the deferred write): a failed publication of the
/// first update's checkpoint, and of its `update.json`, fails the overlapped
/// run with the serial run's error. Nothing is committed, and nothing of the
/// next iteration exists: its collection had started (the handoff comes
/// before the checkpoint's first write) and wrote nothing. A resume of the
/// run whose `update.json` failed reaches the serial run's states.
#[test]
fn update_tail_overlap_failed_checkpoint_writes_fail_like_the_serial_run() {
    let serial = overlap_config_v1("overlap-write-failures", 2);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let root = publication_root_v1(&directory);
    let _ = fs::remove_dir_all(&directory);
    let reference = fresh_run_v1(&serial).unwrap();
    let reference_states = after_states_v1(&reference);
    fs::remove_dir_all(&directory).unwrap();
    let update = directory
        .join("iterations")
        .join("000000")
        .join("attempt-000000")
        .join("update");
    let second = directory.join("iterations").join("000001");
    // Iteration 0's publications in the order they start, with or without the
    // key: the plan (0), the collection's command (1), two trajectories (2
    // and 3), the collection (4), the update's input (5) and command (6),
    // the checkpoint (7) and the update (8).
    for (publication, name, checkpoint_published) in
        [(7usize, "checkpoint.json", false), (8, "update.json", true)]
    {
        let mut errors = Vec::new();
        for (config, key) in [(&serial, "serial"), (&overlapped, "overlapped")] {
            let case = format!("{name} {key}");
            let ((failed, _), _, crashed) = with_crash_point_v1(
                &root,
                Some((publication, CrashPointV1::BeforeFinal)),
                || run_counted_v1(config, None, || {}),
            );
            assert!(crashed, "{case}: never reached");
            let error = failed.unwrap_err();
            assert!(error.contains("injected crash"), "{case}: {error}");
            // The failed publication is the one named: its staging file is
            // left and its final name is absent.
            assert!(
                update.join(format!(".{name}.stage")).exists() && !update.join(name).exists(),
                "{case}: publication {publication} is not {name}"
            );
            assert_eq!(
                update.join("checkpoint.json").exists(),
                checkpoint_published,
                "{case}"
            );
            assert_eq!(committed_v1(&directory), 0, "{case}");
            assert!(!directory.join("completion.json").exists(), "{case}");
            assert!(
                !second.exists(),
                "{case}: the next iteration wrote before the update was durable"
            );
            if checkpoint_published && config.update_tail_overlap_v1 {
                let (resumed, _) = run_counted_v1(config, None, || {});
                let resumed = resumed.unwrap_or_else(|error| panic!("{case}: resume: {error}"));
                assert_eq!(resumed["complete"], true, "{case}");
                assert_eq!(after_states_v1(&resumed), reference_states, "{case}");
                assert_eq!(
                    resumed["actual_identity"]["state_sha256"],
                    reference["actual_identity"]["state_sha256"],
                    "{case}"
                );
            }
            errors.push(error);
            fs::remove_dir_all(&directory).unwrap();
        }
        assert_eq!(
            errors[0], errors[1],
            "{name}: the overlapped run failed with another error"
        );
    }
}

/// Invariant 3 (failures after the checkpoint) and invariant 2 (no later
/// commit, and a resume to the same states): the update's own crash point
/// (the checkpoint is published and read back, `update.json` is not) and a
/// failed `complete.json` each fail the overlapped run with the serial run's
/// error and commit nothing. Before `update.json` nothing of the next
/// iteration exists. After it, the next iteration may hold what an
/// interrupted collection holds, never an update or a receipt, and a resume
/// commits the published update without running it again and reaches the
/// serial run's states.
#[test]
fn update_tail_overlap_failed_receipts_fail_like_the_serial_run_and_commit_nothing_later() {
    let serial = overlap_config_v1("overlap-receipt-failures", 2);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let _ = fs::remove_dir_all(&directory);
    let reference = fresh_run_v1(&serial).unwrap();
    let reference_states = after_states_v1(&reference);
    fs::remove_dir_all(&directory).unwrap();
    let first = directory.join("iterations").join("000000");
    let second = directory.join("iterations").join("000001");
    // The update's crash point.
    let mut errors = Vec::new();
    for (config, key) in [(&serial, "serial"), (&overlapped, "overlapped")] {
        let (failed, prefetched) = run_counted_v1(
            config,
            None,
            crate::expanded_deck_training_v1::arm_crash_point_for_test_v1,
        );
        let error = failed.unwrap_err();
        assert!(error.contains("crash point"), "{key}: {error}");
        assert_eq!(prefetched, 0, "{key}");
        assert_eq!(committed_v1(&directory), 0, "{key}");
        assert!(first.join("attempt-000000/update/checkpoint.json").exists());
        assert!(!update_published_v1(&directory, 0), "{key}");
        assert!(
            !second.exists(),
            "{key}: the next iteration wrote before the update was durable"
        );
        errors.push(error);
        fs::remove_dir_all(&directory).unwrap();
    }
    assert_eq!(errors[0], errors[1]);
    // A failed receipt: `complete.json` of the first iteration.
    let mut errors = Vec::new();
    for (config, key) in [(&serial, "serial"), (&overlapped, "overlapped")] {
        let (failed, _) = run_counted_v1(config, None, || {
            fail_next_publish_for_test_v1("complete.json")
        });
        let error = failed.unwrap_err();
        assert!(error.contains("injected failure"), "{key}: {error}");
        assert_eq!(
            committed_v1(&directory),
            0,
            "{key}: an update was committed"
        );
        assert!(!directory.join("completion.json").exists(), "{key}");
        assert!(update_published_v1(&directory, 0), "{key}");
        if second.exists() {
            // Only the overlapped run's next collection can have written,
            // and only what an interrupted collection writes.
            assert!(config.update_tail_overlap_v1, "{key}");
            for (path, _) in files_under_v1(&second) {
                if staging_debris_v1(&path) {
                    continue;
                }
                let name = path.to_string_lossy().replace('\\', "/");
                assert!(
                    name == "attempt-000000/collect-command.json"
                        || name.starts_with("attempt-000000/collect/"),
                    "{key}: {name} was written for an iteration whose input is not committed"
                );
            }
        }
        let before = files_under_v1(&first);
        let (resumed, _) = run_counted_v1(config, None, || {});
        let resumed = resumed.unwrap_or_else(|error| panic!("{key}: resume: {error}"));
        assert_eq!(resumed["complete"], true, "{key}");
        assert_eq!(after_states_v1(&resumed), reference_states, "{key}");
        // The published update was committed as it was: no second attempt,
        // and every file it had is unchanged.
        let after = files_under_v1(&first);
        for (path, bytes) in &before {
            assert!(after.get(path) == Some(bytes), "{key}: {}", path.display());
        }
        assert!(!first.join("attempt-000001").exists(), "{key}");
        assert!(first.join("complete.json").exists(), "{key}");
        errors.push(error);
        fs::remove_dir_all(&directory).unwrap();
    }
    assert_eq!(errors[0], errors[1]);
}

/// Invariant 2 (crash safety), every crash point: a crash just before or just
/// after any publication of a three-update overlapped run leaves only files
/// the uninterrupted serial run publishes, with the same bytes apart from
/// timing telemetry and the key in the plan, plus staging debris; nothing of
/// an iteration exists unless the update before it is published, and the
/// committed iterations are a prefix; and resuming reaches the serial run's
/// state at every iteration.
#[test]
#[ignore = "slow: one crashed run and its resume per crash point; run before shipping a change to the update tail overlap"]
fn update_tail_overlap_crash_at_any_publication_resumes_to_the_serial_result() {
    let serial = overlap_config_v1("overlap-crash-points", 3);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("uninterrupted");
    let root = publication_root_v1(&directory);
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let serial_result = fresh_run_v1(&serial).unwrap();
    assert_eq!(serial_result["complete"], true);
    let reference_states = after_states_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let ((counted, _), publications, crashed) =
        with_crash_point_v1(&root, None, || run_counted_v1(&overlapped, None, || {}));
    assert!(!crashed);
    assert_eq!(counted.unwrap()["complete"], true);
    fs::remove_dir_all(&directory).unwrap();
    // Each iteration publishes its two commands, two trajectories, the
    // collection, the update input, the checkpoint, the update and its
    // receipt; the run adds its plan and completion.
    assert_eq!(publications, 3 * 9 + 2, "{publications} publications");
    for index in 0..publications {
        for point in [CrashPointV1::BeforeFinal, CrashPointV1::AfterFinal] {
            let case = format!("publication {index} {point:?}");
            let ((crashed_run, _), _, crashed) =
                with_crash_point_v1(&root, Some((index, point)), || {
                    run_counted_v1(&overlapped, None, || {})
                });
            assert!(crashed, "{case}: never reached");
            let error = crashed_run.unwrap_err();
            assert!(error.contains("injected crash"), "{case}: {error}");
            assert_same_tree_v1(&case, &directory, &reference, false);
            let committed = committed_v1(&directory);
            for iteration in 0..3 {
                let path = directory.join("iterations").join(format!("{iteration:06}"));
                let has_files = path.exists() && !files_under_v1(&path).is_empty();
                if iteration > 0 && has_files {
                    assert!(
                        update_published_v1(&directory, iteration - 1),
                        "{case}: iteration {iteration} wrote before update {} was durable",
                        iteration - 1
                    );
                }
                assert_eq!(
                    path.join("complete.json").exists(),
                    iteration < committed,
                    "{case}: the committed iterations are not a prefix"
                );
            }
            let (resumed, _) = run_counted_v1(&overlapped, None, || {});
            let resumed = resumed.unwrap_or_else(|error| panic!("{case}: resume failed: {error}"));
            assert_eq!(resumed["complete"], true, "{case}");
            assert_eq!(after_states_v1(&resumed), reference_states, "{case}");
            for field in ["state_sha256", "adam_step"] {
                assert_eq!(
                    resumed["actual_identity"][field], serial_result["actual_identity"][field],
                    "{case}: {field}"
                );
            }
            fs::remove_dir_all(&directory).unwrap();
        }
    }
    fs::remove_dir_all(&reference).unwrap();
}
