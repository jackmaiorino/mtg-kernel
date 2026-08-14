//! Pure typed `run.json` authority for Native Training Store V2.
//!
//! This module owns no capture, filesystem, publication, execution, or
//! learning-quality behavior. It accepts only canonical JSON, validates the
//! complete dependency-closed run/v2 grammar, reconstructs the standalone
//! semantics projection, and independently recomputes every run-root digest.

use crate::KERNEL_VERSION;
use crate::canonical_json_v1::{
    CanonicalJsonErrorKindV1, CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1,
};
pub use crate::common_model_snapshot_v1::CommonModelSnapshotRecordV1;
use crate::common_model_snapshot_v1::{
    AUTHORITY_RUNTIME_IDENTITY_V1, BASE_SEED_V1, INITIALIZER_IDENTITY_V1, MODEL_INIT_SEED_V1,
    NONCLAIM_V1, PARAMETER_ELEMENT_COUNT_V1, PARAMETER_TENSOR_COUNT_V1, PAYLOAD_BYTE_COUNT_V1,
    RUST_LOADER_IDENTITY_V1, SNAPSHOT_IDENTITY_V1, SNAPSHOT_SCHEMA_V1,
};
use crate::environment_randomization_v2::{
    ENVIRONMENT_RANDOMIZATION_ATOM_FRAMING_V2, ENVIRONMENT_RANDOMIZATION_EXTRACTION_V2,
    ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1, ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1,
    ENVIRONMENT_RANDOMIZATION_IDENTITY_V2, ENVIRONMENT_RANDOMIZATION_INITIAL_ORDINAL_RULE_V2,
    ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2, ENVIRONMENT_RANDOMIZATION_ORDERED_ATOMS_V2,
    ENVIRONMENT_RANDOMIZATION_OVERFLOW_RULE_V2, ENVIRONMENT_RANDOMIZATION_OWNERS_V2,
    ENVIRONMENT_RANDOMIZATION_PURPOSES_V2, ENVIRONMENT_RANDOMIZATION_SHUFFLE_ALGORITHM_V2,
};
use crate::fast_sampler::{
    FAST_CATEGORICAL_CROSS_LANGUAGE_VECTOR_STREAM_SHA256,
    FAST_CATEGORICAL_CROSS_LANGUAGE_VECTORS_FILE_SHA256, FAST_CATEGORICAL_EXP_TABLE_SHA256,
    FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256, FAST_CATEGORICAL_SAMPLER_VERSION,
};
use crate::native_flat_tensorizer_v2::{
    NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2,
    NATIVE_FLAT_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2, NATIVE_FLAT_TENSORIZER_FIXTURE_SHA256_V2,
    NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
};
use crate::native_full_episode_trajectory_v1::{
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V1, NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1,
};
/// The V2 trajectory six-pin tuple is imported from its owner module rather
/// than restated, so the classifier and the trajectory contract cannot drift.
use crate::native_full_episode_trajectory_v2::{
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V2,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V2,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2,
    NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V2, NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2,
};
use crate::native_opponent_policy_v2::{
    FROZEN_CHECKPOINT_OPPONENT_POLICY_IDENTITY_V2, FROZEN_CHECKPOINT_OPPONENT_POLICY_MODEL_RULE_V2,
    FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2, OPPONENT_LADDER_POOL_IDENTITY_V2,
    OPPONENT_LADDER_POOL_SIZE_V2, OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2,
    OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2, OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2,
    OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
};
use crate::native_opponent_sampler_v1::{
    NATIVE_OPPONENT_SAMPLER_VECTOR_STREAM_SHA256_V1,
    NATIVE_OPPONENT_SAMPLER_VECTORS_FILE_SHA256_V1,
    NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1,
    NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1, UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1,
    UNIFORM_INDEX_MODULO_U64_IDENTITY_V1,
};
use crate::native_policy_train_step_v1::{
    ADAM_BETA1_V1, ADAM_BETA2_V1, ADAM_EPSILON_V1, ADAM_WEIGHT_DECAY_V1,
    CANONICAL_GAUGE_PARAMETERS_V1, NATIVE_OPTIMIZER_IDENTITY_V1,
    NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1,
    NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1, TRAIN_STEP_IDENTITY_V1, TRAINER_ALGORITHM_V1,
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
use crate::native_trainer_schedule_v2::NATIVE_TRAINER_SCHEDULE_CONTRACT_V2;
use crate::native_trainer_v1::{
    NATIVE_TRAINER_CONTRACT_IDENTITY_V2, NATIVE_TRAINER_MAX_BATCH_EPISODES_V2,
    NATIVE_TRAINER_MIN_BATCH_EPISODES_V2,
};
use crate::policy_surface_v5::POLICY_SURFACE_VERSION;
use crate::rl_session::{
    CANONICAL_RALLY_DECK_ID, RL_SESSION_PROTOCOL_NAME, RL_SESSION_PROTOCOL_VERSION,
    RL_SESSION_PROTOCOL_VERSION_V6, RL_SESSION_SCHEMA_VERSION, RL_SESSION_SCHEMA_VERSION_V6,
};
use crate::runtime_decks::{
    RUNTIME_DECK_CATALOG_SCHEMA, RUNTIME_DECK_PROTOCOL, runtime_deck_by_id,
};
use crate::strict_source_tree_attestation_v1::{
    STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1,
    STRICT_SOURCE_TREE_RECIPE_IDENTITY_V1 as SOURCE_TREE_RECIPE_IDENTITY_V1,
    STRICT_SOURCE_TREE_RECIPE_SHA256_V1 as SOURCE_TREE_RECIPE_SHA256_V1,
};
use crate::surface_v2::H2_PREDICATE_VERSION;
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

// These literals freeze the revision-3 RunV2 grammar. Production modules own
// the live algorithms and artifacts; validation independently requires both
// each owner constant and each record field to equal the corresponding frozen
// literal so owner drift cannot silently reinterpret an existing RunV2.
const FROZEN_SOURCE_TREE_RECIPE_IDENTITY_V2: &str = "mtg-kernel-strict-source-tree-sha256-v1";
const FROZEN_SOURCE_TREE_RECIPE_SHA256_V2: &str =
    "13ab31b8e4810d683007182d1b5fc3b76db0b9761c877a6e78880c0cadf3fece";
const FROZEN_SOURCE_TREE_RECIPE_BYTE_COUNT_V2: u64 = 5_847;

// Dual-Profile Catalog Successor (collab CLAUDE #220): the numeric u64 form
// is no longer compared against any live build constant at decode time (see
// `validate_frozen_rev3_authorities_v2`'s doc comment), so it is otherwise
// unused; retained byte-identical as part of the permanent historical
// authority record rather than deleted.
#[allow(dead_code)]
const FROZEN_CARD_DB_HASH_U64_V2: u64 = 0xa06f_a956_6106_f0ea;
const FROZEN_CARD_DB_HASH_U64_HEX_V2: &str = "a06fa9566106f0ea";
const FROZEN_RUNTIME_CATALOG_SCHEMA_V2: &str = "kernel_runtime_decks/v1";
const FROZEN_RUNTIME_CATALOG_PROTOCOL_V2: &str = "canonical-mainboard-bo1/v1";
const FROZEN_RUNTIME_CATALOG_SHA256_V2: &str =
    "5ea19e8a08f0e9c9657e9a6a90382329785f27eeabbbe066e80e7025e8ee62c0";
const FROZEN_RALLY_DECK_ID_V2: &str = "Rally";
const FROZEN_RALLY_DECK_HASH_U64_V2: u64 = 0x0c9f_01c2_5444_12bf;
const FROZEN_RALLY_DECK_HASH_U64_HEX_V2: &str = "0c9f01c2544412bf";

// CURRENT catalog profile (Dual-Profile Catalog Successor, collab CLAUDE
// #220): the live nine-deck catalog identity as of the runtime-decks-nine
// landing, pinned as its own frozen authority parallel to and independent of
// the FROZEN_CARD_DB_HASH_U64_V2 / FROZEN_RUNTIME_CATALOG_SHA256_V2
// (historical/rev3) literals above, which stay untouched forever. Only the
// two catalog *content* identities differ between the two profiles --
// `RUNTIME_DECK_CATALOG_SCHEMA`/`RUNTIME_DECK_PROTOCOL` (format) and the
// Rally deck's own hash are unchanged and shared by both profiles, so they
// are not restated here. See `classify_catalog_profile_v1` for the
// mutually-exclusive, hybrid-rejecting classifier that selects between the
// two tuples, and `validate_environment_v2`/`validate_frozen_rev3_authorities_v2`
// for why neither tuple is checked against the crate's live build constants
// at decode time (only construction/mutation call sites read live constants,
// and they always do so directly, never through a frozen pin).
const FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1: &str = "64c82a261e078f1a";
const FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1: &str =
    "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851";

const FROZEN_PROTOCOL_V2: &str = "kernel_rl_jsonl";
const FROZEN_PROTOCOL_VERSION_V2: u32 = 5;
const FROZEN_SCHEMA_VERSION_V2: u32 = 5;
const FROZEN_KERNEL_VERSION_V2: &str = "0.0.4-spike";
const FROZEN_SURFACE_VERSION_V2: u32 = 2;
const FROZEN_POLICY_SURFACE_VERSION_V2: u32 = 5;

const FROZEN_SNAPSHOT_SCHEMA_V2: &str = "mtg-kernel-common-model-snapshot/v1";
const FROZEN_SNAPSHOT_IDENTITY_V2: &str =
    "mtg-kernel-python-authoritative-common-model-snapshot-v1";
const FROZEN_SNAPSHOT_SHA256_V2: &str =
    "33455d0fedc5aea8abd4deeaf37c5480f1832dbea34b9391c9a942d95f040771";
const FROZEN_SNAPSHOT_MANIFEST_FILE_SHA256_V2: &str =
    "d5d296f5d4ee1f7e40a6005f1e1dd328b2885f6b95f0c6968c6bf1b87351c7cc";
const FROZEN_SNAPSHOT_MANIFEST_CORE_SHA256_V2: &str =
    "456a5f8d2c3973c88e47b9d8c8a6ce6069561c4b5aa6582c73e31d837c13816d";
const FROZEN_SNAPSHOT_PAYLOAD_SHA256_V2: &str =
    "79f715b11ccce80ac66cc832bfdc0c963a8a20f27f7b492fdfbb433c008a90a5";
const FROZEN_SNAPSHOT_PAYLOAD_BYTE_COUNT_V2: u64 = 4_923_976;
const FROZEN_PARAMETER_LAYOUT_SHA256_V2: &str =
    "266966ba3f3c49dd758f694aaef65234e01e8c077ab85a7b1058efedd8e5b887";
const FROZEN_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V2: &str =
    "36157c71b9fd736d4913e6c5722dcb9c1e4f119b7b28b108bde9d74f18862d54";
const FROZEN_PARAMETER_TENSOR_COUNT_V2: u64 = 33;
const FROZEN_PARAMETER_ELEMENT_COUNT_V2: u64 = 1_230_994;
const FROZEN_MODEL_CONFIG_FINGERPRINT_V2: &str =
    "f3836afa17acc74b4856fe18222345116f27c12fa5ad18c34b4dec3f04855251";
const FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2: &str = "kernel-policy-value-net-8";
const FROZEN_FEATURE_CONTRACT_DIGEST_V2: &str =
    "bcc808186e40a1ad6aec679d8a386631cb1226379366a632603f0beb95b47396";
const FROZEN_FEATURE_ENCODING_DIGEST_V2: &str =
    "918e57a0796807e84310026de48d30b500813ef37d939462ea85b7255a39111c";
const FROZEN_INITIALIZER_IDENTITY_V2: &str = "trainer-seeded-v1";
const FROZEN_BASE_SEED_V2: u64 = 0;
const FROZEN_MODEL_INIT_SEED_V2: u64 = 6_443_515_232_517_447_393;
const FROZEN_TRAINER_SCHEDULE_IDENTITY_V2: &str = "mtg-kernel-native-trainer-schedule-sha256-v1";
const FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2: &str = "kernel-python-rl-trainer-sha256-v2";
const FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2: &str =
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c";
const FROZEN_SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V2: &str =
    "78f0a0409b91df169ab895d4328ba525564cf62135e8fb0be9f0f3ece9e77e87";
const FROZEN_SNAPSHOT_AUTHORITY_RUNTIME_IDENTITY_V2: &str =
    "python-torch-windows-amd64-python3.13.14-torch2.13.0+cpu-cpu-f32-deterministic-threads1-v1";
const FROZEN_SNAPSHOT_LOADER_IDENTITY_V2: &str = "mtg-kernel-rust-common-model-snapshot-loader-v1";
const FROZEN_ADAM_STEP_INITIAL_V2: u64 = 0;
const FROZEN_MOMENT_INITIALIZATION_V2: &str = "positive-zero-f32";
const FROZEN_CANONICAL_GAUGE_PARAMETERS_V2: [&str; 1] = ["scorer.2.bias"];
const FROZEN_SCORER_BIAS_ANCHOR_F32_BITS_V2: u64 = 3_141_403_366;
const FROZEN_SNAPSHOT_NONCLAIM_V2: &str = "Rust does not reproduce the Python trainer-seeded-v1 initializer in this snapshot configuration; the snapshot proves bit-exact initial parameters only and does not establish seeded-initializer parity, cross-runtime numerical bit parity, learning parity, or speedup.";

// Capacity-experiment wide-net (`kernel-policy-value-net-8w128`) mirrors of
// the frozen Net8 literals above (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md,
// SHA-256 a50d067a5fb0f77b888e4e3c77386ca626e9b399a2a19f6959a1e7494f01380a,
// Section 3). Present if and only if a record carries
// `contracts.wide_model_experiment_v1`; every frozen literal above stays
// UNTOUCHED and a record without the wide section validates byte-for-byte as
// it always has. Fields shared verbatim between the frozen and wide
// snapshots by contract (same Xavier/seeded initializer construction, same
// manifest schema: initializer identity, base/init seeds, trainer-schedule
// and python-reference-seed identities, schedule goldens, feature digests,
// authority runtime identity, optimizer identity, Adam bootstrap, canonical
// gauge parameters) are validated against the EXISTING `FROZEN_*_V2`
// constants directly in the wide branch rather than duplicated here; only
// values the wide architecture actually changes get a `FROZEN_WIDE_*`
// mirror.
const FROZEN_WIDE_SNAPSHOT_SCHEMA_V1: &str = "mtg-kernel-common-model-snapshot/v1";
const FROZEN_WIDE_SNAPSHOT_IDENTITY_V1: &str =
    "mtg-kernel-python-authoritative-wide-model-experiment-snapshot-v1";
const FROZEN_WIDE_SNAPSHOT_SHA256_V1: &str =
    "91c658633436250ffc11e62594d3af38778e2025d251d29def23cf4e589f5e13";
const FROZEN_WIDE_SNAPSHOT_MANIFEST_FILE_SHA256_V1: &str =
    "e5ea68881e5fe9c0daf45f8dc9a95cdb385577daed72a4b231c15a7a7a551db0";
const FROZEN_WIDE_SNAPSHOT_MANIFEST_CORE_SHA256_V1: &str =
    "968a49c3efce869a80eb64775ff9c9728cc0a9e4ee4dd9cc9bb79f32df5c6ade";
const FROZEN_WIDE_SNAPSHOT_PAYLOAD_SHA256_V1: &str =
    "8d54e3072ab4607e96b1dfc56691bb5ddf053045473ecc6d8d9fca494b5e489f";
const FROZEN_WIDE_SNAPSHOT_PAYLOAD_BYTE_COUNT_V1: u64 = 11_003_016;
const FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1: &str =
    "5478606755c5c47f1deb048d254265db16298dd9c27f6a9fc4a948c66ccb7fa3";
const FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1: &str =
    "574be71cde83d6e9494b87d9b8a6a98dddedd0cc3aa7d0cfa685e2489b8a1c5b";
const FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1: u64 = 33;
const FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1: u64 = 2_750_754;
const FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1: &str =
    "b34c87f46e7709d8b03ee21710d7f0345ff0fcf49ec3d09cf25b94cfe71bf1c6";
const FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1: &str = "kernel-policy-value-net-8w128";
const FROZEN_WIDE_SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V1: &str =
    "85446eae753b1055d3dedeb56b7080a49327eeee52e492b74f42a0cfde52cb8b";
const FROZEN_WIDE_SNAPSHOT_LOADER_IDENTITY_V1: &str =
    "mtg-kernel-rust-wide-model-snapshot-loader-v1";
const FROZEN_WIDE_SCORER_BIAS_ANCHOR_F32_BITS_V1: u64 = 975_689_200;
// = FROZEN_SNAPSHOT_NONCLAIM_V2 (shared base text) + " Label: " +
// FROZEN_WIDE_DIAGNOSTIC_LABEL_V1, exactly what
// `common_model_snapshot_v1::wide_record_from_validated` computes at load
// time. Spelled out as its own literal (not derived via `concat!`, which
// cannot reference another `const`) so this stays a plain frozen mirror.
const FROZEN_WIDE_SNAPSHOT_NONCLAIM_V1: &str = "Rust does not reproduce the Python trainer-seeded-v1 initializer in this snapshot configuration; the snapshot proves bit-exact initial parameters only and does not establish seeded-initializer parity, cross-runtime numerical bit parity, learning parity, or speedup. Label: WIDE-DIAGNOSTIC-NON-EVIDENCE";
const FROZEN_WIDE_DIAGNOSTIC_LABEL_V1: &str = "WIDE-DIAGNOSTIC-NON-EVIDENCE";

const FROZEN_TRAINER_IDENTITY_V2: &str = "mtg-kernel-native-even-batch-trainer-v2";
const FROZEN_TENSORIZER_IDENTITY_V2: &str = "mtg-kernel-python-encoded-decision-tensor-contract-v2";
// Feature-Encoder Successor (collab CLAUDE #221, folding CODEX #235's
// historical stack-source encoder fix into the versioned feature-authority
// successor): the HISTORICAL features.py identity, byte-identical forever.
// Every already-sealed RunV2 record captured before this successor landed
// carries this value in `contracts.tensorizer.authoritative_features_source_sha256`
// and must keep decoding. This is independently typed from
// `native_flat_tensorizer_v2::NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2`
// on purpose (this module's own frozen mirror, same discipline as every
// other FROZEN_*_V2 pin), not derived from it.
const FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2: &str =
    "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb";
// CURRENT features.py identity (Feature-Encoder Successor): the live source
// hash as of the historical stack-source encoder fix, pinned as its own
// frozen authority parallel to and independent of
// `FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2` above, which stays
// untouched forever. `feature_contract_digest`/`feature_encoding_digest` are
// unchanged by the fix (same declared dimensions, same contract/encoding
// digests for both profiles), so only this one tensorizer field gets a
// CURRENT sibling; see `matches_frozen_tensorizer_authority_source_sha256_v1`
// for the whole-tuple-free, single-field acceptance this axis uses instead
// of a `classify_catalog_profile_v1`-style closed enum (mixing historical
// and current feature-encoder-profile records in the science loop is safe:
// the encoding shape did not change, only this narrow bugfix + provenance
// hash did).
//
// UNVERIFIED PENDING RECONCILIATION: collab binds this to `b316c0aa...`;
// this branch has no byte-exact patch for the described fix, only its
// semantic specification, and this branch's own reconstruction hashes to
// the value below instead. See the branch report for the mismatch; do not
// treat this as production-ready until reconciled.
const FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1: &str =
    "5d82f5b87a6819076c903390230015da456f914828890d9c5384af410f21be1c";
const FROZEN_TENSORIZER_FIXTURE_SHA256_V2: &str =
    "5dbece4f903a09260a499295d866c7e6ff4283f9de83f842224511f977ae8a97";
const FROZEN_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2: &str =
    "2f87d49106806a402148fc8b115a54ac94713eb717f45f897eff57a3bd1184ec";

/// Feature-Encoder Successor: accepts either the HISTORICAL or CURRENT
/// features.py identity for `contracts.tensorizer.authoritative_features_source_sha256`.
/// Every other tensorizer field (`fixture_sha256`, `fixture_payload_sha256`)
/// stays pinned to its single existing frozen literal in
/// `validate_contracts_v2`: this branch's reconstruction of the encoder fix
/// does not regenerate `data/flat_policy_v2/python_full_features_v2.json`
/// (that requires running the Python generator, out of scope for a
/// code-only change), so the two fixture-derived hashes have no known
/// CURRENT value yet. A CURRENT-profile record whose tensorizer contract
/// carries the new fixture hashes will not decode until that follow-up
/// lands (see the branch report's deferred verification batch).
fn matches_frozen_tensorizer_authority_source_sha256_v1(value: &str) -> bool {
    value == FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2
        || value == FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1
}

const FROZEN_LOSS_IDENTITY_V2: &str = "terminal_reinforce_value/v3";
const FROZEN_TRAIN_STEP_IDENTITY_V2: &str = "native-policy-value-cpu-train-step-v1";
const FROZEN_NUMERICAL_BACKEND_IDENTITY_V2: &str =
    "rust-production-native-policy-train-step-v1-cpu-ieee754-binary32-sequential";
const CPU_RUNTIME_TUPLE_IDENTITY_V2: &str = "mtg-kernel-native-windows-cpu-runtime-tuple-v1";
const CUDA_RUNTIME_TUPLE_IDENTITY_V2: &str = "mtg-kernel-native-windows-cuda-runtime-tuple-v1";

/// The store-admitted (runtime tuple, numerical backend identity) pairs. A
/// record may declare either pair; a CPU tuple can never carry the CUDA
/// backend identity or vice versa.
fn store_backend_identity_for_runtime_tuple_v2(tuple_identity: &str) -> Option<&'static str> {
    match tuple_identity {
        CPU_RUNTIME_TUPLE_IDENTITY_V2 => Some(FROZEN_NUMERICAL_BACKEND_IDENTITY_V2),
        CUDA_RUNTIME_TUPLE_IDENTITY_V2 => {
            Some(crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1)
        }
        _ => None,
    }
}
const FROZEN_OPTIMIZER_IDENTITY_V2: &str = "native-adam-canonical-scorer-bias-gauge-v1";
const FROZEN_GAUGE_EVIDENCE_IDENTITY_V2: &str = "mtg-kernel-native-scorer-bias-gauge-evidence-v1";
const FROZEN_ENVIRONMENT_SEED_DERIVATION_IDENTITY_V2: &str = "train-env/base_seed/pair_index";
const FROZEN_LEARNER_ACTION_SEED_DERIVATION_IDENTITY_V2: &str = "train-learner-action-group/base_seed/episode_index/learner_physical_decision_index -> train-learner-action-substep/group_seed/substep_index";
const FROZEN_OPPONENT_ACTION_SEED_DERIVATION_IDENTITY_V2: &str = "train-opponent-action-group/base_seed/episode_index/opponent_physical_decision_index -> train-opponent-action-substep/group_seed/substep_index";
const FROZEN_LEARNER_SAMPLER_IDENTITY_V2: &str = "f32-q8-expq63-hamilton-splitmix64-v1";
const FROZEN_LEARNER_SAMPLER_CONTRACT_SHA256_V2: &str =
    "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6";
const FROZEN_LEARNER_SAMPLER_EXP_TABLE_SHA256_V2: &str =
    "2cdd19abdec245d7a9f892e8757c299a282ae097361baecc46cfd6a57c476e2a";
const FROZEN_LEARNER_VECTORS_FILE_SHA256_V2: &str =
    "407a08fb9b9bb5012f14d779d0878c986ce0f16530820a89f5bd54c33d5e7456";
const FROZEN_LEARNER_VECTOR_STREAM_SHA256_V2: &str =
    "69fe3e72dd8fdb245e59e1959359aff3cb6c326fab9f7f2b2ab56e3744d4f3de";
const FROZEN_OPPONENT_POLICY_IDENTITY_V2: &str = "mtg-kernel-trainer-uniform-policy-v1";
const FROZEN_OPPONENT_POLICY_MODEL_RULE_V2: &str = "no-model-uniform-legal-index";
const FROZEN_OPPONENT_SAMPLER_IDENTITY_V2: &str = "mtg-kernel-uniform-index-modulo-u64-v1";
const FROZEN_OPPONENT_SAMPLER_ALGORITHM_V2: &str =
    "selected-index-equals-action-seed-mod-legal-count";
const FROZEN_OPPONENT_VECTORS_FILE_SHA256_V2: &str =
    "9e5898308d30614a4a09cecb584200521b1a3b727606d8cf78dbe70b51106e18";
const FROZEN_OPPONENT_VECTOR_STREAM_SHA256_V2: &str =
    "2b65520a528dcf9eba8d7baded50cc9ad50cf507704c2b4410e2afb4b34d7fad";

// Ladder-opponent successor identities (Self-Play Ladder Design Contract S2,
// Section 2). The uniform literals above stay frozen forever; a record
// carries EITHER the uniform pair above OR this ladder pair below in its
// `opponent_policy`, never a mix, and only the ladder identity admits a
// present `opponent_ladder_pool` section (Section 3).
const FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2: &str =
    "mtg-kernel-trainer-frozen-checkpoint-policy-v2";
const FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2: &str =
    "frozen-checkpoint-softmax-t1-one-seed-per-decision";
const FROZEN_LADDER_POLICY_SAMPLING_RULE_V2: &str = "seeded-categorical-sample-from-softmax-temperature-1.0-checkpoint-policy-one-seed-per-decision";
const FROZEN_LADDER_POOL_IDENTITY_V2: &str = "mtg-kernel-opponent-ladder-pool-v1";
const FROZEN_LADDER_POOL_SIZE_V2: u64 = 4;
const FROZEN_LADDER_POOL_WEIGHT_PRIMARY_V2: u64 = 40;
const FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2: u64 = 20;
const FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2: u64 = 20;
const FROZEN_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2: u64 = 20;

const POPULATION_PROGRAM_IDENTITY_V1: &str = "mtg-kernel-native-scaled-selfplay-population/v1";
const POPULATION_PACKAGE_COMMIT_V1: &str = "838920e359c7a1152d97c450f4575c6be2309f22";
const POPULATION_PROGRAM_DOCUMENT_SHA256_V1: &str =
    "b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38";
const POPULATION_RETEST_MANIFEST_SHA256_V1: &str =
    "f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8";
const POPULATION_REPLAY_END_GENERATION_V1: u64 = 512;
const POPULATION_PROGRAM_UPDATE_COUNT_V1: u64 = 1_024;
const POPULATION_REFRESH_INTERVAL_V1: u64 = 128;
const POPULATION_SLOT_COUNT_V1: u64 = 8;
const POPULATION_REWARD_IDENTITY_V1: &str =
    "terminal-wdl-win-plus-one-draw-zero-loss-minus-one/v1";
const POPULATION_REFRESH_MANIFEST_IDENTITY_V1: &str =
    "mtg-kernel-native-scaled-selfplay-refresh-manifest/v1";
const POPULATION_RETEST_BETA_F32_BITS_V1: &str = "3dcccccd";
const POPULATION_POOL_IDENTITY_V1: &str = FROZEN_LADDER_POOL_IDENTITY_V2;
const POPULATION_POOL_DOCUMENT_SHA256_V1: &str =
    "6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71";
const POPULATION_PARENT_SOURCE_RUN_SHA256_V1: &str =
    "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae";
const POPULATION_PARENT_GENERATION_V1: u64 = 384;
const POPULATION_PARENT_CHECKPOINT_SHA256_V1: &str =
    "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8";
const POPULATION_PARENT_SIDECAR_SHA256_V1: &str =
    "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb";
const POPULATION_PARENT_STATE_SHA256_V1: &str =
    "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99";
const POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1: &str =
    "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d";
const POPULATION_EXPECTED_BASE_SEEDS_V1: [u64; 3] = [970_001, 970_002, 970_003];

const POPULATION_SOURCE_LINEAGES_V1: [(u64, &str, &str, &str, &str, &str, &str); 3] = [
    (
        970_001,
        "2d6650f111cebcb8e87271fb3446127306e2c4006da793c45a7aec5d80c7780e",
        "2307caf5a0093bf3f6f9d3673788eac1d73bcd248bfb6fcb3af785a596304cab",
        "21f95221663a7a064d4d5935d19c95dc108a84085513524f48def0b0da21a2bc",
        "2ee82c53afb9c4cd8343ca67411d9a0b5db800215688f809a08a44c8016953a5",
        "e2e3fdb4216a013fdb043bcb90f33f590d5f7d72a77b5999c423919da3ae3b85",
        "a51d05f8f89e3cca652e8c2daaa289a65cfdb317164d07410395430044b54ed0",
    ),
    (
        970_002,
        "bcecb18db197a5ef14c8512642a3f15191f7dd05e389c02c129853c9496deda7",
        "fdbd65dca0660afe1156f4dff49204325064802e7d44606eb44b7529db528ce1",
        "c3aa704e7670c158da82ad4602a20bcec3240f275ecb7aac9ca42fb341f482df",
        "16c834b632e99589c5970dc52164ea12647f954e43e7bfe61b5d4d767133b9aa",
        "304053bdc96ef094d97506f5605fc599aae045c770cbd6fa7efcebfccc9069b6",
        "1e9022105aec341101c0b14ffa4d509b4073a2f80b213e71dd0065f036e701dd",
    ),
    (
        970_003,
        "1a1bdb75099b50b4d250d3e03ab6d882718f017e2c6d715bc8a67d3022b627ec",
        "9a1c417e6990c54929481f5eee19cf0f9f8d816fa72a3e3a575fdde603364295",
        "814583b210191bc00ec1cf5f485eb6b83ffce2d4c2e632b87874d64e3b62cb3e",
        "50108e3751ab52b6432903cac0b57addb747e287e41bc83f57e0bf9110149788",
        "b3a8811923533bda7b1a8d2dbfa0b5b8ec187b1d40a7029d348a0dabbb04dbc3",
        "861f28ca95316e68d1552986294aae0f7677af64b21f615d5bfcaff01276602c",
    ),
];

const RESPONSE_EXPLOITER_IDENTITY_V1: &str =
    "mtg-kernel-native-scaled-selfplay-response-exploiter/v1";
const RESPONSE_EXPLOITER_TARGET_REFRESH_SHA256_V1: &str =
    "9c9490b205b7b5a933eae7ca86916e5ff5ff9307a150dc35487a8e1c28e73e22";
const RESPONSE_EXPLOITER_TARGET_GLOBAL_GENERATION_V1: u64 = 1_536;
const RESPONSE_EXPLOITER_SOURCE_REFRESH_INDEX_V1: u64 = 8;
const RESPONSE_EXPLOITER_SOURCE_PROGRAM_UPDATE_V1: u64 = 1_024;
const RESPONSE_EXPLOITER_ACTIVE_SLOT_INDICES_V1: [u64; 6] = [0, 1, 2, 3, 4, 5];
const RESPONSE_EXPLOITER_EXCLUDED_SLOT_INDICES_V1: [u64; 2] = [6, 7];
const RESPONSE_EXPLOITER_RENORMALIZATION_IDENTITY_V1: &str =
    "integer-preserving-renormalization-drop-excluded-slots-redeclare-total/v1";
const RESPONSE_EXPLOITER_EFFECTIVE_WEIGHT_UNITS_V1: [u64; 8] = [
    125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 0, 0,
];
const RESPONSE_EXPLOITER_EFFECTIVE_WEIGHT_TOTAL_UNITS_V1: u64 = 751_292;
const RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1: u64 = 256;
const RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1: u64 = 64;
const RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1: u64 = 4;
const RESPONSE_EXPLOITER_FRESH_ADAM_AFTER_WEIGHT_INIT_IDENTITY_V1: &str =
    "weights-bit-exact-from-promoted2-adam-moments-positive-zero-adam-step-zero/v1";
// Response-exploiter build v2 (CLAUDE-RESPONSE-EXPLOITER-V2-SHEET-V1.md
// Section 1) adds three fresh build seeds (971101/971102/971103) and two
// fresh preflight/screen seeds (971191/971192, introduced for Section 13's
// infrastructure sanity check; the sheet itself does not specify preflight
// seed literals) alongside the original v1 campaign's two build and two
// screen seeds. The original four seeds keep the exact same role and
// completion-generation semantics as before; this is a pure widening, not a
// reinterpretation.
const RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1: [u64; 5] =
    [971_001, 971_002, 971_101, 971_102, 971_103];
const RESPONSE_EXPLOITER_AUTHORIZED_SCREEN_SEEDS_V1: [u64; 4] =
    [971_091, 971_092, 971_191, 971_192];
const RESPONSE_EXPLOITER_SCREEN_COMPLETION_GENERATION_V1: u64 = 4;
const RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1: &str = "3dcccccd";
const RESPONSE_EXPLOITER_RETRY_BETA_F32_BITS_V1: &str = "3cf5c28f";

