//! The fixed-tree backward (`update_backward_execution: "fixed_tree_v1"`)
//! through the real Collect, Update and UpdatePrepared commands: the same
//! state and checkpoint bytes at every worker count, another state than the
//! sequential backward's, a receipt that names it, and commands and receipts
//! without it that keep their bytes.

use super::*;
use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;

/// The worker counts every invariance test runs at.
const WORKER_COUNTS: [usize; 5] = [1, 2, 3, 7, 16];

fn list(id: &str) -> ExpandedDeckListV1 {
    let d = checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let c = d.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: c.mainboard().to_vec(),
        sideboard: c.sideboard().to_vec(),
    }
}

pub(super) fn gae() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
        pregame_advantage: PregameAdvantageV1::default(),
        pregame_announcement_step_scale: None,
    }
}

/// The update the run driver issues for `workers` preparation workers: the
/// serial command for one, the prepared command otherwise.
pub(super) fn update(
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    loss_selection: ExpandedLossSelectionV1,
    update_backward_execution: UpdateBackwardExecutionV1,
    workers: usize,
    output_directory: PathBuf,
) -> ExpandedTrainingCommandV1 {
    if workers == 1 {
        ExpandedTrainingCommandV1::Update {
            source: source.clone(),
            trajectories: trajectories.to_vec(),
            learning_rate: 0.0001,
            value_coefficient: 0.5,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution,
            loss_selection,
            line_b: None,
            training_math: TrainingMathV1::PlatformLibm,
            record_format: CheckpointRecordFormatV1::default(),
            output_directory,
        }
    } else {
        ExpandedTrainingCommandV1::UpdatePrepared {
            source: source.clone(),
            trajectories: trajectories.to_vec(),
            learning_rate: 0.0001,
            value_coefficient: 0.5,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution,
            loss_selection,
            preparation_workers: workers,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            max_trajectory_batch_mebibytes: DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
            line_b: None,
            training_math: TrainingMathV1::PlatformLibm,
            record_format: CheckpointRecordFormatV1::default(),
            record_envelope: RecordEnvelopeV1::None,
            output_directory,
        }
    }
}

/// A synthetic fresh source and three real games collected from it: the
/// decks, seats and seeds of `trajectory_batch_bound_tests`' fixture, which
/// are known to end naturally.
pub(super) fn fixture(tag: &str) -> (PathBuf, ExpandedModelSourceV1, Vec<PinnedFileV1>) {
    let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
    let root = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let descriptor = fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        feature_identity,
        |_parameters| {},
    );
    let descriptor_path = root.join("descriptor.json");
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    fs::write(&descriptor_path, &bytes).unwrap();
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: descriptor_path.canonicalize().unwrap(),
            sha256: hex(&Sha256::digest(bytes)),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    let decks = [list("Affinity"), list("Terror")];
    let episodes = (0..3u64)
        .map(|index| ExpandedEpisodeV1 {
            id: format!("{tag}-{index}"),
            seed: 2026111900 + index,
            starting_player: (index % 2) as u8,
            learner_seat: 0,
            opponent: Some(source.clone()),
            opponent_search: None,
            opponent_kind: None,
            registered: decks.clone(),
            selected: decks.clone(),
            postboard: false,
            max_physical_decisions: 100000,
            max_policy_steps: 1000000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
            london_keep_offset: None,
            priority_windows: None,
            priority_pass_offset: None,
        })
        .collect();
    let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        output_directory: root.join("collect"),
    })
    .unwrap();
    let trajectories = serde_json::from_value(collection["trajectories"].clone()).unwrap();
    (root, source, trajectories)
}

fn checkpoint_bytes(result: &Value) -> Vec<u8> {
    let pin: PinnedFileV1 = serde_json::from_value(result["checkpoint"].clone()).unwrap();
    fs::read(pin.path).unwrap()
}

