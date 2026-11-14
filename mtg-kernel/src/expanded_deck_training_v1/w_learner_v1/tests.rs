//! The wide learner in RT1: wire bytes without it, the widening tool on a
//! real net-8 checkpoint, collection against itself and a net-8 opponent,
//! the GAE update, checkpoint readback, determinism, and the refusals.

use super::*;
// Backport (source c5719e1cf): main's `s_update_fixture_v1` module came after
// this source; the three helpers these tests use are restated here as main
// defines them (test-only).
mod s_update_fixture_v1 {
    use crate::expanded_deck_training_v1::*;
    use crate::sideboard_play_policy_v1::{
        FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V4,
    };
    use std::path::Path;

    pub(super) fn loss_selection_v1() -> ExpandedLossSelectionV1 {
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        }
    }

    pub(super) fn net8_source_v1(root: &Path) -> Result<ExpandedModelSourceV1, String> {
        let identity = FRESH_FEATURE_IDENTITY_V4;
        let descriptor =
            super::super::super::fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
                &root.join("net8"),
                identity,
                |_| {},
            );
        let bytes = serde_json::to_vec(&descriptor).map_err(|e| e.to_string())?;
        let path = root.join("net8-descriptor.json");
        fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: identity.feature_contract_digest.into(),
                expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
            },
            checkpoint: None,
            s_learner: None,
            w_learner: None,
        })
    }

    fn deck_list_v1(id: &str) -> Result<ExpandedDeckListV1, String> {
        let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id)
            .map_err(|error| format!("checked-in Pauper deck {id}: {error:?}"))?;
        let cards = registration.registered_configuration();
        Ok(ExpandedDeckListV1 {
            label: id.into(),
            mainboard: cards.mainboard().to_vec(),
            sideboard: cards.sideboard().to_vec(),
        })
    }

    pub(super) fn episode_v1(
        id: &str,
        seed: u64,
        opponent: Option<ExpandedModelSourceV1>,
    ) -> Result<ExpandedEpisodeV1, String> {
        let decks = [deck_list_v1("Affinity")?, deck_list_v1("Terror")?];
        Ok(ExpandedEpisodeV1 {
            id: id.into(),
            seed,
            starting_player: 0,
            learner_seat: 0,
            opponent,
            opponent_kind: None,
            opponent_search: None,
            registered: decks.clone(),
            selected: decks,
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
        })
    }
}
use crate::expanded_deck_training_v1::{
    execute_v1, read_pinned, ExpandedEpisodeV1, ExpandedTrainingCommandV1,
};
use std::path::{Path, PathBuf};

fn on_large_stack(body: fn()) {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(body)
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "w-learner-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

fn episode(id: &str, seed: u64, opponent: Option<ExpandedModelSourceV1>) -> ExpandedEpisodeV1 {
    s_update_fixture_v1::episode_v1(id, seed, opponent).unwrap()
}

fn gae() -> ExpandedLossSelectionV1 {
    s_update_fixture_v1::loss_selection_v1()
}

fn collect(
    source: &ExpandedModelSourceV1,
    episodes: Vec<ExpandedEpisodeV1>,
    output_directory: PathBuf,
) -> Result<Value, String> {
    execute_v1(ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        output_directory,
    })
}

fn update(
    source: &ExpandedModelSourceV1,
    trajectories: Vec<PinnedFileV1>,
    output_directory: PathBuf,
) -> Result<Value, String> {
    execute_v1(ExpandedTrainingCommandV1::Update {
        source: source.clone(),
        trajectories,
        learning_rate: 1.0e-3,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::V1,
        output_directory,
    })
}

fn trajectories_of(collection: &Value) -> Vec<PinnedFileV1> {
    serde_json::from_value(collection["trajectories"].clone()).unwrap()
}

fn checkpoint_of(update: &Value) -> PinnedFileV1 {
    serde_json::from_value(update["checkpoint"].clone()).unwrap()
}

/// A net-8 checkpoint source: the fixture's fresh net-8 shell after one
/// collect and update, so its checkpoint has its own update trajectories.
fn net8_checkpoint_source(root: &Path) -> (ExpandedModelSourceV1, ExpandedModelSourceV1) {
    let net8 = s_update_fixture_v1::net8_source_v1(root).unwrap();
    let collection = collect(
        &net8,
        vec![
            episode("n8-self", 2_026_111_301, None),
            episode("n8-self-b", 2_026_111_302, None),
        ],
        root.join("n8-collect"),
    )
    .unwrap();
    let updated = update(&net8, trajectories_of(&collection), root.join("n8-update")).unwrap();
    let parent = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint_of(&updated)),
        ..net8.clone()
    };
    (net8, parent)
}

