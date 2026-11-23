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
        pregame_advantage: crate::expanded_deck_training_v1::PregameAdvantageV1::default(),
    }
}

fn update(
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    record_format: CheckpointRecordFormatV1,
    output_directory: PathBuf,
) -> Result<Value, String> {
    execute_v1(ExpandedTrainingCommandV1::Update {
        card_semantics: None,
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

/// A synthetic fresh source of `feature_identity` and a collector over it.
fn fixture(
    feature_identity: crate::sideboard_play_policy_v1::FreshFeatureIdentityV1,
    tag: &str,
) -> (
    crate::test_temp_dir_v1::TempTestDir,
    ExpandedModelSourceV1,
    impl Fn(&ExpandedModelSourceV1, &str) -> Vec<PinnedFileV1>,
) {
    let root = crate::test_temp_dir_v1::TempTestDir::new_v1(tag);
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
        w_learner: None,
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
        priority_windows: None,
        london_keep_offset: None,
    };
    let collect_root = root.to_path_buf();
    let collect = move |source: &ExpandedModelSourceV1, label: &str| -> Vec<PinnedFileV1> {
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
            output_directory: collect_root.join(label),
        })
        .unwrap();
        serde_json::from_value(collection["trajectories"].clone()).unwrap()
    };
    (root, source, collect)
}

