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
        entropy_coefficient: 0.0,
        ppo: None,
    }
}

#[test]
fn public_entropy_preserves_zero_config_and_rejects_invalid_values() {
    let old = config();
    let bytes = serde_json::to_vec(&old).unwrap();
    let mut wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(wire.get("entropy_coefficient").is_none());
    for coefficient in [0.0_f32, -0.0] {
        wire["entropy_coefficient"] = json!(coefficient);
        let parsed: Config = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
        validate_entropy(&parsed).unwrap();
    }
    for coefficient in [0.01, 0.05, 1.0] {
        let mut changed = old.clone();
        changed.entropy_coefficient = coefficient;
        validate_entropy(&changed).unwrap();
        assert_ne!(serde_json::to_vec(&changed).unwrap(), bytes);
    }
    for coefficient in [-0.1, 1.01, f32::NAN, f32::INFINITY] {
        let mut invalid = old.clone();
        invalid.entropy_coefficient = coefficient;
        assert!(validate_entropy(&invalid).is_err());
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

#[test]
fn public_execution_device_preserves_config_and_legacy_default() {
    let source = config();
    let bytes = serde_json::to_vec(&source).unwrap();
    let mut wire =
        json!({"config":source,"output_directory":"test-output","resume":null,"stop_after":1});
    let original: Command = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(original.execution_gpu_ordinal, None);
    assert_eq!(
        original
            .execution_gpu_ordinal
            .unwrap_or(original.config.gpu_ordinal),
        1
    );
    for device in [0, 1] {
        wire["execution_gpu_ordinal"] = json!(device);
        let placed: Command = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(placed.execution_gpu_ordinal, Some(device));
        assert_eq!(serde_json::to_vec(&placed.config).unwrap(), bytes);
    }
    wire["execution_gpu_ordinal"] = json!(-1);
    assert!(serde_json::from_value::<Command>(wire).is_err());
}

#[test]
fn public_ppo_preserves_absent_config_and_rejects_invalid_values() {
    let old = config();
    let bytes = serde_json::to_vec(&old).unwrap();
    let mut wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(wire.get("ppo").is_none());
    assert_eq!(adam_steps_per_update(&old), 1);
    wire["ppo"] = Value::Null;
    let parsed: Config = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
    let ppo = PpoConfigV1 {
        epochs: 4,
        minibatches: 8,
        clip: 0.2,
        shuffle_seed: 7,
    };
    let mut changed = old.clone();
    changed.ppo = Some(ppo);
    validate_entropy(&changed).unwrap();
    assert_ne!(serde_json::to_vec(&changed).unwrap(), bytes);
    assert_eq!(adam_steps_per_update(&changed), 32);
    for invalid in [
        PpoConfigV1 { epochs: 0, ..ppo },
        PpoConfigV1 { epochs: 17, ..ppo },
        PpoConfigV1 {
            minibatches: 0,
            ..ppo
        },
        PpoConfigV1 {
            minibatches: 65,
            ..ppo
        },
        PpoConfigV1 { clip: 0.0, ..ppo },
        PpoConfigV1 { clip: 1.5, ..ppo },
        PpoConfigV1 {
            clip: f32::NAN,
            ..ppo
        },
    ] {
        let mut config = old.clone();
        config.ppo = Some(invalid);
        assert!(validate_entropy(&config).is_err());
    }
    wire["ppo"] = json!({"epochs":4,"minibatches":8,"clip":0.2,"shuffle_seed":7,"extra":1});
    assert!(serde_json::from_value::<Config>(wire).is_err());
}

#[test]
fn ppo_minibatch_partition_is_deterministic_and_disjoint() {
    for groups in [1_usize, 7, 64, 1000] {
        for minibatches in [1_u32, 3, 8] {
            let a = ppo_minibatches(groups, minibatches, 11, 2, 1);
            assert_eq!(a, ppo_minibatches(groups, minibatches, 11, 2, 1));
            assert_eq!(a.len(), (minibatches as usize).min(groups));
            assert!(a.iter().all(|m| !m.is_empty()));
            let mut seen: Vec<usize> = a.concat();
            seen.sort_unstable();
            assert_eq!(seen, (0..groups).collect::<Vec<_>>());
            assert!(a.iter().all(|m| m.windows(2).all(|w| w[0] < w[1])));
        }
    }
    assert_ne!(
        ppo_minibatches(1000, 8, 11, 2, 1),
        ppo_minibatches(1000, 8, 11, 2, 2)
    );
}

#[test]
fn ppo_old_joint_log_probability_matches_f64_softmax() {
    let logits = [1.0_f32, 2.0, 0.5].map(f32::to_bits);
    let second = [0.0_f32, -1.0].map(f32::to_bits);
    let joint = behavior_joint_log_probability(&[(&logits[..], 1), (&second[..], 0)]).unwrap();
    let lse1 = (1.0_f64.exp() + 2.0_f64.exp() + 0.5_f64.exp()).ln();
    let lse2 = (0.0_f64.exp() + (-1.0_f64).exp()).ln();
    let expected = (2.0 - lse1) + (0.0 - lse2);
    assert!((f64::from(joint) - expected).abs() < 1e-6);
    assert!(behavior_joint_log_probability(&[(&logits[..], 3)]).is_err());
}