#[test]
fn a_source_without_w_learner_keeps_its_wire_bytes() {
    let legacy = concat!(
        r#"{"play_import":{"path":"/models/import.json","sha256":"#,
        r#""0000000000000000000000000000000000000000000000000000000000000001"},"#,
        r#""feature_transfer":{"expected_feature_contract_digest":"c","#,
        r#""expected_feature_encoding_digest":"e"},"checkpoint":null}"#
    );
    let source: ExpandedModelSourceV1 = serde_json::from_str(legacy).unwrap();
    assert!(source.w_learner.is_none());
    assert_eq!(serde_json::to_string(&source).unwrap(), legacy);
    let with = ExpandedModelSourceV1 {
        w_learner: Some(Box::new(WLearnerSourceV1 {
            schema: W_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: source.play_import.clone(),
        })),
        ..source.clone()
    };
    let text = serde_json::to_string(&with).unwrap();
    assert!(text.ends_with(
        r#""checkpoint":null,"w_learner":{"schema":"w128-learner/v1","initial":{"path":"/models/import.json","sha256":"0000000000000000000000000000000000000000000000000000000000000001"}}}"#
    ));
    assert_eq!(
        serde_json::from_str::<ExpandedModelSourceV1>(&text).unwrap(),
        with
    );
}

#[test]
fn widening_seed_is_the_prescreen_rule() {
    use sha2::{Digest, Sha256};
    let label = "w128-20261113/widen|c1|1048576";
    let digest = Sha256::digest(label.as_bytes());
    let expected = u64::from_le_bytes(digest[..8].try_into().unwrap());
    assert_eq!(widening_seed_v1(label), expected);
    assert_ne!(widening_seed_v1(label), widening_seed_v1("other"));
}

/// End to end on real games: a net-8 checkpoint widens (both optimizer
/// options) with an equality receipt; the wide learner collects against
/// itself and a net-8 opponent, updates, publishes a record that resumes
/// and binds its inputs, and a second update from the same inputs is
/// bit-identical.
#[test]
fn wide_learner_widens_collects_updates_and_resumes() {
    on_large_stack(wide_learner_end_to_end_v1);
}

