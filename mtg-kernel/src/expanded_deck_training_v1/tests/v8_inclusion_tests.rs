//! The V8 inclusion contract on collected decisions (collab
//! LANES/w4-combat-20261030/DESIGN.md v5.1, section 5): the converted b512
//! gives b512's logits and value as the same f32 bits on decisions it faces in
//! real games, include and pairing rows among them (the converter's own
//! preservation check, `v8_inclusion_conversion_v1`). The b512 checkpoint and
//! its play import are not in the repository, so that test is `#[ignore]`d
//! and reads `MTG_KERNEL_V8_B512_CHECKPOINT` (an OH2 `doses/b512.json`) and
//! `MTG_KERNEL_V8_PLAY_IMPORT` (its `t1-import.json`). The other tests use a
//! synthetic fresh source.

use super::*;
use crate::expanded_deck_training_v1::v8_inclusion_conversion_v1::{
    convert_checkpoint_to_v8_inclusion_v1, write_v8_checkpoint_v1, V8ConversionRequestV1,
    V8PreservationGameV1, V8_CONVERSION_REQUEST_SCHEMA_V1,
};
use crate::native_flat_tensorizer_v8::{FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8};

fn pin_file_v1(path: &Path) -> PinnedFileV1 {
    PinnedFileV1 {
        path: path.canonicalize().unwrap(),
        sha256: sha(&fs::read(path).unwrap()),
    }
}

/// Creature-heavy pairings, both starting players, one London game.
fn preservation_games_v1() -> Vec<V8PreservationGameV1> {
    [
        ("Elves", "Affinity", false),
        ("Affinity", "Elves", false),
        ("CawGates", "Rally", false),
        ("Terror", "Faeries", false),
        ("Rally", "Elves", false),
        ("Burn", "CawGates", true),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (first, second, london))| V8PreservationGameV1 {
        seed: 2_026_103_100 + index as u64,
        starting_player: (index % 2) as u8,
        decks: [first.into(), second.into()],
        london,
    })
    .collect()
}

