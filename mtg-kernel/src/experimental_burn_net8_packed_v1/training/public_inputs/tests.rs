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
fn public_projection_state_rejects_invalid_adam() {
    let mut state = ProjectionSnapshot::zero();
    state.object_second[0] = (-0.01f32).to_bits();
    assert!(state.validate().is_err());
    state = ProjectionSnapshot::zero();
    state.state[0] = f32::INFINITY.to_bits();
    assert!(state.validate().is_err());
    state = ProjectionSnapshot::zero();
    state.adam_step = u64::MAX;
    assert!(state.validate().is_err());
}

#[test]
fn public_optimizer_snapshot_rejects_contract_and_state_corruption() {
    let legacy = NativePolicyValueTrainStateV1::new_v1(
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap(),
    )
    .unwrap()
    .snapshot_v1()
    .unwrap();
    let bytes = snapshot::encode(&legacy, &ProjectionSnapshot::zero()).unwrap();
    let decoded = snapshot::decode(&bytes).unwrap();
    assert_eq!(bytes, snapshot::encode(&decoded.0, &decoded.1).unwrap());
    for (path, value) in [
        ("auxiliary_contract", json!("wrong")),
        ("legacy_adam_step", json!(1)),
    ] {
        let mut saved: Value = serde_json::from_slice(&bytes).unwrap();
        saved[path] = value;
        assert!(snapshot::decode(&serde_json::to_vec(&saved).unwrap()).is_err());
    }
    let mut saved: Value = serde_json::from_slice(&bytes).unwrap();
    saved["public"]["state_second"][0] = json!((-1f32).to_bits());
    assert!(snapshot::decode(&serde_json::to_vec(&saved).unwrap()).is_err());
}

