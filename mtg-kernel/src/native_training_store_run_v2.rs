//! Pure typed `run.json` authority for Native Training Store V2.
//!
//! This module owns no capture, filesystem, publication, execution, or
//! learning-quality behavior. It accepts only canonical JSON, validates the
//! complete dependency-closed run/v2 grammar, reconstructs the standalone
//! semantics projection, and independently recomputes every run-root digest.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::card_def::KERNEL_CARDDB_HASH;
pub use crate::common_model_snapshot_v1::CommonModelSnapshotRecordV1;
use crate::common_model_snapshot_v1::{
    AUTHORITY_RUNTIME_IDENTITY_V1, BASE_SEED_V1, INITIALIZER_IDENTITY_V1, MODEL_INIT_SEED_V1,
    NONCLAIM_V1, PARAMETER_ELEMENT_COUNT_V1, PARAMETER_TENSOR_COUNT_V1, PAYLOAD_BYTE_COUNT_V1,
    RUST_LOADER_IDENTITY_V1, SNAPSHOT_IDENTITY_V1, SNAPSHOT_SCHEMA_V1,
};
use crate::fast_sampler::{
    FAST_CATEGORICAL_EXP_TABLE_SHA256, FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    FAST_CATEGORICAL_SAMPLER_VERSION,
};
use crate::native_full_episode_trajectory_v1::NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1;
use crate::native_opponent_sampler_v1::{
    UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1, UNIFORM_INDEX_MODULO_U64_IDENTITY_V1,
};
use crate::native_policy_train_step_v1::{
    ADAM_BETA1_V1, ADAM_BETA2_V1, ADAM_EPSILON_V1, ADAM_WEIGHT_DECAY_V1,
    CANONICAL_GAUGE_PARAMETERS_V1, NATIVE_OPTIMIZER_IDENTITY_V1,
    NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1,
    NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1, TRAINER_ALGORITHM_V1, TRAIN_STEP_IDENTITY_V1,
};
use crate::native_policy_value_net_v1::{
    FEATURE_CONTRACT_DIGEST_V1, FEATURE_ENCODING_DIGEST_V1, MODEL_ARCHITECTURE_VERSION_V1,
    MODEL_CONFIG_FINGERPRINT_V1, PARAMETER_COUNT_V1,
};
use crate::native_train_state_payload_v1::{
    NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1, NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1,
};
use crate::native_trainer_schedule_v1::{
    NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1, NATIVE_TRAINER_SCHEDULE_VERSION_V1,
    PYTHON_REFERENCE_SEED_VERSION_V1,
};
use crate::native_trainer_v1::{
    NATIVE_TRAINER_CONTRACT_IDENTITY_V2, NATIVE_TRAINER_MAX_BATCH_EPISODES_V2,
    NATIVE_TRAINER_MIN_BATCH_EPISODES_V2,
};
use crate::policy_surface_v5::POLICY_SURFACE_VERSION;
use crate::rl_session::{
    CANONICAL_RALLY_DECK_ID, RL_SESSION_PROTOCOL_NAME, RL_SESSION_PROTOCOL_VERSION,
    RL_SESSION_SCHEMA_VERSION,
};
use crate::runtime_decks::{
    runtime_deck_by_id, RUNTIME_DECK_CATALOG_SCHEMA, RUNTIME_DECK_PROTOCOL,
};
use crate::strict_source_tree_attestation_v1::{
    STRICT_SOURCE_TREE_RECIPE_IDENTITY_V1 as SOURCE_TREE_RECIPE_IDENTITY_V1,
    STRICT_SOURCE_TREE_RECIPE_SHA256_V1 as SOURCE_TREE_RECIPE_SHA256_V1,
};
use crate::surface_v2::H2_PREDICATE_VERSION;
use crate::KERNEL_VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const TRAIN_RUN_SCHEMA_V2: &str = "mtg_kernel_native_train_run/v2";
pub const NATIVE_TRAINING_STORE_IDENTITY_V2: &str = "mtg-kernel-native-training-store-v2";
pub const STANDALONE_SEMANTICS_IDENTITY_V2: &str =
    "mtg-kernel-native-standalone-training-semantics-v2";
pub const IDENTITY_BUNDLE_IDENTITY_V2: &str =
    "mtg-kernel-native-training-identity-bundle-sha256-v2";
pub const TRAIN_RUN_MAX_BYTES_V2: usize = 1024 * 1024;

const U63_MAX: u64 = (1_u64 << 63) - 1;
const MAX_SUCCESSFUL_UPDATES_V2: u64 = 99_999_999;
const MAX_POLICY_STEPS_V2: u64 = 131_072;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const FROZEN_CARD_DB_HASH_U64_V2: u64 = 0xa06f_a956_6106_f0ea;
const FROZEN_CARD_DB_HASH_U64_HEX_V2: &str = "a06fa9566106f0ea";
const FROZEN_RUNTIME_CATALOG_SCHEMA_V2: &str = "kernel_runtime_decks/v1";
const FROZEN_RUNTIME_CATALOG_PROTOCOL_V2: &str = "canonical-mainboard-bo1/v1";
const RUNTIME_CATALOG_SHA256_V1: &str =
    "5ea19e8a08f0e9c9657e9a6a90382329785f27eeabbbe066e80e7025e8ee62c0";
const FROZEN_RALLY_DECK_ID_V2: &str = "Rally";
const FROZEN_RALLY_DECK_HASH_U64_V2: u64 = 0x0c9f_01c2_5444_12bf;
const FROZEN_RALLY_DECK_HASH_U64_HEX_V2: &str = "0c9f01c2544412bf";
const FROZEN_PROTOCOL_V2: &str = "kernel_rl_jsonl";
const FROZEN_PROTOCOL_VERSION_V2: u32 = 5;
const FROZEN_SCHEMA_VERSION_V2: u32 = 5;
const FROZEN_KERNEL_VERSION_V2: &str = "0.0.4-spike";
const FROZEN_SURFACE_VERSION_V2: u32 = 2;
const FROZEN_POLICY_SURFACE_VERSION_V2: u32 = 5;
const PARAMETER_LAYOUT_SHA256_V1: &str =
    "266966ba3f3c49dd758f694aaef65234e01e8c077ab85a7b1058efedd8e5b887";
const SNAPSHOT_SHA256_V1: &str = "33455d0fedc5aea8abd4deeaf37c5480f1832dbea34b9391c9a942d95f040771";
const SNAPSHOT_MANIFEST_FILE_SHA256_V1: &str =
    "d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc";
const SNAPSHOT_MANIFEST_CORE_SHA256_V1: &str =
    "456a5f8d2c3973c88e47b9d8c8a6ce6069561c4b5aa6582c73e31d837c13816d";
const SNAPSHOT_PAYLOAD_SHA256_V1: &str =
    "79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5";
const SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1: &str =
    "36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54";
const SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V1: &str =
    "78f0a0409b91df169ab895d4328ba525564cf62135e8fb0be9f0f3ece9e77e87";
const TENSORIZER_IDENTITY_V2: &str = "mtg-kernel-python-encoded-decision-tensor-contract-v2";
const TENSORIZER_AUTHORITY_SOURCE_SHA256_V2: &str =
    "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb";
const TENSORIZER_FIXTURE_SHA256_V2: &str =
    "5dbece4f903a09260a499295d866c7e6ff4283f9de83f842224511f977ae8a97";
const TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2: &str =
    "2f87d49106806a402148fc8b115a54ac94713eb717f45f897eff57a3bd1184ec";
const LEARNER_VECTORS_FILE_SHA256_V1: &str =
    "407a08fb9b9bb5012f14d779d0878c986ce0f16530820a89f5bd54c33d5e7456";
const LEARNER_VECTOR_STREAM_SHA256_V1: &str =
    "69fe3e72dd8fdb245e59e1959359aff3cb6c326fab9f7f2b2ab56e3744d4f3de";
const OPPONENT_VECTORS_FILE_SHA256_V1: &str =
    "9e5898308d30614a4a09cecb584200521b1a3b727606d8cf78dbe70b51106e18";
const OPPONENT_VECTOR_STREAM_SHA256_V1: &str =
    "2b65520a528dcf9eba8d7baded50cc9ad50cf507704c2b4410e2afb4b34d7fad";
const TRAJECTORY_GOLDENS_FILE_SHA256_V1: &str =
    "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b";
const TRAJECTORY_GOLDEN_STREAM_SHA256_V1: &str =
    "f5230cbbc0b87735e7aa14c89ce31e41ce769de3f4292cafe63dad4733168d7a";

/// Stable, input-independent failure categories for the run/v2 authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrainRunV2ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidScalar,
    InvalidLiteral,
    InvalidArithmetic,
    CrossBinding,
    StandaloneSemanticsMismatch,
    StandaloneSemanticsDigestMismatch,
    IdentityBundleDigestMismatch,
}

impl TrainRunV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_run_v2_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidScalar => "native_train_run_v2_invalid_scalar",
            Self::InvalidLiteral => "native_train_run_v2_invalid_literal",
            Self::InvalidArithmetic => "native_train_run_v2_invalid_arithmetic",
            Self::CrossBinding => "native_train_run_v2_cross_binding",
            Self::StandaloneSemanticsMismatch => {
                "native_train_run_v2_standalone_semantics_mismatch"
            }
            Self::StandaloneSemanticsDigestMismatch => {
                "native_train_run_v2_standalone_semantics_digest_mismatch"
            }
            Self::IdentityBundleDigestMismatch => {
                "native_train_run_v2_identity_bundle_digest_mismatch"
            }
        }
    }
}

/// No source bytes, field names, values, paths, or parser text are retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrainRunV2Error {
    kind: TrainRunV2ErrorKind,
}

impl TrainRunV2Error {
    const fn new(kind: TrainRunV2ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> TrainRunV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for TrainRunV2Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(TrainRunV2ErrorKind::CanonicalJson(error.kind()))
    }
}

impl Display for TrainRunV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for TrainRunV2Error {}

type Result<T> = std::result::Result<T, TrainRunV2Error>;

