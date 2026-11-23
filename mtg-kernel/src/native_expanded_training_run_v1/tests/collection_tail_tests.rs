//! Collection tail (`collection_tail_v1`): the key's wire form and admission,
//! and that a run with it publishes the bytes, commits, fails and stops as
//! the same run without either key does, at two and twelve collection and
//! preparation workers, and with sealed records and resident reads.
//!
//! Every run of a comparison uses the same output directory (the reference
//! tree is moved aside), so documents that name it compare equal.

use super::update_tail_overlap_tests::{
    after_states_v1, assert_same_tree_v1, committed_v1, iteration_directory_v1, overlap_config_v1,
    publication_root_v1, run_content_v1,
};
use super::*;
use crate::durable_publication_v1::crash_points_for_test_v1::{with_crash_point_v1, CrashPointV1};
use crate::held_output_v1::HeldEventV1;
use crate::native_expanded_training_run_v1::collection_tail_v1::{
    admitted_for_test_v1, hold_helper_for_test_v1, record_update_output_for_test_v1,
    restores_for_test_v1, VALIDATION_REFUSAL_V1,
};
use crate::native_expanded_training_run_v1::update_tail_overlap_v1::{
    collected_for_test_v1, hold_at_gate_for_test_v1,
};
use std::sync::{Arc, Mutex};

/// `overlap_config_v1` with `games` games an iteration (two or more), its
/// collections on `collection_workers` workers and its updates prepared on
/// `preparation_workers` workers, and neither key.
fn tail_config_v1(
    label: &str,
    updates: usize,
    games: usize,
    collection_workers: usize,
    preparation_workers: usize,
) -> NativeExpandedTrainingRunV1 {
    let mut config = overlap_config_v1(label, updates);
    for (iteration, scheduled) in config.iterations.iter_mut().enumerate() {
        let first = scheduled.episodes[0].clone();
        while scheduled.episodes.len() < games {
            let slot = scheduled.episodes.len();
            let mut next = first.clone();
            next.episode.id = format!("{label}-iter-{iteration}-slot-{slot}");
            next.episode.seed += 1_000 * slot as u64;
            next.episode.starting_player = (slot % 2) as u8;
            scheduled.episodes.push(next);
        }
    }
    config.collection_workers = collection_workers;
    config.preparation_workers = preparation_workers;
    config.validate_v1().unwrap();
    config
}

/// `config` with both keys on.
fn tailed_v1(config: &NativeExpandedTrainingRunV1) -> NativeExpandedTrainingRunV1 {
    let mut tailed = config.clone();
    tailed.update_tail_overlap_v1 = true;
    tailed.collection_tail_v1 = true;
    tailed.validate_v1().unwrap();
    tailed
}

/// What a run on its own thread reports besides its result: the
/// collections its driver took from a prefetch, the speculative updates
/// admitted at their learner gate, and the learners restored from a
/// template.
#[derive(Debug, Default, PartialEq, Eq)]
struct CountsV1 {
    prefetched: usize,
    admitted: usize,
    restored: usize,
}

/// Runs `config` on a new thread, which starts with empty thread-local
/// caches and test hooks as a new process does, after `before` on that
/// thread.
fn run_tailed_v1(
    config: &NativeExpandedTrainingRunV1,
    limit: Option<usize>,
    before: impl FnOnce() + Send + 'static,
) -> (Result<Value, String>, CountsV1) {
    let config = config.clone();
    std::thread::spawn(move || {
        before();
        let result = run_native_expanded_training_v1(&config, limit);
        let counts = CountsV1 {
            prefetched: collected_for_test_v1(),
            admitted: admitted_for_test_v1(),
            restored: restores_for_test_v1(),
        };
        (result, counts)
    })
    .join()
    .unwrap()
}

/// The held output of a run's speculative updates (`held_output_v1`).
#[derive(Debug, Default)]
struct UpdateLogV1 {
    /// Every held line, once written.
    written: Vec<String>,
    /// For each holder released: the lines it held, and how many
    /// `update-command.json` files of the run existed at that moment.
    released: Vec<(usize, usize)>,
    /// The lines each dropped holder held.
    discarded: Vec<usize>,
}

