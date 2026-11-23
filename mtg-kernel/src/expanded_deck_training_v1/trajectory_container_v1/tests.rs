use super::*;
use crate::expanded_deck_training_v1::{
    checkpoint_container_v1::CheckpointRecordFormatV1, execute_v1, fresh_initialization_source,
    record_envelope_v1::RecordEnvelopeV1, sha, tests::replay_fixture, CollectionSamplerV1,
    ExpandedDeckListV1, ExpandedEpisodeV1, ExpandedLossSelectionV1, ExpandedModelSourceV1,
    ExpandedTrainingCommandV1, ExpandedUpdateBackendV1, PinnedFileV1, UpdateBackwardExecutionV1,
    DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES, DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
};
use crate::sideboard_play_policy_v1::{
    FrozenPlayObservationTransferV3, FrozenPlayPolicyV1, FRESH_FEATURE_IDENTITY_V4,
};
use std::fs;

#[test]
fn binary_and_zstd1_containers_roundtrip_bit_identical_including_edge_bits() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (mut t, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0, 1]);
    // Inject adversarial bit patterns: -0.0, NaN payload, subnormal, large i64 (> i32::MAX), FlatV8 pairing.
    t.decisions[0].tensor.state[0] = (-0.0f32).to_bits();
    t.decisions[0].tensor.state[1] = f32::from_bits(1).to_bits();
    t.decisions[0].tensor.state[2] = 0x7fc0_1234; // non-canonical NaN payload
    t.decisions[0]
        .tensor
        .object_node_ids
        .push(i64::from(i32::MAX) + 7);
    t.decisions[0].tensor.pairing_node_indices = vec![0, 1, 2, 3];
    t.decisions[0].sampler_identity = Some("wide-categorical-v1".into());

    let json_bytes = serde_json::to_vec(&t).unwrap();
    for format in [
        TrajectoryRecordFormatV1::BinaryV1,
        TrajectoryRecordFormatV1::BinaryZstd1,
    ] {
        let encoded = encode_v1(&t, format).unwrap();
        assert!(is_container_v1(&encoded));
        assert!(
            encoded.len() < json_bytes.len(),
            "{format:?} should be smaller than JSON ({} vs {})",
            encoded.len(),
            json_bytes.len()
        );
        let decoded = decode_any_v1(&encoded, MAX_FILE_BYTES).unwrap();
        assert_eq!(decoded, t, "roundtrip mismatch for {format:?}");

        // O(1) header-only validation read matches full JSON parse.
        let fields_bin = read_any_collection_fields_v1(&encoded, MAX_FILE_BYTES).unwrap();
        let fields_json = read_any_collection_fields_v1(&json_bytes, MAX_FILE_BYTES).unwrap();
        assert_eq!(fields_bin, fields_json);
    }

    // Learner-only strips opponent tensors while preserving learner tensors and opponent metadata.
    let lo_bytes = encode_v1(&t, TrajectoryRecordFormatV1::BinaryZstd1LearnerOnly).unwrap();
    let lo = decode_any_v1(&lo_bytes, MAX_FILE_BYTES).unwrap();
    assert!(lo.opponent_tensors_omitted);
    assert_eq!(lo.decisions.len(), t.decisions.len());
    for (orig, dec) in t.decisions.iter().zip(&lo.decisions) {
        assert_eq!(dec.step, orig.step);
        assert_eq!(dec.actor, orig.actor);
        assert_eq!(dec.selected, orig.selected);
        assert_eq!(dec.logits, orig.logits);
        assert_eq!(dec.value, orig.value);
        if orig.actor == t.episode.learner_seat {
            assert_eq!(dec.tensor, orig.tensor);
        } else {
            assert!(dec.tensor.is_empty_v1());
        }
    }
}

#[test]
fn container_reader_fails_closed_on_corruption_truncation_and_flag_mismatch() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (t, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1]);
    let encoded = encode_v1(&t, TrajectoryRecordFormatV1::BinaryZstd1).unwrap();

    // 1. Truncation
    assert!(decode_v1(&encoded[..encoded.len() - 1], MAX_FILE_BYTES).is_err());
    // 2. Trailing garbage
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(decode_v1(&trailing, MAX_FILE_BYTES).is_err());
    // 3. Unknown flag bit
    let mut bad_flags = encoded.clone();
    bad_flags[12] |= 0x80;
    assert!(decode_v1(&bad_flags, MAX_FILE_BYTES).is_err());
    // 4. Flipped byte in body fails zstd checksum / SHA-256
    let mut flipped_body = encoded.clone();
    let last = flipped_body.len() - 1;
    flipped_body[last] ^= 0xff;
    assert!(decode_v1(&flipped_body, MAX_FILE_BYTES).is_err());
    // 5. Flipped byte in uncompressed container body caught by body_sha256
    let raw_enc = encode_v1(&t, TrajectoryRecordFormatV1::BinaryV1).unwrap();
    let mut flipped_raw = raw_enc.clone();
    let last_raw = flipped_raw.len() - 1;
    flipped_raw[last_raw] ^= 0x01;
    assert!(decode_v1(&flipped_raw, MAX_FILE_BYTES).is_err());
    // 6. Size limit enforced before decompressing body
    assert!(decode_v1(&encoded, 64).is_err());
    assert!(read_collection_fields_v1(&encoded, 64).is_err());
    // 7. Header-only read succeeds even when body bytes are not decompressed (only framing checked)
    assert!(read_collection_fields_v1(&encoded, MAX_FILE_BYTES).is_ok());
}

