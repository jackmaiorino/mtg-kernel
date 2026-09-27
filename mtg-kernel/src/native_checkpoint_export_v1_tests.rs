use super::*;
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
use crate::model_guided_search_authority_v1::authorized_seed_block_v1;
use crate::model_guided_search_contract_digests_v1::MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1;
use crate::native_checkpoint_shadow_stdio_v1::BoundModelGuidedSearchV1;
use crate::native_flat_tensorizer_v3::{FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3};
use crate::sideboard_play_policy_v1::{
    FrozenPlayObservationTransferV3, FrozenPlayPolicyImportV1, FrozenPlayPolicyV1,
};

const DESTINATION_REGISTRY_V1: &[u8] = include_bytes!("../../data/cards_v1.json");

/// Synthetic metadata/model fixture, never a claim of an actual Store export.
fn fixture_with_run_v1(
    kind: ExportStoreKindV1,
    run_sha256: [u8; 32],
) -> (LoadedShadowCheckpointV1, ExportCheckpointSourceV1) {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let model_sha = model.parameter_manifest_sha256_raw_v1();
    let inference = NativeCheckpointInferenceV1::from_inference_export_v1(
        model, run_sha256, [2; 32], [3; 32], [4; 32], model_sha, 2048,
    )
    .unwrap();
    let (authority_kind, environment) = match kind {
        ExportStoreKindV1::Population => (
            "population-store-validated-generation",
            POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
        ),
        ExportStoreKindV1::Original => (
            "original-promoted2-validated-store-generation",
            SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
        ),
        ExportStoreKindV1::Portable => unreachable!(),
    };
    let identity = ShadowCheckpointIdentityV1 {
        authority_kind: authority_kind.into(),
        source_run_sha256: lower_hex_raw32_v1(run_sha256),
        source_generation: 2048,
        source_checkpoint_sha256: lower_hex_raw32_v1([2; 32]),
        source_sidecar_sha256: lower_hex_raw32_v1([5; 32]),
        source_payload_sha256: lower_hex_raw32_v1([3; 32]),
        source_train_state_sha256: lower_hex_raw32_v1([4; 32]),
        loaded_run_sha256: lower_hex_raw32_v1(run_sha256),
        loaded_generation: 2048,
        loaded_checkpoint_sha256: lower_hex_raw32_v1([2; 32]),
        loaded_payload_sha256: lower_hex_raw32_v1([3; 32]),
        loaded_train_state_sha256: lower_hex_raw32_v1([4; 32]),
        model_parameter_sha256: lower_hex_raw32_v1(model_sha),
        environment_trajectory_contract: environment,
        sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    };
    let source = ExportCheckpointSourceV1 {
        method: ExportCheckpointMethodV1::LegacyValidatedStore,
        checkpoint: ExportCheckpointRefV1 {
            store_kind: kind,
            store_root: std::env::temp_dir().join("inference-export-synthetic-source-not-opened"),
            generation: 2048,
            expected_model_sha256: identity.model_parameter_sha256.clone(),
        },
        expected_design_sha256: None,
        expected_method_sha256: None,
        max_auxiliary_bytes: None,
        topology_qualification: None,
        expected_export_metadata_sha256: None,
        export_source_method: None,
    };
    (
        LoadedShadowCheckpointV1 {
            inference,
            identity,
            max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
            max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
        },
        source,
    )
}

fn fixture_v1(kind: ExportStoreKindV1) -> (LoadedShadowCheckpointV1, ExportCheckpointSourceV1) {
    fixture_with_run_v1(kind, [1; 32])
}

fn expected_v1(source: &ExportCheckpointSourceV1, metadata: &[u8]) -> ExportCheckpointSourceV1 {
    export_reference_v1(
        source,
        &std::env::temp_dir().join("inference-export-not-opened"),
        hash_v1(metadata),
    )
    .unwrap()
}