// De-novo response screen (CLAUDE-DENOVO-SCREEN-SHEET-V1.md, Mechanism
// Decision Memo V1 Option A / coordinator ruling 2026-08-07): a third
// response-exploiter run role, structurally distinct from "build" and
// "screen" (both of which always warm-start from promoted(2) gen-384 under a
// nonzero KL anchor). "denovo-screen" trains a fresh Net8 from the frozen
// common model snapshot (no warm start, beta=0, no KL anchor) against the
// exact same frozen refresh-008 mixture "build"/"screen" already use. One
// authorized seed today (971_201, verified fresh against collab and every
// sibling mtg-kernel-* worktree before authorization); the array stays
// closed/compile-bound like its build/screen siblings and is widened, not
// reinterpreted, if a future seed is authorized.
const RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_SEEDS_V1: [u64; 1] = [971_201];
// 0.0f32 bits: exactly the literal `parse_policy_anchor_coefficient_v1`
// (native_science_loop_v1) already maps to "no anchor installed" for the
// string "0", independent of this contract.
const RESPONSE_EXPLOITER_DENOVO_BETA_F32_BITS_V1: &str = "00000000";
// Denovo-screen is a full 256-update run like "build" (unlike "screen",
// which is a 4-update infrastructure smoke test); reusing
// RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1 directly as its own completion
// generation keeps this a widening, not a new schedule shape.
const RESPONSE_EXPLOITER_DENOVO_FRESH_ADAM_AFTER_WEIGHT_INIT_IDENTITY_V1: &str =
    "weights-bit-exact-from-common-model-snapshot-adam-moments-positive-zero-adam-step-zero/v1";

// De-novo response screen, Phase 2 horizon amendment
// (CLAUDE-DENOVO-SCREEN-SHEET-V1.md Phase 2 amendment, owner-authorized
// 2026-08-08): a fourth response-exploiter run role, "denovo-screen-512",
// structurally identical to "denovo-screen" in every respect (fresh Net8
// from the frozen common model snapshot, no warm start, beta=0, no KL
// anchor, same frozen refresh-008 mixture) except the training horizon,
// which extends from 256 to 512 updates to read whether the screen's
// decisively positive, still-climbing 256-update curve crosses 50 percent
// against the mixture. A genuinely new schedule shape (unlike the original
// 256-update denovo-screen role, which could reuse
// RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1 unchanged because it matched
// "build"'s existing 256-update shape): this role gets its own dedicated
// seed array and its own training-update-count/completion-generation
// constant, mirroring exactly how "build" and "screen" already use separate
// arrays and separate completion generations rather than one shared,
// reinterpreted array. One authorized seed today (971_202, verified fresh
// against collab and every sibling mtg-kernel-* worktree before
// authorization; the array stays closed/compile-bound and is widened, not
// reinterpreted, if a future seed is authorized).
const RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1: [u64; 1] = [971_202];
const RESPONSE_EXPLOITER_DENOVO_512_TRAINING_UPDATE_COUNT_V1: u64 = 512;

// V2 opponent seed-schedule namespace declarations (Self-Play Ladder Design
// Contract S2, Section 2), owned by `native_trainer_schedule_v2`. Present in
// a record if and only if `opponent_policy.identity` carries the ladder
// identity above; rejected (must be absent) for the uniform identity.
const FROZEN_LADDER_SCHEDULE_VERSION_V2: &str = "mtg-kernel-native-trainer-schedule-sha256-v2";
const FROZEN_LADDER_SCHEDULE_SEED_VERSION_V2: &str = "kernel-native-ladder-trainer-sha256-v1";
const FROZEN_LADDER_SCHEDULE_POOL_CHOICE_NAMESPACE_V2: &str = "train-opponent-pool-choice";
const FROZEN_LADDER_SCHEDULE_POOL_CHOICE_FIELDS_V2: &str = "base_seed,episode_index";
const FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_NAMESPACE_V2: &str = "train-opponent-policy-substep";
const FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_FIELDS_V2: &str =
    "base_seed,episode_index,opponent_physical_decision_index,substep_index";
const FROZEN_LADDER_SCHEDULE_POOL_CHOICE_MODULO_V2: u64 = 100;
const FROZEN_LADDER_SCHEDULE_POOL_CHOICE_THRESHOLD_RULE_V2: &str = "draw=pool_choice_seed%100;[0,40)->primary;[40,60)->predecessor_a;[60,80)->predecessor_b;[80,100)->uniform_floor";
const FROZEN_LADDER_SCHEDULE_POOL_CHOICE_BIAS_RULE_V2: &str = "intentional-modulo-bias-no-rejection-sampling;when-100-does-not-divide-the-seed-domain-low-residues-have-one-extra-preimage;consistent-with-the-v1-uniform-sampler-bias-rule;changing-this-rule-requires-a-new-schedule-version";
const FROZEN_LADDER_SCHEDULE_VERSION_CHANGE_RULE_V2: &str = "any seed, namespace, framing, domain, field-order, weight-threshold, or golden change requires a new schedule version announced on the CODEX-CLAUDE channel";

/// The legacy V1 trajectory six-pin tuple.
///
/// These constants carry *V1* values and always did; the historical `_V2`
/// suffix referred to the run-record schema generation, not the trajectory
/// contract, and became actively misleading once a real V2 trajectory contract
/// existed. They are renamed here to say what they hold. The V2 tuple is not
/// restated: it is imported from the trajectory V2 owner module, so the two
/// tuples cannot drift independently.
const FROZEN_LEGACY_TRAJECTORY_IDENTITY_V1: &str =
    "mtg-kernel-native-full-episode-trajectory-sha256-v1";
const FROZEN_LEGACY_TRAJECTORY_GOLDENS_SCHEMA_V1: &str =
    "mtg_kernel_native_full_episode_trajectory_goldens/v1";
const FROZEN_LEGACY_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1: &str =
    "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1";
const FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1: &str =
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1";
const FROZEN_LEGACY_TRAJECTORY_GOLDENS_FILE_SHA256_V1: &str =
    "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b";
const FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_SHA256_V1: &str =
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
    /// Environment randomization V2 manifest section.
    ///
    /// Present if and only if this record declares the environment
    /// randomization V2 trajectory contract. Absent for every legacy V1
    /// record, and omitted entirely from canonical bytes when absent, so all
    /// existing run records keep byte-identical canonical output, `run_sha256`,
    /// standalone-semantics digest, and identity-bundle digest.
    ///
    /// Since C2 the declared classification is live: runtime entry points
    /// admit it exactly on the sealed mode diagonal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) environment_randomization_v2: Option<EnvironmentRandomizationContractV2>,
}

/// The strict environment randomization V2 manifest section.
///
/// Every field is validated against a projection of the production owner
/// constants in `environment_randomization_v2.rs`, never against a second
/// restatement of the same strings. `deny_unknown_fields` plus the full field
/// list makes both an unknown key and a missing key a decode failure, and the
/// canonical parser already rejects explicit `null`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnvironmentRandomizationContractV2 {
    pub(crate) identity: String,
    pub(crate) namespace: String,
    pub(crate) atom: String,
    pub(crate) extraction: String,
    pub(crate) ordered_atoms: Vec<Vec<String>>,
    pub(crate) owners: [String; 2],
    pub(crate) purposes: [String; 2],
    pub(crate) initial_ordinal_rule: String,
    pub(crate) overflow_rule: String,
    pub(crate) shuffle_algorithm: String,
    pub(crate) cross_language_goldens_schema: String,
    pub(crate) cross_language_goldens_file_sha256: String,
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
    /// Present if and only if `opponent_policy.identity` carries the ladder
    /// identity (Self-Play Ladder Design Contract S2, Section 3). Absent for
    /// every uniform-identity record; omitted entirely from canonical bytes
    /// when absent, so existing uniform run records are byte-for-byte
    /// unaffected by this field's addition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) opponent_ladder_pool: Option<OpponentLadderPoolContractV1>,
    /// Continual-initialization checkpoint reference (Self-Play Ladder
    /// Design Contract S2, Amendment 1 / Section 8A point 2). MUST be absent
    /// for the uniform identity. MAY be present or absent for the ladder
    /// identity: absent means fresh init from the common model snapshot
    /// (the pilot's historical shape); present means generation 0 seeds
    /// from this referenced checkpoint instead. Omitted entirely from
    /// canonical bytes when absent, so every existing run record (uniform
    /// AND the pilot's real fresh-init ladder record) is byte-for-byte
    /// unaffected by this field's addition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) opponent_ladder_initialization: Option<OpponentLadderInitializationContractV1>,
    /// Present if and only if `opponent_policy.identity` carries the ladder
    /// identity (Self-Play Ladder Design Contract S2, Section 2). Pins the
    /// V2 opponent seed-schedule namespace declarations layered on the V1
    /// trainer schedule (`trainer_schedule` above, unaffected). Absent for
    /// every uniform-identity record; omitted entirely from canonical bytes
    /// when absent, so existing uniform run records are byte-for-byte
    /// unaffected by this field's addition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) opponent_schedule_v2: Option<OpponentScheduleV2ContractV1>,
    pub(crate) trajectory: TrajectoryContractV2,
    pub(crate) standalone_semantics: StandaloneSemanticsV2,
    /// Capacity-experiment wide-net (`kernel-policy-value-net-8w128`) section
    /// (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3, SHA-256
    /// a50d067a5fb0f77b888e4e3c77386ca626e9b399a2a19f6959a1e7494f01380a).
    /// Present if and only if this record trains or evaluates the wide net;
    /// absent for every frozen-Net8 record. Omitted entirely from canonical
    /// bytes when absent, so every existing run record is byte-for-byte
    /// unaffected by this field's addition. Present: `model_snapshot`
    /// (`validate_snapshot_v1`) and `contracts.model`
    /// (`validate_model_contract_v2`) validate against the frozen WIDE
    /// constants instead of the frozen Net8 ones, fail-closed both
    /// directions. `diagnostic_label` MUST equal the frozen
    /// `WIDE-DIAGNOSTIC-NON-EVIDENCE` literal: the record-level, programmatic
    /// emission of the non-evidence label the contract requires in every
    /// wide report (as opposed to a label that is merely documented).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) wide_model_experiment_v1: Option<WideModelExperimentContractV1>,
    /// Fixed replay plus scaled population-program authority. Absent for
    /// every pre-existing RunV2 record, so its canonical bytes are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) population_program_v1: Option<PopulationProgramContractV1>,
    /// Exact program-update-1024 response-exploiter build authority. This is
    /// additive and omitted for every pre-existing RunV2 record, preserving
    /// their canonical bytes and behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) response_exploiter_v1: Option<ResponseExploiterContractV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationProgramContractV1 {
    pub(crate) identity: String,
    pub(crate) package_commit: String,
    pub(crate) program_document_sha256: String,
    pub(crate) retest_manifest_sha256: String,
    pub(crate) replay_end_generation: u64,
    pub(crate) program_update_count: u64,
    pub(crate) refresh_interval: u64,
    pub(crate) slot_count: u64,
    pub(crate) reward_identity: String,
    pub(crate) refresh_manifest_identity: String,
    pub(crate) retest_beta_f32_bits: String,
    pub(crate) expected_base_seed: u64,
    pub(crate) pool_identity: String,
    pub(crate) pool_document_sha256: String,
    pub(crate) parent_source_run_sha256: String,
    pub(crate) parent_generation: u64,
    pub(crate) parent_checkpoint_sha256: String,
    pub(crate) parent_sidecar_sha256: String,
    pub(crate) parent_state_sha256: String,
    pub(crate) parent_model_parameter_sha256: String,
    pub(crate) source_lineages: [PopulationSourceLineageV1; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationSourceLineageV1 {
    pub(crate) base_seed: u64,
    pub(crate) store_tree_sha256: String,
    pub(crate) run_sha256: String,
    pub(crate) checkpoint_sha256: String,
    pub(crate) sidecar_sha256: String,
    pub(crate) state_sha256: String,
    pub(crate) model_parameter_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseExploiterContractV1 {
    pub(crate) identity: String,
    pub(crate) package_commit: String,
    pub(crate) program_document_sha256: String,
    pub(crate) target_refresh_manifest_sha256: String,
    pub(crate) target_global_generation: u64,
    pub(crate) source_refresh_index: u64,
    pub(crate) source_program_update: u64,
    pub(crate) active_slot_indices: [u64; 6],
    pub(crate) excluded_slot_indices: [u64; 2],
    pub(crate) renormalization_identity: String,
    pub(crate) effective_weight_units: [u64; 8],
    pub(crate) effective_weight_total_units: u64,
    pub(crate) training_update_count: u64,
    pub(crate) episodes_per_update: u64,
    pub(crate) reward_identity: String,
    pub(crate) fresh_adam_after_weight_init_identity: String,
    pub(crate) authorized_base_seeds: [u64; 5],
    pub(crate) authorized_screen_seeds: [u64; 4],
    pub(crate) authorized_denovo_seeds: [u64; 1],
    // Phase 2 horizon amendment (CLAUDE-DENOVO-SCREEN-SHEET-V1.md), amended
    // again for backward compatibility: the 512-update denovo-screen-512
    // role's own dedicated authorized-seed array. It was originally
    // modeled as an unconditional, always-present field like
    // `authorized_base_seeds`/`authorized_screen_seeds`/`authorized_denovo_seeds`,
    // but that orphaned every record written before this field existed
    // (denovo-screen-256's real store's run.json among them) with a hard
    // decode failure, which is a real backward-compatibility defect, not
    // intended fail-closed behavior for a pre-amendment record. It is now
    // `Option`-shaped like the `parent_*` fields below:
    // `#[serde(default, skip_serializing_if = "Option::is_none")]` lets a
    // pre-amendment record (any role) decode with the field absent and
    // keeps its canonical bytes unchanged on re-encode (absence never
    // serializes as `null`). Presence/content is still role-conditional
    // and fail-closed, not merely tolerated: see
    // `validate_response_exploiter_v1` for the exact rule (denovo-screen-512
    // REQUIRES it present and exactly correct; every other role accepts
    // absence but still rejects wrong content if present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authorized_denovo_512_seeds: Option<[u64; 1]>,
    pub(crate) expected_base_seed: u64,
    pub(crate) run_role: String,
    pub(crate) expected_completion_generation: u64,
    pub(crate) policy_anchor_beta_f32_bits: String,
    // Option-per-role by design, not a sentinel/zero-digest scheme: "build"
    // and "screen" roles always carry `Some` (bound to promoted(2) gen-384,
    // matching `contracts.opponent_ladder_initialization`); "denovo-screen"
    // always carries `None` (there is no parent -- genesis is the frozen
    // common model snapshot, already declared and verified on every record
    // via `TrainRunV2::model_snapshot`, independent of this contract).
    // `#[serde(default, skip_serializing_if = "Option::is_none")]` keeps
    // every existing "build"/"screen" record's canonical bytes unchanged
    // (the field is always `Some` there and serializes exactly as before);
    // only new "denovo-screen" records omit the field. Validation (not
    // deserialization) enforces that presence is role-consistent -- see
    // `validate_response_exploiter_v1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_source_run_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_checkpoint_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_sidecar_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_state_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_model_parameter_sha256: Option<String>,
}

/// Capacity-experiment wide-net record section. See
/// [`TrainRunContractsV2::wide_model_experiment_v1`] for the presence/absence
/// contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WideModelExperimentContractV1 {
    pub(crate) architecture_identity: String,
    pub(crate) config_fingerprint: String,
    pub(crate) snapshot_sha256: String,
    pub(crate) manifest_core_sha256: String,
    pub(crate) payload_sha256: String,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) named_parameter_stream_sha256: String,
    pub(crate) parameter_tensor_count: u64,
    pub(crate) parameter_element_count: u64,
    pub(crate) diagnostic_label: String,
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

/// One checkpoint reference in the ladder pool (contract Section 3):
/// PRIMARY, PREDECESSOR-A, or PREDECESSOR-B. Hash-pins the source run, the
/// generation within that run, and the three artifact digests re-validated
/// at checkpoint load (checkpoint/sidecar/state).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentLadderCheckpointRefV1 {
    pub(crate) source_run_sha256: String,
    pub(crate) generation: u64,
    pub(crate) checkpoint_sha256: String,
    pub(crate) sidecar_sha256: String,
    pub(crate) state_sha256: String,
}

/// The continual-initialization checkpoint reference (Self-Play Ladder
/// Design Contract S2, Amendment 1 / Section 8A point 2): rung N's learner
/// initializes generation 0 from THIS checkpoint instead of the common
/// model snapshot. The first five fields are exactly
/// [`OpponentLadderCheckpointRefV1`]'s (same five-field hash-pin: source
/// run, generation within that run, and the three artifact digests
/// re-validated at load) -- a deliberately separate type, not a reuse of
/// that one, so the two sections stay independently versioned even though
/// their shapes largely coincide.
///
/// `derived_model_parameter_sha256` (design directive slice 2, making
/// ladder-init records SELF-CONTAINED for genesis validation): the
/// weights-only genesis payload's model-parameter digest, computed at
/// authoring time from the resolved reference via
/// `derive_genesis_weights_only_payload_v2_v3` +
/// `derive_genesis_model_parameter_sha256_v2_v3`
/// (`native_training_store_checkpoint_v3`). A REQUIRED field of this
/// section (not an `Option`): every record carrying the section carries a
/// complete, self-verifiable pin, so genesis validation
/// (`decode_genesis_checkpoint_manifest_v2_v3_self_contained`) never needs
/// to resolve the reference checkpoint from the filesystem. Shape-validated
/// as sha256-hex like the other five fields; see
/// `stage_ladder_checkpoint_initialization_v1`
/// (`native_ladder_pool_resolution_v1`) for the staging helper that keeps
/// record authoring and genesis authoring from ever independently
/// disagreeing about it.
///
/// Presence rule (validated in [`validate_opponent_policy_and_ladder_pool_v2`]):
/// MUST be absent for the uniform identity; MAY be present or absent for the
/// ladder identity. Absent under the ladder identity means FRESH INIT (the
/// pilot's historical shape, from the common model snapshot, which must
/// keep validating forever -- see
/// `real_ladder_pilot_run_json_validates_with_unchanged_run_sha256`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
/// TRUST BOUNDARY (review-adopted disclosure): the self-contained
/// validation paths (walk/publish decode dispatch and the inference-layer
/// authority binding) trust this record's own derived_model_parameter_sha256
/// claim, checked for digest shape only. The binding to a GENUINE prior
/// checkpoint is enforced exclusively at real training-time construction,
/// where run_native_science_loop_v1's genesis branch requires a resolved,
/// digest-gated, run-sha256-cross-checked reference and fails closed on any
/// mismatch with this claim. A hand-forged store directory with
/// self-consistent digests would pass self-contained validation, the same
/// pre-existing forged-record threat model that applies to trained
/// checkpoints' evidence chains.
pub struct OpponentLadderInitializationContractV1 {
    pub(crate) source_run_sha256: String,
    pub(crate) generation: u64,
    pub(crate) checkpoint_sha256: String,
    pub(crate) sidecar_sha256: String,
    pub(crate) state_sha256: String,
    pub(crate) derived_model_parameter_sha256: String,
}

/// The fourth pool slot: the uniform sampler, reused verbatim via the
/// superseded-identity semantics preserved for this slot only (contract
/// Section 3). These fields are pinned equal to the frozen uniform
/// `OpponentPolicyContractV2`/`OpponentSamplerContractV2` literals; the
/// uniform identities are not edited or repurposed by this reuse.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentLadderUniformFloorV1 {
    pub(crate) identity: String,
    pub(crate) model_rule: String,
    pub(crate) sampler_identity: String,
    pub(crate) sampler_algorithm: String,
}

/// The K = 4 ladder opponent pool (contract Section 3). Present if and only
/// if `opponent_policy` carries the ladder identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentLadderPoolContractV1 {
    pub(crate) identity: String,
    pub(crate) size: u64,
    /// The softmax-temperature-1.0 seeded categorical selection rule
    /// governing the three policy-driven slots (primary/predecessor-a/
    /// predecessor-b). The uniform floor slot has its own rule fields.
    pub(crate) policy_member_sampling_rule: String,
    pub(crate) weight_primary: u64,
    pub(crate) weight_predecessor_a: u64,
    pub(crate) weight_predecessor_b: u64,
    pub(crate) weight_uniform_floor: u64,
    pub(crate) primary: OpponentLadderCheckpointRefV1,
    pub(crate) predecessor_a: OpponentLadderCheckpointRefV1,
    pub(crate) predecessor_b: OpponentLadderCheckpointRefV1,
    pub(crate) uniform_floor: OpponentLadderUniformFloorV1,
}

/// Pins the V2 opponent seed-schedule namespace declarations layered on the
/// V1 trainer schedule (Self-Play Ladder Design Contract S2, Section 2):
/// `train-opponent-pool-choice` (per-episode pool member selection) and
/// `train-opponent-policy-substep` (per-decision softmax sampling). Present
/// if and only if `opponent_policy` carries the ladder identity; mirrors
/// `OpponentLadderPoolContractV1`'s presence rule exactly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpponentScheduleV2ContractV1 {
    pub(crate) schedule_version: String,
    pub(crate) seed_version: String,
    pub(crate) opponent_pool_choice_namespace: String,
    pub(crate) opponent_pool_choice_fields: String,
    pub(crate) opponent_policy_substep_namespace: String,
    pub(crate) opponent_policy_substep_fields: String,
    pub(crate) pool_choice_modulo: u64,
    pub(crate) pool_choice_threshold_rule: String,
    pub(crate) pool_choice_bias_rule: String,
    pub(crate) version_change_rule: String,
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
    /// The closed trajectory-contract classification decided at decode time.
    /// Private on purpose: no caller may construct, override, or widen it, and
    /// the only way to obtain one is to decode a complete, coherent record.
    environment_trajectory_contract: NativeRunEnvironmentTrajectoryContractV1,
    /// The closed catalog-identity profile classification decided at decode
    /// time (Dual-Profile Catalog Successor, collab CLAUDE #220). Private for
    /// the same reason as `environment_trajectory_contract` above.
    catalog_profile: NativeRunCatalogProfileV1,
}

/// The closed trajectory-contract classification of a validated run.
///
/// Sealed and crate-private. A record is exactly one of these, decided by a
/// complete-tuple match at decode time; there is no third state, no default,
/// and no caller-selectable version flag. Since C2 both classifications are
/// live: every runtime entry point admits exactly the diagonal of this
/// classification against the sealed executor mode, transition mode, and
/// receipt variant, and rejects every off-diagonal pairing fail-closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeRunEnvironmentTrajectoryContractV1 {
    LegacyV1,
    EnvironmentRandomizationV2,
}

/// The closed catalog-identity profile classification of a validated run
/// (Dual-Profile Catalog Successor, collab CLAUDE #220).
///
/// Sealed and crate-private. A record is exactly one of these two, decided by
/// a complete-tuple match (`card_db_hash_u64_hex`, `runtime_catalog_sha256`)
/// against exactly one of two disjoint frozen literal pairs at decode time in
/// [`classify_catalog_profile_v1`]; there is no third state, no default, and
/// every hybrid (neither pair, or a value from one field's pair paired with
/// the other field's opposite pair) is rejected. `Historical` pins the frozen
/// rev3 two-deck catalog forever, byte-identical, and stays readable: a
/// historical record decodes and validates cleanly. `Current` pins the live
/// nine-deck catalog as of the runtime-decks-nine landing. Callers that must
/// admit only live science-loop authority (science-loop use, publication,
/// resume) reject `Historical` with a specific error at their own boundary;
/// this module itself never refuses to decode or validate either profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeRunCatalogProfileV1 {
    Historical,
    Current,
}

impl ValidatedTrainRunV2 {
    pub fn record(&self) -> &TrainRunV2 {
        &self.record
    }

    /// The trajectory contract this record was classified as at decode time.
    /// Crate-private and by value: the classification is read-only evidence,
    /// not a switch.
    pub(crate) fn environment_trajectory_contract_v1(
        &self,
    ) -> NativeRunEnvironmentTrajectoryContractV1 {
        self.environment_trajectory_contract
    }

    /// The catalog-identity profile this record was classified as at decode
    /// time. Crate-private and by value, same discipline as
    /// `environment_trajectory_contract_v1`: read-only evidence, not a
    /// switch. Callers that must admit only live science-loop authority
    /// (science-loop use, publication, resume) use this to reject
    /// `NativeRunCatalogProfileV1::Historical` at their own boundary with
    /// their own specific error.
    pub(crate) fn catalog_profile_v1(&self) -> NativeRunCatalogProfileV1 {
        self.catalog_profile
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

    /// The store-admitted numerical backend this run declares, derived from
    /// the validated train-step backend identity literal.
    pub(crate) fn store_numerical_backend_v2(
        &self,
    ) -> Option<crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1> {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        match self
            .record
            .contracts
            .train_step
            .numerical_backend_identity
            .as_str()
        {
            FROZEN_NUMERICAL_BACKEND_IDENTITY_V2 => {
                Some(NativeTrainingNumericalBackendV1::Sequential)
            }
            crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1 => {
                Some(NativeTrainingNumericalBackendV1::CudaBurnDense)
            }
            _ => None,
        }
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
    validate_frozen_rev3_authorities_v2()?;
    validate_package_v2(&record.package)?;
    validate_toolchain_v2(&record.toolchain)?;
    validate_source_v2(&record.source)?;
    validate_runtime_v2(&record.runtime, &record.toolchain)?;
    validate_environment_v2(&record.environment)?;
    validate_snapshot_v1(
        &record.model_snapshot,
        record.contracts.wide_model_experiment_v1.as_ref(),
    )?;
    validate_contracts_v2(&record.contracts)?;
    validate_optimization_v2(&record.optimization)?;
    let requested_episode_count = validate_schedule_v2(&record.schedule, &record.model_snapshot)?;
    validate_population_program_v1(&record)?;
    validate_response_exploiter_v1(&record)?;
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

    // Two closed classifications, after all shared environment/contracts/
    // cross-binding validation and before standalone-semantics reconstruction.
    let environment_trajectory_contract = classify_environment_trajectory_contract_v1(&record)?;
    let catalog_profile = classify_catalog_profile_v1(&record.environment)?;

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
        environment_trajectory_contract,
        catalog_profile,
    })
}

/// The one closed trajectory-contract classifier.
///
/// Exactly two complete tuples are admissible, and every other cross-product of
/// the section state, the protocol/schema pair, and the six trajectory pins is
/// rejected. Each arm is complete in itself: the legacy arm never consults V2
/// authority health, and the V2 arm validates the V2 live owners plus every
/// field of the manifest section against projections of the production owner
/// constants.
fn classify_environment_trajectory_contract_v1(
    record: &TrainRunV2,
) -> Result<NativeRunEnvironmentTrajectoryContractV1> {
    let environment = &record.environment;
    let trajectory = &record.contracts.trajectory;

    // Each arm computes only its own tuple. Nothing about the V2 authority
    // owners is evaluated on the legacy path, so a legacy record can never be
    // rejected, accepted, or even influenced by unrelated V2 authority health.
    match environment.environment_randomization_v2.as_ref() {
        None => {
            let legacy_versions = environment.protocol_version
                == u64::from(FROZEN_PROTOCOL_VERSION_V2)
                && environment.schema_version == u64::from(FROZEN_SCHEMA_VERSION_V2);
            let legacy_pins = trajectory.identity == FROZEN_LEGACY_TRAJECTORY_IDENTITY_V1
                && trajectory.cross_language_goldens_schema
                    == FROZEN_LEGACY_TRAJECTORY_GOLDENS_SCHEMA_V1
                && trajectory.cross_language_generator_identity
                    == FROZEN_LEGACY_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1
                && trajectory.cross_language_golden_stream_identity
                    == FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1
                && trajectory.cross_language_goldens_file_sha256
                    == FROZEN_LEGACY_TRAJECTORY_GOLDENS_FILE_SHA256_V1
                && trajectory.cross_language_golden_stream_sha256
                    == FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_SHA256_V1;
            if legacy_pins && legacy_versions {
                Ok(NativeRunEnvironmentTrajectoryContractV1::LegacyV1)
            } else {
                Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral))
            }
        }
        Some(section) => {
            let v2_versions = environment.protocol_version
                == u64::from(RL_SESSION_PROTOCOL_VERSION_V6)
                && environment.schema_version == u64::from(RL_SESSION_SCHEMA_VERSION_V6);
            let v2_pins = trajectory.identity == NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2
                && trajectory.cross_language_goldens_schema
                    == NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V2
                && trajectory.cross_language_generator_identity
                    == NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2
                && trajectory.cross_language_golden_stream_identity
                    == NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2
                && trajectory.cross_language_goldens_file_sha256
                    == NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V2
                && trajectory.cross_language_golden_stream_sha256
                    == NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V2;
            if v2_pins && v2_versions && environment_randomization_section_is_exact_v2(section) {
                Ok(NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2)
            } else {
                Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral))
            }
        }
    }
}

/// Every manifest section field against a projection of the production owner
/// constants. This is the only place V2 environment authority health is
/// consulted, and it runs only inside the V2 classifier arm.
fn environment_randomization_section_is_exact_v2(
    section: &EnvironmentRandomizationContractV2,
) -> bool {
    let expected_ordered_atoms: Vec<Vec<String>> = ENVIRONMENT_RANDOMIZATION_ORDERED_ATOMS_V2
        .iter()
        .map(|row| row.iter().map(|part| (*part).to_string()).collect())
        .collect();
    section.identity == ENVIRONMENT_RANDOMIZATION_IDENTITY_V2
        && section.namespace == ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2
        && section.atom == ENVIRONMENT_RANDOMIZATION_ATOM_FRAMING_V2
        && section.extraction == ENVIRONMENT_RANDOMIZATION_EXTRACTION_V2
        && section.ordered_atoms == expected_ordered_atoms
        && section.owners == ENVIRONMENT_RANDOMIZATION_OWNERS_V2
        && section.purposes == ENVIRONMENT_RANDOMIZATION_PURPOSES_V2
        && section.initial_ordinal_rule == ENVIRONMENT_RANDOMIZATION_INITIAL_ORDINAL_RULE_V2
        && section.overflow_rule == ENVIRONMENT_RANDOMIZATION_OVERFLOW_RULE_V2
        && section.shuffle_algorithm == ENVIRONMENT_RANDOMIZATION_SHUFFLE_ALGORITHM_V2
        && section.cross_language_goldens_schema == ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1
        && section.cross_language_goldens_file_sha256 == ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1
}

/// The one closed catalog-identity profile classifier (Dual-Profile Catalog
/// Successor, collab CLAUDE #220).
///
/// Exactly two complete tuples are admissible: the record's own
/// `card_db_hash_u64_hex` and `runtime_catalog_sha256` fields must equal
/// EITHER both HISTORICAL frozen rev3 literals (`FROZEN_CARD_DB_HASH_U64_HEX_V2`,
/// `FROZEN_RUNTIME_CATALOG_SHA256_V2`, byte-identical forever) OR both CURRENT
/// frozen literals (`FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`,
/// `FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1`, pinned to the live nine-deck
/// catalog as of the runtime-decks-nine landing); every other combination,
/// including a hybrid that matches one field's literal from one tuple and the
/// other field's literal from the other tuple, is rejected. This mirrors
/// `classify_environment_trajectory_contract_v1`'s own shape (whole-tuple
/// selection before any partial-field tolerance, every hybrid rejected) but
/// is a distinct, independent classification: a record's trajectory contract
/// and its catalog profile vary independently.
fn classify_catalog_profile_v1(
    environment: &TrainRunEnvironmentV2,
) -> Result<NativeRunCatalogProfileV1> {
    let historical = environment.card_db_hash_u64_hex == FROZEN_CARD_DB_HASH_U64_HEX_V2
        && environment.runtime_catalog_sha256 == FROZEN_RUNTIME_CATALOG_SHA256_V2;
    let current = environment.card_db_hash_u64_hex == FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1
        && environment.runtime_catalog_sha256 == FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1;
    match (historical, current) {
        (true, false) => Ok(NativeRunCatalogProfileV1::Historical),
        (false, true) => Ok(NativeRunCatalogProfileV1::Current),
        _ => Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral)),
    }
}

/// The crate's actual live catalog-identity build constants, read fresh each
/// call. Test builds may override the returned pair for exactly the calling
/// thread via [`LiveCatalogBuildIdentityOverrideGuardV1`] to simulate a
/// future catalog change (a build whose live constants have moved past the
/// pinned CURRENT literal) without waiting for one; production builds always
/// return the real, unmodified `KERNEL_CARDDB_HASH`/`RUNTIME_DECK_CATALOG_FILE_SHA256`.
fn live_catalog_build_identity_v1() -> (String, String) {
    #[cfg(test)]
    if let Some(overridden) =
        LIVE_CATALOG_BUILD_IDENTITY_OVERRIDE_V1.with(|cell| cell.borrow().clone())
    {
        return overridden;
    }
    (
        format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH),
        crate::runtime_decks::RUNTIME_DECK_CATALOG_FILE_SHA256.to_owned(),
    )
}

#[cfg(test)]
thread_local! {
    static LIVE_CATALOG_BUILD_IDENTITY_OVERRIDE_V1: std::cell::RefCell<Option<(String, String)>> =
        const { std::cell::RefCell::new(None) };
}

/// Test-only per-thread override for [`live_catalog_build_identity_v1`].
/// Installing simulates a build whose live catalog constants differ from the
/// pinned CURRENT literal (the only way to exercise the CURRENT-profile
/// mutation-boundary authenticity check's rejection path today, since the
/// crate's real live constants currently equal the pinned CURRENT literal
/// exactly -- see `current_frozen_literal_matches_the_live_build_constant`).
/// RAII: the override is cleared on drop, including on panic, so no failed
/// test can leak a shimmed identity into a later same-thread test.
#[cfg(test)]
pub(crate) struct LiveCatalogBuildIdentityOverrideGuardV1;

