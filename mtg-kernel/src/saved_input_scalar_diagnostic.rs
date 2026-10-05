//! Bounded, opt-in offline diagnostic using the existing scalar V4 forward.
//! All four captured originals must pass exact logit-bit parity before hybrids.
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1,
    NativePolicyValueNetV1,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{self, Read};

const CHECKPOINT_SHA: &str = "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1";
const WEIGHTS_SHA: &str = "e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333";
const PARAMETER_SHA: &str = "614326d2ec55c94583b1b050451f770ce9404e03bc21b9fb5fb6cb4f7d32263f";
const EMBEDDING_SHA: &str = "9f2ba50d7097345caf1930edd2e911bad87383524b30bbe09726fb344096f2bb";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    float_encoding: String,
    state: Vec<u32>,
    object_features: Vec<u32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<u32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features: Vec<u32>,
    action_ref_features: Vec<u32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Condition {
    id: String,
    encoded_input: Input,
    expected_logit_bits: Option<Vec<u32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    originals: Vec<Condition>,
    hybrids: Vec<Condition>,
}

fn score(model: &NativePolicyValueNetV1, cell: &Condition) -> Result<Value, String> {
    let e = &cell.encoded_input;
    if e.float_encoding != "IEEE754 binary32 u32 bits, flattened row-major" {
        return Err("float encoding mismatch".into());
    }
    let f = |x: &[u32]| x.iter().copied().map(f32::from_bits).collect::<Vec<_>>();
    let state = f(&e.state);
    let objects = f(&e.object_features);
    let edges = f(&e.edge_features);
    let actions = f(&e.action_features);
    let refs = f(&e.action_ref_features);
    let config = model.feature_transfer_config_v4();
    let schema = NativeEncodedDecisionSchemaV1 {
        version: config.feature_schema_version,
        registry_version: config.feature_registry_version,
        contract_digest: config.feature_contract_digest,
        encoding_digest: config.feature_encoding_digest,
        ..NativeEncodedDecisionSchemaV1::contract_v1()
    };
    let view = NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema,
        &state,
        &objects,
        &e.object_card_ids,
        &e.object_groups,
        &e.object_node_ids,
        &edges,
        &e.edge_source_indices,
        &e.edge_target_indices,
        &actions,
        &refs,
        &e.action_ref_card_ids,
        &e.action_ref_action_indices,
        &e.action_ref_node_indices,
    );
    let output = model
        .forward_feature_transfer_v4(view)
        .map_err(|e| e.to_string())?;
    let bits: Vec<_> = output.logits.iter().map(|x| x.to_bits()).collect();
    if cell
        .expected_logit_bits
        .as_ref()
        .is_some_and(|expected| *expected != bits)
    {
        return Err(format!(
            "original {} logit bit mismatch: actual {bits:?}, expected {:?}",
            cell.id, cell.expected_logit_bits
        ));
    }
    let masses: Vec<_> = crate::fast_sampler::WideCategoricalScratchV1::default()
        .apportion(&output.logits)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|x| x.to_string())
        .collect();
    Ok(
        json!({"id":cell.id,"logit_bits":bits,"value_bits":output.value.to_bits(),
        "mass_numerators":masses,"mass_denominator":"18446744073709551616"}),
    )
}

