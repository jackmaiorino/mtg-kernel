use super::*;
use crate::native_flat_tensorizer_v4::{encoded_decision_view_v4, NativeFlatDecisionTensorV4};
use serde_json::{json, Value};

fn fixed_model() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

#[test]
fn public_weights_reject_malformed_or_nonfinite_tensors() {
    assert!(PublicInputWeightsV1::new(vec![], vec![0.0; 384]).is_err());
    assert!(PublicInputWeightsV1::new(vec![0.0; 2048], vec![]).is_err());
    assert!(PublicInputWeightsV1::new(vec![f32::NAN; 2048], vec![0.0; 384]).is_err());
    assert!(PublicInputWeightsV1::new(vec![0.0; 2048], vec![f32::INFINITY; 384]).is_err());
    assert_eq!(
        NativePublicInputNetV1::new(fixed_model(), PublicInputWeightsV1::zero())
            .unwrap()
            .architecture(),
        ARCHITECTURE
    );
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

fn verified_json(path: &str, sha: &str) -> Value {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), sha);
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
#[ignore = "requires explicit pinned g115 checkpoint and Python fixture via MTG_PUBLIC_NATIVE_PARITY"]
fn public_native_g115_matches_python_and_preserves_hidden_invariance() {
    let path = std::env::var("MTG_PUBLIC_NATIVE_PARITY").unwrap();
    let fixture_bytes = std::fs::read(&path).unwrap();
    let fixture: Value = serde_json::from_slice(&fixture_bytes).unwrap();
    assert_eq!(
        fixture["schema"],
        "public-input-native-forward-qualification/v1"
    );
    assert_eq!(fixture["absolute_tolerance"], 1e-3);
    assert_eq!(fixture["relative_tolerance"], 1e-3);
    assert_eq!(
        fixture["checkpoint_sha256"],
        "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
    );
    let source = verified_json(
        fixture["checkpoint"].as_str().unwrap(),
        fixture["checkpoint_sha256"].as_str().unwrap(),
    );
    assert_eq!(
        source["feature_contract_digest"],
        crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
    );
    assert_eq!(
        source["feature_encoding_digest"],
        crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4
    );
    assert_eq!(source["card_db_hash"], "064a7c989255ab3c");
    let samples = verified_json(
        fixture["samples"].as_str().unwrap(),
        fixture["samples_sha256"].as_str().unwrap(),
    );
    let samples = samples["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 12);
    let mut base = fixed_model();
    let mut parameters = base.parameter_snapshot_v1();
    let raw_parameters = source["parameters"].as_array().unwrap();
    assert_eq!(parameters.len(), raw_parameters.len());
    for (parameter, raw) in parameters.iter_mut().zip(raw_parameters) {
        assert_eq!(raw["name"], parameter.name);
        assert_eq!(
            serde_json::from_value::<Vec<usize>>(raw["shape"].clone()).unwrap(),
            parameter.shape
        );
        parameter.values = serde_json::from_value::<Vec<u32>>(raw["values"].clone())
            .unwrap()
            .into_iter()
            .map(f32::from_bits)
            .collect();
    }
    base.replace_parameter_snapshot_v1(&parameters).unwrap();
    let mut reports = Vec::new();
    let variants = fixture["variants"].as_array().unwrap();
    assert_eq!(
        variants
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["zero", "object", "state", "both"]
    );
    for variant in variants {
        let name = variant["name"].as_str().unwrap();
        let model = NativePublicInputNetV1::new(
            base.clone(),
            PublicInputWeightsV1::new(
                serde_json::from_value(variant["object"].clone()).unwrap(),
                serde_json::from_value(variant["state"].clone()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let mut max_delta = 0.0f32;
        let mut changes = 0;
        let mut outputs = Vec::new();
        let expected = variant["outputs"].as_array().unwrap();
        assert_eq!(expected.len(), samples.len());
        for (sample, expected) in samples.iter().zip(expected) {
            let encoded = tensor(&sample["native"]);
            let observation: ObservationV6 =
                serde_json::from_value(sample["observation"].clone()).unwrap();
            let legacy = base
                .forward_feature_transfer_v4(encoded_decision_view_v4(&encoded))
                .unwrap();
            let actual = model
                .forward(encoded_decision_view_v4(&encoded), &observation)
                .unwrap();
            let bits = |out: &NativePolicyValueOutputV1| {
                out.logits
                    .iter()
                    .chain(std::iter::once(&out.value))
                    .map(|f| f.to_bits())
                    .collect::<Vec<_>>()
            };
            if name == "zero" {
                assert_eq!(bits(&actual), bits(&legacy));
            }
            if bits(&actual) != bits(&legacy) {
                changes += 1;
            }
            let mut expected_values: Vec<f32> =
                serde_json::from_value(expected["logits"].clone()).unwrap();
            expected_values.push(expected["value"].as_f64().unwrap() as f32);
            assert_eq!(expected_values.len(), actual.logits.len() + 1);
            for (actual, expected) in actual
                .logits
                .iter()
                .chain(std::iter::once(&actual.value))
                .zip(expected_values)
            {
                let delta = (actual - expected).abs();
                assert!(
                    delta <= 1e-3 + 1e-3 * expected.abs(),
                    "{name}: {actual} vs {expected}, delta {delta}"
                );
                max_delta = max_delta.max(delta);
            }
            outputs.push(bits(&actual));
        }
        for pair in outputs.chunks_exact(2) {
            assert_eq!(pair[0], pair[1], "hidden variant changed {name} scores");
        }
        if name != "zero" {
            assert!(changes > 0, "{name} projection was disconnected");
        }
        reports.push(json!({"variant":name,"samples":samples.len(),"max_absolute_delta":max_delta,"changed_from_legacy":changes,"hidden_pair_scores_bit_exact":true}));
    }
    // Reject old input schemas and registry mismatch before evaluating.
    let model = NativePublicInputNetV1::new(base, PublicInputWeightsV1::zero()).unwrap();
    let encoded = tensor(&samples[0]["native"]);
    let mut observation: ObservationV6 =
        serde_json::from_value(samples[0]["observation"].clone()).unwrap();
    let mut wrong = encoded_decision_view_v4(&encoded);
    wrong.schema = NativeEncodedDecisionSchemaV1::contract_v1();
    assert!(model.forward(wrong, &observation).is_err());
    observation.card_db_hash ^= 1;
    assert!(model
        .forward(encoded_decision_view_v4(&encoded), &observation)
        .is_err());
    let report = json!({"status":"NATIVE-FORWARD-ENGINEERING-PASS","architecture":ARCHITECTURE,
        "fixture_sha256":format!("{:x}",Sha256::digest(&fixture_bytes)),"variants":reports,
        "zero_projection_native_scores_bit_exact":true,"absolute_tolerance":1e-3,"relative_tolerance":1e-3,
        "non_claim":"No native training, CUDA update parity, rollout replay or playing-strength evidence."});
    let output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var("MTG_PUBLIC_NATIVE_REPORT").unwrap())
        .unwrap();
    serde_json::to_writer_pretty(output, &report).unwrap();
    println!("{report}");
}
