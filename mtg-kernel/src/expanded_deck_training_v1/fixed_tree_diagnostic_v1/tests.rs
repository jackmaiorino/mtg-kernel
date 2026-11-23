use super::*;
use crate::expanded_deck_training_v1::fixed_tree_update_tests::{fixture, gae, update};

/// Every file under `directory` with its bytes, in path order.
fn snapshot(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = fs::read(&path).unwrap();
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

/// The diagnostic on a completed update of three real games. Its sequential
/// step reaches the state that update published and its fixed-tree step the
/// state a fixed-tree update of the same batch publishes, at the given
/// threads and on one. It reports the batch, the timings and the
/// differences, and leaves the run's files exactly as they were.
#[test]
fn the_diagnostic_reproduces_both_updates_and_writes_nothing() {
    let (root, source, trajectories) = fixture("fixed-tree-diagnostic");
    let sequential_command = update(
        &source,
        &trajectories,
        gae(),
        UpdateBackwardExecutionV1::Sequential,
        3,
        root.join("update-sequential"),
    );
    let sequential = execute_v1(sequential_command.clone()).unwrap();
    let fixed_tree = execute_v1(update(
        &source,
        &trajectories,
        gae(),
        UpdateBackwardExecutionV1::FixedTreeV1,
        3,
        root.join("update-fixed-tree"),
    ))
    .unwrap();
    let before = snapshot(&root);

    let options = FixedTreeDiagnosticOptionsV1 {
        threads: Some(3),
        repeats: 2,
        relative_floor: DEFAULT_RELATIVE_FLOOR_V1,
    };
    // The completed update's own command: its output directory exists.
    let report = diagnose_fixed_tree_v1(sequential_command, options).unwrap();
    assert!(snapshot(&root) == before, "the diagnostic changed a file");

    assert_eq!(report["schema"], "mtg-kernel-fixed-tree-diagnostic/v1");
    assert_eq!(report["threads"], 3);
    assert_eq!(report["repeats"], 2);
    assert_eq!(report["command_preparation_workers"], 3);
    assert_eq!(
        report["before_state_sha256"],
        sequential["before_state_sha256"]
    );
    let rows = sequential["policy_substeps"].as_u64().unwrap();
    assert_eq!(report["batch"]["episodes"], 3);
    assert_eq!(report["batch"]["rows"], rows);
    assert_eq!(
        report["batch"]["physical_decisions"],
        sequential["physical_decisions"]
    );
    assert_eq!(report["batch"]["chunk_rows"], 64);
    assert_eq!(report["batch"]["chunks"], rows.div_ceil(64));
    assert!(rows > 64, "{rows}");

    // Both ways reach the states the real updates published.
    for (way, published) in [("sequential", &sequential), ("fixed_tree", &fixed_tree)] {
        for threads in ["at_threads", "at_one_thread"] {
            let measured = &report[way][threads];
            assert_eq!(
                measured["after_state_sha256"], published["after_state_sha256"],
                "{way} {threads}"
            );
            assert_eq!(measured["repeats_agree"], true, "{way} {threads}");
            for seconds in [
                "gradient_pass_seconds",
                "forward_and_loss_seconds",
                "adam_seconds",
                "learner_step_seconds",
                "update_seconds",
            ] {
                let value = measured[seconds].as_f64().unwrap();
                assert!(
                    value.is_finite() && value >= 0.0,
                    "{way} {threads} {seconds}"
                );
            }
            assert!(
                measured["update_seconds"].as_f64().unwrap()
                    >= measured["learner_step_seconds"].as_f64().unwrap()
            );
            assert_eq!(
                measured["samples"]["gradient_pass_seconds"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
        }
    }
    assert_eq!(
        report["invariance"],
        json!({
            "sequential_threads_agree": true,
            "fixed_tree_threads_agree": true,
            "repeats_agree": true,
        })
    );

    // The loss is summed before any backward runs: the same bits both ways.
    let difference = &report["difference"];
    assert_eq!(difference["relative_floor"], DEFAULT_RELATIVE_FLOOR_V1);
    for scalar in ["loss", "policy_sum", "value_sum"] {
        assert_eq!(difference[scalar]["same_bits"], true, "{scalar}");
        assert_eq!(difference[scalar]["abs"], 0.0, "{scalar}");
    }
    assert_eq!(difference["loss"]["sequential"], sequential["loss"]);
    // The gradient is another reduction: some last bits differ, by little.
    let gradient = &difference["gradient"];
    assert!(gradient["elements"].as_u64().unwrap() > 1_000_000);
    assert!(gradient["differing_elements"].as_u64().unwrap() > 0);
    let max_abs = gradient["max_abs"].as_f64().unwrap();
    let max_magnitude = gradient["max_magnitude"].as_f64().unwrap();
    assert!(
        max_abs > 0.0 && max_abs < 1.0e-3 * max_magnitude,
        "{max_abs} {max_magnitude}"
    );
    assert!(gradient["max_rel"].as_f64().unwrap().is_finite());
    assert!(!gradient["differing_tensors"].as_array().unwrap().is_empty());
    // So are Adam's moments after the step. (From a fresh optimizer a first
    // step is close to the gradient's sign times the learning rate, so the
    // parameters themselves may or may not differ in any bit here.)
    for section in ["first_moments_after_step", "second_moments_after_step"] {
        assert!(
            difference[section]["differing_elements"].as_u64().unwrap() > 0,
            "{section}"
        );
    }
    for section in [
        "parameters_after_step",
        "first_moments_after_step",
        "second_moments_after_step",
    ] {
        assert_eq!(
            difference[section]["elements"], gradient["elements"],
            "{section}"
        );
        assert!(difference[section]["max_abs"].as_f64().unwrap().is_finite());
        assert!(difference[section]["max_rel"].as_f64().unwrap().is_finite());
    }
    let _ = fs::remove_dir_all(&root);
}

/// The diagnostic takes an update command and sane options only, and refuses
/// the rest before reading anything.
#[test]
fn the_diagnostic_refuses_other_commands_and_options() {
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: "missing-descriptor.json".into(),
            sha256: "0".repeat(64),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    let collect = ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes: Vec::new(),
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        output_directory: "missing-output".into(),
    };
    let error =
        diagnose_fixed_tree_v1(collect, FixedTreeDiagnosticOptionsV1::default()).unwrap_err();
    assert!(
        error.contains("update or update_prepared command"),
        "{error}"
    );

    let defaults = FixedTreeDiagnosticOptionsV1::default();
    assert_eq!(defaults.threads, None);
    assert_eq!(defaults.repeats, DEFAULT_REPEATS_V1);
    defaults.validate_v1().unwrap();
    for options in [
        FixedTreeDiagnosticOptionsV1 {
            threads: Some(0),
            ..defaults
        },
        FixedTreeDiagnosticOptionsV1 {
            threads: Some(MAX_THREADS_V1 + 1),
            ..defaults
        },
        FixedTreeDiagnosticOptionsV1 {
            repeats: 0,
            ..defaults
        },
        FixedTreeDiagnosticOptionsV1 {
            relative_floor: 0.0,
            ..defaults
        },
        FixedTreeDiagnosticOptionsV1 {
            relative_floor: f64::NAN,
            ..defaults
        },
    ] {
        assert!(options.validate_v1().is_err(), "{options:?}");
        let command = update(
            &source,
            &[],
            gae(),
            UpdateBackwardExecutionV1::Sequential,
            2,
            "missing-output".into(),
        );
        assert!(diagnose_fixed_tree_v1(command, options).is_err());
    }
}

/// Medians and element differences, on values small enough to check by hand.
#[test]
fn medians_and_differences_are_what_they_say() {
    assert_eq!(median_v1([3.0, 1.0, 2.0].into_iter()), 2.0);
    assert_eq!(median_v1([4.0, 1.0, 2.0, 3.0].into_iter()), 2.5);
    assert_eq!(median_v1([7.0].into_iter()), 7.0);
    assert_eq!(median_v1(std::iter::empty()), 0.0);

    let left = [1.0f32, 0.0, -2.0, 1.0e-12];
    let right = [1.0f32, 0.0, -2.5, 3.0e-12];
    let difference = DifferenceV1::of_v1(&left, &right, 1.0e-9).unwrap();
    assert_eq!(difference.elements, 4);
    assert_eq!(difference.differing_elements, 2);
    assert_eq!(difference.max_abs, 0.5);
    // 0.5 / 2.5 for the third element; the fourth is below the floor, so its
    // 2e-12 is taken against 1e-9.
    assert!((difference.max_rel - 0.2).abs() < 1.0e-12);
    assert_eq!(difference.max_magnitude, 2.5);
    // Without the floor the near-zero pair would dominate.
    let unfloored = DifferenceV1::of_v1(&left, &right, f64::MIN_POSITIVE).unwrap();
    assert!(unfloored.max_rel > 0.6);
    assert!(DifferenceV1::of_v1(&left, &right[..3], 1.0e-9).is_err());

    let scalar = scalar_difference_v1(2.0, 2.0, 1.0e-9);
    assert_eq!(scalar["same_bits"], true);
    assert_eq!(scalar["abs"], 0.0);
    let scalar = scalar_difference_v1(2.0, 3.0, 1.0e-9);
    assert_eq!(scalar["same_bits"], false);
    assert_eq!(scalar["abs"], 1.0);
}
