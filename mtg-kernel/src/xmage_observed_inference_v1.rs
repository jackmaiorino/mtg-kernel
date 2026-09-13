//! Stateless inference over externally projected, actor-visible V3 tensors.
//!
//! The caller owns XMage observations and legal actions. This service loads
//! one pinned current checkpoint, validates and scores the supplied tensor,
//! and returns a deterministic first-argmax action. It owns no game state,
//! rollout, search, training, or knowledge of hidden cards. Tensor validity
//! does not establish that an external producer applied the visibility rules.

use crate::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedInferenceIdentityV1, ExpandedModelSourceV1,
};
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v3::{
    encoded_decision_view_v3, NativeFlatDecisionTensorV3, FEATURES_SOURCE_SHA256_V3,
    FEATURE_CONTRACT_DIGEST_V3, FEATURE_DESCRIPTOR_SHA256_V3, FEATURE_ENCODING_DIGEST_V3,
    FEATURE_REGISTRY_VERSION_V3, FEATURE_SCHEMA_VERSION_V3,
};
use crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::Path;

pub const CONFIG_SCHEMA_V1: &str = "mtg-kernel-xmage-observed-inference-config/v1";
pub const REQUEST_SCHEMA_V1: &str = "mtg-kernel-xmage-observed-decision/v1";
pub const SELECTION_V1: &str = "argmax-first-v1";
pub const MAX_REQUEST_BYTES_V1: usize = 16 * 1024 * 1024;
const MAX_CONFIG_BYTES_V1: usize = 1024 * 1024;
const MAX_ID_BYTES_V1: usize = 128;
const MAX_ACTIONS_V1: usize = 65_536;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedInferenceConfigV1 {
    pub schema: String,
    pub source: ExpandedModelSourceV1,
}

/// All floating values are raw IEEE754 binary32 bits. Index arrays remain
/// signed integers so negative values are rejected by the native validator.
/// This wire deliberately has no arena handles, library ordering or GameState.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedTensorBitsV1 {
    pub state: Vec<u32>,
    pub object_features: Vec<u32>,
    pub object_card_ids: Vec<i64>,
    pub object_groups: Vec<i64>,
    pub object_node_ids: Vec<i64>,
    pub edge_features: Vec<u32>,
    pub edge_source_indices: Vec<i64>,
    pub edge_target_indices: Vec<i64>,
    pub action_features: Vec<u32>,
    pub action_ref_features: Vec<u32>,
    pub action_ref_card_ids: Vec<i64>,
    pub action_ref_action_indices: Vec<i64>,
    pub action_ref_node_indices: Vec<i64>,
}