fn wide_learner_end_to_end_v1() {
    let root = temp_root("e2e");
    let (net8, parent) = net8_checkpoint_source(&root);
    let (_, parent_identity) = super::super::load_expanded_inference_any_math_v1(&parent).unwrap();

    // Widening, option K: the parent's Adam step and its function.
    let receipt = widen_source_v1(
        &parent,
        "test|parent|1",
        WideningOptimizerV1::Keep,
        &root.join("widen-keep"),
    )
    .unwrap();
    assert_eq!(receipt["equality"]["equal"], true);
    assert!(receipt["equality"]["learner_rows"].as_u64().unwrap() > 0);
    assert_eq!(receipt["initial_adam_step"], parent_identity.adam_step);
    assert_eq!(
        receipt["parent_state_sha256"],
        parent_identity.state_sha256.as_str()
    );
    let wide: ExpandedModelSourceV1 = serde_json::from_value(receipt["source"].clone()).unwrap();
    assert!(wide.checkpoint.is_none() && wide.w_learner.is_some());
    let (_, identity) = load_inference_v1(&wide).unwrap();
    assert_eq!(identity.adam_step, parent_identity.adam_step);
    assert_eq!(identity.state_sha256, receipt["initial_state_sha256"]);
    // The same label and parent give the same bytes.
    let again = widen_source_v1(
        &parent,
        "test|parent|1",
        WideningOptimizerV1::Keep,
        &root.join("widen-keep-again"),
    )
    .unwrap();
    assert_eq!(again["initial"]["sha256"], receipt["initial"]["sha256"]);

    // Option F: zero moments and step.
    let fresh = widen_source_v1(
        &parent,
        "test|parent|1",
        WideningOptimizerV1::Fresh,
        &root.join("widen-fresh"),
    )
    .unwrap();
    assert_eq!(fresh["initial_adam_step"], 0);
    let fresh_source: ExpandedModelSourceV1 =
        serde_json::from_value(fresh["source"].clone()).unwrap();
    let loaded = load_w_v1(&fresh_source, None).unwrap();
    let snapshot = loaded.state.snapshot_v1().unwrap();
    assert!(snapshot
        .first_moments
        .iter()
        .chain(&snapshot.second_moments)
        .all(|t| t.values.iter().all(|m| m.to_bits() == 0)));
    drop(loaded);

    // Collection against itself and a net-8 opponent.
    let collection = collect(
        &wide,
        vec![
            episode("w-self", 2_026_111_311, None),
            episode("w-vs-net8", 2_026_111_312, Some(net8.clone())),
        ],
        root.join("w-collect"),
    )
    .unwrap();
    assert_eq!(collection["behavior_state_sha256"], identity.state_sha256);
    let trajectories = trajectories_of(&collection);
    let crossed: super::super::ExpandedTrajectoryV1 = read_pinned(&trajectories[1]).unwrap();
    let behaviors = crossed.seat_behaviors.as_ref().unwrap();
    assert_eq!(behaviors[0].identity, identity);
    assert_eq!(behaviors[1].source, net8);

    // The update and its record.
    let updated = update(&wide, trajectories.clone(), root.join("w-update")).unwrap();
    assert_eq!(updated["adam_step"], parent_identity.adam_step + 1);
    assert_eq!(updated["before_state_sha256"], identity.state_sha256);
    assert_ne!(updated["after_state_sha256"], identity.state_sha256);
    assert_eq!(updated["numerical_backend"], "native-cpu-sequential");
    assert_eq!(
        updated["w_learner"]["architecture"],
        W_MODEL_ARCHITECTURE_VERSION_V1
    );
    let checkpoint = checkpoint_of(&updated);
    let next = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint.clone()),
        ..wide.clone()
    };
    let (_, after) = load_inference_v1(&next).unwrap();
    assert_eq!(updated["after_state_sha256"], after.state_sha256);
    let record: WLearnerCheckpointV1 = read_pinned(&checkpoint).unwrap();
    assert_eq!(record.trajectories, trajectories);
    assert_eq!(record.architecture, W_MODEL_ARCHITECTURE_VERSION_V1);
    // The decode cache: a held container gives exactly a fresh decode, and a
    // changed file is refused by its pin before any lookup.
    let held = decode_pinned_w_state_v1(&record.state, false).unwrap();
    let decoded = decode_pinned_w_state_v1(&record.state, true).unwrap();
    assert_eq!(held.1, decoded.1);
    assert_eq!(held.1, record.state_sha256);
    assert_eq!(
        held.0.snapshot_v1().unwrap(),
        decoded.0.snapshot_v1().unwrap()
    );
    let written = fs::read(&record.state.path).unwrap();
    let mut changed = written.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    fs::write(&record.state.path, &changed).unwrap();
    assert!(decode_pinned_w_state_v1(&record.state, false).is_err());
    assert!(load_inference_v1(&next).is_err());
    fs::write(&record.state.path, &written).unwrap();
    // A continuation refuses an initial file that is not a wide container,
    // even when the source pin and the record both name its bytes.
    let bogus = root.join("bogus-initial.bin");
    fs::write(&bogus, b"MTGW128\0 not a container").unwrap();
    let bogus_pin = PinnedFileV1 {
        path: bogus.clone(),
        sha256: sha(&fs::read(&bogus).unwrap()),
    };
    let mut bogus_record = record.clone();
    bogus_record.initial_sha256 = bogus_pin.sha256.clone();
    let bogus_record_bytes = serde_json::to_vec(&bogus_record).unwrap();
    let bogus_record_path = root.join("bogus-checkpoint.json");
    fs::write(&bogus_record_path, &bogus_record_bytes).unwrap();
    let mut bogus_source = next.clone();
    bogus_source.checkpoint = Some(PinnedFileV1 {
        path: bogus_record_path,
        sha256: sha(&bogus_record_bytes),
    });
    bogus_source.w_learner.as_mut().unwrap().initial = bogus_pin;
    let refused = load_inference_v1(&bogus_source).err().unwrap();
    assert!(refused.contains("container"), "{refused}");
    let expectation = CheckpointInputExpectationV1 {
        trajectories: &trajectories,
        learning_rate_bits: 1.0e-3f32.to_bits(),
        value_coefficient_bits: 0.5f32.to_bits(),
    };
    assert_eq!(
        load_update_inference_v1(&next, &expectation).unwrap(),
        after
    );
    validate_update_loss_v1(&next, &gae()).unwrap();
    assert!(validate_update_loss_v1(
        &next,
        &ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 0.99,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        }
    )
    .is_err());

    // Determinism: the same update again.
    let again = update(&wide, trajectories.clone(), root.join("w-update-again")).unwrap();
    assert_eq!(again["after_state_sha256"], updated["after_state_sha256"]);

    // The continuation collects and updates again.
    let second = collect(
        &next,
        vec![episode("w-next", 2_026_111_313, Some(net8.clone()))],
        root.join("w-collect-2"),
    )
    .unwrap();
    let second_update = update(&next, trajectories_of(&second), root.join("w-update-2")).unwrap();
    assert_eq!(second_update["adam_step"], parent_identity.adam_step + 2);
    assert_eq!(second_update["before_state_sha256"], after.state_sha256);

    // Net-8's loaders refuse a wide source.
    let refused = super::super::initialize(&wide).err().unwrap();
    assert!(refused.contains("net-8w128"), "{refused}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn wide_state_container_round_trips_and_refuses_other_bytes() {
    let state = NativePolicyValueTrainStateWideV1::new_wide_v1(
        NativePolicyValueNetWideV1::runner_fixed_wide_v1(
            NativePolicyValueModelConfigV1::contract_wide_v1(),
        )
        .unwrap(),
    )
    .unwrap();
    let bytes = encode_w_state_v1(&state).unwrap();
    let decoded = decode_w_state_v1(&bytes).unwrap();
    assert_eq!(
        w_state_sha256_v1(&decoded).unwrap(),
        w_state_sha256_v1(&state).unwrap()
    );
    let mut bad = bytes.clone();
    bad[0] ^= 1;
    assert!(decode_w_state_v1(&bad).is_err());
    let mut version = bytes.clone();
    version[8] = 2;
    assert!(decode_w_state_v1(&version).is_err());
    assert!(decode_w_state_v1(&bytes[..bytes.len() - 4]).is_err());
}

