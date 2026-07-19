//! Strict loader for the Python-authoritative common initial-model snapshot.
//!
//! The snapshot is not a checkpoint and does not claim that Rust reproduces
//! Torch initialization. All parsing, hashing, model construction, and Adam
//! bootstrap work completes on a private candidate before live-state replacement.

use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainStateV1, CANONICAL_GAUGE_PARAMETERS_V1, NATIVE_OPTIMIZER_IDENTITY_V1,
};
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    CARD_EMBEDDING_DIM_V1, FEATURE_CONTRACT_DIGEST_V1, FEATURE_ENCODING_DIGEST_V1,
    FEATURE_REGISTRY_VERSION_V1, FEATURE_SCHEMA_VERSION_V1, MODEL_ARCHITECTURE_VERSION_V1,
    MODEL_CONFIG_FINGERPRINT_V1, MODEL_CONFIG_SCHEMA_VERSION_V1, PARAMETER_COUNT_V1,
};
use crate::native_trainer_schedule_v1::{
    derive_native_trainer_model_init_seed_v1, NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
    NATIVE_TRAINER_SCHEDULE_VERSION_V1, PYTHON_REFERENCE_SEED_VERSION_V1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(crate) const SNAPSHOT_SCHEMA_V1: &str = "mtg-kernel-common-model-snapshot/v1";
pub(crate) const SNAPSHOT_IDENTITY_V1: &str =
    "mtg-kernel-python-authoritative-common-model-snapshot-v1";
pub(crate) const SNAPSHOT_PURPOSE_V1: &str = "matched-throughput-trial-initial-model-only";
pub(crate) const RUST_LOADER_IDENTITY_V1: &str = "mtg-kernel-rust-common-model-snapshot-loader-v1";
pub(crate) const INITIALIZER_AUTHORITY_V1: &str =
    "Python KernelPolicyValueNet.reset_seeded_parameters";
pub(crate) const INITIALIZER_IDENTITY_V1: &str = "trainer-seeded-v1";
pub(crate) const AUTHORITY_RUNTIME_IDENTITY_V1: &str =
    "python-torch-windows-amd64-python3.13.14-torch2.13.0+cpu-cpu-f32-deterministic-threads1-v1";
pub(crate) const BASE_SEED_V1: u64 = 0;
pub(crate) const MODEL_INIT_SEED_V1: u64 = 6_443_515_232_517_447_393;
pub(crate) const PARAMETER_TENSOR_COUNT_V1: usize = 33;
pub(crate) const PARAMETER_ELEMENT_COUNT_V1: usize = 1_230_994;
pub(crate) const PAYLOAD_BYTE_COUNT_V1: usize = 4_923_976;
pub(crate) const MANIFEST_MAX_BYTES_V1: usize = 64 * 1024;
pub(crate) const PAYLOAD_MAX_BYTES_V1: usize = 8 * 1024 * 1024;
const PAYLOAD_ENCODING_V1: &str = "ieee-754-binary32-little-endian";
const PAYLOAD_LAYOUT_V1: &str =
    "torch-named-parameters-c-contiguous-row-major-linear-output-input-no-padding-v1";
const MOMENT_INITIALIZATION_V1: &str = "positive-zero-f32";
const VALUE_HEAD_GAUGE_V1: &str = "none";
const SOURCE_BUNDLE_CONTRACT_V1: &str =
    "sha256(repeated(frame(source-relative-path,raw32(source-sha256))))";
pub(crate) const NONCLAIM_V1: &str =
    "Rust does not reproduce the Python trainer-seeded-v1 initializer in this snapshot configuration; the snapshot proves bit-exact initial parameters only and does not establish seeded-initializer parity, cross-runtime numerical bit parity, learning parity, or speedup.";
const LEGACY_OPTIMIZER_NONCLAIM_V1: &str =
    "The legacy Python-v3 optimizer is not the matched optimizer lane because it retains accidental scorer-bias gauge drift.";

const AUTHORITY_SOURCE_PATHS_V1: [&str; 4] = [
    "python/mtg_kernel_rl/model.py",
    "python/mtg_kernel_rl/features.py",
    "python/mtg_kernel_rl/determinism.py",
    "python/mtg_kernel_rl/common_model_snapshot_v1.py",
];

const AUTHORITY_SOURCE_BYTES_V1: [&[u8]; 4] = [
    include_bytes!("../../python/mtg_kernel_rl/model.py"),
    include_bytes!("../../python/mtg_kernel_rl/features.py"),
    include_bytes!("../../python/mtg_kernel_rl/determinism.py"),
    include_bytes!("../../python/mtg_kernel_rl/common_model_snapshot_v1.py"),
];

#[derive(Clone, Copy)]
struct ExpectedParameterV1 {
    name: &'static str,
    shape: &'static [u64],
    element_offset: u64,
    element_count: u64,
}

const EXPECTED_PARAMETER_LAYOUT_V1: [ExpectedParameterV1; PARAMETER_TENSOR_COUNT_V1] = [
    ExpectedParameterV1 {
        name: "card_embedding.weight",
        shape: &[65537, 16],
        element_offset: 0,
        element_count: 1048592,
    },
    ExpectedParameterV1 {
        name: "object_encoder.0.weight",
        shape: &[64, 114],
        element_offset: 1048592,
        element_count: 7296,
    },
    ExpectedParameterV1 {
        name: "object_encoder.0.bias",
        shape: &[64],
        element_offset: 1055888,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "object_encoder.2.weight",
        shape: &[64, 64],
        element_offset: 1055952,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "object_encoder.2.bias",
        shape: &[64],
        element_offset: 1060048,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "edge_encoder.0.weight",
        shape: &[64, 169],
        element_offset: 1060112,
        element_count: 10816,
    },
    ExpectedParameterV1 {
        name: "edge_encoder.0.bias",
        shape: &[64],
        element_offset: 1070928,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "edge_encoder.2.weight",
        shape: &[64, 64],
        element_offset: 1070992,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "edge_encoder.2.bias",
        shape: &[64],
        element_offset: 1075088,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "node_update.0.weight",
        shape: &[64, 128],
        element_offset: 1075152,
        element_count: 8192,
    },
    ExpectedParameterV1 {
        name: "node_update.0.bias",
        shape: &[64],
        element_offset: 1083344,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "node_update.2.weight",
        shape: &[64, 64],
        element_offset: 1083408,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "node_update.2.bias",
        shape: &[64],
        element_offset: 1087504,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "state_encoder.0.weight",
        shape: &[64, 1499],
        element_offset: 1087568,
        element_count: 95936,
    },
    ExpectedParameterV1 {
        name: "state_encoder.0.bias",
        shape: &[64],
        element_offset: 1183504,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "state_encoder.2.weight",
        shape: &[64, 64],
        element_offset: 1183568,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "state_encoder.2.bias",
        shape: &[64],
        element_offset: 1187664,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "action_ref_encoder.0.weight",
        shape: &[64, 89],
        element_offset: 1187728,
        element_count: 5696,
    },
    ExpectedParameterV1 {
        name: "action_ref_encoder.0.bias",
        shape: &[64],
        element_offset: 1193424,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "action_ref_encoder.2.weight",
        shape: &[64, 64],
        element_offset: 1193488,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "action_ref_encoder.2.bias",
        shape: &[64],
        element_offset: 1197584,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "action_encoder.0.weight",
        shape: &[64, 259],
        element_offset: 1197648,
        element_count: 16576,
    },
    ExpectedParameterV1 {
        name: "action_encoder.0.bias",
        shape: &[64],
        element_offset: 1214224,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "action_encoder.2.weight",
        shape: &[64, 64],
        element_offset: 1214288,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "action_encoder.2.bias",
        shape: &[64],
        element_offset: 1218384,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "scorer.0.weight",
        shape: &[64, 128],
        element_offset: 1218448,
        element_count: 8192,
    },
    ExpectedParameterV1 {
        name: "scorer.0.bias",
        shape: &[64],
        element_offset: 1226640,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "scorer.2.weight",
        shape: &[1, 64],
        element_offset: 1226704,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "scorer.2.bias",
        shape: &[1],
        element_offset: 1226768,
        element_count: 1,
    },
    ExpectedParameterV1 {
        name: "value_head.0.weight",
        shape: &[64, 64],
        element_offset: 1226769,
        element_count: 4096,
    },
    ExpectedParameterV1 {
        name: "value_head.0.bias",
        shape: &[64],
        element_offset: 1230865,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "value_head.2.weight",
        shape: &[1, 64],
        element_offset: 1230929,
        element_count: 64,
    },
    ExpectedParameterV1 {
        name: "value_head.2.bias",
        shape: &[1],
        element_offset: 1230993,
        element_count: 1,
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommonModelSnapshotErrorV1(String);

impl CommonModelSnapshotErrorV1 {
    fn contract(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for CommonModelSnapshotErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "common model snapshot v1 error: {}", self.0)
    }
}

impl Error for CommonModelSnapshotErrorV1 {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ManifestV1 {
    schema: String,
    identity: String,
    purpose: String,
    model: ModelBindingV1,
    initializer: InitializerBindingV1,
    authority: AuthorityBindingV1,
    payload: PayloadBindingV1,
    optimizer_bootstrap: OptimizerBootstrapV1,
    parameters: Vec<ParameterEntryV1>,
    integrity: IntegrityBindingV1,
    nonclaims: NonclaimsV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ModelBindingV1 {
    feature_contract_digest: String,
    feature_encoding_digest: String,
    model_architecture_version: String,
    model_config: ModelConfigBindingV1,
    model_config_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ModelConfigBindingV1 {
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InitializerBindingV1 {
    authority: String,
    base_seed: u64,
    identity: String,
    model_init_seed: u64,
    python_reference_seed_version: String,
    schedule_goldens_sha256: String,
    trainer_schedule_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthorityBindingV1 {
    runtime_configuration: AuthorityRuntimeConfigurationV1,
    runtime_configuration_sha256: String,
    runtime_identity: String,
    source_bundle_contract: String,
    source_bundle_sha256: String,
    sources: Vec<AuthoritySourceV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthorityRuntimeConfigurationV1 {
    byte_order: String,
    device: String,
    platform_machine: String,
    platform_system: String,
    python_version: String,
    torch_default_dtype: String,
    torch_deterministic_algorithms: bool,
    torch_num_interop_threads: u64,
    torch_num_threads: u64,
    torch_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AuthoritySourceV1 {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PayloadBindingV1 {
    buffers: Vec<Value>,
    encoding: String,
    layout: String,
    parameter_element_count: u64,
    parameter_tensor_count: u64,
    payload_byte_count: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OptimizerBootstrapV1 {
    adam_step: u64,
    canonical_gauge_parameters: Vec<String>,
    moment_initialization: String,
    optimizer_identity: String,
    scorer_bias_anchor_f32_bits: u64,
    value_head_gauge: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ParameterEntryV1 {
    byte_count: u64,
    byte_offset: u64,
    element_count: u64,
    element_offset: u64,
    name: String,
    ordinal: u64,
    shape: Vec<u64>,
    tensor_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IntegrityBindingV1 {
    manifest_core_sha256: String,
    named_parameter_stream_sha256: String,
    parameter_layout_sha256: String,
    snapshot_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NonclaimsV1 {
    independent_gates: Vec<String>,
    legacy_optimizer: String,
    scope: String,
}

#[derive(Clone, Debug)]
struct ValidatedSnapshotV1 {
    manifest: ManifestV1,
    manifest_file_sha256: String,
    named_parameters: Vec<NativeNamedParameterV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommonModelSnapshotRecordV1 {
    pub(crate) schema: String,
    pub(crate) identity: String,
    pub(crate) snapshot_sha256: String,
    pub(crate) manifest_file_sha256: String,
    pub(crate) manifest_core_sha256: String,
    pub(crate) payload_sha256: String,
    pub(crate) payload_byte_count: u64,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) named_parameter_stream_sha256: String,
    pub(crate) loaded_named_parameter_stream_sha256: String,
    pub(crate) parameter_tensor_count: u64,
    pub(crate) parameter_element_count: u64,
    pub(crate) model_config_fingerprint: String,
    pub(crate) model_architecture_version: String,
    pub(crate) feature_contract_digest: String,
    pub(crate) feature_encoding_digest: String,
    pub(crate) initializer_identity: String,
    pub(crate) base_seed: u64,
    pub(crate) model_init_seed: u64,
    pub(crate) trainer_schedule_version: String,
    pub(crate) python_reference_seed_version: String,
    pub(crate) schedule_goldens_sha256: String,
    pub(crate) authority_source_bundle_sha256: String,
    pub(crate) authority_runtime_identity: String,
    pub(crate) loader_identity: String,
    pub(crate) optimizer_identity: String,
    pub(crate) adam_step_initial: u64,
    pub(crate) moment_initialization: String,
    pub(crate) canonical_gauge_parameters: Vec<String>,
    pub(crate) scorer_bias_anchor_f32_bits: u64,
    pub(crate) snapshot_load_completed_before_trial_start: bool,
    pub(crate) snapshot_load_timed: bool,
    pub(crate) rust_seeded_initializer_reproduced: bool,
    pub(crate) nonclaim: String,
}

impl CommonModelSnapshotRecordV1 {
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }

    pub fn manifest_file_sha256(&self) -> &str {
        &self.manifest_file_sha256
    }

    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }

    pub fn payload_byte_count(&self) -> u64 {
        self.payload_byte_count
    }

    pub fn parameter_layout_sha256(&self) -> &str {
        &self.parameter_layout_sha256
    }

    pub fn model_config_fingerprint(&self) -> &str {
        &self.model_config_fingerprint
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn frame(tag: &str, bytes: &[u8]) -> Result<Vec<u8>, CommonModelSnapshotErrorV1> {
    if tag.is_empty() || !tag.is_ascii() {
        return Err(CommonModelSnapshotErrorV1::contract(
            "digest frame tag must be nonempty ASCII",
        ));
    }
    let tag_length = u32::try_from(tag.len())
        .map_err(|_| CommonModelSnapshotErrorV1::contract("digest frame tag overflow"))?;
    let byte_length = u64::try_from(bytes.len())
        .map_err(|_| CommonModelSnapshotErrorV1::contract("digest frame payload overflow"))?;
    let mut framed = Vec::with_capacity(4 + tag.len() + 8 + bytes.len());
    framed.extend_from_slice(&tag_length.to_be_bytes());
    framed.extend_from_slice(tag.as_bytes());
    framed.extend_from_slice(&byte_length.to_be_bytes());
    framed.extend_from_slice(bytes);
    Ok(framed)
}

fn decode_lower_hex_32(value: &str) -> Result<[u8; 32], CommonModelSnapshotErrorV1> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "digest must be 64 lowercase hexadecimal characters",
        ));
    }
    let mut result = [0u8; 32];
    for (position, output) in result.iter_mut().enumerate() {
        let high = hex_nibble(value.as_bytes()[position * 2]);
        let low = hex_nibble(value.as_bytes()[position * 2 + 1]);
        *output = high << 4 | low;
    }
    Ok(result)
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("hex alphabet checked"),
    }
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, CommonModelSnapshotErrorV1> {
    serde_json::to_vec(value)
        .map_err(|error| CommonModelSnapshotErrorV1::contract(format!("canonical JSON: {error}")))
}

fn require_ascii_strings(value: &Value) -> Result<(), CommonModelSnapshotErrorV1> {
    match value {
        Value::String(string) if !string.is_ascii() => Err(CommonModelSnapshotErrorV1::contract(
            "manifest strings must be ASCII",
        )),
        Value::Array(values) => values.iter().try_for_each(require_ascii_strings),
        Value::Object(values) => {
            if values.keys().any(|key| !key.is_ascii()) {
                return Err(CommonModelSnapshotErrorV1::contract(
                    "manifest keys must be ASCII",
                ));
            }
            values.values().try_for_each(require_ascii_strings)
        }
        _ => Ok(()),
    }
}

#[cfg(windows)]
fn metadata_identity(metadata: &Metadata) -> Vec<u64> {
    use std::os::windows::fs::MetadataExt;
    vec![
        u64::from(metadata.file_attributes()),
        metadata.creation_time(),
        metadata.last_write_time(),
        metadata.file_size(),
    ]
}

#[cfg(unix)]
fn metadata_identity(metadata: &Metadata) -> Vec<u64> {
    use std::os::unix::fs::MetadataExt;
    vec![
        metadata.dev(),
        metadata.ino(),
        metadata.mode().into(),
        metadata.nlink(),
        metadata.size(),
        metadata.mtime() as u64,
        metadata.mtime_nsec() as u64,
        metadata.ctime() as u64,
        metadata.ctime_nsec() as u64,
    ]
}

#[cfg(not(any(windows, unix)))]
fn metadata_identity(metadata: &Metadata) -> Vec<u64> {
    use std::time::UNIX_EPOCH;
    vec![
        metadata.len(),
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or_default(),
    ]
}

#[cfg(windows)]
fn is_reparse(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_metadata: &Metadata) -> bool {
    false
}

#[cfg(windows)]
fn file_handle_identity(file: &File) -> Result<Vec<u64>, CommonModelSnapshotErrorV1> {
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTimeV1 {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformationV1 {
        file_attributes: u32,
        creation_time: FileTimeV1,
        last_access_time: FileTimeV1,
        last_write_time: FileTimeV1,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformationV1,
        ) -> i32;
    }

    let mut information = MaybeUninit::<ByHandleFileInformationV1>::uninit();
    // SAFETY: `file` is a live owned handle, and Windows writes exactly one
    // BY_HANDLE_FILE_INFORMATION value to the valid out pointer on success.
    let success = unsafe {
        GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
    };
    if success == 0 {
        return Err(CommonModelSnapshotErrorV1::contract(format!(
            "cannot query Windows file identity: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: the successful API call initialized the whole structure.
    let information = unsafe { information.assume_init() };
    Ok(vec![
        u64::from(information.volume_serial_number),
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low),
        u64::from(information.number_of_links),
        (u64::from(information.file_size_high) << 32) | u64::from(information.file_size_low),
    ])
}

#[cfg(not(windows))]
fn file_handle_identity(file: &File) -> Result<Vec<u64>, CommonModelSnapshotErrorV1> {
    file.metadata()
        .map(|metadata| metadata_identity(&metadata))
        .map_err(|error| {
            CommonModelSnapshotErrorV1::contract(format!("cannot query file identity: {error}"))
        })
}

fn capture_regular_file(
    path: &Path,
    maximum_bytes: usize,
) -> Result<Vec<u8>, CommonModelSnapshotErrorV1> {
    capture_regular_file_with_hook(path, maximum_bytes, || Ok(()))
}

fn capture_regular_file_with_hook(
    path: &Path,
    maximum_bytes: usize,
    after_read: impl FnOnce() -> Result<(), CommonModelSnapshotErrorV1>,
) -> Result<Vec<u8>, CommonModelSnapshotErrorV1> {
    let path_before = fs::symlink_metadata(path).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("cannot stat {}: {error}", path.display()))
    })?;
    if !path_before.file_type().is_file()
        || path_before.file_type().is_symlink()
        || is_reparse(&path_before)
    {
        return Err(CommonModelSnapshotErrorV1::contract(format!(
            "snapshot path is not a regular non-link file: {}",
            path.display()
        )));
    }
    let maximum_u64 = u64::try_from(maximum_bytes)
        .map_err(|_| CommonModelSnapshotErrorV1::contract("file cap overflow"))?;
    if path_before.len() > maximum_u64 {
        return Err(CommonModelSnapshotErrorV1::contract(format!(
            "snapshot file exceeds allocation cap: {}",
            path.display()
        )));
    }
    let mut file = File::open(path).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("cannot open {}: {error}", path.display()))
    })?;
    let opened_before = file.metadata().map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!(
            "cannot inspect open {}: {error}",
            path.display()
        ))
    })?;
    if !opened_before.is_file()
        || metadata_identity(&opened_before) != metadata_identity(&path_before)
    {
        return Err(CommonModelSnapshotErrorV1::contract(format!(
            "snapshot file changed before capture: {}",
            path.display()
        )));
    }
    let handle_identity_before = file_handle_identity(&file)?;
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("cannot seek {}: {error}", path.display()))
    })?;
    let mut bytes = Vec::with_capacity(path_before.len() as usize);
    (&mut file)
        .take(maximum_u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            CommonModelSnapshotErrorV1::contract(format!("cannot read {}: {error}", path.display()))
        })?;
    if bytes.len() > maximum_bytes {
        return Err(CommonModelSnapshotErrorV1::contract(format!(
            "snapshot file exceeds allocation cap: {}",
            path.display()
        )));
    }
    after_read()?;
    let opened_after = file.metadata().map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!(
            "cannot re-inspect open {}: {error}",
            path.display()
        ))
    })?;
    let path_after = fs::symlink_metadata(path).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!(
            "snapshot file disappeared during capture {}: {error}",
            path.display()
        ))
    })?;
    let handle_identity_after = file_handle_identity(&file)?;
    let path_handle_after = File::open(path).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!(
            "cannot reopen snapshot file {}: {error}",
            path.display()
        ))
    })?;
    let path_handle_identity_after = file_handle_identity(&path_handle_after)?;
    if metadata_identity(&opened_before) != metadata_identity(&opened_after)
        || metadata_identity(&opened_after) != metadata_identity(&path_after)
        || handle_identity_before != handle_identity_after
        || handle_identity_after != path_handle_identity_after
        || !path_after.file_type().is_file()
        || path_after.file_type().is_symlink()
        || is_reparse(&path_after)
        || opened_after.len() != bytes.len() as u64
    {
        return Err(CommonModelSnapshotErrorV1::contract(format!(
            "snapshot file changed during capture: {}",
            path.display()
        )));
    }
    Ok(bytes)
}