impl ObservedTensorBitsV1 {
    fn into_tensor(self) -> NativeFlatDecisionTensorV3 {
        let floats = |bits: Vec<u32>| bits.into_iter().map(f32::from_bits).collect();
        NativeFlatDecisionTensorV3 {
            common: NativeFlatDecisionTensorV2 {
                state: floats(self.state),
                object_features: floats(self.object_features),
                object_card_ids: self.object_card_ids,
                object_groups: self.object_groups,
                object_node_ids: self.object_node_ids,
                edge_features: floats(self.edge_features),
                edge_source_indices: self.edge_source_indices,
                edge_target_indices: self.edge_target_indices,
                action_features: floats(self.action_features),
                action_ref_features: floats(self.action_ref_features),
                action_ref_card_ids: self.action_ref_card_ids,
                action_ref_action_indices: self.action_ref_action_indices,
                action_ref_node_indices: self.action_ref_node_indices,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedDecisionRequestV1 {
    pub schema: String,
    pub request_id: String,
    pub session_id: String,
    pub decision_id: u64,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    /// Opaque action handles, exactly one per scorer row in the same order.
    /// They are echoed, never encoded as model features.
    pub action_ids: Vec<String>,
    pub tensor: ObservedTensorBitsV1,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES_V1
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

struct ValidatedRequestV1 {
    request_id: String,
    session_id: String,
    decision_id: u64,
    action_ids: Vec<String>,
    tensor: NativeFlatDecisionTensorV3,
}

fn validate_request(
    request: ObservedDecisionRequestV1,
) -> Result<ValidatedRequestV1, &'static str> {
    if request.schema != REQUEST_SCHEMA_V1
        || request.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V3
        || request.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V3
    {
        return Err("request_feature_contract");
    }
    if !valid_id(&request.request_id) || !valid_id(&request.session_id) {
        return Err("request_identity");
    }
    if request.action_ids.is_empty() || request.action_ids.len() > MAX_ACTIONS_V1 {
        return Err("action_count");
    }
    let mut unique = BTreeSet::new();
    if request
        .action_ids
        .iter()
        .any(|id| !valid_id(id) || !unique.insert(id))
    {
        return Err("action_identity");
    }
    drop(unique);
    let tensor = request.tensor.into_tensor();
    let config = NativePolicyValueModelConfigV1 {
        feature_schema_version: FEATURE_SCHEMA_VERSION_V3,
        feature_registry_version: FEATURE_REGISTRY_VERSION_V3,
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3,
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3,
        ..NativePolicyValueModelConfigV1::contract_v1()
    };
    // This is the same shape/finiteness/card/index validator used by current
    // native V3 forward, including contiguous model node ids and empty refs.
    let counts = encoded_decision_view_v3(&tensor)
        .validate(config)
        .map_err(|_| "tensor_contract")?;
    if counts.action_count != request.action_ids.len() {
        return Err("action_tensor_count");
    }
    Ok(ValidatedRequestV1 {
        request_id: request.request_id,
        session_id: request.session_id,
        decision_id: request.decision_id,
        action_ids: request.action_ids,
        tensor,
    })
}

fn first_argmax(logits: &[f32]) -> Result<usize, &'static str> {
    if logits.is_empty() || logits.iter().any(|value| !value.is_finite()) {
        return Err("invalid_model_output");
    }
    let mut selected = 0;
    for index in 1..logits.len() {
        if logits[index] > logits[selected] {
            selected = index;
        }
    }
    Ok(selected)
}

fn score_request(
    policy: &FrozenPlayPolicyV1,
    identity: &ExpandedInferenceIdentityV1,
    bytes: &[u8],
) -> Result<Value, &'static str> {
    let request: ObservedDecisionRequestV1 =
        serde_json::from_slice(bytes).map_err(|_| "request_json")?;
    let request = validate_request(request)?;
    let output = policy
        .score_training_tensor_v3(&request.tensor)
        .map_err(|_| "native_inference")?;
    if output.logits.len() != request.action_ids.len() || !output.value.is_finite() {
        return Err("invalid_model_output");
    }
    let selected = first_argmax(&output.logits)?;
    Ok(json!({
        "schema": "mtg-kernel-xmage-observed-action/v1",
        "request_id": request.request_id,
        "session_id": request.session_id,
        "decision_id": request.decision_id,
        "request_sha256": format!("{:x}", Sha256::digest(bytes)),
        "model_state_sha256": identity.state_sha256,
        "model_weights_sha256": identity.model.weights_sha256,
        "selection": SELECTION_V1,
        "logits_bits": output.logits.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
        "value_bits": output.value.to_bits(),
        "selected_action_index": selected,
        "selected_action_id": request.action_ids[selected],
    }))
}

/// Reads at most one bounded JSONL record without allocating an unbounded
/// line. EOF after a final non-newline-terminated record still consumes it.
fn read_record<R: BufRead>(reader: &mut R, limit: usize) -> Result<Option<Vec<u8>>, &'static str> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|_| "input_io")?;
        if buffer.is_empty() {
            return Ok((!bytes.is_empty()).then_some(bytes));
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let take = newline.unwrap_or(buffer.len());
        if bytes
            .len()
            .checked_add(take)
            .is_none_or(|size| size > limit)
        {
            return Err("request_too_large");
        }
        bytes.extend_from_slice(&buffer[..take]);
        reader.consume(take + usize::from(newline.is_some()));
        if newline.is_some() {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok(Some(bytes));
        }
    }
}

fn write_record<W: Write>(writer: &mut W, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|_| "output_json".to_string())?;
    writer
        .write_all(b"\n")
        .map_err(|_| "output_io".to_string())?;
    writer.flush().map_err(|_| "output_io".to_string())
}

