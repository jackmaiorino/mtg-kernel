use super::*;
use crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;

struct Fixture {
    source: ExpandedModelSourceV1,
    first_episode: PinnedFileV1,
}

fn fixture(root: &Path, attempt: &str, changed_action: bool, changed_moment: bool) -> Fixture {
    let data = root.join("inputs");
    let run = root.join(attempt);
    fs::create_dir_all(&run).unwrap();
    let features = FRESH_FEATURE_IDENTITY_V4;
    let descriptor = write_synthetic_fresh_source_with_parameters_v1(&data, features, |_| {});
    let mut source = ExpandedModelSourceV1 {
        play_import: publish_json(&data, "descriptor.json", &descriptor).unwrap(),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: features.feature_contract_digest.into(),
            expected_feature_encoding_digest: features.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    };
    let policy =
        fresh_initialization_source::load_policy_v1(&descriptor, &source.feature_transfer).unwrap();
    let state = NativePolicyValueTrainStateV1::new_v1(policy.training_model_v3()).unwrap();
    let fresh = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity: inference_identity_v1(&source, &policy, &state).unwrap(),
    };
    let (_, _, mut saved) = super::super::tests::checkpoint_fixture_v1();
    saved.schema = FRESH_CHECKPOINT_SCHEMA.into();
    saved.source_import = policy.identity_v1().clone();
    saved.feature_contract_digest = features.feature_contract_digest.into();
    saved.feature_encoding_digest = features.feature_encoding_digest.into();
    let mut snapshot = state.snapshot_v1().unwrap();
    let embedding = snapshot
        .first_moments
        .iter()
        .position(|parameter| parameter.name == "card_embedding.weight")
        .unwrap();
    // Row zero is frozen padding, including its optimizer moments. Exercise
    // both Adam buffers at the first coordinate of a trainable embedding row.
    let moment_index = snapshot.first_moments[embedding].shape[1];
    snapshot.first_moments[embedding].values[moment_index] = 0.0625;
    snapshot.second_moments[embedding].values[moment_index] = 0.03125;
    saved.scorer_bias_anchor_bits = snapshot.scorer_bias_anchor_bits;
    saved.parameters = snapshot
        .parameters
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.second_moments = snapshot
        .second_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let (mut episode, _, _) = super::super::tests::replay_fixture(
        &mut FrozenPlayPolicyV1::training_fixture_v3(),
        Some(&mut opponent),
        0,
        &[0],
    );
    episode.schema = FRESH_TRAJECTORY_SCHEMA.into();
    episode.feature_contract_digest = features.feature_contract_digest.into();
    episode.feature_encoding_digest = features.feature_encoding_digest.into();
    episode.source_import = policy.identity_v1().clone();
    episode.behavior_state_sha256 = fresh.identity.state_sha256.clone();
    episode.episode.opponent = Some(source.clone());
    episode.seat_behaviors = Some([fresh.clone(), fresh.clone()]);
    let original_action = episode.decisions[0].selected;
    if changed_action {
        episode.decisions[0].selected =
            (original_action + 1) % episode.decisions[0].logits.len() as u32;
    }
    let first_episode = publish_json(&run, "first-episode.json", &episode).unwrap();
    episode.decisions[0].selected = original_action;

    saved.trajectories = vec![first_episode.clone()];
    saved.adam_step = 1;
    snapshot.adam_step = 1;
    let original_moment = snapshot.first_moments[embedding].values[moment_index];
    if changed_moment {
        snapshot.first_moments[embedding].values[moment_index] = 0.125;
    }
    saved.state_sha256 = hex(&snapshot.state_sha256_v1().unwrap());
    saved.first_moments = snapshot
        .first_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    source.checkpoint = Some(if attempt == "b001-a1" {
        publish_json(&run, "checkpoint-1.json", &saved).unwrap()
    } else {
        let bytes = checkpoint_container_v1::encode_v1(&saved).unwrap();
        publish_json_bytes_v1(&run, "checkpoint-1.json", &bytes).unwrap()
    });
    let (_, prior_identity) = load_expanded_inference_any_math_v1(&source).unwrap();
    episode.behavior_state_sha256 = prior_identity.state_sha256.clone();
    episode.seat_behaviors = Some([
        ExpandedSeatBehaviorV1 {
            source: source.clone(),
            identity: prior_identity,
        },
        fresh,
    ]);
    episode.episode.id = "second-update".into();
    let bytes = serde_json::to_vec(&episode).unwrap();
    let sealed = RecordEnvelopeV1::Zstd1.seal_owned_v1(bytes).unwrap();
    saved.trajectories = vec![publish_json_bytes_v1(&run, "second-episode.json", &sealed).unwrap()];
    saved.adam_step = 2;
    snapshot.adam_step = 2;
    snapshot.first_moments[embedding].values[moment_index] = original_moment;
    saved.state_sha256 = hex(&snapshot.state_sha256_v1().unwrap());
    saved.first_moments = snapshot
        .first_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    source.checkpoint = Some(publish_json(&run, "checkpoint-2.json", &saved).unwrap());
    Fixture {
        source,
        first_episode,
    }
}