fn expected_authority_sources() -> (Vec<AuthoritySourceV1>, String) {
    let mut framed = Vec::new();
    let mut sources = Vec::new();
    for (path, bytes) in AUTHORITY_SOURCE_PATHS_V1
        .iter()
        .zip(AUTHORITY_SOURCE_BYTES_V1)
    {
        let digest = Sha256::digest(bytes);
        sources.push(AuthoritySourceV1 {
            path: (*path).to_owned(),
            sha256: format!("{digest:x}"),
        });
        framed.extend_from_slice(&frame(path, &digest).expect("frozen source path is valid"));
    }
    (sources, sha256_hex(&framed))
}

fn expected_runtime_configuration() -> AuthorityRuntimeConfigurationV1 {
    AuthorityRuntimeConfigurationV1 {
        byte_order: "little".to_owned(),
        device: "cpu".to_owned(),
        platform_machine: "AMD64".to_owned(),
        platform_system: "Windows".to_owned(),
        python_version: "3.13.14".to_owned(),
        torch_default_dtype: "torch.float32".to_owned(),
        torch_deterministic_algorithms: true,
        torch_num_interop_threads: 1,
        torch_num_threads: 1,
        torch_version: "2.13.0+cpu".to_owned(),
    }
}

fn require_exact_model_config(
    config: &ModelConfigBindingV1,
) -> Result<(), CommonModelSnapshotErrorV1> {
    let expected = NativePolicyValueModelConfigV1::contract_v1();
    macro_rules! require {
        ($actual:expr, $expected:expr, $name:literal) => {
            if $actual != $expected {
                return Err(CommonModelSnapshotErrorV1::contract(concat!(
                    "model config mismatch: ",
                    $name
                )));
            }
        };
    }
    require!(
        config.schema_version,
        MODEL_CONFIG_SCHEMA_VERSION_V1 as u64,
        "schema_version"
    );
    require!(
        config.model_architecture_version,
        MODEL_ARCHITECTURE_VERSION_V1,
        "model_architecture_version"
    );
    require!(
        config.feature_schema_version,
        FEATURE_SCHEMA_VERSION_V1,
        "feature_schema_version"
    );
    require!(
        config.feature_registry_version,
        FEATURE_REGISTRY_VERSION_V1,
        "feature_registry_version"
    );
    require!(
        config.feature_contract_digest,
        FEATURE_CONTRACT_DIGEST_V1,
        "feature_contract_digest"
    );
    require!(
        config.feature_encoding_digest,
        FEATURE_ENCODING_DIGEST_V1,
        "feature_encoding_digest"
    );
    require!(
        config.card_vocab_size,
        expected.card_vocab_size as u64,
        "card_vocab_size"
    );
    require!(
        config.card_embedding_dim,
        expected.card_embedding_dim as u64,
        "card_embedding_dim"
    );
    require!(config.hidden_dim, expected.hidden_dim as u64, "hidden_dim");
    require!(config.state_dim, expected.state_dim as u64, "state_dim");
    require!(
        config.object_feature_dim,
        expected.object_feature_dim as u64,
        "object_feature_dim"
    );
    require!(
        config.edge_feature_dim,
        expected.edge_feature_dim as u64,
        "edge_feature_dim"
    );
    require!(
        config.action_feature_dim,
        expected.action_feature_dim as u64,
        "action_feature_dim"
    );
    require!(
        config.object_group_count,
        expected.object_group_count as u64,
        "object_group_count"
    );
    require!(
        config.action_ref_feature_dim,
        expected.action_ref_feature_dim as u64,
        "action_ref_feature_dim"
    );
    let value = serde_json::to_value(config).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("model config serialization: {error}"))
    })?;
    if sha256_hex(&canonical_json_bytes(&value)?) != MODEL_CONFIG_FINGERPRINT_V1 {
        return Err(CommonModelSnapshotErrorV1::contract(
            "model config fingerprint is internally inconsistent",
        ));
    }
    Ok(())
}

