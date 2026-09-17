//! Strict, read-only import of sampled Net8 initialization bytes.
//!
//! Only the two explicit source pins are opened. Producer file locations are
//! historical provenance, including when producer and consumer use different
//! operating systems. This verifies interchange consistency, not execution of
//! the claimed Python RNG or independent training/convergence.

use super::{FrozenPlayObservationTransferV3, FrozenPlayPolicyV1, PinnedFileV1};
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1;
use crate::native_policy_value_net_v1::{
    NativePolicyValueModelConfigV1, NativePolicyValueNetV1, CARD_EMBEDDING_DIM_V1,
    MODEL_ARCHITECTURE_VERSION_V1, MODEL_CONFIG_FINGERPRINT_V1, PARAMETER_COUNT_V1,
};
use crate::sideboard_play_policy_v1::{
    fresh_lineage_generation_v1, FreshLineageGenerationV1, FreshPlayPolicyIdentityV1,
};
#[cfg(test)]
use crate::native_policy_train_step_v1::native_train_state_parameter_layout_v1;
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
/// V3 (frozen, `features_v6.py`) source pin inventory. Never touched by the
/// V4 admission below; imported/frozen origins stay pinned to this forever.
const SOURCE_PATHS_V3: [&str; 8] = [
    "python/mtg_kernel_rl/phase1_fresh_initialization_v1.py",
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
    "python/mtg_kernel_rl/features_v6.py",
    "data/flat_policy_v3/feature_contract_v3.json",
    "data/cards_v1.json",
];
/// V4 (fresh-lineage, `features_v7.py`) sibling of `SOURCE_PATHS_V3`. Every
/// index but 5 and 6 (the feature-source and feature-contract pins) is
/// identical between the two, by construction of the shared producer script.
const SOURCE_PATHS_V4: [&str; 8] = [
    "python/mtg_kernel_rl/phase1_fresh_initialization_v1.py",
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
    "python/mtg_kernel_rl/features_v7.py",
    "data/flat_policy_v4/feature_contract_v4.json",
    "data/cards_v1.json",
];

fn source_paths_v1(generation: FreshLineageGenerationV1) -> [&'static str; 8] {
    match generation {
        FreshLineageGenerationV1::V3 => SOURCE_PATHS_V3,
        FreshLineageGenerationV1::V4 => SOURCE_PATHS_V4,
    }
}

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
) -> Result<FreshLineageGenerationV1, String> {
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
    // Whole-tuple classification first: matches exactly the compiled V3 or
    // V4 four-field tuple, so a mixed tuple (a V3 contract digest paired
    // with a V4 encoding digest, say) is rejected here before any other
    // target field is even checked, never accepted by a per-field OR.
    let generation = fresh_lineage_generation_v1(
        &target.feature_contract_digest,
        &target.feature_encoding_digest,
        &target.features_source_sha256,
        &target.feature_descriptor_sha256,
    )
    .map_err(|_| "fresh target registry, features or token mapping differs from this runtime")?;
    require(
        provenance_absolute(&target.registry.path)
            && target.registry.sha256 == hash(REGISTRY)
            && target.registry_card_count == CARD_DEFS.len()
            && target.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}")
            && target.card_token_rule == "card-token=id+1; padding=0"
            && features.expected_feature_contract_digest == target.feature_contract_digest
            && features.expected_feature_encoding_digest == target.feature_encoding_digest,
        "fresh target registry, features or token mapping differs from this runtime",
    )?;
    let producer = &manifest.producer;
    let source_paths = source_paths_v1(generation);
    require(
        is_hex(&producer.source_git_commit, 40)
            && producer.source_files.len() == source_paths.len(),
        "fresh producer/source inventory differs",
    )?;
    for (pin, expected) in producer.source_files.iter().zip(source_paths) {
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
    )?;
    Ok(generation)
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
    let generation = validate_metadata(&manifest, features)?;
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
    // `validate_metadata` already classified this manifest's whole feature
    // tuple; dispatch to the matching generation's constructor rather than
    // re-deriving the choice from the identity a second time.
    match generation {
        FreshLineageGenerationV1::V3 => {
            FrozenPlayPolicyV1::from_fresh_initialization_v1(model, identity)
        }
        FreshLineageGenerationV1::V4 => {
            FrozenPlayPolicyV1::from_fresh_initialization_v4(model, identity)
        }
    }
}

