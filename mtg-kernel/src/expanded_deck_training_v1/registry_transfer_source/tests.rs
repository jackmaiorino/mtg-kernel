use super::*;
use crate::expanded_deck_training_v1::tests::{checkpoint_fixture_v1, replay_fixture};
use crate::phase1_registry_transfer_v1::{
    transfer_expanded_checkpoint_to_current_registry_v1, RegistryTransferFeaturesV1,
    RegistryTransferRequestV1,
};
use crate::sideboard_play_policy_v1::FrozenPlayObservationReceiptV3;

const LR: f32 = 0.00003;
const VC: f32 = 0.75;
const REGISTRY: &[u8] = include_bytes!("../../../../data/cards_v1.json");

struct Fixture {
    root: PathBuf,
    source: ExpandedModelSourceV1,
    descriptor: ExpandedRegistryTransferSourceV1,
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
                        encoded_decision_view_v3(tensor),
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
        "registry-trainer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let (mut policy, model, mut saved) = checkpoint_fixture_v1();
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
    saved.parameters = snapshot
        .parameters
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.first_moments = snapshot
        .first_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.second_moments = snapshot
        .second_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.state_sha256 = hex(&snapshot.state_sha256_v1().unwrap());
    saved.adam_step = snapshot.adam_step;
    saved.learning_rate_bits = LR.to_bits();
    saved.value_coefficient_bits = VC.to_bits();
    let PlayPolicyOriginV1::Imported(origin) = &mut saved.source_import else {
        panic!("registry transfer fixture must retain imported origin")
    };
    origin.destination_registry_sha256 = sha(REGISTRY);
    origin.observation_successor = Some(FrozenPlayObservationReceiptV3 {
        schema: "mtg-kernel-frozen-play-observation-transfer/v3".into(),
        source_feature_contract_digest: "1".repeat(64),
        source_feature_encoding_digest: "2".repeat(64),
        destination: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
        },
        features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
        feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
        semantics: "synthetic unit-test ancestry; no evidence claim".into(),
    });
    let checkpoint_bytes = serde_json::to_vec(&saved).unwrap();
    let request = RegistryTransferRequestV1 {
        source_checkpoint_sha256: sha(&checkpoint_bytes),
        source_registry_sha256: sha(REGISTRY),
        source_state_sha256: saved.state_sha256.clone(),
        source_adam_step: saved.adam_step,
        source_card_db_hash: saved.card_db_hash.clone(),
        destination_registry_sha256: sha(REGISTRY),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        features: RegistryTransferFeaturesV1::current_v1(),
        initialization_seed: 41,
    };
    let transfer =
        transfer_expanded_checkpoint_to_current_registry_v1(&checkpoint_bytes, REGISTRY, &request)
            .unwrap();
    let (template, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1, 0]);
    let batches: Vec<_> = (0..2)
        .map(|index| {
            let mut episode = template.episode.clone();
            episode.id = format!("transfer-batch-{index}");
            vec![episode]
        })
        .collect();
    let schedule = ExpandedRegistryTransferScheduleV1 {
        schema: SCHEDULE_SCHEMA.into(),
        initial_adam_step: saved.adam_step,
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
        descriptor,
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
fn phase1_registry_trainer_real_update_reloads_exact_adam_and_transfer_provenance() {
    let f = fixture();
    let (mut policy, mut expected, context) = initialize_with_transfer_context(&f.source).unwrap();
    assert_eq!(expected.adam_step_v1(), 2);
    assert_eq!(
        policy.actual_model_identity_v1().card_db_hash,
        format!("{KERNEL_CARDDB_HASH:016x}")
    );
    assert_eq!(
        policy.actual_model_identity_v1().model_parameter_sha256,
        expected.model_v1().parameter_manifest_sha256_v1()
    );
    assert_eq!(
        bits(policy.embedding_rows_v1()),
        expected.snapshot_v1().unwrap().parameters[0]
            .values
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        policy.identity_v1().as_imported_v1().unwrap().schema,
        "mtg-kernel-registry-transferred-play/v1"
    );
    assert!(policy
        .identity_v1()
        .as_imported_v1()
        .unwrap()
        .appended_rows
        .contains(&f.descriptor.transfer_envelope.sha256));
    let first = trajectory(&mut policy, &expected, &f.schedule.batches[0][0]);
    let trajectory_pin = pin_json(&f.root, "batch-0.json", &first);
    apply(&mut expected, &policy, &first);
    let result = execute_v1(ExpandedTrainingCommandV1::Update {
        source: f.source.clone(),
        trajectories: vec![trajectory_pin],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        output_directory: f.root.join("update-0"),
    })
    .unwrap();
    let checkpoint: PinnedFileV1 = serde_json::from_value(result["checkpoint"].clone()).unwrap();
    let saved: ExpandedCheckpointV1 = read_pinned(&checkpoint).unwrap();
    assert_eq!(saved.schema, CHECKPOINT_SCHEMA_TRANSFER);
    assert_eq!(
        saved.registry_transfer,
        Some(context.unwrap().after_update(3).unwrap())
    );
    let source = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint),
        ..f.source.clone()
    };
    let (mut resumed_policy, mut resumed, next) =
        initialize_with_transfer_context(&source).unwrap();
    assert_eq!(
        expected.snapshot_v1().unwrap(),
        resumed.snapshot_v1().unwrap()
    );
    assert_eq!(saved.state_sha256, hex(&resumed.state_sha256_v1().unwrap()));
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
    apply(&mut resumed, &resumed_policy, &second);
    let second_result = execute_v1(ExpandedTrainingCommandV1::Update {
        source: source.clone(),
        trajectories: vec![pin],
        learning_rate: LR,
        value_coefficient: VC,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        output_directory: f.root.join("update-1"),
    })
    .unwrap();
    let last_pin: PinnedFileV1 =
        serde_json::from_value(second_result["checkpoint"].clone()).unwrap();
    let (last_policy, last, exhausted) = initialize_with_transfer_context(&ExpandedModelSourceV1 {
        checkpoint: Some(last_pin),
        ..source
    })
    .unwrap();
    assert_eq!(last.snapshot_v1().unwrap(), resumed.snapshot_v1().unwrap());
    assert_eq!(last.adam_step_v1(), 4);
    assert_eq!(
        last_policy
            .actual_model_identity_v1()
            .model_parameter_sha256,
        last.model_v1().parameter_manifest_sha256_v1()
    );
    assert!(exhausted
        .unwrap()
        .validate_batch(&f.schedule.batches[1])
        .unwrap_err()
        .contains("exhausted"));
}