fn layout_projection(manifest: &ManifestV1) -> Value {
    let parameters = manifest
        .parameters
        .iter()
        .map(|entry| {
            json!({
                "byte_count": entry.byte_count,
                "byte_offset": entry.byte_offset,
                "element_count": entry.element_count,
                "element_offset": entry.element_offset,
                "name": entry.name,
                "ordinal": entry.ordinal,
                "shape": entry.shape,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "buffers": [],
        "encoding": PAYLOAD_ENCODING_V1,
        "layout": PAYLOAD_LAYOUT_V1,
        "parameter_element_count": PARAMETER_ELEMENT_COUNT_V1,
        "parameter_tensor_count": PARAMETER_TENSOR_COUNT_V1,
        "parameters": parameters,
        "payload_byte_count": PAYLOAD_BYTE_COUNT_V1,
    })
}

fn named_parameter_stream_sha256(
    parameters: &[ParameterEntryV1],
    payload: &[u8],
) -> Result<String, CommonModelSnapshotErrorV1> {
    let mut digest = Sha256::new();
    for entry in parameters {
        let name_length = u32::try_from(entry.name.len())
            .map_err(|_| CommonModelSnapshotErrorV1::contract("parameter name length overflow"))?;
        let rank = u32::try_from(entry.shape.len())
            .map_err(|_| CommonModelSnapshotErrorV1::contract("parameter rank overflow"))?;
        digest.update(name_length.to_be_bytes());
        digest.update(entry.name.as_bytes());
        digest.update(rank.to_be_bytes());
        for dimension in &entry.shape {
            digest.update(dimension.to_be_bytes());
        }
        digest.update(entry.element_count.to_be_bytes());
        let begin = usize::try_from(entry.byte_offset)
            .map_err(|_| CommonModelSnapshotErrorV1::contract("parameter byte offset overflow"))?;
        let count = usize::try_from(entry.byte_count)
            .map_err(|_| CommonModelSnapshotErrorV1::contract("parameter byte count overflow"))?;
        let end = begin
            .checked_add(count)
            .ok_or_else(|| CommonModelSnapshotErrorV1::contract("parameter byte end overflow"))?;
        let slice = payload.get(begin..end).ok_or_else(|| {
            CommonModelSnapshotErrorV1::contract("parameter slice outside payload")
        })?;
        digest.update(slice);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn manifest_core_sha256(manifest: &ManifestV1) -> Result<String, CommonModelSnapshotErrorV1> {
    let mut core = serde_json::to_value(manifest).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("manifest core serialization: {error}"))
    })?;
    let integrity = core
        .get_mut("integrity")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| CommonModelSnapshotErrorV1::contract("integrity object missing"))?;
    integrity.remove("manifest_core_sha256");
    integrity.remove("snapshot_sha256");
    let canonical = canonical_json_bytes(&core)?;
    Ok(sha256_hex(&frame(
        "mtg-kernel-common-model-snapshot-v1/manifest-core",
        &canonical,
    )?))
}

