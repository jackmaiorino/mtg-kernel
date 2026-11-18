use super::*;
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "limited-campaign-checkpoint-{name}-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("checkpoints")).unwrap();
    fs::create_dir_all(root.join("iterations")).unwrap();
    root
}

fn identity() -> SourceIdentityV1 {
    SourceIdentityV1 {
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
    }
}

const CAMPAIGN: &str = "77777777777777777777777777777777777777777777777777777777777777ff";

/// The step-0 snapshot with `step` and one parameter changed, as a stand-in
/// for an updated learner (restore admits nonzero moments only after step 0,
/// and these stay zero).
fn snapshot_at(step: u64, marker: f64) -> SnapshotV2 {
    let initial = LearnerV2::new(OptimizerConfigV2::campaign_v1(3))
        .unwrap()
        .snapshot();
    if step == 0 {
        return initial;
    }
    let mut value = serde_json::to_value(initial).unwrap();
    value["step"] = json!(step);
    value["parameter_bits"][0] = json!(marker.to_bits());
    serde_json::from_value(value).unwrap()
}

fn write(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
}

/// Writes update `step` (receipt, checkpoint, complete record) and returns
/// the checkpoint's SHA-256.
fn commit(root: &Path, step: u64, parent: Option<String>, marker: f64) -> String {
    let directory = iteration_dir_v1(root, step);
    fs::create_dir_all(&directory).unwrap();
    let receipt = format!("{{\"update\":{step}}}");
    write(&directory.join("receipt.json"), receipt.as_bytes());
    let receipt_sha256 = sha256_hex(receipt.as_bytes());
    let checkpoint = CheckpointV1::new(
        CAMPAIGN,
        &identity(),
        parent,
        &receipt_sha256,
        LearnerSnapshotV1::ActorCritic(snapshot_at(step, marker)),
    )
    .unwrap();
    let bytes = checkpoint.to_bytes().unwrap();
    write(&checkpoint_path_v1(root, step), &bytes);
    let sha256 = sha256_hex(&bytes);
    let complete = IterationCompleteV1::new(step, &sha256, &receipt_sha256, None);
    write(
        &directory.join("complete.json"),
        &complete.to_bytes().unwrap(),
    );
    sha256
}

fn chain(name: &str) -> (PathBuf, [String; 3]) {
    let root = temp_root(name);
    let first = commit(&root, 0, None, 0.);
    let second = commit(&root, 1, Some(first.clone()), 0.25);
    let third = commit(&root, 2, Some(second.clone()), 0.5);
    (root, [first, second, third])
}

#[test]
fn checkpoint_round_trips_in_canonical_form_and_restores_its_learner() {
    let snapshot = snapshot_at(0, 0.);
    let receipt = "88".repeat(32);
    let checkpoint = CheckpointV1::new(
        CAMPAIGN,
        &identity(),
        None,
        &receipt,
        LearnerSnapshotV1::ActorCritic(snapshot.clone()),
    )
    .unwrap();
    assert_eq!(checkpoint.schema, CHECKPOINT_SCHEMA_V1);
    assert_eq!(checkpoint.step, 0);
    assert_eq!(checkpoint.learner_contract, CONTRACT_V2);
    assert_eq!(checkpoint.feature_sha256, feature_identity_v2());
    assert_eq!(checkpoint.identity(), identity());
    let bytes = checkpoint.to_bytes().unwrap();
    let (parsed, sha256) = CheckpointV1::parse_v1(&bytes).unwrap();
    assert_eq!(parsed, checkpoint);
    assert_eq!(sha256, sha256_hex(&bytes));
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for field in [
        "schema",
        "git_head",
        "binary_sha256",
        "binary_source_sha256",
        "card_db_hash",
        "sample_sha256_in_pool_order",
        "actor_profile",
        "actor_producer_sha256",
        "hand_knowledge",
        "reveal_producer_sha256",
        "reveal_reducer_sha256",
        "learner_contract",
        "feature_sha256",
        "optimizer_config",
        "parent_checkpoint_sha256",
        "iteration_receipt_sha256",
        "snapshot",
    ] {
        assert!(value.get(field).is_some(), "{field}");
    }
    assert!(value["parent_checkpoint_sha256"].is_null());
    for field in [
        "step",
        "parameter_bits",
        "first_moment_bits",
        "second_moment_bits",
    ] {
        assert!(value["snapshot"].get(field).is_some(), "{field}");
    }
    match parsed.restore_learner_v1().unwrap() {
        CampaignLearnerV1::ActorCriticH32(learner) => {
            assert_eq!(learner.snapshot(), snapshot)
        }
        _ => panic!("wrong learner family"),
    }
    // Any whitespace makes the bytes non-canonical.
    let mut spaced = bytes.clone();
    spaced.push(b'\n');
    assert!(CheckpointV1::parse_v1(&spaced).is_err());
}

