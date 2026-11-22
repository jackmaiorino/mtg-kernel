//! Parse-ahead retention and the external collection's feed
//! (`trajectory_parse_ahead_v1`, `external_feed_v1`) change only which reads
//! find a parse made ahead: every run publishes the same trees and states
//! with the retention at its default, large or zero, collected in process or
//! by an external collector, and fails the same way.

use super::follow_tests::assert_trees_match_v1;
use super::*;
use crate::expanded_deck_training_v1::{
    forget_parse_ahead_served_for_test_v1, parse_ahead_held_under_for_test_v1,
    parse_ahead_retention_for_test_v1, parse_ahead_served_for_test_v1,
    release_parse_ahead_under_v1, DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const MEBIBYTE: usize = 1 << 20;

/// This thread's external collector until dropped.
pub(super) struct FakeCollectorV1;

impl FakeCollectorV1 {
    pub(super) fn install_v1(fake: impl Fn(&Path) -> bool + 'static) -> Self {
        FAKE_EXTERNAL_COLLECTOR_V1.with(|slot| *slot.borrow_mut() = Some(Rc::new(fake)));
        Self
    }
}

impl Drop for FakeCollectorV1 {
    fn drop(&mut self) {
        FAKE_EXTERNAL_COLLECTOR_V1.with(|slot| *slot.borrow_mut() = None);
    }
}

/// A path as it appears inside a JSON string.
fn json_text_v1(path: &Path) -> String {
    let quoted = serde_json::to_string(path).unwrap();
    quoted[1..quoted.len() - 1].to_owned()
}

/// Writes `bytes` aside, then renames them to `name`: a file appears only
/// whole under its final name, as the merge publishes it.
fn publish_whole_v1(directory: &Path, name: &str, bytes: &[u8]) {
    let stage = directory.join(format!(".{name}.stage"));
    fs::write(&stage, bytes).unwrap();
    fs::rename(&stage, directory.join(name)).unwrap();
}

/// A stand-in for an external collector (the pooled dispatcher and its
/// merge, in another process): it plays the attempt's collection command in
/// this process into a private directory under `staging`, drops what that
/// left in this process's parse-ahead store (another process shares none),
/// then publishes the trajectories into the command's output directory one
/// by one with a pause before each (at most `publish` of them; all when
/// None), and the collection receipt with its paths moved there last. It
/// waits a little before returning, so the watcher sees every file, and
/// counts its calls in `calls`. With `publish` set it fails after that many.
pub(super) fn fake_external_collector_v1(
    staging: PathBuf,
    calls: Rc<Cell<usize>>,
    publish: Option<usize>,
) -> impl Fn(&Path) -> bool + 'static {
    move |command_path: &Path| {
        calls.set(calls.get() + 1);
        let mut command: ExpandedTrainingCommandV1 =
            serde_json::from_slice(&fs::read(command_path).unwrap()).unwrap();
        let ExpandedTrainingCommandV1::CollectParallel {
            output_directory, ..
        } = &mut command
        else {
            return false;
        };
        let target = output_directory.clone();
        let private = staging.join(format!("call-{}", calls.get()));
        let _ = fs::remove_dir_all(&private);
        fs::create_dir_all(private.parent().unwrap()).unwrap();
        *output_directory = private.clone();
        let receipt = execute_v1(command).unwrap();
        release_parse_ahead_under_v1(&private);
        fs::create_dir_all(&target).unwrap();
        let pins: Vec<PinnedFileV1> =
            serde_json::from_value(receipt["trajectories"].clone()).unwrap();
        for (published, pin) in pins.iter().enumerate() {
            if publish == Some(published) {
                std::thread::sleep(Duration::from_millis(200));
                return false;
            }
            std::thread::sleep(Duration::from_millis(30));
            let name = pin.path.file_name().unwrap().to_str().unwrap();
            publish_whole_v1(&target, name, &fs::read(&pin.path).unwrap());
        }
        let text = fs::read_to_string(private.join("collection.json"))
            .unwrap()
            .replace(&json_text_v1(&private), &json_text_v1(&target));
        publish_whole_v1(&target, "collection.json", text.as_bytes());
        std::thread::sleep(Duration::from_millis(200));
        true
    }
}

/// The trajectory pins of every iteration of a run's result.
fn trajectory_pins_v1(result: &Value) -> Vec<PinnedFileV1> {
    (0..result["iterations"].as_array().unwrap().len())
        .flat_map(|index| {
            let receipt = iteration_receipt_v1(result, index);
            let collection = read_pin(&parse_pin(&receipt, "collection").unwrap()).unwrap();
            serde_json::from_value::<Vec<PinnedFileV1>>(collection["trajectories"].clone()).unwrap()
        })
        .collect()
}