fn snapshot_sha256(core: &str, payload: &str) -> Result<String, CommonModelSnapshotErrorV1> {
    let mut framed = frame(
        "mtg-kernel-common-model-snapshot-v1/manifest-core-sha256",
        &decode_lower_hex_32(core)?,
    )?;
    framed.extend_from_slice(&frame(
        "mtg-kernel-common-model-snapshot-v1/payload-sha256",
        &decode_lower_hex_32(payload)?,
    )?);
    Ok(sha256_hex(&framed))
}

fn validate_snapshot_bytes(
    manifest_file: &[u8],
    payload: &[u8],
) -> Result<ValidatedSnapshotV1, CommonModelSnapshotErrorV1> {
    if manifest_file.is_empty() || manifest_file.len() > MANIFEST_MAX_BYTES_V1 {
        return Err(CommonModelSnapshotErrorV1::contract(
            "manifest size is outside the bounded contract",
        ));
    }
    if payload.len() != PAYLOAD_BYTE_COUNT_V1 || payload.len() > PAYLOAD_MAX_BYTES_V1 {
        return Err(CommonModelSnapshotErrorV1::contract(
            "payload has the wrong exact size",
        ));
    }
    let manifest: ManifestV1 = serde_json::from_slice(manifest_file).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("strict manifest JSON: {error}"))
    })?;
    let value = serde_json::to_value(&manifest).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("manifest serialization: {error}"))
    })?;
    require_ascii_strings(&value)?;
    let mut expected_file = canonical_json_bytes(&value)?;
    expected_file.push(b'\n');
    if expected_file != manifest_file {
        return Err(CommonModelSnapshotErrorV1::contract(
            "manifest is not canonical JSON followed by one LF",
        ));
    }
    if manifest.schema != SNAPSHOT_SCHEMA_V1
        || manifest.identity != SNAPSHOT_IDENTITY_V1
        || manifest.purpose != SNAPSHOT_PURPOSE_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "snapshot identity or purpose mismatch",
        ));
    }
    if manifest.model.model_architecture_version != MODEL_ARCHITECTURE_VERSION_V1
        || manifest.model.model_config_fingerprint != MODEL_CONFIG_FINGERPRINT_V1
        || manifest.model.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V1
        || manifest.model.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "model binding mismatch",
        ));
    }
    require_exact_model_config(&manifest.model.model_config)?;
    if manifest.initializer.authority != INITIALIZER_AUTHORITY_V1
        || manifest.initializer.identity != INITIALIZER_IDENTITY_V1
        || manifest.initializer.base_seed != BASE_SEED_V1
        || manifest.initializer.model_init_seed != MODEL_INIT_SEED_V1
        || manifest.initializer.trainer_schedule_version != NATIVE_TRAINER_SCHEDULE_VERSION_V1
        || manifest.initializer.python_reference_seed_version != PYTHON_REFERENCE_SEED_VERSION_V1
        || manifest.initializer.schedule_goldens_sha256 != NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "initializer or schedule binding mismatch",
        ));
    }
    if derive_native_trainer_model_init_seed_v1(BASE_SEED_V1).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("seed derivation: {error:?}"))
    })? != MODEL_INIT_SEED_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "native schedule no longer derives the frozen model seed",
        ));
    }
    let expected_runtime = expected_runtime_configuration();
    let expected_runtime_value = serde_json::to_value(&expected_runtime).map_err(|error| {
        CommonModelSnapshotErrorV1::contract(format!("runtime serialization: {error}"))
    })?;
    if manifest.authority.runtime_identity != AUTHORITY_RUNTIME_IDENTITY_V1
        || manifest.authority.runtime_configuration.byte_order != expected_runtime.byte_order
        || manifest.authority.runtime_configuration.device != expected_runtime.device
        || manifest.authority.runtime_configuration.platform_machine
            != expected_runtime.platform_machine
        || manifest.authority.runtime_configuration.platform_system
            != expected_runtime.platform_system
        || manifest.authority.runtime_configuration.python_version
            != expected_runtime.python_version
        || manifest.authority.runtime_configuration.torch_default_dtype
            != expected_runtime.torch_default_dtype
        || manifest
            .authority
            .runtime_configuration
            .torch_deterministic_algorithms
            != expected_runtime.torch_deterministic_algorithms
        || manifest
            .authority
            .runtime_configuration
            .torch_num_interop_threads
            != expected_runtime.torch_num_interop_threads
        || manifest.authority.runtime_configuration.torch_num_threads
            != expected_runtime.torch_num_threads
        || manifest.authority.runtime_configuration.torch_version != expected_runtime.torch_version
        || manifest.authority.runtime_configuration_sha256
            != sha256_hex(&canonical_json_bytes(&expected_runtime_value)?)
        || manifest.authority.source_bundle_contract != SOURCE_BUNDLE_CONTRACT_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "authority runtime binding mismatch",
        ));
    }
    let (expected_sources, expected_bundle) = expected_authority_sources();
    if manifest.authority.sources != expected_sources
        || manifest.authority.source_bundle_sha256 != expected_bundle
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "authority source binding mismatch",
        ));
    }
    if !manifest.payload.buffers.is_empty()
        || manifest.payload.encoding != PAYLOAD_ENCODING_V1
        || manifest.payload.layout != PAYLOAD_LAYOUT_V1
        || manifest.payload.parameter_tensor_count != PARAMETER_TENSOR_COUNT_V1 as u64
        || manifest.payload.parameter_element_count != PARAMETER_ELEMENT_COUNT_V1 as u64
        || manifest.payload.payload_byte_count != PAYLOAD_BYTE_COUNT_V1 as u64
        || manifest.payload.sha256 != sha256_hex(payload)
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "payload declaration or digest mismatch",
        ));
    }
    if manifest.parameters.len() != PARAMETER_TENSOR_COUNT_V1 {
        return Err(CommonModelSnapshotErrorV1::contract(
            "parameter tensor count mismatch",
        ));
    }
    let mut expected_element_offset = 0u64;
    let mut expected_byte_offset = 0u64;
    let mut named_parameters = Vec::with_capacity(PARAMETER_TENSOR_COUNT_V1);
    for (ordinal, (entry, expected)) in manifest
        .parameters
        .iter()
        .zip(EXPECTED_PARAMETER_LAYOUT_V1)
        .enumerate()
    {
        let shape_product = entry.shape.iter().try_fold(1u64, |product, dimension| {
            if *dimension == 0 {
                None
            } else {
                product.checked_mul(*dimension)
            }
        });
        let expected_byte_count = expected
            .element_count
            .checked_mul(4)
            .ok_or_else(|| CommonModelSnapshotErrorV1::contract("parameter byte count overflow"))?;
        let frozen_byte_offset = expected.element_offset.checked_mul(4).ok_or_else(|| {
            CommonModelSnapshotErrorV1::contract("parameter byte offset overflow")
        })?;
        if entry.ordinal != ordinal as u64
            || entry.name != expected.name
            || entry.shape != expected.shape
            || entry.element_offset != expected.element_offset
            || entry.element_offset != expected_element_offset
            || entry.element_count != expected.element_count
            || shape_product != Some(expected.element_count)
            || entry.byte_offset != frozen_byte_offset
            || entry.byte_offset != expected_byte_offset
            || entry.byte_count != expected_byte_count
        {
            return Err(CommonModelSnapshotErrorV1::contract(format!(
                "parameter layout mismatch at ordinal {ordinal}"
            )));
        }
        let begin = usize::try_from(entry.byte_offset)
            .map_err(|_| CommonModelSnapshotErrorV1::contract("parameter offset overflow"))?;
        let count = usize::try_from(entry.byte_count)
            .map_err(|_| CommonModelSnapshotErrorV1::contract("parameter count overflow"))?;
        let end = begin
            .checked_add(count)
            .ok_or_else(|| CommonModelSnapshotErrorV1::contract("parameter end overflow"))?;
        let bytes = payload
            .get(begin..end)
            .ok_or_else(|| CommonModelSnapshotErrorV1::contract("parameter exceeds payload"))?;
        if entry.tensor_sha256 != sha256_hex(bytes) {
            return Err(CommonModelSnapshotErrorV1::contract(format!(
                "parameter digest mismatch at ordinal {ordinal}"
            )));
        }
        let mut values = Vec::with_capacity(expected.element_count as usize);
        for (position, bytes) in bytes.chunks_exact(4).enumerate() {
            let bits = u32::from_le_bytes(bytes.try_into().expect("exact chunk"));
            let value = f32::from_bits(bits);
            if !value.is_finite() {
                return Err(CommonModelSnapshotErrorV1::contract(format!(
                    "non-finite parameter at ordinal {ordinal} position {position}"
                )));
            }
            values.push(value);
        }
        named_parameters.push(NativeNamedParameterV1 {
            name: expected.name,
            shape: expected
                .shape
                .iter()
                .map(|dimension| usize::try_from(*dimension))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    CommonModelSnapshotErrorV1::contract("parameter dimension overflow")
                })?,
            values,
        });
        expected_element_offset = expected_element_offset
            .checked_add(entry.element_count)
            .ok_or_else(|| CommonModelSnapshotErrorV1::contract("element end overflow"))?;
        expected_byte_offset = expected_byte_offset
            .checked_add(entry.byte_count)
            .ok_or_else(|| CommonModelSnapshotErrorV1::contract("byte end overflow"))?;
    }
    if expected_element_offset != PARAMETER_ELEMENT_COUNT_V1 as u64
        || expected_byte_offset != PAYLOAD_BYTE_COUNT_V1 as u64
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "parameter final offset mismatch",
        ));
    }
    if named_parameters[0].values[..CARD_EMBEDDING_DIM_V1]
        .iter()
        .any(|value| value.to_bits() != 0)
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "padding embedding row is not exact positive zero",
        ));
    }
    let layout_digest = sha256_hex(&canonical_json_bytes(&layout_projection(&manifest))?);
    if manifest.integrity.parameter_layout_sha256 != layout_digest {
        return Err(CommonModelSnapshotErrorV1::contract(
            "parameter layout digest mismatch",
        ));
    }
    let named_digest = named_parameter_stream_sha256(&manifest.parameters, payload)?;
    if manifest.integrity.named_parameter_stream_sha256 != named_digest {
        return Err(CommonModelSnapshotErrorV1::contract(
            "named parameter stream digest mismatch",
        ));
    }
    if manifest.optimizer_bootstrap.optimizer_identity != NATIVE_OPTIMIZER_IDENTITY_V1
        || manifest.optimizer_bootstrap.adam_step != 0
        || manifest.optimizer_bootstrap.moment_initialization != MOMENT_INITIALIZATION_V1
        || manifest.optimizer_bootstrap.canonical_gauge_parameters != CANONICAL_GAUGE_PARAMETERS_V1
        || manifest.optimizer_bootstrap.value_head_gauge != VALUE_HEAD_GAUGE_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "optimizer bootstrap mismatch",
        ));
    }
    let scorer_bits = u64::from(named_parameters[28].values[0].to_bits());
    if manifest.optimizer_bootstrap.scorer_bias_anchor_f32_bits != scorer_bits {
        return Err(CommonModelSnapshotErrorV1::contract(
            "scorer-bias anchor differs from decoded ordinal 28",
        ));
    }
    if manifest.nonclaims.scope != NONCLAIM_V1
        || manifest.nonclaims.legacy_optimizer != LEGACY_OPTIMIZER_NONCLAIM_V1
        || manifest.nonclaims.independent_gates
            != [
                "exact Torch initializer reproduction",
                "native checkpoint/resume",
                "learning noninferiority",
                "speed ratio",
            ]
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "snapshot nonclaims mismatch",
        ));
    }
    let core_digest = manifest_core_sha256(&manifest)?;
    if manifest.integrity.manifest_core_sha256 != core_digest
        || manifest.integrity.snapshot_sha256
            != snapshot_sha256(&core_digest, &manifest.payload.sha256)?
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "manifest core or snapshot digest mismatch",
        ));
    }
    if named_parameters
        .iter()
        .map(|parameter| parameter.values.len())
        .sum::<usize>()
        != PARAMETER_COUNT_V1
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "decoded parameter count mismatch",
        ));
    }
    Ok(ValidatedSnapshotV1 {
        manifest,
        manifest_file_sha256: sha256_hex(manifest_file),
        named_parameters,
    })
}