#[test]
fn a_source_with_both_s_and_wide_learners_is_refused_by_every_loader() {
    let pin = PinnedFileV1 {
        path: PathBuf::from("/models/none.bin"),
        sha256: "0".repeat(64),
    };
    let mixed = ExpandedModelSourceV1 {
        play_import: pin.clone(),
        feature_transfer: crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: Some(Box::new(super::super::s_learner_v1::SLearnerSourceV1 {
            schema: super::super::s_learner_v1::S_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: pin.clone(),
        })),
        w_learner: Some(Box::new(WLearnerSourceV1 {
            schema: W_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: pin,
        })),
    };
    for refused in [
        super::super::load_expanded_inference_any_math_v1(&mixed)
            .err()
            .unwrap(),
        load_w_v1(&mixed, None).err().unwrap(),
        collection_learner_v1(
            &mixed,
            TrainingMathV1::PlatformLibm,
            CollectionSamplerV1::Legacy,
        )
        .err()
        .unwrap(),
    ] {
        assert!(refused.contains("at most one"), "{refused}");
    }
}

/// A state whose scorer bias (and so its gauge anchor) is not the runner's
/// fixed value, as every trained or widened production state: the container
/// round trip keeps it (Sol round 2).
#[test]
fn wide_state_container_keeps_a_non_runner_gauge_anchor() {
    let mut model = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .unwrap();
    let mut parameters = model.parameter_snapshot_wide_v1();
    let bias = parameters
        .iter_mut()
        .find(|tensor| tensor.name == "scorer.2.bias")
        .unwrap();
    bias.values[0] = 0.003_125;
    model
        .replace_parameter_snapshot_wide_v1(&parameters)
        .unwrap();
    let state = NativePolicyValueTrainStateWideV1::new_wide_v1(model).unwrap();
    assert_eq!(
        state.scorer_bias_anchor_f32_bits_v1(),
        0.003_125f32.to_bits()
    );
    let bytes = encode_w_state_v1(&state).unwrap();
    let decoded = decode_w_state_v1(&bytes).unwrap();
    assert_eq!(
        decoded.scorer_bias_anchor_f32_bits_v1(),
        0.003_125f32.to_bits()
    );
    assert_eq!(
        w_state_sha256_v1(&decoded).unwrap(),
        w_state_sha256_v1(&state).unwrap()
    );
}