/// Read-only raw record retained inside [`ValidatedTrainRunV2`].
///
/// It deliberately has neither a public deserializer nor `Clone`; callers may
/// inspect a validated borrow but cannot manufacture a second raw authority.
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_run_v2::TrainRunV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<TrainRunV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_run_v2::TrainRunV2;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<TrainRunV2>();
/// ```
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunV2 {
    pub(crate) schema: String,
    pub(crate) store_identity: String,
    pub(crate) package: TrainRunPackageV2,
    pub(crate) toolchain: TrainRunToolchainV2,
    pub(crate) source: TrainRunSourceV2,
    pub(crate) runtime: TrainRunRuntimeV2,
    pub(crate) environment: TrainRunEnvironmentV2,
    pub(crate) contracts: TrainRunContractsV2,
    pub(crate) model_snapshot: CommonModelSnapshotRecordV1,
    pub(crate) optimization: TrainRunOptimizationV2,
    pub(crate) schedule: TrainRunScheduleV2,
    pub(crate) limits: TrainRunLimitsV2,
    pub(crate) topology: TrainRunTopologyV2,
    pub(crate) artifact_schemas: TrainRunArtifactSchemasV2,
    pub(crate) publication: TrainRunPublicationV2,
    pub(crate) nonclaims: [String; 8],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainRunWireV2 {
    schema: String,
    store_identity: String,
    package: TrainRunPackageV2,
    toolchain: TrainRunToolchainV2,
    source: TrainRunSourceV2,
    runtime: TrainRunRuntimeV2,
    environment: TrainRunEnvironmentV2,
    contracts: TrainRunContractsV2,
    model_snapshot: CommonModelSnapshotRecordV1,
    optimization: TrainRunOptimizationV2,
    schedule: TrainRunScheduleV2,
    limits: TrainRunLimitsV2,
    topology: TrainRunTopologyV2,
    artifact_schemas: TrainRunArtifactSchemasV2,
    publication: TrainRunPublicationV2,
    nonclaims: [String; 8],
}

impl From<TrainRunWireV2> for TrainRunV2 {
    fn from(wire: TrainRunWireV2) -> Self {
        Self {
            schema: wire.schema,
            store_identity: wire.store_identity,
            package: wire.package,
            toolchain: wire.toolchain,
            source: wire.source,
            runtime: wire.runtime,
            environment: wire.environment,
            contracts: wire.contracts,
            model_snapshot: wire.model_snapshot,
            optimization: wire.optimization,
            schedule: wire.schedule,
            limits: wire.limits,
            topology: wire.topology,
            artifact_schemas: wire.artifact_schemas,
            publication: wire.publication,
            nonclaims: wire.nonclaims,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunPackageV2 {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) workspace_manifest_sha256: String,
    pub(crate) crate_manifest_sha256: String,
    pub(crate) cargo_lock_sha256: String,
    pub(crate) enabled_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunToolchainV2 {
    pub(crate) capture_identity: String,
    pub(crate) rustc_release: String,
    pub(crate) rustc_commit_hash: String,
    pub(crate) rustc_commit_date: String,
    pub(crate) host_triple: String,
    pub(crate) target_triple: String,
    pub(crate) llvm_version: String,
    pub(crate) rustc_verbose_version_sha256: String,
    pub(crate) rustc_verbose_version_line_ending: String,
    pub(crate) build_profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunSourceV2 {
    pub(crate) git_commit: String,
    pub(crate) source_tree_recipe_identity: String,
    pub(crate) source_tree_recipe_sha256: String,
    pub(crate) source_tree_recipe_byte_count: u64,
    pub(crate) source_tree_sha256: String,
    pub(crate) worktree_clean: bool,
    pub(crate) git_status_sha256: String,
    pub(crate) executable_capture_identity: String,
    pub(crate) binary_name: String,
    pub(crate) binary_sha256: String,
    pub(crate) binary_byte_len: u64,
    pub(crate) binary_volume_serial_u64_hex: String,
    pub(crate) binary_file_id_128_hex: String,
    pub(crate) binary_pe_size_of_image_bytes: u64,
    pub(crate) capture_scope: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunRuntimeV2 {
    pub(crate) tuple_identity: String,
    pub(crate) os_capture_identity: String,
    pub(crate) os_system: String,
    pub(crate) os_major: u64,
    pub(crate) os_minor: u64,
    pub(crate) os_build: u64,
    pub(crate) service_pack_major: u64,
    pub(crate) service_pack_minor: u64,
    pub(crate) product_type: u64,
    pub(crate) suite_mask_u16_hex: String,
    pub(crate) native_architecture: String,
    pub(crate) process_architecture: String,
    pub(crate) byte_order: String,
    pub(crate) numerical_backend_identity: String,
    pub(crate) rustc_release: String,
    pub(crate) rustc_commit_hash: String,
    pub(crate) target_triple: String,
    pub(crate) build_profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunEnvironmentV2 {
    pub(crate) card_db_hash_u64_hex: String,
    pub(crate) runtime_catalog_schema: String,
    pub(crate) runtime_catalog_protocol: String,
    pub(crate) runtime_catalog_sha256: String,
    pub(crate) deck_ids: [String; 2],
    pub(crate) deck_hashes_u64_hex: [String; 2],
    pub(crate) protocol: String,
    pub(crate) protocol_version: u64,
    pub(crate) schema_version: u64,
    pub(crate) kernel_version: String,
    pub(crate) surface_version: u64,
    pub(crate) policy_surface_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunContractsV2 {
    pub(crate) trainer_identity: String,
    pub(crate) identity_bundle_identity: String,
    pub(crate) identity_bundle_sha256: String,
    pub(crate) tensorizer: TensorizerContractV2,
    pub(crate) model: ModelContractV2,
    pub(crate) loss: LossContractV2,
    pub(crate) train_step: TrainStepContractV2,
    pub(crate) optimizer: OptimizerContractV2,
    pub(crate) trainer_schedule: TrainerScheduleContractV2,
    pub(crate) learner_sampler: LearnerSamplerContractV2,
    pub(crate) opponent_policy: OpponentPolicyContractV2,
    pub(crate) opponent_sampler: OpponentSamplerContractV2,
    pub(crate) trajectory: TrajectoryContractV2,
    pub(crate) standalone_semantics: StandaloneSemanticsV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TensorizerContractV2 {
    pub(crate) identity: String,
    pub(crate) feature_contract_digest: String,
    pub(crate) feature_encoding_digest: String,
    pub(crate) authoritative_features_source_sha256: String,
    pub(crate) fixture_sha256: String,
    pub(crate) fixture_payload_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelContractV2 {
    pub(crate) architecture_identity: String,
    pub(crate) config_fingerprint: String,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) parameter_tensor_count: u64,
    pub(crate) parameter_element_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LossContractV2 {
    pub(crate) identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainStepContractV2 {
    pub(crate) identity: String,
    pub(crate) numerical_backend_identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizerContractV2 {
    pub(crate) identity: String,
    pub(crate) gauge_identity: String,
    pub(crate) gauge_evidence_identity: String,
    pub(crate) canonical_gauge_parameters: [String; 1],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerScheduleContractV2 {
    pub(crate) identity: String,
    pub(crate) python_reference_seed_identity: String,
    pub(crate) environment_seed_derivation_identity: String,
    pub(crate) learner_action_seed_derivation_identity: String,
    pub(crate) opponent_action_seed_derivation_identity: String,
    pub(crate) goldens_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerSamplerContractV2 {
    pub(crate) identity: String,
    pub(crate) contract_sha256: String,
    pub(crate) exp_table_sha256: String,
    pub(crate) cross_language_vectors_file_sha256: String,
    pub(crate) cross_language_vector_stream_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentPolicyContractV2 {
    pub(crate) identity: String,
    pub(crate) model_rule: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentSamplerContractV2 {
    pub(crate) identity: String,
    pub(crate) algorithm: String,
    pub(crate) seed_derivation_identity: String,
    pub(crate) seed_goldens_sha256: String,
    pub(crate) cross_language_vectors_file_sha256: String,
    pub(crate) cross_language_vector_stream_sha256: String,
    pub(crate) width_one_consumes_seed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrajectoryContractV2 {
    pub(crate) identity: String,
    pub(crate) cross_language_goldens_schema: String,
    pub(crate) cross_language_generator_identity: String,
    pub(crate) cross_language_golden_stream_identity: String,
    pub(crate) cross_language_goldens_file_sha256: String,
    pub(crate) cross_language_golden_stream_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneSemanticsV2 {
    pub(crate) identity: String,
    pub(crate) core: StandaloneSemanticsCoreV2,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunOptimizationV2 {
    pub(crate) learning_rate_f32_bits: String,
    pub(crate) value_coefficient_f32_bits: String,
    pub(crate) beta1_f32_bits: String,
    pub(crate) beta2_f32_bits: String,
    pub(crate) epsilon_f32_bits: String,
    pub(crate) weight_decay_f32_bits: String,
    pub(crate) amsgrad: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunScheduleV2 {
    pub(crate) base_seed: u64,
    pub(crate) batch_episodes: u64,
    pub(crate) checkpoint_segment_updates: u64,
    pub(crate) requested_successful_updates: u64,
    pub(crate) checkpoint_episode_interval: u64,
    pub(crate) measurement_mode: String,
    pub(crate) learner_seat_rule: String,
    pub(crate) paired_environment_seed_rule: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunLimitsV2 {
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunTopologyV2 {
    pub(crate) worker_count: u64,
    pub(crate) sessions_per_worker: u64,
    pub(crate) logical_actor_count: u64,
    pub(crate) broker_batch_target: u64,
    pub(crate) scheduler_timeout_ms: u64,
    pub(crate) measure_broker_service_time: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunArtifactSchemasV2 {
    pub(crate) run: String,
    pub(crate) episode: String,
    pub(crate) update_evidence: String,
    pub(crate) segment: String,
    pub(crate) segment_continuation: String,
    pub(crate) checkpoint: String,
    pub(crate) state_payload: String,
    pub(crate) sidecar: String,
    pub(crate) head: String,
    pub(crate) latest: String,
    pub(crate) checkpoint_ref: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrainRunPublicationV2 {
    pub(crate) canonical_json: String,
    pub(crate) state_payload: String,
    pub(crate) segment_boundary: String,
    pub(crate) same_parent_stage: String,
    pub(crate) latest_published_last: bool,
    pub(crate) windows_only: bool,
    pub(crate) observed_timing_fields_in_deterministic_store: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneSemanticsCoreV2 {
    pub(crate) identity: String,
    pub(crate) snapshot: StandaloneSnapshotSemanticsV2,
    pub(crate) tensorizer: TensorizerContractV2,
    pub(crate) model: ModelContractV2,
    pub(crate) loss: StandaloneLossSemanticsV2,
    pub(crate) train_step: TrainStepContractV2,
    pub(crate) optimizer: StandaloneOptimizerSemanticsV2,
    pub(crate) learner_sampler: LearnerSamplerContractV2,
    pub(crate) opponent_policy: OpponentPolicyContractV2,
    pub(crate) opponent_sampler: OpponentSamplerContractV2,
    pub(crate) schedule: StandaloneScheduleSemanticsV2,
    pub(crate) trajectory: TrajectoryContractV2,
    pub(crate) environment: TrainRunEnvironmentV2,
    pub(crate) workload: StandaloneWorkloadSemanticsV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneSnapshotSemanticsV2 {
    pub(crate) identity: String,
    pub(crate) snapshot_sha256: String,
    pub(crate) manifest_file_sha256: String,
    pub(crate) payload_sha256: String,
    pub(crate) payload_byte_count: u64,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) named_parameter_stream_sha256: String,
    pub(crate) model_config_fingerprint: String,
    pub(crate) scorer_bias_anchor_f32_bits: u64,
    pub(crate) optimizer_identity: String,
    pub(crate) adam_step_initial: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneLossSemanticsV2 {
    pub(crate) identity: String,
    pub(crate) value_coefficient_f32_bits: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneOptimizerSemanticsV2 {
    pub(crate) identity: String,
    pub(crate) gauge_identity: String,
    pub(crate) gauge_evidence_identity: String,
    pub(crate) canonical_gauge_parameters: [String; 1],
    pub(crate) learning_rate_f32_bits: String,
    pub(crate) beta1_f32_bits: String,
    pub(crate) beta2_f32_bits: String,
    pub(crate) epsilon_f32_bits: String,
    pub(crate) weight_decay_f32_bits: String,
    pub(crate) amsgrad: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneScheduleSemanticsV2 {
    pub(crate) identity: String,
    pub(crate) python_reference_seed_identity: String,
    pub(crate) base_seed: u64,
    pub(crate) environment_seed_derivation_identity: String,
    pub(crate) learner_action_seed_derivation_identity: String,
    pub(crate) opponent_action_seed_derivation_identity: String,
    pub(crate) learner_seat_rule: String,
    pub(crate) paired_environment_seed_rule: String,
    pub(crate) goldens_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneWorkloadSemanticsV2 {
    pub(crate) batch_episodes: u64,
    pub(crate) checkpoint_segment_updates: u64,
    pub(crate) checkpoint_episode_interval: u64,
    pub(crate) requested_successful_updates: u64,
    pub(crate) requested_episode_count: u64,
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
    pub(crate) measurement_mode: String,
    pub(crate) durability_semantics: String,
}

/// Validated immutable authority consumed by later trainer/runner/evaluator layers.
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_run_v2::ValidatedTrainRunV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<ValidatedTrainRunV2>();
/// ```
#[derive(Debug)]
pub struct ValidatedTrainRunV2 {
    record: TrainRunV2,
    canonical_bytes: Vec<u8>,
    run_sha256: String,
    identity_bundle_sha256: String,
    standalone_semantics_sha256: String,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
}

impl ValidatedTrainRunV2 {
    pub fn record(&self) -> &TrainRunV2 {
        &self.record
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub fn run_sha256(&self) -> &str {
        &self.run_sha256
    }

    pub fn identity_bundle_sha256(&self) -> &str {
        &self.identity_bundle_sha256
    }

    pub fn standalone_semantics_sha256(&self) -> &str {
        &self.standalone_semantics_sha256
    }

    pub fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub fn checkpoint_segment_updates(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub fn requested_successful_updates(&self) -> u64 {
        self.requested_successful_updates
    }
}

impl TrainRunV2 {
    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn store_identity(&self) -> &str {
        &self.store_identity
    }

    pub fn environment(&self) -> &TrainRunEnvironmentV2 {
        &self.environment
    }

    pub fn contracts(&self) -> &TrainRunContractsV2 {
        &self.contracts
    }

    pub fn model_snapshot(&self) -> &CommonModelSnapshotRecordV1 {
        &self.model_snapshot
    }

    pub fn optimization(&self) -> &TrainRunOptimizationV2 {
        &self.optimization
    }

    pub fn schedule(&self) -> &TrainRunScheduleV2 {
        &self.schedule
    }

    pub fn limits(&self) -> &TrainRunLimitsV2 {
        &self.limits
    }

    pub fn topology(&self) -> &TrainRunTopologyV2 {
        &self.topology
    }
}

impl TrainRunEnvironmentV2 {
    pub fn deck_ids(&self) -> &[String; 2] {
        &self.deck_ids
    }

    pub fn deck_hashes_u64_hex(&self) -> &[String; 2] {
        &self.deck_hashes_u64_hex
    }
}

impl TrainRunContractsV2 {
    pub fn identity_bundle_sha256(&self) -> &str {
        &self.identity_bundle_sha256
    }

    pub fn standalone_semantics(&self) -> &StandaloneSemanticsV2 {
        &self.standalone_semantics
    }
}

impl StandaloneSemanticsV2 {
    pub fn core(&self) -> &StandaloneSemanticsCoreV2 {
        &self.core
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

impl TrainRunOptimizationV2 {
    pub fn learning_rate_f32_bits(&self) -> &str {
        &self.learning_rate_f32_bits
    }

    pub fn value_coefficient_f32_bits(&self) -> &str {
        &self.value_coefficient_f32_bits
    }
}

impl TrainRunScheduleV2 {
    pub fn base_seed(&self) -> u64 {
        self.base_seed
    }

    pub fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub fn checkpoint_segment_updates(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub fn requested_successful_updates(&self) -> u64 {
        self.requested_successful_updates
    }

    pub fn checkpoint_episode_interval(&self) -> u64 {
        self.checkpoint_episode_interval
    }
}

impl TrainRunLimitsV2 {
    pub fn max_physical_decisions(&self) -> u64 {
        self.max_physical_decisions
    }

    pub fn max_policy_steps(&self) -> u64 {
        self.max_policy_steps
    }
}

impl TrainRunTopologyV2 {
    pub fn worker_count(&self) -> u64 {
        self.worker_count
    }

    pub fn sessions_per_worker(&self) -> u64 {
        self.sessions_per_worker
    }

    pub fn logical_actor_count(&self) -> u64 {
        self.logical_actor_count
    }

    pub fn broker_batch_target(&self) -> u64 {
        self.broker_batch_target
    }
}

/// Decode and validate exact canonical `run.json` bytes.
pub fn decode_train_run_v2(bytes: &[u8]) -> Result<ValidatedTrainRunV2> {
    if bytes.len() > TRAIN_RUN_MAX_BYTES_V2 {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::RecordTooLarge));
    }
    let wire: TrainRunWireV2 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)?;
    let record = TrainRunV2::from(wire);
    let reencoded = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != bytes {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::CanonicalJson(
            CanonicalJsonErrorKindV1::NonCanonicalBytes,
        )));
    }
    validate_decoded_train_run_v2(record, reencoded)
}

/// Internal construction seam for a capture layer. No unchecked record is
/// exported and all derived fields are independently recomputed.
#[allow(dead_code)]
pub(crate) fn validate_train_run_record_v2(record: TrainRunV2) -> Result<ValidatedTrainRunV2> {
    let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid)?;
    if bytes.len() > TRAIN_RUN_MAX_BYTES_V2 {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::RecordTooLarge));
    }
    validate_decoded_train_run_v2(record, bytes)
}

fn validate_decoded_train_run_v2(
    record: TrainRunV2,
    canonical_bytes: Vec<u8>,
) -> Result<ValidatedTrainRunV2> {
    validate_package_v2(&record.package)?;
    validate_toolchain_v2(&record.toolchain)?;
    validate_source_v2(&record.source)?;
    validate_runtime_v2(&record.runtime, &record.toolchain)?;
    validate_environment_v2(&record.environment)?;
    validate_snapshot_v1(&record.model_snapshot)?;
    validate_contracts_v2(&record.contracts)?;
    validate_optimization_v2(&record.optimization)?;
    let requested_episode_count = validate_schedule_v2(&record.schedule, &record.model_snapshot)?;
    validate_limits_v2(&record.limits)?;
    validate_topology_v2(&record.topology)?;
    validate_artifact_schemas_v2(&record.artifact_schemas)?;
    validate_publication_v2(&record.publication)?;
    validate_nonclaims_v2(&record.nonclaims)?;

    if record.schema != TRAIN_RUN_SCHEMA_V2
        || record.store_identity != NATIVE_TRAINING_STORE_IDENTITY_V2
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }

    validate_cross_bindings_v2(&record)?;

    let expected_core = reconstruct_standalone_semantics_core_v2(&record, requested_episode_count)?;
    if record.contracts.standalone_semantics.core != expected_core {
        return Err(TrainRunV2Error::new(
            TrainRunV2ErrorKind::StandaloneSemanticsMismatch,
        ));
    }
    let standalone_semantics_sha256 = standalone_semantics_sha256_v2(&expected_core)?;
    if record.contracts.standalone_semantics.sha256 != standalone_semantics_sha256 {
        return Err(TrainRunV2Error::new(
            TrainRunV2ErrorKind::StandaloneSemanticsDigestMismatch,
        ));
    }

    let identity_bundle_sha256 = identity_bundle_sha256_v2(&record)?;
    if record.contracts.identity_bundle_sha256 != identity_bundle_sha256 {
        return Err(TrainRunV2Error::new(
            TrainRunV2ErrorKind::IdentityBundleDigestMismatch,
        ));
    }

    let run_sha256 = sha256_hex(&canonical_bytes);
    Ok(ValidatedTrainRunV2 {
        batch_episodes: record.schedule.batch_episodes,
        checkpoint_segment_updates: record.schedule.checkpoint_segment_updates,
        requested_successful_updates: record.schedule.requested_successful_updates,
        record,
        canonical_bytes,
        run_sha256,
        identity_bundle_sha256,
        standalone_semantics_sha256,
    })
}

fn validate_package_v2(package: &TrainRunPackageV2) -> Result<()> {
    if package.name != "mtg-kernel"
        || package.version != env!("CARGO_PKG_VERSION")
        || !is_semver(&package.version)
        || !is_sha256(&package.workspace_manifest_sha256)
        || !is_sha256(&package.crate_manifest_sha256)
        || !is_sha256(&package.cargo_lock_sha256)
        || !package
            .enabled_features
            .iter()
            .any(|feature| feature == "native-training-store-v2-production")
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    let mut previous: Option<&str> = None;
    for feature in &package.enabled_features {
        if feature.is_empty()
            || !feature
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || previous.is_some_and(|prior| prior >= feature.as_str())
        {
            return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
        }
        previous = Some(feature);
    }
    Ok(())
}

fn validate_toolchain_v2(toolchain: &TrainRunToolchainV2) -> Result<()> {
    if toolchain.capture_identity != "rustc-verbose-version-build-embed-v1"
        || !is_nonempty_printable_ascii(&toolchain.rustc_release)
        || !is_lower_hex(&toolchain.rustc_commit_hash, 40)
        || !is_valid_date(&toolchain.rustc_commit_date)
        || !is_windows_msvc_triple(&toolchain.host_triple)
        || !is_windows_msvc_triple(&toolchain.target_triple)
        || !is_nonempty_printable_ascii(&toolchain.llvm_version)
        || !is_sha256(&toolchain.rustc_verbose_version_sha256)
        || !matches!(
            toolchain.rustc_verbose_version_line_ending.as_str(),
            "lf" | "crlf"
        )
        || toolchain.build_profile != "release"
        || looks_private_location(&toolchain.rustc_release)
        || looks_private_location(&toolchain.llvm_version)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_source_v2(source: &TrainRunSourceV2) -> Result<()> {
    if !is_lower_hex(&source.git_commit, 40)
        || source.source_tree_recipe_identity != SOURCE_TREE_RECIPE_IDENTITY_V1
        || source.source_tree_recipe_sha256 != SOURCE_TREE_RECIPE_SHA256_V1
        || source.source_tree_recipe_byte_count != 5_847
        || !is_sha256(&source.source_tree_sha256)
        || !source.worktree_clean
        || source.git_status_sha256 != EMPTY_SHA256
        || source.executable_capture_identity != "windows-current-module-path-file-v2"
        || source.binary_name != "mtg-kernel-native.exe"
        || !is_sha256(&source.binary_sha256)
        || !is_positive_u63(source.binary_byte_len)
        || !is_lower_hex(&source.binary_volume_serial_u64_hex, 16)
        || !is_lower_hex(&source.binary_file_id_128_hex, 32)
        || !is_positive_u63(source.binary_pe_size_of_image_bytes)
        || source.capture_scope != "module-path-file-not-loaded-section-provenance/v1"
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_runtime_v2(runtime: &TrainRunRuntimeV2, toolchain: &TrainRunToolchainV2) -> Result<()> {
    if runtime.tuple_identity != "mtg-kernel-native-windows-cpu-runtime-tuple-v1"
        || runtime.os_capture_identity != "windows-rtlgetversion-native-system-info-v1"
        || runtime.os_system != "windows"
        || !is_u63(runtime.os_major)
        || !is_u63(runtime.os_minor)
        || !is_u63(runtime.os_build)
        || !is_u63(runtime.service_pack_major)
        || !is_u63(runtime.service_pack_minor)
        || !is_u63(runtime.product_type)
        || !is_lower_hex(&runtime.suite_mask_u16_hex, 4)
        || !matches!(runtime.native_architecture.as_str(), "amd64" | "arm64")
        || !matches!(runtime.process_architecture.as_str(), "amd64" | "arm64")
        || runtime.byte_order != "little"
        || runtime.numerical_backend_identity
            != NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1
        || runtime.build_profile != "release"
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    let expected_process_architecture = match runtime.target_triple.as_str() {
        "x86_64-pc-windows-msvc" => "amd64",
        "aarch64-pc-windows-msvc" => "arm64",
        _ => {
            return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
        }
    };
    if runtime.rustc_release != toolchain.rustc_release
        || runtime.rustc_commit_hash != toolchain.rustc_commit_hash
        || runtime.target_triple != toolchain.target_triple
        || runtime.process_architecture != expected_process_architecture
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::CrossBinding));
    }
    Ok(())
}

fn validate_environment_v2(environment: &TrainRunEnvironmentV2) -> Result<()> {
    let rally = runtime_deck_by_id(CANONICAL_RALLY_DECK_ID)
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral))?;
    if KERNEL_CARDDB_HASH != FROZEN_CARD_DB_HASH_U64_V2
        || RUNTIME_DECK_CATALOG_SCHEMA != FROZEN_RUNTIME_CATALOG_SCHEMA_V2
        || RUNTIME_DECK_PROTOCOL != FROZEN_RUNTIME_CATALOG_PROTOCOL_V2
        || CANONICAL_RALLY_DECK_ID != FROZEN_RALLY_DECK_ID_V2
        || rally.runtime_deck_hash != FROZEN_RALLY_DECK_HASH_U64_V2
        || RL_SESSION_PROTOCOL_NAME != FROZEN_PROTOCOL_V2
        || RL_SESSION_PROTOCOL_VERSION != FROZEN_PROTOCOL_VERSION_V2
        || RL_SESSION_SCHEMA_VERSION != FROZEN_SCHEMA_VERSION_V2
        || KERNEL_VERSION != FROZEN_KERNEL_VERSION_V2
        || H2_PREDICATE_VERSION != FROZEN_SURFACE_VERSION_V2
        || POLICY_SURFACE_VERSION != FROZEN_POLICY_SURFACE_VERSION_V2
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    if environment.card_db_hash_u64_hex != FROZEN_CARD_DB_HASH_U64_HEX_V2
        || environment.runtime_catalog_schema != FROZEN_RUNTIME_CATALOG_SCHEMA_V2
        || environment.runtime_catalog_protocol != FROZEN_RUNTIME_CATALOG_PROTOCOL_V2
        || environment.runtime_catalog_sha256 != RUNTIME_CATALOG_SHA256_V1
        || environment.deck_ids != [FROZEN_RALLY_DECK_ID_V2, FROZEN_RALLY_DECK_ID_V2]
        || environment.deck_hashes_u64_hex
            != [
                FROZEN_RALLY_DECK_HASH_U64_HEX_V2,
                FROZEN_RALLY_DECK_HASH_U64_HEX_V2,
            ]
        || environment.protocol != FROZEN_PROTOCOL_V2
        || environment.protocol_version != u64::from(FROZEN_PROTOCOL_VERSION_V2)
        || environment.schema_version != u64::from(FROZEN_SCHEMA_VERSION_V2)
        || environment.kernel_version != FROZEN_KERNEL_VERSION_V2
        || environment.surface_version != u64::from(FROZEN_SURFACE_VERSION_V2)
        || environment.policy_surface_version != u64::from(FROZEN_POLICY_SURFACE_VERSION_V2)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_snapshot_v1(snapshot: &CommonModelSnapshotRecordV1) -> Result<()> {
    if snapshot.schema != SNAPSHOT_SCHEMA_V1
        || snapshot.identity != SNAPSHOT_IDENTITY_V1
        || snapshot.snapshot_sha256 != SNAPSHOT_SHA256_V1
        || snapshot.manifest_file_sha256 != SNAPSHOT_MANIFEST_FILE_SHA256_V1
        || snapshot.manifest_core_sha256 != SNAPSHOT_MANIFEST_CORE_SHA256_V1
        || snapshot.payload_sha256 != SNAPSHOT_PAYLOAD_SHA256_V1
        || snapshot.payload_byte_count != PAYLOAD_BYTE_COUNT_V1 as u64
        || snapshot.parameter_layout_sha256 != PARAMETER_LAYOUT_SHA256_V1
        || snapshot.named_parameter_stream_sha256 != SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
        || snapshot.loaded_named_parameter_stream_sha256
            != SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
        || snapshot.parameter_tensor_count != PARAMETER_TENSOR_COUNT_V1 as u64
        || snapshot.parameter_element_count != PARAMETER_ELEMENT_COUNT_V1 as u64
        || snapshot.model_config_fingerprint != MODEL_CONFIG_FINGERPRINT_V1
        || snapshot.model_architecture_version != MODEL_ARCHITECTURE_VERSION_V1
        || snapshot.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V1
        || snapshot.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V1
        || snapshot.initializer_identity != INITIALIZER_IDENTITY_V1
        || snapshot.base_seed != BASE_SEED_V1
        || snapshot.model_init_seed != MODEL_INIT_SEED_V1
        || snapshot.trainer_schedule_version != NATIVE_TRAINER_SCHEDULE_VERSION_V1
        || snapshot.python_reference_seed_version != PYTHON_REFERENCE_SEED_VERSION_V1
        || snapshot.schedule_goldens_sha256 != NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
        || snapshot.authority_source_bundle_sha256 != SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V1
        || snapshot.authority_runtime_identity != AUTHORITY_RUNTIME_IDENTITY_V1
        || snapshot.loader_identity != RUST_LOADER_IDENTITY_V1
        || snapshot.optimizer_identity != NATIVE_OPTIMIZER_IDENTITY_V1
        || snapshot.adam_step_initial != 0
        || snapshot.moment_initialization != "positive-zero-f32"
        || snapshot.canonical_gauge_parameters != CANONICAL_GAUGE_PARAMETERS_V1
        || snapshot.scorer_bias_anchor_f32_bits != 3_141_403_366
        || !snapshot.snapshot_load_completed_before_trial_start
        || snapshot.snapshot_load_timed
        || snapshot.rust_seeded_initializer_reproduced
        || snapshot.nonclaim != NONCLAIM_V1
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    if !is_u63(snapshot.payload_byte_count)
        || !is_u63(snapshot.parameter_tensor_count)
        || !is_u63(snapshot.parameter_element_count)
        || !is_u63(snapshot.base_seed)
        || !is_u63(snapshot.model_init_seed)
        || !is_u63(snapshot.adam_step_initial)
        || snapshot.scorer_bias_anchor_f32_bits > u64::from(u32::MAX)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    Ok(())
}

fn validate_contracts_v2(contracts: &TrainRunContractsV2) -> Result<()> {
    if contracts.trainer_identity != NATIVE_TRAINER_CONTRACT_IDENTITY_V2
        || contracts.identity_bundle_identity != IDENTITY_BUNDLE_IDENTITY_V2
        || !is_sha256(&contracts.identity_bundle_sha256)
        || contracts.tensorizer.identity != TENSORIZER_IDENTITY_V2
        || contracts.tensorizer.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V1
        || contracts.tensorizer.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V1
        || contracts.tensorizer.authoritative_features_source_sha256
            != TENSORIZER_AUTHORITY_SOURCE_SHA256_V2
        || contracts.tensorizer.fixture_sha256 != TENSORIZER_FIXTURE_SHA256_V2
        || contracts.tensorizer.fixture_payload_sha256
            != TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2
        || contracts.model.architecture_identity != MODEL_ARCHITECTURE_VERSION_V1
        || contracts.model.config_fingerprint != MODEL_CONFIG_FINGERPRINT_V1
        || contracts.model.parameter_layout_sha256 != PARAMETER_LAYOUT_SHA256_V1
        || contracts.model.parameter_tensor_count != PARAMETER_TENSOR_COUNT_V1 as u64
        || contracts.model.parameter_element_count != PARAMETER_COUNT_V1 as u64
        || contracts.loss.identity != TRAINER_ALGORITHM_V1
        || contracts.train_step.identity != TRAIN_STEP_IDENTITY_V1
        || contracts.train_step.numerical_backend_identity
            != NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1
        || contracts.optimizer.identity != NATIVE_OPTIMIZER_IDENTITY_V1
        || contracts.optimizer.gauge_identity != NATIVE_OPTIMIZER_IDENTITY_V1
        || contracts.optimizer.gauge_evidence_identity
            != NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1
        || contracts.optimizer.canonical_gauge_parameters
            != CANONICAL_GAUGE_PARAMETERS_V1.map(str::to_owned)
        || contracts.trainer_schedule.identity != NATIVE_TRAINER_SCHEDULE_VERSION_V1
        || contracts.trainer_schedule.python_reference_seed_identity
            != PYTHON_REFERENCE_SEED_VERSION_V1
        || contracts.trainer_schedule.environment_seed_derivation_identity
            != "train-env/base_seed/pair_index"
        || contracts.trainer_schedule.learner_action_seed_derivation_identity
            != "train-learner-action-group/base_seed/episode_index/learner_physical_decision_index -> train-learner-action-substep/group_seed/substep_index"
        || contracts.trainer_schedule.opponent_action_seed_derivation_identity
            != "train-opponent-action-group/base_seed/episode_index/opponent_physical_decision_index -> train-opponent-action-substep/group_seed/substep_index"
        || contracts.trainer_schedule.goldens_sha256
            != NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
        || contracts.learner_sampler.identity != FAST_CATEGORICAL_SAMPLER_VERSION
        || contracts.learner_sampler.contract_sha256
            != FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256
        || contracts.learner_sampler.exp_table_sha256 != FAST_CATEGORICAL_EXP_TABLE_SHA256
        || contracts.learner_sampler.cross_language_vectors_file_sha256
            != LEARNER_VECTORS_FILE_SHA256_V1
        || contracts.learner_sampler.cross_language_vector_stream_sha256
            != LEARNER_VECTOR_STREAM_SHA256_V1
        || contracts.opponent_policy.identity != "mtg-kernel-trainer-uniform-policy-v1"
        || contracts.opponent_policy.model_rule != "no-model-uniform-legal-index"
        || contracts.opponent_sampler.identity != UNIFORM_INDEX_MODULO_U64_IDENTITY_V1
        || contracts.opponent_sampler.algorithm != UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1
        || contracts.opponent_sampler.seed_derivation_identity
            != contracts
                .trainer_schedule
                .opponent_action_seed_derivation_identity
        || contracts.opponent_sampler.seed_goldens_sha256
            != NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
        || contracts.opponent_sampler.cross_language_vectors_file_sha256
            != OPPONENT_VECTORS_FILE_SHA256_V1
        || contracts.opponent_sampler.cross_language_vector_stream_sha256
            != OPPONENT_VECTOR_STREAM_SHA256_V1
        || !contracts.opponent_sampler.width_one_consumes_seed
        || contracts.trajectory.identity != NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1
        || contracts.trajectory.cross_language_goldens_schema
            != "mtg_kernel_native_full_episode_trajectory_goldens/v1"
        || contracts.trajectory.cross_language_generator_identity
            != "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1"
        || contracts.trajectory.cross_language_golden_stream_identity
            != "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1"
        || contracts.trajectory.cross_language_goldens_file_sha256
            != TRAJECTORY_GOLDENS_FILE_SHA256_V1
        || contracts.trajectory.cross_language_golden_stream_sha256
            != TRAJECTORY_GOLDEN_STREAM_SHA256_V1
        || contracts.standalone_semantics.identity != STANDALONE_SEMANTICS_IDENTITY_V2
        || contracts.standalone_semantics.core.identity != STANDALONE_SEMANTICS_IDENTITY_V2
        || !is_sha256(&contracts.standalone_semantics.sha256)
    {
        return Err(TrainRunV2Error::new(
            TrainRunV2ErrorKind::InvalidLiteral,
        ));
    }
    Ok(())
}

fn validate_optimization_v2(optimization: &TrainRunOptimizationV2) -> Result<()> {
    let learning_rate = decode_f32_hex(&optimization.learning_rate_f32_bits)?;
    let value_coefficient = decode_f32_hex(&optimization.value_coefficient_f32_bits)?;
    let beta1 = decode_f32_hex(&optimization.beta1_f32_bits)?;
    let beta2 = decode_f32_hex(&optimization.beta2_f32_bits)?;
    let epsilon = decode_f32_hex(&optimization.epsilon_f32_bits)?;
    let weight_decay = decode_f32_hex(&optimization.weight_decay_f32_bits)?;
    if !learning_rate.is_normal()
        || learning_rate <= 0.0
        || !value_coefficient.is_normal()
        || value_coefficient <= 0.0
        || !beta1.is_finite()
        || !(0.0..1.0).contains(&beta1)
        || !beta2.is_finite()
        || !(0.0..1.0).contains(&beta2)
        || !epsilon.is_finite()
        || epsilon <= 0.0
        || !weight_decay.is_finite()
        || weight_decay < 0.0
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    if beta1.to_bits() != ADAM_BETA1_V1.to_bits()
        || beta2.to_bits() != ADAM_BETA2_V1.to_bits()
        || epsilon.to_bits() != ADAM_EPSILON_V1.to_bits()
        || weight_decay.to_bits() != ADAM_WEIGHT_DECAY_V1.to_bits()
        || optimization.amsgrad
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_schedule_v2(
    schedule: &TrainRunScheduleV2,
    snapshot: &CommonModelSnapshotRecordV1,
) -> Result<u64> {
    let k = schedule.batch_episodes;
    let s = schedule.checkpoint_segment_updates;
    let n = schedule.requested_successful_updates;
    if !is_u63(schedule.base_seed)
        || schedule.base_seed == snapshot.base_seed
        || !(NATIVE_TRAINER_MIN_BATCH_EPISODES_V2..=NATIVE_TRAINER_MAX_BATCH_EPISODES_V2)
            .contains(&k)
        || !k.is_multiple_of(2)
        || !(1..=MAX_SUCCESSFUL_UPDATES_V2).contains(&s)
        || !(s..=MAX_SUCCESSFUL_UPDATES_V2).contains(&n)
        || !n.is_multiple_of(s)
        || schedule.measurement_mode != "fixed-successful-updates/v1"
        || schedule.learner_seat_rule != "p0-even-p1-odd/v1"
        || schedule.paired_environment_seed_rule != "episodes-2k-and-2k-plus-1-share-pair-seed/v1"
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    let checkpoint_episode_interval = checked_u63_mul(k, s)?;
    let requested_episode_count = checked_u63_mul(k, n)?;
    if schedule.checkpoint_episode_interval != checkpoint_episode_interval {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::CrossBinding));
    }
    Ok(requested_episode_count)
}

fn validate_limits_v2(limits: &TrainRunLimitsV2) -> Result<()> {
    if !is_positive_u63(limits.max_physical_decisions)
        || !is_positive_u63(limits.max_policy_steps)
        || limits.max_physical_decisions > limits.max_policy_steps
        || limits.max_policy_steps > MAX_POLICY_STEPS_V2
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    Ok(())
}

fn validate_topology_v2(topology: &TrainRunTopologyV2) -> Result<()> {
    if !(1..=16).contains(&topology.worker_count)
        || !(1..=64).contains(&topology.sessions_per_worker)
        || !is_positive_u63(topology.scheduler_timeout_ms)
        || topology.measure_broker_service_time
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    let logical_actor_count = checked_u63_mul(topology.worker_count, topology.sessions_per_worker)?;
    if topology.logical_actor_count != logical_actor_count
        || !(1..=logical_actor_count).contains(&topology.broker_batch_target)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::CrossBinding));
    }
    Ok(())
}

fn validate_artifact_schemas_v2(schemas: &TrainRunArtifactSchemasV2) -> Result<()> {
    if schemas.run != TRAIN_RUN_SCHEMA_V2
        || schemas.episode != "mtg_kernel_native_train_episode/v1"
        || schemas.update_evidence != "mtg_kernel_native_train_update_evidence/v1"
        || schemas.segment != "mtg_kernel_native_train_checkpoint_segment/v2"
        || schemas.segment_continuation != "mtg_kernel_native_train_segment_continuation/v2"
        || schemas.checkpoint != "mtg_kernel_native_train_checkpoint/v3"
        || schemas.state_payload != NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1
        || schemas.sidecar != "mtg_kernel_native_train_checkpoint_sidecar/v2"
        || schemas.head != "mtg_kernel_native_train_head/v2"
        || schemas.latest != "mtg_kernel_native_train_latest/v2"
        || schemas.checkpoint_ref != "mtg_kernel_native_checkpoint_ref/v2"
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_publication_v2(publication: &TrainRunPublicationV2) -> Result<()> {
    if publication.canonical_json != "canonical-sorted-ascii-json-lf/v1"
        || publication.state_payload != NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1
        || publication.segment_boundary != "s-successful-updates/v1"
        || publication.same_parent_stage != "fixed-dot-basename-stage-v2/v1"
        || !publication.latest_published_last
        || !publication.windows_only
        || publication.observed_timing_fields_in_deterministic_store
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_nonclaims_v2(nonclaims: &[String; 8]) -> Result<()> {
    const EXPECTED: [&str; 8] = [
        "rust-seeded-initializer-not-reproduced",
        "not-decimal-softmax-hamilton-splitmix64-v1",
        "not-cross-platform-numerical-bit-equality",
        "not-power-loss-durability",
        "not-linux-store-durability",
        "not-xmage-speedup-evidence",
        "rally-mirror-only",
        "not-nine-deck-or-science-ready-evidence",
    ];
    if nonclaims.each_ref().map(String::as_str) != EXPECTED {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_cross_bindings_v2(record: &TrainRunV2) -> Result<()> {
    let contracts = &record.contracts;
    let snapshot = &record.model_snapshot;
    if record.runtime.numerical_backend_identity != contracts.train_step.numerical_backend_identity
        || snapshot.feature_contract_digest != contracts.tensorizer.feature_contract_digest
        || snapshot.feature_encoding_digest != contracts.tensorizer.feature_encoding_digest
        || snapshot.model_architecture_version != contracts.model.architecture_identity
        || snapshot.model_config_fingerprint != contracts.model.config_fingerprint
        || snapshot.parameter_layout_sha256 != contracts.model.parameter_layout_sha256
        || snapshot.parameter_tensor_count != contracts.model.parameter_tensor_count
        || snapshot.parameter_element_count != contracts.model.parameter_element_count
        || snapshot.optimizer_identity != contracts.optimizer.identity
        || snapshot.optimizer_identity != contracts.optimizer.gauge_identity
        || snapshot.canonical_gauge_parameters != contracts.optimizer.canonical_gauge_parameters
        || snapshot.trainer_schedule_version != contracts.trainer_schedule.identity
        || snapshot.python_reference_seed_version
            != contracts.trainer_schedule.python_reference_seed_identity
        || snapshot.schedule_goldens_sha256 != contracts.trainer_schedule.goldens_sha256
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::CrossBinding));
    }
    Ok(())
}

fn reconstruct_standalone_semantics_core_v2(
    record: &TrainRunV2,
    requested_episode_count: u64,
) -> Result<StandaloneSemanticsCoreV2> {
    let checkpoint_episode_interval = checked_u63_mul(
        record.schedule.batch_episodes,
        record.schedule.checkpoint_segment_updates,
    )?;
    Ok(StandaloneSemanticsCoreV2 {
        identity: STANDALONE_SEMANTICS_IDENTITY_V2.to_owned(),
        snapshot: StandaloneSnapshotSemanticsV2 {
            identity: record.model_snapshot.identity.clone(),
            snapshot_sha256: record.model_snapshot.snapshot_sha256.clone(),
            manifest_file_sha256: record.model_snapshot.manifest_file_sha256.clone(),
            payload_sha256: record.model_snapshot.payload_sha256.clone(),
            payload_byte_count: record.model_snapshot.payload_byte_count,
            parameter_layout_sha256: record.model_snapshot.parameter_layout_sha256.clone(),
            named_parameter_stream_sha256: record
                .model_snapshot
                .named_parameter_stream_sha256
                .clone(),
            model_config_fingerprint: record.model_snapshot.model_config_fingerprint.clone(),
            scorer_bias_anchor_f32_bits: record.model_snapshot.scorer_bias_anchor_f32_bits,
            optimizer_identity: record.model_snapshot.optimizer_identity.clone(),
            adam_step_initial: record.model_snapshot.adam_step_initial,
        },
        tensorizer: record.contracts.tensorizer.clone(),
        model: record.contracts.model.clone(),
        loss: StandaloneLossSemanticsV2 {
            identity: record.contracts.loss.identity.clone(),
            value_coefficient_f32_bits: record.optimization.value_coefficient_f32_bits.clone(),
        },
        train_step: record.contracts.train_step.clone(),
        optimizer: StandaloneOptimizerSemanticsV2 {
            identity: record.contracts.optimizer.identity.clone(),
            gauge_identity: record.contracts.optimizer.gauge_identity.clone(),
            gauge_evidence_identity: record.contracts.optimizer.gauge_evidence_identity.clone(),
            canonical_gauge_parameters: record
                .contracts
                .optimizer
                .canonical_gauge_parameters
                .clone(),
            learning_rate_f32_bits: record.optimization.learning_rate_f32_bits.clone(),
            beta1_f32_bits: record.optimization.beta1_f32_bits.clone(),
            beta2_f32_bits: record.optimization.beta2_f32_bits.clone(),
            epsilon_f32_bits: record.optimization.epsilon_f32_bits.clone(),
            weight_decay_f32_bits: record.optimization.weight_decay_f32_bits.clone(),
            amsgrad: record.optimization.amsgrad,
        },
        learner_sampler: record.contracts.learner_sampler.clone(),
        opponent_policy: record.contracts.opponent_policy.clone(),
        opponent_sampler: record.contracts.opponent_sampler.clone(),
        schedule: StandaloneScheduleSemanticsV2 {
            identity: record.contracts.trainer_schedule.identity.clone(),
            python_reference_seed_identity: record
                .contracts
                .trainer_schedule
                .python_reference_seed_identity
                .clone(),
            base_seed: record.schedule.base_seed,
            environment_seed_derivation_identity: record
                .contracts
                .trainer_schedule
                .environment_seed_derivation_identity
                .clone(),
            learner_action_seed_derivation_identity: record
                .contracts
                .trainer_schedule
                .learner_action_seed_derivation_identity
                .clone(),
            opponent_action_seed_derivation_identity: record
                .contracts
                .trainer_schedule
                .opponent_action_seed_derivation_identity
                .clone(),
            learner_seat_rule: record.schedule.learner_seat_rule.clone(),
            paired_environment_seed_rule: record.schedule.paired_environment_seed_rule.clone(),
            goldens_sha256: record.contracts.trainer_schedule.goldens_sha256.clone(),
        },
        trajectory: record.contracts.trajectory.clone(),
        environment: record.environment.clone(),
        workload: StandaloneWorkloadSemanticsV2 {
            batch_episodes: record.schedule.batch_episodes,
            checkpoint_segment_updates: record.schedule.checkpoint_segment_updates,
            checkpoint_episode_interval,
            requested_successful_updates: record.schedule.requested_successful_updates,
            requested_episode_count,
            max_physical_decisions: record.limits.max_physical_decisions,
            max_policy_steps: record.limits.max_policy_steps,
            measurement_mode: "fixed-successful-updates/v1".to_owned(),
            durability_semantics: "checkpoint-segment-replay-at-most-k-times-s-episodes/v1"
                .to_owned(),
        },
    })
}

fn standalone_semantics_sha256_v2(core: &StandaloneSemanticsCoreV2) -> Result<String> {
    let bytes = to_canonical_json_bytes_v1(core, CanonicalJsonNullPolicyV1::Forbid)?;
    Ok(sha256_hex(&bytes))
}

fn identity_bundle_sha256_v2(record: &TrainRunV2) -> Result<String> {
    let config_fingerprint = decode_raw32(&record.contracts.model.config_fingerprint)?;
    let standalone_semantics = decode_raw32(&record.contracts.standalone_semantics.sha256)?;
    let k = record.schedule.batch_episodes.to_be_bytes();
    let s = record.schedule.checkpoint_segment_updates.to_be_bytes();
    let atoms = [
        prepare_atom_v2("domain", IDENTITY_BUNDLE_IDENTITY_V2.as_bytes())?,
        prepare_atom_v2(
            "architecture_identity_utf8",
            record.contracts.model.architecture_identity.as_bytes(),
        )?,
        prepare_atom_v2("config_fingerprint_raw32", &config_fingerprint)?,
        prepare_atom_v2(
            "train_step_identity_utf8",
            record.contracts.train_step.identity.as_bytes(),
        )?,
        prepare_atom_v2(
            "numerical_backend_identity_utf8",
            record.runtime.numerical_backend_identity.as_bytes(),
        )?,
        prepare_atom_v2(
            "learner_sampler_identity_utf8",
            record.contracts.learner_sampler.identity.as_bytes(),
        )?,
        prepare_atom_v2(
            "opponent_sampler_identity_utf8",
            record.contracts.opponent_sampler.identity.as_bytes(),
        )?,
        prepare_atom_v2(
            "schedule_identity_utf8",
            record.contracts.trainer_schedule.identity.as_bytes(),
        )?,
        prepare_atom_v2("batch_episodes_u64be", &k)?,
        prepare_atom_v2("checkpoint_segment_updates_u64be", &s)?,
        prepare_atom_v2(
            "optimizer_identity_utf8",
            record.contracts.optimizer.identity.as_bytes(),
        )?,
        prepare_atom_v2(
            "optimizer_gauge_identity_utf8",
            record.contracts.optimizer.gauge_identity.as_bytes(),
        )?,
        prepare_atom_v2(
            "snapshot_identity_utf8",
            record.model_snapshot.identity.as_bytes(),
        )?,
        prepare_atom_v2("standalone_semantics_sha256_raw32", &standalone_semantics)?,
    ];
    let framed_len = atoms.iter().try_fold(0_usize, |total, atom| {
        total
            .checked_add(atom.encoded_len)
            .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))
    })?;
    let mut framed = Vec::with_capacity(framed_len);
    for atom in atoms {
        atom.append_to(&mut framed);
    }
    debug_assert_eq!(framed.len(), framed_len);
    Ok(sha256_hex(&framed))
}

struct PreparedAtomV2<'a> {
    tag_len: [u8; 4],
    tag: &'a [u8],
    payload_len: [u8; 8],
    payload: &'a [u8],
    encoded_len: usize,
}

impl PreparedAtomV2<'_> {
    fn append_to(self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.tag_len);
        output.extend_from_slice(self.tag);
        output.extend_from_slice(&self.payload_len);
        output.extend_from_slice(self.payload);
    }
}

fn prepare_atom_v2<'a>(tag: &'a str, payload: &'a [u8]) -> Result<PreparedAtomV2<'a>> {
    let tag_len = u32::try_from(tag.len())
        .map_err(|_| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))?;
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))?;
    let capacity = 4_usize
        .checked_add(tag.len())
        .and_then(|value| value.checked_add(8))
        .and_then(|value| value.checked_add(payload.len()))
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))?;
    Ok(PreparedAtomV2 {
        tag_len: tag_len.to_be_bytes(),
        tag: tag.as_bytes(),
        payload_len: payload_len.to_be_bytes(),
        payload,
        encoded_len: capacity,
    })
}

fn checked_u63_mul(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| is_u63(*value))
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))
}

fn is_u63(value: u64) -> bool {
    value <= U63_MAX
}

fn is_positive_u63(value: u64) -> bool {
    value != 0 && is_u63(value)
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_sha256(value: &str) -> bool {
    is_lower_hex(value, 64)
}

fn decode_raw32(value: &str) -> Result<[u8; 32]> {
    if !is_sha256(value) {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    let mut result = [0_u8; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        let high = decode_hex_nibble(value.as_bytes()[index * 2])?;
        let low = decode_hex_nibble(value.as_bytes()[index * 2 + 1])?;
        *byte = (high << 4) | low;
    }
    Ok(result)
}

fn decode_f32_hex(value: &str) -> Result<f32> {
    if !is_lower_hex(value, 8) {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    let mut bytes = [0_u8; 4];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let high = decode_hex_nibble(value.as_bytes()[index * 2])?;
        let low = decode_hex_nibble(value.as_bytes()[index * 2 + 1])?;
        *byte = (high << 4) | low;
    }
    Ok(f32::from_bits(u32::from_be_bytes(bytes)))
}

fn decode_hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar)),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_nonempty_printable_ascii(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_windows_msvc_triple(value: &str) -> bool {
    matches!(value, "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc")
}

fn is_valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let parse = |start: usize, end: usize| -> Option<u32> {
        std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()
    };
    let Some(year) = parse(0, 4) else {
        return false;
    };
    let Some(month) = parse(5, 7) else {
        return false;
    };
    let Some(day) = parse(8, 10) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year != 0 && (1..=max_day).contains(&day)
}

fn is_semver(value: &str) -> bool {
    let (without_build, build) = value
        .split_once('+')
        .map_or((value, None), |(left, right)| (left, Some(right)));
    if build.is_some_and(|identifiers| !valid_semver_identifiers(identifiers, false)) {
        return false;
    }
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(left, right)| (left, Some(right)));
    if prerelease.is_some_and(|identifiers| !valid_semver_identifiers(identifiers, true)) {
        return false;
    }
    let mut components = core.split('.');
    let valid_numeric = |component: &str| {
        !component.is_empty()
            && component.bytes().all(|byte| byte.is_ascii_digit())
            && (component == "0" || !component.starts_with('0'))
    };
    let valid = components.by_ref().take(3).all(valid_numeric);
    valid && components.next().is_none() && core.matches('.').count() == 2
}

fn valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!reject_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || identifier == "0"
                    || !identifier.starts_with('0'))
        })
}

fn looks_private_location(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let bytes = value.as_bytes();
    value.starts_with('/')
        || bytes.windows(2).any(|window| window == b"\\\\")
        || bytes.windows(3).any(|window| {
            window[0].is_ascii_alphabetic()
                && window[1] == b':'
                && matches!(window[2], b'\\' | b'/')
        })
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| *byte == b'/' && is_location_boundary(bytes, index))
        || ["file:", "http:", "https:"]
            .iter()
            .any(|scheme| contains_at_location_boundary(lower.as_bytes(), scheme.as_bytes()))
}

fn is_location_boundary(bytes: &[u8], index: usize) -> bool {
    index == 0
        || !matches!(
            bytes[index - 1],
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.' | b'-'
        )
}

fn contains_at_location_boundary(bytes: &[u8], needle: &[u8]) -> bool {
    bytes
        .windows(needle.len())
        .enumerate()
        .any(|(index, window)| window == needle && is_location_boundary(bytes, index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    fn empty_semantics_core() -> StandaloneSemanticsCoreV2 {
        StandaloneSemanticsCoreV2 {
            identity: String::new(),
            snapshot: StandaloneSnapshotSemanticsV2 {
                identity: String::new(),
                snapshot_sha256: ZERO_SHA256.to_owned(),
                manifest_file_sha256: ZERO_SHA256.to_owned(),
                payload_sha256: ZERO_SHA256.to_owned(),
                payload_byte_count: 0,
                parameter_layout_sha256: ZERO_SHA256.to_owned(),
                named_parameter_stream_sha256: ZERO_SHA256.to_owned(),
                model_config_fingerprint: ZERO_SHA256.to_owned(),
                scorer_bias_anchor_f32_bits: 0,
                optimizer_identity: String::new(),
                adam_step_initial: 0,
            },
            tensorizer: TensorizerContractV2 {
                identity: String::new(),
                feature_contract_digest: ZERO_SHA256.to_owned(),
                feature_encoding_digest: ZERO_SHA256.to_owned(),
                authoritative_features_source_sha256: ZERO_SHA256.to_owned(),
                fixture_sha256: ZERO_SHA256.to_owned(),
                fixture_payload_sha256: ZERO_SHA256.to_owned(),
            },
            model: ModelContractV2 {
                architecture_identity: String::new(),
                config_fingerprint: ZERO_SHA256.to_owned(),
                parameter_layout_sha256: ZERO_SHA256.to_owned(),
                parameter_tensor_count: 0,
                parameter_element_count: 0,
            },
            loss: StandaloneLossSemanticsV2 {
                identity: String::new(),
                value_coefficient_f32_bits: "00000000".to_owned(),
            },
            train_step: TrainStepContractV2 {
                identity: String::new(),
                numerical_backend_identity: String::new(),
            },
            optimizer: StandaloneOptimizerSemanticsV2 {
                identity: String::new(),
                gauge_identity: String::new(),
                gauge_evidence_identity: String::new(),
                canonical_gauge_parameters: [String::new()],
                learning_rate_f32_bits: "00000000".to_owned(),
                beta1_f32_bits: "00000000".to_owned(),
                beta2_f32_bits: "00000000".to_owned(),
                epsilon_f32_bits: "00000000".to_owned(),
                weight_decay_f32_bits: "00000000".to_owned(),
                amsgrad: false,
            },
            learner_sampler: LearnerSamplerContractV2 {
                identity: String::new(),
                contract_sha256: ZERO_SHA256.to_owned(),
                exp_table_sha256: ZERO_SHA256.to_owned(),
                cross_language_vectors_file_sha256: ZERO_SHA256.to_owned(),
                cross_language_vector_stream_sha256: ZERO_SHA256.to_owned(),
            },
            opponent_policy: OpponentPolicyContractV2 {
                identity: String::new(),
                model_rule: String::new(),
            },
            opponent_sampler: OpponentSamplerContractV2 {
                identity: String::new(),
                algorithm: String::new(),
                seed_derivation_identity: String::new(),
                seed_goldens_sha256: ZERO_SHA256.to_owned(),
                cross_language_vectors_file_sha256: ZERO_SHA256.to_owned(),
                cross_language_vector_stream_sha256: ZERO_SHA256.to_owned(),
                width_one_consumes_seed: false,
            },
            schedule: StandaloneScheduleSemanticsV2 {
                identity: String::new(),
                python_reference_seed_identity: String::new(),
                base_seed: 0,
                environment_seed_derivation_identity: String::new(),
                learner_action_seed_derivation_identity: String::new(),
                opponent_action_seed_derivation_identity: String::new(),
                learner_seat_rule: String::new(),
                paired_environment_seed_rule: String::new(),
                goldens_sha256: ZERO_SHA256.to_owned(),
            },
            trajectory: TrajectoryContractV2 {
                identity: String::new(),
                cross_language_goldens_schema: String::new(),
                cross_language_generator_identity: String::new(),
                cross_language_golden_stream_identity: String::new(),
                cross_language_goldens_file_sha256: ZERO_SHA256.to_owned(),
                cross_language_golden_stream_sha256: ZERO_SHA256.to_owned(),
            },
            environment: TrainRunEnvironmentV2 {
                card_db_hash_u64_hex: "0000000000000000".to_owned(),
                runtime_catalog_schema: String::new(),
                runtime_catalog_protocol: String::new(),
                runtime_catalog_sha256: ZERO_SHA256.to_owned(),
                deck_ids: [String::new(), String::new()],
                deck_hashes_u64_hex: ["0000000000000000".to_owned(), "0000000000000000".to_owned()],
                protocol: String::new(),
                protocol_version: 0,
                schema_version: 0,
                kernel_version: String::new(),
                surface_version: 0,
                policy_surface_version: 0,
            },
            workload: StandaloneWorkloadSemanticsV2 {
                batch_episodes: 0,
                checkpoint_segment_updates: 0,
                checkpoint_episode_interval: 0,
                requested_successful_updates: 0,
                requested_episode_count: 0,
                max_physical_decisions: 0,
                max_policy_steps: 0,
                measurement_mode: String::new(),
                durability_semantics: String::new(),
            },
        }
    }

    fn fixture_record() -> TrainRunV2 {
        let value = json!({
            "schema": TRAIN_RUN_SCHEMA_V2,
            "store_identity": NATIVE_TRAINING_STORE_IDENTITY_V2,
            "package": {
                "name": "mtg-kernel",
                "version": env!("CARGO_PKG_VERSION"),
                "workspace_manifest_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
                "crate_manifest_sha256": "2222222222222222222222222222222222222222222222222222222222222222",
                "cargo_lock_sha256": "3333333333333333333333333333333333333333333333333333333333333333",
                "enabled_features": ["native-training-store-v2-production"]
            },
            "toolchain": {
                "capture_identity": "rustc-verbose-version-build-embed-v1",
                "rustc_release": "1.94.1",
                "rustc_commit_hash": "4444444444444444444444444444444444444444",
                "rustc_commit_date": "2026-06-01",
                "host_triple": "x86_64-pc-windows-msvc",
                "target_triple": "x86_64-pc-windows-msvc",
                "llvm_version": "20.1.8",
                "rustc_verbose_version_sha256": "5555555555555555555555555555555555555555555555555555555555555555",
                "rustc_verbose_version_line_ending": "crlf",
                "build_profile": "release"
            },
            "source": {
                "git_commit": "6666666666666666666666666666666666666666",
                "source_tree_recipe_identity": SOURCE_TREE_RECIPE_IDENTITY_V1,
                "source_tree_recipe_sha256": SOURCE_TREE_RECIPE_SHA256_V1,
                "source_tree_recipe_byte_count": 5847,
                "source_tree_sha256": "7777777777777777777777777777777777777777777777777777777777777777",
                "worktree_clean": true,
                "git_status_sha256": EMPTY_SHA256,
                "executable_capture_identity": "windows-current-module-path-file-v2",
                "binary_name": "mtg-kernel-native.exe",
                "binary_sha256": "8888888888888888888888888888888888888888888888888888888888888888",
                "binary_byte_len": 123456,
                "binary_volume_serial_u64_hex": "0123456789abcdef",
                "binary_file_id_128_hex": "0123456789abcdef0123456789abcdef",
                "binary_pe_size_of_image_bytes": 131072,
                "capture_scope": "module-path-file-not-loaded-section-provenance/v1"
            },
            "runtime": {
                "tuple_identity": "mtg-kernel-native-windows-cpu-runtime-tuple-v1",
                "os_capture_identity": "windows-rtlgetversion-native-system-info-v1",
                "os_system": "windows",
                "os_major": 10,
                "os_minor": 0,
                "os_build": 26100,
                "service_pack_major": 0,
                "service_pack_minor": 0,
                "product_type": 1,
                "suite_mask_u16_hex": "0100",
                "native_architecture": "amd64",
                "process_architecture": "amd64",
                "byte_order": "little",
                "numerical_backend_identity": NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1,
                "rustc_release": "1.94.1",
                "rustc_commit_hash": "4444444444444444444444444444444444444444",
                "target_triple": "x86_64-pc-windows-msvc",
                "build_profile": "release"
            },
            "environment": {
                "card_db_hash_u64_hex": "a06fa9566106f0ea",
                "runtime_catalog_schema": "kernel_runtime_decks/v1",
                "runtime_catalog_protocol": "canonical-mainboard-bo1/v1",
                "runtime_catalog_sha256": RUNTIME_CATALOG_SHA256_V1,
                "deck_ids": ["Rally", "Rally"],
                "deck_hashes_u64_hex": ["0c9f01c2544412bf", "0c9f01c2544412bf"],
                "protocol": "kernel_rl_jsonl",
                "protocol_version": 5,
                "schema_version": 5,
                "kernel_version": "0.0.4-spike",
                "surface_version": 2,
                "policy_surface_version": 5
            },
            "contracts": {
                "trainer_identity": "mtg-kernel-native-even-batch-trainer-v2",
                "identity_bundle_identity": IDENTITY_BUNDLE_IDENTITY_V2,
                "identity_bundle_sha256": ZERO_SHA256,
                "tensorizer": {
                    "identity": TENSORIZER_IDENTITY_V2,
                    "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V1,
                    "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V1,
                    "authoritative_features_source_sha256": TENSORIZER_AUTHORITY_SOURCE_SHA256_V2,
                    "fixture_sha256": TENSORIZER_FIXTURE_SHA256_V2,
                    "fixture_payload_sha256": TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2
                },
                "model": {
                    "architecture_identity": MODEL_ARCHITECTURE_VERSION_V1,
                    "config_fingerprint": MODEL_CONFIG_FINGERPRINT_V1,
                    "parameter_layout_sha256": PARAMETER_LAYOUT_SHA256_V1,
                    "parameter_tensor_count": 33,
                    "parameter_element_count": 1230994
                },
                "loss": {"identity": TRAINER_ALGORITHM_V1},
                "train_step": {
                    "identity": TRAIN_STEP_IDENTITY_V1,
                    "numerical_backend_identity": NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1
                },
                "optimizer": {
                    "identity": NATIVE_OPTIMIZER_IDENTITY_V1,
                    "gauge_identity": NATIVE_OPTIMIZER_IDENTITY_V1,
                    "gauge_evidence_identity": "mtg-kernel-native-scorer-bias-gauge-evidence-v1",
                    "canonical_gauge_parameters": ["scorer.2.bias"]
                },
                "trainer_schedule": {
                    "identity": NATIVE_TRAINER_SCHEDULE_VERSION_V1,
                    "python_reference_seed_identity": PYTHON_REFERENCE_SEED_VERSION_V1,
                    "environment_seed_derivation_identity": "train-env/base_seed/pair_index",
                    "learner_action_seed_derivation_identity": "train-learner-action-group/base_seed/episode_index/learner_physical_decision_index -> train-learner-action-substep/group_seed/substep_index",
                    "opponent_action_seed_derivation_identity": "train-opponent-action-group/base_seed/episode_index/opponent_physical_decision_index -> train-opponent-action-substep/group_seed/substep_index",
                    "goldens_sha256": NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
                },
                "learner_sampler": {
                    "identity": FAST_CATEGORICAL_SAMPLER_VERSION,
                    "contract_sha256": FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                    "exp_table_sha256": FAST_CATEGORICAL_EXP_TABLE_SHA256,
                    "cross_language_vectors_file_sha256": LEARNER_VECTORS_FILE_SHA256_V1,
                    "cross_language_vector_stream_sha256": LEARNER_VECTOR_STREAM_SHA256_V1
                },
                "opponent_policy": {
                    "identity": "mtg-kernel-trainer-uniform-policy-v1",
                    "model_rule": "no-model-uniform-legal-index"
                },
                "opponent_sampler": {
                    "identity": UNIFORM_INDEX_MODULO_U64_IDENTITY_V1,
                    "algorithm": UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1,
                    "seed_derivation_identity": "train-opponent-action-group/base_seed/episode_index/opponent_physical_decision_index -> train-opponent-action-substep/group_seed/substep_index",
                    "seed_goldens_sha256": NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
                    "cross_language_vectors_file_sha256": OPPONENT_VECTORS_FILE_SHA256_V1,
                    "cross_language_vector_stream_sha256": OPPONENT_VECTOR_STREAM_SHA256_V1,
                    "width_one_consumes_seed": true
                },
                "trajectory": {
                    "identity": NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1,
                    "cross_language_goldens_schema": "mtg_kernel_native_full_episode_trajectory_goldens/v1",
                    "cross_language_generator_identity": "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1",
                    "cross_language_golden_stream_identity": "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1",
                    "cross_language_goldens_file_sha256": TRAJECTORY_GOLDENS_FILE_SHA256_V1,
                    "cross_language_golden_stream_sha256": TRAJECTORY_GOLDEN_STREAM_SHA256_V1
                },
                "standalone_semantics": {
                    "identity": STANDALONE_SEMANTICS_IDENTITY_V2,
                    "core": empty_semantics_core(),
                    "sha256": ZERO_SHA256
                }
            },
            "model_snapshot": {
                "schema": SNAPSHOT_SCHEMA_V1,
                "identity": SNAPSHOT_IDENTITY_V1,
                "snapshot_sha256": SNAPSHOT_SHA256_V1,
                "manifest_file_sha256": SNAPSHOT_MANIFEST_FILE_SHA256_V1,
                "manifest_core_sha256": SNAPSHOT_MANIFEST_CORE_SHA256_V1,
                "payload_sha256": SNAPSHOT_PAYLOAD_SHA256_V1,
                "payload_byte_count": 4923976,
                "parameter_layout_sha256": PARAMETER_LAYOUT_SHA256_V1,
                "named_parameter_stream_sha256": SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1,
                "loaded_named_parameter_stream_sha256": SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1,
                "parameter_tensor_count": 33,
                "parameter_element_count": 1230994,
                "model_config_fingerprint": MODEL_CONFIG_FINGERPRINT_V1,
                "model_architecture_version": MODEL_ARCHITECTURE_VERSION_V1,
                "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V1,
                "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V1,
                "initializer_identity": INITIALIZER_IDENTITY_V1,
                "base_seed": 0,
                "model_init_seed": 6443515232517447393_u64,
                "trainer_schedule_version": NATIVE_TRAINER_SCHEDULE_VERSION_V1,
                "python_reference_seed_version": PYTHON_REFERENCE_SEED_VERSION_V1,
                "schedule_goldens_sha256": NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
                "authority_source_bundle_sha256": SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V1,
                "authority_runtime_identity": AUTHORITY_RUNTIME_IDENTITY_V1,
                "loader_identity": RUST_LOADER_IDENTITY_V1,
                "optimizer_identity": NATIVE_OPTIMIZER_IDENTITY_V1,
                "adam_step_initial": 0,
                "moment_initialization": "positive-zero-f32",
                "canonical_gauge_parameters": ["scorer.2.bias"],
                "scorer_bias_anchor_f32_bits": 3141403366_u64,
                "snapshot_load_completed_before_trial_start": true,
                "snapshot_load_timed": false,
                "rust_seeded_initializer_reproduced": false,
                "nonclaim": NONCLAIM_V1
            },
            "optimization": {
                "learning_rate_f32_bits": format!("{:08x}", 0.001_f32.to_bits()),
                "value_coefficient_f32_bits": format!("{:08x}", 0.5_f32.to_bits()),
                "beta1_f32_bits": format!("{:08x}", ADAM_BETA1_V1.to_bits()),
                "beta2_f32_bits": format!("{:08x}", ADAM_BETA2_V1.to_bits()),
                "epsilon_f32_bits": format!("{:08x}", ADAM_EPSILON_V1.to_bits()),
                "weight_decay_f32_bits": format!("{:08x}", ADAM_WEIGHT_DECAY_V1.to_bits()),
                "amsgrad": false
            },
            "schedule": {
                "base_seed": 71501,
                "batch_episodes": 2,
                "checkpoint_segment_updates": 4,
                "requested_successful_updates": 12,
                "checkpoint_episode_interval": 8,
                "measurement_mode": "fixed-successful-updates/v1",
                "learner_seat_rule": "p0-even-p1-odd/v1",
                "paired_environment_seed_rule": "episodes-2k-and-2k-plus-1-share-pair-seed/v1"
            },
            "limits": {"max_physical_decisions": 32768, "max_policy_steps": 65536},
            "topology": {
                "worker_count": 2,
                "sessions_per_worker": 4,
                "logical_actor_count": 8,
                "broker_batch_target": 8,
                "scheduler_timeout_ms": 30000,
                "measure_broker_service_time": false
            },
            "artifact_schemas": {
                "run": TRAIN_RUN_SCHEMA_V2,
                "episode": "mtg_kernel_native_train_episode/v1",
                "update_evidence": "mtg_kernel_native_train_update_evidence/v1",
                "segment": "mtg_kernel_native_train_checkpoint_segment/v2",
                "segment_continuation": "mtg_kernel_native_train_segment_continuation/v2",
                "checkpoint": "mtg_kernel_native_train_checkpoint/v3",
                "state_payload": "mtg_kernel_native_train_state_payload/v1",
                "sidecar": "mtg_kernel_native_train_checkpoint_sidecar/v2",
                "head": "mtg_kernel_native_train_head/v2",
                "latest": "mtg_kernel_native_train_latest/v2",
                "checkpoint_ref": "mtg_kernel_native_checkpoint_ref/v2"
            },
            "publication": {
                "canonical_json": "canonical-sorted-ascii-json-lf/v1",
                "state_payload": "ordered-three-section-f32le/v1",
                "segment_boundary": "s-successful-updates/v1",
                "same_parent_stage": "fixed-dot-basename-stage-v2/v1",
                "latest_published_last": true,
                "windows_only": true,
                "observed_timing_fields_in_deterministic_store": false
            },
            "nonclaims": [
                "rust-seeded-initializer-not-reproduced",
                "not-decimal-softmax-hamilton-splitmix64-v1",
                "not-cross-platform-numerical-bit-equality",
                "not-power-loss-durability",
                "not-linux-store-durability",
                "not-xmage-speedup-evidence",
                "rally-mirror-only",
                "not-nine-deck-or-science-ready-evidence"
            ]
        });
        let wire: TrainRunWireV2 = serde_json::from_value(value).unwrap();
        let mut record = TrainRunV2::from(wire);
        refresh_derived(&mut record);
        record
    }

    fn refresh_derived(record: &mut TrainRunV2) {
        let requested_episode_count = record
            .schedule
            .batch_episodes
            .checked_mul(record.schedule.requested_successful_updates)
            .unwrap();
        let core =
            reconstruct_standalone_semantics_core_v2(record, requested_episode_count).unwrap();
        record.contracts.standalone_semantics.core = core;
        record.contracts.standalone_semantics.sha256 =
            standalone_semantics_sha256_v2(&record.contracts.standalone_semantics.core).unwrap();
        record.contracts.identity_bundle_sha256 = identity_bundle_sha256_v2(record).unwrap();
    }

    fn fixture_bytes() -> Vec<u8> {
        to_canonical_json_bytes_v1(&fixture_record(), CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    fn assert_record_error(record: TrainRunV2, expected: TrainRunV2ErrorKind) {
        assert_eq!(
            validate_train_run_record_v2(record).unwrap_err().kind(),
            expected
        );
    }

    fn canonical_value_bytes(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    fn reference_canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
        fn emit(value: &Value, output: &mut String) {
            match value {
                Value::Null => output.push_str("null"),
                Value::Bool(boolean) => output.push_str(if *boolean { "true" } else { "false" }),
                Value::Number(number) => output.push_str(&number.to_string()),
                Value::String(string) => output.push_str(&serde_json::to_string(string).unwrap()),
                Value::Array(values) => {
                    output.push('[');
                    for (index, value) in values.iter().enumerate() {
                        if index != 0 {
                            output.push(',');
                        }
                        emit(value, output);
                    }
                    output.push(']');
                }
                Value::Object(values) => {
                    output.push('{');
                    let mut entries: Vec<_> = values.iter().collect();
                    entries
                        .sort_unstable_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
                    for (index, (key, value)) in entries.into_iter().enumerate() {
                        if index != 0 {
                            output.push(',');
                        }
                        output.push_str(&serde_json::to_string(key).unwrap());
                        output.push(':');
                        emit(value, output);
                    }
                    output.push('}');
                }
            }
        }
        let value = serde_json::to_value(value).unwrap();
        let mut output = String::new();
        emit(&value, &mut output);
        output.push('\n');
        output.into_bytes()
    }

    fn reference_identity_bundle(record: &TrainRunV2) -> String {
        fn append_atom(bytes: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            bytes.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            bytes.extend_from_slice(tag.as_bytes());
            bytes.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            bytes.extend_from_slice(payload);
        }
        let config = decode_raw32(&record.contracts.model.config_fingerprint).unwrap();
        let semantics = decode_raw32(&record.contracts.standalone_semantics.sha256).unwrap();
        let mut bytes = Vec::new();
        for (tag, payload) in [
            ("domain", IDENTITY_BUNDLE_IDENTITY_V2.as_bytes()),
            (
                "architecture_identity_utf8",
                record.contracts.model.architecture_identity.as_bytes(),
            ),
            ("config_fingerprint_raw32", config.as_slice()),
            (
                "train_step_identity_utf8",
                record.contracts.train_step.identity.as_bytes(),
            ),
            (
                "numerical_backend_identity_utf8",
                record.runtime.numerical_backend_identity.as_bytes(),
            ),
            (
                "learner_sampler_identity_utf8",
                record.contracts.learner_sampler.identity.as_bytes(),
            ),
            (
                "opponent_sampler_identity_utf8",
                record.contracts.opponent_sampler.identity.as_bytes(),
            ),
            (
                "schedule_identity_utf8",
                record.contracts.trainer_schedule.identity.as_bytes(),
            ),
            (
                "batch_episodes_u64be",
                record.schedule.batch_episodes.to_be_bytes().as_slice(),
            ),
            (
                "checkpoint_segment_updates_u64be",
                record
                    .schedule
                    .checkpoint_segment_updates
                    .to_be_bytes()
                    .as_slice(),
            ),
            (
                "optimizer_identity_utf8",
                record.contracts.optimizer.identity.as_bytes(),
            ),
            (
                "optimizer_gauge_identity_utf8",
                record.contracts.optimizer.gauge_identity.as_bytes(),
            ),
            (
                "snapshot_identity_utf8",
                record.model_snapshot.identity.as_bytes(),
            ),
            ("standalone_semantics_sha256_raw32", semantics.as_slice()),
        ] {
            append_atom(&mut bytes, tag, payload);
        }
        sha256_hex(&bytes)
    }

    #[test]
    fn valid_fixture_roundtrips_and_exposes_authority() {
        let bytes = fixture_bytes();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(validated.canonical_bytes(), bytes);
        assert_eq!(validated.batch_episodes(), 2);
        assert_eq!(validated.checkpoint_segment_updates(), 4);
        assert_eq!(validated.requested_successful_updates(), 12);
        assert_eq!(validated.record().schema(), TRAIN_RUN_SCHEMA_V2);
        assert_eq!(
            validated.record().model_snapshot().identity(),
            SNAPSHOT_IDENTITY_V1
        );
        assert_eq!(
            validated.record().environment().deck_ids(),
            &["Rally", "Rally"]
        );
        assert_eq!(validated.run_sha256(), sha256_hex(&bytes));
        assert_eq!(
            validated.record().contracts().identity_bundle_sha256(),
            validated.identity_bundle_sha256()
        );
        assert_eq!(
            validated
                .record()
                .contracts()
                .standalone_semantics()
                .sha256(),
            validated.standalone_semantics_sha256()
        );
        assert!(!String::from_utf8(bytes).unwrap().contains("run_sha256"));
    }

    #[test]
    fn hierarchy_has_exact_root_snapshot_and_core_key_counts() {
        let value = serde_json::to_value(fixture_record()).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 16);
        assert_eq!(value["model_snapshot"].as_object().unwrap().len(), 34);
        assert_eq!(value["contracts"].as_object().unwrap().len(), 14);
        assert_eq!(
            value["contracts"]["standalone_semantics"]["core"]
                .as_object()
                .unwrap()
                .len(),
            14
        );
        assert_eq!(value["artifact_schemas"].as_object().unwrap().len(), 11);
    }

    #[test]
    fn independent_digest_references_and_goldens_match() {
        let record = fixture_record();
        let semantics_bytes =
            reference_canonical_bytes(&record.contracts.standalone_semantics.core);
        let semantics = sha256_hex(&semantics_bytes);
        let identity = reference_identity_bundle(&record);
        let run_bytes = reference_canonical_bytes(&record);
        assert_eq!(semantics, record.contracts.standalone_semantics.sha256);
        assert_eq!(identity, record.contracts.identity_bundle_sha256);
        assert_eq!(run_bytes, fixture_bytes());
        assert_eq!(
            semantics,
            "2b2b65d958f74e631a5ca995410af641dda25505db65550f08c94c04c910cdbe"
        );
        assert_eq!(
            identity,
            "b42a00d17ffe03b3e4221985587a24f56227658a73cd5a48c671c6b013842eed"
        );
        assert_eq!(
            sha256_hex(&run_bytes),
            "dae0b647887ef07ffe6e307490a96bfff69a22b29d69f8d1d9c3f96eb484846f"
        );
    }

    #[test]
    fn canonical_corruption_matrix_fails_closed() {
        let mut unknown = serde_json::to_value(fixture_record()).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert!(matches!(
            decode_train_run_v2(&canonical_value_bytes(&unknown))
                .unwrap_err()
                .kind(),
            TrainRunV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        ));

        let mut missing = serde_json::to_value(fixture_record()).unwrap();
        missing.as_object_mut().unwrap().remove("schema");
        assert!(matches!(
            decode_train_run_v2(&canonical_value_bytes(&missing))
                .unwrap_err()
                .kind(),
            TrainRunV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        ));

        let mut nested_unknown = serde_json::to_value(fixture_record()).unwrap();
        nested_unknown["model_snapshot"]
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert!(matches!(
            decode_train_run_v2(&canonical_value_bytes(&nested_unknown))
                .unwrap_err()
                .kind(),
            TrainRunV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        ));

        for (bytes, expected) in [
            (
                b"{\"schema\":1,\"schema\":2}\n".as_slice(),
                CanonicalJsonErrorKindV1::DuplicateObjectKey,
            ),
            (
                b"null\n".as_slice(),
                CanonicalJsonErrorKindV1::NullForbidden,
            ),
            (
                b"1.0\n".as_slice(),
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ),
            (
                b"\"\\u00e9\"\n".as_slice(),
                CanonicalJsonErrorKindV1::NonPrintableAscii,
            ),
        ] {
            assert_eq!(
                decode_train_run_v2(bytes).unwrap_err().kind(),
                TrainRunV2ErrorKind::CanonicalJson(expected)
            );
        }

        let canonical = String::from_utf8(fixture_bytes()).unwrap();
        let noncanonical = canonical.replacen(":", ": ", 1);
        assert_eq!(
            decode_train_run_v2(noncanonical.as_bytes())
                .unwrap_err()
                .kind(),
            TrainRunV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::NonCanonicalBytes)
        );

        let float = canonical.replacen(
            &format!("\"learning_rate_f32_bits\":\"{:08x}\"", 0.001_f32.to_bits()),
            "\"learning_rate_f32_bits\":1.0",
            1,
        );
        assert_eq!(
            decode_train_run_v2(float.as_bytes()).unwrap_err().kind(),
            TrainRunV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::FloatingPointForbidden)
        );

        assert_eq!(
            decode_train_run_v2(&vec![b' '; TRAIN_RUN_MAX_BYTES_V2 + 1])
                .unwrap_err()
                .kind(),
            TrainRunV2ErrorKind::RecordTooLarge
        );
    }

    #[test]
    fn scalar_and_hex_corruption_matrix_fails_closed() {
        let mut cases = Vec::new();
        let mut record = fixture_record();
        record.model_snapshot.snapshot_sha256 = SNAPSHOT_SHA256_V1.to_ascii_uppercase();
        cases.push(record);
        let mut record = fixture_record();
        record.source.binary_volume_serial_u64_hex = "0".repeat(15);
        cases.push(record);
        let mut record = fixture_record();
        record.toolchain.rustc_commit_hash = "g".repeat(40);
        cases.push(record);
        let mut record = fixture_record();
        record.model_snapshot.scorer_bias_anchor_f32_bits = u64::from(u32::MAX) + 1;
        cases.push(record);
        let mut record = fixture_record();
        record.runtime.os_build = U63_MAX + 1;
        cases.push(record);
        for record in cases {
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    #[test]
    fn schedule_boundaries_and_corruption_matrix_are_enforced() {
        for k in [2, 10_000] {
            let mut record = fixture_record();
            record.schedule.batch_episodes = k;
            record.schedule.checkpoint_episode_interval =
                k * record.schedule.checkpoint_segment_updates;
            refresh_derived(&mut record);
            validate_train_run_record_v2(record).unwrap();
        }
        for k in [0, 1, 3, 10_001] {
            let mut record = fixture_record();
            record.schedule.batch_episodes = k;
            assert!(validate_train_run_record_v2(record).is_err());
        }

        let mut maximum = fixture_record();
        maximum.schedule.checkpoint_segment_updates = MAX_SUCCESSFUL_UPDATES_V2;
        maximum.schedule.requested_successful_updates = MAX_SUCCESSFUL_UPDATES_V2;
        maximum.schedule.checkpoint_episode_interval = 2 * MAX_SUCCESSFUL_UPDATES_V2;
        refresh_derived(&mut maximum);
        validate_train_run_record_v2(maximum).unwrap();

        for (s, n) in [(0, 12), (5, 4), (5, 12), (1, 100_000_000)] {
            let mut record = fixture_record();
            record.schedule.checkpoint_segment_updates = s;
            record.schedule.requested_successful_updates = n;
            assert!(validate_train_run_record_v2(record).is_err());
        }
        let mut wrong_interval = fixture_record();
        wrong_interval.schedule.checkpoint_episode_interval += 1;
        assert_record_error(wrong_interval, TrainRunV2ErrorKind::CrossBinding);
        assert_eq!(
            checked_u63_mul(U63_MAX, 2).unwrap_err().kind(),
            TrainRunV2ErrorKind::InvalidArithmetic
        );
    }

    #[test]
    fn f32_class_and_production_constant_matrix_is_enforced() {
        for bits in [
            "00000000", "00000001", "bf800000", "7f800000", "7fc00000", "3A83126F",
        ] {
            let mut record = fixture_record();
            record.optimization.learning_rate_f32_bits = bits.to_owned();
            assert!(validate_train_run_record_v2(record).is_err(), "{bits}");
        }
        let mut value_zero = fixture_record();
        value_zero.optimization.value_coefficient_f32_bits = "00000000".to_owned();
        assert!(validate_train_run_record_v2(value_zero).is_err());
        let mut beta_one = fixture_record();
        beta_one.optimization.beta1_f32_bits = "3f800000".to_owned();
        assert!(validate_train_run_record_v2(beta_one).is_err());
        let mut beta_other = fixture_record();
        beta_other.optimization.beta1_f32_bits = "3f000000".to_owned();
        assert_record_error(beta_other, TrainRunV2ErrorKind::InvalidLiteral);
        let mut epsilon_zero = fixture_record();
        epsilon_zero.optimization.epsilon_f32_bits = "00000000".to_owned();
        assert!(validate_train_run_record_v2(epsilon_zero).is_err());
        let mut weight_negative = fixture_record();
        weight_negative.optimization.weight_decay_f32_bits = "bf800000".to_owned();
        assert!(validate_train_run_record_v2(weight_negative).is_err());
        let mut amsgrad = fixture_record();
        amsgrad.optimization.amsgrad = true;
        assert_record_error(amsgrad, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn limits_and_topology_matrix_is_enforced() {
        for (physical, policy) in [(0, 1), (2, 1), (1, 131_073)] {
            let mut record = fixture_record();
            record.limits.max_physical_decisions = physical;
            record.limits.max_policy_steps = policy;
            assert!(validate_train_run_record_v2(record).is_err());
        }
        for (workers, sessions) in [(0, 1), (17, 1), (1, 0), (1, 65)] {
            let mut record = fixture_record();
            record.topology.worker_count = workers;
            record.topology.sessions_per_worker = sessions;
            assert!(validate_train_run_record_v2(record).is_err());
        }
        let mut actor_mismatch = fixture_record();
        actor_mismatch.topology.logical_actor_count += 1;
        assert_record_error(actor_mismatch, TrainRunV2ErrorKind::CrossBinding);
        let mut broker_high = fixture_record();
        broker_high.topology.broker_batch_target = 9;
        assert_record_error(broker_high, TrainRunV2ErrorKind::CrossBinding);
        let mut timeout_zero = fixture_record();
        timeout_zero.topology.scheduler_timeout_ms = 0;
        assert!(validate_train_run_record_v2(timeout_zero).is_err());
    }

    #[test]
    fn runtime_toolchain_backend_and_snapshot_contract_matrices_fail_closed() {
        let mut rustc = fixture_record();
        rustc.runtime.rustc_release = "1.94.0".to_owned();
        assert_record_error(rustc, TrainRunV2ErrorKind::CrossBinding);

        let mut target = fixture_record();
        target.runtime.process_architecture = "arm64".to_owned();
        assert_record_error(target, TrainRunV2ErrorKind::CrossBinding);

        let mut backend = fixture_record();
        backend.runtime.numerical_backend_identity = "other-backend".to_owned();
        assert!(validate_train_run_record_v2(backend).is_err());

        let mut cases = Vec::new();
        let mut record = fixture_record();
        record.model_snapshot.feature_contract_digest = ZERO_SHA256.to_owned();
        cases.push(record);
        let mut record = fixture_record();
        record.model_snapshot.parameter_layout_sha256 = ZERO_SHA256.to_owned();
        cases.push(record);
        let mut record = fixture_record();
        record.model_snapshot.parameter_element_count -= 1;
        cases.push(record);
        let mut record = fixture_record();
        record.model_snapshot.optimizer_identity = "other".to_owned();
        cases.push(record);
        let mut record = fixture_record();
        record.model_snapshot.schedule_goldens_sha256 = ZERO_SHA256.to_owned();
        cases.push(record);
        for record in cases {
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    #[test]
    fn structural_semantics_and_digest_corruptions_are_distinguished() {
        let mut structural = fixture_record();
        structural
            .contracts
            .standalone_semantics
            .core
            .workload
            .requested_episode_count += 1;
        assert_record_error(structural, TrainRunV2ErrorKind::StandaloneSemanticsMismatch);

        let mut semantics_digest = fixture_record();
        semantics_digest.contracts.standalone_semantics.sha256 = ZERO_SHA256.to_owned();
        assert_record_error(
            semantics_digest,
            TrainRunV2ErrorKind::StandaloneSemanticsDigestMismatch,
        );

        let mut identity = fixture_record();
        identity.contracts.identity_bundle_sha256 = ZERO_SHA256.to_owned();
        assert_record_error(identity, TrainRunV2ErrorKind::IdentityBundleDigestMismatch);
    }

    #[test]
    fn run_digest_includes_final_lf_and_has_no_self_field() {
        let bytes = fixture_bytes();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert_eq!(validated.run_sha256(), sha256_hex(&bytes));
        assert_ne!(
            validated.run_sha256(),
            sha256_hex(&bytes[..bytes.len() - 1])
        );
        let value: Value = serde_json::from_slice(&bytes[..bytes.len() - 1]).unwrap();
        assert!(value.get("run_sha256").is_none());
    }

    #[test]
    fn sorted_unique_features_semver_date_and_privacy_are_enforced() {
        for features in [
            vec![
                "native-training-store-v2-production".to_owned(),
                "aaa".to_owned(),
            ],
            vec![
                "native-training-store-v2-production".to_owned(),
                "native-training-store-v2-production".to_owned(),
            ],
            vec![
                "bad.feature".to_owned(),
                "native-training-store-v2-production".to_owned(),
            ],
        ] {
            let mut record = fixture_record();
            record.package.enabled_features = features;
            assert!(validate_train_run_record_v2(record).is_err());
        }
        for version in ["1", "01.2.3", "1.2.3-01", "1.2.3+"] {
            assert!(!is_semver(version), "{version}");
        }
        assert!(is_semver("1.2.3-alpha.1+build.5"));
        assert!(is_valid_date("2024-02-29"));
        assert!(!is_valid_date("2025-02-29"));

        for private in [
            "C:\\secret\\rustc.exe",
            "release C:\\secret\\rustc.exe",
            "1.94.1(\\\\server\\share)",
            "release /secret/rustc",
            "LLVM=/home/jack/toolchain",
            "LLVM|/home/jack/toolchain",
            "file:///secret",
            "LLVM|FiLe:/secret",
            "prefix https://secret",
        ] {
            let mut record = fixture_record();
            record.toolchain.rustc_release = private.to_owned();
            let error = validate_train_run_record_v2(record).unwrap_err();
            assert!(!error.to_string().contains(private));
            assert_eq!(error.to_string(), error.code());
        }
        assert!(!looks_private_location("LLVM=relative/toolchain"));
        assert!(!looks_private_location("profile:release"));
    }

    #[test]
    fn stored_interval_and_semantics_projection_follow_k_s_n_exactly() {
        let mut record = fixture_record();
        record.schedule.batch_episodes = 512;
        record.schedule.checkpoint_segment_updates = 7;
        record.schedule.requested_successful_updates = 21;
        record.schedule.checkpoint_episode_interval = 3584;
        refresh_derived(&mut record);
        let validated = validate_train_run_record_v2(record).unwrap();
        let workload = &validated
            .record()
            .contracts()
            .standalone_semantics()
            .core()
            .workload;
        assert_eq!(workload.batch_episodes, 512);
        assert_eq!(workload.checkpoint_segment_updates, 7);
        assert_eq!(workload.checkpoint_episode_interval, 3584);
        assert_eq!(workload.requested_successful_updates, 21);
        assert_eq!(workload.requested_episode_count, 10_752);
    }
}
