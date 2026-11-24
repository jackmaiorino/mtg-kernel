//! Update tail overlap (`update_tail_overlap_v1`): the key's wire form and
//! admission, and that a run with it publishes the bytes, reads its
//! boundaries, commits, fails and stops as the same run without it does,
//! leaving no trace of a collection, on disk or in its log, before the
//! update before it is committed.
//!
//! Every run of a comparison uses the same output directory (the reference
//! tree is moved aside), so documents that name it compare equal.

use super::follow_tests::{
    chain_base_v1, checkpoint_pin_v1, followed_v1, fresh_run_v1, segment_v1, without_telemetry_v1,
    TIMED,
};
use super::*;
use crate::durable_publication_v1::crash_points_for_test_v1::{with_crash_point_v1, CrashPointV1};
use crate::held_output_v1::HeldEventV1;
use crate::native_expanded_training_run_v1::follow_v1::FollowEndV1;
use crate::native_expanded_training_run_v1::update_tail_overlap_v1::{
    capture_refusal_v1, collected_for_test_v1, gate_opened_for_test_v1, hold_at_gate_for_test_v1,
    record_held_output_for_test_v1, refuse_captures_v1, LEDGER_STOP_REFUSAL_V1,
    VALIDATION_REFUSAL_V1,
};
use std::sync::{Arc, Mutex};