#[test]
fn a_v2_update_publishes_a_container_equal_to_the_v1_checkpoint_and_resumes_identically() {
    let (root, source, collect) = fixture(
        crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4,
        "checkpoint-container-update",
    );
    let trajectories = collect(&source, "collect");

    let v1 = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V1,
        root.join("v1"),
    )
    .unwrap();
    checkpoint_metadata_memo_v1::clear_for_test_v1();
    let v2 = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V2,
        root.join("v2"),
    )
    .unwrap();
    // The v2 update itself remembered its container's metadata view after
    // its readback (no metadata probe has run since the memo was cleared).
    let pin_v2_now: PinnedFileV1 = serde_json::from_value(v2["checkpoint"].clone()).unwrap();
    let remembered = checkpoint_metadata_memo_v1::lookup_v1(&pin_v2_now.sha256)
        .expect("the update remembers its container's metadata view");
    assert_eq!(
        remembered.as_slice(),
        checkpoint_container_v1::metadata_view_v1(&fs::read(&pin_v2_now.path).unwrap())
            .unwrap()
            .as_ref()
    );
    // The default (an absent field) publishes the same container as an
    // explicit v2 for this in-scope kind.
    let auto = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::default(),
        root.join("auto"),
    )
    .unwrap();
    assert_eq!(auto["checkpoint"]["sha256"], v2["checkpoint"]["sha256"]);
    assert_eq!(v1["after_state_sha256"], auto["after_state_sha256"]);
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
    // What the update remembers (the record's document, taken before
    // sealing) is the view a fresh probe takes from a sealed file too.
    let sealed = record_envelope_v1::RecordEnvelopeV1::Zstd1
        .seal_v1(&bytes_v2)
        .unwrap();
    assert!(record_envelope_v1::is_sealed_v1(&sealed));
    assert_eq!(
        checkpoint_container_v1::metadata_view_v1(&sealed)
            .unwrap()
            .as_ref(),
        checkpoint_container_v1::document_v1(&bytes_v2).unwrap()
    );
    // The remembered view is exactly the view a fresh probe takes from the
    // file.
    let view = checkpoint_container_v1::metadata_view_v1(&bytes_v2).unwrap();
    assert_eq!(
        checkpoint_metadata_memo_v1::lookup_v1(&pin_v2.sha256).as_deref(),
        Some(&view.to_vec())
    );
    // A probe parses the remembered view only after the file check: a copy
    // with one byte changed under the same pin is refused, with or without
    // the memo, and a probe without the memo gives the remembered answer.
    let changed = root.join("changed.ckpt");
    let mut altered = bytes_v2.clone();
    let last = altered.len() - 1;
    altered[last] ^= 1;
    fs::write(&changed, &altered).unwrap();
    let changed_pin = PinnedFileV1 {
        path: changed.clone(),
        sha256: pin_v2.sha256.clone(),
    };
    assert!(read_checkpoint_metadata_v1::<CheckpointESupportedProbeV1>(&changed_pin).is_err());
    checkpoint_metadata_memo_v1::clear_for_test_v1();
    assert!(read_checkpoint_metadata_v1::<CheckpointESupportedProbeV1>(&changed_pin).is_err());
    let cold: CheckpointESupportedProbeV1 = read_checkpoint_metadata_v1(&pin_v2).unwrap();
    assert_eq!(
        (cold.e_supported, cold.soft_q, cold.search_targets, cold.ex1),
        (
            probe.e_supported,
            probe.soft_q,
            probe.search_targets,
            probe.ex1
        )
    );
    assert_eq!(
        checkpoint_metadata_memo_v1::lookup_v1(&pin_v2.sha256).as_deref(),
        Some(&view.to_vec())
    );
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

    // Sealed checkpoints (`record_envelope_v1`) of either format give the
    // same answers through every reader, the converter and the exporter.
    let sealed_pin = |bytes: &[u8], name: &str| {
        let sealed = crate::expanded_deck_training_v1::RecordEnvelopeV1::Zstd1
            .seal_v1(bytes)
            .unwrap()
            .into_owned();
        let path = root.join(name);
        fs::write(&path, &sealed).unwrap();
        PinnedFileV1 {
            path,
            sha256: hex(&Sha256::digest(&sealed)),
        }
    };
    let sealed_v1 = sealed_pin(&bytes_v1, "sealed-v1.json");
    let sealed_v2 = sealed_pin(&bytes_v2, "sealed-v2.json");
    for pin in [&sealed_v1, &sealed_v2] {
        assert_eq!(
            serde_json::to_vec(&read_checkpoint_v1(pin).unwrap()).unwrap(),
            bytes_v1
        );
        let typed: ExpandedCheckpointV1 = read_pinned(pin).unwrap();
        assert_eq!(serde_json::to_vec(&typed).unwrap(), bytes_v1);
        assert_eq!(
            checkpoint_feature_contract_v1(pin).unwrap(),
            checkpoint_feature_contract_v1(&pin_v1).unwrap()
        );
    }
    let converted =
        convert_checkpoint_v1_to_v2(&from(&sealed_v1), &root.join("sealed.ckpt")).unwrap();
    let converted: PinnedFileV1 = serde_json::from_value(converted["output"].clone()).unwrap();
    assert_eq!(converted.sha256, pin_v2.sha256);
    let exported =
        export_checkpoint_v2_to_v1(&sealed_v2, &root.join("sealed-exported.json")).unwrap();
    let exported: PinnedFileV1 = serde_json::from_value(exported["output"].clone()).unwrap();
    assert_eq!(exported.sha256, pin_v1.sha256);

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
}

/// A real update of a kind outside the gate (the V3 fresh identity): the
/// default keeps writing v1 JSON, an explicit v1 does the same, and an
/// explicit v2 is refused by name without publishing anything.
#[test]
fn the_default_keeps_v1_outside_the_gate_and_explicit_v2_is_refused() {
    let (root, source, collect) = fixture(
        crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3,
        "checkpoint-container-default-outside-gate",
    );
    let trajectories = collect(&source, "collect");
    let auto = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::default(),
        root.join("auto"),
    )
    .unwrap();
    let explicit = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V1,
        root.join("v1"),
    )
    .unwrap();
    let pin: PinnedFileV1 = serde_json::from_value(auto["checkpoint"].clone()).unwrap();
    let bytes = fs::read(&pin.path).unwrap();
    assert!(!is_container_v1(&bytes) && bytes.starts_with(b"{"));
    assert_eq!(
        auto["checkpoint"]["sha256"],
        explicit["checkpoint"]["sha256"]
    );
    let refused = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V2,
        root.join("v2"),
    )
    .unwrap_err();
    assert!(refused.contains("keeps record_format v1"), "{refused}");
    assert!(!root.join("v2").join("checkpoint.json").exists());
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

    // The selection resolves on this checkpoint: the default and v1 stay v1
    // outside the gate, and v2 is refused by name.
    use CheckpointRecordFormatV1::{Auto, V1, V2};
    assert_eq!(Auto.resolve_v1(&v4).unwrap(), V2);
    assert_eq!(V1.resolve_v1(&v4).unwrap(), V1);
    assert_eq!(V2.resolve_v1(&v4).unwrap(), V2);
    let mut outside = v4.clone();
    outside.ex1 = Some(json!({}));
    assert_eq!(Auto.resolve_v1(&outside).unwrap(), V1);
    assert_eq!(V1.resolve_v1(&outside).unwrap(), V1);
    assert!(V2
        .resolve_v1(&outside)
        .unwrap_err()
        .contains("keeps record_format v1"));
}