#[test]
fn end_to_end_binary_and_learner_only_trajectories_match_json_state_sha256_bit_for_bit() {
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(end_to_end_binary_and_learner_only_trajectories_match_json_state_sha256_bit_for_bit_body)
        .unwrap();
    if let Err(payload) = handle.join() {
        std::panic::resume_unwind(payload);
    }
}

fn end_to_end_binary_and_learner_only_trajectories_match_json_state_sha256_bit_for_bit_body() {
    let root = crate::test_temp_dir_v1::TempTestDir::new_v1("traj-container-e2e");
    let source_struct =
        fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
            &root.join("source"),
            FRESH_FEATURE_IDENTITY_V4,
            |_parameters| {},
        );
    let descriptor_path = root.join("descriptor.json");
    let descriptor_bytes = serde_json::to_vec(&source_struct).unwrap();
    fs::write(&descriptor_path, &descriptor_bytes).unwrap();
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: descriptor_path.canonicalize().unwrap(),
            sha256: sha(&descriptor_bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FRESH_FEATURE_IDENTITY_V4
                .feature_contract_digest
                .into(),
            expected_feature_encoding_digest: FRESH_FEATURE_IDENTITY_V4
                .feature_encoding_digest
                .into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    };
    let deck = |id: &str| {
        let registration =
            crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
        let cards = registration.registered_configuration();
        ExpandedDeckListV1 {
            label: id.into(),
            mainboard: cards.mainboard().to_vec(),
            sideboard: cards.sideboard().to_vec(),
        }
    };
    let decks = [deck("Affinity"), deck("Terror")];
    let episodes: Vec<_> = (0..2u64)
        .map(|i| ExpandedEpisodeV1 {
            id: format!("traj-e2e-{i}"),
            seed: 2_026_112_310 + i,
            starting_player: (i % 2) as u8,
            learner_seat: 0,
            opponent: None,
            opponent_kind: None,
            opponent_search: None,
            registered: decks.clone(),
            selected: decks.clone(),
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
            priority_windows: None,
            london_keep_offset: None,
        })
        .collect();

    let gae = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
        pregame_advantage: Default::default(),
    };

    let run_format = |tag: &str, format: TrajectoryRecordFormatV1| -> (String, u64) {
        let collect = execute_v1(ExpandedTrainingCommandV1::CollectParallel {
            source: source.clone(),
            episodes: episodes.clone(),
            workers: 2,
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            record_envelope: RecordEnvelopeV1::None,
            trajectory_format: format,
            output_directory: root.join(format!("collect-{tag}")),
        })
        .unwrap();
        let pins: Vec<PinnedFileV1> =
            serde_json::from_value(collect["trajectories"].clone()).unwrap();
        let total_bytes: u64 = pins
            .iter()
            .map(|p| fs::metadata(&p.path).unwrap().len())
            .sum();
        let update = execute_v1(ExpandedTrainingCommandV1::UpdatePrepared {
            source: source.clone(),
            trajectories: pins,
            learning_rate: 1e-4,
            value_coefficient: 0.5,
            preparation_workers: 2,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            max_trajectory_batch_mebibytes: DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
            output_directory: root.join(format!("update-{tag}")),
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution: UpdateBackwardExecutionV1::Sequential,
            line_b: None,
            training_math: TrainingMathV1::PlatformLibm,
            loss_selection: gae,
            record_format: CheckpointRecordFormatV1::default(),
            record_envelope: RecordEnvelopeV1::None,
            card_semantics: None,
        })
        .unwrap();
        (
            update["after_state_sha256"].as_str().unwrap().to_owned(),
            total_bytes,
        )
    };

    let (json_sha, json_bytes) = run_format("json", TrajectoryRecordFormatV1::Json);
    let (bin_sha, bin_bytes) = run_format("bin", TrajectoryRecordFormatV1::BinaryV1);
    let (zstd_sha, zstd_bytes) = run_format("zstd", TrajectoryRecordFormatV1::BinaryZstd1);
    let (lo_sha, lo_bytes) = run_format("lo", TrajectoryRecordFormatV1::BinaryZstd1LearnerOnly);

    eprintln!(
        "trajectory_container_v1 2-game sizes: json={json_bytes} B, binary_v1={bin_bytes} B ({:.2}x), \
         binary_zstd1={zstd_bytes} B ({:.2}x), binary_zstd1_learner_only={lo_bytes} B ({:.2}x)",
        json_bytes as f64 / bin_bytes as f64,
        json_bytes as f64 / zstd_bytes as f64,
        json_bytes as f64 / lo_bytes as f64,
    );
    assert_eq!(bin_sha, json_sha, "BinaryV1 state hash must equal JSON");
    assert_eq!(zstd_sha, json_sha, "BinaryZstd1 state hash must equal JSON");
    assert_eq!(
        lo_sha, json_sha,
        "BinaryZstd1LearnerOnly state hash must equal JSON"
    );
    assert!(
        bin_bytes < json_bytes / 2,
        "expected >=2x reduction for BinaryV1: {bin_bytes} vs {json_bytes}"
    );
    assert!(
        zstd_bytes < json_bytes / 8,
        "expected >=8x reduction for BinaryZstd1: {zstd_bytes} vs {json_bytes}"
    );
    assert!(
        lo_bytes < zstd_bytes,
        "expected learner-only smaller than full BinaryZstd1: {lo_bytes} vs {zstd_bytes}"
    );
}