fn bind_v1(loaded: &LoadedShadowCheckpointV1) -> BoundModelGuidedSearchV1 {
    BoundModelGuidedSearchV1::bind_v1(
        KernelNativeSearchTierV1::T512,
        6,
        authorized_seed_block_v1(6).unwrap(),
        &MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1,
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        &loaded.identity,
        loaded
            .inference
            .search_model_v1()
            .architecture_identity_v1(),
    )
    .unwrap()
}

fn unique_directory_v1(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn inference_export_roundtrip_preserves_model_identity_and_search_authority_v1() {
    for kind in [ExportStoreKindV1::Population, ExportStoreKindV1::Original] {
        let (original, source) = fixture_v1(kind);
        let build = current_build_v1().unwrap();
        let (metadata, parameters) = encode_loaded_v1(&original, &source, build.clone()).unwrap();
        assert_eq!(parameters.len(), 4_923_976);
        let restored = decode_export_v1(
            &metadata,
            &parameters,
            &expected_v1(&source, &metadata),
            build,
        )
        .unwrap();
        assert_eq!(original.identity, restored.checkpoint.identity);
        assert_eq!(
            original.inference.model_parameter_sha256(),
            restored.checkpoint.inference.model_parameter_sha256()
        );
        let mut restored_parameters = Vec::new();
        encode_section_v1(
            &mut restored_parameters,
            &restored
                .checkpoint
                .inference
                .search_model_v1()
                .parameter_snapshot_v1(),
        );
        assert_eq!(parameters, restored_parameters);
        let before = bind_v1(&original);
        let after = bind_v1(&restored.checkpoint);
        assert_eq!(
            before.wrapper_identity.checkpoint_lineage_id,
            after.wrapper_identity.checkpoint_lineage_id
        );
        assert_eq!(
            before.authority_digest_sha256,
            after.authority_digest_sha256
        );
        assert!(!restored.receipt.reader_revalidated_store_chain);
        assert_eq!(restored.receipt.source_validation, EXPORT_VALIDATION_V1);
        assert_eq!(restored.receipt.metadata_sha256, hash_v1(&metadata));
    }
}

#[test]
fn inference_export_rejects_raw_size_digest_nonfinite_and_padding_tampering_v1() {
    let (original, source) = fixture_v1(ExportStoreKindV1::Population);
    let build = current_build_v1().unwrap();
    let (metadata, parameters) = encode_loaded_v1(&original, &source, build.clone()).unwrap();
    let expected = expected_v1(&source, &metadata);
    assert!(decode_export_v1(
        &metadata,
        &parameters[..parameters.len() - 4],
        &expected,
        build.clone()
    )
    .is_err());
    let mut corrupt = parameters.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(decode_export_v1(&metadata, &corrupt, &expected, build.clone()).is_err());
    // Re-pin the raw bytes and canonical metadata to reach semantic checks,
    // rather than letting a checksum mismatch mask non-finite/padding defects.
    for (offset, word) in [
        (parameters.len() - 4, f32::NAN.to_bits()),
        (0, 1.0_f32.to_bits()),
    ] {
        let mut corrupt = parameters.clone();
        corrupt[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        let mut value: Value = serde_json::from_slice(&metadata).unwrap();
        value["parameter_section_sha256"] = json!(hash_v1(&corrupt));
        let changed = canonical_v1(&value).unwrap();
        assert!(decode_export_v1(
            &changed,
            &corrupt,
            &expected_v1(&source, &changed),
            build.clone()
        )
        .is_err());
    }
    // A self-consistent metadata model claim must still match the decoded net.
    let mut value: Value = serde_json::from_slice(&metadata).unwrap();
    value["identity"]["model_parameter_sha256"] = json!("f".repeat(64));
    value["source"]["checkpoint"]["expected_model_sha256"] = json!("f".repeat(64));
    let changed = canonical_v1(&value).unwrap();
    let mut expected = expected_v1(&source, &changed);
    expected.checkpoint.expected_model_sha256 = "f".repeat(64);
    assert!(decode_export_v1(&changed, &parameters, &expected, build).is_err());
}

#[test]
fn inference_export_rejects_metadata_identity_and_expected_pin_tampering_v1() {
    let (original, source) = fixture_v1(ExportStoreKindV1::Population);
    let build = current_build_v1().unwrap();
    let (metadata, parameters) = encode_loaded_v1(&original, &source, build.clone()).unwrap();
    let mut wrong = expected_v1(&source, &metadata);
    wrong.expected_export_metadata_sha256 = Some("f".repeat(64));
    assert!(decode_export_v1(&metadata, &parameters, &wrong, build.clone()).is_err());
    let mut wrong = expected_v1(&source, &metadata);
    wrong.checkpoint.generation = 2052;
    assert!(decode_export_v1(&metadata, &parameters, &wrong, build.clone()).is_err());
    let mutations: [fn(&mut Value); 11] = [
        |v| v["unexpected"] = json!(true),
        |v| v["identity"]["unexpected"] = json!(true),
        |v| v["exporter_build"]["unexpected"] = json!(true),
        |v| v["identity"]["authority_kind"] = json!("export-transport"),
        |v| v["identity"]["loaded_generation"] = json!(2052),
        |v| v["identity"]["source_payload_sha256"] = json!("f".repeat(64)),
        |v| v["identity"]["environment_trajectory_contract"] = json!("legacy-v1"),
        |v| v["max_policy_steps"] = json!(2049),
        |v| v["architecture_identity"] = json!("kernel-policy-value-net-8w128"),
        |v| v["model_config_fingerprint"] = json!("different-config"),
        |v| {
            v["method_producer"] = json!({"method_sha256": "7".repeat(64),
                "design_sha256": "6".repeat(64), "method_contract_sha256": "8".repeat(64),
                "writer_build": {}, "writer_executable_sha256": "9".repeat(64)})
        },
    ];
    for mutate in mutations {
        let mut value: Value = serde_json::from_slice(&metadata).unwrap();
        mutate(&mut value);
        let changed = canonical_v1(&value).unwrap();
        assert!(decode_export_v1(
            &changed,
            &parameters,
            &expected_v1(&source, &changed),
            build.clone()
        )
        .is_err());
    }
}

#[test]
fn inference_export_refuses_unported_weight_only_and_reexport_routes_v1() {
    let (original, source) = fixture_v1(ExportStoreKindV1::Population);
    let build = current_build_v1().unwrap();
    let mut method = source.clone();
    method.method = ExportCheckpointMethodV1::SearchDistillationV1;
    assert!(encode_loaded_v1(&original, &method, build.clone()).is_err());
    assert!(load_store_checkpoint_v1(&method).is_err());
    let mut portable = source.clone();
    portable.checkpoint.store_kind = ExportStoreKindV1::Portable;
    assert!(encode_loaded_v1(&original, &portable, build.clone()).is_err());
    assert!(load_store_checkpoint_v1(&portable).is_err());
    let (metadata, _) = encode_loaded_v1(&original, &source, build.clone()).unwrap();
    let reexport = expected_v1(&source, &metadata);
    assert!(encode_loaded_v1(&original, &reexport, build.clone()).is_err());
    let mut relative = source.clone();
    relative.checkpoint.store_root = PathBuf::from("relative-store");
    assert!(encode_loaded_v1(&original, &relative, build.clone()).is_err());
    let mut pinned = source;
    pinned.expected_design_sha256 = Some("6".repeat(64));
    assert!(encode_loaded_v1(&original, &pinned, build).is_err());
}

#[test]
fn inference_export_files_are_exclusive_bounded_and_read_back_v1() {
    let (original, source) = fixture_v1(ExportStoreKindV1::Population);
    let directory = unique_directory_v1("inference-export-test");
    let completion =
        write_loaded_v1(&original, &source, &directory, current_build_v1().unwrap()).unwrap();
    let expected: ExportCheckpointSourceV1 =
        serde_json::from_value(completion["reference"].clone()).unwrap();
    let readback = load_inference_export_v1(&expected).unwrap();
    assert_eq!(readback.checkpoint.identity, original.identity);
    assert!(write_loaded_v1(&original, &source, &directory, current_build_v1().unwrap()).is_err());
    let file = OpenOptions::new()
        .write(true)
        .open(directory.join(MODEL_FILENAME_V1))
        .unwrap();
    file.set_len(NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1 as u64 + 1)
        .unwrap();
    drop(file);
    assert!(load_inference_export_v1(&expected).is_err());
    // Only the unique two-file fixture created by this test is removed.
    fs::remove_file(directory.join(MODEL_FILENAME_V1)).unwrap();
    fs::remove_file(directory.join(METADATA_FILENAME_V1)).unwrap();
    fs::remove_dir(directory).unwrap();
}

/// The unchanged importer and explicit V3 transfer accept this exporter's
/// bundle, and the imported model is bit-identical to the exported one.
#[test]
fn inference_export_imports_through_unchanged_v1_reader_and_v3_transfer_v1() {
    let card_db_hash = format!("{KERNEL_CARDDB_HASH:016x}");
    let run = json!({
        "contracts": {
            "model": {"architecture_identity": MODEL_ARCHITECTURE_VERSION_V1},
            "tensorizer": {"feature_contract_digest": FEATURE_CONTRACT_DIGEST_V1,
                "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V1}
        },
        "environment": {"deck_ids": ["Rally", "Rally"], "card_db_hash_u64_hex": card_db_hash}
    });
    let run_bytes = serde_json::to_vec(&run).unwrap();
    let (original, source) =
        fixture_with_run_v1(ExportStoreKindV1::Population, sha256_v1(&run_bytes));
    let root = unique_directory_v1("inference-export-import-test");
    fs::create_dir(&root).unwrap();
    let export_directory = root.join("export");
    let run_path = root.join("run.json");
    let registry_path = root.join("registry.json");
    fs::write(&run_path, &run_bytes).unwrap();
    fs::write(&registry_path, DESTINATION_REGISTRY_V1).unwrap();
    let build = current_build_v1().unwrap();
    write_loaded_v1(&original, &source, &export_directory, build.clone()).unwrap();
    let metadata = fs::read(export_directory.join(METADATA_FILENAME_V1)).unwrap();
    let parameters = fs::read(export_directory.join(MODEL_FILENAME_V1)).unwrap();
    let import = FrozenPlayPolicyImportV1 {
        export_directory: export_directory.clone(),
        expected_metadata_sha256: hash_v1(&metadata),
        expected_model_parameter_sha256: original.identity.model_parameter_sha256.clone(),
        source_run_path: run_path.clone(),
        source_registry_path: registry_path.clone(),
        expected_source_registry_sha256: hash_v1(DESTINATION_REGISTRY_V1),
        source_registry_git_commit: build["source_git_commit"].as_str().unwrap().to_owned(),
        expected_destination_card_db_hash: card_db_hash,
    };
    let transfer = FrozenPlayObservationTransferV3 {
        expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
        expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
    };
    let policy = FrozenPlayPolicyV1::load_feature_transfer_v3(&import, &transfer).unwrap();
    let actual = policy.actual_model_identity_v1();
    assert_eq!(
        actual.model_parameter_sha256,
        original.identity.model_parameter_sha256
    );
    assert_eq!(actual.weights_sha256, hash_v1(&parameters));
    let mut original_section = Vec::new();
    encode_section_v1(
        &mut original_section,
        &original.inference.search_model_v1().parameter_snapshot_v1(),
    );
    assert_eq!(parameters, original_section);
    // The importer still refuses a registry commit that does not name the exporter.
    let mut wrong_commit = import.clone();
    wrong_commit.source_registry_git_commit = "0".repeat(40);
    assert!(FrozenPlayPolicyV1::load_feature_transfer_v3(&wrong_commit, &transfer).is_err());
    // Only the unique fixture files created by this test are removed.
    fs::remove_file(export_directory.join(MODEL_FILENAME_V1)).unwrap();
    fs::remove_file(export_directory.join(METADATA_FILENAME_V1)).unwrap();
    fs::remove_dir(&export_directory).unwrap();
    fs::remove_file(run_path).unwrap();
    fs::remove_file(registry_path).unwrap();
    fs::remove_dir(root).unwrap();
}