#[test]
fn the_default_record_format_is_never_serialized() {
    // Absence and an explicit "auto" are the default; "v1" and "v2" are explicit.
    assert_eq!(
        serde_json::from_str::<CheckpointRecordFormatV1>("\"auto\"").unwrap(),
        CheckpointRecordFormatV1::default()
    );
    assert!(CheckpointRecordFormatV1::default().is_auto());
    assert_eq!(
        serde_json::from_str::<CheckpointRecordFormatV1>("\"v1\"").unwrap(),
        CheckpointRecordFormatV1::V1
    );
    assert_eq!(
        serde_json::from_str::<CheckpointRecordFormatV1>("\"v2\"").unwrap(),
        CheckpointRecordFormatV1::V2
    );
    assert!(serde_json::from_str::<CheckpointRecordFormatV1>("\"v3\"").is_err());
    let command = |record_format| ExpandedTrainingCommandV1::Update {
        card_semantics: None,
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
            s_learner: None,
            w_learner: None,
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
    // The default serializes exactly as the pre-switch absent field did: the
    // explicit v2 bytes without the member.
    let auto = serde_json::to_string(&command(CheckpointRecordFormatV1::default())).unwrap();
    assert!(!auto.contains("record_format"));
    let v2 = serde_json::to_string(&command(CheckpointRecordFormatV1::V2)).unwrap();
    assert!(v2.contains("\"record_format\":\"v2\""));
    assert_eq!(v2.replace("\"record_format\":\"v2\",", ""), auto);
    // An explicit v1 is serialized (it no longer shares bytes with absence)
    // and reads back as itself; absence reads back as the default.
    let v1 = serde_json::to_string(&command(CheckpointRecordFormatV1::V1)).unwrap();
    assert!(v1.contains("\"record_format\":\"v1\""));
    assert_eq!(v1.replace("\"record_format\":\"v1\",", ""), auto);
    for text in [&auto, &v1, &v2] {
        let back: ExpandedTrainingCommandV1 = serde_json::from_str(text).unwrap();
        assert_eq!(&serde_json::to_string(&back).unwrap(), text);
    }
    // The prepared command follows the same rule.
    let prepared = |record_format| ExpandedTrainingCommandV1::UpdatePrepared {
        card_semantics: None,
        source: match command(CheckpointRecordFormatV1::default()) {
            ExpandedTrainingCommandV1::Update { source, .. } => source,
            _ => unreachable!(),
        },
        trajectories: Vec::new(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        preparation_workers: 2,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        max_trajectory_batch_mebibytes: DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format,
        record_envelope: Default::default(),
        output_directory: "out".into(),
    };
    let auto = serde_json::to_string(&prepared(CheckpointRecordFormatV1::default())).unwrap();
    assert!(!auto.contains("record_format"));
    for (format, name) in [
        (CheckpointRecordFormatV1::V1, "v1"),
        (CheckpointRecordFormatV1::V2, "v2"),
    ] {
        let text = serde_json::to_string(&prepared(format)).unwrap();
        let member = format!("\"record_format\":\"{name}\",");
        assert!(text.contains(&member));
        assert_eq!(text.replace(&member, ""), auto);
        let back: ExpandedTrainingCommandV1 = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), text);
    }
}

