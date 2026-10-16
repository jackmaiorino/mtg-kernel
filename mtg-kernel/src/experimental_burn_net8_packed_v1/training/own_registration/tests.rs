use super::*;
use crate::native_flat_tensorizer_v4::{encoded_decision_view_v4, NativeFlatDecisionTensorV4};
use serde_json::{json, Value};

#[test]
fn registration_training_step_rejects_valid_context_from_another_episode_seat_or_deck() {
    use crate::own_registration_v1::{KnownOwnMainboardV1, RegistrationExpectationV1};
    use crate::rl::PlayerSeatV1;
    let decks = ["Wildfire", "Burn"].map(|name| {
        crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(name)
            .unwrap()
            .registered_configuration()
            .clone()
    });
    let expected = RegistrationExpectationV1 {
        external_episode_id: "expected-episode",
        actor: PlayerSeatV1::P0,
        selected_mainboard_sha256: decks[0].mainboard_sha256_v1(),
    };
    let tensor = NativeFlatDecisionTensorV4::default();
    for mode in [RegistrationModeV1::Own, RegistrationModeV1::Mean] {
        for (episode, actor, deck, accepted) in [
            ("expected-episode", PlayerSeatV1::P0, &decks[0], true),
            ("other-episode", PlayerSeatV1::P0, &decks[0], false),
            ("expected-episode", PlayerSeatV1::P1, &decks[0], false),
            ("expected-episode", PlayerSeatV1::P0, &decks[1], false),
        ] {
            let rows = KnownOwnMainboardV1::new(episode, actor, deck)
                .unwrap()
                .rows(mode)
                .unwrap();
            rows.validate().unwrap();
            let step = RegistrationTrainingStep {
                view: encoded_decision_view_v4(&tensor),
                auxiliary: &rows,
                expected,
                selected: 0,
                expected_logits: &[0],
                expected_value: 0,
            };
            assert_eq!(step.validate_context(mode).is_ok(), accepted);
        }
    }
}

fn legacy_state() -> NativePolicyValueTrainSnapshotV1 {
    let state = NativePolicyValueTrainStateV1::new_v1(
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap(),
    )
    .unwrap();
    patterned_snapshot_v1(&state).unwrap()
}

#[test]
fn registration_projection_rejects_shape_nan_negative_moments_and_age() {
    for mode in [RegistrationModeV1::Own, RegistrationModeV1::Mean] {
        let good = ProjectionSnapshot::zero(mode).unwrap();
        assert_eq!(good.weights.len(), 64 * 192);
        assert_eq!(good.adam_step, 0);
        assert!(good.projection_enabled);
        good.validate().unwrap();
        let mut bad = good.clone();
        bad.weights.pop();
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.first[0] = f32::NAN.to_bits();
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.second[0] = (-0.01f32).to_bits();
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.weights[0] = f32::INFINITY.to_bits();
        assert!(bad.validate().is_err());
        let mut bad = good;
        bad.adam_step = u64::MAX;
        assert!(bad.validate().is_err());
    }
    let baseline = ProjectionSnapshot::baseline().unwrap();
    baseline.validate().unwrap();
    let mut bad = baseline.clone();
    bad.weights[0] = 1.0f32.to_bits();
    assert!(bad.validate().is_err());
    let mut bad = baseline.clone();
    bad.adam_step = 1;
    assert!(bad.validate().is_err());
    let mut bad = baseline;
    bad.mode = RegistrationModeV1::Mean;
    assert!(bad.validate().is_err());
}