fn validate_counts(request: &Request) -> Result<(), String> {
    if request.originals.len() != 4
        || request.hybrids.len() != 12
        || request
            .originals
            .iter()
            .any(|c| c.expected_logit_bits.is_none())
        || request
            .hybrids
            .iter()
            .any(|c| c.expected_logit_bits.is_some())
    {
        return Err("requires four parity originals and twelve hybrids".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    if request
        .originals
        .iter()
        .chain(&request.hybrids)
        .any(|c| !ids.insert(&c.id))
    {
        return Err("duplicate condition identity".into());
    }
    Ok(())
}

pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: saved_input_scalar_diagnostic CHECKPOINT < request.json".into());
    }
    let raw = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    if format!("{:x}", Sha256::digest(&raw)) != CHECKPOINT_SHA {
        return Err("checkpoint hash mismatch".into());
    }
    let checkpoint: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|e| e.to_string())?;
    let config = model.feature_transfer_config_v4();
    if checkpoint["feature_contract_digest"] != config.feature_contract_digest
        || checkpoint["feature_encoding_digest"] != config.feature_encoding_digest
        || checkpoint["adam_step"] != 32400
    {
        return Err("checkpoint contract mismatch".into());
    }
    let mut parameters = model.parameter_snapshot_v1();
    let saved = checkpoint["parameters"]
        .as_array()
        .ok_or("missing parameters")?;
    if saved.len() != parameters.len() {
        return Err("parameter count mismatch".into());
    }
    for (parameter, saved) in parameters.iter_mut().zip(saved) {
        if saved["name"] != parameter.name
            || serde_json::from_value::<Vec<usize>>(saved["shape"].clone())
                .map_err(|e| e.to_string())?
                != parameter.shape
        {
            return Err("parameter layout mismatch".into());
        }
        parameter.values = serde_json::from_value::<Vec<u32>>(saved["values"].clone())
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(f32::from_bits)
            .collect();
    }
    model
        .replace_parameter_snapshot_v1(&parameters)
        .map_err(|e| e.to_string())?;
    let mut weights = Sha256::new();
    let mut embeddings = Sha256::new();
    model.visit_parameters_v1(|name, _, values| {
        for v in values {
            weights.update(v.to_bits().to_le_bytes());
            if name == "card_embedding.weight" {
                embeddings.update(v.to_bits().to_le_bytes());
            }
        }
    });
    if format!("{:x}", weights.finalize()) != WEIGHTS_SHA
        || model.parameter_manifest_sha256_v1() != PARAMETER_SHA
        || format!("{:x}", embeddings.finalize()) != EMBEDDING_SHA
    {
        return Err("loaded parameter identity mismatch".into());
    }
    let mut input = Vec::new();
    io::stdin()
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() > 4 * 1024 * 1024 {
        return Err("request byte bound exceeded".into());
    }
    let request: Request = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
    validate_counts(&request)?;
    let mut results = Vec::new();
    for cell in &request.originals {
        results.push(score(&model, cell)?);
    }
    // No hybrid forward occurs unless every original passed exact parity.
    for cell in &request.hybrids {
        results.push(score(&model, cell)?);
    }
    println!(
        "{}",
        json!({"original_parity":true,"forward_calls":16,"conditions":results,
        "weights_sha256":WEIGHTS_SHA,"model_parameter_sha256":PARAMETER_SHA,"embedding_table_sha256":EMBEDDING_SHA})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn condition(id: String, original: bool) -> Condition {
        serde_json::from_value(json!({"id":id,"expected_logit_bits":original.then_some(vec![0_u32]),
            "encoded_input":{"float_encoding":"IEEE754 binary32 u32 bits, flattened row-major",
            "state":[],"object_features":[],"object_card_ids":[],"object_groups":[],"object_node_ids":[],
            "edge_features":[],"edge_source_indices":[],"edge_target_indices":[],"action_features":[],
            "action_ref_features":[],"action_ref_card_ids":[],"action_ref_action_indices":[],"action_ref_node_indices":[]}})).unwrap()
    }

    fn request() -> Request {
        Request {
            originals: (0..4)
                .map(|i| condition(format!("original-{i}"), true))
                .collect(),
            hybrids: (0..12)
                .map(|i| condition(format!("hybrid-{i}"), false))
                .collect(),
        }
    }

    #[test]
    fn refuses_missing_original_bits_and_extra_conditions() {
        let mut value = request();
        assert!(validate_counts(&value).is_ok());
        value.originals[0].expected_logit_bits = None;
        assert!(validate_counts(&value).is_err());
        let mut value = request();
        value.hybrids.push(condition("extra".into(), false));
        assert!(validate_counts(&value).is_err());
    }

    #[test]
    fn refuses_duplicate_identity_and_hybrid_parity_metadata() {
        let mut value = request();
        value.hybrids[0].id = value.originals[0].id.clone();
        assert!(validate_counts(&value).is_err());
        let mut value = request();
        value.hybrids[0].expected_logit_bits = Some(vec![0]);
        assert!(validate_counts(&value).is_err());
    }
}