/// Lever 19 and the external feed: in process and with an external
/// collector, a parse-ahead retention at its default, at the largest bound
/// and at zero gives the same trees (every file byte for byte, timed
/// documents without their telemetry) and states. With a retention above
/// zero, collection validation and the update take every trajectory's parse
/// from the store (for the external collector, through the feed); at zero
/// none.
#[test]
fn parse_ahead_retention_and_the_external_feed_change_no_run() {
    let retention = parse_ahead_retention_for_test_v1();
    let base = parallel_two_iteration_config_v1("parse-ahead-retention");
    base.validate_v1().unwrap();
    assert_eq!(
        base.max_trajectory_batch_mebibytes,
        DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES
    );
    let root = base.output_directory.clone();
    let staging = root.with_file_name("fake-collector-staging");
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&staging);
    let cases: [(&str, bool, usize); 6] = [
        ("in-process-default", false, 512 * MEBIBYTE),
        ("in-process-large", false, 16 * 1024 * MEBIBYTE),
        ("in-process-zero", false, 0),
        ("external-default", true, 512 * MEBIBYTE),
        ("external-large", true, 16 * 1024 * MEBIBYTE),
        ("external-zero", true, 0),
    ];
    let mut kept = Vec::new();
    let mut reference: Option<(PathBuf, Value)> = None;
    for (case, external, bytes) in cases {
        retention.set_v1(bytes);
        let calls = Rc::new(Cell::new(0));
        let fake = external.then(|| {
            FakeCollectorV1::install_v1(fake_external_collector_v1(
                staging.join(case),
                Rc::clone(&calls),
                None,
            ))
        });
        let result = run_native_expanded_training_v1(&base, None).unwrap();
        drop(fake);
        assert_eq!(result["complete"], true, "{case}");
        assert_eq!(calls.get(), if external { 2 } else { 0 }, "{case}");
        let pins = trajectory_pins_v1(&result);
        assert_eq!(pins.len(), 4, "{case}");
        for pin in &pins {
            let expected = [bytes > 0; 2];
            assert_eq!(
                parse_ahead_served_for_test_v1(&pin.sha256),
                expected,
                "{case}: {}",
                pin.path.display()
            );
            forget_parse_ahead_served_for_test_v1(&pin.sha256);
        }
        assert_eq!(parse_ahead_held_under_for_test_v1(&root), 0, "{case}");
        let moved = root.with_file_name(format!("run-{case}"));
        let _ = fs::remove_dir_all(&moved);
        fs::rename(&root, &moved).unwrap();
        match &reference {
            None => reference = Some((moved.clone(), result)),
            Some((first, first_result)) => {
                assert_eq!(result["actual_identity"], first_result["actual_identity"]);
                assert_eq!(result["source"], first_result["source"]);
                assert_trees_match_v1(case, &moved, first, true);
            }
        }
        kept.push(moved);
    }
    drop(retention);
    for tree in kept {
        fs::remove_dir_all(tree).unwrap();
    }
    let _ = fs::remove_dir_all(&staging);
}

/// An external collector that fails after publishing part of its files
/// fails the run with the error a run without parse-ahead gives, and leaves
/// nothing of that iteration in the parse-ahead store.
#[test]
fn a_failed_external_collection_fails_as_before_and_leaves_nothing_held() {
    let retention = parse_ahead_retention_for_test_v1();
    let base = parallel_two_iteration_config_v1("parse-ahead-failed-feed");
    let root = base.output_directory.clone();
    let staging = root.with_file_name("fake-collector-staging-failed");
    let mut errors = Vec::new();
    for bytes in [512 * MEBIBYTE, 0] {
        retention.set_v1(bytes);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staging);
        let calls = Rc::new(Cell::new(0));
        let fake = FakeCollectorV1::install_v1(fake_external_collector_v1(
            staging.clone(),
            Rc::clone(&calls),
            Some(1),
        ));
        let error = run_native_expanded_training_v1(&base, None).unwrap_err();
        drop(fake);
        assert_eq!(calls.get(), 1);
        assert_eq!(parse_ahead_held_under_for_test_v1(&root), 0);
        errors.push(error);
    }
    assert_eq!(errors[0], errors[1]);
    assert_eq!(errors[0], "external collector failed");
    drop(retention);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&staging);
}
