//! The trajectory batch bound of a prepared update
//! (`max_trajectory_batch_mebibytes`) through the real Collect and
//! UpdatePrepared commands: a batch just over the historical 512 MiB is
//! refused with the historical error when the key is absent (or 512), and
//! accepted, reaching the same state as the same games read plainly, when the
//! key allows it.

use super::*;
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
        pregame_advantage: PregameAdvantageV1::default(),
        pregame_announcement_step_scale: None,
    }
}

fn prepared(
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    max_trajectory_batch_mebibytes: usize,
    output_directory: PathBuf,
) -> ExpandedTrainingCommandV1 {
    ExpandedTrainingCommandV1::UpdatePrepared {
        source: source.clone(),
        trajectories: trajectories.to_vec(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        preparation_workers: 2,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        max_trajectory_batch_mebibytes,
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::default(),
        record_envelope: RecordEnvelopeV1::None,
        output_directory,
    }
}

/// A synthetic fresh source and three real games collected from it.
fn fixture(tag: &str) -> (PathBuf, ExpandedModelSourceV1, Vec<PinnedFileV1>) {
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
            id: format!("e-batch-bound-{index}"),
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

/// The same games as sealed files whose record bytes (each trajectory
/// followed by spaces, which parse to the same trajectory) total exactly one
/// byte over the historical batch bound.
fn padded_just_over_the_default_bound(
    root: &Path,
    trajectories: &[PinnedFileV1],
) -> Vec<PinnedFileV1> {
    let originals: Vec<Vec<u8>> = trajectories
        .iter()
        .map(|pin| fs::read(&pin.path).unwrap())
        .collect();
    let target = MAX_BATCH_BYTES + 1;
    let plain: u64 = originals.iter().map(|bytes| bytes.len() as u64).sum();
    let count = originals.len() as u64;
    let share = (target - plain) / count;
    let directory = root.join("padded");
    fs::create_dir_all(&directory).unwrap();
    let mut total = 0u64;
    let pins = originals
        .into_iter()
        .enumerate()
        .map(|(index, mut bytes)| {
            let pad = if index as u64 == count - 1 {
                target - plain - share * (count - 1)
            } else {
                share
            };
            bytes.resize(bytes.len() + pad as usize, b' ');
            total += bytes.len() as u64;
            let sealed = RecordEnvelopeV1::Zstd1.seal_owned_v1(bytes).unwrap();
            let path = directory.join(format!("episode-{index:04}.json"));
            fs::write(&path, &sealed).unwrap();
            PinnedFileV1 {
                path,
                sha256: sha(&sealed),
            }
        })
        .collect();
    assert_eq!(total, MAX_BATCH_BYTES + 1);
    pins
}

#[test]
fn an_update_just_over_the_default_batch_bound_runs_when_the_key_allows_it() {
    let (root, source, trajectories) = fixture("trajectory-batch-bound");
    let plain = execute_v1(prepared(
        &source,
        &trajectories,
        DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        root.join("plain"),
    ))
    .unwrap();
    let padded = padded_just_over_the_default_bound(&root, &trajectories);
    // Absent (the default) and an explicit 512 refuse it with the historical
    // error, before any file of the batch is opened.
    for (mebibytes, name) in [
        (DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES, "default"),
        (512, "explicit"),
    ] {
        assert_eq!(
            execute_v1(prepared(&source, &padded, mebibytes, root.join(name))).unwrap_err(),
            "trajectory batch exceeds 512 MiB CPU ingestion bound"
        );
    }
    // One MiB more admits it, and the update reaches the plain update's state.
    let raised = execute_v1(prepared(&source, &padded, 513, root.join("raised"))).unwrap();
    assert_eq!(raised["complete"], true);
    assert_eq!(raised["after_state_sha256"], plain["after_state_sha256"]);
    assert_eq!(raised["before_state_sha256"], plain["before_state_sha256"]);
    // A bound under the batch names itself.
    assert_eq!(
        execute_v1(prepared(&source, &padded, 1, root.join("small"))).unwrap_err(),
        "trajectory batch exceeds 1 MiB CPU ingestion bound"
    );
    // Out-of-range values are refused before any work.
    for refused in [0, 4097] {
        assert_eq!(
            execute_v1(prepared(
                &source,
                &trajectories,
                refused,
                root.join("refused")
            ))
            .unwrap_err(),
            "max_trajectory_batch_mebibytes must be in 1..=4096"
        );
    }
    // One preparation worker keeps the default, as on the run config; with
    // the default it still runs.
    let one_worker = |mebibytes, name: &str| match prepared(
        &source,
        &trajectories,
        mebibytes,
        root.join(name),
    ) {
        ExpandedTrainingCommandV1::UpdatePrepared {
            preparation_workers: _,
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            max_prepared_tensor_mebibytes,
            max_trajectory_batch_mebibytes,
            line_b,
            training_math,
            record_format,
            record_envelope,
            output_directory,
        } => execute_v1(ExpandedTrainingCommandV1::UpdatePrepared {
            preparation_workers: 1,
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            max_prepared_tensor_mebibytes,
            max_trajectory_batch_mebibytes,
            line_b,
            training_math,
            record_format,
            record_envelope,
            output_directory,
        }),
        _ => unreachable!(),
    };
    assert_eq!(
        one_worker(2048, "one-worker-raised").unwrap_err(),
        "max_trajectory_batch_mebibytes applies to prepared updates only (preparation_workers > 1)"
    );
    let serial = one_worker(DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES, "one-worker").unwrap();
    assert_eq!(serial["after_state_sha256"], plain["after_state_sha256"]);
    let _ = fs::remove_dir_all(&root);
}

/// Absent and the default share wire bytes; a raised bound is serialized and
/// read back as itself; an absent key reads back as 512.
#[test]
fn the_prepared_command_keeps_its_wire_bytes_at_the_default_bound() {
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: "import.json".into(),
            sha256: "0".repeat(64),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: "c".into(),
            expected_feature_encoding_digest: "e".into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    let default = serde_json::to_string(&prepared(
        &source,
        &[],
        DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        "out".into(),
    ))
    .unwrap();
    assert!(!default.contains("max_trajectory_batch_mebibytes"));
    let back: ExpandedTrainingCommandV1 = serde_json::from_str(&default).unwrap();
    match &back {
        ExpandedTrainingCommandV1::UpdatePrepared {
            max_trajectory_batch_mebibytes,
            ..
        } => assert_eq!(*max_trajectory_batch_mebibytes, 512),
        _ => unreachable!(),
    }
    assert_eq!(serde_json::to_string(&back).unwrap(), default);
    let raised = serde_json::to_string(&prepared(&source, &[], 2048, "out".into())).unwrap();
    let member = "\"max_trajectory_batch_mebibytes\":2048,";
    assert!(raised.contains(member));
    assert_eq!(raised.replace(member, ""), default);
    let back: ExpandedTrainingCommandV1 = serde_json::from_str(&raised).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), raised);
}