#[test]
fn movement_counts_changes_per_widening_class() {
    use sha2::{Digest, Sha256};
    let root = temp_root("movement");
    let model = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .unwrap();
    let before = NativePolicyValueTrainStateWideV1::new_wide_v1(model.clone()).unwrap();
    let mut parameters = model.parameter_snapshot_wide_v1();
    // value_head.0.weight is [128, 128]: (0, 0) is old, (0, 127) a new
    // unit's fan-out, (127, 0) a new unit's fan-in.
    let weight = parameters
        .iter_mut()
        .find(|tensor| tensor.name == "value_head.0.weight")
        .unwrap();
    weight.values[0] += 0.5;
    weight.values[127] += 0.25;
    weight.values[127 * 128] -= 0.125;
    let mut moved = model;
    moved
        .replace_parameter_snapshot_wide_v1(&parameters)
        .unwrap();
    let after = NativePolicyValueTrainStateWideV1::new_wide_v1(moved).unwrap();
    let pin = |name: &str, state: &NativePolicyValueTrainStateWideV1| {
        let bytes = encode_w_state_v1(state).unwrap();
        let path = root.join(name);
        fs::write(&path, &bytes).unwrap();
        PinnedFileV1 {
            path,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        }
    };
    let report = w_movement_v1(&pin("a.bin", &before), &pin("b.bin", &after)).unwrap();
    let classes = &report["classes"];
    assert_eq!(classes["old"]["changed"], 1);
    assert!((classes["old"]["max_abs_change"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert_eq!(classes["new_fan_out"]["changed"], 1);
    assert!((classes["new_fan_out"]["max_abs_change"].as_f64().unwrap() - 0.25).abs() < 1e-6);
    assert_eq!(classes["new_fan_in"]["changed"], 1);
    assert!((classes["new_fan_in"]["max_abs_change"].as_f64().unwrap() - 0.125).abs() < 1e-6);
    assert_eq!(classes["new_embedding_column"]["changed"], 0);
    let total: u64 = classes
        .as_object()
        .unwrap()
        .values()
        .map(|c| c["coordinates"].as_u64().unwrap())
        .sum();
    assert_eq!(total, 2_750_754);
    let _ = fs::remove_dir_all(&root);
}

/// The equality check on games the parent itself played (for a parent whose
/// own update trajectories are gone), and its refusal of games another state
/// played.
#[test]
fn widening_checks_explicit_parent_played_rows() {
    on_large_stack(widening_explicit_rows_v1);
}

fn widening_explicit_rows_v1() {
    let root = temp_root("rows");
    let (_, parent) = net8_checkpoint_source(&root);
    let played = trajectories_of(
        &collect(
            &parent,
            vec![episode("p-self", 2_026_111_321, None)],
            root.join("p-collect"),
        )
        .unwrap(),
    );
    let receipt = widen_source_with_rows_v1(
        &parent,
        "test|rows|1",
        WideningOptimizerV1::Keep,
        Some(&played),
        &root.join("widen-rows"),
    )
    .unwrap();
    assert_eq!(receipt["equality"]["rows_source"], "parent-played");
    assert!(receipt["equality"]["learner_rows"].as_u64().unwrap() > 0);
    // The parent's own update inputs were played by the state before it.
    let earlier: Vec<PinnedFileV1> = {
        let record: Value = read_pinned(parent.checkpoint.as_ref().unwrap()).unwrap();
        serde_json::from_value(record["trajectories"].clone()).unwrap()
    };
    let refused = widen_source_with_rows_v1(
        &parent,
        "test|rows|1",
        WideningOptimizerV1::Keep,
        Some(&earlier),
        &root.join("widen-earlier"),
    )
    .err()
    .unwrap();
    assert!(refused.contains("not played by the parent"), "{refused}");
    // The parent's hash with another recorded output (a learner row's value
    // altered) is refused too.
    let mut doc: Value = read_pinned(&played[0]).unwrap();
    let learner = doc["episode"]["learner_seat"].as_u64().unwrap();
    let row = doc["decisions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["actor"].as_u64() == Some(learner))
        .unwrap();
    let value = row["value"].as_u64().unwrap() as u32;
    row["value"] = json!(f32::from_bits(value).mul_add(0.5, 0.25).to_bits());
    let altered = root.join("altered.json");
    let bytes = serde_json::to_vec(&doc).unwrap();
    fs::write(&altered, &bytes).unwrap();
    let pin = PinnedFileV1 {
        path: altered,
        sha256: {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(&bytes))
        },
    };
    let refused = widen_source_with_rows_v1(
        &parent,
        "test|rows|1",
        WideningOptimizerV1::Keep,
        Some(&[pin]),
        &root.join("widen-altered"),
    )
    .err()
    .unwrap();
    assert!(refused.contains("not the parent's"), "{refused}");
    let _ = fs::remove_dir_all(&root);
}
