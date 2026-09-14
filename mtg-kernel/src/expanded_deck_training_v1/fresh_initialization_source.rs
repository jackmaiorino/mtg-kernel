//! Strict, read-only import of sampled Net8 initialization bytes.
//!
//! Only the two explicit source pins are opened. Producer file locations are
//! historical provenance, including when producer and consumer use different
//! operating systems. This verifies interchange consistency, not execution of
//! the claimed Python RNG or independent training/convergence.

use super::{FrozenPlayObservationTransferV3, FrozenPlayPolicyV1, PinnedFileV1};
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1;
use crate::native_flat_tensorizer_v3::{
    FEATURES_SOURCE_SHA256_V3, FEATURE_CONTRACT_DIGEST_V3, FEATURE_DESCRIPTOR_SHA256_V3,
    FEATURE_ENCODING_DIGEST_V3,
};
use crate::native_policy_value_net_v1::{
    NativePolicyValueModelConfigV1, NativePolicyValueNetV1, CARD_EMBEDDING_DIM_V1,
    MODEL_ARCHITECTURE_VERSION_V1, MODEL_CONFIG_FINGERPRINT_V1, PARAMETER_COUNT_V1,
};
use crate::sideboard_play_policy_v1::FreshPlayPolicyIdentityV1;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;

