//! The v2 writer through the real Collect and Update commands: a v2 update
//! publishes a container whose export equals the v1 update's checkpoint byte
//! for byte, with the same `after_state_sha256`, and the next update from
//! either checkpoint reaches the same state.

use super::super::*;
use super::{admit_writer_v1, convert_v1_bytes, export_v1, is_container_v1};
use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;

fn list(id: &str) -> ExpandedDeckListV1 {
    let d = checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let c = d.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: c.mainboard().to_vec(),
        sideboard: c.sideboard().to_vec(),
    }
}

fn gae() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
    }
}

fn update(
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    record_format: CheckpointRecordFormatV1,
    output_directory: PathBuf,
) -> Result<Value, String> {
    execute_v1(ExpandedTrainingCommandV1::Update {
        source: source.clone(),
        trajectories: trajectories.to_vec(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format,
        output_directory,
    })
}

#[test]
fn a_v2_update_publishes_a_container_equal_to_the_v1_checkpoint_and_resumes_identically() {
    let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
    let root = std::env::temp_dir().join(format!(
        "checkpoint-container-update-{}",
        std::process::id()
    ));
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
    };
    let decks = [list("Affinity"), list("Terror")];
    let episode = ExpandedEpisodeV1 {
        id: "e-checkpoint-container".into(),
        seed: 2026110101,
        starting_player: 0,
        learner_seat: 0,
        opponent: Some(source.clone()),
        opponent_search: None,
        opponent_kind: None,
        registered: decks.clone(),
        selected: decks,
        postboard: false,
        max_physical_decisions: 100000,
        max_policy_steps: 1000000,
        runtime_rules: None,
        reset: None,
        opening: None,
        mulligan_modes: None,
    };
    let collect = |source: &ExpandedModelSourceV1, label: &str| -> Vec<PinnedFileV1> {
        let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
            source: source.clone(),
            episodes: vec![ExpandedEpisodeV1 {
                opponent: Some(source.clone()),
                ..episode.clone()
            }],
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            output_directory: root.join(label),
        })
        .unwrap();
        serde_json::from_value(collection["trajectories"].clone()).unwrap()
    };
    let trajectories = collect(&source, "collect");

    let v1 = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V1,
        root.join("v1"),
    )
    .unwrap();
    let v2 = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V2,
        root.join("v2"),
    )
    .unwrap();
    assert_eq!(v1["after_state_sha256"], v2["after_state_sha256"]);
    let pin_v1: PinnedFileV1 = serde_json::from_value(v1["checkpoint"].clone()).unwrap();
    let pin_v2: PinnedFileV1 = serde_json::from_value(v2["checkpoint"].clone()).unwrap();
    let bytes_v1 = fs::read(&pin_v1.path).unwrap();
    let bytes_v2 = fs::read(&pin_v2.path).unwrap();
    assert!(is_container_v1(&bytes_v2) && !is_container_v1(&bytes_v1));
    assert_eq!(export_v1(&bytes_v2).unwrap(), bytes_v1);
    assert_eq!(convert_v1_bytes(&bytes_v1).unwrap(), bytes_v2);
    assert_eq!(pin_v2.sha256, hex(&Sha256::digest(&bytes_v2)));

    // Every reader interface gives the v1 answer for the container.
    let typed: ExpandedCheckpointV1 = read_pinned(&pin_v2).unwrap();
    assert_eq!(serde_json::to_vec(&typed).unwrap(), bytes_v1);
    assert_eq!(
        serde_json::to_vec(&read_checkpoint_v1(&pin_v2).unwrap()).unwrap(),
        bytes_v1
    );
    assert_eq!(
        checkpoint_feature_contract_v1(&pin_v2).unwrap(),
        checkpoint_feature_contract_v1(&pin_v1).unwrap()
    );
    let probe: CheckpointESupportedProbeV1 = read_checkpoint_metadata_v1(&pin_v2).unwrap();
    assert!(probe.e_supported.is_none() && probe.ex1.is_none());
    assert!(!no_state_digest_v1::names_another_file_v1(&bytes_v2).unwrap());
    // A truncated container is refused by the metadata view as by a load.
    let header_end = bytes_v2.len() - 4;
    assert!(checkpoint_container_v1::metadata_view_v1(&bytes_v2[..header_end]).is_err());

    // The converter (restoring both through the loader) and the exporter
    // reproduce both files.
    let from = |pin: &PinnedFileV1| ExpandedModelSourceV1 {
        checkpoint: Some(pin.clone()),
        ..source.clone()
    };
    let converted =
        convert_checkpoint_v1_to_v2(&from(&pin_v1), &root.join("converted.ckpt")).unwrap();
    let converted: PinnedFileV1 = serde_json::from_value(converted["output"].clone()).unwrap();
    assert_eq!(fs::read(&converted.path).unwrap(), bytes_v2);
    assert_eq!(converted.sha256, pin_v2.sha256);
    let exported = export_checkpoint_v2_to_v1(&pin_v2, &root.join("exported.json")).unwrap();
    let exported: PinnedFileV1 = serde_json::from_value(exported["output"].clone()).unwrap();
    assert_eq!(exported.sha256, pin_v1.sha256);
    assert!(
        export_checkpoint_v2_to_v1(&pin_v1, &root.join("not-a-container.json"))
            .unwrap_err()
            .contains("not a checkpoint container")
    );
    assert!(
        convert_checkpoint_v1_to_v2(&from(&pin_v2), &root.join("twice.ckpt"))
            .unwrap_err()
            .contains("already a checkpoint container")
    );

    // The next update from either checkpoint, on trajectories collected from
    // that same checkpoint, reaches the same state, in either format and
    // across formats.
    let next_from_v1 = from(&pin_v1);
    let next_from_v2 = from(&pin_v2);
    let trajectories_v1 = collect(&next_from_v1, "collect-from-v1");
    let trajectories_v2 = collect(&next_from_v2, "collect-from-v2");
    let next_v1 = update(
        &next_from_v1,
        &trajectories_v1,
        CheckpointRecordFormatV1::V1,
        root.join("next-v1"),
    )
    .unwrap();
    let next_v2 = update(
        &next_from_v2,
        &trajectories_v2,
        CheckpointRecordFormatV1::V2,
        root.join("next-v2"),
    )
    .unwrap();
    let cross = update(
        &next_from_v2,
        &trajectories_v2,
        CheckpointRecordFormatV1::V1,
        root.join("cross"),
    )
    .unwrap();
    assert_eq!(next_v1["after_state_sha256"], next_v2["after_state_sha256"]);
    assert_eq!(next_v1["after_state_sha256"], cross["after_state_sha256"]);
    let next_pin_v2: PinnedFileV1 = serde_json::from_value(next_v2["checkpoint"].clone()).unwrap();
    let cross_pin: PinnedFileV1 = serde_json::from_value(cross["checkpoint"].clone()).unwrap();
    // Same inputs, different format: the container exports the v1 bytes.
    assert_eq!(
        export_v1(&fs::read(&next_pin_v2.path).unwrap()).unwrap(),
        fs::read(&cross_pin.path).unwrap()
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_v2_writer_refuses_kinds_outside_its_whitelist() {
    let (_, _, saved) = crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1();
    // The fixture is a V3 checkpoint: outside the whitelist.
    assert!(admit_writer_v1(&saved)
        .unwrap_err()
        .contains("keeps record_format v1"));
    let mut v4 = saved;
    v4.schema = FRESH_CHECKPOINT_SCHEMA.into();
    v4.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V4.into();
    v4.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
    v4.loss_identity = "terminal_reinforce_value/v3".into();
    admit_writer_v1(&v4).unwrap();
    let refused = |edit: fn(&mut ExpandedCheckpointV1)| {
        let mut c = v4.clone();
        edit(&mut c);
        admit_writer_v1(&c).is_err()
    };
    assert!(refused(|c| c.ex1 = Some(json!({}))));
    assert!(refused(|c| c.e_supported = Some(json!({}))));
    assert!(refused(|c| c.soft_q = Some(json!({}))));
    assert!(refused(|c| c.search_targets = Some(json!({}))));
    assert!(refused(|c| {
        c.loss_identity = "gae_uniform_kl_recovery/v1".into()
    }));
    assert!(refused(|c| c.schema = CHECKPOINT_SCHEMA.into()));
    assert!(refused(|c| c.training_math = TrainingMathV1::PortableV1));
}

#[test]
fn record_format_v1_serializes_like_absence() {
    let explicit: CheckpointRecordFormatV1 = serde_json::from_str("\"v1\"").unwrap();
    assert!(explicit.is_v1());
    assert_eq!(
        serde_json::from_str::<CheckpointRecordFormatV1>("\"v2\"").unwrap(),
        CheckpointRecordFormatV1::V2
    );
    assert!(serde_json::from_str::<CheckpointRecordFormatV1>("\"v3\"").is_err());
    let command = |record_format| ExpandedTrainingCommandV1::Update {
        source: ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: "import.json".into(),
                sha256: "0".repeat(64),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
            },
            checkpoint: None,
        },
        trajectories: Vec::new(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format,
        output_directory: "out".into(),
    };
    let v1 = serde_json::to_string(&command(CheckpointRecordFormatV1::V1)).unwrap();
    assert!(!v1.contains("record_format"));
    let v2 = serde_json::to_string(&command(CheckpointRecordFormatV1::V2)).unwrap();
    assert!(v2.contains("\"record_format\":\"v2\""));
    let back: ExpandedTrainingCommandV1 = serde_json::from_str(&v1).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), v1);
}