fn reexport_named_parameters(
    parameters: &[NativeNamedParameterV1],
) -> Result<(Vec<u8>, String), CommonModelSnapshotErrorV1> {
    if parameters.len() != PARAMETER_TENSOR_COUNT_V1 {
        return Err(CommonModelSnapshotErrorV1::contract(
            "candidate parameter tensor count mismatch",
        ));
    }
    let mut payload = Vec::with_capacity(PAYLOAD_BYTE_COUNT_V1);
    let mut digest = Sha256::new();
    for (parameter, expected) in parameters.iter().zip(EXPECTED_PARAMETER_LAYOUT_V1) {
        if parameter.name != expected.name
            || parameter
                .shape
                .iter()
                .map(|value| *value as u64)
                .collect::<Vec<_>>()
                != expected.shape
            || parameter.values.len() != expected.element_count as usize
        {
            return Err(CommonModelSnapshotErrorV1::contract(
                "candidate parameter layout mismatch",
            ));
        }
        let name_length = u32::try_from(parameter.name.len())
            .map_err(|_| CommonModelSnapshotErrorV1::contract("candidate name overflow"))?;
        let rank = u32::try_from(parameter.shape.len())
            .map_err(|_| CommonModelSnapshotErrorV1::contract("candidate rank overflow"))?;
        digest.update(name_length.to_be_bytes());
        digest.update(parameter.name.as_bytes());
        digest.update(rank.to_be_bytes());
        for dimension in &parameter.shape {
            digest.update((*dimension as u64).to_be_bytes());
        }
        digest.update((parameter.values.len() as u64).to_be_bytes());
        for value in &parameter.values {
            if !value.is_finite() {
                return Err(CommonModelSnapshotErrorV1::contract(
                    "candidate parameter is non-finite",
                ));
            }
            let bytes = value.to_le_bytes();
            payload.extend_from_slice(&bytes);
            digest.update(bytes);
        }
    }
    Ok((payload, format!("{:x}", digest.finalize())))
}