/// `parallel_two_iteration_config_v1` (two games an iteration, collected and
/// prepared on two workers, every opponent the current model) with `updates`
/// iterations, two or more.
pub(super) fn overlap_config_v1(label: &str, updates: usize) -> NativeExpandedTrainingRunV1 {
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
/// and test hooks as a new process does, after `before` on that thread. The
/// result, and how many of the run's collections its driver took from a
/// prefetch.
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

/// A run's plan without the keys: the members an overlapped run's plan has
/// more (`update_tail_overlap_v1`, and `collection_tail_v1` on top of it).
fn plan_without_key_v1(bytes: &[u8]) -> Value {
    let mut plan: Value = serde_json::from_slice(bytes).unwrap();
    let config = plan["config"].as_object_mut().unwrap();
    config.remove("update_tail_overlap_v1");
    config.remove("collection_tail_v1");
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
pub(super) fn assert_same_tree_v1(case: &str, got: &Path, want: &Path, same_files: bool) {
    assert_same_tree_except_v1(case, got, want, same_files, |_| false);
}

/// `assert_same_tree_v1`, where a file for which `may_lack` holds may also be
/// missing from either tree (it is compared when both have it).
fn assert_same_tree_except_v1(
    case: &str,
    got: &Path,
    want: &Path,
    same_files: bool,
    may_lack: impl Fn(&Path) -> bool,
) {
    let wanted = files_under_v1(want);
    let found = files_under_v1(got);
    for (path, bytes) in &found {
        if staging_debris_v1(path) {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(expected) = wanted.get(path) else {
            assert!(may_lack(path), "{case}: unexpected file {}", path.display());
            continue;
        };
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
                staging_debris_v1(path) || found.contains_key(path) || may_lack(path),
                "{case}: {} missing",
                path.display()
            );
        }
    }
}

/// The after-state of every iteration of a run result, from its update
/// records.
pub(super) fn after_states_v1(result: &Value) -> Vec<Value> {
    (0..result["iterations"].as_array().unwrap().len())
        .map(|index| {
            let update = parse_pin(&iteration_receipt_v1(result, index), "update").unwrap();
            read_pin(&update).unwrap()["after_state_sha256"].clone()
        })
        .collect()
}

/// What a run's iterations hold apart from the paths they name: each
/// update's states, step and loss, and each game's decisions (by digest)
/// and terminal. Read through the result's pins.
pub(super) fn run_content_v1(result: &Value) -> Vec<Value> {
    (0..result["iterations"].as_array().unwrap().len())
        .map(|index| {
            let receipt = iteration_receipt_v1(result, index);
            let update = read_pin(&parse_pin(&receipt, "update").unwrap()).unwrap();
            let collection = read_pin(&parse_pin(&receipt, "collection").unwrap()).unwrap();
            let trajectories: Vec<PinnedFileV1> =
                serde_json::from_value(collection["trajectories"].clone()).unwrap();
            let games: Vec<Value> = trajectories
                .iter()
                .map(|pin| {
                    let game = read_pin_bounded(pin, ARTIFACT_CAP).unwrap();
                    json!({"decisions": digest(&serde_json::to_vec(&game["decisions"]).unwrap()),
                        "terminal": game["terminal"]})
                })
                .collect();
            json!({"before": update["before_state_sha256"], "after": update["after_state_sha256"],
                "adam_step": update["adam_step"], "loss": update["loss"], "games": games})
        })
        .collect()
}

/// How many iterations of the run in `directory` are committed.
pub(super) fn committed_v1(directory: &Path) -> usize {
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

/// Iteration `index`'s directory in the run in `directory`.
pub(super) fn iteration_directory_v1(directory: &Path, index: usize) -> PathBuf {
    directory.join("iterations").join(format!("{index:06}"))
}

/// The root publications of the run in `directory` are counted under: its
/// canonical path, which exists before the run only up to its parent.
pub(super) fn publication_root_v1(directory: &Path) -> PathBuf {
    directory
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join(directory.file_name().unwrap())
}

/// What the held output of a run's collections did (`held_output_v1`).
#[derive(Debug, Default)]
struct HeldLogV1 {
    /// Every line written, with whether the update before the line's
    /// iteration was committed (its `complete.json` existed) when it was.
    written: Vec<(String, bool)>,
    /// The lines each collection held when its output was released.
    released: Vec<usize>,
    /// The lines each collection held when its output was dropped.
    discarded: Vec<usize>,
}

/// The iteration a collection's progress line belongs to: the line ends
/// with the episode's id, and every id here holds `-iter-{index}`.
fn line_iteration_v1(line: &str) -> usize {
    let start = line.find("-iter-").unwrap_or_else(|| panic!("{line}")) + "-iter-".len();
    line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

/// The progress lines of iteration `index`'s collection.
fn progress_lines_v1(config: &NativeExpandedTrainingRunV1, index: usize) -> Vec<String> {
    let episodes = &config.iterations[index].episodes;
    episodes
        .iter()
        .enumerate()
        .map(|(slot, scheduled)| {
            format!(
                "collect parallel episode {}/{} {}",
                slot + 1,
                episodes.len(),
                scheduled.episode.id
            )
        })
        .collect()
}

/// A `before` for `run_counted_v1` that records the held output of every
/// collection the run starts, checked against the run in `directory`, and
/// with `hold` also holds every update at its handoff until the collection it
/// started waits at its gate with a played game (`hold_at_gate_for_test_v1`).
fn held_log_v1(
    directory: &Path,
    hold: bool,
) -> (impl FnOnce() + Send + 'static, Arc<Mutex<HeldLogV1>>) {
    let log = Arc::new(Mutex::new(HeldLogV1::default()));
    let sink = Arc::clone(&log);
    let directory = directory.to_path_buf();
    let arm = move || {
        record_held_output_for_test_v1(Arc::new(move |event: HeldEventV1| {
            let mut log = sink.lock().unwrap();
            match event {
                HeldEventV1::Written(line) => {
                    let iteration = line_iteration_v1(&line);
                    let committed = iteration > 0
                        && iteration_directory_v1(&directory, iteration - 1)
                            .join("complete.json")
                            .exists();
                    log.written.push((line, committed));
                }
                HeldEventV1::Released { held } => log.released.push(held),
                HeldEventV1::Discarded { held } => log.discarded.push(held),
            }
        }));
        if hold {
            hold_at_gate_for_test_v1();
        }
    };
    (arm, log)
}

/// A failed run's held output: an overlapped run's collection had played a
/// game and waited at its gate (the hold), and its output was dropped
/// without a line written; a serial run has none.
fn assert_dropped_unwritten_v1(case: &str, log: &Mutex<HeldLogV1>, overlapped: bool) {
    let log = log.lock().unwrap();
    assert!(
        log.written.is_empty() && log.released.is_empty(),
        "{case}: {log:?}"
    );
    if overlapped {
        assert!(
            matches!(log.discarded.as_slice(), [held] if *held >= 1),
            "{case}: the collection held no played game: {log:?}"
        );
    } else {
        assert!(log.discarded.is_empty(), "{case}: {log:?}");
    }
}

/// An overlapped config that validation admits, for the refusal tests:
/// `schedule` with parallel collection and preparation.
fn admitted_overlap_v1() -> NativeExpandedTrainingRunV1 {
    let mut config = schedule();
    config.collection_workers = 2;
    config.preparation_workers = 2;
    config.update_tail_overlap_v1 = true;
    config.validate_v1().unwrap();
    config
}

/// Validation refuses the key with its own message once `change` is made to
/// an admitted overlapped config. It asserts the key's own message, so the
/// test of an exclusion fails when that exclusion is removed.
fn assert_refused_v1(change: impl FnOnce(&mut NativeExpandedTrainingRunV1)) {
    let mut config = admitted_overlap_v1();
    change(&mut config);
    assert_eq!(config.validate_v1().unwrap_err(), VALIDATION_REFUSAL_V1);
}

fn write_cpu_stat_v1(stat: &Path, usage_usec: u64) {
    fs::write(
        stat,
        format!("usage_usec {usage_usec}\nuser_usec {usage_usec}\nsystem_usec 0\n"),
    )
    .unwrap();
}

fn write_ledger_source_v1(source: &Path, stat: &Path, stop_at_usec: Option<u64>) {
    let mut document = json!({"schema":CHAIN_LEDGER_SOURCE_SCHEMA_V1,
        "chain":"overlap/chain","closed_usec":5_000,"cpu_stat":stat,"instance":2});
    if let Some(stop) = stop_at_usec {
        document["stop_at_usec"] = json!(stop);
    }
    fs::write(source, serde_json::to_vec(&document).unwrap()).unwrap();
}

/// Invariant: with the key absent the config has its bytes and identity (the
/// key is never serialized, also when written as false); true is serialized;
/// and no command carries it.
#[test]
fn update_tail_overlap_default_preserves_the_config_wire() {
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
    let parallel = parallel_two_iteration_config_v1("overlap-validation");
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
}

/// Validation exclusion: serial collection keeps its serial order.
#[test]
fn update_tail_overlap_is_refused_with_serial_collection() {
    assert_refused_v1(|config| config.collection_workers = 1);
}

/// Validation exclusion: a serial (unprepared) update keeps its serial order.
#[test]
fn update_tail_overlap_is_refused_with_a_serial_update() {
    assert_refused_v1(|config| config.preparation_workers = 1);
}

/// Validation exclusion: the CUDA update backend.
#[test]
fn update_tail_overlap_is_refused_with_the_cuda_update_backend() {
    assert_refused_v1(|config| {
        config.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 0 }
    });
}

/// Validation exclusion: line (b).
#[test]
fn update_tail_overlap_is_refused_with_line_b() {
    assert_refused_v1(|config| {
        config.line_b = Some(crate::expanded_deck_training_v1::LineBUpdateOptionsV1 {
            optimizer_mask: crate::expanded_deck_training_v1::OptimizerMaskV1::LineBHeadOnlyV1,
            teacher: None,
        })
    });
}

/// Validation exclusion: E supported-choice supervision.
#[test]
fn update_tail_overlap_is_refused_with_e_supported() {
    assert_refused_v1(|config| {
        config.e_supported = Some(ESupportedRunOptionsV1 {
            teacher: config.initial_source.clone(),
            treatment: true,
            prior_deck_indices: vec![0],
            selection_transition_cap: 1,
            slots: vec![Vec::new()],
        })
    });
}

/// Validation exclusion: SQ1 soft rollout-Q teaching.
#[test]
fn update_tail_overlap_is_refused_with_soft_q() {
    assert_refused_v1(|config| {
        config.soft_q = Some(SoftQRunOptionsV1 {
            arm: SoftQArmV1::Treatment,
            case_namespace: "overlap".into(),
            prior_deck_indices: vec![0],
            teacher_workers: 1,
            engineering_zero_treatment: false,
        })
    });
}

/// Validation exclusion: TP1 Stage B searched-game targets.
#[test]
fn update_tail_overlap_is_refused_with_search_targets() {
    assert_refused_v1(|config| {
        config.search_targets = Some(SearchTargetsRunOptionsV1 {
            packets_directory: std::env::temp_dir().join("overlap-packets"),
            bank_spec: config.initial_source.play_import.clone(),
            opponents: Vec::new(),
            coefficient: 1.0,
            denominator: 1,
            run: "overlap".into(),
            arm: "overlap".into(),
            seed: 1,
            update_offset: 0,
            wait_seconds: 1,
            allocation: json!({}),
            target: Default::default(),
            cap: Default::default(),
        })
    });
}

/// Validation exclusion: EX1 (and W5's search in the loop).
#[test]
fn update_tail_overlap_is_refused_with_ex1() {
    assert_refused_v1(|config| {
        config.ex1 = Some(Ex1RunOptionsV1 {
            run: "overlap".into(),
            update_offset: 0,
            coefficient: 1.0,
            cap: Default::default(),
            w5: None,
        })
    });
}

/// Validation exclusion: W5's native S learner.
#[test]
fn update_tail_overlap_is_refused_with_an_s_learner() {
    assert_refused_v1(|config| {
        config.initial_source.s_learner = Some(Box::new(
            crate::expanded_deck_training_v1::s_learner_v1::SLearnerSourceV1 {
                schema: "overlap".into(),
                initial: PinnedFileV1 {
                    path: std::env::temp_dir().join("overlap-s-initial.json"),
                    sha256: "b".repeat(64),
                },
            },
        ))
    });
}

/// Invariant 1 (bytes) and invariant 2 (no trace before the commit): a
/// three-update run with the key publishes the files of the same run without
/// it, with the same bytes: every command, trajectory and checkpoint byte for
/// byte, every receipt apart from its timing telemetry, and the plan apart
/// from the key. Every update but the last hands its successor's collection
/// over, held at its handoff until that collection has a played game waiting
/// at its gate. Each collection's held progress lines are written only once
/// the update before it is committed, every one exactly once, and none is
/// dropped. Every iteration stays in its first attempt, and the finished run
/// revalidates as complete.
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
    let (arm, log) = held_log_v1(&directory, true);
    let (result, prefetched) = run_counted_v1(&overlapped, None, arm);
    let result = result.unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(prefetched, 2, "updates 0 and 1 hand their successor over");
    assert_eq!(result["actual_identity"], serial_result["actual_identity"]);
    assert_eq!(result["source"], serial_result["source"]);
    assert_eq!(after_states_v1(&result), serial_states);
    assert_same_tree_v1("overlapped against serial", &directory, &reference, true);
    assert!(!directory.join("restarts.log").exists());
    for index in 0..3 {
        let mut names: Vec<String> = fs::read_dir(iteration_directory_v1(&directory, index))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["attempt-000000", "complete.json"]);
    }
    {
        let log = log.lock().unwrap();
        assert!(log.discarded.is_empty(), "{log:?}");
        assert_eq!(log.released.len(), 2, "{log:?}");
        assert!(
            log.released.iter().all(|&held| held >= 1),
            "a collection held no played game's line: {log:?}"
        );
        assert!(
            log.written.iter().all(|(_, committed)| *committed),
            "a line was written before the update before its collection was committed: {log:?}"
        );
        let mut written: Vec<String> = log.written.iter().map(|(line, _)| line.clone()).collect();
        written.sort();
        let mut expected: Vec<String> = (1..3)
            .flat_map(|index| progress_lines_v1(&serial, index))
            .collect();
        expected.sort();
        assert_eq!(written, expected);
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
/// trees hold the same bytes (`assert_same_tree_v1`). The overlapped run holds
/// every update at its handoff until its collection has played a game
/// (`hold_at_gate_for_test_v1`): the collection loaded the new checkpoint,
/// metadata and record, while the checkpoint was absent on disk, so only
/// from the pending publication in memory. `before_move` sees the serial
/// result while its tree is still in place. Leaves neither tree.
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
    let (result, prefetched) = run_counted_v1(&overlapped, None, hold_at_gate_for_test_v1);
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

/// The environment variable that runs one of this module's child tests in
/// its child process, with the child's role as its value.
const CHILD_ENV_V1: &str = "MTG_KERNEL_UPDATE_TAIL_OVERLAP_CHILD_V1";

/// The environment variable that names, for a child test, the config file
/// its parent wrote.
const CHILD_CONFIG_ENV_V1: &str = "MTG_KERNEL_UPDATE_TAIL_OVERLAP_CHILD_CONFIG_V1";

/// The environment variable that names, for a child test, where it leaves
/// its result for its parent.
const CHILD_RESULT_ENV_V1: &str = "MTG_KERNEL_UPDATE_TAIL_OVERLAP_CHILD_RESULT_V1";

/// Runs the ignored test `name` of this module alone in a new process of this
/// test binary, with `CHILD_ENV_V1` set to `role` and every variable of
/// `paths` set to its path, and asserts that it passed.
fn run_child_v1(name: &str, role: &str, paths: &[(&str, &Path)]) {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.envs(paths.iter().map(|&(variable, path)| (variable, path)));
    let output = command
        .args([
            format!("native_expanded_training_run_v1::tests::update_tail_overlap_tests::{name}")
                .as_str(),
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV_V1, role)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child {name} ({role}) failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Invariant 1 (bytes) in the modes the C-recipe chains run: a three-update
/// run with sealed records (`record_envelope: zstd1`) and resident reads
/// (`verified_reads: resident`) publishes with the key the bytes it publishes
/// without it. Resident reads are a one-way process switch and fill a
/// process cache, so each run has a process of its own
/// (`update_tail_overlap_sealed_resident_child_v1`): the serial run's cache
/// never serves the overlapped run, whose collections load each new
/// checkpoint while it is absent on disk (the gate hold), so only from the
/// pending publication. The fixture's root is named after the process, so
/// the parent makes the one config, with its sources, and gives both
/// children its file and their result paths.
#[test]
fn update_tail_overlap_keeps_every_byte_with_sealed_records_and_resident_reads() {
    use crate::expanded_deck_training_v1::record_envelope_v1::is_sealed_v1;
    let mut serial = overlap_config_v1("overlap-sealed-resident", 3);
    serial.record_envelope = crate::expanded_deck_training_v1::RecordEnvelopeV1::Zstd1;
    serial.verified_reads = VerifiedReadsV1::Resident;
    serial.validate_v1().unwrap();
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let config_path = directory.with_file_name("sealed-resident-config.json");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    fs::write(&config_path, serde_json::to_vec(&serial).unwrap()).unwrap();
    let run_child = |role: &str| -> Value {
        let result_path = directory.with_file_name(format!("result-{role}.json"));
        let _ = fs::remove_file(&result_path);
        run_child_v1(
            "update_tail_overlap_sealed_resident_child_v1",
            role,
            &[
                (CHILD_CONFIG_ENV_V1, &config_path),
                (CHILD_RESULT_ENV_V1, &result_path),
            ],
        );
        let result = serde_json::from_slice(&fs::read(&result_path).unwrap()).unwrap();
        fs::remove_file(&result_path).unwrap();
        result
    };
    let serial_result = run_child("serial");
    assert_eq!(serial_result["complete"], true);
    // The records are sealed. Read through the receipts' pins, so before
    // the tree moves.
    let checkpoint = iteration_checkpoint_v1(&serial_result, 0);
    assert!(is_sealed_v1(&fs::read(checkpoint.path).unwrap()));
    let serial_states = after_states_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let result = run_child("overlapped");
    assert_eq!(result["complete"], true);
    assert_eq!(result["actual_identity"], serial_result["actual_identity"]);
    assert_eq!(after_states_v1(&result), serial_states);
    assert_same_tree_v1("sealed and resident", &directory, &reference, true);
    assert!(!directory.join("restarts.log").exists());
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
    fs::remove_file(&config_path).unwrap();
}

/// A child of
/// `update_tail_overlap_keeps_every_byte_with_sealed_records_and_resident_reads`:
/// with role `serial` the run without the key, with role `overlapped` the
/// run with it, every update held at its handoff until its collection has
/// played a game. Each runs in a new process that reads resident, on the
/// config its parent wrote, and leaves its result where its parent said.
/// Skipped unless the parent set its role.
#[test]
#[ignore = "run by update_tail_overlap_keeps_every_byte_with_sealed_records_and_resident_reads in a child process"]
fn update_tail_overlap_sealed_resident_child_v1() {
    let Ok(role) = std::env::var(CHILD_ENV_V1) else {
        return;
    };
    let path = |variable: &str| PathBuf::from(std::env::var_os(variable).unwrap());
    let serial: NativeExpandedTrainingRunV1 =
        serde_json::from_slice(&fs::read(path(CHILD_CONFIG_ENV_V1)).unwrap()).unwrap();
    serial.validate_v1().unwrap();
    let (result, prefetched) = match role.as_str() {
        "serial" => run_counted_v1(&serial, None, || {}),
        "overlapped" => run_counted_v1(&overlapped_v1(&serial), None, hold_at_gate_for_test_v1),
        other => panic!("unknown role {other}"),
    };
    let result = result.unwrap_or_else(|error| panic!("{role} run: {error}"));
    assert_eq!(result["complete"], true, "{role}");
    let expected = if role == "serial" { 0 } else { 2 };
    assert_eq!(
        prefetched, expected,
        "{role}: collections taken from a prefetch"
    );
    assert!(verified_bytes_cache_v1::global_v1().resident_v1());
    fs::write(
        path(CHILD_RESULT_ENV_V1),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
}

/// A followed process (`follow_v1`) over two segments with the key publishes
/// the trees two fresh processes without it publish, with one collection
/// handed over in each segment.
#[test]
fn update_tail_overlap_followed_chain_publishes_the_serial_chains_trees() {
    let (serial_base, chain) = chain_base_v1("overlap-follow");
    let overlapped_base = overlapped_v1(&serial_base);
    let serial_chain = chain.with_file_name("chain-serial");
    let _ = fs::remove_dir_all(&chain);
    let _ = fs::remove_dir_all(&serial_chain);
    let first = segment_v1(&serial_base, &chain, 0, None);
    let first_result = fresh_run_v1(&first).unwrap();
    let second = segment_v1(
        &serial_base,
        &chain,
        1,
        Some(checkpoint_pin_v1(&first_result)),
    );
    fresh_run_v1(&second).unwrap();
    fs::rename(&chain, &serial_chain).unwrap();
    let spool = chain.join("spool");
    let before = collected_for_test_v1();
    let (ended, results) =
        followed_v1(&overlapped_base, &chain, 0, 2, &spool, &mut |_| {}).unwrap();
    assert_eq!(ended, FollowEndV1::Stopped);
    assert_eq!(results.len(), 2);
    assert_eq!(collected_for_test_v1() - before, 2, "one per segment");
    fs::remove_dir_all(&spool).unwrap();
    assert_same_tree_v1(
        "followed overlapped against fresh serial",
        &chain,
        &serial_chain,
        true,
    );
    fs::remove_dir_all(&chain).unwrap();
    fs::remove_dir_all(&serial_chain).unwrap();
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
    let third = iteration_directory_v1(&directory, 2);
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

/// Invariant 4 (the chain ledger) and the boundary readings. A test hook
/// advances the ledger's live counter inside every update's write-out (after
/// its `update.json`, before the driver checks it): update `k` takes it to
/// `1,000 (k + 1)`, so a reading taken at the handoff, before the write-out,
/// would be 1,000 lower than the serial one.
///
/// - Without a stop value, the serial and the overlapped runs commit the
///   same updates and record the same reading at every boundary, the
///   post-write-out value `5,000 + 1,000 (k + 1)`, the last boundary (with
///   no collection beside it) included; their trees are the same (the
///   readings are compared, not stripped as telemetry). So the stop rule,
///   applied to the overlapped run's readings, stops it at the update where
///   the serial run with that stop value stops.
/// - The overlapped run with a stop value in the ledger source is refused
///   before any work, with the key's message.
/// - A stop value put into the source during an overlapped run (in update
///   0's write-out) is refused at that boundary, before its receipt; the
///   collection it had started, with a played game at its gate, is abandoned
///   and leaves nothing, on disk or in the log.
#[test]
fn update_tail_overlap_reads_the_ledger_where_the_serial_run_does_and_refuses_its_stop() {
    let mut serial = overlap_config_v1("overlap-ledger", 3);
    let root = serial.output_directory.parent().unwrap().to_path_buf();
    let stat = root.join("cpu.stat");
    let source = root.join("ledger-source.json");
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
    // The counter advances in every update's write-out; with `stop`, the
    // source gains that stop value in update 0's write-out.
    let advancing = |stop_in_write_out: Option<u64>| {
        let (stat, source) = (stat.clone(), source.clone());
        move || {
            after_update_for_test_v1(move |index| {
                write_cpu_stat_v1(&stat, 1_000 * (index as u64 + 1));
                if index == 0 && stop_in_write_out.is_some() {
                    write_ledger_source_v1(&source, &stat, stop_in_write_out);
                }
            })
        }
    };
    let readings = |result: &Value| -> Vec<u64> {
        (0..result["iterations"].as_array().unwrap().len())
            .map(|index| {
                iteration_receipt_v1(result, index)["chain_ledger"]["usec"]
                    .as_u64()
                    .unwrap()
            })
            .collect()
    };
    let expected: Vec<u64> = (0..3).map(|k| 5_000 + 1_000 * (k + 1)).collect();
    // Without a stop value: the same readings and trees.
    write_cpu_stat_v1(&stat, 0);
    write_ledger_source_v1(&source, &stat, None);
    let (serial_result, _) = run_counted_v1(&serial, None, advancing(None));
    let serial_result = serial_result.unwrap();
    assert_eq!(serial_result["complete"], true);
    assert_eq!(readings(&serial_result), expected, "serial readings");
    fs::rename(&directory, &reference).unwrap();
    write_cpu_stat_v1(&stat, 0);
    let (arm_held, log) = held_log_v1(&directory, true);
    let arm_advancing = advancing(None);
    let (result, prefetched) = run_counted_v1(&overlapped, None, move || {
        arm_held();
        arm_advancing();
    });
    let result = result.unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(prefetched, 2);
    let overlapped_readings = readings(&result);
    assert_eq!(overlapped_readings, expected, "overlapped readings");
    for index in 0..3 {
        let receipt = iteration_receipt_v1(&result, index);
        assert!(receipt["driver_process_cpu_seconds"].as_f64().unwrap() > 0.0);
    }
    assert_same_tree_v1("ledger without a stop", &directory, &reference, true);
    assert!(log.lock().unwrap().discarded.is_empty());
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
    // A stop value reached at boundary 1 (5,000 + 2,000): the stop rule on
    // the overlapped run's readings stops it after update 1, and so does the
    // serial run with that stop value.
    let stop = 7_000;
    let rule_stop = overlapped_readings
        .iter()
        .position(|&usec| usec >= stop)
        .unwrap();
    assert_eq!(rule_stop, 1);
    write_cpu_stat_v1(&stat, 0);
    write_ledger_source_v1(&source, &stat, Some(stop));
    let (stopped, _) = run_counted_v1(&serial, None, advancing(None));
    let stopped = stopped.unwrap();
    assert_eq!(stopped["complete"], false);
    assert_eq!(stopped["completed_iterations"], rule_stop + 1);
    assert_eq!(committed_v1(&directory), rule_stop + 1);
    fs::remove_dir_all(&directory).unwrap();
    // The overlapped run with that stop value is refused before any work.
    write_cpu_stat_v1(&stat, 0);
    let (refused, prefetched) = run_counted_v1(&overlapped, None, advancing(None));
    assert_eq!(refused.unwrap_err(), LEDGER_STOP_REFUSAL_V1);
    assert_eq!(prefetched, 0);
    assert!(!directory.exists(), "a refused run did work");
    // A stop value put into the source during the overlapped run is refused
    // at the boundary that reads it, before its receipt.
    write_cpu_stat_v1(&stat, 0);
    write_ledger_source_v1(&source, &stat, None);
    let (arm_held, log) = held_log_v1(&directory, true);
    let arm_advancing = advancing(Some(stop));
    let (refused, prefetched) = run_counted_v1(&overlapped, None, move || {
        arm_held();
        arm_advancing();
    });
    assert_eq!(refused.unwrap_err(), LEDGER_STOP_REFUSAL_V1);
    assert_eq!(prefetched, 0);
    assert_eq!(committed_v1(&directory), 0);
    assert!(update_published_v1(&directory, 0));
    assert!(
        !iteration_directory_v1(&directory, 1).exists(),
        "the abandoned collection left a trace"
    );
    assert_dropped_unwritten_v1("stop put in during the run", &log, true);
    fs::remove_dir_all(&directory).unwrap();
}

/// Sol 1 of the second review (diagnostic captures): a run with the key is
/// refused while any of the five capture variables is set, with
/// a message that names that variable, before any work. No run directory
/// and no collection exist, so no collection can write a capture and then be
/// abandoned. The check admits the same run without the key. The variables
/// are set in a child process (`update_tail_overlap_capture_child_v1`), so
/// no other test sees them.
#[test]
fn update_tail_overlap_is_refused_while_a_diagnostic_capture_is_on() {
    run_child_v1("update_tail_overlap_capture_child_v1", "capture", &[]);
}

/// The child of `update_tail_overlap_is_refused_while_a_diagnostic_capture_is_on`.
/// Skipped unless the parent set its role.
#[test]
#[ignore = "run by update_tail_overlap_is_refused_while_a_diagnostic_capture_is_on in a child process"]
fn update_tail_overlap_capture_child_v1() {
    if std::env::var(CHILD_ENV_V1).as_deref() != Ok("capture") {
        return;
    }
    let serial = overlap_config_v1("overlap-capture", 2);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let capture = directory.with_file_name("capture.json");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_file(&capture);
    assert_eq!(refuse_captures_v1(&overlapped), Ok(()));
    // Listed here, not taken from `CAPTURE_VARIABLES_V1`, so dropping one
    // from the refusal fails this test.
    let variables = [
        "MTG_KERNEL_FAST_ACTOR_REJECTION_CAPTURE",
        "MTG_KERNEL_V3_ACTION_ERROR_CAPTURE",
        "MTG_KERNEL_V3_SCORING_ERROR_CAPTURE",
        "MTG_V4_SEARCH_FAILURE_STATE",
        "MTG_V4_SEARCH_BOUNDARY_FAILURE_STATE",
    ];
    for variable in variables {
        assert!(std::env::var_os(variable).is_none(), "{variable} was set");
        std::env::set_var(variable, &capture);
        let (refused, prefetched) = run_counted_v1(&overlapped, None, || {});
        let error = refused.unwrap_err();
        assert_eq!(error, capture_refusal_v1(variable));
        assert!(error.contains(variable), "{error}");
        assert_eq!(prefetched, 0, "{variable}");
        assert!(!directory.exists(), "{variable}: a refused run did work");
        assert!(!capture.exists(), "{variable}: a capture was written");
        assert_eq!(
            refuse_captures_v1(&serial),
            Ok(()),
            "{variable}: without the key"
        );
        std::env::remove_var(variable);
    }
}

/// Neon 2 of the second review: a stop value put in the ledger source after
/// admission is refused at the driver's startup read of it, before any
/// iteration. Here a hook puts it in right after admission. For a fresh run
/// no iteration's directory exists afterwards (the boundary refusal would
/// come only after iteration 0's update). For a resumed run whose recorded
/// boundary is at the stop value, the run is refused instead of ending
/// incomplete without an error, and nothing past that boundary exists.
#[test]
fn update_tail_overlap_refuses_a_ledger_stop_put_in_before_its_startup_read() {
    let mut serial = overlap_config_v1("overlap-ledger-startup", 3);
    let root = serial.output_directory.parent().unwrap().to_path_buf();
    let stat = root.join("cpu-startup.stat");
    let source = root.join("ledger-source-startup.json");
    serial.chain_ledger = Some(ChainLedgerOptionsV1 {
        source: source.clone(),
    });
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let _ = fs::remove_dir_all(&directory);
    let put_stop = |stop: u64| {
        let (source, stat) = (source.clone(), stat.clone());
        move || {
            after_admission_for_test_v1(move || write_ledger_source_v1(&source, &stat, Some(stop)))
        }
    };
    write_cpu_stat_v1(&stat, 1_000);
    write_ledger_source_v1(&source, &stat, None);
    let (refused, prefetched) = run_counted_v1(&overlapped, None, put_stop(1 << 40));
    assert_eq!(refused.unwrap_err(), LEDGER_STOP_REFUSAL_V1);
    assert_eq!(prefetched, 0);
    assert!(
        !iteration_directory_v1(&directory, 0).exists(),
        "an iteration ran before the refusal"
    );
    fs::remove_dir_all(&directory).unwrap();
    // One committed iteration, whose boundary records 5,000 + 1,000.
    write_ledger_source_v1(&source, &stat, None);
    let (first, _) = run_counted_v1(&overlapped, Some(1), || {});
    let first = first.unwrap();
    assert_eq!(first["completed_iterations"], 1);
    assert_eq!(
        iteration_receipt_v1(&first, 0)["chain_ledger"]["usec"],
        6_000
    );
    let (refused, prefetched) = run_counted_v1(&overlapped, None, put_stop(6_000));
    assert_eq!(refused.unwrap_err(), LEDGER_STOP_REFUSAL_V1);
    assert_eq!(prefetched, 0);
    assert_eq!(committed_v1(&directory), 1);
    assert!(!iteration_directory_v1(&directory, 1).exists());
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_file(&source).unwrap();
    fs::remove_file(&stat).unwrap();
}

/// Neon 3 of the second review: when iteration 1's directory already exists
/// as the gate opens (a hook makes it in update 0's write-out), the
/// collection's held output is dropped unwritten, with its played game: the
/// driver runs iteration 1 itself, as without the key, and prints its own
/// lines. The run publishes the serial run's tree.
#[test]
fn update_tail_overlap_drops_the_held_output_of_a_collection_whose_directory_is_not_new() {
    let serial = overlap_config_v1("overlap-not-new", 2);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let reference = directory.with_file_name("run-serial");
    let _ = fs::remove_dir_all(&directory);
    let _ = fs::remove_dir_all(&reference);
    let (serial_result, _) = run_counted_v1(&serial, None, || {});
    assert_eq!(serial_result.unwrap()["complete"], true);
    fs::rename(&directory, &reference).unwrap();
    let (arm_held, log) = held_log_v1(&directory, true);
    let made = directory.clone();
    let (result, prefetched) = run_counted_v1(&overlapped, None, move || {
        arm_held();
        after_update_for_test_v1(move |index| {
            if index == 0 {
                fs::create_dir(iteration_directory_v1(&made, 1)).unwrap();
            }
        });
    });
    assert_eq!(result.unwrap()["complete"], true);
    assert_eq!(prefetched, 0, "the driver ran iteration 1 itself");
    assert_same_tree_v1("a directory that was not new", &directory, &reference, true);
    assert_dropped_unwritten_v1("a directory that was not new", &log, true);
    fs::remove_dir_all(&directory).unwrap();
    fs::remove_dir_all(&reference).unwrap();
}

/// What a run printed and when its collections' gates opened, in order.
type CommitEventsV1 = Arc<Mutex<Vec<String>>>;

/// A `before` for a run on a new thread that records into `events` every
/// iteration's stdout line (`stdout {index}`, or, with `failing`, a panic as
/// a failed `println!` makes at that iteration's line), every gate's opening
/// (`gate {index}`) and every release of a collection's held output
/// (`released`), and holds every update at its handoff until its collection
/// has played a game.
fn commit_events_v1(
    events: &CommitEventsV1,
    failing: Option<usize>,
) -> impl FnOnce() + Send + 'static {
    let (lines, gates, held) = (Arc::clone(events), Arc::clone(events), Arc::clone(events));
    move || {
        stdout_for_test_v1(move |index, line| {
            assert!(
                line.contains(&format!("\"completed_iteration\":{index}")),
                "{line}"
            );
            if failing == Some(index) {
                panic!("failed printing to stdout: the test closed it");
            }
            lines.lock().unwrap().push(format!("stdout {index}"));
        });
        gate_opened_for_test_v1(move |index| gates.lock().unwrap().push(format!("gate {index}")));
        record_held_output_for_test_v1(Arc::new(move |event: HeldEventV1| match event {
            HeldEventV1::Released { .. } => held.lock().unwrap().push("released".into()),
            HeldEventV1::Discarded { held: count } => {
                held.lock().unwrap().push(format!("discarded {count}"))
            }
            HeldEventV1::Written(_) => {}
        }));
        hold_at_gate_for_test_v1();
    }
}

/// Sol 2 of the second review (the stdout barrier): each collection's gate
/// opens only after the stdout line of the iteration before it, and its held
/// output is written only after that. When printing the line of iteration 0
/// fails (as `println!` does, by panicking), the run fails like the run
/// without the key: iteration 0 is committed, and nothing of iteration 1
/// exists, on disk or in the log.
#[test]
fn update_tail_overlap_opens_a_gate_only_after_the_stdout_line_before_it() {
    let serial = overlap_config_v1("overlap-stdout", 3);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let _ = fs::remove_dir_all(&directory);
    let events = CommitEventsV1::default();
    let (result, prefetched) = run_counted_v1(&overlapped, None, commit_events_v1(&events, None));
    assert_eq!(result.unwrap()["complete"], true);
    assert_eq!(prefetched, 2);
    assert_eq!(
        *events.lock().unwrap(),
        ["stdout 0", "gate 1", "released", "stdout 1", "gate 2", "released", "stdout 2"]
    );
    fs::remove_dir_all(&directory).unwrap();
    for (case, config) in [("serial", &serial), ("overlapped", &overlapped)] {
        let events = CommitEventsV1::default();
        let before = commit_events_v1(&events, Some(0));
        let config = config.clone();
        let failed = std::thread::spawn(move || {
            before();
            run_native_expanded_training_v1(&config, None)
        })
        .join();
        assert!(
            failed.is_err(),
            "{case}: the failed stdout line did not fail the run"
        );
        assert_eq!(committed_v1(&directory), 1, "{case}");
        assert!(
            !iteration_directory_v1(&directory, 1).exists(),
            "{case}: iteration 1 left a trace"
        );
        let events = events.lock().unwrap();
        if case == "serial" {
            assert!(events.is_empty(), "{case}: {events:?}");
        } else {
            assert!(
                matches!(events.as_slice(), [only] if only.starts_with("discarded ") && only != "discarded 0"),
                "{case}: the collection's output was not dropped with a played game: {events:?}"
            );
        }
        drop(events);
        fs::remove_dir_all(&directory).unwrap();
    }
}

/// Two trees of one crash case, each resumed to completion, one with the
/// key (`got`) and one without (`want`): each holds every file of the other,
/// with the same bytes as `assert_same_tree_v1` compares them, apart from
/// trajectories of an attempt whose collection neither tree published. The
/// two workers' trajectory publications race, so which of them a crash
/// interrupts, and so which trajectories an interrupted attempt holds, can
/// differ between two runs crashed at the same publication; a trajectory
/// both trees hold has the same bytes in both.
fn assert_same_resumed_tree_v1(case: &str, got: &Path, want: &Path) {
    let interrupted_trajectory = |path: &Path| {
        let name = path.file_name().unwrap().to_str().unwrap();
        let collect = path.parent().unwrap();
        name.starts_with("episode-")
            && collect.file_name().is_some_and(|name| name == "collect")
            && !got.join(collect).join("collection.json").exists()
            && !want.join(collect).join("collection.json").exists()
    };
    assert_same_tree_except_v1(
        &format!("{case} resumed against the serial run crashed there and resumed"),
        got,
        want,
        true,
        interrupted_trajectory,
    );
}

/// Invariant 3 (failures of the deferred write): a failed publication of the
/// first update's checkpoint, and of its `update.json`, fails the overlapped
/// run with the serial run's error. Nothing is committed, and nothing of the
/// next iteration exists, not even its directory, though its collection had
/// started (the handoff comes before the checkpoint's first write) and held a
/// played game at its gate: its output was dropped unwritten. A resume of the
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
    let update = iteration_directory_v1(&directory, 0)
        .join("attempt-000000")
        .join("update");
    let second = iteration_directory_v1(&directory, 1);
    // Iteration 0's publications in the order they start, with or without the
    // key: the plan (0), the collection's command (1), two trajectories (2
    // and 3), the collection (4), the update's input (5) and command (6),
    // the checkpoint (7) and the update (8).
    for (publication, name, checkpoint_published) in
        [(7usize, "checkpoint.json", false), (8, "update.json", true)]
    {
        let mut errors = Vec::new();
        for config in [&serial, &overlapped] {
            let overlapped_run = config.update_tail_overlap_v1;
            let case = format!(
                "{name} {}",
                if overlapped_run {
                    "overlapped"
                } else {
                    "serial"
                }
            );
            let (arm, log) = held_log_v1(&directory, true);
            let ((failed, prefetched), _, crashed) = with_crash_point_v1(
                &root,
                Some((publication, CrashPointV1::BeforeFinal)),
                || run_counted_v1(config, None, arm),
            );
            assert!(crashed, "{case}: never reached");
            let error = failed.unwrap_err();
            assert!(error.contains("injected crash"), "{case}: {error}");
            assert_eq!(prefetched, 0, "{case}");
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
                "{case}: the next iteration left a trace before the update was committed"
            );
            assert_dropped_unwritten_v1(&case, &log, overlapped_run);
            if checkpoint_published && overlapped_run {
                let (resumed, _) = run_counted_v1(config, None, || {});
                let resumed = resumed.unwrap_or_else(|error| panic!("{case}: resume: {error}"));
                assert_eq!(resumed["complete"], true, "{case}");
                assert_eq!(after_states_v1(&resumed), reference_states, "{case}");
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

/// Invariants 2 and 3 for every deferred step after the checkpoint's write:
/// the update's own crash point (its checkpoint is published and read back,
/// `update.json` is not), a revalidation that refuses a trajectory changed
/// after the update, and a failed `complete.json`. Each fails the overlapped
/// run with the serial run's error and commits nothing. Nothing of the next
/// iteration exists, not even its directory, though its collection held a
/// played game at its gate the whole time; its output was dropped unwritten.
/// A resume reaches the serial run's states; after the failed `complete.json`
/// it commits the published update as it is, without running it again.
#[test]
fn update_tail_overlap_failed_deferred_steps_fail_like_the_serial_run_and_leave_nothing_later() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailureV1 {
        UpdateCrashPoint,
        Revalidation,
        Receipt,
    }
    let serial = overlap_config_v1("overlap-deferred-failures", 2);
    let overlapped = overlapped_v1(&serial);
    let directory = serial.output_directory.clone();
    let _ = fs::remove_dir_all(&directory);
    let reference = fresh_run_v1(&serial).unwrap();
    let reference_states = after_states_v1(&reference);
    fs::remove_dir_all(&directory).unwrap();
    let first = iteration_directory_v1(&directory, 0);
    let second = iteration_directory_v1(&directory, 1);
    let trajectory = first
        .join("attempt-000000")
        .join("collect")
        .join("episode-0000.json");
    for failure in [
        FailureV1::UpdateCrashPoint,
        FailureV1::Revalidation,
        FailureV1::Receipt,
    ] {
        let mut errors = Vec::new();
        for config in [&serial, &overlapped] {
            let overlapped_run = config.update_tail_overlap_v1;
            let case = format!(
                "{failure:?} {}",
                if overlapped_run {
                    "overlapped"
                } else {
                    "serial"
                }
            );
            let (arm, log) = held_log_v1(&directory, true);
            let changed = trajectory.clone();
            let (failed, prefetched) = run_counted_v1(config, None, move || {
                arm();
                match failure {
                    FailureV1::UpdateCrashPoint => {
                        crate::expanded_deck_training_v1::arm_crash_point_for_test_v1()
                    }
                    FailureV1::Revalidation => after_update_for_test_v1(move |_| {
                        let mut bytes = fs::read(&changed).unwrap();
                        let middle = bytes.len() / 2;
                        bytes[middle] ^= 1;
                        fs::write(&changed, bytes).unwrap();
                    }),
                    FailureV1::Receipt => fail_next_publish_for_test_v1("complete.json"),
                }
            });
            let error = failed.unwrap_err();
            let expected = match failure {
                FailureV1::UpdateCrashPoint => "crash point",
                FailureV1::Revalidation => "hash differs",
                FailureV1::Receipt => "injected failure",
            };
            assert!(error.contains(expected), "{case}: {error}");
            assert_eq!(prefetched, 0, "{case}");
            assert_eq!(
                committed_v1(&directory),
                0,
                "{case}: an update was committed"
            );
            assert!(!directory.join("completion.json").exists(), "{case}");
            assert_eq!(
                update_published_v1(&directory, 0),
                failure != FailureV1::UpdateCrashPoint,
                "{case}"
            );
            assert!(
                !second.exists(),
                "{case}: the next iteration left a trace before the update was committed"
            );
            assert_dropped_unwritten_v1(&case, &log, overlapped_run);
            if failure != FailureV1::Revalidation {
                let before = files_under_v1(&first);
                let (resumed, _) = run_counted_v1(config, None, || {});
                let resumed = resumed.unwrap_or_else(|error| panic!("{case}: resume: {error}"));
                assert_eq!(resumed["complete"], true, "{case}");
                assert_eq!(after_states_v1(&resumed), reference_states, "{case}");
                if failure == FailureV1::Receipt {
                    // The published update was committed as it was: no
                    // second attempt, and every file it had is unchanged.
                    let after = files_under_v1(&first);
                    for (path, bytes) in &before {
                        assert!(after.get(path) == Some(bytes), "{case}: {}", path.display());
                    }
                    assert!(!first.join("attempt-000001").exists(), "{case}");
                }
            }
            errors.push(error);
            fs::remove_dir_all(&directory).unwrap();
        }
        assert_eq!(errors[0], errors[1], "{failure:?}");
    }
}

/// Invariant 2 (crash safety), every crash point. The overlapped run starts
/// the same publications as the serial run (counted the same way), and its
/// threads never publish at the same time (a collection writes only after
/// the update before it is committed, and the driver waits for it). A crash
/// just before or just after any of them, with every collection held at its
/// gate with a played game until its update is committed, leaves only files
/// the uninterrupted serial run publishes, with the same bytes apart from
/// timing telemetry and the key in the plan, plus staging debris; nothing of
/// an iteration exists, not even its directory, unless the iteration before
/// it is committed; and the committed iterations are a prefix. Resuming
/// reaches the serial run's state, step, loss and games at every iteration,
/// and its very tree when the resume made no new attempt. When it made one,
/// the resumed tree holds every file, with the same bytes (as
/// `assert_same_tree_v1` compares them), of the run without the key crashed
/// at the same publication and resumed (`assert_same_resumed_tree_v1`).
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
    let ((serial_result, _), serial_publications, crashed) =
        with_crash_point_v1(&root, None, || run_counted_v1(&serial, None, || {}));
    assert!(!crashed);
    let serial_result = serial_result.unwrap();
    assert_eq!(serial_result["complete"], true);
    // Read through the receipts' pins, so before the tree moves.
    let reference_states = after_states_v1(&serial_result);
    let reference_content = run_content_v1(&serial_result);
    fs::rename(&directory, &reference).unwrap();
    let ((counted, prefetched), publications, crashed) = with_crash_point_v1(&root, None, || {
        run_counted_v1(&overlapped, None, hold_at_gate_for_test_v1)
    });
    assert!(!crashed);
    assert_eq!(counted.unwrap()["complete"], true);
    assert_eq!(prefetched, 2);
    assert_same_tree_v1("uninterrupted", &directory, &reference, true);
    fs::remove_dir_all(&directory).unwrap();
    // Each iteration publishes its two commands, two trajectories, the
    // collection, the update input, the checkpoint, the update and its
    // receipt; the run adds its plan and completion.
    assert_eq!(publications, serial_publications);
    assert_eq!(publications, 3 * 9 + 2, "{publications} publications");
    for index in 0..publications {
        for point in [CrashPointV1::BeforeFinal, CrashPointV1::AfterFinal] {
            let case = format!("publication {index} {point:?}");
            let ((crashed_run, _), _, crashed) =
                with_crash_point_v1(&root, Some((index, point)), || {
                    run_counted_v1(&overlapped, None, hold_at_gate_for_test_v1)
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
            }
            let (resumed, _) = run_counted_v1(&overlapped, None, hold_at_gate_for_test_v1);
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
            // A resume that made no new attempt publishes the very tree.
            let new_attempt = (0..3).any(|iteration| {
                iteration_directory_v1(&directory, iteration)
                    .join("attempt-000001")
                    .exists()
            });
            if new_attempt {
                // Against the run without the key, crashed at the same
                // publication and resumed in the same directory.
                let resumed_tree = directory.with_file_name("resumed-overlapped");
                let _ = fs::remove_dir_all(&resumed_tree);
                fs::rename(&directory, &resumed_tree).unwrap();
                let ((serial_crashed, _), _, crashed) =
                    with_crash_point_v1(&root, Some((index, point)), || {
                        run_counted_v1(&serial, None, || {})
                    });
                assert!(crashed, "{case}: the serial run never reached it");
                let error = serial_crashed.unwrap_err();
                assert!(error.contains("injected crash"), "{case}: serial: {error}");
                let (serial_resumed, _) = run_counted_v1(&serial, None, || {});
                let serial_resumed = serial_resumed
                    .unwrap_or_else(|error| panic!("{case}: serial resume failed: {error}"));
                assert_eq!(serial_resumed["complete"], true, "{case}");
                assert_same_resumed_tree_v1(&case, &resumed_tree, &directory);
                fs::remove_dir_all(&resumed_tree).unwrap();
            } else {
                assert_same_tree_v1(&format!("{case} resumed"), &directory, &reference, true);
            }
            fs::remove_dir_all(&directory).unwrap();
        }
    }
    fs::remove_dir_all(&reference).unwrap();
}
