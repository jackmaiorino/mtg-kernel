use super::*;
use crate::native_flat_tensorizer_v4::{encoded_decision_view_v4, NativeFlatDecisionTensorV4};
use serde_json::{json, Value};
use std::path::Path;

fn read_verified(path: &str, sha: &str) -> Value {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), sha);
    serde_json::from_slice(&bytes).unwrap()
}

fn source_snapshot(source: &Value) -> NativePolicyValueTrainSnapshotV1 {
    assert_eq!(
        source["feature_contract_digest"],
        crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
    );
    assert_eq!(
        source["feature_encoding_digest"],
        crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4
    );
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let template = model.parameter_snapshot_v1();
    let unpack = |key: &str| {
        let rows = source[key].as_array().unwrap();
        assert_eq!(rows.len(), template.len());
        rows.iter()
            .zip(&template)
            .map(|(row, t)| {
                assert_eq!(row["name"], t.name);
                assert_eq!(
                    serde_json::from_value::<Vec<usize>>(row["shape"].clone()).unwrap(),
                    t.shape
                );
                NativeNamedParameterV1 {
                    name: t.name,
                    shape: t.shape.clone(),
                    values: serde_json::from_value::<Vec<u32>>(row["values"].clone())
                        .unwrap()
                        .into_iter()
                        .map(f32::from_bits)
                        .collect(),
                }
            })
            .collect()
    };
    let state = NativePolicyValueTrainSnapshotV1 {
        adam_step: source["adam_step"].as_u64().unwrap(),
        scorer_bias_anchor_bits: source["scorer_bias_anchor_bits"].as_u64().unwrap() as u32,
        parameters: unpack("parameters"),
        first_moments: unpack("first_moments"),
        second_moments: unpack("second_moments"),
    };
    assert_eq!(
        state
            .state_sha256_v1()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        source["state_sha256"].as_str().unwrap()
    );
    state
}

fn tensor(row: &Value) -> NativeFlatDecisionTensorV4 {
    let mut result = NativeFlatDecisionTensorV4::default();
    macro_rules! field {
        ($name:ident) => {
            result.common.$name = serde_json::from_value(row[stringify!($name)].clone()).unwrap();
        };
    }
    field!(state);
    field!(object_features);
    field!(object_card_ids);
    field!(object_groups);
    field!(object_node_ids);
    field!(edge_features);
    field!(edge_source_indices);
    field!(edge_target_indices);
    field!(action_features);
    field!(action_ref_features);
    field!(action_ref_card_ids);
    field!(action_ref_action_indices);
    field!(action_ref_node_indices);
    result
}

fn write(path: impl AsRef<Path>, bytes: &[u8]) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