/// Builds a real, on-disk fresh-initialization source (a real deterministic
/// Net8 model, real weight/layout/model-config hashes, a canonical
/// manifest) for one compiled generation. This is not RNG or Python
/// producer execution evidence; it proves the Rust admission path (this
/// module plus `FrozenPlayPolicyV1::from_fresh_initialization_v1`/`_v4`)
/// end to end against real bytes.
#[cfg(test)]
pub(super) fn write_synthetic_fresh_source_v1(
    dir: &std::path::Path,
    feature_identity: crate::sideboard_play_policy_v1::FreshFeatureIdentityV1,
) -> ExpandedFreshInitializationSourceV1 {
    write_synthetic_fresh_source_with_parameters_v1(dir, feature_identity, |_parameters| {})
}

/// Sibling of `write_synthetic_fresh_source_v1` that additionally lets a
/// caller mutate the sampled parameters before they are serialized (for
/// example, the same weight-manipulation trick
/// `compact_board_policy`/`compact_board_policy_v4`
/// (`phase1_bo3_collection_v1/tests.rs`) use to make a real game terminate
/// quickly by mutual decking), keeping the model, the payload bytes and
/// every computed hash mutually consistent.
#[cfg(test)]
pub(crate) fn write_synthetic_fresh_source_with_parameters_v1(
    dir: &std::path::Path,
    feature_identity: crate::sideboard_play_policy_v1::FreshFeatureIdentityV1,
    mutate: impl FnOnce(&mut Vec<crate::native_policy_value_net_v1::NativeNamedParameterV1>),
) -> ExpandedFreshInitializationSourceV1 {
    use crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1;
    std::fs::create_dir_all(dir).unwrap();

    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let mut parameters = model.parameter_snapshot_v1();
    mutate(&mut parameters);
    model.replace_parameter_snapshot_v1(&parameters).unwrap();
    let expected: Vec<_> = native_train_state_parameter_layout_v1().collect();
    assert_eq!(parameters.len(), expected.len());

    let mut payload = vec![0u8; PAYLOAD_BYTES];
    let mut offset = 0usize;
    let mut parameter_rows = Vec::with_capacity(parameters.len());
    let mut layout_values = Vec::with_capacity(parameters.len());
    let mut anchor_bits: Option<u32> = None;
    for (ordinal, (parameter, (name, shape))) in parameters.iter().zip(expected).enumerate() {
        assert_eq!(parameter.name, name);
        let byte_count = parameter.values.len() * 4;
        for (i, value) in parameter.values.iter().enumerate() {
            payload[offset + i * 4..offset + i * 4 + 4]
                .copy_from_slice(&value.to_bits().to_le_bytes());
        }
        let row_sha256 = hash(&payload[offset..offset + byte_count]);
        parameter_rows.push(Parameter {
            ordinal,
            name: name.to_string(),
            shape: shape.to_vec(),
            byte_offset: offset,
            byte_count,
            sha256: row_sha256,
        });
        layout_values.push(serde_json::json!({"ordinal": ordinal, "name": name,
            "shape": shape, "byte_offset": offset, "byte_count": byte_count}));
        if name == "scorer.2.bias" {
            anchor_bits = Some(parameter.values[0].to_bits());
        }
        offset += byte_count;
    }
    assert_eq!(offset, PAYLOAD_BYTES);
    let anchor_bits = anchor_bits.expect("scorer.2.bias parameter present");

    let config = NativePolicyValueModelConfigV1::contract_v1();
    let generator_model_config = GeneratorModelConfig {
        schema_version: config.schema_version as u64,
        model_architecture_version: config.model_architecture_version.to_string(),
        feature_schema_version: config.feature_schema_version.to_string(),
        feature_registry_version: config.feature_registry_version.to_string(),
        feature_contract_digest: config.feature_contract_digest.to_string(),
        feature_encoding_digest: config.feature_encoding_digest.to_string(),
        card_vocab_size: config.card_vocab_size as u64,
        card_embedding_dim: config.card_embedding_dim as u64,
        hidden_dim: config.hidden_dim as u64,
        state_dim: config.state_dim as u64,
        object_feature_dim: config.object_feature_dim as u64,
        edge_feature_dim: config.edge_feature_dim as u64,
        action_feature_dim: config.action_feature_dim as u64,
        object_group_count: config.object_group_count as u64,
        action_ref_feature_dim: config.action_ref_feature_dim as u64,
    };
    let generator_model_config_sha256 = hash(
        &canonical(&serde_json::to_value(&generator_model_config).unwrap()).unwrap(),
    );
    assert_eq!(generator_model_config_sha256, MODEL_CONFIG_FINGERPRINT_V1);

    let paths = source_paths_v1(feature_identity.generation);
    let mut source_files = Vec::with_capacity(8);
    for (index, path) in paths.into_iter().enumerate() {
        let sha256 = match index {
            5 => feature_identity.features_source_sha256.to_string(),
            6 => feature_identity.feature_descriptor_sha256.to_string(),
            7 => hash(REGISTRY),
            _ => format!("{:064x}", index + 1),
        };
        source_files.push(SizedHistoricalPin {
            path: path.to_string(),
            sha256,
            bytes: 123,
        });
    }
    let historical_pin = |suffix: &str| SizedHistoricalPin {
        path: format!("C:/synthetic/{suffix}"),
        sha256: "2".repeat(64),
        bytes: 1,
    };

    let manifest = Manifest {
        schema: MANIFEST_SCHEMA.into(),
        lineage_id: "synthetic-fresh-fixture".into(),
        initializer: Initializer {
            identity: "trainer-seeded-v1".into(),
            authority: "Python KernelPolicyValueNet.reset_seeded_parameters".into(),
            base_seed: 0,
            // The one known-good (base_seed, model_init_seed) pair also
            // used by origin.rs's own synthetic fixtures.
            model_init_seed: 6_443_515_232_517_447_393,
            seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
        },
        producer: Producer {
            source_git_commit: "1".repeat(40),
            source_git_clean: true,
            source_files,
            runtime: Runtime {
                python_version: "synthetic".into(),
                python_implementation: "synthetic".into(),
                platform_system: "Windows".into(),
                platform_machine: "synthetic".into(),
                byte_order: "little".into(),
                torch_version: "synthetic".into(),
                device: "cpu".into(),
                dtype: "torch.float32".into(),
                deterministic_algorithms: true,
                num_threads: 1,
                num_interop_threads: 1,
                python_executable: historical_pin("python.exe"),
                torch_c: historical_pin("torch_C.pyd"),
                torch_cpu_library: historical_pin("torch_cpu.dll"),
            },
        },
        target: Target {
            registry: HistoricalPin {
                path: "C:/synthetic/cards.json".into(),
                sha256: hash(REGISTRY),
            },
            card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            feature_contract_digest: feature_identity.feature_contract_digest.into(),
            feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
            features_source_sha256: feature_identity.features_source_sha256.into(),
            feature_descriptor_sha256: feature_identity.feature_descriptor_sha256.into(),
            registry_card_count: CARD_DEFS.len(),
            card_token_rule: "card-token=id+1; padding=0".into(),
        },
        model: Model {
            architecture: MODEL_ARCHITECTURE_VERSION_V1.into(),
            generator_model_config,
            generator_model_config_sha256,
        },
        payload: Payload {
            file: "parameters.f32le".into(),
            encoding: "ieee-754-binary32-little-endian".into(),
            layout: "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1".into(),
            bytes: PAYLOAD_BYTES,
            sha256: hash(&payload),
            model_parameter_sha256: model.parameter_manifest_sha256_v1(),
            parameter_layout_sha256: hash(&canonical(&Value::Array(layout_values)).unwrap()),
            tensor_count: 33,
            element_count: PARAMETER_COUNT_V1,
        },
        parameters: parameter_rows,
        optimizer_bootstrap: OptimizerBootstrap {
            optimizer_identity: "native-adam-canonical-scorer-bias-gauge-v1".into(),
            adam_step: 0,
            moment_initialization: "positive-zero-f32".into(),
            scorer_bias_anchor_bits: anchor_bits,
            value_head_gauge: "none".into(),
        },
    };

    let manifest_value = serde_json::to_value(&manifest).unwrap();
    let mut manifest_bytes = canonical(&manifest_value).unwrap();
    manifest_bytes.push(b'\n');

    let manifest_path = dir.join("initialization.json");
    std::fs::write(&manifest_path, &manifest_bytes).unwrap();
    let parameters_path = dir.join("parameters.f32le");
    std::fs::write(&parameters_path, &payload).unwrap();

    ExpandedFreshInitializationSourceV1 {
        schema: SOURCE_SCHEMA.into(),
        initialization: PinnedFileV1 {
            path: manifest_path.canonicalize().unwrap(),
            sha256: hash(&manifest_bytes),
        },
        parameters: PinnedFileV1 {
            path: parameters_path.canonicalize().unwrap(),
            sha256: hash(&payload),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard_play_policy_v1::{FRESH_FEATURE_IDENTITY_V3, FRESH_FEATURE_IDENTITY_V4};

    #[test]
    fn fresh_v4_manifest_is_admitted_and_activates_fresh_successor() {
        let dir = std::env::temp_dir().join(format!(
            "fresh-v4-source-{}-{}",
            std::process::id(),
            "admitted"
        ));
        let source = write_synthetic_fresh_source_v1(&dir, FRESH_FEATURE_IDENTITY_V4);
        let features = FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FRESH_FEATURE_IDENTITY_V4
                .feature_contract_digest
                .into(),
            expected_feature_encoding_digest: FRESH_FEATURE_IDENTITY_V4
                .feature_encoding_digest
                .into(),
        };
        let policy = load_policy_v1(&source, &features).unwrap();
        assert_eq!(
            policy.identity_v1().feature_contract_digest_v1(),
            FRESH_FEATURE_IDENTITY_V4.feature_contract_digest
        );
        assert_eq!(
            policy.identity_v1().feature_encoding_digest_v1(),
            FRESH_FEATURE_IDENTITY_V4.feature_encoding_digest
        );
        // The V4 arm activates `fresh_successor`, never `successor`: proven
        // indirectly here (in-module private fields are not visible from
        // this test) by the wide sampler identity both generations share,
        // and directly in sideboard_play_policy_v1.rs's own dispatch tests.
        assert_eq!(
            policy.runtime_sampler_identity_v1(),
            crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1
        );
    }

    #[test]
    fn fresh_v3_manifest_still_admitted_and_a_v4_declared_manifest_is_rejected_by_v3_only_fields() {
        let dir = std::env::temp_dir().join(format!(
            "fresh-v3-source-{}-{}",
            std::process::id(),
            "admitted"
        ));
        let source = write_synthetic_fresh_source_v1(&dir, FRESH_FEATURE_IDENTITY_V3);
        let features = FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FRESH_FEATURE_IDENTITY_V3
                .feature_contract_digest
                .into(),
            expected_feature_encoding_digest: FRESH_FEATURE_IDENTITY_V3
                .feature_encoding_digest
                .into(),
        };
        assert!(load_policy_v1(&source, &features).is_ok());

        // A mixed tuple (V4 contract+source pins, but a V3-declared
        // features/descriptor pair) must be rejected, never accepted by
        // checking each field independently against "V3 or V4".
        let mixed_dir = std::env::temp_dir().join(format!(
            "fresh-mixed-source-{}-{}",
            std::process::id(),
            "rejected"
        ));
        let mut mixed_identity = FRESH_FEATURE_IDENTITY_V4;
        mixed_identity.feature_encoding_digest = FRESH_FEATURE_IDENTITY_V3.feature_encoding_digest;
        std::fs::create_dir_all(&mixed_dir).unwrap();
        // validate_metadata's whole-tuple classifier rejects this target
        // before any file is even read; parse the manifest bytes directly.
        let bad_target = serde_json::json!({"registry":{"path":"C:/synthetic/cards.json",
            "sha256": hash(REGISTRY)}, "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
            "feature_contract_digest": mixed_identity.feature_contract_digest,
            "feature_encoding_digest": mixed_identity.feature_encoding_digest,
            "features_source_sha256": mixed_identity.features_source_sha256,
            "feature_descriptor_sha256": mixed_identity.feature_descriptor_sha256,
            "registry_card_count": CARD_DEFS.len(), "card_token_rule": "card-token=id+1; padding=0"});
        let target: Target = serde_json::from_value(bad_target).unwrap();
        let features = FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: target.feature_contract_digest.clone(),
            expected_feature_encoding_digest: target.feature_encoding_digest.clone(),
        };
        let manifest = Manifest {
            schema: MANIFEST_SCHEMA.into(),
            lineage_id: "synthetic-mixed-fixture".into(),
            initializer: Initializer {
                identity: "trainer-seeded-v1".into(),
                authority: "Python KernelPolicyValueNet.reset_seeded_parameters".into(),
                base_seed: 0,
                model_init_seed: 6_443_515_232_517_447_393,
                seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
            },
            producer: Producer {
                source_git_commit: "1".repeat(40),
                source_git_clean: true,
                source_files: source_paths_v1(FreshLineageGenerationV1::V4)
                    .into_iter()
                    .enumerate()
                    .map(|(index, path)| SizedHistoricalPin {
                        path: path.to_string(),
                        sha256: match index {
                            5 => target.features_source_sha256.clone(),
                            6 => target.feature_descriptor_sha256.clone(),
                            7 => target.registry.sha256.clone(),
                            _ => format!("{:064x}", index + 1),
                        },
                        bytes: 123,
                    })
                    .collect(),
                runtime: Runtime {
                    python_version: "synthetic".into(),
                    python_implementation: "synthetic".into(),
                    platform_system: "Windows".into(),
                    platform_machine: "synthetic".into(),
                    byte_order: "little".into(),
                    torch_version: "synthetic".into(),
                    device: "cpu".into(),
                    dtype: "torch.float32".into(),
                    deterministic_algorithms: true,
                    num_threads: 1,
                    num_interop_threads: 1,
                    python_executable: SizedHistoricalPin {
                        path: "C:/synthetic/python.exe".into(),
                        sha256: "2".repeat(64),
                        bytes: 1,
                    },
                    torch_c: SizedHistoricalPin {
                        path: "C:/synthetic/torch_C.pyd".into(),
                        sha256: "2".repeat(64),
                        bytes: 1,
                    },
                    torch_cpu_library: SizedHistoricalPin {
                        path: "C:/synthetic/torch_cpu.dll".into(),
                        sha256: "2".repeat(64),
                        bytes: 1,
                    },
                },
            },
            target,
            model: Model {
                architecture: MODEL_ARCHITECTURE_VERSION_V1.into(),
                generator_model_config: GeneratorModelConfig {
                    schema_version: 0,
                    model_architecture_version: String::new(),
                    feature_schema_version: String::new(),
                    feature_registry_version: String::new(),
                    feature_contract_digest: String::new(),
                    feature_encoding_digest: String::new(),
                    card_vocab_size: 0,
                    card_embedding_dim: 0,
                    hidden_dim: 0,
                    state_dim: 0,
                    object_feature_dim: 0,
                    edge_feature_dim: 0,
                    action_feature_dim: 0,
                    object_group_count: 0,
                    action_ref_feature_dim: 0,
                },
                generator_model_config_sha256: "0".repeat(64),
            },
            payload: Payload {
                file: "parameters.f32le".into(),
                encoding: "ieee-754-binary32-little-endian".into(),
                layout: "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1".into(),
                bytes: PAYLOAD_BYTES,
                sha256: "0".repeat(64),
                model_parameter_sha256: "0".repeat(64),
                parameter_layout_sha256: "0".repeat(64),
                tensor_count: 33,
                element_count: PARAMETER_COUNT_V1,
            },
            parameters: Vec::new(),
            optimizer_bootstrap: OptimizerBootstrap {
                optimizer_identity: "native-adam-canonical-scorer-bias-gauge-v1".into(),
                adam_step: 0,
                moment_initialization: "positive-zero-f32".into(),
                scorer_bias_anchor_bits: 0,
                value_head_gauge: "none".into(),
            },
        };
        assert!(validate_metadata(&manifest, &features).is_err());
        let _ = mixed_dir; // no files needed: validate_metadata rejects first.
    }

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