#[test]
fn checkpoint_metadata_memo_bounds_size_and_lru_entries() {
    checkpoint_metadata_memo_v1::clear_for_test_v1();
    // Views up to 1 MiB are retained; larger views (v1 JSON checkpoints) are not.
    let at_limit = vec![b'a'; 1 << 20];
    let over_limit = vec![b'b'; (1 << 20) + 1];
    checkpoint_metadata_memo_v1::remember_v1("at-limit", &at_limit);
    checkpoint_metadata_memo_v1::remember_v1("over-limit", &over_limit);
    assert_eq!(
        checkpoint_metadata_memo_v1::lookup_v1("at-limit").as_deref(),
        Some(&at_limit)
    );
    assert!(checkpoint_metadata_memo_v1::lookup_v1("over-limit").is_none());

    // Four-entry LRU: refreshing k0 before inserting k4 evicts k1 instead of k0.
    checkpoint_metadata_memo_v1::clear_for_test_v1();
    for key in ["k0", "k1", "k2", "k3"] {
        checkpoint_metadata_memo_v1::remember_v1(key, key.as_bytes());
    }
    assert_eq!(
        checkpoint_metadata_memo_v1::lookup_v1("k0")
            .as_deref()
            .map(Vec::as_slice),
        Some(b"k0".as_slice())
    );
    checkpoint_metadata_memo_v1::remember_v1("k4", b"k4");
    assert!(checkpoint_metadata_memo_v1::lookup_v1("k1").is_none());
    for key in ["k0", "k2", "k3", "k4"] {
        assert_eq!(
            checkpoint_metadata_memo_v1::lookup_v1(key)
                .as_deref()
                .map(Vec::as_slice),
            Some(key.as_bytes())
        );
    }
}