pub(super) const SOURCE_SCHEMA: &str = "mtg-kernel-fresh-initialization-source/v1";
const MANIFEST_SCHEMA: &str = "mtg-kernel-fresh-initialization/v1";
const JSON_CAP: usize = 1024 * 1024;
const PAYLOAD_BYTES: usize = 4_923_976;
const SOURCE_CAP: u64 = 16 * 1024 * 1024;
const RUNTIME_CAP: u64 = 2 * 1024 * 1024 * 1024;
const REGISTRY: &[u8] = include_bytes!("../../../data/cards_v1.json");
const SOURCE_PATHS: [&str; 8] = [
    "python/mtg_kernel_rl/phase1_fresh_initialization_v1.py",
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
    "python/mtg_kernel_rl/features_v6.py",
    "data/flat_policy_v3/feature_contract_v3.json",
    "data/cards_v1.json",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedFreshInitializationSourceV1 {
    pub schema: String,
    pub initialization: PinnedFileV1,
    pub parameters: PinnedFileV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Initializer {
    identity: String,
    authority: String,
    base_seed: u64,
    model_init_seed: u64,
    seed_derivation: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalPin {
    path: String,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SizedHistoricalPin {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    python_version: String,
    python_implementation: String,
    platform_system: String,
    platform_machine: String,
    byte_order: String,
    torch_version: String,
    device: String,
    dtype: String,
    deterministic_algorithms: bool,
    num_threads: u64,
    num_interop_threads: u64,
    python_executable: SizedHistoricalPin,
    #[serde(rename = "torch_C")]
    torch_c: SizedHistoricalPin,
    torch_cpu_library: SizedHistoricalPin,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Producer {
    source_git_commit: String,
    source_git_clean: bool,
    source_files: Vec<SizedHistoricalPin>,
    runtime: Runtime,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    registry: HistoricalPin,
    card_db_hash: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    features_source_sha256: String,
    feature_descriptor_sha256: String,
    registry_card_count: usize,
    card_token_rule: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratorModelConfig {
    schema_version: u64,
    model_architecture_version: String,
    feature_schema_version: String,
    feature_registry_version: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_vocab_size: u64,
    card_embedding_dim: u64,
    hidden_dim: u64,
    state_dim: u64,
    object_feature_dim: u64,
    edge_feature_dim: u64,
    action_feature_dim: u64,
    object_group_count: u64,
    action_ref_feature_dim: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    architecture: String,
    generator_model_config: GeneratorModelConfig,
    generator_model_config_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    file: String,
    encoding: String,
    layout: String,
    bytes: usize,
    sha256: String,
    model_parameter_sha256: String,
    parameter_layout_sha256: String,
    tensor_count: usize,
    element_count: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameter {
    ordinal: usize,
    name: String,
    shape: Vec<usize>,
    byte_offset: usize,
    byte_count: usize,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OptimizerBootstrap {
    optimizer_identity: String,
    adam_step: u64,
    moment_initialization: String,
    scorer_bias_anchor_bits: u32,
    value_head_gauge: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    lineage_id: String,
    initializer: Initializer,
    producer: Producer,
    target: Target,
    model: Model,
    payload: Payload,
    parameters: Vec<Parameter>,
    optimizer_bootstrap: OptimizerBootstrap,
}

fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn actual_pin(pin: &PinnedFileV1) -> Result<(), String> {
    require(
        pin.path.is_absolute() && is_hex(&pin.sha256, 64),
        "fresh source requires absolute SHA256-pinned inputs",
    )
}

fn provenance_absolute(path: &str) -> bool {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return false;
    }
    let bytes = path.as_bytes();
    // A historical Windows drive or UNC path remains valid provenance on Linux.
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    let unc = path.starts_with("\\\\") && {
        let mut components = path[2..].split(['\\', '/']);
        components.next().is_some_and(|s| !s.is_empty())
            && components.next().is_some_and(|s| !s.is_empty())
    };
    path.starts_with('/') || drive || unc
}

fn strict<T: DeserializeOwned>(bytes: &[u8]) -> Result<(T, Value), String> {
    require(
        bytes.len() <= JSON_CAP,
        "fresh initialization JSON exceeds 1 MiB",
    )?;
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let raw = crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
    // Decode original bytes so all nested DTOs enforce integer types and fields.
    Ok((serde_json::from_str(text).map_err(|e| e.to_string())?, raw))
}

/// Python's sorted compact ensure_ascii JSON, without a trailing LF. All
/// numerical manifest fields are integers; floats have no role in this format.
fn canonical(value: &Value) -> Result<Vec<u8>, String> {
    fn quoted(text: &str, output: &mut String) {
        output.push('"');
        for ch in text.chars() {
            match ch {
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                '\u{08}' => output.push_str("\\b"),
                '\u{0c}' => output.push_str("\\f"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                ' '..='~' => output.push(ch),
                _ => {
                    let mut units = [0_u16; 2];
                    for unit in ch.encode_utf16(&mut units) {
                        output.push_str(&format!("\\u{unit:04x}"));
                    }
                }
            }
        }
        output.push('"');
    }
    fn append(value: &Value, output: &mut String) -> Result<(), String> {
        match value {
            Value::Null => output.push_str("null"),
            Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Value::Number(value) => {
                require(
                    value.is_i64() || value.is_u64(),
                    "manifest number must be an integer",
                )?;
                output.push_str(&value.to_string());
            }
            Value::String(value) => quoted(value, output),
            Value::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    append(value, output)?;
                }
                output.push(']');
            }
            Value::Object(values) => {
                let mut entries: Vec<_> = values.iter().collect();
                entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
                output.push('{');
                for (index, (key, value)) in entries.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    quoted(key, output);
                    output.push(':');
                    append(value, output)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
    let mut result = String::new();
    append(value, &mut result)?;
    Ok(result.into_bytes())
}

pub(super) fn parse_source_v1(bytes: &[u8]) -> Result<ExpandedFreshInitializationSourceV1, String> {
    let (source, _): (ExpandedFreshInitializationSourceV1, _) = strict(bytes)?;
    validate_source(&source)?;
    Ok(source)
}

fn validate_source(source: &ExpandedFreshInitializationSourceV1) -> Result<(), String> {
    require(
        source.schema == SOURCE_SCHEMA,
        "unknown fresh initialization source schema",
    )?;
    actual_pin(&source.initialization)?;
    actual_pin(&source.parameters)
}

fn read_actual(pin: &PinnedFileV1, cap: usize) -> Result<Vec<u8>, String> {
    actual_pin(pin)?;
    let file = File::open(&pin.path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    require(
        metadata.is_file() && metadata.len() <= cap as u64,
        "fresh input is not a bounded regular file",
    )?;
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        bytes.len() <= cap && hash(&bytes) == pin.sha256,
        "fresh input bytes differ from their size bound or SHA256 pin",
    )?;
    Ok(bytes)
}

fn validate_metadata(
    manifest: &Manifest,
    features: &FrozenPlayObservationTransferV3,
) -> Result<(), String> {
    require(
        manifest.schema == MANIFEST_SCHEMA,
        "unknown fresh initialization manifest schema",
    )?;
    require(
        manifest.initializer.identity == "trainer-seeded-v1"
            && manifest.initializer.authority
                == "Python KernelPolicyValueNet.reset_seeded_parameters"
            && manifest.initializer.seed_derivation == "kernel-python-rl-trainer-sha256-v2",
        "fresh initializer authority differs",
    )?;
    let target = &manifest.target;
    require(
        provenance_absolute(&target.registry.path)
            && target.registry.sha256 == hash(REGISTRY)
            && target.registry_card_count == CARD_DEFS.len()
            && target.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}")
            && target.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V3
            && target.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V3
            && target.features_source_sha256 == FEATURES_SOURCE_SHA256_V3
            && target.feature_descriptor_sha256 == FEATURE_DESCRIPTOR_SHA256_V3
            && target.card_token_rule == "card-token=id+1; padding=0"
            && features.expected_feature_contract_digest == target.feature_contract_digest
            && features.expected_feature_encoding_digest == target.feature_encoding_digest,
        "fresh target registry, features or token mapping differs from this runtime",
    )?;
    let producer = &manifest.producer;
    require(
        is_hex(&producer.source_git_commit, 40)
            && producer.source_files.len() == SOURCE_PATHS.len(),
        "fresh producer/source inventory differs",
    )?;
    for (pin, expected) in producer.source_files.iter().zip(SOURCE_PATHS) {
        require(
            pin.path == expected
                && is_hex(&pin.sha256, 64)
                && (1..=SOURCE_CAP).contains(&pin.bytes),
            "fresh producer source pin differs",
        )?;
    }
    require(
        producer.source_files[5].sha256 == target.features_source_sha256
            && producer.source_files[6].sha256 == target.feature_descriptor_sha256
            && producer.source_files[7].sha256 == target.registry.sha256,
        "fresh target and producer source pins differ",
    )?;
    let runtime = &producer.runtime;
    for value in [
        &runtime.python_version,
        &runtime.python_implementation,
        &runtime.platform_machine,
        &runtime.torch_version,
    ] {
        require(
            !value.is_empty() && value.chars().count() <= 256,
            "invalid fresh producer runtime string",
        )?;
    }
    require(
        matches!(runtime.platform_system.as_str(), "Windows" | "Linux")
            && runtime.byte_order == "little"
            && runtime.device == "cpu"
            && runtime.dtype == "torch.float32"
            && runtime.deterministic_algorithms
            && runtime.num_threads == 1
            && runtime.num_interop_threads == 1,
        "fresh producer runtime is not the declared CPU float32 contract",
    )?;
    for pin in [
        &runtime.python_executable,
        &runtime.torch_c,
        &runtime.torch_cpu_library,
    ] {
        require(
            provenance_absolute(&pin.path)
                && is_hex(&pin.sha256, 64)
                && (1..=RUNTIME_CAP).contains(&pin.bytes),
            "invalid historical runtime pin",
        )?;
    }
    let config =
        serde_json::to_value(&manifest.model.generator_model_config).map_err(|e| e.to_string())?;
    let config_sha = hash(&canonical(&config)?);
    require(
        manifest.model.architecture == MODEL_ARCHITECTURE_VERSION_V1
            && manifest.model.generator_model_config_sha256 == config_sha
            && config_sha == MODEL_CONFIG_FINGERPRINT_V1,
        "fresh generator Net8 model configuration differs",
    )?;
    let payload = &manifest.payload;
    require(
        payload.file == "parameters.f32le"
            && payload.encoding == "ieee-754-binary32-little-endian"
            && payload.layout
                == "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1"
            && payload.bytes == PAYLOAD_BYTES
            && payload.element_count == PARAMETER_COUNT_V1
            && payload.tensor_count == 33
            && manifest.parameters.len() == 33
            && is_hex(&payload.sha256, 64)
            && is_hex(&payload.model_parameter_sha256, 64)
            && is_hex(&payload.parameter_layout_sha256, 64),
        "fresh payload contract differs",
    )?;
    let optimizer = &manifest.optimizer_bootstrap;
    require(
        optimizer.optimizer_identity == "native-adam-canonical-scorer-bias-gauge-v1"
            && optimizer.adam_step == 0
            && optimizer.moment_initialization == "positive-zero-f32"
            && optimizer.value_head_gauge == "none",
        "fresh optimizer bootstrap differs",
    )
}

pub(super) fn load_policy_v1(
    source: &ExpandedFreshInitializationSourceV1,
    features: &FrozenPlayObservationTransferV3,
) -> Result<FrozenPlayPolicyV1, String> {
    validate_source(source)?;
    let manifest_bytes = read_actual(&source.initialization, JSON_CAP)?;
    let (manifest, raw): (Manifest, _) = strict(&manifest_bytes)?;
    let mut canonical_manifest = canonical(&raw)?;
    canonical_manifest.push(b'\n');
    require(
        canonical_manifest == manifest_bytes,
        "fresh manifest is not canonical ASCII JSON plus LF",
    )?;
    validate_metadata(&manifest, features)?;
    require(
        source.parameters.sha256 == manifest.payload.sha256,
        "fresh descriptor/payload digest differs",
    )?;
    let payload = read_actual(&source.parameters, PAYLOAD_BYTES)?;
    require(
        payload.len() == PAYLOAD_BYTES && PAYLOAD_BYTES == PARAMETER_COUNT_V1 * 4,
        "fresh parameter payload byte count differs",
    )?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|e| e.to_string())?;
    let mut parameters = model.parameter_snapshot_v1();
    require(
        parameters.len() == manifest.parameters.len(),
        "fresh native tensor count differs",
    )?;
    let mut offset = 0usize;
    let mut layout = Vec::with_capacity(parameters.len());
    let mut anchor = None;
    for (ordinal, (parameter, record)) in
        parameters.iter_mut().zip(&manifest.parameters).enumerate()
    {
        let byte_count = parameter
            .values
            .len()
            .checked_mul(4)
            .ok_or("fresh tensor size overflow")?;
        let end = offset
            .checked_add(byte_count)
            .ok_or("fresh tensor offset overflow")?;
        require(
            record.ordinal == ordinal
                && record.name == parameter.name
                && record.shape == parameter.shape
                && record.byte_offset == offset
                && record.byte_count == byte_count
                && end <= payload.len(),
            "fresh tensor name, shape or contiguous byte layout differs",
        )?;
        let bytes = &payload[offset..end];
        require(
            is_hex(&record.sha256, 64) && hash(bytes) == record.sha256,
            "fresh tensor payload hash differs",
        )?;
        for (value, raw) in parameter.values.iter_mut().zip(bytes.chunks_exact(4)) {
            let decoded = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            require(decoded.is_finite(), "fresh parameter is not finite")?;
            *value = decoded;
        }
        if parameter.name == "card_embedding.weight" {
            require(
                parameter.values.len() >= CARD_EMBEDDING_DIM_V1
                    && parameter.values[..CARD_EMBEDDING_DIM_V1]
                        .iter()
                        .all(|v| v.to_bits() == 0),
                "fresh padding embedding row must be positive zero",
            )?;
        }
        if parameter.name == "scorer.2.bias" {
            require(
                parameter.values.len() == 1,
                "fresh scorer gauge tensor differs",
            )?;
            anchor = Some(parameter.values[0].to_bits());
        }
        layout.push(
            serde_json::json!({"ordinal": ordinal, "name": parameter.name,
            "shape": parameter.shape, "byte_offset": offset, "byte_count": byte_count}),
        );
        offset = end;
    }
    require(
        offset == payload.len()
            && anchor == Some(manifest.optimizer_bootstrap.scorer_bias_anchor_bits)
            && hash(&canonical(&Value::Array(layout))?) == manifest.payload.parameter_layout_sha256,
        "fresh layout or sampled scorer anchor differs",
    )?;
    model
        .replace_parameter_snapshot_v1(&parameters)
        .map_err(|e| e.to_string())?;
    require(
        model.parameter_manifest_sha256_v1() == manifest.payload.model_parameter_sha256,
        "fresh named model parameter digest differs",
    )?;
    let identity = FreshPlayPolicyIdentityV1 {
        schema: "mtg-kernel-fresh-play-initialization/v1".into(),
        initialization_manifest_sha256: source.initialization.sha256.clone(),
        lineage_id: manifest.lineage_id,
        initializer: manifest.initializer.identity,
        base_seed: manifest.initializer.base_seed,
        model_init_seed: manifest.initializer.model_init_seed,
        seed_derivation: manifest.initializer.seed_derivation,
        producer_git_commit: manifest.producer.source_git_commit,
        initial_weights_sha256: manifest.payload.sha256,
        initial_model_parameter_sha256: manifest.payload.model_parameter_sha256,
        parameter_layout_sha256: manifest.payload.parameter_layout_sha256,
        destination_registry_sha256: manifest.target.registry.sha256,
        destination_card_db_hash: manifest.target.card_db_hash,
        destination_card_count: manifest.target.registry_card_count,
        feature_contract_digest: manifest.target.feature_contract_digest,
        feature_encoding_digest: manifest.target.feature_encoding_digest,
        features_source_sha256: manifest.target.features_source_sha256,
        feature_descriptor_sha256: manifest.target.feature_descriptor_sha256,
        sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
    };
    // The factory validates seed derivation, initial bytes and current target.
    // The caller creates/restores Adam only after the installed model is valid.
    FrozenPlayPolicyV1::from_fresh_initialization_v1(model, identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_source_rejects_duplicate_nested_unknown_and_wrong_pin_shapes() {
        let path = std::env::temp_dir().join("not-read-fresh-initialization.json");
        let source = serde_json::json!({"schema": SOURCE_SCHEMA,
            "initialization": {"path": path, "sha256": "a".repeat(64)},
            "parameters": {"path": path, "sha256": "b".repeat(64)}});
        let bytes = serde_json::to_vec(&source).unwrap();
        assert!(parse_source_v1(&bytes).is_ok());
        let duplicate = String::from_utf8(bytes).unwrap().replacen(
            "\"sha256\":",
            "\"sha256\":\"duplicate\",\"sha256\":",
            1,
        );
        assert!(parse_source_v1(duplicate.as_bytes()).is_err());
        let mut unknown = source.clone();
        unknown["initialization"]["unchecked"] = Value::Bool(true);
        assert!(parse_source_v1(&serde_json::to_vec(&unknown).unwrap()).is_err());
        let mut relative = source.clone();
        relative["parameters"]["path"] = Value::String("relative.f32le".into());
        assert!(parse_source_v1(&serde_json::to_vec(&relative).unwrap()).is_err());
        let mut uppercase = source;
        uppercase["initialization"]["sha256"] = Value::String("A".repeat(64));
        assert!(parse_source_v1(&serde_json::to_vec(&uppercase).unwrap()).is_err());
        assert!(parse_source_v1(&vec![b' '; JSON_CAP + 1]).is_err());
    }

    #[test]
    fn fresh_canonical_json_matches_ascii_escapes_and_sorted_projection() {
        let value = serde_json::json!({"z": "\u{7f}é😀\n", "a": [1, true, "\\\""]});
        assert_eq!(
            canonical(&value).unwrap(),
            br#"{"a":[1,true,"\\\""],"z":"\u007f\u00e9\ud83d\ude00\n"}"#
        );
        assert!(canonical(&serde_json::json!({"count": 1.0})).is_err());
    }

    #[test]
    fn fresh_historical_paths_are_not_reinterpreted_as_current_host_inputs() {
        for path in [
            "C:/Python/python.exe",
            "C:\\Python\\python.exe",
            "/opt/python/bin/python",
            "\\\\host\\share\\python.exe",
        ] {
            assert!(provenance_absolute(path));
        }
        for path in [
            "",
            "C:python.exe",
            "python.exe",
            "\\only-rooted",
            "\\\\host",
            "bad\0path",
        ] {
            assert!(!provenance_absolute(path));
        }
    }
}
