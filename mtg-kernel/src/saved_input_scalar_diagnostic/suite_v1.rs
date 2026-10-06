//! Read-only, pinned checkpoint/case sibling of the frozen T1 diagnostic.
//! The reconstruction producer establishes legality and live candidate binding.
//! This reader verifies that exact ordered input and never steps or samples.
use super::{score, Condition, Input};
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedInferenceIdentityV1, ExpandedModelSourceV1, PinnedFileV1,
};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v4::{
    encoded_decision_view_v4, NativeFlatDecisionTensorV4, FEATURE_CONTRACT_DIGEST_V4,
    FEATURE_ENCODING_DIGEST_V4, FEATURE_REGISTRY_VERSION_V4, FEATURE_SCHEMA_VERSION_V4,
};
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use crate::policy_observation_v6::{ObservationV6, OBSERVATION_SCHEMA_VERSION_V6};
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::rl_session::{
    FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2, FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2,
    FLAT_ACTION_DECISION_SLICE_VERSION_V2, FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Read, path::Path, time::Instant};

const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CASES: usize = 22;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    endpoint: PinnedFileV1,
    membership_sha256: String,
    cases: Vec<CasePin>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CasePin {
    case_id: String,
    input: PinnedFileV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    schema: String,
    endpoint_id: String,
    source: ExpandedModelSourceV1,
    expected_identity: ExpectedIdentity,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedIdentity {
    checkpoint_sha256: String,
    state_sha256: String,
    weights_sha256: String,
    model_parameter_sha256: String,
    embedding_table_sha256: String,
    adam_step: u64,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    slice_version: u32,
    ref_role_mapping_version: u32,
    card_token_mapping_version: u32,
    candidate_commitment_version: u32,
    card_db_hash: u64,
    episode_id: u64,
    environment_revision: u64,
    bound_policy_step_count: u64,
    physical_decision_id: u64,
    bound_physical_decision_count: u64,
    substep_index: u32,
    substep_count: u32,
    acting_player: u8,
    decision_kind: u8,
    legal_action_count: u32,
    candidate_order_commitment: [u8; 16],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectedInput {
    schema: String,
    case_id: String,
    origin: Value,
    status: String,
    observation: ObservationV6,
    ordered_actions: Vec<ActionSemanticV1>,
    binding: Binding,
    encoded_input: Input,
    encoded_input_sha256: String,
    semantic_menu_sha256: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    runtime: Value,
    differences: Vec<Value>,
}

fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

// Hash a Value, not struct field insertion order. The producer uses the same
// sorted-object-key JSON representation; array order is never normalized.
fn value_sha(value: &impl Serialize) -> Result<String, String> {
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    Ok(sha(&serde_json::to_vec(&value).map_err(|e| e.to_string())?))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    require(path.is_absolute(), "suite file path must be absolute")?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        bytes.len() as u64 <= MAX_JSON_BYTES,
        "suite JSON exceeds 4 MiB",
    )?;
    Ok(bytes)
}

fn read_pin<T: serde::de::DeserializeOwned>(pin: &PinnedFileV1) -> Result<T, String> {
    require(digest_valid(&pin.sha256), "invalid suite file digest")?;
    let bytes = read_bytes(&pin.path)?;
    require(sha(&bytes) == pin.sha256, "suite file hash mismatch")?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn validate_request(request: &Request) -> Result<(), String> {
    require(
        request.schema == "gameplay-saved-input-suite/v1",
        "wrong suite schema",
    )?;
    require(
        digest_valid(&request.membership_sha256),
        "invalid membership digest",
    )?;
    require(
        !request.cases.is_empty() && request.cases.len() <= MAX_CASES,
        "suite case count outside 1..22",
    )?;
    let mut ids = BTreeSet::new();
    for case in &request.cases {
        require(
            !case.case_id.is_empty() && ids.insert(&case.case_id),
            "empty or duplicate case id",
        )?;
    }
    Ok(())
}

fn validate_endpoint(endpoint: &Endpoint) -> Result<(), String> {
    let expected = &endpoint.expected_identity;
    require(
        endpoint.schema == "gameplay-checkpoint-descriptor/v1" && !endpoint.endpoint_id.is_empty(),
        "wrong endpoint descriptor",
    )?;
    for value in [
        &expected.checkpoint_sha256,
        &expected.state_sha256,
        &expected.weights_sha256,
        &expected.model_parameter_sha256,
        &expected.embedding_table_sha256,
    ] {
        require(digest_valid(value), "invalid endpoint identity digest")?;
    }
    require(
        endpoint
            .source
            .checkpoint
            .as_ref()
            .is_some_and(|p| p.sha256 == expected.checkpoint_sha256),
        "endpoint checkpoint descriptor mismatch",
    )?;
    require(
        expected.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
            && expected.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V4
            && expected.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}"),
        "endpoint is not current V4/card identity",
    )?;
    require(
        endpoint
            .source
            .feature_transfer
            .expected_feature_contract_digest
            == expected.feature_contract_digest
            && endpoint
                .source
                .feature_transfer
                .expected_feature_encoding_digest
                == expected.feature_encoding_digest,
        "endpoint feature descriptor mismatch",
    )
}

fn validate_loaded(
    expected: &ExpectedIdentity,
    actual: &ExpandedInferenceIdentityV1,
) -> Result<(), String> {
    require(
        actual.checkpoint_sha256.as_ref() == Some(&expected.checkpoint_sha256)
            && actual.state_sha256 == expected.state_sha256
            && actual.adam_step == expected.adam_step
            && actual.model.weights_sha256 == expected.weights_sha256
            && actual.model.model_parameter_sha256 == expected.model_parameter_sha256
            && actual.model.embedding_table_sha256 == expected.embedding_table_sha256
            && actual.model.feature_contract_digest == expected.feature_contract_digest
            && actual.model.feature_encoding_digest == expected.feature_encoding_digest
            && actual.model.card_db_hash == expected.card_db_hash
            && actual.feature_schema_version == FEATURE_SCHEMA_VERSION_V4
            && actual.feature_registry_version == FEATURE_REGISTRY_VERSION_V4,
        "loaded checkpoint identity differs from frozen endpoint",
    )
}

fn decoded(input: &Input) -> Result<NativeFlatDecisionTensorV4, String> {
    require(
        input.float_encoding == "IEEE754 binary32 u32 bits, flattened row-major",
        "wrong tensor bit encoding",
    )?;
    let f = |v: &[u32]| v.iter().copied().map(f32::from_bits).collect();
    Ok(NativeFlatDecisionTensorV4 {
        common: NativeFlatDecisionTensorV2 {
            state: f(&input.state),
            object_features: f(&input.object_features),
            object_card_ids: input.object_card_ids.clone(),
            object_groups: input.object_groups.clone(),
            object_node_ids: input.object_node_ids.clone(),
            edge_features: f(&input.edge_features),
            edge_source_indices: input.edge_source_indices.clone(),
            edge_target_indices: input.edge_target_indices.clone(),
            action_features: f(&input.action_features),
            action_ref_features: f(&input.action_ref_features),
            action_ref_card_ids: input.action_ref_card_ids.clone(),
            action_ref_action_indices: input.action_ref_action_indices.clone(),
            action_ref_node_indices: input.action_ref_node_indices.clone(),
        },
    })
}

fn validate_tensor(input: &Input) -> Result<usize, String> {
    let tensor = decoded(input)?;
    let config = NativePolicyValueModelConfigV1 {
        feature_schema_version: FEATURE_SCHEMA_VERSION_V4,
        feature_registry_version: FEATURE_REGISTRY_VERSION_V4,
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4,
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4,
        ..NativePolicyValueModelConfigV1::contract_v1()
    };
    let counts = encoded_decision_view_v4(&tensor)
        .validate(config)
        .map_err(|e| e.to_string())?;
    require(
        counts.action_count <= 65536,
        "case exceeds native sampler action envelope",
    )?;
    Ok(counts.action_count)
}

fn validate_menu(
    binding: &Binding,
    actions: &[ActionSemanticV1],
    legal_count: usize,
    menu_sha256: &str,
    actor: PlayerSeatV1,
) -> Result<(), String> {
    require(
        binding.slice_version == FLAT_ACTION_DECISION_SLICE_VERSION_V2
            && binding.ref_role_mapping_version == FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2
            && binding.card_token_mapping_version == FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2
            && binding.candidate_commitment_version == FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2,
        "binding is not the live V4 producer's V2 row commitment",
    )?;
    require(
        legal_count == actions.len() && legal_count == binding.legal_action_count as usize,
        "tensor, binding and semantic legal counts differ",
    )?;
    require(
        value_sha(&actions)? == menu_sha256,
        "semantic menu order/hash differs",
    )?;
    let actor = serde_json::to_value(actor).map_err(|e| e.to_string())?;
    let mut unique = std::collections::HashSet::new();
    for action in actions {
        let value = serde_json::to_value(action).map_err(|e| e.to_string())?;
        require(
            value.get("actor") == Some(&actor) && unique.insert(action),
            "ambiguous, wrong-actor or duplicate semantic action",
        )?;
    }
    Ok(())
}

fn validate_case(case: &CorrectedInput, pin: &CasePin) -> Result<(), String> {
    require(
        case.schema == "gameplay-corrected-input/v1"
            && case.case_id == pin.case_id
            && matches!(case.status.as_str(), "exact" | "corrected"),
        "invalid or unscorable corrected case",
    )?;
    require(
        case.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
            && case.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V4
            && case.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}"),
        "case feature/card identity differs",
    )?;
    require(
        value_sha(&case.encoded_input)? == case.encoded_input_sha256,
        "encoded tensor hash differs",
    )?;
    let count = validate_tensor(&case.encoded_input)?;
    let obs = &case.observation;
    let binding = &case.binding;
    let actor = match obs.acting_player {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    };
    require(
        obs.schema_version == OBSERVATION_SCHEMA_VERSION_V6
            && obs.card_db_hash == KERNEL_CARDDB_HASH
            && binding.card_db_hash == obs.card_db_hash
            && binding.acting_player == actor
            && binding.bound_policy_step_count == obs.step_index
            && binding.physical_decision_id == obs.physical_decision_id
            && binding.substep_index == obs.substep_index
            && binding.substep_count == obs.substep_count
            && obs.substep_count > 0
            && obs.substep_index < obs.substep_count,
        "observation and native decision binding differ",
    )?;
    validate_menu(
        binding,
        &case.ordered_actions,
        count,
        &case.semantic_menu_sha256,
        obs.acting_player,
    )
}