#[test]
fn registration_snapshot_roundtrips_all_inherited_bits_and_both_ages() {
    let legacy = legacy_state();
    for mut projection in [
        ProjectionSnapshot::zero(RegistrationModeV1::Own).unwrap(),
        ProjectionSnapshot::zero(RegistrationModeV1::Mean).unwrap(),
        ProjectionSnapshot::baseline().unwrap(),
    ] {
        if projection.projection_enabled {
            projection.weights[19] = (-0.25f32).to_bits();
            projection.first[19] = 0.0001f32.to_bits();
            projection.second[19] = 0.00001f32.to_bits();
            projection.adam_step = 3;
        }
        let bytes = snapshot::encode(&legacy, &projection).unwrap();
        let restored = snapshot::decode(&bytes).unwrap();
        assert_eq!(restored.0, legacy);
        assert_eq!(restored.1, projection);
        assert_eq!(bytes, snapshot::encode(&restored.0, &restored.1).unwrap());
        assert!(super::super::public_inputs::snapshot::decode(&bytes).is_err());
        assert!(super::super::stack_inputs::snapshot::decode(&bytes).is_err());
    }
}

#[test]
fn registration_snapshot_rejects_identity_layout_legacy_and_projection_tampering() {
    let bytes = snapshot::encode(
        &legacy_state(),
        &ProjectionSnapshot::zero(RegistrationModeV1::Own).unwrap(),
    )
    .unwrap();
    for (pointer, value) in [
        ("/schema", json!("legacy")),
        ("/optimizer_identity", json!("wrong")),
        ("/parameter_tensor_count", json!(33)),
        ("/feature_contract", json!("wrong")),
        ("/feature_encoding", json!("wrong")),
        ("/registry", json!("wrong")),
        ("/auxiliary_contract", json!("wrong")),
        ("/legacy_adam_step", json!(1)),
        ("/scorer_bias_anchor_bits", json!(f32::NAN.to_bits())),
        ("/parameters/1/name", json!("wrong")),
        ("/parameters/1/shape/0", json!(1)),
        ("/first_moments/1/values/0", json!(1)),
        ("/second_moments/1/values/0", json!((-1f32).to_bits())),
        ("/projection/mode", json!("mean")),
        ("/projection/registry_sha256", json!("wrong")),
        ("/projection/roster_sha256", json!("wrong")),
        ("/projection/mean_sha256", json!("wrong")),
        ("/projection/feature_identity/normalization", json!("wrong")),
        ("/projection/feature_identity/width", json!(191)),
        ("/projection/mean_bits/0", json!(1)),
        ("/projection/weights/0", json!(1)),
        ("/projection/first/0", json!(1)),
        ("/projection/second/0", json!(1)),
        ("/projection/adam_step", json!(1)),
    ] {
        let mut saved: Value = serde_json::from_slice(&bytes).unwrap();
        *saved.pointer_mut(pointer).unwrap() = value;
        assert!(
            snapshot::decode(&serde_json::to_vec(&saved).unwrap()).is_err(),
            "accepted {pointer}"
        );
    }
    let mut saved: Value = serde_json::from_slice(&bytes).unwrap();
    saved["unknown"] = json!(true);
    assert!(snapshot::decode(&serde_json::to_vec(&saved).unwrap()).is_err());
}

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

fn close(actual: &[f32], expected: &[f32], absolute: f32, relative: f32) {
    assert_eq!(actual.len(), expected.len());
    for (&a, &b) in actual.iter().zip(expected) {
        assert!(
            a.is_finite() && b.is_finite() && (a - b).abs() <= absolute + relative * b.abs(),
            "{a} vs {b}"
        );
    }
}
fn close_named(
    actual: &[NativeNamedParameterV1],
    expected: &[NativeNamedParameterV1],
    absolute: f32,
    relative: f32,
) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.shape, b.shape);
        close(&a.values, &b.values, absolute, relative);
    }
}

