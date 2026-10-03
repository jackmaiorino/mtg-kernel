use super::*;

fn config() -> Config {
    Config {
        schema: "mtg-kernel-stack-training-config/v1".into(),
        stack_contract_sha256: sha(CONTRACT),
        permutation_contract_sha256: sha(PERMUTATION_CONTRACT),
        source: ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: "test-import.json".into(),
                sha256: "0".repeat(64),
            },
            checkpoint: None,
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
            },
        },
        updates: Vec::new(),
        input_mode: StackInputModeV1::Structured,
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        gamma: 1.0,
        lambda: 0.9,
        gpu_ordinal: 1,
        max_chunk_substeps: 128,
        entropy_coefficient: 0.0,
    }
}

#[test]
fn public_stack_training_rejects_contract_drift_and_disabled_learned_state() {
    let original = config();
    validate_entropy(&original).unwrap();
    for which in 0..3 {
        let mut changed = original.clone();
        match which {
            0 => changed.schema.push('x'),
            1 => changed.stack_contract_sha256.push('x'),
            _ => changed.permutation_contract_sha256.push('x'),
        };
        assert!(validate_entropy(&changed).is_err());
    }
    let mut disabled = original.clone();
    disabled.input_mode = StackInputModeV1::Disabled;
    let zero = StackProjectionSnapshot::zero();
    validate_projection_mode(&disabled, &zero).unwrap();
    for which in 0..3 {
        let mut changed = zero.clone();
        match which {
            0 => changed.weight[0] = 0.1f32.to_bits(),
            1 => changed.first[0] = 0.1f32.to_bits(),
            _ => changed.second[0] = 0.1f32.to_bits(),
        };
        assert!(validate_projection_mode(&disabled, &changed).is_err());
        validate_projection_mode(&original, &changed).unwrap();
    }
    let mut permuted = original.clone();
    permuted.input_mode = StackInputModeV1::Permuted;
    assert_ne!(
        serde_json::to_vec(&permuted).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.01] {
        let mut changed = original.clone();
        changed.entropy_coefficient = value;
        assert!(validate_entropy(&changed).is_err());
    }
}
