//! `update_backward_execution: "fixed_tree_v1"` in a run config and through
//! the production run: every config without it keeps its bytes and identity,
//! a config with it names it in the manifest, the update commands and the
//! update receipts, trains one state at every worker count, and is its own
//! lineage.

use super::*;
use crate::expanded_deck_training_v1::PregameAdvantageV1;

fn after_states(result: &Value) -> Vec<Value> {
    (0..2)
        .map(|index| iteration_update_v1(result, index)["after_state_sha256"].clone())
        .collect()
}

/// A config that does not name the fixed tree serializes exactly as before
/// (no `update_backward_execution` member, an explicit `sequential` not
/// written back), and one that names it differs by that one member, in the
/// config and in both update commands.
#[test]
fn the_fixed_tree_value_keeps_every_other_config_byte() {
    let plan = schedule();
    let wire = value(&plan).unwrap();
    assert!(wire.get("update_backward_execution").is_none());
    let bytes = serde_json::to_vec(&plan).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("fixed_tree"));
    let mut explicit = wire.clone();
    explicit["update_backward_execution"] = json!("sequential");
    let decoded: NativeExpandedTrainingRunV1 = serde_json::from_value(explicit).unwrap();
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    assert_eq!(identity(&decoded).unwrap(), identity(&plan).unwrap());

    let mut sequential = schedule();
    sequential.loss_selection = gae_loss_v1(PregameAdvantageV1::Gae);
    let mut fixed_tree = sequential.clone();
    fixed_tree.update_backward_execution = UpdateBackwardExecutionV1::FixedTreeV1;
    fixed_tree.validate_v1().unwrap();
    let wire = value(&fixed_tree).unwrap();
    assert_eq!(wire["update_backward_execution"], "fixed_tree_v1");
    let mut without = wire.clone();
    without
        .as_object_mut()
        .unwrap()
        .remove("update_backward_execution");
    assert_eq!(without, value(&sequential).unwrap());
    assert_ne!(
        identity(&fixed_tree).unwrap(),
        identity(&sequential).unwrap()
    );
    let decoded: NativeExpandedTrainingRunV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(
        decoded.update_backward_execution,
        UpdateBackwardExecutionV1::FixedTreeV1
    );
    for other in ["fixed_tree", "fixed_tree_v2", "FixedTreeV1"] {
        let mut misspelled = wire.clone();
        misspelled["update_backward_execution"] = json!(other);
        assert!(
            serde_json::from_value::<NativeExpandedTrainingRunV1>(misspelled).is_err(),
            "{other}"
        );
    }
    // The serial and the prepared update command both carry the value.
    for workers in [1usize, 4] {
        fixed_tree.preparation_workers = workers;
        sequential.preparation_workers = workers;
        let command = |config: &NativeExpandedTrainingRunV1| {
            value(&update_command(
                config,
                0,
                &config.initial_source,
                Vec::new(),
                PathBuf::from("out"),
            ))
            .unwrap()
        };
        let mut with = command(&fixed_tree);
        assert_eq!(
            with.as_object_mut()
                .unwrap()
                .remove("update_backward_execution"),
            Some(json!("fixed_tree_v1"))
        );
        assert_eq!(with, command(&sequential));
        assert!(command(&sequential)
            .get("update_backward_execution")
            .is_none());
    }
    // Neither manifest helper adds a key for it: the config itself names it.
    let mut manifest = json!({});
    record_collection_execution(&fixed_tree, &mut manifest);
    let mut sequential_manifest = json!({});
    record_collection_execution(&sequential, &mut sequential_manifest);
    assert_eq!(manifest, sequential_manifest);
}

/// The fixed tree is the plain CPU GAE update's only: every other pairing is
/// refused when the config is validated, before anything runs. The pregame
/// announcement step scale, which the fixed tree routes, stays admitted.
#[test]
fn the_fixed_tree_is_refused_off_the_plain_cpu_gae_update() {
    use crate::expanded_deck_training_v1::s_learner_v1::{
        SLearnerSourceV1, S_LEARNER_SOURCE_SCHEMA_V1,
    };
    let mut config = schedule();
    config.loss_selection = gae_loss_v1(PregameAdvantageV1::Gae);
    config.update_backward_execution = UpdateBackwardExecutionV1::FixedTreeV1;
    config.validate_v1().unwrap();
    let refused = |config: &NativeExpandedTrainingRunV1| {
        let error = config.validate_v1().unwrap_err();
        assert!(
            error.contains("fixed_tree_v1 requires the plain CPU GAE update"),
            "{error}"
        );
    };
    let mut terminal = config.clone();
    terminal.loss_selection = ExpandedLossSelectionV1::default();
    refused(&terminal);
    let mut cuda = config.clone();
    cuda.update_backend = ExpandedUpdateBackendV1::Cuda { device_ordinal: 0 };
    refused(&cuda);
    let mut masked = config.clone();
    masked.line_b = Some(LineBUpdateOptionsV1 {
        optimizer_mask: crate::expanded_deck_training_v1::OptimizerMaskV1::LineBHeadOnlyV1,
        teacher: None,
    });
    refused(&masked);
    let mut ex1 = config.clone();
    ex1.ex1 = Some(Ex1RunOptionsV1 {
        run: "chain".into(),
        update_offset: 0,
        coefficient: 0.1,
        cap: Default::default(),
        w5: None,
    });
    refused(&ex1);
    let mut s_learner = config.clone();
    s_learner.initial_source.s_learner = Some(Box::new(SLearnerSourceV1 {
        schema: S_LEARNER_SOURCE_SCHEMA_V1.into(),
        initial: PinnedFileV1 {
            path: std::env::temp_dir().join("s0.bin"),
            sha256: "b".repeat(64),
        },
    }));
    refused(&s_learner);
    // The scale's second buffer is routed by the fixed tree as by the
    // sequential backward, and still not by the fixed-partition backward.
    let mut scaled = config.clone();
    scaled.loss_selection = gae_loss_step_scale_v1(PregameAdvantageV1::TerminalV1, Some(0.25));
    scaled.validate_v1().unwrap();
    scaled.update_backward_execution = UpdateBackwardExecutionV1::FixedPartition4;
    assert!(scaled
        .validate_v1()
        .unwrap_err()
        .contains("pregame_announcement_step_scale requires"));
}