#[test]
#[ignore = "requires explicitly pinned g115 qualification manifest and idle GPU 1"]
fn public_cuda_g115_update_and_fresh_process_resume() {
    let root = std::path::PathBuf::from(std::env::var("MTG_PUBLIC_CUDA_ROOT").unwrap());
    let manifest_bytes = std::fs::read(root.join("manifest.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["schema"], "public-input-cuda-engineering/v1");
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
    let samples = read_verified(
        manifest["samples"].as_str().unwrap(),
        manifest["samples_sha256"].as_str().unwrap(),
    );
    let samples = samples["samples"].as_array().unwrap();
    let tensors: Vec<_> = samples.iter().map(|s| tensor(&s["native"])).collect();
    let views: Vec<_> = tensors.iter().map(encoded_decision_view_v4).collect();
    let rows: Vec<PublicFeatureRowsV1> = samples
        .iter()
        .map(|s| serde_json::from_value(s["public"].clone()).unwrap())
        .collect();
    let mut host = HostPackingWorkspace::default();
    host.pack_views(&views).unwrap();
    let device = burn_cuda::CudaDevice::new(1);
    let batch =
        DevicePackedBatch::<CudaAutodiffBackendV1>::upload_feature_transfer_v3(&device, &host);
    let indices: Vec<usize> = serde_json::from_value(manifest["selected_indices"].clone()).unwrap();
    let targets: Vec<f32> = serde_json::from_value(manifest["targets"].clone()).unwrap();
    let advantages: Vec<f32> = serde_json::from_value(manifest["advantages"].clone()).unwrap();
    let groups: Vec<_> = (0..samples.len()).collect();
    let plan = build_dense_group_loss_plan_gae_v1(
        &host,
        &indices,
        &groups,
        &groups,
        &targets,
        &advantages,
        &device,
    )
    .unwrap();
    let resume = std::env::var("MTG_PUBLIC_CUDA_RESUME").is_ok();
    let (initial, public) = if resume {
        snapshot::decode(&std::fs::read(root.join("step1.json")).unwrap()).unwrap()
    } else {
        (legacy.clone(), ProjectionSnapshot::zero())
    };
    let mut state = PublicDeviceTrainState::import(&initial, &public, &device).unwrap();
    let imported = state.snapshot().unwrap();
    assert_eq!(
        snapshot::encode(&initial, &public).unwrap(),
        snapshot::encode(&imported.0, &imported.1).unwrap(),
        "GPU import altered parameters/moments/ages"
    );
    let started = Instant::now();
    for step in if resume { vec![2] } else { vec![1, 2] } {
        let mut accumulator = PublicGradientAccumulator::default();
        let output = state
            .chunk_backward_gae(
                &mut accumulator,
                &batch,
                &rows,
                &plan,
                0.5,
                samples.len() as f32,
                0.0,
            )
            .unwrap();
        if step == 1 {
            let reference = state.legacy.forward_outputs_v1(&batch).unwrap();
            let bits = |row: &[f32]| row.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(
                bits(&reference.0),
                bits(&output.logit_outputs),
                "initial CUDA logits changed"
            );
            assert_eq!(
                bits(&reference.1),
                bits(&output.value_outputs),
                "initial CUDA values changed"
            );
            let mut gradients = accumulator.base.grads();
            let gauge = state.legacy.model.scorer.output.bias.as_ref().unwrap();
            let gauge_gradient = gradients.remove::<CudaBackendV1, 1>(gauge.id).unwrap();
            gradients.register(gauge.id, gauge_gradient.zeros_like());
            let exported =
                export_named_state_v1(&state.legacy.model, &gradients, &legacy.parameters).unwrap();
            let bits = |t: Tensor<CudaBackendV1, 2>| {
                t.into_data()
                    .to_vec::<f32>()
                    .unwrap()
                    .into_iter()
                    .map(f32::to_bits)
                    .collect::<Vec<_>>()
            };
            let object = bits(accumulator.object.as_ref().unwrap().clone());
            let state_gradient = bits(accumulator.state.as_ref().unwrap().clone());
            assert!(object.iter().copied().map(f32::from_bits).any(|v| v != 0.0));
            assert!(state_gradient
                .iter()
                .copied()
                .map(f32::from_bits)
                .any(|v| v != 0.0));
            write(root.join("gradients.json"),&serde_json::to_vec(&json!({"legacy":snapshot::export_gradient_bits(&exported),"object":object,"state":state_gradient,
                "logits":output.logit_outputs,"values":output.value_outputs,"raw_gauge_residual":output.raw_gauge_residual})).unwrap());
            accumulator.base.accumulate(&state.legacy.model, gradients);
        }
        state.apply(accumulator, 0.0001).unwrap();
        let (next, public) = state.snapshot().unwrap();
        assert_eq!(next.adam_step, legacy.adam_step + step);
        assert_eq!(public.adam_step, step);
        assert!(public.object.iter().any(|b| *b != 0));
        assert!(public.state.iter().any(|b| *b != 0));
        let bytes = snapshot::encode(&next, &public).unwrap();
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
                std::fs::read(root.join("step2-uninterrupted.json")).unwrap(),
                "fresh-process CUDA resume changed state"
            );
        }
    }
    if !resume {
        // Deliberate dummy-token numerical probe: force embedding row zero into
        // a live graph, show its raw derivative is nonzero, then verify the
        // public training path suppresses it before updating weights/moments.
        let mut dummy_tensors = tensors.clone();
        let mut dummy_rows = rows.clone();
        for (t, r) in dummy_tensors.iter_mut().zip(&mut dummy_rows) {
            t.common.object_card_ids[0] = 0;
            r.objects[0].fill(0.0);
        }
        let mut host = HostPackingWorkspace::default();
        host.pack_views(
            &dummy_tensors
                .iter()
                .map(encoded_decision_view_v4)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let dummy_batch =
            DevicePackedBatch::<CudaAutodiffBackendV1>::upload_feature_transfer_v3(&device, &host);
        let mut dummy =
            PublicDeviceTrainState::import(&legacy, &ProjectionSnapshot::zero(), &device).unwrap();
        let model = PublicModel {
            base: dummy.legacy.model.clone(),
            public: dummy.public.clone(),
        };
        let auxiliary = PublicBatch::upload(&dummy_rows, &dummy_batch).unwrap();
        let (l, v) = model.forward(&dummy_batch, &auxiliary);
        let loss = dense_group_loss_gae_v1(l, v, &plan, 0.5, samples.len() as f32).unwrap();
        let mut raw = GradientsParams::from_grads(loss.backward(), &model);
        let raw_padding = raw
            .remove::<CudaBackendV1, 2>(model.base.card_embedding.weight.id)
            .unwrap()
            .slice([0..1, 0..CARD_EMBEDDING_DIM_V1])
            .into_data()
            .to_vec::<f32>()
            .unwrap();
        assert!(
            raw_padding.iter().any(|v| *v != 0.0),
            "dummy fixture has no padding derivative"
        );
        let mut accumulator = PublicGradientAccumulator::default();
        dummy
            .chunk_backward_gae(
                &mut accumulator,
                &dummy_batch,
                &dummy_rows,
                &plan,
                0.5,
                samples.len() as f32,
                0.0,
            )
            .unwrap();
        dummy.apply(accumulator, 0.0001).unwrap();
        let after = dummy.snapshot().unwrap().0;
        for tensors in [
            &after.parameters,
            &after.first_moments,
            &after.second_moments,
        ] {
            let row = tensors
                .iter()
                .find(|t| t.name == "card_embedding.weight")
                .unwrap();
            assert!(row.values[..CARD_EMBEDDING_DIM_V1]
                .iter()
                .all(|v| v.to_bits() == 0));
        }
        write(root.join("padding-probe.json"),&serde_json::to_vec(&json!({"raw_padding_gradient":raw_padding,"parameter_and_both_moment_rows_remain_positive_zero":true})).unwrap());
    }
    write(root.join(if resume {"resume-execution.json"} else {"start-execution.json"}),&serde_json::to_vec_pretty(&json!({"status":"CUDA-ENGINEERING-EXECUTION-PASS",
        "gpu_ordinal":1,"manifest_sha256":format!("{:x}",Sha256::digest(&manifest_bytes)),"seconds":started.elapsed().as_secs_f64(),
        "non_claim":"Fixed synthetic GAE targets on actual visible fixtures, not reward-based training or a trained candidate."})).unwrap());
}