/// A synthetic fresh V4 source, one GAE update of one real game, so the
/// parent checkpoint has live moments; then the V8 conversion, its
/// preservation check on real games, and a FlatV8 source that collects,
/// updates and resumes from the update's checkpoint.
#[test]
fn v8_conversion_of_a_trained_checkpoint_preserves_outputs_and_trains() {
    let root = std::env::temp_dir().join(format!("v8-inclusion-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
    let descriptor = fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        feature_identity,
        |_parameters| {},
    );
    let descriptor_path = root.join("descriptor.json");
    fs::write(&descriptor_path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    let play_import = pin_file_v1(&descriptor_path);
    let gae = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.95,
        entropy_coefficient: 0.0,
    };
    let list = |id: &str| {
        let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
        ExpandedDeckListV1 {
            label: id.into(),
            mainboard: registered.registered_configuration().mainboard().to_vec(),
            sideboard: registered.registered_configuration().sideboard().to_vec(),
        }
    };
    let episode = |id: &str, seed: u64| ExpandedEpisodeV1 {
        id: id.into(),
        seed,
        starting_player: 0,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: [list("Elves"), list("Affinity")],
        selected: [list("Elves"), list("Affinity")],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: None,
        mulligan_modes: None,
    };
    let iterate = |source: &ExpandedModelSourceV1, label: &str, seed: u64| -> PinnedFileV1 {
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
        let trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collected["trajectories"].clone()).unwrap();
        let updated = execute_v1(ExpandedTrainingCommandV1::Update {
            line_b: None,
            source: source.clone(),
            trajectories,
            learning_rate: 0.0003,
            value_coefficient: 0.5,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution: UpdateBackwardExecutionV1::FixedPartition4,
            loss_selection: gae,
            training_math: TrainingMathV1::PlatformLibm,
            record_format: crate::expanded_deck_training_v1::CheckpointRecordFormatV1::V1,
            output_directory: root.join(format!("update-{label}")),
        })
        .unwrap();
        serde_json::from_value(updated["checkpoint"].clone()).unwrap()
    };
    let v4_source = ExpandedModelSourceV1 {
        play_import: play_import.clone(),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
    };
    let parent = iterate(&v4_source, "v4-parent", 2_026_103_001);
    let receipt = convert_checkpoint_to_v8_inclusion_v1(&V8ConversionRequestV1 {
        schema: V8_CONVERSION_REQUEST_SCHEMA_V1.into(),
        play_import: play_import.clone(),
        parent_checkpoint: parent.clone(),
        output_checkpoint: root.join("v8.json"),
        seed: 7,
        preservation_games: preservation_games_v1(),
    })
    .unwrap();
    let preservation = &receipt["preservation"];
    eprintln!("V8 preservation: {preservation}");
    assert_eq!(preservation["mismatches"], 0);
    assert!(preservation["rows"].as_u64().unwrap() > 0);
    assert!(preservation["include_active_rows"].as_u64().unwrap() > 0);
    assert!(preservation["london_pregame_rows"].as_u64().unwrap() > 0);
    // A second conversion to the same path is refused.
    assert!(write_v8_checkpoint_v1(&parent, &root.join("v8.json"), 7).is_err());
    let v8_checkpoint = PinnedFileV1 {
        path: root.join("v8.json"),
        sha256: receipt["output"]["sha256"].as_str().unwrap().into(),
    };
    let mut v8_source = ExpandedModelSourceV1 {
        play_import: play_import.clone(),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V8.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V8.into(),
        },
        checkpoint: Some(v8_checkpoint.clone()),
    };
    // A FlatV8 source refuses the V4 parent checkpoint and a missing one.
    assert!(initialize(&ExpandedModelSourceV1 {
        checkpoint: Some(parent.clone()),
        ..v8_source.clone()
    })
    .is_err());
    assert!(initialize(&ExpandedModelSourceV1 {
        checkpoint: None,
        ..v8_source.clone()
    })
    .is_err());
    // A V4 source refuses the V8 checkpoint.
    assert!(initialize(&ExpandedModelSourceV1 {
        checkpoint: Some(v8_checkpoint.clone()),
        ..v4_source.clone()
    })
    .is_err());
    for (label, seed) in [("v8-first", 2_026_103_002), ("v8-second", 2_026_103_003)] {
        let checkpoint = iterate(&v8_source, label, seed);
        let saved: ExpandedCheckpointV1 = read_pinned(&checkpoint).unwrap();
        assert_eq!(saved.feature_contract_digest, FEATURE_CONTRACT_DIGEST_V8);
        assert_eq!(saved.parameters.len(), 37);
        v8_source.checkpoint = Some(checkpoint);
    }
    let (policy, state) = initialize(&v8_source).unwrap();
    assert_eq!(
        policy.feature_identity_v1(),
        crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V8
    );
    let parent_saved: ExpandedCheckpointV1 = read_pinned(&parent).unwrap();
    assert_eq!(state.adam_step_v1(), parent_saved.adam_step + 2);
    // A fixed V8 opponent, the converted checkpoint and the trained one,
    // loads twice through the opponent cache: the second load is a hit and
    // serves the full load's receipt (the hit once refused every FlatV8
    // opponent, comparing the V4 import digest with the V8 identity).
    let trained = v8_source.clone();
    for opponent in [
        ExpandedModelSourceV1 {
            checkpoint: Some(v8_checkpoint.clone()),
            ..trained.clone()
        },
        trained,
    ] {
        let full = load_expanded_inference_any_math_v1(&opponent).unwrap();
        assert_eq!(
            full.1.model.feature_contract_digest,
            FEATURE_CONTRACT_DIGEST_V4
        );
        assert_eq!(
            no_state_digest_v1::effective_contract_digest_v1(&full.0),
            FEATURE_CONTRACT_DIGEST_V8
        );
        let (first, _) =
            ordered_update_preparation::load_opponent_cached_v1(&opponent, true).unwrap();
        let (second, hit) =
            ordered_update_preparation::load_opponent_cached_v1(&opponent, true).unwrap();
        assert!(hit);
        assert_eq!(first.behavior.identity, full.1);
        assert_eq!(second.behavior.identity, full.1);
        assert_eq!(
            second.policy.feature_identity_v1(),
            crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V8
        );
    }
    let _ = fs::remove_dir_all(&root);
}

/// The real b512 conversion's preservation check, as the converter runs it
/// for every OH2 chain.
#[test]
#[ignore = "needs MTG_KERNEL_V8_B512_CHECKPOINT and MTG_KERNEL_V8_PLAY_IMPORT; plays real games"]
fn v8_converted_b512_reproduces_b512_on_its_collected_decisions() {
    let checkpoint = PathBuf::from(
        std::env::var_os("MTG_KERNEL_V8_B512_CHECKPOINT")
            .expect("set MTG_KERNEL_V8_B512_CHECKPOINT to a b512 checkpoint"),
    );
    let import = PathBuf::from(
        std::env::var_os("MTG_KERNEL_V8_PLAY_IMPORT")
            .expect("set MTG_KERNEL_V8_PLAY_IMPORT to the checkpoint's play import"),
    );
    let output = std::env::temp_dir().join(format!("v8-b512-{}.json", std::process::id()));
    let _ = fs::remove_file(&output);
    let receipt = convert_checkpoint_to_v8_inclusion_v1(&V8ConversionRequestV1 {
        schema: V8_CONVERSION_REQUEST_SCHEMA_V1.into(),
        play_import: pin_file_v1(&import),
        parent_checkpoint: pin_file_v1(&checkpoint),
        output_checkpoint: output.clone(),
        seed: 2_026_103_101,
        preservation_games: preservation_games_v1(),
    })
    .unwrap();
    let preservation = &receipt["preservation"];
    eprintln!("b512 V8 preservation: {preservation}");
    assert_eq!(preservation["mismatches"], 0);
    assert!(preservation["include_active_rows"].as_u64().unwrap() > 0);
    assert!(preservation["pairing_active_rows"].as_u64().unwrap() > 0);
    let _ = fs::remove_file(&output);
}