fn checkpoint(root: &Path, pin: &PinnedFileV1) -> Box<ExpandedCheckpointV1> {
    let Record::Checkpoint(checkpoint) = Object::new(Kind::Checkpoint, pin)
        .unwrap()
        .read(Some(root))
        .unwrap()
    else {
        panic!("expected checkpoint")
    };
    checkpoint
}

#[test]
fn portable_chain_v1_relocates_and_binds_the_complete_ancestry() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp =
        std::env::temp_dir().join(format!("mtg-portable-chain-{}-{nonce}", std::process::id()));
    fs::create_dir(&temp).unwrap();
    let input_a = temp.join("root-a");
    let input_b = temp.join("root-b");
    let a = fixture(&input_a, "b001-a1", false, false);
    let b = fixture(&input_b, "b001-a7", false, false);
    assert_ne!(a.source.checkpoint, b.source.checkpoint);
    assert_ne!(a.first_episode.sha256, b.first_episode.sha256);
    let package_a = temp.join("package-a");
    let package_b = temp.join("package-b");
    let first = write_v1(&a.source, &package_a).unwrap();
    let second = write_v1(&b.source, &package_b).unwrap();
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(
        fs::read(first.path).unwrap(),
        fs::read(second.path).unwrap()
    );

    let (_, mut expected) = load_expanded_inference_any_math_v1(&a.source).unwrap();
    for (name, action, moment) in [
        ("changed-action", true, false),
        ("changed-moment", false, true),
    ] {
        let changed = fixture(
            &temp.join(format!("{name}-input")),
            "b001-a8",
            action,
            moment,
        );
        let package = temp.join(name);
        let manifest = write_v1(&changed.source, &package).unwrap();
        assert_ne!(first.sha256, manifest.sha256);
        assert_eq!(
            inspect_v1(&package, &manifest.sha256).unwrap().state_sha256,
            expected.state_sha256
        );
    }

    fs::write(&a.first_episode.path, b"broken input").unwrap();
    let rejected = temp.join("rejected");
    assert!(write_v1(&a.source, &rejected).unwrap_err().contains("SHA"));
    assert!(!rejected.join(MANIFEST).exists());
    fs::remove_dir_all(&input_a).unwrap();
    fs::remove_dir_all(&input_b).unwrap();
    let relocated = temp.join("different-root");
    fs::rename(&package_a, &relocated).unwrap();
    let root = fs::canonicalize(&relocated).unwrap();
    let chain: Chain = serde_json::from_slice(&fs::read(root.join(MANIFEST)).unwrap()).unwrap();
    let final_pin = chain.source.checkpoint.unwrap();
    expected.checkpoint_sha256 = Some(final_pin.sha256.clone());
    assert!(inspect_v1(&relocated, &"0".repeat(64))
        .unwrap_err()
        .contains("manifest SHA"));
    assert_eq!(inspect_v1(&relocated, &first.sha256).unwrap(), expected);

    let saved = checkpoint(&root, &final_pin);
    let Record::Trajectory(last_episode) = Object::new(Kind::Trajectory, &saved.trajectories[0])
        .unwrap()
        .read(Some(&root))
        .unwrap()
    else {
        panic!("expected trajectory")
    };
    let prior = last_episode.seat_behaviors.unwrap()[0]
        .source
        .checkpoint
        .clone()
        .unwrap();
    let prior_episode = &checkpoint(&root, &prior).trajectories[0].path;
    fs::write(root.join(prior_episode), b"broken earlier episode").unwrap();
    assert!(inspect_v1(&relocated, &first.sha256)
        .unwrap_err()
        .contains("SHA"));
    fs::remove_dir_all(temp).unwrap();
}