/// Three real games through the real update commands under the fixed tree,
/// at 1, 2, 3, 7 and 16 preparation workers: one after-state hash and one
/// checkpoint, byte for byte. The receipt names the backward and its
/// constants. The sequential backward on the same games reaches another
/// state, and its receipt has no `backward_execution` member.
#[test]
fn fixed_tree_updates_are_the_same_bytes_at_every_worker_count() {
    let (root, source, trajectories) = fixture("fixed-tree-update-workers");
    let mut reference: Option<(String, Vec<u8>)> = None;
    for workers in WORKER_COUNTS {
        let result = execute_v1(update(
            &source,
            &trajectories,
            gae(),
            UpdateBackwardExecutionV1::FixedTreeV1,
            workers,
            root.join(format!("fixed-tree-w{workers}")),
        ))
        .unwrap();
        // Real games: more rows than one chunk, so the tree combines.
        let rows = result["policy_substeps"].as_u64().unwrap() as usize;
        assert!(
            rows > crate::native_policy_train_step_v1::FIXED_TREE_CHUNK_ROWS_V1,
            "{rows}"
        );
        assert_eq!(
            result["backward_execution"],
            json!({
                "identity": "fixed_tree_v1",
                "numerical_backend": "rust-production-native-policy-train-step-v1-cpu-ieee754-binary32-fixed-tree-rows64-v1",
                "chunk_rows": 64,
                "rows": rows,
                "chunks": rows.div_ceil(64),
            })
        );
        UpdateBackwardExecutionV1::FixedTreeV1
            .validate_receipt_v1(&result)
            .unwrap();
        assert!(UpdateBackwardExecutionV1::Sequential
            .validate_receipt_v1(&result)
            .is_err());
        let state = result["after_state_sha256"].as_str().unwrap().to_owned();
        eprintln!("fixed_tree_v1 fixture state at {workers} workers: {state}");
        let observed = (state, checkpoint_bytes(&result));
        match &reference {
            None => reference = Some(observed),
            Some(reference) => {
                assert_eq!(observed.0, reference.0, "state at {workers} workers");
                assert!(
                    observed.1 == reference.1,
                    "checkpoint bytes at {workers} workers"
                );
            }
        }
    }
    let (fixed_tree_state, _) = reference.unwrap();

    // The sequential backward: serial and prepared reach one state, which is
    // not the fixed tree's, and their receipts do not name a backward.
    let mut sequential_states = Vec::new();
    for workers in [1, 3] {
        let result = execute_v1(update(
            &source,
            &trajectories,
            gae(),
            UpdateBackwardExecutionV1::Sequential,
            workers,
            root.join(format!("sequential-w{workers}")),
        ))
        .unwrap();
        assert!(result.get("backward_execution").is_none());
        assert_eq!(result["numerical_backend"], "native-cpu-sequential");
        UpdateBackwardExecutionV1::Sequential
            .validate_receipt_v1(&result)
            .unwrap();
        assert!(UpdateBackwardExecutionV1::FixedTreeV1
            .validate_receipt_v1(&result)
            .is_err());
        sequential_states.push(result["after_state_sha256"].as_str().unwrap().to_owned());
    }
    assert_eq!(sequential_states[0], sequential_states[1]);
    assert_ne!(fixed_tree_state, sequential_states[0]);
    let _ = fs::remove_dir_all(&root);
}

/// The fixed tree is the plain CPU GAE update's only: the terminal loss and
/// line (b) refuse it by name before any input is read (the sources here do
/// not exist).
#[test]
fn the_fixed_tree_is_refused_outside_the_plain_cpu_gae_update() {
    let missing = std::env::temp_dir().join(format!("fixed-tree-missing-{}", std::process::id()));
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: missing.join("descriptor.json"),
            sha256: "0".repeat(64),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest:
                crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest:
                crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    let trajectories = vec![PinnedFileV1 {
        path: missing.join("trajectory.json"),
        sha256: "1".repeat(64),
    }];
    let refused = |command: ExpandedTrainingCommandV1| {
        let error = execute_v1(command).unwrap_err();
        assert!(
            error.contains("fixed_tree_v1 requires the plain CPU GAE update"),
            "{error}"
        );
    };
    for workers in [1, 4] {
        // The terminal loss.
        refused(update(
            &source,
            &trajectories,
            ExpandedLossSelectionV1::default(),
            UpdateBackwardExecutionV1::FixedTreeV1,
            workers,
            missing.join("out"),
        ));
        // Line (b).
        let mut masked = update(
            &source,
            &trajectories,
            gae(),
            UpdateBackwardExecutionV1::FixedTreeV1,
            workers,
            missing.join("out"),
        );
        match &mut masked {
            ExpandedTrainingCommandV1::Update { line_b, .. }
            | ExpandedTrainingCommandV1::UpdatePrepared { line_b, .. } => {
                *line_b = Some(LineBUpdateOptionsV1 {
                    optimizer_mask: OptimizerMaskV1::LineBHeadOnlyV1,
                    teacher: None,
                });
            }
            _ => unreachable!("update builds an update command"),
        }
        refused(masked);
    }
    assert!(!missing.exists());
}

