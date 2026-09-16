use super::*;
use crate::expanded_deck_training_v1::tests::replay_fixture;
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::phase1_registry_transfer_v1::{
    transfer_fresh_expanded_checkpoint_to_current_registry_v1, FreshRegistryTransferRequestV1,
    RegistryTransferFeaturesV1,
};
use crate::sideboard_play_policy_v1::{FreshPlayPolicyIdentityV1, FRESH_PLAY_INITIALIZATION_SCHEMA_V1};

const LR: f32 = 0.00003;
const VC: f32 = 0.75;
const REGISTRY: &[u8] = include_bytes!("../../../../data/cards_v1.json");
// Shared with the imported family; no distinct fresh schedule schema exists.
const SCHEDULE_SCHEMA: &str = "mtg-kernel-expanded-registry-resolved-schedule/v1";

struct Fixture {
    root: PathBuf,
    source: ExpandedModelSourceV1,
    schedule: ExpandedRegistryTransferScheduleV1,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn pin_bytes(root: &Path, name: &str, bytes: &[u8]) -> PinnedFileV1 {
    let path = root.join(name);
    fs::write(&path, bytes).unwrap();
    PinnedFileV1 {
        path,
        sha256: sha(bytes),
    }
}
fn pin_json(root: &Path, name: &str, value: &impl Serialize) -> PinnedFileV1 {
    pin_bytes(root, name, &serde_json::to_vec(value).unwrap())
}

#[derive(Serialize)]
struct Tensor {
    name: String,
    shape: Vec<usize>,
    values: Vec<u32>,
}
#[derive(Serialize)]
struct FreshCheckpointWire {
    schema: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    source_import: FreshPlayPolicyIdentityV1,
    state_sha256: String,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<Tensor>,
    first_moments: Vec<Tensor>,
    second_moments: Vec<Tensor>,
    trajectories: Vec<Value>,
    loss_identity: String,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
}
fn to_wire(list: &[NativeNamedParameterV1]) -> Vec<Tensor> {
    list.iter()
        .map(|p| Tensor {
            name: p.name.into(),
            shape: p.shape.clone(),
            values: p.values.iter().map(|v| v.to_bits()).collect(),
        })
        .collect()
}

// Reuse real actor-visible engine decisions and actual native forward/backward.
// The enclosing terminal is the existing synthetic unit-test reward fixture.
fn apply(
    state: &mut NativePolicyValueTrainStateV1,
    policy: &FrozenPlayPolicyV1,
    trajectory: &ExpandedTrajectoryV1,
) {
    let tensors = replay_learner_groups_v1(trajectory, policy, None).unwrap();
    let substeps: Vec<Vec<NativePolicySubstepV1<'_>>> = tensors
        .iter()
        .map(|(_, rows)| {
            rows.iter()
                .map(|(row, tensor)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_generic_v1(tensor, FreshLineageGenerationV1::V3),
                    )),
                    selected_action_index: row.selected as usize,
                    expected_raw_action_logit_bits: &row.logits,
                    expected_value_bits: row.value,
                })
                .collect()
        })
        .collect();
    let groups: Vec<_> = substeps
        .iter()
        .zip(&tensors)
        .map(|(steps, (reward, _))| NativePolicyPhysicalDecisionV1 {
            substeps: steps,
            terminal_return: *reward,
            baseline_bits: 0,
        })
        .collect();
    state
        .train_step_feature_transfer_v3(&groups, VC, LR)
        .unwrap();
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "fresh-registry-trainer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();

    // Build the trained native state that becomes the fresh checkpoint. A
    // throwaway imported-origin policy is used purely to generate an episode
    // template and replay real actor-visible tensors; its own ancestry plays
    // no role in the fresh checkpoint being constructed below. `model` is
    // explicitly synced to `policy`'s actual parameters (not merely assumed
    // identical from two separate `runner_fixed_v1` calls), matching
    // `checkpoint_fixture_v1`'s own pattern, and `policy` is resynced to the
    // trained state after every round so its next trajectory is generated
    // and replay-verified against the model it will actually be scored with.
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .unwrap();
    let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    for _ in 0..2 {
        let (trajectory, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0]);
        apply(&mut state, &policy, &trajectory);
        policy
            .replace_training_parameters_v3(&state.snapshot_v1().unwrap().parameters)
            .unwrap();
    }
    let snapshot = state.snapshot_v1().unwrap();
    assert!(snapshot
        .first_moments
        .iter()
        .flat_map(|t| &t.values)
        .any(|v| *v != 0.0));

    // Metadata-only synthetic ancestry. This does not attest to a real
    // transfer, generated model, producer execution or playing strength.
    let identity = FreshPlayPolicyIdentityV1 {
        schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
        initialization_manifest_sha256: "a".repeat(64),
        lineage_id: "fresh-registry-trainer-fixture".into(),
        initializer: "trainer-seeded-v1".into(),
        base_seed: 0,
        model_init_seed: 6_443_515_232_517_447_393,
        seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
        producer_git_commit: "1".repeat(40),
        initial_weights_sha256: "b".repeat(64),
        initial_model_parameter_sha256: "c".repeat(64),
        parameter_layout_sha256: "d".repeat(64),
        destination_registry_sha256: sha(REGISTRY),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        destination_card_count: crate::card_def::CARD_DEFS.len(),
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
        features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
        feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
        sampler_identity: crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
    };
    let state_hex: String = snapshot
        .state_sha256_v1()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let checkpoint = FreshCheckpointWire {
        schema: "mtg-kernel-expanded-deck-fresh-checkpoint/v1".into(),
        feature_contract_digest: identity.feature_contract_digest.clone(),
        feature_encoding_digest: identity.feature_encoding_digest.clone(),
        card_db_hash: identity.destination_card_db_hash.clone(),
        source_import: identity,
        state_sha256: state_hex.clone(),
        adam_step: snapshot.adam_step,
        scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
        parameters: to_wire(&snapshot.parameters),
        first_moments: to_wire(&snapshot.first_moments),
        second_moments: to_wire(&snapshot.second_moments),
        trajectories: vec![],
        loss_identity: "terminal_reinforce_value/v3".into(),
        learning_rate_bits: LR.to_bits(),
        value_coefficient_bits: VC.to_bits(),
    };
    let checkpoint_bytes = serde_json::to_vec(&checkpoint).unwrap();
    let request = FreshRegistryTransferRequestV1 {
        source_checkpoint_sha256: sha(&checkpoint_bytes),
        source_registry_sha256: sha(REGISTRY),
        source_state_sha256: state_hex,
        source_adam_step: snapshot.adam_step,
        source_card_db_hash: checkpoint.card_db_hash.clone(),
        destination_registry_sha256: sha(REGISTRY),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        features: RegistryTransferFeaturesV1::current_v1(),
    };
    let transfer = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
        &checkpoint_bytes,
        REGISTRY,
        &request,
    )
    .unwrap();

    let (template, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0]);
    let batches: Vec<_> = (0..2)
        .map(|index| {
            let mut episode = template.episode.clone();
            episode.id = format!("fresh-transfer-batch-{index}");
            vec![episode]
        })
        .collect();
    let schedule = ExpandedRegistryTransferScheduleV1 {
        schema: SCHEDULE_SCHEMA.into(),
        initial_adam_step: snapshot.adam_step,
        learning_rate_bits: LR.to_bits(),
        value_coefficient_bits: VC.to_bits(),
        batches,
    };
    let descriptor = ExpandedRegistryTransferSourceV1 {
        schema: SOURCE_SCHEMA.into(),
        source_checkpoint: pin_bytes(&root, "original-checkpoint.json", &checkpoint_bytes),
        source_registry: pin_bytes(&root, "original-registry.json", REGISTRY),
        transfer_envelope: pin_bytes(
            &root,
            "transfer-envelope.json",
            &transfer.artifact_bytes_v1().unwrap(),
        ),
        continuation_schedule: pin_json(&root, "schedule.json", &schedule),
    };
    let source = ExpandedModelSourceV1 {
        play_import: pin_json(&root, "source.json", &descriptor),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
        },
        checkpoint: None,
    };
    Fixture {
        root,
        source,
        schedule,
    }
}