#[test]
fn narrow_profile_and_control_arm_snapshots_round_trip() {
    let narrow = LearnerV2H16::new(OptimizerConfigV2::campaign_v1(2)).unwrap();
    let control = LearnerV1::new(
        ModelV1::deterministic_fixture(),
        AdamConfigV1 {
            learning_rate: 3e-4,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
        },
    )
    .unwrap();
    for (snapshot, kind) in [
        (
            LearnerSnapshotV1::ActorCritic(narrow.snapshot()),
            LearnerKindV1::ActorCriticH16,
        ),
        (
            LearnerSnapshotV1::PairCenteredControl(control.snapshot()),
            LearnerKindV1::PairCenteredControl,
        ),
    ] {
        let checkpoint =
            CheckpointV1::new(CAMPAIGN, &identity(), None, &"99".repeat(32), snapshot).unwrap();
        assert_eq!(checkpoint.learner_contract, kind.contract());
        let bytes = checkpoint.to_bytes().unwrap();
        let (parsed, _) = CheckpointV1::parse_v1(&bytes).unwrap();
        assert_eq!(parsed, checkpoint);
        assert_eq!(parsed.learner_kind().unwrap(), kind);
        assert_eq!(parsed.restore_learner_v1().unwrap().kind(), kind);
        assert_eq!(
            parsed.restore_learner_v1().unwrap().snapshot(),
            checkpoint.snapshot
        );
    }
    assert!(LearnerKindV1::from_contract("current-tree-learner/v3").is_err());
}

#[test]
fn checkpoint_validation_refuses_inconsistent_fields() {
    let good = CheckpointV1::new(
        CAMPAIGN,
        &identity(),
        Some("aa".repeat(32)),
        &"bb".repeat(32),
        LearnerSnapshotV1::ActorCritic(snapshot_at(1, 0.25)),
    )
    .unwrap();
    good.validate().unwrap();
    let refuse = |change: &dyn Fn(&mut CheckpointV1)| {
        let mut checkpoint = good.clone();
        change(&mut checkpoint);
        assert!(checkpoint.validate().is_err());
        assert!(CheckpointV1::parse_v1(&checkpoint.to_bytes().unwrap()).is_err());
    };
    refuse(&|c| c.schema = "limited-campaign-checkpoint/v2".into());
    refuse(&|c| c.parent_checkpoint_sha256 = None);
    refuse(&|c| c.parent_checkpoint_sha256 = Some("AA".repeat(32)));
    refuse(&|c| c.step = 2);
    refuse(&|c| c.learner_contract = CONTRACT_V2_H16.into());
    refuse(&|c| c.feature_sha256 = "cc".repeat(32));
    refuse(&|c| {
        c.optimizer_config = OptimizerConfigV1::ActorCritic(OptimizerConfigV2::campaign_v1(9))
    });
    refuse(&|c| c.iteration_receipt_sha256 = "short".into());
    refuse(&|c| c.campaign_sha256 = String::new());
    refuse(&|c| c.card_db_hash = "0123456789ABCDEF".into());
    refuse(&|c| c.hand_knowledge = "public-reveal-minima/v2".into());
    refuse(&|c| c.binary_sha256 = "x".repeat(64));
    // A step-0 checkpoint has no parent.
    assert!(CheckpointV1::new(
        CAMPAIGN,
        &identity(),
        Some("aa".repeat(32)),
        &"bb".repeat(32),
        LearnerSnapshotV1::ActorCritic(snapshot_at(0, 0.)),
    )
    .is_err());
}

