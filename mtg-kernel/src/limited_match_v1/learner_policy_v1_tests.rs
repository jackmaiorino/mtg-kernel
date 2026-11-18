use super::*;
use crate::current_tree_learner_v2::OptimizerConfigV2;
use crate::limited_match_v1::PolicySpecV1;
use crate::state::SplitMix64;
use crate::unclamped_softmax_sampler_v1::UnclampedSoftmaxScratchV1;

#[test]
fn sampler_matches_the_unclamped_softmax_sampler_on_binary32_logits() {
    // On logits a binary32 menu can hold, the f64 path computes the same
    // gaps, weights, sums and threshold as `UnclampedSoftmaxScratchV1`.
    let mut values = SplitMix64::seed(657_301);
    let mut scratch = UnclampedSoftmaxScratchV1::default();
    for case in 0..512 {
        let width = 1 + case % 17;
        let narrow: Vec<f32> = (0..width)
            .map(|_| -(((values.next_u64() >> 8) % 4_001) as f32) / 100.0)
            .collect();
        let wide: Vec<f64> = narrow.iter().map(|&value| f64::from(value)).collect();
        let draw = values.next_u64();
        assert_eq!(
            sample_learner_action_v1(&wide, draw).unwrap(),
            scratch.sample(&narrow, draw).unwrap(),
            "case {case}"
        );
    }
}

#[test]
fn sampler_gives_no_mass_below_the_gap_floor_and_refuses_bad_menus() {
    for draw in [0_u64, 1 << 63, u64::MAX] {
        assert_eq!(sample_learner_action_v1(&[5.0, -800.0], draw), Ok(0));
        assert_eq!(sample_learner_action_v1(&[-800.0, 5.0], draw), Ok(1));
    }
    // Two equal maxima split the draw grid at one half.
    assert_eq!(sample_learner_action_v1(&[1.0, 1.0], (1 << 63) - 1), Ok(0));
    assert_eq!(sample_learner_action_v1(&[1.0, 1.0], 1 << 63), Ok(1));
    assert!(sample_learner_action_v1(&[], 7).is_err());
    assert!(sample_learner_action_v1(&[0.0, f64::NAN], 7).is_err());
    assert!(sample_learner_action_v1(&[f64::INFINITY, 0.0], 7).is_err());
}

#[test]
fn learner_streams_are_per_seat_leg_pair_and_seed_and_their_seeds_are_pinned() {
    let base = MatchStreamV1 {
        seed: 657_101,
        pair: 0,
        leg: 0,
    };
    // SHA-256 over the tag and the four little-endian words, first eight
    // bytes little-endian; computed independently of this crate.
    assert_eq!(learner_stream_seed_v1(base, 0), 0xd088_fafc_c534_7a0a);
    let other = MatchStreamV1 {
        pair: 3,
        leg: 1,
        ..base
    };
    assert_eq!(learner_stream_seed_v1(other, 1), 0xca51_30f8_2ac9_313d);
    let seeds = [
        learner_stream_seed_v1(base, 0),
        learner_stream_seed_v1(base, 1),
        learner_stream_seed_v1(MatchStreamV1 { leg: 1, ..base }, 0),
        learner_stream_seed_v1(MatchStreamV1 { pair: 1, ..base }, 0),
        learner_stream_seed_v1(
            MatchStreamV1 {
                seed: 657_102,
                ..base
            },
            0,
        ),
    ];
    for (i, a) in seeds.iter().enumerate() {
        for b in &seeds[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn a_checkpoint_file_loads_through_its_spec_pinned_by_its_bytes() {
    use crate::limited_campaign_v1::checkpoint::{
        LearnerSnapshotV1, SourceIdentityV1, HAND_KNOWLEDGE_V1,
    };
    use crate::limited_campaign_v1::sha256_hex;
    let identity = SourceIdentityV1 {
        git_head: "0123456789abcdef0123456789abcdef01234567".into(),
        binary_sha256: "11".repeat(32),
        binary_source_sha256: "22".repeat(32),
        card_db_hash: "0123456789abcdef".into(),
        sample_sha256_in_pool_order: "33".repeat(32),
        actor_profile: "fdn-full-pool-consumer/v1".into(),
        actor_producer_sha256: "44".repeat(32),
        hand_knowledge: HAND_KNOWLEDGE_V1.into(),
        reveal_producer_sha256: "55".repeat(32),
        reveal_reducer_sha256: "66".repeat(32),
    };
    let snapshot = LearnerV2::new(OptimizerConfigV2::campaign_v1(200))
        .unwrap()
        .snapshot();
    let checkpoint = CheckpointV1::new(
        &"77".repeat(32),
        &identity,
        None,
        &"88".repeat(32),
        LearnerSnapshotV1::ActorCritic(snapshot),
    )
    .unwrap();
    let bytes = checkpoint.to_bytes().unwrap();
    let directory = std::env::temp_dir().join(format!(
        "limited-eval-learner-checkpoint-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("step_0000.json");
    std::fs::write(&path, &bytes).unwrap();
    let spec = format!("learner-checkpoint:{}", path.display());
    let loaded = match PolicySpecV1::parse(&spec).unwrap() {
        PolicySpecV1::LearnerCheckpoint(loaded) => loaded,
        other => panic!("{other:?}"),
    };
    assert_eq!(loaded.sha256(), sha256_hex(&bytes));
    assert_eq!((loaded.step(), loaded.contract()), (0, CONTRACT_V2));
    // Bytes that are not the canonical encoding (here one extra space) are
    // refused. A valid canonical checkpoint with other contents would load
    // under its own digest: the loader checks form and identity, not which
    // checkpoint it is. The digest recorded in the match summary, which the
    // evaluation tool compares across a run, its replay and a candidate's
    // runs, is what ties the games to one checkpoint file.
    let mut spaced = bytes.clone();
    spaced.push(b' ');
    std::fs::write(&path, &spaced).unwrap();
    assert!(PolicySpecV1::parse(&spec).is_err());
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn snapshots_of_both_profiles_load_and_other_contracts_or_identities_are_refused() {
    let config = OptimizerConfigV2::campaign_v1(200);
    let wide = LearnerV2::new(config.clone()).unwrap().snapshot();
    let narrow = LearnerV2H16::new(config).unwrap().snapshot();
    let loaded =
        LearnerCheckpointV1::from_snapshot_v1("a/step_0000.json", "ab", wide.clone()).unwrap();
    assert_eq!(
        (
            loaded.contract(),
            loaded.step(),
            loaded.sha256(),
            loaded.path()
        ),
        (CONTRACT_V2, 0, "ab", "a/step_0000.json")
    );
    let small = LearnerCheckpointV1::from_snapshot_v1("b", "cd", narrow).unwrap();
    assert_eq!(small.contract(), CONTRACT_V2_H16);
    assert_eq!(
        loaded,
        LearnerCheckpointV1::from_snapshot_v1("a/step_0000.json", "ab", wide.clone()).unwrap()
    );
    assert_ne!(
        loaded,
        LearnerCheckpointV1::from_snapshot_v1("a/step_0000.json", "ef", wide.clone()).unwrap()
    );
    for (field, value) in [
        ("contract", serde_json::json!("current-tree-learner/v1")),
        ("feature_identity", serde_json::json!("00")),
    ] {
        let mut tampered = serde_json::to_value(&wide).unwrap();
        tampered[field] = value;
        let tampered: SnapshotV2 = serde_json::from_value(tampered).unwrap();
        assert!(LearnerCheckpointV1::from_snapshot_v1("a", "ab", tampered).is_err());
    }
}