fn trajectory(
    policy: &mut FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    episode: &ExpandedEpisodeV1,
) -> ExpandedTrajectoryV1 {
    let (mut trajectory, _, _) = replay_fixture(policy, None, 0, &[0, 1, 0]);
    trajectory.episode = episode.clone();
    trajectory.behavior_state_sha256 = hex(&state.state_sha256_v1().unwrap());
    trajectory.source_import = policy.identity_v1().clone();
    validate_trajectory(&trajectory).unwrap();
    trajectory
}

#[test]
fn phase1_fresh_registry_trainer_real_update_reloads_exact_adam_and_transfer_provenance() {
    let f = fixture();
    let (mut policy, mut expected, context) = initialize_with_transfer_context(&f.source).unwrap();
    assert_eq!(expected.adam_step_v1(), 2);
    assert!(policy.identity_v1().is_fresh_v1());
    assert!(policy.identity_v1().as_imported_v1().is_none());
    assert_eq!(
        policy.actual_model_identity_v1().card_db_hash,
        format!("{KERNEL_CARDDB_HASH:016x}")
    );
    assert_eq!(
        policy.actual_model_identity_v1().model_parameter_sha256,
        expected.model_v1().parameter_manifest_sha256_v1()
    );
    let first = trajectory(&mut policy, &expected, &f.schedule.batches[0][0]);
    let trajectory_pin = pin_json(&f.root, "batch-0.json", &first);
    apply(&mut expected, &policy, &first);
    let result = execute_v1(ExpandedTrainingCommandV1::Update {
        source: f.source.clone(),
        trajectories: vec![trajectory_pin],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        output_directory: f.root.join("update-0"),
    })
    .unwrap();
    let checkpoint: PinnedFileV1 = serde_json::from_value(result["checkpoint"].clone()).unwrap();
    let saved: ExpandedCheckpointV1 = read_pinned(&checkpoint).unwrap();
    assert_eq!(saved.schema, CHECKPOINT_SCHEMA_TRANSFER);
    assert!(saved.source_import.is_fresh_v1());
    assert_eq!(
        saved.registry_transfer,
        Some(context.unwrap().after_update(3).unwrap())
    );
    let source = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint),
        ..f.source.clone()
    };
    // A second load on this lineage must still route through the fresh
    // registry-transfer source, never falling through to the ordinary reader.
    let (mut resumed_policy, resumed, next) = initialize_with_transfer_context(&source).unwrap();
    assert_eq!(
        expected.snapshot_v1().unwrap(),
        resumed.snapshot_v1().unwrap()
    );
    assert!(resumed_policy.identity_v1().is_fresh_v1());
    next.as_ref()
        .unwrap()
        .validate_batch(&f.schedule.batches[1])
        .unwrap();
    assert!(next
        .as_ref()
        .unwrap()
        .validate_batch(&f.schedule.batches[0])
        .is_err());
    let second = trajectory(&mut resumed_policy, &resumed, &f.schedule.batches[1][0]);
    let pin = pin_json(&f.root, "batch-1.json", &second);
    let mut resumed_state = resumed;
    apply(&mut resumed_state, &resumed_policy, &second);
    let second_result = execute_v1(ExpandedTrainingCommandV1::Update {
        source: source.clone(),
        trajectories: vec![pin],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        output_directory: f.root.join("update-1"),
    })
    .unwrap();
    let last_pin: PinnedFileV1 =
        serde_json::from_value(second_result["checkpoint"].clone()).unwrap();
    let last_saved: ExpandedCheckpointV1 = read_pinned(&last_pin).unwrap();
    assert_eq!(last_saved.schema, CHECKPOINT_SCHEMA_TRANSFER);
    assert_eq!(
        last_saved.registry_transfer.as_ref().unwrap().completed_updates,
        2
    );
    assert_eq!(last_saved.adam_step, resumed_state.adam_step_v1());
}

#[test]
fn phase1_fresh_registry_trainer_rejects_wrong_scalars_before_publication() {
    let f = fixture();
    let (mut policy, state) = super::super::initialize(&f.source).unwrap();
    let first = trajectory(&mut policy, &state, &f.schedule.batches[0][0]);
    let pin = pin_json(&f.root, "trajectory.json", &first);
    let output = f.root.join("wrong-lr");
    let error = execute_v1(ExpandedTrainingCommandV1::Update {
        source: f.source.clone(),
        trajectories: vec![pin],
        learning_rate: LR * 2.0,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        output_directory: output.clone(),
    })
    .unwrap_err();
    assert!(error.contains("scalar settings"));
    assert!(!output.exists());
}

#[test]
fn phase1_fresh_registry_source_schema_document_is_rejected_by_the_ordinary_reader() {
    let f = fixture();
    assert!(validate_ordinary_source_descriptor_v1(&ExpandedModelSourceV1 {
        checkpoint: Some(f.source.play_import.clone()),
        ..f.source.clone()
    })
    .is_err());
    assert!(load_ordinary_bo3_parent_v1(&ExpandedModelSourceV1 {
        checkpoint: Some(f.source.play_import.clone()),
        ..f.source.clone()
    })
    .is_err());
}