#[test]
fn chain_verifies_from_step_zero_to_the_committed_head() {
    let (root, digests) = chain("verifies");
    let verified = verify_chain_v1(&root, 2).unwrap();
    assert_eq!(verified.len(), 3);
    for (link, digest) in verified.iter().zip(&digests) {
        assert_eq!(&link.sha256, digest);
    }
    assert_eq!(verified[0].checkpoint.parent_checkpoint_sha256, None);
    assert_eq!(
        verified[2].checkpoint.parent_checkpoint_sha256.as_deref(),
        Some(digests[1].as_str())
    );
    assert_eq!(verify_chain_v1(&root, 1).unwrap().len(), 2);
    assert!(verify_chain_v1(&root, 3).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn chain_refuses_every_tampered_digest() {
    let tampered = |name: &str, change: &dyn Fn(&Path, &[String; 3])| {
        let (root, digests) = chain(name);
        verify_chain_v1(&root, 2).unwrap();
        change(&root, &digests);
        assert!(verify_chain_v1(&root, 2).is_err(), "{name}");
        fs::remove_dir_all(root).unwrap();
    };
    // A changed parameter bit in the middle checkpoint, re-encoded canonically.
    tampered("parameter", &|root, digests| {
        let receipt = sha256_hex(b"{\"update\":1}");
        let changed = CheckpointV1::new(
            CAMPAIGN,
            &identity(),
            Some(digests[0].clone()),
            &receipt,
            LearnerSnapshotV1::ActorCritic(snapshot_at(1, 0.375)),
        )
        .unwrap();
        write(&checkpoint_path_v1(root, 1), &changed.to_bytes().unwrap());
    });
    // Step 0 rewritten and its own complete record updated to match: the
    // next step's parent digest no longer holds.
    tampered("parent", &|root, _| {
        let receipt = sha256_hex(b"{\"update\":0}");
        let mut other = identity();
        other.git_head = "fedcba9876543210fedcba9876543210fedcba98".into();
        let changed = CheckpointV1::new(
            CAMPAIGN,
            &other,
            None,
            &receipt,
            LearnerSnapshotV1::ActorCritic(snapshot_at(0, 0.)),
        )
        .unwrap();
        let bytes = changed.to_bytes().unwrap();
        write(&checkpoint_path_v1(root, 0), &bytes);
        let complete = IterationCompleteV1::new(0, &sha256_hex(&bytes), &receipt, None);
        write(
            &iteration_dir_v1(root, 0).join("complete.json"),
            &complete.to_bytes().unwrap(),
        );
    });
    tampered("receipt", &|root, _| {
        write(
            &iteration_dir_v1(root, 1).join("receipt.json"),
            b"{\"update\":10}",
        );
    });
    tampered("missing-complete", &|root, _| {
        fs::remove_file(iteration_dir_v1(root, 2).join("complete.json")).unwrap();
    });
    tampered("non-canonical", &|root, _| {
        let path = checkpoint_path_v1(root, 2);
        let mut bytes = fs::read(&path).unwrap();
        bytes.insert(1, b' ');
        write(&path, &bytes);
    });
    tampered("unrecorded-dev", &|root, _| {
        write(&iteration_dir_v1(root, 1).join("dev.json"), b"{}");
    });
    tampered("complete-digest", &|root, digests| {
        let receipt = sha256_hex(b"{\"update\":2}");
        let complete = IterationCompleteV1::new(2, &digests[1], &receipt, None);
        write(
            &iteration_dir_v1(root, 2).join("complete.json"),
            &complete.to_bytes().unwrap(),
        );
    });
    tampered("complete-update", &|root, digests| {
        let receipt = sha256_hex(b"{\"update\":2}");
        let complete = IterationCompleteV1::new(1, &digests[2], &receipt, None);
        write(
            &iteration_dir_v1(root, 2).join("complete.json"),
            &complete.to_bytes().unwrap(),
        );
    });
    // An identity change after step 0, with a consistent chain otherwise.
    tampered("identity", &|root, digests| {
        let receipt = sha256_hex(b"{\"update\":2}");
        let mut other = identity();
        other.binary_sha256 = "12".repeat(32);
        let changed = CheckpointV1::new(
            CAMPAIGN,
            &other,
            Some(digests[1].clone()),
            &receipt,
            LearnerSnapshotV1::ActorCritic(snapshot_at(2, 0.5)),
        )
        .unwrap();
        let bytes = changed.to_bytes().unwrap();
        write(&checkpoint_path_v1(root, 2), &bytes);
        let complete = IterationCompleteV1::new(2, &sha256_hex(&bytes), &receipt, None);
        write(
            &iteration_dir_v1(root, 2).join("complete.json"),
            &complete.to_bytes().unwrap(),
        );
    });
}

#[test]
fn complete_record_refuses_malformed_bytes() {
    let complete = IterationCompleteV1::new(3, &"aa".repeat(32), &"bb".repeat(32), None);
    let bytes = complete.to_bytes().unwrap();
    assert_eq!(IterationCompleteV1::parse_v1(&bytes).unwrap(), complete);
    let mut spaced = bytes.clone();
    spaced.push(b' ');
    assert!(IterationCompleteV1::parse_v1(&spaced).is_err());
    let mut wrong = complete.clone();
    wrong.dev_sha256 = Some("dev".into());
    assert!(IterationCompleteV1::parse_v1(&wrong.to_bytes().unwrap()).is_err());
    wrong = complete;
    wrong.schema = "other".into();
    assert!(IterationCompleteV1::parse_v1(&wrong.to_bytes().unwrap()).is_err());
}