#[cfg(test)]
impl LiveCatalogBuildIdentityOverrideGuardV1 {
    pub(crate) fn install(card_db_hash_u64_hex: &str, runtime_catalog_sha256: &str) -> Self {
        LIVE_CATALOG_BUILD_IDENTITY_OVERRIDE_V1.with(|cell| {
            *cell.borrow_mut() = Some((
                card_db_hash_u64_hex.to_owned(),
                runtime_catalog_sha256.to_owned(),
            ));
        });
        Self
    }
}

#[cfg(test)]
impl Drop for LiveCatalogBuildIdentityOverrideGuardV1 {
    fn drop(&mut self) {
        LIVE_CATALOG_BUILD_IDENTITY_OVERRIDE_V1.with(|cell| {
            *cell.borrow_mut() = None;
        });
    }
}

/// Live catalog-identity authenticity check for CURRENT-profile mutation
/// boundaries (Dual-Profile Catalog Successor fix round, panel finding 1,
/// blocker: bypass). Decode intentionally never reads the crate's live build
/// constants (a record's catalog fields are checked only against the pinned
/// CURRENT/HISTORICAL literal pairs in `classify_catalog_profile_v1`) -- but
/// that alone means a hand-fabricated record whose catalog fields merely
/// equal the pinned CURRENT literal would seal with no authenticity check
/// anywhere, including after a future catalog change moves the crate's real
/// live constants past that literal (the exact bypass the panel identified,
/// symmetric to the original rev3 outage this successor exists to fix).
/// This closes it at the two mutation boundaries (publish, resume): a
/// CURRENT-profile record's own embedded fields must equal the crate's live
/// constants at THIS moment, not merely the frozen pin. Returns `true` when
/// they match. HISTORICAL-profile records never reach this function (each
/// boundary's exhaustive match rejects them in their own arm first).
pub(crate) fn current_profile_matches_live_build_identity_v1(
    environment: &TrainRunEnvironmentV2,
) -> bool {
    let (live_card_db_hash_u64_hex, live_runtime_catalog_sha256) = live_catalog_build_identity_v1();
    environment.card_db_hash_u64_hex == live_card_db_hash_u64_hex
        && environment.runtime_catalog_sha256 == live_runtime_catalog_sha256
}

/// Dual-Profile Catalog Successor (collab CLAUDE #220): this function is now
/// catalog-profile-scoped. It no longer compares the crate's live
/// `KERNEL_CARDDB_HASH` / `RUNTIME_DECK_CATALOG_FILE_SHA256` build constants
/// against the frozen rev3 (`FROZEN_CARD_DB_HASH_U64_V2` /
/// `FROZEN_RUNTIME_CATALOG_SHA256_V2`) literals at all -- that ambient
/// live-owner check is removed here (previously two `||` arms of this same
/// function). Two disjoint reasons, one per profile: for a HISTORICAL-profile
/// decode, the crate's live catalog constants moved permanently to the
/// nine-deck identity with the runtime-decks-nine landing, so requiring them
/// to still equal the frozen rev3 pin would make every historical record
/// permanently undecodable, destroying regression evidence rather than
/// gating it (the exact catch the historical-decoder ruling made the first
/// time this pattern came up); for a CURRENT-profile decode, this module
/// deliberately performs no live-build-constant check at decode time at all
/// -- only construction/mutation call sites (production capture) read the
/// live constants, and they always do so directly, never through a decode-
/// time pin, per the design's "live checks at construction/mutation time
/// only" binding. Catalog-identity exactness for BOTH profiles is instead
/// independently enforced against the record's own embedded
/// `card_db_hash_u64_hex`/`runtime_catalog_sha256` fields by
/// `classify_catalog_profile_v1`, which every decode still calls and which
/// rejects every hybrid. Every other literal check below is untouched and
/// unconditional: none of these other authorities differ between the two
/// catalog profiles, so they continue to gate both exactly as before. The
/// frozen literals themselves (rev3 and the two catalog format constants
/// shared by both profiles) are byte-identical to before this change.
fn validate_frozen_rev3_authorities_v2() -> Result<()> {
    let rally = runtime_deck_by_id(CANONICAL_RALLY_DECK_ID)
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral))?;
    if SOURCE_TREE_RECIPE_IDENTITY_V1 != FROZEN_SOURCE_TREE_RECIPE_IDENTITY_V2
        || SOURCE_TREE_RECIPE_SHA256_V1 != FROZEN_SOURCE_TREE_RECIPE_SHA256_V2
        || STRICT_SOURCE_TREE_RECIPE_BYTE_COUNT_V1 != FROZEN_SOURCE_TREE_RECIPE_BYTE_COUNT_V2
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
        || SNAPSHOT_SCHEMA_V1 != FROZEN_SNAPSHOT_SCHEMA_V2
        || SNAPSHOT_IDENTITY_V1 != FROZEN_SNAPSHOT_IDENTITY_V2
        || u64::try_from(PAYLOAD_BYTE_COUNT_V1).ok() != Some(FROZEN_SNAPSHOT_PAYLOAD_BYTE_COUNT_V2)
        || u64::try_from(PARAMETER_TENSOR_COUNT_V1).ok() != Some(FROZEN_PARAMETER_TENSOR_COUNT_V2)
        || u64::try_from(PARAMETER_ELEMENT_COUNT_V1).ok() != Some(FROZEN_PARAMETER_ELEMENT_COUNT_V2)
        || MODEL_CONFIG_FINGERPRINT_V1 != FROZEN_MODEL_CONFIG_FINGERPRINT_V2
        || MODEL_ARCHITECTURE_VERSION_V1 != FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2
        || FEATURE_CONTRACT_DIGEST_V1 != FROZEN_FEATURE_CONTRACT_DIGEST_V2
        || FEATURE_ENCODING_DIGEST_V1 != FROZEN_FEATURE_ENCODING_DIGEST_V2
        || INITIALIZER_IDENTITY_V1 != FROZEN_INITIALIZER_IDENTITY_V2
        || BASE_SEED_V1 != FROZEN_BASE_SEED_V2
        || MODEL_INIT_SEED_V1 != FROZEN_MODEL_INIT_SEED_V2
        || NATIVE_TRAINER_SCHEDULE_VERSION_V1 != FROZEN_TRAINER_SCHEDULE_IDENTITY_V2
        || PYTHON_REFERENCE_SEED_VERSION_V1 != FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2
        || NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1 != FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2
        || AUTHORITY_RUNTIME_IDENTITY_V1 != FROZEN_SNAPSHOT_AUTHORITY_RUNTIME_IDENTITY_V2
        || RUST_LOADER_IDENTITY_V1 != FROZEN_SNAPSHOT_LOADER_IDENTITY_V2
        || NONCLAIM_V1 != FROZEN_SNAPSHOT_NONCLAIM_V2
        || NATIVE_TRAINER_CONTRACT_IDENTITY_V2 != FROZEN_TRAINER_IDENTITY_V2
        || NATIVE_FLAT_TENSORIZER_IDENTITY_V2 != FROZEN_TENSORIZER_IDENTITY_V2
        // Feature-Encoder Successor (collab CLAUDE #221): the live
        // `NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2` constant now
        // permanently tracks the CURRENT features.py identity (moved
        // forward with the historical stack-source encoder fix), so an
        // unconditional equality check against the HISTORICAL-only
        // `FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2` mirror here would
        // make every live build fail this function permanently -- the same
        // outage class the Dual-Profile Catalog Successor fixed for
        // `KERNEL_CARDDB_HASH`/`FROZEN_CARD_DB_HASH_U64_V2`. The record-field
        // check (`validate_contracts_v2`, via
        // `matches_frozen_tensorizer_authority_source_sha256_v1`) is the
        // authority for this axis now; no live-build-constant check runs at
        // decode time. See `current_profile_matches_live_build_identity_v1`
        // for the equivalent catalog-axis mutation-boundary authenticity
        // check; an analogous check for this axis is a reasonable follow-up
        // fix-round item but is not required to prevent decode outages
        // (unlike catalog identity, mixing historical/current
        // feature-encoder-profile records in the science loop is safe).
        || NATIVE_FLAT_TENSORIZER_FIXTURE_SHA256_V2 != FROZEN_TENSORIZER_FIXTURE_SHA256_V2
        || NATIVE_FLAT_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2
            != FROZEN_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2
        || u64::try_from(PARAMETER_COUNT_V1).ok() != Some(FROZEN_PARAMETER_ELEMENT_COUNT_V2)
        || TRAINER_ALGORITHM_V1 != FROZEN_LOSS_IDENTITY_V2
        || TRAIN_STEP_IDENTITY_V1 != FROZEN_TRAIN_STEP_IDENTITY_V2
        || NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1
            != FROZEN_NUMERICAL_BACKEND_IDENTITY_V2
        || NATIVE_OPTIMIZER_IDENTITY_V1 != FROZEN_OPTIMIZER_IDENTITY_V2
        || NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1 != FROZEN_GAUGE_EVIDENCE_IDENTITY_V2
        || CANONICAL_GAUGE_PARAMETERS_V1 != FROZEN_CANONICAL_GAUGE_PARAMETERS_V2
        || FAST_CATEGORICAL_SAMPLER_VERSION != FROZEN_LEARNER_SAMPLER_IDENTITY_V2
        || FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256 != FROZEN_LEARNER_SAMPLER_CONTRACT_SHA256_V2
        || FAST_CATEGORICAL_EXP_TABLE_SHA256 != FROZEN_LEARNER_SAMPLER_EXP_TABLE_SHA256_V2
        || FAST_CATEGORICAL_CROSS_LANGUAGE_VECTORS_FILE_SHA256
            != FROZEN_LEARNER_VECTORS_FILE_SHA256_V2
        || FAST_CATEGORICAL_CROSS_LANGUAGE_VECTOR_STREAM_SHA256
            != FROZEN_LEARNER_VECTOR_STREAM_SHA256_V2
        || NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1 != FROZEN_OPPONENT_POLICY_IDENTITY_V2
        || NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1
            != FROZEN_OPPONENT_POLICY_MODEL_RULE_V2
        || UNIFORM_INDEX_MODULO_U64_IDENTITY_V1 != FROZEN_OPPONENT_SAMPLER_IDENTITY_V2
        || UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1 != FROZEN_OPPONENT_SAMPLER_ALGORITHM_V2
        || NATIVE_OPPONENT_SAMPLER_VECTORS_FILE_SHA256_V1 != FROZEN_OPPONENT_VECTORS_FILE_SHA256_V2
        || NATIVE_OPPONENT_SAMPLER_VECTOR_STREAM_SHA256_V1
            != FROZEN_OPPONENT_VECTOR_STREAM_SHA256_V2
        || FROZEN_CHECKPOINT_OPPONENT_POLICY_IDENTITY_V2
            != FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2
        || FROZEN_CHECKPOINT_OPPONENT_POLICY_MODEL_RULE_V2
            != FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2
        || FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2
            != FROZEN_LADDER_POLICY_SAMPLING_RULE_V2
        || OPPONENT_LADDER_POOL_IDENTITY_V2 != FROZEN_LADDER_POOL_IDENTITY_V2
        || OPPONENT_LADDER_POOL_SIZE_V2 != FROZEN_LADDER_POOL_SIZE_V2
        || OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2 != FROZEN_LADDER_POOL_WEIGHT_PRIMARY_V2
        || OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2
            != FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2
        || OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2
            != FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2
        || OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2
            != FROZEN_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.schedule_version != FROZEN_LADDER_SCHEDULE_VERSION_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.seed_version
            != FROZEN_LADDER_SCHEDULE_SEED_VERSION_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.opponent_pool_choice_namespace
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_NAMESPACE_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.opponent_pool_choice_fields
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_FIELDS_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.opponent_policy_substep_namespace
            != FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_NAMESPACE_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.opponent_policy_substep_fields
            != FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_FIELDS_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.pool_choice_modulo
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_MODULO_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.pool_choice_threshold_rule
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_THRESHOLD_RULE_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.pool_choice_bias_rule
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_BIAS_RULE_V2
        || NATIVE_TRAINER_SCHEDULE_CONTRACT_V2.version_change_rule
            != FROZEN_LADDER_SCHEDULE_VERSION_CHANGE_RULE_V2
        // The live V1 trajectory owner guard is unconditional and stays that
        // way: the V2 envelope still wraps the inner V1 trajectory digest, so
        // V1 authority health is a precondition of both classifier arms. The
        // converse is not true, which is why V2 owner health is checked only
        // inside the V2 arm of the classifier and never here.
        || NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1 != FROZEN_LEGACY_TRAJECTORY_IDENTITY_V1
        || NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V1
            != FROZEN_LEGACY_TRAJECTORY_GOLDENS_SCHEMA_V1
        || NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1
            != FROZEN_LEGACY_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1
        || NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1
            != FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1
        || NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V1
            != FROZEN_LEGACY_TRAJECTORY_GOLDENS_FILE_SHA256_V1
        || NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V1
            != FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_SHA256_V1
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
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
        || source.source_tree_recipe_identity != FROZEN_SOURCE_TREE_RECIPE_IDENTITY_V2
        || source.source_tree_recipe_sha256 != FROZEN_SOURCE_TREE_RECIPE_SHA256_V2
        || source.source_tree_recipe_byte_count != FROZEN_SOURCE_TREE_RECIPE_BYTE_COUNT_V2
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
    if store_backend_identity_for_runtime_tuple_v2(&runtime.tuple_identity).is_none()
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
        || store_backend_identity_for_runtime_tuple_v2(&runtime.tuple_identity)
            != Some(runtime.numerical_backend_identity.as_str())
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

/// Dual-Profile Catalog Successor (collab CLAUDE #220): the live-owner block
/// below no longer pins `KERNEL_CARDDB_HASH`/`RUNTIME_DECK_CATALOG_FILE_SHA256`
/// against the historical rev3 literal, and the record-field block no longer
/// pins `environment.card_db_hash_u64_hex`/`environment.runtime_catalog_sha256`
/// against it either -- both catalog *content* identities are deliberately
/// carved out of this function entirely (for both profiles, at decode time)
/// and delegated whole to `classify_catalog_profile_v1`, exactly the same
/// delegation shape this function already uses for
/// `environment.protocol_version`/`environment.schema_version` (owned by
/// `classify_environment_trajectory_contract_v1`, see the comment below).
/// See `validate_frozen_rev3_authorities_v2` for the full reasoning; every
/// other literal here (catalog *format* schema/protocol, the Rally deck's own
/// hash, protocol name, kernel/surface/policy-surface versions) is unchanged
/// and unconditional, since none of those differ between catalog profiles.
fn validate_environment_v2(environment: &TrainRunEnvironmentV2) -> Result<()> {
    let rally = runtime_deck_by_id(CANONICAL_RALLY_DECK_ID)
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral))?;
    if RUNTIME_DECK_CATALOG_SCHEMA != FROZEN_RUNTIME_CATALOG_SCHEMA_V2
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
    if environment.runtime_catalog_schema != FROZEN_RUNTIME_CATALOG_SCHEMA_V2
        || environment.runtime_catalog_protocol != FROZEN_RUNTIME_CATALOG_PROTOCOL_V2
        // `environment.card_db_hash_u64_hex` and `environment.runtime_catalog_sha256`
        // are deliberately not pinned here; see the function doc comment.
        || environment.deck_ids != [FROZEN_RALLY_DECK_ID_V2, FROZEN_RALLY_DECK_ID_V2]
        || environment.deck_hashes_u64_hex
            != [
                FROZEN_RALLY_DECK_HASH_U64_HEX_V2,
                FROZEN_RALLY_DECK_HASH_U64_HEX_V2,
            ]
        || environment.protocol != FROZEN_PROTOCOL_V2
        // `environment.protocol_version` and `environment.schema_version` are
        // deliberately not pinned here. They are version-bearing members of the
        // closed trajectory-contract tuple (5/5 legacy, 6/6 environment
        // randomization V2) and are validated by
        // `classify_environment_trajectory_contract_v1`.
        //
        // The live-owner block above pins this *build* at protocol/schema 5,
        // which is why this build's production capture cannot mint a 6/6
        // declaration: capture reads the live constants. A coherent 6/6
        // record still validates and classifies here, and since C2 it
        // executes: runtime entry points admit exactly the diagonal of the
        // sealed classification against executor mode, transition mode, and
        // receipt variant, with production capture comparing a V2 run's ten
        // common live environment facts against the live Legacy capture.
        || environment.kernel_version != FROZEN_KERNEL_VERSION_V2
        || environment.surface_version != u64::from(FROZEN_SURFACE_VERSION_V2)
        || environment.policy_surface_version != u64::from(FROZEN_POLICY_SURFACE_VERSION_V2)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

/// Dispatches on the record's wide-net section (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md
/// Section 3): absent validates `model_snapshot` byte-for-byte as today
/// (`validate_frozen_snapshot_v1`, untouched); present validates against the
/// frozen WIDE mirrors instead (`validate_wide_snapshot_v1`). Fail-closed
/// both directions: neither branch accepts the other's literals.
fn validate_snapshot_v1(
    snapshot: &CommonModelSnapshotRecordV1,
    wide: Option<&WideModelExperimentContractV1>,
) -> Result<()> {
    match wide {
        None => validate_frozen_snapshot_v1(snapshot),
        Some(wide_contract) => validate_wide_snapshot_v1(snapshot, wide_contract),
    }
}

fn validate_frozen_snapshot_v1(snapshot: &CommonModelSnapshotRecordV1) -> Result<()> {
    if snapshot.schema != FROZEN_SNAPSHOT_SCHEMA_V2
        || snapshot.identity != FROZEN_SNAPSHOT_IDENTITY_V2
        || snapshot.snapshot_sha256 != FROZEN_SNAPSHOT_SHA256_V2
        || snapshot.manifest_file_sha256 != FROZEN_SNAPSHOT_MANIFEST_FILE_SHA256_V2
        || snapshot.manifest_core_sha256 != FROZEN_SNAPSHOT_MANIFEST_CORE_SHA256_V2
        || snapshot.payload_sha256 != FROZEN_SNAPSHOT_PAYLOAD_SHA256_V2
        || snapshot.payload_byte_count != FROZEN_SNAPSHOT_PAYLOAD_BYTE_COUNT_V2
        || snapshot.parameter_layout_sha256 != FROZEN_PARAMETER_LAYOUT_SHA256_V2
        || snapshot.named_parameter_stream_sha256
            != FROZEN_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V2
        || snapshot.loaded_named_parameter_stream_sha256
            != FROZEN_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V2
        || snapshot.parameter_tensor_count != FROZEN_PARAMETER_TENSOR_COUNT_V2
        || snapshot.parameter_element_count != FROZEN_PARAMETER_ELEMENT_COUNT_V2
        || snapshot.model_config_fingerprint != FROZEN_MODEL_CONFIG_FINGERPRINT_V2
        || snapshot.model_architecture_version != FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2
        || snapshot.feature_contract_digest != FROZEN_FEATURE_CONTRACT_DIGEST_V2
        || snapshot.feature_encoding_digest != FROZEN_FEATURE_ENCODING_DIGEST_V2
        || snapshot.initializer_identity != FROZEN_INITIALIZER_IDENTITY_V2
        || snapshot.base_seed != FROZEN_BASE_SEED_V2
        || snapshot.model_init_seed != FROZEN_MODEL_INIT_SEED_V2
        || snapshot.trainer_schedule_version != FROZEN_TRAINER_SCHEDULE_IDENTITY_V2
        || snapshot.python_reference_seed_version != FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2
        || snapshot.schedule_goldens_sha256 != FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2
        || snapshot.authority_source_bundle_sha256
            != FROZEN_SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V2
        || snapshot.authority_runtime_identity != FROZEN_SNAPSHOT_AUTHORITY_RUNTIME_IDENTITY_V2
        || snapshot.loader_identity != FROZEN_SNAPSHOT_LOADER_IDENTITY_V2
        || snapshot.optimizer_identity != FROZEN_OPTIMIZER_IDENTITY_V2
        || snapshot.adam_step_initial != FROZEN_ADAM_STEP_INITIAL_V2
        || snapshot.moment_initialization != FROZEN_MOMENT_INITIALIZATION_V2
        || snapshot.canonical_gauge_parameters != FROZEN_CANONICAL_GAUGE_PARAMETERS_V2
        || snapshot.scorer_bias_anchor_f32_bits != FROZEN_SCORER_BIAS_ANCHOR_F32_BITS_V2
        || !snapshot.snapshot_load_completed_before_trial_start
        || snapshot.snapshot_load_timed
        || snapshot.rust_seeded_initializer_reproduced
        || snapshot.nonclaim != FROZEN_SNAPSHOT_NONCLAIM_V2
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

/// Lockstep cross-check for the capacity-experiment wide-net mirrors: each
/// wide "owner" constant (the identities/counts the wide modules actually
/// construct with) must equal the frozen literal pinned in this module,
/// exactly the discipline `validate_frozen_rev3_authorities_v2` applies to
/// the Net8 identities. Called only from the wide branch of
/// `validate_snapshot_v1`, so a record without
/// `contracts.wide_model_experiment_v1` never pays this cost and the frozen
/// path above is untouched.
fn validate_frozen_wide_rev1_authorities_v1() -> Result<()> {
    use crate::common_model_snapshot_v1::{
        WIDE_PARAMETER_ELEMENT_COUNT_V1, WIDE_PARAMETER_TENSOR_COUNT_V1,
        WIDE_PAYLOAD_BYTE_COUNT_V1, WIDE_RUST_LOADER_IDENTITY_V1, WIDE_SNAPSHOT_IDENTITY_V1,
    };
    use crate::native_policy_value_net_v1::{
        W_ARCHITECTURE_LABEL_V1, W_MODEL_ARCHITECTURE_VERSION_V1, W_MODEL_CONFIG_FINGERPRINT_V1,
        W_PARAMETER_COUNT_V1,
    };
    if WIDE_SNAPSHOT_IDENTITY_V1 != FROZEN_WIDE_SNAPSHOT_IDENTITY_V1
        || u64::try_from(WIDE_PAYLOAD_BYTE_COUNT_V1).ok()
            != Some(FROZEN_WIDE_SNAPSHOT_PAYLOAD_BYTE_COUNT_V1)
        || u64::try_from(WIDE_PARAMETER_TENSOR_COUNT_V1).ok()
            != Some(FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1)
        || u64::try_from(WIDE_PARAMETER_ELEMENT_COUNT_V1).ok()
            != Some(FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1)
        || W_MODEL_CONFIG_FINGERPRINT_V1 != FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1
        || W_MODEL_ARCHITECTURE_VERSION_V1 != FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1
        || WIDE_RUST_LOADER_IDENTITY_V1 != FROZEN_WIDE_SNAPSHOT_LOADER_IDENTITY_V1
        || u64::try_from(W_PARAMETER_COUNT_V1).ok() != Some(FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1)
        || W_ARCHITECTURE_LABEL_V1 != FROZEN_WIDE_DIAGNOSTIC_LABEL_V1
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

/// Capacity-experiment wide-net sibling of [`validate_frozen_snapshot_v1`]:
/// validates `model_snapshot` against the frozen WIDE mirrors instead of the
/// frozen Net8 ones, fail-closed both directions (this branch rejects every
/// frozen-Net8 literal; the frozen branch above rejects every wide one). The
/// wide record section's own fields are validated in lockstep with the
/// snapshot, so a record cannot carry mismatched `model_snapshot` /
/// `contracts.wide_model_experiment_v1` data.
fn validate_wide_snapshot_v1(
    snapshot: &CommonModelSnapshotRecordV1,
    wide: &WideModelExperimentContractV1,
) -> Result<()> {
    validate_frozen_wide_rev1_authorities_v1()?;
    if snapshot.schema != FROZEN_WIDE_SNAPSHOT_SCHEMA_V1
        || snapshot.identity != FROZEN_WIDE_SNAPSHOT_IDENTITY_V1
        || snapshot.snapshot_sha256 != FROZEN_WIDE_SNAPSHOT_SHA256_V1
        || snapshot.manifest_file_sha256 != FROZEN_WIDE_SNAPSHOT_MANIFEST_FILE_SHA256_V1
        || snapshot.manifest_core_sha256 != FROZEN_WIDE_SNAPSHOT_MANIFEST_CORE_SHA256_V1
        || snapshot.payload_sha256 != FROZEN_WIDE_SNAPSHOT_PAYLOAD_SHA256_V1
        || snapshot.payload_byte_count != FROZEN_WIDE_SNAPSHOT_PAYLOAD_BYTE_COUNT_V1
        || snapshot.parameter_layout_sha256 != FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1
        || snapshot.named_parameter_stream_sha256
            != FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
        || snapshot.loaded_named_parameter_stream_sha256
            != FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
        || snapshot.parameter_tensor_count != FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1
        || snapshot.parameter_element_count != FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1
        || snapshot.model_config_fingerprint != FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1
        || snapshot.model_architecture_version != FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1
        || snapshot.feature_contract_digest != FROZEN_FEATURE_CONTRACT_DIGEST_V2
        || snapshot.feature_encoding_digest != FROZEN_FEATURE_ENCODING_DIGEST_V2
        || snapshot.initializer_identity != FROZEN_INITIALIZER_IDENTITY_V2
        || snapshot.base_seed != FROZEN_BASE_SEED_V2
        || snapshot.model_init_seed != FROZEN_MODEL_INIT_SEED_V2
        || snapshot.trainer_schedule_version != FROZEN_TRAINER_SCHEDULE_IDENTITY_V2
        || snapshot.python_reference_seed_version != FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2
        || snapshot.schedule_goldens_sha256 != FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2
        || snapshot.authority_source_bundle_sha256
            != FROZEN_WIDE_SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V1
        || snapshot.authority_runtime_identity != FROZEN_SNAPSHOT_AUTHORITY_RUNTIME_IDENTITY_V2
        || snapshot.loader_identity != FROZEN_WIDE_SNAPSHOT_LOADER_IDENTITY_V1
        || snapshot.optimizer_identity != FROZEN_OPTIMIZER_IDENTITY_V2
        || snapshot.adam_step_initial != FROZEN_ADAM_STEP_INITIAL_V2
        || snapshot.moment_initialization != FROZEN_MOMENT_INITIALIZATION_V2
        || snapshot.canonical_gauge_parameters != FROZEN_CANONICAL_GAUGE_PARAMETERS_V2
        || snapshot.scorer_bias_anchor_f32_bits != FROZEN_WIDE_SCORER_BIAS_ANCHOR_F32_BITS_V1
        || !snapshot.snapshot_load_completed_before_trial_start
        || snapshot.snapshot_load_timed
        || snapshot.rust_seeded_initializer_reproduced
        || snapshot.nonclaim != FROZEN_WIDE_SNAPSHOT_NONCLAIM_V1
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
    if wide.architecture_identity != FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1
        || wide.config_fingerprint != FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1
        || wide.snapshot_sha256 != FROZEN_WIDE_SNAPSHOT_SHA256_V1
        || wide.manifest_core_sha256 != FROZEN_WIDE_SNAPSHOT_MANIFEST_CORE_SHA256_V1
        || wide.payload_sha256 != FROZEN_WIDE_SNAPSHOT_PAYLOAD_SHA256_V1
        || wide.parameter_layout_sha256 != FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1
        || wide.named_parameter_stream_sha256
            != FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
        || wide.parameter_tensor_count != FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1
        || wide.parameter_element_count != FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1
        || wide.diagnostic_label != FROZEN_WIDE_DIAGNOSTIC_LABEL_V1
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_contracts_v2(contracts: &TrainRunContractsV2) -> Result<()> {
    if contracts.trainer_identity != FROZEN_TRAINER_IDENTITY_V2
        || contracts.identity_bundle_identity != IDENTITY_BUNDLE_IDENTITY_V2
        || !is_sha256(&contracts.identity_bundle_sha256)
        || contracts.tensorizer.identity != FROZEN_TENSORIZER_IDENTITY_V2
        || contracts.tensorizer.feature_contract_digest != FROZEN_FEATURE_CONTRACT_DIGEST_V2
        || contracts.tensorizer.feature_encoding_digest != FROZEN_FEATURE_ENCODING_DIGEST_V2
        // Feature-Encoder Successor (collab CLAUDE #221): accepts either the
        // HISTORICAL or CURRENT features.py identity; see
        // `matches_frozen_tensorizer_authority_source_sha256_v1`.
        || !matches_frozen_tensorizer_authority_source_sha256_v1(
            &contracts.tensorizer.authoritative_features_source_sha256,
        )
        || contracts.tensorizer.fixture_sha256 != FROZEN_TENSORIZER_FIXTURE_SHA256_V2
        || contracts.tensorizer.fixture_payload_sha256
            != FROZEN_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2
        || contracts.loss.identity != FROZEN_LOSS_IDENTITY_V2
        || contracts.train_step.identity != FROZEN_TRAIN_STEP_IDENTITY_V2
        || !matches!(
            contracts.train_step.numerical_backend_identity.as_str(),
            FROZEN_NUMERICAL_BACKEND_IDENTITY_V2
                | crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1
        )
        || contracts.optimizer.identity != FROZEN_OPTIMIZER_IDENTITY_V2
        || contracts.optimizer.gauge_identity != FROZEN_OPTIMIZER_IDENTITY_V2
        || contracts.optimizer.gauge_evidence_identity != FROZEN_GAUGE_EVIDENCE_IDENTITY_V2
        || contracts.optimizer.canonical_gauge_parameters
            != FROZEN_CANONICAL_GAUGE_PARAMETERS_V2.map(str::to_owned)
        || contracts.trainer_schedule.identity != FROZEN_TRAINER_SCHEDULE_IDENTITY_V2
        || contracts.trainer_schedule.python_reference_seed_identity
            != FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2
        || contracts
            .trainer_schedule
            .environment_seed_derivation_identity
            != FROZEN_ENVIRONMENT_SEED_DERIVATION_IDENTITY_V2
        || contracts
            .trainer_schedule
            .learner_action_seed_derivation_identity
            != FROZEN_LEARNER_ACTION_SEED_DERIVATION_IDENTITY_V2
        || contracts
            .trainer_schedule
            .opponent_action_seed_derivation_identity
            != FROZEN_OPPONENT_ACTION_SEED_DERIVATION_IDENTITY_V2
        || contracts.trainer_schedule.goldens_sha256 != FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2
        || contracts.learner_sampler.identity != FROZEN_LEARNER_SAMPLER_IDENTITY_V2
        || contracts.learner_sampler.contract_sha256 != FROZEN_LEARNER_SAMPLER_CONTRACT_SHA256_V2
        || contracts.learner_sampler.exp_table_sha256 != FROZEN_LEARNER_SAMPLER_EXP_TABLE_SHA256_V2
        || contracts.learner_sampler.cross_language_vectors_file_sha256
            != FROZEN_LEARNER_VECTORS_FILE_SHA256_V2
        || contracts
            .learner_sampler
            .cross_language_vector_stream_sha256
            != FROZEN_LEARNER_VECTOR_STREAM_SHA256_V2
        || contracts.opponent_sampler.identity != FROZEN_OPPONENT_SAMPLER_IDENTITY_V2
        || contracts.opponent_sampler.algorithm != FROZEN_OPPONENT_SAMPLER_ALGORITHM_V2
        || contracts.opponent_sampler.seed_derivation_identity
            != FROZEN_OPPONENT_ACTION_SEED_DERIVATION_IDENTITY_V2
        || contracts.opponent_sampler.seed_derivation_identity
            != contracts
                .trainer_schedule
                .opponent_action_seed_derivation_identity
        || contracts.opponent_sampler.seed_goldens_sha256
            != FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2
        || contracts
            .opponent_sampler
            .cross_language_vectors_file_sha256
            != FROZEN_OPPONENT_VECTORS_FILE_SHA256_V2
        || contracts
            .opponent_sampler
            .cross_language_vector_stream_sha256
            != FROZEN_OPPONENT_VECTOR_STREAM_SHA256_V2
        || !contracts.opponent_sampler.width_one_consumes_seed
        // The six record trajectory pins are deliberately NOT checked here any
        // more. They are one half of a closed tuple whose other half is the
        // environment randomization section and the protocol/schema pair, so
        // checking them unconditionally would hard-reject every V2 record
        // before the classifier could see it. They are now validated as a
        // complete tuple by `classify_environment_trajectory_contract_v1`.
        || contracts.standalone_semantics.identity != STANDALONE_SEMANTICS_IDENTITY_V2
        || contracts.standalone_semantics.core.identity != STANDALONE_SEMANTICS_IDENTITY_V2
        || !is_sha256(&contracts.standalone_semantics.sha256)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    validate_model_contract_v2(
        &contracts.model,
        contracts.wide_model_experiment_v1.as_ref(),
    )?;
    validate_opponent_policy_and_ladder_pool_v2(contracts)?;
    Ok(())
}

/// The second, independent coupling on `contracts.model`
/// (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3): re-checks
/// architecture/config/layout/counts against the frozen constants exactly
/// like [`validate_wide_snapshot_v1`]/[`validate_frozen_snapshot_v1`] do for
/// `model_snapshot`, just on the `contracts.model` copy of the same
/// identity. Absent wide section: frozen Net8 literals, byte-for-byte as
/// before this function existed. Present: the frozen WIDE mirrors instead.
/// Fail-closed both directions.
fn validate_model_contract_v2(
    model: &ModelContractV2,
    wide: Option<&WideModelExperimentContractV1>,
) -> Result<()> {
    let (architecture, fingerprint, layout, tensor_count, element_count) = match wide {
        None => (
            FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2,
            FROZEN_MODEL_CONFIG_FINGERPRINT_V2,
            FROZEN_PARAMETER_LAYOUT_SHA256_V2,
            FROZEN_PARAMETER_TENSOR_COUNT_V2,
            FROZEN_PARAMETER_ELEMENT_COUNT_V2,
        ),
        Some(_) => (
            FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1,
            FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1,
            FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1,
            FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1,
            FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1,
        ),
    };
    if model.architecture_identity != architecture
        || model.config_fingerprint != fingerprint
        || model.parameter_layout_sha256 != layout
        || model.parameter_tensor_count != tensor_count
        || model.parameter_element_count != element_count
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

/// Lockstep validator for the opponent-policy/ladder-pool pair (contract
/// Section 2/3). A record carrying the uniform identity validates exactly as
/// before and MUST NOT carry a ladder pool section (fail-closed). A record
/// carrying the ladder identity MUST carry a structurally valid ladder pool
/// section (fail-closed). Any other `opponent_policy.identity` is rejected.
fn validate_opponent_policy_and_ladder_pool_v2(contracts: &TrainRunContractsV2) -> Result<()> {
    match contracts.opponent_policy.identity.as_str() {
        FROZEN_OPPONENT_POLICY_IDENTITY_V2 => {
            if contracts.opponent_policy.model_rule != FROZEN_OPPONENT_POLICY_MODEL_RULE_V2
                || contracts.opponent_ladder_pool.is_some()
                || contracts.opponent_ladder_initialization.is_some()
                || contracts.opponent_schedule_v2.is_some()
            {
                return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
            }
            Ok(())
        }
        FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2 => {
            if contracts.opponent_policy.model_rule != FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2 {
                return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
            }
            match &contracts.opponent_ladder_pool {
                Some(pool) => validate_opponent_ladder_pool_v2(pool)?,
                None => return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral)),
            }
            // Amendment 1 / Section 8A point 2: MAY be present or absent
            // under the ladder identity (absent = fresh init, the pilot's
            // historical shape); shape-validate like the pool refs when
            // present, but presence itself is never required.
            if let Some(init) = &contracts.opponent_ladder_initialization {
                validate_opponent_ladder_initialization_v1(init)?;
            }
            match &contracts.opponent_schedule_v2 {
                Some(schedule) => validate_opponent_schedule_v2(schedule),
                None => Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral)),
            }
        }
        _ => Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral)),
    }
}

