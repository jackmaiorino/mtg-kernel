//! Inference-only transport of a checkpoint already validated through a Store.
//! Fixed filenames and independent metadata/model pins keep provenance separate
//! from the original search identity. This module creates no Store or train state.
//!
//! Port of the historical producer (introduced in 8a2b720c, built at 18f4ca51)
//! for the line (a) archival panel imports. The metadata wire schema, parameter
//! encoding and layout, and every validation are unchanged, and the metadata
//! names this exporter's own compiled build. Only the legacy validated-Store
//! source route is ported; the search-distillation method route is refused.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1,
};
use crate::expanded_deck_training_v1::{load_expanded_inference_v1, ExpandedModelSourceV1};
use crate::fast_sampler::{
    FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256, FAST_CATEGORICAL_SAMPLER_VERSION,
};
use crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
use crate::native_checkpoint_shadow_stdio_v1::{
    load_checkpoint_v1, LoadedShadowCheckpointV1, ShadowCheckpointAuthorityV1,
    ShadowCheckpointIdentityV1, FIXED_MAX_PHYSICAL_DECISIONS_V1, FIXED_MAX_POLICY_STEPS_V1,
    POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1, SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
};
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    FEATURE_CONTRACT_DIGEST_V1, FEATURE_ENCODING_DIGEST_V1, MODEL_ARCHITECTURE_VERSION_V1,
    MODEL_CONFIG_FINGERPRINT_V1,
};
use crate::native_train_state_payload_v1::{
    decode_section_v1, encode_section_v1, NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
    NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::paired_bo1_harness_v1::PlayPolicyGenerationV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyImportV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const SCHEMA_V1: &str = "mtg-kernel-native-inference-export/v1";
const METADATA_FILENAME_V1: &str = "metadata.json";
const MODEL_FILENAME_V1: &str = "parameters.f32le";
const METADATA_MAX_BYTES_V1: usize = 1_048_576;
const ENCODING_V1: &str = "native-train-state-parameters-section-f32le/v1";
const EXPORT_VALIDATION_V1: &str =
    "full-store-chain-checkpoint-payload-and-inference-validated-by-exporter";
const READER_VALIDATION_V1: [&str; 6] = [
    "independently-pinned-canonical-metadata-and-original-source-route",
    "exact-4923976-byte-parameter-section-and-raw-sha256",
    "compiled-layout-architecture-config-and-feature-contracts",
    "transactional-finite-parameters-and-zero-padding-row",
    "recomputed-named-model-sha256",
    "preserved-original-checkpoint-identity-and-decision-limits",
];
const UNPORTED_METHOD_V1: &str =
    "the search-distillation method route is not part of this exporter port";

/// Wire form of the historical `TtsS2StoreKindV1`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStoreKindV1 {
    Original,
    Population,
    Portable,
}

/// Wire form of the historical `TtsS2CheckpointRefV1`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExportCheckpointRefV1 {
    pub store_kind: ExportStoreKindV1,
    pub store_root: PathBuf,
    pub generation: u64,
    pub expected_model_sha256: String,
}

/// Wire form of the historical `GeneralizationCheckpointMethodV1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportCheckpointMethodV1 {
    LegacyValidatedStore,
    SearchDistillationV1,
    InferenceExportV1,
}

/// Wire form of the historical `GeneralizationExportSourceMethodV1`: the
/// original Store route stays explicit when inference bytes use an export.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportSourceMethodV1 {
    LegacyValidatedStore,
    SearchDistillationV1,
}

/// Wire form of the historical `GeneralizationCheckpointV1`, the export's
/// `source` record and the exporter's reference input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportCheckpointSourceV1 {
    pub method: ExportCheckpointMethodV1,
    pub checkpoint: ExportCheckpointRefV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_design_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_method_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_auxiliary_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology_qualification: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_export_metadata_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_source_method: Option<ExportSourceMethodV1>,
}