fn error_record(code: &str) -> Value {
    json!({"schema":"mtg-kernel-xmage-observed-inference-error/v1", "code":code,
        "action_emitted":false, "service_continues":false})
}

/// Loads the pinned import/checkpoint once, then scores records without any
/// game state. Any rejected record emits one bounded error and terminates.
pub fn run_observed_inference_v1<R: BufRead, W: Write>(
    config: ObservedInferenceConfigV1,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    if config.schema != CONFIG_SCHEMA_V1 {
        write_record(writer, &error_record("config_schema"))?;
        return Err("config_schema".into());
    }
    let (policy, identity) = match load_expanded_inference_v1(&config.source) {
        Ok(loaded) => loaded,
        Err(_) => {
            write_record(writer, &error_record("checkpoint_load"))?;
            return Err("checkpoint_load".into());
        }
    };
    write_record(
        writer,
        &json!({
            "schema":"mtg-kernel-xmage-observed-inference-ready/v1",
            "model":identity,
            "feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
            "feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3,
            "features_source_sha256":FEATURES_SOURCE_SHA256_V3,
            "feature_descriptor_sha256":FEATURE_DESCRIPTOR_SHA256_V3,
            "selection":SELECTION_V1,
            "float_encoding":"ieee754-binary32-u32-bits",
            "max_request_bytes":MAX_REQUEST_BYTES_V1,
            "visibility_authority":"external-actor-visible-projector",
            "inference_backend":"native-cpu-sequential",
            "game_state_owned":false,
        }),
    )?;
    loop {
        let result = match read_record(reader, MAX_REQUEST_BYTES_V1) {
            Ok(None) => return Ok(()),
            Ok(Some(bytes)) => score_request(&policy, &identity, &bytes),
            Err(code) => Err(code),
        };
        match result {
            Ok(value) => write_record(writer, &value)?,
            Err(code) => {
                write_record(writer, &error_record(code))?;
                return Err(code.into());
            }
        }
    }
}