fn validate_opponent_ladder_pool_v2(pool: &OpponentLadderPoolContractV1) -> Result<()> {
    if pool.identity != FROZEN_LADDER_POOL_IDENTITY_V2
        || pool.size != FROZEN_LADDER_POOL_SIZE_V2
        || pool.policy_member_sampling_rule != FROZEN_LADDER_POLICY_SAMPLING_RULE_V2
        || pool.weight_primary != FROZEN_LADDER_POOL_WEIGHT_PRIMARY_V2
        || pool.weight_predecessor_a != FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2
        || pool.weight_predecessor_b != FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2
        || pool.weight_uniform_floor != FROZEN_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2
        || pool.uniform_floor.identity != FROZEN_OPPONENT_POLICY_IDENTITY_V2
        || pool.uniform_floor.model_rule != FROZEN_OPPONENT_POLICY_MODEL_RULE_V2
        || pool.uniform_floor.sampler_identity != FROZEN_OPPONENT_SAMPLER_IDENTITY_V2
        || pool.uniform_floor.sampler_algorithm != FROZEN_OPPONENT_SAMPLER_ALGORITHM_V2
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    for entry in [&pool.primary, &pool.predecessor_a, &pool.predecessor_b] {
        if !is_sha256(&entry.source_run_sha256)
            || !is_u63(entry.generation)
            || !is_sha256(&entry.checkpoint_sha256)
            || !is_sha256(&entry.sidecar_sha256)
            || !is_sha256(&entry.state_sha256)
        {
            return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
        }
    }
    Ok(())
}

/// Shape validator for the continual-initialization checkpoint reference
/// (Amendment 1 / Section 8A point 2): no frozen literal to compare against
/// (unlike the pool's fixed weights/identities, this section names a
/// caller-chosen source checkpoint), so this mirrors exactly the pool refs'
/// per-entry shape gate in [`validate_opponent_ladder_pool_v2`].
fn validate_opponent_ladder_initialization_v1(
    init: &OpponentLadderInitializationContractV1,
) -> Result<()> {
    if !is_sha256(&init.source_run_sha256)
        || !is_u63(init.generation)
        || !is_sha256(&init.checkpoint_sha256)
        || !is_sha256(&init.sidecar_sha256)
        || !is_sha256(&init.state_sha256)
        || !is_sha256(&init.derived_model_parameter_sha256)
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidScalar));
    }
    Ok(())
}

fn validate_opponent_schedule_v2(schedule: &OpponentScheduleV2ContractV1) -> Result<()> {
    if schedule.schedule_version != FROZEN_LADDER_SCHEDULE_VERSION_V2
        || schedule.seed_version != FROZEN_LADDER_SCHEDULE_SEED_VERSION_V2
        || schedule.opponent_pool_choice_namespace
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_NAMESPACE_V2
        || schedule.opponent_pool_choice_fields != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_FIELDS_V2
        || schedule.opponent_policy_substep_namespace
            != FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_NAMESPACE_V2
        || schedule.opponent_policy_substep_fields
            != FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_FIELDS_V2
        || schedule.pool_choice_modulo != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_MODULO_V2
        || schedule.pool_choice_threshold_rule
            != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_THRESHOLD_RULE_V2
        || schedule.pool_choice_bias_rule != FROZEN_LADDER_SCHEDULE_POOL_CHOICE_BIAS_RULE_V2
        || schedule.version_change_rule != FROZEN_LADDER_SCHEDULE_VERSION_CHANGE_RULE_V2
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }
    Ok(())
}

fn validate_population_program_v1(record: &TrainRunV2) -> Result<()> {
    let Some(program) = record.contracts.population_program_v1.as_ref() else {
        return Ok(());
    };

    if program.identity != POPULATION_PROGRAM_IDENTITY_V1
        || program.package_commit != POPULATION_PACKAGE_COMMIT_V1
        || program.program_document_sha256 != POPULATION_PROGRAM_DOCUMENT_SHA256_V1
        || program.retest_manifest_sha256 != POPULATION_RETEST_MANIFEST_SHA256_V1
        || program.replay_end_generation != POPULATION_REPLAY_END_GENERATION_V1
        || program.program_update_count != POPULATION_PROGRAM_UPDATE_COUNT_V1
        || program.refresh_interval != POPULATION_REFRESH_INTERVAL_V1
        || program.slot_count != POPULATION_SLOT_COUNT_V1
        || program.reward_identity != POPULATION_REWARD_IDENTITY_V1
        || program.refresh_manifest_identity != POPULATION_REFRESH_MANIFEST_IDENTITY_V1
        || program.retest_beta_f32_bits != POPULATION_RETEST_BETA_F32_BITS_V1
        || program.pool_identity != POPULATION_POOL_IDENTITY_V1
        || program.pool_document_sha256 != POPULATION_POOL_DOCUMENT_SHA256_V1
        || program.parent_source_run_sha256 != POPULATION_PARENT_SOURCE_RUN_SHA256_V1
        || program.parent_generation != POPULATION_PARENT_GENERATION_V1
        || program.parent_checkpoint_sha256 != POPULATION_PARENT_CHECKPOINT_SHA256_V1
        || program.parent_sidecar_sha256 != POPULATION_PARENT_SIDECAR_SHA256_V1
        || program.parent_state_sha256 != POPULATION_PARENT_STATE_SHA256_V1
        || program.parent_model_parameter_sha256 != POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1
        || record.contracts.wide_model_experiment_v1.is_some()
        || record.contracts.response_exploiter_v1.is_some()
        || record.environment.environment_randomization_v2.is_none()
        || record.contracts.opponent_policy.identity != FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2
        || record.contracts.opponent_ladder_pool.is_none()
        || record
            .contracts
            .opponent_ladder_initialization
            .as_ref()
            .is_none_or(|initialization| {
                initialization.source_run_sha256 != POPULATION_PARENT_SOURCE_RUN_SHA256_V1
                    || initialization.generation != POPULATION_PARENT_GENERATION_V1
                    || initialization.checkpoint_sha256
                        != POPULATION_PARENT_CHECKPOINT_SHA256_V1
                    || initialization.sidecar_sha256 != POPULATION_PARENT_SIDECAR_SHA256_V1
                    || initialization.state_sha256 != POPULATION_PARENT_STATE_SHA256_V1
                    || initialization.derived_model_parameter_sha256
                        != POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1
            })
        || !POPULATION_EXPECTED_BASE_SEEDS_V1.contains(&program.expected_base_seed)
        || program.expected_base_seed != record.schedule.base_seed
        || record.schedule.batch_episodes != 64
        || record.schedule.checkpoint_segment_updates != 4
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    }

    let requested_successful_updates = program
        .replay_end_generation
        .checked_add(program.program_update_count)
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))?;
    if requested_successful_updates != 1_536
        || record.schedule.requested_successful_updates != requested_successful_updates
        || !program
            .source_lineages
            .iter()
            .zip(POPULATION_SOURCE_LINEAGES_V1)
            .all(|(actual, expected)| {
                actual.base_seed == expected.0
                    && actual.store_tree_sha256 == expected.1
                    && actual.run_sha256 == expected.2
                    && actual.checkpoint_sha256 == expected.3
                    && actual.sidecar_sha256 == expected.4
                    && actual.state_sha256 == expected.5
                    && actual.model_parameter_sha256 == expected.6
            })
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::CrossBinding));
    }
    Ok(())
}

fn validate_response_exploiter_v1(record: &TrainRunV2) -> Result<()> {
    let Some(response) = record.contracts.response_exploiter_v1.as_ref() else {
        return Ok(());
    };

    let effective_weight_total = response
        .effective_weight_units
        .iter()
        .try_fold(0_u64, |sum, weight| sum.checked_add(*weight))
        .ok_or_else(|| TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidArithmetic))?;
    let initialization = record
        .contracts
        .opponent_ladder_initialization
        .as_ref();
    // Third tuple element: the role's own training-update-count/schedule-
    // length pin. For "build"/"screen"/"denovo-screen" this always coincides
    // with the shared RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1 (256),
    // independent of "screen"'s shorter completion generation (4) -- "screen"
    // is still a 256-update-scheduled run read early, not a shorter schedule.
    // "denovo-screen-512" (Phase 2 horizon amendment) is a genuinely longer
    // schedule, so its training-update-count, completion generation, and
    // `record.schedule.requested_successful_updates` all move together to
    // its own dedicated 512 constant.
    let expected_role_and_completion = if RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1
        .contains(&response.expected_base_seed)
    {
        (
            "build",
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
        )
    } else if RESPONSE_EXPLOITER_AUTHORIZED_SCREEN_SEEDS_V1
        .contains(&response.expected_base_seed)
    {
        (
            "screen",
            RESPONSE_EXPLOITER_SCREEN_COMPLETION_GENERATION_V1,
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
        )
    } else if RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_SEEDS_V1
        .contains(&response.expected_base_seed)
    {
        (
            "denovo-screen",
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
        )
    } else if RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1
        .contains(&response.expected_base_seed)
    {
        (
            "denovo-screen-512",
            RESPONSE_EXPLOITER_DENOVO_512_TRAINING_UPDATE_COUNT_V1,
            RESPONSE_EXPLOITER_DENOVO_512_TRAINING_UPDATE_COUNT_V1,
        )
    } else {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
    };
    let is_denovo = response.run_role == "denovo-screen" || response.run_role == "denovo-screen-512";

    // Backward-compatibility amendment: unlike `authorized_base_seeds` /
    // `authorized_screen_seeds` / `authorized_denovo_seeds` above (always
    // present, always checked unconditionally), this array may be entirely
    // absent -- a record written before the Phase 2 512-horizon amendment
    // introduced it, of ANY role (the real denovo-screen-256 store's
    // run.json among them). Absence is accepted for every role except
    // "denovo-screen-512" itself, which cannot have a pre-amendment shape
    // (the role did not exist before the amendment that added this field).
    // Presence is still checked exactly, for every role: a record that
    // does carry the field but with wrong content is rejected, the same
    // fail-closed treatment every other literal in this contract gets.
    let authorized_denovo_512_seeds_invalid = match response.authorized_denovo_512_seeds {
        Some(seeds) => seeds != RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1,
        None => expected_role_and_completion.0 == "denovo-screen-512",
    };

    // Parent lineage and warm-start initialization are role-conditional, not
    // sentinel-filled: "build"/"screen" always bind the exact promoted(2)
    // gen-384 identity on both this contract's parent_* fields and the
    // record's own `opponent_ladder_initialization` section; "denovo-screen"
    // requires both to be entirely absent, since there is no parent for a
    // fresh-init run. Never a partial match either direction.
    let parent_and_initialization_invalid = if is_denovo {
        response.parent_source_run_sha256.is_some()
            || response.parent_generation.is_some()
            || response.parent_checkpoint_sha256.is_some()
            || response.parent_sidecar_sha256.is_some()
            || response.parent_state_sha256.is_some()
            || response.parent_model_parameter_sha256.is_some()
            || response.fresh_adam_after_weight_init_identity
                != RESPONSE_EXPLOITER_DENOVO_FRESH_ADAM_AFTER_WEIGHT_INIT_IDENTITY_V1
            || initialization.is_some()
    } else {
        response.parent_source_run_sha256.as_deref() != Some(POPULATION_PARENT_SOURCE_RUN_SHA256_V1)
            || response.parent_generation != Some(POPULATION_PARENT_GENERATION_V1)
            || response.parent_checkpoint_sha256.as_deref()
                != Some(POPULATION_PARENT_CHECKPOINT_SHA256_V1)
            || response.parent_sidecar_sha256.as_deref()
                != Some(POPULATION_PARENT_SIDECAR_SHA256_V1)
            || response.parent_state_sha256.as_deref() != Some(POPULATION_PARENT_STATE_SHA256_V1)
            || response.parent_model_parameter_sha256.as_deref()
                != Some(POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1)
            || response.fresh_adam_after_weight_init_identity
                != RESPONSE_EXPLOITER_FRESH_ADAM_AFTER_WEIGHT_INIT_IDENTITY_V1
            || initialization.is_none_or(|initialization| {
                initialization.source_run_sha256 != POPULATION_PARENT_SOURCE_RUN_SHA256_V1
                    || initialization.generation != POPULATION_PARENT_GENERATION_V1
                    || initialization.checkpoint_sha256 != POPULATION_PARENT_CHECKPOINT_SHA256_V1
                    || initialization.sidecar_sha256 != POPULATION_PARENT_SIDECAR_SHA256_V1
                    || initialization.state_sha256 != POPULATION_PARENT_STATE_SHA256_V1
                    || initialization.derived_model_parameter_sha256
                        != POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1
            })
    };

    if response.identity != RESPONSE_EXPLOITER_IDENTITY_V1
        || response.package_commit != POPULATION_PACKAGE_COMMIT_V1
        || response.program_document_sha256 != POPULATION_PROGRAM_DOCUMENT_SHA256_V1
        || response.target_refresh_manifest_sha256
            != RESPONSE_EXPLOITER_TARGET_REFRESH_SHA256_V1
        || response.target_global_generation != RESPONSE_EXPLOITER_TARGET_GLOBAL_GENERATION_V1
        || response.source_refresh_index != RESPONSE_EXPLOITER_SOURCE_REFRESH_INDEX_V1
        || response.source_program_update != RESPONSE_EXPLOITER_SOURCE_PROGRAM_UPDATE_V1
        || response.active_slot_indices != RESPONSE_EXPLOITER_ACTIVE_SLOT_INDICES_V1
        || response.excluded_slot_indices != RESPONSE_EXPLOITER_EXCLUDED_SLOT_INDICES_V1
        || response.renormalization_identity
            != RESPONSE_EXPLOITER_RENORMALIZATION_IDENTITY_V1
        || response.effective_weight_units != RESPONSE_EXPLOITER_EFFECTIVE_WEIGHT_UNITS_V1
        || response.effective_weight_total_units
            != RESPONSE_EXPLOITER_EFFECTIVE_WEIGHT_TOTAL_UNITS_V1
        || effective_weight_total != response.effective_weight_total_units
        || response.training_update_count != expected_role_and_completion.2
        || response.episodes_per_update != RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1
        || response.reward_identity != POPULATION_REWARD_IDENTITY_V1
        || response.authorized_base_seeds != RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1
        || response.authorized_screen_seeds != RESPONSE_EXPLOITER_AUTHORIZED_SCREEN_SEEDS_V1
        || response.authorized_denovo_seeds != RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_SEEDS_V1
        || authorized_denovo_512_seeds_invalid
        || response.expected_base_seed != record.schedule.base_seed
        || response.run_role != expected_role_and_completion.0
        || response.expected_completion_generation != expected_role_and_completion.1
        || !matches!(
            response.policy_anchor_beta_f32_bits.as_str(),
            RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1
                | RESPONSE_EXPLOITER_RETRY_BETA_F32_BITS_V1
                | RESPONSE_EXPLOITER_DENOVO_BETA_F32_BITS_V1
        )
        || (response.run_role == "screen"
            && response.policy_anchor_beta_f32_bits
                != RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1)
        || (is_denovo
            && response.policy_anchor_beta_f32_bits != RESPONSE_EXPLOITER_DENOVO_BETA_F32_BITS_V1)
        || (!is_denovo
            && response.policy_anchor_beta_f32_bits == RESPONSE_EXPLOITER_DENOVO_BETA_F32_BITS_V1)
        || parent_and_initialization_invalid
        || record.contracts.population_program_v1.is_some()
        || record.contracts.wide_model_experiment_v1.is_some()
        || record.environment.environment_randomization_v2.is_none()
        || record.contracts.opponent_policy.identity
            != FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2
        || record.contracts.opponent_ladder_pool.is_none()
        || record.contracts.opponent_schedule_v2.is_none()
        || record.schedule.batch_episodes != RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1
        || record.schedule.checkpoint_segment_updates
            != RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1
        || record.schedule.checkpoint_episode_interval
            != RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1
                * RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1
        || record.schedule.requested_successful_updates != expected_role_and_completion.2
    {
        return Err(TrainRunV2Error::new(TrainRunV2ErrorKind::InvalidLiteral));
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
pub(crate) fn test_fixture_bytes_v2() -> Vec<u8> {
    tests::fixture_bytes()
}

/// The HISTORICAL-profile sibling of [`test_fixture_bytes_v2`]: a coherent
/// record carrying the frozen rev3 catalog literals instead of the live
/// nine-deck ones. Test-only: used by the dual-profile decode-acceptance and
/// boundary-rejection suites (this module and its consumers) to exercise a
/// record that decodes clean but must be rejected at the science-loop,
/// publisher, and resume boundaries.
#[cfg(test)]
pub(crate) fn test_fixture_bytes_historical_v1() -> Vec<u8> {
    tests::fixture_bytes_historical()
}

/// A coherent, fully reminted environment randomization V2 record. Test-only:
/// the diagonal and genuine-execution suites use it as the validated V2 run
/// authority.
#[cfg(test)]
pub(crate) fn test_fixture_bytes_environment_randomization_v2() -> Vec<u8> {
    tests::coherent_v2_bytes()
}

/// Backend-parametrized fixture: the matched runtime tuple and train-step
/// backend identity pair for the requested store-admitted backend.
#[cfg(test)]
pub(crate) fn test_fixture_bytes_with_backend_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_backend(backend)
}

#[cfg(test)]
pub(crate) fn test_fixture_bytes_with_base_seed_v2(base_seed: u64) -> Vec<u8> {
    tests::fixture_bytes_with_base_seed(base_seed)
}

/// Parametrized schedule/topology fixture for K-scaling and sizing tests,
/// declaring the matched runtime pair for the requested backend.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
#[cfg_attr(
    not(all(windows, feature = "experimental-burn-net8-packed-cuda-v1")),
    allow(dead_code)
)]
pub(crate) fn test_fixture_bytes_with_schedule_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
    )
}

/// Combined schedule/topology plus held-out base-seed fixture: builds one
/// record, applies the backend pair and schedule/topology fields, sets the
/// base seed BEFORE derived refresh, and canonical-encodes, producing an
/// honestly re-digested run rather than a JSON patch of derived fields.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
#[cfg_attr(
    not(all(windows, feature = "experimental-burn-net8-packed-cuda-v1")),
    allow(dead_code)
)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
    )
}

/// Environment-randomization-V2 variant of
/// [`test_fixture_bytes_with_schedule_and_base_seed_v2`]. The schedule,
/// topology, backend, and seed are unchanged; only the complete sealed V2
/// trajectory tuple is installed before all derived digests are reminted.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
#[cfg_attr(
    not(all(windows, feature = "experimental-burn-net8-packed-cuda-v1")),
    allow(dead_code)
)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
    )
}

/// Ladder variant of [`test_fixture_bytes_with_schedule_and_base_seed_v2`]
/// (Self-Play Ladder Design Contract S2, pilot runner integration): the
/// SAME schedule/topology/base-seed fields, but the run record carries the
/// ladder opponent identity plus the caller-supplied `pool` and the frozen
/// `opponent_schedule_v2` section, instead of the uniform identity. Kept as
/// a genuinely separate function (not a flag on the existing one) so the
/// uniform fixture's bytes stay byte-identical by construction: nothing
/// about `test_fixture_bytes_with_schedule_and_base_seed_v2`'s own body
/// changed to add this.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_ladder(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
    )
}

/// Environment-randomization-V2 composition of
/// [`test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`]. This is the
/// fresh-init self-play shape used by a V2 ladder run.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
    )
}

/// Population-program RunV2 authority used by the scaled self-play replay
/// and every later 128-update population segment. The complete ladder plus
/// environment-v2 record remains present because generations 0 through 512
/// replay that exact recipe; this function adds the separately frozen
/// population-program authority before reminting the run digests.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_population_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
    initialization: OpponentLadderInitializationContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_population_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
        initialization,
    )
}

/// Exact program-update-1024 response-exploiter RunV2 authority. It composes
/// the existing ladder-init plus environment-v2 carrier, then adds only the
/// response contract before reminting the derived digests.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
    initialization: OpponentLadderInitializationContractV1,
    policy_anchor_beta_f32_bits: &str,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
        initialization,
        policy_anchor_beta_f32_bits,
    )
}

/// De-novo sibling of
/// [`test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2`]:
/// same ladder-pool/mixture/schedule binding, but takes no
/// `OpponentLadderInitializationContractV1` at all (there is no parent to
/// pin) and stamps the "denovo-screen" role/beta=0 contract instead. Built
/// on the existing no-init ladder+envrand-v2 fixture
/// (`fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2`, the
/// same builder the generic `(Some(pool), None, true)` multirun-harness
/// dispatch arm already uses), not the init variant.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_denovo_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_response_exploiter_denovo_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
    )
}

/// Continual-initialization variant of
/// [`test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`] (Self-Play
/// Ladder Design Contract S2, Amendment 1 / Section 8A point 2 pilot harness
/// wiring): the SAME ladder fixture, plus the caller-supplied
/// `initialization` section wired into
/// `contracts.opponent_ladder_initialization`. Kept as a genuinely separate
/// function (not a flag on the existing ladder builder) for the same reason
/// the ladder builder is separate from the uniform one: the fresh-init
/// ladder fixture's bytes stay byte-identical by construction.
///
/// Currently unused outside this module's own round-trip/corruption tests
/// (`native_training_store_run_v2::tests`), which exercise the schema
/// directly against `TrainRunV2` rather than through this wire-bytes
/// builder: `run_native_science_loop_v1`'s genesis path cannot yet consume
/// an init-bearing record (Deliverable 3's STOP finding -- generation 0 is
/// structurally bound to the frozen common model snapshot, see
/// `native_training_store_checkpoint_v3::tests::genesis_authoring_rejects_a_real_trained_payload_structurally`).
/// Retained ready for whenever that store-contract question is resolved.
#[cfg(test)]
#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
    initialization: OpponentLadderInitializationContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_ladder_init(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
        initialization,
    )
}

/// Environment-randomization-V2 composition of
/// [`test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2`]. This is
/// the exact continual-init run-record shape for the macro self-play rung.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
    initialization: OpponentLadderInitializationContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
        initialization,
    )
}

/// Capacity-experiment wide-net variant of
/// [`test_fixture_bytes_with_schedule_and_base_seed_v2`]
/// (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3): the SAME
/// schedule/topology/base-seed fields, but `model_snapshot` and
/// `contracts.model` carry the wide (`kernel-policy-value-net-8w128`)
/// literals and `contracts.wide_model_experiment_v1` is populated. Kept as a
/// genuinely separate function (not a flag on the existing one) so the
/// frozen fixture's bytes stay byte-identical by construction.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_wide_environment_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_wide_environment_v2(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_wide_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_wide(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
    )
}