/// How many attempts of the run in `directory` hold `update-command.json`.
fn published_commands_v1(directory: &Path) -> usize {
    let iterations = directory.join("iterations");
    if !iterations.exists() {
        return 0;
    }
    fs::read_dir(iterations)
        .unwrap()
        .flat_map(|iteration| fs::read_dir(iteration.unwrap().path()).unwrap())
        .filter(|attempt| {
            attempt
                .as_ref()
                .unwrap()
                .path()
                .join("update-command.json")
                .exists()
        })
        .count()
}

/// A `before` for `run_tailed_v1` that holds every helper until its
/// speculative update has reached its gate or returned, holds every update
/// at its handoff until the collection it started waits at its gate with a
/// played game (`hold_at_gate_for_test_v1`), and records the speculative
/// updates' held output against the run in `directory`.
fn held_update_log_v1(
    directory: &Path,
) -> (impl FnOnce() + Send + 'static, Arc<Mutex<UpdateLogV1>>) {
    let log = Arc::new(Mutex::new(UpdateLogV1::default()));
    let sink = Arc::clone(&log);
    let directory = directory.to_path_buf();
    let arm = move || {
        record_update_output_for_test_v1(Arc::new(move |event: HeldEventV1| {
            let mut log = sink.lock().unwrap();
            match event {
                HeldEventV1::Written(line) => log.written.push(line),
                HeldEventV1::Released { held } => {
                    let commands = published_commands_v1(&directory);
                    log.released.push((held, commands));
                }
                HeldEventV1::Discarded { held } => log.discarded.push(held),
                HeldEventV1::Forwarded { held } => panic!("an update forwarded {held} lines"),
            }
        }));
        hold_helper_for_test_v1();
        hold_at_gate_for_test_v1();
    };
    (arm, log)
}