fn record_from_validated(
    validated: &ValidatedSnapshotV1,
    loaded_named_parameter_stream_sha256: String,
) -> CommonModelSnapshotRecordV1 {
    let manifest = &validated.manifest;
    CommonModelSnapshotRecordV1 {
        schema: manifest.schema.clone(),
        identity: manifest.identity.clone(),
        snapshot_sha256: manifest.integrity.snapshot_sha256.clone(),
        manifest_file_sha256: validated.manifest_file_sha256.clone(),
        manifest_core_sha256: manifest.integrity.manifest_core_sha256.clone(),
        payload_sha256: manifest.payload.sha256.clone(),
        payload_byte_count: manifest.payload.payload_byte_count,
        parameter_layout_sha256: manifest.integrity.parameter_layout_sha256.clone(),
        named_parameter_stream_sha256: manifest.integrity.named_parameter_stream_sha256.clone(),
        loaded_named_parameter_stream_sha256,
        parameter_tensor_count: manifest.payload.parameter_tensor_count,
        parameter_element_count: manifest.payload.parameter_element_count,
        model_config_fingerprint: manifest.model.model_config_fingerprint.clone(),
        model_architecture_version: manifest.model.model_architecture_version.clone(),
        feature_contract_digest: manifest.model.feature_contract_digest.clone(),
        feature_encoding_digest: manifest.model.feature_encoding_digest.clone(),
        initializer_identity: manifest.initializer.identity.clone(),
        base_seed: manifest.initializer.base_seed,
        model_init_seed: manifest.initializer.model_init_seed,
        trainer_schedule_version: manifest.initializer.trainer_schedule_version.clone(),
        python_reference_seed_version: manifest.initializer.python_reference_seed_version.clone(),
        schedule_goldens_sha256: manifest.initializer.schedule_goldens_sha256.clone(),
        authority_source_bundle_sha256: manifest.authority.source_bundle_sha256.clone(),
        authority_runtime_identity: manifest.authority.runtime_identity.clone(),
        loader_identity: RUST_LOADER_IDENTITY_V1.to_owned(),
        optimizer_identity: manifest.optimizer_bootstrap.optimizer_identity.clone(),
        adam_step_initial: manifest.optimizer_bootstrap.adam_step,
        moment_initialization: manifest.optimizer_bootstrap.moment_initialization.clone(),
        canonical_gauge_parameters: manifest
            .optimizer_bootstrap
            .canonical_gauge_parameters
            .clone(),
        scorer_bias_anchor_f32_bits: manifest.optimizer_bootstrap.scorer_bias_anchor_f32_bits,
        snapshot_load_completed_before_trial_start: true,
        snapshot_load_timed: false,
        rust_seeded_initializer_reproduced: false,
        nonclaim: NONCLAIM_V1.to_owned(),
    }
}