/// Kept separately from ShadowCheckpointIdentityV1, so transport cannot reseed search.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct InferenceExportReceiptV1 {
    pub(crate) schema: &'static str,
    pub(crate) metadata_sha256: String,
    pub(crate) parameter_section_sha256: String,
    pub(crate) exporter_build: Value,
    pub(crate) reader_build: Value,
    pub(crate) source_validation: &'static str,
    pub(crate) reader_validation: [&'static str; 6],
    pub(crate) reader_revalidated_store_chain: bool,
}

/// Historical method-producer receipt. Retained so the metadata shape stays
/// unchanged; the legacy route requires it to be absent.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckpointMethodProducerV1 {
    pub(crate) method_sha256: String,
    pub(crate) design_sha256: String,
    pub(crate) method_contract_sha256: String,
    pub(crate) writer_build: Value,
    pub(crate) writer_executable_sha256: String,
}

/// An export read back into the unchanged shadow-checkpoint form. The
/// transport receipt stays beside it, never inside checkpoint authority.
pub(crate) struct LoadedInferenceExportV1 {
    pub(crate) checkpoint: LoadedShadowCheckpointV1,
    pub(crate) receipt: InferenceExportReceiptV1,
}

/// Owned wire counterpart. Static runtime strings are checked, never leaked or
/// interned from input. Every original identity field is serialized unchanged.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExportIdentityV1 {
    authority_kind: String,
    source_run_sha256: String,
    source_generation: u64,
    source_checkpoint_sha256: String,
    source_sidecar_sha256: String,
    source_payload_sha256: String,
    source_train_state_sha256: String,
    loaded_run_sha256: String,
    loaded_generation: u64,
    loaded_checkpoint_sha256: String,
    loaded_payload_sha256: String,
    loaded_train_state_sha256: String,
    model_parameter_sha256: String,
    environment_trajectory_contract: String,
    sampler_identity: String,
    sampler_contract_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExportMetadataV1 {
    schema: String,
    /// Original path is provenance only. The reader never opens it, including
    /// when its Windows spelling is not a native absolute path on Linux.
    source: ExportCheckpointSourceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    method_producer: Option<CheckpointMethodProducerV1>,
    identity: ExportIdentityV1,
    parameter_encoding: String,
    parameter_byte_count: usize,
    parameter_section_sha256: String,
    architecture_identity: String,
    model_config_fingerprint: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    exporter_build: Value,
    source_validation: String,
}

fn canonical_v1<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    to_canonical_json_bytes_v1(value, CanonicalJsonNullPolicyV1::Forbid).map_err(|e| e.to_string())
}

fn hash_v1(bytes: &[u8]) -> String {
    lower_hex_raw32_v1(sha256_v1(bytes))
}

fn digest_valid_v1(value: &str) -> bool {
    parse_lower_hex_raw32_v1(value).is_ok()
}

