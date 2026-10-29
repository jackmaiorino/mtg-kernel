use super::*;
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::expanded_deck_training_v1::archive_distillation_v1::{
    ArchiveDistillationArmV1, ArchiveDistillationProvenanceV1,
};
use crate::expanded_deck_training_v1::{
    bits, hex, inference_identity_v1, initialize, load_expanded_update_inference_v1,
    ordinary_checkpoint_schema_v1, publish_json, sha,
    write_synthetic_fresh_source_with_parameters_v1, CollectionSamplerV1, ParameterBitsV1,
    GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
};
use crate::sideboard_play_policy_v1::{FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V3};
use crate::training_math_v1::TrainingMathV1;
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

const LEARNING_RATE: f32 = 0.0001;
const VALUE_COEFFICIENT: f32 = 0.5;

type LoadedV1 = (FrozenPlayPolicyV1, NativePolicyValueTrainStateV1);

/// Lookups on this thread that found an entry.
fn matches() -> u64 {
    MATCHES_FOR_TEST_V1.with(Cell::get)
}

/// Runs `load` on this thread with the cache switched off: a full load that
/// neither reads nor fills the cache.
fn bypassed<T>(load: impl FnOnce() -> T) -> T {
    BYPASS_FOR_TEST_V1.with(|bypass| bypass.set(true));
    let result = load();
    BYPASS_FOR_TEST_V1.with(|bypass| bypass.set(false));
    result
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "loaded-source-cache-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

/// A fresh-initialization source without a checkpoint, written under `root`.
fn fresh_source(root: &Path) -> ExpandedModelSourceV1 {
    let identity = FRESH_FEATURE_IDENTITY_V3;
    let descriptor =
        write_synthetic_fresh_source_with_parameters_v1(&root.join("source"), identity, |_| {});
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = root.join("descriptor.json");
    fs::write(&path, &bytes).unwrap();
    ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
    }
}