pub(crate) fn load_common_model_snapshot_v1(
    manifest_path: &Path,
    payload_path: &Path,
    live_state: &mut NativePolicyValueTrainStateV1,
) -> Result<CommonModelSnapshotRecordV1, CommonModelSnapshotErrorV1> {
    let manifest_file = capture_regular_file(manifest_path, MANIFEST_MAX_BYTES_V1)?;
    let payload = capture_regular_file(payload_path, PAYLOAD_MAX_BYTES_V1)?;
    let validated = validate_snapshot_bytes(&manifest_file, &payload)?;

    let mut candidate_model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|error| {
                CommonModelSnapshotErrorV1::contract(format!(
                    "candidate model construction: {error}"
                ))
            })?;
    candidate_model
        .replace_parameter_snapshot_v1(&validated.named_parameters)
        .map_err(|error| {
            CommonModelSnapshotErrorV1::contract(format!("candidate parameter install: {error}"))
        })?;
    let candidate_parameters = candidate_model.parameter_snapshot_v1();
    let (loaded_payload, loaded_named_digest) = reexport_named_parameters(&candidate_parameters)?;
    if loaded_payload != payload
        || loaded_named_digest != validated.manifest.integrity.named_parameter_stream_sha256
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "private candidate re-export or named-stream digest mismatch",
        ));
    }
    let candidate_state =
        NativePolicyValueTrainStateV1::new_v1(candidate_model).map_err(|error| {
            CommonModelSnapshotErrorV1::contract(format!("candidate optimizer bootstrap: {error}"))
        })?;
    if candidate_state.adam_step_v1() != 0
        || u64::from(candidate_state.scorer_bias_anchor_f32_bits_v1())
            != validated
                .manifest
                .optimizer_bootstrap
                .scorer_bias_anchor_f32_bits
        || candidate_state
            .first_moment_snapshot_v1()
            .iter()
            .chain(candidate_state.second_moment_snapshot_v1().iter())
            .flat_map(|parameter| &parameter.values)
            .any(|value| value.to_bits() != 0)
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "candidate optimizer bootstrap is not exact positive zero",
        ));
    }
    let record = record_from_validated(&validated, loaded_named_digest);
    if record.rust_seeded_initializer_reproduced
        || record.scorer_bias_anchor_f32_bits
            != u64::from(candidate_parameters[28].values[0].to_bits())
    {
        return Err(CommonModelSnapshotErrorV1::contract(
            "candidate record invariant mismatch",
        ));
    }
    *live_state = candidate_state;
    Ok(record)
}