fn require_v1(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn read_bounded_v1(path: &Path, exact: Option<usize>, cap: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    require_v1(
        metadata.is_file() && metadata.len() <= cap as u64,
        "export input must be a bounded regular file",
    )?;
    if let Some(size) = exact {
        require_v1(
            metadata.len() == size as u64,
            "export parameter length differs",
        )?;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require_v1(
        bytes.len() <= cap && exact.is_none_or(|size| bytes.len() == size),
        "export input length changed or exceeds its bound",
    )?;
    Ok(bytes)
}

/// The historical `validate_checkpoint_v1`, restricted to the ported routes.
fn validate_source_v1(reference: &ExportCheckpointSourceV1) -> Result<(), String> {
    require_v1(
        reference.checkpoint.store_root.is_absolute()
            && digest_valid_v1(&reference.checkpoint.expected_model_sha256),
        "checkpoint requires an absolute Store path and exact model SHA256",
    )?;
    match reference.method {
        ExportCheckpointMethodV1::LegacyValidatedStore => require_v1(
            reference.expected_design_sha256.is_none()
                && reference.expected_method_sha256.is_none()
                && reference.max_auxiliary_bytes.is_none()
                && reference.topology_qualification.is_none()
                && reference.expected_export_metadata_sha256.is_none()
                && reference.export_source_method.is_none(),
            "legacy checkpoint must omit search-method inputs",
        ),
        ExportCheckpointMethodV1::SearchDistillationV1 => Err(UNPORTED_METHOD_V1.into()),
        ExportCheckpointMethodV1::InferenceExportV1 => require_v1(
            reference.export_source_method == Some(ExportSourceMethodV1::LegacyValidatedStore)
                && reference.expected_design_sha256.is_none()
                && reference.expected_method_sha256.is_none()
                && reference.checkpoint.store_kind != ExportStoreKindV1::Portable
                && reference
                    .expected_export_metadata_sha256
                    .as_deref()
                    .is_some_and(digest_valid_v1)
                && reference.max_auxiliary_bytes.is_none()
                && reference.topology_qualification.is_none(),
            "inference export requires independent metadata/model and original source method pins; Store loading inputs are forbidden",
        ),
    }
}

fn source_method_v1(source: &ExportCheckpointSourceV1) -> Result<ExportSourceMethodV1, String> {
    require_v1(source.checkpoint.store_kind != ExportStoreKindV1::Portable,
        "inference export requires an original or population full Store; weights-genesis transport is not admitted")?;
    match source.method {
        ExportCheckpointMethodV1::LegacyValidatedStore => {
            Ok(ExportSourceMethodV1::LegacyValidatedStore)
        }
        ExportCheckpointMethodV1::SearchDistillationV1 => Err(UNPORTED_METHOD_V1.into()),
        ExportCheckpointMethodV1::InferenceExportV1 => {
            Err("export source must be an actually validated Store, not another export".into())
        }
    }
}

fn export_reference_v1(
    source: &ExportCheckpointSourceV1,
    directory: &Path,
    metadata_sha256: String,
) -> Result<ExportCheckpointSourceV1, String> {
    let mut reference = source.clone();
    reference.method = ExportCheckpointMethodV1::InferenceExportV1;
    reference.checkpoint.store_root = directory.to_owned();
    reference.max_auxiliary_bytes = None;
    reference.topology_qualification = None;
    reference.expected_export_metadata_sha256 = Some(metadata_sha256);
    reference.export_source_method = Some(source_method_v1(source)?);
    Ok(reference)
}

fn checked_identity_v1(
    metadata: &ExportMetadataV1,
    expected: &ExportCheckpointSourceV1,
) -> Result<ShadowCheckpointIdentityV1, String> {
    let source = &metadata.source;
    let id = &metadata.identity;
    let source_method = source_method_v1(source)?;
    require_v1(
        expected.export_source_method == Some(source_method)
            && source.checkpoint.store_kind == expected.checkpoint.store_kind
            && source.checkpoint.generation == expected.checkpoint.generation
            && source.checkpoint.expected_model_sha256 == expected.checkpoint.expected_model_sha256
            && source.expected_design_sha256 == expected.expected_design_sha256
            && source.expected_method_sha256 == expected.expected_method_sha256
            && source.expected_export_metadata_sha256.is_none()
            && source.export_source_method.is_none()
            && !source.checkpoint.store_root.as_os_str().is_empty(),
        "export source differs from independent expected route, generation or pins",
    )?;
    require_v1(
        source.expected_design_sha256.is_none()
            && source.expected_method_sha256.is_none()
            && source.max_auxiliary_bytes.is_none()
            && source.topology_qualification.is_none()
            && metadata.method_producer.is_none(),
        "legacy export provenance contains method-only fields",
    )?;
    let (kind, environment) = match source.checkpoint.store_kind {
        ExportStoreKindV1::Population => (
            "population-store-validated-generation",
            POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
        ),
        ExportStoreKindV1::Original => (
            "original-promoted2-validated-store-generation",
            SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
        ),
        ExportStoreKindV1::Portable => {
            return Err("unsupported original export authority route".into())
        }
    };
    for pin in [
        &id.source_run_sha256,
        &id.source_checkpoint_sha256,
        &id.source_sidecar_sha256,
        &id.source_payload_sha256,
        &id.source_train_state_sha256,
        &id.loaded_run_sha256,
        &id.loaded_checkpoint_sha256,
        &id.loaded_payload_sha256,
        &id.loaded_train_state_sha256,
        &id.model_parameter_sha256,
    ] {
        parse_lower_hex_raw32_v1(pin).map_err(|_| "export identity digest invalid")?;
    }
    require_v1(
        id.authority_kind == kind
            && id.loaded_generation == expected.checkpoint.generation
            && id.model_parameter_sha256 == expected.checkpoint.expected_model_sha256
            && id.environment_trajectory_contract == environment
            && id.sampler_identity == FAST_CATEGORICAL_SAMPLER_VERSION
            && id.sampler_contract_sha256 == FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        "export original checkpoint identity or inference contract differs",
    )?;
    require_v1(
        id.source_run_sha256 == id.loaded_run_sha256
            && id.source_generation == id.loaded_generation
            && id.source_checkpoint_sha256 == id.loaded_checkpoint_sha256
            && id.source_payload_sha256 == id.loaded_payload_sha256
            && id.source_train_state_sha256 == id.loaded_train_state_sha256,
        "validated Store source and loaded identities differ",
    )?;
    Ok(ShadowCheckpointIdentityV1 {
        authority_kind: id.authority_kind.clone(),
        source_run_sha256: id.source_run_sha256.clone(),
        source_generation: id.source_generation,
        source_checkpoint_sha256: id.source_checkpoint_sha256.clone(),
        source_sidecar_sha256: id.source_sidecar_sha256.clone(),
        source_payload_sha256: id.source_payload_sha256.clone(),
        source_train_state_sha256: id.source_train_state_sha256.clone(),
        loaded_run_sha256: id.loaded_run_sha256.clone(),
        loaded_generation: id.loaded_generation,
        loaded_checkpoint_sha256: id.loaded_checkpoint_sha256.clone(),
        loaded_payload_sha256: id.loaded_payload_sha256.clone(),
        loaded_train_state_sha256: id.loaded_train_state_sha256.clone(),
        model_parameter_sha256: id.model_parameter_sha256.clone(),
        environment_trajectory_contract: environment,
        sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    })
}

fn decode_export_v1(
    metadata_bytes: &[u8],
    parameters: &[u8],
    expected: &ExportCheckpointSourceV1,
    reader_build: Value,
) -> Result<LoadedInferenceExportV1, String> {
    validate_source_v1(expected)?;
    require_v1(
        expected.method == ExportCheckpointMethodV1::InferenceExportV1,
        "explicit inference export method required",
    )?;
    let metadata_sha256 = hash_v1(metadata_bytes);
    require_v1(
        metadata_bytes.len() <= METADATA_MAX_BYTES_V1
            && expected.expected_export_metadata_sha256.as_deref() == Some(&metadata_sha256),
        "export metadata size or independent digest differs",
    )?;
    let metadata: ExportMetadataV1 =
        from_canonical_json_bytes_v1(metadata_bytes, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|e| e.to_string())?;
    validate_exporter_build_v1(&metadata.exporter_build)?;
    require_v1(
        metadata.schema == SCHEMA_V1
            && metadata.parameter_encoding == ENCODING_V1
            && metadata.parameter_byte_count == NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1
            && parameters.len() == NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1
            && metadata.parameter_section_sha256 == hash_v1(parameters)
            && metadata.architecture_identity == MODEL_ARCHITECTURE_VERSION_V1
            && metadata.model_config_fingerprint == MODEL_CONFIG_FINGERPRINT_V1
            && metadata.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V1
            && metadata.feature_encoding_digest == FEATURE_ENCODING_DIGEST_V1
            && metadata.max_physical_decisions == FIXED_MAX_PHYSICAL_DECISIONS_V1
            && metadata.max_policy_steps == FIXED_MAX_POLICY_STEPS_V1
            && metadata.source_validation == EXPORT_VALIDATION_V1,
        "export parameter bytes, limits or compiled inference contracts differ",
    )?;
    let identity = checked_identity_v1(&metadata, expected)?;
    let parameters = decode_section_v1(parameters).map_err(|e| e.to_string())?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|e| e.to_string())?;
    model
        .replace_parameter_snapshot_v1(&parameters)
        .map_err(|e| e.to_string())?;
    let digest = |value: &str| parse_lower_hex_raw32_v1(value).map_err(|e| e.to_string());
    let inference = NativeCheckpointInferenceV1::from_inference_export_v1(
        model,
        digest(&identity.loaded_run_sha256)?,
        digest(&identity.loaded_checkpoint_sha256)?,
        digest(&identity.loaded_payload_sha256)?,
        digest(&identity.loaded_train_state_sha256)?,
        digest(&identity.model_parameter_sha256)?,
        identity.loaded_generation,
    )
    .map_err(|e| e.to_string())?;
    Ok(LoadedInferenceExportV1 {
        checkpoint: LoadedShadowCheckpointV1 {
            inference,
            identity,
            max_physical_decisions: metadata.max_physical_decisions,
            max_policy_steps: metadata.max_policy_steps,
        },
        receipt: InferenceExportReceiptV1 {
            schema: "mtg-kernel-native-inference-export-read-receipt/v1",
            metadata_sha256,
            parameter_section_sha256: metadata.parameter_section_sha256,
            exporter_build: metadata.exporter_build,
            reader_build,
            source_validation: EXPORT_VALIDATION_V1,
            reader_validation: READER_VALIDATION_V1,
            reader_revalidated_store_chain: false,
        },
    })
}

pub(crate) fn load_inference_export_v1(
    expected: &ExportCheckpointSourceV1,
) -> Result<LoadedInferenceExportV1, String> {
    // A supported production build is mandatory before any payload is opened.
    let reader_build = current_build_v1()?;
    validate_source_v1(expected)?;
    require_v1(
        expected.method == ExportCheckpointMethodV1::InferenceExportV1,
        "explicit inference export method required",
    )?;
    let root = &expected.checkpoint.store_root;
    let metadata = read_bounded_v1(
        &root.join(METADATA_FILENAME_V1),
        None,
        METADATA_MAX_BYTES_V1,
    )?;
    // Verify the independent metadata pin before reading the larger fixed file.
    require_v1(
        expected.expected_export_metadata_sha256.as_deref() == Some(&hash_v1(&metadata)),
        "export independent metadata digest differs",
    )?;
    let parameters = read_bounded_v1(
        &root.join(MODEL_FILENAME_V1),
        Some(NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1),
        NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
    )?;
    decode_export_v1(&metadata, &parameters, expected, reader_build)
}

/// Strict Store loading for an export source: the historical
/// `load_generalization_checkpoint_v1` legacy route and `load_model_v1`
/// authority mapping. No weight-only, recovery or alternate authority path.
pub(crate) fn load_store_checkpoint_v1(
    source: &ExportCheckpointSourceV1,
) -> Result<LoadedShadowCheckpointV1, String> {
    validate_source_v1(source)?;
    source_method_v1(source)?;
    let checkpoint = &source.checkpoint;
    // Same fixed marker as the historical legacy route: a config label cannot
    // route a new-method Store through the legacy reader.
    let parent = checkpoint
        .store_root
        .parent()
        .ok_or("Store root has no parent for method context")?;
    require_v1(
        !parent
            .join("search-distillation-continuation.json")
            .try_exists()
            .map_err(|e| e.to_string())?,
        "new-method continuation present; its route is not part of this exporter port",
    )?;
    let authority = match checkpoint.store_kind {
        ExportStoreKindV1::Original => {
            ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration {
                root: checkpoint.store_root.clone(),
                generation: checkpoint.generation,
            }
        }
        ExportStoreKindV1::Population => ShadowCheckpointAuthorityV1::PopulationStoreGeneration {
            root: checkpoint.store_root.clone(),
            generation: checkpoint.generation,
        },
        ExportStoreKindV1::Portable => {
            return Err("weights-genesis transport is not admitted for export".into())
        }
    };
    let loaded = load_checkpoint_v1(authority).map_err(|e| e.to_string())?;
    require_v1(
        loaded.identity.loaded_generation == checkpoint.generation
            && loaded.identity.model_parameter_sha256 == checkpoint.expected_model_sha256,
        "loaded checkpoint does not match its generation and model hash",
    )?;
    Ok(loaded)
}

fn encode_loaded_v1(
    loaded: &LoadedShadowCheckpointV1,
    source: &ExportCheckpointSourceV1,
    exporter_build: Value,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    validate_source_v1(source)?;
    source_method_v1(source)?;
    let net = loaded.inference.search_model_v1();
    require_v1(
        net.parameter_manifest_sha256_v1() == source.checkpoint.expected_model_sha256
            && loaded.identity.model_parameter_sha256 == source.checkpoint.expected_model_sha256
            && loaded.identity.loaded_generation == source.checkpoint.generation
            && lower_hex_raw32_v1(loaded.inference.model_parameter_sha256())
                == loaded.identity.model_parameter_sha256
            && lower_hex_raw32_v1(loaded.inference.run_sha256())
                == loaded.identity.loaded_run_sha256
            && lower_hex_raw32_v1(loaded.inference.checkpoint_manifest_sha256())
                == loaded.identity.loaded_checkpoint_sha256
            && lower_hex_raw32_v1(loaded.inference.checkpoint_payload_sha256())
                == loaded.identity.loaded_payload_sha256
            && lower_hex_raw32_v1(loaded.inference.train_state_sha256())
                == loaded.identity.loaded_train_state_sha256
            && loaded.inference.generation_index() == loaded.identity.loaded_generation,
        "validated export model differs from independent source pins",
    )?;
    let identity: ExportIdentityV1 =
        serde_json::from_value(serde_json::to_value(&loaded.identity).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut parameters = Vec::with_capacity(NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1);
    encode_section_v1(&mut parameters, &net.parameter_snapshot_v1());
    require_v1(
        parameters.len() == NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
        "export parameter encoding has unexpected size",
    )?;
    let metadata = ExportMetadataV1 {
        schema: SCHEMA_V1.into(),
        source: source.clone(),
        identity,
        method_producer: None,
        parameter_encoding: ENCODING_V1.into(),
        parameter_byte_count: parameters.len(),
        parameter_section_sha256: hash_v1(&parameters),
        architecture_identity: net.architecture_identity_v1().into(),
        model_config_fingerprint: MODEL_CONFIG_FINGERPRINT_V1.into(),
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1.into(),
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1.into(),
        max_physical_decisions: loaded.max_physical_decisions,
        max_policy_steps: loaded.max_policy_steps,
        exporter_build,
        source_validation: EXPORT_VALIDATION_V1.into(),
    };
    let bytes = canonical_v1(&metadata)?;
    require_v1(
        bytes.len() <= METADATA_MAX_BYTES_V1,
        "export metadata exceeds bound",
    )?;
    Ok((bytes, parameters))
}

fn write_exclusive_v1(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

fn write_loaded_v1(
    loaded: &LoadedShadowCheckpointV1,
    source: &ExportCheckpointSourceV1,
    output_directory: &Path,
    exporter_build: Value,
) -> Result<Value, String> {
    require_v1(
        output_directory.is_absolute(),
        "export output directory must be absolute",
    )?;
    let (metadata, parameters) = encode_loaded_v1(loaded, source, exporter_build.clone())?;
    let reference = export_reference_v1(source, output_directory, hash_v1(&metadata))?;
    // Decode before publication, then independently read both committed files.
    let checked = decode_export_v1(&metadata, &parameters, &reference, exporter_build)?;
    require_v1(
        checked.checkpoint.identity == loaded.identity,
        "export changed original checkpoint identity",
    )?;
    fs::create_dir(output_directory).map_err(|e| e.to_string())?;
    write_exclusive_v1(&output_directory.join(MODEL_FILENAME_V1), &parameters)?;
    write_exclusive_v1(&output_directory.join(METADATA_FILENAME_V1), &metadata)?;
    let readback = load_inference_export_v1(&reference)?;
    require_v1(
        readback.checkpoint.identity == loaded.identity
            && readback.checkpoint.inference.model_parameter_sha256()
                == loaded.inference.model_parameter_sha256(),
        "export readback identity differs",
    )?;
    Ok(
        json!({"schema":"mtg-kernel-native-inference-export-completion/v1",
        "reference":reference, "source_identity":readback.checkpoint.identity,
        "metadata_bytes":metadata.len(), "parameter_bytes":parameters.len(),
        "readback_receipt":readback.receipt}),
    )
}

/// The only production producer of export bytes. It performs the existing
/// full Store validation before anything is written, into a fresh directory.
pub fn export_native_checkpoint_v1(
    reference_path: &Path,
    output_directory: &Path,
) -> Result<Value, String> {
    let exporter_build = current_build_v1()?;
    require_v1(
        reference_path.is_absolute() && output_directory.is_absolute(),
        "export reference and output paths must be absolute",
    )?;
    require_v1(
        !output_directory.try_exists().map_err(|e| e.to_string())?,
        "export output directory must be fresh",
    )?;
    let bytes = read_bounded_v1(reference_path, None, METADATA_MAX_BYTES_V1)?;
    let source: ExportCheckpointSourceV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    source_method_v1(&source)?;
    let loaded = load_store_checkpoint_v1(&source)?;
    write_loaded_v1(&loaded, &source, output_directory, exporter_build)
}

fn tensors_bit_identical_v1(
    left: &[NativeNamedParameterV1],
    right: &[NativeNamedParameterV1],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.name == b.name
                && a.shape == b.shape
                && a.values.len() == b.values.len()
                && a.values
                    .iter()
                    .zip(&b.values)
                    .all(|(x, y)| x.to_bits() == y.to_bits())
        })
}

/// Round-trip acceptance for one exported archival member. The Store is
/// loaded through the strict route and its payload file is re-read and hashed
/// independently; the bundle is imported through the evaluator's own expanded
/// inference loader (FrozenPlayPolicyImportV1 plus the explicit V3 transfer).
/// Every named tensor must be bit-identical. Nothing is written.
pub fn roundtrip_check_v1(
    reference_path: &Path,
    model_source_path: &Path,
) -> Result<Value, String> {
    require_v1(
        reference_path.is_absolute() && model_source_path.is_absolute(),
        "round-trip inputs must be absolute paths",
    )?;
    let reference_bytes = read_bounded_v1(reference_path, None, METADATA_MAX_BYTES_V1)?;
    let source: ExportCheckpointSourceV1 =
        serde_json::from_slice(&reference_bytes).map_err(|e| e.to_string())?;
    let store = load_store_checkpoint_v1(&source)?;
    let payload_path = source
        .checkpoint
        .store_root
        .join("checkpoints")
        .join(format!(
            "update-{:08}.state.f32le",
            source.checkpoint.generation
        ));
    let payload = read_bounded_v1(
        &payload_path,
        Some(NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1),
        NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
    )?;
    require_v1(
        hash_v1(&payload) == store.identity.loaded_payload_sha256,
        "Store payload file differs from the validated checkpoint payload",
    )?;
    let section = &payload[..NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1];
    let model_source_bytes = read_bounded_v1(model_source_path, None, METADATA_MAX_BYTES_V1)?;
    let model_source: ExpandedModelSourceV1 =
        serde_json::from_slice(&model_source_bytes).map_err(|e| e.to_string())?;
    require_v1(
        model_source.checkpoint.is_none(),
        "archival imports are inference-only; a successor checkpoint is not admitted",
    )?;
    let import_bytes =
        read_bounded_v1(&model_source.play_import.path, None, METADATA_MAX_BYTES_V1)?;
    require_v1(
        hash_v1(&import_bytes) == model_source.play_import.sha256,
        "import descriptor SHA differs",
    )?;
    let import: FrozenPlayPolicyImportV1 =
        serde_json::from_slice(&import_bytes).map_err(|e| e.to_string())?;
    let bundle = read_bounded_v1(
        &import.export_directory.join(MODEL_FILENAME_V1),
        Some(NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1),
        NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
    )?;
    let (policy, identity) = load_expanded_inference_v1(&model_source)?;
    require_v1(
        policy.uses_observation_successor_v3()
            && policy.feature_generation_v1() == PlayPolicyGenerationV1::V3,
        "imported member is not an explicitly V3-transferred policy",
    )?;
    let stored = store.inference.search_model_v1().parameter_snapshot_v1();
    let imported = policy.training_parameters_v3();
    let decoded_bundle = decode_section_v1(&bundle).map_err(|e| e.to_string())?;
    let decoded_section = decode_section_v1(section).map_err(|e| e.to_string())?;
    let actual = policy.actual_model_identity_v1();
    let expected_model = &source.checkpoint.expected_model_sha256;
    let checks = json!({
        "raw_bundle_equals_store_parameter_section": bundle.as_slice() == section,
        "tensors_bit_identical_imported_vs_store": tensors_bit_identical_v1(&imported, &stored),
        "tensors_bit_identical_bundle_vs_store": tensors_bit_identical_v1(&decoded_bundle, &stored),
        "tensors_bit_identical_payload_section_vs_store": tensors_bit_identical_v1(&decoded_section, &stored),
        "store_model_parameter_sha256_equals_reference": &store.identity.model_parameter_sha256 == expected_model,
        "imported_model_parameter_sha256_equals_reference": &actual.model_parameter_sha256 == expected_model
            && &identity.model.model_parameter_sha256 == expected_model,
        "imported_weights_sha256_equals_parameter_section_sha256": actual.weights_sha256 == hash_v1(section),
    });
    let failed: Vec<&String> = checks
        .as_object()
        .ok_or("round-trip checks must form an object")?
        .iter()
        .filter(|(_, passed)| **passed != Value::Bool(true))
        .map(|(name, _)| name)
        .collect();
    require_v1(failed.is_empty(), &format!("round trip failed: {failed:?}"))?;
    Ok(json!({"schema":"mtg-kernel-line-a-panel-roundtrip/v1",
        "reference_sha256":hash_v1(&reference_bytes),
        "model_source_sha256":hash_v1(&model_source_bytes),
        "import_descriptor_sha256":model_source.play_import.sha256,
        "store_identity":store.identity,
        "store_payload_path":payload_path,
        "parameter_section_sha256":hash_v1(section),
        "tensor_count":stored.len(),
        "parameter_count":stored.iter().map(|p| p.values.len()).sum::<usize>(),
        "inference_identity":identity,
        "checks":checks,
        "passed":true}))
}

#[cfg(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
))]
fn current_build_v1() -> Result<Value, String> {
    let bytes =
        crate::native_store_production_capture_v2::current_launcher_build_identity_json_v2()
            .map_err(|e| e.to_string())?;
    serde_json::from_str(&bytes).map_err(|e| e.to_string())
}