pub(super) fn run(path: &Path) -> Result<(), String> {
    let started = Instant::now();
    let bytes = read_bytes(path)?;
    let request_pin = PinnedFileV1 {
        path: path.to_path_buf(),
        sha256: sha(&bytes),
    };
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate_request(&request)?;
    let endpoint: Endpoint = read_pin(&request.endpoint)?;
    validate_endpoint(&endpoint)?;
    let mut cases = Vec::with_capacity(request.cases.len());
    for pin in &request.cases {
        let case = read_pin(&pin.input)?;
        validate_case(&case, pin)?;
        cases.push(case);
    }
    // Every case and descriptor is validated before the first checkpoint load
    // and forward. The existing reader checks complete parameters and Adam.
    let (policy, actual) = load_expanded_inference_v1(&endpoint.source)?;
    validate_loaded(&endpoint.expected_identity, &actual)?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|e| e.to_string())?;
    model
        .replace_parameter_snapshot_v1(&policy.training_parameters_v3())
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(cases.len());
    for (case, pin) in cases.into_iter().zip(&request.cases) {
        let scored = score(
            &model,
            &Condition {
                id: case.case_id.clone(),
                encoded_input: case.encoded_input,
                expected_logit_bits: None,
            },
        )?;
        require(
            scored["logit_bits"]
                .as_array()
                .is_some_and(|v| v.len() == case.ordered_actions.len())
                && scored["mass_numerators"]
                    .as_array()
                    .is_some_and(|v| v.len() == case.ordered_actions.len()),
            "scored output count differs from frozen menu",
        )?;
        let value_bits: u32 =
            serde_json::from_value(scored["value_bits"].clone()).map_err(|e| e.to_string())?;
        require(
            f32::from_bits(value_bits).is_finite(),
            "nonfinite scored value",
        )?;
        rows.push(json!({"case_id":case.case_id,"input":pin.input,"case_sha256":pin.input.sha256,
            "encoded_input_sha256":case.encoded_input_sha256,"semantic_menu_sha256":case.semantic_menu_sha256,
            "origin":case.origin,"status":case.status,"input_runtime":case.runtime,"differences":case.differences,
            "ordered_actions":case.ordered_actions,"binding":case.binding,
            "logit_bits":scored["logit_bits"],"value_bits":value_bits,
            "mass_numerators":scored["mass_numerators"],"mass_denominator":scored["mass_denominator"]}));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    println!(
        "{}",
        json!({"schema":"gameplay-saved-input-suite-result/v1","complete":true,
        "request":request_pin,"endpoint":request.endpoint,"endpoint_id":endpoint.endpoint_id,
        "source":endpoint.source,"actual_identity":actual,"membership_sha256":request.membership_sha256,
        "forward_calls":rows.len(),"sampling_calls":0,"games":0,"backward_calls":0,"cases":rows,
        "runtime":{"engine_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
            "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
            "executable_sha256":sha(&fs::read(exe).map_err(|e| e.to_string())?)},
        "seconds":started.elapsed().as_secs_f64()})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Input {
        Input {
            float_encoding: "IEEE754 binary32 u32 bits, flattened row-major".into(),
            state: vec![0; 219],
            object_features: vec![0; 98],
            object_card_ids: vec![0],
            object_groups: vec![0],
            object_node_ids: vec![0],
            edge_features: vec![],
            edge_source_indices: vec![],
            edge_target_indices: vec![],
            action_features: vec![0; 390],
            action_ref_features: vec![],
            action_ref_card_ids: vec![],
            action_ref_action_indices: vec![],
            action_ref_node_indices: vec![],
        }
    }

    fn binding() -> Binding {
        Binding {
            slice_version: 2,
            ref_role_mapping_version: FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2,
            card_token_mapping_version: 2,
            candidate_commitment_version: 2,
            card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 1,
            environment_revision: 1,
            bound_policy_step_count: 1,
            physical_decision_id: 1,
            bound_physical_decision_count: 1,
            substep_index: 0,
            substep_count: 1,
            acting_player: 0,
            decision_kind: 0,
            legal_action_count: 2,
            candidate_order_commitment: [1; 16],
        }
    }

    #[cfg(feature = "gameplay-checkpoint-reconstruction-v1")]
    #[test]
    fn suite_deserializes_and_validates_actual_producer_envelope() {
        // Static fixture data only: no session, replay, model or forward call.
        let golden: Value = serde_json::from_str(include_str!(
            "../../../data/flat_policy_v2/python_full_features_v2.json"
        ))
        .unwrap();
        let mut observation: Value = serde_json::from_str(
            golden["cases"][0]["canonical_observation_json"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        observation["schema_version"] = json!(OBSERVATION_SCHEMA_VERSION_V6);
        observation["card_db_hash"] = json!(KERNEL_CARDDB_HASH);
        observation["acting_player"] = json!("p0");
        observation["step_index"] = json!(1);
        observation["physical_decision_id"] = json!(1);
        observation["substep_index"] = json!(0);
        observation["substep_count"] = json!(1);
        observation["extensions"] = serde_json::to_value(
            crate::policy_observation_v6::PolicyObservationExtensionsV6::default(),
        )
        .unwrap();
        let actions = json!([
            {"action_kind":"choose_optional_cost_use","actor":"p0","use_cost":false},
            {"action_kind":"choose_optional_cost_use","actor":"p0","use_cost":true}]);
        let record = crate::gameplay_checkpoint_reconstruction_v1::corrected_input_record(
            &json!({"case_id":"synthetic","origin":{}}),
            &observation,
            &actions,
            &serde_json::to_value(input()).unwrap(),
            &serde_json::to_value(binding()).unwrap(),
            &json!({}),
            &[],
        )
        .unwrap();
        let serialized = serde_json::to_vec(&record).unwrap();
        let case: CorrectedInput = serde_json::from_slice(&serialized).unwrap();
        validate_case(
            &case,
            &CasePin {
                case_id: "synthetic".into(),
                input: PinnedFileV1 {
                    path: "unused".into(),
                    sha256: sha(&serialized),
                },
            },
        )
        .unwrap();
        assert!(record["card_db_hash"].is_string());
        assert!(record["observation"]["card_db_hash"].is_u64());
        let mut wrong = record;
        wrong["card_db_hash"] = json!(KERNEL_CARDDB_HASH);
        assert!(serde_json::from_value::<CorrectedInput>(wrong).is_err());
    }

    #[test]
    fn suite_refuses_tensor_shape_nonfinite_and_index_corruption() {
        let good = input();
        assert_eq!(validate_tensor(&good).unwrap(), 2);
        let mut bad = good.clone();
        bad.action_features.pop();
        assert!(validate_tensor(&bad).is_err());
        let mut bad = good.clone();
        bad.state[0] = f32::NAN.to_bits();
        assert!(validate_tensor(&bad).is_err());
        let mut bad = good;
        bad.object_node_ids[0] = 1;
        assert!(validate_tensor(&bad).is_err());
    }

    #[test]
    fn suite_refuses_menu_reordering_binding_count_and_wrong_actor() {
        let actions = vec![
            ActionSemanticV1::ChooseOptionalCostUse {
                actor: PlayerSeatV1::P0,
                use_cost: false,
            },
            ActionSemanticV1::ChooseOptionalCostUse {
                actor: PlayerSeatV1::P0,
                use_cost: true,
            },
        ];
        let hash = value_sha(&actions).unwrap();
        let b = binding();
        assert!(validate_menu(&b, &actions, 2, &hash, PlayerSeatV1::P0).is_ok());
        let mut changed = actions.clone();
        changed.reverse();
        assert!(validate_menu(&b, &changed, 2, &hash, PlayerSeatV1::P0).is_err());
        let mut changed = b.clone();
        changed.legal_action_count = 1;
        assert!(validate_menu(&changed, &actions, 2, &hash, PlayerSeatV1::P0).is_err());
        assert!(validate_menu(&b, &actions, 2, &hash, PlayerSeatV1::P1).is_err());
    }

    #[test]
    fn suite_refuses_wrong_checkpoint_pin_before_loading() {
        let mut value = json!({"schema":"gameplay-checkpoint-descriptor/v1","endpoint_id":"fixture",
            "source":{"play_import":{"path":"unused","sha256":"a".repeat(64)},
            "checkpoint":{"path":"unused","sha256":"a".repeat(64)},
            "feature_transfer":{"expected_feature_contract_digest":FEATURE_CONTRACT_DIGEST_V4,
            "expected_feature_encoding_digest":FEATURE_ENCODING_DIGEST_V4}},
            "expected_identity":{"checkpoint_sha256":"a".repeat(64),"state_sha256":"b".repeat(64),
            "weights_sha256":"c".repeat(64),"model_parameter_sha256":"d".repeat(64),
            "embedding_table_sha256":"e".repeat(64),"adam_step":32464,
            "feature_contract_digest":FEATURE_CONTRACT_DIGEST_V4,"feature_encoding_digest":FEATURE_ENCODING_DIGEST_V4,
            "card_db_hash":format!("{KERNEL_CARDDB_HASH:016x}")}});
        assert!(validate_endpoint(&serde_json::from_value(value.clone()).unwrap()).is_ok());
        value["source"]["checkpoint"]["sha256"] = json!("f".repeat(64));
        assert!(validate_endpoint(&serde_json::from_value(value).unwrap()).is_err());
    }

    #[test]
    fn suite_native_q64_preserves_mass_ties_clamp_and_wide_menus() {
        let mut sampler = crate::fast_sampler::WideCategoricalScratchV1::default();
        let total = 1_u128 << 64;
        assert_eq!(sampler.apportion(&[0.0]).unwrap(), &[total]);
        assert_eq!(
            sampler.apportion(&[0.0; 3]).unwrap(),
            &[total / 3 + 1, total / 3, total / 3]
        );
        let clamped = sampler.apportion(&[0.0, -16.0]).unwrap().to_vec();
        assert_eq!(sampler.apportion(&[0.0, -100.0]).unwrap(), clamped);
        assert_eq!(
            sampler.apportion(&[0.0; 65]).unwrap().iter().sum::<u128>(),
            total
        );
    }
}