/// The wire names. A command without the fixed tree serializes exactly as it
/// did (no `update_backward_execution` member for the sequential backward),
/// the fixed tree adds that one member, and no other spelling is accepted.
#[test]
fn commands_name_the_fixed_tree_and_keep_their_bytes_without_it() {
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: "descriptor.json".into(),
            sha256: "0".repeat(64),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest:
                crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest:
                crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    for workers in [1, 4] {
        let command = |backward| update(&source, &[], gae(), backward, workers, "out".into());
        let sequential =
            serde_json::to_value(command(UpdateBackwardExecutionV1::Sequential)).unwrap();
        assert!(sequential.get("update_backward_execution").is_none());
        let fixed_tree =
            serde_json::to_value(command(UpdateBackwardExecutionV1::FixedTreeV1)).unwrap();
        assert_eq!(fixed_tree["update_backward_execution"], "fixed_tree_v1");
        // That member is the whole difference.
        let mut without = fixed_tree.clone();
        without
            .as_object_mut()
            .unwrap()
            .remove("update_backward_execution");
        assert_eq!(without, sequential);
        // It reads back, and an absent member is the sequential backward.
        for (document, expected) in [
            (fixed_tree, UpdateBackwardExecutionV1::FixedTreeV1),
            (sequential, UpdateBackwardExecutionV1::Sequential),
        ] {
            match serde_json::from_value::<ExpandedTrainingCommandV1>(document).unwrap() {
                ExpandedTrainingCommandV1::Update {
                    update_backward_execution,
                    ..
                }
                | ExpandedTrainingCommandV1::UpdatePrepared {
                    update_backward_execution,
                    ..
                } => assert_eq!(update_backward_execution, expected),
                _ => unreachable!("update builds an update command"),
            }
        }
    }
    assert_eq!(
        serde_json::to_string(&UpdateBackwardExecutionV1::FixedTreeV1).unwrap(),
        "\"fixed_tree_v1\""
    );
    assert_eq!(
        serde_json::to_string(&UpdateBackwardExecutionV1::Sequential).unwrap(),
        "\"sequential\""
    );
    assert_eq!(
        serde_json::to_string(&UpdateBackwardExecutionV1::FixedPartition4).unwrap(),
        "\"fixed_partition_4\""
    );
    for other in [
        "fixed_tree",
        "fixed_tree_v2",
        "fixed-tree-v1",
        "FixedTreeV1",
    ] {
        assert!(
            serde_json::from_value::<UpdateBackwardExecutionV1>(json!(other)).is_err(),
            "{other}"
        );
    }
}

/// What a receipt records and what a run accepts on recovery: the fixed tree
/// by its identity, numerical backend and chunk size, and no member at all
/// for the two backwards that exist today.
#[test]
fn receipts_bind_the_backward_execution() {
    let fixed_tree = UpdateBackwardExecutionV1::FixedTreeV1;
    assert!(fixed_tree.is_fixed_tree_v1() && !fixed_tree.is_sequential());
    assert!(fixed_tree.routes_announcement_buffer_v1());
    assert!(UpdateBackwardExecutionV1::Sequential.routes_announcement_buffer_v1());
    assert!(!UpdateBackwardExecutionV1::FixedPartition4.routes_announcement_buffer_v1());
    for other in [
        UpdateBackwardExecutionV1::Sequential,
        UpdateBackwardExecutionV1::FixedPartition4,
    ] {
        assert_eq!(other.receipt_v1(6_000), None);
        other
            .validate_receipt_v1(&json!({"complete": true}))
            .unwrap();
        assert!(other
            .validate_receipt_v1(&json!({"backward_execution": fixed_tree.receipt_v1(6_000)}))
            .is_err());
    }
    let record = fixed_tree.receipt_v1(6_000).unwrap();
    assert_eq!(
        record["identity"],
        FIXED_TREE_BACKWARD_EXECUTION_IDENTITY_V1
    );
    assert_eq!(record["chunk_rows"], 64);
    assert_eq!(record["rows"], 6_000);
    assert_eq!(record["chunks"], 94);
    assert_eq!(fixed_tree.receipt_v1(0).unwrap()["chunks"], 0);
    fixed_tree
        .validate_receipt_v1(&json!({"backward_execution": record}))
        .unwrap();
    assert!(fixed_tree
        .validate_receipt_v1(&json!({"complete": true}))
        .is_err());
    for (member, value) in [
        ("identity", json!("fixed_tree_v2")),
        ("numerical_backend", json!("native-cpu-sequential")),
        ("chunk_rows", json!(32)),
    ] {
        let mut changed = record.clone();
        changed[member] = value;
        assert!(
            fixed_tree
                .validate_receipt_v1(&json!({"backward_execution": changed}))
                .is_err(),
            "{member}"
        );
    }
}