#[cfg(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
))]
fn validate_exporter_build_v1(value: &Value) -> Result<(), String> {
    use crate::native_store_production_capture_v2::{
        LauncherBuildIdentityV2, LAUNCHER_BUILD_IDENTITY_SCHEMA_V2,
    };
    // Typed decoding rejects unknown build fields. Cross-platform transport
    // deliberately does not assert equality of host, target or feature list.
    let build: LauncherBuildIdentityV2 =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    require_v1(
        build.schema == LAUNCHER_BUILD_IDENTITY_SCHEMA_V2
            && build.package_name == "mtg-kernel"
            && build.build_profile == "release"
            && build
                .enabled_features
                .iter()
                .any(|f| f == "native-training-store-v2-production")
            && build.source_git_commit.len() == 40
            && build
                .source_git_commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "exporter production build identity invalid",
    )?;
    for pin in [
        &build.workspace_manifest_sha256,
        &build.crate_manifest_sha256,
        &build.cargo_lock_sha256,
        &build.rustc_verbose_version_sha256,
        &build.source_tree_recipe_sha256,
        &build.source_tree_sha256,
    ] {
        parse_lower_hex_raw32_v1(pin).map_err(|_| "exporter build digest invalid")?;
    }
    Ok(())
}

#[cfg(not(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
)))]
fn current_build_v1() -> Result<Value, String> {
    Err("inference export requires a supported release production build".into())
}

#[cfg(not(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
)))]
fn validate_exporter_build_v1(_value: &Value) -> Result<(), String> {
    Err("inference export requires a supported release production build".into())
}

#[cfg(all(
    test,
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
))]
#[path = "native_checkpoint_export_v1_tests.rs"]
mod tests;