/// Invariant: with the key absent the config has its bytes and identity (the
/// key is never serialized, also when written as false); true is serialized
/// and is another config; and no command carries it.
#[test]
fn collection_tail_default_preserves_the_config_wire() {
    let default = schedule();
    assert!(!default.collection_tail_v1);
    let default_value = value(&default).unwrap();
    assert!(default_value.get("collection_tail_v1").is_none());
    let default_bytes = serde_json::to_vec(&default).unwrap();
    assert!(!String::from_utf8_lossy(&default_bytes).contains("collection_tail"));
    let reparsed: NativeExpandedTrainingRunV1 = serde_json::from_slice(&default_bytes).unwrap();
    assert!(!reparsed.collection_tail_v1);
    assert_eq!(serde_json::to_vec(&reparsed).unwrap(), default_bytes);
    let mut explicit_false = default_value.clone();
    explicit_false["collection_tail_v1"] = json!(false);
    let parsed: NativeExpandedTrainingRunV1 = serde_json::from_value(explicit_false).unwrap();
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), default_bytes);
    assert_eq!(identity(&parsed).unwrap(), identity(&default).unwrap());
    let mut on = default.clone();
    on.collection_tail_v1 = true;
    let on_value = value(&on).unwrap();
    assert_eq!(on_value["collection_tail_v1"], true);
    assert_ne!(identity(&on).unwrap(), identity(&default).unwrap());
    let mut stripped = on_value.clone();
    stripped
        .as_object_mut()
        .unwrap()
        .remove("collection_tail_v1");
    assert_eq!(stripped, default_value);
    let mut not_a_flag = on_value;
    not_a_flag["collection_tail_v1"] = json!("on");
    assert!(serde_json::from_value::<NativeExpandedTrainingRunV1>(not_a_flag).is_err());
    // The key is the run's alone: its commands are the commands without it.
    let parallel = tail_config_v1("tail-wire", 2, 2, 2, 2);
    let tailed = tailed_v1(&parallel);
    let episodes = resolve_episodes(&parallel, 0, &parallel.initial_source, &[]).unwrap();
    let output = parallel.output_directory.join("command");
    assert_eq!(
        value(&collection_command(
            &tailed,
            0,
            &tailed.initial_source,
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
            &tailed,
            0,
            &tailed.initial_source,
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
}

/// Validation: the key requires `update_tail_overlap_v1`, with its own
/// message, and is admitted with it.
#[test]
fn collection_tail_is_refused_without_the_update_tail_overlap() {
    let mut config = schedule();
    config.collection_workers = 2;
    config.preparation_workers = 2;
    config.collection_tail_v1 = true;
    assert_eq!(config.validate_v1().unwrap_err(), VALIDATION_REFUSAL_V1);
    config.update_tail_overlap_v1 = true;
    config.validate_v1().unwrap();
}

/// Runs `serial` and then, in the same output directory, the same run with
/// both keys and the holds of `held_update_log_v1`, and asserts that the two
/// runs reach the same states and their trees hold the same bytes
/// (`assert_same_tree_v1`); that every update but the last handed its
/// successor's collection over, every update ran ahead of its helper and was
/// admitted at its gate, and `restored` learners came from the template; and
/// that each update's held lines (its input line) were written only once its
/// own `update-command.json` existed, none dropped. `before_move` sees the
/// serial result while its tree is still in place. Leaves neither tree.
fn assert_tailed_run_matches_v1(
    case: &str,
    serial: &NativeExpandedTrainingRunV1,
    restored: usize,
    before_move: impl FnOnce(&Value),
) {
    serial.validate_v1().unwrap();
    let tailed = tailed_v1(serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let (serial_result, serial_counts) = run_tailed_v1(serial, None, || {});
    let serial_result = serial_result.unwrap_or_else(|error| panic!("{case}: serial run: {error}"));
    assert_eq!(serial_result["complete"], true, "{case}");
    assert_eq!(serial_counts, CountsV1::default(), "{case}");
    before_move(&serial_result);
    // Read through the receipts' pins, so before the tree moves.
    let serial_states = after_states_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let (arm, log) = held_update_log_v1(&directory);
    let (result, counts) = run_tailed_v1(&tailed, None, arm);
    let result = result.unwrap_or_else(|error| panic!("{case}: tailed run: {error}"));
    assert_eq!(result["complete"], true, "{case}");
    let updates = serial.iterations.len();
    assert_eq!(
        counts,
        CountsV1 {
            prefetched: updates - 1,
            admitted: updates,
            restored,
        },
        "{case}"
    );
    assert_eq!(
        result["actual_identity"], serial_result["actual_identity"],
        "{case}"
    );
    assert_eq!(result["source"], serial_result["source"], "{case}");
    assert_eq!(after_states_v1(&result), serial_states, "{case}");
    assert_same_tree_v1(case, &directory, &reference, true);
    assert!(!directory.join("restarts.log").exists(), "{case}");
    {
        let log = log.lock().unwrap();
        assert!(log.discarded.is_empty(), "{case}: {log:?}");
        assert_eq!(
            log.released
                .iter()
                .map(|(_, commands)| *commands)
                .collect::<Vec<_>>(),
            (1..=updates).collect::<Vec<_>>(),
            "{case}: an update's lines were written before its own command was durable: {log:?}"
        );
        assert!(
            log.released.iter().all(|(held, _)| *held >= 1),
            "{case}: an update held no line: {log:?}"
        );
        assert_eq!(log.written.len(), updates, "{case}: {log:?}");
        assert!(
            log.written
                .iter()
                .all(|line| line.starts_with("update input: ")),
            "{case}: {log:?}"
        );
    }
    // A completed run only verifies: the same result, nothing run again.
    let (again, counts) = run_tailed_v1(&tailed, None, || {});
    assert_eq!(again.unwrap(), result, "{case}");
    assert_eq!(counts, CountsV1::default(), "{case}");
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// Invariant 1 (bytes) at two and twelve collection workers and two and
/// twelve preparation workers: a three-update run with both keys publishes
/// the files of the same run with neither, with the same bytes (commands,
/// trajectories and checkpoints byte for byte, receipts apart from their
/// timing telemetry, the plan apart from the keys). Each update reads and
/// prepares its inputs before its collection is validated (the helper is
/// held until then), its learner and its lines wait for its documents, and
/// the learners of updates 1 and 2 come from the template. Twelve
/// collection workers play thirteen games an iteration, so every worker
/// plays.
#[test]
fn collection_tail_runs_publish_the_serial_runs_bytes_at_two_and_twelve_workers() {
    for (collection_workers, preparation_workers) in [(2, 2), (12, 2), (2, 12), (12, 12)] {
        let case = format!(
            "{collection_workers} collection and {preparation_workers} preparation workers"
        );
        let games = if collection_workers > 2 {
            collection_workers + 1
        } else {
            2
        };
        let serial = tail_config_v1(
            &format!("tail-bytes-{collection_workers}-{preparation_workers}"),
            3,
            games,
            collection_workers,
            preparation_workers,
        );
        assert_tailed_run_matches_v1(&case, &serial, 2, |_| {});
    }
}

/// Invariant 1 (bytes) for the sources the RT1 chains train, whose learners
/// restore from their FlatV7 and FlatV8 templates: a FlatV7 run (from the V4
/// import, then from its own FlatV7 checkpoints) and a FlatV8 run (from a
/// converted V8 checkpoint, with the fixed-partition backward), each of
/// three updates with London openings, publish with both keys the bytes they
/// publish with neither. In both, the learners of updates 1 and 2 are
/// restored from the template (the FlatV8 run's first learner is served by
/// the loaded-source cache, which the run's start filled).
#[test]
fn collection_tail_flat_v7_and_v8_runs_publish_the_serial_runs_bytes() {
    use crate::native_flat_tensorizer_v8::{
        FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8,
    };
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
    let v7 = flat("tail-flat-v7");
    let v8_start = v7.output_directory.with_file_name("v8-start.json");
    let _ = fs::remove_file(&v8_start);
    let mut converted = None;
    assert_tailed_run_matches_v1("FlatV7", &v7, 2, |serial_result| {
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
    let mut v8 = flat("tail-flat-v8");
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
    assert_tailed_run_matches_v1("FlatV8", &v8, 2, |_| {});
    fs::remove_file(&v8_start).unwrap();
}

/// The environment variable that runs
/// `collection_tail_sealed_resident_child_v1` in its child process.
const SEALED_RESIDENT_CHILD_ENV_V1: &str = "MTG_KERNEL_COLLECTION_TAIL_SEALED_RESIDENT_CHILD_V1";

/// Invariant 1 (bytes) in the modes the C-recipe chains run: a three-update
/// run with sealed records (`record_envelope: zstd1`) and resident reads
/// (`verified_reads: resident`), at two and at twelve collection and
/// preparation workers, publishes with both keys the bytes it publishes with
/// neither. Resident reads are a one-way process switch, so the comparison
/// runs in a child process (`collection_tail_sealed_resident_child_v1`).
#[test]
fn collection_tail_keeps_every_byte_with_sealed_records_and_resident_reads() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "native_expanded_training_run_v1::tests::collection_tail_tests::collection_tail_sealed_resident_child_v1",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(SEALED_RESIDENT_CHILD_ENV_V1, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The child of
/// `collection_tail_keeps_every_byte_with_sealed_records_and_resident_reads`.
/// Skipped unless the parent set its environment variable.
#[test]
#[ignore = "run by collection_tail_keeps_every_byte_with_sealed_records_and_resident_reads in a child process"]
fn collection_tail_sealed_resident_child_v1() {
    use crate::expanded_deck_training_v1::record_envelope_v1::is_sealed_v1;
    if std::env::var(SEALED_RESIDENT_CHILD_ENV_V1).as_deref() != Ok("1") {
        return;
    }
    for workers in [2, 12] {
        let games = if workers > 2 { workers + 1 } else { 2 };
        let mut serial = tail_config_v1(
            &format!("tail-sealed-resident-{workers}"),
            3,
            games,
            workers,
            workers,
        );
        serial.record_envelope = crate::expanded_deck_training_v1::RecordEnvelopeV1::Zstd1;
        serial.verified_reads = VerifiedReadsV1::Resident;
        assert_tailed_run_matches_v1(
            &format!("sealed and resident, {workers} workers"),
            &serial,
            2,
            |serial_result| {
                assert!(verified_bytes_cache_v1::global_v1().resident_v1());
                let checkpoint = iteration_checkpoint_v1(serial_result, 0);
                assert!(is_sealed_v1(&fs::read(checkpoint.path).unwrap()));
            },
        );
    }
}

/// Invariants 2 and 3 for every step the key defers or runs ahead: the
/// collection's validation (a trajectory changed after the collection), the
/// publication of `update-input.json`, the publication of
/// `update-command.json`, and the update itself failing after its input read
/// and preparation, before its learner. Each fails the run with both keys
/// with the error of the run with neither, publishes exactly the documents
/// that run publishes (no update directory in any case), commits nothing and
/// leaves nothing of the next iteration. The speculative update, which ran
/// ahead of its held helper, wrote and printed nothing before the helper
/// failed (its held output dropped unwritten); after its own failure, its
/// held lines are written once both documents are durable, as the run with
/// neither key writes them. A resume reaches the serial run's states.
#[test]
fn collection_tail_failed_steps_fail_like_the_serial_run_and_commit_nothing_later() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailureV1 {
        Validation,
        UpdateInput,
        UpdateCommand,
        BeforeLearner,
    }
    let serial = tail_config_v1("tail-failures", 2, 2, 2, 2);
    let tailed = tailed_v1(&serial);
    let directory = serial.output_directory.clone();
    let _ = fs::remove_dir_all(&directory);
    let reference = run_tailed_v1(&serial, None, || {}).0.unwrap();
    let reference_states = after_states_v1(&reference);
    fs::remove_dir_all(&directory).unwrap();
    let attempt = iteration_directory_v1(&directory, 0).join("attempt-000000");
    let second = iteration_directory_v1(&directory, 1);
    let trajectory = attempt.join("collect").join("episode-0000.json");
    for failure in [
        FailureV1::Validation,
        FailureV1::UpdateInput,
        FailureV1::UpdateCommand,
        FailureV1::BeforeLearner,
    ] {
        let mut errors = Vec::new();
        for config in [&serial, &tailed] {
            let tailed_run = config.collection_tail_v1;
            let case = format!(
                "{failure:?} {}",
                if tailed_run { "tailed" } else { "serial" }
            );
            let (arm, log) = held_update_log_v1(&directory);
            let changed = trajectory.clone();
            let (failed, counts) = run_tailed_v1(config, None, move || {
                if tailed_run {
                    arm();
                }
                match failure {
                    FailureV1::Validation => after_collection_for_test_v1(move |index| {
                        if index == 0 {
                            let mut bytes = fs::read(&changed).unwrap();
                            let middle = bytes.len() / 2;
                            bytes[middle] ^= 1;
                            fs::write(&changed, bytes).unwrap();
                        }
                    }),
                    FailureV1::UpdateInput => fail_next_publish_for_test_v1("update-input.json"),
                    FailureV1::UpdateCommand => {
                        fail_next_publish_for_test_v1("update-command.json")
                    }
                    FailureV1::BeforeLearner => {
                        crate::expanded_deck_training_v1::fail_before_learner_for_test_v1()
                    }
                }
            });
            let error = failed.unwrap_err();
            let expected = match failure {
                FailureV1::Validation => "hash differs",
                FailureV1::UpdateInput | FailureV1::UpdateCommand => "injected failure",
                FailureV1::BeforeLearner => "before its learner",
            };
            assert!(error.contains(expected), "{case}: {error}");
            assert_eq!(counts, CountsV1::default(), "{case}: nothing was admitted");
            assert_eq!(committed_v1(&directory), 0, "{case}");
            assert!(!directory.join("completion.json").exists(), "{case}");
            assert_eq!(
                attempt.join("update-input.json").exists(),
                matches!(failure, FailureV1::UpdateCommand | FailureV1::BeforeLearner),
                "{case}"
            );
            assert_eq!(
                attempt.join("update-command.json").exists(),
                failure == FailureV1::BeforeLearner,
                "{case}"
            );
            assert!(
                !attempt.join("update").exists(),
                "{case}: the update wrote before its documents were durable"
            );
            assert!(!second.exists(), "{case}: the next iteration left a trace");
            {
                let log = log.lock().unwrap();
                if !tailed_run {
                    assert!(
                        log.written.is_empty()
                            && log.released.is_empty()
                            && log.discarded.is_empty(),
                        "{case}: {log:?}"
                    );
                } else if failure == FailureV1::BeforeLearner {
                    assert!(log.discarded.is_empty(), "{case}: {log:?}");
                    assert_eq!(log.released.len(), 1, "{case}: {log:?}");
                    let (held, commands) = log.released[0];
                    assert!(held >= 1 && commands == 1, "{case}: {log:?}");
                    assert_eq!(log.written.len(), 1, "{case}: {log:?}");
                } else {
                    assert!(
                        log.written.is_empty() && log.released.is_empty(),
                        "{case}: the speculative update printed: {log:?}"
                    );
                    assert_eq!(log.discarded.len(), 1, "{case}: {log:?}");
                }
            }
            if failure != FailureV1::Validation {
                let (resumed, _) = run_tailed_v1(config, None, || {});
                let resumed = resumed.unwrap_or_else(|error| panic!("{case}: resume: {error}"));
                assert_eq!(resumed["complete"], true, "{case}");
                assert_eq!(after_states_v1(&resumed), reference_states, "{case}");
            }
            errors.push(error);
            fs::remove_dir_all(&directory).unwrap();
        }
        assert_eq!(errors[0], errors[1], "{failure:?}");
    }
}

/// Invariant 4 (the invocation limit): a limited run with both keys stops at
/// the update the run with neither stops at, starts nothing past it, and
/// resumes to the serial run's tree.
#[test]
fn collection_tail_stops_at_the_invocation_limit_like_the_serial_run() {
    let serial = tail_config_v1("tail-limit", 3, 2, 2, 2);
    let tailed = tailed_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let serial_result = run_tailed_v1(&serial, None, || {}).0.unwrap();
    assert_eq!(serial_result["complete"], true);
    fs::rename(&directory, &reference).unwrap();
    let (limited, counts) = run_tailed_v1(&tailed, Some(2), || {});
    let limited = limited.unwrap();
    assert_eq!(limited["complete"], false);
    assert_eq!(limited["completed_iterations"], 2);
    assert_eq!(
        counts,
        CountsV1 {
            prefetched: 1,
            admitted: 2,
            restored: 1,
        }
    );
    assert!(
        !iteration_directory_v1(&directory, 2).exists(),
        "a collection was started past the limit"
    );
    assert_same_tree_v1("limited", &directory, &reference, false);
    let (resumed, counts) = run_tailed_v1(&tailed, None, || {});
    assert_eq!(resumed.unwrap()["complete"], true);
    // The resume's last collection is its first: not prefetched, and its
    // learner may be served by the loaded-source cache its prefix check
    // filled.
    assert_eq!((counts.prefetched, counts.admitted), (0, 1));
    assert_same_tree_v1("resumed", &directory, &reference, true);
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// Invariant 2 (crash safety), every crash point. The run with both keys
/// starts the same publications as the run with neither (counted the same
/// way), with every helper held until its update has read and prepared its
/// inputs and every collection held at its gate with a played game until its
/// update is committed. A crash just before or just after any publication
/// leaves only files the uninterrupted serial run publishes, with the same
/// bytes apart from timing telemetry and the keys in the plan, plus staging
/// debris; never an update directory before its `update-command.json`; and
/// committed iterations that are a prefix. Resuming reaches the serial run's
/// state, step, loss and games at every iteration, and its very tree when the
/// resume made no new attempt.
#[test]
#[ignore = "slow: one crashed run and its resume per crash point; run before shipping a change to the collection tail"]
fn collection_tail_crash_at_any_publication_resumes_to_the_serial_result() {
    let serial = tail_config_v1("tail-crash-points", 3, 2, 2, 2);
    let tailed = tailed_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("uninterrupted");
    let root = publication_root_v1(&directory);
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let ((serial_result, _), serial_publications, crashed) =
        with_crash_point_v1(&root, None, || run_tailed_v1(&serial, None, || {}));
    assert!(!crashed);
    let serial_result = serial_result.unwrap();
    assert_eq!(serial_result["complete"], true);
    let reference_states = after_states_v1(&serial_result);
    let reference_content = run_content_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let holds = || {
        hold_helper_for_test_v1();
        hold_at_gate_for_test_v1();
    };
    let ((counted, counts), publications, crashed) =
        with_crash_point_v1(&root, None, || run_tailed_v1(&tailed, None, holds));
    assert!(!crashed);
    assert_eq!(counted.unwrap()["complete"], true);
    assert_eq!(
        counts,
        CountsV1 {
            prefetched: 2,
            admitted: 3,
            restored: 2,
        }
    );
    assert_same_tree_v1("uninterrupted", &directory, &reference, true);
    fs::remove_dir_all(&directory).unwrap();
    assert_eq!(publications, serial_publications);
    assert_eq!(publications, 3 * 9 + 2, "{publications} publications");
    for index in 0..publications {
        for point in [CrashPointV1::BeforeFinal, CrashPointV1::AfterFinal] {
            let case = format!("publication {index} {point:?}");
            let ((crashed_run, _), _, crashed) =
                with_crash_point_v1(&root, Some((index, point)), || {
                    run_tailed_v1(&tailed, None, holds)
                });
            assert!(crashed, "{case}: never reached");
            let error = crashed_run.unwrap_err();
            assert!(error.contains("injected crash"), "{case}: {error}");
            assert_same_tree_v1(&case, &directory, &reference, false);
            let committed = committed_v1(&directory);
            for iteration in 0..3 {
                let path = iteration_directory_v1(&directory, iteration);
                if iteration > 0 {
                    assert!(
                        !path.exists()
                            || iteration_directory_v1(&directory, iteration - 1)
                                .join("complete.json")
                                .exists(),
                        "{case}: iteration {iteration} exists before iteration {} is committed",
                        iteration - 1
                    );
                }
                assert_eq!(
                    path.join("complete.json").exists(),
                    iteration < committed,
                    "{case}: the committed iterations are not a prefix"
                );
                if path.exists() {
                    for attempt in fs::read_dir(&path).unwrap() {
                        let attempt = attempt.unwrap().path();
                        assert!(
                            !attempt.join("update").exists()
                                || attempt.join("update-command.json").exists(),
                            "{case}: an update directory without its command"
                        );
                    }
                }
            }
            let (resumed, _) = run_tailed_v1(&tailed, None, holds);
            let resumed = resumed.unwrap_or_else(|error| panic!("{case}: resume failed: {error}"));
            assert_eq!(resumed["complete"], true, "{case}");
            assert_eq!(after_states_v1(&resumed), reference_states, "{case}");
            assert_eq!(run_content_v1(&resumed), reference_content, "{case}");
            for field in ["state_sha256", "adam_step"] {
                assert_eq!(
                    resumed["actual_identity"][field], serial_result["actual_identity"][field],
                    "{case}: {field}"
                );
            }
            let new_attempt = (0..3).any(|iteration| {
                iteration_directory_v1(&directory, iteration)
                    .join("attempt-000001")
                    .exists()
            });
            if !new_attempt {
                assert_same_tree_v1(&format!("{case} resumed"), &directory, &reference, true);
            }
            fs::remove_dir_all(&directory).unwrap();
        }
    }
    fs::remove_dir_all(&reference).unwrap();
}