/// The production run under the fixed tree, two real iterations each paused
/// and resumed, at 1, 2, 3, 7 and 16 preparation workers: one pair of states.
/// The manifest and every update receipt name the backward, the states are
/// not the sequential run's, and neither run's directory resumes under the
/// other's backward.
#[test]
fn a_fixed_tree_run_trains_one_state_at_every_worker_count_and_is_its_own_lineage() {
    // One source and one schedule; each run has its own directory.
    let base = real_two_iteration_config_v1(
        "fixed-tree",
        gae_loss_v1(PregameAdvantageV1::Gae),
        ExpandedUpdateBackendV1::Cpu,
    );
    let mut reference: Option<Vec<Value>> = None;
    let mut last = None;
    for workers in [1usize, 2, 3, 7, 16] {
        let mut config = base.clone();
        config.update_backward_execution = UpdateBackwardExecutionV1::FixedTreeV1;
        config.preparation_workers = workers;
        config.output_directory = base
            .output_directory
            .with_file_name(format!("run-fixed-tree-w{workers}"));
        config.validate_v1().unwrap();
        // One iteration, then a resume that revalidates it and runs the second.
        run_native_expanded_training_v1(&config, Some(1)).unwrap();
        let result = run_native_expanded_training_v1(&config, None).unwrap();
        assert_eq!(result["complete"], true);
        let manifest: Value =
            read_json(&config.output_directory.join("run.json"), SMALL_CAP).unwrap();
        assert_eq!(
            manifest["config"]["update_backward_execution"],
            "fixed_tree_v1"
        );
        for index in 0..2 {
            let update = iteration_update_v1(&result, index);
            let record = &update["backward_execution"];
            assert_eq!(record["identity"], "fixed_tree_v1", "{update}");
            assert_eq!(record["chunk_rows"], 64);
            assert_eq!(record["rows"], update["policy_substeps"]);
            // Real games: more rows than one chunk.
            assert!(record["chunks"].as_u64().unwrap() >= 2, "{record}");
        }
        let states = after_states(&result);
        eprintln!("fixed_tree_v1 run states at {workers} workers: {states:?}");
        match &reference {
            None => reference = Some(states),
            Some(reference) => assert_eq!(&states, reference, "{workers} workers"),
        }
        last = Some(config);
    }
    let fixed_tree_states = reference.unwrap();
    let fixed_tree = last.unwrap();

    let mut sequential = base.clone();
    sequential.output_directory = base.output_directory.with_file_name("run-sequential");
    let sequential_result = run_native_expanded_training_v1(&sequential, None).unwrap();
    let manifest_text = fs::read_to_string(sequential.output_directory.join("run.json")).unwrap();
    assert!(!manifest_text.contains("backward_execution"));
    for index in 0..2 {
        let update = iteration_update_v1(&sequential_result, index);
        assert!(update.get("backward_execution").is_none());
        assert!(!update.to_string().contains("fixed_tree"));
    }
    let sequential_states = after_states(&sequential_result);
    assert_ne!(fixed_tree_states[0], sequential_states[0]);
    assert_ne!(fixed_tree_states[1], sequential_states[1]);

    // One run, one backward: neither directory resumes under the other's.
    let mut removed = fixed_tree.clone();
    removed.update_backward_execution = UpdateBackwardExecutionV1::Sequential;
    let mut added = sequential.clone();
    added.update_backward_execution = UpdateBackwardExecutionV1::FixedTreeV1;
    for changed in [removed, added] {
        let error = run_native_expanded_training_v1(&changed, None).unwrap_err();
        assert!(
            error.contains("resume plan or implementation differs"),
            "{error}"
        );
    }
}
