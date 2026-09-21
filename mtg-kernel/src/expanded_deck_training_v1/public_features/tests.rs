use super::*;

fn config() -> Config {
    Config {
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
        inputs_enabled: true,
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        gamma: 1.0,
        lambda: 0.9,
        gpu_ordinal: 1,
        max_chunk_substeps: 128,
        projection_mode: ProjectionMode::All,
    }
}

#[test]
fn public_projection_mode_preserves_legacy_config_bytes() {
    let old = config();
    let bytes = serde_json::to_vec(&old).unwrap();
    let mut wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(wire.get("projection_mode").is_none());
    let parsed: Config = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
    wire["projection_mode"] = json!("all");
    let explicit: Config = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_vec(&explicit).unwrap(), bytes);
    wire["projection_mode"] = json!("state_only");
    let state: Config = serde_json::from_value(wire.clone()).unwrap();
    assert_ne!(serde_json::to_vec(&state).unwrap(), bytes);
    assert_eq!(state.projection_mode, ProjectionMode::StateOnly);
    wire["projection_mode"] = json!("unknown");
    assert!(serde_json::from_value::<Config>(wire).is_err());
}

#[test]
fn public_projection_mode_rejects_masked_parameters_and_moments() {
    let mut config = config();
    config.projection_mode = ProjectionMode::StateOnly;
    let zero = ProjectionSnapshot::zero();
    assert!(validate_projection_mode(&config, &zero).is_ok());
    for array in 0..3 {
        let mut dirty = zero.clone();
        match array {
            0 => dirty.object[0] = 1.0f32.to_bits(),
            1 => dirty.object_first[0] = 1.0f32.to_bits(),
            _ => dirty.object_second[0] = 1.0f32.to_bits(),
        }
        assert!(validate_projection_mode(&config, &dirty).is_err());
    }
    let mut learned = zero.clone();
    learned.state[0] = 0.1f32.to_bits();
    learned.state_first[0] = 0.01f32.to_bits();
    learned.state_second[0] = 0.001f32.to_bits();
    assert!(validate_projection_mode(&config, &learned).is_ok());
    config.inputs_enabled = false;
    assert!(validate_projection_mode(&config, &learned).is_err());
}