#[test]
fn stack_optimizer_snapshot_rejects_contract_and_projection_corruption() {
    let legacy = NativePolicyValueTrainStateV1::new_v1(
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap(),
    )
    .unwrap()
    .snapshot_v1()
    .unwrap();
    let stack = StackProjectionSnapshot::zero();
    let bytes = snapshot::encode(&legacy, &stack).unwrap();
    let decoded = snapshot::decode(&bytes).unwrap();
    assert_eq!(bytes, snapshot::encode(&decoded.0, &decoded.1).unwrap());
    for key in ["auxiliary_contract", "architecture", "projection_sha256"] {
        let mut bad: Value = serde_json::from_slice(&bytes).unwrap();
        bad[key] = json!("wrong");
        assert!(snapshot::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let mut bad: Value = serde_json::from_slice(&bytes).unwrap();
    bad["stack"]["weight"][0] = json!(1.0f32.to_bits());
    assert!(snapshot::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = stack.clone();
    bad.second[0] = (-0.01f32).to_bits();
    assert!(bad.validate().is_err());
    let mut bad = stack.clone();
    bad.weight[0] = f32::NAN.to_bits();
    assert!(bad.validate().is_err());
    let mut bad = stack;
    bad.adam_step = u64::MAX;
    assert!(bad.validate().is_err());
}

#[test]
#[ignore = "requires pinned stack fixtures and idle GPU1"]
fn stack_cuda_g115_grouped_update_and_fresh_process_resume() {
    let root = std::path::PathBuf::from(std::env::var("MTG_STACK_CUDA_ROOT").unwrap());
    let manifest_bytes = std::fs::read(root.join("manifest.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["schema"], "public-stack-cuda-engineering/v1");
    assert_eq!(manifest["gpu_ordinal"], 1);
    assert_eq!(
        manifest["checkpoint_sha256"],
        "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
    );
    let source = read_verified(
        manifest["checkpoint"].as_str().unwrap(),
        manifest["checkpoint_sha256"].as_str().unwrap(),
    );
    let legacy = source_snapshot(&source);
    let fixture = read_verified(
        manifest["fixture"].as_str().unwrap(),
        manifest["fixture_sha256"].as_str().unwrap(),
    );
    let samples = fixture["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 16);
    let tensors: Vec<_> = samples.iter().map(|s| tensor(&s["native"])).collect();
    let views: Vec<_> = tensors.iter().map(encoded_decision_view_v4).collect();
    let rows: Vec<StackFeatureRowsV1> = samples
        .iter()
        .map(|s| serde_json::from_value(s["stack"].clone()).unwrap())
        .collect();
    let counts: Vec<_> = views.iter().map(|v| v.object_card_ids.len()).collect();
    let mut host = HostPackingWorkspace::default();
    host.pack_views(&views).unwrap();
    let device = burn_cuda::CudaDevice::new(1);
    let batch =
        DevicePackedBatch::<CudaAutodiffBackendV1>::upload_feature_transfer_v3(&device, &host);
    let indices: Vec<usize> = serde_json::from_value(manifest["selected_indices"].clone()).unwrap();
    let targets: Vec<f32> = serde_json::from_value(manifest["targets"].clone()).unwrap();
    let advantages: Vec<f32> = serde_json::from_value(manifest["advantages"].clone()).unwrap();
    let groups: Vec<_> = (0..samples.len()).map(|i| i / 2).collect();
    let first: Vec<_> = (0..samples.len()).step_by(2).collect();
    let plan = build_dense_group_loss_plan_gae_v1(
        &host,
        &indices,
        &groups,
        &first,
        &targets,
        &advantages,
        &device,
    )
    .unwrap();
    let resume = std::env::var("MTG_STACK_CUDA_RESUME").is_ok();
    let (initial, projection) = if resume {
        snapshot::decode(&std::fs::read(root.join("step1.json")).unwrap()).unwrap()
    } else {
        (legacy.clone(), StackProjectionSnapshot::zero())
    };
    let mut state = StackDeviceTrainState::import(&initial, &projection, &device).unwrap();
    let imported = state.snapshot().unwrap();
    assert_eq!(
        snapshot::encode(&initial, &projection).unwrap(),
        snapshot::encode(&imported.0, &imported.1).unwrap()
    );
    let started = Instant::now();
    for step in if resume { vec![2] } else { vec![1, 2] } {
        let mut accumulator = StackGradientAccumulator::default();
        let output = state
            .chunk_backward_gae(
                &mut accumulator,
                &batch,
                &rows,
                &counts,
                &plan,
                0.5,
                first.len() as f32,
                0.0,
            )
            .unwrap();
        if step == 1 {
            let reference = state.legacy.forward_outputs_v1(&batch).unwrap();
            let bits = |r: &[f32]| r.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&reference.0), bits(&output.logit_outputs));
            assert_eq!(bits(&reference.1), bits(&output.value_outputs));
        }
        // Export both zero and nonzero-projection gradients for an independent oracle.
        if !resume {
            let mut gradients = accumulator.base.grads();
            let gauge = state.legacy.model.scorer.output.bias.as_ref().unwrap();
            let raw = gradients.remove::<CudaBackendV1, 1>(gauge.id).unwrap();
            gradients.register(gauge.id, raw.zeros_like());
            let exported =
                export_named_state_v1(&state.legacy.model, &gradients, &legacy.parameters).unwrap();
            let stack = accumulator
                .stack
                .as_ref()
                .unwrap()
                .clone()
                .into_data()
                .to_vec::<f32>()
                .unwrap();
            assert!(stack.iter().all(|v| v.is_finite()));
            assert!(stack.iter().any(|v| *v != 0.0));
            write(root.join(format!("gradients-{step}.json")),&serde_json::to_vec(&json!({"legacy":snapshot::export_gradient_bits(&exported),"stack":stack.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"logits":output.logit_outputs,"values":output.value_outputs,"raw_gauge_residual":output.raw_gauge_residual})).unwrap());
            accumulator.base.accumulate(&state.legacy.model, gradients);
        }
        state.apply(accumulator, 0.0001).unwrap();
        let (next, projection) = state.snapshot().unwrap();
        assert_eq!(next.adam_step, legacy.adam_step + step);
        assert_eq!(projection.adam_step, step);
        assert!(projection.weight.iter().any(|b| *b != 0));
        let bytes = snapshot::encode(&next, &projection).unwrap();
        let decoded = snapshot::decode(&bytes).unwrap();
        assert_eq!(bytes, snapshot::encode(&decoded.0, &decoded.1).unwrap());
        let name = if step == 1 {
            "step1.json"
        } else if resume {
            "step2-resumed.json"
        } else {
            "step2-uninterrupted.json"
        };
        write(root.join(name), &bytes);
        if resume {
            assert_eq!(
                bytes,
                std::fs::read(root.join("step2-uninterrupted.json")).unwrap()
            );
        }
    }
    if !resume {
        // Empty auxiliary/control path must have an explicit zero derivative and
        // no fabricated item. Deliberately does not claim the legacy fixture is empty.
        let mut empty = rows.clone();
        for r in &mut empty {
            r.stack_items = 0;
            r.rows.clear();
        }
        let mut control =
            StackDeviceTrainState::import(&legacy, &StackProjectionSnapshot::zero(), &device)
                .unwrap();
        let mut accumulator = StackGradientAccumulator::default();
        control
            .chunk_backward_gae(
                &mut accumulator,
                &batch,
                &empty,
                &counts,
                &plan,
                0.5,
                first.len() as f32,
                0.0,
            )
            .unwrap();
        let zero = accumulator
            .stack
            .as_ref()
            .unwrap()
            .clone()
            .into_data()
            .to_vec::<f32>()
            .unwrap();
        assert!(zero.iter().all(|v| *v == 0.0));
        control.apply(accumulator, 0.0001).unwrap();
        let snapshot = control.snapshot().unwrap();
        assert!(snapshot
            .1
            .weight
            .iter()
            .chain(&snapshot.1.first)
            .chain(&snapshot.1.second)
            .all(|v| *v == 0));
        // Guard per-decision boundaries even if total object count is unchanged.
        let mut bad_counts = counts.clone();
        bad_counts[0] += 1;
        bad_counts[1] -= 1;
        assert!(StackBatch::upload(&rows, &bad_counts, &batch).is_err());
        let decisions: Vec<_> = tensors
            .iter()
            .cloned()
            .zip(rows.iter().cloned())
            .map(
                |(legacy, stack)| crate::public_stack_features_v1::StackEncodedDecisionV1 {
                    legacy,
                    stack,
                },
            )
            .collect();
        let expected_logits: Vec<Vec<u32>> = samples
            .iter()
            .map(|s| {
                s["zero"]["logits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (v.as_f64().unwrap() as f32).to_bits())
                    .collect()
            })
            .collect();
        let steps: Vec<_> = decisions
            .iter()
            .enumerate()
            .map(|(i, d)| StackTrainingStep {
                decision: d,
                selected: indices[i],
                expected_logits: &expected_logits[i],
                expected_value: (samples[i]["zero"]["value"].as_f64().unwrap() as f32).to_bits(),
            })
            .collect();
        let training_groups: Vec<_> = steps
            .chunks(2)
            .map(|s| StackTrainingGroup { substeps: s })
            .collect();
        let mut chunked =
            StackDeviceTrainState::import(&legacy, &StackProjectionSnapshot::zero(), &device)
                .unwrap();
        chunked
            .update_groups(
                &training_groups,
                &targets,
                &advantages,
                0.0001,
                0.5,
                0.0,
                true,
                4,
            )
            .unwrap();
        let chunked = chunked.snapshot().unwrap();
        write(
            root.join("step1-chunked.json"),
            &snapshot::encode(&chunked.0, &chunked.1).unwrap(),
        );
        write(root.join("control-probe.json"),&serde_json::to_vec(&json!({"empty_stack_gradient_zero":true,"zero_projection_after_control_update":true,"per_decision_boundary_rejection":true,"actual_empty_stack_fixtures":samples.iter().filter(|s|s["scenario"]=="empty").count()})).unwrap());
    }
    write(root.join(if resume {"resume-execution.json"}else{"start-execution.json"}),&serde_json::to_vec_pretty(&json!({"status":"STACK-CUDA-ENGINEERING-EXECUTION-PASS","gpu_ordinal":1,"manifest_sha256":format!("{:x}",Sha256::digest(&manifest_bytes)),"seconds":started.elapsed().as_secs_f64(),"non_claim":"Fixed synthetic GAE targets on actual actor fixtures. No reward-based training, collector integration or strength result."})).unwrap());
}