#[test]
fn phase1_registry_trainer_rejects_scalars_and_serial_parallel_schedule_before_publication() {
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
        output_directory: output.clone(),
    })
    .unwrap_err();
    assert!(error.contains("scalar settings"));
    assert!(!output.exists());
    let mut wrong = f.schedule.batches[0].clone();
    wrong[0].seed += 1;
    for parallel in [false, true] {
        let output = f.root.join(if parallel {
            "wrong-parallel"
        } else {
            "wrong-serial"
        });
        let command = if parallel {
            ExpandedTrainingCommandV1::CollectParallel {
                source: f.source.clone(),
                episodes: wrong.clone(),
                workers: 2,
                output_directory: output.clone(),
            }
        } else {
            ExpandedTrainingCommandV1::Collect {
                source: f.source.clone(),
                episodes: wrong.clone(),
                output_directory: output.clone(),
            }
        };
        assert!(execute_v1(command).unwrap_err().contains("pinned schedule"));
        assert!(!output.exists());
    }
}

#[test]
fn phase1_registry_trainer_rejects_tampered_cursor_provenance_and_feature_bindings() {
    let f = fixture();
    let (mut policy, mut state, context) = initialize_with_transfer_context(&f.source).unwrap();
    let first = trajectory(&mut policy, &state, &f.schedule.batches[0][0]);
    apply(&mut state, &policy, &first);
    let (_, _, mut saved) = checkpoint_fixture_v1();
    let snapshot = state.snapshot_v1().unwrap();
    saved.schema = CHECKPOINT_SCHEMA_TRANSFER.into();
    saved.source_import = policy.identity_v1().clone();
    saved.parameters = snapshot
        .parameters
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.first_moments = snapshot
        .first_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.second_moments = snapshot
        .second_moments
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    saved.adam_step = snapshot.adam_step;
    saved.state_sha256 = hex(&snapshot.state_sha256_v1().unwrap());
    saved.learning_rate_bits = LR.to_bits();
    saved.value_coefficient_bits = VC.to_bits();
    saved.registry_transfer = Some(context.unwrap().after_update(snapshot.adam_step).unwrap());
    for name in ["cursor", "schedule", "scalars", "feature", "missing"] {
        let mut changed = saved.clone();
        match name {
            "cursor" => {
                changed
                    .registry_transfer
                    .as_mut()
                    .unwrap()
                    .completed_updates = 2
            }
            "schedule" => {
                changed
                    .registry_transfer
                    .as_mut()
                    .unwrap()
                    .continuation_schedule_sha256 = "0".repeat(64)
            }
            "scalars" => changed.learning_rate_bits = (LR * 2.0).to_bits(),
            "feature" => changed.feature_encoding_digest = "0".repeat(64),
            _ => changed.registry_transfer = None,
        }
        let pin = pin_json(&f.root, &format!("bad-{name}.json"), &changed);
        assert!(
            super::super::initialize(&ExpandedModelSourceV1 {
                checkpoint: Some(pin),
                ..f.source.clone()
            })
            .is_err(),
            "{name}"
        );
    }
    assert!(
        restore_checkpoint_state_v1(&saved, &mut policy, state.model_v1().clone()).is_err(),
        "ordinary checkpoint loader must reject the transferred schema"
    );
}

#[test]
fn phase1_registry_trainer_rejects_dynamic_schedule_and_preserves_legacy_shapes() {
    let f = fixture();
    let mut schedule = serde_json::to_value(&f.schedule).unwrap();
    schedule["batches"][0][0]["opponent"] = json!({"kind":"completed_iteration","index":0});
    let mut descriptor = f.descriptor.clone();
    descriptor.continuation_schedule = pin_json(&f.root, "dynamic-schedule.json", &schedule);
    let source = ExpandedModelSourceV1 {
        play_import: pin_json(&f.root, "dynamic-source.json", &descriptor),
        ..f.source.clone()
    };
    let error = super::super::initialize(&source).err().unwrap();
    assert!(error.contains("dynamic historical references"), "{error}");
    let value = serde_json::to_value(&f.source).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert!(value["checkpoint"].is_null());
    assert!(value.get("registry_transfer").is_none());
    let (mut legacy_policy, model, saved) = checkpoint_fixture_v1();
    let bytes = serde_json::to_vec(&saved).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["schema"], CHECKPOINT_SCHEMA);
    assert!(value.get("registry_transfer").is_none());
    let parsed: ExpandedCheckpointV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&parsed).unwrap());
    assert!(restore_checkpoint_state_v1(&parsed, &mut legacy_policy, model).is_ok());
    let mut null_metadata = value;
    null_metadata["registry_transfer"] = Value::Null;
    let pin = pin_json(&f.root, "legacy-with-null-transfer.json", &null_metadata);
    assert!(
        read_legacy_checkpoint_v1(&pin).is_err(),
        "legacy unknown-field rejection must include null metadata"
    );
}
