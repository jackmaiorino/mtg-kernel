//! Action-block function-preserving gradient diagnostic V1.
//!
//! Implements the frozen design
//! `collab/ACTION-BLOCK-GRADIENT-DIAGNOSTIC-V1.md`, SHA-256
//! `88c34d15cee86fb4986c1c933bcd3a7d832075ab0d5f44743aedc88eff04feb5`
//! (8,734 bytes, 159 lines), under Codex authority #130.
//!
//! The question: repaired FULL beat ZERO and PERMUTE in the completed
//! action-ingress micro-rung, but did not clear the promoted(2) safeguard.
//! Direct coordinates `[0,99)` carried mean squared norm
//! `9.348080156950672`; digest coordinates `[99,195)` carried
//! `33.84172446829393`. This diagnostic tests whether an exact half scaling
//! of the digest block improves local optimization **without changing the
//! initial policy at all**.
//!
//! The transform is function-preserving by construction. `action_encoder.0`
//! computes, for output row `o`,
//! `y_o = b_o + sum_c W[o,c] * x[c]` over `c in [0,259)`. Scaling every
//! digest input `x[c]`, `c in [99,195)`, by exact `0.5f32` and the matching
//! weight columns `W[o,c]` by exact `2.0f32` leaves every product
//! `(2w)*(0.5x)` equal to `w*x` as a real number, and the summation order is
//! untouched, so the f32 rounding of each product and of the running sum is
//! bit-identical. Both scalings are exact powers of two, hence exact in f32,
//! **provided** no `2w` overflows and no `0.5x` falls into the subnormal
//! range where the halving would lose a bit. That proviso is precisely why
//! the design demands finite inputs, normal affected nonzero values, and the
//! two round-trip identities `0.5f32*(2.0f32*w)==w` and
//! `2.0f32*(0.5f32*x)==x` before any measurement is admitted.
//!
//! The gradient, by contrast, does change exactly where the design predicts:
//! `dL/dW'[o,c] = dL/dy_o * (0.5 * x[c]) = 0.5 * dL/dW[o,c]` on the digest
//! columns and is unchanged elsewhere. That asymmetry -- identical function,
//! halved digest gradient -- is the entire measured effect.
//!
//! Scope. Everything here lives below `#[cfg(test)]` (its parent
//! `checkpoint_reliance_probe_v1` is declared `#[cfg(test)]` in
//! `native_checkpoint_inference_v1.rs`), so nothing in this file is reachable
//! from a production library build. The pure diagnostic is Store-independent.
//! Its ignored live preflight and separately authorized single-shot formal
//! driver may use the pinned Store authority loader's shared read lock and may
//! publish only diagnostic evidence to explicit new external directories;
//! neither takes an exclusive Store lock, resumes a run, or publishes or
//! mutates a checkpoint.

use super::action_ingress_admission_v2::{
    repair_and_gate_v1, synthetic_action_tensor_v1, AdmissionTransformErrorV1, DigestGateV1,
    SLOT69_V1,
};
use super::{ACTION_HASH_BEGIN_V1, ACTION_HASH_END_V1};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::async_flat_scored_rollout_v2::run_async_flat_scored_rollout_native_environment_randomization_v2;
use crate::async_flat_scored_rollout_v2::{
    expected_scorer_contract, FlatBatchScorerErrorV2, FlatBatchScorerV2, FlatScoredSelectedEventV2,
    FlatScoredTerminalEventV2, FlatScoredTrajectoryObserverV2, FlatScoringBatchViewV2,
};
use crate::async_rollout::AsyncRolloutTerminalV1;
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::fast_sampler::FastCategoricalScratch;
use crate::flat_policy_v2::{
    FlatDecisionBindingV2, FlatGlobalsV2, FlatScorerActionCoreV2, FlatScorerActionKindV2,
};
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_full_episode_trajectory_v2::preflight_native_environment_window_v2;
use crate::native_full_episode_trajectory_v2::{
    envelope_probe_receipt_for_test_v2, independent_envelope_sha256_for_test_v2, validate_start_v2,
    zero_learner_envelope_probe_receipt_for_test_v2, NativeFullEpisodeTrajectoryReceiptV2,
    NativeFullEpisodeTrajectoryStartV2, NativeTrainingTrajectoryReceiptV2,
    NativeV2ReceiptFactMutationForTestV2,
};
use crate::native_ladder_opponent_v1::ladder_pool_member_for_episode_v1;
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_ladder_pool_resolution_v1::{
    resolve_ladder_checkpoint_authority_v1, resolve_ladder_pool_v1,
};
use crate::native_policy_train_step_v1::{
    native_train_state_parameter_layout_v1, selected_log_softmax,
    NativePolicyPackedForwardBuilderV1,
};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_policy_train_step_v1::{
    NativeGaugeSubstepBoundV1, NativePhysicalLossTermV1, NativePolicyForwardInputV1,
    NativePolicyPhysicalDecisionV1, NativePolicySubstepV1, NativePolicyTrainStepResultV1,
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1, NativeScorerBiasGaugeRecordV1,
    NativeSelectedOutputV1, CANONICAL_GAUGE_PARAMETERS_V1,
};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_policy_value_net_v1::NativePolicyValueNetV1;
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1,
};
use crate::native_policy_value_net_v1::{NativeNamedParameterV1, ACTION_FEATURE_DIM_V1};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_train_state_payload_v1::decode_native_train_state_payload_v1;
use crate::native_trainer_schedule_v1::{
    derive_native_trainer_learner_action_seed_v1, native_trainer_episode_schedule_v1,
};
use crate::native_trainer_schedule_v2::OpponentLadderPoolMemberV2;
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_training_store_checkpoint_v3::derive_genesis_weights_only_payload_v2_v3;
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use crate::native_training_store_run_v2::{
    OpponentLadderCheckpointRefV1, OpponentLadderPoolContractV1,
};
use crate::private_physical_trajectory_core::{
    decision_kind_code, player_seat_code, FlatGroupedEpisodeCore, FlatGroupedTrajectoryBatchCore,
    FlatLearnerSubstepSampleCore, FlatPhysicalDecisionSampleCore, FlatPhysicalLearnerSeatRuleCore,
    FlatPhysicalUpdateStagingCore,
};
use crate::private_physical_trajectory_v2::{
    FlatOwnedScoringInputsV2, FlatPhysicalTrajectoryErrorV2, NativeFlatGroupedTrajectoryBatchV2,
    NativeFlatPhysicalTrajectoryObserverV2,
};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1, TerminalSafeCodeV2};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FlatActionDecisionBindingV2, SessionDeckHashesV1,
    SessionDeckIdsV1, FLAT_ACTION_FLAG_INCLUDE_V1, FLAT_ACTION_FLAG_VALUE_V1,
};
use crate::runtime_decks::runtime_deck_by_id;
use sha2::{Digest, Sha256};
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

// ---------------------------------------------------------------------------
// Frozen design identities and constants. Copied from the accepted design,
// never inferred. Changing any of these invalidates the contract.
// ---------------------------------------------------------------------------

/// The accepted design document's exact bytes.
pub(super) const DESIGN_DOCUMENT_SHA256_V1: &str =
    "88c34d15cee86fb4986c1c933bcd3a7d832075ab0d5f44743aedc88eff04feb5";
pub(super) const DESIGN_DOCUMENT_BYTE_COUNT_V1: u64 = 8_734;
pub(super) const DESIGN_DOCUMENT_LINE_COUNT_V1: u64 = 159;

/// Pool3 opponent contract document, copied and rehashed, never retyped.
pub(super) const POOL3_DOCUMENT_SHA256_V1: &str =
    "6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71";

/// Diagnostic-only composite numerical-backend identity. This is NOT the
/// production Store-admitted CUDA identity, which authority #130 leaves
/// unchanged. It is proved by the root Cargo patch, locked vendored-path
/// resolution, vendored tree object
/// `6fad4dfdc90762682675f0e9d3313af35c1a9572`, and compilation records.
pub(super) const DIAGNOSTIC_BACKEND_IDENTITY_V1: &str =
    "rust-experimental-native-policy-train-step-v1-cuda-burn-dense-padded-cubecl-simpleunit-register-f32-v2";
pub(super) const VENDORED_SIMPLEUNIT_TREE_OBJECT_V1: &str =
    "6fad4dfdc90762682675f0e9d3313af35c1a9572";

/// Source checkpoint: promoted(2), base seed `920012`, generation `384`.
pub(super) const SOURCE_BASE_SEED_V1: u64 = 920_012;
pub(super) const SOURCE_GENERATION_V1: u64 = 384;
pub(super) const SOURCE_RUN_SHA256_V1: &str =
    "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae";
pub(super) const SOURCE_CHECKPOINT_SHA256_V1: &str =
    "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8";
pub(super) const SOURCE_SIDECAR_SHA256_V1: &str =
    "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb";
pub(super) const SOURCE_PAYLOAD_SHA256_V1: &str =
    "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99";
pub(super) const SOURCE_MODEL_PARAMETER_SHA256_V1: &str =
    "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d";

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const POOL3_ROOT_WINDOWS_V1: &str = r"D:\mtg-kernel-ladder-pilot-20260725\pool3";
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const PREFLIGHT_GPU_ORDINAL_V1: &str = "1";
const PREFLIGHT_GPU_ORDINAL_U64_V1: u64 = 1;
const PREFLIGHT_GPU_NAME_V1: &str = "NVIDIA GeForce RTX 3050";
const PREFLIGHT_GPU_UUID_V1: &str = "GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3";
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const PREFLIGHT_CUDA_DEVICE_ORDER_V1: &str = "PCI_BUS_ID";
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const PREFLIGHT_LIVE_TEST_NAME_SUFFIX_V1: &str =
    "::action_block_gradient_preflight_seed949999_gpu1_v1";
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const FORMAL_LIVE_TEST_NAME_SUFFIX_V1: &str = "::action_block_gradient_formal_six_unit_gpu1_v1";
const PREFLIGHT_UPDATE_PAIR_SCHEMA_V1: &str =
    "mtg_kernel_action_block_gradient_preflight_update_pair/v1";
const FORMAL_UNIT_TAPE_SCHEMA_V1: &str = "action-block-gradient-formal-unit-tape/v1";
const FORMAL_MANIFEST_SCHEMA_V1: &str = "action-block-gradient-formal-manifest/v1";

// These values must be supplied while compiling the one live-preflight test
// binary. `option_env!` embeds them without making ordinary feature builds
// depend on a qualification launcher. Git HEAD/clean/tracked-tree identities
// are already embedded unconditionally by `build.rs`; tool executable
// identities complete that existing build-time authority.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_RUSTC_PATH_V1: Option<&str> = option_env!("RUSTC");
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_LINKER_PATH_V1: Option<&str> =
    option_env!("CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER");
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_RUSTC_EXE_SHA256_V1: Option<&str> =
    option_env!("MTG_KERNEL_ACTION_BLOCK_BUILD_RUSTC_EXE_SHA256_V1");
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_RUSTC_VERBOSE_SHA256_V1: Option<&str> =
    option_env!("MTG_KERNEL_ACTION_BLOCK_BUILD_RUSTC_VERBOSE_SHA256_V1");
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_LINKER_EXE_SHA256_V1: Option<&str> =
    option_env!("MTG_KERNEL_ACTION_BLOCK_BUILD_LINKER_EXE_SHA256_V1");
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_NVIDIA_SMI_PATH_V1: Option<&str> =
    option_env!("MTG_KERNEL_ACTION_BLOCK_BUILD_NVIDIA_SMI_PATH_V1");
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const EMBEDDED_BUILD_NVIDIA_SMI_SHA256_V1: Option<&str> =
    option_env!("MTG_KERNEL_ACTION_BLOCK_BUILD_NVIDIA_SMI_SHA256_V1");

/// Exact tensor-side and weight-side scale bits.
pub(super) const HALF_DIGEST_SCALE_BITS_V1: u32 = 0x3f00_0000; // 0.5f32
pub(super) const DOUBLE_WEIGHT_SCALE_BITS_V1: u32 = 0x4000_0000; // 2.0f32

/// The transformed weight tensor and its row-major shape.
pub(super) const ACTION_ENCODER_WEIGHT_NAME_V1: &str = "action_encoder.0.weight";
pub(super) const ACTION_ENCODER_ROWS_V1: usize = 64;
pub(super) const ACTION_ENCODER_COLUMNS_V1: usize = 259;

/// Production loss and canonical-gauge Adam authorities, frozen as bits.
pub(super) const VALUE_COEFFICIENT_BITS_V1: u32 = 0x3f00_0000;
pub(super) const LEARNING_RATE_BITS_V1: u32 = 0x3a83_126f;
pub(super) const ADAM_BETA1_BITS_V1: u32 = 0x3f66_6666;
pub(super) const ADAM_BETA2_BITS_V1: u32 = 0x3f7f_be77;
pub(super) const ADAM_EPSILON_BITS_V1: u32 = 0x322b_cc77;
pub(super) const ADAM_WEIGHT_DECAY_BITS_V1: u32 = 0x0000_0000;
pub(super) const ADAM_AMSGRAD_V1: bool = false;

/// Fixed numerical envelope. Do not substitute the historical `0.03803`
/// envelope or the experimental CPU/CUDA parity tolerances.
pub(super) const ENVELOPE_ABSOLUTE_BITS_V1: u32 = 0x3586_37bd;
pub(super) const ENVELOPE_RELATIVE_BITS_V1: u32 = 0x3580_0000;
/// Per-coordinate absolute step-one delta ceiling base, `0.001f32`.
pub(super) const MAX_ABSOLUTE_DELTA_BASE_BITS_V1: u32 = 0x3a83_126f;

/// Paired-cluster bootstrap: all `6^6` ordered tuples, last coordinate
/// varying fastest, two-sided read at these zero-based sorted indices.
pub(super) const BOOTSTRAP_UNIT_COUNT_V1: usize = 6;
pub(super) const BOOTSTRAP_TUPLE_COUNT_V1: usize = 46_656;
pub(super) const BOOTSTRAP_LOW_INDEX_V1: usize = 1_166;
pub(super) const BOOTSTRAP_HIGH_INDEX_V1: usize = 45_489;
/// At least this many of the six paired differences must be strictly
/// positive.
pub(super) const REQUIRED_POSITIVE_UNITS_V1: usize = 5;

/// Episode budget per tape and the retryable preflight base seed.
pub(super) const EPISODES_PER_TAPE_V1: u64 = 64;
pub(super) const PREFLIGHT_BASE_SEED_V1: u64 = 949_999;
/// The preflight tape's exact Pool3 choices, straight from the design.
pub(super) const PREFLIGHT_REQUIRED_COUNTS_V1: [u32; 4] = [27, 13, 13, 11];
/// Formal seeds, admitted only by the private single-shot formal authorities.
pub(super) const FORMAL_TRAINING_SEEDS_V1: [u64; BOOTSTRAP_UNIT_COUNT_V1] =
    [950_001, 950_002, 950_003, 950_004, 950_005, 950_006];
pub(super) const FORMAL_VALIDATION_SEEDS_V1: [u64; BOOTSTRAP_UNIT_COUNT_V1] =
    [951_001, 951_002, 951_003, 951_004, 951_005, 951_006];
pub(super) const FORMAL_TRAINING_COUNTS_V1: [[u32; 4]; BOOTSTRAP_UNIT_COUNT_V1] = [
    [25, 12, 19, 8],
    [20, 19, 11, 14],
    [39, 8, 7, 10],
    [28, 11, 12, 13],
    [18, 12, 13, 21],
    [25, 14, 17, 8],
];
pub(super) const FORMAL_VALIDATION_COUNTS_V1: [[u32; 4]; BOOTSTRAP_UNIT_COUNT_V1] = [
    [27, 13, 13, 11],
    [30, 12, 11, 11],
    [25, 12, 11, 16],
    [28, 14, 10, 12],
    [24, 13, 15, 12],
    [18, 12, 14, 20],
];

// ---------------------------------------------------------------------------
// Opponent strata and pure schedule counts.
// ---------------------------------------------------------------------------

/// Ordered promoted(2) / predecessor-A / predecessor-B / uniform counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PoolCountsV1 {
    pub(super) promoted2: u32,
    pub(super) predecessor_a: u32,
    pub(super) predecessor_b: u32,
    pub(super) uniform: u32,
}

impl PoolCountsV1 {
    pub(super) const fn as_array_v1(self) -> [u32; 4] {
        [
            self.promoted2,
            self.predecessor_a,
            self.predecessor_b,
            self.uniform,
        ]
    }

    pub(super) fn total_v1(self) -> u32 {
        self.promoted2 + self.predecessor_a + self.predecessor_b + self.uniform
    }
}

/// Pure schedule projection: counts the Pool3 member drawn for each absolute
/// episode index in `[0, episode_count)` at `base_seed`.
///
/// This calls the production per-episode selector
/// [`ladder_pool_member_for_episode_v1`] directly, so the counts are the
/// trainer's own opponent draw and not a reimplementation. It touches no
/// episode, forward, loss, gradient, update, or classifier: the selector is
/// a pure function of `(base_seed, episode_index)` that loads no checkpoint.
pub(super) fn pool_choice_counts_v1(base_seed: u64, episode_count: u64) -> PoolCountsV1 {
    let mut counts = PoolCountsV1::default();
    for episode_index in 0..episode_count {
        let member = ladder_pool_member_for_episode_v1(base_seed, episode_index)
            .expect("preflight and formal base seeds are inside u63");
        match member {
            OpponentLadderPoolMemberV2::Primary => counts.promoted2 += 1,
            OpponentLadderPoolMemberV2::PredecessorA => counts.predecessor_a += 1,
            OpponentLadderPoolMemberV2::PredecessorB => counts.predecessor_b += 1,
            OpponentLadderPoolMemberV2::UniformFloor => counts.uniform += 1,
        }
    }
    counts
}

// ---------------------------------------------------------------------------
// Exact scaling validity: finiteness, normality, and the round trips.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScalingValidityErrorV1 {
    /// A scaling input was NaN or infinite.
    NonFinite,
    /// A nonzero affected value was subnormal, where the exact-power-of-two
    /// scaling identity is not guaranteed to round-trip.
    Subnormal,
    /// `2.0f32 * (0.5f32 * x) != x`, so halving this input lost information.
    HalveRoundTrip,
    /// `0.5f32 * (2.0f32 * w) != w`, so doubling this weight lost
    /// information (or overflowed).
    DoubleRoundTrip,
}

/// A value is admissible for the transform when it is finite and either
/// exactly zero or normal. Subnormal nonzero values are rejected because the
/// halving identity is not guaranteed there.
pub(super) fn scaling_input_admissible_v1(value: f32) -> Result<(), ScalingValidityErrorV1> {
    if !value.is_finite() {
        return Err(ScalingValidityErrorV1::NonFinite);
    }
    if value != 0.0 && !value.is_normal() {
        return Err(ScalingValidityErrorV1::Subnormal);
    }
    Ok(())
}

/// The design's mandatory halving round trip for a digest activation.
pub(super) fn halve_round_trip_v1(x: f32) -> Result<f32, ScalingValidityErrorV1> {
    scaling_input_admissible_v1(x)?;
    let half = f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * x;
    if f32::from_bits(DOUBLE_WEIGHT_SCALE_BITS_V1) * half != x {
        return Err(ScalingValidityErrorV1::HalveRoundTrip);
    }
    Ok(half)
}

/// The design's mandatory doubling round trip for a digest weight column.
pub(super) fn double_round_trip_v1(w: f32) -> Result<f32, ScalingValidityErrorV1> {
    scaling_input_admissible_v1(w)?;
    let doubled = f32::from_bits(DOUBLE_WEIGHT_SCALE_BITS_V1) * w;
    if !doubled.is_finite() {
        return Err(ScalingValidityErrorV1::DoubleRoundTrip);
    }
    if f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * doubled != w {
        return Err(ScalingValidityErrorV1::DoubleRoundTrip);
    }
    Ok(doubled)
}

// ---------------------------------------------------------------------------
// The HALF transform: tensor side and weight side.
// ---------------------------------------------------------------------------

/// Every way a parameter-side transform can refuse to run. All of these are
/// enforced in **release** builds, not behind `debug_assert!`: a malformed
/// snapshot must fail closed in the shipped diagnostic, not only in a debug
/// run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ParameterTransformErrorV1 {
    /// The `action_encoder.0.weight` target was not present at all.
    MissingTarget,
    /// The target name appeared more than once in one snapshot.
    DuplicateTarget,
    /// The target's declared shape was not exactly `[64, 259]`.
    ShapeMismatch,
    /// The target's value count was not exactly `64 * 259 = 16,576`.
    ValueLengthMismatch,
    /// A touched value failed finiteness, normality, or a round trip.
    Scaling(ScalingValidityErrorV1),
}

impl From<ScalingValidityErrorV1> for ParameterTransformErrorV1 {
    fn from(error: ScalingValidityErrorV1) -> Self {
        Self::Scaling(error)
    }
}

/// The exact element count of `action_encoder.0.weight`.
pub(super) const ACTION_ENCODER_VALUE_COUNT_V1: usize =
    ACTION_ENCODER_ROWS_V1 * ACTION_ENCODER_COLUMNS_V1;

/// Locates the single `action_encoder.0.weight` target and proves it is
/// well-formed before any scaling touches it. Rejects a missing target, a
/// duplicated target, a wrong shape, and a wrong value length.
fn validate_single_target_v1(
    parameters: &[NativeNamedParameterV1],
) -> Result<usize, ParameterTransformErrorV1> {
    let mut found: Option<usize> = None;
    for (index, parameter) in parameters.iter().enumerate() {
        if parameter.name != ACTION_ENCODER_WEIGHT_NAME_V1 {
            continue;
        }
        if found.is_some() {
            return Err(ParameterTransformErrorV1::DuplicateTarget);
        }
        found = Some(index);
    }
    let index = found.ok_or(ParameterTransformErrorV1::MissingTarget)?;
    let target = &parameters[index];
    if target.shape.as_slice() != [ACTION_ENCODER_ROWS_V1, ACTION_ENCODER_COLUMNS_V1] {
        return Err(ParameterTransformErrorV1::ShapeMismatch);
    }
    if target.values.len() != ACTION_ENCODER_VALUE_COUNT_V1 {
        return Err(ParameterTransformErrorV1::ValueLengthMismatch);
    }
    Ok(index)
}

/// Derives the H parameter set from an F parameter set by doubling exactly
/// the `action_encoder.0.weight` row-major columns `[99,195)`.
///
/// The caller must pass an untreated F snapshot. Every touched value is
/// round-trip proved before the transform is admitted, so a snapshot that
/// would lose information fails closed rather than silently producing a
/// non-function-preserving H.
pub(super) fn derive_half_parameters_v1(
    full_parameters: &[NativeNamedParameterV1],
) -> Result<Vec<NativeNamedParameterV1>, ParameterTransformErrorV1> {
    let index = validate_single_target_v1(full_parameters)?;
    let mut half = full_parameters.to_vec();
    for row in half[index]
        .values
        .chunks_exact_mut(ACTION_ENCODER_COLUMNS_V1)
    {
        for value in row
            .iter_mut()
            .take(ACTION_HASH_END_V1)
            .skip(ACTION_HASH_BEGIN_V1)
        {
            *value = double_round_trip_v1(*value)?;
        }
    }
    Ok(half)
}

/// Bit-exact inverse of [`derive_half_parameters_v1`], used to prove the H
/// parameter set really is the doubled F set and nothing else drifted.
/// Enforces the identical release-mode target/shape/length gates.
pub(super) fn halve_action_encoder_digest_columns_v1(
    half_parameters: &[NativeNamedParameterV1],
) -> Result<Vec<NativeNamedParameterV1>, ParameterTransformErrorV1> {
    let index = validate_single_target_v1(half_parameters)?;
    let mut restored = half_parameters.to_vec();
    for row in restored[index]
        .values
        .chunks_exact_mut(ACTION_ENCODER_COLUMNS_V1)
    {
        for value in row
            .iter_mut()
            .take(ACTION_HASH_END_V1)
            .skip(ACTION_HASH_BEGIN_V1)
        {
            *value = halve_round_trip_v1(*value)?;
        }
    }
    Ok(restored)
}

// ---------------------------------------------------------------------------
// Tensor-side treatment lineage.
//
// Authority #131: F and H must come from ONE repaired-FULL bit-copy lineage
// with a pure HALF transform. Repair runs exactly once, ever; H is never a
// second repair call.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TreatmentLineageErrorV1 {
    /// The single repair of the retained pre-repair baseline failed.
    Repair(AdmissionTransformErrorV1),
    /// A digest coordinate failed the exact halving gate.
    Scaling(ScalingValidityErrorV1),
}

/// The pure HALF tensor transform: exact `0.5f32` on digest coordinates
/// `[99,195)` of an ALREADY-REPAIRED tensor. Performs no repair, touches no
/// slot-69, and reads no action metadata.
pub(super) fn pure_half_digest_transform_v1(
    repaired_full: &NativeFlatDecisionTensorV2,
) -> Result<NativeFlatDecisionTensorV2, ScalingValidityErrorV1> {
    let mut half = repaired_full.clone();
    for row in half.action_features.chunks_exact_mut(ACTION_FEATURE_DIM_V1) {
        for value in row
            .iter_mut()
            .take(ACTION_HASH_END_V1)
            .skip(ACTION_HASH_BEGIN_V1)
        {
            *value = halve_round_trip_v1(*value)?;
        }
    }
    Ok(half)
}

/// One repaired-FULL base, then a bit-copy for F and one pure HALF transform
/// for H.
#[derive(Clone, Debug)]
struct TreatmentPairV1 {
    full: NativeFlatDecisionTensorV2,
    half: NativeFlatDecisionTensorV2,
}

/// Derives both treatments from a retained pre-repair baseline using exactly
/// one `repair_and_gate_v1` call.
///
/// This is the only sanctioned way to build F and H. Calling repair a second
/// time -- even on the same baseline -- is forbidden by authority #131,
/// because repair is a stateful, non-idempotent slot-69 rewrite and the two
/// arms must be provably the same repaired object.
fn derive_treatment_pair_v1(
    pre_repair_baseline: &NativeFlatDecisionTensorV2,
    actions: &[FlatScorerActionCoreV2],
) -> Result<TreatmentPairV1, TreatmentLineageErrorV1> {
    // EXACTLY ONE repair, for the whole lineage.
    let repaired_full = repair_and_gate_v1(pre_repair_baseline, actions, DigestGateV1::Full)
        .map_err(TreatmentLineageErrorV1::Repair)?;
    // F is a pure bit-copy of that base.
    let full = repaired_full.clone();
    // H is a pure transform of that same base: no repair, no action reads.
    let half =
        pure_half_digest_transform_v1(&repaired_full).map_err(TreatmentLineageErrorV1::Scaling)?;
    Ok(TreatmentPairV1 { full, half })
}

fn f32_slices_bit_equal_v1(left: &[f32], right: &[f32]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}

/// Exact equality for every tensor field other than `action_features`.
/// `PartialEq` is insufficient for the f32 fields because it collapses signed
/// zero and cannot compare NaN payload bits; lineage authority is bit-level.
fn decision_tensor_non_action_fields_bit_equal_v1(
    left: &NativeFlatDecisionTensorV2,
    right: &NativeFlatDecisionTensorV2,
) -> bool {
    f32_slices_bit_equal_v1(&left.state, &right.state)
        && f32_slices_bit_equal_v1(&left.object_features, &right.object_features)
        && left.object_card_ids == right.object_card_ids
        && left.object_groups == right.object_groups
        && left.object_node_ids == right.object_node_ids
        && f32_slices_bit_equal_v1(&left.edge_features, &right.edge_features)
        && left.edge_source_indices == right.edge_source_indices
        && left.edge_target_indices == right.edge_target_indices
        && f32_slices_bit_equal_v1(&left.action_ref_features, &right.action_ref_features)
        && left.action_ref_card_ids == right.action_ref_card_ids
        && left.action_ref_action_indices == right.action_ref_action_indices
        && left.action_ref_node_indices == right.action_ref_node_indices
}

/// Private construction seal. Code outside this module can borrow a retained
/// lineage but cannot synthesize one from independently repaired tensors.
#[derive(Debug)]
struct RetainedTreatmentLineageSealV1(());

/// Owned PRE/FULL/HALF lineage for one scored decision. Deliberately not
/// `Clone`: the scorer constructs it once, the join moves it once, and every
/// later check is a read-only coordinate predicate. In particular, validation
/// never calls repair, even on a reconstructed PRE tensor.
#[derive(Debug)]
struct RetainedTreatmentLineageV1 {
    pre_repair_action_features: Vec<f32>,
    source_action_cores: Vec<FlatScorerActionCoreV2>,
    full_tensor: NativeFlatDecisionTensorV2,
    half_tensor: NativeFlatDecisionTensorV2,
    _seal: RetainedTreatmentLineageSealV1,
}

impl RetainedTreatmentLineageV1 {
    /// The sole retained-lineage constructor and the sole repair invocation
    /// for this scored row. The pre-repair tensor is consumed so it cannot be
    /// accidentally reused by the scoring path after construction.
    fn from_pre_repair_v1(
        pre_repair_baseline: NativeFlatDecisionTensorV2,
        source_action_cores: Vec<FlatScorerActionCoreV2>,
    ) -> Result<Self, TreatmentLineageErrorV1> {
        let pre_repair_action_features = pre_repair_baseline.action_features.clone();
        let pair = derive_treatment_pair_v1(&pre_repair_baseline, &source_action_cores)?;
        let lineage = Self {
            pre_repair_action_features,
            source_action_cores,
            full_tensor: pair.full,
            half_tensor: pair.half,
            _seal: RetainedTreatmentLineageSealV1(()),
        };
        debug_assert!(lineage.relation_valid_v1());
        Ok(lineage)
    }

    fn pre_repair_action_features_v1(&self) -> &[f32] {
        &self.pre_repair_action_features
    }

    fn source_action_cores_v1(&self) -> &[FlatScorerActionCoreV2] {
        &self.source_action_cores
    }

    fn full_tensor_v1(&self) -> &NativeFlatDecisionTensorV2 {
        &self.full_tensor
    }

    fn half_tensor_v1(&self) -> &NativeFlatDecisionTensorV2 {
        &self.half_tensor
    }

    /// Proves the retained relation without cloning or transforming anything:
    /// PRE -> FULL is exactly the ingress slot-69 repair, while FULL -> HALF
    /// is exactly the digest-coordinate halving relation.
    fn relation_valid_v1(&self) -> bool {
        let config = NativePolicyValueModelConfigV1::contract_v1();
        let Ok(full_counts) = encoded_decision_view_v1(&self.full_tensor).validate(config) else {
            return false;
        };
        let Ok(half_counts) = encoded_decision_view_v1(&self.half_tensor).validate(config) else {
            return false;
        };
        let action_count = self.source_action_cores.len();
        let Some(expected_action_feature_count) = action_count.checked_mul(ACTION_FEATURE_DIM_V1)
        else {
            return false;
        };
        if action_count == 0
            || full_counts.object_count != half_counts.object_count
            || full_counts.edge_count != half_counts.edge_count
            || full_counts.action_count != half_counts.action_count
            || full_counts.action_ref_count != half_counts.action_ref_count
            || full_counts.action_count != action_count
            || self.pre_repair_action_features.len() != expected_action_feature_count
            || self.full_tensor.action_features.len() != expected_action_feature_count
            || self.half_tensor.action_features.len() != expected_action_feature_count
            || !decision_tensor_non_action_fields_bit_equal_v1(&self.full_tensor, &self.half_tensor)
        {
            return false;
        }

        let kind_count = FlatScorerActionKindV2::OrderTriggers as usize + 1;
        for row_index in 0..action_count {
            let begin = row_index * ACTION_FEATURE_DIM_V1;
            let end = begin + ACTION_FEATURE_DIM_V1;
            let pre = &self.pre_repair_action_features[begin..end];
            let full = &self.full_tensor.action_features[begin..end];
            let half = &self.half_tensor.action_features[begin..end];
            let action = &self.source_action_cores[row_index];
            let kind = action.kind as usize;
            if kind >= kind_count
                || pre[..kind_count].iter().enumerate().any(|(index, value)| {
                    value.to_bits() != if index == kind { 1.0f32.to_bits() } else { 0 }
                })
            {
                return false;
            }

            let repaired_slot69_bits = match action.kind {
                FlatScorerActionKindV2::ChooseEffectBoolean => {
                    if action.flags & !FLAT_ACTION_FLAG_VALUE_V1 != 0 {
                        return false;
                    }
                    let expected = if action.flags & FLAT_ACTION_FLAG_VALUE_V1 != 0 {
                        1.0f32.to_bits()
                    } else {
                        0
                    };
                    if pre[SLOT69_V1].to_bits() != expected {
                        return false;
                    }
                    expected
                }
                FlatScorerActionKindV2::ChooseAttackerInclusion
                | FlatScorerActionKindV2::ChooseBlockerInclusion => {
                    if action.flags & !FLAT_ACTION_FLAG_INCLUDE_V1 != 0
                        || pre[SLOT69_V1].to_bits() != 0
                    {
                        return false;
                    }
                    if action.flags & FLAT_ACTION_FLAG_INCLUDE_V1 != 0 {
                        1.0f32.to_bits()
                    } else {
                        0
                    }
                }
                _ => pre[SLOT69_V1].to_bits(),
            };

            for column in 0..ACTION_FEATURE_DIM_V1 {
                let expected_full_bits = if column == SLOT69_V1 {
                    repaired_slot69_bits
                } else {
                    pre[column].to_bits()
                };
                if full[column].to_bits() != expected_full_bits {
                    return false;
                }
                if (ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1).contains(&column) {
                    let Ok(expected_half) = halve_round_trip_v1(full[column]) else {
                        return false;
                    };
                    if half[column].to_bits() != expected_half.to_bits() {
                        return false;
                    }
                } else if half[column].to_bits() != full[column].to_bits() {
                    return false;
                }
            }
        }
        true
    }
}

/// The single-layer pre-activation the transform must preserve exactly.
/// Canonical left-to-right accumulation over all `259` inputs, matching the
/// production summation order the design freezes.
pub(super) fn action_encoder_preactivation_v1(weight_row: &[f32], input: &[f32], bias: f32) -> f32 {
    debug_assert_eq!(weight_row.len(), ACTION_ENCODER_COLUMNS_V1);
    debug_assert_eq!(input.len(), ACTION_ENCODER_COLUMNS_V1);
    let mut sum = bias;
    for (weight, value) in weight_row.iter().zip(input) {
        sum += weight * value;
    }
    sum
}

// ---------------------------------------------------------------------------
// Source-integrated treatment-aware scorer (authority #141).
//
// The scorer is where the treatment is applied and where the evidence that
// "the loss consumed the scored row" is created. It retains, per scored
// decision: the exact `FlatDecisionBindingV2`, the retained PRE-REPAIR
// action-feature row, the ordered source action cores, the single-repair F/H
// tensor pair, and the initial CPU outputs. F is the only arm published to
// the rollout, and only after F and H are proved bit-identical.
// ---------------------------------------------------------------------------

/// Rebuilds the encoded view a packed forward consumes. Mirrors the
/// trainer's private `native_encoded_decision_view_v1` exactly; it is a pure
/// borrow constructor over the thirteen tensor fields.
fn encoded_decision_view_v1(
    tensor: &NativeFlatDecisionTensorV2,
) -> NativeEncodedDecisionViewV1<'_> {
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        NativeEncodedDecisionSchemaV1::contract_v1(),
        &tensor.state,
        &tensor.object_features,
        &tensor.object_card_ids,
        &tensor.object_groups,
        &tensor.object_node_ids,
        &tensor.edge_features,
        &tensor.edge_source_indices,
        &tensor.edge_target_indices,
        &tensor.action_features,
        &tensor.action_ref_features,
        &tensor.action_ref_card_ids,
        &tensor.action_ref_action_indices,
        &tensor.action_ref_node_indices,
    )
}

/// One retained scored entry. Exactly one of these must join to exactly one
/// grouped substep after rollout.
/// Deliberately NOT `Clone` (authority #144): the join consumes each
/// entry by value, so a duplicate match is structurally impossible.
#[derive(Debug)]
pub(super) struct RetainedScoredEntryV1 {
    binding: FlatDecisionBindingV2,
    /// Private, non-Clone, constructor-sealed single-repair lineage.
    lineage: RetainedTreatmentLineageV1,
    /// Initial CPU outputs, proved bit-identical between F and H.
    initial_logits: Vec<f32>,
    initial_value: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TreatmentScorerFailureV1 {
    Contract,
    OutputShape,
    MissingDecision,
    Tensor,
    PackedForward,
    /// The transform was not function-preserving on this decision: F and H
    /// produced different initial logits or values.
    InitialFunctionDivergence,
    Lineage(TreatmentLineageErrorV1),
    Allocation,
}

impl TreatmentScorerFailureV1 {
    pub(super) const fn code_v1(self) -> u32 {
        match self {
            Self::Contract => 1,
            Self::OutputShape => 2,
            Self::MissingDecision => 3,
            Self::Tensor => 4,
            Self::PackedForward => 5,
            Self::InitialFunctionDivergence => 6,
            Self::Lineage(_) => 7,
            Self::Allocation => 8,
        }
    }
}

/// A `FlatBatchScorerV2` that applies the action-block treatment once per
/// decision and retains the full evidence needed for the post-rollout join.
pub(super) struct TreatmentAwareScorerV1 {
    tensorizer: NativeFlatTensorizerV2,
    /// F parameters (the repaired-FULL arm) and H parameters (digest columns
    /// of `action_encoder.0.weight` doubled).
    full_forward: NativePolicyPackedForwardBuilderV1,
    half_forward: NativePolicyPackedForwardBuilderV1,
    retained: Vec<RetainedScoredEntryV1>,
    last_failure: Option<TreatmentScorerFailureV1>,
    accepted_decision_count: u64,
}

impl TreatmentAwareScorerV1 {
    pub(super) fn new_v1(
        full_forward: NativePolicyPackedForwardBuilderV1,
        half_forward: NativePolicyPackedForwardBuilderV1,
    ) -> Self {
        Self {
            tensorizer: NativeFlatTensorizerV2::new(),
            full_forward,
            half_forward,
            retained: Vec::new(),
            last_failure: None,
            accepted_decision_count: 0,
        }
    }

    pub(super) fn retained_v1(&self) -> &[RetainedScoredEntryV1] {
        &self.retained
    }

    pub(super) fn into_retained_v1(self) -> Vec<RetainedScoredEntryV1> {
        self.retained
    }

    pub(super) fn last_failure_v1(&self) -> Option<TreatmentScorerFailureV1> {
        self.last_failure
    }

    pub(super) fn accepted_decision_count_v1(&self) -> u64 {
        self.accepted_decision_count
    }

    /// Transactional scoring chunk. Mirrors the production trainer's
    /// cardinality and contract checks, then stages every candidate before
    /// any caller-visible slice or counter changes.
    fn score_chunk_v1(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), TreatmentScorerFailureV1> {
        let contract = batch.contract();
        if contract != expected_scorer_contract(contract.card_db_hash) {
            return Err(TreatmentScorerFailureV1::Contract);
        }
        if batch.decision_count() == 0
            || values.len() != batch.decision_count()
            || action_logits.len() != batch.total_action_count()
            || action_logits.is_empty()
            || batch.action_offsets().len() != batch.decision_count() + 1
        {
            return Err(TreatmentScorerFailureV1::OutputShape);
        }
        let next_decision_count = self
            .accepted_decision_count
            .checked_add(batch.decision_count() as u64)
            .ok_or(TreatmentScorerFailureV1::OutputShape)?;

        let mut candidate_logits: Vec<f32> = Vec::new();
        candidate_logits
            .try_reserve_exact(action_logits.len())
            .map_err(|_| TreatmentScorerFailureV1::Allocation)?;
        let mut candidate_values: Vec<f32> = Vec::new();
        candidate_values
            .try_reserve_exact(values.len())
            .map_err(|_| TreatmentScorerFailureV1::Allocation)?;
        let mut candidate_retained: Vec<RetainedScoredEntryV1> = Vec::new();
        candidate_retained
            .try_reserve_exact(batch.decision_count())
            .map_err(|_| TreatmentScorerFailureV1::Allocation)?;

        for decision_index in 0..batch.decision_count() {
            let decision = batch
                .decision(decision_index)
                .ok_or(TreatmentScorerFailureV1::MissingDecision)?;
            let binding = batch
                .binding(decision_index)
                .ok_or(TreatmentScorerFailureV1::MissingDecision)?;
            let begin = batch.action_offsets()[decision_index];
            let end = batch.action_offsets()[decision_index + 1];
            if end < begin || end > action_logits.len() || end - begin != decision.actions().len() {
                return Err(TreatmentScorerFailureV1::OutputShape);
            }
            let source_action_cores = decision.actions().to_vec();

            // Tensorize once; this is the retained PRE-REPAIR baseline.
            let mut baseline = NativeFlatDecisionTensorV2::default();
            self.tensorizer
                .fill(decision, &mut baseline)
                .map_err(|_| TreatmentScorerFailureV1::Tensor)?;
            // Exactly one repair for the whole private lineage; consuming the
            // PRE tensor prevents this scoring path from reusing it.
            let lineage =
                RetainedTreatmentLineageV1::from_pre_repair_v1(baseline, source_action_cores)
                    .map_err(TreatmentScorerFailureV1::Lineage)?;

            // Both arms forward on CPU. F uses the F parameters, H uses the
            // doubled-column H parameters; the design's whole premise is
            // that these agree bit for bit at t = 0.
            let full_tape = self
                .full_forward
                .forward_v1(encoded_decision_view_v1(lineage.full_tensor_v1()))
                .map_err(|_| TreatmentScorerFailureV1::PackedForward)?;
            let half_tape = self
                .half_forward
                .forward_v1(encoded_decision_view_v1(lineage.half_tensor_v1()))
                .map_err(|_| TreatmentScorerFailureV1::PackedForward)?;

            let full_logits = full_tape.logits_v1();
            let half_logits = half_tape.logits_v1();
            if full_logits.len() != end - begin
                || full_logits.iter().any(|value| !value.is_finite())
                || !full_tape.value_v1().is_finite()
            {
                return Err(TreatmentScorerFailureV1::OutputShape);
            }
            if full_logits.len() != half_logits.len()
                || full_tape.value_v1().to_bits() != half_tape.value_v1().to_bits()
                || full_logits
                    .iter()
                    .zip(half_logits)
                    .any(|(full, half)| full.to_bits() != half.to_bits())
            {
                return Err(TreatmentScorerFailureV1::InitialFunctionDivergence);
            }

            candidate_logits.extend_from_slice(full_logits);
            candidate_values.push(full_tape.value_v1());
            candidate_retained.push(RetainedScoredEntryV1 {
                binding,
                lineage,
                initial_logits: full_logits.to_vec(),
                initial_value: full_tape.value_v1(),
            });
        }

        if candidate_logits.len() != action_logits.len() || candidate_values.len() != values.len() {
            return Err(TreatmentScorerFailureV1::OutputShape);
        }
        // Commit: only F reaches the rollout, and only after every decision
        // in the chunk passed the identity gate.
        self.retained
            .try_reserve(candidate_retained.len())
            .map_err(|_| TreatmentScorerFailureV1::Allocation)?;
        self.retained.extend(candidate_retained);
        action_logits.copy_from_slice(&candidate_logits);
        values.copy_from_slice(&candidate_values);
        self.accepted_decision_count = next_decision_count;
        Ok(())
    }
}

impl FlatBatchScorerV2 for TreatmentAwareScorerV1 {
    fn score_batch_v2(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV2> {
        match self.score_chunk_v1(batch, action_logits, values) {
            Ok(()) => Ok(()),
            Err(failure) => {
                if self.last_failure.is_none() {
                    self.last_failure = Some(failure);
                }
                Err(FlatBatchScorerErrorV2::new(failure.code_v1()))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Forwarding observer with V2 receipt capture (authority #141).
//
// `AsyncFlatScoredRolloutResultV2` drops the per-episode receipts, and the
// grouped batch's learner trace hash alone is not the trajectory commitment.
// This wrapper copies each terminal receipt BEFORE forwarding the event, so
// the finished output is the grouped hierarchy plus all 64 receipts.
// ---------------------------------------------------------------------------

/// A sealed capability proving the caller holds the seed-949999 preflight
/// authority. It cannot be constructed with any other value, has no public
/// field, and reads its seed only from `PREFLIGHT_BASE_SEED_V1`, so no
/// environment, CLI, or caller-supplied override can reach the join.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PreflightSeed949999AuthorityV1(());

impl PreflightSeed949999AuthorityV1 {
    /// The only constructor. Takes no argument by design.
    pub(super) const fn seal_v1() -> Self {
        Self(())
    }

    pub(super) const fn seed_v1(self) -> u64 {
        PREFLIGHT_BASE_SEED_V1
    }
}

/// Private seed/count capability shared by the already-qualified preflight
/// and the separately authorized formal tapes. Callers cannot inject an
/// arbitrary seed or substitute schedule counts.
pub(super) trait DiagnosticTapeAuthorityV1 {
    fn seed_v1(&self) -> u64;
    fn expected_counts_v1(&self) -> [u32; 4];
    fn allows_update_v1(&self) -> bool;
}

impl DiagnosticTapeAuthorityV1 for PreflightSeed949999AuthorityV1 {
    fn seed_v1(&self) -> u64 {
        PREFLIGHT_BASE_SEED_V1
    }

    fn expected_counts_v1(&self) -> [u32; 4] {
        PREFLIGHT_REQUIRED_COUNTS_V1
    }

    fn allows_update_v1(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FormalTapeRoleV1 {
    Training,
    Validation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FormalSeedAuthorityV1 {
    unit_index: usize,
    role: FormalTapeRoleV1,
}

impl FormalSeedAuthorityV1 {
    fn training_v1(unit_index: usize) -> Self {
        assert!(unit_index < BOOTSTRAP_UNIT_COUNT_V1);
        Self {
            unit_index,
            role: FormalTapeRoleV1::Training,
        }
    }

    fn validation_v1(unit_index: usize) -> Self {
        assert!(unit_index < BOOTSTRAP_UNIT_COUNT_V1);
        Self {
            unit_index,
            role: FormalTapeRoleV1::Validation,
        }
    }
}

impl DiagnosticTapeAuthorityV1 for FormalSeedAuthorityV1 {
    fn seed_v1(&self) -> u64 {
        match self.role {
            FormalTapeRoleV1::Training => FORMAL_TRAINING_SEEDS_V1[self.unit_index],
            FormalTapeRoleV1::Validation => FORMAL_VALIDATION_SEEDS_V1[self.unit_index],
        }
    }

    fn expected_counts_v1(&self) -> [u32; 4] {
        match self.role {
            FormalTapeRoleV1::Training => FORMAL_TRAINING_COUNTS_V1[self.unit_index],
            FormalTapeRoleV1::Validation => FORMAL_VALIDATION_COUNTS_V1[self.unit_index],
        }
    }

    fn allows_update_v1(&self) -> bool {
        self.role == FormalTapeRoleV1::Training
    }
}

const PREFLIGHT_DECK_ID_V1: &str = "Rally";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReceiptExpectedFactsV1 {
    episode_id: u64,
    learner_seat: PlayerSeatV1,
    policy_step_count: u64,
    physical_decision_count: u64,
    learner_policy_step_count: u64,
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_physical_decision_count: u64,
}

fn receipt_outer_commitment_matches_v1(
    validated_start: &crate::native_full_episode_trajectory_v2::NativeFullEpisodeTrajectoryValidatedStartV2,
    inner: [u8; 32],
    outer: Option<[u8; 32]>,
) -> bool {
    outer
        == Some(independent_envelope_sha256_for_test_v2(
            validated_start,
            inner,
        ))
}

/// Re-establishes the complete environment-randomization-V2 receipt
/// diagonal from sealed seed and runtime deck authority. A digest is never
/// accepted merely because it is nonzero: the outer envelope is rebuilt
/// independently over the exact inner digest and validated start.
fn validate_preflight_receipt_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    expected: ReceiptExpectedFactsV1,
    receipt: &NativeTrainingTrajectoryReceiptV2,
) -> Result<(), JoinErrorV1> {
    if expected_deck_ids[0] != PREFLIGHT_DECK_ID_V1 || expected_deck_ids[1] != PREFLIGHT_DECK_ID_V1
    {
        return Err(JoinErrorV1::ReceiptDeckMismatch);
    }
    let schedule = native_trainer_episode_schedule_v1(authority.seed_v1(), expected.episode_id)
        .map_err(|_| JoinErrorV1::ReceiptScheduleMismatch)?;
    if expected.learner_seat != schedule.learner_seat {
        return Err(JoinErrorV1::ReceiptScheduleMismatch);
    }
    let validated_start = validate_start_v2(&NativeFullEpisodeTrajectoryStartV2 {
        episode_index: expected.episode_id,
        pair_environment_seed: schedule.environment_seed,
        deck_ids: expected_deck_ids.clone(),
        deck_hashes: expected_deck_hashes,
        learner_seat: schedule.learner_seat,
    })
    .map_err(|_| JoinErrorV1::ReceiptStartInvalid)?;

    if !receipt.is_environment_randomization_v2()
        || receipt.episode_index() != expected.episode_id
        || receipt.pair_index_v2() != Some(schedule.pair_index)
        || receipt.environment_seed() != schedule.environment_seed
        || receipt.learner_seat() != schedule.learner_seat
    {
        return Err(JoinErrorV1::ReceiptScheduleMismatch);
    }
    if receipt.deck_ids_v2() != Some(validated_start.deck_ids)
        || receipt.deck_hashes() != validated_start.deck_hashes
    {
        return Err(JoinErrorV1::ReceiptDeckMismatch);
    }

    let split_policy_total = expected
        .learner_policy_step_count
        .checked_add(expected.opponent_policy_step_count)
        .ok_or(JoinErrorV1::ReceiptFactMismatch)?;
    let split_physical_total = expected
        .learner_physical_decision_count
        .checked_add(expected.opponent_physical_decision_count)
        .ok_or(JoinErrorV1::ReceiptFactMismatch)?;
    if split_policy_total != expected.policy_step_count
        || split_physical_total != expected.physical_decision_count
        || receipt.policy_step_count() != expected.policy_step_count
        || receipt.physical_decision_count() != expected.physical_decision_count
        || receipt.learner_policy_step_count() != expected.learner_policy_step_count
        || receipt.opponent_policy_step_count() != expected.opponent_policy_step_count
        || receipt.learner_physical_decision_count() != expected.learner_physical_decision_count
        || receipt.opponent_physical_decision_count() != expected.opponent_physical_decision_count
    {
        return Err(JoinErrorV1::ReceiptFactMismatch);
    }
    if !receipt_outer_commitment_matches_v1(
        &validated_start,
        receipt.trajectory_sha256(),
        receipt.outer_trajectory_sha256_v2(),
    ) {
        return Err(JoinErrorV1::ReceiptCommitmentMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_join_binding_v1(
    observed: FlatDecisionBindingV2,
    retained: FlatDecisionBindingV2,
    expected: FastActorDecisionV1,
    episode_id: u64,
    physical_decision_id: u64,
    group_substep_count: u32,
    group_actor: PlayerSeatV1,
    action_count: usize,
) -> Result<(), JoinErrorV1> {
    let action_count = u32::try_from(action_count).map_err(|_| JoinErrorV1::BindingFactMismatch)?;
    let contract = expected_scorer_contract(KERNEL_CARDDB_HASH);
    let binding = observed.action_binding;
    if observed != retained
        || binding.card_db_hash != KERNEL_CARDDB_HASH
        || binding.slice_version != contract.action_slice_version
        || binding.ref_role_mapping_version != contract.action_ref_role_mapping_version
        || binding.card_token_mapping_version != contract.card_token_mapping_version
        || binding.candidate_commitment_version != contract.candidate_commitment_version
        || observed.typed_layout_version != contract.typed_layout_version
        || observed.feature_inventory_version != contract.feature_inventory_version
        || observed.enum_mapping_version != contract.enum_mapping_version
        || observed.object_group_mapping_version != contract.object_group_mapping_version
        || observed.relation_role_mapping_version != contract.relation_role_mapping_version
        || observed.context_subrole_mapping_version != contract.context_subrole_mapping_version
        || observed.action_ref_projection_role_mapping_version
            != contract.action_ref_projection_role_mapping_version
        || observed.contract_digests != contract.contract_digests
        || expected.episode_id != episode_id
        || expected.physical_decision_id != physical_decision_id
        || expected.substep_count != group_substep_count
        || expected.acting_player != group_actor
        || expected.legal_action_count != action_count
        || binding.episode_id != expected.episode_id
        || binding.environment_revision != expected.environment_revision
        || binding.bound_policy_step_count != expected.step
        || binding.physical_decision_id != expected.physical_decision_id
        || binding.bound_physical_decision_count != expected.physical_decision_id
        || binding.substep_index != expected.substep_index
        || binding.substep_count != expected.substep_count
        || binding.acting_player != player_seat_code(expected.acting_player)
        || binding.decision_kind != decision_kind_code(expected.decision_kind)
        || binding.legal_action_count != expected.legal_action_count
    {
        return Err(JoinErrorV1::BindingFactMismatch);
    }
    Ok(())
}

/// The grouped hierarchy plus every retained terminal receipt.
pub(super) struct ObservedRolloutV1 {
    pub(super) batch: NativeFlatGroupedTrajectoryBatchV2,
    pub(super) receipts: Vec<NativeTrainingTrajectoryReceiptV2>,
}

pub(super) struct ReceiptRetainingObserverV1 {
    inner: NativeFlatPhysicalTrajectoryObserverV2,
    receipts: Vec<NativeTrainingTrajectoryReceiptV2>,
}

impl ReceiptRetainingObserverV1 {
    pub(super) fn new_v1(
        first_episode_id: u64,
        episode_count: u64,
    ) -> Result<Self, FlatPhysicalTrajectoryErrorV2> {
        Ok(Self {
            inner: NativeFlatPhysicalTrajectoryObserverV2::new(first_episode_id, episode_count)?,
            receipts: Vec::new(),
        })
    }
}

impl FlatScoredTrajectoryObserverV2 for ReceiptRetainingObserverV1 {
    type Error = FlatPhysicalTrajectoryErrorV2;
    type Output = ObservedRolloutV1;

    fn observe_selected_v2(
        &mut self,
        event: FlatScoredSelectedEventV2<'_>,
    ) -> Result<(), Self::Error> {
        self.inner.observe_selected_v2(event)
    }

    fn observe_terminal_v2(&mut self, event: FlatScoredTerminalEventV2) -> Result<(), Self::Error> {
        // Copy the receipt before forwarding; the inner observer does not
        // retain it and the rollout result discards it.
        if let Some(receipt) = event.native_full_trajectory_receipt {
            self.receipts.push(receipt);
        }
        self.inner.observe_terminal_v2(event)
    }

    fn finish_v2(self) -> Result<Self::Output, Self::Error> {
        Ok(ObservedRolloutV1 {
            batch: self.inner.finish_v2()?,
            receipts: self.receipts,
        })
    }
}

// ---------------------------------------------------------------------------
// Exact one-to-one consuming join (authority #141/#144).
//
// Every grouped substep must consume exactly one retained scored entry by
// exact `FlatDecisionBindingV2`. Entries are MOVED out of the retained pool,
// so a duplicate match is structurally impossible and any residual entry is
// visible at the end.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JoinErrorV1 {
    /// Batch-level episode/count/staging facts disagree with the hierarchy.
    BatchFactMismatch,
    /// No retained scored entry carries this substep's binding.
    MissingRetainedEntry,
    /// The retained entry for this binding was already consumed by an
    /// earlier substep.
    DuplicateBinding,
    /// Retained entries survived the join, so the scorer scored rows the
    /// loss never consumed.
    ResidualRetainedEntries,
    /// The observer's action cores disagree with the retained ones.
    ActionCoreMismatch,
    /// The observer's raw logit bits disagree with the retained initial
    /// logits.
    LogitBitsMismatch,
    /// The observer's predicted value bits disagree with the retained value.
    ValueBitsMismatch,
    /// The observer's selected index is outside its own action row.
    SelectedIndexOutOfRange,
    /// The production categorical sampler, re-run from the bound logits and
    /// `action_seed`, does not reproduce the observer's selected index.
    SamplerDisagreement,
    /// The production `selected_log_softmax`, recomputed from the bound raw
    /// logits, does not reproduce the observer's selected-log-probability
    /// bits.
    LogSoftmaxDisagreement,
    /// The terminal classification/code/outcome/winner/reward tuple, the
    /// scheduled learner seat, or the supplied learner return disagreed.
    TerminalFactMismatch,
    /// Absolute episode identifiers were not exactly `[0,64)` in order.
    EpisodeIdentityViolation,
    /// Physical-decision or learner-ordinal order was not strictly
    /// increasing in canonical order.
    OrderViolation,
    /// No training learner group exists anywhere on the tape.
    NoLearnerGroups,
    /// The retained receipt count is not exactly one per episode.
    ReceiptCardinality,
    /// Receipt episode identifiers were not unique and exactly `[0,64)`.
    ReceiptEpisodeIdentity,
    /// A receipt's own facts disagree with its grouped episode record.
    ReceiptFactMismatch,
    /// Two retained scored entries carry the same binding, so the join
    /// cannot be one-to-one.
    DuplicateRetainedBinding,
    /// A substep's action seed is not the schedule-derived learner action
    /// seed for its (base seed, episode, group, substep) coordinates.
    ActionSeedAuthority,
    /// The receipt or grouped episode disagrees with the production
    /// seed-949999 episode schedule.
    ReceiptScheduleMismatch,
    /// The exact Rally/Rally V2 start facts failed production validation.
    ReceiptStartInvalid,
    /// The receipt's deck hashes are not the pinned Rally/Rally hashes.
    ReceiptDeckMismatch,
    /// The receipt's outer commitment is not the independent envelope over
    /// its exact inner commitment and validated V2 start.
    ReceiptCommitmentMismatch,
    /// A grouped, expected, observed-binding, or retained-binding scalar or
    /// production contract identity disagreed before consumption.
    BindingFactMismatch,
    /// A physical group's redundant first-ordinal, joint-log-probability, or
    /// first-value summary disagreed with its ordered substeps.
    GroupSummaryMismatch,
    /// The retained FULL/HALF tensors are not the exact one-repair lineage
    /// reconstructed from the retained pre-repair action row.
    TreatmentLineageMismatch,
}

/// Exact production natural-terminal diagonal plus learner-return derivation.
/// The return is never accepted merely because it lies in `[-1,1]`.
fn validate_natural_terminal_v1(
    expected_episode_id: u64,
    scheduled_learner_seat: PlayerSeatV1,
    observed_learner_return: i32,
    terminal: AsyncRolloutTerminalV1,
) -> Result<i8, JoinErrorV1> {
    if terminal.episode_id != expected_episode_id
        || terminal.terminal_classification != TerminalClassificationV1::Natural
        || terminal.terminal_code != TerminalSafeCodeV2::NaturalGameOver
    {
        return Err(JoinErrorV1::TerminalFactMismatch);
    }
    let (expected_winner, expected_reward) = match terminal.terminal_outcome {
        TerminalOutcomeV1::P0Win => (Some(PlayerSeatV1::P0), [1, -1]),
        TerminalOutcomeV1::P1Win => (Some(PlayerSeatV1::P1), [-1, 1]),
        TerminalOutcomeV1::Draw => (None, [0, 0]),
        TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
            return Err(JoinErrorV1::TerminalFactMismatch);
        }
    };
    if terminal.winner != expected_winner || terminal.terminal_reward != expected_reward {
        return Err(JoinErrorV1::TerminalFactMismatch);
    }
    let learner_return = match (scheduled_learner_seat, terminal.winner) {
        (_, None) => 0,
        (learner, Some(winner)) if learner == winner => 1,
        _ => -1,
    };
    let learner_reward_index = match scheduled_learner_seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    };
    if terminal.terminal_reward[learner_reward_index] != learner_return
        || observed_learner_return != learner_return
    {
        return Err(JoinErrorV1::TerminalFactMismatch);
    }
    i8::try_from(learner_return).map_err(|_| JoinErrorV1::TerminalFactMismatch)
}

#[derive(Debug)]
struct AuthenticatedNaturalTerminalSealV1(());

/// Private invariant object tying the retained return to the exact natural
/// terminal tuple and scheduled learner seat. Deliberately not `Clone`.
#[derive(Debug)]
struct AuthenticatedNaturalTerminalV1 {
    terminal: AsyncRolloutTerminalV1,
    learner_return: i8,
    _seal: AuthenticatedNaturalTerminalSealV1,
}

impl AuthenticatedNaturalTerminalV1 {
    fn authenticate_v1(
        expected_episode_id: u64,
        scheduled_learner_seat: PlayerSeatV1,
        observed_learner_return: i32,
        terminal: AsyncRolloutTerminalV1,
    ) -> Result<Self, JoinErrorV1> {
        let learner_return = validate_natural_terminal_v1(
            expected_episode_id,
            scheduled_learner_seat,
            observed_learner_return,
            terminal,
        )?;
        Ok(Self {
            terminal,
            learner_return,
            _seal: AuthenticatedNaturalTerminalSealV1(()),
        })
    }

    fn relation_valid_v1(
        &self,
        expected_episode_id: u64,
        scheduled_learner_seat: PlayerSeatV1,
    ) -> bool {
        validate_natural_terminal_v1(
            expected_episode_id,
            scheduled_learner_seat,
            i32::from(self.learner_return),
            self.terminal,
        ) == Ok(self.learner_return)
    }

    fn learner_return_v1(&self) -> i8 {
        self.learner_return
    }

    fn terminal_v1(&self) -> AsyncRolloutTerminalV1 {
        self.terminal
    }
}

/// One joined substep: the observer record and the retained scored entry it
/// consumed, proved to be the same decision.
pub(super) struct JoinedSubstepV1 {
    pub(super) learner_ordinal: u64,
    pub(super) selected_index: u32,
    pub(super) action_seed: u64,
    pub(super) raw_action_logit_bits: Vec<u32>,
    pub(super) selected_log_probability_bits: u32,
    pub(super) predicted_value_bits: u32,
    pub(super) binding: FlatDecisionBindingV2,
    pub(super) expected: FastActorDecisionV1,
    /// The consumed retained entry, moved in.
    pub(super) retained: RetainedScoredEntryV1,
}

pub(super) struct JoinedGroupV1 {
    pub(super) physical_decision_id: u64,
    pub(super) substeps: Vec<JoinedSubstepV1>,
}

pub(super) struct JoinedEpisodeV1 {
    pub(super) episode_id: u64,
    pub(super) stratum: u32,
    authenticated_terminal: AuthenticatedNaturalTerminalV1,
    pub(super) trace_hash: u64,
    pub(super) receipt: NativeTrainingTrajectoryReceiptV2,
    receipt_expected_facts: ReceiptExpectedFactsV1,
    pub(super) groups: Vec<JoinedGroupV1>,
}

pub(super) struct JoinedTapeV1 {
    pub(super) base_seed: u64,
    pub(super) counts: PoolCountsV1,
    pub(super) episodes: Vec<JoinedEpisodeV1>,
}

impl JoinedTapeV1 {
    pub(super) fn total_group_count_v1(&self) -> usize {
        self.episodes
            .iter()
            .map(|episode| episode.groups.len())
            .sum()
    }

    pub(super) fn total_substep_count_v1(&self) -> usize {
        self.episodes
            .iter()
            .flat_map(|episode| &episode.groups)
            .map(|group| group.substeps.len())
            .sum()
    }

    /// Learner physical decision groups per stratum, derived from the joined
    /// hierarchy itself.
    pub(super) fn groups_per_stratum_v1(&self) -> [u32; 4] {
        let mut per_stratum = [0u32; 4];
        for episode in &self.episodes {
            per_stratum[episode.stratum as usize] += episode.groups.len() as u32;
        }
        per_stratum
    }
}

/// Joins the observed hierarchy to the retained scored entries, consuming
/// each entry exactly once.
///
/// `base_seed` is the run's own authority; every episode's stratum is
/// recomputed from it through the production schedule rather than trusted.
pub(super) fn join_rollout_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    observed: ObservedRolloutV1,
    retained: Vec<RetainedScoredEntryV1>,
) -> Result<JoinedTapeV1, JoinErrorV1> {
    let base_seed = authority.seed_v1();
    if observed.receipts.len() as u64 != EPISODES_PER_TAPE_V1
        || observed.batch.episodes.len() as u64 != EPISODES_PER_TAPE_V1
    {
        return Err(JoinErrorV1::ReceiptCardinality);
    }
    if observed.batch.first_episode_id != 0
        || observed.batch.episode_count != EPISODES_PER_TAPE_V1
        || !matches!(
            observed.batch.learner_seat_rule,
            crate::private_physical_trajectory_core::FlatPhysicalLearnerSeatRuleCore::EpisodeParity
        )
    {
        return Err(JoinErrorV1::BatchFactMismatch);
    }
    let mut hierarchy_policy_step_count = 0u64;
    let mut hierarchy_physical_decision_count = 0u64;
    for episode in &observed.batch.episodes {
        let episode_policy_step_count = episode.groups.iter().try_fold(0u64, |sum, group| {
            u64::try_from(group.substeps.len())
                .ok()
                .and_then(|count| sum.checked_add(count))
        });
        let Some(episode_policy_step_count) = episode_policy_step_count else {
            return Err(JoinErrorV1::BatchFactMismatch);
        };
        let episode_physical_decision_count =
            u64::try_from(episode.groups.len()).map_err(|_| JoinErrorV1::BatchFactMismatch)?;
        if episode.learner_policy_step_count != episode_policy_step_count
            || episode.learner_physical_decision_count != episode_physical_decision_count
        {
            return Err(JoinErrorV1::BatchFactMismatch);
        }
        hierarchy_policy_step_count = hierarchy_policy_step_count
            .checked_add(episode_policy_step_count)
            .ok_or(JoinErrorV1::BatchFactMismatch)?;
        hierarchy_physical_decision_count = hierarchy_physical_decision_count
            .checked_add(episode_physical_decision_count)
            .ok_or(JoinErrorV1::BatchFactMismatch)?;
    }
    if observed.batch.learner_policy_step_count != hierarchy_policy_step_count
        || observed.batch.learner_physical_decision_count != hierarchy_physical_decision_count
        || u64::try_from(retained.len()).map_err(|_| JoinErrorV1::BatchFactMismatch)?
            != hierarchy_policy_step_count
        || !matches!(
            observed.batch.update_staging,
            crate::private_physical_trajectory_core::FlatPhysicalUpdateStagingCore::Ready {
                learner_group_count
            } if learner_group_count == hierarchy_physical_decision_count
        )
    {
        return Err(JoinErrorV1::BatchFactMismatch);
    }

    // Defect-3 fix: retained bindings must be GLOBALLY UNIQUE before any
    // consumption, so a 2:2 duplicate cannot be silently paired off.
    for left in 0..retained.len() {
        for right in (left + 1)..retained.len() {
            if retained[left].binding == retained[right].binding {
                return Err(JoinErrorV1::DuplicateRetainedBinding);
            }
        }
    }
    // Receipts must carry unique episode identifiers exactly `[0,64)`; the
    // join is keyed by episode ID, never by asynchronous arrival order.
    let mut receipt_by_episode: Vec<Option<NativeTrainingTrajectoryReceiptV2>> =
        (0..EPISODES_PER_TAPE_V1).map(|_| None).collect();
    for receipt in observed.receipts {
        let index = usize::try_from(receipt.episode_index())
            .map_err(|_| JoinErrorV1::ReceiptEpisodeIdentity)?;
        let slot = receipt_by_episode
            .get_mut(index)
            .ok_or(JoinErrorV1::ReceiptEpisodeIdentity)?;
        if slot.is_some() {
            return Err(JoinErrorV1::ReceiptEpisodeIdentity);
        }
        *slot = Some(receipt);
    }
    if receipt_by_episode.iter().any(Option::is_none) {
        return Err(JoinErrorV1::ReceiptEpisodeIdentity);
    }

    let mut pool: Vec<Option<RetainedScoredEntryV1>> = retained.into_iter().map(Some).collect();
    let mut consumed_bindings: Vec<FlatDecisionBindingV2> = Vec::new();
    let mut sampler = FastCategoricalScratch::default();

    let mut episodes = Vec::with_capacity(observed.batch.episodes.len());
    for (episode_index, episode) in observed.batch.episodes.into_iter().enumerate() {
        // Defect-1 fix: production learner ordinals restart per episode, so
        // canonical progression is required WITHIN each episode only.
        let mut previous_ordinal: Option<u64> = None;
        // Absolute episode identifiers must be exactly [0,64) in order.
        if episode.episode_id != episode_index as u64 {
            return Err(JoinErrorV1::EpisodeIdentityViolation);
        }
        let schedule = native_trainer_episode_schedule_v1(base_seed, episode.episode_id)
            .map_err(|_| JoinErrorV1::ReceiptScheduleMismatch)?;
        if episode.learner_seat != schedule.learner_seat {
            return Err(JoinErrorV1::ReceiptScheduleMismatch);
        }
        let authenticated_terminal = AuthenticatedNaturalTerminalV1::authenticate_v1(
            episode.episode_id,
            schedule.learner_seat,
            episode.learner_return,
            episode.terminal,
        )?;
        let stratum = stratum_ordinal_v1(
            ladder_pool_member_for_episode_v1(base_seed, episode_index as u64)
                .map_err(|_| JoinErrorV1::EpisodeIdentityViolation)?,
        );
        let split_policy_total = episode
            .learner_policy_step_count
            .checked_add(episode.opponent_policy_step_count)
            .ok_or(JoinErrorV1::TerminalFactMismatch)?;
        let split_physical_total = episode
            .learner_physical_decision_count
            .checked_add(episode.opponent_physical_decision_count)
            .ok_or(JoinErrorV1::TerminalFactMismatch)?;
        if split_policy_total != episode.terminal.policy_step_count
            || split_physical_total != episode.terminal.physical_decision_count
            || episode.opponent_policy_step_count < episode.opponent_physical_decision_count
        {
            return Err(JoinErrorV1::TerminalFactMismatch);
        }
        let expected_receipt_facts = ReceiptExpectedFactsV1 {
            episode_id: episode.episode_id,
            learner_seat: episode.learner_seat,
            policy_step_count: episode.terminal.policy_step_count,
            physical_decision_count: episode.terminal.physical_decision_count,
            learner_policy_step_count: episode.learner_policy_step_count,
            opponent_policy_step_count: episode.opponent_policy_step_count,
            learner_physical_decision_count: episode.learner_physical_decision_count,
            opponent_physical_decision_count: episode.opponent_physical_decision_count,
        };

        let mut previous_decision_id: Option<u64> = None;
        let mut groups = Vec::with_capacity(episode.groups.len());
        for (group_index, group) in episode.groups.into_iter().enumerate() {
            let exact_substep_count = u32::try_from(group.substeps.len())
                .map_err(|_| JoinErrorV1::BindingFactMismatch)?;
            let expected_first_ordinal = previous_ordinal.map_or(0, |previous| previous + 1);
            if group.substeps.is_empty()
                || group.episode_id != episode.episode_id
                || group.acting_player != episode.learner_seat
                || group.substep_count != exact_substep_count
                || group.first_learner_ordinal != expected_first_ordinal
            {
                return Err(JoinErrorV1::BindingFactMismatch);
            }
            if group.physical_decision_id >= episode.terminal.physical_decision_count {
                return Err(JoinErrorV1::TerminalFactMismatch);
            }
            // Strict physical-decision order within the episode.
            if let Some(previous) = previous_decision_id {
                if group.physical_decision_id <= previous {
                    return Err(JoinErrorV1::OrderViolation);
                }
            }
            previous_decision_id = Some(group.physical_decision_id);

            let mut substeps = Vec::with_capacity(group.substeps.len());
            let mut recomputed_joint_log_probability = 0.0f32;
            let mut recomputed_first_value_bits: Option<u32> = None;
            for (substep_index, substep) in group.substeps.into_iter().enumerate() {
                let exact_substep_index =
                    u32::try_from(substep_index).map_err(|_| JoinErrorV1::BindingFactMismatch)?;
                if substep.expected.substep_index != exact_substep_index {
                    return Err(JoinErrorV1::BindingFactMismatch);
                }
                if substep.expected.step >= episode.terminal.policy_step_count {
                    return Err(JoinErrorV1::TerminalFactMismatch);
                }
                // Learner ordinals must be EXACTLY contiguous from zero
                // within the episode, not merely increasing.
                let expected_ordinal = previous_ordinal.map_or(0, |previous| previous + 1);
                if substep.learner_ordinal != expected_ordinal {
                    return Err(JoinErrorV1::OrderViolation);
                }
                previous_ordinal = Some(substep.learner_ordinal);

                // Authenticate the SCHEDULE-DERIVED learner action seed. A
                // wrong seed can select the same action, so rerunning the
                // sampler alone is not sufficient evidence.
                let scheduled_action_seed = derive_native_trainer_learner_action_seed_v1(
                    base_seed,
                    episode.episode_id,
                    group_index as u64,
                    substep.expected.substep_index,
                )
                .map_err(|_| JoinErrorV1::ActionSeedAuthority)?;
                if substep.action_seed != scheduled_action_seed {
                    return Err(JoinErrorV1::ActionSeedAuthority);
                }

                // A grouped binding may occur at most once across the whole
                // tape; a second occurrence is a duplicate, not a new match.
                if consumed_bindings.contains(&substep.binding) {
                    return Err(JoinErrorV1::DuplicateBinding);
                }
                // Exactly one retained entry must match, and it is MOVED out.
                let mut matches = pool.iter().enumerate().filter(|(_, entry)| {
                    entry
                        .as_ref()
                        .is_some_and(|entry| entry.binding == substep.binding)
                });
                let slot = matches.next().map(|(index, _)| index);
                if matches.next().is_some() {
                    return Err(JoinErrorV1::DuplicateRetainedBinding);
                }
                let slot = slot.ok_or(JoinErrorV1::MissingRetainedEntry)?;
                let retained_entry = pool[slot].as_ref().expect("slot was just proved occupied");
                // Revalidate the complete production contract and every
                // group/expected/binding scalar, including the candidate
                // commitment through observed == retained, before moving
                // the retained entry out of the one-to-one pool.
                validate_join_binding_v1(
                    substep.binding,
                    retained_entry.binding,
                    substep.expected,
                    episode.episode_id,
                    group.physical_decision_id,
                    group.substep_count,
                    group.acting_player,
                    substep.scoring_inputs.actions.len(),
                )?;
                if !retained_entry.lineage.relation_valid_v1() {
                    return Err(JoinErrorV1::TreatmentLineageMismatch);
                }
                let entry = pool[slot].take().expect("slot was just proved occupied");
                consumed_bindings.push(substep.binding);

                // The observer's own record must agree with the retained
                // scored row in every load-bearing field.
                if substep.scoring_inputs.actions != entry.lineage.source_action_cores {
                    return Err(JoinErrorV1::ActionCoreMismatch);
                }
                if substep.raw_action_logit_bits.len() != entry.initial_logits.len()
                    || entry.initial_logits.iter().any(|value| !value.is_finite())
                    || substep
                        .raw_action_logit_bits
                        .iter()
                        .zip(&entry.initial_logits)
                        .any(|(observed, retained)| *observed != retained.to_bits())
                {
                    return Err(JoinErrorV1::LogitBitsMismatch);
                }
                if !entry.initial_value.is_finite()
                    || substep.predicted_value_bits != entry.initial_value.to_bits()
                {
                    return Err(JoinErrorV1::ValueBitsMismatch);
                }
                if substep.selected_index as usize >= substep.raw_action_logit_bits.len() {
                    return Err(JoinErrorV1::SelectedIndexOutOfRange);
                }

                // Re-run the PRODUCTION categorical sampler from the bound
                // logits and action seed; it must reproduce the selection.
                let logits: Vec<f32> = substep
                    .raw_action_logit_bits
                    .iter()
                    .map(|bits| f32::from_bits(*bits))
                    .collect();
                let resampled = sampler
                    .sample(&logits, substep.action_seed)
                    .map_err(|_| JoinErrorV1::SamplerDisagreement)?;
                if resampled != substep.selected_index as usize {
                    return Err(JoinErrorV1::SamplerDisagreement);
                }

                // Recompute the loss's own log-probability through the
                // PRODUCTION path; it must reproduce the observer's bits.
                let (log_probability, _row) =
                    selected_log_softmax(&logits, substep.selected_index as usize)
                        .map_err(|_| JoinErrorV1::LogSoftmaxDisagreement)?;
                if log_probability.to_bits() != substep.selected_log_probability_bits {
                    return Err(JoinErrorV1::LogSoftmaxDisagreement);
                }
                recomputed_joint_log_probability += log_probability;
                if !recomputed_joint_log_probability.is_finite() {
                    return Err(JoinErrorV1::GroupSummaryMismatch);
                }
                recomputed_first_value_bits.get_or_insert(substep.predicted_value_bits);

                substeps.push(JoinedSubstepV1 {
                    learner_ordinal: substep.learner_ordinal,
                    selected_index: substep.selected_index,
                    action_seed: substep.action_seed,
                    raw_action_logit_bits: substep.raw_action_logit_bits,
                    selected_log_probability_bits: substep.selected_log_probability_bits,
                    predicted_value_bits: substep.predicted_value_bits,
                    binding: substep.binding,
                    expected: substep.expected,
                    retained: entry,
                });
            }
            if group.joint_selected_log_probability_bits
                != recomputed_joint_log_probability.to_bits()
                || Some(group.value_bits) != recomputed_first_value_bits
            {
                return Err(JoinErrorV1::GroupSummaryMismatch);
            }
            groups.push(JoinedGroupV1 {
                physical_decision_id: group.physical_decision_id,
                substeps,
            });
        }

        // Bind and authenticate the receipt for THIS episode id against the
        // complete production environment-randomization-V2 diagonal.
        let receipt = receipt_by_episode[episode_index]
            .take()
            .ok_or(JoinErrorV1::ReceiptEpisodeIdentity)?;
        // The grouped episode must also agree with the joined group count.
        if receipt.learner_physical_decision_count() != groups.len() as u64 {
            return Err(JoinErrorV1::ReceiptFactMismatch);
        }
        validate_preflight_receipt_v1(
            authority,
            expected_deck_ids,
            expected_deck_hashes,
            expected_receipt_facts,
            &receipt,
        )?;
        episodes.push(JoinedEpisodeV1 {
            episode_id: episode.episode_id,
            stratum,
            authenticated_terminal,
            trace_hash: episode.learner_trace_hash,
            receipt,
            receipt_expected_facts: expected_receipt_facts,
            groups,
        });
    }

    // No retained entry may survive: every scored row must have been
    // consumed by the loss.
    if pool.iter().any(Option::is_some) {
        return Err(JoinErrorV1::ResidualRetainedEntries);
    }

    let tape = JoinedTapeV1 {
        base_seed,
        counts: pool_choice_counts_v1(base_seed, EPISODES_PER_TAPE_V1),
        episodes,
    };
    // At least one training learner group must exist somewhere.
    if tape.total_group_count_v1() == 0 {
        return Err(JoinErrorV1::NoLearnerGroups);
    }
    Ok(tape)
}

// ---------------------------------------------------------------------------
// Numerical envelope and delta arithmetic. Written in the design's exact f64
// order; the operand conversions happen first and each delta is one
// subtraction.
// ---------------------------------------------------------------------------

/// `E(a,b) = f64::from(0x358637bd) + f64::from(0x35800000) * max(|a|,|b|)`.
///
/// Returns `None` for any nonfinite operand and for a nonfinite result. A
/// comparison against a nonfinite envelope is never "within tolerance"; it
/// is an `INVALID` condition, so the envelope refuses to produce one.
pub(super) fn envelope_v1(a: f64, b: f64) -> Option<f64> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let envelope = f64::from(f32::from_bits(ENVELOPE_ABSOLUTE_BITS_V1))
        + f64::from(f32::from_bits(ENVELOPE_RELATIVE_BITS_V1)) * a.abs().max(b.abs());
    envelope.is_finite().then_some(envelope)
}

/// One f64 subtraction of exactly converted f32 operands.
pub(super) fn delta_v1(before_f32: f32, after_f32: f32) -> f64 {
    f64::from(after_f32) - f64::from(before_f32)
}

/// `|left - right| <= E(left, right)`, false-on-nonfinite throughout: any
/// nonfinite operand, difference, or envelope fails the comparison rather
/// than silently passing it.
pub(super) fn within_envelope_v1(left: f64, right: f64) -> bool {
    let Some(envelope) = envelope_v1(left, right) else {
        return false;
    };
    let difference = left - right;
    difference.is_finite() && difference.abs() <= envelope
}

/// The step-one absolute delta ceiling,
/// `f64::from(0.001f32) + E(f64::from(0.001f32), 0.0)`.
pub(super) fn max_absolute_delta_bound_v1() -> Option<f64> {
    let base = f64::from(f32::from_bits(MAX_ABSOLUTE_DELTA_BASE_BITS_V1));
    let bound = base + envelope_v1(base, 0.0)?;
    bound.is_finite().then_some(bound)
}

/// A step-one parameter delta must be finite and inside the frozen ceiling.
pub(super) fn absolute_delta_admissible_v1(delta: f64) -> bool {
    let Some(bound) = max_absolute_delta_bound_v1() else {
        return false;
    };
    delta.is_finite() && delta.abs() <= bound
}

/// The design's H digest-column consistency gate, in the exact written f64
/// order: `ds=a-b`, `eb=0.5*b`, `ea=0.5*a`, `dd=ea-eb`, `dm=0.5*ds`.
/// Requires `eb == f64::from(F_before_f32)` exactly and
/// `|dd-dm| <= E(dd,dm)`.
pub(super) fn digest_column_delta_consistent_v1(
    h_before_f32: f32,
    h_after_f32: f32,
    f_before_f32: f32,
) -> bool {
    if !h_before_f32.is_finite() || !h_after_f32.is_finite() || !f_before_f32.is_finite() {
        return false;
    }
    let b = f64::from(h_before_f32);
    let a = f64::from(h_after_f32);
    let ds = a - b;
    let eb = 0.5 * b;
    let ea = 0.5 * a;
    let dd = ea - eb;
    let dm = 0.5 * ds;
    eb == f64::from(f_before_f32) && within_envelope_v1(dd, dm)
}

/// Non-digest H gradients and deltas must match F within `E`; digest H
/// gradients must match the exactly converted `0.5f32 * g_F` within `E`.
pub(super) fn digest_gradient_matches_halved_full_v1(h_gradient: f32, f_gradient: f32) -> bool {
    if !h_gradient.is_finite() || !f_gradient.is_finite() {
        return false;
    }
    let expected = f64::from(f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * f_gradient);
    within_envelope_v1(f64::from(h_gradient), expected)
}

// ---------------------------------------------------------------------------
// Primary measurement arithmetic.
// ---------------------------------------------------------------------------

/// Per-stratum f32 group means, in the frozen stratum order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StratumMeansV1 {
    pub(super) promoted2: f32,
    pub(super) predecessor_a: f32,
    pub(super) predecessor_b: f32,
    pub(super) uniform: f32,
}

/// `L(T) = ((((2.0*primary)+pred_a)+pred_b)+uniform)/5.0`, computed in that
/// exact order after converting the four f32 stratum means to f64.
pub(super) fn combined_validation_loss_v1(means: StratumMeansV1) -> f64 {
    let primary = f64::from(means.promoted2);
    let predecessor_a = f64::from(means.predecessor_a);
    let predecessor_b = f64::from(means.predecessor_b);
    let uniform = f64::from(means.uniform);
    ((((2.0 * primary) + predecessor_a) + predecessor_b) + uniform) / 5.0
}

/// The frozen advantage: one f32 subtraction of substep zero's initial value
/// from the terminal return.
pub(super) fn frozen_advantage_v1(terminal_return_i8: i8, predicted_value: f32) -> f32 {
    f32::from(terminal_return_i8) - predicted_value
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ValidationAtomErrorV1 {
    /// Substep count and chosen-action count disagree, or the group is
    /// empty. Every validation group must have at least one substep.
    Shape,
    /// A chosen action index is outside its substep's logit row.
    SelectedOutOfRange,
    /// The production log-softmax rejected the row (nonfinite logit or
    /// nonfinite log-probability).
    ProductionLogSoftmax,
    /// The advantage or the accumulated atom was nonfinite.
    NonFinite,
}

/// The policy-only validation atom,
/// `ell_j(T) = -(sum_k log_softmax_f32(z_jk)[a_jk]) * A_j`.
///
/// Uses the **production** `selected_log_softmax` from
/// `native_policy_train_step_v1` -- the identical function the CPU trainer
/// (`:1391`) and the CUDA bridge (`bridge.rs:683`) call -- so this atom
/// cannot drift from production semantics. Accumulation is canonical
/// left-to-right over substeps.
pub(super) fn validation_atom_v1(
    substep_logits: &[Vec<f32>],
    chosen_actions: &[usize],
    advantage: f32,
) -> Result<f32, ValidationAtomErrorV1> {
    if substep_logits.len() != chosen_actions.len() || substep_logits.is_empty() {
        return Err(ValidationAtomErrorV1::Shape);
    }
    if !advantage.is_finite() {
        return Err(ValidationAtomErrorV1::NonFinite);
    }
    let mut total = 0.0f32;
    for (logits, chosen) in substep_logits.iter().zip(chosen_actions) {
        if logits.is_empty() || *chosen >= logits.len() {
            return Err(ValidationAtomErrorV1::SelectedOutOfRange);
        }
        let (log_probability, _row) = selected_log_softmax(logits, *chosen)
            .map_err(|_| ValidationAtomErrorV1::ProductionLogSoftmax)?;
        total += log_probability;
    }
    let atom = -total * advantage;
    atom.is_finite()
        .then_some(atom)
        .ok_or(ValidationAtomErrorV1::NonFinite)
}

/// `I_s(T) = L_before(T) - L_after(T)`, one f64 subtraction.
pub(super) fn improvement_v1(loss_before: f64, loss_after: f64) -> f64 {
    loss_before - loss_after
}

/// `d_s = I_s(H) - I_s(F)`, one f64 subtraction.
pub(super) fn paired_difference_v1(improvement_half: f64, improvement_full: f64) -> f64 {
    improvement_half - improvement_full
}

// ---------------------------------------------------------------------------
// Exhaustive paired-cluster bootstrap and the classifier.
// ---------------------------------------------------------------------------

/// Enumerates all `6^6 = 46,656` ordered paired-cluster bootstrap means with
/// the last coordinate varying fastest, starting each mean at positive-zero
/// f64, adding the six selected values left-to-right, dividing once by
/// `6.0`, then sorting with `f64::total_cmp`.
///
/// Returns `None` if any input is nonfinite, which the classifier treats as
/// `INVALID`.
pub(super) fn paired_cluster_bootstrap_v1(
    values: &[f64; BOOTSTRAP_UNIT_COUNT_V1],
) -> Option<Vec<f64>> {
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut means = Vec::with_capacity(BOOTSTRAP_TUPLE_COUNT_V1);
    let mut selection = [0usize; BOOTSTRAP_UNIT_COUNT_V1];
    for tuple in 0..BOOTSTRAP_TUPLE_COUNT_V1 {
        let mut remainder = tuple;
        // Last coordinate varies fastest.
        for slot in (0..BOOTSTRAP_UNIT_COUNT_V1).rev() {
            selection[slot] = remainder % BOOTSTRAP_UNIT_COUNT_V1;
            remainder /= BOOTSTRAP_UNIT_COUNT_V1;
        }
        let mut sum = 0.0f64;
        for slot in selection {
            sum += values[slot];
            // A running sum that leaves the finite range is INVALID; it must
            // never be sorted as an infinity.
            if !sum.is_finite() {
                return None;
            }
        }
        let mean = sum / 6.0;
        if !mean.is_finite() {
            return None;
        }
        means.push(mean);
    }
    means.sort_by(f64::total_cmp);
    Some(means)
}

/// The two-sided read of a completed bootstrap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BootstrapReadV1 {
    pub(super) low_index_value: f64,
    pub(super) high_index_value: f64,
}

/// Reads the two frozen quantile indices.
///
/// Independently callable, so it re-establishes its own preconditions: exact
/// length, every value finite, and ascending `total_cmp` order. An unsorted
/// or nonfinite distribution can never yield a read.
pub(super) fn bootstrap_read_v1(sorted_means: &[f64]) -> Option<BootstrapReadV1> {
    if sorted_means.len() != BOOTSTRAP_TUPLE_COUNT_V1 {
        return None;
    }
    if sorted_means.iter().any(|value| !value.is_finite()) {
        return None;
    }
    if sorted_means
        .windows(2)
        .any(|pair| pair[0].total_cmp(&pair[1]).is_gt())
    {
        return None;
    }
    Some(BootstrapReadV1 {
        low_index_value: sorted_means[BOOTSTRAP_LOW_INDEX_V1],
        high_index_value: sorted_means[BOOTSTRAP_HIGH_INDEX_V1],
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DispositionV1 {
    HalfNominated,
    NoNomination,
    Invalid,
}

impl DispositionV1 {
    pub(super) const fn name_v1(self) -> &'static str {
        match self {
            Self::HalfNominated => "HALF-NOMINATED",
            Self::NoNomination => "NO-NOMINATION",
            Self::Invalid => "INVALID",
        }
    }
}

/// Explicit, published evidence for every gate the classifier consulted.
/// Publishing the individual gate outcomes (not just their conjunction) is
/// what lets a reviewer see *why* a disposition was reached and recompute
/// it independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GateEvidenceV1 {
    /// Identity and numerical gates supplied by the measurement driver.
    pub(super) identity_and_numerical_gates_pass: bool,
    /// At least five strictly positive `d_s`.
    pub(super) paired_sign_gate: bool,
    /// The `d` index-1166 value is strictly positive.
    pub(super) paired_interval_gate: bool,
    /// At least five strictly positive promoted(2) `I_s,p2(H)`.
    pub(super) promoted2_sign_gate: bool,
    /// The promoted(2) index-1166 value is strictly positive.
    pub(super) promoted2_interval_gate: bool,
    /// Both bootstraps produced finite, readable distributions.
    pub(super) bootstraps_readable: bool,
}

impl GateEvidenceV1 {
    /// All four nomination conditions plus the driver-supplied gates.
    pub(super) const fn all_pass_v1(self) -> bool {
        self.identity_and_numerical_gates_pass
            && self.paired_sign_gate
            && self.paired_interval_gate
            && self.promoted2_sign_gate
            && self.promoted2_interval_gate
            && self.bootstraps_readable
    }
}

/// The complete, recomputable classifier result. Every derived field is
/// published so a reviewer can recompute it and reject any mismatch.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ClassifierResultV1 {
    pub(super) disposition: DispositionV1,
    pub(super) paired_differences: [f64; BOOTSTRAP_UNIT_COUNT_V1],
    pub(super) promoted2_improvements: [f64; BOOTSTRAP_UNIT_COUNT_V1],
    pub(super) positive_paired_count: usize,
    pub(super) positive_promoted2_count: usize,
    pub(super) paired_read: Option<BootstrapReadV1>,
    pub(super) promoted2_read: Option<BootstrapReadV1>,
    pub(super) gates: GateEvidenceV1,
}

/// Independently recomputes every derived field of a published result from
/// its published inputs and rejects any mismatch.
///
/// This is the forgery check: a result whose stated disposition, sign
/// counts, bootstrap reads, or gate evidence disagree with what its own
/// published `d_s` and `I_s,p2(H)` arrays actually produce is rejected, so
/// a hand-edited summary cannot pass review.
/// Bitwise f64 array equality. Derived `PartialEq` is unusable here: an
/// honest `INVALID` result carrying a NaN raw input would compare unequal to
/// itself, so a genuine result would be rejected as forged.
fn f64_arrays_bit_equal_v1(left: &[f64], right: &[f64]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}

fn bootstrap_reads_bit_equal_v1(
    left: Option<BootstrapReadV1>,
    right: Option<BootstrapReadV1>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.low_index_value.to_bits() == right.low_index_value.to_bits()
                && left.high_index_value.to_bits() == right.high_index_value.to_bits()
        }
        _ => false,
    }
}

pub(super) fn verify_classifier_result_v1(published: &ClassifierResultV1) -> bool {
    let recomputed = classify_v1(
        &published.paired_differences,
        &published.promoted2_improvements,
        published.gates.identity_and_numerical_gates_pass,
    );
    // Discrete fields compare exactly; every f64 compares by raw bits.
    recomputed.disposition == published.disposition
        && recomputed.positive_paired_count == published.positive_paired_count
        && recomputed.positive_promoted2_count == published.positive_promoted2_count
        && recomputed.gates == published.gates
        && f64_arrays_bit_equal_v1(
            &recomputed.paired_differences,
            &published.paired_differences,
        )
        && f64_arrays_bit_equal_v1(
            &recomputed.promoted2_improvements,
            &published.promoted2_improvements,
        )
        && bootstrap_reads_bit_equal_v1(recomputed.paired_read, published.paired_read)
        && bootstrap_reads_bit_equal_v1(recomputed.promoted2_read, published.promoted2_read)
}

/// Classifies one completed six-unit measurement.
///
/// `HALF-NOMINATED` requires all four conditions: at least five strictly
/// positive `d_s`; a strictly positive `d` index-1166 value; at least five
/// strictly positive promoted(2) `I_s,p2(H)`; and a strictly positive
/// promoted(2) index-1166 value -- plus every identity and numerical gate
/// passing, which the caller supplies as `gates_pass`. Every other valid
/// result is `NO-NOMINATION`. `INVALID` has precedence over both.
pub(super) fn classify_v1(
    paired_differences: &[f64; BOOTSTRAP_UNIT_COUNT_V1],
    promoted2_improvements: &[f64; BOOTSTRAP_UNIT_COUNT_V1],
    gates_pass: bool,
) -> ClassifierResultV1 {
    let paired_read = paired_cluster_bootstrap_v1(paired_differences)
        .as_deref()
        .and_then(bootstrap_read_v1);
    let promoted2_read = paired_cluster_bootstrap_v1(promoted2_improvements)
        .as_deref()
        .and_then(bootstrap_read_v1);
    let positive_paired_count = paired_differences
        .iter()
        .filter(|value| **value > 0.0)
        .count();
    let positive_promoted2_count = promoted2_improvements
        .iter()
        .filter(|value| **value > 0.0)
        .count();

    let bootstraps_readable = paired_read.is_some() && promoted2_read.is_some();
    let gates = GateEvidenceV1 {
        identity_and_numerical_gates_pass: gates_pass,
        paired_sign_gate: positive_paired_count >= REQUIRED_POSITIVE_UNITS_V1,
        paired_interval_gate: paired_read.is_some_and(|read| read.low_index_value > 0.0),
        promoted2_sign_gate: positive_promoted2_count >= REQUIRED_POSITIVE_UNITS_V1,
        promoted2_interval_gate: promoted2_read.is_some_and(|read| read.low_index_value > 0.0),
        bootstraps_readable,
    };

    // INVALID has precedence: a failed identity/numerical gate or a
    // nonfinite bootstrap input outranks any nomination arithmetic.
    let disposition = if !gates_pass || !bootstraps_readable {
        DispositionV1::Invalid
    } else if gates.all_pass_v1() {
        DispositionV1::HalfNominated
    } else {
        DispositionV1::NoNomination
    };

    ClassifierResultV1 {
        disposition,
        paired_differences: *paired_differences,
        promoted2_improvements: *promoted2_improvements,
        positive_paired_count,
        positive_promoted2_count,
        paired_read,
        promoted2_read,
        gates,
    }
}

// ---------------------------------------------------------------------------
// Compact versioned framed serializer.
//
// One length-framed atom stream, reusing the crate's established framing
// (`u32be(label_len) || label || u64be(payload_len) || payload`, with
// integer and f32/f64 bit arrays little-endian). Every record opens with a
// version atom and a schema atom, so a schema change cannot silently
// collide with an old digest.
// ---------------------------------------------------------------------------

pub(super) const SERIALIZER_VERSION_V1: &str = "mtg-kernel-action-block-gradient-diagnostic/v1";
pub(super) const SERIALIZER_ENCODING_V1: &str =
    "ordered typed atoms; atom=u32be(label_len)||label||u64be(value_len)||value; integer and f32/f64 bit arrays are little-endian";
pub(super) const TAPE_SCHEMA_V1: &str = "action-block-gradient-tape/v1";
pub(super) const JOINED_TAPE_SCHEMA_V1: &str = "action-block-gradient-joined-tape/v1";
pub(super) const UPDATE_SCHEMA_V1: &str = "action-block-gradient-update/v1";
pub(super) const SUMMARY_SCHEMA_V1: &str = "action-block-gradient-summary/v1";
pub(super) const MANIFEST_SCHEMA_V1: &str = "action-block-gradient-manifest/v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FramedWriterV1 {
    buffer: Vec<u8>,
}

impl FramedWriterV1 {
    pub(super) fn new_v1(schema: &str) -> Self {
        let mut writer = Self { buffer: Vec::new() };
        writer.atom_v1("version", SERIALIZER_VERSION_V1.as_bytes());
        writer.atom_v1("schema", schema.as_bytes());
        writer
    }

    pub(super) fn atom_v1(&mut self, label: &str, payload: &[u8]) {
        let label_length =
            u32::try_from(label.len()).expect("diagnostic atom labels are short literals");
        let payload_length =
            u64::try_from(payload.len()).expect("diagnostic atom payloads fit in u64");
        self.buffer.extend_from_slice(&label_length.to_be_bytes());
        self.buffer.extend_from_slice(label.as_bytes());
        self.buffer.extend_from_slice(&payload_length.to_be_bytes());
        self.buffer.extend_from_slice(payload);
    }

    pub(super) fn text_v1(&mut self, label: &str, value: &str) {
        self.atom_v1(label, value.as_bytes());
    }

    pub(super) fn u64_v1(&mut self, label: &str, value: u64) {
        self.atom_v1(label, &value.to_le_bytes());
    }

    pub(super) fn u32_array_v1(&mut self, label: &str, values: &[u32]) {
        let mut payload = Vec::with_capacity(values.len() * 4);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        self.atom_v1(label, &payload);
    }

    pub(super) fn i64_array_v1(&mut self, label: &str, values: &[i64]) {
        let mut payload = Vec::with_capacity(values.len() * 8);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        self.atom_v1(label, &payload);
    }

    /// f32 values are always framed as their exact bit patterns, never as
    /// decimal text, so no formatting path can round a published value.
    pub(super) fn f32_bits_array_v1(&mut self, label: &str, values: &[f32]) {
        let mut payload = Vec::with_capacity(values.len() * 4);
        for value in values {
            payload.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        self.atom_v1(label, &payload);
    }

    pub(super) fn f64_bits_array_v1(&mut self, label: &str, values: &[f64]) {
        let mut payload = Vec::with_capacity(values.len() * 8);
        for value in values {
            payload.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        self.atom_v1(label, &payload);
    }

    pub(super) fn bytes_v1(&self) -> &[u8] {
        &self.buffer
    }

    pub(super) fn sha256_v1(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.buffer);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Every f32 field of a decision tensor must be finite before it is bound.
fn tensor_f32_fields_finite_v1(tensor: &NativeFlatDecisionTensorV2) -> bool {
    [
        tensor.state.as_slice(),
        tensor.object_features.as_slice(),
        tensor.edge_features.as_slice(),
        tensor.action_features.as_slice(),
        tensor.action_ref_features.as_slice(),
    ]
    .iter()
    .all(|stream| stream.iter().all(|value| value.is_finite()))
}

/// One substep: the atomic scored learner decision.
///
/// A substep's legal-action row is variable length. Nothing here may be
/// equated with a group, an episode, or another substep.
#[derive(Clone, Copy, Debug)]
pub(super) struct TapeSubstepV1<'a> {
    pub(super) learner_ordinal: u64,
    pub(super) selected_index: u32,
    /// Raw logit bits for THIS substep's legal-action row.
    pub(super) raw_action_logit_bits: &'a [u32],
    /// Display-only softmax probabilities reconstructed from the production
    /// log-softmax row. The loss consumes the separately framed selected
    /// log-probability bits, never this display row.
    pub(super) display_softmax_probability_bits: &'a [u32],
    pub(super) selected_log_probability_bits: u32,
    /// Substep zero's value is what the frozen group advantage subtracts.
    pub(super) predicted_value_bits: u32,
    /// The retained source action-core rows this substep scored.
    pub(super) source_action_cores: &'a [FlatScorerActionCoreV2],
    /// The repaired-FULL tensor for THIS substep; all thirteen fields bound.
    pub(super) repaired_tensor: &'a NativeFlatDecisionTensorV2,
}

impl TapeSubstepV1<'_> {
    /// The substep's own action-row cardinality, which every per-action
    /// stream on this substep must match exactly.
    fn action_row_len_v1(&self) -> usize {
        self.source_action_cores.len()
    }

    fn well_formed_v1(&self) -> bool {
        let actions = self.action_row_len_v1();
        actions > 0
            && self.raw_action_logit_bits.len() == actions
            && self.display_softmax_probability_bits.len() == actions
            && (self.selected_index as usize) < actions
            && self.repaired_tensor.action_features.len() == actions * ACTION_FEATURE_DIM_V1
            && f32::from_bits(self.predicted_value_bits).is_finite()
            && f32::from_bits(self.selected_log_probability_bits).is_finite()
            && self
                .raw_action_logit_bits
                .iter()
                .all(|bits| f32::from_bits(*bits).is_finite())
            && self
                .display_softmax_probability_bits
                .iter()
                .all(|bits| f32::from_bits(*bits).is_finite())
            && tensor_f32_fields_finite_v1(self.repaired_tensor)
    }
}

/// One learner physical decision group: one or more substeps.
#[derive(Clone, Debug)]
pub(super) struct TapeGroupV1<'a> {
    pub(super) physical_decision_id: u64,
    pub(super) substeps: Vec<TapeSubstepV1<'a>>,
}

impl TapeGroupV1<'_> {
    /// The frozen advantage for this group: its episode's terminal return
    /// minus substep zero's initial predicted value, one f32 subtraction.
    /// Advantages are per group, never per substep and never per episode.
    pub(super) fn frozen_advantage_v1(&self, terminal_return: i8) -> Option<f32> {
        let first = self.substeps.first()?;
        let advantage =
            frozen_advantage_v1(terminal_return, f32::from_bits(first.predicted_value_bits));
        advantage.is_finite().then_some(advantage)
    }

    fn well_formed_v1(&self) -> bool {
        !self.substeps.is_empty() && self.substeps.iter().all(TapeSubstepV1::well_formed_v1)
    }
}

/// One episode. Zero learner groups is legal: an episode in which the
/// learner never faced a physical decision still belongs on the tape.
#[derive(Clone, Debug)]
pub(super) struct TapeEpisodeV1<'a> {
    pub(super) episode_id: u64,
    /// Pool3 stratum ordinal drawn by the production schedule.
    pub(super) stratum: u32,
    pub(super) learner_return: i8,
    pub(super) trace_hash: u64,
    pub(super) groups: Vec<TapeGroupV1<'a>>,
}

/// One side of a unit's tape: either the training tape or the independent
/// validation tape, preserved as the observer's
/// `episodes -> groups -> substeps` hierarchy.
#[derive(Clone, Debug)]
pub(super) struct TapeSideV1<'a> {
    pub(super) seed: u64,
    pub(super) counts: PoolCountsV1,
    pub(super) episodes: Vec<TapeEpisodeV1<'a>>,
}

impl TapeSideV1<'_> {
    /// Learner physical decision groups per Pool3 stratum, derived from the
    /// bound hierarchy itself rather than accepted as a caller assertion.
    pub(super) fn groups_per_stratum_v1(&self) -> [u32; 4] {
        let mut per_stratum = [0u32; 4];
        for episode in &self.episodes {
            if (episode.stratum as usize) < per_stratum.len() {
                per_stratum[episode.stratum as usize] += episode.groups.len() as u32;
            }
        }
        per_stratum
    }

    pub(super) fn total_group_count_v1(&self) -> usize {
        self.episodes
            .iter()
            .map(|episode| episode.groups.len())
            .sum()
    }

    pub(super) fn total_substep_count_v1(&self) -> usize {
        self.episodes
            .iter()
            .flat_map(|episode| &episode.groups)
            .map(|group| group.substeps.len())
            .sum()
    }
}

/// Binds one substep's source action-core row set, in canonical field order.
fn frame_action_cores_v1(
    writer: &mut FramedWriterV1,
    label: &str,
    cores: &[FlatScorerActionCoreV2],
) {
    writer.u64_v1(&format!("{label}.count"), cores.len() as u64);
    let mut payload = Vec::with_capacity(cores.len() * 40);
    for core in cores {
        payload.extend_from_slice(&(core.kind as u32).to_le_bytes());
        payload.extend_from_slice(&core.flags.to_le_bytes());
        payload.push(core.ability_index);
        payload.push(core.remaining);
        payload.push(core.mode_index);
        payload.push(core.mode_count);
        payload.extend_from_slice(&core.option_index.to_le_bytes());
        payload.extend_from_slice(&core.option_count.to_le_bytes());
        payload.extend_from_slice(&core.selected_count.to_le_bytes());
        payload.extend_from_slice(&core.min_targets.to_le_bytes());
        payload.extend_from_slice(&core.max_targets.to_le_bytes());
        payload.extend_from_slice(&core.number.to_le_bytes());
        payload.extend_from_slice(&core.minimum.to_le_bytes());
        payload.extend_from_slice(&core.maximum.to_le_bytes());
        payload.push(core.mana_choice);
        payload.push(core.color);
        payload.push(core.cast_mode);
        payload.push(core.cost_kind);
        payload.push(core.optional_cost_choice);
        payload.push(core.target_kind);
        payload.push(core.target_player);
        payload.extend_from_slice(&core.ref_start.to_le_bytes());
        payload.extend_from_slice(&core.ref_len.to_le_bytes());
    }
    writer.atom_v1(label, &payload);
}

/// Binds all thirteen `NativeFlatDecisionTensorV2` fields for one substep.
fn frame_decision_tensor_v1(
    writer: &mut FramedWriterV1,
    label: &str,
    tensor: &NativeFlatDecisionTensorV2,
) {
    writer.f32_bits_array_v1(&format!("{label}.state"), &tensor.state);
    writer.f32_bits_array_v1(&format!("{label}.object_features"), &tensor.object_features);
    writer.i64_array_v1(&format!("{label}.object_card_ids"), &tensor.object_card_ids);
    writer.i64_array_v1(&format!("{label}.object_groups"), &tensor.object_groups);
    writer.i64_array_v1(&format!("{label}.object_node_ids"), &tensor.object_node_ids);
    writer.f32_bits_array_v1(&format!("{label}.edge_features"), &tensor.edge_features);
    writer.i64_array_v1(
        &format!("{label}.edge_source_indices"),
        &tensor.edge_source_indices,
    );
    writer.i64_array_v1(
        &format!("{label}.edge_target_indices"),
        &tensor.edge_target_indices,
    );
    writer.f32_bits_array_v1(&format!("{label}.action_features"), &tensor.action_features);
    writer.f32_bits_array_v1(
        &format!("{label}.action_ref_features"),
        &tensor.action_ref_features,
    );
    writer.i64_array_v1(
        &format!("{label}.action_ref_card_ids"),
        &tensor.action_ref_card_ids,
    );
    writer.i64_array_v1(
        &format!("{label}.action_ref_action_indices"),
        &tensor.action_ref_action_indices,
    );
    writer.i64_array_v1(
        &format!("{label}.action_ref_node_indices"),
        &tensor.action_ref_node_indices,
    );
}

/// Binds one side of the tape record-by-record, preserving the observer's
/// hierarchy: every episode declares its group count, every group declares
/// its substep count, and every substep declares its action-row count, so
/// the decision boundaries are recoverable from the stream alone.
fn frame_tape_side_v1(writer: &mut FramedWriterV1, side: &str, tape: &TapeSideV1<'_>) {
    writer.text_v1("side", side);
    writer.u64_v1(&format!("{side}.seed"), tape.seed);
    writer.u32_array_v1(&format!("{side}.counts"), &tape.counts.as_array_v1());
    writer.u64_v1(&format!("{side}.episode_count"), tape.episodes.len() as u64);
    writer.u64_v1(
        &format!("{side}.total_group_count"),
        tape.total_group_count_v1() as u64,
    );
    writer.u64_v1(
        &format!("{side}.total_substep_count"),
        tape.total_substep_count_v1() as u64,
    );
    writer.u32_array_v1(
        &format!("{side}.groups_per_stratum"),
        &tape.groups_per_stratum_v1(),
    );

    for (episode_index, episode) in tape.episodes.iter().enumerate() {
        let episode_label = format!("{side}.episode[{episode_index}]");
        writer.u64_v1(&format!("{episode_label}.episode_id"), episode.episode_id);
        writer.u32_array_v1(&format!("{episode_label}.stratum"), &[episode.stratum]);
        writer.atom_v1(
            &format!("{episode_label}.learner_return"),
            &[episode.learner_return as u8],
        );
        writer.u64_v1(&format!("{episode_label}.trace_hash"), episode.trace_hash);
        // Episode-to-group boundary.
        writer.u64_v1(
            &format!("{episode_label}.group_count"),
            episode.groups.len() as u64,
        );

        for (group_index, group) in episode.groups.iter().enumerate() {
            let group_label = format!("{episode_label}.group[{group_index}]");
            writer.u64_v1(
                &format!("{group_label}.physical_decision_id"),
                group.physical_decision_id,
            );
            // Group-to-substep boundary.
            writer.u64_v1(
                &format!("{group_label}.substep_count"),
                group.substeps.len() as u64,
            );
            // The frozen advantage is a per-group quantity derived from this
            // episode's return and this group's substep zero.
            let advantage = group
                .frozen_advantage_v1(episode.learner_return)
                .unwrap_or(f32::NAN);
            writer.f32_bits_array_v1(&format!("{group_label}.frozen_advantage"), &[advantage]);

            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let substep_label = format!("{group_label}.substep[{substep_index}]");
                writer.u64_v1(
                    &format!("{substep_label}.learner_ordinal"),
                    substep.learner_ordinal,
                );
                // Substep-to-action boundary.
                writer.u64_v1(
                    &format!("{substep_label}.action_row_len"),
                    substep.action_row_len_v1() as u64,
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.selected_index"),
                    &[substep.selected_index],
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.raw_action_logit_bits"),
                    substep.raw_action_logit_bits,
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.display_softmax_probability_bits"),
                    substep.display_softmax_probability_bits,
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.selected_log_probability_bits"),
                    &[substep.selected_log_probability_bits],
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.predicted_value_bits"),
                    &[substep.predicted_value_bits],
                );
                frame_action_cores_v1(
                    writer,
                    &format!("{substep_label}.source_action_cores"),
                    substep.source_action_cores,
                );
                frame_decision_tensor_v1(
                    writer,
                    &format!("{substep_label}.repaired_tensor"),
                    substep.repaired_tensor,
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TapeFramingErrorV1 {
    /// The tape did not carry exactly 64 episode records.
    EpisodeCardinality,
    /// A group had no substeps, or a substep was internally inconsistent
    /// (empty action row, out-of-range selection, stream length disagreeing
    /// with its own action-row length, or a nonfinite published value).
    MalformedHierarchy,
    /// An episode's stratum ordinal was out of range, or the declared counts
    /// do not total 64.
    MalformedStrata,
    /// The bound episode strata do not match the production schedule's own
    /// draw for that seed. This stops a tape claiming strata it never played.
    CountsDisagreeWithSchedule,
    /// A validation stratum contains no learner physical decision group, so
    /// its stratum mean would be taken over an empty set.
    EmptyStratumGroup,
    /// A group's frozen advantage was not derivable or was nonfinite.
    UnderivableAdvantage,
    /// A joined receipt was not an exact environment-randomization-V2
    /// receipt for its production schedule, catalog-resolved decks, split
    /// counts, inner digest, and independently rebuilt outer envelope.
    MalformedReceipt,
    /// A joined expected decision or complete observed/retained binding did
    /// not re-establish the production contract and hierarchy coordinates.
    BindingMismatch,
    /// Joined action rows, outputs, choice, action seed, or treatment tensors
    /// disagreed or contained a nonfinite primary CPU value.
    OutputMismatch,
}

/// Frames one unit's neutral tape identity, binding the training and the
/// validation side separately and completely.
///
/// F and H share one tape because both derive from the same retained
/// pre-repair baseline; the tape therefore binds the single repaired-FULL
/// tensor per substep, not two.
///
/// The validation gate is derived, not asserted: per-stratum learner group
/// counts come from the bound hierarchy, and every stratum must contain at
/// least one group. Episodes with zero learner groups are legal.
pub(super) fn frame_tape_v1(
    training: &TapeSideV1<'_>,
    validation: &TapeSideV1<'_>,
) -> Result<FramedWriterV1, TapeFramingErrorV1> {
    for side in [training, validation] {
        if side.episodes.len() as u64 != EPISODES_PER_TAPE_V1 {
            return Err(TapeFramingErrorV1::EpisodeCardinality);
        }
        if side.counts.total_v1() != EPISODES_PER_TAPE_V1 as u32
            || side.episodes.iter().any(|episode| episode.stratum >= 4)
        {
            return Err(TapeFramingErrorV1::MalformedStrata);
        }
        // The bound strata must be exactly what the production schedule
        // draws for that seed, episode by episode in absolute order.
        for (episode_index, episode) in side.episodes.iter().enumerate() {
            let scheduled = ladder_pool_member_for_episode_v1(side.seed, episode_index as u64)
                .map_err(|_| TapeFramingErrorV1::CountsDisagreeWithSchedule)?;
            if episode.stratum != stratum_ordinal_v1(scheduled) {
                return Err(TapeFramingErrorV1::CountsDisagreeWithSchedule);
            }
        }
        let mut recounted = [0u32; 4];
        for episode in &side.episodes {
            recounted[episode.stratum as usize] += 1;
        }
        if recounted != side.counts.as_array_v1() {
            return Err(TapeFramingErrorV1::CountsDisagreeWithSchedule);
        }
        // Every group must have at least one substep and every substep must
        // be internally consistent. Zero-group episodes remain legal.
        for episode in &side.episodes {
            for group in &episode.groups {
                if !group.well_formed_v1() {
                    return Err(TapeFramingErrorV1::MalformedHierarchy);
                }
                if group.frozen_advantage_v1(episode.learner_return).is_none() {
                    return Err(TapeFramingErrorV1::UnderivableAdvantage);
                }
            }
        }
    }
    // Derived, not asserted: every validation stratum needs a real learner
    // physical decision group.
    if validation.groups_per_stratum_v1().contains(&0) {
        return Err(TapeFramingErrorV1::EmptyStratumGroup);
    }

    let mut writer = FramedWriterV1::new_v1(TAPE_SCHEMA_V1);
    writer.u64_v1("episodes_per_tape", EPISODES_PER_TAPE_V1);
    frame_tape_side_v1(&mut writer, "training", training);
    frame_tape_side_v1(&mut writer, "validation", validation);
    Ok(writer)
}

/// Binds the complete V2 decision binding, including the candidate-order
/// commitment and all seven production semantic digests.
fn frame_binding_v1(writer: &mut FramedWriterV1, label: &str, binding: FlatDecisionBindingV2) {
    let action = binding.action_binding;
    writer.u32_array_v1(&format!("{label}.slice_version"), &[action.slice_version]);
    writer.u32_array_v1(
        &format!("{label}.ref_role_mapping_version"),
        &[action.ref_role_mapping_version],
    );
    writer.u32_array_v1(
        &format!("{label}.card_token_mapping_version"),
        &[action.card_token_mapping_version],
    );
    writer.u32_array_v1(
        &format!("{label}.candidate_commitment_version"),
        &[action.candidate_commitment_version],
    );
    writer.u64_v1(&format!("{label}.card_db_hash"), action.card_db_hash);
    writer.u64_v1(&format!("{label}.episode_id"), action.episode_id);
    writer.u64_v1(
        &format!("{label}.environment_revision"),
        action.environment_revision,
    );
    writer.u64_v1(
        &format!("{label}.bound_policy_step_count"),
        action.bound_policy_step_count,
    );
    writer.u64_v1(
        &format!("{label}.physical_decision_id"),
        action.physical_decision_id,
    );
    writer.u64_v1(
        &format!("{label}.bound_physical_decision_count"),
        action.bound_physical_decision_count,
    );
    writer.u32_array_v1(&format!("{label}.substep_index"), &[action.substep_index]);
    writer.u32_array_v1(&format!("{label}.substep_count"), &[action.substep_count]);
    writer.u32_array_v1(
        &format!("{label}.acting_player"),
        &[u32::from(action.acting_player)],
    );
    writer.u32_array_v1(
        &format!("{label}.decision_kind"),
        &[u32::from(action.decision_kind)],
    );
    writer.u32_array_v1(
        &format!("{label}.legal_action_count"),
        &[action.legal_action_count],
    );
    writer.atom_v1(
        &format!("{label}.candidate_order_commitment"),
        &action.candidate_order_commitment,
    );
    writer.u32_array_v1(
        &format!("{label}.typed_layout_version"),
        &[binding.typed_layout_version],
    );
    writer.u32_array_v1(
        &format!("{label}.feature_inventory_version"),
        &[binding.feature_inventory_version],
    );
    writer.u32_array_v1(
        &format!("{label}.enum_mapping_version"),
        &[binding.enum_mapping_version],
    );
    writer.u32_array_v1(
        &format!("{label}.object_group_mapping_version"),
        &[binding.object_group_mapping_version],
    );
    writer.u32_array_v1(
        &format!("{label}.relation_role_mapping_version"),
        &[binding.relation_role_mapping_version],
    );
    writer.u32_array_v1(
        &format!("{label}.context_subrole_mapping_version"),
        &[binding.context_subrole_mapping_version],
    );
    writer.u32_array_v1(
        &format!("{label}.action_ref_projection_role_mapping_version"),
        &[binding.action_ref_projection_role_mapping_version],
    );
    let digests = binding.contract_digests;
    writer.atom_v1(&format!("{label}.mapping_sha256"), &digests.mapping_sha256);
    writer.atom_v1(
        &format!("{label}.feature_inventory_sha256"),
        &digests.feature_inventory_sha256,
    );
    writer.atom_v1(
        &format!("{label}.base_typed_layout_sha256"),
        &digests.base_typed_layout_sha256,
    );
    writer.atom_v1(
        &format!("{label}.overlay_typed_layout_sha256"),
        &digests.overlay_typed_layout_sha256,
    );
    writer.atom_v1(
        &format!("{label}.typed_layout_sha256"),
        &digests.typed_layout_sha256,
    );
    writer.atom_v1(
        &format!("{label}.action_contract_source_sha256"),
        &digests.action_contract_source_sha256,
    );
    writer.atom_v1(
        &format!("{label}.action_contract_sha256"),
        &digests.action_contract_sha256,
    );
}

fn frame_expected_decision_v1(
    writer: &mut FramedWriterV1,
    label: &str,
    expected: FastActorDecisionV1,
) {
    writer.u64_v1(&format!("{label}.episode_id"), expected.episode_id);
    writer.u64_v1(&format!("{label}.step"), expected.step);
    writer.u64_v1(
        &format!("{label}.environment_revision"),
        expected.environment_revision,
    );
    writer.u64_v1(
        &format!("{label}.physical_decision_id"),
        expected.physical_decision_id,
    );
    writer.u32_array_v1(&format!("{label}.substep_index"), &[expected.substep_index]);
    writer.u32_array_v1(&format!("{label}.substep_count"), &[expected.substep_count]);
    writer.u32_array_v1(
        &format!("{label}.acting_player"),
        &[u32::from(player_seat_code(expected.acting_player))],
    );
    writer.u32_array_v1(
        &format!("{label}.decision_kind"),
        &[u32::from(decision_kind_code(expected.decision_kind))],
    );
    writer.u32_array_v1(
        &format!("{label}.legal_action_count"),
        &[expected.legal_action_count],
    );
}

fn validate_joined_receipt_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    episode: &JoinedEpisodeV1,
) -> Result<(), TapeFramingErrorV1> {
    let learner_policy_step_count = episode
        .groups
        .iter()
        .try_fold(0u64, |sum, group| {
            u64::try_from(group.substeps.len())
                .ok()
                .and_then(|count| sum.checked_add(count))
        })
        .ok_or(TapeFramingErrorV1::MalformedReceipt)?;
    let learner_physical_decision_count =
        u64::try_from(episode.groups.len()).map_err(|_| TapeFramingErrorV1::MalformedReceipt)?;
    if episode.receipt_expected_facts.episode_id != episode.episode_id
        || episode.receipt_expected_facts.learner_policy_step_count != learner_policy_step_count
        || episode
            .receipt_expected_facts
            .learner_physical_decision_count
            != learner_physical_decision_count
    {
        return Err(TapeFramingErrorV1::MalformedReceipt);
    }
    validate_preflight_receipt_v1(
        authority,
        expected_deck_ids,
        expected_deck_hashes,
        episode.receipt_expected_facts,
        &episode.receipt,
    )
    .map_err(|_| TapeFramingErrorV1::MalformedReceipt)
}

fn frame_joined_receipt_v1(
    writer: &mut FramedWriterV1,
    label: &str,
    receipt: &NativeTrainingTrajectoryReceiptV2,
) -> Result<(), TapeFramingErrorV1> {
    let pair_index = receipt
        .pair_index_v2()
        .ok_or(TapeFramingErrorV1::MalformedReceipt)?;
    let deck_ids = receipt
        .deck_ids_v2()
        .ok_or(TapeFramingErrorV1::MalformedReceipt)?;
    let outer = receipt
        .outer_trajectory_sha256_v2()
        .ok_or(TapeFramingErrorV1::MalformedReceipt)?;
    writer.u32_array_v1(&format!("{label}.variant"), &[2]);
    writer.u64_v1(&format!("{label}.episode_index"), receipt.episode_index());
    writer.u64_v1(&format!("{label}.pair_index"), pair_index);
    writer.u64_v1(
        &format!("{label}.pair_environment_seed"),
        receipt.environment_seed(),
    );
    writer.text_v1(&format!("{label}.deck_id[0]"), deck_ids[0]);
    writer.text_v1(&format!("{label}.deck_id[1]"), deck_ids[1]);
    writer.u64_v1(&format!("{label}.deck_hash[0]"), receipt.deck_hashes()[0]);
    writer.u64_v1(&format!("{label}.deck_hash[1]"), receipt.deck_hashes()[1]);
    writer.u32_array_v1(
        &format!("{label}.learner_seat"),
        &[u32::from(player_seat_code(receipt.learner_seat()))],
    );
    writer.u64_v1(
        &format!("{label}.policy_step_count"),
        receipt.policy_step_count(),
    );
    writer.u64_v1(
        &format!("{label}.physical_decision_count"),
        receipt.physical_decision_count(),
    );
    writer.u64_v1(
        &format!("{label}.learner_policy_step_count"),
        receipt.learner_policy_step_count(),
    );
    writer.u64_v1(
        &format!("{label}.opponent_policy_step_count"),
        receipt.opponent_policy_step_count(),
    );
    writer.u64_v1(
        &format!("{label}.learner_physical_decision_count"),
        receipt.learner_physical_decision_count(),
    );
    writer.u64_v1(
        &format!("{label}.opponent_physical_decision_count"),
        receipt.opponent_physical_decision_count(),
    );
    writer.atom_v1(
        &format!("{label}.inner_trajectory_sha256"),
        &receipt.trajectory_sha256(),
    );
    writer.atom_v1(&format!("{label}.outer_trajectory_sha256"), &outer);
    Ok(())
}

fn validate_joined_tape_side_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    tape: &JoinedTapeV1,
) -> Result<(), TapeFramingErrorV1> {
    if tape.base_seed != authority.seed_v1()
        || tape.counts.as_array_v1() != authority.expected_counts_v1()
    {
        return Err(TapeFramingErrorV1::CountsDisagreeWithSchedule);
    }
    if tape.episodes.len() as u64 != EPISODES_PER_TAPE_V1 {
        return Err(TapeFramingErrorV1::EpisodeCardinality);
    }
    if tape.counts.total_v1() != EPISODES_PER_TAPE_V1 as u32
        || tape.episodes.iter().any(|episode| episode.stratum >= 4)
    {
        return Err(TapeFramingErrorV1::MalformedStrata);
    }
    let mut recounted = [0u32; 4];
    let mut sampler = FastCategoricalScratch::default();
    for (episode_index, episode) in tape.episodes.iter().enumerate() {
        let mut previous_ordinal: Option<u64> = None;
        let scheduled_member =
            ladder_pool_member_for_episode_v1(tape.base_seed, episode_index as u64)
                .map_err(|_| TapeFramingErrorV1::CountsDisagreeWithSchedule)?;
        let schedule = native_trainer_episode_schedule_v1(tape.base_seed, episode_index as u64)
            .map_err(|_| TapeFramingErrorV1::CountsDisagreeWithSchedule)?;
        let terminal = episode.authenticated_terminal.terminal_v1();
        if episode.episode_id != episode_index as u64
            || episode.stratum != stratum_ordinal_v1(scheduled_member)
            || episode.receipt_expected_facts.learner_seat != schedule.learner_seat
            || !episode
                .authenticated_terminal
                .relation_valid_v1(episode.episode_id, schedule.learner_seat)
            || terminal.policy_step_count != episode.receipt_expected_facts.policy_step_count
            || terminal.physical_decision_count
                != episode.receipt_expected_facts.physical_decision_count
            || episode.receipt_expected_facts.opponent_policy_step_count
                < episode
                    .receipt_expected_facts
                    .opponent_physical_decision_count
        {
            return Err(TapeFramingErrorV1::CountsDisagreeWithSchedule);
        }
        recounted[episode.stratum as usize] += 1;
        validate_joined_receipt_v1(authority, expected_deck_ids, expected_deck_hashes, episode)?;

        let mut previous_physical_decision_id: Option<u64> = None;
        for (group_index, group) in episode.groups.iter().enumerate() {
            if group.substeps.is_empty()
                || previous_physical_decision_id
                    .is_some_and(|previous| group.physical_decision_id <= previous)
                || group.physical_decision_id >= terminal.physical_decision_count
            {
                return Err(TapeFramingErrorV1::MalformedHierarchy);
            }
            previous_physical_decision_id = Some(group.physical_decision_id);
            let group_substep_count = u32::try_from(group.substeps.len())
                .map_err(|_| TapeFramingErrorV1::MalformedHierarchy)?;
            let group_actor = group.substeps[0].expected.acting_player;
            if group_actor != episode.receipt_expected_facts.learner_seat {
                return Err(TapeFramingErrorV1::BindingMismatch);
            }
            let mut first_value: Option<f32> = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let exact_substep_index = u32::try_from(substep_index)
                    .map_err(|_| TapeFramingErrorV1::MalformedHierarchy)?;
                let expected_ordinal = previous_ordinal.map_or(0, |previous| previous + 1);
                if substep.learner_ordinal != expected_ordinal
                    || substep.expected.substep_index != exact_substep_index
                    || substep.expected.acting_player != group_actor
                    || substep.expected.step >= terminal.policy_step_count
                {
                    return Err(TapeFramingErrorV1::MalformedHierarchy);
                }
                previous_ordinal = Some(substep.learner_ordinal);
                validate_join_binding_v1(
                    substep.binding,
                    substep.retained.binding,
                    substep.expected,
                    episode.episode_id,
                    group.physical_decision_id,
                    group_substep_count,
                    group_actor,
                    substep.retained.lineage.source_action_cores_v1().len(),
                )
                .map_err(|_| TapeFramingErrorV1::BindingMismatch)?;
                let action_count = substep.retained.lineage.source_action_cores_v1().len();
                if action_count == 0
                    || substep.raw_action_logit_bits.len() != action_count
                    || substep.retained.initial_logits.len() != action_count
                    || substep
                        .retained
                        .lineage
                        .pre_repair_action_features_v1()
                        .len()
                        != action_count * ACTION_FEATURE_DIM_V1
                    || substep
                        .retained
                        .lineage
                        .full_tensor_v1()
                        .action_features
                        .len()
                        != action_count * ACTION_FEATURE_DIM_V1
                    || substep
                        .retained
                        .lineage
                        .half_tensor_v1()
                        .action_features
                        .len()
                        != action_count * ACTION_FEATURE_DIM_V1
                    || substep.selected_index as usize >= action_count
                    || substep.predicted_value_bits != substep.retained.initial_value.to_bits()
                    || substep
                        .raw_action_logit_bits
                        .iter()
                        .zip(&substep.retained.initial_logits)
                        .any(|(observed, retained)| *observed != retained.to_bits())
                    || !substep.retained.initial_value.is_finite()
                    || substep
                        .retained
                        .lineage
                        .pre_repair_action_features_v1()
                        .iter()
                        .any(|value| !value.is_finite())
                    || !tensor_f32_fields_finite_v1(substep.retained.lineage.full_tensor_v1())
                    || !tensor_f32_fields_finite_v1(substep.retained.lineage.half_tensor_v1())
                    || !substep.retained.lineage.relation_valid_v1()
                {
                    return Err(TapeFramingErrorV1::OutputMismatch);
                }
                let scheduled_action_seed = derive_native_trainer_learner_action_seed_v1(
                    tape.base_seed,
                    episode.episode_id,
                    group_index as u64,
                    exact_substep_index,
                )
                .map_err(|_| TapeFramingErrorV1::OutputMismatch)?;
                if substep.action_seed != scheduled_action_seed {
                    return Err(TapeFramingErrorV1::OutputMismatch);
                }
                let logits: Vec<f32> = substep
                    .raw_action_logit_bits
                    .iter()
                    .map(|bits| f32::from_bits(*bits))
                    .collect();
                if logits.iter().any(|value| !value.is_finite())
                    || sampler
                        .sample(&logits, substep.action_seed)
                        .map_err(|_| TapeFramingErrorV1::OutputMismatch)?
                        != substep.selected_index as usize
                {
                    return Err(TapeFramingErrorV1::OutputMismatch);
                }
                let (selected_log_probability, _row) =
                    selected_log_softmax(&logits, substep.selected_index as usize)
                        .map_err(|_| TapeFramingErrorV1::OutputMismatch)?;
                if selected_log_probability.to_bits() != substep.selected_log_probability_bits {
                    return Err(TapeFramingErrorV1::OutputMismatch);
                }
                first_value.get_or_insert(substep.retained.initial_value);
            }
            let first_value = first_value.ok_or(TapeFramingErrorV1::UnderivableAdvantage)?;
            if !frozen_advantage_v1(
                episode.authenticated_terminal.learner_return_v1(),
                first_value,
            )
            .is_finite()
            {
                return Err(TapeFramingErrorV1::UnderivableAdvantage);
            }
        }
    }
    if recounted != tape.counts.as_array_v1() {
        return Err(TapeFramingErrorV1::CountsDisagreeWithSchedule);
    }
    if tape.total_group_count_v1() == 0 {
        return Err(TapeFramingErrorV1::MalformedHierarchy);
    }
    Ok(())
}

fn frame_joined_tape_side_v1(
    writer: &mut FramedWriterV1,
    side: &str,
    tape: &JoinedTapeV1,
) -> Result<(), TapeFramingErrorV1> {
    writer.text_v1("side", side);
    writer.u64_v1(&format!("{side}.seed"), tape.base_seed);
    writer.u32_array_v1(&format!("{side}.counts"), &tape.counts.as_array_v1());
    writer.u64_v1(&format!("{side}.episode_count"), tape.episodes.len() as u64);
    writer.u64_v1(
        &format!("{side}.total_group_count"),
        tape.total_group_count_v1() as u64,
    );
    writer.u64_v1(
        &format!("{side}.total_substep_count"),
        tape.total_substep_count_v1() as u64,
    );
    writer.u32_array_v1(
        &format!("{side}.groups_per_stratum"),
        &tape.groups_per_stratum_v1(),
    );
    for (episode_index, episode) in tape.episodes.iter().enumerate() {
        let episode_label = format!("{side}.episode[{episode_index}]");
        writer.u64_v1(&format!("{episode_label}.episode_id"), episode.episode_id);
        writer.u32_array_v1(&format!("{episode_label}.stratum"), &[episode.stratum]);
        writer.atom_v1(
            &format!("{episode_label}.learner_return"),
            &[episode.authenticated_terminal.learner_return_v1() as u8],
        );
        writer.u64_v1(&format!("{episode_label}.trace_hash"), episode.trace_hash);
        frame_joined_receipt_v1(
            writer,
            &format!("{episode_label}.receipt"),
            &episode.receipt,
        )?;
        writer.u64_v1(
            &format!("{episode_label}.group_count"),
            episode.groups.len() as u64,
        );
        for (group_index, group) in episode.groups.iter().enumerate() {
            let group_label = format!("{episode_label}.group[{group_index}]");
            writer.u64_v1(
                &format!("{group_label}.physical_decision_id"),
                group.physical_decision_id,
            );
            writer.u64_v1(
                &format!("{group_label}.substep_count"),
                group.substeps.len() as u64,
            );
            let advantage = frozen_advantage_v1(
                episode.authenticated_terminal.learner_return_v1(),
                group.substeps[0].retained.initial_value,
            );
            writer.f32_bits_array_v1(&format!("{group_label}.frozen_advantage"), &[advantage]);
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let substep_label = format!("{group_label}.substep[{substep_index}]");
                writer.u64_v1(
                    &format!("{substep_label}.learner_ordinal"),
                    substep.learner_ordinal,
                );
                frame_expected_decision_v1(
                    writer,
                    &format!("{substep_label}.expected"),
                    substep.expected,
                );
                frame_binding_v1(writer, &format!("{substep_label}.binding"), substep.binding);
                writer.u64_v1(&format!("{substep_label}.action_seed"), substep.action_seed);
                writer.u64_v1(
                    &format!("{substep_label}.action_row_len"),
                    substep.retained.lineage.source_action_cores_v1().len() as u64,
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.selected_index"),
                    &[substep.selected_index],
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.observer_raw_action_logit_bits"),
                    &substep.raw_action_logit_bits,
                );
                let logits: Vec<f32> = substep
                    .raw_action_logit_bits
                    .iter()
                    .map(|bits| f32::from_bits(*bits))
                    .collect();
                let (_selected, log_softmax_row) =
                    selected_log_softmax(&logits, substep.selected_index as usize)
                        .map_err(|_| TapeFramingErrorV1::OutputMismatch)?;
                let display_softmax_probability_bits: Vec<u32> = log_softmax_row
                    .iter()
                    .map(|log_probability| log_probability.exp().to_bits())
                    .collect();
                writer.u32_array_v1(
                    &format!("{substep_label}.display_softmax_probability_bits"),
                    &display_softmax_probability_bits,
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.selected_log_probability_bits"),
                    &[substep.selected_log_probability_bits],
                );
                writer.u32_array_v1(
                    &format!("{substep_label}.observer_predicted_value_bits"),
                    &[substep.predicted_value_bits],
                );
                frame_action_cores_v1(
                    writer,
                    &format!("{substep_label}.source_action_cores"),
                    substep.retained.lineage.source_action_cores_v1(),
                );
                writer.f32_bits_array_v1(
                    &format!("{substep_label}.pre_repair_action_features"),
                    substep.retained.lineage.pre_repair_action_features_v1(),
                );
                frame_decision_tensor_v1(
                    writer,
                    &format!("{substep_label}.full_tensor"),
                    substep.retained.lineage.full_tensor_v1(),
                );
                frame_decision_tensor_v1(
                    writer,
                    &format!("{substep_label}.half_tensor"),
                    substep.retained.lineage.half_tensor_v1(),
                );
                writer.f32_bits_array_v1(
                    &format!("{substep_label}.retained_initial_logits"),
                    &substep.retained.initial_logits,
                );
                writer.f32_bits_array_v1(
                    &format!("{substep_label}.retained_initial_value"),
                    &[substep.retained.initial_value],
                );
            }
        }
    }
    Ok(())
}

/// Canonical tape framing consumes the joined evidence object itself. This
/// prevents a caller from projecting away receipt facts, complete bindings,
/// the pre-repair row, or either treatment tensor before hashing.
pub(super) fn frame_joined_tape_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    tape: &JoinedTapeV1,
) -> Result<FramedWriterV1, TapeFramingErrorV1> {
    validate_joined_tape_side_v1(authority, expected_deck_ids, expected_deck_hashes, tape)?;
    let mut writer = FramedWriterV1::new_v1(JOINED_TAPE_SCHEMA_V1);
    writer.u64_v1("episodes_per_tape", EPISODES_PER_TAPE_V1);
    frame_joined_tape_side_v1(&mut writer, "neutral", tape)?;
    Ok(writer)
}

/// The frozen stratum ordinal for a Pool3 member.
pub(super) const fn stratum_ordinal_v1(member: OpponentLadderPoolMemberV2) -> u32 {
    match member {
        OpponentLadderPoolMemberV2::Primary => 0,
        OpponentLadderPoolMemberV2::PredecessorA => 1,
        OpponentLadderPoolMemberV2::PredecessorB => 2,
        OpponentLadderPoolMemberV2::UniformFloor => 3,
    }
}

/// The exact named-tensor cardinality every update stream must carry.
pub(super) const NAMED_TENSOR_COUNT_V1: usize = 33;
/// The only two legal treatment labels.
pub(super) const TREATMENT_FULL_V1: &str = "FULL";
pub(super) const TREATMENT_HALF_V1: &str = "HALF";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UpdateFramingErrorV1 {
    /// A named stream did not carry exactly 33 tensors.
    TensorCardinality,
    /// The treatment label was neither `FULL` nor `HALF`.
    UnknownTreatment,
    /// Two streams disagreed on a tensor's name, shape, or value length, so
    /// they are not aligned tensor-for-tensor.
    StreamMisalignment,
    /// A published f32 or f64 value was nonfinite.
    NonFiniteValue,
    /// The delta stream length did not match the total element count.
    DeltaCardinality,
    /// A stream did not match the frozen native 33-tensor name/order/shape
    /// manifest. This is what makes duplicate dummy tensors fail.
    NativeManifestMismatch,
    /// A published f64 delta was not bit-identical to the exact
    /// `f64::from(after) - f64::from(before)` of its own coordinate.
    DeltaNotDerivedFromParameters,
}

/// Validates one named stream against the frozen native parameter manifest:
/// exact count, exact names in exact order, exact shapes. A stream of 33
/// duplicated dummy tensors fails here.
fn matches_native_manifest_v1(stream: &[NativeNamedParameterV1]) -> bool {
    let manifest = native_train_state_parameter_layout_v1();
    if stream.len() != manifest.len() {
        return false;
    }
    manifest.zip(stream).all(|((name, shape), parameter)| {
        parameter.name == name
            && parameter.shape.as_slice() == shape
            && parameter.values.len() == shape.iter().product::<usize>()
    })
}

/// Frames one treatment's actual-update streams: all 33 named gradients,
/// before/after parameters, moments, and f64 deltas in native order.
///
/// Validates cardinality, cross-stream alignment, treatment label, and
/// finiteness before any byte is bound, so a malformed update can never be
/// published under a valid-looking digest.
pub(super) fn frame_update_v1(
    treatment: &str,
    named_gradients: &[NativeNamedParameterV1],
    parameters_before: &[NativeNamedParameterV1],
    parameters_after: &[NativeNamedParameterV1],
    first_moments: &[NativeNamedParameterV1],
    second_moments: &[NativeNamedParameterV1],
    deltas: &[f64],
) -> Result<FramedWriterV1, UpdateFramingErrorV1> {
    if treatment != TREATMENT_FULL_V1 && treatment != TREATMENT_HALF_V1 {
        return Err(UpdateFramingErrorV1::UnknownTreatment);
    }
    let streams = [
        named_gradients,
        parameters_before,
        parameters_after,
        first_moments,
        second_moments,
    ];
    for stream in streams {
        if stream.len() != NAMED_TENSOR_COUNT_V1 {
            return Err(UpdateFramingErrorV1::TensorCardinality);
        }
        if stream
            .iter()
            .any(|parameter| parameter.values.iter().any(|value| !value.is_finite()))
        {
            return Err(UpdateFramingErrorV1::NonFiniteValue);
        }
        // Each stream must be the real native manifest, not 33 copies of one
        // tensor.
        if !matches_native_manifest_v1(stream) {
            return Err(UpdateFramingErrorV1::NativeManifestMismatch);
        }
    }
    // Every stream must be aligned tensor-for-tensor with the gradients:
    // same name, same shape, same value length, same order.
    for stream in streams {
        for (parameter, reference) in stream.iter().zip(named_gradients) {
            if parameter.name != reference.name
                || parameter.shape != reference.shape
                || parameter.values.len() != reference.values.len()
            {
                return Err(UpdateFramingErrorV1::StreamMisalignment);
            }
        }
    }
    if deltas.iter().any(|value| !value.is_finite()) {
        return Err(UpdateFramingErrorV1::NonFiniteValue);
    }
    let element_total: usize = named_gradients
        .iter()
        .map(|parameter| parameter.values.len())
        .sum();
    if deltas.len() != element_total {
        return Err(UpdateFramingErrorV1::DeltaCardinality);
    }
    // Every published delta must be bit-identical to the exact single f64
    // subtraction of its own before/after coordinates, and inside the frozen
    // step-one ceiling. A hand-written delta stream cannot survive this.
    let mut cursor = 0usize;
    for (before, after) in parameters_before.iter().zip(parameters_after) {
        for (before_value, after_value) in before.values.iter().zip(&after.values) {
            let derived = delta_v1(*before_value, *after_value);
            if deltas[cursor].to_bits() != derived.to_bits() {
                return Err(UpdateFramingErrorV1::DeltaNotDerivedFromParameters);
            }
            if !absolute_delta_admissible_v1(derived) {
                return Err(UpdateFramingErrorV1::DeltaNotDerivedFromParameters);
            }
            cursor += 1;
        }
    }

    let mut writer = FramedWriterV1::new_v1(UPDATE_SCHEMA_V1);
    writer.u64_v1("named_tensor_count", NAMED_TENSOR_COUNT_V1 as u64);
    writer.text_v1("treatment", treatment);
    writer.text_v1("backend_identity", DIAGNOSTIC_BACKEND_IDENTITY_V1);
    writer.u32_array_v1(
        "adam_authority_bits",
        &[
            VALUE_COEFFICIENT_BITS_V1,
            LEARNING_RATE_BITS_V1,
            ADAM_BETA1_BITS_V1,
            ADAM_BETA2_BITS_V1,
            ADAM_EPSILON_BITS_V1,
            ADAM_WEIGHT_DECAY_BITS_V1,
        ],
    );
    writer.u64_v1("amsgrad", u64::from(ADAM_AMSGRAD_V1));
    for (label, stream) in [
        ("named_gradients", named_gradients),
        ("parameters_before", parameters_before),
        ("parameters_after", parameters_after),
        ("first_moments", first_moments),
        ("second_moments", second_moments),
    ] {
        writer.u64_v1(&format!("{label}_count"), stream.len() as u64);
        for parameter in stream {
            writer.text_v1("name", parameter.name);
            writer.u32_array_v1(
                "shape",
                &parameter
                    .shape
                    .iter()
                    .map(|value| *value as u32)
                    .collect::<Vec<u32>>(),
            );
            writer.f32_bits_array_v1(label, &parameter.values);
        }
    }
    writer.u64_v1("delta_count", deltas.len() as u64);
    writer.f64_bits_array_v1("deltas", deltas);
    Ok(writer)
}

// ---------------------------------------------------------------------------
// Actual grouped CUDA update over an immutable joined tape.
// ---------------------------------------------------------------------------

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActualTreatmentV1 {
    Full,
    Half,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl ActualTreatmentV1 {
    const fn label_v1(self) -> &'static str {
        match self {
            Self::Full => TREATMENT_FULL_V1,
            Self::Half => TREATMENT_HALF_V1,
        }
    }

    fn tensor_v1(self, lineage: &RetainedTreatmentLineageV1) -> &NativeFlatDecisionTensorV2 {
        match self {
            Self::Full => lineage.full_tensor_v1(),
            Self::Half => lineage.half_tensor_v1(),
        }
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActualUpdateErrorV1 {
    Tape(TapeFramingErrorV1),
    UnauthorizedTapeRole,
    EmptyTape,
    SnapshotBefore,
    NonGenesisState,
    GenesisParameterRelation,
    StateForward,
    StateOutputMismatch,
    CudaUpdate,
    SnapshotAfter,
    WrongAdamStep,
    ResultMismatch,
    AdamReplayMismatch,
    CrossArmMismatch,
    Frame(UpdateFramingErrorV1),
}

/// Private proof that the complete joined hierarchy was successfully
/// revalidated and framed immediately before either GPU arm was constructed.
/// The raw tape cannot reach the update primitive without this seal.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct ValidatedJoinedTapeForUpdateV1<'a> {
    tape: &'a JoinedTapeV1,
    frame: FramedWriterV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl<'a> ValidatedJoinedTapeForUpdateV1<'a> {
    fn new_v1(
        authority: &impl DiagnosticTapeAuthorityV1,
        expected_deck_ids: &SessionDeckIdsV1,
        expected_deck_hashes: SessionDeckHashesV1,
        tape: &'a JoinedTapeV1,
    ) -> Result<Self, ActualUpdateErrorV1> {
        if !authority.allows_update_v1() {
            return Err(ActualUpdateErrorV1::UnauthorizedTapeRole);
        }
        let frame = frame_joined_tape_v1(authority, expected_deck_ids, expected_deck_hashes, tape)
            .map_err(ActualUpdateErrorV1::Tape)?;
        if tape.total_group_count_v1() == 0 {
            return Err(ActualUpdateErrorV1::EmptyTape);
        }
        Ok(Self { tape, frame })
    }
}

/// Sealed bit-level authority for the promoted(2) generation-384
/// weights-only genesis. The native-Windows loader constructs this only
/// after the fixed run/checkpoint/sidecar/payload/model digest diagonal has
/// passed. The paired updater binds both supplied live states to these exact
/// named streams before any CUDA work.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct Promoted2Generation384GenesisV1 {
    full_parameters: Vec<NativeNamedParameterV1>,
    half_parameters: Vec<NativeNamedParameterV1>,
    scorer_bias_anchor_bits: u32,
    _seal: Promoted2Generation384GenesisSealV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct Promoted2Generation384GenesisSealV1(());

/// A consumed, rollout-bound, step-zero candidate. Construction is private
/// to the paired preflight below, so an individual arm cannot be updated from
/// a raw state/tape pair.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct ValidatedActualArmV1 {
    treatment: ActualTreatmentV1,
    state: NativePolicyValueTrainStateV1,
    before: NativePolicyValueTrainSnapshotV1,
}

/// Complete owned evidence emitted by one actual grouped CUDA update. The
/// borrowed group arena never escapes the executor; only owned named streams
/// and the framed bytes survive.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct ActualUpdateEvidenceV1 {
    treatment: ActualTreatmentV1,
    result: NativePolicyTrainStepResultV1,
    before: NativePolicyValueTrainSnapshotV1,
    after: NativePolicyValueTrainSnapshotV1,
    deltas: Vec<f64>,
    frame: FramedWriterV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl ActualUpdateEvidenceV1 {
    fn sha256_v1(&self) -> String {
        self.frame.sha256_v1()
    }
}

/// The only successful return from the live updater. Both mutated states are
/// owned here and become visible to the caller only after both individual
/// updates, both independent Adam replays, and every cross-arm gate pass.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct ActualPairedUpdateV1 {
    full_state: NativePolicyValueTrainStateV1,
    half_state: NativePolicyValueTrainStateV1,
    full: ActualUpdateEvidenceV1,
    half: ActualUpdateEvidenceV1,
    tape_frame: FramedWriterV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn named_streams_bit_equal_v1(
    left: &[NativeNamedParameterV1],
    right: &[NativeNamedParameterV1],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.shape == right.shape
                && f32_slices_bit_equal_v1(&left.values, &right.values)
        })
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn snapshot_is_genesis_v1(snapshot: &NativePolicyValueTrainSnapshotV1) -> bool {
    snapshot.adam_step == 0
        && matches_native_manifest_v1(&snapshot.parameters)
        && snapshot.first_moments.len() == snapshot.parameters.len()
        && snapshot.second_moments.len() == snapshot.parameters.len()
        && snapshot
            .parameters
            .iter()
            .zip(&snapshot.first_moments)
            .zip(&snapshot.second_moments)
            .all(|((parameter, first), second)| {
                first.name == parameter.name
                    && second.name == parameter.name
                    && first.shape == parameter.shape
                    && second.shape == parameter.shape
                    && first.values.len() == parameter.values.len()
                    && second.values.len() == parameter.values.len()
                    && parameter.values.iter().all(|value| value.is_finite())
                    && first.values.iter().all(|value| value.to_bits() == 0)
                    && second.values.iter().all(|value| value.to_bits() == 0)
            })
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn genesis_snapshots_match_authority_v1(
    full: &NativePolicyValueTrainSnapshotV1,
    half: &NativePolicyValueTrainSnapshotV1,
    genesis: &Promoted2Generation384GenesisV1,
) -> bool {
    if !snapshot_is_genesis_v1(full) || !snapshot_is_genesis_v1(half) {
        return false;
    }
    let Ok(derived_authoritative_half) = derive_half_parameters_v1(&genesis.full_parameters) else {
        return false;
    };
    let Ok(restored_half) = halve_action_encoder_digest_columns_v1(&half.parameters) else {
        return false;
    };
    named_streams_bit_equal_v1(&genesis.half_parameters, &derived_authoritative_half)
        && named_streams_bit_equal_v1(&full.parameters, &genesis.full_parameters)
        && named_streams_bit_equal_v1(&half.parameters, &genesis.half_parameters)
        && named_streams_bit_equal_v1(&full.parameters, &restored_half)
        && full.scorer_bias_anchor_bits == genesis.scorer_bias_anchor_bits
        && half.scorer_bias_anchor_bits == genesis.scorer_bias_anchor_bits
}

/// Re-forward every treatment tensor with the exact supplied genesis state.
/// Besides logits/value, this independently re-establishes the production
/// log-softmax and categorical choice bits, binding the state that will reach
/// CUDA to the outputs and selections retained by the rollout.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn state_matches_joined_tape_v1(
    state: &NativePolicyValueTrainStateV1,
    tape: &ValidatedJoinedTapeForUpdateV1<'_>,
    treatment: ActualTreatmentV1,
) -> Result<(), ActualUpdateErrorV1> {
    let forward = NativePolicyPackedForwardBuilderV1::from_model_v1(state.model_v1())
        .map_err(|_| ActualUpdateErrorV1::StateForward)?;
    let mut sampler = FastCategoricalScratch::default();
    for episode in &tape.tape.episodes {
        for group in &episode.groups {
            for substep in &group.substeps {
                let tensor = treatment.tensor_v1(&substep.retained.lineage);
                let output = forward
                    .forward_v1(encoded_decision_view_v1(tensor))
                    .map_err(|_| ActualUpdateErrorV1::StateForward)?;
                let logits = output.logits_v1();
                let selected = substep.selected_index as usize;
                if logits.len() != substep.raw_action_logit_bits.len()
                    || selected >= logits.len()
                    || output.value_v1().to_bits() != substep.predicted_value_bits
                    || logits
                        .iter()
                        .zip(&substep.raw_action_logit_bits)
                        .any(|(value, expected)| value.to_bits() != *expected)
                {
                    return Err(ActualUpdateErrorV1::StateOutputMismatch);
                }
                let (selected_log_probability, _) = selected_log_softmax(logits, selected)
                    .map_err(|_| ActualUpdateErrorV1::StateOutputMismatch)?;
                if selected_log_probability.to_bits() != substep.selected_log_probability_bits
                    || sampler
                        .sample(logits, substep.action_seed)
                        .map_err(|_| ActualUpdateErrorV1::StateOutputMismatch)?
                        != selected
                {
                    return Err(ActualUpdateErrorV1::StateOutputMismatch);
                }
            }
        }
    }
    Ok(())
}

/// Recomputes the complete selected-output and physical-loss projections in
/// the production f32 order. A CUDA result with correct gradients but a
/// reordered, substituted, or self-consistent loss projection is rejected.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn gauge_bounds_match_backward_action_counts_v1(
    bounds: &[NativeGaugeSubstepBoundV1],
    forward_action_counts: &[usize],
) -> bool {
    bounds.len() == forward_action_counts.len()
        && bounds
            .iter()
            .zip(forward_action_counts.iter().rev())
            .all(|(bound, action_count)| {
                bound.action_count == *action_count
                    && bound.abs_policy_coefficient.is_finite()
                    && bound.gamma.is_finite()
                    && bound.bound_component.is_finite()
            })
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn actual_result_matches_tape_v1(
    result: &NativePolicyTrainStepResultV1,
    before: &NativePolicyValueTrainSnapshotV1,
    after: &NativePolicyValueTrainSnapshotV1,
    tape: &ValidatedJoinedTapeForUpdateV1<'_>,
) -> bool {
    let group_count = tape.tape.total_group_count_v1();
    let substep_count = tape.tape.total_substep_count_v1();
    let Ok(group_count_u32) = u32::try_from(group_count) else {
        return false;
    };
    if group_count_u32 == 0
        || (group_count_u32 as f32) as u32 != group_count_u32
        || result.adam_step != 1
        || result.selected_outputs.len() != substep_count
        || result.physical_terms.len() != group_count
        || !matches_native_manifest_v1(&result.gradients)
        || !matches_native_manifest_v1(&before.parameters)
        || !matches_native_manifest_v1(&after.parameters)
    {
        return false;
    }

    let mut selected_cursor = 0usize;
    let mut group_cursor = 0usize;
    let mut policy_sum = 0.0f32;
    let mut value_sum = 0.0f32;
    let mut total_action_count = 0usize;
    let mut max_action_count = 0usize;
    // Production's gauge accumulator observes the CPU backward traversal:
    // groups in reverse order and substeps in reverse order within each
    // group. Because the forward tape is flattened group-major, this is the
    // exact reverse of this vector. Selected outputs and physical terms stay
    // forward-ordered and are validated above/below independently.
    let mut forward_action_counts = Vec::with_capacity(substep_count);
    for episode in &tape.tape.episodes {
        for group in &episode.groups {
            let mut joint_log_probability: Option<f32> = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let Some(selected_output) = result.selected_outputs.get(selected_cursor) else {
                    return false;
                };
                let selected = substep.selected_index as usize;
                let action_count = substep.raw_action_logit_bits.len();
                total_action_count = match total_action_count.checked_add(action_count) {
                    Some(value) => value,
                    None => return false,
                };
                max_action_count = max_action_count.max(action_count);
                forward_action_counts.push(action_count);
                if selected >= action_count
                    || selected_output.group_index != group_cursor
                    || selected_output.substep_index != substep_index
                    || selected_output.selected_action_index != selected
                    || selected_output.selected_logit.to_bits()
                        != substep.raw_action_logit_bits[selected]
                    || selected_output.value.to_bits() != substep.predicted_value_bits
                    || selected_output.selected_log_probability.to_bits()
                        != substep.selected_log_probability_bits
                {
                    return false;
                }
                let selected_log_probability =
                    f32::from_bits(substep.selected_log_probability_bits);
                joint_log_probability = Some(match joint_log_probability {
                    None => selected_log_probability,
                    Some(active) => active + selected_log_probability,
                });
                selected_cursor += 1;
            }

            let Some(joint_log_probability) = joint_log_probability else {
                return false;
            };
            let Some(first) = group.substeps.first() else {
                return false;
            };
            let value = f32::from_bits(first.predicted_value_bits);
            let terminal_return = episode.authenticated_terminal.learner_return_v1();
            let target = f32::from(terminal_return);
            let advantage = target - value;
            let policy_term = -joint_log_probability * advantage;
            let value_error = value - target;
            let value_term = value_error * value_error;
            policy_sum += policy_term;
            value_sum += value_term;

            let Some(physical) = result.physical_terms.get(group_cursor) else {
                return false;
            };
            let Ok(expected_substep_count) = u32::try_from(group.substeps.len()) else {
                return false;
            };
            if physical.joint_log_probability.to_bits() != joint_log_probability.to_bits()
                || physical.value.to_bits() != value.to_bits()
                || physical.terminal_return != terminal_return
                || physical.substep_count != expected_substep_count
            {
                return false;
            }
            group_cursor += 1;
        }
    }
    let loss = (policy_sum + f32::from_bits(VALUE_COEFFICIENT_BITS_V1) * value_sum)
        / group_count_u32 as f32;
    if selected_cursor != substep_count
        || group_cursor != group_count
        || result.policy_sum.to_bits() != policy_sum.to_bits()
        || result.value_sum.to_bits() != value_sum.to_bits()
        || result.loss.to_bits() != loss.to_bits()
        || !policy_sum.is_finite()
        || !value_sum.is_finite()
        || !loss.is_finite()
    {
        return false;
    }

    let scorer_name = CANONICAL_GAUGE_PARAMETERS_V1[0];
    let Some(scorer_index) = before
        .parameters
        .iter()
        .position(|parameter| parameter.name == scorer_name)
    else {
        return false;
    };
    if before
        .parameters
        .iter()
        .skip(scorer_index + 1)
        .any(|parameter| parameter.name == scorer_name)
        || before.parameters[scorer_index].shape.as_slice() != [1]
        || result.scorer_bias_gauge.parameter_name != scorer_name
        || result.scorer_bias_gauge.substep_count != substep_count
        || result.scorer_bias_gauge.substep_bounds.len() != substep_count
        || result.scorer_bias_gauge.total_action_count != total_action_count
        || result.scorer_bias_gauge.max_action_count != max_action_count
        || result.scorer_bias_gauge.canonical_gradient.to_bits() != 0
        || result.scorer_bias_gauge.parameter_before_bits
            != before.parameters[scorer_index].values[0].to_bits()
        || result.scorer_bias_gauge.parameter_after_bits
            != after.parameters[scorer_index].values[0].to_bits()
        || result.gradients[scorer_index].values[0].to_bits() != 0
    {
        return false;
    }
    if !gauge_bounds_match_backward_action_counts_v1(
        &result.scorer_bias_gauge.substep_bounds,
        &forward_action_counts,
    ) {
        return false;
    }
    for value in [
        result.scorer_bias_gauge.sum_abs_policy_coefficients,
        result.scorer_bias_gauge.per_substep_bound_sum,
        result.scorer_bias_gauge.cross_substep_bound,
        f64::from(result.scorer_bias_gauge.raw_gradient_residual),
        result.scorer_bias_gauge.derived_absolute_bound,
        result.scorer_bias_gauge.high_precision_residual,
    ] {
        if !value.is_finite() {
            return false;
        }
    }
    true
}

/// Independent scalar replay of the frozen production Adam association.
/// This does not call the optimizer implementation or the CUDA mapper.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct IndependentAdamStepV1 {
    beta1: f32,
    beta2: f32,
    epsilon: f32,
    step_size: f32,
    bias_correction2_sqrt: f32,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl IndependentAdamStepV1 {
    fn new_v1(step: u64) -> Option<Self> {
        let exponent = i32::try_from(step).ok()?;
        let beta1 = f32::from_bits(ADAM_BETA1_BITS_V1);
        let beta2 = f32::from_bits(ADAM_BETA2_BITS_V1);
        let epsilon = f32::from_bits(ADAM_EPSILON_BITS_V1);
        let learning_rate = f32::from_bits(LEARNING_RATE_BITS_V1);
        let bias_correction1 = 1.0f64 - f64::from(beta1).powi(exponent);
        let bias_correction2 = 1.0f64 - f64::from(beta2).powi(exponent);
        let step_size = (f64::from(learning_rate) / bias_correction1) as f32;
        let bias_correction2_sqrt = bias_correction2.sqrt() as f32;
        (step_size.is_finite() && bias_correction2_sqrt.is_finite()).then_some(Self {
            beta1,
            beta2,
            epsilon,
            step_size,
            bias_correction2_sqrt,
        })
    }

    fn coordinate_v1(
        &self,
        before_parameter: f32,
        gradient: f32,
        previous_first: f32,
        previous_second: f32,
    ) -> Option<(f32, f32, f32)> {
        if !before_parameter.is_finite()
            || !gradient.is_finite()
            || !previous_first.is_finite()
            || !previous_second.is_finite()
        {
            return None;
        }
        if gradient.to_bits() == 0
            && previous_first.to_bits() == 0
            && previous_second.to_bits() == 0
        {
            return Some((before_parameter, previous_first, previous_second));
        }
        let first = previous_first + (gradient - previous_first) * (1.0 - self.beta1);
        let second = previous_second * self.beta2 + gradient * gradient * (1.0 - self.beta2);
        let denominator = second.sqrt() / self.bias_correction2_sqrt + self.epsilon;
        let parameter = before_parameter + (-self.step_size) * first / denominator;
        (first.is_finite() && second.is_finite() && second >= 0.0 && parameter.is_finite())
            .then_some((parameter, first, second))
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn canonical_scorer_bias_coordinate_valid_v1(
    before_parameter: f32,
    after_parameter: f32,
    gradient: f32,
    after_first: f32,
    after_second: f32,
    anchor_bits: u32,
) -> bool {
    before_parameter.to_bits() == anchor_bits
        && after_parameter.to_bits() == anchor_bits
        && gradient.to_bits() == 0
        && after_first.to_bits() == 0
        && after_second.to_bits() == 0
        && delta_v1(before_parameter, after_parameter).to_bits() == 0
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn actual_update_matches_independent_adam_v1(evidence: &ActualUpdateEvidenceV1) -> bool {
    if evidence.before.adam_step != 0
        || evidence.after.adam_step != 1
        || evidence.result.adam_step != 1
        || evidence.before.parameters.len() != evidence.result.gradients.len()
        || evidence.before.parameters.len() != evidence.after.parameters.len()
        || evidence.before.parameters.len() != evidence.before.first_moments.len()
        || evidence.before.parameters.len() != evidence.before.second_moments.len()
        || evidence.before.parameters.len() != evidence.after.first_moments.len()
        || evidence.before.parameters.len() != evidence.after.second_moments.len()
        || !matches_native_manifest_v1(&evidence.before.parameters)
        || !matches_native_manifest_v1(&evidence.before.first_moments)
        || !matches_native_manifest_v1(&evidence.before.second_moments)
        || !matches_native_manifest_v1(&evidence.result.gradients)
        || !matches_native_manifest_v1(&evidence.after.parameters)
        || !matches_native_manifest_v1(&evidence.after.first_moments)
        || !matches_native_manifest_v1(&evidence.after.second_moments)
    {
        return false;
    }
    let Some(adam) = IndependentAdamStepV1::new_v1(evidence.after.adam_step) else {
        return false;
    };
    let scorer_name = CANONICAL_GAUGE_PARAMETERS_V1[0];
    for tensor_index in 0..evidence.before.parameters.len() {
        let before = &evidence.before.parameters[tensor_index];
        let gradient = &evidence.result.gradients[tensor_index];
        let before_first = &evidence.before.first_moments[tensor_index];
        let before_second = &evidence.before.second_moments[tensor_index];
        let after = &evidence.after.parameters[tensor_index];
        let after_first = &evidence.after.first_moments[tensor_index];
        let after_second = &evidence.after.second_moments[tensor_index];
        if [
            gradient,
            before_first,
            before_second,
            after,
            after_first,
            after_second,
        ]
        .iter()
        .any(|stream| {
            stream.name != before.name
                || stream.shape != before.shape
                || stream.values.len() != before.values.len()
        }) {
            return false;
        }
        for value_index in 0..before.values.len() {
            let before_parameter = before.values[value_index];
            let gradient = gradient.values[value_index];
            let previous_first = before_first.values[value_index];
            let previous_second = before_second.values[value_index];
            if !before_parameter.is_finite()
                || !gradient.is_finite()
                || !previous_first.is_finite()
                || !previous_second.is_finite()
            {
                return false;
            }

            if before.name == scorer_name {
                if before.shape.as_slice() != [1]
                    || value_index != 0
                    || !canonical_scorer_bias_coordinate_valid_v1(
                        before_parameter,
                        after.values[value_index],
                        gradient,
                        after_first.values[value_index],
                        after_second.values[value_index],
                        evidence.before.scorer_bias_anchor_bits,
                    )
                {
                    return false;
                }
                continue;
            }

            let Some((expected_parameter, expected_first, expected_second)) =
                adam.coordinate_v1(before_parameter, gradient, previous_first, previous_second)
            else {
                return false;
            };
            if !within_envelope_v1(
                f64::from(after.values[value_index]),
                f64::from(expected_parameter),
            ) || !within_envelope_v1(
                f64::from(after_first.values[value_index]),
                f64::from(expected_first),
            ) || !within_envelope_v1(
                f64::from(after_second.values[value_index]),
                f64::from(expected_second),
            ) {
                return false;
            }
        }
    }
    true
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn action_encoder_digest_coordinate_v1(
    tensor_index: usize,
    target_index: usize,
    value_index: usize,
) -> bool {
    tensor_index == target_index
        && (ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1)
            .contains(&(value_index % ACTION_ENCODER_COLUMNS_V1))
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn cross_arm_coordinate_valid_v1(
    is_digest_column: bool,
    f_before: f32,
    f_after: f32,
    f_gradient: f32,
    h_before: f32,
    h_after: f32,
    h_gradient: f32,
) -> bool {
    let f_delta = delta_v1(f_before, f_after);
    let h_delta = delta_v1(h_before, h_after);
    if !absolute_delta_admissible_v1(f_delta) || !absolute_delta_admissible_v1(h_delta) {
        return false;
    }
    if is_digest_column {
        digest_gradient_matches_halved_full_v1(h_gradient, f_gradient)
            && digest_column_delta_consistent_v1(h_before, h_after, f_before)
    } else {
        within_envelope_v1(f64::from(h_gradient), f64::from(f_gradient))
            && within_envelope_v1(h_delta, f_delta)
    }
}

/// Cross-arm gradient/delta identity over the native 33-tensor order.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn actual_updates_cross_arm_valid_v1(
    full: &ActualUpdateEvidenceV1,
    half: &ActualUpdateEvidenceV1,
) -> bool {
    if full.treatment != ActualTreatmentV1::Full
        || half.treatment != ActualTreatmentV1::Half
        || full.result.gradients.len() != NAMED_TENSOR_COUNT_V1
        || half.result.gradients.len() != NAMED_TENSOR_COUNT_V1
        || full.before.parameters.len() != NAMED_TENSOR_COUNT_V1
        || half.before.parameters.len() != NAMED_TENSOR_COUNT_V1
        || full.after.parameters.len() != NAMED_TENSOR_COUNT_V1
        || half.after.parameters.len() != NAMED_TENSOR_COUNT_V1
        || !matches_native_manifest_v1(&full.before.parameters)
        || !matches_native_manifest_v1(&half.before.parameters)
        || !matches_native_manifest_v1(&full.after.parameters)
        || !matches_native_manifest_v1(&half.after.parameters)
        || !matches_native_manifest_v1(&full.result.gradients)
        || !matches_native_manifest_v1(&half.result.gradients)
    {
        return false;
    }
    let Ok(target_index) = validate_single_target_v1(&full.before.parameters) else {
        return false;
    };
    let Ok(half_target_index) = validate_single_target_v1(&half.before.parameters) else {
        return false;
    };
    if target_index != half_target_index {
        return false;
    }
    for tensor_index in 0..NAMED_TENSOR_COUNT_V1 {
        let f_before = &full.before.parameters[tensor_index];
        let h_before = &half.before.parameters[tensor_index];
        let f_after = &full.after.parameters[tensor_index];
        let h_after = &half.after.parameters[tensor_index];
        let f_gradient = &full.result.gradients[tensor_index];
        let h_gradient = &half.result.gradients[tensor_index];
        if [h_before, f_after, h_after, f_gradient, h_gradient]
            .iter()
            .any(|stream| {
                stream.name != f_before.name
                    || stream.shape != f_before.shape
                    || stream.values.len() != f_before.values.len()
            })
        {
            return false;
        }
        for value_index in 0..f_before.values.len() {
            let is_digest_column =
                action_encoder_digest_coordinate_v1(tensor_index, target_index, value_index);
            if !cross_arm_coordinate_valid_v1(
                is_digest_column,
                f_before.values[value_index],
                f_after.values[value_index],
                f_gradient.values[value_index],
                h_before.values[value_index],
                h_after.values[value_index],
                h_gradient.values[value_index],
            ) {
                return false;
            }
        }
    }
    true
}

/// Executes one production grouped loss/backward/Adam step through the
/// diagnostic-only named-gradient capture wrapper. Both inputs are private
/// validated capabilities. The state is consumed; any error drops the
/// possibly-mutated candidate instead of returning it to a caller.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn execute_validated_actual_arm_v1(
    mut arm: ValidatedActualArmV1,
    tape: &ValidatedJoinedTapeForUpdateV1<'_>,
) -> Result<(NativePolicyValueTrainStateV1, ActualUpdateEvidenceV1), ActualUpdateErrorV1> {
    let treatment = arm.treatment;
    let before = arm.before;
    let mut substep_groups: Vec<Vec<NativePolicySubstepV1<'_>>> = Vec::new();
    let mut terminal_returns: Vec<i8> = Vec::new();
    substep_groups.reserve(tape.tape.total_group_count_v1());
    terminal_returns.reserve(tape.tape.total_group_count_v1());
    for episode in &tape.tape.episodes {
        for group in &episode.groups {
            let mut substeps = Vec::with_capacity(group.substeps.len());
            for substep in &group.substeps {
                let tensor = treatment.tensor_v1(&substep.retained.lineage);
                substeps.push(NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_v1(tensor),
                    )),
                    selected_action_index: substep.selected_index as usize,
                    expected_raw_action_logit_bits: &substep.raw_action_logit_bits,
                    expected_value_bits: substep.predicted_value_bits,
                });
            }
            substep_groups.push(substeps);
            terminal_returns.push(episode.authenticated_terminal.learner_return_v1());
        }
    }
    let groups: Vec<NativePolicyPhysicalDecisionV1<'_>> = substep_groups
        .iter()
        .zip(&terminal_returns)
        .map(
            |(substeps, terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: *terminal_return,
            },
        )
        .collect();
    let result = crate::experimental_burn_net8_packed_v1::bridge::train_step_cuda_burn_dense_capture_named_gradients_v1(
        &mut arm.state,
        &groups,
        f32::from_bits(VALUE_COEFFICIENT_BITS_V1),
        f32::from_bits(LEARNING_RATE_BITS_V1),
    )
    .map_err(|_| ActualUpdateErrorV1::CudaUpdate)?;
    drop(groups);
    drop(substep_groups);

    let after = arm
        .state
        .snapshot_v1()
        .map_err(|_| ActualUpdateErrorV1::SnapshotAfter)?;
    if result.adam_step != 1 || after.adam_step != 1 {
        return Err(ActualUpdateErrorV1::WrongAdamStep);
    }
    if !actual_result_matches_tape_v1(&result, &before, &after, tape) {
        return Err(ActualUpdateErrorV1::ResultMismatch);
    }
    let deltas = derived_deltas_v1(&before.parameters, &after.parameters);
    let frame = frame_update_v1(
        treatment.label_v1(),
        &result.gradients,
        &before.parameters,
        &after.parameters,
        &after.first_moments,
        &after.second_moments,
        &deltas,
    )
    .map_err(ActualUpdateErrorV1::Frame)?;
    let evidence = ActualUpdateEvidenceV1 {
        treatment,
        result,
        before,
        after,
        deltas,
        frame,
    };
    Ok((arm.state, evidence))
}

/// Validates both genesis states against the immutable joined tape, executes
/// one whole-tape update per arm, and commits neither unless the paired
/// numerical proof is complete.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn execute_actual_paired_update_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    genesis: &Promoted2Generation384GenesisV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    tape: &JoinedTapeV1,
    full_state: NativePolicyValueTrainStateV1,
    half_state: NativePolicyValueTrainStateV1,
) -> Result<ActualPairedUpdateV1, ActualUpdateErrorV1> {
    let validated_tape = ValidatedJoinedTapeForUpdateV1::new_v1(
        authority,
        expected_deck_ids,
        expected_deck_hashes,
        tape,
    )?;
    let full_before = full_state
        .snapshot_v1()
        .map_err(|_| ActualUpdateErrorV1::SnapshotBefore)?;
    let half_before = half_state
        .snapshot_v1()
        .map_err(|_| ActualUpdateErrorV1::SnapshotBefore)?;
    if !snapshot_is_genesis_v1(&full_before) || !snapshot_is_genesis_v1(&half_before) {
        return Err(ActualUpdateErrorV1::NonGenesisState);
    }
    if !genesis_snapshots_match_authority_v1(&full_before, &half_before, genesis) {
        return Err(ActualUpdateErrorV1::GenesisParameterRelation);
    }
    state_matches_joined_tape_v1(&full_state, &validated_tape, ActualTreatmentV1::Full)?;
    state_matches_joined_tape_v1(&half_state, &validated_tape, ActualTreatmentV1::Half)?;

    let (full_state, full) = execute_validated_actual_arm_v1(
        ValidatedActualArmV1 {
            treatment: ActualTreatmentV1::Full,
            state: full_state,
            before: full_before,
        },
        &validated_tape,
    )?;
    if !actual_update_matches_independent_adam_v1(&full) {
        return Err(ActualUpdateErrorV1::AdamReplayMismatch);
    }
    let (half_state, half) = execute_validated_actual_arm_v1(
        ValidatedActualArmV1 {
            treatment: ActualTreatmentV1::Half,
            state: half_state,
            before: half_before,
        },
        &validated_tape,
    )?;
    if !actual_update_matches_independent_adam_v1(&half) {
        return Err(ActualUpdateErrorV1::AdamReplayMismatch);
    }
    if !actual_updates_cross_arm_valid_v1(&full, &half) {
        return Err(ActualUpdateErrorV1::CrossArmMismatch);
    }
    Ok(ActualPairedUpdateV1 {
        full_state,
        half_state,
        full,
        half,
        tape_frame: validated_tape.frame,
    })
}

// ---------------------------------------------------------------------------
// Authorized seed-949999 live preflight driver. The test is compiled on any
// target when the diagnostic CUDA feature is enabled, so Linux CI checks the
// complete Rust API shape; its first runtime gate admits only native
// x86_64-pc-windows-msvc. It remains ignored and is never run implicitly.
// ---------------------------------------------------------------------------

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct PreflightLiveAuthoritiesV1 {
    engine: Arc<LadderOpponentEngineV1>,
    genesis: Promoted2Generation384GenesisV1,
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Debug, Eq, PartialEq)]
struct PreflightRepeatDigestsV1 {
    tape_sha256: String,
    full_update_sha256: String,
    half_update_sha256: String,
    update_pair_sha256: String,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct PreflightRepeatArtifactsV1 {
    digests: PreflightRepeatDigestsV1,
    tape_frame: FramedWriterV1,
    full_update_frame: FramedWriterV1,
    half_update_frame: FramedWriterV1,
    update_pair_frame: FramedWriterV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn checkpoint_ref_v1(
    source_run_sha256: &str,
    generation: u64,
    checkpoint_sha256: &str,
    sidecar_sha256: &str,
    state_sha256: &str,
) -> OpponentLadderCheckpointRefV1 {
    OpponentLadderCheckpointRefV1 {
        source_run_sha256: source_run_sha256.to_owned(),
        generation,
        checkpoint_sha256: checkpoint_sha256.to_owned(),
        sidecar_sha256: sidecar_sha256.to_owned(),
        state_sha256: state_sha256.to_owned(),
    }
}

/// Non-constructible evidence that this dedicated process directly resolved
/// CUDA logical ordinal 1 to the frozen physical GPU and retained its primary
/// driver context for the whole preflight.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct ValidatedPreflightGpuV1 {
    ordinal: u64,
    name: String,
    uuid: String,
    _context: Arc<cudarc::driver::CudaContext>,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn cuda_uuid_text_v1(uuid: cudarc::driver::sys::CUuuid) -> String {
    let bytes = uuid.bytes.map(|value| value as u8);
    format!(
        "GPU-{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_dedicated_exact_test_process_v1(expected_test_suffix: &str) {
    let arguments = std::env::args().collect::<Vec<_>>();
    assert!(
        arguments.iter().any(|argument| argument == "--exact"),
        "live preflight must be the exact filtered libtest"
    );
    assert!(
        arguments.iter().any(|argument| argument == "--ignored"),
        "live preflight must be explicitly admitted with --ignored"
    );
    assert!(
        arguments
            .iter()
            .any(|argument| argument.ends_with(expected_test_suffix)),
        "live diagnostic must use its fully-qualified exact test name"
    );
    let single_threaded = arguments
        .iter()
        .any(|argument| argument == "--test-threads=1")
        || arguments
            .windows(2)
            .any(|pair| pair[0] == "--test-threads" && pair[1] == "1");
    assert!(single_threaded, "live preflight must use --test-threads=1");
    assert!(
        crate::experimental_burn_net8_packed_v1::bridge::resident_device_process_is_fresh_for_test_v1(),
        "bridge state/counters prove another CUDA update already ran in this process"
    );
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn nvidia_smi_compute_clients_v1() -> Vec<(u32, String)> {
    let program = PathBuf::from(required_build_binding_v1(
        EMBEDDED_BUILD_NVIDIA_SMI_PATH_V1,
        "MTG_KERNEL_ACTION_BLOCK_BUILD_NVIDIA_SMI_PATH_V1",
    ));
    let output = checked_command_output_v1(
        &program,
        &[
            "--query-compute-apps=pid,gpu_uuid",
            "--format=csv,noheader,nounits",
        ],
    );
    let stdout = String::from_utf8(output.stdout).expect("nvidia-smi clients must be UTF-8");
    let mut clients = Vec::new();
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        assert_eq!(
            fields.len(),
            2,
            "every nvidia-smi compute-client row must be exact CSV"
        );
        let pid = fields[0]
            .parse::<u32>()
            .expect("nvidia-smi compute-client PID must be u32");
        assert!(
            fields[1].starts_with("GPU-") && fields[1].len() == 40,
            "nvidia-smi compute-client UUID must be canonical"
        );
        clients.push((pid, fields[1].to_owned()));
    }
    clients
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_no_foreign_gpu1_compute_client_v1(allow_current_process: bool) -> bool {
    let current_pid = std::process::id();
    let mut saw_current_process = false;
    for (pid, uuid) in nvidia_smi_compute_clients_v1() {
        if uuid == PREFLIGHT_GPU_UUID_V1 {
            assert!(
                allow_current_process && pid == current_pid,
                "GPU1 has a foreign or premature compute client: pid={pid}"
            );
            saw_current_process = true;
        }
    }
    saw_current_process
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_physical_gpu1_headless_v1() {
    let nvidia_smi = PathBuf::from(required_build_binding_v1(
        EMBEDDED_BUILD_NVIDIA_SMI_PATH_V1,
        "MTG_KERNEL_ACTION_BLOCK_BUILD_NVIDIA_SMI_PATH_V1",
    ));
    let output = checked_command_output_v1(
        &nvidia_smi,
        &[
            "-i",
            PREFLIGHT_GPU_ORDINAL_V1,
            "--query-gpu=index,name,uuid,display_active,display_attached",
            "--format=csv,noheader,nounits",
        ],
    );
    let stdout = String::from_utf8(output.stdout).expect("nvidia-smi output must be UTF-8");
    let rows = stdout.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 1, "physical GPU1 query must return one row");
    let fields: Vec<&str> = rows[0].split(',').map(str::trim).collect();
    assert_eq!(fields.len(), 5, "GPU identity row must be exact CSV");
    assert_eq!(fields[0], PREFLIGHT_GPU_ORDINAL_V1);
    assert_eq!(fields[1], PREFLIGHT_GPU_NAME_V1);
    assert_eq!(fields[2], PREFLIGHT_GPU_UUID_V1);
    assert_eq!(fields[3], "Disabled", "GPU1 display must be inactive");
    assert_eq!(fields[4], "No", "GPU1 must have no attached display");
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_bridge_gpu_binding_v1(gpu: &ValidatedPreflightGpuV1) {
    assert_eq!(gpu.ordinal, PREFLIGHT_GPU_ORDINAL_U64_V1);
    assert_eq!(gpu.name, PREFLIGHT_GPU_NAME_V1);
    assert_eq!(gpu.uuid, PREFLIGHT_GPU_UUID_V1);
    assert_eq!(
        std::env::var("MTG_KERNEL_PILOT_CUDA_ORDINAL").as_deref(),
        Ok(PREFLIGHT_GPU_ORDINAL_V1)
    );
    assert_eq!(
        std::env::var("CUDA_DEVICE_ORDER").as_deref(),
        Ok(PREFLIGHT_CUDA_DEVICE_ORDER_V1)
    );
    assert!(std::env::var_os("CUDA_VISIBLE_DEVICES").is_none());
}

/// Bounded observed-exclusivity evidence for the complete two-repeat window.
/// Consumer WDDM hardware cannot promise CUDA exclusive-process mode, so the
/// claim is deliberately polling-based: every sample must contain no GPU1
/// compute PID other than this exact test process, and any query/parser failure
/// poisons the monitor and fails the final join.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct BoundedGpu1ExclusivityMonitorV1 {
    stop: Arc<std::sync::atomic::AtomicBool>,
    saw_current_process: Arc<std::sync::atomic::AtomicBool>,
    worker: std::thread::JoinHandle<()>,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl BoundedGpu1ExclusivityMonitorV1 {
    fn start_v1() -> Self {
        let initially_saw_current_process = require_no_foreign_gpu1_compute_client_v1(true);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let saw_current_process = Arc::new(std::sync::atomic::AtomicBool::new(
            initially_saw_current_process,
        ));
        let worker_stop = Arc::clone(&stop);
        let worker_saw_current_process = Arc::clone(&saw_current_process);
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(std::sync::atomic::Ordering::Acquire) {
                if require_no_foreign_gpu1_compute_client_v1(true) {
                    worker_saw_current_process.store(true, std::sync::atomic::Ordering::Release);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            if require_no_foreign_gpu1_compute_client_v1(true) {
                worker_saw_current_process.store(true, std::sync::atomic::Ordering::Release);
            }
        });
        Self {
            stop,
            saw_current_process,
            worker,
        }
    }

    fn finish_v1(self, gpu: &ValidatedPreflightGpuV1) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        self.worker
            .join()
            .expect("GPU1 exclusivity monitor must finish without a failed sample");
        assert!(
            self.saw_current_process
                .load(std::sync::atomic::Ordering::Acquire),
            "GPU1 monitor must observe this exact test PID during the bounded window"
        );
        require_bridge_gpu_binding_v1(gpu);
        assert_eq!(
            gpu._context
                .name()
                .expect("final CUDA name query must pass"),
            PREFLIGHT_GPU_NAME_V1
        );
        assert_eq!(
            cuda_uuid_text_v1(
                gpu._context
                    .uuid()
                    .expect("final CUDA UUID query must pass")
            ),
            PREFLIGHT_GPU_UUID_V1
        );
        require_physical_gpu1_headless_v1();
        let _ = require_no_foreign_gpu1_compute_client_v1(true);
    }
}

/// Fail-closed GPU admission before any rollout/update. This binds the exact
/// CUDA driver enumeration used by CubeCL to the frozen UUID, while the
/// external query independently proves physical index, headless state, and
/// absence of foreign compute clients.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_fresh_physical_gpu1_v1(expected_test_suffix: &str) -> ValidatedPreflightGpuV1 {
    require_dedicated_exact_test_process_v1(expected_test_suffix);
    assert_eq!(
        std::env::var("MTG_KERNEL_PILOT_CUDA_ORDINAL").as_deref(),
        Ok(PREFLIGHT_GPU_ORDINAL_V1),
        "the bridge must be pinned to logical CUDA ordinal 1"
    );
    assert_eq!(
        std::env::var("CUDA_DEVICE_ORDER").as_deref(),
        Ok(PREFLIGHT_CUDA_DEVICE_ORDER_V1),
        "CUDA_DEVICE_ORDER must be pinned before driver initialization"
    );
    assert!(
        std::env::var_os("CUDA_VISIBLE_DEVICES").is_none(),
        "CUDA_VISIBLE_DEVICES would make bridge ordinal 1 ambiguous"
    );
    assert!(
        !require_no_foreign_gpu1_compute_client_v1(false),
        "GPU1 must have no compute client before CUDA context creation"
    );
    require_physical_gpu1_headless_v1();

    // CubeCL's CudaDevice index is resolved through this same CUDA driver
    // ordinal (`cuDeviceGet`). Holding the primary context prevents a later
    // reinitialization seam between this identity query and both updates.
    let context = cudarc::driver::CudaContext::new(PREFLIGHT_GPU_ORDINAL_U64_V1 as usize)
        .expect("CUDA logical ordinal 1 must create a driver context");
    assert_eq!(context.ordinal(), PREFLIGHT_GPU_ORDINAL_U64_V1 as usize);
    let name = context.name().expect("CUDA device name query must pass");
    let uuid = cuda_uuid_text_v1(context.uuid().expect("CUDA UUID query must pass"));
    assert_eq!(name, PREFLIGHT_GPU_NAME_V1);
    assert_eq!(uuid, PREFLIGHT_GPU_UUID_V1);
    let _ = require_no_foreign_gpu1_compute_client_v1(true);
    ValidatedPreflightGpuV1 {
        ordinal: PREFLIGHT_GPU_ORDINAL_U64_V1,
        name,
        uuid,
        _context: context,
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn load_preflight_live_authorities_v1() -> PreflightLiveAuthoritiesV1 {
    let pool_root = PathBuf::from(POOL3_ROOT_WINDOWS_V1);
    let pool_bytes =
        std::fs::read(pool_root.join("pool.json")).expect("fixed Pool3 pool.json must be readable");
    assert_eq!(pool_bytes.len(), 1_854);
    assert_eq!(
        lower_hex_raw32_v1(sha256_v1(&pool_bytes)),
        POOL3_DOCUMENT_SHA256_V1
    );
    let pool: OpponentLadderPoolContractV1 =
        serde_json::from_slice(&pool_bytes).expect("fixed Pool3 document must decode");
    assert_eq!(
        pool.primary,
        checkpoint_ref_v1(
            SOURCE_RUN_SHA256_V1,
            SOURCE_GENERATION_V1,
            SOURCE_CHECKPOINT_SHA256_V1,
            SOURCE_SIDECAR_SHA256_V1,
            SOURCE_PAYLOAD_SHA256_V1,
        )
    );
    assert_eq!(
        pool.predecessor_a,
        checkpoint_ref_v1(
            "8bc06b6cf2e26df8002b5cece2784e0cd165cdd6bbd199a835e06c17e8d5de5c",
            512,
            "03f0e226f884f51bf7128f70bec189bd6ac2c8f231ced8886f2cb7d3e936cc90",
            "c56a8ba1361ab172c669307084c4522ee06ac79e39b7cf4a306f11effe36b031",
            "2904dd7b899c21234c64925440277dbfa8d6f552d8f620b153bc8d16c44f523a",
        )
    );
    assert_eq!(
        pool.predecessor_b,
        checkpoint_ref_v1(
            "520d3b849ac3ff37ea50a0498acf335885625feaed5437539bec5c42c5896b06",
            256,
            "b051f364fa69185ae9bd2bd7bcfa3a23a974742bd002efbf04c7453885ab688f",
            "702bcb64164453011572e95db1a6ae1fe2b392ee83531c0d68ed07180cdf1874",
            "ddbacacc6f9108fef9c1dd9e704e4d35fbbbc37f44a74526d27b0b3fb607f6db",
        )
    );

    let primary_root = pool_root.join("primary");
    let predecessor_a_root = pool_root.join("pred-a");
    let predecessor_b_root = pool_root.join("pred-b");
    let source = resolve_ladder_checkpoint_authority_v1(Path::new(&primary_root), &pool.primary)
        .expect("promoted(2) generation-384 source authority must resolve");
    assert_eq!(source.run().run_sha256(), SOURCE_RUN_SHA256_V1);
    assert_eq!(
        source.run().record().schedule().base_seed(),
        SOURCE_BASE_SEED_V1
    );
    assert_eq!(source.checkpoint().generation_index(), SOURCE_GENERATION_V1);
    assert_eq!(source.checkpoint().run_sha256(), SOURCE_RUN_SHA256_V1);
    assert_eq!(
        lower_hex_raw32_v1(source.checkpoint().checkpoint_manifest_sha256()),
        SOURCE_CHECKPOINT_SHA256_V1
    );
    assert_eq!(
        lower_hex_raw32_v1(source.checkpoint().checkpoint_payload_sha256()),
        SOURCE_PAYLOAD_SHA256_V1
    );
    assert_eq!(
        lower_hex_raw32_v1(source.checkpoint().model_parameter_sha256()),
        SOURCE_MODEL_PARAMETER_SHA256_V1
    );
    assert_eq!(
        source.checkpoint().train_state().model_parameter_sha256(),
        SOURCE_MODEL_PARAMETER_SHA256_V1
    );

    let run = source.run().record();
    assert_eq!(
        run.environment().deck_ids(),
        &["Rally".to_owned(), "Rally".to_owned()]
    );
    assert_eq!(
        run.environment().deck_hashes_u64_hex(),
        &["0c9f01c2544412bf".to_owned(), "0c9f01c2544412bf".to_owned()]
    );
    assert_eq!(run.optimization().learning_rate_f32_bits(), "3a83126f");
    assert_eq!(run.optimization().value_coefficient_f32_bits(), "3f000000");
    assert_eq!(run.limits().max_physical_decisions(), 1_024);
    assert_eq!(run.limits().max_policy_steps(), 2_048);
    assert_eq!(run.topology().worker_count(), 2);
    assert_eq!(run.topology().sessions_per_worker(), 32);
    assert_eq!(run.topology().logical_actor_count(), 64);
    assert_eq!(run.topology().broker_batch_target(), 16);

    let reset_payload = derive_genesis_weights_only_payload_v2_v3(source.payload())
        .expect("weights-only g384 payload derivation must pass");
    let anchor = u32::try_from(
        source
            .checkpoint()
            .train_state()
            .scorer_bias_anchor_f32_bits(),
    )
    .expect("source scorer-bias anchor must fit u32");
    let decoded = decode_native_train_state_payload_v1(&reset_payload, 0, anchor)
        .expect("weights-only g384 payload must decode");
    assert_eq!(
        lower_hex_raw32_v1(decoded.digests.model_parameter_sha256),
        SOURCE_MODEL_PARAMETER_SHA256_V1
    );
    assert!(snapshot_is_genesis_v1(&decoded.snapshot));
    let full_parameters = decoded.snapshot.parameters;
    let half_parameters =
        derive_half_parameters_v1(&full_parameters).expect("g384 HALF transform must pass");
    let restored = halve_action_encoder_digest_columns_v1(&half_parameters)
        .expect("g384 HALF inverse must pass");
    assert!(named_streams_bit_equal_v1(&full_parameters, &restored));
    let genesis = Promoted2Generation384GenesisV1 {
        full_parameters,
        half_parameters,
        scorer_bias_anchor_bits: anchor,
        _seal: Promoted2Generation384GenesisSealV1(()),
    };

    let (primary, predecessor_a, predecessor_b) = resolve_ladder_pool_v1(
        &pool,
        Path::new(&primary_root),
        Path::new(&predecessor_a_root),
        Path::new(&predecessor_b_root),
    )
    .expect("all fixed Pool3 checkpoint authorities must resolve");
    let engine = Arc::new(
        LadderOpponentEngineV1::new_v1(pool, primary, predecessor_a, predecessor_b)
            .expect("fixed Pool3 contract must construct"),
    );
    let rally = runtime_deck_by_id(PREFLIGHT_DECK_ID_V1).expect("Rally must be catalog-resolved");
    assert_eq!(rally.runtime_deck_hash, 0x0c9f_01c2_5444_12bf);
    let deck_ids = [
        PREFLIGHT_DECK_ID_V1.to_owned(),
        PREFLIGHT_DECK_ID_V1.to_owned(),
    ];
    let deck_hashes = [rally.runtime_deck_hash; 2];
    PreflightLiveAuthoritiesV1 {
        engine,
        genesis,
        deck_ids,
        deck_hashes,
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn fresh_state_from_parameters_v1(
    parameters: &[NativeNamedParameterV1],
) -> NativePolicyValueTrainStateV1 {
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .expect("runner-fixed diagnostic model must construct");
    model
        .replace_parameter_snapshot_v1(parameters)
        .expect("authorized diagnostic parameters must match the native manifest");
    NativePolicyValueTrainStateV1::new_v1(model)
        .expect("fresh diagnostic state must use step zero and +0 moments")
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn run_neutral_tape_v1(
    authority: &impl DiagnosticTapeAuthorityV1,
    authorities: &PreflightLiveAuthoritiesV1,
    full_state: &NativePolicyValueTrainStateV1,
    half_state: &NativePolicyValueTrainStateV1,
) -> JoinedTapeV1 {
    let full_forward = NativePolicyPackedForwardBuilderV1::from_model_v1(full_state.model_v1())
        .expect("FULL rollout forward builder");
    let half_forward = NativePolicyPackedForwardBuilderV1::from_model_v1(half_state.model_v1())
        .expect("HALF rollout forward builder");
    let mut scorer = TreatmentAwareScorerV1::new_v1(full_forward, half_forward);
    let environment_authority = preflight_native_environment_window_v2(
        authority.seed_v1(),
        0,
        EPISODES_PER_TAPE_V1,
        &authorities.deck_ids,
        authorities.deck_hashes,
    )
    .expect("complete environment-randomization-V2 window must preflight");
    let rollout_config = AsyncRolloutConfigV2 {
        deck_ids: authorities.deck_ids.clone(),
        learner_seat: PlayerSeatV1::P0,
        environment_seed: authority.seed_v1(),
        opponent_policy_seed: authority.seed_v1(),
        learner_policy_seed: authority.seed_v1(),
        max_physical_decisions: 1_024,
        max_policy_steps: 2_048,
        worker_count: 2,
        sessions_per_worker: 32,
        broker_batch_target: 16,
        first_episode_id: 0,
        episode_count: EPISODES_PER_TAPE_V1,
        scheduler_timeout: Duration::from_millis(30_000),
        measure_broker_service_time: false,
        starting_player: None,
    };
    let observer = ReceiptRetainingObserverV1::new_v1(0, EPISODES_PER_TAPE_V1)
        .expect("receipt-retaining observer must construct");
    let (rollout, observed) = run_async_flat_scored_rollout_native_environment_randomization_v2(
        rollout_config,
        authority.seed_v1(),
        environment_authority,
        Some(Arc::clone(&authorities.engine)),
        &mut scorer,
        observer,
    )
    .expect("authorized Pool3 diagnostic rollout must complete");
    assert_eq!(rollout.episodes.len() as u64, EPISODES_PER_TAPE_V1);
    assert!(rollout.all_natural());
    assert_eq!(
        rollout.metrics.terminal_notification_count,
        EPISODES_PER_TAPE_V1
    );
    assert_eq!(
        scorer.accepted_decision_count_v1(),
        rollout.metrics.scored_decision_count
    );
    assert_eq!(
        rollout.metrics.scored_decision_count,
        rollout.metrics.sampled_action_count
    );
    assert_eq!(
        rollout.metrics.scored_decision_count,
        rollout.metrics.batch_width_sum
    );
    assert_eq!(scorer.last_failure_v1(), None);
    let retained = scorer.into_retained_v1();
    let tape = join_rollout_v1(
        authority,
        &authorities.deck_ids,
        authorities.deck_hashes,
        observed,
        retained,
    )
    .expect("rollout, receipts, scored rows, and schedule must join exactly once");
    assert_eq!(tape.counts.as_array_v1(), authority.expected_counts_v1());
    tape
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeldOutEvaluationErrorV1 {
    EmptyStratum,
    Forward,
    InitialOutputMismatch,
    Atom(ValidationAtomErrorV1),
    NonFinite,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeldOutLossV1 {
    means: StratumMeansV1,
    combined: f64,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl HeldOutLossV1 {
    fn promoted2_v1(self) -> f64 {
        f64::from(self.means.promoted2)
    }
}

/// CPU-only held-out policy loss over the frozen validation trajectory.
/// Actions, terminal returns, and advantages remain those selected at
/// genesis; only the requested state/treatment logits are re-forwarded.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn evaluate_held_out_loss_v1(
    state: &NativePolicyValueTrainStateV1,
    treatment: ActualTreatmentV1,
    tape: &JoinedTapeV1,
    require_initial_outputs: bool,
) -> Result<HeldOutLossV1, HeldOutEvaluationErrorV1> {
    let forward = NativePolicyPackedForwardBuilderV1::from_model_v1(state.model_v1())
        .map_err(|_| HeldOutEvaluationErrorV1::Forward)?;
    let mut sums = [0.0f32; 4];
    let mut counts = [0u32; 4];
    for episode in &tape.episodes {
        let stratum = usize::try_from(episode.stratum)
            .ok()
            .filter(|value| *value < 4)
            .ok_or(HeldOutEvaluationErrorV1::NonFinite)?;
        for group in &episode.groups {
            let first = group
                .substeps
                .first()
                .ok_or(HeldOutEvaluationErrorV1::NonFinite)?;
            let advantage = frozen_advantage_v1(
                episode.authenticated_terminal.learner_return_v1(),
                first.retained.initial_value,
            );
            if !advantage.is_finite() {
                return Err(HeldOutEvaluationErrorV1::NonFinite);
            }
            let mut logits_by_substep = Vec::with_capacity(group.substeps.len());
            let mut selected_actions = Vec::with_capacity(group.substeps.len());
            for substep in &group.substeps {
                let output = forward
                    .forward_v1(encoded_decision_view_v1(
                        treatment.tensor_v1(&substep.retained.lineage),
                    ))
                    .map_err(|_| HeldOutEvaluationErrorV1::Forward)?;
                if !output.value_v1().is_finite()
                    || output.logits_v1().iter().any(|value| !value.is_finite())
                {
                    return Err(HeldOutEvaluationErrorV1::NonFinite);
                }
                if require_initial_outputs
                    && (output.value_v1().to_bits() != substep.predicted_value_bits
                        || output.logits_v1().len() != substep.raw_action_logit_bits.len()
                        || output
                            .logits_v1()
                            .iter()
                            .zip(&substep.raw_action_logit_bits)
                            .any(|(value, expected)| value.to_bits() != *expected))
                {
                    return Err(HeldOutEvaluationErrorV1::InitialOutputMismatch);
                }
                logits_by_substep.push(output.logits_v1().to_vec());
                selected_actions.push(substep.selected_index as usize);
            }
            let atom = validation_atom_v1(&logits_by_substep, &selected_actions, advantage)
                .map_err(HeldOutEvaluationErrorV1::Atom)?;
            sums[stratum] += atom;
            if !sums[stratum].is_finite() {
                return Err(HeldOutEvaluationErrorV1::NonFinite);
            }
            counts[stratum] = counts[stratum]
                .checked_add(1)
                .ok_or(HeldOutEvaluationErrorV1::NonFinite)?;
        }
    }
    if counts.contains(&0) {
        return Err(HeldOutEvaluationErrorV1::EmptyStratum);
    }
    let mut means = [0.0f32; 4];
    for index in 0..4 {
        means[index] = sums[index] / counts[index] as f32;
        if !means[index].is_finite() {
            return Err(HeldOutEvaluationErrorV1::NonFinite);
        }
    }
    let means = StratumMeansV1 {
        promoted2: means[0],
        predecessor_a: means[1],
        predecessor_b: means[2],
        uniform: means[3],
    };
    let combined = combined_validation_loss_v1(means);
    if !combined.is_finite() {
        return Err(HeldOutEvaluationErrorV1::NonFinite);
    }
    Ok(HeldOutLossV1 { means, combined })
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn stratum_means_bit_equal_v1(left: StratumMeansV1, right: StratumMeansV1) -> bool {
    [
        (left.promoted2, right.promoted2),
        (left.predecessor_a, right.predecessor_a),
        (left.predecessor_b, right.predecessor_b),
        (left.uniform, right.uniform),
    ]
    .iter()
    .all(|(left, right)| left.to_bits() == right.to_bits())
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn frame_formal_unit_tape_v1(
    training_authority: &FormalSeedAuthorityV1,
    validation_authority: &FormalSeedAuthorityV1,
    expected_deck_ids: &SessionDeckIdsV1,
    expected_deck_hashes: SessionDeckHashesV1,
    training: &JoinedTapeV1,
    validation: &JoinedTapeV1,
) -> Result<FramedWriterV1, TapeFramingErrorV1> {
    if training_authority.role != FormalTapeRoleV1::Training
        || validation_authority.role != FormalTapeRoleV1::Validation
        || training_authority.unit_index != validation_authority.unit_index
    {
        return Err(TapeFramingErrorV1::CountsDisagreeWithSchedule);
    }
    validate_joined_tape_side_v1(
        training_authority,
        expected_deck_ids,
        expected_deck_hashes,
        training,
    )?;
    validate_joined_tape_side_v1(
        validation_authority,
        expected_deck_ids,
        expected_deck_hashes,
        validation,
    )?;
    if validation.groups_per_stratum_v1().contains(&0) {
        return Err(TapeFramingErrorV1::EmptyStratumGroup);
    }
    let mut writer = FramedWriterV1::new_v1(FORMAL_UNIT_TAPE_SCHEMA_V1);
    writer.u64_v1("unit_index", training_authority.unit_index as u64);
    writer.u64_v1("episodes_per_tape", EPISODES_PER_TAPE_V1);
    frame_joined_tape_side_v1(&mut writer, "training", training)?;
    frame_joined_tape_side_v1(&mut writer, "validation", validation)?;
    Ok(writer)
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn run_seed949999_preflight_repeat_v1(
    gpu: &ValidatedPreflightGpuV1,
    authorities: &PreflightLiveAuthoritiesV1,
) -> PreflightRepeatArtifactsV1 {
    require_bridge_gpu_binding_v1(gpu);
    let full_state = fresh_state_from_parameters_v1(&authorities.genesis.full_parameters);
    let half_state = fresh_state_from_parameters_v1(&authorities.genesis.half_parameters);
    let authority = PreflightSeed949999AuthorityV1::seal_v1();
    let tape = run_neutral_tape_v1(&authority, authorities, &full_state, &half_state);
    // Close the environment-read seam immediately before the bridge selects
    // its CudaDevice ordinal for both arm updates.
    require_bridge_gpu_binding_v1(gpu);
    let paired = execute_actual_paired_update_v1(
        &authority,
        &authorities.genesis,
        &authorities.deck_ids,
        authorities.deck_hashes,
        &tape,
        full_state,
        half_state,
    )
    .expect("both grouped CUDA updates and every paired numerical gate must pass");

    let full_committed = paired
        .full_state
        .snapshot_v1()
        .expect("committed FULL state snapshot");
    let half_committed = paired
        .half_state
        .snapshot_v1()
        .expect("committed HALF state snapshot");
    assert_eq!(full_committed.adam_step, 1);
    assert_eq!(half_committed.adam_step, 1);
    assert!(named_streams_bit_equal_v1(
        &full_committed.parameters,
        &paired.full.after.parameters
    ));
    assert!(named_streams_bit_equal_v1(
        &half_committed.parameters,
        &paired.half.after.parameters
    ));
    assert_eq!(paired.full.deltas.len(), paired.half.deltas.len());
    let full_update_sha256 = paired.full.sha256_v1();
    let half_update_sha256 = paired.half.sha256_v1();
    let mut pair_frame = FramedWriterV1::new_v1(PREFLIGHT_UPDATE_PAIR_SCHEMA_V1);
    pair_frame.text_v1("full_update_sha256", &full_update_sha256);
    pair_frame.text_v1("half_update_sha256", &half_update_sha256);
    let digests = PreflightRepeatDigestsV1 {
        tape_sha256: paired.tape_frame.sha256_v1(),
        full_update_sha256,
        half_update_sha256,
        update_pair_sha256: pair_frame.sha256_v1(),
    };
    PreflightRepeatArtifactsV1 {
        digests,
        tape_frame: paired.tape_frame,
        full_update_frame: paired.full.frame,
        half_update_frame: paired.half.frame,
        update_pair_frame: pair_frame,
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct FormalUnitArtifactsV1 {
    unit_index: usize,
    summary_input: SummaryUnitInputV1,
    tape_frame: FramedWriterV1,
    full_update_frame: FramedWriterV1,
    half_update_frame: FramedWriterV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn run_formal_unit_v1(
    unit_index: usize,
    gpu: &ValidatedPreflightGpuV1,
    authorities: &PreflightLiveAuthoritiesV1,
) -> FormalUnitArtifactsV1 {
    require_bridge_gpu_binding_v1(gpu);
    let training_authority = FormalSeedAuthorityV1::training_v1(unit_index);
    let validation_authority = FormalSeedAuthorityV1::validation_v1(unit_index);
    let full_state = fresh_state_from_parameters_v1(&authorities.genesis.full_parameters);
    let half_state = fresh_state_from_parameters_v1(&authorities.genesis.half_parameters);

    // Both tapes are fixed at the common genesis before either treatment is
    // updated. The validation actions and advantages therefore remain held
    // out from the one training update.
    let training_tape =
        run_neutral_tape_v1(&training_authority, authorities, &full_state, &half_state);
    let validation_tape =
        run_neutral_tape_v1(&validation_authority, authorities, &full_state, &half_state);
    let tape_frame = frame_formal_unit_tape_v1(
        &training_authority,
        &validation_authority,
        &authorities.deck_ids,
        authorities.deck_hashes,
        &training_tape,
        &validation_tape,
    )
    .expect("formal training and validation tapes must bind as one unit");

    let full_before =
        evaluate_held_out_loss_v1(&full_state, ActualTreatmentV1::Full, &validation_tape, true)
            .expect("FULL genesis held-out loss must evaluate");
    let half_before =
        evaluate_held_out_loss_v1(&half_state, ActualTreatmentV1::Half, &validation_tape, true)
            .expect("HALF genesis held-out loss must evaluate");
    assert!(stratum_means_bit_equal_v1(
        full_before.means,
        half_before.means
    ));
    assert_eq!(
        full_before.combined.to_bits(),
        half_before.combined.to_bits()
    );

    require_bridge_gpu_binding_v1(gpu);
    let paired = execute_actual_paired_update_v1(
        &training_authority,
        &authorities.genesis,
        &authorities.deck_ids,
        authorities.deck_hashes,
        &training_tape,
        full_state,
        half_state,
    )
    .expect("formal paired update and every numerical gate must pass");
    let full_after = evaluate_held_out_loss_v1(
        &paired.full_state,
        ActualTreatmentV1::Full,
        &validation_tape,
        false,
    )
    .expect("FULL post-update held-out loss must evaluate");
    let half_after = evaluate_held_out_loss_v1(
        &paired.half_state,
        ActualTreatmentV1::Half,
        &validation_tape,
        false,
    )
    .expect("HALF post-update held-out loss must evaluate");

    let tape_sha256 = tape_frame.sha256_v1();
    let full_update_sha256 = paired.full.sha256_v1();
    let half_update_sha256 = paired.half.sha256_v1();
    let summary_input = SummaryUnitInputV1 {
        unit_index,
        training_seed: training_authority.seed_v1(),
        validation_seed: validation_authority.seed_v1(),
        full_loss_before_bits: full_before.combined.to_bits(),
        full_loss_after_bits: full_after.combined.to_bits(),
        half_loss_before_bits: half_before.combined.to_bits(),
        half_loss_after_bits: half_after.combined.to_bits(),
        promoted2_half_loss_before_bits: half_before.promoted2_v1().to_bits(),
        promoted2_half_loss_after_bits: half_after.promoted2_v1().to_bits(),
        tape_sha256,
        full_update_sha256,
        half_update_sha256,
    };
    assert!(summary_input.improvement_full_v1().is_finite());
    assert!(summary_input.improvement_half_v1().is_finite());
    assert!(summary_input.paired_difference_v1().is_finite());
    assert!(summary_input.promoted2_improvement_v1().is_finite());
    assert!(frame_has_schema_prefix_v1(
        tape_frame.bytes_v1(),
        FORMAL_UNIT_TAPE_SCHEMA_V1
    ));
    assert!(frame_has_schema_prefix_v1(
        paired.full.frame.bytes_v1(),
        UPDATE_SCHEMA_V1
    ));
    assert!(frame_has_schema_prefix_v1(
        paired.half.frame.bytes_v1(),
        UPDATE_SCHEMA_V1
    ));
    assert_eq!(
        exact_framed_atom_occurrences_v1(
            paired.full.frame.bytes_v1(),
            "treatment",
            TREATMENT_FULL_V1.as_bytes(),
        ),
        1
    );
    assert_eq!(
        exact_framed_atom_occurrences_v1(
            paired.half.frame.bytes_v1(),
            "treatment",
            TREATMENT_HALF_V1.as_bytes(),
        ),
        1
    );
    eprintln!(
        "formal unit {}: train_seed={} validation_seed={} I_F_bits={:016x} I_H_bits={:016x} d_bits={:016x} p2_H_bits={:016x}",
        unit_index + 1,
        training_authority.seed_v1(),
        validation_authority.seed_v1(),
        summary_input.improvement_full_v1().to_bits(),
        summary_input.improvement_half_v1().to_bits(),
        summary_input.paired_difference_v1().to_bits(),
        summary_input.promoted2_improvement_v1().to_bits(),
    );
    FormalUnitArtifactsV1 {
        unit_index,
        summary_input,
        tape_frame,
        full_update_frame: paired.full.frame,
        half_update_frame: paired.half.frame,
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn checked_command_output_v1(program: &Path, arguments: &[&str]) -> std::process::Output {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()
        .unwrap_or_else(|_| panic!("{} must be executable", program.display()));
    assert!(
        output.status.success(),
        "{} {arguments:?} failed with {:?}",
        program.display(),
        output.status.code()
    );
    assert!(
        output.stderr.is_empty(),
        "{} {arguments:?} emitted stderr",
        program.display()
    );
    output
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct StableFileIdentityV1 {
    exact_length: u64,
    sha256: String,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct StableFileGuardV1 {
    path: PathBuf,
    retained: std::fs::File,
    initial: StableFileIdentityV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn identity_from_open_file_v1(file: &mut std::fs::File) -> StableFileIdentityV1 {
    let metadata_before = file.metadata().expect("provenance file metadata must read");
    assert!(metadata_before.is_file(), "provenance path must be a file");
    file.seek(SeekFrom::Start(0))
        .expect("provenance file must seek to start");
    let mut hasher = Sha256::new();
    let mut exact_length = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .expect("provenance file must remain readable");
        if count == 0 {
            break;
        }
        exact_length = exact_length
            .checked_add(count as u64)
            .expect("provenance file length must fit u64");
        hasher.update(&buffer[..count]);
    }
    let metadata_after = file.metadata().expect("provenance metadata must reread");
    assert_eq!(metadata_before.len(), exact_length);
    assert_eq!(metadata_after.len(), exact_length);
    file.seek(SeekFrom::Start(0))
        .expect("provenance handle must rewind");
    StableFileIdentityV1 {
        exact_length,
        sha256: hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl StableFileGuardV1 {
    fn begin_v1(path: PathBuf, expected_sha256: Option<&str>) -> Self {
        assert!(path.is_absolute(), "build-bound tool path must be absolute");
        let mut retained = std::fs::File::open(&path)
            .unwrap_or_else(|_| panic!("provenance file must open: {}", path.display()));
        let initial = identity_from_open_file_v1(&mut retained);
        assert!(initial.exact_length > 0, "provenance file must be nonempty");
        let mut reopened = std::fs::File::open(&path)
            .unwrap_or_else(|_| panic!("provenance file must reopen: {}", path.display()));
        assert_eq!(
            identity_from_open_file_v1(&mut reopened),
            initial,
            "provenance path changed across initial recapture"
        );
        if let Some(expected) = expected_sha256 {
            assert!(
                exact_lower_hex_v1(expected, 64),
                "compile-bound provenance digest must be lowercase SHA-256"
            );
            assert_eq!(
                initial.sha256, expected,
                "compile-bound file digest mismatch"
            );
        }
        Self {
            path,
            retained,
            initial,
        }
    }

    fn finish_v1(mut self) -> StableFileIdentityV1 {
        assert_eq!(
            identity_from_open_file_v1(&mut self.retained),
            self.initial,
            "retained provenance file changed during preflight"
        );
        let mut reopened = std::fs::File::open(&self.path)
            .unwrap_or_else(|_| panic!("provenance file must reopen: {}", self.path.display()));
        assert_eq!(
            identity_from_open_file_v1(&mut reopened),
            self.initial,
            "provenance path changed during preflight"
        );
        self.initial
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn required_build_binding_v1(value: Option<&'static str>, label: &str) -> &'static str {
    value.unwrap_or_else(|| {
        panic!(
            "missing compile-bound {label}; rebuild the dedicated live-preflight test executable"
        )
    })
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_no_compile_wrapper_or_flags_v1() {
    for (label, value) in [
        ("RUSTC_WRAPPER", option_env!("RUSTC_WRAPPER")),
        (
            "RUSTC_WORKSPACE_WRAPPER",
            option_env!("RUSTC_WORKSPACE_WRAPPER"),
        ),
        ("RUSTFLAGS", option_env!("RUSTFLAGS")),
        (
            "CARGO_ENCODED_RUSTFLAGS",
            option_env!("CARGO_ENCODED_RUSTFLAGS"),
        ),
        (
            "CARGO_BUILD_RUSTFLAGS",
            option_env!("CARGO_BUILD_RUSTFLAGS"),
        ),
        (
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS",
            option_env!("CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"),
        ),
    ] {
        assert!(
            value.map(str::is_empty).unwrap_or(true),
            "dedicated evidence build rejects {label}"
        );
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct PreflightProvenanceGuardV1 {
    git_tree: &'static str,
    executable: StableFileGuardV1,
    rustc: StableFileGuardV1,
    linker: StableFileGuardV1,
    nvidia_smi: StableFileGuardV1,
    rustc_path: PathBuf,
    linker_path: PathBuf,
    nvidia_smi_path: PathBuf,
    rustc_verbose_stdout: Vec<u8>,
    rustc_verbose_sha256: String,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Debug, Eq, PartialEq)]
struct ValidatedPreflightProvenanceV1 {
    git_commit: &'static str,
    git_tree: &'static str,
    tracked_tree_sha256: &'static str,
    tracked_tree_contract: &'static str,
    toolchain: String,
    rustc_executable_sha256: String,
    linker_path: String,
    linker_executable_sha256: String,
    nvidia_smi_path: String,
    nvidia_smi_sha256: String,
    test_executable_sha256: String,
    test_executable_byte_len: u64,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl PreflightProvenanceGuardV1 {
    // This is a runtime guard: it must still panic if this function is ever
    // called from a debug build, so it stays a plain assert! rather than a
    // const block, which would instead fail every debug compile outright.
    #[allow(clippy::assertions_on_constants)]
    fn begin_v1() -> Self {
        assert!(!cfg!(debug_assertions), "live preflight requires --release");
        assert_eq!(
            env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
            "true",
            "live executable must have been compiled from a clean tree"
        );
        assert!(exact_lower_hex_v1(env!("MTG_KERNEL_BUILD_GIT_HEAD"), 40));
        assert!(exact_lower_hex_v1(
            env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
            64
        ));
        assert!(bounded_identity_text_v1(env!(
            "MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT"
        )));
        let git_tree = env!("MTG_KERNEL_BUILD_GIT_TREE");
        assert!(exact_lower_hex_v1(git_tree, 40));
        require_no_compile_wrapper_or_flags_v1();

        let rustc_path = PathBuf::from(required_build_binding_v1(
            EMBEDDED_BUILD_RUSTC_PATH_V1,
            "absolute RUSTC",
        ));
        let linker_path = PathBuf::from(required_build_binding_v1(
            EMBEDDED_BUILD_LINKER_PATH_V1,
            "absolute CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER",
        ));
        let nvidia_smi_path = PathBuf::from(required_build_binding_v1(
            EMBEDDED_BUILD_NVIDIA_SMI_PATH_V1,
            "MTG_KERNEL_ACTION_BLOCK_BUILD_NVIDIA_SMI_PATH_V1",
        ));
        let rustc_expected = required_build_binding_v1(
            EMBEDDED_BUILD_RUSTC_EXE_SHA256_V1,
            "MTG_KERNEL_ACTION_BLOCK_BUILD_RUSTC_EXE_SHA256_V1",
        );
        let linker_expected = required_build_binding_v1(
            EMBEDDED_BUILD_LINKER_EXE_SHA256_V1,
            "MTG_KERNEL_ACTION_BLOCK_BUILD_LINKER_EXE_SHA256_V1",
        );
        let nvidia_smi_expected = required_build_binding_v1(
            EMBEDDED_BUILD_NVIDIA_SMI_SHA256_V1,
            "MTG_KERNEL_ACTION_BLOCK_BUILD_NVIDIA_SMI_SHA256_V1",
        );
        let rustc_verbose_expected = required_build_binding_v1(
            EMBEDDED_BUILD_RUSTC_VERBOSE_SHA256_V1,
            "MTG_KERNEL_ACTION_BLOCK_BUILD_RUSTC_VERBOSE_SHA256_V1",
        );
        assert!(exact_lower_hex_v1(rustc_verbose_expected, 64));
        let executable = StableFileGuardV1::begin_v1(
            std::env::current_exe().expect("current test executable path must resolve"),
            None,
        );
        let rustc = StableFileGuardV1::begin_v1(rustc_path.clone(), Some(rustc_expected));
        let linker = StableFileGuardV1::begin_v1(linker_path.clone(), Some(linker_expected));
        let nvidia_smi =
            StableFileGuardV1::begin_v1(nvidia_smi_path.clone(), Some(nvidia_smi_expected));
        let output = std::process::Command::new(&rustc_path)
            .arg("-vV")
            .output()
            .expect("compile-bound rustc must execute");
        assert!(output.status.success(), "compile-bound rustc -vV failed");
        assert!(!output.stdout.is_empty(), "rustc -vV must be nonempty");
        let rustc_verbose_sha256 = lower_hex_raw32_v1(sha256_v1(&output.stdout));
        assert_eq!(rustc_verbose_sha256, rustc_verbose_expected);
        Self {
            git_tree,
            executable,
            rustc,
            linker,
            nvidia_smi,
            rustc_path,
            linker_path,
            nvidia_smi_path,
            rustc_verbose_stdout: output.stdout,
            rustc_verbose_sha256,
        }
    }

    fn finish_v1(self) -> ValidatedPreflightProvenanceV1 {
        let output = std::process::Command::new(&self.rustc_path)
            .arg("-vV")
            .output()
            .expect("compile-bound rustc must re-execute");
        assert!(
            output.status.success(),
            "compile-bound rustc -vV recheck failed"
        );
        assert_eq!(output.stdout, self.rustc_verbose_stdout);
        assert_eq!(
            lower_hex_raw32_v1(sha256_v1(&output.stdout)),
            self.rustc_verbose_sha256
        );
        let toolchain_text = String::from_utf8(output.stdout)
            .expect("rustc -vV must be UTF-8")
            .trim()
            .to_owned();
        assert!(bounded_identity_text_v1(&toolchain_text));
        let executable = self.executable.finish_v1();
        let rustc = self.rustc.finish_v1();
        let linker = self.linker.finish_v1();
        let nvidia_smi = self.nvidia_smi.finish_v1();
        let rustc_path = self
            .rustc_path
            .to_str()
            .expect("rustc path must be UTF-8 for manifest");
        let linker_path = self
            .linker_path
            .to_str()
            .expect("linker path must be UTF-8 for manifest");
        let nvidia_smi_path = self
            .nvidia_smi_path
            .to_str()
            .expect("nvidia-smi path must be UTF-8 for manifest");
        ValidatedPreflightProvenanceV1 {
            git_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD"),
            git_tree: self.git_tree,
            tracked_tree_sha256: env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
            tracked_tree_contract: env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT"),
            toolchain: format!(
                "path={rustc_path}\nexecutable_sha256={}\nverbose_stdout_sha256={}\n{}",
                rustc.sha256, self.rustc_verbose_sha256, toolchain_text
            ),
            rustc_executable_sha256: rustc.sha256,
            linker_path: linker_path.to_owned(),
            linker_executable_sha256: linker.sha256,
            nvidia_smi_path: nvidia_smi_path.to_owned(),
            nvidia_smi_sha256: nvidia_smi.sha256,
            test_executable_sha256: executable.sha256,
            test_executable_byte_len: executable.exact_length,
        }
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
const PREFLIGHT_ARTIFACT_BASENAMES_V1: [&str; 5] = [
    "tape.frame",
    "full-update.frame",
    "half-update.frame",
    "update-pair.frame",
    "manifest.frame",
];

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct SealedPreflightArtifactFileV1 {
    basename: &'static str,
    bytes: Vec<u8>,
}

/// The only capability accepted by persistence. Its private constructor
/// recomputes every role digest, proves FULL/HALF role assignment, rebuilds
/// the pair frame, and constructs the manifest internally from the same fresh
/// digests. No separately mutable generic frame can cross this boundary.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct ValidatedPreflightPublicationBundleV1 {
    files: [SealedPreflightArtifactFileV1; 5],
    digests: PreflightRepeatDigestsV1,
    manifest_sha256: String,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreflightPublicationValidationErrorV1 {
    ClaimedDigest,
    SchemaRole,
    PairBinding,
    Manifest,
}

fn exact_framed_atom_occurrences_v1(bytes: &[u8], label: &str, payload: &[u8]) -> usize {
    let mut atom = Vec::new();
    atom.extend_from_slice(&(label.len() as u32).to_be_bytes());
    atom.extend_from_slice(label.as_bytes());
    atom.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    atom.extend_from_slice(payload);
    bytes
        .windows(atom.len())
        .filter(|window| *window == atom.as_slice())
        .count()
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn frame_has_schema_prefix_v1(bytes: &[u8], schema: &str) -> bool {
    bytes.starts_with(FramedWriterV1::new_v1(schema).bytes_v1())
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn validate_preflight_artifact_roles_v1(
    artifacts: &PreflightRepeatArtifactsV1,
) -> Result<PreflightRepeatDigestsV1, PreflightPublicationValidationErrorV1> {
    let tape_sha256 = lower_hex_raw32_v1(sha256_v1(artifacts.tape_frame.bytes_v1()));
    let full_update_sha256 = lower_hex_raw32_v1(sha256_v1(artifacts.full_update_frame.bytes_v1()));
    let half_update_sha256 = lower_hex_raw32_v1(sha256_v1(artifacts.half_update_frame.bytes_v1()));
    if artifacts.digests.tape_sha256 != tape_sha256
        || artifacts.digests.full_update_sha256 != full_update_sha256
        || artifacts.digests.half_update_sha256 != half_update_sha256
    {
        return Err(PreflightPublicationValidationErrorV1::ClaimedDigest);
    }
    if !frame_has_schema_prefix_v1(artifacts.tape_frame.bytes_v1(), JOINED_TAPE_SCHEMA_V1)
        || !frame_has_schema_prefix_v1(artifacts.full_update_frame.bytes_v1(), UPDATE_SCHEMA_V1)
        || !frame_has_schema_prefix_v1(artifacts.half_update_frame.bytes_v1(), UPDATE_SCHEMA_V1)
        || exact_framed_atom_occurrences_v1(
            artifacts.full_update_frame.bytes_v1(),
            "treatment",
            TREATMENT_FULL_V1.as_bytes(),
        ) != 1
        || exact_framed_atom_occurrences_v1(
            artifacts.full_update_frame.bytes_v1(),
            "treatment",
            TREATMENT_HALF_V1.as_bytes(),
        ) != 0
        || exact_framed_atom_occurrences_v1(
            artifacts.half_update_frame.bytes_v1(),
            "treatment",
            TREATMENT_HALF_V1.as_bytes(),
        ) != 1
        || exact_framed_atom_occurrences_v1(
            artifacts.half_update_frame.bytes_v1(),
            "treatment",
            TREATMENT_FULL_V1.as_bytes(),
        ) != 0
    {
        return Err(PreflightPublicationValidationErrorV1::SchemaRole);
    }
    let mut expected_pair = FramedWriterV1::new_v1(PREFLIGHT_UPDATE_PAIR_SCHEMA_V1);
    expected_pair.text_v1("full_update_sha256", &full_update_sha256);
    expected_pair.text_v1("half_update_sha256", &half_update_sha256);
    if artifacts.update_pair_frame.bytes_v1() != expected_pair.bytes_v1() {
        return Err(PreflightPublicationValidationErrorV1::PairBinding);
    }
    let update_pair_sha256 = expected_pair.sha256_v1();
    if artifacts.digests.update_pair_sha256 != update_pair_sha256 {
        return Err(PreflightPublicationValidationErrorV1::ClaimedDigest);
    }
    Ok(PreflightRepeatDigestsV1 {
        tape_sha256,
        full_update_sha256,
        half_update_sha256,
        update_pair_sha256,
    })
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl ValidatedPreflightPublicationBundleV1 {
    fn seal_v1(
        artifacts: PreflightRepeatArtifactsV1,
        provenance: &ValidatedPreflightProvenanceV1,
        gpu: &ValidatedPreflightGpuV1,
    ) -> Result<Self, PreflightPublicationValidationErrorV1> {
        let digests = validate_preflight_artifact_roles_v1(&artifacts)?;
        assert_eq!(gpu.ordinal, PREFLIGHT_GPU_ORDINAL_U64_V1);
        assert_eq!(gpu.name, PREFLIGHT_GPU_NAME_V1);
        assert_eq!(gpu.uuid, PREFLIGHT_GPU_UUID_V1);
        let mut expected_pair = FramedWriterV1::new_v1(PREFLIGHT_UPDATE_PAIR_SCHEMA_V1);
        expected_pair.text_v1("full_update_sha256", &digests.full_update_sha256);
        expected_pair.text_v1("half_update_sha256", &digests.half_update_sha256);
        let manifest = frame_manifest_v1(PreflightManifestInputV1 {
            git_commit: provenance.git_commit,
            git_tree: provenance.git_tree,
            tracked_tree_sha256: provenance.tracked_tree_sha256,
            tracked_tree_contract: provenance.tracked_tree_contract,
            toolchain: &provenance.toolchain,
            rustc_executable_sha256: &provenance.rustc_executable_sha256,
            linker_path: &provenance.linker_path,
            linker_executable_sha256: &provenance.linker_executable_sha256,
            nvidia_smi_path: &provenance.nvidia_smi_path,
            nvidia_smi_sha256: &provenance.nvidia_smi_sha256,
            test_executable_sha256: &provenance.test_executable_sha256,
            test_executable_byte_len: provenance.test_executable_byte_len,
            tape_sha256: &digests.tape_sha256,
            full_update_sha256: &digests.full_update_sha256,
            half_update_sha256: &digests.half_update_sha256,
            update_pair_sha256: &digests.update_pair_sha256,
        })
        .map_err(|_| PreflightPublicationValidationErrorV1::Manifest)?;
        let manifest_sha256 = manifest.sha256_v1();
        Ok(Self {
            files: [
                SealedPreflightArtifactFileV1 {
                    basename: PREFLIGHT_ARTIFACT_BASENAMES_V1[0],
                    bytes: artifacts.tape_frame.buffer,
                },
                SealedPreflightArtifactFileV1 {
                    basename: PREFLIGHT_ARTIFACT_BASENAMES_V1[1],
                    bytes: artifacts.full_update_frame.buffer,
                },
                SealedPreflightArtifactFileV1 {
                    basename: PREFLIGHT_ARTIFACT_BASENAMES_V1[2],
                    bytes: artifacts.half_update_frame.buffer,
                },
                SealedPreflightArtifactFileV1 {
                    basename: PREFLIGHT_ARTIFACT_BASENAMES_V1[3],
                    bytes: expected_pair.buffer,
                },
                SealedPreflightArtifactFileV1 {
                    basename: PREFLIGHT_ARTIFACT_BASENAMES_V1[4],
                    bytes: manifest.buffer,
                },
            ],
            digests,
            manifest_sha256,
        })
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn require_absent_path_v1(path: &Path, label: &str) {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => panic!("{label} must not already exist: {}", path.display()),
        Err(error) => panic!("cannot inspect {label} {}: {error}", path.display()),
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn verify_exact_artifact_inventory_v1(
    parent: &crate::durable_publication_v1::ValidatedPublicationParentV1,
    files: &[SealedPreflightArtifactFileV1; 5],
) {
    let mut observed = std::fs::read_dir(parent.canonical_path())
        .expect("artifact directory inventory must read")
        .map(|entry| {
            let entry = entry.expect("artifact directory entry must read");
            assert!(
                entry
                    .file_type()
                    .expect("artifact type must read")
                    .is_file(),
                "artifact inventory may contain only regular files"
            );
            entry
                .file_name()
                .to_str()
                .expect("artifact basename must be UTF-8")
                .to_owned()
        })
        .collect::<Vec<_>>();
    observed.sort();
    let mut expected = PREFLIGHT_ARTIFACT_BASENAMES_V1
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(observed, expected, "artifact inventory must be exact");
    for file in files {
        let expectation =
            crate::durable_publication_v1::DurableFileExpectationV1::from_bytes(&file.bytes)
                .expect("sealed artifact expectation must construct");
        crate::durable_publication_v1::verify_existing_publication_v1(
            parent,
            file.basename,
            expectation,
        )
        .unwrap_or_else(|_| panic!("artifact must reread exactly: {}", file.basename));
    }
}

#[cfg(all(feature = "experimental-burn-net8-packed-cuda-v1", windows))]
fn move_preflight_directory_write_through_v1(
    stage: &Path,
    final_path: &Path,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_WRITE_THROUGH_V1: u32 = 0x0000_0008;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    fn wide(path: &Path) -> std::io::Result<Vec<u16>> {
        let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if value.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "publication path contains NUL",
            ));
        }
        value.push(0);
        Ok(value)
    }
    let stage = wide(stage)?;
    let final_path = wide(final_path)?;
    // SAFETY: both paths are live NUL-terminated UTF-16 buffers. No replace
    // or copy flag is present; a destination collision therefore fails.
    let success = unsafe {
        MoveFileExW(
            stage.as_ptr(),
            final_path.as_ptr(),
            MOVEFILE_WRITE_THROUGH_V1,
        )
    };
    if success == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(feature = "experimental-burn-net8-packed-cuda-v1", not(windows)))]
fn move_preflight_directory_write_through_v1(
    _stage: &Path,
    _final_path: &Path,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "preflight directory publication is Windows-only",
    ))
}

#[cfg(all(feature = "experimental-burn-net8-packed-cuda-v1", windows))]
#[test]
fn preflight_directory_move_is_no_replace_and_preserves_collision_v1() {
    struct TempTreeV1(PathBuf);

    impl Drop for TempTreeV1 {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let temporary_parent = std::env::temp_dir();
    let unique = format!(
        "mtg-kernel-preflight-dir-move-v1-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must follow the Unix epoch")
            .as_nanos()
    );
    let root = temporary_parent.join(unique);
    assert!(root.starts_with(&temporary_parent));
    std::fs::create_dir(&root).expect("unique move-test root must be created new");
    let _cleanup = TempTreeV1(root.clone());

    let staging = root.join("candidate.partial");
    let final_path = root.join("candidate");
    std::fs::create_dir(&staging).expect("first staging directory must be created new");
    std::fs::write(staging.join("marker"), b"first").expect("first staging marker must write");
    move_preflight_directory_write_through_v1(&staging, &final_path)
        .expect("absent final directory must accept the complete staging directory");
    assert!(!staging.exists());
    assert_eq!(
        std::fs::read(final_path.join("marker")).expect("published marker must reread"),
        b"first"
    );

    std::fs::create_dir(&staging).expect("collision staging directory must be created new");
    std::fs::write(staging.join("marker"), b"second").expect("collision staging marker must write");
    assert!(move_preflight_directory_write_through_v1(&staging, &final_path).is_err());
    assert_eq!(
        std::fs::read(final_path.join("marker")).expect("existing final marker must survive"),
        b"first"
    );
    assert_eq!(
        std::fs::read(staging.join("marker")).expect("failed staging marker must survive"),
        b"second"
    );
}

/// Crash-consistent, no-replace publication of the already sealed bundle.
/// Before the top-level move, any failure leaves only `.partial`; after a
/// successful move, any verification failure leaves the complete final
/// directory but returns no receipt and never adopts, replaces, or deletes it.
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn persist_preflight_artifacts_v1(bundle: ValidatedPreflightPublicationBundleV1) {
    let requested = PathBuf::from(
        std::env::var("MTG_KERNEL_ACTION_BLOCK_PREFLIGHT_OUTPUT_DIR_V1")
            .expect("an explicit new preflight output directory is required"),
    );
    assert!(requested.is_absolute(), "preflight output must be absolute");
    let leaf = requested
        .file_name()
        .expect("preflight output must have a normal leaf")
        .to_os_string();
    let requested_parent = requested
        .parent()
        .expect("preflight output must have an existing parent");
    let outer_parent =
        crate::durable_publication_v1::capture_existing_publication_parent_v1(requested_parent)
            .expect("preflight parent must be a validated existing directory");
    let final_dir = crate::durable_publication_v1::child_path_v1(&outer_parent, &leaf)
        .expect("preflight output leaf must be one safe child name");
    let mut staging_leaf = leaf.clone();
    staging_leaf.push(".partial");
    let staging_dir = crate::durable_publication_v1::child_path_v1(&outer_parent, &staging_leaf)
        .expect("preflight staging leaf must be one safe child name");
    require_absent_path_v1(&final_dir, "final preflight directory");
    require_absent_path_v1(&staging_dir, "staging preflight directory");
    crate::durable_publication_v1::revalidate_parent_v1(&outer_parent)
        .expect("preflight parent must remain stable before staging creation");
    std::fs::create_dir(&staging_dir).expect("staging directory must be created new");
    let staging_parent =
        crate::durable_publication_v1::capture_existing_publication_parent_v1(&staging_dir)
            .expect("new staging directory must validate");
    for file in &bundle.files {
        let expectation =
            crate::durable_publication_v1::DurableFileExpectationV1::from_bytes(&file.bytes)
                .expect("sealed artifact expectation must construct");
        let stage_name = format!("{}.publish-stage", file.basename);
        crate::durable_move_publication_v2::publish_immutable_file_by_move_v2(
            &staging_parent,
            stage_name,
            file.basename,
            &file.bytes,
            expectation,
        )
        .unwrap_or_else(|_| panic!("durable artifact publication failed: {}", file.basename));
    }
    verify_exact_artifact_inventory_v1(&staging_parent, &bundle.files);
    crate::durable_publication_v1::revalidate_parent_v1(&outer_parent)
        .expect("preflight parent must remain stable before directory publication");
    require_absent_path_v1(&final_dir, "final preflight directory");
    move_preflight_directory_write_through_v1(&staging_dir, &final_dir)
        .expect("complete preflight directory must publish write-through without replacement");
    crate::durable_publication_v1::revalidate_parent_v1(&outer_parent)
        .expect("preflight parent must remain stable after directory publication");
    require_absent_path_v1(&staging_dir, "published staging directory");
    let final_parent =
        crate::durable_publication_v1::capture_existing_publication_parent_v1(&final_dir)
            .expect("published final directory must validate");
    verify_exact_artifact_inventory_v1(&final_parent, &bundle.files);
    eprintln!(
        "seed949999 preflight artifacts: dir={} tape={} full={} half={} pair={} manifest={}",
        final_parent.canonical_path().display(),
        bundle.digests.tape_sha256,
        bundle.digests.full_update_sha256,
        bundle.digests.half_update_sha256,
        bundle.digests.update_pair_sha256,
        bundle.manifest_sha256,
    );
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
#[ignore = "authorized seed-949999 Pool3 GPU1 preflight; native Windows/MSVC, dedicated process, and explicit invocation required"]
// Runtime platform guard for an explicitly-invoked live test: must still
// panic if run on the wrong platform rather than fail to compile there.
#[allow(clippy::assertions_on_constants)]
fn action_block_gradient_preflight_seed949999_gpu1_v1() {
    assert!(
        cfg!(all(
            target_arch = "x86_64",
            target_os = "windows",
            target_env = "msvc"
        )),
        "the validated Store authority and real preflight are native Windows/MSVC only"
    );
    let provenance = PreflightProvenanceGuardV1::begin_v1();
    let gpu = require_fresh_physical_gpu1_v1(PREFLIGHT_LIVE_TEST_NAME_SUFFIX_V1);
    let exclusivity = BoundedGpu1ExclusivityMonitorV1::start_v1();
    let authorities = load_preflight_live_authorities_v1();
    let first = run_seed949999_preflight_repeat_v1(&gpu, &authorities);
    let second = run_seed949999_preflight_repeat_v1(&gpu, &authorities);
    assert_eq!(
        &first.digests, &second.digests,
        "both tape and paired update must repeat exactly"
    );
    assert_eq!(first.tape_frame.bytes_v1(), second.tape_frame.bytes_v1());
    assert_eq!(
        first.full_update_frame.bytes_v1(),
        second.full_update_frame.bytes_v1()
    );
    assert_eq!(
        first.half_update_frame.bytes_v1(),
        second.half_update_frame.bytes_v1()
    );
    assert_eq!(
        first.update_pair_frame.bytes_v1(),
        second.update_pair_frame.bytes_v1()
    );
    drop(second);
    exclusivity.finish_v1(&gpu);
    let provenance = provenance.finish_v1();
    let bundle = ValidatedPreflightPublicationBundleV1::seal_v1(first, &provenance, &gpu)
        .expect("repeat artifacts, provenance, and manifest must seal atomically");
    persist_preflight_artifacts_v1(bundle);
    // The identity-bound primary context remains live through publication.
    drop(gpu);
}

/// The authoritative per-unit record the summary is derived FROM.
///
/// Self-recomputation of a classifier result can only prove its derived
/// fields agree with its own published raw arrays; it cannot detect a
/// self-consistent replacement of those arrays. These typed unit records are
/// the external authority that closes that gap: the summary re-derives the
/// six-element arrays from the held-out loss bits and requires the published
/// arrays to match bit for bit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SummaryUnitInputV1 {
    pub(super) unit_index: usize,
    pub(super) training_seed: u64,
    pub(super) validation_seed: u64,
    /// Combined held-out loss `L(T)` raw f64 bits, before and after the one
    /// update, for each treatment.
    pub(super) full_loss_before_bits: u64,
    pub(super) full_loss_after_bits: u64,
    pub(super) half_loss_before_bits: u64,
    pub(super) half_loss_after_bits: u64,
    /// promoted(2)-stratum-only held-out loss bits for the H arm, which the
    /// safeguard consumes.
    pub(super) promoted2_half_loss_before_bits: u64,
    pub(super) promoted2_half_loss_after_bits: u64,
    /// The artifacts this unit's numbers came from.
    pub(super) tape_sha256: String,
    pub(super) full_update_sha256: String,
    pub(super) half_update_sha256: String,
}

impl SummaryUnitInputV1 {
    /// `I_s(F) = L_before(F) - L_after(F)`.
    pub(super) fn improvement_full_v1(&self) -> f64 {
        improvement_v1(
            f64::from_bits(self.full_loss_before_bits),
            f64::from_bits(self.full_loss_after_bits),
        )
    }

    /// `I_s(H) = L_before(H) - L_after(H)`.
    pub(super) fn improvement_half_v1(&self) -> f64 {
        improvement_v1(
            f64::from_bits(self.half_loss_before_bits),
            f64::from_bits(self.half_loss_after_bits),
        )
    }

    /// `d_s = I_s(H) - I_s(F)`.
    pub(super) fn paired_difference_v1(&self) -> f64 {
        paired_difference_v1(self.improvement_half_v1(), self.improvement_full_v1())
    }

    /// promoted(2) `I_s,p2(H)`.
    pub(super) fn promoted2_improvement_v1(&self) -> f64 {
        improvement_v1(
            f64::from_bits(self.promoted2_half_loss_before_bits),
            f64::from_bits(self.promoted2_half_loss_after_bits),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SummaryFramingErrorV1 {
    /// The result's derived fields disagree with an independent
    /// recomputation from its own published inputs.
    ForgedResult,
    /// The unit records were not exactly six, in unit order, or carried a
    /// malformed artifact digest.
    MalformedUnits,
    /// A published raw array does not match the array derived from the
    /// authoritative unit records. This is the forgery self-recomputation
    /// alone cannot see.
    RawInputsDisagreeWithUnits,
}

/// Derives the two six-element classifier input arrays from the
/// authoritative unit records, in the frozen arithmetic order.
pub(super) fn derive_classifier_inputs_v1(
    units: &[SummaryUnitInputV1],
) -> Option<(
    [f64; BOOTSTRAP_UNIT_COUNT_V1],
    [f64; BOOTSTRAP_UNIT_COUNT_V1],
)> {
    if units.len() != BOOTSTRAP_UNIT_COUNT_V1 {
        return None;
    }
    let mut paired = [0.0f64; BOOTSTRAP_UNIT_COUNT_V1];
    let mut promoted2 = [0.0f64; BOOTSTRAP_UNIT_COUNT_V1];
    for (index, unit) in units.iter().enumerate() {
        if unit.unit_index != index
            || unit.training_seed != FORMAL_TRAINING_SEEDS_V1[index]
            || unit.validation_seed != FORMAL_VALIDATION_SEEDS_V1[index]
        {
            return None;
        }
        paired[index] = unit.paired_difference_v1();
        promoted2[index] = unit.promoted2_improvement_v1();
    }
    Some((paired, promoted2))
}

fn lowercase_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

/// Frames the six-unit summary with every classifier input explicit.
///
/// Two independent gates run before a single byte is bound:
///
/// 1. the published raw arrays must equal, bit for bit, the arrays derived
///    from the authoritative unit records (rejects raw-array forgery);
/// 2. the result's derived fields must survive self-recomputation (rejects
///    derived-field forgery).
///
/// Neither gate subsumes the other.
pub(super) fn frame_summary_v1(
    units: &[SummaryUnitInputV1],
    result: &ClassifierResultV1,
) -> Result<FramedWriterV1, SummaryFramingErrorV1> {
    let (derived_paired, derived_promoted2) =
        derive_classifier_inputs_v1(units).ok_or(SummaryFramingErrorV1::MalformedUnits)?;
    for unit in units {
        if !lowercase_sha256_v1(&unit.tape_sha256)
            || !lowercase_sha256_v1(&unit.full_update_sha256)
            || !lowercase_sha256_v1(&unit.half_update_sha256)
        {
            return Err(SummaryFramingErrorV1::MalformedUnits);
        }
    }
    // Gate 1: the published arrays must BE the authoritative arrays.
    if !f64_arrays_bit_equal_v1(&derived_paired, &result.paired_differences)
        || !f64_arrays_bit_equal_v1(&derived_promoted2, &result.promoted2_improvements)
    {
        return Err(SummaryFramingErrorV1::RawInputsDisagreeWithUnits);
    }
    // Gate 2: the derived fields must survive self-recomputation.
    if !verify_classifier_result_v1(result) {
        return Err(SummaryFramingErrorV1::ForgedResult);
    }
    let mut writer = FramedWriterV1::new_v1(SUMMARY_SCHEMA_V1);
    // Bind the authoritative unit records the arrays were derived from.
    writer.u64_v1("unit_count", units.len() as u64);
    for unit in units {
        let label = format!("unit[{}]", unit.unit_index);
        writer.u64_v1(&format!("{label}.training_seed"), unit.training_seed);
        writer.u64_v1(&format!("{label}.validation_seed"), unit.validation_seed);
        writer.u64_v1(
            &format!("{label}.full_loss_before_bits"),
            unit.full_loss_before_bits,
        );
        writer.u64_v1(
            &format!("{label}.full_loss_after_bits"),
            unit.full_loss_after_bits,
        );
        writer.u64_v1(
            &format!("{label}.half_loss_before_bits"),
            unit.half_loss_before_bits,
        );
        writer.u64_v1(
            &format!("{label}.half_loss_after_bits"),
            unit.half_loss_after_bits,
        );
        writer.u64_v1(
            &format!("{label}.promoted2_half_loss_before_bits"),
            unit.promoted2_half_loss_before_bits,
        );
        writer.u64_v1(
            &format!("{label}.promoted2_half_loss_after_bits"),
            unit.promoted2_half_loss_after_bits,
        );
        writer.text_v1(&format!("{label}.tape_sha256"), &unit.tape_sha256);
        writer.text_v1(
            &format!("{label}.full_update_sha256"),
            &unit.full_update_sha256,
        );
        writer.text_v1(
            &format!("{label}.half_update_sha256"),
            &unit.half_update_sha256,
        );
    }
    writer.text_v1("disposition", result.disposition.name_v1());
    writer.f64_bits_array_v1("paired_differences", &result.paired_differences);
    writer.f64_bits_array_v1("promoted2_improvements", &result.promoted2_improvements);
    writer.u64_v1("positive_paired_count", result.positive_paired_count as u64);
    writer.u64_v1(
        "positive_promoted2_count",
        result.positive_promoted2_count as u64,
    );
    writer.u64_v1("bootstrap_low_index", BOOTSTRAP_LOW_INDEX_V1 as u64);
    writer.u64_v1("bootstrap_high_index", BOOTSTRAP_HIGH_INDEX_V1 as u64);
    writer.u64_v1("bootstrap_tuple_count", BOOTSTRAP_TUPLE_COUNT_V1 as u64);
    writer.u64_v1("required_positive_units", REQUIRED_POSITIVE_UNITS_V1 as u64);
    // Publish each gate outcome individually, not just their conjunction.
    for (label, outcome) in [
        (
            "gate.identity_and_numerical",
            result.gates.identity_and_numerical_gates_pass,
        ),
        ("gate.paired_sign", result.gates.paired_sign_gate),
        ("gate.paired_interval", result.gates.paired_interval_gate),
        ("gate.promoted2_sign", result.gates.promoted2_sign_gate),
        (
            "gate.promoted2_interval",
            result.gates.promoted2_interval_gate,
        ),
        ("gate.bootstraps_readable", result.gates.bootstraps_readable),
    ] {
        writer.u64_v1(label, u64::from(outcome));
    }
    for (label, read) in [
        ("paired_read", result.paired_read),
        ("promoted2_read", result.promoted2_read),
    ] {
        match read {
            Some(read) => {
                writer.f64_bits_array_v1(label, &[read.low_index_value, read.high_index_value]);
            }
            None => writer.atom_v1(label, b""),
        }
    }
    Ok(writer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ManifestFramingErrorV1 {
    GitObject,
    ToolIdentity,
    BuildIdentity,
    ArtifactDigest,
}

/// Closed preflight-manifest input. Artifact roles are typed fields, so a
/// caller cannot omit one, duplicate a label, substitute a formal seed, or
/// smuggle an arbitrary input/output list under the V1 schema.
#[derive(Clone, Copy)]
pub(super) struct PreflightManifestInputV1<'a> {
    pub(super) git_commit: &'a str,
    pub(super) git_tree: &'a str,
    pub(super) tracked_tree_sha256: &'a str,
    pub(super) tracked_tree_contract: &'a str,
    pub(super) toolchain: &'a str,
    pub(super) rustc_executable_sha256: &'a str,
    pub(super) linker_path: &'a str,
    pub(super) linker_executable_sha256: &'a str,
    pub(super) nvidia_smi_path: &'a str,
    pub(super) nvidia_smi_sha256: &'a str,
    pub(super) test_executable_sha256: &'a str,
    pub(super) test_executable_byte_len: u64,
    pub(super) tape_sha256: &'a str,
    pub(super) full_update_sha256: &'a str,
    pub(super) half_update_sha256: &'a str,
    pub(super) update_pair_sha256: &'a str,
}

fn exact_lower_hex_v1(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bounded_identity_text_v1(value: &str) -> bool {
    !value.is_empty() && value.len() <= 65_536 && !value.as_bytes().contains(&0)
}

/// The one small, closed seed-949999 preflight manifest. Formal seeds are
/// deliberately absent: they remain pure-schedule authorities and cannot be
/// serialized as if measurement had been authorized.
pub(super) fn frame_manifest_v1(
    input: PreflightManifestInputV1<'_>,
) -> Result<FramedWriterV1, ManifestFramingErrorV1> {
    if !exact_lower_hex_v1(input.git_commit, 40) || !exact_lower_hex_v1(input.git_tree, 40) {
        return Err(ManifestFramingErrorV1::GitObject);
    }
    if !bounded_identity_text_v1(input.tracked_tree_contract)
        || !bounded_identity_text_v1(input.toolchain)
        || !bounded_identity_text_v1(input.linker_path)
        || !bounded_identity_text_v1(input.nvidia_smi_path)
    {
        return Err(ManifestFramingErrorV1::ToolIdentity);
    }
    if input.test_executable_byte_len == 0
        || [
            input.tracked_tree_sha256,
            input.rustc_executable_sha256,
            input.linker_executable_sha256,
            input.nvidia_smi_sha256,
            input.test_executable_sha256,
        ]
        .iter()
        .any(|digest| !exact_lower_hex_v1(digest, 64))
    {
        return Err(ManifestFramingErrorV1::BuildIdentity);
    }
    for digest in [
        input.tape_sha256,
        input.full_update_sha256,
        input.half_update_sha256,
        input.update_pair_sha256,
    ] {
        if !exact_lower_hex_v1(digest, 64) {
            return Err(ManifestFramingErrorV1::ArtifactDigest);
        }
    }
    let mut writer = FramedWriterV1::new_v1(MANIFEST_SCHEMA_V1);
    writer.text_v1("design_sha256", DESIGN_DOCUMENT_SHA256_V1);
    writer.u64_v1("design_bytes", DESIGN_DOCUMENT_BYTE_COUNT_V1);
    writer.u64_v1("design_lines", DESIGN_DOCUMENT_LINE_COUNT_V1);
    writer.text_v1("git_commit", input.git_commit);
    writer.text_v1("git_tree", input.git_tree);
    writer.text_v1("tracked_tree_sha256", input.tracked_tree_sha256);
    writer.text_v1("tracked_tree_contract", input.tracked_tree_contract);
    writer.text_v1("toolchain", input.toolchain);
    writer.text_v1("rustc_executable_sha256", input.rustc_executable_sha256);
    writer.text_v1("linker_path", input.linker_path);
    writer.text_v1("linker_executable_sha256", input.linker_executable_sha256);
    writer.text_v1("nvidia_smi_path", input.nvidia_smi_path);
    writer.text_v1("nvidia_smi_sha256", input.nvidia_smi_sha256);
    writer.text_v1("test_executable_sha256", input.test_executable_sha256);
    writer.u64_v1("test_executable_byte_len", input.test_executable_byte_len);
    writer.text_v1("target", "x86_64-pc-windows-msvc");
    writer.text_v1("backend_identity", DIAGNOSTIC_BACKEND_IDENTITY_V1);
    writer.text_v1("vendored_tree_object", VENDORED_SIMPLEUNIT_TREE_OBJECT_V1);
    writer.text_v1("pool3_sha256", POOL3_DOCUMENT_SHA256_V1);
    writer.text_v1("source_run_sha256", SOURCE_RUN_SHA256_V1);
    writer.text_v1("source_checkpoint_sha256", SOURCE_CHECKPOINT_SHA256_V1);
    writer.text_v1("source_sidecar_sha256", SOURCE_SIDECAR_SHA256_V1);
    writer.text_v1("source_payload_sha256", SOURCE_PAYLOAD_SHA256_V1);
    writer.text_v1(
        "source_model_parameter_sha256",
        SOURCE_MODEL_PARAMETER_SHA256_V1,
    );
    writer.u64_v1("source_base_seed", SOURCE_BASE_SEED_V1);
    writer.u64_v1("source_generation", SOURCE_GENERATION_V1);
    writer.u64_v1("preflight_base_seed", PREFLIGHT_BASE_SEED_V1);
    writer.u64_v1("preflight_episode_count", EPISODES_PER_TAPE_V1);
    writer.u32_array_v1("preflight_pool_counts", &PREFLIGHT_REQUIRED_COUNTS_V1);
    writer.u64_v1("gpu_ordinal", PREFLIGHT_GPU_ORDINAL_U64_V1);
    writer.text_v1("gpu_name", PREFLIGHT_GPU_NAME_V1);
    writer.text_v1("gpu_uuid", PREFLIGHT_GPU_UUID_V1);
    writer.text_v1("tape_sha256", input.tape_sha256);
    writer.text_v1("full_update_sha256", input.full_update_sha256);
    writer.text_v1("half_update_sha256", input.half_update_sha256);
    writer.text_v1("update_pair_sha256", input.update_pair_sha256);
    Ok(writer)
}

// ---------------------------------------------------------------------------
// Authorized single-shot six-unit formal measurement.
// ---------------------------------------------------------------------------

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct FormalArtifactIdentityV1 {
    basename: String,
    exact_length: u64,
    sha256: String,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct FormalPublishedUnitV1 {
    unit_index: usize,
    training_seed: u64,
    validation_seed: u64,
    tape: FormalArtifactIdentityV1,
    full_update: FormalArtifactIdentityV1,
    half_update: FormalArtifactIdentityV1,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn frame_formal_manifest_v1(
    provenance: &ValidatedPreflightProvenanceV1,
    gpu: &ValidatedPreflightGpuV1,
    units: &[FormalPublishedUnitV1],
    summary: &FormalArtifactIdentityV1,
    result: &ClassifierResultV1,
) -> FramedWriterV1 {
    assert_eq!(units.len(), BOOTSTRAP_UNIT_COUNT_V1);
    assert!(verify_classifier_result_v1(result));
    assert_eq!(gpu.ordinal, PREFLIGHT_GPU_ORDINAL_U64_V1);
    assert_eq!(gpu.name, PREFLIGHT_GPU_NAME_V1);
    assert_eq!(gpu.uuid, PREFLIGHT_GPU_UUID_V1);
    let mut writer = FramedWriterV1::new_v1(FORMAL_MANIFEST_SCHEMA_V1);
    writer.text_v1("design_sha256", DESIGN_DOCUMENT_SHA256_V1);
    writer.u64_v1("design_bytes", DESIGN_DOCUMENT_BYTE_COUNT_V1);
    writer.u64_v1("design_lines", DESIGN_DOCUMENT_LINE_COUNT_V1);
    writer.text_v1("git_commit", provenance.git_commit);
    writer.text_v1("git_tree", provenance.git_tree);
    writer.text_v1("tracked_tree_sha256", provenance.tracked_tree_sha256);
    writer.text_v1("tracked_tree_contract", provenance.tracked_tree_contract);
    writer.text_v1("toolchain", &provenance.toolchain);
    writer.text_v1(
        "rustc_executable_sha256",
        &provenance.rustc_executable_sha256,
    );
    writer.text_v1("linker_path", &provenance.linker_path);
    writer.text_v1(
        "linker_executable_sha256",
        &provenance.linker_executable_sha256,
    );
    writer.text_v1("nvidia_smi_path", &provenance.nvidia_smi_path);
    writer.text_v1("nvidia_smi_sha256", &provenance.nvidia_smi_sha256);
    writer.text_v1("test_executable_sha256", &provenance.test_executable_sha256);
    writer.u64_v1(
        "test_executable_byte_len",
        provenance.test_executable_byte_len,
    );
    writer.text_v1("target", "x86_64-pc-windows-msvc");
    writer.text_v1("backend_identity", DIAGNOSTIC_BACKEND_IDENTITY_V1);
    writer.text_v1("vendored_tree_object", VENDORED_SIMPLEUNIT_TREE_OBJECT_V1);
    writer.text_v1("pool3_sha256", POOL3_DOCUMENT_SHA256_V1);
    writer.text_v1("source_run_sha256", SOURCE_RUN_SHA256_V1);
    writer.text_v1("source_checkpoint_sha256", SOURCE_CHECKPOINT_SHA256_V1);
    writer.text_v1("source_sidecar_sha256", SOURCE_SIDECAR_SHA256_V1);
    writer.text_v1("source_payload_sha256", SOURCE_PAYLOAD_SHA256_V1);
    writer.text_v1(
        "source_model_parameter_sha256",
        SOURCE_MODEL_PARAMETER_SHA256_V1,
    );
    writer.u64_v1("source_base_seed", SOURCE_BASE_SEED_V1);
    writer.u64_v1("source_generation", SOURCE_GENERATION_V1);
    writer.u64_v1("gpu_ordinal", gpu.ordinal);
    writer.text_v1("gpu_name", &gpu.name);
    writer.text_v1("gpu_uuid", &gpu.uuid);
    writer.u64_v1("unit_count", units.len() as u64);
    for (index, unit) in units.iter().enumerate() {
        assert_eq!(unit.unit_index, index);
        assert_eq!(unit.training_seed, FORMAL_TRAINING_SEEDS_V1[index]);
        assert_eq!(unit.validation_seed, FORMAL_VALIDATION_SEEDS_V1[index]);
        let label = format!("unit[{index}]");
        writer.u64_v1(&format!("{label}.training_seed"), unit.training_seed);
        writer.u64_v1(&format!("{label}.validation_seed"), unit.validation_seed);
        writer.u32_array_v1(
            &format!("{label}.training_counts"),
            &FORMAL_TRAINING_COUNTS_V1[index],
        );
        writer.u32_array_v1(
            &format!("{label}.validation_counts"),
            &FORMAL_VALIDATION_COUNTS_V1[index],
        );
        for (role, artifact) in [
            ("tape", &unit.tape),
            ("full_update", &unit.full_update),
            ("half_update", &unit.half_update),
        ] {
            assert!(exact_lower_hex_v1(&artifact.sha256, 64));
            assert!(artifact.exact_length > 0);
            writer.text_v1(&format!("{label}.{role}.basename"), &artifact.basename);
            writer.u64_v1(&format!("{label}.{role}.byte_len"), artifact.exact_length);
            writer.text_v1(&format!("{label}.{role}.sha256"), &artifact.sha256);
        }
    }
    assert!(exact_lower_hex_v1(&summary.sha256, 64));
    assert!(summary.exact_length > 0);
    writer.text_v1("summary.basename", &summary.basename);
    writer.u64_v1("summary.byte_len", summary.exact_length);
    writer.text_v1("summary.sha256", &summary.sha256);
    writer.text_v1("disposition", result.disposition.name_v1());
    writer
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
struct FormalStagingPublicationV1 {
    outer_parent: crate::durable_publication_v1::ValidatedPublicationParentV1,
    staging_parent: crate::durable_publication_v1::ValidatedPublicationParentV1,
    staging_dir: PathBuf,
    final_dir: PathBuf,
    artifacts: Vec<FormalArtifactIdentityV1>,
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
impl FormalStagingPublicationV1 {
    fn begin_v1() -> Self {
        let requested = PathBuf::from(
            std::env::var("MTG_KERNEL_ACTION_BLOCK_FORMAL_OUTPUT_DIR_V1")
                .expect("an explicit new formal output directory is required"),
        );
        assert!(requested.is_absolute(), "formal output must be absolute");
        let leaf = requested
            .file_name()
            .expect("formal output must have a normal leaf")
            .to_os_string();
        let requested_parent = requested
            .parent()
            .expect("formal output must have an existing parent");
        let outer_parent =
            crate::durable_publication_v1::capture_existing_publication_parent_v1(requested_parent)
                .expect("formal parent must be a validated existing directory");
        let final_dir = crate::durable_publication_v1::child_path_v1(&outer_parent, &leaf)
            .expect("formal output leaf must be one safe child name");
        let mut staging_leaf = leaf.clone();
        staging_leaf.push(".partial");
        let staging_dir =
            crate::durable_publication_v1::child_path_v1(&outer_parent, &staging_leaf)
                .expect("formal staging leaf must be one safe child name");
        require_absent_path_v1(&final_dir, "final formal directory");
        require_absent_path_v1(&staging_dir, "staging formal directory");
        crate::durable_publication_v1::revalidate_parent_v1(&outer_parent)
            .expect("formal parent must remain stable before staging creation");
        std::fs::create_dir(&staging_dir).expect("formal staging directory must be created new");
        let staging_parent =
            crate::durable_publication_v1::capture_existing_publication_parent_v1(&staging_dir)
                .expect("new formal staging directory must validate");
        Self {
            outer_parent,
            staging_parent,
            staging_dir,
            final_dir,
            artifacts: Vec::with_capacity(20),
        }
    }

    fn publish_frame_v1(
        &mut self,
        basename: String,
        frame: FramedWriterV1,
    ) -> FormalArtifactIdentityV1 {
        assert!(
            !self.artifacts.iter().any(|item| item.basename == basename),
            "formal artifact basename must be unique"
        );
        let exact_length = frame.buffer.len() as u64;
        let sha256 = frame.sha256_v1();
        let expectation =
            crate::durable_publication_v1::DurableFileExpectationV1::from_bytes(&frame.buffer)
                .expect("formal artifact expectation must construct");
        let stage_name = format!("{basename}.publish-stage");
        let receipt = crate::durable_move_publication_v2::publish_immutable_file_by_move_v2(
            &self.staging_parent,
            stage_name,
            &basename,
            &frame.buffer,
            expectation,
        )
        .unwrap_or_else(|_| panic!("durable formal publication failed: {basename}"));
        assert_eq!(receipt.exact_length(), exact_length);
        assert_eq!(lower_hex_raw32_v1(receipt.sha256()), sha256);
        let identity = FormalArtifactIdentityV1 {
            basename,
            exact_length,
            sha256,
        };
        self.artifacts.push(identity.clone());
        identity
    }

    fn verify_inventory_v1(
        parent: &crate::durable_publication_v1::ValidatedPublicationParentV1,
        expected: &[FormalArtifactIdentityV1],
    ) {
        let mut observed = std::fs::read_dir(parent.canonical_path())
            .expect("formal artifact directory inventory must read")
            .map(|entry| {
                let entry = entry.expect("formal artifact directory entry must read");
                assert!(
                    entry
                        .file_type()
                        .expect("formal artifact type must read")
                        .is_file(),
                    "formal artifact inventory may contain only regular files"
                );
                entry
                    .file_name()
                    .to_str()
                    .expect("formal artifact basename must be UTF-8")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        observed.sort();
        let mut expected_names = expected
            .iter()
            .map(|item| item.basename.clone())
            .collect::<Vec<_>>();
        expected_names.sort();
        assert_eq!(
            observed, expected_names,
            "formal artifact inventory must be exact"
        );
        for artifact in expected {
            let path = crate::durable_publication_v1::child_path_v1(
                parent,
                std::ffi::OsStr::new(&artifact.basename),
            )
            .expect("formal artifact basename must remain safe");
            let metadata =
                std::fs::symlink_metadata(&path).expect("formal artifact metadata must reread");
            assert!(metadata.file_type().is_file());
            assert!(!metadata.file_type().is_symlink());
            let mut file = std::fs::File::open(&path).expect("formal artifact must reopen");
            let recaptured = identity_from_open_file_v1(&mut file);
            assert_eq!(recaptured.exact_length, artifact.exact_length);
            assert_eq!(recaptured.sha256, artifact.sha256);
        }
    }

    fn finish_v1(self) -> (PathBuf, Vec<FormalArtifactIdentityV1>) {
        assert_eq!(
            self.artifacts.len(),
            20,
            "formal inventory must contain 20 files"
        );
        Self::verify_inventory_v1(&self.staging_parent, &self.artifacts);
        crate::durable_publication_v1::revalidate_parent_v1(&self.outer_parent)
            .expect("formal parent must remain stable before directory publication");
        require_absent_path_v1(&self.final_dir, "final formal directory");
        move_preflight_directory_write_through_v1(&self.staging_dir, &self.final_dir)
            .expect("complete formal directory must publish without replacement");
        crate::durable_publication_v1::revalidate_parent_v1(&self.outer_parent)
            .expect("formal parent must remain stable after directory publication");
        require_absent_path_v1(&self.staging_dir, "published formal staging directory");
        let final_parent =
            crate::durable_publication_v1::capture_existing_publication_parent_v1(&self.final_dir)
                .expect("published formal directory must validate");
        Self::verify_inventory_v1(&final_parent, &self.artifacts);
        (final_parent.canonical_path().to_path_buf(), self.artifacts)
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
#[ignore = "authorized single-shot six-unit formal HALF measurement; native Windows/MSVC, dedicated process, and explicit invocation required"]
// Runtime platform guard for an explicitly-invoked live test: must still
// panic if run on the wrong platform rather than fail to compile there.
#[allow(clippy::assertions_on_constants)]
fn action_block_gradient_formal_six_unit_gpu1_v1() {
    assert!(
        cfg!(all(
            target_arch = "x86_64",
            target_os = "windows",
            target_env = "msvc"
        )),
        "the formal measurement is native Windows/MSVC only"
    );
    let provenance = PreflightProvenanceGuardV1::begin_v1();
    let gpu = require_fresh_physical_gpu1_v1(FORMAL_LIVE_TEST_NAME_SUFFIX_V1);
    let exclusivity = BoundedGpu1ExclusivityMonitorV1::start_v1();
    let authorities = load_preflight_live_authorities_v1();
    let mut publication = FormalStagingPublicationV1::begin_v1();
    let mut summary_inputs = Vec::with_capacity(BOOTSTRAP_UNIT_COUNT_V1);
    let mut published_units = Vec::with_capacity(BOOTSTRAP_UNIT_COUNT_V1);

    for unit_index in 0..BOOTSTRAP_UNIT_COUNT_V1 {
        let artifacts = run_formal_unit_v1(unit_index, &gpu, &authorities);
        assert_eq!(artifacts.unit_index, unit_index);
        let tape = publication.publish_frame_v1(
            format!("unit-{:02}-tape.frame", unit_index + 1),
            artifacts.tape_frame,
        );
        let full_update = publication.publish_frame_v1(
            format!("unit-{:02}-full-update.frame", unit_index + 1),
            artifacts.full_update_frame,
        );
        let half_update = publication.publish_frame_v1(
            format!("unit-{:02}-half-update.frame", unit_index + 1),
            artifacts.half_update_frame,
        );
        assert_eq!(tape.sha256, artifacts.summary_input.tape_sha256);
        assert_eq!(
            full_update.sha256,
            artifacts.summary_input.full_update_sha256
        );
        assert_eq!(
            half_update.sha256,
            artifacts.summary_input.half_update_sha256
        );
        published_units.push(FormalPublishedUnitV1 {
            unit_index,
            training_seed: artifacts.summary_input.training_seed,
            validation_seed: artifacts.summary_input.validation_seed,
            tape,
            full_update,
            half_update,
        });
        summary_inputs.push(artifacts.summary_input);
    }

    let (paired_differences, promoted2_improvements) = derive_classifier_inputs_v1(&summary_inputs)
        .expect("six ordered formal units must derive classifier inputs");
    let result = classify_v1(&paired_differences, &promoted2_improvements, true);
    assert!(verify_classifier_result_v1(&result));
    let summary_frame = frame_summary_v1(&summary_inputs, &result)
        .expect("formal summary must bind its authoritative unit inputs");
    let summary = publication.publish_frame_v1("summary.frame".to_owned(), summary_frame);

    exclusivity.finish_v1(&gpu);
    let provenance = provenance.finish_v1();
    let manifest_frame =
        frame_formal_manifest_v1(&provenance, &gpu, &published_units, &summary, &result);
    let manifest = publication.publish_frame_v1("manifest.frame".to_owned(), manifest_frame);
    let (final_dir, inventory) = publication.finish_v1();
    assert_eq!(inventory.len(), 20);
    eprintln!(
        "formal HALF result: disposition={} positive_d={}/6 d_low_bits={:016x} positive_p2={}/6 p2_low_bits={:016x} summary={} manifest={} dir={}",
        result.disposition.name_v1(),
        result.positive_paired_count,
        result
            .paired_read
            .expect("finite formal paired bootstrap")
            .low_index_value
            .to_bits(),
        result.positive_promoted2_count,
        result
            .promoted2_read
            .expect("finite formal promoted2 bootstrap")
            .low_index_value
            .to_bits(),
        summary.sha256,
        manifest.sha256,
        final_dir.display(),
    );
    drop(gpu);
}

// ---------------------------------------------------------------------------
// Focused CPU tests. No GPU, no feature flag, no Store, no episode.
// ---------------------------------------------------------------------------

/// The design's full formal count table, `(training, validation)` per unit.
const FORMAL_COUNT_GOLDENS_V1: [([u32; 4], [u32; 4]); BOOTSTRAP_UNIT_COUNT_V1] = [
    ([25, 12, 19, 8], [27, 13, 13, 11]),
    ([20, 19, 11, 14], [30, 12, 11, 11]),
    ([39, 8, 7, 10], [25, 12, 11, 16]),
    ([28, 11, 12, 13], [28, 14, 10, 12]),
    ([18, 12, 13, 21], [24, 13, 15, 12]),
    ([25, 14, 17, 8], [18, 12, 14, 20]),
];

type FixtureSubstepV1 =
    FlatLearnerSubstepSampleCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>;
type FixtureGroupV1 =
    FlatPhysicalDecisionSampleCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>;
type FixtureEpisodeV1 = FlatGroupedEpisodeCore<FlatDecisionBindingV2, FlatOwnedScoringInputsV2>;

struct JoinFixtureV1 {
    authority: PreflightSeed949999AuthorityV1,
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    observed: ObservedRolloutV1,
    retained: Vec<RetainedScoredEntryV1>,
}

fn fixture_binding_v1(
    expected: FastActorDecisionV1,
    candidate_order_commitment: [u8; 16],
) -> FlatDecisionBindingV2 {
    let contract = expected_scorer_contract(KERNEL_CARDDB_HASH);
    FlatDecisionBindingV2 {
        action_binding: FlatActionDecisionBindingV2 {
            slice_version: contract.action_slice_version,
            ref_role_mapping_version: contract.action_ref_role_mapping_version,
            card_token_mapping_version: contract.card_token_mapping_version,
            candidate_commitment_version: contract.candidate_commitment_version,
            card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: expected.episode_id,
            environment_revision: expected.environment_revision,
            bound_policy_step_count: expected.step,
            physical_decision_id: expected.physical_decision_id,
            bound_physical_decision_count: expected.physical_decision_id,
            substep_index: expected.substep_index,
            substep_count: expected.substep_count,
            acting_player: player_seat_code(expected.acting_player),
            decision_kind: decision_kind_code(expected.decision_kind),
            legal_action_count: expected.legal_action_count,
            candidate_order_commitment,
        },
        typed_layout_version: contract.typed_layout_version,
        feature_inventory_version: contract.feature_inventory_version,
        enum_mapping_version: contract.enum_mapping_version,
        object_group_mapping_version: contract.object_group_mapping_version,
        relation_role_mapping_version: contract.relation_role_mapping_version,
        context_subrole_mapping_version: contract.context_subrole_mapping_version,
        action_ref_projection_role_mapping_version: contract
            .action_ref_projection_role_mapping_version,
        contract_digests: contract.contract_digests,
    }
}

fn fixture_owned_inputs_v1(actions: &[FlatScorerActionCoreV2]) -> FlatOwnedScoringInputsV2 {
    FlatOwnedScoringInputsV2 {
        globals: FlatGlobalsV2::default(),
        objects: Vec::new(),
        relations: Vec::new(),
        object_subtypes: Vec::new(),
        ability_uses: Vec::new(),
        goads: Vec::new(),
        completed_dungeons: Vec::new(),
        effect_subtype_changes: Vec::new(),
        context_path_elements: Vec::new(),
        actions: actions.to_vec(),
        action_refs: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn raw_environment_receipt_v1(
    episode_id: u64,
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    inner: [u8; 32],
    learner_policy_step_count: u64,
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_physical_decision_count: u64,
    outer_override: Option<[u8; 32]>,
) -> NativeTrainingTrajectoryReceiptV2 {
    let schedule = native_trainer_episode_schedule_v1(PREFLIGHT_BASE_SEED_V1, episode_id).unwrap();
    let validated = validate_start_v2(&NativeFullEpisodeTrajectoryStartV2 {
        episode_index: episode_id,
        pair_environment_seed: schedule.environment_seed,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        learner_seat: schedule.learner_seat,
    })
    .unwrap();
    let outer = outer_override
        .unwrap_or_else(|| independent_envelope_sha256_for_test_v2(&validated, inner));
    NativeTrainingTrajectoryReceiptV2::from_environment_randomization_v2(
        NativeFullEpisodeTrajectoryReceiptV2 {
            episode_index: validated.episode_index,
            pair_index: validated.pair_index,
            pair_environment_seed: validated.pair_environment_seed,
            deck_ids: validated.deck_ids,
            deck_hashes: validated.deck_hashes,
            learner_seat: validated.learner_seat,
            inner_trajectory_sha256: inner,
            trajectory_sha256_v2: outer,
            policy_step_count: learner_policy_step_count + opponent_policy_step_count,
            physical_decision_count: learner_physical_decision_count
                + opponent_physical_decision_count,
            learner_policy_step_count,
            opponent_policy_step_count,
            learner_physical_decision_count,
            opponent_physical_decision_count,
        },
    )
}

fn join_fixture_v1() -> JoinFixtureV1 {
    let authority = PreflightSeed949999AuthorityV1::seal_v1();
    let rally = runtime_deck_by_id(PREFLIGHT_DECK_ID_V1).unwrap();
    let deck_ids = [rally.id.to_owned(), rally.id.to_owned()];
    let deck_hashes = [rally.runtime_deck_hash; 2];
    let (baseline, actions) = synthetic_action_tensor_v1(&[
        (FlatScorerActionKindV2::ChooseEffectBoolean, 0),
        (
            FlatScorerActionKindV2::ChooseAttackerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
        (FlatScorerActionKindV2::Pass, 0),
    ]);
    let logits = vec![-0.25f32, 0.0f32, 0.25f32];

    let mut first_episode_per_stratum: [Option<u64>; 4] = [None; 4];
    for episode_id in 0..EPISODES_PER_TAPE_V1 {
        let member = ladder_pool_member_for_episode_v1(authority.seed_v1(), episode_id).unwrap();
        let stratum = stratum_ordinal_v1(member) as usize;
        first_episode_per_stratum[stratum].get_or_insert(episode_id);
    }

    let mut episodes: Vec<FixtureEpisodeV1> = Vec::new();
    let mut receipts = Vec::new();
    let mut retained = Vec::new();
    let mut sampler = FastCategoricalScratch::default();
    for episode_id in 0..EPISODES_PER_TAPE_V1 {
        let schedule = native_trainer_episode_schedule_v1(authority.seed_v1(), episode_id).unwrap();
        let member = ladder_pool_member_for_episode_v1(authority.seed_v1(), episode_id).unwrap();
        let stratum = stratum_ordinal_v1(member) as usize;
        let active = first_episode_per_stratum[stratum] == Some(episode_id);
        let receipt = if active {
            envelope_probe_receipt_for_test_v2(
                episode_id,
                schedule.environment_seed,
                &deck_ids,
                deck_hashes,
            )
        } else {
            zero_learner_envelope_probe_receipt_for_test_v2(
                episode_id,
                schedule.environment_seed,
                &deck_ids,
                deck_hashes,
            )
        };
        let learner_return = match schedule.learner_seat {
            PlayerSeatV1::P0 => 1,
            PlayerSeatV1::P1 => -1,
        };
        let mut groups = Vec::new();
        if active {
            let mut substeps: Vec<FixtureSubstepV1> = Vec::new();
            let mut joint_log_probability = 0.0f32;
            let mut first_value_bits = None;
            for substep_index in 0..2u32 {
                let expected = FastActorDecisionV1 {
                    episode_id,
                    step: u64::from(substep_index),
                    environment_revision: 0,
                    physical_decision_id: 0,
                    substep_index,
                    substep_count: 2,
                    acting_player: schedule.learner_seat,
                    decision_kind: FastActorDecisionKindV1::Surface,
                    legal_action_count: actions.len() as u32,
                };
                let mut commitment = [0u8; 16];
                commitment[..8].copy_from_slice(&episode_id.to_le_bytes());
                commitment[8..12].copy_from_slice(&substep_index.to_le_bytes());
                let binding = fixture_binding_v1(expected, commitment);
                let action_seed = derive_native_trainer_learner_action_seed_v1(
                    authority.seed_v1(),
                    episode_id,
                    0,
                    substep_index,
                )
                .unwrap();
                let selected_index = sampler.sample(&logits, action_seed).unwrap();
                let (selected_log_probability, _row) =
                    selected_log_softmax(&logits, selected_index).unwrap();
                joint_log_probability += selected_log_probability;
                let value = 0.125f32 + substep_index as f32 * 0.0625f32;
                first_value_bits.get_or_insert(value.to_bits());
                substeps.push(FixtureSubstepV1 {
                    expected,
                    binding,
                    learner_ordinal: u64::from(substep_index),
                    action_seed,
                    selected_index: selected_index as u32,
                    raw_action_logit_bits: logits.iter().map(|value| value.to_bits()).collect(),
                    selected_log_probability_bits: selected_log_probability.to_bits(),
                    predicted_value_bits: value.to_bits(),
                    scoring_inputs: fixture_owned_inputs_v1(&actions),
                });
                let lineage = RetainedTreatmentLineageV1::from_pre_repair_v1(
                    baseline.clone(),
                    actions.clone(),
                )
                .unwrap();
                retained.push(RetainedScoredEntryV1 {
                    binding,
                    lineage,
                    initial_logits: logits.clone(),
                    initial_value: value,
                });
            }
            groups.push(FixtureGroupV1 {
                episode_id,
                physical_decision_id: 0,
                acting_player: schedule.learner_seat,
                first_learner_ordinal: 0,
                substep_count: 2,
                joint_selected_log_probability_bits: joint_log_probability.to_bits(),
                value_bits: first_value_bits.unwrap(),
                substeps,
            });
        }
        episodes.push(FixtureEpisodeV1 {
            episode_id,
            learner_seat: schedule.learner_seat,
            learner_return,
            learner_policy_step_count: receipt.learner_policy_step_count(),
            opponent_policy_step_count: receipt.opponent_policy_step_count(),
            learner_physical_decision_count: receipt.learner_physical_decision_count(),
            opponent_physical_decision_count: receipt.opponent_physical_decision_count(),
            learner_trace_hash: 0xa11c_e000_0000_0000u64 ^ episode_id,
            terminal: AsyncRolloutTerminalV1 {
                episode_id,
                terminal_outcome: TerminalOutcomeV1::P0Win,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner: Some(PlayerSeatV1::P0),
                terminal_reward: [1, -1],
                policy_step_count: receipt.policy_step_count(),
                physical_decision_count: receipt.physical_decision_count(),
            },
            groups,
        });
        receipts.push(receipt);
    }
    receipts.reverse();
    JoinFixtureV1 {
        authority,
        deck_ids,
        deck_hashes,
        observed: ObservedRolloutV1 {
            batch: FlatGroupedTrajectoryBatchCore {
                learner_seat_rule: FlatPhysicalLearnerSeatRuleCore::EpisodeParity,
                first_episode_id: 0,
                episode_count: EPISODES_PER_TAPE_V1,
                learner_policy_step_count: retained.len() as u64,
                learner_physical_decision_count: 4,
                update_staging: FlatPhysicalUpdateStagingCore::Ready {
                    learner_group_count: 4,
                },
                episodes,
            },
            receipts,
        },
        retained,
    }
}

fn joined_fixture_v1() -> (
    PreflightSeed949999AuthorityV1,
    SessionDeckIdsV1,
    SessionDeckHashesV1,
    JoinedTapeV1,
) {
    let fixture = join_fixture_v1();
    let tape = join_rollout_v1(
        &fixture.authority,
        &fixture.deck_ids,
        fixture.deck_hashes,
        fixture.observed,
        fixture.retained,
    )
    .unwrap();
    (
        fixture.authority,
        fixture.deck_ids,
        fixture.deck_hashes,
        tape,
    )
}

fn assert_join_error_v1(result: Result<JoinedTapeV1, JoinErrorV1>, expected: JoinErrorV1) {
    match result {
        Err(actual) => assert_eq!(actual, expected),
        Ok(_) => panic!("join unexpectedly succeeded; expected {expected:?}"),
    }
}

#[test]
fn natural_terminal_diagonal_authenticates_return_exactly_v1() {
    let diagonals = [
        (TerminalOutcomeV1::P0Win, Some(PlayerSeatV1::P0), [1, -1]),
        (TerminalOutcomeV1::P1Win, Some(PlayerSeatV1::P1), [-1, 1]),
        (TerminalOutcomeV1::Draw, None, [0, 0]),
    ];
    for learner_seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        for (outcome, winner, reward) in diagonals {
            let expected_return = match (learner_seat, winner) {
                (_, None) => 0,
                (learner, Some(winner)) if learner == winner => 1,
                _ => -1,
            };
            let terminal = AsyncRolloutTerminalV1 {
                episode_id: 7,
                terminal_outcome: outcome,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner,
                terminal_reward: reward,
                policy_step_count: 11,
                physical_decision_count: 5,
            };
            assert_eq!(
                validate_natural_terminal_v1(7, learner_seat, expected_return, terminal),
                Ok(expected_return as i8)
            );
        }
    }
}

#[test]
fn natural_terminal_diagonal_rejects_every_independent_fact_mutation_v1() {
    let base = AsyncRolloutTerminalV1 {
        episode_id: 7,
        terminal_outcome: TerminalOutcomeV1::P0Win,
        terminal_classification: TerminalClassificationV1::Natural,
        terminal_code: TerminalSafeCodeV2::NaturalGameOver,
        winner: Some(PlayerSeatV1::P0),
        terminal_reward: [1, -1],
        policy_step_count: 11,
        physical_decision_count: 5,
    };
    let reject = |terminal, learner_return| {
        assert_eq!(
            validate_natural_terminal_v1(7, PlayerSeatV1::P0, learner_return, terminal),
            Err(JoinErrorV1::TerminalFactMismatch)
        );
    };
    reject(base, 0);
    reject(base, 2);

    let mut changed = base;
    changed.episode_id ^= 1;
    reject(changed, 1);
    for classification in [
        TerminalClassificationV1::Truncated,
        TerminalClassificationV1::Halted,
    ] {
        let mut changed = base;
        changed.terminal_classification = classification;
        reject(changed, 1);
    }
    for code in [
        TerminalSafeCodeV2::DecisionCap,
        TerminalSafeCodeV2::FailClosed,
    ] {
        let mut changed = base;
        changed.terminal_code = code;
        reject(changed, 1);
    }
    for outcome in [
        TerminalOutcomeV1::P1Win,
        TerminalOutcomeV1::Draw,
        TerminalOutcomeV1::Truncated,
        TerminalOutcomeV1::Halted,
    ] {
        let mut changed = base;
        changed.terminal_outcome = outcome;
        reject(changed, 1);
    }
    for winner in [Some(PlayerSeatV1::P1), None] {
        let mut changed = base;
        changed.winner = winner;
        reject(changed, 1);
    }
    for reward_index in 0..2 {
        let mut changed = base;
        changed.terminal_reward[reward_index] ^= 1;
        reject(changed, 1);
    }
}

#[test]
fn preflight_receipt_exact_diagonal_and_mutations_v1() {
    let authority = PreflightSeed949999AuthorityV1::seal_v1();
    let rally = runtime_deck_by_id(PREFLIGHT_DECK_ID_V1).unwrap();
    let deck_ids = [rally.id.to_owned(), rally.id.to_owned()];
    let deck_hashes = [rally.runtime_deck_hash; 2];
    let schedule = native_trainer_episode_schedule_v1(authority.seed_v1(), 0).unwrap();
    let expected = ReceiptExpectedFactsV1 {
        episode_id: 0,
        learner_seat: schedule.learner_seat,
        policy_step_count: 3,
        physical_decision_count: 2,
        learner_policy_step_count: 2,
        opponent_policy_step_count: 1,
        learner_physical_decision_count: 1,
        opponent_physical_decision_count: 1,
    };
    let genuine =
        || envelope_probe_receipt_for_test_v2(0, schedule.environment_seed, &deck_ids, deck_hashes);
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &genuine(),),
        Ok(())
    );

    let schedule_mutations = [
        NativeV2ReceiptFactMutationForTestV2::PairIndex,
        NativeV2ReceiptFactMutationForTestV2::EpisodeIndex,
        NativeV2ReceiptFactMutationForTestV2::PairRoot,
        NativeV2ReceiptFactMutationForTestV2::LearnerSeat,
    ];
    for mutation in schedule_mutations {
        let mut receipt = genuine();
        receipt.mutate_environment_fact_for_test_v2(mutation);
        assert_eq!(
            validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &receipt,),
            Err(JoinErrorV1::ReceiptScheduleMismatch),
            "schedule mutation {mutation:?}"
        );
    }
    let deck_mutations = [
        NativeV2ReceiptFactMutationForTestV2::DeckId0,
        NativeV2ReceiptFactMutationForTestV2::DeckId1,
        NativeV2ReceiptFactMutationForTestV2::DeckHash0,
        NativeV2ReceiptFactMutationForTestV2::DeckHash1,
    ];
    for mutation in deck_mutations {
        let mut receipt = genuine();
        receipt.mutate_environment_fact_for_test_v2(mutation);
        assert_eq!(
            validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &receipt,),
            Err(JoinErrorV1::ReceiptDeckMismatch),
            "deck mutation {mutation:?}"
        );
    }
    let count_mutations = [
        NativeV2ReceiptFactMutationForTestV2::PolicyStepCount,
        NativeV2ReceiptFactMutationForTestV2::PhysicalDecisionCount,
        NativeV2ReceiptFactMutationForTestV2::LearnerPolicyStepCount,
        NativeV2ReceiptFactMutationForTestV2::LearnerPhysicalDecisionCount,
        NativeV2ReceiptFactMutationForTestV2::OpponentPolicyStepCount,
        NativeV2ReceiptFactMutationForTestV2::OpponentPhysicalDecisionCount,
    ];
    for mutation in count_mutations {
        let mut receipt = genuine();
        receipt.mutate_environment_fact_for_test_v2(mutation);
        assert_eq!(
            validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &receipt,),
            Err(JoinErrorV1::ReceiptFactMismatch),
            "count mutation {mutation:?}"
        );
    }

    let legacy = genuine().variant_flipped_preserving_commons_for_test_v2(["Rally", "Rally"]);
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &legacy,),
        Err(JoinErrorV1::ReceiptScheduleMismatch)
    );
    let burn = runtime_deck_by_id("Burn").unwrap();
    let wrong_ids = [burn.id.to_owned(), burn.id.to_owned()];
    assert_eq!(
        validate_preflight_receipt_v1(
            &authority,
            &wrong_ids,
            [burn.runtime_deck_hash; 2],
            expected,
            &genuine(),
        ),
        Err(JoinErrorV1::ReceiptDeckMismatch)
    );
    assert_eq!(
        validate_preflight_receipt_v1(
            &authority,
            &deck_ids,
            [rally.runtime_deck_hash ^ 1; 2],
            expected,
            &genuine(),
        ),
        Err(JoinErrorV1::ReceiptStartInvalid)
    );
    let mut wrong_seat = expected;
    wrong_seat.learner_seat = match expected.learner_seat {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    };
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, wrong_seat, &genuine(),),
        Err(JoinErrorV1::ReceiptScheduleMismatch)
    );
    let mut overflow = expected;
    overflow.policy_step_count = 0;
    overflow.learner_policy_step_count = u64::MAX;
    overflow.opponent_policy_step_count = 1;
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, overflow, &genuine(),),
        Err(JoinErrorV1::ReceiptFactMismatch)
    );
}

#[test]
fn preflight_receipt_recomputes_outer_over_exact_inner_v1() {
    let authority = PreflightSeed949999AuthorityV1::seal_v1();
    let rally = runtime_deck_by_id(PREFLIGHT_DECK_ID_V1).unwrap();
    let deck_ids = [rally.id.to_owned(), rally.id.to_owned()];
    let deck_hashes = [rally.runtime_deck_hash; 2];
    let schedule = native_trainer_episode_schedule_v1(authority.seed_v1(), 0).unwrap();
    let expected = ReceiptExpectedFactsV1 {
        episode_id: 0,
        learner_seat: schedule.learner_seat,
        policy_step_count: 3,
        physical_decision_count: 2,
        learner_policy_step_count: 2,
        opponent_policy_step_count: 1,
        learner_physical_decision_count: 1,
        opponent_physical_decision_count: 1,
    };
    let zero_inner =
        raw_environment_receipt_v1(0, &deck_ids, deck_hashes, [0; 32], 2, 1, 1, 1, None);
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &zero_inner,),
        Ok(()),
        "the gate is exact cryptographic reconstruction, not a nonzero rule"
    );

    let coherent =
        raw_environment_receipt_v1(0, &deck_ids, deck_hashes, [0x3c; 32], 2, 1, 1, 1, None);
    let coherent_outer = coherent.outer_trajectory_sha256_v2().unwrap();
    let wrong_inner = raw_environment_receipt_v1(
        0,
        &deck_ids,
        deck_hashes,
        [0x3d; 32],
        2,
        1,
        1,
        1,
        Some(coherent_outer),
    );
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &wrong_inner,),
        Err(JoinErrorV1::ReceiptCommitmentMismatch)
    );
    let mut wrong_outer = coherent_outer;
    wrong_outer[0] ^= 1;
    let wrong_outer = raw_environment_receipt_v1(
        0,
        &deck_ids,
        deck_hashes,
        [0x3c; 32],
        2,
        1,
        1,
        1,
        Some(wrong_outer),
    );
    assert_eq!(
        validate_preflight_receipt_v1(&authority, &deck_ids, deck_hashes, expected, &wrong_outer,),
        Err(JoinErrorV1::ReceiptCommitmentMismatch)
    );
}

#[test]
fn join_binding_revalidates_contract_and_every_coordinate_v1() {
    let expected = FastActorDecisionV1 {
        episode_id: 7,
        step: 11,
        environment_revision: 13,
        physical_decision_id: 17,
        substep_index: 1,
        substep_count: 3,
        acting_player: PlayerSeatV1::P1,
        decision_kind: FastActorDecisionKindV1::Surface,
        legal_action_count: 5,
    };
    let binding = fixture_binding_v1(expected, [0; 16]);
    assert_eq!(
        validate_join_binding_v1(
            binding,
            binding,
            expected,
            expected.episode_id,
            expected.physical_decision_id,
            expected.substep_count,
            expected.acting_player,
            expected.legal_action_count as usize,
        ),
        Ok(()),
        "an exact zero candidate commitment is valid transport evidence"
    );

    let mut unequal = binding;
    unequal.action_binding.candidate_order_commitment[0] ^= 1;
    assert_eq!(
        validate_join_binding_v1(
            unequal,
            binding,
            expected,
            expected.episode_id,
            expected.physical_decision_id,
            expected.substep_count,
            expected.acting_player,
            expected.legal_action_count as usize,
        ),
        Err(JoinErrorV1::BindingFactMismatch)
    );

    let mutations: &[fn(&mut FlatDecisionBindingV2)] = &[
        |value| value.action_binding.slice_version ^= 1,
        |value| value.action_binding.ref_role_mapping_version ^= 1,
        |value| value.action_binding.card_token_mapping_version ^= 1,
        |value| value.action_binding.candidate_commitment_version ^= 1,
        |value| value.action_binding.card_db_hash ^= 1,
        |value| value.typed_layout_version ^= 1,
        |value| value.feature_inventory_version ^= 1,
        |value| value.enum_mapping_version ^= 1,
        |value| value.object_group_mapping_version ^= 1,
        |value| value.relation_role_mapping_version ^= 1,
        |value| value.context_subrole_mapping_version ^= 1,
        |value| value.action_ref_projection_role_mapping_version ^= 1,
        |value| value.contract_digests.mapping_sha256[0] ^= 1,
        |value| value.contract_digests.feature_inventory_sha256[0] ^= 1,
        |value| value.contract_digests.base_typed_layout_sha256[0] ^= 1,
        |value| value.contract_digests.overlay_typed_layout_sha256[0] ^= 1,
        |value| value.contract_digests.typed_layout_sha256[0] ^= 1,
        |value| value.contract_digests.action_contract_source_sha256[0] ^= 1,
        |value| value.contract_digests.action_contract_sha256[0] ^= 1,
    ];
    for mutate in mutations {
        let mut observed = binding;
        let mut retained = binding;
        mutate(&mut observed);
        mutate(&mut retained);
        assert_eq!(
            validate_join_binding_v1(
                observed,
                retained,
                expected,
                expected.episode_id,
                expected.physical_decision_id,
                expected.substep_count,
                expected.acting_player,
                expected.legal_action_count as usize,
            ),
            Err(JoinErrorV1::BindingFactMismatch)
        );
    }

    let expected_mutations: &[fn(&mut FastActorDecisionV1)] = &[
        |value| value.episode_id ^= 1,
        |value| value.step ^= 1,
        |value| value.environment_revision ^= 1,
        |value| value.physical_decision_id ^= 1,
        |value| value.substep_index ^= 1,
        |value| value.substep_count ^= 1,
        |value| {
            value.acting_player = match value.acting_player {
                PlayerSeatV1::P0 => PlayerSeatV1::P1,
                PlayerSeatV1::P1 => PlayerSeatV1::P0,
            }
        },
        |value| value.legal_action_count ^= 1,
    ];
    for mutate in expected_mutations {
        let mut changed = expected;
        mutate(&mut changed);
        assert_eq!(
            validate_join_binding_v1(
                binding,
                binding,
                changed,
                expected.episode_id,
                expected.physical_decision_id,
                expected.substep_count,
                expected.acting_player,
                expected.legal_action_count as usize,
            ),
            Err(JoinErrorV1::BindingFactMismatch)
        );
    }
}

#[test]
fn join_rollout_positive_and_fail_closed_wiring_v1() {
    let fixture = join_fixture_v1();
    let tape = join_rollout_v1(
        &fixture.authority,
        &fixture.deck_ids,
        fixture.deck_hashes,
        fixture.observed,
        fixture.retained,
    )
    .unwrap();
    assert_eq!(tape.base_seed, PREFLIGHT_BASE_SEED_V1);
    assert_eq!(tape.counts.as_array_v1(), PREFLIGHT_REQUIRED_COUNTS_V1);
    assert_eq!(tape.episodes.len() as u64, EPISODES_PER_TAPE_V1);
    assert_eq!(tape.total_group_count_v1(), 4);
    assert_eq!(tape.total_substep_count_v1(), 8);
    assert_eq!(tape.groups_per_stratum_v1(), [1, 1, 1, 1]);

    // Receipts were deliberately delivered in reverse asynchronous order;
    // successful episode-keyed joining proves arrival order is not authority.
    for (episode_id, episode) in tape.episodes.iter().enumerate() {
        assert_eq!(episode.receipt.episode_index(), episode_id as u64);
    }

    let mut wrong_batch = join_fixture_v1();
    wrong_batch.observed.batch.learner_policy_step_count ^= 1;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_batch.authority,
            &wrong_batch.deck_ids,
            wrong_batch.deck_hashes,
            wrong_batch.observed,
            wrong_batch.retained,
        ),
        JoinErrorV1::BatchFactMismatch,
    );

    let mut wrong_return = join_fixture_v1();
    wrong_return.observed.batch.episodes[0].learner_return = 0;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_return.authority,
            &wrong_return.deck_ids,
            wrong_return.deck_hashes,
            wrong_return.observed,
            wrong_return.retained,
        ),
        JoinErrorV1::TerminalFactMismatch,
    );

    let mut wrong_terminal_tuple = join_fixture_v1();
    wrong_terminal_tuple.observed.batch.episodes[0]
        .terminal
        .terminal_reward[0] = 0;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_terminal_tuple.authority,
            &wrong_terminal_tuple.deck_ids,
            wrong_terminal_tuple.deck_hashes,
            wrong_terminal_tuple.observed,
            wrong_terminal_tuple.retained,
        ),
        JoinErrorV1::TerminalFactMismatch,
    );

    let mut duplicate = join_fixture_v1();
    duplicate.retained[1].binding = duplicate.retained[0].binding;
    assert_join_error_v1(
        join_rollout_v1(
            &duplicate.authority,
            &duplicate.deck_ids,
            duplicate.deck_hashes,
            duplicate.observed,
            duplicate.retained,
        ),
        JoinErrorV1::DuplicateRetainedBinding,
    );

    let mut duplicate_grouped = join_fixture_v1();
    let group = duplicate_grouped
        .observed
        .batch
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap();
    let first_binding = group.substeps[0].binding;
    group.substeps[1].binding = first_binding;
    assert_join_error_v1(
        join_rollout_v1(
            &duplicate_grouped.authority,
            &duplicate_grouped.deck_ids,
            duplicate_grouped.deck_hashes,
            duplicate_grouped.observed,
            duplicate_grouped.retained,
        ),
        JoinErrorV1::DuplicateBinding,
    );

    let mut duplicate_receipt = join_fixture_v1();
    duplicate_receipt.observed.receipts[1] = duplicate_receipt.observed.receipts[0];
    assert_join_error_v1(
        join_rollout_v1(
            &duplicate_receipt.authority,
            &duplicate_receipt.deck_ids,
            duplicate_receipt.deck_hashes,
            duplicate_receipt.observed,
            duplicate_receipt.retained,
        ),
        JoinErrorV1::ReceiptEpisodeIdentity,
    );

    let mut missing_receipt = join_fixture_v1();
    missing_receipt.observed.receipts.pop();
    assert_join_error_v1(
        join_rollout_v1(
            &missing_receipt.authority,
            &missing_receipt.deck_ids,
            missing_receipt.deck_hashes,
            missing_receipt.observed,
            missing_receipt.retained,
        ),
        JoinErrorV1::ReceiptCardinality,
    );

    let mut bad_receipt = join_fixture_v1();
    bad_receipt.receipts_mut_v1()[0]
        .mutate_environment_fact_for_test_v2(NativeV2ReceiptFactMutationForTestV2::PairRoot);
    assert_join_error_v1(
        join_rollout_v1(
            &bad_receipt.authority,
            &bad_receipt.deck_ids,
            bad_receipt.deck_hashes,
            bad_receipt.observed,
            bad_receipt.retained,
        ),
        JoinErrorV1::ReceiptScheduleMismatch,
    );

    let mut wrong_actor = join_fixture_v1();
    let group = wrong_actor
        .observed
        .batch
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap();
    group.acting_player = match group.acting_player {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    };
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_actor.authority,
            &wrong_actor.deck_ids,
            wrong_actor.deck_hashes,
            wrong_actor.observed,
            wrong_actor.retained,
        ),
        JoinErrorV1::BindingFactMismatch,
    );

    let mut wrong_summary = join_fixture_v1();
    wrong_summary
        .observed
        .batch
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap()
        .joint_selected_log_probability_bits ^= 1;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_summary.authority,
            &wrong_summary.deck_ids,
            wrong_summary.deck_hashes,
            wrong_summary.observed,
            wrong_summary.retained,
        ),
        JoinErrorV1::GroupSummaryMismatch,
    );

    let mut wrong_ordinal = join_fixture_v1();
    wrong_ordinal
        .observed
        .batch
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap()
        .substeps[1]
        .learner_ordinal = 7;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_ordinal.authority,
            &wrong_ordinal.deck_ids,
            wrong_ordinal.deck_hashes,
            wrong_ordinal.observed,
            wrong_ordinal.retained,
        ),
        JoinErrorV1::OrderViolation,
    );

    let mut wrong_lineage = join_fixture_v1();
    let coordinate = ACTION_HASH_BEGIN_V1;
    wrong_lineage.retained[0]
        .lineage
        .half_tensor
        .action_features[coordinate] += 0.125;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_lineage.authority,
            &wrong_lineage.deck_ids,
            wrong_lineage.deck_hashes,
            wrong_lineage.observed,
            wrong_lineage.retained,
        ),
        JoinErrorV1::TreatmentLineageMismatch,
    );

    let mut wrong_contract = join_fixture_v1();
    let original = wrong_contract
        .observed
        .batch
        .episodes
        .iter()
        .find_map(|episode| episode.groups.first())
        .unwrap()
        .substeps[0]
        .binding;
    let retained = wrong_contract
        .retained
        .iter_mut()
        .find(|entry| entry.binding == original)
        .unwrap();
    retained.binding.typed_layout_version ^= 1;
    wrong_contract
        .observed
        .batch
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap()
        .substeps[0]
        .binding
        .typed_layout_version ^= 1;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_contract.authority,
            &wrong_contract.deck_ids,
            wrong_contract.deck_hashes,
            wrong_contract.observed,
            wrong_contract.retained,
        ),
        JoinErrorV1::BindingFactMismatch,
    );

    let mut wrong_seed = join_fixture_v1();
    let substep = wrong_seed
        .observed
        .batch
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap()
        .substeps
        .first_mut()
        .unwrap();
    let logits: Vec<f32> = substep
        .raw_action_logit_bits
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect();
    let selected = substep.selected_index as usize;
    let mut candidate = substep.action_seed.wrapping_add(1);
    let mut scratch = FastCategoricalScratch::default();
    while scratch.sample(&logits, candidate).unwrap() != selected {
        candidate = candidate.wrapping_add(1);
    }
    substep.action_seed = candidate;
    assert_join_error_v1(
        join_rollout_v1(
            &wrong_seed.authority,
            &wrong_seed.deck_ids,
            wrong_seed.deck_hashes,
            wrong_seed.observed,
            wrong_seed.retained,
        ),
        JoinErrorV1::ActionSeedAuthority,
    );
}

impl JoinFixtureV1 {
    fn receipts_mut_v1(&mut self) -> &mut [NativeTrainingTrajectoryReceiptV2] {
        &mut self.observed.receipts
    }
}

#[test]
fn joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1() {
    let (authority, deck_ids, deck_hashes, tape) = joined_fixture_v1();
    let frame = frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &tape).unwrap();
    // Re-baselined once per the owner ruling on record (collab CLAUDE #236,
    // 2026-08-14): joined_fixture_v1 carries live deck_ids/deck_hashes, so
    // this serializer golden moves with the nine-deck catalog landing.
    assert_eq!(
        frame.sha256_v1(),
        "d6812f9e689c56911b38426ee17eddf00d400e94ee3ff8aa3ebf8d2310e00970",
        "the complete compact joined-body fixture is a frozen serializer golden"
    );
    assert!(frame
        .bytes_v1()
        .windows("neutral".len())
        .any(|window| window == b"neutral"));
    assert!(!frame
        .bytes_v1()
        .windows("validation".len())
        .any(|window| window == b"validation"));

    let (_, _, _, mut wrong_seed) = joined_fixture_v1();
    wrong_seed.base_seed ^= 1;
    assert_eq!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &wrong_seed),
        Err(TapeFramingErrorV1::CountsDisagreeWithSchedule)
    );

    let burn = runtime_deck_by_id("Burn").unwrap();
    let burn_ids = [burn.id.to_owned(), burn.id.to_owned()];
    assert_eq!(
        frame_joined_tape_v1(&authority, &burn_ids, [burn.runtime_deck_hash; 2], &tape),
        Err(TapeFramingErrorV1::MalformedReceipt)
    );

    let (_, _, _, mut wrong_lineage) = joined_fixture_v1();
    let retained = &mut wrong_lineage
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap()
        .substeps[0]
        .retained;
    retained.lineage.half_tensor.action_features[ACTION_HASH_BEGIN_V1] += 0.125;
    assert_eq!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &wrong_lineage),
        Err(TapeFramingErrorV1::OutputMismatch)
    );

    let (_, _, _, mut wrong_return) = joined_fixture_v1();
    wrong_return.episodes[0]
        .authenticated_terminal
        .learner_return = 0;
    assert_eq!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &wrong_return),
        Err(TapeFramingErrorV1::CountsDisagreeWithSchedule)
    );

    let (_, _, _, mut wrong_terminal) = joined_fixture_v1();
    wrong_terminal.episodes[0]
        .authenticated_terminal
        .terminal
        .winner = Some(PlayerSeatV1::P1);
    assert_eq!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &wrong_terminal),
        Err(TapeFramingErrorV1::CountsDisagreeWithSchedule)
    );

    let (_, _, _, mut wrong_actor) = joined_fixture_v1();
    let group = wrong_actor
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap();
    let actor = match group.substeps[0].expected.acting_player {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    };
    for substep in &mut group.substeps {
        substep.expected.acting_player = actor;
        substep.binding.action_binding.acting_player = player_seat_code(actor);
        substep.retained.binding.action_binding.acting_player = player_seat_code(actor);
    }
    assert_eq!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &wrong_actor),
        Err(TapeFramingErrorV1::BindingMismatch)
    );

    // Preflight is a single non-held-out training tape. Removing the only
    // learner group from one stratum remains valid when the receipt is
    // coherently replaced with its zero-learner sibling; the formal
    // held-out all-strata group gate must not leak into this serializer.
    let (_, _, _, mut one_empty_stratum) = joined_fixture_v1();
    let episode = one_empty_stratum
        .episodes
        .iter_mut()
        .find(|episode| episode.stratum == 3 && !episode.groups.is_empty())
        .unwrap();
    let schedule =
        native_trainer_episode_schedule_v1(authority.seed_v1(), episode.episode_id).unwrap();
    let receipt = zero_learner_envelope_probe_receipt_for_test_v2(
        episode.episode_id,
        schedule.environment_seed,
        &deck_ids,
        deck_hashes,
    );
    episode.groups.clear();
    episode.receipt_expected_facts = ReceiptExpectedFactsV1 {
        episode_id: episode.episode_id,
        learner_seat: schedule.learner_seat,
        policy_step_count: receipt.policy_step_count(),
        physical_decision_count: receipt.physical_decision_count(),
        learner_policy_step_count: receipt.learner_policy_step_count(),
        opponent_policy_step_count: receipt.opponent_policy_step_count(),
        learner_physical_decision_count: receipt.learner_physical_decision_count(),
        opponent_physical_decision_count: receipt.opponent_physical_decision_count(),
    };
    episode.receipt = receipt;
    assert_eq!(one_empty_stratum.groups_per_stratum_v1(), [1, 1, 1, 0]);
    assert!(frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &one_empty_stratum,).is_ok());
}

#[test]
fn joined_frame_binds_every_mutable_evidence_family_v1() {
    let (authority, deck_ids, deck_hashes, tape) = joined_fixture_v1();
    let baseline = frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &tape)
        .unwrap()
        .sha256_v1();

    let (_, _, _, mut trace_mutation) = joined_fixture_v1();
    trace_mutation.episodes[0].trace_hash ^= 1;
    assert_ne!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &trace_mutation)
            .unwrap()
            .sha256_v1(),
        baseline
    );

    let (_, _, _, mut commitment_mutation) = joined_fixture_v1();
    let substep = &mut commitment_mutation
        .episodes
        .iter_mut()
        .find_map(|episode| episode.groups.first_mut())
        .unwrap()
        .substeps[0];
    substep.binding.action_binding.candidate_order_commitment[0] ^= 1;
    substep
        .retained
        .binding
        .action_binding
        .candidate_order_commitment[0] ^= 1;
    assert_ne!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &commitment_mutation,)
            .unwrap()
            .sha256_v1(),
        baseline
    );

    let (_, _, _, mut receipt_mutation) = joined_fixture_v1();
    let episode = receipt_mutation
        .episodes
        .iter_mut()
        .find(|episode| !episode.groups.is_empty())
        .unwrap();
    episode.receipt = raw_environment_receipt_v1(
        episode.episode_id,
        &deck_ids,
        deck_hashes,
        [0x5a; 32],
        episode.receipt_expected_facts.learner_policy_step_count,
        episode.receipt_expected_facts.opponent_policy_step_count,
        episode
            .receipt_expected_facts
            .learner_physical_decision_count,
        episode
            .receipt_expected_facts
            .opponent_physical_decision_count,
        None,
    );
    assert_ne!(
        frame_joined_tape_v1(&authority, &deck_ids, deck_hashes, &receipt_mutation)
            .unwrap()
            .sha256_v1(),
        baseline
    );
}

#[test]
fn preflight_seed_949999_draws_the_required_pool3_counts_v1() {
    let counts = pool_choice_counts_v1(PREFLIGHT_BASE_SEED_V1, EPISODES_PER_TAPE_V1);
    assert_eq!(
        counts.as_array_v1(),
        PREFLIGHT_REQUIRED_COUNTS_V1,
        "preflight seed 949999 must draw exactly 27/13/13/11 over episodes [0,64)"
    );
    assert_eq!(counts.total_v1(), EPISODES_PER_TAPE_V1 as u32);
}

/// Pure schedule reproduction of the formal count goldens and the private
/// runtime authorities that now admit exactly those twelve tapes.
#[test]
fn formal_seed_pool3_count_goldens_reproduce_exactly_v1() {
    for (unit, (training_golden, validation_golden)) in FORMAL_COUNT_GOLDENS_V1.iter().enumerate() {
        assert_eq!(*training_golden, FORMAL_TRAINING_COUNTS_V1[unit]);
        assert_eq!(*validation_golden, FORMAL_VALIDATION_COUNTS_V1[unit]);
        let training_authority = FormalSeedAuthorityV1::training_v1(unit);
        let validation_authority = FormalSeedAuthorityV1::validation_v1(unit);
        assert_eq!(training_authority.seed_v1(), FORMAL_TRAINING_SEEDS_V1[unit]);
        assert_eq!(
            validation_authority.seed_v1(),
            FORMAL_VALIDATION_SEEDS_V1[unit]
        );
        assert_eq!(training_authority.expected_counts_v1(), *training_golden);
        assert_eq!(
            validation_authority.expected_counts_v1(),
            *validation_golden
        );
        assert!(training_authority.allows_update_v1());
        assert!(!validation_authority.allows_update_v1());
        let training = pool_choice_counts_v1(FORMAL_TRAINING_SEEDS_V1[unit], EPISODES_PER_TAPE_V1);
        let validation =
            pool_choice_counts_v1(FORMAL_VALIDATION_SEEDS_V1[unit], EPISODES_PER_TAPE_V1);
        assert_eq!(
            training.as_array_v1(),
            *training_golden,
            "unit {} training seed {} count mismatch",
            unit + 1,
            FORMAL_TRAINING_SEEDS_V1[unit]
        );
        assert_eq!(
            validation.as_array_v1(),
            *validation_golden,
            "unit {} validation seed {} count mismatch",
            unit + 1,
            FORMAL_VALIDATION_SEEDS_V1[unit]
        );
        assert_eq!(training.total_v1(), EPISODES_PER_TAPE_V1 as u32);
        assert_eq!(validation.total_v1(), EPISODES_PER_TAPE_V1 as u32);
    }
}

/// Every validation stratum must contain at least one learner physical
/// decision group, so no stratum mean is taken over an empty set.
#[test]
fn every_validation_stratum_is_non_empty_v1() {
    for seed in FORMAL_VALIDATION_SEEDS_V1 {
        let counts = pool_choice_counts_v1(seed, EPISODES_PER_TAPE_V1);
        for (index, count) in counts.as_array_v1().iter().enumerate() {
            assert!(
                *count > 0,
                "validation seed {seed} stratum {index} is empty; every stratum needs a group"
            );
        }
    }
    let preflight = pool_choice_counts_v1(PREFLIGHT_BASE_SEED_V1, EPISODES_PER_TAPE_V1);
    assert!(preflight.as_array_v1().iter().all(|count| *count > 0));
}

#[test]
// This runtime-style assert deliberately matches its neighboring
// assert_eq! frozen-bit-pattern checks in this test rather than moving to
// a const block; accepted.
#[allow(clippy::assertions_on_constants)]
fn frozen_scale_and_authority_bits_decode_exactly_v1() {
    assert_eq!(f32::from_bits(HALF_DIGEST_SCALE_BITS_V1), 0.5f32);
    assert_eq!(f32::from_bits(DOUBLE_WEIGHT_SCALE_BITS_V1), 2.0f32);
    assert_eq!(f32::from_bits(VALUE_COEFFICIENT_BITS_V1), 0.5f32);
    assert_eq!(f32::from_bits(ADAM_WEIGHT_DECAY_BITS_V1), 0.0f32);
    assert!(!ADAM_AMSGRAD_V1);
    // The digest block the design names, [99,195), and the transformed
    // weight's row-major shape.
    assert_eq!(ACTION_HASH_BEGIN_V1, 99);
    assert_eq!(ACTION_HASH_END_V1, 195);
    assert_eq!(ACTION_ENCODER_COLUMNS_V1, ACTION_FEATURE_DIM_V1 + 64);
    // Bit-exact learning rate and moment authorities.
    assert_eq!(f32::from_bits(LEARNING_RATE_BITS_V1).to_bits(), 0x3a83_126f);
    assert_eq!(f32::from_bits(ADAM_BETA1_BITS_V1).to_bits(), 0x3f66_6666);
    assert_eq!(f32::from_bits(ADAM_BETA2_BITS_V1).to_bits(), 0x3f7f_be77);
    assert_eq!(f32::from_bits(ADAM_EPSILON_BITS_V1).to_bits(), 0x322b_cc77);
}

#[test]
fn exact_half_and_double_round_trips_hold_on_normal_values_v1() {
    let probes = [
        1.0f32,
        -1.0f32,
        0.0f32,
        33.841_724_f32,
        9.348_08_f32,
        f32::MIN_POSITIVE,
        1.5e-38f32,
        3.4e38f32,
        -7.125f32,
    ];
    for probe in probes {
        if probe != 0.0 && !probe.is_normal() {
            continue;
        }
        // Halving a normal value round-trips unless it lands subnormal.
        let half = f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * probe;
        if probe == 0.0 || half.is_normal() {
            assert_eq!(halve_round_trip_v1(probe), Ok(half));
            assert_eq!(f32::from_bits(DOUBLE_WEIGHT_SCALE_BITS_V1) * half, probe);
        }
        // Doubling round-trips unless it overflows.
        let doubled = f32::from_bits(DOUBLE_WEIGHT_SCALE_BITS_V1) * probe;
        if doubled.is_finite() {
            assert_eq!(double_round_trip_v1(probe), Ok(doubled));
            assert_eq!(f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * doubled, probe);
        }
    }
}

/// Fail-closed proof that the normality gate is load-bearing: halving the
/// smallest normal f32 produces a subnormal, and doubling a near-max f32
/// overflows. Both must be rejected, not silently admitted.
#[test]
fn scaling_gate_rejects_subnormal_and_overflow_and_nonfinite_v1() {
    let subnormal_after_halving = f32::MIN_POSITIVE;
    assert!(!(f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * subnormal_after_halving).is_normal());
    assert_eq!(
        halve_round_trip_v1(subnormal_after_halving),
        Ok(f32::MIN_POSITIVE * 0.5)
    );
    // A genuinely subnormal input is rejected outright.
    let subnormal = f32::from_bits(1);
    assert!(!subnormal.is_normal() && subnormal != 0.0);
    assert_eq!(
        halve_round_trip_v1(subnormal),
        Err(ScalingValidityErrorV1::Subnormal)
    );
    assert_eq!(
        double_round_trip_v1(subnormal),
        Err(ScalingValidityErrorV1::Subnormal)
    );
    // Halving a subnormal loses the low bit, so the round trip fails.
    let odd_subnormal = f32::from_bits(3);
    assert_ne!(
        f32::from_bits(DOUBLE_WEIGHT_SCALE_BITS_V1)
            * (f32::from_bits(HALF_DIGEST_SCALE_BITS_V1) * odd_subnormal),
        odd_subnormal
    );
    // Doubling overflows near the top of the range.
    assert_eq!(
        double_round_trip_v1(f32::MAX),
        Err(ScalingValidityErrorV1::DoubleRoundTrip)
    );
    for nonfinite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            halve_round_trip_v1(nonfinite),
            Err(ScalingValidityErrorV1::NonFinite)
        );
        assert_eq!(
            double_round_trip_v1(nonfinite),
            Err(ScalingValidityErrorV1::NonFinite)
        );
    }
}

/// The heart of the design: the HALF transform is exactly
/// function-preserving at the `action_encoder.0` pre-activation, for every
/// output row, bit for bit.
#[test]
fn half_transform_preserves_the_action_encoder_preactivation_bit_exactly_v1() {
    // Deterministic, reproducible F weights and inputs spanning both the
    // direct block [0,99) and the digest block [99,195), plus the hidden
    // tail [195,259) which the transform must not touch.
    let weight_row: Vec<f32> = (0..ACTION_ENCODER_COLUMNS_V1)
        .map(|column| ((column as f32) - 129.5) / 37.0)
        .collect();
    let input: Vec<f32> = (0..ACTION_ENCODER_COLUMNS_V1)
        .map(|column| ((column as f32) * 0.013).sin() * 1.75)
        .collect();
    let bias = -0.3125f32;

    let mut half_weight_row = weight_row.clone();
    let mut half_input = input.clone();
    for column in ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1 {
        half_weight_row[column] = double_round_trip_v1(weight_row[column]).unwrap();
        half_input[column] = halve_round_trip_v1(input[column]).unwrap();
    }

    let full = action_encoder_preactivation_v1(&weight_row, &input, bias);
    let half = action_encoder_preactivation_v1(&half_weight_row, &half_input, bias);
    assert_eq!(
        half.to_bits(),
        full.to_bits(),
        "HALF must reproduce the FULL pre-activation bit-exactly"
    );

    // Every individual product is preserved too, not just the total.
    for column in ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1 {
        assert_eq!(
            (half_weight_row[column] * half_input[column]).to_bits(),
            (weight_row[column] * input[column]).to_bits()
        );
    }
    // Untouched coordinates are bit-identical.
    for column in (0..ACTION_HASH_BEGIN_V1).chain(ACTION_HASH_END_V1..ACTION_ENCODER_COLUMNS_V1) {
        assert_eq!(
            half_weight_row[column].to_bits(),
            weight_row[column].to_bits()
        );
        assert_eq!(half_input[column].to_bits(), input[column].to_bits());
    }
}

#[test]
fn retained_lineage_relation_is_pure_structural_and_fail_closed_v1() {
    let make_lineage = || {
        let (baseline, actions) = synthetic_action_tensor_v1(&[
            (FlatScorerActionKindV2::ChooseEffectBoolean, 0),
            (
                FlatScorerActionKindV2::ChooseAttackerInclusion,
                FLAT_ACTION_FLAG_INCLUDE_V1,
            ),
            (FlatScorerActionKindV2::ChooseBlockerInclusion, 0),
            (FlatScorerActionKindV2::Pass, 0),
        ]);
        RetainedTreatmentLineageV1::from_pre_repair_v1(baseline, actions).unwrap()
    };

    assert!(make_lineage().relation_valid_v1());

    let mut changed = make_lineage();
    changed.pre_repair_action_features.pop();
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.source_action_cores[0].flags |= FLAT_ACTION_FLAG_INCLUDE_V1;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.pre_repair_action_features[ACTION_FEATURE_DIM_V1 + SLOT69_V1] = -0.0;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.full_tensor.action_features[ACTION_FEATURE_DIM_V1 + SLOT69_V1] = 0.0;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.half_tensor.action_features[SLOT69_V1] = 1.0;
    assert!(!changed.relation_valid_v1());

    let ordinary_column = 31;
    let mut changed = make_lineage();
    changed.full_tensor.action_features[ordinary_column] += 0.25;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.half_tensor.action_features[ordinary_column] += 0.25;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.pre_repair_action_features[ACTION_HASH_BEGIN_V1] += 0.25;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    let digest = &mut changed.half_tensor.action_features[ACTION_HASH_BEGIN_V1];
    *digest = f32::from_bits(digest.to_bits().wrapping_add(1));
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.half_tensor.state[0] += 0.25;
    assert!(!changed.relation_valid_v1());

    let mut changed = make_lineage();
    changed.full_tensor.object_node_ids[0] = 1;
    assert!(!changed.relation_valid_v1());
}

/// The tensor-side treatments come from the single-repair lineage helper.
/// Repair is non-idempotent, so a repaired or HALF tensor can never be
/// repaired again.
#[test]
fn half_tensor_is_the_single_repair_lineage_under_an_exact_half_transform_v1() {
    let (baseline, actions) = synthetic_action_tensor_v1(&[
        (FlatScorerActionKindV2::ChooseEffectBoolean, 0),
        (
            FlatScorerActionKindV2::ChooseEffectBoolean,
            FLAT_ACTION_FLAG_VALUE_V1,
        ),
        (
            FlatScorerActionKindV2::ChooseAttackerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
        (FlatScorerActionKindV2::ChooseBlockerInclusion, 0),
        (FlatScorerActionKindV2::Pass, 0),
    ]);

    // ONE repair for the whole lineage; H is a pure transform of that base.
    let lineage = RetainedTreatmentLineageV1::from_pre_repair_v1(baseline, actions).unwrap();
    assert!(lineage.relation_valid_v1());
    let full = lineage.full_tensor_v1();
    let half = lineage.half_tensor_v1();

    for (full_row, half_row) in full
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .zip(half.action_features.chunks_exact(ACTION_FEATURE_DIM_V1))
    {
        // Direct coordinates [0,99) are untouched by the treatment.
        for column in 0..ACTION_HASH_BEGIN_V1 {
            assert_eq!(half_row[column].to_bits(), full_row[column].to_bits());
        }
        // Digest coordinates [99,195) are exactly halved.
        for column in ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1 {
            assert_eq!(
                half_row[column].to_bits(),
                (full_row[column] * 0.5f32).to_bits()
            );
        }
    }

    // No test invokes repair on FULL or HALF: the read-only relation itself
    // is the evidence, matching the live join/framing boundary exactly.
}

/// The H parameter derivation doubles exactly the digest columns of
/// `action_encoder.0.weight` and nothing else, and is exactly invertible.
#[test]
fn half_parameter_derivation_touches_only_action_encoder_digest_columns_v1() {
    let full = vec![
        NativeNamedParameterV1 {
            name: "action_encoder.0.bias",
            shape: vec![ACTION_ENCODER_ROWS_V1],
            values: (0..ACTION_ENCODER_ROWS_V1)
                .map(|index| index as f32 * 0.5 - 8.0)
                .collect(),
        },
        NativeNamedParameterV1 {
            name: ACTION_ENCODER_WEIGHT_NAME_V1,
            shape: vec![ACTION_ENCODER_ROWS_V1, ACTION_ENCODER_COLUMNS_V1],
            values: (0..ACTION_ENCODER_ROWS_V1 * ACTION_ENCODER_COLUMNS_V1)
                .map(|index| ((index % 511) as f32 - 255.0) / 64.0)
                .collect(),
        },
    ];

    let half = derive_half_parameters_v1(&full).unwrap();
    assert_eq!(half.len(), full.len());

    // The bias tensor is untouched.
    assert_eq!(half[0].values, full[0].values);

    for (full_row, half_row) in full[1]
        .values
        .chunks_exact(ACTION_ENCODER_COLUMNS_V1)
        .zip(half[1].values.chunks_exact(ACTION_ENCODER_COLUMNS_V1))
    {
        for column in 0..ACTION_ENCODER_COLUMNS_V1 {
            let expected = if (ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1).contains(&column) {
                (full_row[column] * 2.0f32).to_bits()
            } else {
                full_row[column].to_bits()
            };
            assert_eq!(half_row[column].to_bits(), expected);
        }
    }

    // Exact inversion recovers F bit for bit.
    let restored = halve_action_encoder_digest_columns_v1(&half).unwrap();
    for (restored_parameter, full_parameter) in restored.iter().zip(&full) {
        assert!(restored_parameter
            .values
            .iter()
            .zip(&full_parameter.values)
            .all(|(left, right)| left.to_bits() == right.to_bits()));
    }
}

#[test]
fn envelope_and_delta_arithmetic_match_the_written_order_v1() {
    // The two frozen envelope constants decode to the design's values.
    assert_eq!(
        f64::from(f32::from_bits(ENVELOPE_ABSOLUTE_BITS_V1)),
        f64::from(1.0e-6f32)
    );
    assert_eq!(
        f64::from(f32::from_bits(ENVELOPE_RELATIVE_BITS_V1)),
        f64::from(f32::from_bits(0x3580_0000))
    );
    // E is symmetric and grows with the larger operand magnitude.
    assert_eq!(envelope_v1(3.0, -7.0), envelope_v1(-7.0, 3.0));
    assert!(envelope_v1(1000.0, 0.0) > envelope_v1(1.0, 0.0));
    assert_eq!(
        envelope_v1(0.0, 0.0),
        Some(f64::from(f32::from_bits(ENVELOPE_ABSOLUTE_BITS_V1)))
    );
    // A delta is exactly one f64 subtraction of exactly converted operands.
    assert_eq!(delta_v1(1.5f32, 2.25f32), 0.75f64);
    assert_eq!(delta_v1(0.0f32, -0.0f32), 0.0f64);
    // The step-one ceiling is the design's base plus its own envelope.
    let base = f64::from(f32::from_bits(MAX_ABSOLUTE_DELTA_BASE_BITS_V1));
    assert_eq!(
        max_absolute_delta_bound_v1(),
        Some(base + envelope_v1(base, 0.0).unwrap())
    );
    assert!(max_absolute_delta_bound_v1().unwrap() > base);
    // The historical 0.03803 envelope must not be reachable from here.
    assert!(max_absolute_delta_bound_v1().unwrap() < 0.0038);
}

/// Authority correction: every envelope comparison must reject nonfinite
/// operands and nonfinite results rather than silently passing them.
#[test]
fn envelope_rejects_every_nonfinite_operand_and_result_v1() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(envelope_v1(bad, 0.0), None);
        assert_eq!(envelope_v1(0.0, bad), None);
        assert_eq!(envelope_v1(bad, bad), None);
        // A nonfinite operand never compares "within tolerance".
        assert!(!within_envelope_v1(bad, 0.0));
        assert!(!within_envelope_v1(0.0, bad));
        assert!(!within_envelope_v1(bad, bad));
        assert!(!absolute_delta_admissible_v1(bad));
        // Nonfinite inputs to the derived gates also fail closed.
        assert!(!digest_column_delta_consistent_v1(f32::NAN, 1.0, 1.0));
        assert!(!digest_gradient_matches_halved_full_v1(f32::NAN, 1.0));
        assert!(!digest_gradient_matches_halved_full_v1(1.0, f32::INFINITY));
    }
    // An operand large enough to overflow the relative term is rejected too.
    assert_eq!(
        envelope_v1(f64::MAX, f64::MAX),
        Some(f64::MAX * 9.5367431640625e-7 + f64::from(f32::from_bits(ENVELOPE_ABSOLUTE_BITS_V1)))
            .filter(|value| value.is_finite())
    );
    // Identical finite values are always inside their own envelope.
    assert!(within_envelope_v1(1.0, 1.0));
    assert!(within_envelope_v1(0.0, 0.0));
    // The step-one ceiling admits a compliant delta and rejects an oversized
    // one.
    assert!(absolute_delta_admissible_v1(0.0));
    assert!(absolute_delta_admissible_v1(0.0009));
    assert!(!absolute_delta_admissible_v1(0.01));
}

#[test]
fn digest_column_delta_consistency_gate_is_exact_v1() {
    // H's stored parameter is exactly twice F's, so eb == F_before.
    let f_before = -1.375f32;
    let h_before = 2.0f32 * f_before;
    let h_after = h_before + 0.0009765625f32;
    assert!(digest_column_delta_consistent_v1(
        h_before, h_after, f_before
    ));
    // If H's before-parameter is not exactly the doubled F value, the gate
    // must reject: this catches a mis-derived or drifted H.
    assert!(!digest_column_delta_consistent_v1(
        h_before,
        h_after,
        f_before + 0.25
    ));
    // The halved-gradient relation holds within the envelope.
    let f_gradient = 0.5f32;
    assert!(digest_gradient_matches_halved_full_v1(0.25f32, f_gradient));
    assert!(!digest_gradient_matches_halved_full_v1(0.5f32, f_gradient));
}

#[test]
fn combined_validation_loss_uses_the_frozen_weighted_order_v1() {
    let means = StratumMeansV1 {
        promoted2: 2.0,
        predecessor_a: 3.0,
        predecessor_b: 4.0,
        uniform: 5.0,
    };
    // ((((2*2)+3)+4)+5)/5 = 16/5 = 3.2, reproduced in the written order.
    assert_eq!(combined_validation_loss_v1(means), 16.0f64 / 5.0f64);
    assert_eq!(
        combined_validation_loss_v1(means),
        ((((2.0f64 * 2.0f64) + 3.0f64) + 4.0f64) + 5.0f64) / 5.0f64
    );
    // The promoted(2) stratum carries exactly double weight. The difference
    // is 2/5 in exact arithmetic; neither 18/5 nor 16/5 is representable in
    // f64, so this compares the realized f64 quotients rather than assuming
    // the subtraction is exact.
    let doubled_primary = StratumMeansV1 {
        promoted2: 3.0,
        ..means
    };
    assert_eq!(
        combined_validation_loss_v1(doubled_primary),
        18.0f64 / 5.0f64
    );
    assert!(
        (combined_validation_loss_v1(doubled_primary)
            - combined_validation_loss_v1(means)
            - (2.0f64 / 5.0f64))
            .abs()
            <= f64::EPSILON
    );
    assert_eq!(improvement_v1(5.0, 3.0), 2.0);
    assert_eq!(paired_difference_v1(2.0, 0.5), 1.5);
    assert_eq!(frozen_advantage_v1(-1, 0.25f32), -1.25f32);
    assert_eq!(frozen_advantage_v1(1, 0.25f32), 0.75f32);
}

#[test]
fn validation_atom_uses_production_log_softmax_and_the_frozen_advantage_v1() {
    let logits = vec![vec![0.0f32, 0.0f32], vec![1.0f32, 1.0f32, 1.0f32]];
    let chosen = vec![0usize, 2usize];
    // Uniform logits: log_softmax = -ln(n) for every entry.
    let atom = validation_atom_v1(&logits, &chosen, 2.0f32).unwrap();
    let expected = -((-(2.0f32.ln())) + (-(3.0f32.ln()))) * 2.0f32;
    assert!((atom - expected).abs() < 1e-6);

    // The atom is computed through the PRODUCTION selected_log_softmax, so
    // it must agree with that function called directly.
    let (first, _) = selected_log_softmax(&logits[0], chosen[0]).unwrap();
    let (second, _) = selected_log_softmax(&logits[1], chosen[1]).unwrap();
    assert_eq!(atom.to_bits(), (-(first + second) * 2.0f32).to_bits());

    // Shape, range, and finiteness are fail-closed.
    assert_eq!(
        validation_atom_v1(&logits, &[0usize], 1.0),
        Err(ValidationAtomErrorV1::Shape)
    );
    assert_eq!(
        validation_atom_v1(&[], &[], 1.0),
        Err(ValidationAtomErrorV1::Shape)
    );
    assert_eq!(
        validation_atom_v1(&logits, &[0usize, 9usize], 1.0),
        Err(ValidationAtomErrorV1::SelectedOutOfRange)
    );
    assert_eq!(
        validation_atom_v1(&logits, &chosen, f32::NAN),
        Err(ValidationAtomErrorV1::NonFinite)
    );
    assert_eq!(
        validation_atom_v1(&[vec![f32::NAN, 0.0]], &[0usize], 1.0),
        Err(ValidationAtomErrorV1::ProductionLogSoftmax)
    );
    // The production row sums to 1 in probability space.
    let (_, row) = selected_log_softmax(&[2.0, -1.0, 0.5], 0).unwrap();
    let total: f32 = row.iter().map(|value| value.exp()).sum();
    assert!((total - 1.0).abs() < 1e-6);
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn held_out_evaluator_uses_real_groups_and_rejects_an_empty_stratum_v1() {
    let (_authority, _deck_ids, _deck_hashes, mut tape) = joined_fixture_v1();
    let parameters = native_stream_fixture_v1(0.0);
    let state = fresh_state_from_parameters_v1(&parameters);
    let loss = evaluate_held_out_loss_v1(&state, ActualTreatmentV1::Full, &tape, false)
        .expect("all-zero model must yield a finite held-out projection");
    assert!(loss.combined.is_finite());
    assert!(loss.promoted2_v1().is_finite());
    assert_eq!(
        evaluate_held_out_loss_v1(&state, ActualTreatmentV1::Full, &tape, true),
        Err(HeldOutEvaluationErrorV1::InitialOutputMismatch)
    );

    for episode in &mut tape.episodes {
        if episode.stratum == 0 {
            episode.groups.clear();
        }
    }
    assert_eq!(
        evaluate_held_out_loss_v1(&state, ActualTreatmentV1::Full, &tape, false),
        Err(HeldOutEvaluationErrorV1::EmptyStratum)
    );
}

/// Authority #131: F and H come from ONE repaired-FULL bit-copy lineage.
/// Repair runs exactly once; H is a pure transform of that same base.
#[test]
fn treatment_pair_uses_one_repair_and_a_pure_half_transform_v1() {
    let (baseline, actions) = synthetic_action_tensor_v1(&[
        (FlatScorerActionKindV2::ChooseEffectBoolean, 0),
        (
            FlatScorerActionKindV2::ChooseAttackerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
        (FlatScorerActionKindV2::Pass, 0),
    ]);

    let lineage = RetainedTreatmentLineageV1::from_pre_repair_v1(baseline, actions).unwrap();
    assert!(lineage.relation_valid_v1());
    let full = lineage.full_tensor_v1();
    let half = lineage.half_tensor_v1();

    // H is exactly F outside the digest block, and exactly halved inside it.
    for (full_row, half_row) in full
        .action_features
        .chunks_exact(ACTION_FEATURE_DIM_V1)
        .zip(half.action_features.chunks_exact(ACTION_FEATURE_DIM_V1))
    {
        for column in 0..ACTION_HASH_BEGIN_V1 {
            assert_eq!(half_row[column].to_bits(), full_row[column].to_bits());
        }
        for column in ACTION_HASH_BEGIN_V1..ACTION_HASH_END_V1 {
            assert_eq!(
                half_row[column].to_bits(),
                (full_row[column] * 0.5f32).to_bits()
            );
        }
    }

    // The pure transform performs NO repair and reproduces the sealed H arm.
    assert!(pure_half_digest_transform_v1(full).is_ok());

    // The pure transform is also independent of action metadata: it never
    // reads `actions`, so an empty action list cannot change its result.
    let again = pure_half_digest_transform_v1(full).unwrap();
    assert!(again
        .action_features
        .iter()
        .zip(&half.action_features)
        .all(|(left, right)| left.to_bits() == right.to_bits()));
}

/// Release-mode fail-closed gates on the parameter transforms: exactly one
/// target, exact `[64,259]` shape, exact value length, and rejection of
/// missing, duplicated, or malformed targets.
#[test]
fn parameter_transforms_fail_closed_on_target_shape_and_length_v1() {
    let good = |values: Vec<f32>, shape: Vec<usize>| NativeNamedParameterV1 {
        name: ACTION_ENCODER_WEIGHT_NAME_V1,
        shape,
        values,
    };
    let exact = || {
        good(
            vec![0.25f32; ACTION_ENCODER_VALUE_COUNT_V1],
            vec![ACTION_ENCODER_ROWS_V1, ACTION_ENCODER_COLUMNS_V1],
        )
    };
    assert_eq!(ACTION_ENCODER_VALUE_COUNT_V1, 64 * 259);

    // A well-formed single target succeeds and round-trips exactly.
    let full = vec![exact()];
    let half = derive_half_parameters_v1(&full).unwrap();
    let restored = halve_action_encoder_digest_columns_v1(&half).unwrap();
    assert!(restored[0]
        .values
        .iter()
        .zip(&full[0].values)
        .all(|(left, right)| left.to_bits() == right.to_bits()));

    // Missing target.
    let missing = vec![NativeNamedParameterV1 {
        name: "action_encoder.0.bias",
        shape: vec![ACTION_ENCODER_ROWS_V1],
        values: vec![0.0; ACTION_ENCODER_ROWS_V1],
    }];
    assert_eq!(
        derive_half_parameters_v1(&missing),
        Err(ParameterTransformErrorV1::MissingTarget)
    );
    assert_eq!(
        halve_action_encoder_digest_columns_v1(&missing),
        Err(ParameterTransformErrorV1::MissingTarget)
    );
    assert_eq!(
        derive_half_parameters_v1(&[]),
        Err(ParameterTransformErrorV1::MissingTarget)
    );

    // Duplicate target.
    let duplicated = vec![exact(), exact()];
    assert_eq!(
        derive_half_parameters_v1(&duplicated),
        Err(ParameterTransformErrorV1::DuplicateTarget)
    );
    assert_eq!(
        halve_action_encoder_digest_columns_v1(&duplicated),
        Err(ParameterTransformErrorV1::DuplicateTarget)
    );

    // Wrong shape, including the transposed and the mistaken-128 forms.
    for shape in [
        vec![ACTION_ENCODER_COLUMNS_V1, ACTION_ENCODER_ROWS_V1],
        vec![128, ACTION_ENCODER_COLUMNS_V1],
        vec![ACTION_ENCODER_ROWS_V1],
        vec![ACTION_ENCODER_ROWS_V1, ACTION_ENCODER_COLUMNS_V1, 1],
    ] {
        let wrong = vec![good(vec![0.25f32; ACTION_ENCODER_VALUE_COUNT_V1], shape)];
        assert_eq!(
            derive_half_parameters_v1(&wrong),
            Err(ParameterTransformErrorV1::ShapeMismatch)
        );
        assert_eq!(
            halve_action_encoder_digest_columns_v1(&wrong),
            Err(ParameterTransformErrorV1::ShapeMismatch)
        );
    }

    // Correct shape but wrong value length.
    for length in [
        0,
        ACTION_ENCODER_VALUE_COUNT_V1 - 1,
        ACTION_ENCODER_VALUE_COUNT_V1 + 1,
    ] {
        let wrong = vec![good(
            vec![0.25f32; length],
            vec![ACTION_ENCODER_ROWS_V1, ACTION_ENCODER_COLUMNS_V1],
        )];
        assert_eq!(
            derive_half_parameters_v1(&wrong),
            Err(ParameterTransformErrorV1::ValueLengthMismatch)
        );
        assert_eq!(
            halve_action_encoder_digest_columns_v1(&wrong),
            Err(ParameterTransformErrorV1::ValueLengthMismatch)
        );
    }

    // A nonfinite or overflowing value fails closed through the scaling gate.
    let mut nonfinite = exact();
    nonfinite.values[ACTION_HASH_BEGIN_V1] = f32::NAN;
    assert_eq!(
        derive_half_parameters_v1(&[nonfinite]),
        Err(ParameterTransformErrorV1::Scaling(
            ScalingValidityErrorV1::NonFinite
        ))
    );
    let mut overflowing = exact();
    overflowing.values[ACTION_HASH_BEGIN_V1] = f32::MAX;
    assert_eq!(
        derive_half_parameters_v1(&[overflowing]),
        Err(ParameterTransformErrorV1::Scaling(
            ScalingValidityErrorV1::DoubleRoundTrip
        ))
    );
}

#[test]
fn paired_cluster_bootstrap_enumerates_all_tuples_in_the_frozen_order_v1() {
    let values = [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0];
    let means = paired_cluster_bootstrap_v1(&values).unwrap();
    assert_eq!(means.len(), BOOTSTRAP_TUPLE_COUNT_V1);
    assert_eq!(BOOTSTRAP_TUPLE_COUNT_V1, 6usize.pow(6));
    // Sorted ascending by total_cmp.
    assert!(means.windows(2).all(|pair| pair[0] <= pair[1]));
    // The extremes are the all-min and all-max tuples.
    assert_eq!(means[0], 1.0);
    assert_eq!(means[BOOTSTRAP_TUPLE_COUNT_V1 - 1], 6.0);
    // A constant input collapses every mean to that constant.
    let constant = paired_cluster_bootstrap_v1(&[2.5; BOOTSTRAP_UNIT_COUNT_V1]).unwrap();
    assert!(constant.iter().all(|mean| *mean == 2.5));
    // Nonfinite input is rejected, which the classifier maps to INVALID.
    assert!(paired_cluster_bootstrap_v1(&[f64::NAN, 1.0, 1.0, 1.0, 1.0, 1.0]).is_none());
    assert!(paired_cluster_bootstrap_v1(&[f64::INFINITY, 1.0, 1.0, 1.0, 1.0, 1.0]).is_none());
    // The two published read indices are inside the enumeration.
    let read = bootstrap_read_v1(&means).unwrap();
    assert_eq!(read.low_index_value, means[BOOTSTRAP_LOW_INDEX_V1]);
    assert_eq!(read.high_index_value, means[BOOTSTRAP_HIGH_INDEX_V1]);
    assert!(read.low_index_value < read.high_index_value);
    // A short or long vector cannot be read.
    assert!(bootstrap_read_v1(&means[..10]).is_none());
}

/// Independent, slow reference enumeration: six nested loops in the exact
/// declared order, cross-checked against the production enumerator.
#[test]
fn bootstrap_matches_an_independent_nested_loop_reference_v1() {
    let values = [-2.0f64, 0.5, 1.25, -0.75, 3.0, 0.0];
    let mut reference = Vec::with_capacity(BOOTSTRAP_TUPLE_COUNT_V1);
    for a in 0..6 {
        for b in 0..6 {
            for c in 0..6 {
                for d in 0..6 {
                    for e in 0..6 {
                        for f in 0..6 {
                            // Positive zero start, left-to-right adds, one
                            // division.
                            let mut sum = 0.0f64;
                            sum += values[a];
                            sum += values[b];
                            sum += values[c];
                            sum += values[d];
                            sum += values[e];
                            sum += values[f];
                            reference.push(sum / 6.0);
                        }
                    }
                }
            }
        }
    }
    reference.sort_by(f64::total_cmp);
    let produced = paired_cluster_bootstrap_v1(&values).unwrap();
    assert_eq!(produced.len(), reference.len());
    assert!(produced
        .iter()
        .zip(&reference)
        .all(|(left, right)| left.to_bits() == right.to_bits()));
}

#[test]
fn classifier_requires_all_four_conditions_for_half_nomination_v1() {
    // A clearly positive, low-variance six-unit result nominates.
    let strong = [0.10f64, 0.11, 0.12, 0.09, 0.13, 0.10];
    let nominated = classify_v1(&strong, &strong, true);
    assert_eq!(nominated.disposition, DispositionV1::HalfNominated);
    assert_eq!(nominated.positive_paired_count, 6);
    assert!(nominated.paired_read.unwrap().low_index_value > 0.0);

    // Exactly five positive still nominates when the interval clears zero.
    let five_positive = [0.10f64, 0.11, 0.12, 0.09, 0.13, -0.001];
    assert_eq!(
        classify_v1(&five_positive, &five_positive, true).positive_paired_count,
        5
    );

    // Four positive fails the sign-count condition.
    let four_positive = [0.10f64, 0.11, 0.12, 0.09, -0.13, -0.10];
    assert_eq!(
        classify_v1(&four_positive, &strong, true).disposition,
        DispositionV1::NoNomination
    );

    // Five positive with one materially negative unit: the sign gate passes
    // (5 >= 5) but the index-1166 bootstrap value is dragged below zero, so
    // the interval gate fails and the disposition must be NO-NOMINATION.
    // This is the case that distinguishes the two gates from each other.
    let straddling = [0.01f64, 0.02, 0.015, 0.012, 0.018, -0.9];
    let straddling_result = classify_v1(&straddling, &strong, true);
    assert_eq!(straddling_result.positive_paired_count, 5);
    assert!(straddling_result.gates.paired_sign_gate);
    assert!(
        !straddling_result.gates.paired_interval_gate,
        "one materially negative unit must pull the index-1166 value below zero"
    );
    assert!(straddling_result.paired_read.unwrap().low_index_value < 0.0);
    assert_eq!(straddling_result.disposition, DispositionV1::NoNomination);

    // A failing promoted(2) safeguard blocks nomination even when d passes.
    let negative = [-0.10f64, -0.11, -0.12, -0.09, -0.13, -0.10];
    assert_eq!(
        classify_v1(&strong, &negative, true).disposition,
        DispositionV1::NoNomination
    );

    // INVALID has precedence over everything.
    assert_eq!(
        classify_v1(&strong, &strong, false).disposition,
        DispositionV1::Invalid
    );
    let nonfinite = [f64::NAN, 0.11, 0.12, 0.09, 0.13, 0.10];
    assert_eq!(
        classify_v1(&nonfinite, &strong, true).disposition,
        DispositionV1::Invalid
    );
    assert_eq!(
        classify_v1(&strong, &nonfinite, true).disposition,
        DispositionV1::Invalid
    );
    assert_eq!(DispositionV1::HalfNominated.name_v1(), "HALF-NOMINATED");
    assert_eq!(DispositionV1::NoNomination.name_v1(), "NO-NOMINATION");
    assert_eq!(DispositionV1::Invalid.name_v1(), "INVALID");
}

/// A reviewer must be able to recompute every derived field from the
/// published inputs and reject a mismatch.
#[test]
fn classifier_result_is_fully_recomputable_from_published_inputs_v1() {
    let paired = [0.02f64, -0.01, 0.03, 0.04, 0.05, 0.06];
    let promoted2 = [0.20f64, 0.21, 0.22, 0.19, 0.23, 0.20];
    let result = classify_v1(&paired, &promoted2, true);

    let recomputed_paired = paired_cluster_bootstrap_v1(&result.paired_differences).unwrap();
    let recomputed_promoted2 = paired_cluster_bootstrap_v1(&result.promoted2_improvements).unwrap();
    assert_eq!(
        result.paired_read.unwrap(),
        bootstrap_read_v1(&recomputed_paired).unwrap()
    );
    assert_eq!(
        result.promoted2_read.unwrap(),
        bootstrap_read_v1(&recomputed_promoted2).unwrap()
    );
    assert_eq!(
        result.positive_paired_count,
        paired.iter().filter(|value| **value > 0.0).count()
    );
    assert_eq!(
        result.positive_promoted2_count,
        promoted2.iter().filter(|value| **value > 0.0).count()
    );
    // Recomputing the classifier from the published arrays reproduces the
    // identical disposition.
    assert_eq!(
        classify_v1(
            &result.paired_differences,
            &result.promoted2_improvements,
            true
        ),
        result
    );
}

// ---------------------------------------------------------------------------
// Serializer goldens. These pin the exact framed bytes so a silent encoding
// change is caught here rather than after a formal run.
// ---------------------------------------------------------------------------

#[test]
fn framed_writer_atom_layout_is_exact_v1() {
    let mut writer = FramedWriterV1 { buffer: Vec::new() };
    writer.atom_v1("ab", &[0x01, 0x02, 0x03]);
    assert_eq!(
        writer.bytes_v1(),
        &[
            0, 0, 0, 2, // u32be label length
            b'a', b'b', // label
            0, 0, 0, 0, 0, 0, 0, 3, // u64be payload length
            1, 2, 3, // payload
        ]
    );
    // Little-endian bit arrays.
    let mut numbers = FramedWriterV1 { buffer: Vec::new() };
    numbers.f32_bits_array_v1("f", &[1.0f32]);
    assert_eq!(
        numbers.bytes_v1(),
        &[0, 0, 0, 1, b'f', 0, 0, 0, 0, 0, 0, 0, 4, 0x00, 0x00, 0x80, 0x3f]
    );
    let mut wide = FramedWriterV1 { buffer: Vec::new() };
    wide.u64_v1("n", 1);
    assert_eq!(
        wide.bytes_v1(),
        &[0, 0, 0, 1, b'n', 0, 0, 0, 0, 0, 0, 0, 8, 1, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn framed_writer_opens_every_record_with_version_and_schema_v1() {
    let writer = FramedWriterV1::new_v1(TAPE_SCHEMA_V1);
    let mut expected = FramedWriterV1 { buffer: Vec::new() };
    expected.atom_v1("version", SERIALIZER_VERSION_V1.as_bytes());
    expected.atom_v1("schema", TAPE_SCHEMA_V1.as_bytes());
    assert_eq!(writer.bytes_v1(), expected.bytes_v1());
    // Distinct schemas produce distinct digests for otherwise identical
    // content, so a record cannot be reinterpreted as another kind.
    assert_ne!(
        FramedWriterV1::new_v1(TAPE_SCHEMA_V1).sha256_v1(),
        FramedWriterV1::new_v1(UPDATE_SCHEMA_V1).sha256_v1()
    );
}

/// Length framing must make the stream unambiguous: two different atom
/// splits of the same concatenated payload cannot collide.
#[test]
fn framed_writer_is_injective_across_atom_boundaries_v1() {
    let mut left = FramedWriterV1 { buffer: Vec::new() };
    left.atom_v1("x", b"ab");
    left.atom_v1("y", b"c");
    let mut right = FramedWriterV1 { buffer: Vec::new() };
    right.atom_v1("x", b"a");
    right.atom_v1("y", b"bc");
    assert_ne!(left.sha256_v1(), right.sha256_v1());

    let mut label_shift = FramedWriterV1 { buffer: Vec::new() };
    label_shift.atom_v1("xy", b"a");
    let mut label_shift_other = FramedWriterV1 { buffer: Vec::new() };
    label_shift_other.atom_v1("x", b"ya");
    assert_ne!(label_shift.sha256_v1(), label_shift_other.sha256_v1());
}

/// Frozen digest goldens for the six record headers. Any change to the
/// version atom, a schema atom, or the framing changes these, which is
/// exactly the silent drift this pin is here to catch.
///
/// Cross-checked against an independent out-of-crate reimplementation of the
/// atom framing, which reproduced all six digests exactly.
const SERIALIZER_HEADER_GOLDENS_V1: [(&str, &str); 6] = [
    (
        TAPE_SCHEMA_V1,
        "e3757757121b9976f4f618399f44e2ec569bc2f5dafd458ffb87251f38a2e083",
    ),
    (
        JOINED_TAPE_SCHEMA_V1,
        "c4abe9ff2e165ed4381d23003c4c7e12d77f6170990a702ce08cf664b8549809",
    ),
    (
        UPDATE_SCHEMA_V1,
        "a005f83d8229be221635ad520cc03b79670dc071c5f4a7f6d3707f961c75f15d",
    ),
    (
        SUMMARY_SCHEMA_V1,
        "258c4cf52ae3b8cf65f265af1ed9282c5c162591fc1cc5f2b3987925fcabb109",
    ),
    (
        MANIFEST_SCHEMA_V1,
        "bbbed0896cb0a7c682aeccd458ac8074f92ab79500eb20eca60a7b0a2b99c05c",
    ),
    (
        PREFLIGHT_UPDATE_PAIR_SCHEMA_V1,
        "0f75424762a1d55f1b96b9b7f1b0f77750665633ef8ade73771887f00b85e53f",
    ),
];

#[test]
fn serializer_record_header_goldens_are_frozen_v1() {
    let mut seen = std::collections::HashSet::new();
    for (schema, golden) in SERIALIZER_HEADER_GOLDENS_V1 {
        let digest = FramedWriterV1::new_v1(schema).sha256_v1();
        assert_eq!(digest.len(), 64);
        assert!(digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
        assert!(
            seen.insert(digest.clone()),
            "every schema header digest must be distinct"
        );
        assert_eq!(
            digest, golden,
            "frozen header digest drift for schema {schema}"
        );
    }
}

fn summary_unit_fixture_v1(
    index: usize,
    full_after: f64,
    half_after: f64,
    p2_after: f64,
) -> SummaryUnitInputV1 {
    SummaryUnitInputV1 {
        unit_index: index,
        training_seed: FORMAL_TRAINING_SEEDS_V1[index],
        validation_seed: FORMAL_VALIDATION_SEEDS_V1[index],
        full_loss_before_bits: 1.0f64.to_bits(),
        full_loss_after_bits: full_after.to_bits(),
        half_loss_before_bits: 1.0f64.to_bits(),
        half_loss_after_bits: half_after.to_bits(),
        promoted2_half_loss_before_bits: 1.0f64.to_bits(),
        promoted2_half_loss_after_bits: p2_after.to_bits(),
        tape_sha256: "a".repeat(64),
        full_update_sha256: "b".repeat(64),
        half_update_sha256: "c".repeat(64),
    }
}

fn summary_units_fixture_v1() -> Vec<SummaryUnitInputV1> {
    // H beats F on every unit, and promoted(2) improves, so this is an
    // honest HALF-NOMINATED six-unit result.
    (0..BOOTSTRAP_UNIT_COUNT_V1)
        .map(|index| {
            let jitter = index as f64 * 0.001;
            summary_unit_fixture_v1(index, 0.90 + jitter, 0.80 + jitter, 0.70 + jitter)
        })
        .collect()
}

/// The summary binds its published raw arrays back to the authoritative
/// per-unit records, which is the only thing that can reject a
/// self-consistent raw-array replacement.
#[test]
fn summary_binds_published_arrays_to_authoritative_units_v1() {
    let units = summary_units_fixture_v1();
    let (paired, promoted2) = derive_classifier_inputs_v1(&units).unwrap();
    let result = classify_v1(&paired, &promoted2, true);
    let frame = frame_summary_v1(&units, &result).unwrap();
    let bytes = frame.bytes_v1();

    // Every published raw f64 appears verbatim.
    for value in paired.iter().chain(&promoted2) {
        let needle = value.to_bits().to_le_bytes();
        assert!(
            bytes.windows(8).any(|window| window == needle),
            "every published f64 must appear as raw bits"
        );
    }
    assert!(bytes
        .windows(result.disposition.name_v1().len())
        .any(|window| window == result.disposition.name_v1().as_bytes()));

    // THE #143 GATE: a self-consistent replacement of the published raw
    // arrays is rejected, even though it self-verifies perfectly.
    let mut swapped = paired;
    swapped[0] = paired[0] + 0.5;
    let self_consistent_forgery = classify_v1(&swapped, &promoted2, true);
    assert!(
        verify_classifier_result_v1(&self_consistent_forgery),
        "the forgery is internally self-consistent, so self-recomputation accepts it"
    );
    assert_eq!(
        frame_summary_v1(&units, &self_consistent_forgery),
        Err(SummaryFramingErrorV1::RawInputsDisagreeWithUnits),
        "binding to the authoritative units must reject it anyway"
    );

    // Derived-field forgery is still rejected by the second gate.
    let mut forged = result.clone();
    forged.positive_paired_count += 1;
    assert_eq!(
        frame_summary_v1(&units, &forged),
        Err(SummaryFramingErrorV1::ForgedResult)
    );

    // A genuine SOURCE-unit mutation followed by full recomputation is a
    // different HONEST summary, and must produce a different artifact hash.
    let mut mutated_units = units.clone();
    mutated_units[0].half_loss_after_bits = 0.85f64.to_bits();
    let (mutated_paired, mutated_promoted2) = derive_classifier_inputs_v1(&mutated_units).unwrap();
    let mutated_result = classify_v1(&mutated_paired, &mutated_promoted2, true);
    let mutated_frame = frame_summary_v1(&mutated_units, &mutated_result).unwrap();
    assert_ne!(frame.sha256_v1(), mutated_frame.sha256_v1());

    // Malformed unit sets are rejected before anything is bound.
    assert_eq!(
        frame_summary_v1(&units[..5], &result),
        Err(SummaryFramingErrorV1::MalformedUnits)
    );
    let mut misordered = units.clone();
    misordered.swap(0, 1);
    assert_eq!(
        frame_summary_v1(&misordered, &result),
        Err(SummaryFramingErrorV1::MalformedUnits)
    );
    let mut wrong_seed_pair = units.clone();
    wrong_seed_pair[0].training_seed = PREFLIGHT_BASE_SEED_V1;
    wrong_seed_pair[0].validation_seed = PREFLIGHT_BASE_SEED_V1;
    assert_eq!(
        frame_summary_v1(&wrong_seed_pair, &result),
        Err(SummaryFramingErrorV1::MalformedUnits)
    );
    let mut bad_digest = units.clone();
    bad_digest[2].tape_sha256 = "NOTAHASH".to_string();
    assert_eq!(
        frame_summary_v1(&bad_digest, &result),
        Err(SummaryFramingErrorV1::MalformedUnits)
    );
    let mut uppercase_digest = units.clone();
    uppercase_digest[3].full_update_sha256 = "A".repeat(64);
    assert_eq!(
        frame_summary_v1(&uppercase_digest, &result),
        Err(SummaryFramingErrorV1::MalformedUnits)
    );
}

/// An honest NaN-bearing authoritative unit produces an INVALID summary that
/// frames and verifies; mutating only the published payload does not.
#[test]
fn honest_nan_bearing_units_produce_a_verifiable_invalid_summary_v1() {
    let mut units = summary_units_fixture_v1();
    units[2].half_loss_after_bits = f64::NAN.to_bits();
    let (paired, promoted2) = derive_classifier_inputs_v1(&units).unwrap();
    assert!(paired[2].is_nan());

    let result = classify_v1(&paired, &promoted2, true);
    assert_eq!(result.disposition, DispositionV1::Invalid);
    // The honest INVALID summary frames successfully.
    let frame = frame_summary_v1(&units, &result).unwrap();
    assert_eq!(frame.sha256_v1().len(), 64);

    // Mutating only the published raw payload, leaving the units alone,
    // fails the binding gate.
    let mut mutated_payload = result.clone();
    mutated_payload.paired_differences[2] =
        f64::from_bits(f64::NAN.to_bits() ^ 0x0000_0000_0000_0001);
    assert_eq!(
        frame_summary_v1(&units, &mutated_payload),
        Err(SummaryFramingErrorV1::RawInputsDisagreeWithUnits)
    );
}

/// Owns the borrowed data a hierarchical tape fixture points at. This CPU
/// fixture deliberately keeps using the preflight seed `949999`; the formal
/// seeds enter live episodes only through their sealed runtime authorities.
struct TapeFixtureV1 {
    tensor: NativeFlatDecisionTensorV2,
    actions: Vec<FlatScorerActionCoreV2>,
    logit_bits: Vec<u32>,
    display_softmax_probability_bits: Vec<u32>,
    selected_log_probability_bits: u32,
}

fn tape_fixture_v1() -> TapeFixtureV1 {
    let (baseline, actions) = synthetic_action_tensor_v1(&[
        (FlatScorerActionKindV2::ChooseEffectBoolean, 0),
        (
            FlatScorerActionKindV2::ChooseAttackerInclusion,
            FLAT_ACTION_FLAG_INCLUDE_V1,
        ),
        (FlatScorerActionKindV2::Pass, 0),
    ]);
    // The tape binds the repaired-FULL base from the single-repair lineage.
    let pair = derive_treatment_pair_v1(&baseline, &actions).unwrap();
    let logit_bits: Vec<u32> = (0..actions.len())
        .map(|index| (index as f32 * 0.25 - 0.5).to_bits())
        .collect();
    let logits: Vec<f32> = logit_bits
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect();
    let (selected_log_probability, log_softmax_row) = selected_log_softmax(&logits, 1).unwrap();
    let display_softmax_probability_bits = log_softmax_row
        .iter()
        .map(|log_probability| log_probability.exp().to_bits())
        .collect();
    TapeFixtureV1 {
        tensor: pair.full,
        actions,
        logit_bits,
        display_softmax_probability_bits,
        selected_log_probability_bits: selected_log_probability.to_bits(),
    }
}

fn substep_v1(fixture: &TapeFixtureV1, ordinal: u64, value: f32) -> TapeSubstepV1<'_> {
    TapeSubstepV1 {
        learner_ordinal: ordinal,
        selected_index: 1,
        raw_action_logit_bits: &fixture.logit_bits,
        display_softmax_probability_bits: &fixture.display_softmax_probability_bits,
        selected_log_probability_bits: fixture.selected_log_probability_bits,
        predicted_value_bits: value.to_bits(),
        source_action_cores: &fixture.actions,
        repaired_tensor: &fixture.tensor,
    }
}

/// A tape side over the real schedule for `seed`, with a deliberately
/// ragged hierarchy: variable groups per episode (including zero-group
/// episodes, which are legal) and variable substeps per group.
fn tape_side_v1<'a>(fixture: &'a TapeFixtureV1, seed: u64) -> TapeSideV1<'a> {
    let episodes = (0..EPISODES_PER_TAPE_V1)
        .map(|episode_index| {
            let stratum =
                stratum_ordinal_v1(ladder_pool_member_for_episode_v1(seed, episode_index).unwrap());
            // Ragged on purpose: episode 0 of each stratum gets groups; some
            // episodes get none at all.
            let group_count = match episode_index % 4 {
                0 => 3,
                1 => 1,
                2 => 0,
                _ => 2,
            };
            let groups = (0..group_count)
                .map(|group_index| {
                    let substep_count = if group_index == 0 { 2 } else { 1 };
                    TapeGroupV1 {
                        physical_decision_id: episode_index * 16 + group_index,
                        substeps: (0..substep_count)
                            .map(|substep_index| {
                                substep_v1(
                                    fixture,
                                    episode_index * 64 + group_index * 8 + substep_index,
                                    0.25f32 * (substep_index as f32 + 1.0),
                                )
                            })
                            .collect(),
                    }
                })
                .collect();
            TapeEpisodeV1 {
                episode_id: episode_index,
                stratum,
                learner_return: if episode_index % 2 == 0 { 1 } else { -1 },
                trace_hash: 0xA5A5_0000 + episode_index,
                groups,
            }
        })
        .collect();
    TapeSideV1 {
        seed,
        counts: pool_choice_counts_v1(seed, EPISODES_PER_TAPE_V1),
        episodes,
    }
}

/// The tape preserves the observer hierarchy, derives its own validation
/// gate, and treats zero-group episodes as legal.
#[test]
fn tape_preserves_the_episode_group_substep_hierarchy_v1() {
    let fixture = tape_fixture_v1();
    let training = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);
    let validation = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);

    // Groups, substeps, and episodes are genuinely different cardinalities.
    assert_eq!(training.episodes.len() as u64, EPISODES_PER_TAPE_V1);
    assert_ne!(training.total_group_count_v1(), training.episodes.len());
    assert_ne!(
        training.total_substep_count_v1(),
        training.total_group_count_v1()
    );
    // Zero-group episodes are present and legal.
    assert!(training
        .episodes
        .iter()
        .any(|episode| episode.groups.is_empty()));
    // Variable substeps per group.
    let substep_lengths: std::collections::HashSet<usize> = training
        .episodes
        .iter()
        .flat_map(|episode| &episode.groups)
        .map(|group| group.substeps.len())
        .collect();
    assert!(substep_lengths.len() > 1);

    let tape = frame_tape_v1(&training, &validation).unwrap();
    assert_eq!(tape.sha256_v1().len(), 64);

    // The per-stratum group counts are DERIVED from the hierarchy and every
    // stratum carries at least one learner group.
    let derived = validation.groups_per_stratum_v1();
    assert!(derived.iter().all(|count| *count > 0));
    assert_eq!(
        derived.iter().sum::<u32>() as usize,
        validation.total_group_count_v1()
    );

    // A stratum emptied of learner groups fails the derived gate, even
    // though every episode record is still present.
    let mut emptied = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);
    for episode in &mut emptied.episodes {
        if episode.stratum == 1 {
            episode.groups.clear();
        }
    }
    assert_eq!(emptied.groups_per_stratum_v1()[1], 0);
    assert_eq!(
        frame_tape_v1(&training, &emptied),
        Err(TapeFramingErrorV1::EmptyStratumGroup)
    );

    // Every one of the thirteen tensor fields is bound: perturbing a field
    // other than `action_features` must change the digest.
    let mut other_fixture = tape_fixture_v1();
    other_fixture.tensor.object_card_ids.push(7);
    let perturbed = tape_side_v1(&other_fixture, PREFLIGHT_BASE_SEED_V1);
    assert_ne!(
        tape.sha256_v1(),
        frame_tape_v1(&perturbed, &validation).unwrap().sha256_v1(),
        "a change outside action_features must still change the tape digest"
    );

    // A group with no substeps is malformed.
    let mut no_substeps = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);
    for episode in &mut no_substeps.episodes {
        for group in &mut episode.groups {
            group.substeps.clear();
        }
    }
    assert_eq!(
        frame_tape_v1(&no_substeps, &validation),
        Err(TapeFramingErrorV1::MalformedHierarchy)
    );

    // A selection outside its own substep's action row is malformed.
    let mut bad_selection = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);
    'outer: for episode in &mut bad_selection.episodes {
        for group in &mut episode.groups {
            if let Some(substep) = group.substeps.first_mut() {
                substep.selected_index = 99;
                break 'outer;
            }
        }
    }
    assert_eq!(
        frame_tape_v1(&bad_selection, &validation),
        Err(TapeFramingErrorV1::MalformedHierarchy)
    );

    // A wrong episode count is rejected.
    let mut short = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);
    short.episodes.truncate(63);
    assert_eq!(
        frame_tape_v1(&short, &validation),
        Err(TapeFramingErrorV1::EpisodeCardinality)
    );

    // Strata that disagree with the production schedule are rejected.
    let mut forged = tape_side_v1(&fixture, PREFLIGHT_BASE_SEED_V1);
    for episode in &mut forged.episodes {
        episode.stratum = 0;
    }
    assert_eq!(
        frame_tape_v1(&forged, &validation),
        Err(TapeFramingErrorV1::CountsDisagreeWithSchedule)
    );
}

/// The frozen advantage is a per-group quantity built from its episode's
/// terminal return and its own substep zero, never per substep.
#[test]
fn frozen_advantage_is_per_group_from_episode_return_and_substep_zero_v1() {
    let fixture = tape_fixture_v1();
    let group = TapeGroupV1 {
        physical_decision_id: 1,
        substeps: vec![
            substep_v1(&fixture, 0, 0.25f32),
            substep_v1(&fixture, 1, 0.75f32),
        ],
    };
    // Substep zero's value is the one subtracted; substep one is ignored.
    assert_eq!(group.frozen_advantage_v1(1), Some(1.0f32 - 0.25f32));
    assert_eq!(group.frozen_advantage_v1(-1), Some(-1.0f32 - 0.25f32));
    // An empty group has no derivable advantage.
    let empty = TapeGroupV1 {
        physical_decision_id: 2,
        substeps: Vec::new(),
    };
    assert_eq!(empty.frozen_advantage_v1(1), None);
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn publication_artifacts_fixture_v1() -> PreflightRepeatArtifactsV1 {
    let tape_frame = FramedWriterV1::new_v1(JOINED_TAPE_SCHEMA_V1);
    let mut full_update_frame = FramedWriterV1::new_v1(UPDATE_SCHEMA_V1);
    full_update_frame.text_v1("treatment", TREATMENT_FULL_V1);
    let mut half_update_frame = FramedWriterV1::new_v1(UPDATE_SCHEMA_V1);
    half_update_frame.text_v1("treatment", TREATMENT_HALF_V1);
    let full_update_sha256 = full_update_frame.sha256_v1();
    let half_update_sha256 = half_update_frame.sha256_v1();
    let mut update_pair_frame = FramedWriterV1::new_v1(PREFLIGHT_UPDATE_PAIR_SCHEMA_V1);
    update_pair_frame.text_v1("full_update_sha256", &full_update_sha256);
    update_pair_frame.text_v1("half_update_sha256", &half_update_sha256);
    let digests = PreflightRepeatDigestsV1 {
        tape_sha256: tape_frame.sha256_v1(),
        full_update_sha256,
        half_update_sha256,
        update_pair_sha256: update_pair_frame.sha256_v1(),
    };
    PreflightRepeatArtifactsV1 {
        digests,
        tape_frame,
        full_update_frame,
        half_update_frame,
        update_pair_frame,
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn publication_role_seal_rejects_digest_swap_schema_and_pair_mutations_v1() {
    let valid = publication_artifacts_fixture_v1();
    assert_eq!(
        validate_preflight_artifact_roles_v1(&valid).unwrap(),
        valid.digests
    );

    let mut bad_claim = publication_artifacts_fixture_v1();
    bad_claim.digests.tape_sha256 = "0".repeat(64);
    assert_eq!(
        validate_preflight_artifact_roles_v1(&bad_claim),
        Err(PreflightPublicationValidationErrorV1::ClaimedDigest)
    );

    let mut swapped = publication_artifacts_fixture_v1();
    std::mem::swap(
        &mut swapped.full_update_frame,
        &mut swapped.half_update_frame,
    );
    swapped.digests.full_update_sha256 = swapped.full_update_frame.sha256_v1();
    swapped.digests.half_update_sha256 = swapped.half_update_frame.sha256_v1();
    let mut swapped_pair = FramedWriterV1::new_v1(PREFLIGHT_UPDATE_PAIR_SCHEMA_V1);
    swapped_pair.text_v1("full_update_sha256", &swapped.digests.full_update_sha256);
    swapped_pair.text_v1("half_update_sha256", &swapped.digests.half_update_sha256);
    swapped.digests.update_pair_sha256 = swapped_pair.sha256_v1();
    swapped.update_pair_frame = swapped_pair;
    assert_eq!(
        validate_preflight_artifact_roles_v1(&swapped),
        Err(PreflightPublicationValidationErrorV1::SchemaRole)
    );

    let mut bad_schema = publication_artifacts_fixture_v1();
    bad_schema.full_update_frame = FramedWriterV1::new_v1(TAPE_SCHEMA_V1);
    bad_schema
        .full_update_frame
        .text_v1("treatment", TREATMENT_FULL_V1);
    bad_schema.digests.full_update_sha256 = bad_schema.full_update_frame.sha256_v1();
    assert_eq!(
        validate_preflight_artifact_roles_v1(&bad_schema),
        Err(PreflightPublicationValidationErrorV1::SchemaRole)
    );

    let mut unjoined_tape = publication_artifacts_fixture_v1();
    unjoined_tape.tape_frame = FramedWriterV1::new_v1(TAPE_SCHEMA_V1);
    unjoined_tape.digests.tape_sha256 = unjoined_tape.tape_frame.sha256_v1();
    assert_eq!(
        validate_preflight_artifact_roles_v1(&unjoined_tape),
        Err(PreflightPublicationValidationErrorV1::SchemaRole)
    );

    let mut bad_pair = publication_artifacts_fixture_v1();
    bad_pair.update_pair_frame.text_v1("forged", "pair");
    bad_pair.digests.update_pair_sha256 = bad_pair.update_pair_frame.sha256_v1();
    assert_eq!(
        validate_preflight_artifact_roles_v1(&bad_pair),
        Err(PreflightPublicationValidationErrorV1::PairBinding)
    );
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn cuda_uuid_formatter_is_canonical_and_byte_order_preserving_v1() {
    let raw = [
        0x06u8, 0x42, 0xd3, 0xca, 0xe3, 0xd4, 0xba, 0x16, 0x96, 0xab, 0xc5, 0x61, 0xc6, 0xda, 0x90,
        0xe3,
    ];
    let uuid = cudarc::driver::sys::CUuuid {
        bytes: raw.map(|value| value as core::ffi::c_char),
    };
    assert_eq!(cuda_uuid_text_v1(uuid), PREFLIGHT_GPU_UUID_V1);
}

#[test]
fn manifest_frame_binds_its_declared_authorities_v1() {
    let tape_sha256 = "1".repeat(64);
    let full_update_sha256 = "2".repeat(64);
    let half_update_sha256 = "3".repeat(64);
    let update_pair_sha256 = "4".repeat(64);
    let input = PreflightManifestInputV1 {
        git_commit: "a2ceb9905ff731b816798e9c6f081b599fc8c297",
        git_tree: "9f804456b530d7009fe8c0c2dc64cdc9866c6a7c",
        tracked_tree_sha256: DESIGN_DOCUMENT_SHA256_V1,
        tracked_tree_contract: "tracked tree pinned",
        toolchain: "rustc pinned",
        rustc_executable_sha256: SOURCE_RUN_SHA256_V1,
        linker_path: "link.exe pinned",
        linker_executable_sha256: SOURCE_CHECKPOINT_SHA256_V1,
        nvidia_smi_path: "nvidia-smi.exe pinned",
        nvidia_smi_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1,
        test_executable_sha256: SOURCE_PAYLOAD_SHA256_V1,
        test_executable_byte_len: 123,
        tape_sha256: &tape_sha256,
        full_update_sha256: &full_update_sha256,
        half_update_sha256: &half_update_sha256,
        update_pair_sha256: &update_pair_sha256,
    };
    let manifest = frame_manifest_v1(input).unwrap();
    let mut zero_length_executable = input;
    zero_length_executable.test_executable_byte_len = 0;
    assert_eq!(
        frame_manifest_v1(zero_length_executable),
        Err(ManifestFramingErrorV1::BuildIdentity)
    );
    let mut malformed_tool_digest = input;
    malformed_tool_digest.nvidia_smi_sha256 = "BAD";
    assert_eq!(
        frame_manifest_v1(malformed_tool_digest),
        Err(ManifestFramingErrorV1::BuildIdentity)
    );
    let bytes = manifest.bytes_v1();
    // The manifest binds the design, the diagnostic-only backend identity,
    // the vendored tree object, and the Pool3 authority.
    for needle in [
        DESIGN_DOCUMENT_SHA256_V1,
        DIAGNOSTIC_BACKEND_IDENTITY_V1,
        VENDORED_SIMPLEUNIT_TREE_OBJECT_V1,
        POOL3_DOCUMENT_SHA256_V1,
        SOURCE_RUN_SHA256_V1,
        SOURCE_CHECKPOINT_SHA256_V1,
        SOURCE_PAYLOAD_SHA256_V1,
        SOURCE_MODEL_PARAMETER_SHA256_V1,
        PREFLIGHT_GPU_NAME_V1,
        PREFLIGHT_GPU_UUID_V1,
    ] {
        assert!(
            bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes()),
            "manifest must bind {needle}"
        );
    }
    for forbidden in ["formal_training_seeds", "formal_validation_seeds"] {
        assert!(!bytes
            .windows(forbidden.len())
            .any(|window| window == forbidden.as_bytes()));
    }
    for (label, digest) in [
        ("tape_sha256", tape_sha256.as_str()),
        ("full_update_sha256", full_update_sha256.as_str()),
        ("half_update_sha256", half_update_sha256.as_str()),
        ("update_pair_sha256", update_pair_sha256.as_str()),
    ] {
        assert_eq!(
            exact_framed_atom_occurrences_v1(bytes, label, digest.as_bytes()),
            1,
            "manifest must contain exactly one {label} atom"
        );
    }

    assert_eq!(
        frame_manifest_v1(PreflightManifestInputV1 {
            git_commit: "ABC",
            git_tree: "9f804456b530d7009fe8c0c2dc64cdc9866c6a7c",
            tracked_tree_sha256: DESIGN_DOCUMENT_SHA256_V1,
            tracked_tree_contract: "tracked tree pinned",
            toolchain: "rustc pinned",
            rustc_executable_sha256: SOURCE_RUN_SHA256_V1,
            linker_path: "link.exe pinned",
            linker_executable_sha256: SOURCE_CHECKPOINT_SHA256_V1,
            nvidia_smi_path: "nvidia-smi.exe pinned",
            nvidia_smi_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1,
            test_executable_sha256: SOURCE_PAYLOAD_SHA256_V1,
            test_executable_byte_len: 123,
            tape_sha256: DESIGN_DOCUMENT_SHA256_V1,
            full_update_sha256: SOURCE_RUN_SHA256_V1,
            half_update_sha256: SOURCE_CHECKPOINT_SHA256_V1,
            update_pair_sha256: SOURCE_PAYLOAD_SHA256_V1,
        }),
        Err(ManifestFramingErrorV1::GitObject)
    );
    assert_eq!(
        frame_manifest_v1(PreflightManifestInputV1 {
            git_commit: "a2ceb9905ff731b816798e9c6f081b599fc8c297",
            git_tree: "9f804456b530d7009fe8c0c2dc64cdc9866c6a7c",
            tracked_tree_sha256: DESIGN_DOCUMENT_SHA256_V1,
            tracked_tree_contract: "tracked tree pinned",
            toolchain: "",
            rustc_executable_sha256: SOURCE_RUN_SHA256_V1,
            linker_path: "link.exe pinned",
            linker_executable_sha256: SOURCE_CHECKPOINT_SHA256_V1,
            nvidia_smi_path: "nvidia-smi.exe pinned",
            nvidia_smi_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1,
            test_executable_sha256: SOURCE_PAYLOAD_SHA256_V1,
            test_executable_byte_len: 123,
            tape_sha256: DESIGN_DOCUMENT_SHA256_V1,
            full_update_sha256: SOURCE_RUN_SHA256_V1,
            half_update_sha256: SOURCE_CHECKPOINT_SHA256_V1,
            update_pair_sha256: SOURCE_PAYLOAD_SHA256_V1,
        }),
        Err(ManifestFramingErrorV1::ToolIdentity)
    );
    assert_eq!(
        frame_manifest_v1(PreflightManifestInputV1 {
            git_commit: "a2ceb9905ff731b816798e9c6f081b599fc8c297",
            git_tree: "9f804456b530d7009fe8c0c2dc64cdc9866c6a7c",
            tracked_tree_sha256: DESIGN_DOCUMENT_SHA256_V1,
            tracked_tree_contract: "tracked tree pinned",
            toolchain: "rustc pinned",
            rustc_executable_sha256: SOURCE_RUN_SHA256_V1,
            linker_path: "link.exe pinned",
            linker_executable_sha256: SOURCE_CHECKPOINT_SHA256_V1,
            nvidia_smi_path: "nvidia-smi.exe pinned",
            nvidia_smi_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1,
            test_executable_sha256: SOURCE_PAYLOAD_SHA256_V1,
            test_executable_byte_len: 123,
            tape_sha256: "BAD",
            full_update_sha256: SOURCE_RUN_SHA256_V1,
            half_update_sha256: SOURCE_CHECKPOINT_SHA256_V1,
            update_pair_sha256: SOURCE_PAYLOAD_SHA256_V1,
        }),
        Err(ManifestFramingErrorV1::ArtifactDigest)
    );
}

/// Builds a genuine 33-tensor named stream matching the frozen native
/// name/order/shape manifest exactly.
fn native_stream_fixture_v1(base: f32) -> Vec<NativeNamedParameterV1> {
    native_train_state_parameter_layout_v1()
        .map(|(name, shape)| NativeNamedParameterV1 {
            name,
            shape: shape.to_vec(),
            values: vec![base; shape.iter().product::<usize>()],
        })
        .collect()
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn zero_named_stream_like_v1(parameters: &[NativeNamedParameterV1]) -> Vec<NativeNamedParameterV1> {
    parameters
        .iter()
        .map(|parameter| NativeNamedParameterV1 {
            name: parameter.name,
            shape: parameter.shape.clone(),
            values: vec![0.0; parameter.values.len()],
        })
        .collect()
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn genesis_authority_fixture_v1() -> (
    Promoted2Generation384GenesisV1,
    NativePolicyValueTrainSnapshotV1,
    NativePolicyValueTrainSnapshotV1,
) {
    let full_parameters = native_stream_fixture_v1(0.25);
    let half_parameters = derive_half_parameters_v1(&full_parameters).unwrap();
    let scorer_index = full_parameters
        .iter()
        .position(|parameter| parameter.name == CANONICAL_GAUGE_PARAMETERS_V1[0])
        .unwrap();
    let anchor = full_parameters[scorer_index].values[0].to_bits();
    let full = NativePolicyValueTrainSnapshotV1 {
        adam_step: 0,
        scorer_bias_anchor_bits: anchor,
        first_moments: zero_named_stream_like_v1(&full_parameters),
        second_moments: zero_named_stream_like_v1(&full_parameters),
        parameters: full_parameters.clone(),
    };
    let half = NativePolicyValueTrainSnapshotV1 {
        adam_step: 0,
        scorer_bias_anchor_bits: anchor,
        first_moments: zero_named_stream_like_v1(&half_parameters),
        second_moments: zero_named_stream_like_v1(&half_parameters),
        parameters: half_parameters.clone(),
    };
    (
        Promoted2Generation384GenesisV1 {
            full_parameters,
            half_parameters,
            scorer_bias_anchor_bits: anchor,
            _seal: Promoted2Generation384GenesisSealV1(()),
        },
        full,
        half,
    )
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn loader_sealed_genesis_rejects_consistent_unused_and_state_mutations_v1() {
    let (genesis, full, half) = genesis_authority_fixture_v1();
    assert!(genesis_snapshots_match_authority_v1(&full, &half, &genesis));

    // A coordinated non-target mutation preserves the F/H transform and can
    // be invisible to this tape; the independent source authority rejects it.
    let mut wrong_full = full.clone();
    let mut wrong_half = half.clone();
    wrong_full.parameters[0].values[0] = 0.375;
    wrong_half.parameters[0].values[0] = 0.375;
    assert!(!genesis_snapshots_match_authority_v1(
        &wrong_full,
        &wrong_half,
        &genesis
    ));

    let mut wrong_step = full.clone();
    wrong_step.adam_step = 1;
    assert!(!genesis_snapshots_match_authority_v1(
        &wrong_step,
        &half,
        &genesis
    ));

    let mut negative_zero_moment = full.clone();
    negative_zero_moment.first_moments[0].values[0] = -0.0;
    assert!(!genesis_snapshots_match_authority_v1(
        &negative_zero_moment,
        &half,
        &genesis
    ));

    let mut wrong_anchor = full.clone();
    wrong_anchor.scorer_bias_anchor_bits ^= 1;
    assert!(!genesis_snapshots_match_authority_v1(
        &wrong_anchor,
        &half,
        &genesis
    ));

    let target = validate_single_target_v1(&half.parameters).unwrap();
    for column in [ACTION_HASH_BEGIN_V1, ACTION_HASH_BEGIN_V1 - 1] {
        let mut wrong_half = half.clone();
        wrong_half.parameters[target].values[column] += 0.25;
        assert!(!genesis_snapshots_match_authority_v1(
            &full,
            &wrong_half,
            &genesis
        ));
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn independent_adam_scalar_and_canonical_scorer_bias_have_bit_goldens_v1() {
    let adam = IndependentAdamStepV1::new_v1(1).unwrap();
    let (parameter, first, second) = adam.coordinate_v1(0.25, 0.5, 0.0, 0.0).unwrap();
    assert_eq!(parameter.to_bits(), 0x3e7e_f9db);
    assert_eq!(first.to_bits(), 0x3d4c_ccd0);
    assert_eq!(second.to_bits(), 0x3983_1200);
    let zero = adam.coordinate_v1(0.25, 0.0, 0.0, 0.0).unwrap();
    assert_eq!(
        [zero.0.to_bits(), zero.1.to_bits(), zero.2.to_bits()],
        [0.25f32.to_bits(), 0, 0,]
    );
    assert!(adam.coordinate_v1(f32::NAN, 0.0, 0.0, 0.0).is_none());
    assert!(IndependentAdamStepV1::new_v1(0).is_none());
    assert!(IndependentAdamStepV1::new_v1(u64::MAX).is_none());

    let anchor = 0.25f32.to_bits();
    assert!(canonical_scorer_bias_coordinate_valid_v1(
        0.25, 0.25, 0.0, 0.0, 0.0, anchor
    ));
    for (gradient, first, second) in [(-0.0, 0.0, 0.0), (0.0, -0.0, 0.0), (0.0, 0.0, -0.0)] {
        assert!(!canonical_scorer_bias_coordinate_valid_v1(
            0.25, 0.25, gradient, first, second, anchor
        ));
    }
    assert!(!canonical_scorer_bias_coordinate_valid_v1(
        0.25,
        f32::from_bits(0.25f32.to_bits() + 1),
        0.0,
        0.0,
        0.0,
        anchor,
    ));
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn paired_coordinate_gate_covers_both_rows_and_digest_boundaries_v1() {
    let target = 7usize;
    for row in [0usize, ACTION_ENCODER_ROWS_V1 - 1] {
        for (column, expected) in [
            (ACTION_HASH_BEGIN_V1 - 1, false),
            (ACTION_HASH_BEGIN_V1, true),
            (ACTION_HASH_END_V1 - 1, true),
            (ACTION_HASH_END_V1, false),
        ] {
            assert_eq!(
                action_encoder_digest_coordinate_v1(
                    target,
                    target,
                    row * ACTION_ENCODER_COLUMNS_V1 + column,
                ),
                expected
            );
        }
    }
    assert!(!action_encoder_digest_coordinate_v1(
        target + 1,
        target,
        ACTION_HASH_BEGIN_V1
    ));

    assert!(cross_arm_coordinate_valid_v1(
        true, 0.25, 0.2495, 0.5, 0.5, 0.499, 0.25,
    ));
    assert!(!cross_arm_coordinate_valid_v1(
        true, 0.25, 0.2495, 0.5, 0.5001, 0.4991, 0.25,
    ));
    assert!(!cross_arm_coordinate_valid_v1(
        true, 0.25, 0.2495, 0.5, 0.5, 0.499, 0.3,
    ));
    assert!(!cross_arm_coordinate_valid_v1(
        true, 0.25, 0.2495, 0.5, 0.5, 0.49, 0.25,
    ));

    assert!(cross_arm_coordinate_valid_v1(
        false, 0.25, 0.2495, 0.5, 0.25, 0.2495, 0.5,
    ));
    assert!(!cross_arm_coordinate_valid_v1(
        false, 0.25, 0.2495, 0.5, 0.25, 0.2494, 0.5,
    ));
    assert!(!cross_arm_coordinate_valid_v1(
        false,
        0.25,
        0.2495,
        0.5,
        0.25,
        0.2495,
        f32::NAN,
    ));
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn result_lattice_fixture_v1(
    tape: &JoinedTapeV1,
) -> (
    NativePolicyValueTrainSnapshotV1,
    NativePolicyValueTrainSnapshotV1,
    NativePolicyTrainStepResultV1,
) {
    let parameters = native_stream_fixture_v1(0.25);
    let scorer_index = parameters
        .iter()
        .position(|parameter| parameter.name == CANONICAL_GAUGE_PARAMETERS_V1[0])
        .unwrap();
    let anchor = parameters[scorer_index].values[0].to_bits();
    let before = NativePolicyValueTrainSnapshotV1 {
        adam_step: 0,
        scorer_bias_anchor_bits: anchor,
        first_moments: zero_named_stream_like_v1(&parameters),
        second_moments: zero_named_stream_like_v1(&parameters),
        parameters: parameters.clone(),
    };
    let after = NativePolicyValueTrainSnapshotV1 {
        adam_step: 1,
        scorer_bias_anchor_bits: anchor,
        first_moments: zero_named_stream_like_v1(&parameters),
        second_moments: zero_named_stream_like_v1(&parameters),
        parameters,
    };

    let mut selected_outputs = Vec::new();
    let mut physical_terms = Vec::new();
    let mut substep_bounds = Vec::new();
    let mut policy_sum = 0.0f32;
    let mut value_sum = 0.0f32;
    let mut total_action_count = 0usize;
    let mut max_action_count = 0usize;
    let mut group_index = 0usize;
    for episode in &tape.episodes {
        for group in &episode.groups {
            let mut joint: Option<f32> = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let selected = substep.selected_index as usize;
                let selected_log_probability =
                    f32::from_bits(substep.selected_log_probability_bits);
                joint = Some(match joint {
                    None => selected_log_probability,
                    Some(active) => active + selected_log_probability,
                });
                selected_outputs.push(NativeSelectedOutputV1 {
                    group_index,
                    substep_index,
                    selected_action_index: selected,
                    selected_logit: f32::from_bits(substep.raw_action_logit_bits[selected]),
                    value: f32::from_bits(substep.predicted_value_bits),
                    selected_log_probability,
                });
                let action_count = substep.raw_action_logit_bits.len();
                total_action_count += action_count;
                max_action_count = max_action_count.max(action_count);
                substep_bounds.push(NativeGaugeSubstepBoundV1 {
                    action_count,
                    abs_policy_coefficient: 0.0,
                    gamma_operation_count: 0,
                    gamma: 0.0,
                    bound_component: 0.0,
                });
            }
            let joint = joint.unwrap();
            let value = f32::from_bits(group.substeps[0].predicted_value_bits);
            let terminal_return = episode.authenticated_terminal.learner_return_v1();
            let target = f32::from(terminal_return);
            let advantage = target - value;
            policy_sum += -joint * advantage;
            let value_error = value - target;
            value_sum += value_error * value_error;
            physical_terms.push(NativePhysicalLossTermV1 {
                joint_log_probability: joint,
                value,
                terminal_return,
                substep_count: group.substeps.len() as u32,
            });
            group_index += 1;
        }
    }
    let loss =
        (policy_sum + f32::from_bits(VALUE_COEFFICIENT_BITS_V1) * value_sum) / group_index as f32;
    // The production accumulator appends gauge bounds in backward traversal
    // order, independently of the forward-ordered selected outputs and
    // physical terms above.
    substep_bounds.reverse();
    let result = NativePolicyTrainStepResultV1 {
        policy_sum,
        value_sum,
        loss,
        adam_step: 1,
        selected_outputs,
        physical_terms,
        gradients: zero_named_stream_like_v1(&before.parameters),
        scorer_bias_gauge: NativeScorerBiasGaugeRecordV1 {
            parameter_name: CANONICAL_GAUGE_PARAMETERS_V1[0],
            substep_count: substep_bounds.len(),
            total_action_count,
            max_action_count,
            sum_abs_policy_coefficients: 0.0,
            substep_bounds,
            per_substep_bound_sum: 0.0,
            cross_substep_bound: 0.0,
            raw_gradient_residual: 0.0,
            derived_absolute_bound: 0.0,
            high_precision_residual: 0.0,
            canonical_gradient: 0.0,
            parameter_before_bits: anchor,
            parameter_after_bits: anchor,
        },
    };
    (before, after, result)
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn gauge_bound_cardinalities_follow_heterogeneous_backward_order_v1() {
    let forward_action_counts = [2usize, 5, 3, 7, 4];
    let bound = |action_count| NativeGaugeSubstepBoundV1 {
        action_count,
        abs_policy_coefficient: 0.0,
        gamma_operation_count: 0,
        gamma: 0.0,
        bound_component: 0.0,
    };
    let backward_bounds = forward_action_counts
        .iter()
        .rev()
        .copied()
        .map(bound)
        .collect::<Vec<_>>();
    assert!(gauge_bounds_match_backward_action_counts_v1(
        &backward_bounds,
        &forward_action_counts,
    ));

    let forward_bounds = forward_action_counts
        .iter()
        .copied()
        .map(bound)
        .collect::<Vec<_>>();
    assert!(!gauge_bounds_match_backward_action_counts_v1(
        &forward_bounds,
        &forward_action_counts,
    ));
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn actual_result_lattice_rejects_counts_order_fields_and_malformed_streams_v1() {
    let (authority, deck_ids, deck_hashes, tape) = joined_fixture_v1();
    let validated =
        ValidatedJoinedTapeForUpdateV1::new_v1(&authority, &deck_ids, deck_hashes, &tape).unwrap();
    let (before, after, result) = result_lattice_fixture_v1(&tape);
    assert!(actual_result_matches_tape_v1(
        &result, &before, &after, &validated
    ));

    let mut mutations: Vec<NativePolicyTrainStepResultV1> = Vec::new();
    let mut candidate = result.clone();
    candidate.selected_outputs.pop();
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs.swap(0, 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs[0].group_index += 1;
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs[0].substep_index += 1;
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs[0].selected_action_index ^= 1;
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs[0].selected_logit =
        f32::from_bits(candidate.selected_outputs[0].selected_logit.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs[0].value =
        f32::from_bits(candidate.selected_outputs[0].value.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.selected_outputs[0].selected_log_probability = f32::from_bits(
        candidate.selected_outputs[0]
            .selected_log_probability
            .to_bits()
            ^ 1,
    );
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.physical_terms.pop();
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.physical_terms[0].joint_log_probability =
        f32::from_bits(candidate.physical_terms[0].joint_log_probability.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.physical_terms[0].value =
        f32::from_bits(candidate.physical_terms[0].value.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.physical_terms[0].terminal_return = -candidate.physical_terms[0].terminal_return;
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.physical_terms[0].substep_count += 1;
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.policy_sum = f32::from_bits(candidate.policy_sum.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.value_sum = f32::from_bits(candidate.value_sum.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.loss = f32::from_bits(candidate.loss.to_bits() ^ 1);
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.gradients.pop();
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.scorer_bias_gauge.substep_bounds.pop();
    mutations.push(candidate);
    let mut candidate = result.clone();
    candidate.scorer_bias_gauge.canonical_gradient = -0.0;
    mutations.push(candidate);

    for mutation in mutations {
        assert!(!actual_result_matches_tape_v1(
            &mutation, &before, &after, &validated,
        ));
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[test]
fn independent_adam_full_stream_rejects_parameter_moment_and_shape_mutations_v1() {
    let (_, _, _, tape) = joined_fixture_v1();
    let (before, after, result) = result_lattice_fixture_v1(&tape);
    let evidence = ActualUpdateEvidenceV1 {
        treatment: ActualTreatmentV1::Full,
        deltas: derived_deltas_v1(&before.parameters, &after.parameters),
        frame: FramedWriterV1::new_v1(UPDATE_SCHEMA_V1),
        result,
        before,
        after,
    };
    assert!(actual_update_matches_independent_adam_v1(&evidence));

    let mut wrong_parameter = evidence.after.clone();
    wrong_parameter.parameters[0].values[0] += 0.01;
    let mut candidate = ActualUpdateEvidenceV1 {
        treatment: evidence.treatment,
        result: evidence.result.clone(),
        before: evidence.before.clone(),
        after: wrong_parameter,
        deltas: evidence.deltas.clone(),
        frame: FramedWriterV1::new_v1(UPDATE_SCHEMA_V1),
    };
    assert!(!actual_update_matches_independent_adam_v1(&candidate));

    candidate.after = evidence.after.clone();
    candidate.after.first_moments[0].values[0] = 0.01;
    assert!(!actual_update_matches_independent_adam_v1(&candidate));
    candidate.after = evidence.after.clone();
    candidate.after.second_moments[0].values[0] = 0.01;
    assert!(!actual_update_matches_independent_adam_v1(&candidate));

    candidate.after = evidence.after.clone();
    candidate.result.gradients.pop();
    assert!(!actual_update_matches_independent_adam_v1(&candidate));

    candidate.result = evidence.result.clone();
    let scorer_index = candidate
        .result
        .gradients
        .iter()
        .position(|parameter| parameter.name == CANONICAL_GAUGE_PARAMETERS_V1[0])
        .unwrap();
    candidate.result.gradients[scorer_index].values[0] = -0.0;
    assert!(!actual_update_matches_independent_adam_v1(&candidate));
    candidate.result = evidence.result.clone();
    candidate.after.first_moments[scorer_index].values[0] = -0.0;
    assert!(!actual_update_matches_independent_adam_v1(&candidate));
}

/// Thirty-three copies of one tensor: right cardinality, wrong manifest.
fn duplicate_dummy_stream_fixture_v1() -> Vec<NativeNamedParameterV1> {
    vec![
        NativeNamedParameterV1 {
            name: ACTION_ENCODER_WEIGHT_NAME_V1,
            shape: vec![ACTION_ENCODER_ROWS_V1, ACTION_ENCODER_COLUMNS_V1],
            values: vec![0.5f32; ACTION_ENCODER_VALUE_COUNT_V1],
        };
        NAMED_TENSOR_COUNT_V1
    ]
}

/// Exact per-coordinate deltas of two aligned native streams.
fn derived_deltas_v1(
    before: &[NativeNamedParameterV1],
    after: &[NativeNamedParameterV1],
) -> Vec<f64> {
    before
        .iter()
        .zip(after)
        .flat_map(|(before, after)| {
            before
                .values
                .iter()
                .zip(&after.values)
                .map(|(before_value, after_value)| delta_v1(*before_value, *after_value))
                .collect::<Vec<f64>>()
        })
        .collect()
}

/// The update frame validates cardinality, alignment, treatment label,
/// finiteness, and delta count before it binds a single byte.
#[test]
fn update_frame_validates_manifest_alignment_finiteness_and_derived_deltas_v1() {
    let gradients = native_stream_fixture_v1(0.5);
    let before = native_stream_fixture_v1(1.0);
    let after = native_stream_fixture_v1(1.000_976_6);
    let deltas = derived_deltas_v1(&before, &after);
    let moments = native_stream_fixture_v1(2.0);

    let frame = frame_update_v1(
        TREATMENT_HALF_V1,
        &gradients,
        &before,
        &after,
        &moments,
        &moments,
        &deltas,
    )
    .unwrap();
    assert_eq!(frame.sha256_v1().len(), 64);
    assert_eq!(NAMED_TENSOR_COUNT_V1, 33);
    assert_eq!(native_train_state_parameter_layout_v1().len(), 33);

    // Both legal treatments are accepted; anything else is rejected.
    assert!(frame_update_v1(
        TREATMENT_FULL_V1,
        &gradients,
        &before,
        &after,
        &moments,
        &moments,
        &deltas
    )
    .is_ok());
    assert_eq!(
        frame_update_v1(
            "SOMETHING-ELSE",
            &gradients,
            &before,
            &after,
            &moments,
            &moments,
            &deltas
        ),
        Err(UpdateFramingErrorV1::UnknownTreatment)
    );

    // A stream that is not exactly 33 tensors is rejected.
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients[..32],
            &before,
            &after,
            &moments,
            &moments,
            &deltas
        ),
        Err(UpdateFramingErrorV1::TensorCardinality)
    );

    // Thirty-three duplicated dummy tensors have the right cardinality but
    // are not the native manifest, so they must fail.
    let dummies = duplicate_dummy_stream_fixture_v1();
    assert_eq!(dummies.len(), NAMED_TENSOR_COUNT_V1);
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &dummies,
            &before,
            &after,
            &moments,
            &moments,
            &deltas
        ),
        Err(UpdateFramingErrorV1::NativeManifestMismatch)
    );

    // A wrong shape on an otherwise-correct manifest is rejected.
    let mut wrong_shape = native_stream_fixture_v1(1.0);
    wrong_shape[7].shape = vec![3, 2];
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients,
            &wrong_shape,
            &after,
            &moments,
            &moments,
            &deltas
        ),
        Err(UpdateFramingErrorV1::NativeManifestMismatch)
    );

    // Nonfinite published values are rejected in both f32 and f64 streams.
    let mut nonfinite = native_stream_fixture_v1(1.0);
    nonfinite[3].values[2] = f32::INFINITY;
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients,
            &nonfinite,
            &after,
            &moments,
            &moments,
            &deltas
        ),
        Err(UpdateFramingErrorV1::NonFiniteValue)
    );
    let mut nan_deltas = deltas.clone();
    nan_deltas[0] = f64::NAN;
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients,
            &before,
            &after,
            &moments,
            &moments,
            &nan_deltas
        ),
        Err(UpdateFramingErrorV1::NonFiniteValue)
    );

    // The delta stream must cover every element of every named tensor.
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients,
            &before,
            &after,
            &moments,
            &moments,
            &deltas[..deltas.len() - 1]
        ),
        Err(UpdateFramingErrorV1::DeltaCardinality)
    );

    // A delta that is not the exact subtraction of its own before/after
    // coordinates is rejected, even though it is finite and in range.
    let mut forged_deltas = deltas.clone();
    forged_deltas[5] = 0.0001f64;
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients,
            &before,
            &after,
            &moments,
            &moments,
            &forged_deltas
        ),
        Err(UpdateFramingErrorV1::DeltaNotDerivedFromParameters)
    );

    // A delta beyond the frozen step-one ceiling is rejected.
    let oversized_after = native_stream_fixture_v1(1.5);
    assert_eq!(
        frame_update_v1(
            TREATMENT_HALF_V1,
            &gradients,
            &before,
            &oversized_after,
            &moments,
            &moments,
            &derived_deltas_v1(&before, &oversized_after)
        ),
        Err(UpdateFramingErrorV1::DeltaNotDerivedFromParameters)
    );

    // FULL and HALF must never share a digest for otherwise-equal content.
    assert_ne!(
        frame_update_v1(
            TREATMENT_FULL_V1,
            &gradients,
            &before,
            &after,
            &moments,
            &moments,
            &deltas
        )
        .unwrap()
        .sha256_v1(),
        frame.sha256_v1()
    );
}

/// The bootstrap must reject an overflowing running sum or mean, not only
/// nonfinite inputs, and the read must re-establish its own preconditions.
#[test]
fn bootstrap_rejects_overflow_and_unsorted_or_nonfinite_reads_v1() {
    // Six finite f64::MAX values overflow the running sum long before the
    // division; the result must be None, never a sorted infinity.
    assert_eq!(
        paired_cluster_bootstrap_v1(&[f64::MAX; BOOTSTRAP_UNIT_COUNT_V1]),
        None
    );
    assert_eq!(
        paired_cluster_bootstrap_v1(&[f64::MIN; BOOTSTRAP_UNIT_COUNT_V1]),
        None
    );
    // A mixture that overflows only for some tuples is still rejected.
    let mixed = [f64::MAX, f64::MAX, 0.0, 0.0, 0.0, 0.0];
    assert_eq!(paired_cluster_bootstrap_v1(&mixed), None);
    // Ordinary magnitudes still succeed.
    assert!(paired_cluster_bootstrap_v1(&[1.0e300; BOOTSTRAP_UNIT_COUNT_V1]).is_some());

    // The read is independently callable, so it re-checks length,
    // finiteness, and ascending order.
    let mut sorted = paired_cluster_bootstrap_v1(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
    assert!(bootstrap_read_v1(&sorted).is_some());
    let mut unsorted = sorted.clone();
    unsorted.swap(0, BOOTSTRAP_TUPLE_COUNT_V1 - 1);
    assert_eq!(bootstrap_read_v1(&unsorted), None);
    sorted[10] = f64::NAN;
    assert_eq!(bootstrap_read_v1(&sorted), None);
    assert_eq!(bootstrap_read_v1(&[]), None);
}

/// Scope note: this test covers what the classifier verifier alone can do,
/// which is reject DERIVED-FIELD forgery. It deliberately does not claim to
/// reject a self-consistent replacement of the raw arrays; that is the
/// summary-to-unit binding's job
/// (`summary_binds_published_arrays_to_authoritative_units_v1`).
///
/// The regression here is the derived-`PartialEq` self-inequality bug: an
/// honest `INVALID` result carrying a NaN raw input must verify as genuine.
#[test]
fn classifier_verifier_rejects_derived_field_forgery_including_nan_payloads_v1() {
    let nan_paired = [f64::NAN, 0.11, 0.12, 0.09, 0.13, 0.10];
    let promoted2 = [0.20f64, 0.21, 0.22, 0.19, 0.23, 0.20];
    let honest = classify_v1(&nan_paired, &promoted2, true);
    assert_eq!(honest.disposition, DispositionV1::Invalid);
    assert!(honest.paired_differences[0].is_nan());

    // Derived PartialEq would fail here; the bitwise verifier must not.
    assert!(
        verify_classifier_result_v1(&honest),
        "an honest INVALID result carrying NaN must verify as genuine"
    );

    // Any bit mutation of the NaN payload is rejected, including a
    // different NaN bit pattern that compares "equal" under no ordering.
    let mut mutated_payload = honest.clone();
    mutated_payload.paired_differences[0] =
        f64::from_bits(f64::NAN.to_bits() ^ 0x0000_0000_0000_0001);
    assert!(mutated_payload.paired_differences[0].is_nan());
    assert_ne!(
        mutated_payload.paired_differences[0].to_bits(),
        honest.paired_differences[0].to_bits()
    );
    // The mutated payload is itself an honest INVALID of a DIFFERENT input,
    // so it self-verifies. Self-recomputation cannot tell the two apart --
    // only the summary-to-unit binding can. This is stated explicitly so the
    // limitation is not mistaken for coverage.
    assert!(verify_classifier_result_v1(&mutated_payload));
    assert!(!f64_arrays_bit_equal_v1(
        &mutated_payload.paired_differences,
        &honest.paired_differences
    ));

    // Forging the disposition of a NaN-carrying result is still caught.
    let mut forged = honest.clone();
    forged.disposition = DispositionV1::HalfNominated;
    assert!(!verify_classifier_result_v1(&forged));
    // Forging a sign count is caught.
    let mut forged_count = honest.clone();
    forged_count.positive_paired_count += 1;
    assert!(!verify_classifier_result_v1(&forged_count));
    // Signed zeros are distinguished bitwise.
    let negative_zero = [-0.0f64, 0.11, 0.12, 0.09, 0.13, 0.10];
    let positive_zero = [0.0f64, 0.11, 0.12, 0.09, 0.13, 0.10];
    assert!(!f64_arrays_bit_equal_v1(&negative_zero, &positive_zero));
}

/// A published result whose derived fields were tampered with must be
/// rejected by independent recomputation.
#[test]
fn forged_classifier_results_are_rejected_by_recomputation_v1() {
    let paired = [0.10f64, 0.11, 0.12, 0.09, 0.13, 0.10];
    let promoted2 = [0.20f64, 0.21, 0.22, 0.19, 0.23, 0.20];
    let genuine = classify_v1(&paired, &promoted2, true);
    assert_eq!(genuine.disposition, DispositionV1::HalfNominated);
    assert!(verify_classifier_result_v1(&genuine));

    // Forge the disposition while leaving the inputs alone.
    let mut forged_disposition = genuine.clone();
    forged_disposition.disposition = DispositionV1::NoNomination;
    assert!(!verify_classifier_result_v1(&forged_disposition));

    // Forge a sign count.
    let mut forged_count = genuine.clone();
    forged_count.positive_paired_count = 6000;
    assert!(!verify_classifier_result_v1(&forged_count));

    // Forge a bootstrap read.
    let mut forged_read = genuine.clone();
    forged_read.paired_read = Some(BootstrapReadV1 {
        low_index_value: 99.0,
        high_index_value: 99.0,
    });
    assert!(!verify_classifier_result_v1(&forged_read));

    // Forge gate evidence.
    let mut forged_gate = genuine.clone();
    forged_gate.gates.paired_interval_gate = false;
    assert!(!verify_classifier_result_v1(&forged_gate));

    // A genuine NO-NOMINATION and a genuine INVALID also verify.
    let negative = [-0.10f64, -0.11, -0.12, -0.09, -0.13, -0.10];
    let no_nomination = classify_v1(&paired, &negative, true);
    assert_eq!(no_nomination.disposition, DispositionV1::NoNomination);
    assert!(verify_classifier_result_v1(&no_nomination));
    let invalid = classify_v1(&paired, &promoted2, false);
    assert_eq!(invalid.disposition, DispositionV1::Invalid);
    assert!(verify_classifier_result_v1(&invalid));

    // Gate evidence explains the disposition rather than merely asserting
    // it: a NO-NOMINATION must have at least one failed nomination gate.
    assert!(!no_nomination.gates.all_pass_v1());
    assert!(genuine.gates.all_pass_v1());
}

/// The diagnostic-only backend identity is exactly the design's string and
/// is deliberately NOT the production Store-admitted CUDA identity, which
/// authority #130 leaves unchanged.
#[test]
fn diagnostic_backend_identity_is_distinct_from_the_production_identity_v1() {
    assert!(DIAGNOSTIC_BACKEND_IDENTITY_V1.ends_with("-cubecl-simpleunit-register-f32-v2"));
    assert!(DIAGNOSTIC_BACKEND_IDENTITY_V1
        .starts_with("rust-experimental-native-policy-train-step-v1-cuda-burn-dense-padded-"));
    assert_ne!(
        DIAGNOSTIC_BACKEND_IDENTITY_V1,
        "rust-experimental-native-policy-train-step-v1-cuda-burn-dense-padded-v1"
    );
    assert_eq!(VENDORED_SIMPLEUNIT_TREE_OBJECT_V1.len(), 40);
    assert_eq!(DESIGN_DOCUMENT_SHA256_V1.len(), 64);
    assert_eq!(POOL3_DOCUMENT_SHA256_V1.len(), 64);
}