/// Combined wide-net + ladder-opponent variant (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md
/// Section 4: the wide run trains against the ladder pool, "pool2 pinned BY
/// CHECKPOINT REFERENCE"): the SAME wide stamping as
/// [`test_fixture_bytes_with_schedule_and_base_seed_wide_v2`], plus the
/// ladder opponent identity and caller-supplied pool, mirroring
/// [`test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`]'s shape.
/// Fresh-init only (no continual-init section): the wide protocol trains
/// fresh from the new authority snapshot exclusively.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2(
    backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    worker_count: u64,
    sessions_per_worker: u64,
    broker_batch_target: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    base_seed: u64,
    pool: OpponentLadderPoolContractV1,
) -> Vec<u8> {
    tests::fixture_bytes_with_schedule_and_base_seed_wide_ladder(
        backend,
        batch_episodes,
        checkpoint_segment_updates,
        requested_successful_updates,
        worker_count,
        sessions_per_worker,
        broker_batch_target,
        max_physical_decisions,
        max_policy_steps,
        base_seed,
        pool,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

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
                environment_randomization_v2: None,
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
                "source_tree_recipe_identity": FROZEN_SOURCE_TREE_RECIPE_IDENTITY_V2,
                "source_tree_recipe_sha256": FROZEN_SOURCE_TREE_RECIPE_SHA256_V2,
                "source_tree_recipe_byte_count": FROZEN_SOURCE_TREE_RECIPE_BYTE_COUNT_V2,
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
                "numerical_backend_identity": FROZEN_NUMERICAL_BACKEND_IDENTITY_V2,
                "rustc_release": "1.94.1",
                "rustc_commit_hash": "4444444444444444444444444444444444444444",
                "target_triple": "x86_64-pc-windows-msvc",
                "build_profile": "release"
            },
            "environment": {
                // Dual-Profile Catalog Successor (collab CLAUDE #220): the
                // default fixture builds a CURRENT-profile record (live
                // nine-deck catalog identity), matching what production
                // capture actually mints today, so every test built on top
                // of `fixture_record()` exercises the live science-loop/
                // publish/resume paths unrejected. `fixture_record_historical()`
                // below overrides these two fields back to the HISTORICAL
                // (rev3) literals for the dedicated dual-profile tests.
                "card_db_hash_u64_hex": FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1,
                "runtime_catalog_schema": FROZEN_RUNTIME_CATALOG_SCHEMA_V2,
                "runtime_catalog_protocol": FROZEN_RUNTIME_CATALOG_PROTOCOL_V2,
                "runtime_catalog_sha256": FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1,
                "deck_ids": [FROZEN_RALLY_DECK_ID_V2, FROZEN_RALLY_DECK_ID_V2],
                "deck_hashes_u64_hex": [FROZEN_RALLY_DECK_HASH_U64_HEX_V2, FROZEN_RALLY_DECK_HASH_U64_HEX_V2],
                "protocol": FROZEN_PROTOCOL_V2,
                "protocol_version": FROZEN_PROTOCOL_VERSION_V2,
                "schema_version": FROZEN_SCHEMA_VERSION_V2,
                "kernel_version": FROZEN_KERNEL_VERSION_V2,
                "surface_version": FROZEN_SURFACE_VERSION_V2,
                "policy_surface_version": FROZEN_POLICY_SURFACE_VERSION_V2
            },
            "contracts": {
                "trainer_identity": FROZEN_TRAINER_IDENTITY_V2,
                "identity_bundle_identity": IDENTITY_BUNDLE_IDENTITY_V2,
                "identity_bundle_sha256": ZERO_SHA256,
                "tensorizer": {
                    "identity": FROZEN_TENSORIZER_IDENTITY_V2,
                    "feature_contract_digest": FROZEN_FEATURE_CONTRACT_DIGEST_V2,
                    "feature_encoding_digest": FROZEN_FEATURE_ENCODING_DIGEST_V2,
                    "authoritative_features_source_sha256": FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2,
                    "fixture_sha256": FROZEN_TENSORIZER_FIXTURE_SHA256_V2,
                    "fixture_payload_sha256": FROZEN_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2
                },
                "model": {
                    "architecture_identity": FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2,
                    "config_fingerprint": FROZEN_MODEL_CONFIG_FINGERPRINT_V2,
                    "parameter_layout_sha256": FROZEN_PARAMETER_LAYOUT_SHA256_V2,
                    "parameter_tensor_count": FROZEN_PARAMETER_TENSOR_COUNT_V2,
                    "parameter_element_count": FROZEN_PARAMETER_ELEMENT_COUNT_V2
                },
                "loss": {"identity": FROZEN_LOSS_IDENTITY_V2},
                "train_step": {
                    "identity": FROZEN_TRAIN_STEP_IDENTITY_V2,
                    "numerical_backend_identity": FROZEN_NUMERICAL_BACKEND_IDENTITY_V2
                },
                "optimizer": {
                    "identity": FROZEN_OPTIMIZER_IDENTITY_V2,
                    "gauge_identity": FROZEN_OPTIMIZER_IDENTITY_V2,
                    "gauge_evidence_identity": FROZEN_GAUGE_EVIDENCE_IDENTITY_V2,
                    "canonical_gauge_parameters": FROZEN_CANONICAL_GAUGE_PARAMETERS_V2
                },
                "trainer_schedule": {
                    "identity": FROZEN_TRAINER_SCHEDULE_IDENTITY_V2,
                    "python_reference_seed_identity": FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2,
                    "environment_seed_derivation_identity": FROZEN_ENVIRONMENT_SEED_DERIVATION_IDENTITY_V2,
                    "learner_action_seed_derivation_identity": FROZEN_LEARNER_ACTION_SEED_DERIVATION_IDENTITY_V2,
                    "opponent_action_seed_derivation_identity": FROZEN_OPPONENT_ACTION_SEED_DERIVATION_IDENTITY_V2,
                    "goldens_sha256": FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2
                },
                "learner_sampler": {
                    "identity": FROZEN_LEARNER_SAMPLER_IDENTITY_V2,
                    "contract_sha256": FROZEN_LEARNER_SAMPLER_CONTRACT_SHA256_V2,
                    "exp_table_sha256": FROZEN_LEARNER_SAMPLER_EXP_TABLE_SHA256_V2,
                    "cross_language_vectors_file_sha256": FROZEN_LEARNER_VECTORS_FILE_SHA256_V2,
                    "cross_language_vector_stream_sha256": FROZEN_LEARNER_VECTOR_STREAM_SHA256_V2
                },
                "opponent_policy": {
                    "identity": FROZEN_OPPONENT_POLICY_IDENTITY_V2,
                    "model_rule": FROZEN_OPPONENT_POLICY_MODEL_RULE_V2
                },
                "opponent_sampler": {
                    "identity": FROZEN_OPPONENT_SAMPLER_IDENTITY_V2,
                    "algorithm": FROZEN_OPPONENT_SAMPLER_ALGORITHM_V2,
                    "seed_derivation_identity": FROZEN_OPPONENT_ACTION_SEED_DERIVATION_IDENTITY_V2,
                    "seed_goldens_sha256": FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2,
                    "cross_language_vectors_file_sha256": FROZEN_OPPONENT_VECTORS_FILE_SHA256_V2,
                    "cross_language_vector_stream_sha256": FROZEN_OPPONENT_VECTOR_STREAM_SHA256_V2,
                    "width_one_consumes_seed": true
                },
                "trajectory": {
                    "identity": FROZEN_LEGACY_TRAJECTORY_IDENTITY_V1,
                    "cross_language_goldens_schema": FROZEN_LEGACY_TRAJECTORY_GOLDENS_SCHEMA_V1,
                    "cross_language_generator_identity": FROZEN_LEGACY_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1,
                    "cross_language_golden_stream_identity": FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1,
                    "cross_language_goldens_file_sha256": FROZEN_LEGACY_TRAJECTORY_GOLDENS_FILE_SHA256_V1,
                    "cross_language_golden_stream_sha256": FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_SHA256_V1
                },
                "standalone_semantics": {
                    "identity": STANDALONE_SEMANTICS_IDENTITY_V2,
                    "core": empty_semantics_core(),
                    "sha256": ZERO_SHA256
                }
            },
            "model_snapshot": {
                "schema": FROZEN_SNAPSHOT_SCHEMA_V2,
                "identity": FROZEN_SNAPSHOT_IDENTITY_V2,
                "snapshot_sha256": FROZEN_SNAPSHOT_SHA256_V2,
                "manifest_file_sha256": FROZEN_SNAPSHOT_MANIFEST_FILE_SHA256_V2,
                "manifest_core_sha256": FROZEN_SNAPSHOT_MANIFEST_CORE_SHA256_V2,
                "payload_sha256": FROZEN_SNAPSHOT_PAYLOAD_SHA256_V2,
                "payload_byte_count": FROZEN_SNAPSHOT_PAYLOAD_BYTE_COUNT_V2,
                "parameter_layout_sha256": FROZEN_PARAMETER_LAYOUT_SHA256_V2,
                "named_parameter_stream_sha256": FROZEN_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V2,
                "loaded_named_parameter_stream_sha256": FROZEN_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V2,
                "parameter_tensor_count": FROZEN_PARAMETER_TENSOR_COUNT_V2,
                "parameter_element_count": FROZEN_PARAMETER_ELEMENT_COUNT_V2,
                "model_config_fingerprint": FROZEN_MODEL_CONFIG_FINGERPRINT_V2,
                "model_architecture_version": FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2,
                "feature_contract_digest": FROZEN_FEATURE_CONTRACT_DIGEST_V2,
                "feature_encoding_digest": FROZEN_FEATURE_ENCODING_DIGEST_V2,
                "initializer_identity": FROZEN_INITIALIZER_IDENTITY_V2,
                "base_seed": FROZEN_BASE_SEED_V2,
                "model_init_seed": FROZEN_MODEL_INIT_SEED_V2,
                "trainer_schedule_version": FROZEN_TRAINER_SCHEDULE_IDENTITY_V2,
                "python_reference_seed_version": FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2,
                "schedule_goldens_sha256": FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2,
                "authority_source_bundle_sha256": FROZEN_SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V2,
                "authority_runtime_identity": FROZEN_SNAPSHOT_AUTHORITY_RUNTIME_IDENTITY_V2,
                "loader_identity": FROZEN_SNAPSHOT_LOADER_IDENTITY_V2,
                "optimizer_identity": FROZEN_OPTIMIZER_IDENTITY_V2,
                "adam_step_initial": FROZEN_ADAM_STEP_INITIAL_V2,
                "moment_initialization": FROZEN_MOMENT_INITIALIZATION_V2,
                "canonical_gauge_parameters": FROZEN_CANONICAL_GAUGE_PARAMETERS_V2,
                "scorer_bias_anchor_f32_bits": FROZEN_SCORER_BIAS_ANCHOR_F32_BITS_V2,
                "snapshot_load_completed_before_trial_start": true,
                "snapshot_load_timed": false,
                "rust_seeded_initializer_reproduced": false,
                "nonclaim": FROZEN_SNAPSHOT_NONCLAIM_V2
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

    /// The dedicated HISTORICAL-profile sibling of `fixture_record()`: the
    /// exact same coherent record, with the two catalog-content fields
    /// overridden back to the frozen rev3 literals (byte-identical to what
    /// every fixture built before the runtime-decks-nine landing carried).
    /// Used only by the dual-profile decode-acceptance and boundary-rejection
    /// tests; every other test keeps using the CURRENT-profile default.
    pub(super) fn fixture_record_historical() -> TrainRunV2 {
        let mut record = fixture_record();
        record.environment.card_db_hash_u64_hex = FROZEN_CARD_DB_HASH_U64_HEX_V2.to_owned();
        record.environment.runtime_catalog_sha256 = FROZEN_RUNTIME_CATALOG_SHA256_V2.to_owned();
        refresh_derived(&mut record);
        record
    }

    pub(super) fn fixture_bytes_historical() -> Vec<u8> {
        to_canonical_json_bytes_v1(
            &fixture_record_historical(),
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .unwrap()
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

    pub(super) fn fixture_bytes() -> Vec<u8> {
        to_canonical_json_bytes_v1(&fixture_record(), CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    // ------------------------------------------------------------------
    // Live C2: environment randomization V2 manifest classification
    // ------------------------------------------------------------------

    /// The exact manifest section, projected from the production owner
    /// constants so the fixture cannot drift from the validator.
    pub(super) fn exact_environment_randomization_section_v2() -> EnvironmentRandomizationContractV2
    {
        EnvironmentRandomizationContractV2 {
            identity: ENVIRONMENT_RANDOMIZATION_IDENTITY_V2.to_owned(),
            namespace: ENVIRONMENT_RANDOMIZATION_NAMESPACE_V2.to_owned(),
            atom: ENVIRONMENT_RANDOMIZATION_ATOM_FRAMING_V2.to_owned(),
            extraction: ENVIRONMENT_RANDOMIZATION_EXTRACTION_V2.to_owned(),
            ordered_atoms: ENVIRONMENT_RANDOMIZATION_ORDERED_ATOMS_V2
                .iter()
                .map(|row| row.iter().map(|part| (*part).to_string()).collect())
                .collect(),
            owners: [
                ENVIRONMENT_RANDOMIZATION_OWNERS_V2[0].to_owned(),
                ENVIRONMENT_RANDOMIZATION_OWNERS_V2[1].to_owned(),
            ],
            purposes: [
                ENVIRONMENT_RANDOMIZATION_PURPOSES_V2[0].to_owned(),
                ENVIRONMENT_RANDOMIZATION_PURPOSES_V2[1].to_owned(),
            ],
            initial_ordinal_rule: ENVIRONMENT_RANDOMIZATION_INITIAL_ORDINAL_RULE_V2.to_owned(),
            overflow_rule: ENVIRONMENT_RANDOMIZATION_OVERFLOW_RULE_V2.to_owned(),
            shuffle_algorithm: ENVIRONMENT_RANDOMIZATION_SHUFFLE_ALGORITHM_V2.to_owned(),
            cross_language_goldens_schema: ENVIRONMENT_RANDOMIZATION_GOLDENS_SCHEMA_V1.to_owned(),
            cross_language_goldens_file_sha256: ENVIRONMENT_RANDOMIZATION_GOLDENS_SHA256_V1
                .to_owned(),
        }
    }

    fn legacy_trajectory_contract_v1() -> TrajectoryContractV2 {
        TrajectoryContractV2 {
            identity: FROZEN_LEGACY_TRAJECTORY_IDENTITY_V1.to_owned(),
            cross_language_goldens_schema: FROZEN_LEGACY_TRAJECTORY_GOLDENS_SCHEMA_V1.to_owned(),
            cross_language_generator_identity:
                FROZEN_LEGACY_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1.to_owned(),
            cross_language_golden_stream_identity:
                FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1.to_owned(),
            cross_language_goldens_file_sha256: FROZEN_LEGACY_TRAJECTORY_GOLDENS_FILE_SHA256_V1
                .to_owned(),
            cross_language_golden_stream_sha256: FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_SHA256_V1
                .to_owned(),
        }
    }

    fn v2_trajectory_contract_v2() -> TrajectoryContractV2 {
        TrajectoryContractV2 {
            identity: NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V2.to_owned(),
            cross_language_goldens_schema: NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_SCHEMA_V2
                .to_owned(),
            cross_language_generator_identity:
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V2.to_owned(),
            cross_language_golden_stream_identity:
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V2.to_owned(),
            cross_language_goldens_file_sha256:
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDENS_FILE_SHA256_V2.to_owned(),
            cross_language_golden_stream_sha256:
                NATIVE_FULL_EPISODE_TRAJECTORY_GOLDEN_STREAM_SHA256_V2.to_owned(),
        }
    }

    /// A coherent environment randomization V2 record: the complete tuple, with
    /// every derived digest reminted by the existing `refresh_derived` order.
    pub(super) fn coherent_v2_record() -> TrainRunV2 {
        let mut record = fixture_record();
        record.environment.protocol_version = u64::from(RL_SESSION_PROTOCOL_VERSION_V6);
        record.environment.schema_version = u64::from(RL_SESSION_SCHEMA_VERSION_V6);
        record.environment.environment_randomization_v2 =
            Some(exact_environment_randomization_section_v2());
        record.contracts.trajectory = v2_trajectory_contract_v2();
        refresh_derived(&mut record);
        record
    }

    pub(super) fn coherent_v2_bytes() -> Vec<u8> {
        to_canonical_json_bytes_v1(&coherent_v2_record(), CanonicalJsonNullPolicyV1::Forbid)
            .unwrap()
    }

    #[test]
    fn coherent_v2_record_remints_decodes_and_classifies_as_v2() {
        let bytes = coherent_v2_bytes();
        let run = decode_train_run_v2(&bytes).expect("a coherent V2 record decodes");
        assert_eq!(
            run.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        // The reminted digests are the ones the decoder recomputed.
        assert_eq!(run.canonical_bytes(), bytes.as_slice());
        assert_eq!(run.run_sha256(), sha256_hex(&bytes));
        assert_eq!(
            run.record().contracts.standalone_semantics.sha256,
            run.standalone_semantics_sha256()
        );
        assert_eq!(
            run.record().contracts.identity_bundle_sha256,
            run.identity_bundle_sha256()
        );
        // The section rode into standalone semantics through the existing
        // complete-environment clone, with no duplicate semantics-core field.
        assert_eq!(
            run.record()
                .contracts
                .standalone_semantics
                .core
                .environment
                .environment_randomization_v2,
            Some(exact_environment_randomization_section_v2())
        );
    }

    /// The synthetic legacy fixture still classifies as legacy and still omits
    /// the optional section from canonical bytes.
    ///
    /// This is a round-trip and omission check on a fixture this module builds,
    /// so it is deliberately NOT offered as the byte-identity proof: comparing
    /// a fixture to itself cannot detect a change that moved both sides. The
    /// noncircular proof that real historical records are unaffected lives in
    /// the existing `real_s1_mirror` and `real_ladder_pilot` tests, which pin
    /// literal stored `run_sha256` values captured before this change. Those
    /// tests are untouched by this patch and must keep passing.
    #[test]
    fn legacy_v1_fixture_still_classifies_as_legacy_and_omits_the_section() {
        let bytes = fixture_bytes();
        let run = decode_train_run_v2(&bytes).expect("the legacy fixture decodes");
        assert_eq!(
            run.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1
        );
        assert_eq!(run.record().environment.environment_randomization_v2, None);
        assert_eq!(run.canonical_bytes(), bytes.as_slice());
        assert_eq!(run.run_sha256(), sha256_hex(&bytes));
        let text = String::from_utf8(bytes).expect("canonical bytes are UTF-8");
        assert!(
            !text.contains("environment_randomization_v2"),
            "an absent section must be omitted from canonical bytes entirely"
        );
    }

    /// The exhaustive closed-tuple proof: two section states, four
    /// protocol/schema pairs, and all sixty-four masks over the six trajectory
    /// pins. Exactly two of the five hundred twelve combinations are accepted.
    #[test]
    fn classification_accepts_only_complete_v1_and_complete_v2_tuples() {
        let legacy = legacy_trajectory_contract_v1();
        let v2 = v2_trajectory_contract_v2();
        let version_pairs = [
            (
                u64::from(FROZEN_PROTOCOL_VERSION_V2),
                u64::from(FROZEN_SCHEMA_VERSION_V2),
            ),
            (
                u64::from(FROZEN_PROTOCOL_VERSION_V2),
                u64::from(RL_SESSION_SCHEMA_VERSION_V6),
            ),
            (
                u64::from(RL_SESSION_PROTOCOL_VERSION_V6),
                u64::from(FROZEN_SCHEMA_VERSION_V2),
            ),
            (
                u64::from(RL_SESSION_PROTOCOL_VERSION_V6),
                u64::from(RL_SESSION_SCHEMA_VERSION_V6),
            ),
        ];
        let mut accepted = 0_usize;
        let mut total = 0_usize;
        for section_present in [false, true] {
            for (protocol_version, schema_version) in version_pairs {
                for mask in 0_u8..64 {
                    total += 1;
                    let mut record = fixture_record();
                    record.environment.protocol_version = protocol_version;
                    record.environment.schema_version = schema_version;
                    record.environment.environment_randomization_v2 = if section_present {
                        Some(exact_environment_randomization_section_v2())
                    } else {
                        None
                    };
                    // Bit i selects the V2 value for pin i, so mask 0 is the
                    // complete V1 tuple and mask 63 the complete V2 tuple.
                    let pick = |bit: u8, v1: &str, v2v: &str| {
                        if mask & (1 << bit) == 0 {
                            v1.to_owned()
                        } else {
                            v2v.to_owned()
                        }
                    };
                    record.contracts.trajectory = TrajectoryContractV2 {
                        identity: pick(0, &legacy.identity, &v2.identity),
                        cross_language_goldens_schema: pick(
                            1,
                            &legacy.cross_language_goldens_schema,
                            &v2.cross_language_goldens_schema,
                        ),
                        cross_language_generator_identity: pick(
                            2,
                            &legacy.cross_language_generator_identity,
                            &v2.cross_language_generator_identity,
                        ),
                        cross_language_golden_stream_identity: pick(
                            3,
                            &legacy.cross_language_golden_stream_identity,
                            &v2.cross_language_golden_stream_identity,
                        ),
                        cross_language_goldens_file_sha256: pick(
                            4,
                            &legacy.cross_language_goldens_file_sha256,
                            &v2.cross_language_goldens_file_sha256,
                        ),
                        cross_language_golden_stream_sha256: pick(
                            5,
                            &legacy.cross_language_golden_stream_sha256,
                            &v2.cross_language_golden_stream_sha256,
                        ),
                    };
                    let observed = classify_environment_trajectory_contract_v1(&record);
                    let expect_legacy = !section_present
                        && mask == 0
                        && protocol_version == u64::from(FROZEN_PROTOCOL_VERSION_V2)
                        && schema_version == u64::from(FROZEN_SCHEMA_VERSION_V2);
                    let expect_v2 = section_present
                        && mask == 63
                        && protocol_version == u64::from(RL_SESSION_PROTOCOL_VERSION_V6)
                        && schema_version == u64::from(RL_SESSION_SCHEMA_VERSION_V6);
                    match (expect_legacy, expect_v2) {
                        (true, false) => {
                            assert_eq!(
                                observed.as_ref().ok(),
                                Some(&NativeRunEnvironmentTrajectoryContractV1::LegacyV1),
                                "complete V1 tuple must classify as legacy"
                            );
                            accepted += 1;
                        }
                        (false, true) => {
                            assert_eq!(
                                observed.as_ref().ok(),
                                Some(
                                    &NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
                                ),
                                "complete V2 tuple must classify as V2"
                            );
                            accepted += 1;
                        }
                        _ => {
                            assert!(
                                observed.is_err(),
                                "section_present={section_present} versions=({protocol_version},{schema_version}) mask={mask} must reject"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(total, 2 * 4 * 64);
        assert_eq!(accepted, 2, "exactly two complete tuples are admissible");
    }

    /// Every descriptor field and every ordered array rejects under
    /// one-at-a-time mutation, with the rest of the tuple left complete.
    ///
    /// `TrainRunV2` deliberately has no `Clone`, so each iteration builds a
    /// fresh `coherent_v2_record()` rather than cloning a shared base. Only the
    /// optional section is cloned, which it derives from `Clone`.
    #[test]
    fn every_environment_descriptor_rejects_under_single_mutation() {
        type SectionMutatorV2 = Box<dyn Fn(&mut EnvironmentRandomizationContractV2)>;

        assert!(classify_environment_trajectory_contract_v1(&coherent_v2_record()).is_ok());

        let mut mutators: Vec<(String, SectionMutatorV2)> = vec![
            (
                "identity".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.identity.push_str("-drift")
                }),
            ),
            (
                "namespace".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.namespace.push_str("-drift")
                }),
            ),
            (
                "atom".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| s.atom.push_str("-drift")),
            ),
            (
                "extraction".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.extraction.push_str("-drift")
                }),
            ),
            (
                "initial_ordinal_rule".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.initial_ordinal_rule.push_str("-drift")
                }),
            ),
            (
                "overflow_rule".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.overflow_rule.push_str("-drift")
                }),
            ),
            (
                "shuffle_algorithm".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.shuffle_algorithm.push_str("-drift")
                }),
            ),
            (
                "cross_language_goldens_schema".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.cross_language_goldens_schema.push_str("-drift")
                }),
            ),
            (
                "cross_language_goldens_file_sha256".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.cross_language_goldens_file_sha256.push('0')
                }),
            ),
            (
                "owners[0]".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.owners[0].push_str("-drift")
                }),
            ),
            (
                "owners[1]".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.owners[1].push_str("-drift")
                }),
            ),
            (
                "owners swapped".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| s.owners.swap(0, 1)),
            ),
            (
                "purposes[0]".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.purposes[0].push_str("-drift")
                }),
            ),
            (
                "purposes[1]".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.purposes[1].push_str("-drift")
                }),
            ),
            (
                "purposes swapped".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| s.purposes.swap(0, 1)),
            ),
            (
                "ordered_atoms truncated".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.ordered_atoms.pop();
                }),
            ),
            (
                "ordered_atoms extended".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| {
                    s.ordered_atoms.push(vec!["field-name".to_owned()])
                }),
            ),
            (
                "ordered_atoms reordered".to_owned(),
                Box::new(|s: &mut EnvironmentRandomizationContractV2| s.ordered_atoms.swap(0, 1)),
            ),
        ];

        // Every one of the six frozen rows carries authority, not just row
        // zero: mutate the last element of each row in turn.
        assert_eq!(ENVIRONMENT_RANDOMIZATION_ORDERED_ATOMS_V2.len(), 6);
        for row in 0..ENVIRONMENT_RANDOMIZATION_ORDERED_ATOMS_V2.len() {
            mutators.push((
                format!("ordered_atoms[{row}] last element"),
                Box::new(move |s: &mut EnvironmentRandomizationContractV2| {
                    let last = s.ordered_atoms[row].len() - 1;
                    s.ordered_atoms[row][last].push_str("-drift");
                }),
            ));
        }

        // Row arity is authority too, for both frozen row shapes: rows 0 and 1
        // are two-element rows, rows 2 through 5 are four-element rows. Shrink
        // and grow one row of each shape.
        for (row, arity) in [(0_usize, 2_usize), (1, 2), (2, 4), (3, 4), (4, 4), (5, 4)] {
            assert_eq!(ENVIRONMENT_RANDOMIZATION_ORDERED_ATOMS_V2[row].len(), arity);
            mutators.push((
                format!("ordered_atoms[{row}] arity {arity} shrunk"),
                Box::new(move |s: &mut EnvironmentRandomizationContractV2| {
                    s.ordered_atoms[row].pop();
                }),
            ));
            mutators.push((
                format!("ordered_atoms[{row}] arity {arity} grown"),
                Box::new(move |s: &mut EnvironmentRandomizationContractV2| {
                    s.ordered_atoms[row].push("extra".to_owned())
                }),
            ));
        }

        for (label, mutate) in &mutators {
            let mut record = coherent_v2_record();
            let mut section = record
                .environment
                .environment_randomization_v2
                .clone()
                .expect("the V2 base carries a section");
            mutate(&mut section);
            record.environment.environment_randomization_v2 = Some(section);
            assert!(
                classify_environment_trajectory_contract_v1(&record).is_err(),
                "{label} must reject"
            );
        }
    }

    #[test]
    fn environment_section_rejects_unknown_and_missing_fields() {
        let mut value = serde_json::to_value(exact_environment_randomization_section_v2()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.insert("unexpected".to_owned(), serde_json::json!("x"));
        assert!(
            serde_json::from_value::<EnvironmentRandomizationContractV2>(value.clone()).is_err(),
            "an unknown section field must not decode"
        );
        for field in [
            "identity",
            "namespace",
            "atom",
            "extraction",
            "ordered_atoms",
            "owners",
            "purposes",
            "initial_ordinal_rule",
            "overflow_rule",
            "shuffle_algorithm",
            "cross_language_goldens_schema",
            "cross_language_goldens_file_sha256",
        ] {
            let mut missing =
                serde_json::to_value(exact_environment_randomization_section_v2()).unwrap();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<EnvironmentRandomizationContractV2>(missing).is_err(),
                "missing {field} must not decode"
            );
        }
    }

    #[test]
    fn cuda_backend_records_validate_and_mismatched_pairs_reject() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        let bytes =
            test_fixture_bytes_with_backend_v2(NativeTrainingNumericalBackendV1::CudaBurnDense);
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(
            validated.store_numerical_backend_v2(),
            Some(NativeTrainingNumericalBackendV1::CudaBurnDense)
        );

        let cpu_bytes = fixture_bytes();
        let cpu_validated = decode_train_run_v2(&cpu_bytes).unwrap();
        assert_eq!(
            cpu_validated.store_numerical_backend_v2(),
            Some(NativeTrainingNumericalBackendV1::Sequential)
        );

        // A CUDA tuple with the CPU backend identity is a mismatched pair.
        let mut record = fixture_record();
        record.runtime.tuple_identity = CUDA_RUNTIME_TUPLE_IDENTITY_V2.to_owned();
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);

        // A CPU tuple with the CUDA backend identity is likewise rejected.
        let mut record = fixture_record();
        record.runtime.numerical_backend_identity =
            crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1
                .to_owned();
        record.contracts.train_step.numerical_backend_identity =
            record.runtime.numerical_backend_identity.clone();
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    fn apply_backend_pair(
        record: &mut TrainRunV2,
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    ) {
        let (tuple, identity) = match backend {
            crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1::CudaBurnDense => (
                CUDA_RUNTIME_TUPLE_IDENTITY_V2,
                crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1,
            ),
            _ => (
                CPU_RUNTIME_TUPLE_IDENTITY_V2,
                FROZEN_NUMERICAL_BACKEND_IDENTITY_V2,
            ),
        };
        record.runtime.tuple_identity = tuple.to_owned();
        record.runtime.numerical_backend_identity = identity.to_owned();
        record.contracts.train_step.numerical_backend_identity = identity.to_owned();
    }

    pub(super) fn fixture_bytes_with_backend(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        apply_backend_pair(&mut record, backend);
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(
        not(all(windows, feature = "experimental-burn-net8-packed-cuda-v1")),
        allow(dead_code)
    )]
    pub(super) fn fixture_bytes_with_schedule(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> Vec<u8> {
        fixture_bytes_with_schedule_and_base_seed(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            71501,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        apply_backend_pair(&mut record, backend);
        record.limits.max_physical_decisions = max_physical_decisions;
        record.limits.max_policy_steps = max_policy_steps;
        record.schedule.batch_episodes = batch_episodes;
        record.schedule.checkpoint_segment_updates = checkpoint_segment_updates;
        record.schedule.requested_successful_updates = requested_successful_updates;
        record.schedule.checkpoint_episode_interval = batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .unwrap();
        record.topology.worker_count = worker_count;
        record.topology.sessions_per_worker = sessions_per_worker;
        record.topology.logical_actor_count =
            worker_count.checked_mul(sessions_per_worker).unwrap();
        record.topology.broker_batch_target = broker_batch_target;
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    /// Installs the complete environment-randomization-V2 tuple on an
    /// already-built test run record, then remints every derived field. The
    /// base builder remains the sole owner of schedule, topology, backend,
    /// ladder, initialization, and model choices.
    fn compose_environment_randomization_v2(base_bytes: Vec<u8>) -> Vec<u8> {
        let wire: TrainRunWireV2 = serde_json::from_slice(&base_bytes).unwrap();
        let mut record = TrainRunV2::from(wire);
        record.environment.protocol_version = u64::from(RL_SESSION_PROTOCOL_VERSION_V6);
        record.environment.schema_version = u64::from(RL_SESSION_SCHEMA_VERSION_V6);
        record.environment.environment_randomization_v2 =
            Some(exact_environment_randomization_section_v2());
        record.contracts.trajectory = v2_trajectory_contract_v2();
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
    ) -> Vec<u8> {
        compose_environment_randomization_v2(fixture_bytes_with_schedule_and_base_seed(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            base_seed,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_ladder(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        apply_backend_pair(&mut record, backend);
        record.limits.max_physical_decisions = max_physical_decisions;
        record.limits.max_policy_steps = max_policy_steps;
        record.schedule.batch_episodes = batch_episodes;
        record.schedule.checkpoint_segment_updates = checkpoint_segment_updates;
        record.schedule.requested_successful_updates = requested_successful_updates;
        record.schedule.checkpoint_episode_interval = batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .unwrap();
        record.topology.worker_count = worker_count;
        record.topology.sessions_per_worker = sessions_per_worker;
        record.topology.logical_actor_count =
            worker_count.checked_mul(sessions_per_worker).unwrap();
        record.topology.broker_batch_target = broker_batch_target;
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(pool);
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
    ) -> Vec<u8> {
        compose_environment_randomization_v2(fixture_bytes_with_schedule_and_base_seed_ladder(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            base_seed,
            pool,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_population_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
        initialization: OpponentLadderInitializationContractV1,
    ) -> Vec<u8> {
        let base = fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            base_seed,
            pool,
            initialization,
        );
        let wire: TrainRunWireV2 = serde_json::from_slice(&base).unwrap();
        let mut record = TrainRunV2::from(wire);
        record.contracts.population_program_v1 =
            Some(population_program_fixture_for_seed(base_seed));
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
        initialization: OpponentLadderInitializationContractV1,
        policy_anchor_beta_f32_bits: &str,
    ) -> Vec<u8> {
        let base = fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            base_seed,
            pool,
            initialization,
        );
        let wire: TrainRunWireV2 = serde_json::from_slice(&base).unwrap();
        let mut record = TrainRunV2::from(wire);
        let mut response = response_exploiter_fixture_for_seed(base_seed);
        response.policy_anchor_beta_f32_bits = policy_anchor_beta_f32_bits.to_owned();
        record.contracts.response_exploiter_v1 = Some(response);
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_response_exploiter_denovo_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
    ) -> Vec<u8> {
        // Built on the no-init ladder+envrand-v2 base (no
        // `opponent_ladder_initialization` at all), unlike the
        // warm-start-response-exploiter builder above, which is built on the
        // init variant. This is the structural fact that makes
        // "denovo-screen" fresh-init by construction rather than by a
        // separately-checked flag.
        let base = fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            base_seed,
            pool,
        );
        let wire: TrainRunWireV2 = serde_json::from_slice(&base).unwrap();
        let mut record = TrainRunV2::from(wire);
        record.contracts.response_exploiter_v1 =
            Some(response_exploiter_denovo_fixture_for_seed(base_seed));
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_ladder_init(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
        initialization: OpponentLadderInitializationContractV1,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        apply_backend_pair(&mut record, backend);
        record.limits.max_physical_decisions = max_physical_decisions;
        record.limits.max_policy_steps = max_policy_steps;
        record.schedule.batch_episodes = batch_episodes;
        record.schedule.checkpoint_segment_updates = checkpoint_segment_updates;
        record.schedule.requested_successful_updates = requested_successful_updates;
        record.schedule.checkpoint_episode_interval = batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .unwrap();
        record.topology.worker_count = worker_count;
        record.topology.sessions_per_worker = sessions_per_worker;
        record.topology.logical_actor_count =
            worker_count.checked_mul(sessions_per_worker).unwrap();
        record.topology.broker_batch_target = broker_batch_target;
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(pool);
        record.contracts.opponent_ladder_initialization = Some(initialization);
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
        initialization: OpponentLadderInitializationContractV1,
    ) -> Vec<u8> {
        compose_environment_randomization_v2(fixture_bytes_with_schedule_and_base_seed_ladder_init(
            backend,
            batch_episodes,
            checkpoint_segment_updates,
            requested_successful_updates,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            max_physical_decisions,
            max_policy_steps,
            base_seed,
            pool,
            initialization,
        ))
    }

    /// Stamps `record.model_snapshot`/`contracts.model` with the frozen WIDE
    /// literals and populates `contracts.wide_model_experiment_v1`. Shared by
    /// every wide fixture builder so they can never drift from each other.
    fn apply_wide_model_experiment(record: &mut TrainRunV2) {
        record.model_snapshot = CommonModelSnapshotRecordV1 {
            schema: FROZEN_WIDE_SNAPSHOT_SCHEMA_V1.to_owned(),
            identity: FROZEN_WIDE_SNAPSHOT_IDENTITY_V1.to_owned(),
            snapshot_sha256: FROZEN_WIDE_SNAPSHOT_SHA256_V1.to_owned(),
            manifest_file_sha256: FROZEN_WIDE_SNAPSHOT_MANIFEST_FILE_SHA256_V1.to_owned(),
            manifest_core_sha256: FROZEN_WIDE_SNAPSHOT_MANIFEST_CORE_SHA256_V1.to_owned(),
            payload_sha256: FROZEN_WIDE_SNAPSHOT_PAYLOAD_SHA256_V1.to_owned(),
            payload_byte_count: FROZEN_WIDE_SNAPSHOT_PAYLOAD_BYTE_COUNT_V1,
            parameter_layout_sha256: FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1.to_owned(),
            named_parameter_stream_sha256: FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
                .to_owned(),
            loaded_named_parameter_stream_sha256:
                FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1.to_owned(),
            parameter_tensor_count: FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1,
            parameter_element_count: FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1,
            model_config_fingerprint: FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1.to_owned(),
            model_architecture_version: FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1.to_owned(),
            feature_contract_digest: FROZEN_FEATURE_CONTRACT_DIGEST_V2.to_owned(),
            feature_encoding_digest: FROZEN_FEATURE_ENCODING_DIGEST_V2.to_owned(),
            initializer_identity: FROZEN_INITIALIZER_IDENTITY_V2.to_owned(),
            base_seed: FROZEN_BASE_SEED_V2,
            model_init_seed: FROZEN_MODEL_INIT_SEED_V2,
            trainer_schedule_version: FROZEN_TRAINER_SCHEDULE_IDENTITY_V2.to_owned(),
            python_reference_seed_version: FROZEN_PYTHON_REFERENCE_SEED_IDENTITY_V2.to_owned(),
            schedule_goldens_sha256: FROZEN_TRAINER_SCHEDULE_GOLDENS_SHA256_V2.to_owned(),
            authority_source_bundle_sha256: FROZEN_WIDE_SNAPSHOT_AUTHORITY_SOURCE_BUNDLE_SHA256_V1
                .to_owned(),
            authority_runtime_identity: FROZEN_SNAPSHOT_AUTHORITY_RUNTIME_IDENTITY_V2.to_owned(),
            loader_identity: FROZEN_WIDE_SNAPSHOT_LOADER_IDENTITY_V1.to_owned(),
            optimizer_identity: FROZEN_OPTIMIZER_IDENTITY_V2.to_owned(),
            adam_step_initial: FROZEN_ADAM_STEP_INITIAL_V2,
            moment_initialization: FROZEN_MOMENT_INITIALIZATION_V2.to_owned(),
            canonical_gauge_parameters: FROZEN_CANONICAL_GAUGE_PARAMETERS_V2
                .map(str::to_owned)
                .to_vec(),
            scorer_bias_anchor_f32_bits: FROZEN_WIDE_SCORER_BIAS_ANCHOR_F32_BITS_V1,
            snapshot_load_completed_before_trial_start: true,
            snapshot_load_timed: false,
            rust_seeded_initializer_reproduced: false,
            nonclaim: FROZEN_WIDE_SNAPSHOT_NONCLAIM_V1.to_owned(),
        };
        record.contracts.model = ModelContractV2 {
            architecture_identity: FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1.to_owned(),
            config_fingerprint: FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1.to_owned(),
            parameter_layout_sha256: FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1.to_owned(),
            parameter_tensor_count: FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1,
            parameter_element_count: FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1,
        };
        record.contracts.wide_model_experiment_v1 = Some(WideModelExperimentContractV1 {
            architecture_identity: FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1.to_owned(),
            config_fingerprint: FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1.to_owned(),
            snapshot_sha256: FROZEN_WIDE_SNAPSHOT_SHA256_V1.to_owned(),
            manifest_core_sha256: FROZEN_WIDE_SNAPSHOT_MANIFEST_CORE_SHA256_V1.to_owned(),
            payload_sha256: FROZEN_WIDE_SNAPSHOT_PAYLOAD_SHA256_V1.to_owned(),
            parameter_layout_sha256: FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1.to_owned(),
            named_parameter_stream_sha256: FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
                .to_owned(),
            parameter_tensor_count: FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1,
            parameter_element_count: FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1,
            diagnostic_label: FROZEN_WIDE_DIAGNOSTIC_LABEL_V1.to_owned(),
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_wide(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        apply_backend_pair(&mut record, backend);
        record.limits.max_physical_decisions = max_physical_decisions;
        record.limits.max_policy_steps = max_policy_steps;
        record.schedule.batch_episodes = batch_episodes;
        record.schedule.checkpoint_segment_updates = checkpoint_segment_updates;
        record.schedule.requested_successful_updates = requested_successful_updates;
        record.schedule.checkpoint_episode_interval = batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .unwrap();
        record.topology.worker_count = worker_count;
        record.topology.sessions_per_worker = sessions_per_worker;
        record.topology.logical_actor_count =
            worker_count.checked_mul(sessions_per_worker).unwrap();
        record.topology.broker_batch_target = broker_batch_target;
        apply_wide_model_experiment(&mut record);
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    /// Wide plus environment randomization V2: the wide schedule builder's
    /// exact record with the complete V2 declaration tuple installed, then
    /// reminted. Used by the runner's genuinely wide V2 acceptance oracle.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_wide_environment_v2(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        apply_backend_pair(&mut record, backend);
        record.limits.max_physical_decisions = max_physical_decisions;
        record.limits.max_policy_steps = max_policy_steps;
        record.schedule.batch_episodes = batch_episodes;
        record.schedule.checkpoint_segment_updates = checkpoint_segment_updates;
        record.schedule.requested_successful_updates = requested_successful_updates;
        record.schedule.checkpoint_episode_interval = batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .unwrap();
        record.topology.worker_count = worker_count;
        record.topology.sessions_per_worker = sessions_per_worker;
        record.topology.logical_actor_count =
            worker_count.checked_mul(sessions_per_worker).unwrap();
        record.topology.broker_batch_target = broker_batch_target;
        apply_wide_model_experiment(&mut record);
        record.environment.protocol_version = u64::from(RL_SESSION_PROTOCOL_VERSION_V6);
        record.environment.schema_version = u64::from(RL_SESSION_SCHEMA_VERSION_V6);
        record.environment.environment_randomization_v2 =
            Some(exact_environment_randomization_section_v2());
        record.contracts.trajectory = v2_trajectory_contract_v2();
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixture_bytes_with_schedule_and_base_seed_wide_ladder(
        backend: crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1,
        batch_episodes: u64,
        checkpoint_segment_updates: u64,
        requested_successful_updates: u64,
        worker_count: u64,
        sessions_per_worker: u64,
        broker_batch_target: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        pool: OpponentLadderPoolContractV1,
    ) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        apply_backend_pair(&mut record, backend);
        record.limits.max_physical_decisions = max_physical_decisions;
        record.limits.max_policy_steps = max_policy_steps;
        record.schedule.batch_episodes = batch_episodes;
        record.schedule.checkpoint_segment_updates = checkpoint_segment_updates;
        record.schedule.requested_successful_updates = requested_successful_updates;
        record.schedule.checkpoint_episode_interval = batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .unwrap();
        record.topology.worker_count = worker_count;
        record.topology.sessions_per_worker = sessions_per_worker;
        record.topology.logical_actor_count =
            worker_count.checked_mul(sessions_per_worker).unwrap();
        record.topology.broker_batch_target = broker_batch_target;
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(pool);
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        apply_wide_model_experiment(&mut record);
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    pub(super) fn fixture_bytes_with_base_seed(base_seed: u64) -> Vec<u8> {
        let mut record = fixture_record();
        record.schedule.base_seed = base_seed;
        refresh_derived(&mut record);
        to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap()
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
            FROZEN_SNAPSHOT_IDENTITY_V2
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
    fn production_owners_and_record_are_independently_pinned_to_frozen_rev3() {
        validate_frozen_rev3_authorities_v2().unwrap();

        // The fixture is assembled from the frozen RunV2 literals, not from
        // production owners (with the sole deliberate exception of the two
        // catalog-content fields, which `fixture_record()` now sets to the
        // CURRENT-profile literals -- see `classify_catalog_profile_v1` and
        // the dedicated dual-profile tests below for that axis). Successful
        // decode therefore exercises the independent owner-to-frozen and
        // record-to-frozen checks together for every other authority.
        let record = fixture_record();
        assert_eq!(
            record.source.source_tree_recipe_byte_count,
            FROZEN_SOURCE_TREE_RECIPE_BYTE_COUNT_V2
        );
        assert_eq!(
            record.environment.runtime_catalog_sha256,
            FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1
        );
        assert_eq!(
            record.contracts.tensorizer,
            TensorizerContractV2 {
                identity: FROZEN_TENSORIZER_IDENTITY_V2.to_owned(),
                feature_contract_digest: FROZEN_FEATURE_CONTRACT_DIGEST_V2.to_owned(),
                feature_encoding_digest: FROZEN_FEATURE_ENCODING_DIGEST_V2.to_owned(),
                authoritative_features_source_sha256: FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2
                    .to_owned(),
                fixture_sha256: FROZEN_TENSORIZER_FIXTURE_SHA256_V2.to_owned(),
                fixture_payload_sha256: FROZEN_TENSORIZER_FIXTURE_PAYLOAD_SHA256_V2.to_owned(),
            }
        );
        assert_eq!(
            record
                .contracts
                .learner_sampler
                .cross_language_vectors_file_sha256,
            FROZEN_LEARNER_VECTORS_FILE_SHA256_V2
        );
        assert_eq!(
            record
                .contracts
                .learner_sampler
                .cross_language_vector_stream_sha256,
            FROZEN_LEARNER_VECTOR_STREAM_SHA256_V2
        );
        assert_eq!(
            record.contracts.opponent_policy.identity,
            FROZEN_OPPONENT_POLICY_IDENTITY_V2
        );
        assert_eq!(
            record.contracts.opponent_policy.model_rule,
            FROZEN_OPPONENT_POLICY_MODEL_RULE_V2
        );
        assert_eq!(
            record
                .contracts
                .opponent_sampler
                .cross_language_vectors_file_sha256,
            FROZEN_OPPONENT_VECTORS_FILE_SHA256_V2
        );
        assert_eq!(
            record
                .contracts
                .opponent_sampler
                .cross_language_vector_stream_sha256,
            FROZEN_OPPONENT_VECTOR_STREAM_SHA256_V2
        );
        assert_eq!(
            record.contracts.trajectory,
            TrajectoryContractV2 {
                identity: FROZEN_LEGACY_TRAJECTORY_IDENTITY_V1.to_owned(),
                cross_language_goldens_schema: FROZEN_LEGACY_TRAJECTORY_GOLDENS_SCHEMA_V1
                    .to_owned(),
                cross_language_generator_identity:
                    FROZEN_LEGACY_TRAJECTORY_GOLDENS_GENERATOR_IDENTITY_V1.to_owned(),
                cross_language_golden_stream_identity:
                    FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_IDENTITY_V1.to_owned(),
                cross_language_goldens_file_sha256: FROZEN_LEGACY_TRAJECTORY_GOLDENS_FILE_SHA256_V1
                    .to_owned(),
                cross_language_golden_stream_sha256:
                    FROZEN_LEGACY_TRAJECTORY_GOLDEN_STREAM_SHA256_V1.to_owned(),
            }
        );

        let bytes = fixture_bytes();
        decode_train_run_v2(&bytes).unwrap();
        assert!(
            !bytes
                .windows(b"membership".len())
                .any(|window| window == b"membership")
        );
    }

    // ------------------------------------------------------------------
    // Dual-Profile Catalog Successor (collab CLAUDE #220)
    // ------------------------------------------------------------------

    /// Canary: the new CURRENT-profile frozen literals must equal today's
    /// live build constants exactly. If this ever fails, either the crate's
    /// card database/runtime catalog changed again (needs a new profile) or
    /// the frozen literals were typed wrong when this successor landed.
    #[test]
    fn current_frozen_literal_matches_the_live_build_constant() {
        use crate::card_def::KERNEL_CARDDB_HASH;
        use crate::runtime_decks::RUNTIME_DECK_CATALOG_FILE_SHA256;
        assert_eq!(
            format!("{KERNEL_CARDDB_HASH:016x}"),
            FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1
        );
        assert_eq!(
            RUNTIME_DECK_CATALOG_FILE_SHA256,
            FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1
        );
    }

    /// Current-pin tripwire (fix round, panel finding 4). The two literals
    /// below are typed independently of `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`/
    /// `FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1`'s own definitions, not
    /// derived from them, so this test cannot pass by construction the way a
    /// self-referential comparison would.
    ///
    /// THE RULE: a future catalog move must add a NEW frozen profile
    /// (a fourth literal pair, a new `NativeRunCatalogProfileV1` variant, a
    /// new arm in `classify_catalog_profile_v1` and at every mutation
    /// boundary) exactly the way this successor added CURRENT alongside the
    /// untouched HISTORICAL pair. It must never overwrite
    /// `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`/`FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1`
    /// in place -- doing so would silently reinterpret every already-sealed
    /// CURRENT-profile record (this successor's own science-evidence era) as
    /// whatever the new catalog happens to be, exactly the byte-identity
    /// violation the whole frozen-literal discipline exists to prevent. If
    /// this test fails, someone overwrote the constant in place; the fix is
    /// to revert that edit and instead follow the third-profile pattern.
    #[test]
    fn current_pin_is_not_silently_overwritten_in_place() {
        assert_eq!(
            FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1, "64c82a261e078f1a",
            "FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1 was overwritten in place; add a new frozen \
             profile instead of moving this one"
        );
        assert_eq!(
            FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1,
            "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851",
            "FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1 was overwritten in place; add a new frozen \
             profile instead of moving this one"
        );
    }

    /// Direct unit coverage of `current_profile_matches_live_build_identity_v1`
    /// (fix round, panel finding 1) in isolation, both directions, before the
    /// fuller boundary-integration tests exercise it through
    /// `resume_native_training_store_v2`/`publish_generation_v2`.
    #[test]
    fn current_profile_live_identity_check_matches_real_and_rejects_shimmed() {
        let record = fixture_record();
        assert!(current_profile_matches_live_build_identity_v1(
            &record.environment
        ));

        let _shim = LiveCatalogBuildIdentityOverrideGuardV1::install(
            "ffffffffffffffff",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        );
        assert!(!current_profile_matches_live_build_identity_v1(
            &record.environment
        ));
    }

    #[test]
    fn historical_fixture_decodes_clean_and_classifies_historical() {
        let validated = decode_train_run_v2(&fixture_bytes_historical()).unwrap();
        assert_eq!(
            validated.catalog_profile_v1(),
            NativeRunCatalogProfileV1::Historical
        );
        assert_eq!(
            validated.record().environment.card_db_hash_u64_hex,
            FROZEN_CARD_DB_HASH_U64_HEX_V2
        );
        assert_eq!(
            validated.record().environment.runtime_catalog_sha256,
            FROZEN_RUNTIME_CATALOG_SHA256_V2
        );
    }

    #[test]
    fn current_fixture_decodes_clean_and_classifies_current() {
        let validated = decode_train_run_v2(&fixture_bytes()).unwrap();
        assert_eq!(
            validated.catalog_profile_v1(),
            NativeRunCatalogProfileV1::Current
        );
        assert_eq!(
            validated.record().environment.card_db_hash_u64_hex,
            FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1
        );
        assert_eq!(
            validated.record().environment.runtime_catalog_sha256,
            FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1
        );
    }

    #[test]
    fn hybrid_current_card_db_with_historical_catalog_sha_is_rejected() {
        let mut record = fixture_record();
        record.environment.card_db_hash_u64_hex =
            FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1.to_owned();
        record.environment.runtime_catalog_sha256 = FROZEN_RUNTIME_CATALOG_SHA256_V2.to_owned();
        refresh_derived(&mut record);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(
            decode_train_run_v2(&bytes).unwrap_err().kind(),
            TrainRunV2ErrorKind::InvalidLiteral
        );
    }

    #[test]
    fn hybrid_historical_card_db_with_current_catalog_sha_is_rejected() {
        let mut record = fixture_record_historical();
        record.environment.runtime_catalog_sha256 =
            FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1.to_owned();
        refresh_derived(&mut record);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(
            decode_train_run_v2(&bytes).unwrap_err().kind(),
            TrainRunV2ErrorKind::InvalidLiteral
        );
    }

    #[test]
    fn neither_known_catalog_literal_pair_is_rejected() {
        let mut record = fixture_record();
        record.environment.card_db_hash_u64_hex = "1111111111111111".to_owned();
        refresh_derived(&mut record);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(
            decode_train_run_v2(&bytes).unwrap_err().kind(),
            TrainRunV2ErrorKind::InvalidLiteral
        );
    }

    // ------------------------------------------------------------------
    // Feature-Encoder Successor (collab CLAUDE #221, folding CODEX #235)
    // ------------------------------------------------------------------
    //
    // `fixture_record()`'s tensorizer section is deliberately left at the
    // HISTORICAL `authoritative_features_source_sha256` (unlike the catalog
    // dual-profile work, which switched `fixture_record()`'s default to
    // CURRENT): this file has multiple hardcoded digests
    // (`independent_digest_references_and_goldens_match` above, and others)
    // that are recomputed BY RUNNING the test suite whenever
    // `fixture_record()`'s bytes change, which the no-cargo-until-GO
    // constraint on this branch does not allow. The tests below build their
    // own CURRENT-profile variant locally instead, exactly as the
    // hybrid-rejection tests above do for the catalog axis.

    /// Canary: the CURRENT tensorizer-authority literal must equal the live
    /// `native_flat_tensorizer_v2` module constant exactly (the two are
    /// independently typed on purpose; this is the cross-module tripwire).
    /// If this fails, either the two literals were typed inconsistently or
    /// only one side of the successor was updated.
    #[test]
    fn tensorizer_current_frozen_literal_matches_the_live_module_constant() {
        assert_eq!(
            crate::native_flat_tensorizer_v2::NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2,
            FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1
        );
    }

    /// Current-pin tripwire, same discipline as
    /// `current_pin_is_not_silently_overwritten_in_place`: the literal below
    /// is typed independently of `FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1`'s
    /// own definition, so this cannot pass by self-reference. A future
    /// features.py change must add a fourth tensorizer-authority literal
    /// alongside this one, never overwrite it in place.
    ///
    /// UNVERIFIED PENDING RECONCILIATION: this literal is this branch's own
    /// computed SHA-256 of its reconstruction of CODEX #235's fix, not the
    /// collab-bound `b316c0aa...` value (see the branch report). Once
    /// reconciled, update both this literal and
    /// `FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1` together.
    #[test]
    fn tensorizer_current_pin_is_not_silently_overwritten_in_place() {
        assert_eq!(
            FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1,
            "5d82f5b87a6819076c903390230015da456f914828890d9c5384af410f21be1c",
            "FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1 was overwritten in place; add a \
             new frozen profile instead of moving this one"
        );
    }

    /// A record whose tensorizer contract carries the CURRENT
    /// `authoritative_features_source_sha256` (fixture hashes left at their
    /// single existing historical value, since no CURRENT fixture golden
    /// exists yet) decodes clean. This is the acceptance half of the
    /// dual-profile widen: new captures from a crate built with the
    /// historical stack-source encoder fix must not hit the outage
    /// `classify_catalog_profile_v1`'s sibling design was built to prevent.
    #[test]
    fn current_tensorizer_authority_source_sha256_decodes_clean() {
        let mut record = fixture_record();
        record.contracts.tensorizer.authoritative_features_source_sha256 =
            FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1.to_owned();
        refresh_derived(&mut record);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(
            validated
                .record()
                .contracts
                .tensorizer
                .authoritative_features_source_sha256,
            FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_CURRENT_V1
        );
    }

    /// The historical-default fixture (unmodified `fixture_record()`) still
    /// decodes clean under the widened check -- the historical literal was
    /// never removed, only joined by the current one. Every already-sealed
    /// RunV2 evidence store keeps decoding.
    #[test]
    fn historical_tensorizer_authority_source_sha256_still_decodes_clean() {
        let validated = decode_train_run_v2(&fixture_bytes()).unwrap();
        assert_eq!(
            validated
                .record()
                .contracts
                .tensorizer
                .authoritative_features_source_sha256,
            FROZEN_TENSORIZER_AUTHORITY_SOURCE_SHA256_V2
        );
    }

    /// Neither the historical nor the current literal: rejected. Mutation
    /// target for the widen itself -- deleting either `||` arm of
    /// `matches_frozen_tensorizer_authority_source_sha256_v1` would make one
    /// of `current_tensorizer_authority_source_sha256_decodes_clean` /
    /// `historical_tensorizer_authority_source_sha256_still_decodes_clean`
    /// fail, and collapsing the whole check to `true` would make this test
    /// fail.
    #[test]
    fn neither_known_tensorizer_authority_source_sha256_literal_is_rejected() {
        let mut record = fixture_record();
        record.contracts.tensorizer.authoritative_features_source_sha256 =
            "1111111111111111111111111111111111111111111111111111111111111111".to_owned();
        // (64 hex-looking chars would also work; length is not itself
        // validated for this field, only exact equality against one of the
        // two frozen literals, so any non-matching value proves the point.)
        refresh_derived(&mut record);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(
            decode_train_run_v2(&bytes).unwrap_err().kind(),
            TrainRunV2ErrorKind::InvalidLiteral
        );
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
        // Dual-Profile Catalog Successor (collab CLAUDE #220): these three
        // digests are recomputed here because `fixture_record()` now embeds
        // the CURRENT-profile catalog literals (see that function's doc
        // comment); the digests themselves are unrelated to catalog identity
        // otherwise, so a value change here is expected and is not itself
        // evidence of anything beyond the fixture's own environment fields
        // changing.
        assert_eq!(
            semantics,
            "affcfccc974e48a0da001147812e6ce0f0d106b6d1c8b4a545b8e5512185c8e0"
        );
        assert_eq!(
            identity,
            "f118e0a86ab58a145279ec0f4fb7446d1c67adeb7a968eb7d67aa4763d7bf323"
        );
        assert_eq!(
            sha256_hex(&run_bytes),
            "b99df8567b9ec40dff2d12db221c5e9af66d531c6dbf252dbf3eeae789387e8e"
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
        record.model_snapshot.snapshot_sha256 = FROZEN_SNAPSHOT_SHA256_V2.to_ascii_uppercase();
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

    fn hex64(fill: char) -> String {
        std::iter::repeat(fill).take(64).collect()
    }

    fn valid_ladder_checkpoint_ref(fill: char, generation: u64) -> OpponentLadderCheckpointRefV1 {
        OpponentLadderCheckpointRefV1 {
            source_run_sha256: hex64(fill),
            generation,
            checkpoint_sha256: hex64(fill),
            sidecar_sha256: hex64(fill),
            state_sha256: hex64(fill),
        }
    }

    fn valid_ladder_pool_fixture() -> OpponentLadderPoolContractV1 {
        OpponentLadderPoolContractV1 {
            identity: FROZEN_LADDER_POOL_IDENTITY_V2.to_owned(),
            size: FROZEN_LADDER_POOL_SIZE_V2,
            policy_member_sampling_rule: FROZEN_LADDER_POLICY_SAMPLING_RULE_V2.to_owned(),
            weight_primary: FROZEN_LADDER_POOL_WEIGHT_PRIMARY_V2,
            weight_predecessor_a: FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2,
            weight_predecessor_b: FROZEN_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2,
            weight_uniform_floor: FROZEN_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
            primary: valid_ladder_checkpoint_ref('a', 256),
            predecessor_a: valid_ladder_checkpoint_ref('b', 256),
            predecessor_b: valid_ladder_checkpoint_ref('c', 256),
            uniform_floor: OpponentLadderUniformFloorV1 {
                identity: FROZEN_OPPONENT_POLICY_IDENTITY_V2.to_owned(),
                model_rule: FROZEN_OPPONENT_POLICY_MODEL_RULE_V2.to_owned(),
                sampler_identity: FROZEN_OPPONENT_SAMPLER_IDENTITY_V2.to_owned(),
                sampler_algorithm: FROZEN_OPPONENT_SAMPLER_ALGORITHM_V2.to_owned(),
            },
        }
    }

    fn valid_opponent_schedule_v2_fixture() -> OpponentScheduleV2ContractV1 {
        OpponentScheduleV2ContractV1 {
            schedule_version: FROZEN_LADDER_SCHEDULE_VERSION_V2.to_owned(),
            seed_version: FROZEN_LADDER_SCHEDULE_SEED_VERSION_V2.to_owned(),
            opponent_pool_choice_namespace: FROZEN_LADDER_SCHEDULE_POOL_CHOICE_NAMESPACE_V2
                .to_owned(),
            opponent_pool_choice_fields: FROZEN_LADDER_SCHEDULE_POOL_CHOICE_FIELDS_V2.to_owned(),
            opponent_policy_substep_namespace: FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_NAMESPACE_V2
                .to_owned(),
            opponent_policy_substep_fields: FROZEN_LADDER_SCHEDULE_POLICY_SUBSTEP_FIELDS_V2
                .to_owned(),
            pool_choice_modulo: FROZEN_LADDER_SCHEDULE_POOL_CHOICE_MODULO_V2,
            pool_choice_threshold_rule: FROZEN_LADDER_SCHEDULE_POOL_CHOICE_THRESHOLD_RULE_V2
                .to_owned(),
            pool_choice_bias_rule: FROZEN_LADDER_SCHEDULE_POOL_CHOICE_BIAS_RULE_V2.to_owned(),
            version_change_rule: FROZEN_LADDER_SCHEDULE_VERSION_CHANGE_RULE_V2.to_owned(),
        }
    }

    fn ladder_record() -> TrainRunV2 {
        let mut record = fixture_record();
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(valid_ladder_pool_fixture());
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        refresh_derived(&mut record);
        record
    }

    fn population_program_fixture_for_seed(expected_base_seed: u64) -> PopulationProgramContractV1 {
        PopulationProgramContractV1 {
            identity: POPULATION_PROGRAM_IDENTITY_V1.to_owned(),
            package_commit: POPULATION_PACKAGE_COMMIT_V1.to_owned(),
            program_document_sha256: POPULATION_PROGRAM_DOCUMENT_SHA256_V1.to_owned(),
            retest_manifest_sha256: POPULATION_RETEST_MANIFEST_SHA256_V1.to_owned(),
            replay_end_generation: POPULATION_REPLAY_END_GENERATION_V1,
            program_update_count: POPULATION_PROGRAM_UPDATE_COUNT_V1,
            refresh_interval: POPULATION_REFRESH_INTERVAL_V1,
            slot_count: POPULATION_SLOT_COUNT_V1,
            reward_identity: POPULATION_REWARD_IDENTITY_V1.to_owned(),
            refresh_manifest_identity: POPULATION_REFRESH_MANIFEST_IDENTITY_V1.to_owned(),
            retest_beta_f32_bits: POPULATION_RETEST_BETA_F32_BITS_V1.to_owned(),
            expected_base_seed,
            pool_identity: POPULATION_POOL_IDENTITY_V1.to_owned(),
            pool_document_sha256: POPULATION_POOL_DOCUMENT_SHA256_V1.to_owned(),
            parent_source_run_sha256: POPULATION_PARENT_SOURCE_RUN_SHA256_V1.to_owned(),
            parent_generation: POPULATION_PARENT_GENERATION_V1,
            parent_checkpoint_sha256: POPULATION_PARENT_CHECKPOINT_SHA256_V1.to_owned(),
            parent_sidecar_sha256: POPULATION_PARENT_SIDECAR_SHA256_V1.to_owned(),
            parent_state_sha256: POPULATION_PARENT_STATE_SHA256_V1.to_owned(),
            parent_model_parameter_sha256: POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1.to_owned(),
            source_lineages: POPULATION_SOURCE_LINEAGES_V1.map(|lineage| {
                PopulationSourceLineageV1 {
                    base_seed: lineage.0,
                    store_tree_sha256: lineage.1.to_owned(),
                    run_sha256: lineage.2.to_owned(),
                    checkpoint_sha256: lineage.3.to_owned(),
                    sidecar_sha256: lineage.4.to_owned(),
                    state_sha256: lineage.5.to_owned(),
                    model_parameter_sha256: lineage.6.to_owned(),
                }
            }),
        }
    }

    fn population_program_fixture() -> PopulationProgramContractV1 {
        population_program_fixture_for_seed(970_001)
    }

    fn population_parent_initialization_fixture() -> OpponentLadderInitializationContractV1 {
        OpponentLadderInitializationContractV1 {
            source_run_sha256: POPULATION_PARENT_SOURCE_RUN_SHA256_V1.to_owned(),
            generation: POPULATION_PARENT_GENERATION_V1,
            checkpoint_sha256: POPULATION_PARENT_CHECKPOINT_SHA256_V1.to_owned(),
            sidecar_sha256: POPULATION_PARENT_SIDECAR_SHA256_V1.to_owned(),
            state_sha256: POPULATION_PARENT_STATE_SHA256_V1.to_owned(),
            derived_model_parameter_sha256: POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1.to_owned(),
        }
    }

    fn response_exploiter_fixture_for_seed(expected_base_seed: u64) -> ResponseExploiterContractV1 {
        ResponseExploiterContractV1 {
            identity: RESPONSE_EXPLOITER_IDENTITY_V1.to_owned(),
            package_commit: POPULATION_PACKAGE_COMMIT_V1.to_owned(),
            program_document_sha256: POPULATION_PROGRAM_DOCUMENT_SHA256_V1.to_owned(),
            target_refresh_manifest_sha256: RESPONSE_EXPLOITER_TARGET_REFRESH_SHA256_V1.to_owned(),
            target_global_generation: RESPONSE_EXPLOITER_TARGET_GLOBAL_GENERATION_V1,
            source_refresh_index: RESPONSE_EXPLOITER_SOURCE_REFRESH_INDEX_V1,
            source_program_update: RESPONSE_EXPLOITER_SOURCE_PROGRAM_UPDATE_V1,
            active_slot_indices: RESPONSE_EXPLOITER_ACTIVE_SLOT_INDICES_V1,
            excluded_slot_indices: RESPONSE_EXPLOITER_EXCLUDED_SLOT_INDICES_V1,
            renormalization_identity: RESPONSE_EXPLOITER_RENORMALIZATION_IDENTITY_V1.to_owned(),
            effective_weight_units: RESPONSE_EXPLOITER_EFFECTIVE_WEIGHT_UNITS_V1,
            effective_weight_total_units: RESPONSE_EXPLOITER_EFFECTIVE_WEIGHT_TOTAL_UNITS_V1,
            training_update_count: RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
            episodes_per_update: RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1,
            reward_identity: POPULATION_REWARD_IDENTITY_V1.to_owned(),
            fresh_adam_after_weight_init_identity:
                RESPONSE_EXPLOITER_FRESH_ADAM_AFTER_WEIGHT_INIT_IDENTITY_V1.to_owned(),
            authorized_base_seeds: RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1,
            authorized_screen_seeds: RESPONSE_EXPLOITER_AUTHORIZED_SCREEN_SEEDS_V1,
            authorized_denovo_seeds: RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_SEEDS_V1,
            authorized_denovo_512_seeds: Some(RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1),
            expected_base_seed,
            run_role: if RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1
                .contains(&expected_base_seed)
            {
                "build".to_owned()
            } else {
                "screen".to_owned()
            },
            expected_completion_generation: if RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1
                .contains(&expected_base_seed)
            {
                RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1
            } else {
                RESPONSE_EXPLOITER_SCREEN_COMPLETION_GENERATION_V1
            },
            policy_anchor_beta_f32_bits: RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1.to_owned(),
            parent_source_run_sha256: Some(POPULATION_PARENT_SOURCE_RUN_SHA256_V1.to_owned()),
            parent_generation: Some(POPULATION_PARENT_GENERATION_V1),
            parent_checkpoint_sha256: Some(POPULATION_PARENT_CHECKPOINT_SHA256_V1.to_owned()),
            parent_sidecar_sha256: Some(POPULATION_PARENT_SIDECAR_SHA256_V1.to_owned()),
            parent_state_sha256: Some(POPULATION_PARENT_STATE_SHA256_V1.to_owned()),
            parent_model_parameter_sha256: Some(
                POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1.to_owned(),
            ),
        }
    }

    /// De-novo sibling of [`response_exploiter_fixture_for_seed`]: same
    /// mixture/schedule binding, "denovo-screen"/"denovo-screen-512" role
    /// (auto-selected by `expected_base_seed` membership, exactly mirroring
    /// how the base fixture auto-selects "build" vs "screen"), beta=0 bits,
    /// no parent lineage (see the struct's own doc comment: Option-per-role,
    /// not a sentinel). The 512-update Phase 2 horizon amendment
    /// (CLAUDE-DENOVO-SCREEN-SHEET-V1.md) shares every structural denovo
    /// requirement with the original 256-update role; only the seed
    /// membership, role string, completion generation, and
    /// training-update-count differ, so this stays one function rather than
    /// a duplicated sibling -- seed 971_201 takes the exact same branch and
    /// produces byte-identical output as before this amendment.
    fn response_exploiter_denovo_fixture_for_seed(expected_base_seed: u64) -> ResponseExploiterContractV1 {
        let is_horizon_512 =
            RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1.contains(&expected_base_seed);
        let training_update_count = if is_horizon_512 {
            RESPONSE_EXPLOITER_DENOVO_512_TRAINING_UPDATE_COUNT_V1
        } else {
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1
        };
        ResponseExploiterContractV1 {
            fresh_adam_after_weight_init_identity:
                RESPONSE_EXPLOITER_DENOVO_FRESH_ADAM_AFTER_WEIGHT_INIT_IDENTITY_V1.to_owned(),
            run_role: if is_horizon_512 { "denovo-screen-512" } else { "denovo-screen" }.to_owned(),
            training_update_count,
            expected_completion_generation: training_update_count,
            policy_anchor_beta_f32_bits: RESPONSE_EXPLOITER_DENOVO_BETA_F32_BITS_V1.to_owned(),
            parent_source_run_sha256: None,
            parent_generation: None,
            parent_checkpoint_sha256: None,
            parent_sidecar_sha256: None,
            parent_state_sha256: None,
            parent_model_parameter_sha256: None,
            ..response_exploiter_fixture_for_seed(expected_base_seed)
        }
    }

    fn response_exploiter_record_for_seed(expected_base_seed: u64) -> TrainRunV2 {
        let mut record = coherent_v2_record();
        record.schedule.base_seed = expected_base_seed;
        record.schedule.batch_episodes = RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1;
        record.schedule.checkpoint_segment_updates =
            RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1;
        record.schedule.requested_successful_updates =
            RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1;
        record.schedule.checkpoint_episode_interval = RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1
            * RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1;
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(valid_ladder_pool_fixture());
        record.contracts.opponent_ladder_initialization =
            Some(population_parent_initialization_fixture());
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        record.contracts.response_exploiter_v1 =
            Some(response_exploiter_fixture_for_seed(expected_base_seed));
        refresh_derived(&mut record);
        record
    }

    /// De-novo sibling of [`response_exploiter_record_for_seed`]: identical
    /// mixture/ladder-pool/schedule binding, but
    /// `opponent_ladder_initialization` stays `None` (no warm start) and the
    /// attached contract is the "denovo-screen"/"denovo-screen-512" fixture
    /// (auto-selected by seed, see
    /// [`response_exploiter_denovo_fixture_for_seed`]). Seed 971_201 takes
    /// the exact same branch as before the Phase 2 horizon amendment.
    fn response_exploiter_denovo_record_for_seed(expected_base_seed: u64) -> TrainRunV2 {
        let mut record = coherent_v2_record();
        record.schedule.base_seed = expected_base_seed;
        record.schedule.batch_episodes = RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1;
        record.schedule.checkpoint_segment_updates =
            RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1;
        record.schedule.requested_successful_updates =
            if RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1.contains(&expected_base_seed) {
                RESPONSE_EXPLOITER_DENOVO_512_TRAINING_UPDATE_COUNT_V1
            } else {
                RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1
            };
        record.schedule.checkpoint_episode_interval = RESPONSE_EXPLOITER_EPISODES_PER_UPDATE_V1
            * RESPONSE_EXPLOITER_CHECKPOINT_SEGMENT_UPDATES_V1;
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(valid_ladder_pool_fixture());
        record.contracts.opponent_ladder_initialization = None;
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        record.contracts.response_exploiter_v1 =
            Some(response_exploiter_denovo_fixture_for_seed(expected_base_seed));
        refresh_derived(&mut record);
        record
    }

    fn population_record() -> TrainRunV2 {
        let mut record = coherent_v2_record();
        record.schedule.base_seed = 970_001;
        record.schedule.batch_episodes = 64;
        record.schedule.requested_successful_updates = 1_536;
        record.schedule.checkpoint_episode_interval = 64 * 4;
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        record.contracts.opponent_ladder_pool = Some(valid_ladder_pool_fixture());
        record.contracts.opponent_ladder_initialization =
            Some(population_parent_initialization_fixture());
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        record.contracts.population_program_v1 = Some(population_program_fixture());
        refresh_derived(&mut record);
        record
    }

    #[test]
    fn population_program_round_trips_and_binds_exact_authority() {
        let record = population_record();
        assert_ne!(record.source.git_commit, POPULATION_PACKAGE_COMMIT_V1);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());
        assert_eq!(
            validated.record().schedule.requested_successful_updates,
            POPULATION_REPLAY_END_GENERATION_V1 + POPULATION_PROGRAM_UPDATE_COUNT_V1
        );
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .contains("\"population_program_v1\":{")
        );
    }

    #[test]
    fn population_program_builder_mints_each_authorized_lineage() {
        for seed in POPULATION_EXPECTED_BASE_SEEDS_V1 {
            let bytes = fixture_bytes_with_schedule_and_base_seed_population_environment_v2(
                crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1::Sequential,
                64,
                4,
                1_536,
                2,
                32,
                16,
                400,
                2_000,
                seed,
                valid_ladder_pool_fixture(),
                population_parent_initialization_fixture(),
            );
            let run = decode_train_run_v2(&bytes).unwrap();
            let population = run
                .record()
                .contracts()
                .population_program_v1
                .as_ref()
                .unwrap();
            assert_eq!(population.expected_base_seed, seed);
            assert_eq!(run.requested_successful_updates(), 1_536);
            assert!(run
                .record()
                .environment
                .environment_randomization_v2
                .is_some());
        }
    }

    #[test]
    fn population_program_absence_preserves_legacy_bytes_and_run_hash() {
        let bytes = fixture_bytes();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert!(
            validated
                .record()
                .contracts()
                .population_program_v1
                .is_none()
        );
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());
        assert_eq!(
            sha256_hex(&bytes),
            "b99df8567b9ec40dff2d12db221c5e9af66d531c6dbf252dbf3eeae789387e8e"
        );
        assert!(
            !String::from_utf8(bytes)
                .unwrap()
                .contains("population_program_v1")
        );
    }

    #[test]
    fn population_program_update_arithmetic_and_hashes_fail_closed() {
        let mut wrong_updates = population_record();
        wrong_updates.schedule.requested_successful_updates = 1_532;
        refresh_derived(&mut wrong_updates);
        assert_record_error(wrong_updates, TrainRunV2ErrorKind::CrossBinding);

        let mut wrong_program_count = population_record();
        wrong_program_count
            .contracts
            .population_program_v1
            .as_mut()
            .unwrap()
            .program_update_count = 1_023;
        refresh_derived(&mut wrong_program_count);
        assert_record_error(wrong_program_count, TrainRunV2ErrorKind::InvalidLiteral);

        let mut wrong_parent_generation = population_record();
        wrong_parent_generation
            .contracts
            .population_program_v1
            .as_mut()
            .unwrap()
            .parent_generation = 383;
        refresh_derived(&mut wrong_parent_generation);
        assert_record_error(
            wrong_parent_generation,
            TrainRunV2ErrorKind::InvalidLiteral,
        );

        for mutate in [
            |p: &mut PopulationProgramContractV1| {
                p.program_document_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| p.retest_manifest_sha256 = ZERO_SHA256.to_owned(),
            |p: &mut PopulationProgramContractV1| {
                p.parent_source_run_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.parent_checkpoint_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.parent_sidecar_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.parent_state_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.parent_model_parameter_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.source_lineages[0].store_tree_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.source_lineages[1].run_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.source_lineages[2].checkpoint_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.source_lineages[0].sidecar_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.source_lineages[1].state_sha256 = ZERO_SHA256.to_owned()
            },
            |p: &mut PopulationProgramContractV1| {
                p.source_lineages[2].model_parameter_sha256 = ZERO_SHA256.to_owned()
            },
        ] {
            let mut record = population_record();
            mutate(record.contracts.population_program_v1.as_mut().unwrap());
            refresh_derived(&mut record);
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    #[test]
    fn population_program_seed_and_presence_gates_fail_closed() {
        let mut wrong_seed = population_record();
        wrong_seed
            .contracts
            .population_program_v1
            .as_mut()
            .unwrap()
            .expected_base_seed = 970_002;
        refresh_derived(&mut wrong_seed);
        assert_record_error(wrong_seed, TrainRunV2ErrorKind::InvalidLiteral);

        let mut no_environment = population_record();
        no_environment.environment.environment_randomization_v2 = None;
        assert!(validate_train_run_record_v2(no_environment).is_err());

        let mut no_ladder = population_record();
        no_ladder.contracts.opponent_policy.identity =
            FROZEN_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        no_ladder.contracts.opponent_policy.model_rule =
            FROZEN_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        no_ladder.contracts.opponent_ladder_pool = None;
        no_ladder.contracts.opponent_schedule_v2 = None;
        refresh_derived(&mut no_ladder);
        assert!(validate_train_run_record_v2(no_ladder).is_err());

        let mut no_initialization = population_record();
        no_initialization.contracts.opponent_ladder_initialization = None;
        refresh_derived(&mut no_initialization);
        assert!(validate_train_run_record_v2(no_initialization).is_err());
    }

    #[test]
    fn population_program_section_rejects_unknown_missing_and_null_fields() {
        let section = serde_json::to_value(population_program_fixture()).unwrap();
        let mut unknown = section.clone();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(1));
        assert!(serde_json::from_value::<PopulationProgramContractV1>(unknown).is_err());

        let mut missing = section.clone();
        missing.as_object_mut().unwrap().remove("source_lineages");
        assert!(serde_json::from_value::<PopulationProgramContractV1>(missing).is_err());

        let mut null = section;
        null.as_object_mut()
            .unwrap()
            .insert("program_update_count".to_owned(), Value::Null);
        assert!(serde_json::from_value::<PopulationProgramContractV1>(null).is_err());
    }

    #[test]
    fn response_exploiter_round_trips_and_binds_exact_update_1024_authority() {
        let record = response_exploiter_record_for_seed(971_001);
        assert_ne!(record.source.git_commit, POPULATION_PACKAGE_COMMIT_V1);
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let validated = decode_train_run_v2(&bytes).unwrap();
        let response = validated
            .record()
            .contracts()
            .response_exploiter_v1
            .as_ref()
            .unwrap();

        assert_eq!(validated.canonical_bytes(), bytes.as_slice());
        assert_eq!(response, &response_exploiter_fixture_for_seed(971_001));
        assert_eq!(validated.record().schedule.batch_episodes, 64);
        assert_eq!(validated.requested_successful_updates(), 256);
        assert_eq!(validated.record().schedule.checkpoint_segment_updates, 4);
        assert_eq!(
            validated.record().contracts().model.architecture_identity,
            FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2
        );
        assert!(validated.record().contracts().population_program_v1.is_none());
        assert!(validated.record().contracts().wide_model_experiment_v1.is_none());
        assert!(
            validated
                .record()
                .environment
                .environment_randomization_v2
                .is_some()
        );
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("\"response_exploiter_v1\":{"));
    }

    #[test]
    fn response_exploiter_builder_mints_both_authorized_seeds() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        for seed in RESPONSE_EXPLOITER_AUTHORIZED_BASE_SEEDS_V1 {
            let bytes =
                test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2(
                    NativeTrainingNumericalBackendV1::Sequential,
                    64,
                    4,
                    256,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    seed,
                    valid_ladder_pool_fixture(),
                    population_parent_initialization_fixture(),
                    RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1,
                );
            let validated = decode_train_run_v2(&bytes).unwrap();
            let response = validated
                .record()
                .contracts()
                .response_exploiter_v1
                .as_ref()
                .unwrap();
            assert_eq!(response.expected_base_seed, seed);
            assert_eq!(validated.record().schedule.base_seed, seed);
            assert_eq!(
                validated.record().schedule.batch_episodes
                    * validated.record().schedule.requested_successful_updates,
                256 * 64
            );
            assert!(validated.record().contracts().population_program_v1.is_none());
            assert_eq!(
                validated
                    .record()
                    .contracts()
                    .opponent_ladder_initialization,
                Some(population_parent_initialization_fixture())
            );
        }
    }

    #[test]
    fn response_exploiter_builder_mints_bounded_screen_seeds() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        for seed in RESPONSE_EXPLOITER_AUTHORIZED_SCREEN_SEEDS_V1 {
            let bytes =
                test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2(
                    NativeTrainingNumericalBackendV1::Sequential,
                    64,
                    4,
                    256,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    seed,
                    valid_ladder_pool_fixture(),
                    population_parent_initialization_fixture(),
                    RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1,
                );
            let validated = decode_train_run_v2(&bytes).unwrap();
            let response = validated
                .record()
                .contracts()
                .response_exploiter_v1
                .as_ref()
                .unwrap();
            assert_eq!(response.expected_base_seed, seed);
            assert_eq!(response.run_role, "screen");
            assert_eq!(
                response.expected_completion_generation,
                RESPONSE_EXPLOITER_SCREEN_COMPLETION_GENERATION_V1
            );
        }
    }

    #[test]
    fn response_exploiter_beta_authority_distinguishes_initial_screen_and_retry() {
        let initial = response_exploiter_record_for_seed(971_001);
        assert_eq!(
            initial
                .contracts
                .response_exploiter_v1
                .as_ref()
                .unwrap()
                .policy_anchor_beta_f32_bits,
            RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1
        );

        let mut retry = response_exploiter_record_for_seed(971_001);
        retry
            .contracts
            .response_exploiter_v1
            .as_mut()
            .unwrap()
            .policy_anchor_beta_f32_bits = RESPONSE_EXPLOITER_RETRY_BETA_F32_BITS_V1.to_owned();
        refresh_derived(&mut retry);
        validate_train_run_record_v2(retry).unwrap();

        let mut invalid_screen = response_exploiter_record_for_seed(971_091);
        invalid_screen
            .contracts
            .response_exploiter_v1
            .as_mut()
            .unwrap()
            .policy_anchor_beta_f32_bits = RESPONSE_EXPLOITER_RETRY_BETA_F32_BITS_V1.to_owned();
        refresh_derived(&mut invalid_screen);
        assert!(validate_train_run_record_v2(invalid_screen).is_err());
    }

    #[test]
    fn response_exploiter_absence_preserves_existing_bytes_and_population_behavior() {
        let legacy_bytes = fixture_bytes();
        let legacy = decode_train_run_v2(&legacy_bytes).unwrap();
        assert!(legacy.record().contracts().response_exploiter_v1.is_none());
        assert_eq!(
            sha256_hex(&legacy_bytes),
            "b99df8567b9ec40dff2d12db221c5e9af66d531c6dbf252dbf3eeae789387e8e"
        );
        assert!(!String::from_utf8(legacy_bytes)
            .unwrap()
            .contains("response_exploiter_v1"));

        let population_bytes = fixture_bytes_with_schedule_and_base_seed_population_environment_v2(
            crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1::Sequential,
            64,
            4,
            1_536,
            2,
            32,
            16,
            1_024,
            2_048,
            970_001,
            valid_ladder_pool_fixture(),
            population_parent_initialization_fixture(),
        );
        let population = decode_train_run_v2(&population_bytes).unwrap();
        assert!(population.record().contracts().population_program_v1.is_some());
        assert!(population.record().contracts().response_exploiter_v1.is_none());
    }

    #[test]
    fn response_exploiter_seed_schedule_parent_and_presence_gates_fail_closed() {
        let mut wrong_seed = response_exploiter_record_for_seed(971_001);
        wrong_seed.schedule.base_seed = 971_003;
        wrong_seed
            .contracts
            .response_exploiter_v1
            .as_mut()
            .unwrap()
            .expected_base_seed = 971_003;
        refresh_derived(&mut wrong_seed);
        assert!(validate_train_run_record_v2(wrong_seed).is_err());

        let mut wrong_batch = response_exploiter_record_for_seed(971_001);
        wrong_batch.schedule.batch_episodes = 32;
        wrong_batch.schedule.checkpoint_episode_interval = 32 * 4;
        refresh_derived(&mut wrong_batch);
        assert!(validate_train_run_record_v2(wrong_batch).is_err());

        let mut wrong_updates = response_exploiter_record_for_seed(971_001);
        wrong_updates.schedule.requested_successful_updates = 255;
        refresh_derived(&mut wrong_updates);
        assert!(validate_train_run_record_v2(wrong_updates).is_err());

        let mut wrong_segment = response_exploiter_record_for_seed(971_001);
        wrong_segment.schedule.checkpoint_segment_updates = 8;
        wrong_segment.schedule.checkpoint_episode_interval = 64 * 8;
        refresh_derived(&mut wrong_segment);
        assert!(validate_train_run_record_v2(wrong_segment).is_err());

        let mut no_environment = response_exploiter_record_for_seed(971_001);
        no_environment.environment.environment_randomization_v2 = None;
        assert!(validate_train_run_record_v2(no_environment).is_err());

        let mut no_pool = response_exploiter_record_for_seed(971_001);
        no_pool.contracts.opponent_ladder_pool = None;
        refresh_derived(&mut no_pool);
        assert!(validate_train_run_record_v2(no_pool).is_err());

        let mut no_initialization = response_exploiter_record_for_seed(971_001);
        no_initialization.contracts.opponent_ladder_initialization = None;
        refresh_derived(&mut no_initialization);
        assert!(validate_train_run_record_v2(no_initialization).is_err());

        let mut wrong_parent = response_exploiter_record_for_seed(971_001);
        wrong_parent
            .contracts
            .response_exploiter_v1
            .as_mut()
            .unwrap()
            .parent_generation = Some(383);
        wrong_parent
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .generation = 383;
        refresh_derived(&mut wrong_parent);
        assert!(validate_train_run_record_v2(wrong_parent).is_err());

        let mut simultaneous_population = response_exploiter_record_for_seed(971_001);
        simultaneous_population.contracts.population_program_v1 =
            Some(population_program_fixture());
        refresh_derived(&mut simultaneous_population);
        assert!(validate_train_run_record_v2(simultaneous_population).is_err());

        let mut simultaneous_wide = response_exploiter_record_for_seed(971_001);
        apply_wide_model_experiment(&mut simultaneous_wide);
        refresh_derived(&mut simultaneous_wide);
        assert!(validate_train_run_record_v2(simultaneous_wide).is_err());
    }

    #[test]
    fn response_exploiter_target_literals_hashes_and_vector_fail_closed() {
        let mutations: &[fn(&mut ResponseExploiterContractV1)] = &[
            |r| r.identity = "wrong".to_owned(),
            |r| r.package_commit = "0".repeat(40),
            |r| r.program_document_sha256 = ZERO_SHA256.to_owned(),
            |r| r.target_refresh_manifest_sha256 = ZERO_SHA256.to_owned(),
            |r| r.target_global_generation = 1_535,
            |r| r.source_refresh_index = 7,
            |r| r.source_program_update = 1_023,
            |r| r.active_slot_indices = [0, 1, 2, 3, 4, 6],
            |r| r.excluded_slot_indices = [5, 7],
            |r| r.renormalization_identity = "wrong".to_owned(),
            |r| r.effective_weight_units[0] += 1,
            |r| r.effective_weight_total_units -= 1,
            |r| r.training_update_count = 255,
            |r| r.episodes_per_update = 32,
            |r| r.reward_identity = "wrong".to_owned(),
            |r| r.fresh_adam_after_weight_init_identity = "wrong".to_owned(),
            |r| r.authorized_base_seeds = [971_001, 971_002, 971_101, 971_102, 971_003],
            |r| r.authorized_screen_seeds = [971_091, 971_092, 971_191, 971_003],
            |r| r.authorized_denovo_seeds = [971_202],
            |r| r.authorized_denovo_512_seeds = Some([971_299]),
            |r| r.expected_base_seed = 971_002,
            |r| r.run_role = "screen".to_owned(),
            |r| r.run_role = "denovo-screen".to_owned(),
            |r| r.expected_completion_generation = 4,
            |r| r.policy_anchor_beta_f32_bits = "3dccccce".to_owned(),
            |r| r.policy_anchor_beta_f32_bits = "00000000".to_owned(),
            |r| r.parent_source_run_sha256 = Some(ZERO_SHA256.to_owned()),
            |r| r.parent_generation = Some(383),
            |r| r.parent_checkpoint_sha256 = Some(ZERO_SHA256.to_owned()),
            |r| r.parent_sidecar_sha256 = Some(ZERO_SHA256.to_owned()),
            |r| r.parent_state_sha256 = Some(ZERO_SHA256.to_owned()),
            |r| r.parent_model_parameter_sha256 = Some(ZERO_SHA256.to_owned()),
            |r| r.parent_source_run_sha256 = None,
            |r| r.parent_generation = None,
            |r| r.parent_checkpoint_sha256 = None,
            |r| r.parent_sidecar_sha256 = None,
            |r| r.parent_state_sha256 = None,
            |r| r.parent_model_parameter_sha256 = None,
        ];
        for mutate in mutations {
            let mut record = response_exploiter_record_for_seed(971_001);
            mutate(record.contracts.response_exploiter_v1.as_mut().unwrap());
            refresh_derived(&mut record);
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    /// De-novo-screen mirror of
    /// `response_exploiter_target_literals_hashes_and_vector_fail_closed`:
    /// every field that must be role-specific for "denovo-screen" (role
    /// string, completion generation, beta bits, and each formerly-sentinel
    /// parent_* field flipping back to `Some`) rejects under one-at-a-time
    /// mutation of an otherwise-valid denovo record.
    #[test]
    fn response_exploiter_denovo_role_fields_fail_closed() {
        let mutations: &[fn(&mut ResponseExploiterContractV1)] = &[
            |r| r.run_role = "build".to_owned(),
            |r| r.run_role = "screen".to_owned(),
            |r| r.expected_completion_generation = 4,
            |r| r.policy_anchor_beta_f32_bits = RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1.to_owned(),
            |r| r.policy_anchor_beta_f32_bits = RESPONSE_EXPLOITER_RETRY_BETA_F32_BITS_V1.to_owned(),
            |r| r.fresh_adam_after_weight_init_identity = "wrong".to_owned(),
            |r| {
                r.parent_source_run_sha256 = Some(POPULATION_PARENT_SOURCE_RUN_SHA256_V1.to_owned())
            },
            |r| r.parent_generation = Some(POPULATION_PARENT_GENERATION_V1),
            |r| {
                r.parent_checkpoint_sha256 =
                    Some(POPULATION_PARENT_CHECKPOINT_SHA256_V1.to_owned())
            },
            |r| {
                r.parent_sidecar_sha256 = Some(POPULATION_PARENT_SIDECAR_SHA256_V1.to_owned())
            },
            |r| r.parent_state_sha256 = Some(POPULATION_PARENT_STATE_SHA256_V1.to_owned()),
            |r| {
                r.parent_model_parameter_sha256 =
                    Some(POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1.to_owned())
            },
        ];
        for mutate in mutations {
            let mut record = response_exploiter_denovo_record_for_seed(971_201);
            mutate(record.contracts.response_exploiter_v1.as_mut().unwrap());
            refresh_derived(&mut record);
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    /// De-novo-screen-512 mirror of
    /// `response_exploiter_denovo_role_fields_fail_closed` (Phase 2 horizon
    /// amendment, CLAUDE-DENOVO-SCREEN-SHEET-V1.md): the 512-update horizon
    /// extension shares every structural denovo requirement (no parent,
    /// beta=0) with the original 256-update role, but its own role string,
    /// completion generation, and training-update-count are role-specific
    /// (including against its immediate sibling "denovo-screen") and must
    /// reject a one-at-a-time mutation just like the 256-update role does.
    #[test]
    fn response_exploiter_denovo_512_role_fields_fail_closed() {
        let mutations: &[fn(&mut ResponseExploiterContractV1)] = &[
            |r| r.run_role = "build".to_owned(),
            |r| r.run_role = "screen".to_owned(),
            |r| r.run_role = "denovo-screen".to_owned(),
            |r| r.expected_completion_generation = RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
            |r| r.training_update_count = RESPONSE_EXPLOITER_TRAINING_UPDATE_COUNT_V1,
            |r| r.policy_anchor_beta_f32_bits = RESPONSE_EXPLOITER_INITIAL_BETA_F32_BITS_V1.to_owned(),
            |r| r.policy_anchor_beta_f32_bits = RESPONSE_EXPLOITER_RETRY_BETA_F32_BITS_V1.to_owned(),
            |r| r.fresh_adam_after_weight_init_identity = "wrong".to_owned(),
            |r| {
                r.parent_source_run_sha256 = Some(POPULATION_PARENT_SOURCE_RUN_SHA256_V1.to_owned())
            },
            |r| r.parent_generation = Some(POPULATION_PARENT_GENERATION_V1),
            |r| {
                r.parent_checkpoint_sha256 =
                    Some(POPULATION_PARENT_CHECKPOINT_SHA256_V1.to_owned())
            },
            |r| {
                r.parent_sidecar_sha256 = Some(POPULATION_PARENT_SIDECAR_SHA256_V1.to_owned())
            },
            |r| r.parent_state_sha256 = Some(POPULATION_PARENT_STATE_SHA256_V1.to_owned()),
            |r| {
                r.parent_model_parameter_sha256 =
                    Some(POPULATION_PARENT_MODEL_PARAMETER_SHA256_V1.to_owned())
            },
            // Backward-compatibility amendment: unlike every other role,
            // "denovo-screen-512" has no pre-amendment shape to fall back to
            // (the role did not exist before the field did), so an absent
            // array must still be rejected for this role specifically.
            |r| r.authorized_denovo_512_seeds = None,
            |r| r.authorized_denovo_512_seeds = Some([971_299]),
        ];
        for mutate in mutations {
            let mut record = response_exploiter_denovo_record_for_seed(971_202);
            mutate(record.contracts.response_exploiter_v1.as_mut().unwrap());
            refresh_derived(&mut record);
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    /// A "denovo-screen" record with the warm-start
    /// `opponent_ladder_initialization` section installed (the exact section
    /// "build"/"screen" require) must fail closed: presence, not just
    /// content, is role-conditional.
    #[test]
    fn response_exploiter_denovo_role_rejects_installed_initialization() {
        let mut record = response_exploiter_denovo_record_for_seed(971_201);
        record.contracts.opponent_ladder_initialization =
            Some(population_parent_initialization_fixture());
        refresh_derived(&mut record);
        assert!(validate_train_run_record_v2(record).is_err());
    }

    /// De-novo-screen-512 mirror: presence, not just content, of the
    /// warm-start `opponent_ladder_initialization` section is
    /// role-conditional for the 512-update horizon extension too.
    #[test]
    fn response_exploiter_denovo_512_role_rejects_installed_initialization() {
        let mut record = response_exploiter_denovo_record_for_seed(971_202);
        record.contracts.opponent_ladder_initialization =
            Some(population_parent_initialization_fixture());
        refresh_derived(&mut record);
        assert!(validate_train_run_record_v2(record).is_err());
    }

    /// The valid "denovo-screen" record (no parent, no initialization, beta
    /// bits 0.0, role/completion-generation matching seed 971_201) validates
    /// cleanly, matching the equivalent build/screen positive tests above.
    #[test]
    fn response_exploiter_denovo_role_validates_with_no_parent_and_no_initialization() {
        let record = response_exploiter_denovo_record_for_seed(971_201);
        let validated = validate_train_run_record_v2(record).unwrap();
        let response = validated
            .record()
            .contracts()
            .response_exploiter_v1
            .as_ref()
            .unwrap();
        assert_eq!(response.run_role, "denovo-screen");
        assert_eq!(response.expected_completion_generation, 256);
        assert_eq!(response.policy_anchor_beta_f32_bits, "00000000");
        assert!(response.parent_source_run_sha256.is_none());
        assert!(response.parent_generation.is_none());
        assert!(response.parent_checkpoint_sha256.is_none());
        assert!(response.parent_sidecar_sha256.is_none());
        assert!(response.parent_state_sha256.is_none());
        assert!(response.parent_model_parameter_sha256.is_none());
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_initialization
                .is_none()
        );
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_pool
                .is_some()
        );
    }

    /// The valid "denovo-screen-512" record (Phase 2 horizon amendment; no
    /// parent, no initialization, beta bits 0.0, role/completion-generation/
    /// training-update-count matching seed 971_202) validates cleanly,
    /// matching the equivalent 256-update positive test above.
    #[test]
    fn response_exploiter_denovo_512_role_validates_with_no_parent_and_no_initialization() {
        let record = response_exploiter_denovo_record_for_seed(971_202);
        let validated = validate_train_run_record_v2(record).unwrap();
        let response = validated
            .record()
            .contracts()
            .response_exploiter_v1
            .as_ref()
            .unwrap();
        assert_eq!(response.run_role, "denovo-screen-512");
        assert_eq!(response.expected_completion_generation, 512);
        assert_eq!(response.training_update_count, 512);
        assert_eq!(response.policy_anchor_beta_f32_bits, "00000000");
        assert!(response.parent_source_run_sha256.is_none());
        assert!(response.parent_generation.is_none());
        assert!(response.parent_checkpoint_sha256.is_none());
        assert!(response.parent_sidecar_sha256.is_none());
        assert!(response.parent_state_sha256.is_none());
        assert!(response.parent_model_parameter_sha256.is_none());
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_initialization
                .is_none()
        );
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_pool
                .is_some()
        );
    }

    /// Builder round-trip for the authorized denovo seed, mirroring
    /// `response_exploiter_builder_mints_bounded_screen_seeds`: the omitted
    /// parent_* fields must not appear in the canonical bytes at all
    /// (`skip_serializing_if`, not `null`).
    #[test]
    fn response_exploiter_denovo_builder_mints_authorized_seed() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        for seed in RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_SEEDS_V1 {
            let bytes =
                test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_denovo_environment_v2(
                    NativeTrainingNumericalBackendV1::Sequential,
                    64,
                    4,
                    256,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    seed,
                    valid_ladder_pool_fixture(),
                );
            let validated = decode_train_run_v2(&bytes).unwrap();
            let response = validated
                .record()
                .contracts()
                .response_exploiter_v1
                .as_ref()
                .unwrap();
            assert_eq!(response.expected_base_seed, seed);
            assert_eq!(response.run_role, "denovo-screen");
            assert!(validated.record().contracts().population_program_v1.is_none());
            assert_eq!(
                validated.record().contracts().opponent_ladder_initialization,
                None
            );
            let text = String::from_utf8(bytes).unwrap();
            assert!(!text.contains("parent_source_run_sha256"));
            assert!(!text.contains("parent_generation"));
            assert!(!text.contains("parent_checkpoint_sha256"));
            assert!(!text.contains("parent_sidecar_sha256"));
            assert!(!text.contains("parent_state_sha256"));
            assert!(!text.contains("parent_model_parameter_sha256"));
        }
    }

    /// Builder round-trip for the authorized denovo-512 seed (Phase 2
    /// horizon amendment), mirroring
    /// `response_exploiter_denovo_builder_mints_authorized_seed`: same
    /// builder function (it already dispatches "denovo-screen" vs
    /// "denovo-screen-512" by seed membership), just called with the 512
    /// horizon's own seed and requested-update count.
    #[test]
    fn response_exploiter_denovo_512_builder_mints_authorized_seed() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        for seed in RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1 {
            let bytes =
                test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_denovo_environment_v2(
                    NativeTrainingNumericalBackendV1::Sequential,
                    64,
                    4,
                    512,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    seed,
                    valid_ladder_pool_fixture(),
                );
            let validated = decode_train_run_v2(&bytes).unwrap();
            let response = validated
                .record()
                .contracts()
                .response_exploiter_v1
                .as_ref()
                .unwrap();
            assert_eq!(response.expected_base_seed, seed);
            assert_eq!(response.run_role, "denovo-screen-512");
            assert_eq!(response.expected_completion_generation, 512);
            assert!(validated.record().contracts().population_program_v1.is_none());
            assert_eq!(
                validated.record().contracts().opponent_ladder_initialization,
                None
            );
            let text = String::from_utf8(bytes).unwrap();
            assert!(!text.contains("parent_source_run_sha256"));
            assert!(!text.contains("parent_generation"));
            assert!(!text.contains("parent_checkpoint_sha256"));
            assert!(!text.contains("parent_sidecar_sha256"));
            assert!(!text.contains("parent_state_sha256"));
            assert!(!text.contains("parent_model_parameter_sha256"));
        }
    }

    #[test]
    fn response_exploiter_section_rejects_unknown_missing_and_null_fields() {
        let section = serde_json::to_value(response_exploiter_fixture_for_seed(971_001)).unwrap();

        let mut unknown = section.clone();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(1));
        assert!(serde_json::from_value::<ResponseExploiterContractV1>(unknown).is_err());

        let mut missing = section.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("effective_weight_units");
        assert!(serde_json::from_value::<ResponseExploiterContractV1>(missing).is_err());

        let mut null = section;
        null.as_object_mut()
            .unwrap()
            .insert("target_refresh_manifest_sha256".to_owned(), Value::Null);
        assert!(serde_json::from_value::<ResponseExploiterContractV1>(null).is_err());
    }

    #[test]
    fn ladder_identity_with_valid_pool_validates() {
        let record = ladder_record();
        let validated = validate_train_run_record_v2(record).unwrap();
        assert_eq!(
            validated.record().contracts().opponent_ladder_pool,
            Some(valid_ladder_pool_fixture())
        );
    }

    #[test]
    fn ladder_pool_round_trips_through_canonical_bytes() {
        let record = ladder_record();
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());
        assert_eq!(
            validated.record().contracts().opponent_ladder_pool,
            Some(valid_ladder_pool_fixture())
        );
        assert_eq!(
            validated.record().contracts().opponent_schedule_v2,
            Some(valid_opponent_schedule_v2_fixture())
        );
        // The pool key sorts between "model" and "opponent_policy"; the
        // schedule key sorts after "opponent_sampler" (alphabetical: "s" <
        // "sa" < "sc" residues of "sampler" vs "schedule_v2").
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"opponent_ladder_pool\":{"));
        assert!(text.contains("\"opponent_schedule_v2\":{"));
        let pool_pos = text.find("\"opponent_ladder_pool\":{").unwrap();
        let policy_pos = text.find("\"opponent_policy\":{").unwrap();
        let sampler_pos = text.find("\"opponent_sampler\":{").unwrap();
        let schedule_pos = text.find("\"opponent_schedule_v2\":{").unwrap();
        assert!(pool_pos < policy_pos);
        assert!(policy_pos < sampler_pos);
        assert!(sampler_pos < schedule_pos);
    }

    #[test]
    fn ladder_identity_without_pool_fails_closed() {
        let mut record = fixture_record();
        record.contracts.opponent_policy.identity =
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2.to_owned();
        record.contracts.opponent_policy.model_rule =
            FROZEN_LADDER_OPPONENT_POLICY_MODEL_RULE_V2.to_owned();
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn uniform_identity_with_pool_present_fails_closed() {
        let mut record = fixture_record();
        record.contracts.opponent_ladder_pool = Some(valid_ladder_pool_fixture());
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    // ------------------------------------------------------------------
    // Continual-initialization section (Amendment 1 / Section 8A point 2).
    // ------------------------------------------------------------------

    fn valid_ladder_initialization_fixture() -> OpponentLadderInitializationContractV1 {
        OpponentLadderInitializationContractV1 {
            source_run_sha256: hex64('d'),
            generation: 32,
            checkpoint_sha256: hex64('d'),
            sidecar_sha256: hex64('d'),
            state_sha256: hex64('d'),
            derived_model_parameter_sha256: hex64('e'),
        }
    }

    fn ladder_record_with_init() -> TrainRunV2 {
        let mut record = ladder_record();
        record.contracts.opponent_ladder_initialization =
            Some(valid_ladder_initialization_fixture());
        refresh_derived(&mut record);
        record
    }

    #[test]
    fn ladder_identity_with_init_present_validates() {
        let record = ladder_record_with_init();
        let validated = validate_train_run_record_v2(record).unwrap();
        assert_eq!(
            validated
                .record()
                .contracts()
                .opponent_ladder_initialization,
            Some(valid_ladder_initialization_fixture())
        );
    }

    #[test]
    fn ladder_identity_without_init_still_validates() {
        // `ladder_record()` (used throughout this file's other ladder
        // tests) never sets the init section: this is the fresh-init shape,
        // which MUST keep validating (Amendment 1's explicit requirement).
        let record = ladder_record();
        let validated = validate_train_run_record_v2(record).unwrap();
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_initialization
                .is_none()
        );
    }

    #[test]
    fn ladder_init_round_trips_through_canonical_bytes() {
        let record = ladder_record_with_init();
        let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let validated = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());
        assert_eq!(
            validated
                .record()
                .contracts()
                .opponent_ladder_initialization,
            Some(valid_ladder_initialization_fixture())
        );
        // The init key sorts between "opponent_ladder_pool" (shorter,
        // "opponent_ladder_i..." < "opponent_ladder_p...") and
        // "opponent_policy".
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"opponent_ladder_initialization\":{"));
        let init_pos = text.find("\"opponent_ladder_initialization\":{").unwrap();
        let pool_pos = text.find("\"opponent_ladder_pool\":{").unwrap();
        let policy_pos = text.find("\"opponent_policy\":{").unwrap();
        assert!(init_pos < pool_pos);
        assert!(pool_pos < policy_pos);
    }

    #[test]
    fn uniform_identity_with_init_present_fails_closed() {
        let mut record = fixture_record();
        record.contracts.opponent_ladder_initialization =
            Some(valid_ladder_initialization_fixture());
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn ladder_init_shape_corruption_matrix_fails_closed() {
        let mut cases = Vec::new();

        let mut record = ladder_record_with_init();
        record
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .source_run_sha256 = "not-a-sha256".to_owned();
        cases.push(record);

        let mut record = ladder_record_with_init();
        record
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .generation = u64::MAX;
        cases.push(record);

        let mut record = ladder_record_with_init();
        record
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .checkpoint_sha256 = "not-a-sha256".to_owned();
        cases.push(record);

        let mut record = ladder_record_with_init();
        record
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .sidecar_sha256 = "not-a-sha256".to_owned();
        cases.push(record);

        let mut record = ladder_record_with_init();
        record
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .state_sha256 = "not-a-sha256".to_owned();
        cases.push(record);

        let mut record = ladder_record_with_init();
        record
            .contracts
            .opponent_ladder_initialization
            .as_mut()
            .unwrap()
            .derived_model_parameter_sha256 = "not-a-sha256".to_owned();
        cases.push(record);

        for record in cases {
            assert_record_error(record, TrainRunV2ErrorKind::InvalidScalar);
        }
    }

    #[test]
    fn ladder_identity_without_schedule_fails_closed() {
        let mut record = ladder_record();
        record.contracts.opponent_schedule_v2 = None;
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn uniform_identity_with_schedule_present_fails_closed() {
        let mut record = fixture_record();
        record.contracts.opponent_schedule_v2 = Some(valid_opponent_schedule_v2_fixture());
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn opponent_schedule_v2_corruption_matrix_fails_closed() {
        let mut cases = Vec::new();

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .schedule_version = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .seed_version = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .opponent_pool_choice_namespace = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .opponent_policy_substep_fields = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .pool_choice_modulo = 99;
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .pool_choice_threshold_rule = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .pool_choice_bias_rule = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_schedule_v2
            .as_mut()
            .unwrap()
            .version_change_rule = "wrong".to_owned();
        cases.push(record);

        for record in cases {
            assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
        }
    }

    #[test]
    fn ladder_identity_with_mismatched_model_rule_fails_closed() {
        let mut record = ladder_record();
        record.contracts.opponent_policy.model_rule = "some-other-rule".to_owned();
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn unknown_opponent_policy_identity_fails_closed() {
        let mut record = fixture_record();
        record.contracts.opponent_policy.identity = "some-unknown-identity".to_owned();
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    #[test]
    fn ladder_pool_weight_and_identity_corruption_matrix_fails_closed() {
        let mut cases = Vec::new();

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .weight_primary = 41;
        cases.push(record);

        let mut record = ladder_record();
        record.contracts.opponent_ladder_pool.as_mut().unwrap().size = 5;
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .identity = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .policy_member_sampling_rule = "wrong".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .uniform_floor
            .identity = "wrong".to_owned();
        cases.push(record);

        for record in cases {
            assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
        }
    }

    #[test]
    fn ladder_pool_checkpoint_ref_scalar_corruption_matrix_fails_closed() {
        let mut cases = Vec::new();

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .primary
            .source_run_sha256 = "not-hex".to_owned();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .primary
            .generation = U63_MAX + 1;
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .predecessor_a
            .checkpoint_sha256 = hex64('a').to_ascii_uppercase();
        cases.push(record);

        let mut record = ladder_record();
        record
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .predecessor_b
            .sidecar_sha256 = "short".to_owned();
        cases.push(record);

        for record in cases {
            assert!(validate_train_run_record_v2(record).is_err());
        }
    }

    // ------------------------------------------------------------------
    // Deliverable 2(d): mid-run opponent-swap unrepresentability (contract
    // Section 7 fixture (d), and the contract's core structural claim in
    // Section 1: "a mid-run opponent change is structurally
    // unrepresentable ... store segment boundary validation rejects any
    // segment whose run_sha256 differs from the run root"). The opponent
    // pool is part of what `run_sha256` hashes, so two ladder-shaped
    // records differing in exactly one pool digest have DIFFERENT
    // `run_sha256` values; a checkpoint minted while bound to the first
    // record's identity is rejected when validated against the second
    // record. This mirrors the pattern
    // `native_training_store_boundary_v2`'s own tests use to prove a
    // checkpoint/boundary rejects a mismatched run authority (decode
    // against the WRONG `ValidatedTrainRunV2` and expect a CrossBinding-
    // class rejection), applied here at the checkpoint-manifest layer
    // against two ladder run records instead of one uniform record plus a
    // hand-corrupted JSON `Value`.
    // ------------------------------------------------------------------

    fn ladder_execution_config_v1(
        run: &ValidatedTrainRunV2,
    ) -> crate::native_training_executor_v1::NativeTrainingExecutionConfigV1 {
        crate::native_training_executor_v1::NativeTrainingExecutionConfigV1 {
            run_base_seed: run.record().schedule.base_seed,
            batch_episodes: run.batch_episodes(),
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            max_physical_decisions: run.record().limits.max_physical_decisions,
            max_policy_steps: run.record().limits.max_policy_steps,
            worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
            sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker)
                .unwrap(),
            broker_batch_target: usize::try_from(run.record().topology.broker_batch_target)
                .unwrap(),
            scheduler_timeout: std::time::Duration::from_secs(30),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5_f32.to_bits(),
            learning_rate_bits: 0.001_f32.to_bits(),
            numerical_backend:
                crate::native_training_executor_v1::NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    #[test]
    fn mid_run_opponent_pool_swap_is_structurally_unrepresentable() {
        use crate::native_training_executor_v1::NativeTrainingExecutorV1;
        use crate::native_training_store_checkpoint_v3::{
            build_genesis_checkpoint_manifest_v3, decode_genesis_checkpoint_manifest_v3,
        };

        // Two valid ladder-shaped records differing in EXACTLY one pool
        // digest (the primary member's checkpoint_sha256).
        let mut record_a = ladder_record();
        refresh_derived(&mut record_a);
        let mut record_b = ladder_record();
        record_b
            .contracts
            .opponent_ladder_pool
            .as_mut()
            .unwrap()
            .primary
            .checkpoint_sha256 = hex64('9');
        refresh_derived(&mut record_b);

        let run_a = validate_train_run_record_v2(record_a).unwrap();
        let run_b = validate_train_run_record_v2(record_b).unwrap();
        assert_ne!(
            run_a.run_sha256(),
            run_b.run_sha256(),
            "a one-digest pool change must change run_sha256"
        );

        // Mint one real genesis checkpoint bound to run_a's identity.
        let (snapshot_manifest, snapshot_payload) =
            crate::common_model_snapshot_v1::common_model_snapshot_paths_v1();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            ladder_execution_config_v1(&run_a),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let payload = executor
            .checkpoint_candidate_v1()
            .unwrap()
            .payload()
            .to_vec();
        let checkpoint_a = build_genesis_checkpoint_manifest_v3(&run_a, &payload).unwrap();
        let manifest_bytes = checkpoint_a.canonical_bytes().to_vec();

        // Sanity: the checkpoint validates against the run it was minted
        // under (run_a).
        assert!(decode_genesis_checkpoint_manifest_v3(&manifest_bytes, &payload, &run_a).is_ok());

        // The same checkpoint, claiming run_a's run_sha256, is REJECTED
        // when validated against run_b: a mid-run opponent-pool swap cannot
        // be represented by any valid checkpoint under the new identity.
        let rejected = decode_genesis_checkpoint_manifest_v3(&manifest_bytes, &payload, &run_b);
        assert!(
            rejected.is_err(),
            "a checkpoint claiming run_a's run_sha256 must be rejected against run_b"
        );
        assert_eq!(
            rejected.unwrap_err().kind(),
            crate::native_training_store_checkpoint_v3::CheckpointManifestV3ErrorKind::CrossBinding
        );
    }

    // ------------------------------------------------------------------
    // Deliverable 3: the ladder-variant test fixture builder (pilot runner
    // integration). Proves the new function decodes to a ladder-shaped
    // record carrying the caller-supplied pool, AND that the existing
    // uniform builder it sits beside is untouched (same schedule/topology
    // inputs, uniform identity, no ladder sections) -- the fixture stays
    // byte-identical because nothing in its own body changed.
    // ------------------------------------------------------------------

    #[test]
    fn ladder_fixture_builder_adds_ladder_sections_uniform_builder_stays_unaffected() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        let schedule_args = (
            2_u64, 4_u64, 4_u64, 2_u64, 4_u64, 8_u64, 32_768_u64, 65_536_u64,
        );
        let base_seed = 909_909_u64;

        let uniform_bytes = test_fixture_bytes_with_schedule_and_base_seed_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            schedule_args.0,
            schedule_args.1,
            schedule_args.2,
            schedule_args.3,
            schedule_args.4,
            schedule_args.5,
            schedule_args.6,
            schedule_args.7,
            base_seed,
        );
        let uniform = decode_train_run_v2(&uniform_bytes).unwrap();
        assert!(uniform.record().contracts().opponent_ladder_pool.is_none());
        assert!(uniform.record().contracts().opponent_schedule_v2.is_none());

        let pool = valid_ladder_pool_fixture();
        let ladder_bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            schedule_args.0,
            schedule_args.1,
            schedule_args.2,
            schedule_args.3,
            schedule_args.4,
            schedule_args.5,
            schedule_args.6,
            schedule_args.7,
            base_seed,
            pool.clone(),
        );
        let ladder = decode_train_run_v2(&ladder_bytes).unwrap();
        assert_eq!(ladder.record().contracts().opponent_ladder_pool, Some(pool));
        assert!(ladder.record().contracts().opponent_schedule_v2.is_some());
        assert_ne!(uniform_bytes, ladder_bytes);
        assert_ne!(uniform.run_sha256(), ladder.run_sha256());

        // Same schedule/topology/base-seed inputs are preserved identically
        // by both builders (only the opponent contracts section differs).
        assert_eq!(
            uniform.record().schedule.base_seed,
            ladder.record().schedule.base_seed
        );
        assert_eq!(
            uniform.record().topology.worker_count,
            ladder.record().topology.worker_count
        );

        // The uniform builder's own output is exactly what it always was:
        // re-deriving it a second time (independent call) is bit-identical.
        let uniform_bytes_again = test_fixture_bytes_with_schedule_and_base_seed_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            schedule_args.0,
            schedule_args.1,
            schedule_args.2,
            schedule_args.3,
            schedule_args.4,
            schedule_args.5,
            schedule_args.6,
            schedule_args.7,
            base_seed,
        );
        assert_eq!(uniform_bytes, uniform_bytes_again);
    }

    #[test]
    fn environment_v2_composes_with_uniform_ladder_and_ladder_init_records() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        let args = (
            64_u64, 4_u64, 8_u64, 2_u64, 32_u64, 16_u64, 1_024_u64, 2_048_u64,
        );
        let base_seed = 970_001_u64;
        let pool = valid_ladder_pool_fixture();
        let initialization = valid_ladder_initialization_fixture();

        let legacy_ladder_bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            args.5,
            args.6,
            args.7,
            base_seed,
            pool.clone(),
        );
        let legacy_ladder = decode_train_run_v2(&legacy_ladder_bytes).unwrap();
        assert_eq!(
            legacy_ladder.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1
        );

        let environment_bytes = test_fixture_bytes_with_schedule_and_base_seed_environment_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            args.5,
            args.6,
            args.7,
            base_seed,
        );
        let environment = decode_train_run_v2(&environment_bytes).unwrap();
        assert_eq!(
            environment.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        assert!(
            environment
                .record()
                .contracts()
                .opponent_ladder_pool
                .is_none()
        );

        let ladder_environment_bytes =
            test_fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                base_seed,
                pool.clone(),
            );
        let ladder_environment = decode_train_run_v2(&ladder_environment_bytes).unwrap();
        assert_eq!(
            ladder_environment.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        assert_eq!(
            ladder_environment.record().contracts().opponent_ladder_pool,
            Some(pool.clone())
        );
        assert!(
            ladder_environment
                .record()
                .contracts()
                .opponent_ladder_initialization
                .is_none()
        );

        let ladder_init_environment_bytes =
            test_fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                base_seed,
                pool,
                initialization.clone(),
            );
        let ladder_init_environment = decode_train_run_v2(&ladder_init_environment_bytes).unwrap();
        assert_eq!(
            ladder_init_environment.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        assert_eq!(
            ladder_init_environment
                .record()
                .contracts()
                .opponent_ladder_initialization,
            Some(initialization)
        );

        assert_ne!(legacy_ladder_bytes, ladder_environment_bytes);
        assert_eq!(
            legacy_ladder_bytes,
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                base_seed,
                valid_ladder_pool_fixture(),
            )
        );
    }

    /// HARD CONSTRAINT regression (S2 ladder contract Deliverable 1): every
    /// EXISTING uniform run record's canonical bytes, and therefore its
    /// `run_sha256`, must remain bit-identical after adding the optional
    /// `opponent_ladder_pool` section. This reads a REAL, already-published
    /// S1 run record (read-only; not a fixture, not modified by this test)
    /// and proves the updated decoder still accepts it and recomputes the
    /// exact same `run_sha256` the store already published for it (the same
    /// value the checkpoint sidecar `update-00000000.checkpoint.json` in
    /// that store binds against). This test depends on that external
    /// evidence directory remaining present on this machine.
    #[test]
    fn real_s1_mirror_run_json_validates_with_unchanged_run_sha256() {
        const REAL_RUN_JSON_PATH: &str =
            r"D:\mtg-kernel-s1-mirror-20260724\dev0\run-0\store\run.json";
        // Independently confirmed via `certutil -hashfile run.json SHA256`
        // and cross-checked against the `run_sha256` field stored in that
        // store's `checkpoints\update-00000000.checkpoint.json` sidecar.
        const STORED_RUN_SHA256: &str =
            "47bc46634de718439ea93fbad105cbf96a6339913856805dccca87773760e7ef";

        let bytes = std::fs::read(REAL_RUN_JSON_PATH).unwrap_or_else(|error| {
            panic!("could not read the real S1 mirror run.json fixture at {REAL_RUN_JSON_PATH}: {error}")
        });
        assert_eq!(sha256_hex(&bytes), STORED_RUN_SHA256);

        let validated = decode_train_run_v2(&bytes)
            .unwrap_or_else(|error| panic!("real S1 mirror run.json failed validation: {error:?}"));
        assert_eq!(validated.run_sha256(), STORED_RUN_SHA256);
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());

        // Dual-Profile Catalog Successor (collab CLAUDE #220): this real,
        // pre-nine-deck store must classify HISTORICAL, not merely decode.
        assert_eq!(
            validated.catalog_profile_v1(),
            NativeRunCatalogProfileV1::Historical
        );

        // This is a uniform-identity run: the ladder pool section must be
        // absent, and the uniform identities validate exactly as before.
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_pool
                .is_none()
        );
        assert_eq!(
            validated.record().contracts().opponent_policy.identity,
            FROZEN_OPPONENT_POLICY_IDENTITY_V2
        );
    }

    /// HARD CONSTRAINT regression (Amendment 1 / Section 8A point 2): the
    /// pilot's real, already-published FRESH-INIT ladder run record (no
    /// `opponent_ladder_initialization` section -- the historical shape
    /// this amendment's optional field must keep validating forever) reads
    /// and recomputes the exact same `run_sha256` the store already
    /// published for it, unaffected by adding the new optional section.
    /// Companion to `real_s1_mirror_run_json_validates_with_unchanged_run_sha256`
    /// above (the uniform-identity regression), which must also stay green.
    /// This test depends on that external evidence directory remaining
    /// present on this machine.
    #[test]
    fn real_ladder_pilot_run_json_validates_with_unchanged_run_sha256() {
        const REAL_RUN_JSON_PATH: &str =
            r"D:\mtg-kernel-ladder-pilot-20260725\runs\dev0\run-0\store\run.json";
        // Independently confirmed via `sha256sum run.json` and cross-checked
        // against the `run_sha256` field stored in that store's
        // `checkpoints\update-00000000.checkpoint.json` sidecar.
        const STORED_RUN_SHA256: &str =
            "6a78ae91b616c8f42ccfe9907ff82bdc1b0cd8ed693fd19bcc9e0783ba71e425";

        let bytes = std::fs::read(REAL_RUN_JSON_PATH).unwrap_or_else(|error| {
            panic!(
                "could not read the real ladder pilot run.json fixture at {REAL_RUN_JSON_PATH}: {error}"
            )
        });
        assert_eq!(sha256_hex(&bytes), STORED_RUN_SHA256);

        let validated = decode_train_run_v2(&bytes).unwrap_or_else(|error| {
            panic!("real ladder pilot run.json failed validation: {error:?}")
        });
        assert_eq!(validated.run_sha256(), STORED_RUN_SHA256);
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());

        // Dual-Profile Catalog Successor (collab CLAUDE #220): this real,
        // pre-nine-deck store must classify HISTORICAL, not merely decode.
        assert_eq!(
            validated.catalog_profile_v1(),
            NativeRunCatalogProfileV1::Historical
        );

        // This is a ladder-identity, FRESH-INIT run: the pool section is
        // present, but the init section is absent (fresh init from the
        // common model snapshot -- the shape this amendment's field must
        // not disturb).
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_pool
                .is_some()
        );
        assert!(
            validated
                .record()
                .contracts()
                .opponent_ladder_initialization
                .is_none()
        );
        assert_eq!(
            validated.record().contracts().opponent_policy.identity,
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2
        );
    }

    /// Dual-Profile Catalog Successor (collab CLAUDE #220) acceptance
    /// evidence: the coordinator-designated ladder pilot store root
    /// (`pool3\primary`, distinct from the `runs\dev0\run-0` leg the
    /// pre-existing regression above reads) decodes clean, read-only, and
    /// classifies HISTORICAL. Independently confirms the dual-profile
    /// mechanism against a second real leg of the same evidence tree. This
    /// test depends on that external evidence directory remaining present on
    /// this machine, and never writes to it.
    #[test]
    fn real_ladder_pilot_pool3_primary_run_json_decodes_historical() {
        const REAL_RUN_JSON_PATH: &str =
            r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary\run.json";

        let bytes = std::fs::read(REAL_RUN_JSON_PATH).unwrap_or_else(|error| {
            panic!(
                "could not read the real ladder pilot pool3/primary run.json at {REAL_RUN_JSON_PATH}: {error}"
            )
        });

        let validated = decode_train_run_v2(&bytes).unwrap_or_else(|error| {
            panic!("real ladder pilot pool3/primary run.json failed validation: {error:?}")
        });
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());
        assert_eq!(
            validated.catalog_profile_v1(),
            NativeRunCatalogProfileV1::Historical
        );
        assert_eq!(
            validated.record().environment.card_db_hash_u64_hex,
            FROZEN_CARD_DB_HASH_U64_HEX_V2
        );
        assert_eq!(
            validated.record().environment.runtime_catalog_sha256,
            FROZEN_RUNTIME_CATALOG_SHA256_V2
        );
    }

    /// Dual-Profile Catalog Successor (collab CLAUDE #220) fix round,
    /// panel finding 2 (compat blocker): empirical evidence from the two
    /// real, active population-v2 records named by the coordinator. Two
    /// independent facts, each locked in by its own assertion below:
    ///
    /// (1) Both records' own `card_db_hash_u64_hex`/`runtime_catalog_sha256`
    /// fields are read directly off the raw JSON (bypassing
    /// `decode_train_run_v2`, which cannot reach them -- see fact 2) and are
    /// BIT-IDENTICAL to the existing HISTORICAL frozen literals
    /// (`FROZEN_CARD_DB_HASH_U64_HEX_V2`/`FROZEN_RUNTIME_CATALOG_SHA256_V2`).
    /// There is no third, distinct catalog-hash value here: the population-v2
    /// worktree that produced both records never advanced past the rev3
    /// (two-deck) catalog identity, even though both records were minted
    /// chronologically after the runtime-decks-nine landing on this branch's
    /// base. `classify_catalog_profile_v1` already classifies this exact
    /// value pair correctly (as `Historical`); no new frozen literal pair
    /// exists to register.
    ///
    /// (2) `decode_train_run_v2` currently fails on BOTH real files with
    /// `CanonicalJson(Deserialization)`, not `InvalidLiteral` and not any
    /// catalog-profile-related classification. Root cause (confirmed by
    /// direct inspection, not guessed): both records' `contracts` object
    /// carries a population-v2-era section this branch's `TrainRunContractsV2`
    /// does not define at all (`population_program_v2_cycle2` in the cycle-2
    /// record, `population_program_v2` in the tranche-1 record; this branch
    /// only defines `population_program_v1`), so `deny_unknown_fields`
    /// rejects the record before canonical-JSON deserialization completes --
    /// before `validate_environment_v2` or `classify_catalog_profile_v1` are
    /// ever reached. This is a separate, pre-existing schema-generation gap
    /// (the population-v2 contract-widening lane, collab CLAUDE #226: "the
    /// v2 implementation items... land FIRST, before your run_v2 catalog
    /// successor rebases onto those files"), not something this dual-profile
    /// catalog work introduces or can fix by itself.
    ///
    /// Consequence, flagged for the coordinator rather than guessed at:
    /// neither prescribed outcome (equals CURRENT; or differs and gets a new
    /// third frozen literal pair) applies -- the observed value equals
    /// HISTORICAL exactly. Once the population-v2 schema widening lands and
    /// these records become decodable on some future branch, they will
    /// classify `Historical` under this design and be rejected at the
    /// science-loop/publish/resume boundaries exactly as the panel warns.
    /// That is a real, valid forward-looking concern; resolving it now would
    /// mean inventing a discriminating signal from schema this branch does
    /// not have, which risks being wrong. This test locks in the current,
    /// verified-true state as a tripwire: if it starts failing (either
    /// assertion), that is the signal the schema widening has landed and the
    /// boundary policy for population-v2's specific case needs revisiting
    /// with real decodable evidence in hand.
    #[test]
    fn population_v2_active_records_are_historical_catalog_identity_blocked_by_a_separate_schema_gap()
    {
        for (label, path) in [
            (
                "cycle2",
                r"C:\mtg-kernel-population-v2-cycle2\active\cycle2-active-interval-0256-0384\attempt-001\seed-975001-store\run-0\store\run.json",
            ),
            (
                "tranche1",
                r"D:\mtg-kernel-population-v2-tranche1\active\active-interval-0000-0128\attempt-006\seed-972001-store\run-0\store\run.json",
            ),
        ] {
            let bytes = std::fs::read(path).unwrap_or_else(|error| {
                panic!("could not read the real population-v2 {label} run.json at {path}: {error}")
            });

            // Fact 1: the catalog identity fields, read directly off the raw
            // JSON, equal the HISTORICAL pin exactly. No decode needed.
            let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                raw["environment"]["card_db_hash_u64_hex"].as_str().unwrap(),
                FROZEN_CARD_DB_HASH_U64_HEX_V2,
                "{label}: catalog card_db_hash_u64_hex is not the historical pin"
            );
            assert_eq!(
                raw["environment"]["runtime_catalog_sha256"]
                    .as_str()
                    .unwrap(),
                FROZEN_RUNTIME_CATALOG_SHA256_V2,
                "{label}: catalog runtime_catalog_sha256 is not the historical pin"
            );

            // Fact 2: decode fails at canonical-JSON deserialization (the
            // unknown population-v2 contract field), never reaching catalog
            // classification at all.
            let error = decode_train_run_v2(&bytes)
                .expect_err(&format!("{label}: expected decode to fail (schema gap)"));
            assert_eq!(
                error.kind(),
                TrainRunV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization),
                "{label}: expected the unknown-field schema gap, got a different failure"
            );
        }
    }

    /// Backward-compatibility regression for the Phase 2 512-horizon
    /// amendment: this reads the REAL, already-published denovo-screen-256
    /// store's run.json (read-only; not a fixture, not modified by this
    /// test) and proves it decodes and recomputes the exact same
    /// `run_sha256` the store already published for it. Before the fix in
    /// this commit, `authorized_denovo_512_seeds` was an unconditional,
    /// always-present field, and this exact record (minted before that
    /// field existed) failed to decode at all
    /// (`missing field authorized_denovo_512_seeds`) -- a real
    /// backward-compatibility defect, not intended fail-closed behavior.
    /// This test depends on that external evidence directory remaining
    /// present on this machine.
    #[test]
    fn real_denovo_screen_256_run_json_decodes_after_backward_compatibility_fix() {
        const REAL_RUN_JSON_PATH: &str = r"D:\mtg-kernel-denovo-screen-v1\denovo-screen-build\attempt-002\denovo-store\run-0\store\run.json";
        // Independently confirmed via `certutil -hashfile run.json SHA256`
        // and cross-checked against the `run_sha256` field stored in that
        // store's `latest.json` pointer.
        const STORED_RUN_SHA256: &str =
            "8d98ee5411e2407af7530421d2eac44cfdf3a6b0198b9ab898caec51b7e8e3cc";

        let bytes = std::fs::read(REAL_RUN_JSON_PATH).unwrap_or_else(|error| {
            panic!(
                "could not read the real denovo-screen-256 run.json fixture at {REAL_RUN_JSON_PATH}: {error}"
            )
        });
        assert_eq!(sha256_hex(&bytes), STORED_RUN_SHA256);

        let validated = decode_train_run_v2(&bytes).unwrap_or_else(|error| {
            panic!("real denovo-screen-256 run.json failed validation: {error:?}")
        });
        assert_eq!(validated.run_sha256(), STORED_RUN_SHA256);
        assert_eq!(validated.canonical_bytes(), bytes.as_slice());

        // Dual-Profile Catalog Successor (collab CLAUDE #220): this real,
        // pre-nine-deck store must classify HISTORICAL, not merely decode.
        assert_eq!(
            validated.catalog_profile_v1(),
            NativeRunCatalogProfileV1::Historical
        );

        let response = validated
            .record()
            .contracts()
            .response_exploiter_v1
            .as_ref()
            .expect("this store's record carries the response-exploiter contract");
        assert_eq!(response.run_role, "denovo-screen");
        assert_eq!(response.expected_base_seed, 971_201);
        // The real, pre-amendment shape: the array this fix made optional is
        // genuinely absent from this record's bytes, not merely defaulted.
        assert!(response.authorized_denovo_512_seeds.is_none());
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("authorized_denovo_512_seeds"));
    }

    /// Direct fixture-level companion to the real-store regression above:
    /// a "build"-role record with `authorized_denovo_512_seeds` set to
    /// `None` (the pre-amendment shape, any role other than
    /// "denovo-screen-512") validates, the key is entirely absent from its
    /// canonical bytes (never written as `null`), and decoding those exact
    /// bytes back reproduces them byte for byte -- `skip_serializing_if`
    /// really does make absence round-trip, not just decode.
    #[test]
    fn response_exploiter_absent_denovo_512_seeds_round_trips_without_the_key() {
        let mut record = response_exploiter_record_for_seed(971_001);
        record
            .contracts
            .response_exploiter_v1
            .as_mut()
            .unwrap()
            .authorized_denovo_512_seeds = None;
        refresh_derived(&mut record);
        let validated = validate_train_run_record_v2(record).unwrap();
        let bytes = validated.canonical_bytes().to_vec();
        assert!(!String::from_utf8(bytes.clone())
            .unwrap()
            .contains("authorized_denovo_512_seeds"));

        let redecoded = decode_train_run_v2(&bytes).unwrap();
        assert_eq!(redecoded.canonical_bytes(), bytes.as_slice());
        assert!(
            redecoded
                .record()
                .contracts()
                .response_exploiter_v1
                .as_ref()
                .unwrap()
                .authorized_denovo_512_seeds
                .is_none()
        );
    }

    // =========================================================================
    // Capacity-experiment wide-net record section
    // (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3, SHA-256
    // a50d067a5fb0f77b888e4e3c77386ca626e9b399a2a19f6959a1e7494f01380a).
    // =========================================================================

    fn wide_fixture_bytes() -> Vec<u8> {
        test_fixture_bytes_with_schedule_and_base_seed_wide_v2(
            crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            12,
            2,
            4,
            8,
            32768,
            65536,
            71501,
        )
    }

    #[test]
    fn wide_model_experiment_record_validates_and_labels_diagnostic() {
        let bytes = wide_fixture_bytes();
        let validated = decode_train_run_v2(&bytes).expect("wide record must validate");
        let wide = validated
            .record()
            .contracts()
            .wide_model_experiment_v1
            .as_ref()
            .expect("wide section must be present");
        assert_eq!(wide.diagnostic_label, "WIDE-DIAGNOSTIC-NON-EVIDENCE");
        assert_eq!(
            validated.record().contracts().model.architecture_identity,
            "kernel-policy-value-net-8w128"
        );
        assert_eq!(
            validated.record().model_snapshot.parameter_element_count,
            2_750_754
        );
    }

    /// The label-emission fixture (Section 5 freeze gate): the diagnostic
    /// label is a genuine byte of the record's own canonical output, not
    /// merely documented in prose alongside it.
    #[test]
    fn wide_model_experiment_record_bytes_contain_diagnostic_label() {
        let bytes = wide_fixture_bytes();
        let text = String::from_utf8(bytes).expect("canonical run.json is UTF-8");
        assert!(
            text.contains("WIDE-DIAGNOSTIC-NON-EVIDENCE"),
            "wide record bytes must literally contain the diagnostic label"
        );
    }

    /// Fail-closed direction 1: a record carrying the WIDE `model_snapshot`
    /// literals but no `contracts.wide_model_experiment_v1` section must be
    /// rejected (the frozen branch of `validate_snapshot_v1` demands the
    /// frozen Net8 literals, which the wide snapshot never matches).
    #[test]
    fn wide_model_snapshot_rejected_when_wide_section_absent() {
        let mut record = fixture_record();
        apply_wide_model_experiment(&mut record);
        record.contracts.wide_model_experiment_v1 = None;
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    /// Fail-closed direction 2: a record carrying
    /// `contracts.wide_model_experiment_v1` but the FROZEN `model_snapshot`
    /// literals must be rejected (the wide branch of `validate_snapshot_v1`
    /// demands the wide literals, which the frozen snapshot never matches).
    #[test]
    fn frozen_model_snapshot_rejected_when_wide_section_present() {
        let mut record = fixture_record();
        record.contracts.wide_model_experiment_v1 = Some(WideModelExperimentContractV1 {
            architecture_identity: FROZEN_WIDE_MODEL_ARCHITECTURE_IDENTITY_V1.to_owned(),
            config_fingerprint: FROZEN_WIDE_MODEL_CONFIG_FINGERPRINT_V1.to_owned(),
            snapshot_sha256: FROZEN_WIDE_SNAPSHOT_SHA256_V1.to_owned(),
            manifest_core_sha256: FROZEN_WIDE_SNAPSHOT_MANIFEST_CORE_SHA256_V1.to_owned(),
            payload_sha256: FROZEN_WIDE_SNAPSHOT_PAYLOAD_SHA256_V1.to_owned(),
            parameter_layout_sha256: FROZEN_WIDE_PARAMETER_LAYOUT_SHA256_V1.to_owned(),
            named_parameter_stream_sha256: FROZEN_WIDE_SNAPSHOT_NAMED_PARAMETER_STREAM_SHA256_V1
                .to_owned(),
            parameter_tensor_count: FROZEN_WIDE_PARAMETER_TENSOR_COUNT_V1,
            parameter_element_count: FROZEN_WIDE_PARAMETER_ELEMENT_COUNT_V1,
            diagnostic_label: FROZEN_WIDE_DIAGNOSTIC_LABEL_V1.to_owned(),
        });
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    /// The second, independent coupling (`validate_contracts_v2`'s own
    /// re-check of `contracts.model`): a fully-consistent wide record whose
    /// `contracts.model` is mutated back to the FROZEN Net8 literals (while
    /// `model_snapshot` and the wide section stay wide-consistent) must
    /// still be rejected. Proves `validate_model_contract_v2` is a genuine,
    /// independent fail-closed gate, not merely implied by the snapshot
    /// check.
    #[test]
    fn wide_contracts_model_mismatch_rejected_independently_of_snapshot() {
        let mut record = fixture_record();
        apply_wide_model_experiment(&mut record);
        record.contracts.model = ModelContractV2 {
            architecture_identity: FROZEN_MODEL_ARCHITECTURE_IDENTITY_V2.to_owned(),
            config_fingerprint: FROZEN_MODEL_CONFIG_FINGERPRINT_V2.to_owned(),
            parameter_layout_sha256: FROZEN_PARAMETER_LAYOUT_SHA256_V2.to_owned(),
            parameter_tensor_count: FROZEN_PARAMETER_TENSOR_COUNT_V2,
            parameter_element_count: FROZEN_PARAMETER_ELEMENT_COUNT_V2,
        };
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    /// Fail-closed on the label itself: a fully wide-consistent record whose
    /// `diagnostic_label` has drifted off the frozen
    /// `WIDE-DIAGNOSTIC-NON-EVIDENCE` literal must be rejected.
    #[test]
    fn wide_model_experiment_diagnostic_label_drift_rejected() {
        let mut record = fixture_record();
        apply_wide_model_experiment(&mut record);
        record
            .contracts
            .wide_model_experiment_v1
            .as_mut()
            .unwrap()
            .diagnostic_label = "WIDE-QUALIFIED-EVIDENCE".to_owned();
        refresh_derived(&mut record);
        assert_record_error(record, TrainRunV2ErrorKind::InvalidLiteral);
    }

    /// Combined wide-net + ladder-opponent fixture (contract Section 4: the
    /// wide run trains against the ladder pool): both sections coexist on
    /// one record and the record validates, exactly what the wide harness's
    /// eval-probe WIDE=1 knob reconstructs for a ladder-trained wide store.
    #[test]
    fn wide_model_experiment_combines_with_ladder_pool_and_validates() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;

        let bytes = test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            512,
            2,
            32,
            16,
            1_024,
            2_048,
            920_007,
            valid_ladder_pool_fixture(),
        );
        let validated = decode_train_run_v2(&bytes).expect("wide+ladder record must validate");
        let contracts = validated.record().contracts();
        assert_eq!(
            contracts
                .wide_model_experiment_v1
                .as_ref()
                .unwrap()
                .diagnostic_label,
            "WIDE-DIAGNOSTIC-NON-EVIDENCE"
        );
        assert!(contracts.opponent_ladder_pool.is_some());
        assert_eq!(
            contracts.opponent_policy.identity,
            FROZEN_LADDER_OPPONENT_POLICY_IDENTITY_V2
        );
        assert_eq!(
            validated.record().model_snapshot.parameter_element_count,
            2_750_754
        );
    }
}