pub(crate) fn common_model_snapshot_paths_v1() -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate has repository parent");
    let directory = root.join("data").join("common_model_snapshot_v1");
    (
        directory.join("manifest.json"),
        directory.join("parameters.f32le"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_ORDINAL: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let ordinal = TEMP_ORDINAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mtg-kernel-common-snapshot-v1-{}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create isolated test directory");
            Self { path }
        }

        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, bytes).expect("write test file");
            path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.path).expect("remove isolated test directory");
        }
    }

    fn artifact_bytes() -> (&'static [u8], &'static [u8]) {
        (
            include_bytes!("../../data/common_model_snapshot_v1/manifest.json"),
            include_bytes!("../../data/common_model_snapshot_v1/parameters.f32le"),
        )
    }

    fn runner_state() -> NativePolicyValueTrainStateV1 {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        NativePolicyValueTrainStateV1::new_v1(model).unwrap()
    }

    fn canonical_file(value: &Value) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(value).unwrap();
        bytes.push(b'\n');
        bytes
    }

    fn resigned_manifest(mut value: Value, payload: &[u8]) -> Vec<u8> {
        value["payload"]["sha256"] = Value::String(sha256_hex(payload));
        let parameters = value["parameters"].as_array_mut().unwrap();
        for parameter in parameters {
            let begin = parameter["byte_offset"].as_u64().unwrap() as usize;
            let count = parameter["byte_count"].as_u64().unwrap() as usize;
            parameter["tensor_sha256"] = Value::String(sha256_hex(&payload[begin..begin + count]));
        }
        let parsed: ManifestV1 = serde_json::from_value(value.clone()).unwrap();
        value["integrity"]["parameter_layout_sha256"] = Value::String(sha256_hex(
            &canonical_json_bytes(&layout_projection(&parsed)).unwrap(),
        ));
        let parsed: ManifestV1 = serde_json::from_value(value.clone()).unwrap();
        value["integrity"]["named_parameter_stream_sha256"] =
            Value::String(named_parameter_stream_sha256(&parsed.parameters, payload).unwrap());
        let parsed: ManifestV1 = serde_json::from_value(value.clone()).unwrap();
        let core = manifest_core_sha256(&parsed).unwrap();
        value["integrity"]["manifest_core_sha256"] = Value::String(core.clone());
        value["integrity"]["snapshot_sha256"] =
            Value::String(snapshot_sha256(&core, &sha256_hex(payload)).unwrap());
        canonical_file(&value)
    }

    fn assert_state(
        state: &NativePolicyValueTrainStateV1,
        parameters: &[NativeNamedParameterV1],
        first: &[NativeNamedParameterV1],
        second: &[NativeNamedParameterV1],
        step: u64,
    ) {
        assert_eq!(state.model_v1().parameter_snapshot_v1(), parameters);
        assert_eq!(state.first_moment_snapshot_v1(), first);
        assert_eq!(state.second_moment_snapshot_v1(), second);
        assert_eq!(state.adam_step_v1(), step);
    }

    #[test]
    fn committed_artifact_loads_transactionally_and_reexports_bit_exact() {
        let (manifest_bytes, payload_bytes) = artifact_bytes();
        let validated = validate_snapshot_bytes(manifest_bytes, payload_bytes).unwrap();
        let (reexported, stream_digest) =
            reexport_named_parameters(&validated.named_parameters).unwrap();
        assert_eq!(reexported, payload_bytes);
        assert_eq!(
            stream_digest,
            validated.manifest.integrity.named_parameter_stream_sha256
        );

        let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
        let mut state = runner_state();
        let runner_value_head = state.model_v1().parameter_snapshot_v1()[29..]
            .iter()
            .map(|parameter| parameter.values.clone())
            .collect::<Vec<_>>();
        let record =
            load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut state).unwrap();
        assert_eq!(record.loader_identity, RUST_LOADER_IDENTITY_V1);
        assert!(!record.rust_seeded_initializer_reproduced);
        assert!(record.snapshot_load_completed_before_trial_start);
        assert!(!record.snapshot_load_timed);
        assert_eq!(
            record.named_parameter_stream_sha256,
            record.loaded_named_parameter_stream_sha256
        );
        assert_eq!(state.adam_step_v1(), 0);
        for moment in state
            .first_moment_snapshot_v1()
            .into_iter()
            .chain(state.second_moment_snapshot_v1())
        {
            assert!(moment.values.iter().all(|value| value.to_bits() == 0));
        }
        let loaded = state.model_v1().parameter_snapshot_v1();
        assert_eq!(
            loaded[28].values[0].to_bits(),
            record.scorer_bias_anchor_f32_bits as u32
        );
        assert_eq!(
            state.scorer_bias_anchor_f32_bits_v1(),
            record.scorer_bias_anchor_f32_bits as u32
        );
        assert!(loaded[29..]
            .iter()
            .zip(runner_value_head)
            .any(|(loaded, runner)| loaded.values != runner));
        let (loaded_payload, loaded_digest) = reexport_named_parameters(&loaded).unwrap();
        assert_eq!(loaded_payload, payload_bytes);
        assert_eq!(loaded_digest, record.loaded_named_parameter_stream_sha256);

        let keys = serde_json::to_value(&record)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected = [
            "schema",
            "identity",
            "snapshot_sha256",
            "manifest_file_sha256",
            "manifest_core_sha256",
            "payload_sha256",
            "payload_byte_count",
            "parameter_layout_sha256",
            "named_parameter_stream_sha256",
            "loaded_named_parameter_stream_sha256",
            "parameter_tensor_count",
            "parameter_element_count",
            "model_config_fingerprint",
            "model_architecture_version",
            "feature_contract_digest",
            "feature_encoding_digest",
            "initializer_identity",
            "base_seed",
            "model_init_seed",
            "trainer_schedule_version",
            "python_reference_seed_version",
            "schedule_goldens_sha256",
            "authority_source_bundle_sha256",
            "authority_runtime_identity",
            "loader_identity",
            "optimizer_identity",
            "adam_step_initial",
            "moment_initialization",
            "canonical_gauge_parameters",
            "scorer_bias_anchor_f32_bits",
            "snapshot_load_completed_before_trial_start",
            "snapshot_load_timed",
            "rust_seeded_initializer_reproduced",
            "nonclaim",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        assert_eq!(keys, expected);
    }

    #[test]
    fn strict_manifest_and_layout_corruptions_are_rejected() {
        let (manifest, payload) = artifact_bytes();
        assert!(validate_snapshot_bytes(&[], payload).is_err());
        assert!(validate_snapshot_bytes(manifest, &[]).is_err());
        assert!(validate_snapshot_bytes(&manifest[..manifest.len() - 1], payload).is_err());
        let mut extended_manifest = manifest.to_vec();
        extended_manifest.push(b'\n');
        assert!(validate_snapshot_bytes(&extended_manifest, payload).is_err());
        assert!(validate_snapshot_bytes(manifest, &payload[..payload.len() - 1]).is_err());
        let mut extended_payload = payload.to_vec();
        extended_payload.push(0);
        assert!(validate_snapshot_bytes(manifest, &extended_payload).is_err());

        let text = std::str::from_utf8(manifest).unwrap();
        let duplicate = text.replacen(
            "\"schema\":",
            "\"schema\":\"mtg-kernel-common-model-snapshot/v1\",\"schema\":",
            1,
        );
        assert!(validate_snapshot_bytes(duplicate.as_bytes(), payload).is_err());
        let mut noncanonical = manifest.to_vec();
        noncanonical.insert(0, b' ');
        assert!(validate_snapshot_bytes(&noncanonical, payload).is_err());

        let original: Value = serde_json::from_slice(manifest).unwrap();
        let mut mutations: Vec<Value> = Vec::new();
        let mut value = original.clone();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        mutations.push(value);
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove("purpose");
        mutations.push(value);
        let mut value = original.clone();
        value["schema"] = json!(1);
        mutations.push(value);
        let mut value = original.clone();
        value["identity"] = json!("wrong");
        mutations.push(value);
        let mut value = original.clone();
        value["model"]["model_config"]["hidden_dim"] = json!(65);
        mutations.push(value);
        let mut value = original.clone();
        value["authority"]["sources"][0]["sha256"] = json!("00");
        mutations.push(value);
        let mut value = original.clone();
        value["initializer"]["trainer_schedule_version"] = json!("wrong");
        mutations.push(value);
        let mut value = original.clone();
        value["initializer"]["model_init_seed"] = json!(1);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"].as_array_mut().unwrap().swap(0, 1);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["name"] = value["parameters"][0]["name"].clone();
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["name"] = json!("wrong");
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["shape"] = json!([114, 64]);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["shape"] = json!([7296]);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["element_offset"] = json!(1_048_593u64);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["element_offset"] = json!(1_048_591u64);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][1]["byte_offset"] = json!(u64::MAX);
        mutations.push(value);
        let mut value = original.clone();
        value["parameters"][32]["byte_count"] = json!(8);
        mutations.push(value);
        let mut value = original.clone();
        value["integrity"]["parameter_layout_sha256"] = json!("00");
        mutations.push(value);
        let mut value = original.clone();
        value["integrity"]["named_parameter_stream_sha256"] = json!("00");
        mutations.push(value);
        let mut value = original.clone();
        value["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"] = json!(0);
        mutations.push(value);
        for mutation in mutations {
            assert!(validate_snapshot_bytes(&canonical_file(&mutation), payload).is_err());
        }
    }

    #[test]
    fn payload_corruption_nonfinite_endian_and_padding_are_rejected() {
        let (manifest, payload) = artifact_bytes();
        let value: Value = serde_json::from_slice(manifest).unwrap();
        for entry in value["parameters"].as_array().unwrap() {
            let position = entry["byte_offset"].as_u64().unwrap() as usize;
            let mut corrupted = payload.to_vec();
            corrupted[position] ^= 1;
            assert!(validate_snapshot_bytes(manifest, &corrupted).is_err());
        }

        let mut endian_swapped = payload.to_vec();
        for bytes in endian_swapped.chunks_exact_mut(4) {
            bytes.reverse();
        }
        assert!(validate_snapshot_bytes(manifest, &endian_swapped).is_err());

        for bits in [
            f32::NAN.to_bits(),
            f32::INFINITY.to_bits(),
            f32::NEG_INFINITY.to_bits(),
        ] {
            let mut corrupted = payload.to_vec();
            corrupted[64..68].copy_from_slice(&bits.to_le_bytes());
            let resigned = resigned_manifest(value.clone(), &corrupted);
            assert!(validate_snapshot_bytes(&resigned, &corrupted).is_err());
        }
        for bits in [1u32, (-0.0f32).to_bits()] {
            let mut corrupted = payload.to_vec();
            corrupted[..4].copy_from_slice(&bits.to_le_bytes());
            let resigned = resigned_manifest(value.clone(), &corrupted);
            assert!(validate_snapshot_bytes(&resigned, &corrupted).is_err());
        }

        let mut altered_anchor = value.clone();
        let anchor = altered_anchor["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"]
            .as_u64()
            .unwrap();
        altered_anchor["optimizer_bootstrap"]["scorer_bias_anchor_f32_bits"] = json!(anchor ^ 1);
        let resigned = resigned_manifest(altered_anchor, payload);
        assert!(validate_snapshot_bytes(&resigned, payload).is_err());
    }

    #[test]
    fn file_capture_rejects_caps_nonregular_links_and_races() {
        let directory = TestDirectory::new();
        let regular = directory.file("regular.bin", b"abc");
        assert_eq!(capture_regular_file(&regular, 3).unwrap(), b"abc");
        assert!(capture_regular_file(&regular, 2).is_err());
        assert!(capture_regular_file(&directory.path, 100).is_err());

        let race_path = directory.file("race.bin", b"abc");
        let race_result = capture_regular_file_with_hook(&race_path, 100, || {
            let mut writer = fs::OpenOptions::new()
                .append(true)
                .open(&race_path)
                .map_err(|error| CommonModelSnapshotErrorV1::contract(error.to_string()))?;
            writer
                .write_all(b"d")
                .map_err(|error| CommonModelSnapshotErrorV1::contract(error.to_string()))?;
            writer
                .flush()
                .map_err(|error| CommonModelSnapshotErrorV1::contract(error.to_string()))?;
            Ok(())
        });
        assert!(race_result.is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = directory.path.join("link.bin");
            symlink(&regular, &link).unwrap();
            assert!(capture_regular_file(&link, 100).is_err());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            let link = directory.path.join("link.bin");
            if symlink_file(&regular, &link).is_ok() {
                assert!(capture_regular_file(&link, 100).is_err());
            }
        }
    }

    #[test]
    fn every_load_failure_preserves_model_optimizer_and_step() {
        let (manifest, payload) = artifact_bytes();
        let directory = TestDirectory::new();
        let manifest_path = directory.file("manifest.json", manifest);
        let mut corrupted_payload = payload.to_vec();
        corrupted_payload[4 * 100] ^= 1;
        let payload_path = directory.file("parameters.f32le", &corrupted_payload);

        let mut state = runner_state();
        let parameters = state.model_v1().parameter_snapshot_v1();
        let first = state.first_moment_snapshot_v1();
        let second = state.second_moment_snapshot_v1();
        assert!(load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut state).is_err());
        assert_state(&state, &parameters, &first, &second, 0);
    }
}