pub fn run_observed_inference_config_path_v1<R: BufRead, W: Write>(
    path: &Path,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    let file = File::open(path).map_err(|_| "config_open".to_string())?;
    if !file
        .metadata()
        .map_err(|_| "config_metadata".to_string())?
        .is_file()
    {
        return Err("config_not_file".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_CONFIG_BYTES_V1 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "config_read".to_string())?;
    if bytes.len() > MAX_CONFIG_BYTES_V1 {
        return Err("config_too_large".into());
    }
    let config = serde_json::from_slice(&bytes).map_err(|_| "config_json".to_string())?;
    run_observed_inference_v1(config, reader, writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn request() -> ObservedDecisionRequestV1 {
        let config = NativePolicyValueModelConfigV1::contract_v1();
        ObservedDecisionRequestV1 {
            schema: REQUEST_SCHEMA_V1.into(),
            request_id: "r1".into(),
            session_id: "s1".into(),
            decision_id: 7,
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            action_ids: vec!["pass".into(), "cast-opaque-1".into()],
            tensor: ObservedTensorBitsV1 {
                state: vec![0; config.state_dim],
                object_features: vec![0; config.object_feature_dim],
                object_card_ids: vec![1],
                object_groups: vec![0],
                object_node_ids: vec![0],
                edge_features: vec![],
                edge_source_indices: vec![],
                edge_target_indices: vec![],
                action_features: vec![0; 2 * config.action_feature_dim],
                action_ref_features: vec![0; config.action_ref_feature_dim],
                action_ref_card_ids: vec![1],
                action_ref_action_indices: vec![1],
                action_ref_node_indices: vec![0],
            },
        }
    }

    #[test]
    fn observed_inference_tensor_validates_refs_shapes_and_finite_bits() {
        let original = request();
        validate_request(original.clone()).unwrap();
        let corruptions: &[fn(&mut ObservedDecisionRequestV1)] = &[
            |r| r.feature_encoding_digest.clear(),
            |r| r.action_ids[1] = r.action_ids[0].clone(),
            |r| r.action_ids.pop().map(|_| ()).unwrap(),
            |r| r.tensor.state[0] = f32::NAN.to_bits(),
            |r| {
                r.tensor.object_features.pop();
            },
            |r| r.tensor.object_node_ids[0] = 1,
            |r| r.tensor.object_groups[0] = -1,
            |r| r.tensor.object_card_ids[0] = i64::MAX,
            |r| r.tensor.action_ref_action_indices[0] = 2,
            |r| r.tensor.action_ref_node_indices[0] = 1,
            |r| r.tensor.action_ref_card_ids[0] = -1,
            |r| r.tensor.action_ref_features[0] = f32::INFINITY.to_bits(),
        ];
        for corrupt in corruptions {
            let mut changed = original.clone();
            corrupt(&mut changed);
            assert!(validate_request(changed).is_err());
        }
        let mut empty_refs = original;
        empty_refs.tensor.action_ref_features.clear();
        empty_refs.tensor.action_ref_card_ids.clear();
        empty_refs.tensor.action_ref_action_indices.clear();
        empty_refs.tensor.action_ref_node_indices.clear();
        validate_request(empty_refs).unwrap();
    }

    #[test]
    fn observed_inference_wire_rejects_hidden_or_unknown_fields() {
        let mut value = serde_json::to_value(request()).unwrap();
        value["tensor"]["opponent_hand"] = json!([42]);
        assert!(serde_json::from_value::<ObservedDecisionRequestV1>(value).is_err());
        let mut value = serde_json::to_value(request()).unwrap();
        value["game_state"] = json!({});
        assert!(serde_json::from_value::<ObservedDecisionRequestV1>(value).is_err());
        let mut value = serde_json::to_value(request()).unwrap();
        value["tensor"]["state"][0] = json!(0.5);
        assert!(serde_json::from_value::<ObservedDecisionRequestV1>(value).is_err());
        let mut signed_zero = request();
        signed_zero.tensor.state[0] = (-0.0_f32).to_bits();
        let decoded: ObservedDecisionRequestV1 =
            serde_json::from_slice(&serde_json::to_vec(&signed_zero).unwrap()).unwrap();
        assert_eq!(
            validate_request(decoded).unwrap().tensor.common.state[0].to_bits(),
            (-0.0_f32).to_bits()
        );
    }

    #[test]
    fn observed_inference_first_argmax_has_explicit_ties_and_no_invalid_fallback() {
        assert_eq!(first_argmax(&[-2.0, -1.0, -1.0]).unwrap(), 1);
        assert_eq!(first_argmax(&[-0.0, 0.0]).unwrap(), 0);
        assert!(first_argmax(&[]).is_err());
        assert!(first_argmax(&[f32::NAN]).is_err());
        assert!(first_argmax(&[0.0, f32::INFINITY]).is_err());
    }

    #[test]
    fn observed_inference_jsonl_is_bounded_and_handles_eof() {
        let mut input = Cursor::new(b"{}\r\nlast".to_vec());
        assert_eq!(read_record(&mut input, 5).unwrap(), Some(b"{}".to_vec()));
        assert_eq!(read_record(&mut input, 5).unwrap(), Some(b"last".to_vec()));
        assert_eq!(read_record(&mut input, 5).unwrap(), None);
        let mut input = Cursor::new(b"123456".to_vec());
        assert_eq!(read_record(&mut input, 5).unwrap_err(), "request_too_large");
        let mut input = Cursor::new(b"12345\n".to_vec());
        assert_eq!(read_record(&mut input, 5).unwrap(), Some(b"12345".to_vec()));
    }
}