/// CPU references bind the exact inherited parameters, registration weights,
/// mode and independently supplied expected episode/actor/selected deck.
fn cpu_packet_outputs(
    legacy: &NativePolicyValueTrainSnapshotV1,
    projection: &ProjectionSnapshot,
    views: &[NativeEncodedDecisionViewV1<'_>],
    rows: &[RegistrationFeatureRowsV1],
    expected: &[crate::own_registration_v1::RegistrationExpectationV1<'_>],
) -> Vec<(Vec<u32>, u32)> {
    use crate::native_policy_value_net_v1::own_registration::NativeRegistrationNetV1;
    let mut base =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    base.replace_parameter_snapshot_v1(&legacy.parameters)
        .unwrap();
    let model = if projection.projection_enabled {
        NativeRegistrationNetV1::new(base, projection.input_weights().unwrap(), projection.mode)
    } else {
        NativeRegistrationNetV1::new_legacy(base)
    }
    .unwrap();
    views
        .iter()
        .zip(rows)
        .zip(expected)
        .map(|((view, rows), expected)| {
            let output = model.forward_rows(*view, rows, *expected).unwrap();
            (
                output.logits.iter().map(|v| v.to_bits()).collect(),
                output.value.to_bits(),
            )
        })
        .collect()
}

fn packet_groups<'a>(
    steps: &'a [RegistrationTrainingStep<'a>],
    first_substeps: &[usize],
) -> Vec<RegistrationTrainingGroup<'a>> {
    first_substeps
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = first_substeps
                .get(index + 1)
                .copied()
                .unwrap_or(steps.len());
            RegistrationTrainingGroup {
                substeps: &steps[start..end],
            }
        })
        .collect()
}