/// A GAE checkpoint of `source` with moved parameters, Adam moments and step,
/// as an update would publish it, and the source that resumes it.
fn write_checkpoint(
    root: &Path,
    source: &ExpandedModelSourceV1,
) -> (ExpandedModelSourceV1, ExpandedCheckpointV1) {
    let (policy, state) = bypassed(|| initialize(source)).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    snapshot
        .parameters
        .iter_mut()
        .find(|p| p.name == "card_embedding.weight")
        .unwrap()
        .values[16] = 0.4375;
    snapshot.first_moments[0].values[16] = 0.125;
    snapshot.second_moments[0].values[16] = 0.0625;
    snapshot.adam_step = 12;
    let as_bits = |rows: &[crate::native_policy_value_net_v1::NativeNamedParameterV1]| {
        rows.iter().map(ParameterBitsV1::from_native).collect()
    };
    let identity = policy.feature_identity_v1();
    let saved = ExpandedCheckpointV1 {
        schema: ordinary_checkpoint_schema_v1(policy.identity_v1()).into(),
        feature_contract_digest: identity.feature_contract_digest.into(),
        feature_encoding_digest: identity.feature_encoding_digest.into(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        source_import: policy.identity_v1().clone(),
        state_sha256: hex(&snapshot.state_sha256_v1().unwrap()),
        e_supported: None,
        soft_q: None,
        search_targets: None,
        ex1: None,
        adam_step: snapshot.adam_step,
        scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
        parameters: as_bits(&snapshot.parameters),
        first_moments: as_bits(&snapshot.first_moments),
        second_moments: as_bits(&snapshot.second_moments),
        trajectories: (0..2)
            .map(|index| PinnedFileV1 {
                path: root.join(format!("episode-{index:04}.json")),
                sha256: format!("{index}").repeat(64),
            })
            .collect(),
        loss_identity: GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into(),
        gamma_bits: Some(1.0_f32.to_bits()),
        gae_lambda_bits: Some(0.9_f32.to_bits()),
        entropy_coefficient_bits: Some(0.0_f32.to_bits()),
        exploration: None,
        learning_rate_bits: LEARNING_RATE.to_bits(),
        value_coefficient_bits: VALUE_COEFFICIENT.to_bits(),
        registry_transfer: None,
        archive_distillation: None,
        training_math: TrainingMathV1::PlatformLibm,
        input_contract: None,
    };
    let directory = root.join("update-0000");
    fs::create_dir_all(&directory).unwrap();
    let pin = publish_json(&directory, "checkpoint.json", &saved).unwrap();
    let resumed = ExpandedModelSourceV1 {
        checkpoint: Some(pin),
        ..source.clone()
    };
    (resumed, saved)
}

fn assert_same_load(actual: &LoadedV1, expected: &LoadedV1, source: &ExpandedModelSourceV1) {
    let ((policy, state), (expected_policy, expected_state)) = (actual, expected);
    assert_eq!(
        state.state_sha256_v1().unwrap(),
        expected_state.state_sha256_v1().unwrap()
    );
    assert_eq!(state.adam_step_v1(), expected_state.adam_step_v1());
    assert_eq!(policy.identity_v1(), expected_policy.identity_v1());
    assert_eq!(
        policy.actual_model_identity_v1(),
        expected_policy.actual_model_identity_v1()
    );
    assert_eq!(
        bits(policy.embedding_rows_v1()),
        bits(expected_policy.embedding_rows_v1())
    );
    assert_eq!(
        inference_identity_v1(source, policy, state).unwrap(),
        inference_identity_v1(source, expected_policy, expected_state).unwrap()
    );
    // Every duplicate is handed out in its loaded state.
    policy.duplicate_loaded_v1().unwrap();
}

#[test]
fn a_cache_hit_equals_a_full_load_and_binds_the_update_inputs() {
    let root = temp_root("hit");
    let source = fresh_source(&root);
    let (resumed, saved) = write_checkpoint(&root, &source);
    for source in [&source, &resumed] {
        let full = bypassed(|| initialize(source)).unwrap();
        let before = matches();
        let first = initialize(source).unwrap();
        assert_eq!(matches(), before, "the first load reads everything");
        let second = initialize(source).unwrap();
        assert_eq!(matches(), before + 1, "the second load is a cache hit");
        assert_same_load(&first, &full, source);
        assert_same_load(&second, &full, source);
    }
    assert_ne!(
        initialize(&source).unwrap().1.state_sha256_v1().unwrap(),
        initialize(&resumed).unwrap().1.state_sha256_v1().unwrap(),
        "a checkpoint and its initial source are separate entries"
    );

    // Update validation's input binding, on a hit as on a full load.
    let trajectories = saved.trajectories.clone();
    let before = matches();
    load_expanded_update_inference_v1(&resumed, &trajectories, LEARNING_RATE, VALUE_COEFFICIENT)
        .unwrap();
    assert_eq!(matches(), before + 1);
    let mut other_trajectory = trajectories.clone();
    other_trajectory[1].sha256 = "d".repeat(64);
    let cases = [
        (other_trajectory, LEARNING_RATE, VALUE_COEFFICIENT),
        (trajectories[..1].to_vec(), LEARNING_RATE, VALUE_COEFFICIENT),
        (trajectories.clone(), 0.0003, VALUE_COEFFICIENT),
        (trajectories.clone(), LEARNING_RATE, 0.25),
    ];
    for (inputs, learning_rate, value_coefficient) in cases {
        let load = || {
            load_expanded_update_inference_v1(&resumed, &inputs, learning_rate, value_coefficient)
                .unwrap_err()
        };
        let full = bypassed(load);
        let before = matches();
        assert_eq!(load(), full);
        assert_eq!(matches(), before + 1);
        assert_eq!(full, "checkpoint inputs or optimizer settings differ");
    }
    let load = || {
        load_expanded_update_inference_v1(&source, &[], LEARNING_RATE, VALUE_COEFFICIENT)
            .unwrap_err()
    };
    let full = bypassed(load);
    let before = matches();
    assert_eq!(load(), full);
    assert_eq!(matches(), before + 1);
    assert_eq!(full, "update requires checkpoint");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_changed_or_missing_file_fails_a_hit_as_it_fails_a_full_load() {
    let root = temp_root("files");
    let source = fresh_source(&root);
    let (resumed, _) = write_checkpoint(&root, &source);
    let descriptor: ExpandedFreshInitializationSourceV1 =
        serde_json::from_slice(&fs::read(&source.play_import.path).unwrap()).unwrap();
    let reference = initialize(&resumed).unwrap();
    let files = [
        resumed.checkpoint.as_ref().unwrap().path.clone(),
        descriptor.parameters.path.clone(),
        descriptor.initialization.path.clone(),
    ];
    for path in files {
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed[original.len() / 2] ^= 1;
        for (label, bytes) in [("changed", Some(changed)), ("missing", None)] {
            match bytes {
                Some(bytes) => fs::write(&path, bytes).unwrap(),
                None => fs::remove_file(&path).unwrap(),
            }
            let full = bypassed(|| initialize(&resumed)).err().unwrap();
            let before = matches();
            let cached = initialize(&resumed).err().unwrap();
            assert_eq!(matches(), before + 1, "{label} {}", path.display());
            assert_eq!(cached, full, "{label} {}", path.display());
            fs::write(&path, &original).unwrap();
        }
        // The restored bytes load from the cache again.
        let before = matches();
        let restored = initialize(&resumed).unwrap();
        assert_eq!(matches(), before + 1);
        assert_same_load(&restored, &reference, &resumed);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn handed_out_duplicates_do_not_share_state_with_the_cache() {
    let root = temp_root("duplicates");
    let source = fresh_source(&root);
    let (resumed, _) = write_checkpoint(&root, &source);
    let reference = initialize(&resumed).unwrap();
    let (mut policy, _) = initialize(&resumed).unwrap();
    CollectionSamplerV1::UnclampedSoftmaxF64IcdfU53V1.apply_to_learner_v1(&mut policy);
    assert!(
        policy.duplicate_loaded_v1().is_err(),
        "a policy with a collection sampler is no longer in its loaded state"
    );
    let mut parameters = policy.training_parameters_v3();
    parameters[0].values[16] = -0.75;
    policy.replace_training_parameters_v3(&parameters).unwrap();
    assert_ne!(
        policy.actual_model_identity_v1(),
        reference.0.actual_model_identity_v1()
    );
    let before = matches();
    let again = initialize(&resumed).unwrap();
    assert_eq!(matches(), before + 1);
    assert_same_load(&again, &reference, &resumed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkpoints_that_name_another_file_are_not_cached() {
    let root = temp_root("phase1");
    let source = fresh_source(&root);
    let (resumed, saved) = write_checkpoint(&root, &source);
    let descriptor_bytes = fs::read(&source.play_import.path).unwrap();
    let (policy, state) = bypassed(|| initialize(&resumed)).unwrap();
    let pin = |name: &str, fill: char| PinnedFileV1 {
        path: root.join(name),
        sha256: fill.to_string().repeat(64),
    };
    let mut phase1 = saved.clone();
    phase1.archive_distillation = Some(ArchiveDistillationProvenanceV1 {
        method: "archive-distillation".into(),
        lineage: "r1".into(),
        arm: ArchiveDistillationArmV1::Uniform,
        parent: resumed.checkpoint.clone().unwrap(),
        parent_state_sha256: saved.state_sha256.clone(),
        games: pin("games.jsonl", 'd'),
        archive_sources: vec![pin("ARCHIVE.jsonl", '5')],
        bank: pin("bank.json", 'e'),
        bank_rows_sha256: "f".repeat(64),
        anchor_sha256: "1".repeat(64),
        steps: 179,
        learning_rate_bits: LEARNING_RATE.to_bits(),
        phase1_state_sha256: "2".repeat(64),
        runtime_commit: "test".into(),
        cell: None,
        update_scope: Default::default(),
        rules_pathway: None,
    });
    remember_v1(&resumed, &descriptor_bytes, &policy, &state, Some(&phase1));
    let before = matches();
    assert!(lookup_v1(&resumed, &descriptor_bytes, None)
        .unwrap()
        .is_none());
    assert_eq!(matches(), before);
    // A checkpoint pin without the checkpoint it restored is refused too.
    remember_v1(&resumed, &descriptor_bytes, &policy, &state, None);
    assert!(lookup_v1(&resumed, &descriptor_bytes, None)
        .unwrap()
        .is_none());
    // The same load with an ordinary checkpoint is kept.
    remember_v1(&resumed, &descriptor_bytes, &policy, &state, Some(&saved));
    assert!(lookup_v1(&resumed, &descriptor_bytes, None)
        .unwrap()
        .is_some());
    assert_eq!(matches(), before + 1);
    fs::remove_dir_all(root).unwrap();
}
