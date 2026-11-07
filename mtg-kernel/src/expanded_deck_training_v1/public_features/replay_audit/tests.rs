//! The native audit binds a trajectory to models of its own feature identity
//! (#446). FlatV7 and FlatV8 policies load from a V4 import, so their model
//! identity keeps the import's V4 digests while their trajectories record the
//! effective V7 or V8 identity.
use super::*;
use crate::expanded_deck_training_v1::v8_inclusion_conversion_v1::write_v8_checkpoint_v1;
use crate::native_flat_tensorizer_v7::{FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7};
use crate::native_flat_tensorizer_v8::{FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8};
use crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

fn episode(id: &str, seed: u64) -> ExpandedEpisodeV1 {
    ExpandedEpisodeV1 {
        id: id.into(),
        seed,
        starting_player: 0,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: [deck("Elves"), deck("Affinity")],
        selected: [deck("Elves"), deck("Affinity")],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: None,
        mulligan_modes: None,
    }
}

/// One real game each from a V4 fresh source, its FlatV7 transfer and a FlatV8
/// conversion of its trained checkpoint. Each trajectory audits against two
/// models of its own identity, every row replaying its archived outputs bit
/// for bit (the V8 rows with their pairing relation). Every mixed pair is
/// refused before any row is scored, including a V4 trajectory against a
/// FlatV7 model, whose import digests are V4's.
#[test]
fn native_audit_admits_flat_v7_and_v8_trajectories_only_against_their_own_identity() {
    let root = std::env::temp_dir().join(format!("replay-audit-flat-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let descriptor = write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        FRESH_FEATURE_IDENTITY_V4,
        |_| {},
    );
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let descriptor_path = root.join("descriptor.json");
    fs::write(&descriptor_path, &bytes).unwrap();
    let play_import = PinnedFileV1 {
        path: descriptor_path,
        sha256: sha(&bytes),
    };
    let source = |contract: &str, encoding: &str, checkpoint: Option<PinnedFileV1>| {
        ExpandedModelSourceV1 {
            play_import: play_import.clone(),
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: contract.into(),
                expected_feature_encoding_digest: encoding.into(),
            },
            checkpoint,
            s_learner: None,
        }
    };
    let collect = |source: &ExpandedModelSourceV1, label: &str, seed: u64| -> PinnedFileV1 {
        let collected = execute_v1(ExpandedTrainingCommandV1::Collect {
            source: source.clone(),
            episodes: vec![episode(label, seed)],
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            output_directory: root.join(format!("collect-{label}")),
        })
        .unwrap();
        let mut trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collected["trajectories"].clone()).unwrap();
        assert_eq!(trajectories.len(), 1);
        trajectories.remove(0)
    };
    let v4 = source(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4, None);
    let v4_trajectory = collect(&v4, "v4", 2_026_110_601);
    // One GAE update gives the V8 conversion a trained V4 parent.
    let updated = execute_v1(ExpandedTrainingCommandV1::Update {
        line_b: None,
        source: v4.clone(),
        trajectories: vec![v4_trajectory.clone()],
        learning_rate: 0.0003,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::FixedPartition4,
        loss_selection: ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.95,
            entropy_coefficient: 0.0,
        },
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::V1,
        output_directory: root.join("update-v4"),
    })
    .unwrap();
    let parent: PinnedFileV1 = serde_json::from_value(updated["checkpoint"].clone()).unwrap();
    let v8_checkpoint = write_v8_checkpoint_v1(&parent, &root.join("v8.json"), 7)
        .unwrap()
        .pin;
    let v7 = source(FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7, None);
    let v8 = source(
        FEATURE_CONTRACT_DIGEST_V8,
        FEATURE_ENCODING_DIGEST_V8,
        Some(v8_checkpoint),
    );
    let v7_trajectory = collect(&v7, "v7", 2_026_110_602);
    let v8_trajectory = collect(&v8, "v8", 2_026_110_603);
    let audit = |label: &str, models: [&ExpandedModelSourceV1; 2], trajectory: &PinnedFileV1| {
        run(AuditCommand {
            trajectory_kind: TrajectoryKind::NativeExpandedV3,
            models: models
                .into_iter()
                .enumerate()
                .map(|(index, source)| Model::NativeExpanded {
                    label: format!("model-{index}"),
                    source: source.clone(),
                })
                .collect(),
            trajectories: vec![trajectory.clone()],
            output_directory: root.join(format!("audit-{label}")),
            workers: 1,
            max_choice_rows_per_trajectory: 256,
            minimum_behavior_replay_rows: 1,
        })
    };
    for (label, model, trajectory) in [
        ("v4", &v4, &v4_trajectory),
        ("v7", &v7, &v7_trajectory),
        ("v8", &v8, &v8_trajectory),
    ] {
        let completion =
            audit(label, [model, model], trajectory).unwrap_or_else(|e| panic!("{label}: {e}"));
        let rows = completion["choice_rows"].as_u64().unwrap();
        assert!(rows > 0, "{label}");
        // Both models are the collecting state: every row replays exactly.
        assert_eq!(
            completion["exact_behavior_replay_rows"].as_u64().unwrap(),
            2 * rows,
            "{label}"
        );
    }
    // The audited V8 rows include rows with a pairing relation.
    let saved: ExpandedTrajectoryV1 = read_pinned(&v8_trajectory).unwrap();
    let report: Value = serde_json::from_slice(
        &fs::read(root.join("audit-v8").join("trajectory-000.json")).unwrap(),
    )
    .unwrap();
    let paired = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            let index = row["archive_row"].as_u64().unwrap() as usize;
            !saved.decisions[index].tensor.pairing_node_indices.is_empty()
        })
        .count();
    eprintln!("audited FlatV8 rows with a pairing relation: {paired}");
    assert!(paired > 0);
    for (label, models, trajectory) in [
        ("v7-by-v8", [&v7, &v8], &v7_trajectory),
        ("v8-by-v7", [&v8, &v7], &v8_trajectory),
        ("v4-by-v7", [&v4, &v7], &v4_trajectory),
        ("v7-by-v4", [&v7, &v4], &v7_trajectory),
        ("v4-by-v8", [&v4, &v8], &v4_trajectory),
        ("v8-by-v4", [&v8, &v4], &v8_trajectory),
    ] {
        let refusal = audit(label, models, trajectory).unwrap_err();
        assert_eq!(
            refusal, "native trajectory/model feature identity differs",
            "{label}"
        );
        assert!(!root.join(format!("audit-{label}")).exists(), "{label}");
    }
    let _ = fs::remove_dir_all(&root);
}