/// Future guarded qualification input: manifest pins one exact V4 parent and a
/// saved batch containing native V4 rows, own registration rows, selected
/// indices, group indices/first substeps, targets and advantages. This test
/// never manufactures games or targets and is never run by ordinary tests.
/// Packet schema v2 also pins independent contexts and exact parent CPU outputs.
#[test]
#[ignore = "requires parent-approved saved V4 batch and idle GPU 1; no training authorization"]
fn registration_cuda_zero_w_inherited_gradient_update_and_resume_parity() {
    let manifest_path = std::env::var("MTG_OWN_REGISTRATION_GPU_MANIFEST").unwrap();
    let manifest: Value = serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["schema"], "own-registration-cuda-correctness/v1");
    assert_eq!(manifest["gpu_ordinal"], 1);
    let legacy = source_snapshot(&read_verified(
        manifest["checkpoint"].as_str().unwrap(),
        manifest["checkpoint_sha256"].as_str().unwrap(),
    ));
    let packet = read_verified(
        manifest["batch"].as_str().unwrap(),
        manifest["batch_sha256"].as_str().unwrap(),
    );
    assert_eq!(packet["schema"], "own-registration-cuda-saved-batch/v2");
    let samples = packet["samples"].as_array().unwrap();
    assert!(!samples.is_empty() && samples.len() <= 512);
    // Never derive an expectation from the auxiliary row being checked.
    let contexts: Vec<_> = samples
        .iter()
        .map(|sample| {
            let expected = &sample["expected"];
            let episode = expected["external_episode_id"].as_str().unwrap().to_owned();
            assert!(!episode.trim().is_empty());
            let actor = match expected["actor"].as_u64().unwrap() {
                0 => crate::rl::PlayerSeatV1::P0,
                1 => crate::rl::PlayerSeatV1::P1,
                other => panic!("invalid expected physical actor {other}"),
            };
            let selected = crate::sideboard::DeckConfigurationV1::new_exact_v1(
                serde_json::from_value(expected["selected_mainboard"].clone()).unwrap(),
                serde_json::from_value(expected["selected_sideboard"].clone()).unwrap(),
            )
            .unwrap();
            (episode, actor, selected.mainboard_sha256_v1())
        })
        .collect();
    let expected: Vec<_> = contexts
        .iter()
        .map(
            |(episode, actor, digest)| crate::own_registration_v1::RegistrationExpectationV1 {
                external_episode_id: episode,
                actor: *actor,
                selected_mainboard_sha256: *digest,
            },
        )
        .collect();
    let zero_cpu_logits: Vec<Vec<u32>> = samples
        .iter()
        .map(|sample| serde_json::from_value(sample["cpu_logits_bits"].clone()).unwrap())
        .collect();
    let zero_cpu_values: Vec<u32> = samples
        .iter()
        .map(|sample| serde_json::from_value(sample["cpu_value_bits"].clone()).unwrap())
        .collect();
    let tensors: Vec<_> = samples.iter().map(|s| tensor(&s["native"])).collect();
    let views: Vec<_> = tensors.iter().map(encoded_decision_view_v4).collect();
    let own_rows: Vec<RegistrationFeatureRowsV1> = samples
        .iter()
        .map(|s| serde_json::from_value(s["registration"].clone()).unwrap())
        .collect();
    assert!(own_rows
        .iter()
        .all(|row| row.mode == RegistrationModeV1::Own));
    let selected: Vec<usize> = serde_json::from_value(packet["selected"].clone()).unwrap();
    let group_indices: Vec<usize> =
        serde_json::from_value(packet["group_indices"].clone()).unwrap();
    let first_substeps: Vec<usize> =
        serde_json::from_value(packet["first_substeps"].clone()).unwrap();
    let targets: Vec<f32> = serde_json::from_value(packet["targets"].clone()).unwrap();
    let advantages: Vec<f32> = serde_json::from_value(packet["advantages"].clone()).unwrap();
    assert_eq!(selected.len(), samples.len());
    assert_eq!(group_indices.len(), samples.len());
    assert_eq!(targets.len(), advantages.len());
    assert_eq!(targets.len(), first_substeps.len());
    assert!(!first_substeps.is_empty());
    assert_eq!(first_substeps[0], 0);
    let max_chunk_substeps = packet["max_chunk_substeps"].as_u64().unwrap() as usize;
    assert!((1..=512).contains(&max_chunk_substeps));
    for (index, &start) in first_substeps.iter().enumerate() {
        let end = first_substeps
            .get(index + 1)
            .copied()
            .unwrap_or(samples.len());
        assert!(start < end && end <= samples.len() && end - start <= max_chunk_substeps);
        assert!(group_indices[start..end]
            .iter()
            .all(|group| *group == index));
        assert!(contexts[start..end]
            .iter()
            .all(|context| context == &contexts[start]));
    }
    for (rows, expected) in own_rows.iter().zip(&expected) {
        rows.validate_expected(RegistrationModeV1::Own, *expected)
            .unwrap();
    }
    let mut host = HostPackingWorkspace::default();
    host.pack_views(&views).unwrap();
    let device = burn_cuda::CudaDevice::new(1);
    let batch =
        DevicePackedBatch::<CudaAutodiffBackendV1>::upload_feature_transfer_v3(&device, &host);
    let plan = build_dense_group_loss_plan_gae_v1(
        &host,
        &selected,
        &group_indices,
        &first_substeps,
        &targets,
        &advantages,
        &device,
    )
    .unwrap();
    let denominator = targets.len() as f32;
    let reference = ExperimentalDeviceTrainStateV1::import_snapshot_v1(&legacy, &device).unwrap();
    let (logits, values) = reference.model.forward_core::<FrozenNet8DimsV1>(&batch);
    let expected_logits = logits.clone().inner().into_data().to_vec::<f32>().unwrap();
    let expected_values = values.clone().inner().into_data().to_vec::<f32>().unwrap();
    let loss = dense_group_loss_gae_v1(logits, values, &plan, 0.5, denominator).unwrap();
    let mut reference_gradients = GradientsParams::from_grads(loss.backward(), &reference.model);
    if batch.empty_relations_v3 {
        register_empty_relation_gradients_v3(&reference.model, &batch, &mut reference_gradients)
            .unwrap();
    }
    let embedding_id = reference.model.card_embedding.weight.id;
    let embedding = reference_gradients
        .remove::<CudaBackendV1, 2>(embedding_id)
        .unwrap();
    reference_gradients.register(
        embedding_id,
        embedding.slice_assign(
            [0..1, 0..CARD_EMBEDDING_DIM_V1],
            Tensor::zeros([1, CARD_EMBEDDING_DIM_V1], &device),
        ),
    );
    let gauge_id = reference.model.scorer.output.bias.as_ref().unwrap().id;
    let gauge = reference_gradients
        .remove::<CudaBackendV1, 1>(gauge_id)
        .unwrap();
    reference_gradients.register(gauge_id, gauge.zeros_like());
    let expected_gradients =
        export_named_state_v1(&reference.model, &reference_gradients, &legacy.parameters).unwrap();
    let mut expected_accumulator = burn::optim::GradientsAccumulator::default();
    expected_accumulator.accumulate(&reference.model, reference_gradients);
    let mut reference = reference;
    reference
        .apply_accumulated_v1(expected_accumulator, 0.0001)
        .unwrap();
    let expected_updated = reference.export_snapshot_v1().unwrap();
    for arm in [
        Some(RegistrationModeV1::Own),
        Some(RegistrationModeV1::Mean),
        None,
    ] {
        let initial = arm
            .map(|mode| ProjectionSnapshot::zero(mode).unwrap())
            .unwrap_or_else(|| ProjectionSnapshot::baseline().unwrap());
        let mut state = RegistrationDeviceTrainState::import(&legacy, &initial, &device).unwrap();
        let restored = state.snapshot().unwrap();
        assert_eq!(restored.0, legacy);
        assert_eq!(restored.1, initial);
        let mut rows = own_rows.clone();
        if arm == Some(RegistrationModeV1::Mean) {
            for row in &mut rows {
                row.mode = RegistrationModeV1::Mean;
                row.features = fixed_mean_features_v1().unwrap();
            }
        }
        let zero_cpu = cpu_packet_outputs(&legacy, &initial, &views, &rows, &expected);
        for (index, (logits, value)) in zero_cpu.iter().enumerate() {
            assert_eq!(
                *logits, zero_cpu_logits[index],
                "packet parent CPU logits differ"
            );
            assert_eq!(
                *value, zero_cpu_values[index],
                "packet parent CPU value differs"
            );
        }
        let mut accumulated = RegistrationGradientAccumulator::default();
        let outputs = state
            .chunk_backward_gae(
                &mut accumulated,
                &batch,
                &rows,
                &plan,
                0.5,
                denominator,
                0.0,
            )
            .unwrap();
        close(&outputs.logit_outputs, &expected_logits, 1e-3, 1e-3);
        close(&outputs.value_outputs, &expected_values, 1e-3, 1e-3);
        if arm.is_some() {
            let gradient = accumulated
                .projection
                .as_ref()
                .unwrap()
                .clone()
                .into_data()
                .to_vec::<f32>()
                .unwrap();
            assert!(gradient.iter().all(|v| v.is_finite()));
            assert!(
                gradient.iter().any(|v| *v != 0.0),
                "zero W lost its autograd path"
            );
        } else {
            assert!(accumulated.projection.is_none());
        }
        let mut gradients = accumulated.base.grads();
        let gauge = gradients
            .remove::<CudaBackendV1, 1>(state.legacy.model.scorer.output.bias.as_ref().unwrap().id)
            .unwrap();
        gradients.register(
            state.legacy.model.scorer.output.bias.as_ref().unwrap().id,
            gauge.zeros_like(),
        );
        let inherited =
            export_named_state_v1(&state.legacy.model, &gradients, &legacy.parameters).unwrap();
        close_named(&inherited, &expected_gradients, 5e-3, 5e-3);
        // Gradient inspection above stays at the internals; functional updates
        // enter through the production bridge, including bound contexts and
        // the stored CPU-output transport check. One first chunk preserves
        // direct inherited-update comparison; the next step tests chunking.
        let zero_steps: Vec<_> = (0..samples.len())
            .map(|index| RegistrationTrainingStep {
                view: views[index],
                auxiliary: &rows[index],
                expected: expected[index],
                selected: selected[index],
                expected_logits: &zero_cpu_logits[index],
                expected_value: zero_cpu_values[index],
            })
            .collect();
        let zero_groups = packet_groups(&zero_steps, &first_substeps);
        state
            .update_groups(
                &zero_groups,
                &targets,
                &advantages,
                0.0001,
                0.5,
                0.0,
                samples.len(),
            )
            .unwrap();
        let after = state.snapshot().unwrap();
        close_named(
            &after.0.parameters,
            &expected_updated.parameters,
            2e-3,
            2e-3,
        );
        close_named(
            &after.0.first_moments,
            &expected_updated.first_moments,
            5e-3,
            5e-3,
        );
        close_named(
            &after.0.second_moments,
            &expected_updated.second_moments,
            5e-3,
            5e-3,
        );
        assert_eq!(after.0.adam_step, legacy.adam_step + 1);
        assert_eq!(
            after.0.scorer_bias_anchor_bits,
            legacy.scorer_bias_anchor_bits
        );
        if arm.is_none() {
            assert_eq!(
                after.0, expected_updated,
                "B differs from ordinary legacy update"
            );
            assert_eq!(after.1, initial);
        } else {
            assert_eq!(after.1.adam_step, 1);
            assert!(after.1.weights.iter().any(|v| *v != 0));
        }
        // Learned nonzero W is checked against the CPU successor before its
        // next update. Baseline takes this same path with the projection off.
        let learned_cpu = cpu_packet_outputs(&after.0, &after.1, &views, &rows, &expected);
        let uploaded = RegistrationBatch::upload(&rows, &batch, state.mode).unwrap();
        let model = RegistrationModel {
            base: state.legacy.model.clone(),
            projection: state.projection.clone(),
            enabled: state.enabled,
        };
        let (learned_logits, learned_values) = model.forward(&batch, &uploaded);
        let learned_logits = learned_logits.inner().into_data().to_vec::<f32>().unwrap();
        let learned_values = learned_values.inner().into_data().to_vec::<f32>().unwrap();
        let cpu_logits: Vec<_> = learned_cpu
            .iter()
            .flat_map(|(logits, _)| logits.iter().map(|bits| f32::from_bits(*bits)))
            .collect();
        let cpu_values: Vec<_> = learned_cpu
            .iter()
            .map(|(_, value)| f32::from_bits(*value))
            .collect();
        close(&learned_logits, &cpu_logits, 1e-3, 1e-3);
        close(&learned_values, &cpu_values, 1e-3, 1e-3);
        let learned_steps: Vec<_> = (0..samples.len())
            .map(|index| RegistrationTrainingStep {
                view: views[index],
                auxiliary: &rows[index],
                expected: expected[index],
                selected: selected[index],
                expected_logits: &learned_cpu[index].0,
                expected_value: learned_cpu[index].1,
            })
            .collect();
        let learned_groups = packet_groups(&learned_steps, &first_substeps);
        let bytes = snapshot::encode(&after.0, &after.1).unwrap();
        let decoded = snapshot::decode(&bytes).unwrap();
        let resumed = RegistrationDeviceTrainState::import(&decoded.0, &decoded.1, &device)
            .unwrap()
            .snapshot()
            .unwrap();
        assert_eq!(bytes, snapshot::encode(&resumed.0, &resumed.1).unwrap());
        let mut resumed_state =
            RegistrationDeviceTrainState::import(&decoded.0, &decoded.1, &device).unwrap();
        for candidate in [&mut state, &mut resumed_state] {
            candidate
                .update_groups(
                    &learned_groups,
                    &targets,
                    &advantages,
                    0.0001,
                    0.5,
                    0.0,
                    max_chunk_substeps,
                )
                .unwrap();
        }
        let uninterrupted = state.snapshot().unwrap();
        let resumed = resumed_state.snapshot().unwrap();
        assert_eq!(
            snapshot::encode(&uninterrupted.0, &uninterrupted.1).unwrap(),
            snapshot::encode(&resumed.0, &resumed.1).unwrap(),
            "resume changed the next update"
        );
    }
}