#[test]
fn projection_prepared_update_matches_serial_and_flag_off_legacy_path() {
    let (root, source, collect) = fixture(
        crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4,
        "projection-prepared",
    );
    let trajectories = collect(&source, "collect");
    // Guarantee that both worker pools cross their 32-substep thresholds.
    // Copies retain the real replay and differ only in the episode label.
    let trajectory: ExpandedTrajectoryV1 = read_pinned(&trajectories[0]).unwrap();
    let learner_steps = trajectory
        .decisions
        .iter()
        .filter(|row| row.actor == trajectory.episode.learner_seat)
        .count();
    assert!(learner_steps > 0);
    let trajectories: Vec<_> = (0..128usize.div_ceil(learner_steps))
        .map(|index| {
            let mut copy = trajectory.clone();
            copy.episode.id = format!("projection-workers-{index}");
            publish_json(&root, &format!("trajectory-{index}.json"), &copy).unwrap()
        })
        .collect();
    let config = card_semantics_e1::fixture_table_v1(&root);
    let command = |workers, semantics, label: &str| ExpandedTrainingCommandV1::UpdatePrepared {
        card_semantics: semantics,
        source: source.clone(),
        trajectories: trajectories.clone(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        preparation_workers: workers,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        max_trajectory_batch_mebibytes: DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::V1,
        record_envelope: Default::default(),
        output_directory: root.join(label),
    };
    let serial = execute_v1(command(1, Some(config.clone()), "on-serial")).unwrap();
    crate::native_policy_train_step_v1::take_effective_update_workers_for_test_v1();
    let parallel = execute_v1(command(4, Some(config.clone()), "on-parallel")).unwrap();
    let (forward_workers, _) =
        crate::native_policy_train_step_v1::take_effective_update_workers_for_test_v1();
    assert!(forward_workers > 1);
    assert!(
        parallel["update_preparation"]["started_workers"]
            .as_u64()
            .unwrap()
            > 1
    );
    assert_eq!(
        serial["checkpoint"]["sha256"],
        parallel["checkpoint"]["sha256"]
    );
    let mut fixed_serial = command(1, Some(config.clone()), "fixed-serial");
    let mut fixed_parallel = command(4, Some(config.clone()), "fixed-parallel");
    for cmd in [&mut fixed_serial, &mut fixed_parallel] {
        if let ExpandedTrainingCommandV1::UpdatePrepared {
            update_backward_execution,
            ..
        } = cmd
        {
            *update_backward_execution = UpdateBackwardExecutionV1::FixedPartition4;
        }
    }
    set_fixed_partition_backward_worker_limit_override_for_test_v1(Some(1));
    let fixed_serial_result = execute_v1(fixed_serial);
    set_fixed_partition_backward_worker_limit_override_for_test_v1(None);
    let fixed_serial = fixed_serial_result.unwrap();
    crate::native_policy_train_step_v1::take_effective_update_workers_for_test_v1();
    let fixed_parallel = execute_v1(fixed_parallel).unwrap();
    let workers = crate::native_policy_train_step_v1::take_effective_update_workers_for_test_v1();
    assert!(workers.0 > 1);
    assert_eq!(
        fixed_serial["checkpoint"]["sha256"],
        fixed_parallel["checkpoint"]["sha256"]
    );
    assert_eq!(
        serial["card_semantics"]["binding"],
        config.load_v1().unwrap().section_v1().binding
    );
    let off = execute_v1(command(4, None, "off-prepared")).unwrap();
    let legacy = update(
        &source,
        &trajectories,
        CheckpointRecordFormatV1::V1,
        root.join("off-legacy"),
    )
    .unwrap();
    assert_eq!(off["checkpoint"]["sha256"], legacy["checkpoint"]["sha256"]);
    assert!(off.get("card_semantics").is_none());
    for workers in [1, 4] {
        let mut value = serde_json::to_value(command(workers, None, "null")).unwrap();
        value["card_semantics"] = Value::Null;
        assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(value)
            .unwrap_err()
            .to_string()
            .contains("must not be null"));
    }
    let mut plain_update = serde_json::to_value(ExpandedTrainingCommandV1::Update {
        card_semantics: None,
        source: source.clone(),
        trajectories: trajectories.clone(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::V1,
        output_directory: root.join("null-update"),
    })
    .unwrap();
    plain_update["card_semantics"] = Value::Null;
    assert!(
        serde_json::from_value::<ExpandedTrainingCommandV1>(plain_update)
            .unwrap_err()
            .to_string()
            .contains("must not be null")
    );

    // Exercise update admission, not just the shared binder helper.
    let on_pin: PinnedFileV1 = serde_json::from_value(serial["checkpoint"].clone()).unwrap();
    let off_pin: PinnedFileV1 = serde_json::from_value(off["checkpoint"].clone()).unwrap();
    for (label, pin, semantics, expected) in [
        ("missing", off_pin, Some(config.clone()), "section missing"),
        ("off", on_pin.clone(), None, "requires"),
    ] {
        let mut cmd = command(1, semantics, label);
        if let ExpandedTrainingCommandV1::UpdatePrepared { source, .. } = &mut cmd {
            source.checkpoint = Some(pin);
        }
        take_checkpoint_metadata_reads_for_test_v1();
        assert!(execute_v1(cmd).unwrap_err().contains(expected));
        assert_eq!(
            take_checkpoint_metadata_reads_for_test_v1(),
            1,
            "{label}: reuse the existing update probe"
        );
    }
    let valid = read_checkpoint_v1(&on_pin).unwrap();
    assert_eq!(valid.card_semantics.as_ref().unwrap().p_step, 1);
    for field in ["binding", "schema"] {
        let mut saved = valid.clone();
        let section = saved.card_semantics.as_mut().unwrap();
        if field == "binding" {
            section.binding = "wrong".into();
        } else {
            section.schema = "wrong".into();
        }
        let (_, state) = initialize_any_math_v1(&source).unwrap();
        let hash = checkpoint_tensors_v1(&saved, state.model_v1())
            .unwrap()
            .state_sha256_v1()
            .unwrap();
        saved.state_sha256 = checkpoint_hash_v1(&hash, saved.card_semantics.as_ref()).unwrap();
        let pin = publish_json(&root, &format!("{field}.json"), &saved).unwrap();
        let mut cmd = command(1, Some(config.clone()), field);
        if let ExpandedTrainingCommandV1::UpdatePrepared { source, .. } = &mut cmd {
            source.checkpoint = Some(pin);
        }
        assert!(execute_v1(cmd).unwrap_err().contains("binding"));
    }
}
