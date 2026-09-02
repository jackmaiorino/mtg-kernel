//! Strict JSONL shadow scorer for one exact native checkpoint authority.
//!
//! This bridge deliberately lives inside the library crate. The production
//! scored-rollout packet owner, validator, view constructor, and raw inference
//! output accessors are crate-private so an external binary cannot construct a
//! second, weaker scoring path. The companion binary only selects an explicit
//! checkpoint authority and delegates stdin/stdout here.

use crate::async_flat_scored_rollout_v1::{FlatScoredFamilyCore, NativeLaneScheduleStateV1};
use crate::async_flat_scored_rollout_v2::{FlatScoredFamilyV2, OwnedFlatScoringDecisionV2};
use crate::fast_sampler::{
    FastCategoricalScratch, FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    FAST_CATEGORICAL_SAMPLER_VERSION,
};
use crate::flat_policy_v2::{FlatDecisionBindingV2, FlatScoringDecisionViewV2};
use crate::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
use crate::model_guided_search_authority_v1::{
    authorized_seed_block_v1, ModelGuidedSearchAuthorityV1, ModelGuidedSearchConsumptionModeV1,
};
use crate::model_guided_search_contract_digests_v1::MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1;
use crate::model_guided_search_core_v1::{
    ModelGuidedSearchCoreV1, ModelGuidedSearchDecisionV1,
    ModelGuidedSearchRealForwardValueEvaluatorV1, ModelGuidedSearchSeedHalfV1,
};
use crate::model_guided_search_outcome_v4::{
    lower_hex_sha256_v4, root_statistics_digest_v4, visit_margin_v4, CeilingStatusV4,
    EpisodeCloseReasonV4, ModelGuidedSearchOutcomeWriterV4, ProtocolRequestKindV4,
    SearchDecisionRecordV4, StabilityV4, WallTimeV4, WrapperIdentityV4,
};
use crate::model_guided_search_value_quantization_v1::ModelGuidedSearchValueHeadDomainV1;
use crate::native_checkpoint_inference_v1::{
    NativeCheckpointInferenceOutputV1, NativeCheckpointInferenceV1,
};
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2, NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2,
    NATIVE_FLAT_ACTION_FEATURE_DIM_V2, NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2,
    NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
};
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
use crate::native_ladder_pool_resolution_v1::{
    resolve_ladder_checkpoint_authority_v1, resolve_ladder_pool_v1, stage_ladder_checkpoint_ref_v1,
};
use crate::native_policy_train_step_v1::NativePolicyValueTrainStateV1;
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1,
    NativePolicyValueNetV1,
};
use crate::native_structured_policy_residual_v1::{
    NativeStructuredHistoryEntryV1, CARD_VOCAB_V1, HISTORY_LENGTH_V1,
};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_verified_v1, NativeTrainStatePayloadDigestsV1,
    NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::native_trainer_schedule_v2::OpponentLadderPoolMemberV2;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::native_training_store_run_v2::{
    NativeRunEnvironmentTrajectoryContractV1, OpponentLadderCheckpointRefV1,
    OpponentLadderPoolContractV1, ValidatedTrainRunV2,
};
use crate::rl::{
    parse_strict_json_value, rally_deck_ids, shuffled, ActionSemanticV1, PlayerSeatV1,
};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    RlSessionErrorCode, RlSessionTerminalV1, SessionDeckIdsV1, CANONICAL_RALLY_DECK_ID,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Instant;

/// Elapsed microseconds since `started`, saturating rather than wrapping.
/// DIAGNOSTIC ONLY: no caller compares this against anything that could
/// change a chosen action; see
/// `ShadowScorerServiceV1::model_guided_search_selection_v1`.
fn elapsed_micros_v1(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

pub const CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1: &str = "mtg-kernel-checkpoint-shadow-stdio/v1";
pub const CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1: u32 = 1;
pub const CHECKPOINT_SHADOW_STDIO_PROTOCOL_V2: &str = "mtg-kernel-checkpoint-shadow-stdio/v2";
pub const CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V2: u32 = 2;
pub const CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1: &str =
    "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1";
const CHECKPOINT_SHADOW_PUBLIC_HISTORY_COMMITMENT_V1: &str =
    "mtg-kernel-checkpoint-shadow-public-history-framed-sha256/v1";
pub const CHECKPOINT_SHADOW_MAX_REQUEST_BYTES_V1: usize = 1_048_576;
const BOUNDED_VALUE_SEARCH_MANIFEST_FILENAME_V1: &str = "bounded_value_search.json";
const BOUNDED_VALUE_SEARCH_SCHEMA_V1: &str = "mtg-kernel-qualified-policy-bounded-value-search/v1";
const BOUNDED_VALUE_SEARCH_COMPOSITE_DOMAIN_V1: &[u8] =
    b"mtg-kernel-qualified-policy-bounded-value-search-composite/v1";
const BOUNDED_VALUE_SEARCH_MANIFEST_SHA256_V1: &str =
    "0d883d169fca504e4a413810454565d98cd0e8316cb76e7de4f538187b2865c9";
const BOUNDED_VALUE_SEARCH_POLICY_CANDIDATE_SHA256_V1: &str =
    "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72";
const BOUNDED_VALUE_SEARCH_POLICY_COMPOSITE_SHA256_V1: &str =
    "47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3";
const BOUNDED_VALUE_SEARCH_VALUE_CANDIDATE_SHA256_V1: &str =
    "83d6d2ddb97e96cf5ef4feda525b035bba079d6d1d2f4bc44f4affcf70fd6529";
const BOUNDED_VALUE_SEARCH_VALUE_COMPOSITE_SHA256_V1: &str =
    "6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22";
const FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1: &str = "fixed_native_state.json";
const FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1: &str = "checkpoint.state.f32le";
const FIXED_NATIVE_STATE_SCHEMA_V1: &str = "mtg-kernel-xmage-fixed-native-state/v1";
const FIXED_NATIVE_STATE_ENVIRONMENT_CONTRACT_V1: &str = "environment-randomization-v2";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShadowCandidateSelectorV1 {
    #[default]
    PolicySample,
    OneStepHistoryValueBootstrap,
    CandidateTurnOnlyOneStepBoundedValueBootstrap,
    Depth8HistoryValueBootstrap,
    Depth8BoundedValueTeacherShadow,
    Depth8Cp7OpponentHistoryValueBootstrap,
    /// The test-time-search wrapper
    /// (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md`): bounded PUCT IS-MCTS
    /// over `ModelGuidedSearchCoreV1` with the fixed checkpoint's own
    /// policy prior and value head, overriding the sampled index AFTER the
    /// policy forward so the step-protocol invariant (Java cannot override
    /// the Rust-side choice) is preserved unchanged. Selectable only
    /// through the scorer binary's strict CLI flags, never an environment
    /// variable.
    ModelGuidedSearch,
}

const ONE_STEP_VALUE_MIN_PHYSICAL_DECISION_V1: u64 = 20;
const ONE_STEP_VALUE_MIN_ACTIONS_V1: u32 = 2;
const ONE_STEP_VALUE_MAX_ACTIONS_V1: u32 = 8;
const ONE_STEP_VALUE_OVERRIDE_MARGIN_V1: f32 = 0.25;
const ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1: usize = 4;
const ONE_STEP_VALUE_REDETERMINIZATION_DOMAIN_V1: u64 = 0x6876_616c_7265_6431;
const DEPTH8_VALUE_CONTINUATION_STEPS_V1: usize = 8;
const DEPTH8_VALUE_REDETERMINIZATION_DOMAIN_V1: u64 = 0x6876_6465_7074_6838;
const DEPTH8_CP7_OPPONENT_ACTION_DOMAIN_V1: u64 = 0x6370_376f_7070_7631;
pub const XMAGE_CP7_TEACHER_JSONL_CONTRACT_V1: &str = "mtg-kernel-xmage-cp7-teacher-jsonl/v1";
pub const XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1: u32 = 1;
pub const XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1: &str = "mtg-kernel-xmage-cp7-outcome-jsonl/v1";
pub const XMAGE_CP7_OUTCOME_JSONL_SCHEMA_VERSION_V1: u32 = 1;
pub const XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2: &str = "mtg-kernel-xmage-cp7-outcome-jsonl/v2";
pub const XMAGE_CP7_OUTCOME_JSONL_SCHEMA_VERSION_V2: u32 = 2;
pub const NATIVE_POPULATION_TEACHER_JSONL_CONTRACT_V1: &str =
    "mtg-kernel-native-population-opponent-jsonl/v1";
pub const NATIVE_POPULATION_OUTCOME_JSONL_CONTRACT_V1: &str =
    "mtg-kernel-native-population-outcome-jsonl/v1";

const XMAGE_CP7_TEACHER_SELECTION_SOURCE_V1: &str = "xmage_rally_cp7_mapper";
const XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1: &str = "candidate_checkpoint_policy";
const NATIVE_POPULATION_TEACHER_SELECTION_SOURCE_V1: &str = "native_pool3_ladder_40_20_20_20";

const SOURCE_RUN_SHA256_V1: &str =
    "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae";
const SOURCE_GENERATION_V1: u64 = 384;
const SOURCE_CHECKPOINT_SHA256_V1: &str =
    "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8";
const SOURCE_SIDECAR_SHA256_V1: &str =
    "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb";
const SOURCE_PAYLOAD_SHA256_V1: &str =
    "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99";
const SOURCE_TRAIN_STATE_SHA256_V1: &str =
    "fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8";
const SOURCE_MODEL_PARAMETER_SHA256_V1: &str =
    "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d";
const SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1: &str = "legacy-v1";
const POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1: &str = "environment-randomization-v2";
const SHADOW_RANDOMIZATION_IDENTITY_V1: &str = "legacy_v1";

const PORTABLE_RUN_SHA256_V1: &str =
    "e2972066ee4782f0cb0bb588f5f79e9a2cd0be4620b8b906679f15147dd42c89";
const PORTABLE_GENERATION_V1: u64 = 0;
const PORTABLE_CHECKPOINT_SHA256_V1: &str =
    "31c3d9aca8daaf987d420a5e0791a4dd75d2d35a1e821edc74bfa106ddc6391f";
const PORTABLE_SIDECAR_SHA256_V1: &str =
    "74bebb3eed316062cb1a1537346bcbf54f9f7f75dfdd14c8e10b00905f08502a";
const PORTABLE_PAYLOAD_SHA256_V1: &str =
    "2a0840425ccfd09df56747d016d8fcd6b5bc19bba09b6f8cbcdc4507b7315095";
const PORTABLE_TRAIN_STATE_SHA256_V1: &str =
    "0b35c448201efe92375f48a22201c432d3272a3286fae1440f6e7aa2277b9de5";

const FIXED_MAX_PHYSICAL_DECISIONS_V1: u64 = 1_024;
const FIXED_MAX_POLICY_STEPS_V1: u64 = 2_048;

/// Explicit runtime authority selection. No platform or path fallback exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShadowCheckpointAuthorityV1 {
    /// The original promoted(2), generation-384 complete Store root.
    OriginalPromoted2Generation384Store { root: PathBuf },
    /// One explicitly selected generation from the original promoted(2)
    /// complete Store. The checkpoint digests are staged from the named Store
    /// boundary and then revalidated by the normal chain-proven loader.
    OriginalPromoted2StoreGeneration { root: PathBuf, generation: u64 },
    /// One explicitly selected generation from a population Store. The Store
    /// chain is the authority. Unlike the promoted(2) authority this has no
    /// fixed source-run pin, but still requires the compiled inference and
    /// environment-randomization-v2 contracts.
    PopulationStoreGeneration { root: PathBuf, generation: u64 },
    /// A generation-0, weights-only derivative whose model parameters are
    /// bit-identical to promoted(2) g384. This is portable, but is never
    /// reported as the original checkpoint payload or manifest.
    PortablePromoted2WeightsGenesis { root: PathBuf },
    /// A raw, fully verified policy-CE derivative produced by the narrow CP7
    /// behavior-clone trainer. Existing Store authorities remain unchanged.
    Cp7BehaviorCloneDerivative { root: PathBuf },
    /// A fully verified terminal-return derivative trained from candidate
    /// decisions observed in actual XMage-versus-CP7 games.
    XmageCp7OutcomeDerivative { root: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ShadowCheckpointIdentityV1 {
    pub authority_kind: String,
    pub source_run_sha256: String,
    pub source_generation: u64,
    pub source_checkpoint_sha256: String,
    pub source_sidecar_sha256: String,
    pub source_payload_sha256: String,
    pub source_train_state_sha256: String,
    pub loaded_run_sha256: String,
    pub loaded_generation: u64,
    pub loaded_checkpoint_sha256: String,
    pub loaded_payload_sha256: String,
    pub loaded_train_state_sha256: String,
    pub model_parameter_sha256: String,
    pub environment_trajectory_contract: &'static str,
    pub sampler_identity: &'static str,
    pub sampler_contract_sha256: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShadowScorerStartupErrorKindV1 {
    CheckpointAuthority,
    CheckpointIdentity,
}

impl ShadowScorerStartupErrorKindV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::CheckpointAuthority => "shadow_checkpoint_authority_invalid",
            Self::CheckpointIdentity => "shadow_checkpoint_identity_mismatch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShadowScorerStartupErrorV1 {
    kind: ShadowScorerStartupErrorKindV1,
}

impl ShadowScorerStartupErrorV1 {
    const fn new(kind: ShadowScorerStartupErrorKindV1) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> ShadowScorerStartupErrorKindV1 {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for ShadowScorerStartupErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for ShadowScorerStartupErrorV1 {}

/// `pub(crate)`, not private: the S1 feasibility tooling
/// (`native_tts_s1_corpus_v1`, `native_tts_s1_replay_v1`) loads its
/// checkpoint through [`load_checkpoint_v1`] rather than re-deriving the
/// authority-to-Store resolution, the run-limit check, and the environment
/// contract check in a second place where they could drift. Its own public
/// surface never names this type.
pub(crate) struct LoadedShadowCheckpointV1 {
    pub(crate) inference: NativeCheckpointInferenceV1,
    pub(crate) identity: ShadowCheckpointIdentityV1,
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
}

fn checkpoint_ref_v1(
    authority: &ShadowCheckpointAuthorityV1,
) -> Result<OpponentLadderCheckpointRefV1, ShadowScorerStartupErrorV1> {
    let fixed = |source_run_sha256: &str,
                 generation: u64,
                 checkpoint_sha256: &str,
                 sidecar_sha256: &str,
                 state_sha256: &str| OpponentLadderCheckpointRefV1 {
        source_run_sha256: source_run_sha256.to_owned(),
        generation,
        checkpoint_sha256: checkpoint_sha256.to_owned(),
        sidecar_sha256: sidecar_sha256.to_owned(),
        state_sha256: state_sha256.to_owned(),
    };
    match authority {
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. } => Ok(fixed(
            SOURCE_RUN_SHA256_V1,
            SOURCE_GENERATION_V1,
            SOURCE_CHECKPOINT_SHA256_V1,
            SOURCE_SIDECAR_SHA256_V1,
            SOURCE_PAYLOAD_SHA256_V1,
        )),
        ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { root, generation } => {
            let checkpoint_ref =
                stage_ladder_checkpoint_ref_v1(root, *generation).map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
            if checkpoint_ref.source_run_sha256 != SOURCE_RUN_SHA256_V1 {
                return Err(ShadowScorerStartupErrorV1::new(
                    ShadowScorerStartupErrorKindV1::CheckpointIdentity,
                ));
            }
            Ok(checkpoint_ref)
        }
        ShadowCheckpointAuthorityV1::PopulationStoreGeneration { root, generation } => {
            stage_ladder_checkpoint_ref_v1(root, *generation).map_err(|_| {
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            })
        }
        ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => Ok(fixed(
            PORTABLE_RUN_SHA256_V1,
            PORTABLE_GENERATION_V1,
            PORTABLE_CHECKPOINT_SHA256_V1,
            PORTABLE_SIDECAR_SHA256_V1,
            PORTABLE_PAYLOAD_SHA256_V1,
        )),
        ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { .. }
        | ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { .. } => Err(
            ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointIdentity),
        ),
    }
}

fn authority_root_v1(authority: &ShadowCheckpointAuthorityV1) -> &Path {
    match authority {
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { root }
        | ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { root, .. }
        | ShadowCheckpointAuthorityV1::PopulationStoreGeneration { root, .. }
        | ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { root }
        | ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { root }
        | ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root } => root,
    }
}

fn expected_environment_contract_v1(
    authority: &ShadowCheckpointAuthorityV1,
) -> NativeRunEnvironmentTrajectoryContractV1 {
    match authority {
        ShadowCheckpointAuthorityV1::PopulationStoreGeneration { .. } => {
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        }
        _ => NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
    }
}

fn validate_run_limits_v1(run: &ValidatedTrainRunV2) -> Result<(), ShadowScorerStartupErrorV1> {
    if run.record().limits().max_physical_decisions() != FIXED_MAX_PHYSICAL_DECISIONS_V1
        || run.record().limits().max_policy_steps() != FIXED_MAX_POLICY_STEPS_V1
    {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }
    Ok(())
}

fn require_inference_identity_v1(
    inference: &NativeCheckpointInferenceV1,
    expected_run: &str,
    expected_generation: u64,
    expected_checkpoint: &str,
    expected_payload: &str,
    expected_train_state: &str,
    expected_model_parameter: &str,
) -> Result<(), ShadowScorerStartupErrorV1> {
    if lower_hex_raw32_v1(inference.run_sha256()) != expected_run
        || inference.generation_index() != expected_generation
        || lower_hex_raw32_v1(inference.checkpoint_manifest_sha256()) != expected_checkpoint
        || lower_hex_raw32_v1(inference.checkpoint_payload_sha256()) != expected_payload
        || lower_hex_raw32_v1(inference.train_state_sha256()) != expected_train_state
        || lower_hex_raw32_v1(inference.model_parameter_sha256()) != expected_model_parameter
    {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }
    Ok(())
}

fn require_portable_source_binding_v1(
    run: &ValidatedTrainRunV2,
) -> Result<(), ShadowScorerStartupErrorV1> {
    let Some(initialization) = run
        .record()
        .contracts()
        .opponent_ladder_initialization
        .as_ref()
    else {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    };
    if initialization.source_run_sha256 != SOURCE_RUN_SHA256_V1
        || initialization.generation != SOURCE_GENERATION_V1
        || initialization.checkpoint_sha256 != SOURCE_CHECKPOINT_SHA256_V1
        || initialization.sidecar_sha256 != SOURCE_SIDECAR_SHA256_V1
        || initialization.state_sha256 != SOURCE_PAYLOAD_SHA256_V1
        || initialization.derived_model_parameter_sha256 != SOURCE_MODEL_PARAMETER_SHA256_V1
    {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }
    Ok(())
}

/// `pub(crate)` so the S1 feasibility tooling loads through the identical
/// chokepoint the scorer does: the same authority-to-Store resolution, the
/// same pinned-digest gate, the same run-limit and environment-contract
/// checks, and the same [`ShadowCheckpointIdentityV1`] construction. A
/// second loader would be a second place for those to drift.
pub(crate) fn load_checkpoint_v1(
    requested: ShadowCheckpointAuthorityV1,
) -> Result<LoadedShadowCheckpointV1, ShadowScorerStartupErrorV1> {
    if matches!(
        &requested,
        ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { .. }
            | ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { .. }
    ) {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }
    let checkpoint_ref = checkpoint_ref_v1(&requested)?;
    let authority =
        resolve_ladder_checkpoint_authority_v1(authority_root_v1(&requested), &checkpoint_ref)
            .map_err(|_error| {
                #[cfg(test)]
                eprintln!("shadow checkpoint authority resolution failed: {_error:?}");
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            })?;
    validate_run_limits_v1(authority.run())?;
    let expected_environment_contract = expected_environment_contract_v1(&requested);
    if authority.run().environment_trajectory_contract_v1() != expected_environment_contract {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }

    let (authority_kind, pinned_train_state, pinned_model_parameter) = match &requested {
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. } => (
            "original-promoted2-generation384-store",
            Some(SOURCE_TRAIN_STATE_SHA256_V1),
            Some(SOURCE_MODEL_PARAMETER_SHA256_V1),
        ),
        ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { .. } => {
            ("original-promoted2-validated-store-generation", None, None)
        }
        ShadowCheckpointAuthorityV1::PopulationStoreGeneration { .. } => {
            ("population-store-validated-generation", None, None)
        }
        ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => {
            require_portable_source_binding_v1(authority.run())?;
            (
                "portable-promoted2-weights-generation0",
                Some(PORTABLE_TRAIN_STATE_SHA256_V1),
                Some(SOURCE_MODEL_PARAMETER_SHA256_V1),
            )
        }
        ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { .. }
        | ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { .. } => unreachable!(),
    };
    // Dynamic identity expectations come from the already chain-validated
    // manifest, while the two legacy authorities retain their additional
    // hardcoded train-state and model-parameter pins.
    let expected_train_state = lower_hex_raw32_v1(authority.checkpoint().train_state_sha256());
    let expected_model_parameter =
        lower_hex_raw32_v1(authority.checkpoint().model_parameter_sha256());
    if pinned_train_state
        .map(|expected| expected_train_state != expected)
        .unwrap_or(false)
        || pinned_model_parameter
            .map(|expected| expected_model_parameter != expected)
            .unwrap_or(false)
    {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }
    // This load is the strict model/tensorizer validation chokepoint. It
    // rejects a run whose architecture, feature encoding, tensorizer
    // contract, parameter layout, or checkpoint payload bindings drift from
    // the compiled scorer, including for an explicitly selected generation.
    let inference = authority.load_handle_v1().map_err(|_| {
        ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
    })?;
    require_inference_identity_v1(
        &inference,
        &checkpoint_ref.source_run_sha256,
        checkpoint_ref.generation,
        &checkpoint_ref.checkpoint_sha256,
        &checkpoint_ref.state_sha256,
        &expected_train_state,
        &expected_model_parameter,
    )?;
    let (
        source_run_sha256,
        source_generation,
        source_checkpoint_sha256,
        source_sidecar_sha256,
        source_payload_sha256,
        source_train_state_sha256,
    ) = match &requested {
        ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => (
            SOURCE_RUN_SHA256_V1.to_owned(),
            SOURCE_GENERATION_V1,
            SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
            SOURCE_SIDECAR_SHA256_V1.to_owned(),
            SOURCE_PAYLOAD_SHA256_V1.to_owned(),
            SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
        ),
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. }
        | ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { .. }
        | ShadowCheckpointAuthorityV1::PopulationStoreGeneration { .. } => (
            checkpoint_ref.source_run_sha256.clone(),
            checkpoint_ref.generation,
            checkpoint_ref.checkpoint_sha256.clone(),
            checkpoint_ref.sidecar_sha256.clone(),
            checkpoint_ref.state_sha256.clone(),
            expected_train_state.clone(),
        ),
        ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { .. }
        | ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { .. } => unreachable!(),
    };
    let identity = ShadowCheckpointIdentityV1 {
        authority_kind: authority_kind.to_owned(),
        source_run_sha256,
        source_generation,
        source_checkpoint_sha256,
        source_sidecar_sha256,
        source_payload_sha256,
        source_train_state_sha256,
        loaded_run_sha256: lower_hex_raw32_v1(inference.run_sha256()),
        loaded_generation: inference.generation_index(),
        loaded_checkpoint_sha256: lower_hex_raw32_v1(inference.checkpoint_manifest_sha256()),
        loaded_payload_sha256: lower_hex_raw32_v1(inference.checkpoint_payload_sha256()),
        loaded_train_state_sha256: lower_hex_raw32_v1(inference.train_state_sha256()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256()),
        environment_trajectory_contract: match &requested {
            ShadowCheckpointAuthorityV1::PopulationStoreGeneration { .. } => {
                POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1
            }
            _ => SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
        },
        sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    };
    Ok(LoadedShadowCheckpointV1 {
        inference,
        identity,
        max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
        max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
    })
}

#[derive(Debug)]
struct ShadowModelOutputV1 {
    logits: Vec<f32>,
    value: f32,
    structured_parent_logits: Option<Vec<f32>>,
    structured_parent_value: Option<f32>,
}

/// The narrow search-capable model seam (test-time-search sketch V2
/// Section 5, S0: "a narrow search-capable model interface that retains
/// the typed native net for the searcher while the existing `dyn` scorer
/// keeps flat scoring").
///
/// Exactly one method, returning exactly the one thing the searcher cannot
/// get through [`ShadowModelScorerV1`]: the typed
/// `NativePolicyValueNetV1`, whose `forward_search_deterministic_v1` (the
/// MXCSR-gated, kernel-tanh forward the whole determinism argument rests
/// on) has no flat-scoring equivalent. Everything else the searcher needs
/// -- encoding, tensorizing, quantization, tree mechanics -- already
/// exists behind `ModelGuidedSearchRealForwardValueEvaluatorV1` and is not
/// re-plumbed here.
///
/// Deliberately NOT a supertrait of `ShadowModelScorerV1` and not a new
/// required method on it: the flat scoring path must be unchanged, and
/// making every scorer implement this would force the recurrent and
/// test-only scorers (which have no native net at all) to either fabricate
/// one or panic.
trait ShadowSearchCapableModelV1 {
    fn search_native_net_v1(&self) -> &NativePolicyValueNetV1;
}

trait ShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        false
    }

    /// Search capability, if this scorer has it. The default is `None`, so
    /// every existing scorer keeps flat scoring with no behavior change of
    /// any kind: the `PolicySample` path never calls this, and a scorer
    /// that does not override it simply cannot be selected for search
    /// (the selector fails closed at configuration time rather than
    /// silently degrading to something else).
    fn search_capable_v1(&self) -> Option<&dyn ShadowSearchCapableModelV1> {
        None
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
        substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()>;
}

struct NativeShadowModelScorerV1 {
    inference: NativeCheckpointInferenceV1,
}

impl ShadowSearchCapableModelV1 for NativeShadowModelScorerV1 {
    fn search_native_net_v1(&self) -> &NativePolicyValueNetV1 {
        self.inference.search_model_v1()
    }
}

impl ShadowModelScorerV1 for NativeShadowModelScorerV1 {
    fn search_capable_v1(&self) -> Option<&dyn ShadowSearchCapableModelV1> {
        Some(self)
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        _history: &[NativeStructuredHistoryEntryV1],
        _acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output: NativeCheckpointInferenceOutputV1 =
            self.inference.score_decision_v1(decision).map_err(|_| ())?;
        Ok(ShadowModelOutputV1 {
            logits: output.action_logits().to_vec(),
            value: output.value(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FixedNativeStatePayloadManifestV1 {
    filename: String,
    byte_count: usize,
    adam_step: u64,
    scorer_bias_anchor_f32_bits: u32,
    payload_sha256: String,
    parameters_sha256: String,
    first_moments_sha256: String,
    second_moments_sha256: String,
    model_parameter_sha256: String,
    native_state_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FixedNativeStateManifestV1 {
    schema: String,
    authority_kind: String,
    source_result_sha256: String,
    payload: FixedNativeStatePayloadManifestV1,
    non_claims: Vec<String>,
}

fn fixed_native_authority_kind_is_valid_v1(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 96
        && bytes.iter().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || *byte == b'-'
                || (*byte == b'.'
                    && index > 0
                    && index + 1 < bytes.len()
                    && bytes[index - 1].is_ascii_digit()
                    && bytes[index + 1].is_ascii_digit())
        })
}

struct FixedNativeStateShadowModelScorerV1 {
    state: NativePolicyValueTrainStateV1,
}

impl ShadowSearchCapableModelV1 for FixedNativeStateShadowModelScorerV1 {
    fn search_native_net_v1(&self) -> &NativePolicyValueNetV1 {
        self.state.model_v1()
    }
}

impl ShadowModelScorerV1 for FixedNativeStateShadowModelScorerV1 {
    fn search_capable_v1(&self) -> Option<&dyn ShadowSearchCapableModelV1> {
        Some(self)
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        _history: &[NativeStructuredHistoryEntryV1],
        _acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
        let encoded = NativeEncodedDecisionViewV1::from_slices_unvalidated(
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
        );
        let output = self.state.model_v1().forward_v1(encoded).map_err(|_| ())?;
        if output.logits.len() != decision.actions().len()
            || output.logits.is_empty()
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err(());
        }
        Ok(ShadowModelOutputV1 {
            logits: output.logits,
            value: output.value,
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

struct LoadedFixedNativeStateV1 {
    scorer: FixedNativeStateShadowModelScorerV1,
    authority_kind: String,
    source_result_sha256: [u8; 32],
    manifest_sha256: [u8; 32],
    payload_sha256: [u8; 32],
    native_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    adam_step: u64,
}

pub(crate) struct FixedNativeTrainingSourceV1 {
    pub(crate) state: NativePolicyValueTrainStateV1,
    pub(crate) authority_kind: String,
    pub(crate) source_result_sha256: [u8; 32],
    pub(crate) manifest_sha256: [u8; 32],
    pub(crate) payload_sha256: [u8; 32],
    pub(crate) native_state_sha256: [u8; 32],
    pub(crate) model_parameter_sha256: [u8; 32],
    pub(crate) adam_step: u64,
}

fn load_fixed_native_state_v1(
    root: &Path,
) -> Result<LoadedFixedNativeStateV1, ShadowScorerStartupErrorV1> {
    let authority_error =
        || ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority);
    let identity_error =
        || ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointIdentity);
    let inventory = fs::read_dir(root)
        .map_err(|_| authority_error())?
        .map(|entry| {
            let entry = entry.map_err(|_| ())?;
            let file_type = entry.file_type().map_err(|_| ())?;
            let name = entry.file_name().into_string().map_err(|_| ())?;
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(());
            }
            Ok(name)
        })
        .collect::<Result<std::collections::BTreeSet<_>, ()>>()
        .map_err(|_| authority_error())?;
    if inventory
        != std::collections::BTreeSet::from([
            FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1.to_owned(),
            FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1.to_owned(),
        ])
    {
        return Err(authority_error());
    }
    let manifest_bytes = fs::read(root.join(FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1))
        .map_err(|_| authority_error())?;
    if !manifest_bytes.ends_with(b"\n") || manifest_bytes.contains(&b'\r') {
        return Err(authority_error());
    }
    let manifest_text = std::str::from_utf8(&manifest_bytes).map_err(|_| authority_error())?;
    let manifest_value = parse_strict_json_value(manifest_text).map_err(|_| authority_error())?;
    let manifest: FixedNativeStateManifestV1 =
        serde_json::from_value(manifest_value).map_err(|_| authority_error())?;
    let mut canonical = serde_json::to_vec_pretty(&manifest).map_err(|_| authority_error())?;
    canonical.push(b'\n');
    if canonical != manifest_bytes
        || manifest.schema != FIXED_NATIVE_STATE_SCHEMA_V1
        || !fixed_native_authority_kind_is_valid_v1(&manifest.authority_kind)
        || manifest.payload.filename != FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1
        || manifest.payload.byte_count != NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1
        || manifest.non_claims
            != [
                "external software anchor is not professional-level evidence".to_owned(),
                "terminal win/loss/draw is the only playing-strength outcome".to_owned(),
            ]
    {
        return Err(identity_error());
    }
    let source_result_sha256 =
        parse_lower_hex_raw32_v1(&manifest.source_result_sha256).map_err(|_| identity_error())?;
    let payload_sha256 =
        parse_lower_hex_raw32_v1(&manifest.payload.payload_sha256).map_err(|_| identity_error())?;
    let expected = NativeTrainStatePayloadDigestsV1 {
        payload_sha256,
        parameters_sha256: parse_lower_hex_raw32_v1(&manifest.payload.parameters_sha256)
            .map_err(|_| identity_error())?,
        first_moments_sha256: parse_lower_hex_raw32_v1(&manifest.payload.first_moments_sha256)
            .map_err(|_| identity_error())?,
        second_moments_sha256: parse_lower_hex_raw32_v1(&manifest.payload.second_moments_sha256)
            .map_err(|_| identity_error())?,
        model_parameter_sha256: parse_lower_hex_raw32_v1(&manifest.payload.model_parameter_sha256)
            .map_err(|_| identity_error())?,
        native_state_sha256: parse_lower_hex_raw32_v1(&manifest.payload.native_state_sha256)
            .map_err(|_| identity_error())?,
    };
    let payload = fs::read(root.join(FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1))
        .map_err(|_| authority_error())?;
    if payload.len() != manifest.payload.byte_count || sha256_v1(&payload) != payload_sha256 {
        return Err(identity_error());
    }
    let decoded = decode_native_train_state_payload_verified_v1(
        &payload,
        manifest.payload.adam_step,
        manifest.payload.scorer_bias_anchor_f32_bits,
        &expected,
    )
    .map_err(|_| identity_error())?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|_| identity_error())?;
    model
        .replace_parameter_snapshot_v1(&decoded.snapshot.parameters)
        .map_err(|_| identity_error())?;
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(model, &decoded.snapshot)
        .map_err(|_| identity_error())?;
    if state.adam_step_v1() != manifest.payload.adam_step
        || state.model_v1().parameter_manifest_sha256_v1()
            != manifest.payload.model_parameter_sha256
        || state.state_sha256_v1().map_err(|_| identity_error())? != expected.native_state_sha256
    {
        return Err(identity_error());
    }
    Ok(LoadedFixedNativeStateV1 {
        scorer: FixedNativeStateShadowModelScorerV1 { state },
        authority_kind: manifest.authority_kind,
        source_result_sha256,
        manifest_sha256: sha256_v1(&manifest_bytes),
        payload_sha256,
        native_state_sha256: expected.native_state_sha256,
        model_parameter_sha256: expected.model_parameter_sha256,
        adam_step: manifest.payload.adam_step,
    })
}

pub(crate) fn load_fixed_native_training_source_v1(
    root: &Path,
) -> Result<FixedNativeTrainingSourceV1, ShadowScorerStartupErrorV1> {
    let loaded = load_fixed_native_state_v1(root)?;
    Ok(FixedNativeTrainingSourceV1 {
        state: loaded.scorer.state,
        authority_kind: loaded.authority_kind,
        source_result_sha256: loaded.source_result_sha256,
        manifest_sha256: loaded.manifest_sha256,
        payload_sha256: loaded.payload_sha256,
        native_state_sha256: loaded.native_state_sha256,
        model_parameter_sha256: loaded.model_parameter_sha256,
        adam_step: loaded.adam_step,
    })
}

#[cfg(test)]
mod fixed_native_state_tests_v1 {
    use super::*;
    use crate::native_train_state_payload_v1::encode_native_train_state_payload_v1;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn fixed_native_state_package_loads_dotted_authority_and_rejects_payload_tamper_v1() {
        assert!(fixed_native_authority_kind_is_valid_v1(
            "current-net8-cp7-terminal-response-v4-kl-0.3-arm-candidate"
        ));
        assert!(!fixed_native_authority_kind_is_valid_v1(
            "current-net8-cp7-terminal-response-v4-kl-..-arm-candidate"
        ));
        assert!(!fixed_native_authority_kind_is_valid_v1(
            "current-net8-cp7-terminal-response-v4-kl-.3-arm-candidate"
        ));
        assert!(!fixed_native_authority_kind_is_valid_v1(
            "current-net8-cp7-terminal-response-v4-kl-0.3."
        ));
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let snapshot = state.snapshot_v1().unwrap();
        let encoded = encode_native_train_state_payload_v1(&snapshot).unwrap();
        let mut manifest = FixedNativeStateManifestV1 {
            schema: FIXED_NATIVE_STATE_SCHEMA_V1.to_owned(),
            authority_kind: "current-net8-cp7-terminal-response-v4-kl-0.3-arm-candidate".to_owned(),
            source_result_sha256: lower_hex_raw32_v1([7; 32]),
            payload: FixedNativeStatePayloadManifestV1 {
                filename: FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1.to_owned(),
                byte_count: encoded.bytes.len(),
                adam_step: state.adam_step_v1(),
                scorer_bias_anchor_f32_bits: state.scorer_bias_anchor_f32_bits_v1(),
                payload_sha256: lower_hex_raw32_v1(encoded.digests.payload_sha256),
                parameters_sha256: lower_hex_raw32_v1(encoded.digests.parameters_sha256),
                first_moments_sha256: lower_hex_raw32_v1(encoded.digests.first_moments_sha256),
                second_moments_sha256: lower_hex_raw32_v1(encoded.digests.second_moments_sha256),
                model_parameter_sha256: lower_hex_raw32_v1(encoded.digests.model_parameter_sha256),
                native_state_sha256: lower_hex_raw32_v1(encoded.digests.native_state_sha256),
            },
            non_claims: vec![
                "external software anchor is not professional-level evidence".to_owned(),
                "terminal win/loss/draw is the only playing-strength outcome".to_owned(),
            ],
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-fixed-native-state-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        manifest_bytes.push(b'\n');
        fs::write(
            root.join(FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1),
            manifest_bytes,
        )
        .unwrap();
        fs::write(
            root.join(FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1),
            &encoded.bytes,
        )
        .unwrap();

        let loaded = load_fixed_native_state_v1(&root).unwrap();
        assert_eq!(
            loaded.authority_kind,
            "current-net8-cp7-terminal-response-v4-kl-0.3-arm-candidate"
        );
        assert_eq!(loaded.adam_step, state.adam_step_v1());
        assert_eq!(
            loaded.native_state_sha256,
            encoded.digests.native_state_sha256
        );
        assert_eq!(
            loaded.model_parameter_sha256,
            encoded.digests.model_parameter_sha256
        );
        manifest.authority_kind = "current-net8-gae8-v1".to_owned();
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        manifest_bytes.push(b'\n');
        fs::write(
            root.join(FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1),
            manifest_bytes,
        )
        .unwrap();
        let service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root: root.clone() },
        )
        .unwrap();
        assert_eq!(service.identity.source_run_sha256, SOURCE_RUN_SHA256_V1);
        assert_eq!(service.identity.source_generation, SOURCE_GENERATION_V1);
        assert_eq!(
            service.identity.source_checkpoint_sha256,
            SOURCE_CHECKPOINT_SHA256_V1
        );
        assert_eq!(
            service.identity.loaded_train_state_sha256,
            lower_hex_raw32_v1(encoded.digests.native_state_sha256)
        );
        assert_eq!(service.identity.loaded_generation, state.adam_step_v1());
        let mut export_identity = service.identity.clone();
        export_identity.loaded_generation = 1;
        XmageCp7OutcomeJsonlWriterV1::from_writer_v1(Box::new(io::sink()), &export_identity)
            .unwrap();

        let mut tampered = encoded.bytes.clone();
        tampered[0] ^= 1;
        fs::write(root.join(FIXED_NATIVE_STATE_PAYLOAD_FILENAME_V1), tampered).unwrap();
        assert!(load_fixed_native_state_v1(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ScoredCurrentDecisionV1 {
    expected: FastActorDecisionV1,
    binding: FlatDecisionBindingV2,
    tensor: NativeFlatDecisionTensorV2,
    action_semantics: Vec<ActionSemanticV1>,
    logits_f32_bits: Vec<u32>,
    value_f32_bits: u32,
    structured_parent_logits_f32_bits: Option<Vec<u32>>,
    structured_parent_value_f32_bits: Option<u32>,
    model_input_sha256: String,
    diagnostic_state_hash_u64_hex: String,
    core_environment_hash_u64_hex: String,
    actor_physical_decision_ordinal: u64,
    candidate_action_seed_u64_hex: Option<String>,
    selected_action_index: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Depth8BranchValueV1 {
    value: f32,
    reached_natural_terminal: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Depth8TeacherDiagnosticV1 {
    schema: &'static str,
    pair_index: u64,
    episode_id: u64,
    step: u64,
    environment_revision: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    actor_physical_decision_ordinal: u64,
    candidate_seat: PlayerSeatV1,
    legal_action_count: u32,
    continuation_steps: usize,
    information_set_samples: usize,
    information_set_sample_hashes_u64_hex: Vec<String>,
    fallback_selected_index: u32,
    best_search_index: u32,
    teacher_selected_index: u32,
    teacher_differs_from_fallback: bool,
    search_margin_f32_bits_hex: String,
    action_values_f32_bits_hex: Vec<String>,
    branch_count: usize,
    natural_terminal_branch_count: usize,
    critic_bootstrap_branch_count: usize,
    candidate_action_seed_u64_hex: Option<String>,
    candidate_order_commitment_128_hex: String,
    model_input_sha256: String,
    public_history_sha256: String,
    diagnostic_state_hash_u64_hex: String,
    core_environment_hash_u64_hex: String,
    action_semantics: Vec<ActionSemanticV1>,
}

pub(crate) fn player_seat_index_v1(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PendingStructuredHistoryEntryV1 {
    physical_decision_id: u64,
    acting_player: PlayerSeatV1,
    substep_count: u32,
    observed_substeps: u32,
    action_sum: [f32; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
    public_card_sum: [f32; CARD_VOCAB_V1],
    public_card_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct StructuredHistoryStateV1 {
    completed: Vec<NativeStructuredHistoryEntryV1>,
    pending: Option<PendingStructuredHistoryEntryV1>,
}

impl StructuredHistoryStateV1 {
    fn accept_selected_v1(
        &mut self,
        scored: &ScoredCurrentDecisionV1,
        selected_index: u32,
    ) -> Result<(), ()> {
        let expected = scored.expected;
        if expected.substep_count == 0 || expected.substep_index >= expected.substep_count {
            return Err(());
        }
        let selected = usize::try_from(selected_index).map_err(|_| ())?;
        let action_start = selected
            .checked_mul(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
            .ok_or(())?;
        let action = scored
            .tensor
            .action_features
            .get(
                action_start
                    ..action_start
                        .checked_add(NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2)
                        .ok_or(())?,
            )
            .ok_or(())?;
        if self.pending.is_none() {
            if expected.substep_index != 0 {
                return Err(());
            }
            self.pending = Some(PendingStructuredHistoryEntryV1 {
                physical_decision_id: expected.physical_decision_id,
                acting_player: expected.acting_player,
                substep_count: expected.substep_count,
                observed_substeps: 0,
                action_sum: [0.0; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
                public_card_sum: [0.0; CARD_VOCAB_V1],
                public_card_count: 0,
            });
        }
        let pending = self.pending.as_mut().ok_or(())?;
        if pending.physical_decision_id != expected.physical_decision_id
            || pending.acting_player != expected.acting_player
            || pending.substep_count != expected.substep_count
            || pending.observed_substeps != expected.substep_index
        {
            return Err(());
        }
        for (sum, value) in pending.action_sum.iter_mut().zip(action) {
            *sum += value;
        }
        let selected_i64 = i64::try_from(selected).map_err(|_| ())?;
        for (action_index, card_id) in scored
            .tensor
            .action_ref_action_indices
            .iter()
            .zip(&scored.tensor.action_ref_card_ids)
        {
            if *action_index == selected_i64 {
                let card = usize::try_from(*card_id).map_err(|_| ())? % CARD_VOCAB_V1;
                pending.public_card_sum[card] += 1.0;
                pending.public_card_count += 1;
            }
        }
        pending.observed_substeps += 1;
        if pending.observed_substeps == pending.substep_count {
            let mut completed = self.pending.take().ok_or(())?;
            let action_denominator = completed.substep_count as f32;
            for value in &mut completed.action_sum {
                *value /= action_denominator;
            }
            if completed.public_card_count > 0 {
                let card_denominator = completed.public_card_count as f32;
                for value in &mut completed.public_card_sum {
                    *value /= card_denominator;
                }
            }
            self.completed.push(NativeStructuredHistoryEntryV1::new_v1(
                player_seat_index_v1(completed.acting_player),
                completed.action_sum,
                completed.public_card_sum,
            )?);
            if self.completed.len() > HISTORY_LENGTH_V1 {
                self.completed.remove(0);
            }
        }
        Ok(())
    }
}

struct ActiveShadowSessionV1 {
    session: FastActorSessionV1,
    schedule: NativeLaneScheduleStateV1,
    candidate_seat: PlayerSeatV1,
    population_opponent_member: Option<OpponentLadderPoolMemberV2>,
    deck_ids: SessionDeckIdsV1,
    base_seed: u64,
    pair_index: u64,
    pair_environment_seed: u64,
    initial_library_card_definition_ids: [Vec<u16>; 2],
    current: Option<ScoredCurrentDecisionV1>,
    structured_history: StructuredHistoryStateV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct XmageCp7TeacherTensorV1 {
    state_f32_bits: Vec<u32>,
    object_features_f32_bits: Vec<u32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features_f32_bits: Vec<u32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features_f32_bits: Vec<u32>,
    action_ref_features_f32_bits: Vec<u32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

impl XmageCp7TeacherTensorV1 {
    fn from_native_v1(tensor: &NativeFlatDecisionTensorV2) -> Self {
        Self {
            state_f32_bits: f32_bits_v1(&tensor.state),
            object_features_f32_bits: f32_bits_v1(&tensor.object_features),
            object_card_ids: tensor.object_card_ids.clone(),
            object_groups: tensor.object_groups.clone(),
            object_node_ids: tensor.object_node_ids.clone(),
            edge_features_f32_bits: f32_bits_v1(&tensor.edge_features),
            edge_source_indices: tensor.edge_source_indices.clone(),
            edge_target_indices: tensor.edge_target_indices.clone(),
            action_features_f32_bits: f32_bits_v1(&tensor.action_features),
            action_ref_features_f32_bits: f32_bits_v1(&tensor.action_ref_features),
            action_ref_card_ids: tensor.action_ref_card_ids.clone(),
            action_ref_action_indices: tensor.action_ref_action_indices.clone(),
            action_ref_node_indices: tensor.action_ref_node_indices.clone(),
        }
    }
}

const RECURRENT_CP7_PACKAGE_SCHEMA_V1: &str = "mtg-kernel-recurrent-cp7-deployment/v1";
const RECURRENT_CP7_MANIFEST_FILENAME_V1: &str = "recurrent_cp7_deployment.json";
const RECURRENT_CP7_REQUEST_SCHEMA_V1: &str = "mtg-kernel-recurrent-cp7-inference-request/v1";
const RECURRENT_CP7_RESPONSE_SCHEMA_V1: &str = "mtg-kernel-recurrent-cp7-inference-response/v1";
const RECURRENT_CP7_READY_SCHEMA_V1: &str = "mtg-kernel-recurrent-cp7-inference-ready/v1";
const RECURRENT_CP7_COMPOSITE_DOMAIN_V1: &[u8] =
    b"mtg-kernel-recurrent-cp7-deployment-composite/v1\0";
const RECURRENT_TERMINAL_PACKAGE_SCHEMA_V1: &str = "mtg-kernel-recurrent-terminal-deployment/v1";
const RECURRENT_TERMINAL_COMPOSITE_DOMAIN_V1: &[u8] =
    b"mtg-kernel-recurrent-terminal-deployment-composite/v1\0";
const RECURRENT_CP7_MAX_WORKER_LINE_BYTES_V1: usize = 4 * 1_048_576;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7FileV1 {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7FilesV1 {
    model: RecurrentCp7FileV1,
    model_definition: RecurrentCp7FileV1,
    worker: RecurrentCp7FileV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7ParentV1 {
    path: String,
    adam_step: u64,
    candidate_sha256: String,
    weights_sha256: String,
    report_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7IdentityV1 {
    authority_kind: String,
    model_parameter_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7SourceV1 {
    full_refit_report_sha256: String,
    deployment_calibration_report_sha256: String,
    terminal_training_report_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7PackageV1 {
    schema: String,
    architecture: String,
    git_commit: String,
    deployment_scale: f64,
    log_ratio_budget: f64,
    model_state_sha256: String,
    files: RecurrentCp7FilesV1,
    parent: RecurrentCp7ParentV1,
    identity: RecurrentCp7IdentityV1,
    source: RecurrentCp7SourceV1,
    non_claims: Vec<String>,
}

#[derive(Serialize)]
struct RecurrentCp7RequestV1 {
    schema: &'static str,
    sequence: u64,
    acting_player: u8,
    substep_count: u32,
    tensor: XmageCp7TeacherTensorV1,
    history_f32_bits: Vec<u32>,
    parent_logits_f32_bits: Vec<u32>,
    parent_value_f32_bits: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7ResponseV1 {
    schema: String,
    sequence: u64,
    logits_f32_bits: Vec<u32>,
    projection_scale: f32,
    maximum_absolute_log_ratio: f32,
    value_f32_bits: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurrentCp7ReadyV1 {
    schema: String,
    model_file_sha256: String,
    model_state_sha256: String,
    torch: String,
    device: String,
}

struct RecurrentCp7WorkerV1 {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    sequence: u64,
    deployment_scale: f32,
    log_ratio_budget: f32,
    require_recurrent_value: bool,
}

impl RecurrentCp7WorkerV1 {
    fn launch_v1(
        python_executable: &Path,
        root: &Path,
        expected_model_file_sha256: &str,
        expected_model_state_sha256: &str,
        deployment_scale: f32,
        log_ratio_budget: f32,
        require_recurrent_value: bool,
    ) -> io::Result<Self> {
        let mut child = Command::new(python_executable)
            .arg(root.join("worker_v1.py"))
            .arg("--package-root")
            .arg(root)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "recurrent worker stdin missing")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "recurrent worker stdout missing")
        })?;
        let mut worker = Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            sequence: 0,
            deployment_scale,
            log_ratio_budget,
            require_recurrent_value,
        };
        let mut ready_line = String::new();
        let count = worker.stdout.read_line(&mut ready_line)?;
        if count == 0 || count > RECURRENT_CP7_MAX_WORKER_LINE_BYTES_V1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker ready line invalid",
            ));
        }
        let ready: RecurrentCp7ReadyV1 =
            serde_json::from_str(ready_line.trim_end()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "recurrent worker ready JSON invalid",
                )
            })?;
        if ready.schema != RECURRENT_CP7_READY_SCHEMA_V1
            || ready.model_file_sha256 != expected_model_file_sha256
            || ready.model_state_sha256 != expected_model_state_sha256
            || ready.device != "cpu"
            || ready.torch.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker ready identity mismatch",
            ));
        }
        Ok(worker)
    }

    fn exchange_v1(
        &mut self,
        request: &RecurrentCp7RequestV1,
    ) -> io::Result<(Vec<f32>, Option<f32>)> {
        if request.sequence != self.sequence {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "recurrent worker request sequence mismatch",
            ));
        }
        serde_json::to_writer(&mut self.stdin, request).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker request encoding failed",
            )
        })?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        let mut line = String::new();
        let count = self.stdout.read_line(&mut line)?;
        if count == 0 || count > RECURRENT_CP7_MAX_WORKER_LINE_BYTES_V1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker response line invalid",
            ));
        }
        let response: RecurrentCp7ResponseV1 =
            serde_json::from_str(line.trim_end()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "recurrent worker response JSON invalid",
                )
            })?;
        if response.schema != RECURRENT_CP7_RESPONSE_SCHEMA_V1
            || response.sequence != self.sequence
            || !response.projection_scale.is_finite()
            || !(0.0..=self.deployment_scale + 1.0e-5).contains(&response.projection_scale)
            || !response.maximum_absolute_log_ratio.is_finite()
            || response.maximum_absolute_log_ratio > self.log_ratio_budget + 1.0e-5
            || response.value_f32_bits.is_some() != self.require_recurrent_value
            || response
                .value_f32_bits
                .is_some_and(|bits| !f32::from_bits(bits).is_finite())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker response envelope invalid",
            ));
        }
        let logits = response
            .logits_f32_bits
            .into_iter()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        if logits.is_empty() || logits.iter().any(|value| !value.is_finite()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker logits invalid",
            ));
        }
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "recurrent worker sequence overflow",
            )
        })?;
        Ok((logits, response.value_f32_bits.map(f32::from_bits)))
    }
}

impl Drop for RecurrentCp7WorkerV1 {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn recurrent_cp7_sha256_v1(path: &Path) -> Result<String, ShadowScorerStartupErrorV1> {
    let bytes = fs::read(path).map_err(|_| {
        ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
    })?;
    Ok(lower_hex_raw32_v1(Sha256::digest(bytes).into()))
}

fn recurrent_cp7_is_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn recurrent_cp7_composite_v1(package: &RecurrentCp7PackageV1) -> String {
    let mut digest = Sha256::new();
    digest.update(if package.schema == RECURRENT_TERMINAL_PACKAGE_SCHEMA_V1 {
        RECURRENT_TERMINAL_COMPOSITE_DOMAIN_V1
    } else {
        RECURRENT_CP7_COMPOSITE_DOMAIN_V1
    });
    for value in [
        package.parent.composite_model_parameter_sha256.as_str(),
        package.files.model.sha256.as_str(),
        package.model_state_sha256.as_str(),
        package.files.worker.sha256.as_str(),
        package.files.model_definition.sha256.as_str(),
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    digest.update(package.log_ratio_budget.to_le_bytes());
    digest.update(package.deployment_scale.to_le_bytes());
    lower_hex_raw32_v1(digest.finalize().into())
}

fn f32_bits_v1(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
// Boxing would change construction sites in determinism-adjacent code; accepted.
#[allow(clippy::large_enum_variant)]
enum XmageCp7TeacherRecordV1 {
    Header {
        schema_version: u32,
        record_ordinal: u64,
        export_contract: &'static str,
        selection_source: &'static str,
        tensorizer_identity: &'static str,
        tensorizer_features_source_sha256: &'static str,
        model_input_commitment: &'static str,
        checkpoint: ShadowCheckpointIdentityV1,
    },
    Decision {
        schema_version: u32,
        record_ordinal: u64,
        teacher_decision_ordinal: u64,
        selection_source: &'static str,
        deck_ids: SessionDeckIdsV1,
        randomization_identity: &'static str,
        base_seed_u64_hex: String,
        pair_index: u64,
        pair_environment_seed_u64_hex: String,
        episode_id: u64,
        step: u64,
        environment_revision: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        decision_kind: &'static str,
        candidate_seat: PlayerSeatV1,
        actor_physical_decision_ordinal: u64,
        legal_action_count: u32,
        candidate_order_commitment_128_hex: String,
        model_input_sha256: String,
        old_policy_logits_f32_bits: Vec<u32>,
        old_value_f32_bits: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        structured_parent_policy_logits_f32_bits: Option<Vec<u32>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        structured_parent_value_f32_bits: Option<u32>,
        action_semantics: Vec<ActionSemanticV1>,
        selected_index: u32,
        selected_semantic: ActionSemanticV1,
        tensor: XmageCp7TeacherTensorV1,
    },
    Terminal {
        schema_version: u32,
        record_ordinal: u64,
        deck_ids: SessionDeckIdsV1,
        randomization_identity: &'static str,
        base_seed_u64_hex: String,
        pair_index: u64,
        pair_environment_seed_u64_hex: String,
        episode_id: u64,
        candidate_seat: PlayerSeatV1,
        terminal: RlSessionTerminalV1,
        diagnostic_state_hash_u64_hex: String,
        core_environment_hash_u64_hex: String,
    },
}

struct XmageCp7TeacherJsonlWriterV1 {
    writer: Box<dyn Write>,
    schema_version: u32,
    selection_source: &'static str,
    next_record_ordinal: u64,
    next_teacher_decision_ordinal: u64,
}

impl XmageCp7TeacherJsonlWriterV1 {
    fn create_v1(
        path: &Path,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> Result<Self, Box<dyn Error>> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Self::from_writer_v1(
            Box::new(BufWriter::new(file)),
            checkpoint,
            XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1,
            XMAGE_CP7_TEACHER_JSONL_CONTRACT_V1,
            XMAGE_CP7_TEACHER_SELECTION_SOURCE_V1,
        )
        .map_err(|error| Box::new(error) as Box<dyn Error>)
    }

    fn create_native_population_v1(
        path: &Path,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> Result<Self, Box<dyn Error>> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Self::from_writer_v1(
            Box::new(BufWriter::new(file)),
            checkpoint,
            1,
            NATIVE_POPULATION_TEACHER_JSONL_CONTRACT_V1,
            NATIVE_POPULATION_TEACHER_SELECTION_SOURCE_V1,
        )
        .map_err(|error| Box::new(error) as Box<dyn Error>)
    }

    fn from_writer_v1(
        writer: Box<dyn Write>,
        checkpoint: &ShadowCheckpointIdentityV1,
        schema_version: u32,
        export_contract: &'static str,
        selection_source: &'static str,
    ) -> io::Result<Self> {
        let mut export = Self {
            writer,
            schema_version,
            selection_source,
            next_record_ordinal: 0,
            next_teacher_decision_ordinal: 0,
        };
        export.write_v1(&XmageCp7TeacherRecordV1::Header {
            schema_version,
            record_ordinal: 0,
            export_contract,
            selection_source,
            tensorizer_identity: NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
            tensorizer_features_source_sha256: NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2,
            model_input_commitment: CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1,
            checkpoint: checkpoint.clone(),
        })?;
        export.next_record_ordinal = 1;
        Ok(export)
    }

    fn decision_record_v1(
        &self,
        active: &ActiveShadowSessionV1,
        scored: &ScoredCurrentDecisionV1,
        selected_index: u32,
    ) -> Result<XmageCp7TeacherRecordV1, ()> {
        if scored.expected.acting_player == active.candidate_seat {
            return Err(());
        }
        let selected = usize::try_from(selected_index).map_err(|_| ())?;
        let selected_semantic = scored.action_semantics.get(selected).ok_or(())?.clone();
        Ok(XmageCp7TeacherRecordV1::Decision {
            schema_version: self.schema_version,
            record_ordinal: self.next_record_ordinal,
            teacher_decision_ordinal: self.next_teacher_decision_ordinal,
            selection_source: self.selection_source,
            deck_ids: active.deck_ids.clone(),
            randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
            base_seed_u64_hex: u64_hex_v1(active.base_seed),
            pair_index: active.pair_index,
            pair_environment_seed_u64_hex: u64_hex_v1(active.pair_environment_seed),
            episode_id: scored.expected.episode_id,
            step: scored.expected.step,
            environment_revision: scored.expected.environment_revision,
            physical_decision_id: scored.expected.physical_decision_id,
            substep_index: scored.expected.substep_index,
            substep_count: scored.expected.substep_count,
            acting_player: scored.expected.acting_player,
            decision_kind: decision_kind_v1(scored.expected.decision_kind),
            candidate_seat: active.candidate_seat,
            actor_physical_decision_ordinal: scored.actor_physical_decision_ordinal,
            legal_action_count: scored.expected.legal_action_count,
            candidate_order_commitment_128_hex: lower_hex_bytes_v1(
                &scored.binding.action_binding.candidate_order_commitment,
            ),
            model_input_sha256: scored.model_input_sha256.clone(),
            old_policy_logits_f32_bits: scored.logits_f32_bits.clone(),
            old_value_f32_bits: scored.value_f32_bits,
            structured_parent_policy_logits_f32_bits: scored
                .structured_parent_logits_f32_bits
                .clone(),
            structured_parent_value_f32_bits: scored.structured_parent_value_f32_bits,
            action_semantics: scored.action_semantics.clone(),
            selected_index,
            selected_semantic,
            tensor: XmageCp7TeacherTensorV1::from_native_v1(&scored.tensor),
        })
    }

    fn terminal_record_v1(
        &self,
        active: &ActiveShadowSessionV1,
        terminal: RlSessionTerminalV1,
    ) -> XmageCp7TeacherRecordV1 {
        XmageCp7TeacherRecordV1::Terminal {
            schema_version: self.schema_version,
            record_ordinal: self.next_record_ordinal,
            deck_ids: active.deck_ids.clone(),
            randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
            base_seed_u64_hex: u64_hex_v1(active.base_seed),
            pair_index: active.pair_index,
            pair_environment_seed_u64_hex: u64_hex_v1(active.pair_environment_seed),
            episode_id: terminal.episode_id,
            candidate_seat: active.candidate_seat,
            terminal,
            diagnostic_state_hash_u64_hex: u64_hex_v1(active.session.diagnostic_state_hash()),
            core_environment_hash_u64_hex: u64_hex_v1(
                active.session.privileged_core_environment_hash(),
            ),
        }
    }

    fn write_decision_v1(&mut self, record: &XmageCp7TeacherRecordV1) -> io::Result<()> {
        if !matches!(record, XmageCp7TeacherRecordV1::Decision { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 teacher decision writer received a non-decision row",
            ));
        }
        self.write_v1(record)?;
        self.next_record_ordinal = self
            .next_record_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::other("record ordinal exhausted"))?;
        self.next_teacher_decision_ordinal = self
            .next_teacher_decision_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::other("teacher decision ordinal exhausted"))?;
        Ok(())
    }

    fn write_terminal_v1(&mut self, record: &XmageCp7TeacherRecordV1) -> io::Result<()> {
        if !matches!(record, XmageCp7TeacherRecordV1::Terminal { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 teacher terminal writer received a non-terminal row",
            ));
        }
        self.write_v1(record)?;
        self.next_record_ordinal = self
            .next_record_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::other("record ordinal exhausted"))?;
        Ok(())
    }

    fn write_v1(&mut self, record: &XmageCp7TeacherRecordV1) -> io::Result<()> {
        serde_json::to_writer(&mut self.writer, record)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
// Boxing would change construction sites in determinism-adjacent code; accepted.
#[allow(clippy::large_enum_variant)]
enum XmageCp7OutcomeRecordV1 {
    Header {
        schema_version: u32,
        record_ordinal: u64,
        export_contract: &'static str,
        selection_source: &'static str,
        tensorizer_identity: &'static str,
        tensorizer_features_source_sha256: &'static str,
        model_input_commitment: &'static str,
        checkpoint: ShadowCheckpointIdentityV1,
    },
    Decision {
        schema_version: u32,
        record_ordinal: u64,
        outcome_decision_ordinal: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        checkpoint: Option<ShadowCheckpointIdentityV1>,
        selection_source: &'static str,
        deck_ids: SessionDeckIdsV1,
        randomization_identity: &'static str,
        base_seed_u64_hex: String,
        pair_index: u64,
        pair_environment_seed_u64_hex: String,
        episode_id: u64,
        step: u64,
        environment_revision: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        decision_kind: &'static str,
        candidate_seat: PlayerSeatV1,
        actor_physical_decision_ordinal: u64,
        legal_action_count: u32,
        candidate_order_commitment_128_hex: String,
        model_input_sha256: String,
        old_policy_logits_f32_bits: Vec<u32>,
        old_value_f32_bits: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        structured_parent_policy_logits_f32_bits: Option<Vec<u32>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        structured_parent_value_f32_bits: Option<u32>,
        action_semantics: Vec<ActionSemanticV1>,
        selected_index: u32,
        selected_semantic: ActionSemanticV1,
        tensor: XmageCp7TeacherTensorV1,
    },
    Terminal {
        schema_version: u32,
        record_ordinal: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        checkpoint: Option<ShadowCheckpointIdentityV1>,
        deck_ids: SessionDeckIdsV1,
        randomization_identity: &'static str,
        base_seed_u64_hex: String,
        pair_index: u64,
        pair_environment_seed_u64_hex: String,
        episode_id: u64,
        candidate_seat: PlayerSeatV1,
        first_outcome_decision_ordinal: Option<u64>,
        outcome_decision_count: u64,
        candidate_terminal_reward: i8,
        terminal: RlSessionTerminalV1,
        diagnostic_state_hash_u64_hex: String,
        core_environment_hash_u64_hex: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XmageCp7OutcomeEpisodeV1 {
    pair_index: u64,
    episode_id: u64,
    candidate_seat: PlayerSeatV1,
    first_outcome_decision_ordinal: Option<u64>,
    outcome_decision_count: u64,
}

struct XmageCp7OutcomeJsonlWriterV1 {
    writer: Box<dyn Write>,
    schema_version: u32,
    selection_source: &'static str,
    row_checkpoint: Option<ShadowCheckpointIdentityV1>,
    next_record_ordinal: u64,
    next_outcome_decision_ordinal: u64,
    active_episode: Option<XmageCp7OutcomeEpisodeV1>,
}

impl XmageCp7OutcomeJsonlWriterV1 {
    fn create_v1(
        path: &Path,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> Result<Self, Box<dyn Error>> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Self::from_writer_v1(Box::new(BufWriter::new(file)), checkpoint)
            .map_err(|error| Box::new(error) as Box<dyn Error>)
    }

    fn create_native_population_v1(
        path: &Path,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> Result<Self, Box<dyn Error>> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Self::from_writer_configured_v1(
            Box::new(BufWriter::new(file)),
            checkpoint,
            1,
            NATIVE_POPULATION_OUTCOME_JSONL_CONTRACT_V1,
            XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1,
            Some(checkpoint.clone()),
        )
        .map_err(|error| Box::new(error) as Box<dyn Error>)
    }

    fn from_writer_v1(
        writer: Box<dyn Write>,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> io::Result<Self> {
        let exact_g384 = checkpoint.source_run_sha256 == SOURCE_RUN_SHA256_V1
            && checkpoint.source_generation == SOURCE_GENERATION_V1
            && checkpoint.source_checkpoint_sha256 == SOURCE_CHECKPOINT_SHA256_V1
            && checkpoint.source_sidecar_sha256 == SOURCE_SIDECAR_SHA256_V1
            && checkpoint.source_payload_sha256 == SOURCE_PAYLOAD_SHA256_V1
            && checkpoint.source_train_state_sha256 == SOURCE_TRAIN_STATE_SHA256_V1
            && checkpoint.loaded_run_sha256 == SOURCE_RUN_SHA256_V1
            && checkpoint.loaded_generation == SOURCE_GENERATION_V1
            && checkpoint.loaded_checkpoint_sha256 == SOURCE_CHECKPOINT_SHA256_V1
            && checkpoint.loaded_payload_sha256 == SOURCE_PAYLOAD_SHA256_V1
            && checkpoint.loaded_train_state_sha256 == SOURCE_TRAIN_STATE_SHA256_V1
            && checkpoint.model_parameter_sha256 == SOURCE_MODEL_PARAMETER_SHA256_V1;
        let verified_outcome_parent = (checkpoint.authority_kind
            == "xmage-cp7-outcome-reinforce-derivative-v1"
            || checkpoint.authority_kind == "recurrent-cp7-deployment-v1"
            || checkpoint.authority_kind == "qualified-policy-bounded-value-search-v1"
            || checkpoint.authority_kind == "current-net8-gae8-v1"
            || checkpoint.authority_kind == "current-net8-gae16-v1"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v1"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v2-policy-only"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v2-low-value"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v3-kl-0.3"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v3-kl-1.0"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v3-kl-3.0"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v3-kl-10.0"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v4-kl-0.3"
            || checkpoint.authority_kind == "current-net8-cp7-terminal-response-v4-kl-1.0"
            || checkpoint
                .authority_kind
                .starts_with("xmage-cp7-outcome-structured-policy-successor-v"))
            && checkpoint.source_run_sha256 == SOURCE_RUN_SHA256_V1
            && checkpoint.source_generation == SOURCE_GENERATION_V1
            && checkpoint.source_checkpoint_sha256 == SOURCE_CHECKPOINT_SHA256_V1
            && checkpoint.source_sidecar_sha256 == SOURCE_SIDECAR_SHA256_V1
            && checkpoint.source_payload_sha256 == SOURCE_PAYLOAD_SHA256_V1
            && checkpoint.source_train_state_sha256 == SOURCE_TRAIN_STATE_SHA256_V1
            && checkpoint.loaded_run_sha256 == SOURCE_RUN_SHA256_V1
            && checkpoint.loaded_generation > 0;
        let verified_population_generation = checkpoint.authority_kind
            == "population-store-validated-generation"
            && checkpoint.source_run_sha256 == checkpoint.loaded_run_sha256
            && checkpoint.source_generation == checkpoint.loaded_generation
            && checkpoint.source_checkpoint_sha256 == checkpoint.loaded_checkpoint_sha256
            && checkpoint.source_payload_sha256 == checkpoint.loaded_payload_sha256
            && checkpoint.source_train_state_sha256 == checkpoint.loaded_train_state_sha256
            && checkpoint.environment_trajectory_contract
                == POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1
            && checkpoint.sampler_identity == FAST_CATEGORICAL_SAMPLER_VERSION
            && checkpoint.sampler_contract_sha256 == FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256;
        let (schema_version, export_contract, row_checkpoint) = if exact_g384 {
            (
                XMAGE_CP7_OUTCOME_JSONL_SCHEMA_VERSION_V1,
                XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1,
                None,
            )
        } else if verified_outcome_parent || verified_population_generation {
            (
                XMAGE_CP7_OUTCOME_JSONL_SCHEMA_VERSION_V2,
                XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2,
                Some(checkpoint.clone()),
            )
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 outcome export requires exact g384 or a verified checkpoint authority",
            ));
        };
        Self::from_writer_configured_v1(
            writer,
            checkpoint,
            schema_version,
            export_contract,
            XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1,
            row_checkpoint,
        )
    }

    fn from_writer_configured_v1(
        writer: Box<dyn Write>,
        checkpoint: &ShadowCheckpointIdentityV1,
        schema_version: u32,
        export_contract: &'static str,
        selection_source: &'static str,
        row_checkpoint: Option<ShadowCheckpointIdentityV1>,
    ) -> io::Result<Self> {
        let mut export = Self {
            writer,
            schema_version,
            selection_source,
            row_checkpoint,
            next_record_ordinal: 0,
            next_outcome_decision_ordinal: 0,
            active_episode: None,
        };
        export.write_v1(&XmageCp7OutcomeRecordV1::Header {
            schema_version,
            record_ordinal: 0,
            export_contract,
            selection_source,
            tensorizer_identity: NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
            tensorizer_features_source_sha256: NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2,
            model_input_commitment: CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1,
            checkpoint: checkpoint.clone(),
        })?;
        export.next_record_ordinal = 1;
        Ok(export)
    }

    fn has_open_episode_v1(&self) -> bool {
        self.active_episode.is_some()
    }

    fn begin_episode_v1(&mut self, active: &ActiveShadowSessionV1) -> Result<(), ()> {
        if self.active_episode.is_some() {
            return Err(());
        }
        let episode_id = match active.session.current_response() {
            FastActorResponseV1::Decision(decision) => decision.episode_id,
            FastActorResponseV1::Terminal(terminal) => terminal.episode_id,
        };
        self.active_episode = Some(XmageCp7OutcomeEpisodeV1 {
            pair_index: active.pair_index,
            episode_id,
            candidate_seat: active.candidate_seat,
            first_outcome_decision_ordinal: None,
            outcome_decision_count: 0,
        });
        Ok(())
    }

    fn episode_matches_v1(
        episode: XmageCp7OutcomeEpisodeV1,
        active: &ActiveShadowSessionV1,
    ) -> bool {
        let episode_id = match active.session.current_response() {
            FastActorResponseV1::Decision(decision) => decision.episode_id,
            FastActorResponseV1::Terminal(terminal) => terminal.episode_id,
        };
        episode.pair_index == active.pair_index
            && episode.episode_id == episode_id
            && episode.candidate_seat == active.candidate_seat
    }

    fn decision_record_v1(
        &self,
        active: &ActiveShadowSessionV1,
        scored: &ScoredCurrentDecisionV1,
        selected_index: u32,
    ) -> Result<XmageCp7OutcomeRecordV1, ()> {
        let episode = self.active_episode.ok_or(())?;
        if !Self::episode_matches_v1(episode, active)
            || scored.expected.acting_player != active.candidate_seat
        {
            return Err(());
        }
        let selected = usize::try_from(selected_index).map_err(|_| ())?;
        let selected_semantic = scored.action_semantics.get(selected).ok_or(())?.clone();
        Ok(XmageCp7OutcomeRecordV1::Decision {
            schema_version: self.schema_version,
            record_ordinal: self.next_record_ordinal,
            outcome_decision_ordinal: self.next_outcome_decision_ordinal,
            checkpoint: self.row_checkpoint.clone(),
            selection_source: self.selection_source,
            deck_ids: active.deck_ids.clone(),
            randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
            base_seed_u64_hex: u64_hex_v1(active.base_seed),
            pair_index: active.pair_index,
            pair_environment_seed_u64_hex: u64_hex_v1(active.pair_environment_seed),
            episode_id: scored.expected.episode_id,
            step: scored.expected.step,
            environment_revision: scored.expected.environment_revision,
            physical_decision_id: scored.expected.physical_decision_id,
            substep_index: scored.expected.substep_index,
            substep_count: scored.expected.substep_count,
            acting_player: scored.expected.acting_player,
            decision_kind: decision_kind_v1(scored.expected.decision_kind),
            candidate_seat: active.candidate_seat,
            actor_physical_decision_ordinal: scored.actor_physical_decision_ordinal,
            legal_action_count: scored.expected.legal_action_count,
            candidate_order_commitment_128_hex: lower_hex_bytes_v1(
                &scored.binding.action_binding.candidate_order_commitment,
            ),
            model_input_sha256: scored.model_input_sha256.clone(),
            old_policy_logits_f32_bits: scored.logits_f32_bits.clone(),
            old_value_f32_bits: scored.value_f32_bits,
            structured_parent_policy_logits_f32_bits: scored
                .structured_parent_logits_f32_bits
                .clone(),
            structured_parent_value_f32_bits: scored.structured_parent_value_f32_bits,
            action_semantics: scored.action_semantics.clone(),
            selected_index,
            selected_semantic,
            tensor: XmageCp7TeacherTensorV1::from_native_v1(&scored.tensor),
        })
    }

    fn terminal_record_v1(
        &self,
        active: &ActiveShadowSessionV1,
        terminal: RlSessionTerminalV1,
    ) -> Result<XmageCp7OutcomeRecordV1, ()> {
        let episode = self.active_episode.ok_or(())?;
        if !Self::episode_matches_v1(episode, active) || terminal.episode_id != episode.episode_id {
            return Err(());
        }
        let reward = match active.candidate_seat {
            PlayerSeatV1::P0 => terminal.terminal_reward[0],
            PlayerSeatV1::P1 => terminal.terminal_reward[1],
        };
        let candidate_terminal_reward = i8::try_from(reward).map_err(|_| ())?;
        if !matches!(candidate_terminal_reward, -1..=1) {
            return Err(());
        }
        Ok(XmageCp7OutcomeRecordV1::Terminal {
            schema_version: self.schema_version,
            record_ordinal: self.next_record_ordinal,
            checkpoint: self.row_checkpoint.clone(),
            deck_ids: active.deck_ids.clone(),
            randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
            base_seed_u64_hex: u64_hex_v1(active.base_seed),
            pair_index: active.pair_index,
            pair_environment_seed_u64_hex: u64_hex_v1(active.pair_environment_seed),
            episode_id: terminal.episode_id,
            candidate_seat: active.candidate_seat,
            first_outcome_decision_ordinal: episode.first_outcome_decision_ordinal,
            outcome_decision_count: episode.outcome_decision_count,
            candidate_terminal_reward,
            terminal,
            diagnostic_state_hash_u64_hex: u64_hex_v1(active.session.diagnostic_state_hash()),
            core_environment_hash_u64_hex: u64_hex_v1(
                active.session.privileged_core_environment_hash(),
            ),
        })
    }

    fn write_decision_v1(&mut self, record: &XmageCp7OutcomeRecordV1) -> io::Result<()> {
        if !matches!(record, XmageCp7OutcomeRecordV1::Decision { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 outcome decision writer received a non-decision row",
            ));
        }
        self.write_v1(record)?;
        let episode = self.active_episode.as_mut().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "CP7 outcome episode is not open",
            )
        })?;
        if episode.first_outcome_decision_ordinal.is_none() {
            episode.first_outcome_decision_ordinal = Some(self.next_outcome_decision_ordinal);
        }
        episode.outcome_decision_count = episode
            .outcome_decision_count
            .checked_add(1)
            .ok_or_else(|| io::Error::other("episode decision count exhausted"))?;
        self.next_record_ordinal = self
            .next_record_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::other("record ordinal exhausted"))?;
        self.next_outcome_decision_ordinal = self
            .next_outcome_decision_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::other("outcome decision ordinal exhausted"))?;
        Ok(())
    }

    fn write_terminal_v1(&mut self, record: &XmageCp7OutcomeRecordV1) -> io::Result<()> {
        if !matches!(record, XmageCp7OutcomeRecordV1::Terminal { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 outcome terminal writer received a non-terminal row",
            ));
        }
        self.write_v1(record)?;
        self.next_record_ordinal = self
            .next_record_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::other("record ordinal exhausted"))?;
        self.active_episode = None;
        Ok(())
    }

    fn write_v1(&mut self, record: &XmageCp7OutcomeRecordV1) -> io::Result<()> {
        serde_json::to_writer(&mut self.writer, record)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

/// Wire version this scorer speaks. `V1` is the frozen original: its request
/// grammar, response envelope and every body field stay byte-identical, so a
/// pinned consumer (including the already-built `checkpoint_shadow_stdio_v1`
/// executables and `XMageRallyBridgeJsonCodec`, which rejects unknown fields)
/// keeps working unchanged. `V2` adds the kernel game clock to every decision
/// body and accepts an optional `expected_clock` rendezvous guard on `step`.
/// See `docs/checkpoint_shadow_stdio_protocol_v2.md`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadowStdioProtocolV1 {
    #[default]
    V1,
    V2,
}

impl ShadowStdioProtocolV1 {
    fn protocol_string_v1(self) -> &'static str {
        match self {
            Self::V1 => CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1,
            Self::V2 => CHECKPOINT_SHADOW_STDIO_PROTOCOL_V2,
        }
    }

    fn schema_version_v1(self) -> u32 {
        match self {
            Self::V1 => CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1,
            Self::V2 => CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V2,
        }
    }

    fn carries_kernel_clock_v1(self) -> bool {
        matches!(self, Self::V2)
    }
}

/// Stable wire spelling of `state::Step`. Written out rather than derived from
/// `Debug`/`Serialize` so a kernel-side rename can never silently move the
/// wire contract the Java rendezvous guard compares against.
pub(crate) fn kernel_phase_step_name_v2(step: crate::state::Step) -> &'static str {
    match step {
        crate::state::Step::Untap => "Untap",
        crate::state::Step::Upkeep => "Upkeep",
        crate::state::Step::Draw => "Draw",
        crate::state::Step::Main1 => "Main1",
        crate::state::Step::BeginCombat => "BeginCombat",
        crate::state::Step::DeclareAttackers => "DeclareAttackers",
        crate::state::Step::DeclareBlockers => "DeclareBlockers",
        crate::state::Step::CombatDamage => "CombatDamage",
        crate::state::Step::EndCombat => "EndCombat",
        crate::state::Step::Main2 => "Main2",
        crate::state::Step::End => "End",
        crate::state::Step::Cleanup => "Cleanup",
    }
}

/// The kernel's own game clock at the decision the response describes. This is
/// the field the shadow rendezvous lacked: without it neither side can notice
/// that an XMage callback from one turn is being mapped onto a kernel decision
/// from another. V2 only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct KernelClockV2 {
    turn: u32,
    phase_step: &'static str,
    active_player: PlayerSeatV1,
    priority_player: PlayerSeatV1,
    stack_depth: u32,
}

impl KernelClockV2 {
    fn from_session_v2(session: &FastActorSessionV1) -> Self {
        let state = session.kernel_search_state_v1();
        Self {
            turn: state.turn,
            phase_step: kernel_phase_step_name_v2(state.step),
            active_player: state.active_player.into(),
            priority_player: state.priority_player.into(),
            stack_depth: u32::try_from(state.stack.len()).unwrap_or(u32::MAX),
        }
    }

    fn matches_expected_v2(self, expected: &ExpectedKernelClockV2) -> bool {
        self.turn == expected.turn
            && self.phase_step == expected.phase_step
            && self.active_player == expected.active_player
    }
}

/// Optional caller-supplied rendezvous assertion on `step`. Carrying it means
/// "I believe the kernel decision I am about to answer is the one at this
/// game clock"; a disagreement fails closed instead of silently consuming a
/// decision from another turn.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ExpectedKernelClockV2 {
    turn: u32,
    phase_step: String,
    active_player: PlayerSeatV1,
}

/// Field *presence*, not field value, is what arms the rendezvous guard, so
/// the two must not collapse into the same `None`. A bare
/// `Option<ExpectedKernelClockV2>` cannot tell "the caller omitted the guard"
/// from "the caller sent `expected_clock: null`", and the second is exactly
/// what a nullable serializer emits for an unset field: taking it as absence
/// would silently disarm the check on the V2 path and silently accept a field
/// V1 has always rejected as unknown. `#[serde(default)]` still supplies
/// `None` when the key is absent, and this function is not called then; when
/// the key is present this runs, a null fails to deserialize into the struct,
/// and the whole request is rejected as `malformed_request` before any
/// handler can act on it.
fn deserialize_present_expected_clock_v2<'de, D>(
    deserializer: D,
) -> Result<Option<ExpectedKernelClockV2>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    ExpectedKernelClockV2::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
enum ShadowScorerRequestV1 {
    Reset {
        request_id: String,
        episode_id: u64,
        base_seed: u64,
    },
    ScoreCurrent {
        request_id: String,
        episode_id: u64,
        expected_step: u64,
    },
    Step {
        request_id: String,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        /// V2 only. A V1-mode service rejects any request carrying this field
        /// as `malformed_request`, exactly as it did before the field existed.
        /// An explicit `null` counts as carrying it and is rejected under both
        /// versions; see `deserialize_present_expected_clock_v2`.
        #[serde(default, deserialize_with = "deserialize_present_expected_clock_v2")]
        expected_clock: Option<ExpectedKernelClockV2>,
    },
}

impl ShadowScorerRequestV1 {
    fn request_id(&self) -> &str {
        match self {
            Self::Reset { request_id, .. }
            | Self::ScoreCurrent { request_id, .. }
            | Self::Step { request_id, .. } => request_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct AppliedActionV1 {
    episode_id: u64,
    step: u64,
    candidate_order_commitment_128_hex: String,
    model_input_sha256: String,
    selected_index: u32,
    selected_logit_f32_bits: u32,
    semantic: ActionSemanticV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct DecisionBodyV1 {
    deck_ids: SessionDeckIdsV1,
    randomization_identity: &'static str,
    base_seed_u64_hex: String,
    pair_index: u64,
    pair_environment_seed_u64_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_library_card_definition_ids: Option<[Vec<u16>; 2]>,
    episode_id: u64,
    step: u64,
    environment_revision: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    acting_player: PlayerSeatV1,
    decision_kind: &'static str,
    legal_action_count: u32,
    candidate_seat: PlayerSeatV1,
    candidate_controls_current_actor: bool,
    actor_physical_decision_ordinal: u64,
    candidate_action_seed_u64_hex: Option<String>,
    selected_action_index: Option<u32>,
    candidate_order_commitment_128_hex: String,
    model_input_commitment: &'static str,
    model_input_sha256: String,
    diagnostic_state_hash_u64_hex: String,
    core_environment_hash_u64_hex: String,
    logits_f32_bits: Vec<u32>,
    value_f32_bits: u32,
    action_semantics: Vec<ActionSemanticV1>,
    /// V2 only; `None` (and therefore absent from the wire) under V1, which
    /// keeps every V1 response byte-identical to the frozen protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    kernel_clock: Option<KernelClockV2>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct TerminalBodyV1 {
    deck_ids: SessionDeckIdsV1,
    randomization_identity: &'static str,
    base_seed_u64_hex: String,
    pair_index: u64,
    pair_environment_seed_u64_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_library_card_definition_ids: Option<[Vec<u16>; 2]>,
    terminal: RlSessionTerminalV1,
    candidate_seat: PlayerSeatV1,
    diagnostic_state_hash_u64_hex: String,
    core_environment_hash_u64_hex: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "response_type", rename_all = "snake_case")]
enum ShadowScorerResponseBodyV1 {
    Decision {
        decision: DecisionBodyV1,
        applied_action: Option<AppliedActionV1>,
    },
    Terminal {
        terminal: TerminalBodyV1,
        applied_action: Option<AppliedActionV1>,
    },
    Error {
        error_code: &'static str,
        message: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct ShadowScorerResponseV1 {
    protocol: &'static str,
    schema_version: u32,
    request_id: Option<String>,
    checkpoint: ShadowCheckpointIdentityV1,
    #[serde(flatten)]
    body: ShadowScorerResponseBodyV1,
}

fn response_v1(
    request_id: Option<String>,
    checkpoint: &ShadowCheckpointIdentityV1,
    body: ShadowScorerResponseBodyV1,
) -> ShadowScorerResponseV1 {
    ShadowScorerResponseV1 {
        protocol: CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1,
        schema_version: CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1,
        request_id,
        checkpoint: checkpoint.clone(),
        body,
    }
}

fn error_body_v1(code: &'static str, message: &'static str) -> ShadowScorerResponseBodyV1 {
    ShadowScorerResponseBodyV1::Error {
        error_code: code,
        message,
    }
}

fn valid_request_id_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_graphic() && *byte != b'"' && *byte != b'\\')
}

fn request_id_from_value_v1(value: &serde_json::Value) -> Option<String> {
    value
        .as_object()
        .and_then(|object| object.get("request_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|request_id| valid_request_id_v1(request_id))
        .map(ToOwned::to_owned)
}

fn decision_kind_v1(kind: FastActorDecisionKindV1) -> &'static str {
    match kind {
        FastActorDecisionKindV1::Surface => "surface",
        FastActorDecisionKindV1::AttackerInclusion => "attacker_inclusion",
        FastActorDecisionKindV1::BlockerInclusion => "blocker_inclusion",
    }
}

fn lower_hex_bytes_v1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn u64_hex_v1(value: u64) -> String {
    format!("{value:016x}")
}

fn legacy_rally_initial_library_orders_v1(environment_seed: u64) -> [Vec<u16>; 2] {
    let deck = rally_deck_ids();
    let mut shuffle_rng = SplitMix64::seed(environment_seed);
    [
        shuffled(&deck, &mut shuffle_rng),
        shuffled(&deck, &mut shuffle_rng),
    ]
}

fn framed_atom_v1(
    hasher: &mut Sha256,
    label: &str,
    payload_length: u64,
    payload: impl Iterator<Item = u8>,
) {
    hasher.update(
        u32::try_from(label.len())
            .expect("fixed tensor label length fits u32")
            .to_be_bytes(),
    );
    hasher.update(label.as_bytes());
    hasher.update(payload_length.to_be_bytes());
    for byte in payload {
        hasher.update([byte]);
    }
}

fn frame_f32_v1(hasher: &mut Sha256, label: &str, values: &[f32]) {
    framed_atom_v1(
        hasher,
        label,
        u64::try_from(values.len() * 4).expect("tensor byte length fits u64"),
        values
            .iter()
            .flat_map(|value| value.to_bits().to_le_bytes()),
    );
}

fn frame_i64_v1(hasher: &mut Sha256, label: &str, values: &[i64]) {
    framed_atom_v1(
        hasher,
        label,
        u64::try_from(values.len() * 8).expect("tensor byte length fits u64"),
        values.iter().flat_map(|value| value.to_le_bytes()),
    );
}

fn model_input_sha256_v1(tensor: &NativeFlatDecisionTensorV2) -> String {
    let mut hasher = Sha256::new();
    framed_atom_v1(
        &mut hasher,
        "schema",
        CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1.len() as u64,
        CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1.bytes(),
    );
    frame_f32_v1(&mut hasher, "state", &tensor.state);
    frame_f32_v1(&mut hasher, "object_features", &tensor.object_features);
    frame_i64_v1(&mut hasher, "object_card_ids", &tensor.object_card_ids);
    frame_i64_v1(&mut hasher, "object_groups", &tensor.object_groups);
    frame_i64_v1(&mut hasher, "object_node_ids", &tensor.object_node_ids);
    frame_f32_v1(&mut hasher, "edge_features", &tensor.edge_features);
    frame_i64_v1(
        &mut hasher,
        "edge_source_indices",
        &tensor.edge_source_indices,
    );
    frame_i64_v1(
        &mut hasher,
        "edge_target_indices",
        &tensor.edge_target_indices,
    );
    frame_f32_v1(&mut hasher, "action_features", &tensor.action_features);
    frame_f32_v1(
        &mut hasher,
        "action_ref_features",
        &tensor.action_ref_features,
    );
    frame_i64_v1(
        &mut hasher,
        "action_ref_card_ids",
        &tensor.action_ref_card_ids,
    );
    frame_i64_v1(
        &mut hasher,
        "action_ref_action_indices",
        &tensor.action_ref_action_indices,
    );
    frame_i64_v1(
        &mut hasher,
        "action_ref_node_indices",
        &tensor.action_ref_node_indices,
    );
    lower_hex_bytes_v1(&hasher.finalize())
}

fn public_history_sha256_v1(
    history: &[NativeStructuredHistoryEntryV1],
    acting_player: u8,
) -> Result<String, ()> {
    if history.len() > HISTORY_LENGTH_V1 {
        return Err(());
    }
    let mut features = Vec::new();
    for entry in history {
        features.extend(entry.actor_relative_features_v1(acting_player)?);
    }
    let entry_count = [i64::try_from(history.len()).map_err(|_| ())?];
    let mut hasher = Sha256::new();
    framed_atom_v1(
        &mut hasher,
        "schema",
        CHECKPOINT_SHADOW_PUBLIC_HISTORY_COMMITMENT_V1.len() as u64,
        CHECKPOINT_SHADOW_PUBLIC_HISTORY_COMMITMENT_V1.bytes(),
    );
    frame_i64_v1(&mut hasher, "history_entry_count", &entry_count);
    frame_f32_v1(&mut hasher, "history_features", &features);
    Ok(lower_hex_bytes_v1(&hasher.finalize()))
}

fn rl_error_code_v1(code: RlSessionErrorCode) -> &'static str {
    match code {
        RlSessionErrorCode::EpisodeAlreadyTerminal => "episode_already_terminal",
        RlSessionErrorCode::EpisodeIdMismatch => "episode_id_mismatch",
        RlSessionErrorCode::ExpectedStepMismatch => "expected_step_mismatch",
        RlSessionErrorCode::SelectedIndexOutOfRange => "selected_index_out_of_range",
        RlSessionErrorCode::SelectedActionIdMismatch => "selected_action_id_mismatch",
        RlSessionErrorCode::SelectedActionIdUnknown => "selected_action_id_unknown",
        RlSessionErrorCode::StaleEnvironmentBinding => "stale_environment_binding",
        RlSessionErrorCode::UnsupportedDeck => "unsupported_deck",
        RlSessionErrorCode::EnvironmentRandomization => "environment_randomization",
    }
}

/// Everything the `ModelGuidedSearch` selector needs, held on the service
/// so `score_session_v1` can reach it without widening any existing type.
///
/// The AUTHORITY is not built here. It is bound on the first reset, from
/// the live session's own
/// `kernel_search_private_diagnostic_identity_v1()`, because
/// `ModelGuidedSearchAuthorityV1` commits to that identity and the core
/// refuses to search a session whose identity disagrees with the record's.
/// Which of the two admissible identities a session carries is a property
/// of the loaded checkpoint's environment contract, and reading it off the
/// real session is the only way to be right about it that does not
/// duplicate the environment-contract mapping in a second place where it
/// could drift. Later resets must agree with the first, or the search
/// fails closed rather than silently re-minting an authority mid-run.
struct ModelGuidedSearchRuntimeV1 {
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    action_seed: u64,
    value_domain: ModelGuidedSearchValueHeadDomainV1,
    /// Whether the two diagnostic stability halves run. They execute
    /// synchronously inside each decision, so leaving them on roughly
    /// triples the per-decision transition count and the latency that goes
    /// with it. A formal panel measuring product latency turns them off;
    /// an S2 diagnostics run leaves them on. Either way the per-decision
    /// record says which ran, and `ceiling_status` is classified from the
    /// latency that configuration actually paid.
    stability_halves_enabled: bool,
    diagnostics: ModelGuidedSearchOutcomeWriterV4,
    bound: Option<BoundModelGuidedSearchV1>,
    /// The episode id of an episode that reached a TERMINAL whose footer
    /// has not been published yet.
    ///
    /// A terminal close can fail transiently (a held handle, ENOSPC), and
    /// once it has, the session already reports the episode as terminal:
    /// a retried step is rejected before it can reach the close again, and
    /// the next reset or the end of input would then close the file as
    /// `episode_replaced` or `process_exit`. That misclassifies a game
    /// that actually ended. This flag makes the terminal reason STICK
    /// until the footer is really on disk, and gives the retry paths
    /// something to notice.
    pending_terminal_episode_id: Option<u64>,
}

/// `pub(crate)`: the S1 replay tool binds its authority through
/// [`BoundModelGuidedSearchV1::bind_v1`], the same constructor
/// `begin_episode_v1` uses, so an S1 timing is measured under exactly the
/// authority record (and therefore exactly the seed derivation) a panel
/// would run under. Its own public surface never names this type.
pub(crate) struct BoundModelGuidedSearchV1 {
    pub(crate) core: ModelGuidedSearchCoreV1,
    pub(crate) private_diagnostic_identity: String,
    pub(crate) wrapper_identity: WrapperIdentityV4,
    pub(crate) authority_digest_sha256: String,
}

impl BoundModelGuidedSearchV1 {
    /// Builds the authority record, its digest, the wrapper identity, and
    /// the search core for one (tier, seed block, checkpoint, live
    /// session) combination.
    ///
    /// Factored out of `ModelGuidedSearchRuntimeV1::begin_episode_v1`
    /// (whose body is now a call to this, with the identical arguments in
    /// the identical order) rather than duplicated, so the scorer's own
    /// binding and the S1 replay's binding cannot drift: one of them
    /// carrying a different `consumption_mode`, a different lineage
    /// spelling, or a different `net_architecture_identity` would change
    /// the authority digest, and with it every simulation seed the search
    /// draws.
    pub(crate) fn bind_v1(
        tier: KernelNativeSearchTierV1,
        seed_block_id: usize,
        action_seed: u64,
        value_domain: &ModelGuidedSearchValueHeadDomainV1,
        live_identity: &str,
        checkpoint: &ShadowCheckpointIdentityV1,
        net_architecture_identity: &str,
    ) -> Result<Self, &'static str> {
        let lineage = ModelGuidedSearchRuntimeV1::checkpoint_lineage_id_v1(checkpoint);
        let authority = ModelGuidedSearchAuthorityV1::new(
            tier,
            action_seed,
            live_identity,
            &lineage,
            checkpoint.loaded_generation,
            &checkpoint.model_parameter_sha256,
            net_architecture_identity,
            // Mode (a): the wrapper IS the presented agent's decision
            // rule, not an opponent and not a training-target source.
            ModelGuidedSearchConsumptionModeV1::SearchAtInference,
        )
        .map_err(|_| "model_guided_search_authority_invalid")?;
        let authority_digest = authority
            .digest()
            .map_err(|_| "model_guided_search_authority_invalid")?;
        let wrapper_identity = WrapperIdentityV4 {
            core_algorithm_identity: authority.algorithm_identity.clone(),
            authority_kind: authority.authority_kind.clone(),
            authority_schema: authority.schema.clone(),
            node_key_identity: authority.node_key_identity.clone(),
            seed_domain: authority.seed_domain.clone(),
            tier: search_tier_tag_v1(tier).to_owned(),
            transition_budget: authority.transition_budget,
            policy_step_depth_cap: authority.policy_step_depth_cap,
            seed_block_id: seed_block_id as u64,
            action_seed_u64_hex: u64_hex_v1(action_seed),
            search_authority_digest_sha256: lower_hex_sha256_v4(authority_digest),
            checkpoint_lineage_id: authority.checkpoint_store_path_or_lineage_id.clone(),
            net_architecture_identity: authority.net_architecture_identity.clone(),
            puct_prior_quantization_contract_sha256: authority
                .puct_prior_quantization_contract_sha256
                .clone(),
            value_quantization_contract_sha256: authority
                .value_quantization_contract_sha256
                .clone(),
            forward_determinism_build_identity: authority
                .forward_determinism_build_identity
                .clone(),
            value_head_domain: value_head_domain_tag_v1(value_domain),
            checkpoint_manifest_sha256: checkpoint.loaded_checkpoint_sha256.clone(),
            checkpoint_model_parameter_sha256: checkpoint.model_parameter_sha256.clone(),
            engine_commit: authority.engine_commit.clone(),
        };
        let core = ModelGuidedSearchCoreV1::new(authority)
            .map_err(|_| "model_guided_search_authority_invalid")?;
        Ok(Self {
            core,
            private_diagnostic_identity: live_identity.to_owned(),
            wrapper_identity,
            authority_digest_sha256: lower_hex_sha256_v4(authority_digest),
        })
    }
}

/// Pins this thread's MXCSR, verifies it fail-closed, and returns the
/// production model-guided leaf evaluator.
///
/// The ONLY construction site of
/// `ModelGuidedSearchRealForwardValueEvaluatorV1` outside that type's own
/// tests, so passing the MXCSR gate is a precondition of being able to run
/// a search forward at all rather than a discipline each caller has to
/// remember. `pub(crate)` because the S1 feasibility replay
/// (`native_tts_s1_replay_v1`) runs the production selector and must reach
/// it through this same gate; its own public surface never names the
/// evaluator type.
pub(crate) fn model_guided_search_pinned_evaluator_v1(
    net: &NativePolicyValueNetV1,
    value_domain: ModelGuidedSearchValueHeadDomainV1,
) -> Result<ModelGuidedSearchRealForwardValueEvaluatorV1<'_>, &'static str> {
    crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1()
        .map_err(|_| "model_guided_search_mxcsr_not_pinned")?;
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1()
        .map_err(|_| "model_guided_search_mxcsr_not_pinned")?;
    Ok(ModelGuidedSearchRealForwardValueEvaluatorV1::new(
        net,
        value_domain,
    ))
}

/// One FULL-BUDGET model-guided search, with the scorer's own failure
/// mapping.
///
/// Factored out of `ShadowScorerServiceV1::model_guided_search_selection_v1`
/// so the S1 feasibility replay times the identical call, not a re-derived
/// copy of it: the same core (and therefore the same authority digest and
/// the same simulation-seed derivation), the same evaluator, the same
/// value domain, and the same fail-closed error. The caller owns the clock;
/// nothing here reads one, so no timing can reach the chosen action.
pub(crate) fn model_guided_search_full_budget_v1(
    core: &ModelGuidedSearchCoreV1,
    evaluator: &ModelGuidedSearchRealForwardValueEvaluatorV1<'_>,
    value_domain: &ModelGuidedSearchValueHeadDomainV1,
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
) -> Result<ModelGuidedSearchDecisionV1, &'static str> {
    core.select_action_v1(session, expected, evaluator, value_domain)
        .map_err(|error| {
            eprintln!("MODEL_GUIDED_SEARCH_FAILED full_budget error={error}");
            "model_guided_search_failed"
        })
}

fn value_head_domain_tag_v1(domain: &ModelGuidedSearchValueHeadDomainV1) -> String {
    match domain {
        ModelGuidedSearchValueHeadDomainV1::Tanh => "tanh".to_owned(),
        ModelGuidedSearchValueHeadDomainV1::SigmoidFamily => "sigmoid_family".to_owned(),
        ModelGuidedSearchValueHeadDomainV1::Calibrated { lower, upper } => {
            format!("calibrated:{:08x}:{:08x}", lower.to_bits(), upper.to_bits())
        }
    }
}

fn search_tier_tag_v1(tier: KernelNativeSearchTierV1) -> &'static str {
    match tier {
        KernelNativeSearchTierV1::T512 => "t512",
        KernelNativeSearchTierV1::T2048 => "t2048",
        KernelNativeSearchTierV1::T8192 => "t8192",
        KernelNativeSearchTierV1::T32768 => "t32768",
    }
}

impl ModelGuidedSearchRuntimeV1 {
    /// Resolves the CLI-supplied seed BLOCK ID against the launcher-owned
    /// allowlist and opens the diagnostics directory. Both fail closed:
    /// an unregistered block id and a missing directory are startup
    /// errors, never warnings.
    fn new_v1(
        tier: KernelNativeSearchTierV1,
        seed_block_id: usize,
        diagnostics_directory: PathBuf,
        stability_halves_enabled: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let action_seed = authorized_seed_block_v1(seed_block_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "model-guided search seed block id is not in the authorized allowlist",
            )
        })?;
        let diagnostics = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(
            diagnostics_directory,
        )
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("model-guided search diagnostics directory is unusable: {error}"),
            )
        })?;
        Ok(Self {
            tier,
            seed_block_id,
            action_seed,
            value_domain: MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1,
            stability_halves_enabled,
            diagnostics,
            bound: None,
            pending_terminal_episode_id: None,
        })
    }

    /// The lineage this authority binds: the identity of the checkpoint
    /// that was actually LOADED, not the generic kind string every
    /// Store-backed checkpoint shares.
    ///
    /// `authority_kind` alone is not a lineage. Every checkpoint drawn
    /// from a population Store carries the same value for it, so two
    /// different checkpoints from the same Store produced the same
    /// `checkpoint_store_path_or_lineage_id`, and the authority record and
    /// every seed derived from its digest failed to bind the lineage the
    /// field promises. Composing the loaded run identity, the loaded
    /// checkpoint manifest digest, and the loaded payload digest makes the
    /// field discriminate exactly what it claims to: two different
    /// checkpoints can no longer share an authority digest even when they
    /// share a kind, a generation, and (in the degenerate case) their
    /// weight bytes.
    ///
    /// The kind is kept as a readable prefix, so the field still says at a
    /// glance which authority family a record came from.
    fn checkpoint_lineage_id_v1(checkpoint: &ShadowCheckpointIdentityV1) -> String {
        format!(
            "{}|loaded_run_sha256={}|loaded_generation={}|loaded_checkpoint_sha256={}|loaded_payload_sha256={}",
            checkpoint.authority_kind,
            checkpoint.loaded_run_sha256,
            checkpoint.loaded_generation,
            checkpoint.loaded_checkpoint_sha256,
            checkpoint.loaded_payload_sha256,
        )
    }

    /// Binds (or re-checks) the authority against a live session, then
    /// opens this episode's diagnostics file. Returns a stable error code
    /// on any failure so the caller can surface it through the protocol's
    /// existing error body.
    ///
    /// `net_architecture_identity` is read off the LOADED net rather than
    /// pinned here, so the authority names the architecture that will
    /// actually run.
    fn begin_episode_v1(
        &mut self,
        session: &FastActorSessionV1,
        checkpoint: &ShadowCheckpointIdentityV1,
        net_architecture_identity: &str,
        episode_id: u64,
        base_seed: u64,
        candidate_seat: PlayerSeatV1,
    ) -> Result<(), &'static str> {
        let live_identity = session
            .kernel_search_private_diagnostic_identity_v1()
            .to_owned();
        if self.bound.is_none() {
            self.bound = Some(BoundModelGuidedSearchV1::bind_v1(
                self.tier,
                self.seed_block_id,
                self.action_seed,
                &self.value_domain,
                &live_identity,
                checkpoint,
                net_architecture_identity,
            )?);
        }
        let bound = self
            .bound
            .as_ref()
            .ok_or("model_guided_search_authority_invalid")?;
        if bound.private_diagnostic_identity != live_identity {
            return Err("model_guided_search_diagnostic_identity_changed");
        }
        let wrapper_identity = bound.wrapper_identity.clone();
        // A reset REPLACES whatever episode was open. Closing it with a
        // footer first is what makes its final decision classifiable and
        // marks its file complete; the writer refuses to open a second
        // episode over an unclosed one, so this cannot be skipped by
        // accident.
        self.close_episode_v1(EpisodeCloseReasonV4::EpisodeReplaced)?;
        self.diagnostics
            .begin_episode_v4(episode_id, base_seed, candidate_seat, wrapper_identity)
            .map_err(|_| "model_guided_search_diagnostics_write_failed")
    }

    /// Closes the open diagnostics episode with a footer, if one is open.
    ///
    /// A no-op when nothing is open, so callers on the three closing paths
    /// (terminal, reset replacement, orderly process exit) do not each
    /// need to ask first. A failure is reported: the footer carries the
    /// last decision's publication and the episode's content digest, and
    /// dropping it silently would put the episode back in exactly the
    /// state this record exists to prevent.
    fn close_episode_v1(&mut self, reason: EpisodeCloseReasonV4) -> Result<(), &'static str> {
        let Some(open_episode_id) = self.diagnostics.open_episode_id_v4() else {
            // Nothing open, so nothing is owed. A pending terminal that
            // outlived its file cannot be honored and must not leak into
            // the next episode's footer.
            self.pending_terminal_episode_id = None;
            return Ok(());
        };
        // A REMEMBERED terminal always wins over the reason the caller
        // happens to be closing with. Once an episode has terminated, the
        // only truthful footer says so, whether the footer finally
        // publishes on the retried step, on the next reset, or at end of
        // input.
        let reason = if self.pending_terminal_episode_id == Some(open_episode_id) {
            EpisodeCloseReasonV4::EpisodeTerminal
        } else {
            reason
        };
        self.diagnostics
            .close_episode_v4(reason)
            .map_err(|_| "model_guided_search_diagnostics_write_failed")?;
        self.pending_terminal_episode_id = None;
        Ok(())
    }

    /// Records that `episode_id` reached a terminal and publishes its
    /// footer. The terminal is remembered BEFORE the publish is attempted,
    /// so a transient failure leaves the reason owed rather than lost.
    fn close_terminal_episode_v1(&mut self, episode_id: u64) -> Result<(), &'static str> {
        if self.diagnostics.open_episode_id_v4() == Some(episode_id) {
            self.pending_terminal_episode_id = Some(episode_id);
        }
        self.close_episode_v1(EpisodeCloseReasonV4::EpisodeTerminal)
    }

    /// Whether an episode terminated without its footer reaching disk.
    fn has_pending_terminal_close_v1(&self) -> bool {
        self.pending_terminal_episode_id.is_some()
    }

    /// Reports the outer response boundary to the diagnostics writer: the
    /// client's wait for this request is over, so the tail that followed
    /// the record this request published can be measured.
    fn note_request_completed_v1(&mut self) {
        self.diagnostics.note_request_completed_v4();
    }
}

struct ShadowScorerServiceV1 {
    model: Box<dyn ShadowModelScorerV1>,
    opponent_model: Option<Box<dyn ShadowModelScorerV1>>,
    population_opponent: Option<LadderOpponentEngineV1>,
    identity: ShadowCheckpointIdentityV1,
    candidate_selector: ShadowCandidateSelectorV1,
    protocol: ShadowStdioProtocolV1,
    /// Present exactly when `candidate_selector` is
    /// `ModelGuidedSearch`; the two are installed together by
    /// `run_checkpoint_shadow_stdio_with_model_guided_search_v1` and the
    /// selector fails closed if this is absent.
    search: Option<ModelGuidedSearchRuntimeV1>,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    active: Option<ActiveShadowSessionV1>,
    teacher_export: Option<XmageCp7TeacherJsonlWriterV1>,
    outcome_export: Option<XmageCp7OutcomeJsonlWriterV1>,
    export_poisoned: bool,
}

impl ShadowScorerServiceV1 {
    fn load_v1(authority: ShadowCheckpointAuthorityV1) -> Result<Self, ShadowScorerStartupErrorV1> {
        // ADAPTATION (fable/shadow-scorer-on-main-v1): restored from the
        // source lineage, unlike its two siblings. The source lineage's
        // XmageCp7OutcomeDerivative handling had three sub-paths: this one
        // (fixed_native_state, no missing-module dependency), a
        // bounded_value_search sub-path (needed the missing structured
        // -policy-successor/-residual modules), and a fallback
        // xmage-cp7-outcome-reinforce derivative (needed the missing
        // native_xmage_cp7_outcome_reinforce_v1 module). Only this
        // sub-path's dependencies (load_fixed_native_state_v1 and its
        // FixedNativeStateShadowModelScorerV1) are unaffected by the
        // missing modules, so it is kept; the other two fall through to
        // load_checkpoint_v1's existing unconditional rejection of
        // XmageCp7OutcomeDerivative below, same as Cp7BehaviorCloneDerivative.
        if let ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root } = &authority {
            let fixed_native_state = root
                .join(FIXED_NATIVE_STATE_MANIFEST_FILENAME_V1)
                .try_exists()
                .map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
            if fixed_native_state {
                let loaded = load_fixed_native_state_v1(root)?;
                let identity = ShadowCheckpointIdentityV1 {
                    authority_kind: loaded.authority_kind.clone(),
                    source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    source_generation: SOURCE_GENERATION_V1,
                    source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                    source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                    source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                    source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                    loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    loaded_generation: loaded.adam_step,
                    loaded_checkpoint_sha256: lower_hex_raw32_v1(loaded.manifest_sha256),
                    loaded_payload_sha256: lower_hex_raw32_v1(loaded.payload_sha256),
                    loaded_train_state_sha256: lower_hex_raw32_v1(loaded.native_state_sha256),
                    model_parameter_sha256: lower_hex_raw32_v1(loaded.model_parameter_sha256),
                    environment_trajectory_contract: FIXED_NATIVE_STATE_ENVIRONMENT_CONTRACT_V1,
                    sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                    sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                };
                eprintln!(
                    "FIXED_NATIVE_STATE authority_kind={} source_result_sha256={} manifest_sha256={} payload_sha256={} native_state_sha256={} model_parameter_sha256={} adam_step={}",
                    identity.authority_kind,
                    lower_hex_raw32_v1(loaded.source_result_sha256),
                    identity.loaded_checkpoint_sha256,
                    identity.loaded_payload_sha256,
                    identity.loaded_train_state_sha256,
                    identity.model_parameter_sha256,
                    identity.loaded_generation,
                );
                return Ok(Self {
                    model: Box::new(loaded.scorer),
                    opponent_model: None,
                    population_opponent: None,
                    identity,
                    candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                    protocol: ShadowStdioProtocolV1::V1,
                    search: None,
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
        }
        let loaded = load_checkpoint_v1(authority)?;
        Ok(Self {
            model: Box::new(NativeShadowModelScorerV1 {
                inference: loaded.inference,
            }),
            opponent_model: None,
            population_opponent: None,
            identity: loaded.identity,
            candidate_selector: ShadowCandidateSelectorV1::PolicySample,
            protocol: ShadowStdioProtocolV1::V1,
            search: None,
            max_physical_decisions: loaded.max_physical_decisions,
            max_policy_steps: loaded.max_policy_steps,
            active: None,
            teacher_export: None,
            outcome_export: None,
            export_poisoned: false,
        })
    }

    #[cfg(test)]
    fn with_test_model_v1(model: Box<dyn ShadowModelScorerV1>) -> Self {
        Self {
            model,
            opponent_model: None,
            population_opponent: None,
            identity: ShadowCheckpointIdentityV1 {
                authority_kind: "test-only".to_owned(),
                source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                source_generation: SOURCE_GENERATION_V1,
                source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                loaded_generation: SOURCE_GENERATION_V1,
                loaded_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                loaded_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                loaded_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                model_parameter_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1.to_owned(),
                environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
            },
            candidate_selector: ShadowCandidateSelectorV1::PolicySample,
            protocol: ShadowStdioProtocolV1::V1,
            search: None,
            max_physical_decisions: 128,
            max_policy_steps: 16_384,
            active: None,
            teacher_export: None,
            outcome_export: None,
            export_poisoned: false,
        }
    }

    fn install_teacher_export_v1(
        &mut self,
        writer: XmageCp7TeacherJsonlWriterV1,
    ) -> Result<(), ()> {
        if self.export_poisoned || self.active.is_some() || self.teacher_export.is_some() {
            return Err(());
        }
        self.teacher_export = Some(writer);
        Ok(())
    }

    fn install_outcome_export_v1(
        &mut self,
        writer: XmageCp7OutcomeJsonlWriterV1,
    ) -> Result<(), ()> {
        if self.export_poisoned || self.active.is_some() || self.outcome_export.is_some() {
            return Err(());
        }
        self.outcome_export = Some(writer);
        Ok(())
    }

    fn score_current_model_output_v1(
        model: &dyn ShadowModelScorerV1,
        session: &FastActorSessionV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
    ) -> Result<(FastActorDecisionV1, ShadowModelOutputV1), &'static str> {
        let expected = match session.current_response() {
            FastActorResponseV1::Decision(expected) => expected,
            FastActorResponseV1::Terminal(_) => return Err("value_search_expected_decision"),
        };
        let packet = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::encode_packet(
            session,
            expected,
            &mut Default::default(),
            OwnedFlatScoringDecisionV2::default(),
        )
        .map_err(|_| "value_search_decision_encoding_failed")?;
        let decision = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_decision(&packet);
        if !<FlatScoredFamilyV2 as FlatScoredFamilyCore>::expected_matches_binding(
            expected, decision,
        ) {
            return Err("value_search_decision_binding_mismatch");
        }
        let view = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_view(&packet);
        let output = model
            .score_v1(
                view,
                structured_history,
                player_seat_index_v1(expected.acting_player),
                expected.substep_count,
            )
            .map_err(|_| "value_search_checkpoint_scoring_failed")?;
        drop(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::into_owned_packet(packet));
        if output.logits.len()
            != usize::try_from(expected.legal_action_count)
                .map_err(|_| "value_search_score_width_invalid")?
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err("value_search_checkpoint_score_invalid");
        }
        Ok((expected, output))
    }

    fn score_branch_current_decision_v1(
        model: &dyn ShadowModelScorerV1,
        session: &FastActorSessionV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
    ) -> Result<(ScoredCurrentDecisionV1, ShadowModelOutputV1), &'static str> {
        let expected = match session.current_response() {
            FastActorResponseV1::Decision(expected) => expected,
            FastActorResponseV1::Terminal(_) => return Err("depth8_search_expected_decision"),
        };
        let packet = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::encode_packet(
            session,
            expected,
            &mut Default::default(),
            OwnedFlatScoringDecisionV2::default(),
        )
        .map_err(|_| "depth8_search_decision_encoding_failed")?;
        let decision = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_decision(&packet);
        if !<FlatScoredFamilyV2 as FlatScoredFamilyCore>::expected_matches_binding(
            expected, decision,
        ) {
            return Err("depth8_search_decision_binding_mismatch");
        }
        let binding = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_binding(&packet);
        let view = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_view(&packet);
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer
            .fill(view, &mut tensor)
            .map_err(|_| "depth8_search_decision_tensorization_failed")?;
        let output = model
            .score_v1(
                view,
                structured_history,
                player_seat_index_v1(expected.acting_player),
                expected.substep_count,
            )
            .map_err(|_| "depth8_search_checkpoint_scoring_failed")?;
        if output.logits.len()
            != usize::try_from(expected.legal_action_count)
                .map_err(|_| "depth8_search_score_width_invalid")?
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err("depth8_search_checkpoint_score_invalid");
        }
        // Branch decisions are never exported. Only the tensor and action binding
        // are needed to advance the branch and update its structured history.
        let scored = ScoredCurrentDecisionV1 {
            expected,
            binding,
            tensor,
            action_semantics: Vec::new(),
            logits_f32_bits: output.logits.iter().map(|value| value.to_bits()).collect(),
            value_f32_bits: output.value.to_bits(),
            structured_parent_logits_f32_bits: output
                .structured_parent_logits
                .as_ref()
                .map(|values| values.iter().map(|value| value.to_bits()).collect()),
            structured_parent_value_f32_bits: output.structured_parent_value.map(f32::to_bits),
            model_input_sha256: String::new(),
            diagnostic_state_hash_u64_hex: String::new(),
            core_environment_hash_u64_hex: String::new(),
            actor_physical_decision_ordinal: 0,
            candidate_action_seed_u64_hex: None,
            selected_action_index: None,
        };
        drop(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::into_owned_packet(packet));
        Ok((scored, output))
    }

    #[allow(clippy::too_many_arguments)]
    fn depth8_branch_value_v1(
        model: &dyn ShadowModelScorerV1,
        opponent_model: Option<&dyn ShadowModelScorerV1>,
        sampled_root: &FastActorSessionV1,
        root_scored: &ScoredCurrentDecisionV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
        candidate_seat: PlayerSeatV1,
        selected: u32,
        sample_index: usize,
    ) -> Result<Depth8BranchValueV1, &'static str> {
        let candidate_index = usize::from(player_seat_index_v1(candidate_seat));
        let mut branch = sampled_root.clone();
        let mut branch_history = StructuredHistoryStateV1 {
            completed: structured_history.to_vec(),
            pending: None,
        };
        branch_history
            .accept_selected_v1(root_scored, selected)
            .map_err(|_| "depth8_search_history_update_failed")?;
        let mut next = branch
            .consume_current_flat_action_slice_v2(root_scored.binding.action_binding, selected)
            .map_err(|_| "depth8_search_branch_consume_failed")?;
        for continuation_index in 0..DEPTH8_VALUE_CONTINUATION_STEPS_V1 {
            let expected = match next {
                FastActorResponseV1::Terminal(terminal) => {
                    return Ok(Depth8BranchValueV1 {
                        value: terminal.terminal_reward[candidate_index] as f32,
                        reached_natural_terminal: true,
                    });
                }
                FastActorResponseV1::Decision(expected) => expected,
            };
            let opponent_controls_branch = expected.acting_player != candidate_seat;
            let branch_model = if opponent_controls_branch {
                opponent_model.unwrap_or(model)
            } else {
                model
            };
            let (branch_scored, output) = Self::score_branch_current_decision_v1(
                branch_model,
                &branch,
                &branch_history.completed,
            )?;
            let branch_selected = if opponent_controls_branch && opponent_model.is_some() {
                let action_seed = DEPTH8_CP7_OPPONENT_ACTION_DOMAIN_V1
                    ^ root_scored
                        .expected
                        .episode_id
                        .wrapping_mul(0x9e37_79b1_85eb_ca87)
                    ^ root_scored
                        .expected
                        .physical_decision_id
                        .wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
                    ^ (sample_index as u64).wrapping_mul(0x1656_67b1_9e37_79f9)
                    ^ (continuation_index as u64).wrapping_mul(0xd6e8_feb8_6659_fd93);
                FastCategoricalScratch::default()
                    .sample(&output.logits, action_seed)
                    .map_err(|_| "depth8_search_opponent_sampling_failed")?
            } else {
                let mut best = 0usize;
                for index in 1..output.logits.len() {
                    if output.logits[index].total_cmp(&output.logits[best]).is_gt() {
                        best = index;
                    }
                }
                best
            };
            let branch_selected =
                u32::try_from(branch_selected).map_err(|_| "depth8_search_action_index_invalid")?;
            branch_history
                .accept_selected_v1(&branch_scored, branch_selected)
                .map_err(|_| "depth8_search_history_update_failed")?;
            next = branch
                .consume_current_flat_action_slice_v2(
                    branch_scored.binding.action_binding,
                    branch_selected,
                )
                .map_err(|_| "depth8_search_branch_consume_failed")?;
        }
        match next {
            FastActorResponseV1::Terminal(terminal) => Ok(Depth8BranchValueV1 {
                value: terminal.terminal_reward[candidate_index] as f32,
                reached_natural_terminal: true,
            }),
            FastActorResponseV1::Decision(_) => {
                let (expected, output) =
                    Self::score_current_model_output_v1(model, &branch, &branch_history.completed)?;
                Ok(Depth8BranchValueV1 {
                    value: if expected.acting_player == candidate_seat {
                        output.value
                    } else {
                        -output.value
                    },
                    reached_natural_terminal: false,
                })
            }
        }
    }

    fn depth8_history_value_selection_v1(
        model: &dyn ShadowModelScorerV1,
        _opponent_model: Option<&dyn ShadowModelScorerV1>,
        _session: &FastActorSessionV1,
        scored: &ScoredCurrentDecisionV1,
        _structured_history: &[NativeStructuredHistoryEntryV1],
        _candidate_seat: PlayerSeatV1,
        _fallback_selected_index: u32,
    ) -> Result<Option<Depth8TeacherDiagnosticV1>, &'static str> {
        let expected = scored.expected;
        if !model.uses_structured_history_v1()
            || expected.decision_kind != FastActorDecisionKindV1::Surface
            || expected.substep_index != 0
            || expected.substep_count != 1
            || scored.actor_physical_decision_ordinal < ONE_STEP_VALUE_MIN_PHYSICAL_DECISION_V1
            || expected.legal_action_count < ONE_STEP_VALUE_MIN_ACTIONS_V1
            || expected.legal_action_count > ONE_STEP_VALUE_MAX_ACTIONS_V1
        {
            return Ok(None);
        }
        // ADAPTATION (fable/shadow-scorer-on-main-v1): the rest of this
        // function (information-set redeterminization sampling and
        // teacher-diagnostic aggregation) is removed for the same reason
        // as one_step_history_value_selection_v1 above: it needed
        // FastActorSessionV1::snapshot_current_actor_information_set_v1,
        // a primitive main's rl_session.rs does not have at all (unlike
        // the source lineage). Provably unreachable dead code on this
        // ported lineage regardless, for the exact same reason: no kept
        // model scorer ever returns true from uses_structured_history_v1.
        // Fails closed with an explicit, distinct error rather than
        // silently miscomputing if this is ever reached.
        Err("depth8_search_redeterminization_unavailable_on_this_lineage")
    }

    fn one_step_history_value_selection_v1(
        model: &dyn ShadowModelScorerV1,
        _session: &FastActorSessionV1,
        scored: &ScoredCurrentDecisionV1,
        _structured_history: &[NativeStructuredHistoryEntryV1],
        _candidate_seat: PlayerSeatV1,
        _fallback_selected_index: u32,
        _candidate_turn_only: bool,
    ) -> Result<Option<u32>, &'static str> {
        let expected = scored.expected;
        if !model.uses_structured_history_v1()
            || expected.decision_kind != FastActorDecisionKindV1::Surface
            || expected.substep_index != 0
            || expected.substep_count != 1
            || scored.actor_physical_decision_ordinal < ONE_STEP_VALUE_MIN_PHYSICAL_DECISION_V1
            || expected.legal_action_count < ONE_STEP_VALUE_MIN_ACTIONS_V1
            || expected.legal_action_count > ONE_STEP_VALUE_MAX_ACTIONS_V1
        {
            return Ok(None);
        }
        // ADAPTATION (fable/shadow-scorer-on-main-v1): the rest of
        // this function (information-set redeterminization sampling
        // and value-margin aggregation) is removed. It needed
        // FastActorSessionV1::snapshot_current_actor_information_set_v1,
        // a primitive main's rl_session.rs does not have at all
        // (unlike the source lineage). This is provably unreachable
        // dead code on this ported lineage regardless: the guard
        // above already requires model.uses_structured_history_v1(),
        // and every model scorer kept in this port returns the trait
        // default (false) for it, since every scorer that overrode
        // it to true depended on one of the missing structured
        // -history/policy-residual/successor modules and was removed.
        // Fails closed with an explicit, distinct error rather than
        // silently miscomputing if this is ever reached.
        Err("value_search_redeterminization_unavailable_on_this_lineage")
    }

    /// The `ModelGuidedSearch` selector.
    ///
    /// Runs AFTER the policy forward and after the sampler has produced
    /// `policy_sample`, and returns an index that OVERRIDES it. That
    /// ordering is deliberate and load-bearing: `handle_step_v1` rejects
    /// any `selected_index` that is not `scored.selected_action_index`, so
    /// as long as the override lands in that same field before the
    /// response is emitted, the existing protocol invariant -- Java can
    /// never override the Rust-side choice -- holds for the wrapper
    /// exactly as it does for a raw policy sample. No change to
    /// `handle_step_v1` is needed or made.
    ///
    /// PURITY. The returned index is a function of (checkpoint weights,
    /// game seed and episode, decision identity, authority) and nothing
    /// else. The search's own seeds come from
    /// `derive_simulation_seed_v1(authority digest, decision, simulation
    /// ordinal, player)`; the tree is rebuilt from scratch per decision;
    /// the two stability halves run on domain-separated digests and their
    /// results are recorded, never read. Wall time is measured after the
    /// fact and only ever written to a diagnostics field: `request_received`
    /// below is read exactly once, to subtract, after `full.selected_index`
    /// is already fixed.
    ///
    /// LATENCY. `request_received` is the instant `handle_line_v1` took
    /// delivery of the request line, so the recorded protocol window
    /// covers the packet encode, the tensorization, the model forward, the
    /// policy sample, the search, the halves, and this record's own
    /// construction. The previous shape started its clock here, after all
    /// of the pre-search work, and stopped it before the record was built
    /// and published, so a request genuinely sitting near the 4 s SLO or
    /// the 20 s hard timeout could be recorded as comfortably inside it.
    ///
    /// MXCSR. This is layer three of the S0 requirement: the thread about
    /// to run a search normalizes its own control register (a worker
    /// thread that has never searched normalizes here), and then verifies
    /// fail-closed BEFORE the first search forward. A mismatch is a hard
    /// error returned through the protocol, never a fallback to the policy
    /// sample: silently playing an unwrapped action while the panel
    /// believes it measured the wrapper would be the worst possible
    /// failure mode. The gate now lives inside
    /// [`model_guided_search_pinned_evaluator_v1`], which is the ONLY
    /// constructor of the production leaf evaluator, so no caller (this
    /// one or the S1 feasibility replay) can reach a search forward
    /// without having passed it. That is structurally stronger than the
    /// earlier top-of-function call, which any second caller could have
    /// forgotten; the one observable change is that a broken MXCSR is now
    /// reported after `model_guided_search_model_not_search_capable` and
    /// `model_guided_search_authority_unbound` rather than before them,
    /// which is a message-precedence difference on a
    /// two-things-wrong-at-once path and not a difference in what is
    /// admitted.
    fn model_guided_search_selection_v1(
        model: &dyn ShadowModelScorerV1,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        policy_sample: u32,
        search: &mut ModelGuidedSearchRuntimeV1,
        request_received: Instant,
        protocol_request_kind: ProtocolRequestKindV4,
    ) -> Result<u32, &'static str> {
        let capable = model
            .search_capable_v1()
            .ok_or("model_guided_search_model_not_search_capable")?;
        let net = capable.search_native_net_v1();
        // Field-disjoint borrows: the searcher reads `bound` while the
        // diagnostics writer needs `&mut diagnostics`. Destructuring is
        // what lets both live at once without cloning the authority
        // record on every decision.
        let ModelGuidedSearchRuntimeV1 {
            value_domain,
            stability_halves_enabled,
            diagnostics,
            bound,
            ..
        } = search;
        let value_domain = *value_domain;
        let stability_halves_enabled = *stability_halves_enabled;
        let bound = bound
            .as_ref()
            .ok_or("model_guided_search_authority_unbound")?;
        let core = &bound.core;
        let evaluator = model_guided_search_pinned_evaluator_v1(net, value_domain)?;

        let started = Instant::now();
        let full =
            model_guided_search_full_budget_v1(core, &evaluator, &value_domain, session, expected)?;
        let full_micros = elapsed_micros_v1(started);

        // Diagnostic stability halves. Their results are recorded and
        // never consulted; the chosen action above is already fixed.
        //
        // They run SYNCHRONOUSLY inside the decision, so when they are
        // enabled their cost is part of the protocol latency the panel
        // host actually pays. That is why `--model-guided-search-stability
        // -halves off` exists and why `search_ceiling_status` is classified
        // from `search_micros` below: a formal panel measuring product
        // latency turns them off, and the record says which ran.
        let (stability, half_a_micros, half_b_micros) = if stability_halves_enabled {
            let half_a_started = Instant::now();
            let half_a = core
                .select_action_seed_half_v1(
                    session,
                    expected,
                    &evaluator,
                    &value_domain,
                    ModelGuidedSearchSeedHalfV1::A,
                )
                .map_err(|error| {
                    eprintln!("MODEL_GUIDED_SEARCH_FAILED stability_half_a error={error}");
                    "model_guided_search_stability_half_failed"
                })?;
            let half_a_micros = elapsed_micros_v1(half_a_started);
            let half_b_started = Instant::now();
            let half_b = core
                .select_action_seed_half_v1(
                    session,
                    expected,
                    &evaluator,
                    &value_domain,
                    ModelGuidedSearchSeedHalfV1::B,
                )
                .map_err(|error| {
                    eprintln!("MODEL_GUIDED_SEARCH_FAILED stability_half_b error={error}");
                    "model_guided_search_stability_half_failed"
                })?;
            let half_b_micros = elapsed_micros_v1(half_b_started);
            (
                Some(StabilityV4 {
                    half_a_selected_index: half_a.selected_index,
                    half_b_selected_index: half_b.selected_index,
                    half_transition_budget: half_a.transitions_used.max(half_b.transitions_used),
                    halves_agree: half_a.selected_index == half_b.selected_index,
                    halves_agree_with_full_budget: half_a.selected_index == full.selected_index
                        && half_b.selected_index == full.selected_index,
                }),
                half_a_micros,
                half_b_micros,
            )
        } else {
            (None, 0, 0)
        };
        let search_micros = elapsed_micros_v1(started);

        let mut record = SearchDecisionRecordV4 {
            // Chain and contract fields are writer-assigned; see
            // `write_decision_v4`.
            contract: String::new(),
            schema_version: 0,
            record_kind: String::new(),
            record_ordinal: 0,
            previous_record_sha256: String::new(),
            decision_ordinal: 0,
            episode_id: expected.episode_id,
            step: expected.step,
            physical_decision_id: expected.physical_decision_id,
            substep_index: expected.substep_index,
            acting_player: expected.acting_player,
            legal_action_count: expected.legal_action_count,
            search_authority_digest_sha256: bound.authority_digest_sha256.clone(),
            requested_transitions: core.authority().transition_budget,
            actual_transitions: full.transitions_used,
            simulations: full.simulations,
            tree_node_count: full.tree_node_count,
            leaf_census: full.leaf_census,
            root_statistics_digest_sha256: lower_hex_sha256_v4(root_statistics_digest_v4(&full)),
            chosen_action_index: full.selected_index,
            visit_margin: visit_margin_v4(&full),
            policy_sample_index: policy_sample,
            search_overrode_policy_sample: full.selected_index != policy_sample,
            stability,
            stability_halves_enabled,
            protocol_request_kind,
            // The SEARCH-ONLY verdict, classified from `search_micros`:
            // the full-budget search plus the halves when they ran. It is
            // deliberately NOT the protocol verdict, which additionally
            // needs the pre-search protocol work (already inside
            // `decision_micros` below) and this record's own publication
            // (which only a later record can observe). Read the protocol
            // verdict with `episode_decision_ceilings_v4`.
            //
            // Computed after the decision is already fixed; never acted on.
            search_ceiling_status: CeilingStatusV4::classify_v4(search_micros as f64 / 1_000_000.0),
            wall_time: WallTimeV4 {
                full_search_micros: full_micros,
                stability_half_a_micros: half_a_micros,
                stability_half_b_micros: half_b_micros,
                search_micros,
                // Filled in below, once the record is otherwise complete:
                // record construction and the writer's own bookkeeping are
                // synchronous too, and leaving them outside the measured
                // interval is how a near-boundary request gets recorded as
                // comfortably inside the boundary.
                decision_micros: 0,
                // Writer-assigned, both of them: the diagnostics writer
                // knows how long it spent publishing the previous record
                // and how long the response tail that followed it took,
                // and a caller that could set either could understate the
                // latency it is being measured on.
                previous_record_publish_micros: 0,
                previous_record_response_micros: 0,
            },
        };
        record.wall_time.decision_micros = elapsed_micros_v1(request_received);
        diagnostics
            .write_decision_v4(record)
            .map_err(|_| "model_guided_search_diagnostics_write_failed")?;
        Ok(full.selected_index)
    }

    #[allow(clippy::too_many_arguments)]
    fn score_session_v1(
        model: &dyn ShadowModelScorerV1,
        opponent_model: Option<&dyn ShadowModelScorerV1>,
        population_opponent: Option<&LadderOpponentEngineV1>,
        population_opponent_member: Option<OpponentLadderPoolMemberV2>,
        candidate_selector: ShadowCandidateSelectorV1,
        search: Option<&mut ModelGuidedSearchRuntimeV1>,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        session: &FastActorSessionV1,
        schedule: &mut NativeLaneScheduleStateV1,
        candidate_seat: PlayerSeatV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
        // The OUTER decision boundary: when `handle_line_v1` took delivery
        // of the request whose response this scoring produces. Everything
        // below is synchronous work the client is waiting on, so the
        // diagnostics record's protocol window has to start here and not
        // at the search. Threaded through rather than sampled locally for
        // exactly that reason.
        request_received: Instant,
        protocol_request_kind: ProtocolRequestKindV4,
    ) -> Result<Option<ScoredCurrentDecisionV1>, &'static str> {
        let expected = match session.current_response() {
            FastActorResponseV1::Decision(expected) => expected,
            FastActorResponseV1::Terminal(terminal) => {
                schedule
                    .validate_terminal(&terminal)
                    .map_err(|_| "native_terminal_validation_failed")?;
                return Ok(None);
            }
        };
        let preflight = schedule
            .preflight_action_seed(expected, max_physical_decisions, max_policy_steps)
            .map_err(|_| "native_schedule_preflight_failed")?;
        let packet = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::encode_packet(
            session,
            expected,
            &mut Default::default(),
            OwnedFlatScoringDecisionV2::default(),
        )
        .map_err(|_| "decision_encoding_failed")?;
        let decision = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_decision(&packet);
        if !<FlatScoredFamilyV2 as FlatScoredFamilyCore>::expected_matches_binding(
            expected, decision,
        ) {
            return Err("decision_binding_mismatch");
        }
        let binding = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_binding(&packet);
        let view = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_view(&packet);
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer
            .fill(view, &mut tensor)
            .map_err(|_| "decision_tensorization_failed")?;
        let model_input_sha256 = model_input_sha256_v1(&tensor);
        let output = model
            .score_v1(
                view,
                structured_history,
                player_seat_index_v1(expected.acting_player),
                expected.substep_count,
            )
            .map_err(|_| "checkpoint_scoring_failed")?;
        if output.logits.len()
            != usize::try_from(expected.legal_action_count).map_err(|_| "score_width_invalid")?
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err("checkpoint_score_invalid");
        }
        let candidate_controls_current_actor = expected.acting_player == candidate_seat;
        let (candidate_action_seed_u64_hex, selected_action_index) =
            if candidate_controls_current_actor {
                let selected = FastCategoricalScratch::default()
                    .sample(&output.logits, preflight.action_seed)
                    .map_err(|_| "candidate_sampling_failed")?;
                (
                    Some(u64_hex_v1(preflight.action_seed)),
                    Some(u32::try_from(selected).map_err(|_| "selected_index_invalid")?),
                )
            } else if let (Some(engine), Some(member)) =
                (population_opponent, population_opponent_member)
            {
                let selected = match member {
                    OpponentLadderPoolMemberV2::UniformFloor => {
                        LadderOpponentEngineV1::select_floor_action_v1(
                            base_seed,
                            expected.episode_id,
                            preflight.actor_physical_decision_ordinal,
                            expected.substep_index,
                            expected.legal_action_count,
                        )
                    }
                    policy_member => {
                        engine.select_policy_action_v1(policy_member, view, preflight.action_seed)
                    }
                }
                .map_err(|_| "population_opponent_sampling_failed")?;
                (None, Some(selected))
            } else {
                (None, None)
            };
        let action_semantics = session
            .diagnostic_current_action_semantics()
            .ok_or("action_semantics_missing")?;
        if action_semantics.len() != output.logits.len() {
            return Err("action_semantics_width_mismatch");
        }
        let mut scored = ScoredCurrentDecisionV1 {
            expected,
            binding,
            tensor,
            action_semantics,
            logits_f32_bits: output.logits.iter().map(|value| value.to_bits()).collect(),
            value_f32_bits: output.value.to_bits(),
            structured_parent_logits_f32_bits: output
                .structured_parent_logits
                .as_ref()
                .map(|values| values.iter().map(|value| value.to_bits()).collect()),
            structured_parent_value_f32_bits: output.structured_parent_value.map(f32::to_bits),
            model_input_sha256,
            diagnostic_state_hash_u64_hex: u64_hex_v1(session.diagnostic_state_hash()),
            core_environment_hash_u64_hex: u64_hex_v1(session.privileged_core_environment_hash()),
            actor_physical_decision_ordinal: preflight.actor_physical_decision_ordinal,
            candidate_action_seed_u64_hex,
            selected_action_index,
        };
        if candidate_controls_current_actor
            && matches!(
                candidate_selector,
                ShadowCandidateSelectorV1::OneStepHistoryValueBootstrap
                    | ShadowCandidateSelectorV1::CandidateTurnOnlyOneStepBoundedValueBootstrap
                    | ShadowCandidateSelectorV1::Depth8HistoryValueBootstrap
                    | ShadowCandidateSelectorV1::Depth8BoundedValueTeacherShadow
                    | ShadowCandidateSelectorV1::Depth8Cp7OpponentHistoryValueBootstrap
                    | ShadowCandidateSelectorV1::ModelGuidedSearch
            )
        {
            let fallback = scored
                .selected_action_index
                .ok_or("value_search_fallback_selection_missing")?;
            let selected = match candidate_selector {
                ShadowCandidateSelectorV1::ModelGuidedSearch => {
                    Some(Self::model_guided_search_selection_v1(
                        model,
                        session,
                        expected,
                        fallback,
                        search.ok_or("model_guided_search_runtime_missing")?,
                        request_received,
                        protocol_request_kind,
                    )?)
                }
                ShadowCandidateSelectorV1::OneStepHistoryValueBootstrap => {
                    Self::one_step_history_value_selection_v1(
                        model,
                        session,
                        &scored,
                        structured_history,
                        candidate_seat,
                        fallback,
                        false,
                    )?
                }
                ShadowCandidateSelectorV1::CandidateTurnOnlyOneStepBoundedValueBootstrap => {
                    Self::one_step_history_value_selection_v1(
                        model,
                        session,
                        &scored,
                        structured_history,
                        candidate_seat,
                        fallback,
                        true,
                    )?
                }
                ShadowCandidateSelectorV1::Depth8HistoryValueBootstrap => {
                    Self::depth8_history_value_selection_v1(
                        model,
                        None,
                        session,
                        &scored,
                        structured_history,
                        candidate_seat,
                        fallback,
                    )?
                    .map(|diagnostic| diagnostic.teacher_selected_index)
                }
                ShadowCandidateSelectorV1::Depth8BoundedValueTeacherShadow => {
                    if let Some(diagnostic) = Self::depth8_history_value_selection_v1(
                        model,
                        None,
                        session,
                        &scored,
                        structured_history,
                        candidate_seat,
                        fallback,
                    )? {
                        let json = serde_json::to_string(&diagnostic)
                            .map_err(|_| "depth8_search_diagnostic_serialization_failed")?;
                        eprintln!("NATIVE_DEPTH8_BOUNDED_TEACHER_JSON {json}");
                    }
                    None
                }
                ShadowCandidateSelectorV1::Depth8Cp7OpponentHistoryValueBootstrap => {
                    Self::depth8_history_value_selection_v1(
                        model,
                        Some(opponent_model.ok_or("cp7_opponent_model_missing")?),
                        session,
                        &scored,
                        structured_history,
                        candidate_seat,
                        fallback,
                    )?
                    .map(|diagnostic| diagnostic.teacher_selected_index)
                }
                ShadowCandidateSelectorV1::PolicySample => None,
            };
            if let Some(selected) = selected {
                scored.selected_action_index = Some(selected);
            }
        }
        drop(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::into_owned_packet(packet));
        Ok(Some(scored))
    }

    fn decision_body_v1(
        active: &ActiveShadowSessionV1,
        include_initial_libraries: bool,
        protocol: ShadowStdioProtocolV1,
    ) -> Result<DecisionBodyV1, ()> {
        let scored = active.current.as_ref().ok_or(())?;
        Ok(DecisionBodyV1 {
            deck_ids: active.deck_ids.clone(),
            randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
            base_seed_u64_hex: u64_hex_v1(active.base_seed),
            pair_index: active.pair_index,
            pair_environment_seed_u64_hex: u64_hex_v1(active.pair_environment_seed),
            initial_library_card_definition_ids: include_initial_libraries
                .then(|| active.initial_library_card_definition_ids.clone()),
            episode_id: scored.expected.episode_id,
            step: scored.expected.step,
            environment_revision: scored.expected.environment_revision,
            physical_decision_id: scored.expected.physical_decision_id,
            substep_index: scored.expected.substep_index,
            substep_count: scored.expected.substep_count,
            acting_player: scored.expected.acting_player,
            decision_kind: decision_kind_v1(scored.expected.decision_kind),
            legal_action_count: scored.expected.legal_action_count,
            candidate_seat: active.candidate_seat,
            candidate_controls_current_actor: scored.expected.acting_player
                == active.candidate_seat,
            actor_physical_decision_ordinal: scored.actor_physical_decision_ordinal,
            candidate_action_seed_u64_hex: scored.candidate_action_seed_u64_hex.clone(),
            selected_action_index: scored.selected_action_index,
            candidate_order_commitment_128_hex: lower_hex_bytes_v1(
                &scored.binding.action_binding.candidate_order_commitment,
            ),
            model_input_commitment: CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1,
            model_input_sha256: scored.model_input_sha256.clone(),
            diagnostic_state_hash_u64_hex: scored.diagnostic_state_hash_u64_hex.clone(),
            core_environment_hash_u64_hex: scored.core_environment_hash_u64_hex.clone(),
            logits_f32_bits: scored.logits_f32_bits.clone(),
            value_f32_bits: scored.value_f32_bits,
            action_semantics: scored.action_semantics.clone(),
            kernel_clock: protocol
                .carries_kernel_clock_v1()
                .then(|| KernelClockV2::from_session_v2(&active.session)),
        })
    }

    fn applied_action_v1(
        scored: &ScoredCurrentDecisionV1,
        selected_index: u32,
    ) -> Result<AppliedActionV1, ()> {
        let index = usize::try_from(selected_index).map_err(|_| ())?;
        Ok(AppliedActionV1 {
            episode_id: scored.expected.episode_id,
            step: scored.expected.step,
            candidate_order_commitment_128_hex: lower_hex_bytes_v1(
                &scored.binding.action_binding.candidate_order_commitment,
            ),
            model_input_sha256: scored.model_input_sha256.clone(),
            selected_index,
            selected_logit_f32_bits: *scored.logits_f32_bits.get(index).ok_or(())?,
            semantic: scored.action_semantics.get(index).ok_or(())?.clone(),
        })
    }

    fn terminal_body_v1(
        active: &ActiveShadowSessionV1,
        terminal: RlSessionTerminalV1,
        include_initial_libraries: bool,
    ) -> TerminalBodyV1 {
        TerminalBodyV1 {
            deck_ids: active.deck_ids.clone(),
            randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
            base_seed_u64_hex: u64_hex_v1(active.base_seed),
            pair_index: active.pair_index,
            pair_environment_seed_u64_hex: u64_hex_v1(active.pair_environment_seed),
            initial_library_card_definition_ids: include_initial_libraries
                .then(|| active.initial_library_card_definition_ids.clone()),
            terminal,
            candidate_seat: active.candidate_seat,
            diagnostic_state_hash_u64_hex: u64_hex_v1(active.session.diagnostic_state_hash()),
            core_environment_hash_u64_hex: u64_hex_v1(
                active.session.privileged_core_environment_hash(),
            ),
        }
    }

    fn handle_reset_v1(
        &mut self,
        request_id: String,
        episode_id: u64,
        base_seed: u64,
        request_received: Instant,
    ) -> ShadowScorerResponseV1 {
        if self
            .outcome_export
            .as_ref()
            .is_some_and(XmageCp7OutcomeJsonlWriterV1::has_open_episode_v1)
        {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "outcome_export_episode_incomplete",
                    "the previous outcome-export episode has no terminal",
                ),
            );
        }
        let episode_schedule = match native_trainer_episode_schedule_v1(base_seed, episode_id) {
            Ok(schedule) => schedule,
            Err(_) => {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "native_episode_schedule_invalid",
                        "base_seed and episode_id must satisfy the native trainer schedule",
                    ),
                );
            }
        };
        let candidate_seat = episode_schedule.learner_seat;
        let deck_ids = [
            CANONICAL_RALLY_DECK_ID.to_owned(),
            CANONICAL_RALLY_DECK_ID.to_owned(),
        ];
        let initial_library_card_definition_ids =
            legacy_rally_initial_library_orders_v1(episode_schedule.environment_seed);
        let session = match FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            episode_schedule.environment_seed,
            self.max_physical_decisions,
            self.max_policy_steps,
            deck_ids.clone(),
        ) {
            Ok(session) => session,
            Err(error) => {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(rl_error_code_v1(error.code), "session reset failed"),
                );
            }
        };
        let population_opponent_member = match self.population_opponent.as_ref() {
            Some(engine) => match engine.pool_member_for_episode_v1(base_seed, episode_id) {
                Ok(member) => Some(member),
                Err(_) => {
                    return response_v1(
                        Some(request_id),
                        &self.identity,
                        error_body_v1(
                            "population_opponent_schedule_invalid",
                            "the native population opponent could not be selected",
                        ),
                    );
                }
            },
            None => None,
        };
        // The search-diagnostics episode opens BEFORE the first decision is
        // scored, because that first call may already search: an episode
        // whose header had not been published yet would produce a decision
        // record with nothing to chain to. Publishing the header here also
        // means an episode that terminates before any searched decision
        // still leaves an auditable file naming the wrapper that was
        // configured for it.
        let checkpoint_identity = self.identity.clone();
        // Read off the LOADED net, so the authority names the architecture
        // that will actually run rather than one the caller assumed. A
        // model with no native net cannot be searched with, and saying so
        // here fails the episode closed instead of at the first decision.
        let net_architecture = self
            .model
            .search_capable_v1()
            .map(|capable| capable.search_native_net_v1().architecture_identity_v1());
        if let Some(search) = self.search.as_mut() {
            let Some(net_architecture) = net_architecture else {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "model_guided_search_model_not_search_capable",
                        "model-guided search episode could not be opened",
                    ),
                );
            };
            if let Err(code) = search.begin_episode_v1(
                &session,
                &checkpoint_identity,
                net_architecture,
                episode_id,
                base_seed,
                candidate_seat,
            ) {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(code, "model-guided search episode could not be opened"),
                );
            }
        }
        let mut schedule = NativeLaneScheduleStateV1::new(
            base_seed,
            episode_id,
            candidate_seat,
            population_opponent_member,
        );
        let structured_history = StructuredHistoryStateV1::default();
        let current = match Self::score_session_v1(
            self.model.as_ref(),
            self.opponent_model.as_deref(),
            self.population_opponent.as_ref(),
            population_opponent_member,
            self.candidate_selector,
            self.search.as_mut(),
            self.max_physical_decisions,
            self.max_policy_steps,
            base_seed,
            &session,
            &mut schedule,
            candidate_seat,
            &structured_history.completed,
            request_received,
            ProtocolRequestKindV4::Reset,
        ) {
            Ok(current) => current,
            Err(code) => {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(code, "initial decision scoring failed"),
                );
            }
        };
        // A reset that lands directly on a terminal has no decision to
        // search and never will, so its diagnostics episode closes here
        // rather than waiting for the next reset to replace it.
        if current.is_none() {
            if let Some(search) = self.search.as_mut() {
                if let Err(code) = search.close_terminal_episode_v1(episode_id) {
                    return response_v1(
                        Some(request_id),
                        &self.identity,
                        error_body_v1(code, "model-guided search episode could not be closed"),
                    );
                }
            }
        }
        let active = ActiveShadowSessionV1 {
            session,
            schedule,
            candidate_seat,
            population_opponent_member,
            deck_ids,
            base_seed,
            pair_index: episode_schedule.pair_index,
            pair_environment_seed: episode_schedule.environment_seed,
            initial_library_card_definition_ids,
            current,
            structured_history,
        };
        if let Some(export) = self.outcome_export.as_mut() {
            let write_result = export.begin_episode_v1(&active).and_then(|()| {
                match active.session.current_response() {
                    FastActorResponseV1::Decision(_) => Ok(()),
                    FastActorResponseV1::Terminal(terminal) => export
                        .terminal_record_v1(&active, terminal)
                        .and_then(|record| export.write_terminal_v1(&record).map_err(|_| ())),
                }
            });
            if write_result.is_err() {
                self.export_poisoned = true;
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "outcome_export_write_failed",
                        "the outcome episode could not be opened or persisted",
                    ),
                );
            }
        }
        let protocol = self.protocol;
        let body = match active.session.current_response() {
            FastActorResponseV1::Decision(_) => {
                match Self::decision_body_v1(&active, true, protocol) {
                    Ok(decision) => ShadowScorerResponseBodyV1::Decision {
                        decision,
                        applied_action: None,
                    },
                    Err(()) => error_body_v1("internal_protocol_error", "decision cache missing"),
                }
            }
            FastActorResponseV1::Terminal(terminal) => ShadowScorerResponseBodyV1::Terminal {
                terminal: Self::terminal_body_v1(&active, terminal, true),
                applied_action: None,
            },
        };
        self.active = Some(active);
        response_v1(Some(request_id), &self.identity, body)
    }

    fn handle_score_current_v1(
        &self,
        request_id: String,
        episode_id: u64,
        expected_step: u64,
    ) -> ShadowScorerResponseV1 {
        let protocol = self.protocol;
        let Some(active) = self.active.as_ref() else {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "no_active_session",
                    "reset is required before score_current",
                ),
            );
        };
        let current = active.session.current_response();
        let (bound_episode_id, bound_step) = match &current {
            FastActorResponseV1::Decision(expected) => (expected.episode_id, expected.step),
            FastActorResponseV1::Terminal(terminal) => {
                (terminal.episode_id, terminal.policy_step_count)
            }
        };
        if bound_episode_id != episode_id {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "episode_id_mismatch",
                    "episode_id does not match the active session",
                ),
            );
        }
        if bound_step != expected_step {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "expected_step_mismatch",
                    "expected_step does not match the current decision or terminal",
                ),
            );
        }
        match current {
            FastActorResponseV1::Decision(_) => {
                let body = match Self::decision_body_v1(active, false, protocol) {
                    Ok(decision) => ShadowScorerResponseBodyV1::Decision {
                        decision,
                        applied_action: None,
                    },
                    Err(()) => error_body_v1("internal_protocol_error", "decision cache missing"),
                };
                response_v1(Some(request_id), &self.identity, body)
            }
            FastActorResponseV1::Terminal(terminal) => response_v1(
                Some(request_id),
                &self.identity,
                ShadowScorerResponseBodyV1::Terminal {
                    terminal: Self::terminal_body_v1(active, terminal, false),
                    applied_action: None,
                },
            ),
        }
    }

    fn handle_step_v1(
        &mut self,
        request_id: String,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        expected_clock: Option<ExpectedKernelClockV2>,
        request_received: Instant,
    ) -> ShadowScorerResponseV1 {
        // RETRY an owed terminal footer first. If the terminal close failed
        // transiently, the session already reports this episode as
        // terminal, so the rejection below would answer the driver's retry
        // without ever attempting the footer again, and the episode would
        // eventually be closed as replaced or as a process exit: a game
        // that ended recorded as one that did not. Retrying here is the
        // only place a driver's own retry can reach.
        if self
            .search
            .as_ref()
            .is_some_and(ModelGuidedSearchRuntimeV1::has_pending_terminal_close_v1)
        {
            if let Some(search) = self.search.as_mut() {
                if let Err(code) = search.close_episode_v1(EpisodeCloseReasonV4::EpisodeTerminal) {
                    return response_v1(
                        Some(request_id),
                        &self.identity,
                        error_body_v1(code, "model-guided search episode could not be closed"),
                    );
                }
            }
        }
        let protocol = self.protocol;
        let Some(active) = self.active.as_mut() else {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1("no_active_session", "reset is required before step"),
            );
        };
        let Some(scored) = active.current.as_ref() else {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1("episode_already_terminal", "the active episode is terminal"),
            );
        };
        if scored.expected.episode_id != episode_id {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "episode_id_mismatch",
                    "episode_id does not match the active session",
                ),
            );
        }
        if scored.expected.step != expected_step {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "expected_step_mismatch",
                    "expected_step does not match the current decision",
                ),
            );
        }
        // Rendezvous guard (V2). `expected_step` only pins the scorer's own
        // decision counter, which both sides advance together by construction;
        // it cannot catch a caller whose game is at a different turn than the
        // kernel decision it is about to answer. The optional clock does, and
        // fails closed without advancing the session.
        if let Some(expected_clock) = expected_clock.as_ref() {
            if !KernelClockV2::from_session_v2(&active.session).matches_expected_v2(expected_clock)
            {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "clock_mismatch",
                        "expected_clock does not match the kernel clock of the current decision",
                    ),
                );
            }
        }
        if let Some(model_selected) = scored.selected_action_index {
            if selected_index != model_selected {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "selected_index_not_model_choice",
                        "candidate action must equal the Rust-side sampled index",
                    ),
                );
            }
        }
        let applied = match Self::applied_action_v1(scored, selected_index) {
            Ok(applied) => applied,
            Err(()) => {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "selected_index_out_of_range",
                        "selected_index is outside the current action row",
                    ),
                );
            }
        };
        let teacher_decision_record = match self.teacher_export.as_ref() {
            Some(export) if scored.expected.acting_player != active.candidate_seat => {
                match export.decision_record_v1(active, scored, selected_index) {
                    Ok(record) => Some(record),
                    Err(()) => {
                        return response_v1(
                            Some(request_id),
                            &self.identity,
                            error_body_v1(
                                "teacher_export_record_invalid",
                                "the accepted CP7 teacher row could not be constructed",
                            ),
                        );
                    }
                }
            }
            _ => None,
        };
        let outcome_decision_record = match self.outcome_export.as_ref() {
            Some(export) if scored.expected.acting_player == active.candidate_seat => {
                match export.decision_record_v1(active, scored, selected_index) {
                    Ok(record) => Some(record),
                    Err(()) => {
                        return response_v1(
                            Some(request_id),
                            &self.identity,
                            error_body_v1(
                                "outcome_export_record_invalid",
                                "the accepted candidate outcome row could not be constructed",
                            ),
                        );
                    }
                }
            }
            _ => None,
        };
        let session_before = active.session.snapshot_v1();
        let schedule_before = active.schedule;
        let expected = scored.expected;
        let binding = scored.binding;
        let mut structured_history_after = active.structured_history.clone();
        if self.model.uses_structured_history_v1()
            && structured_history_after
                .accept_selected_v1(scored, selected_index)
                .is_err()
        {
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "structured_history_update_failed",
                    "the selected action could not be added to complete public history",
                ),
            );
        }
        let next = match <FlatScoredFamilyV2 as FlatScoredFamilyCore>::consume(
            &mut active.session,
            binding,
            selected_index,
        ) {
            Ok(next) => next,
            Err(()) => {
                active.session.restore_v1(&session_before);
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "stale_or_invalid_action_binding",
                        "the selected action binding was rejected",
                    ),
                );
            }
        };
        if active.schedule.commit_action(expected).is_err() {
            active.session.restore_v1(&session_before);
            active.schedule = schedule_before;
            return response_v1(
                Some(request_id),
                &self.identity,
                error_body_v1(
                    "native_schedule_commit_failed",
                    "action schedule commit failed",
                ),
            );
        }
        if let FastActorResponseV1::Terminal(terminal) = &next {
            if active.schedule.validate_terminal(terminal).is_err() {
                active.session.restore_v1(&session_before);
                active.schedule = schedule_before;
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "native_terminal_validation_failed",
                        "terminal violates the native trajectory contract",
                    ),
                );
            }
        }
        let next_scored = match next {
            FastActorResponseV1::Decision(_) => match Self::score_session_v1(
                self.model.as_ref(),
                self.opponent_model.as_deref(),
                self.population_opponent.as_ref(),
                active.population_opponent_member,
                self.candidate_selector,
                self.search.as_mut(),
                self.max_physical_decisions,
                self.max_policy_steps,
                active.base_seed,
                &active.session,
                &mut active.schedule,
                active.candidate_seat,
                &structured_history_after.completed,
                request_received,
                ProtocolRequestKindV4::Step,
            ) {
                Ok(Some(scored)) => Some(scored),
                Ok(None) => {
                    active.session.restore_v1(&session_before);
                    active.schedule = schedule_before;
                    return response_v1(
                        Some(request_id),
                        &self.identity,
                        error_body_v1(
                            "internal_protocol_error",
                            "decision unexpectedly became terminal",
                        ),
                    );
                }
                Err(code) => {
                    active.session.restore_v1(&session_before);
                    active.schedule = schedule_before;
                    return response_v1(
                        Some(request_id),
                        &self.identity,
                        error_body_v1(code, "next decision scoring failed"),
                    );
                }
            },
            FastActorResponseV1::Terminal(_) => None,
        };
        if let Some(export) = self.teacher_export.as_mut() {
            let write_result = teacher_decision_record
                .as_ref()
                .map(|record| export.write_decision_v1(record))
                .transpose()
                .and_then(|_| match &next {
                    FastActorResponseV1::Terminal(terminal) => {
                        let record = export.terminal_record_v1(active, terminal.clone());
                        export.write_terminal_v1(&record)
                    }
                    FastActorResponseV1::Decision(_) => Ok(()),
                });
            if write_result.is_err() {
                self.export_poisoned = true;
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "teacher_export_write_failed",
                        "the accepted CP7 teacher row could not be persisted",
                    ),
                );
            }
        }
        if let Some(export) = self.outcome_export.as_mut() {
            let write_result = outcome_decision_record
                .as_ref()
                .map(|record| export.write_decision_v1(record))
                .transpose()
                .and_then(|_| match &next {
                    FastActorResponseV1::Terminal(terminal) => export
                        .terminal_record_v1(active, terminal.clone())
                        .map_err(|()| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "CP7 outcome terminal record is invalid",
                            )
                        })
                        .and_then(|record| export.write_terminal_v1(&record)),
                    FastActorResponseV1::Decision(_) => Ok(()),
                });
            if write_result.is_err() {
                self.export_poisoned = true;
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(
                        "outcome_export_write_failed",
                        "the accepted candidate outcome row could not be persisted",
                    ),
                );
            }
        }
        active.current = next_scored;
        active.structured_history = structured_history_after;
        // The episode is over: CLOSE the diagnostics episode with a
        // footer. The footer carries the publication time of the last
        // decision record, which nothing else follows to observe, so
        // without it the final search of every game has no protocol
        // verdict at all: one dropped sample per game, always the same
        // one.
        //
        // AFTER the session state is committed above, deliberately. The
        // action has already been consumed and every export row already
        // written by this point, so a failed footer must be reported
        // without also leaving the service describing a decision the
        // session has moved past.
        if matches!(next, FastActorResponseV1::Terminal(_)) {
            if let Some(search) = self.search.as_mut() {
                if let Err(code) = search.close_terminal_episode_v1(episode_id) {
                    return response_v1(
                        Some(request_id),
                        &self.identity,
                        error_body_v1(code, "model-guided search episode could not be closed"),
                    );
                }
            }
        }
        let body = match next {
            FastActorResponseV1::Decision(_) => {
                match Self::decision_body_v1(active, false, protocol) {
                    Ok(decision) => ShadowScorerResponseBodyV1::Decision {
                        decision,
                        applied_action: Some(applied),
                    },
                    Err(()) => error_body_v1("internal_protocol_error", "decision cache missing"),
                }
            }
            FastActorResponseV1::Terminal(terminal) => ShadowScorerResponseBodyV1::Terminal {
                terminal: Self::terminal_body_v1(active, terminal, false),
                applied_action: Some(applied),
            },
        };
        response_v1(Some(request_id), &self.identity, body)
    }

    /// Every response leaves through here, so this is the single place the
    /// negotiated wire version is stamped. `response_v1` keeps building the
    /// frozen V1 envelope; under V2 only these two envelope fields change.
    fn stamped_response_v1(&self, mut response: ShadowScorerResponseV1) -> ShadowScorerResponseV1 {
        response.protocol = self.protocol.protocol_string_v1();
        response.schema_version = self.protocol.schema_version_v1();
        response
    }

    /// The OUTER decision boundary, for a caller that owns no response
    /// transport of its own. `run_jsonl_v1` uses
    /// [`Self::handle_line_at_v1`] instead, because the client's wait does
    /// not end until the response has been written and flushed.
    #[cfg(test)]
    fn handle_line_v1(&mut self, line: &str) -> String {
        self.handle_line_at_v1(line, Instant::now())
    }

    /// The OPENING half of the outer request boundary.
    ///
    /// `request_received` is taken by the caller before anything else and
    /// threaded down to the diagnostics record. Everything between that
    /// instant and the record's construction is synchronous work the
    /// client is waiting on (request parsing, packet encoding,
    /// tensorization, the model forward, the policy sample, the search,
    /// the stability halves), so this is where a per-decision protocol
    /// latency has to start being counted. Only requests that actually
    /// score a decision carry it further: `score_current` answers from the
    /// cache and publishes nothing, so it has no decision to charge.
    ///
    /// The CLOSING half is [`Self::note_request_completed_v1`], which the
    /// serving loop calls once the response has been written and flushed.
    /// It cannot be called from here: the record is already published by
    /// the time this returns, and the response has not been serialized,
    /// written or flushed yet.
    fn handle_line_at_v1(&mut self, line: &str, request_received: Instant) -> String {
        if self.export_poisoned {
            let request_id = parse_strict_json_value(line)
                .ok()
                .and_then(|value| request_id_from_value_v1(&value));
            return serialize_response_v1(&self.stamped_response_v1(response_v1(
                request_id,
                &self.identity,
                error_body_v1(
                    "export_poisoned",
                    "a prior export write failed and this scorer cannot continue",
                ),
            )));
        }
        let response = if line.len() > CHECKPOINT_SHADOW_MAX_REQUEST_BYTES_V1 {
            response_v1(
                None,
                &self.identity,
                error_body_v1("request_too_large", "request exceeds the fixed byte limit"),
            )
        } else {
            let value = match parse_strict_json_value(line) {
                Ok(value) => value,
                Err(_) => {
                    let code = if serde_json::from_str::<serde::de::IgnoredAny>(line).is_ok() {
                        "malformed_request"
                    } else {
                        "malformed_json"
                    };
                    return serialize_response_v1(&self.stamped_response_v1(response_v1(
                        None,
                        &self.identity,
                        error_body_v1(code, "request line is not valid strict JSON"),
                    )));
                }
            };
            let recoverable_request_id = request_id_from_value_v1(&value);
            match serde_json::from_value::<ShadowScorerRequestV1>(value) {
                Ok(request) if valid_request_id_v1(request.request_id()) => match request {
                    ShadowScorerRequestV1::Reset {
                        request_id,
                        episode_id,
                        base_seed,
                    } => self.handle_reset_v1(request_id, episode_id, base_seed, request_received),
                    ShadowScorerRequestV1::ScoreCurrent {
                        request_id,
                        episode_id,
                        expected_step,
                    } => self.handle_score_current_v1(request_id, episode_id, expected_step),
                    ShadowScorerRequestV1::Step {
                        request_id,
                        episode_id,
                        expected_step,
                        selected_index,
                        expected_clock,
                    } => {
                        if expected_clock.is_some() && !self.protocol.carries_kernel_clock_v1() {
                            // A V1 service has never accepted this field. Keep
                            // its exact pre-V2 error surface rather than
                            // inventing a new code only V1 callers could see.
                            response_v1(
                                Some(request_id),
                                &self.identity,
                                error_body_v1(
                                    "malformed_request",
                                    "request does not match the shadow scorer schema",
                                ),
                            )
                        } else {
                            self.handle_step_v1(
                                request_id,
                                episode_id,
                                expected_step,
                                selected_index,
                                expected_clock,
                                request_received,
                            )
                        }
                    }
                },
                _ => response_v1(
                    recoverable_request_id,
                    &self.identity,
                    error_body_v1(
                        "malformed_request",
                        "request does not match the shadow scorer schema",
                    ),
                ),
            }
        };
        serialize_response_v1(&self.stamped_response_v1(response))
    }

    /// Closes the model-guided search diagnostics episode, if one is open.
    ///
    /// A no-op when the wrapper is not installed or nothing is open. The
    /// serving loop calls this on orderly exit so an episode that never
    /// reached a terminal (the driver simply stopped) still ends in a
    /// footer, which is what makes its last decision's protocol latency
    /// recoverable and marks the file as a complete episode.
    fn close_search_episode_v1(
        &mut self,
        reason: EpisodeCloseReasonV4,
    ) -> Result<(), &'static str> {
        match self.search.as_mut() {
            Some(search) => search.close_episode_v1(reason),
            None => Ok(()),
        }
    }

    /// The CLOSING half of the outer request boundary: the response line
    /// has been written and flushed, so the client's wait for this request
    /// is over and the tail after the diagnostics publish is now known.
    ///
    /// A no-op when the wrapper is not installed. Called by the serving
    /// loop for every request, including ones that published nothing; the
    /// writer ignores those rather than charging an unrelated interval to
    /// an already measured record.
    fn note_request_completed_v1(&mut self) {
        if let Some(search) = self.search.as_mut() {
            search.note_request_completed_v1();
        }
    }
}

/// Scorer process-startup MXCSR normalization (test-time-search sketch V2
/// Section 5, S0: "MXCSR normalization at scorer process startup and on
/// every worker thread that runs a search, plus fail-closed verification").
///
/// This runs before the checkpoint authority is loaded and before any
/// forward pass, on the process's main thread, and is a HARD startup error
/// if the register cannot be brought to the pinned state. It is not a
/// substitute for the per-thread normalization a search performs on its
/// own thread (MXCSR is per-thread), and it does not replace
/// `forward_search_deterministic_v1`'s own entry assert; it is the first
/// of the three layers, the one that makes the ordinary single-threaded
/// stdio scorer pinned from its first line of work.
fn normalize_scorer_process_mxcsr_v1() -> Result<(), Box<dyn Error>> {
    crate::deterministic_math_v1::normalize_pinned_mxcsr_state_v1().map_err(|error| {
        io::Error::other(format!(
            "scorer startup could not pin the MXCSR floating-point control state: {} (observed 0x{:08x})",
            error.code(),
            error.observed_v1(),
        ))
    })?;
    Ok(())
}

fn serialize_response_v1(response: &ShadowScorerResponseV1) -> String {
    serde_json::to_string(response).unwrap_or_else(|_| {
        format!(
            "{{\"protocol\":\"{CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1}\",\"schema_version\":1,\"request_id\":null,\"response_type\":\"error\",\"error_code\":\"response_serialization_failed\",\"message\":\"response serialization failed\"}}"
        )
    })
}

/// Loads one explicit authority, then serves exactly one strict JSON response
/// per input line. Diagnostics and startup errors remain outside stdout.
pub fn run_checkpoint_shadow_stdio_v1(
    authority: ShadowCheckpointAuthorityV1,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(
        authority,
        ShadowCandidateSelectorV1::PolicySample,
        None,
        None,
    )
}

/// Experimental live selector for the parity-checked complete-history model.
/// It is intentionally unavailable with trajectory exports because those
/// schemas bind selection to direct checkpoint-policy sampling.
pub fn run_checkpoint_shadow_stdio_with_selector_v1(
    authority: ShadowCheckpointAuthorityV1,
    selector: ShadowCandidateSelectorV1,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(authority, selector, None, None)
}

/// Shadow-only depth-8 teacher collection. Candidate actions remain direct
/// policy samples, while depth-8 diagnostics provide supervised targets for
/// an offline learner. Opponent and candidate trajectory exports retain their
/// existing schemas because search never controls the live session.
pub fn run_checkpoint_shadow_stdio_with_depth8_teacher_exports_jsonl_v1(
    authority: ShadowCheckpointAuthorityV1,
    opponent_teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(
        authority,
        ShadowCandidateSelectorV1::Depth8BoundedValueTeacherShadow,
        Some(opponent_teacher_jsonl),
        Some(outcome_jsonl),
    )
}

/// The test-time-search wrapper as a first-class scorer decision mode
/// (`LEAD_RULING_TEST_TIME_SEARCH_V1.md` consequence 1: "The CP7 scorer
/// gains a search-wrapped decision mode as a first-class,
/// contract-validated path"). Every parameter is supplied by an explicit
/// CLI flag on the scorer binary; nothing here reads an environment
/// variable, and there is no default that could turn the wrapper on by
/// accident.
///
/// `seed_block_id` indexes
/// `MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`, so an unregistered
/// seed cannot reach an authority record through a command line at all.
/// `diagnostics_directory` must already exist.
///
/// Deliberately incompatible with the two trajectory exports, for the same
/// reason `run_checkpoint_shadow_stdio_with_selector_v1` documents for the
/// value-search selectors: those export schemas record
/// `selected_action_index` as a direct checkpoint-policy sample, and a
/// wrapped decision is not one. Emitting search-chosen actions into a
/// schema that claims they were policy samples would silently corrupt
/// every downstream consumer of those files, so it fails closed at
/// startup.
pub fn run_checkpoint_shadow_stdio_with_model_guided_search_v1(
    authority: ShadowCheckpointAuthorityV1,
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    diagnostics_directory: PathBuf,
    stability_halves_enabled: bool,
) -> Result<(), Box<dyn Error>> {
    normalize_scorer_process_mxcsr_v1()?;
    let search = ModelGuidedSearchRuntimeV1::new_v1(
        tier,
        seed_block_id,
        diagnostics_directory,
        stability_halves_enabled,
    )?;
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
    if service.model.search_capable_v1().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "model-guided search requires a checkpoint whose model exposes the typed native net",
        )
        .into());
    }
    eprintln!(
        "MODEL_GUIDED_SEARCH tier={} transition_budget={} seed_block_id={} action_seed={} stability_halves={} diagnostics_dir={}",
        search_tier_tag_v1(search.tier),
        search.tier.transition_budget(),
        search.seed_block_id,
        search.action_seed,
        if search.stability_halves_enabled { "on" } else { "off" },
        search.diagnostics.directory_v4().display(),
    );
    service.candidate_selector = ShadowCandidateSelectorV1::ModelGuidedSearch;
    service.search = Some(search);
    run_jsonl_v1(&mut service, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}

/// Opt-in XMage CP7 teacher export. The destination is created exclusively;
/// callers must promote only the file from a fully successful anchor run.
pub fn run_checkpoint_shadow_stdio_with_xmage_cp7_teacher_jsonl_v1(
    authority: ShadowCheckpointAuthorityV1,
    teacher_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(
        authority,
        ShadowCandidateSelectorV1::PolicySample,
        Some(teacher_jsonl),
        None,
    )
}

/// Opt-in candidate trajectory export for offline terminal-return updates.
/// The destination is created exclusively and records only decisions owned by
/// the candidate seat plus the natural terminal that supplies their return.
pub fn run_checkpoint_shadow_stdio_with_xmage_cp7_outcome_jsonl_v1(
    authority: ShadowCheckpointAuthorityV1,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(
        authority,
        ShadowCandidateSelectorV1::PolicySample,
        None,
        Some(outcome_jsonl),
    )
}

/// Opt-in matched CP7 teacher and candidate outcome exports from one trajectory.
/// Both destinations are created exclusively before the first reset.
pub fn run_checkpoint_shadow_stdio_with_xmage_cp7_exports_jsonl_v1(
    authority: ShadowCheckpointAuthorityV1,
    teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(
        authority,
        ShadowCandidateSelectorV1::PolicySample,
        Some(teacher_jsonl),
        Some(outcome_jsonl),
    )
}

/// Opt-in native on-policy corpus generation against the frozen Pool3 ladder.
/// Candidate and opponent decisions are exported separately so the existing
/// complete-public-history cache builder can reconstruct every physical
/// decision. Only the candidate stream receives the natural terminal return.
pub fn run_checkpoint_shadow_stdio_with_native_population_exports_jsonl_v1(
    authority: ShadowCheckpointAuthorityV1,
    pool_root: PathBuf,
    teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    normalize_scorer_process_mxcsr_v1()?;
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
    install_native_population_exports_v1(&mut service, pool_root, teacher_jsonl, outcome_jsonl)?;
    run_jsonl_v1(&mut service, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}

fn install_native_population_exports_v1(
    service: &mut ShadowScorerServiceV1,
    pool_root: PathBuf,
    teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let pool_bytes = fs::read(pool_root.join("pool.json"))?;
    let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)?;
    let (primary, predecessor_a, predecessor_b) = resolve_ladder_pool_v1(
        &pool,
        &pool_root.join("primary"),
        &pool_root.join("pred-a"),
        &pool_root.join("pred-b"),
    )?;
    let engine = LadderOpponentEngineV1::new_v1(pool, primary, predecessor_a, predecessor_b)?;
    let teacher = XmageCp7TeacherJsonlWriterV1::create_native_population_v1(
        &teacher_jsonl,
        &service.identity,
    )?;
    let outcome = XmageCp7OutcomeJsonlWriterV1::create_native_population_v1(
        &outcome_jsonl,
        &service.identity,
    )?;
    service.install_teacher_export_v1(teacher).map_err(|()| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "native population opponent export must be installed before reset",
        )
    })?;
    service.install_outcome_export_v1(outcome).map_err(|()| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "native population outcome export must be installed before reset",
        )
    })?;
    service.population_opponent = Some(engine);
    eprintln!(
        "NATIVE_POPULATION_CORPUS pool_root={} weights=40,20,20,20",
        pool_root.display()
    );
    Ok(())
}

/// The one entry point that can select the wire version. Every other public
/// runner delegates here with [`ShadowStdioProtocolV1::V1`], so a caller that
/// does not opt in keeps the frozen protocol byte for byte.
pub fn run_checkpoint_shadow_stdio_with_protocol_and_exports_v1(
    authority: ShadowCheckpointAuthorityV1,
    protocol: ShadowStdioProtocolV1,
    teacher_jsonl: Option<PathBuf>,
    outcome_jsonl: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_fully_configured_v1(
        authority,
        ShadowCandidateSelectorV1::PolicySample,
        protocol,
        teacher_jsonl,
        outcome_jsonl,
    )
}

fn run_checkpoint_shadow_stdio_configured_v1(
    authority: ShadowCheckpointAuthorityV1,
    selector: ShadowCandidateSelectorV1,
    teacher_jsonl: Option<PathBuf>,
    outcome_jsonl: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_fully_configured_v1(
        authority,
        selector,
        ShadowStdioProtocolV1::V1,
        teacher_jsonl,
        outcome_jsonl,
    )
}

fn run_checkpoint_shadow_stdio_fully_configured_v1(
    authority: ShadowCheckpointAuthorityV1,
    selector: ShadowCandidateSelectorV1,
    protocol: ShadowStdioProtocolV1,
    teacher_jsonl: Option<PathBuf>,
    outcome_jsonl: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    normalize_scorer_process_mxcsr_v1()?;
    if selector == ShadowCandidateSelectorV1::ModelGuidedSearch {
        // The wrapper needs a tier, a seed block, and a diagnostics
        // directory that this entry point has no way to supply, and it is
        // incompatible with both trajectory exports; see
        // `run_checkpoint_shadow_stdio_with_model_guided_search_v1`.
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the model-guided search selector has its own entry point and cannot be configured here",
        )
        .into());
    }
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
    service.protocol = protocol;
    if selector == ShadowCandidateSelectorV1::Depth8Cp7OpponentHistoryValueBootstrap {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "CP7-opponent selector requires an explicit opponent authority",
        )
        .into());
    }
    if matches!(
        selector,
        ShadowCandidateSelectorV1::OneStepHistoryValueBootstrap
            | ShadowCandidateSelectorV1::CandidateTurnOnlyOneStepBoundedValueBootstrap
            | ShadowCandidateSelectorV1::Depth8HistoryValueBootstrap
            | ShadowCandidateSelectorV1::Depth8BoundedValueTeacherShadow
    ) && !service.model.uses_structured_history_v1()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "history-value selection requires a structured-history candidate",
        )
        .into());
    }
    service.candidate_selector = selector;
    if let Some(path) = teacher_jsonl {
        let export = XmageCp7TeacherJsonlWriterV1::create_v1(&path, &service.identity)?;
        service.install_teacher_export_v1(export).map_err(|()| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 teacher export must be installed before reset",
            )
        })?;
    }
    if let Some(path) = outcome_jsonl {
        let export = XmageCp7OutcomeJsonlWriterV1::create_v1(&path, &service.identity)?;
        service.install_outcome_export_v1(export).map_err(|()| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 outcome export must be installed before reset",
            )
        })?;
    }
    run_jsonl_v1(&mut service, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}

fn run_jsonl_v1(
    service: &mut ShadowScorerServiceV1,
    mut reader: impl BufRead,
    mut writer: impl Write,
) -> io::Result<()> {
    // Layer two of the S0 MXCSR requirement: the thread that will actually
    // serve decisions (and therefore run any search) normalizes its own
    // register before the first request, independently of which thread ran
    // `normalize_scorer_process_mxcsr_v1`. MXCSR is per-thread, so process
    // startup alone does not cover a serving loop that was moved onto a
    // different thread. Layer three is the per-decision fail-closed verify
    // the search selector performs before its first forward.
    crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1().map_err(|error| {
        io::Error::other(format!(
            "scorer serving thread could not pin the MXCSR floating-point control state: {} (observed 0x{:08x})",
            error.code(),
            error.observed_v1(),
        ))
    })?;
    let mut line = String::new();
    loop {
        line.clear();
        let count = reader.read_line(&mut line)?;
        if count == 0 {
            break;
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        // The OUTER request boundary, both halves. The client's wait
        // starts when the request line has been read and does not end
        // until the response line is out and flushed, so the diagnostics
        // record's protocol accounting is opened and closed here rather
        // than anywhere inside the handler: exports, response
        // serialization, the write and the flush are all synchronous cost
        // the panel host pays for that decision, and all of them happen
        // after the record is already on disk.
        let request_received = Instant::now();
        let response = service.handle_line_at_v1(&line, request_received);
        writeln!(writer, "{response}")?;
        writer.flush()?;
        service.note_request_completed_v1();
        if service.export_poisoned {
            return Err(io::Error::other(
                "checkpoint shadow export is poisoned after a write failure",
            ));
        }
    }
    // ORDERLY EXIT. End of input, so no further decision can be searched
    // in the open episode: close it with a footer. Without this, a run
    // that a driver simply stops (rather than playing to a terminal)
    // leaves its last decision with no successor record and therefore no
    // protocol-latency verdict at all.
    service
        .close_search_episode_v1(EpisodeCloseReasonV4::ProcessExit)
        .map_err(|code| {
            io::Error::other(format!(
                "the model-guided search diagnostics episode could not be closed: {code}"
            ))
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_guided_search_outcome_v4::verify_episode_chain_v4;
    use std::sync::{Arc, Mutex};

    #[test]
    fn population_store_authority_selects_only_environment_randomization_v2_v1() {
        let population = ShadowCheckpointAuthorityV1::PopulationStoreGeneration {
            root: PathBuf::from("population-store"),
            generation: 1024,
        };
        let original = ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration {
            root: PathBuf::from("original-store"),
            generation: 1024,
        };
        assert_eq!(
            expected_environment_contract_v1(&population),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        assert_eq!(
            expected_environment_contract_v1(&original),
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1
        );
    }

    #[derive(Clone, Default)]
    struct SharedBytesV1(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBytesV1 {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| io::Error::other("shared writer poisoned"))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FlushFailWriterV1 {
        bytes: SharedBytesV1,
        fail_flush: Arc<Mutex<bool>>,
    }

    impl Write for FlushFailWriterV1 {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            if *self
                .fail_flush
                .lock()
                .map_err(|_| io::Error::other("flush control poisoned"))?
            {
                Err(io::Error::other("injected export flush failure"))
            } else {
                Ok(())
            }
        }
    }

    struct DeterministicTestModelV1;

    impl ShadowModelScorerV1 for DeterministicTestModelV1 {
        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
            _history: &[NativeStructuredHistoryEntryV1],
            _acting_player: u8,
            _substep_count: u32,
        ) -> Result<ShadowModelOutputV1, ()> {
            Ok(ShadowModelOutputV1 {
                logits: (0..decision.actions().len())
                    .map(|index| index as f32 * 0.125 - 0.5)
                    .collect(),
                value: 0.25,
                structured_parent_logits: None,
                structured_parent_value: None,
            })
        }
    }

    struct FirstActionTestModelV1;

    impl ShadowModelScorerV1 for FirstActionTestModelV1 {
        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
            _history: &[NativeStructuredHistoryEntryV1],
            _acting_player: u8,
            _substep_count: u32,
        ) -> Result<ShadowModelOutputV1, ()> {
            Ok(ShadowModelOutputV1 {
                logits: (0..decision.actions().len())
                    .map(|index| if index == 0 { 0.0 } else { -1_000.0 })
                    .collect(),
                value: 0.0,
                structured_parent_logits: None,
                structured_parent_value: None,
            })
        }
    }

    struct StructuredParentExportTestModelV1;

    impl ShadowModelScorerV1 for StructuredParentExportTestModelV1 {
        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
            _history: &[NativeStructuredHistoryEntryV1],
            _acting_player: u8,
            _substep_count: u32,
        ) -> Result<ShadowModelOutputV1, ()> {
            let structured_parent_logits = (0..decision.actions().len())
                .map(|index| index as f32 * 0.25 - 0.75)
                .collect::<Vec<_>>();
            Ok(ShadowModelOutputV1 {
                logits: structured_parent_logits
                    .iter()
                    .map(|value| value + 0.125)
                    .collect(),
                value: 0.25,
                structured_parent_logits: Some(structured_parent_logits),
                structured_parent_value: Some(-0.5),
            })
        }
    }

    struct CountingStructuredFirstActionTestModelV1 {
        calls: Arc<Mutex<usize>>,
    }

    impl ShadowModelScorerV1 for CountingStructuredFirstActionTestModelV1 {
        fn uses_structured_history_v1(&self) -> bool {
            true
        }

        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
            _history: &[NativeStructuredHistoryEntryV1],
            _acting_player: u8,
            _substep_count: u32,
        ) -> Result<ShadowModelOutputV1, ()> {
            *self.calls.lock().map_err(|_| ())? += 1;
            Ok(ShadowModelOutputV1 {
                logits: (0..decision.actions().len())
                    .map(|index| if index == 0 { 0.0 } else { -1_000.0 })
                    .collect(),
                value: 0.125,
                structured_parent_logits: None,
                structured_parent_value: None,
            })
        }
    }

    struct CountingStructuredDeterministicTestModelV1 {
        calls: Arc<Mutex<usize>>,
    }

    impl ShadowModelScorerV1 for CountingStructuredDeterministicTestModelV1 {
        fn uses_structured_history_v1(&self) -> bool {
            true
        }

        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
            _history: &[NativeStructuredHistoryEntryV1],
            _acting_player: u8,
            _substep_count: u32,
        ) -> Result<ShadowModelOutputV1, ()> {
            *self.calls.lock().map_err(|_| ())? += 1;
            Ok(ShadowModelOutputV1 {
                logits: (0..decision.actions().len())
                    .map(|index| index as f32 * 0.125 - 0.5)
                    .collect(),
                value: 0.25,
                structured_parent_logits: None,
                structured_parent_value: None,
            })
        }
    }

    fn service_v1() -> ShadowScorerServiceV1 {
        ShadowScorerServiceV1::with_test_model_v1(Box::new(DeterministicTestModelV1))
    }

    /// Pinned `PolicySample` outputs for episode 2 / base seed 71,501 with
    /// `DeterministicTestModelV1`, recorded before the search-capable seam
    /// was added. See
    /// `policy_sample_choice_is_unchanged_for_a_fixed_seed_v1`.
    const POLICY_SAMPLE_REGRESSION_ACTION_SEED_HEX_V1: &str = "3b80443fc0e5af4d";
    const POLICY_SAMPLE_REGRESSION_SELECTED_INDEX_V1: u64 = 0;
    const POLICY_SAMPLE_REGRESSION_MODEL_INPUT_SHA256_V1: &str =
        "fd13e231da01867dfa6ea0897d38baa05ba81bb761c7856374707a58160dddc6";
    const POLICY_SAMPLE_REGRESSION_LEGAL_ACTION_COUNT_V1: u64 = 2;

    // ------------------------------------------------------------------
    // Test-time-search wrapper (LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md
    // Section 5, S0)
    // ------------------------------------------------------------------

    /// A search-capable model over the runner-fixed native net: the same
    /// in-memory net `model_guided_search_core_v1`'s own real-forward
    /// tests use, with no checkpoint-manifest or Store dependency. Its
    /// flat `score_v1` mirrors `FixedNativeStateShadowModelScorerV1`'s
    /// exactly, so the policy-sample path this test model drives is the
    /// production one.
    struct SearchCapableTestModelV1 {
        net: NativePolicyValueNetV1,
    }

    impl SearchCapableTestModelV1 {
        fn new_v1() -> Self {
            Self {
                net: NativePolicyValueNetV1::runner_fixed_v1(
                    NativePolicyValueModelConfigV1::contract_v1(),
                )
                .expect("runner-fixed model builds"),
            }
        }
    }

    impl ShadowSearchCapableModelV1 for SearchCapableTestModelV1 {
        fn search_native_net_v1(&self) -> &NativePolicyValueNetV1 {
            &self.net
        }
    }

    impl ShadowModelScorerV1 for SearchCapableTestModelV1 {
        fn search_capable_v1(&self) -> Option<&dyn ShadowSearchCapableModelV1> {
            Some(self)
        }

        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
            _history: &[NativeStructuredHistoryEntryV1],
            _acting_player: u8,
            _substep_count: u32,
        ) -> Result<ShadowModelOutputV1, ()> {
            let mut tensorizer = NativeFlatTensorizerV2::new();
            let mut tensor = NativeFlatDecisionTensorV2::default();
            tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
            let encoded = crate::native_checkpoint_inference_v1::encoded_decision_view_v1(&tensor);
            let output = self.net.forward_v1(encoded).map_err(|_| ())?;
            if output.logits.len() != decision.actions().len() {
                return Err(());
            }
            Ok(ShadowModelOutputV1 {
                logits: output.logits,
                value: output.value,
                structured_parent_logits: None,
                structured_parent_value: None,
            })
        }
    }

    fn search_scratch_directory_v1(tag: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "mtg-kernel-scorer-search-{}-{tag}",
            std::process::id()
        ));
        fs::remove_dir_all(&directory).ok();
        fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    fn search_runtime_v1(directory: PathBuf) -> ModelGuidedSearchRuntimeV1 {
        search_runtime_with_halves_v1(directory, true)
    }

    fn search_runtime_with_halves_v1(
        directory: PathBuf,
        stability_halves_enabled: bool,
    ) -> ModelGuidedSearchRuntimeV1 {
        ModelGuidedSearchRuntimeV1::new_v1(
            KernelNativeSearchTierV1::T512,
            0,
            directory,
            stability_halves_enabled,
        )
        .expect("the T512 tier and seed block zero are registered")
    }

    fn search_service_v1(directory: PathBuf) -> ShadowScorerServiceV1 {
        search_service_with_halves_v1(directory, true)
    }

    fn search_service_with_halves_v1(
        directory: PathBuf,
        stability_halves_enabled: bool,
    ) -> ShadowScorerServiceV1 {
        let mut service =
            ShadowScorerServiceV1::with_test_model_v1(Box::new(SearchCapableTestModelV1::new_v1()));
        service.candidate_selector = ShadowCandidateSelectorV1::ModelGuidedSearch;
        service.search = Some(search_runtime_with_halves_v1(
            directory,
            stability_halves_enabled,
        ));
        service
    }

    /// The architecture identity of the runner-fixed net the scorer tests
    /// search with, read off the net rather than pinned by hand.
    fn search_test_net_architecture_v1() -> &'static str {
        SearchCapableTestModelV1::new_v1()
            .search_native_net_v1()
            .architecture_identity_v1()
    }

    /// Neutralizes wall time in a published episode file so two runs can
    /// be compared for the bit identity the sketch requires "apart from
    /// wall-time fields". EVERY record kind's timing surface lives in one
    /// member literally named `wall_time` precisely so this is one
    /// substitution and not a field-by-field allowlist that could silently
    /// miss a new field or a new record kind. The replacement value is
    /// null rather than a typed default so this rule does not have to know
    /// which shape of `wall_time` a given record kind carries.
    ///
    /// The chain link is RE-DERIVED over the neutralized lines rather than
    /// carried through. `previous_record_sha256` covers the previous
    /// record's published bytes, wall time included, so two runs of the
    /// same decision legitimately publish different links; comparing them
    /// would be asserting that timing is reproducible, which it is not and
    /// must not need to be. Re-deriving keeps the comparison sensitive to
    /// record ORDER and content (a reordering still breaks it) while
    /// dropping only the timing dependence. The published chain is
    /// separately verified as-is by `verify_episode_chain_v4`.
    /// The footer's `episode_content_sha256` is re-derived for the same
    /// reason and in the same way: it commits to the published bytes,
    /// which legitimately differ in their timing fields, so it is
    /// recomputed over the NEUTRALIZED bytes instead of blanked. A
    /// reordered or edited episode still changes it.
    fn strip_wall_time_v1(bytes: &[u8]) -> Vec<String> {
        let text = String::from_utf8(bytes.to_vec()).expect("diagnostics are UTF-8");
        let mut previous =
            crate::model_guided_search_outcome_v4::MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4
                .to_owned();
        let mut normalized_lines: Vec<String> = Vec::new();
        let mut normalized_content: Vec<u8> = Vec::new();
        for line in text.lines() {
            let mut value: serde_json::Value = serde_json::from_str(line).expect("record is JSON");
            let object = value.as_object_mut().expect("record is an object");
            if object.contains_key("wall_time") {
                object.insert("wall_time".to_owned(), serde_json::Value::Null);
            }
            object.insert(
                "previous_record_sha256".to_owned(),
                serde_json::Value::String(previous.clone()),
            );
            if object.contains_key("episode_content_sha256") {
                object.insert(
                    "episode_content_sha256".to_owned(),
                    serde_json::Value::String(lower_hex_sha256_v4(
                        crate::model_guided_search_outcome_v4::episode_content_digest_v4(
                            &normalized_content,
                        ),
                    )),
                );
            }
            let normalized = serde_json::to_string(&value).unwrap();
            previous = lower_hex_sha256_v4(
                crate::model_guided_search_outcome_v4::record_chain_link_v4(&normalized),
            );
            normalized_content.extend_from_slice(normalized.as_bytes());
            normalized_content.push(b'\n');
            normalized_lines.push(normalized);
        }
        normalized_lines
    }

    /// Drives one short wrapped episode through the production protocol
    /// (reset, then `steps` accepted steps on the scorer's own chosen
    /// index) and returns the published diagnostics bytes.
    fn run_wrapped_episode_v1(directory: &Path, steps: usize) -> (Vec<u32>, Vec<u8>) {
        run_wrapped_episode_with_halves_v1(directory, steps, true)
    }

    fn run_wrapped_episode_with_halves_v1(
        directory: &Path,
        steps: usize,
        stability_halves_enabled: bool,
    ) -> (Vec<u32>, Vec<u8>) {
        run_wrapped_episode_configured_v1(directory, steps, stability_halves_enabled, None, false)
    }

    /// A response transport that is slow to FLUSH.
    ///
    /// The delay sits entirely after the diagnostics record is on disk and
    /// entirely outside `handle_line_at_v1`, which is where a slow export
    /// or a slow stdout sits on a loaded panel host, and which is exactly
    /// the interval a record cannot observe about itself.
    struct SlowFlushWriterV1 {
        sink: Vec<u8>,
        delay: Option<std::time::Duration>,
    }

    impl Write for SlowFlushWriterV1 {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.sink.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            if let Some(delay) = self.delay {
                std::thread::sleep(delay);
            }
            self.sink.flush()
        }
    }

    /// Records the exact request lines one wrapped episode produces, so
    /// the same episode can be replayed through the real serving loop
    /// (which reads a script and cannot ask the scorer what to send next).
    /// The wrapper is deterministic, so a recorded script replays exactly.
    fn wrapped_episode_script_v1(steps: usize) -> Vec<String> {
        let directory = search_scratch_directory_v1("script");
        let mut service = search_service_with_halves_v1(directory.clone(), false);
        let reset = reset_line_v1("loop-reset");
        let mut script = vec![reset.clone()];
        let mut response = value_v1(&service.handle_line_v1(&reset));
        assert_eq!(response["response_type"], "decision", "{response}");
        for index in 0..steps {
            let decision = &response["decision"];
            let selected = decision["selected_action_index"].as_u64().unwrap();
            let episode_id = decision["episode_id"].as_u64().unwrap();
            let step = decision["step"].as_u64().unwrap();
            let line = format!(
                "{{\"request_type\":\"step\",\"request_id\":\"loop-step-{index}\",\"episode_id\":{episode_id},\"expected_step\":{step},\"selected_index\":{selected}}}"
            );
            script.push(line.clone());
            response = value_v1(&service.handle_line_v1(&line));
            if response["response_type"] != "decision" {
                break;
            }
        }
        fs::remove_dir_all(&directory).ok();
        script
    }

    /// Replays a recorded script through the PRODUCTION serving loop, with
    /// an optionally slow response transport, and returns the published
    /// diagnostics. The loop closes the episode with a footer at end of
    /// input, so every decision in the result is classifiable.
    fn run_script_through_loop_v1(
        directory: &Path,
        script: &[String],
        flush_delay: Option<std::time::Duration>,
    ) -> Vec<u8> {
        let mut service = search_service_with_halves_v1(directory.to_path_buf(), false);
        let input: String = script.iter().map(|line| format!("{line}\n")).collect();
        let writer = SlowFlushWriterV1 {
            sink: Vec::new(),
            delay: flush_delay,
        };
        run_jsonl_v1(&mut service, input.as_bytes(), writer).expect("the serving loop runs");
        let path = service
            .search
            .as_ref()
            .expect("search runtime installed")
            .diagnostics
            .episode_path_v4(2, 71_501);
        fs::read(path).expect("episode diagnostics published")
    }

    /// Drives one wrapped episode and, optionally, injects an artificial
    /// delay into every diagnostics publication and closes the episode
    /// with a footer at the end.
    ///
    /// The delay goes INSIDE the measured publication window, exactly
    /// where a slow disk would sit, so a test can make one run genuinely
    /// slower than another without touching anything the search reads.
    fn run_wrapped_episode_configured_v1(
        directory: &Path,
        steps: usize,
        stability_halves_enabled: bool,
        publish_delay: Option<std::time::Duration>,
        close_episode: bool,
    ) -> (Vec<u32>, Vec<u8>) {
        let mut service =
            search_service_with_halves_v1(directory.to_path_buf(), stability_halves_enabled);
        if let Some(delay) = publish_delay {
            service
                .search
                .as_mut()
                .expect("search runtime installed")
                .diagnostics
                .set_publish_delay_for_test_v4(delay);
        }
        let mut chosen = Vec::new();
        let mut response = value_v1(&service.handle_line_v1(&reset_line_v1("mgs-reset")));
        assert_eq!(
            response["response_type"], "decision",
            "wrapped reset must produce a decision: {response}"
        );
        for index in 0..steps {
            let decision = &response["decision"];
            let selected = decision["selected_action_index"]
                .as_u64()
                .expect("the wrapper always fixes a selected index")
                as u32;
            chosen.push(selected);
            let episode_id = decision["episode_id"].as_u64().expect("episode id");
            let step = decision["step"].as_u64().expect("step");
            response = value_v1(&service.handle_line_v1(&format!(
                "{{\"request_type\":\"step\",\"request_id\":\"mgs-step-{index}\",\"episode_id\":{episode_id},\"expected_step\":{step},\"selected_index\":{selected}}}"
            )));
            if response["response_type"] != "decision" {
                break;
            }
        }
        if close_episode {
            // `handle_line_v1` has no response transport, so close the
            // outer boundary the way the serving loop does before the
            // footer picks the tail up.
            service.note_request_completed_v1();
            service
                .close_search_episode_v1(EpisodeCloseReasonV4::ProcessExit)
                .expect("the episode closes with a footer");
        }
        let path = service
            .search
            .as_ref()
            .expect("search runtime installed")
            .diagnostics
            .episode_path_v4(2, 71_501);
        let bytes = fs::read(path).expect("episode diagnostics published");
        (chosen, bytes)
    }

    /// DELIVERABLE 2's regression guard. The `PolicySample` path must be
    /// byte-identical to what it was before the search-capable seam
    /// existed: the seam is a defaulted trait method that `PolicySample`
    /// never calls, so the sampled index, its seed, the model input
    /// digest, and the logits for a fixed seed must all be exactly what
    /// they were. Pinned by literal, not merely by self-consistency, so
    /// this catches a change the two-runs-agree tests never would.
    #[test]
    fn policy_sample_choice_is_unchanged_for_a_fixed_seed_v1() {
        let mut service = service_v1();
        assert_eq!(
            service.candidate_selector,
            ShadowCandidateSelectorV1::PolicySample
        );
        assert!(service.search.is_none());
        let reset = value_v1(&service.handle_line_v1(&reset_line_v1("policy-sample-regression")));
        assert_eq!(reset["response_type"], "decision");
        assert_eq!(reset["decision"]["candidate_controls_current_actor"], true);
        assert_eq!(reset["decision"]["episode_id"], 2);
        assert_eq!(reset["decision"]["step"], 0);
        assert_eq!(
            reset["decision"]["candidate_action_seed_u64_hex"],
            POLICY_SAMPLE_REGRESSION_ACTION_SEED_HEX_V1
        );
        assert_eq!(
            reset["decision"]["selected_action_index"],
            POLICY_SAMPLE_REGRESSION_SELECTED_INDEX_V1
        );
        assert_eq!(
            reset["decision"]["model_input_sha256"],
            POLICY_SAMPLE_REGRESSION_MODEL_INPUT_SHA256_V1
        );
        assert_eq!(
            reset["decision"]["legal_action_count"],
            POLICY_SAMPLE_REGRESSION_LEGAL_ACTION_COUNT_V1
        );
    }

    /// DELIVERABLE 6, first half: one end-to-end bit-identical replay
    /// through the PRODUCTION selector. Same checkpoint fixture, same
    /// seed, two runs; identical chosen actions and identical diagnostics
    /// bytes apart from the wall-time fields.
    #[test]
    fn model_guided_search_replay_is_bit_identical_apart_from_wall_time_v1() {
        let first_directory = search_scratch_directory_v1("replay-a");
        let second_directory = search_scratch_directory_v1("replay-b");
        let (first_chosen, first_bytes) = run_wrapped_episode_v1(&first_directory, 2);
        let (second_chosen, second_bytes) = run_wrapped_episode_v1(&second_directory, 2);

        assert!(
            !first_chosen.is_empty(),
            "the replay must exercise at least one wrapped decision"
        );
        assert_eq!(first_chosen, second_chosen, "chosen actions must replay");
        assert_eq!(
            verify_episode_chain_v4(&first_bytes),
            verify_episode_chain_v4(&second_bytes)
        );
        assert!(verify_episode_chain_v4(&first_bytes).is_ok());
        assert_eq!(
            strip_wall_time_v1(&first_bytes),
            strip_wall_time_v1(&second_bytes),
            "diagnostics must be bit-identical apart from wall time"
        );
        // The diagnostics are NOT trivially equal: the wall-time fields
        // really are present and really do carry a measurement, so the
        // comparison above is a stripping, not a no-op on empty data.
        let text = String::from_utf8(first_bytes).unwrap();
        let decision: serde_json::Value =
            serde_json::from_str(text.lines().nth(1).expect("a decision record")).unwrap();
        assert_eq!(decision["record_kind"], "search_decision");
        assert!(decision["wall_time"]["search_micros"].as_u64().is_some());
        assert!(decision["wall_time"]["decision_micros"].as_u64().is_some());
        assert_eq!(decision["protocol_request_kind"], "reset");
        assert_eq!(decision["requested_transitions"], 512);
        assert!(decision["actual_transitions"].as_u64().unwrap() >= 1);
        assert!(decision["root_statistics_digest_sha256"]
            .as_str()
            .is_some_and(|value| value.len() == 64));
        assert!(decision["stability"]["halves_agree"].is_boolean());
        assert!(decision["search_ceiling_status"].as_str().is_some());
        // Every simulation ends at exactly one leaf class, so the census
        // partitions the simulation count exactly.
        let census = &decision["leaf_census"];
        let total = census["natural_terminal_leaves"].as_u64().unwrap()
            + census["truncated_terminal_leaves"].as_u64().unwrap()
            + census["newly_expanded_leaves"].as_u64().unwrap()
            + census["depth_cap_leaves"].as_u64().unwrap();
        assert_eq!(total, decision["simulations"].as_u64().unwrap());
        fs::remove_dir_all(&first_directory).ok();
        fs::remove_dir_all(&second_directory).ok();
    }

    /// DELIVERABLE 6, second half: the metamorphic NON-LEAKAGE test.
    /// Changing the authoritative hidden cards (the opponent's hand and
    /// library arrangement) while holding public information and the
    /// simulation seed fixed must leave the search result bit-identical.
    ///
    /// If the searcher ever read the authoritative hidden state instead of
    /// its own per-simulation redeterminization, this is the test that
    /// fails: the two sessions below are indistinguishable to every public
    /// observer and differ only in which specific card sits in an unknown
    /// opponent hand slot versus an unknown library slot.
    ///
    /// Run through the production selector
    /// (`ShadowScorerServiceV1::model_guided_search_selection_v1`), not
    /// through `ModelGuidedSearchCoreV1` directly, so the encode bridge,
    /// the real forward, the MXCSR gate, and the diagnostics record are
    /// all inside the invariance claim.
    #[test]
    fn model_guided_search_is_invariant_to_authoritative_hidden_cards_v1() {
        let a = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            50_301,
            81_101,
            256,
            32_768,
            ["Rally".to_owned(), "Burn".to_owned()],
        )
        .expect("session resets");
        let mut b = a.clone();
        let FastActorResponseV1::Decision(decision_a) = a.current_response() else {
            panic!("reset terminated")
        };
        let actor = crate::kernel_native_search_opponent_v1::player_id_v1(decision_a.acting_player);
        let opponent = actor.opponent();
        let unknown_hand = a.kernel_search_state_v1().players[opponent.index()].hand[0];
        let unknown_library = a.kernel_search_state_v1().players[opponent.index()].library[0];
        let hand_def = a
            .kernel_search_state_v1()
            .objects
            .get(unknown_hand)
            .card_def;
        let library_def = a
            .kernel_search_state_v1()
            .objects
            .get(unknown_library)
            .card_def;
        assert_ne!(
            hand_def, library_def,
            "the non-leakage witness must swap two DISTINCT hidden definitions"
        );
        {
            let state = b.kernel_search_state_mut_for_test_v1();
            for (object, definition) in [(unknown_hand, library_def), (unknown_library, hand_def)] {
                state.objects.get_mut(object).card_def = definition;
                state.objects.get_mut(object).name = crate::card_def::CARD_DEFS
                    [definition as usize]
                    .name
                    .to_string();
                state.objects.get_mut(object).v4 =
                    crate::state::ObjectStateV4::from_card_def(definition);
            }
        }
        let FastActorResponseV1::Decision(decision_b) = b.current_response() else {
            panic!("reset terminated")
        };
        assert_eq!(
            decision_a, decision_b,
            "the two sessions must be publicly indistinguishable"
        );

        let model = SearchCapableTestModelV1::new_v1();
        let identity = search_checkpoint_identity_v1();
        let mut results = Vec::new();
        let mut published = Vec::new();
        for (tag, session) in [("leak-a", &a), ("leak-b", &b)] {
            let directory = search_scratch_directory_v1(tag);
            let mut runtime = search_runtime_v1(directory.clone());
            runtime
                .begin_episode_v1(
                    session,
                    &identity,
                    search_test_net_architecture_v1(),
                    50_301,
                    81_101,
                    PlayerSeatV1::P0,
                )
                .expect("search episode opens");
            let selected = ShadowScorerServiceV1::model_guided_search_selection_v1(
                &model,
                session,
                decision_a,
                0,
                &mut runtime,
                Instant::now(),
                ProtocolRequestKindV4::Step,
            )
            .expect("the wrapped decision completes");
            results.push(selected);
            published.push(strip_wall_time_v1(
                &fs::read(runtime.diagnostics.episode_path_v4(50_301, 81_101)).unwrap(),
            ));
            fs::remove_dir_all(&directory).ok();
        }
        assert_eq!(
            results[0], results[1],
            "the chosen action must not depend on authoritative hidden cards"
        );
        assert_eq!(
            published[0], published[1],
            "the whole search record, not just the verdict, must be invariant"
        );
    }

    /// CODEX P1. The authority must bind the LOADED checkpoint's lineage,
    /// not the generic authority-kind string every Store-backed checkpoint
    /// shares, and the ACTUAL net architecture, not the tensorizer's
    /// encoding identity.
    ///
    /// The witness is deliberately the hardest case: two Store-backed
    /// checkpoints that agree on authority kind, generation, AND weight
    /// digest, differing only in the run and manifest identity of the
    /// checkpoint that was loaded. Under the previous binding these
    /// produced the SAME authority digest, so every simulation seed
    /// derived from it was the same too, and the record's promise to name
    /// a lineage was empty.
    #[test]
    fn two_store_backed_checkpoints_yield_different_authority_digests_v1() {
        let mut first = search_checkpoint_identity_v1();
        first.authority_kind = "population-store-generation".to_owned();
        first.loaded_run_sha256 = "1".repeat(64);
        first.loaded_checkpoint_sha256 = "2".repeat(64);
        first.loaded_payload_sha256 = "3".repeat(64);
        let mut second = first.clone();
        second.loaded_run_sha256 = "4".repeat(64);
        second.loaded_checkpoint_sha256 = "5".repeat(64);
        second.loaded_payload_sha256 = "6".repeat(64);

        // Everything the OLD binding looked at is identical.
        assert_eq!(first.authority_kind, second.authority_kind);
        assert_eq!(first.loaded_generation, second.loaded_generation);
        assert_eq!(first.model_parameter_sha256, second.model_parameter_sha256);

        let mut digests = Vec::new();
        let mut lineages = Vec::new();
        for (tag, identity) in [("lineage-a", &first), ("lineage-b", &second)] {
            let directory = search_scratch_directory_v1(tag);
            let mut runtime = search_runtime_v1(directory.clone());
            let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
                50_401,
                81_201,
                256,
                32_768,
                ["Rally".to_owned(), "Burn".to_owned()],
            )
            .expect("session resets");
            runtime
                .begin_episode_v1(
                    &session,
                    identity,
                    search_test_net_architecture_v1(),
                    50_401,
                    81_201,
                    PlayerSeatV1::P0,
                )
                .expect("search episode opens");
            let bound = runtime.bound.as_ref().expect("authority bound");
            digests.push(bound.authority_digest_sha256.clone());
            lineages.push(bound.wrapper_identity.checkpoint_lineage_id.clone());
            // The architecture really is the net's, not the tensorizer's.
            assert_eq!(
                bound.wrapper_identity.net_architecture_identity,
                "kernel-policy-value-net-8"
            );
            assert_ne!(
                bound.wrapper_identity.net_architecture_identity,
                NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
                "the tensorizer identity describes encoding, not architecture"
            );
            fs::remove_dir_all(&directory).ok();
        }
        assert_ne!(
            lineages[0], lineages[1],
            "the lineage id must discriminate two different loaded checkpoints"
        );
        assert_ne!(
            digests[0], digests[1],
            "two Store-backed checkpoints must not share an authority digest"
        );
        // The lineage names what it binds, so a reader of a published
        // header can tell which checkpoint produced it.
        assert!(lineages[0].contains(&first.loaded_run_sha256));
        assert!(lineages[0].contains(&first.loaded_checkpoint_sha256));
        assert!(lineages[0].starts_with("population-store-generation|"));
    }

    /// CODEX P1. The stability halves run synchronously inside the
    /// decision, so with them enabled the protocol's per-decision latency
    /// includes them and `ceiling_status` must be classified from that
    /// full synchronous latency. Turning them off is what a formal panel
    /// does to measure product latency, and the record says which ran.
    #[test]
    fn stability_halves_are_optional_and_the_ceiling_measures_what_actually_ran_v1() {
        // The classifier itself, at both pre-registered boundaries. `>`
        // for the SLO and `>=` for the hard timeout, as pinned.
        assert_eq!(
            CeilingStatusV4::classify_v4(0.0),
            CeilingStatusV4::WithinSlo
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(4.0),
            CeilingStatusV4::WithinSlo
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(4.000_001),
            CeilingStatusV4::SloExceeded
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(19.999),
            CeilingStatusV4::SloExceeded
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(20.0),
            CeilingStatusV4::HardTimeoutExceeded
        );

        let with_directory = search_scratch_directory_v1("halves-on");
        let without_directory = search_scratch_directory_v1("halves-off");
        let (with_chosen, with_bytes) =
            run_wrapped_episode_with_halves_v1(&with_directory, 2, true);
        let (without_chosen, without_bytes) =
            run_wrapped_episode_with_halves_v1(&without_directory, 2, false);

        // Disabling a DIAGNOSTIC must not change the product's decision.
        assert_eq!(
            with_chosen, without_chosen,
            "the stability halves are diagnostics; they cannot move the chosen action"
        );

        let with_record = search_decision_record_v1(&with_bytes);
        let without_record = search_decision_record_v1(&without_bytes);
        assert_eq!(
            with_record["root_statistics_digest_sha256"],
            without_record["root_statistics_digest_sha256"],
            "the full-budget search is identical either way"
        );

        // Halves ON: recorded, and the search timer exceeds the full
        // search on its own.
        assert_eq!(with_record["stability_halves_enabled"], true);
        assert!(with_record["stability"].is_object());
        assert!(with_record["stability"]["halves_agree"].is_boolean());
        let with_full = with_record["wall_time"]["full_search_micros"]
            .as_u64()
            .unwrap();
        let with_search = with_record["wall_time"]["search_micros"].as_u64().unwrap();
        assert!(
            with_search >= with_full,
            "the halves run inside the decision, so the search timer cannot be smaller"
        );

        // Halves OFF: nulled, zero-timed, and the search timer IS the
        // full-budget search, which is the product's own search cost.
        assert_eq!(without_record["stability_halves_enabled"], false);
        assert!(without_record["stability"].is_null());
        assert_eq!(without_record["wall_time"]["stability_half_a_micros"], 0);
        assert_eq!(without_record["wall_time"]["stability_half_b_micros"], 0);
        let without_full = without_record["wall_time"]["full_search_micros"]
            .as_u64()
            .unwrap();
        let without_search = without_record["wall_time"]["search_micros"]
            .as_u64()
            .unwrap();
        assert!(
            without_search >= without_full,
            "with halves off the search timer is the full-budget search alone"
        );

        // Both classify `search_ceiling_status` from their own search
        // timer, and the PROTOCOL window strictly contains it: the encode,
        // tensorization, forward and policy sample that precede the search
        // are inside the window a client waits on, and used not to be.
        for record in [&with_record, &without_record] {
            let search = record["wall_time"]["search_micros"].as_u64().unwrap();
            assert_eq!(
                record["search_ceiling_status"].as_str().unwrap(),
                match CeilingStatusV4::classify_v4(search as f64 / 1_000_000.0) {
                    CeilingStatusV4::WithinSlo => "within_slo",
                    CeilingStatusV4::SloExceeded => "slo_exceeded",
                    CeilingStatusV4::HardTimeoutExceeded => "hard_timeout_exceeded",
                },
                "search_ceiling_status must classify the search that actually ran"
            );
            let decision = record["wall_time"]["decision_micros"].as_u64().unwrap();
            assert!(
                decision >= search,
                "the protocol window must contain the search it wraps: {decision} < {search}"
            );
        }
        fs::remove_dir_all(&with_directory).ok();
        fs::remove_dir_all(&without_directory).ok();
    }

    /// CODEX P1. The protocol verdict for decision `n` needs the publish
    /// time of decision `n`'s own record, which only a LATER record can
    /// report. Before the footer existed, the final decision of every
    /// episode had no later record, so its ceiling status was `None`:
    /// exactly one systematically dropped sample per game, and always the
    /// decision that ended it.
    ///
    /// Both closing paths a live run actually takes are exercised: end of
    /// input with the episode open (the driver stopped), and a new reset
    /// replacing an unfinished episode.
    #[test]
    fn a_closed_episode_classifies_every_decision_including_its_last_v1() {
        use crate::model_guided_search_outcome_v4::{
            episode_decision_ceilings_v4, EpisodeFooterRecordV4,
        };

        fn footer_of_v1(bytes: &[u8]) -> EpisodeFooterRecordV4 {
            let text = String::from_utf8(bytes.to_vec()).expect("diagnostics are UTF-8");
            serde_json::from_str(text.lines().next_back().expect("a record"))
                .expect("the last record is an episode footer")
        }

        // ORDERLY PROCESS EXIT: the serving loop reaches end of input with
        // the episode still open.
        let exit_directory = search_scratch_directory_v1("footer-exit");
        let mut service = search_service_with_halves_v1(exit_directory.clone(), false);
        let mut output = Vec::new();
        run_jsonl_v1(
            &mut service,
            format!("{}\n", reset_line_v1("footer-exit-reset")).as_bytes(),
            &mut output,
        )
        .expect("the serving loop closes the open episode on exit");
        assert!(
            !service
                .search
                .as_ref()
                .expect("search runtime installed")
                .diagnostics
                .has_open_episode_v4(),
            "orderly exit must leave no episode open"
        );
        let exit_bytes = fs::read(
            service
                .search
                .as_ref()
                .unwrap()
                .diagnostics
                .episode_path_v4(2, 71_501),
        )
        .expect("episode diagnostics published");
        assert!(verify_episode_chain_v4(&exit_bytes).is_ok());
        let footer = footer_of_v1(&exit_bytes);
        assert_eq!(footer.close_reason, EpisodeCloseReasonV4::ProcessExit);
        assert_eq!(footer.decision_record_count, 1);
        let ceilings = episode_decision_ceilings_v4(&exit_bytes).expect("chain verifies");
        assert_eq!(ceilings.len(), 1);
        assert!(
            ceilings[0].protocol_ceiling_status.is_some(),
            "the last decision of a closed episode must have a protocol verdict"
        );
        assert_eq!(
            ceilings[0].protocol_micros,
            Some(
                ceilings[0].decision_micros
                    + ceilings[0].publish_micros.unwrap()
                    + ceilings[0].response_micros.unwrap()
            ),
            "the protocol latency is all three synchronous phases of the request"
        );
        assert!(
            ceilings[0].response_micros.is_some(),
            "the serving loop must report a response tail, even a tiny one"
        );
        fs::remove_dir_all(&exit_directory).ok();

        // EPISODE REPLACEMENT: a new reset arrives before the old episode
        // terminated. Episode 4 keeps the P0 learner seat that episode 2
        // has, and writes a different file, so the replaced episode's
        // footer survives to be read.
        let replaced_directory = search_scratch_directory_v1("footer-replaced");
        let mut service = search_service_with_halves_v1(replaced_directory.clone(), false);
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("footer-replaced-reset")));
        assert_eq!(response["response_type"], "decision", "{response}");
        let selected = response["decision"]["selected_action_index"]
            .as_u64()
            .unwrap();
        let step = response["decision"]["step"].as_u64().unwrap();
        let stepped = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"footer-step\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
        )));
        assert_eq!(stepped["response_type"], "decision", "{stepped}");
        let replacement = value_v1(&service.handle_line_v1(
            "{\"request_type\":\"reset\",\"request_id\":\"footer-replacement\",\"episode_id\":4,\"base_seed\":71501}",
        ));
        assert_eq!(replacement["response_type"], "decision", "{replacement}");

        let replaced_bytes = fs::read(
            service
                .search
                .as_ref()
                .unwrap()
                .diagnostics
                .episode_path_v4(2, 71_501),
        )
        .expect("the replaced episode's file survives");
        assert!(verify_episode_chain_v4(&replaced_bytes).is_ok());
        let footer = footer_of_v1(&replaced_bytes);
        assert_eq!(footer.close_reason, EpisodeCloseReasonV4::EpisodeReplaced);
        assert_eq!(footer.episode_id, 2);
        let ceilings = episode_decision_ceilings_v4(&replaced_bytes).expect("chain verifies");
        assert!(
            ceilings.len() >= 2,
            "the replaced episode searched at least twice"
        );
        assert_eq!(footer.decision_record_count, ceilings.len() as u64);
        assert!(
            ceilings
                .iter()
                .all(|ceiling| ceiling.protocol_ceiling_status.is_some()),
            "no decision of a closed episode may be left unclassified"
        );
        fs::remove_dir_all(&replaced_directory).ok();
    }

    /// CODEX P1, the owner law. The measured latency is written down and
    /// never read: a decision whose publication is made artificially slow
    /// must choose the same action, with the same root statistics, as the
    /// identical fast run.
    ///
    /// The delay sits inside the diagnostics publication, which is the one
    /// timed phase that the selector could in principle observe (it calls
    /// the writer and takes its result). Everything except the timing
    /// fields themselves is compared, so this covers the chosen action,
    /// the root-statistics digest, the visit margin, and the leaf census
    /// in one assertion rather than an allowlist.
    #[test]
    fn the_chosen_action_is_independent_of_the_measured_latency_v1() {
        use crate::model_guided_search_outcome_v4::episode_decision_ceilings_v4;

        let fast_directory = search_scratch_directory_v1("latency-fast");
        let slow_directory = search_scratch_directory_v1("latency-slow");
        let (fast_chosen, fast_bytes) =
            run_wrapped_episode_configured_v1(&fast_directory, 2, false, None, true);
        let (slow_chosen, slow_bytes) = run_wrapped_episode_configured_v1(
            &slow_directory,
            2,
            false,
            Some(std::time::Duration::from_millis(120)),
            true,
        );

        assert!(!fast_chosen.is_empty());
        assert_eq!(
            fast_chosen, slow_chosen,
            "a slower decision must play exactly what the faster one played"
        );
        assert_eq!(
            strip_wall_time_v1(&fast_bytes),
            strip_wall_time_v1(&slow_bytes),
            "nothing but the timing fields may depend on the timing"
        );

        let fast = episode_decision_ceilings_v4(&fast_bytes).expect("chain verifies");
        let slow = episode_decision_ceilings_v4(&slow_bytes).expect("chain verifies");
        assert_eq!(fast.len(), slow.len());
        // The root-statistics digests are named explicitly by the finding,
        // so they are compared explicitly too rather than only through the
        // whole-record comparison above.
        let digests = |bytes: &[u8]| -> Vec<String> {
            String::from_utf8(bytes.to_vec())
                .unwrap()
                .lines()
                .filter_map(|line| {
                    let value: serde_json::Value = serde_json::from_str(line).unwrap();
                    value["root_statistics_digest_sha256"]
                        .as_str()
                        .map(str::to_owned)
                })
                .collect()
        };
        assert!(!digests(&fast_bytes).is_empty());
        assert_eq!(digests(&fast_bytes), digests(&slow_bytes));

        // The injected delay really did change the measurement: every
        // publication in the slow run is charged at least its 120 ms, and
        // the protocol latency moves with it.
        for (index, ceiling) in slow.iter().enumerate() {
            let publish = ceiling
                .publish_micros
                .expect("a closed episode reports every publish");
            assert!(
                publish >= 120_000,
                "decision {index} publish {publish} us must carry the injected delay"
            );
            // A statement about the DELAYED run alone. Comparing the two
            // runs' totals would make the fast run an implicit upper
            // bound, and a sleep only ever guarantees a minimum: a
            // descheduled fast run could exceed the delayed one and fail
            // a test that is really about where the delay landed.
            assert!(
                ceiling.protocol_micros.unwrap() >= ceiling.decision_micros + 120_000,
                "decision {index} must classify a total that contains the injected delay"
            );
        }
        fs::remove_dir_all(&fast_directory).ok();
        fs::remove_dir_all(&slow_directory).ok();

        // PART TWO: the delay moved out past the record entirely, into the
        // RESPONSE path, through the production serving loop. This is the
        // interval a record can least observe about itself: it is already
        // published, synced and reverified before the response is
        // serialized, written and flushed. Charging only the search and
        // the publication would let a slow export or a slow stdout push
        // the client's real wait past a pre-registered ceiling while the
        // classification still called it comfortably inside.
        let script = wrapped_episode_script_v1(2);
        assert!(script.len() >= 2, "the script must include at least a step");
        let prompt_directory = search_scratch_directory_v1("loop-prompt");
        let sluggish_directory = search_scratch_directory_v1("loop-sluggish");
        let prompt_bytes = run_script_through_loop_v1(&prompt_directory, &script, None);
        let sluggish_bytes = run_script_through_loop_v1(
            &sluggish_directory,
            &script,
            Some(std::time::Duration::from_millis(150)),
        );

        // Same conclusion as part one, reached by delaying a completely
        // different phase: nothing the search decides may move.
        assert_eq!(
            strip_wall_time_v1(&prompt_bytes),
            strip_wall_time_v1(&sluggish_bytes),
            "a slow response path may not change a single non-timing byte"
        );
        assert_eq!(digests(&prompt_bytes), digests(&sluggish_bytes));

        let prompt = episode_decision_ceilings_v4(&prompt_bytes).expect("chain verifies");
        let sluggish = episode_decision_ceilings_v4(&sluggish_bytes).expect("chain verifies");
        assert!(!prompt.is_empty());
        assert_eq!(prompt.len(), sluggish.len());
        for (index, ceiling) in sluggish.iter().enumerate() {
            let response = ceiling
                .response_micros
                .expect("a closed episode reports every response tail");
            assert!(
                response >= 150_000,
                "decision {index} response tail {response} us must carry the injected flush delay"
            );
            // Again a statement about the delayed run alone; see above.
            assert!(
                ceiling.protocol_micros.unwrap() >= ceiling.decision_micros + 150_000,
                "decision {index} must classify a total that contains the slow response path"
            );
        }
        // And the fast loop run really did measure a tail rather than
        // leaving the field at its unmeasured zero, so the comparison
        // above is between two measurements and not against a hole.
        assert!(prompt
            .iter()
            .all(|ceiling| ceiling.response_micros.is_some()));
        fs::remove_dir_all(&prompt_directory).ok();
        fs::remove_dir_all(&sluggish_directory).ok();
    }

    /// CODEX P2. A terminal footer whose publish fails transiently must
    /// stay owed as a TERMINAL footer.
    ///
    /// By the time the close is attempted the session already reports the
    /// episode as terminal, so the driver's retried step is rejected
    /// before it can reach the close again, and the next reset or the end
    /// of input would close the file as `episode_replaced` or
    /// `process_exit`: a game that actually ended, recorded as one that
    /// did not.
    #[test]
    fn a_failed_terminal_footer_is_retried_and_still_says_terminal_v1() {
        use crate::model_guided_search_outcome_v4::EpisodeFooterRecordV4;

        let directory = search_scratch_directory_v1("pending-terminal");
        let mut service = search_service_with_halves_v1(directory.clone(), false);
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("pending-reset")));
        assert_eq!(response["response_type"], "decision", "{response}");
        let selected = response["decision"]["selected_action_index"]
            .as_u64()
            .unwrap();
        let step = response["decision"]["step"].as_u64().unwrap();
        let step_line = format!(
            "{{\"request_type\":\"step\",\"request_id\":\"pending-step\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
        );
        let path = service
            .search
            .as_ref()
            .unwrap()
            .diagnostics
            .episode_path_v4(2, 71_501);
        let mut stage = path.clone().into_os_string();
        stage.push(".tmp");
        let stage = PathBuf::from(stage);

        // The state the finding describes: the episode reached a terminal
        // and the footer publish failed. Blocking the staging name with a
        // directory is the shape of a transient publish failure that no
        // amount of retrying inside the writer can work around, and
        // `active.current` is already None because the session is terminal.
        service.active.as_mut().unwrap().current = None;
        fs::create_dir(&stage).expect("stage name is occupiable");
        assert_eq!(
            service
                .search
                .as_mut()
                .unwrap()
                .close_terminal_episode_v1(2),
            Err("model_guided_search_diagnostics_write_failed")
        );
        assert!(service
            .search
            .as_ref()
            .unwrap()
            .has_pending_terminal_close_v1());

        // The driver retries its step. The footer is still blocked, so the
        // retry reports the diagnostics failure rather than answering
        // `episode_already_terminal` and quietly abandoning the footer.
        let blocked = value_v1(&service.handle_line_v1(&step_line));
        assert_eq!(
            blocked["error_code"], "model_guided_search_diagnostics_write_failed",
            "{blocked}"
        );

        // Unblock. The next retry publishes the footer, and only then does
        // the step get its ordinary terminal rejection.
        fs::remove_dir(&stage).expect("stage name frees");
        let retried = value_v1(&service.handle_line_v1(&step_line));
        assert_eq!(
            retried["error_code"], "episode_already_terminal",
            "{retried}"
        );
        assert!(!service
            .search
            .as_ref()
            .unwrap()
            .has_pending_terminal_close_v1());

        // A later reset would have closed this file as `episode_replaced`
        // and end of input as `process_exit`. It says what actually
        // happened.
        let text = fs::read_to_string(&path).expect("episode diagnostics published");
        assert!(verify_episode_chain_v4(text.as_bytes()).is_ok());
        let footer: EpisodeFooterRecordV4 =
            serde_json::from_str(text.lines().next_back().unwrap()).expect("a footer");
        assert_eq!(footer.close_reason, EpisodeCloseReasonV4::EpisodeTerminal);
        assert_eq!(footer.episode_id, 2);

        // The same rule holds when the retry never comes and the episode
        // is closed by a reset or by end of input instead: a remembered
        // terminal outranks whatever reason the closing path names.
        let replaced_directory = search_scratch_directory_v1("pending-terminal-replaced");
        let mut runtime = search_runtime_with_halves_v1(replaced_directory.clone(), false);
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            50_303,
            81_107,
            256,
            32_768,
            ["Rally".to_owned(), "Burn".to_owned()],
        )
        .expect("session resets");
        runtime
            .begin_episode_v1(
                &session,
                &search_checkpoint_identity_v1(),
                search_test_net_architecture_v1(),
                50_303,
                81_107,
                PlayerSeatV1::P0,
            )
            .expect("search episode opens");
        let replaced_path = runtime.diagnostics.episode_path_v4(50_303, 81_107);
        let mut replaced_stage = replaced_path.clone().into_os_string();
        replaced_stage.push(".tmp");
        let replaced_stage = PathBuf::from(replaced_stage);
        fs::create_dir(&replaced_stage).expect("stage name is occupiable");
        assert!(runtime.close_terminal_episode_v1(50_303).is_err());
        fs::remove_dir(&replaced_stage).expect("stage name frees");
        runtime
            .close_episode_v1(EpisodeCloseReasonV4::ProcessExit)
            .expect("the owed footer publishes");
        let text = fs::read_to_string(&replaced_path).unwrap();
        let footer: EpisodeFooterRecordV4 =
            serde_json::from_str(text.lines().next_back().unwrap()).expect("a footer");
        assert_eq!(
            footer.close_reason,
            EpisodeCloseReasonV4::EpisodeTerminal,
            "a remembered terminal outranks the reason the closing path names"
        );
        fs::remove_dir_all(&replaced_directory).ok();
        fs::remove_dir_all(&directory).ok();
    }

    fn search_decision_record_v1(bytes: &[u8]) -> serde_json::Value {
        let text = String::from_utf8(bytes.to_vec()).expect("diagnostics are UTF-8");
        let record: serde_json::Value =
            serde_json::from_str(text.lines().nth(1).expect("a decision record")).unwrap();
        assert_eq!(record["record_kind"], "search_decision");
        record
    }

    fn search_checkpoint_identity_v1() -> ShadowCheckpointIdentityV1 {
        ShadowCheckpointIdentityV1 {
            authority_kind: "test-only".to_owned(),
            source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
            source_generation: SOURCE_GENERATION_V1,
            source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
            source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
            source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
            source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
            loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
            loaded_generation: SOURCE_GENERATION_V1,
            loaded_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
            loaded_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
            loaded_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
            model_parameter_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1.to_owned(),
            environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
            sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
            sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        }
    }

    /// The wrapper refuses a model that cannot expose the typed native
    /// net, rather than falling back to the policy sample. A silent
    /// fallback would let a panel believe it measured the wrapper while it
    /// measured the raw policy.
    #[test]
    fn model_guided_search_fails_closed_without_a_search_capable_model_v1() {
        let directory = search_scratch_directory_v1("not-capable");
        let mut runtime = search_runtime_v1(directory.clone());
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            50_302,
            81_103,
            256,
            32_768,
            ["Rally".to_owned(), "Burn".to_owned()],
        )
        .expect("session resets");
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        runtime
            .begin_episode_v1(
                &session,
                &search_checkpoint_identity_v1(),
                search_test_net_architecture_v1(),
                50_302,
                81_103,
                PlayerSeatV1::P0,
            )
            .expect("search episode opens");
        assert!(DeterministicTestModelV1.search_capable_v1().is_none());
        assert_eq!(
            ShadowScorerServiceV1::model_guided_search_selection_v1(
                &DeterministicTestModelV1,
                &session,
                decision,
                0,
                &mut runtime,
                Instant::now(),
                ProtocolRequestKindV4::Step,
            ),
            Err("model_guided_search_model_not_search_capable")
        );
        fs::remove_dir_all(&directory).ok();
    }

    /// The wrapper is not reachable through the general selector entry
    /// point, and an unregistered seed block is a startup error rather
    /// than a silently substituted default.
    #[test]
    fn model_guided_search_configuration_fails_closed_v1() {
        assert!(run_checkpoint_shadow_stdio_with_selector_v1(
            ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis {
                root: PathBuf::from("unused"),
            },
            ShadowCandidateSelectorV1::ModelGuidedSearch,
        )
        .is_err());
        let directory = search_scratch_directory_v1("bad-seed-block");
        assert!(ModelGuidedSearchRuntimeV1::new_v1(
            KernelNativeSearchTierV1::T512,
            usize::MAX,
            directory.clone(),
            true,
        )
        .is_err());
        assert!(ModelGuidedSearchRuntimeV1::new_v1(
            KernelNativeSearchTierV1::T512,
            0,
            directory.join("missing-subdirectory"),
            true,
        )
        .is_err());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn structured_history_commits_only_after_final_physical_substep_v1() {
        let mut service = service_v1();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("history-reset")));
        assert_eq!(response["response_type"], "decision");
        let mut scored = service
            .active
            .as_ref()
            .and_then(|active| active.current.clone())
            .expect("reset produces a scored decision");
        scored.expected.physical_decision_id = 17;
        scored.expected.substep_count = 2;
        scored.expected.substep_index = 0;
        let mut history = StructuredHistoryStateV1::default();
        history.accept_selected_v1(&scored, 0).unwrap();
        assert!(history.completed.is_empty());
        assert!(history.pending.is_some());
        scored.expected.substep_index = 1;
        history.accept_selected_v1(&scored, 0).unwrap();
        assert_eq!(history.completed.len(), 1);
        assert!(history.pending.is_none());
    }

    #[test]
    fn empty_public_history_hash_matches_cross_language_contract_v1() {
        assert_eq!(
            public_history_sha256_v1(&[], 0).unwrap(),
            "fcb4e1c2d26439461ff869ee0aa61bd942763fcee571beec834e5eff4c4fefc2"
        );
    }

    #[test]
    fn depth8_branch_takes_eight_policy_decisions_before_bootstrap_v1() {
        let calls = Arc::new(Mutex::new(0usize));
        let mut service = ShadowScorerServiceV1::with_test_model_v1(Box::new(
            CountingStructuredFirstActionTestModelV1 {
                calls: Arc::clone(&calls),
            },
        ));
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("depth8-reset")));
        assert_eq!(response["response_type"], "decision");
        *calls.lock().unwrap() = 0;
        let active = service.active.as_ref().expect("reset opens a session");
        let scored = active.current.as_ref().expect("reset scores the root");
        let value = ShadowScorerServiceV1::depth8_branch_value_v1(
            service.model.as_ref(),
            None,
            &active.session,
            scored,
            &active.structured_history.completed,
            active.candidate_seat,
            0,
            0,
        )
        .unwrap();
        assert!(value.value.is_finite());
        assert_eq!(*calls.lock().unwrap(), 9);
    }

    #[test]
    fn depth8_branch_routes_opponent_decisions_to_explicit_model_v1() {
        let candidate_calls = Arc::new(Mutex::new(0usize));
        let opponent_calls = Arc::new(Mutex::new(0usize));
        let mut service = ShadowScorerServiceV1::with_test_model_v1(Box::new(
            CountingStructuredFirstActionTestModelV1 {
                calls: Arc::clone(&candidate_calls),
            },
        ));
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("opponent-reset")));
        assert_eq!(response["response_type"], "decision");
        *candidate_calls.lock().unwrap() = 0;
        let opponent = CountingStructuredFirstActionTestModelV1 {
            calls: Arc::clone(&opponent_calls),
        };
        let active = service.active.as_ref().expect("reset opens a session");
        let scored = active.current.as_ref().expect("reset scores the root");
        let value = ShadowScorerServiceV1::depth8_branch_value_v1(
            service.model.as_ref(),
            Some(&opponent),
            &active.session,
            scored,
            &active.structured_history.completed,
            active.candidate_seat,
            0,
            0,
        )
        .unwrap();
        assert!(value.value.is_finite());
        let candidate = *candidate_calls.lock().unwrap();
        let opponent = *opponent_calls.lock().unwrap();
        assert!(opponent > 0);
        assert_eq!(candidate + opponent, 9);
    }

    // ADAPTATION (fable/shadow-scorer-on-main-v1): removed
    // depth8_teacher_shadow_evaluates_search_without_changing_live_actions_v1.
    // It exercised ShadowCandidateSelectorV1::Depth8BoundedValueTeacherShadow,
    // which now fails closed (see depth8_history_value_selection_v1's own
    // adaptation comment): main's rl_session.rs has no
    // snapshot_current_actor_information_set_v1 primitive, and every
    // model scorer kept in this port returns false from
    // uses_structured_history_v1, so the selector cannot exercise the
    // search this test asserted was evaluated.

    fn service_with_teacher_export_v1(
        model: Box<dyn ShadowModelScorerV1>,
    ) -> (ShadowScorerServiceV1, SharedBytesV1) {
        let mut service = ShadowScorerServiceV1::with_test_model_v1(model);
        let bytes = SharedBytesV1::default();
        let export = XmageCp7TeacherJsonlWriterV1::from_writer_v1(
            Box::new(bytes.clone()),
            &service.identity,
            XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1,
            XMAGE_CP7_TEACHER_JSONL_CONTRACT_V1,
            XMAGE_CP7_TEACHER_SELECTION_SOURCE_V1,
        )
        .unwrap();
        service.install_teacher_export_v1(export).unwrap();
        (service, bytes)
    }

    fn teacher_rows_v1(bytes: &SharedBytesV1) -> Vec<serde_json::Value> {
        let bytes = bytes.0.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(value_v1)
            .collect()
    }

    fn service_with_outcome_export_v1(
        model: Box<dyn ShadowModelScorerV1>,
    ) -> (ShadowScorerServiceV1, SharedBytesV1) {
        let mut service = ShadowScorerServiceV1::with_test_model_v1(model);
        let bytes = SharedBytesV1::default();
        let export = XmageCp7OutcomeJsonlWriterV1::from_writer_v1(
            Box::new(bytes.clone()),
            &service.identity,
        )
        .unwrap();
        service.install_outcome_export_v1(export).unwrap();
        (service, bytes)
    }

    fn service_with_failing_outcome_export_v1(
        model: Box<dyn ShadowModelScorerV1>,
    ) -> (ShadowScorerServiceV1, SharedBytesV1, Arc<Mutex<bool>>) {
        let mut service = ShadowScorerServiceV1::with_test_model_v1(model);
        let bytes = SharedBytesV1::default();
        let fail_flush = Arc::new(Mutex::new(false));
        let export = XmageCp7OutcomeJsonlWriterV1::from_writer_v1(
            Box::new(FlushFailWriterV1 {
                bytes: bytes.clone(),
                fail_flush: Arc::clone(&fail_flush),
            }),
            &service.identity,
        )
        .unwrap();
        service.install_outcome_export_v1(export).unwrap();
        (service, bytes, fail_flush)
    }

    fn service_with_iterative_outcome_export_v1(
        model: Box<dyn ShadowModelScorerV1>,
    ) -> (ShadowScorerServiceV1, SharedBytesV1) {
        let mut service = ShadowScorerServiceV1::with_test_model_v1(model);
        service.identity.authority_kind = "xmage-cp7-outcome-reinforce-derivative-v1".to_owned();
        service.identity.loaded_generation = 7;
        service.identity.loaded_checkpoint_sha256 = "1".repeat(64);
        service.identity.loaded_payload_sha256 = "2".repeat(64);
        service.identity.loaded_train_state_sha256 = "3".repeat(64);
        service.identity.model_parameter_sha256 = "4".repeat(64);
        let bytes = SharedBytesV1::default();
        let export = XmageCp7OutcomeJsonlWriterV1::from_writer_v1(
            Box::new(bytes.clone()),
            &service.identity,
        )
        .unwrap();
        service.install_outcome_export_v1(export).unwrap();
        (service, bytes)
    }

    fn outcome_rows_v1(bytes: &SharedBytesV1) -> Vec<serde_json::Value> {
        let bytes = bytes.0.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(value_v1)
            .collect()
    }

    fn reset_line_v1(request_id: &str) -> String {
        format!(
            "{{\"request_type\":\"reset\",\"request_id\":\"{request_id}\",\"episode_id\":2,\"base_seed\":71501}}"
        )
    }

    fn value_v1(line: &str) -> serde_json::Value {
        serde_json::from_str(line).expect("response is JSON")
    }

    fn decision_projection_v1(value: &serde_json::Value) -> serde_json::Value {
        let mut decision = value["decision"].clone();
        decision
            .as_object_mut()
            .expect("decision object")
            .remove("initial_library_card_definition_ids");
        serde_json::json!({
            "checkpoint": value["checkpoint"],
            "response_type": value["response_type"],
            "decision": decision,
        })
    }

    #[test]
    fn default_original_authority_remains_hard_pinned_to_generation384_v1() {
        let checkpoint_ref = checkpoint_ref_v1(
            &ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store {
                root: PathBuf::from("unused-by-fixed-selection"),
            },
        )
        .unwrap();
        assert_eq!(checkpoint_ref.source_run_sha256, SOURCE_RUN_SHA256_V1);
        assert_eq!(checkpoint_ref.generation, SOURCE_GENERATION_V1);
        assert_eq!(
            checkpoint_ref.checkpoint_sha256,
            SOURCE_CHECKPOINT_SHA256_V1
        );
        assert_eq!(checkpoint_ref.sidecar_sha256, SOURCE_SIDECAR_SHA256_V1);
        assert_eq!(checkpoint_ref.state_sha256, SOURCE_PAYLOAD_SHA256_V1);
    }

    #[test]
    fn repeat_score_is_bit_identical_and_rust_selects_candidate_action_v1() {
        let mut service = service_v1();
        let reset = value_v1(&service.handle_line_v1(&reset_line_v1("reset-1")));
        assert_eq!(reset["response_type"], "decision");
        assert_eq!(reset["decision"]["candidate_controls_current_actor"], true);
        assert_eq!(
            reset["decision"]["deck_ids"],
            serde_json::json!(["Rally", "Rally"])
        );
        assert_eq!(reset["decision"]["randomization_identity"], "legacy_v1");
        assert_eq!(reset["decision"]["pair_index"], 1);
        let initial_libraries = reset["decision"]["initial_library_card_definition_ids"]
            .as_array()
            .expect("reset includes initial library orders");
        assert_eq!(initial_libraries.len(), 2);
        assert!(initial_libraries.iter().all(|library| library
            .as_array()
            .expect("library row")
            .len()
            == 60));
        assert!(reset["decision"]["selected_action_index"].is_u64());
        assert!(reset["decision"]["candidate_action_seed_u64_hex"].is_string());
        assert_eq!(
            reset["decision"]["logits_f32_bits"]
                .as_array()
                .expect("logits")
                .len(),
            reset["decision"]["legal_action_count"]
                .as_u64()
                .expect("legal count") as usize
        );
        let first = value_v1(&service.handle_line_v1(
            "{\"request_type\":\"score_current\",\"request_id\":\"score-1\",\"episode_id\":2,\"expected_step\":0}",
        ));
        let second = value_v1(&service.handle_line_v1(
            "{\"request_type\":\"score_current\",\"request_id\":\"score-2\",\"episode_id\":2,\"expected_step\":0}",
        ));
        assert_eq!(
            decision_projection_v1(&reset),
            decision_projection_v1(&first)
        );
        assert_eq!(
            decision_projection_v1(&first),
            decision_projection_v1(&second)
        );
    }

    #[test]
    fn stale_malformed_and_wrong_model_choice_do_not_mutate_v1() {
        let mut service = service_v1();
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("reset-1")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("selected index") as u32;
        let stale = value_v1(&service.handle_line_v1(
            &format!("{{\"request_type\":\"step\",\"request_id\":\"stale\",\"episode_id\":2,\"expected_step\":1,\"selected_index\":{selected}}}"),
        ));
        assert_eq!(stale["error_code"], "expected_step_mismatch");
        let malformed = value_v1(&service.handle_line_v1(
            "{\"request_type\":\"step\",\"request_id\":\"bad\",\"episode_id\":2,\"episode_id\":3,\"expected_step\":0,\"selected_index\":0}",
        ));
        assert_eq!(malformed["error_code"], "malformed_request");
        let wrong_selected = if selected == 0 { 1 } else { 0 };
        let wrong = value_v1(&service.handle_line_v1(
            &format!("{{\"request_type\":\"step\",\"request_id\":\"wrong\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{wrong_selected}}}"),
        ));
        assert_eq!(wrong["error_code"], "selected_index_not_model_choice");
        let after = value_v1(&service.handle_line_v1(
            "{\"request_type\":\"score_current\",\"request_id\":\"after\",\"episode_id\":2,\"expected_step\":0}",
        ));
        assert_eq!(
            decision_projection_v1(&before),
            decision_projection_v1(&after)
        );
    }

    fn service_v2() -> ShadowScorerServiceV1 {
        let mut service =
            ShadowScorerServiceV1::with_test_model_v1(Box::new(DeterministicTestModelV1));
        service.protocol = ShadowStdioProtocolV1::V2;
        service
    }

    fn live_kernel_clock_v2(service: &ShadowScorerServiceV1) -> serde_json::Value {
        let session = &service.active.as_ref().expect("active session").session;
        serde_json::to_value(KernelClockV2::from_session_v2(session)).expect("clock serializes")
    }

    fn step_line_with_clock_v2(
        request_id: &str,
        expected_step: u64,
        selected_index: u64,
        clock: &serde_json::Value,
    ) -> String {
        format!(
            concat!(
                "{{\"request_type\":\"step\",\"request_id\":\"{}\",\"episode_id\":2,",
                "\"expected_step\":{},\"selected_index\":{},\"expected_clock\":",
                "{{\"turn\":{},\"phase_step\":{},\"active_player\":{}}}}}"
            ),
            request_id,
            expected_step,
            selected_index,
            clock["turn"],
            clock["phase_step"],
            clock["active_player"],
        )
    }

    fn step_line_with_null_clock_v2(request_id: &str, selected_index: u64) -> String {
        format!(
            concat!(
                "{{\"request_type\":\"step\",\"request_id\":\"{}\",\"episode_id\":2,",
                "\"expected_step\":0,\"selected_index\":{},\"expected_clock\":null}}"
            ),
            request_id, selected_index,
        )
    }

    fn score_current_line_v1(request_id: &str) -> String {
        format!(
            "{{\"request_type\":\"score_current\",\"request_id\":\"{request_id}\",\"episode_id\":2,\"expected_step\":0}}"
        )
    }

    #[test]
    fn protocol_v1_omits_the_kernel_clock_and_refuses_expected_clock_v1() {
        let mut service = service_v1();
        let reset = value_v1(&service.handle_line_v1(&reset_line_v1("v1-reset")));
        assert_eq!(reset["protocol"], CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1);
        assert_eq!(reset["schema_version"], 1);
        assert!(
            reset["decision"].get("kernel_clock").is_none(),
            "V1 decision bodies stay byte-identical to the frozen protocol"
        );
        let scored = value_v1(&service.handle_line_v1(&score_current_line_v1("v1-score")));
        assert!(scored["decision"].get("kernel_clock").is_none());

        // A V1 service has never accepted expected_clock. Refusing it must
        // neither change V1's error surface nor advance the session.
        let selected = reset["decision"]["selected_action_index"]
            .as_u64()
            .expect("selected index");
        let clock = live_kernel_clock_v2(&service);
        let refused = value_v1(
            &service.handle_line_v1(&step_line_with_clock_v2("v1-clock", 0, selected, &clock)),
        );
        assert_eq!(refused["error_code"], "malformed_request");

        // An explicit null is *carrying* the field, not omitting it. Before V2
        // existed, deny_unknown_fields rejected this exact line; taking it as
        // absence would quietly change V1's answer to a request it has always
        // refused.
        let refused_null =
            value_v1(&service.handle_line_v1(&step_line_with_null_clock_v2("v1-null", selected)));
        assert_eq!(refused_null["error_code"], "malformed_request");

        let after = value_v1(&service.handle_line_v1(&score_current_line_v1("v1-after")));
        assert_eq!(
            decision_projection_v1(&scored),
            decision_projection_v1(&after)
        );
        let stepped = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"v1-step\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_ne!(stepped["response_type"], "error");
        assert!(stepped["decision"].get("kernel_clock").is_none());
    }

    #[test]
    fn protocol_v2_decision_bodies_carry_the_kernel_clock_v2() {
        let mut service = service_v2();
        let reset = value_v1(&service.handle_line_v1(&reset_line_v1("v2-reset")));
        assert_eq!(reset["protocol"], CHECKPOINT_SHADOW_STDIO_PROTOCOL_V2);
        assert_eq!(reset["schema_version"], 2);
        let clock = reset["decision"]["kernel_clock"].clone();
        assert_eq!(clock, live_kernel_clock_v2(&service));
        assert!(clock["turn"].as_u64().expect("turn") >= 1);
        assert_eq!(clock["phase_step"], "Main1");
        assert_eq!(clock["active_player"], "p0");
        assert_eq!(clock["priority_player"], "p0");
        assert_eq!(clock["stack_depth"], 0);

        let scored = value_v1(&service.handle_line_v1(&score_current_line_v1("v2-score")));
        assert_eq!(scored["decision"]["kernel_clock"], clock);

        // The clock describes the decision the body carries, so a step must
        // move it to the next decision's own game clock.
        let selected = reset["decision"]["selected_action_index"]
            .as_u64()
            .expect("selected index");
        let stepped = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"v2-step\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_ne!(stepped["response_type"], "error");
        assert_eq!(stepped["protocol"], CHECKPOINT_SHADOW_STDIO_PROTOCOL_V2);
        assert_eq!(
            stepped["decision"]["kernel_clock"],
            live_kernel_clock_v2(&service)
        );
    }

    #[test]
    fn protocol_v2_expected_clock_guard_fails_closed_v2() {
        let mut service = service_v2();
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("guard-reset")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("selected index");
        let clock = before["decision"]["kernel_clock"].clone();

        let mut wrong_turn = clock.clone();
        wrong_turn["turn"] = serde_json::json!(clock["turn"].as_u64().expect("turn") + 1);
        let mut wrong_step = clock.clone();
        wrong_step["phase_step"] = serde_json::json!("Main2");
        let mut wrong_actor = clock.clone();
        wrong_actor["active_player"] = serde_json::json!("p1");

        for (label, wrong) in [
            ("turn", wrong_turn),
            ("phase_step", wrong_step),
            ("active_player", wrong_actor),
        ] {
            let refused = value_v1(&service.handle_line_v1(&step_line_with_clock_v2(
                "guard-bad",
                0,
                selected,
                &wrong,
            )));
            assert_eq!(
                refused["error_code"], "clock_mismatch",
                "a disagreeing {label} must fail closed"
            );
            let after = value_v1(&service.handle_line_v1(&score_current_line_v1("guard-after")));
            assert_eq!(
                decision_projection_v1(&before),
                decision_projection_v1(&after),
                "a rejected {label} must not advance the session"
            );
        }

        // A nullable serializer emitting `"expected_clock": null` for an unset
        // guard must not be read as "no guard": that would fail open on the
        // exact path this protocol version exists to close.
        let refused_null = value_v1(
            &service.handle_line_v1(&step_line_with_null_clock_v2("guard-null", selected)),
        );
        assert_eq!(refused_null["error_code"], "malformed_request");
        let after_null = value_v1(&service.handle_line_v1(&score_current_line_v1("guard-after")));
        assert_eq!(
            decision_projection_v1(&before),
            decision_projection_v1(&after_null),
            "an explicit null must not advance the session"
        );

        // An unknown field inside the clock is a schema violation, never a
        // silently ignored hint.
        let unknown = value_v1(&service.handle_line_v1(concat!(
            "{\"request_type\":\"step\",\"request_id\":\"guard-unknown\",\"episode_id\":2,",
            "\"expected_step\":0,\"selected_index\":0,\"expected_clock\":",
            "{\"turn\":1,\"phase_step\":\"Main1\",\"active_player\":\"p0\",\"stack_depth\":0}}"
        )));
        assert_eq!(unknown["error_code"], "malformed_request");

        let accepted = value_v1(&service.handle_line_v1(&step_line_with_clock_v2(
            "guard-good",
            0,
            selected,
            &clock,
        )));
        assert_ne!(accepted["response_type"], "error");
        assert_eq!(accepted["applied_action"]["selected_index"], selected);
    }

    #[test]
    fn binding_aware_step_advances_and_commits_applied_action_v1() {
        let mut service = service_v1();
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("reset-1")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("selected index");
        let stepped = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"step-1\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_ne!(stepped["response_type"], "error");
        assert_eq!(stepped["applied_action"]["selected_index"], selected);
        assert_eq!(
            stepped["applied_action"]["candidate_order_commitment_128_hex"],
            before["decision"]["candidate_order_commitment_128_hex"]
        );
    }

    #[test]
    fn teacher_export_records_only_an_accepted_opponent_tensor_action_v1() {
        let (mut service, bytes) =
            service_with_teacher_export_v1(Box::new(DeterministicTestModelV1));
        let mut response = value_v1(&service.handle_line_v1(&reset_line_v1("teacher-reset")));
        for ordinal in 0..512_u64 {
            assert_eq!(response["response_type"], "decision");
            if !response["decision"]["candidate_controls_current_actor"]
                .as_bool()
                .unwrap()
            {
                break;
            }
            let step = response["decision"]["step"].as_u64().unwrap();
            let selected = response["decision"]["selected_action_index"]
                .as_u64()
                .unwrap();
            response = value_v1(&service.handle_line_v1(&format!(
                "{{\"request_type\":\"step\",\"request_id\":\"teacher-candidate-{ordinal}\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
            )));
        }
        assert_eq!(
            response["decision"]["candidate_controls_current_actor"],
            false
        );
        assert_eq!(teacher_rows_v1(&bytes).len(), 1);

        let step = response["decision"]["step"].as_u64().unwrap();
        let stale = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"teacher-stale\",\"episode_id\":2,\"expected_step\":{},\"selected_index\":0}}",
            step + 1
        )));
        assert_eq!(stale["error_code"], "expected_step_mismatch");
        assert_eq!(teacher_rows_v1(&bytes).len(), 1);

        let before = response["decision"].clone();
        let accepted = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"teacher-opponent\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":0}}"
        )));
        assert_ne!(accepted["response_type"], "error");
        let rows = teacher_rows_v1(&bytes);
        assert_eq!(rows.len(), 2);
        let header = &rows[0];
        assert_eq!(header["record_type"], "header");
        assert_eq!(
            header["export_contract"],
            XMAGE_CP7_TEACHER_JSONL_CONTRACT_V1
        );
        assert_eq!(header["record_ordinal"], 0);
        assert_eq!(
            header["model_input_commitment"],
            CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1
        );

        let row = &rows[1];
        assert_eq!(row["record_type"], "decision");
        assert_eq!(row["record_ordinal"], 1);
        assert_eq!(row["teacher_decision_ordinal"], 0);
        assert_eq!(row["selection_source"], "xmage_rally_cp7_mapper");
        assert_ne!(row["acting_player"], row["candidate_seat"]);
        assert_eq!(row["step"], before["step"]);
        assert_eq!(row["physical_decision_id"], before["physical_decision_id"]);
        assert_eq!(row["selected_index"], 0);
        assert_eq!(row["selected_semantic"], before["action_semantics"][0]);
        assert_eq!(row["action_semantics"], before["action_semantics"]);
        assert_eq!(row["old_policy_logits_f32_bits"], before["logits_f32_bits"]);
        assert_eq!(row["old_value_f32_bits"], before["value_f32_bits"]);
        assert_eq!(row["model_input_sha256"], before["model_input_sha256"]);
        assert_eq!(
            row["tensor"]["state_f32_bits"].as_array().unwrap().len(),
            219
        );
        assert_eq!(
            row["tensor"]["action_features_f32_bits"]
                .as_array()
                .unwrap()
                .len(),
            before["legal_action_count"].as_u64().unwrap() as usize * 195
        );
        for field in [
            "state_f32_bits",
            "object_features_f32_bits",
            "object_card_ids",
            "object_groups",
            "object_node_ids",
            "edge_features_f32_bits",
            "edge_source_indices",
            "edge_target_indices",
            "action_features_f32_bits",
            "action_ref_features_f32_bits",
            "action_ref_card_ids",
            "action_ref_action_indices",
            "action_ref_node_indices",
        ] {
            assert!(
                row["tensor"][field].is_array(),
                "missing tensor field {field}"
            );
        }
    }

    #[test]
    fn opponent_decision_that_reaches_terminal_gets_two_consecutive_record_ordinals_v1() {
        let (mut service, bytes) = service_with_teacher_export_v1(Box::new(FirstActionTestModelV1));
        service.max_physical_decisions = 4_096;
        service.max_policy_steps = 8_192;
        let mut response = value_v1(&service.handle_line_v1(&reset_line_v1("teacher-terminal")));
        let mut terminal_was_reached_by_opponent = false;
        for ordinal in 0..8_192_u64 {
            assert_eq!(response["response_type"], "decision");
            let candidate_controls = response["decision"]["candidate_controls_current_actor"]
                .as_bool()
                .unwrap();
            let step = response["decision"]["step"].as_u64().unwrap();
            let selected = response["decision"]["selected_action_index"]
                .as_u64()
                .unwrap_or(0);
            response = value_v1(&service.handle_line_v1(&format!(
                "{{\"request_type\":\"step\",\"request_id\":\"teacher-terminal-{ordinal}\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
            )));
            if response["response_type"] == "terminal" {
                terminal_was_reached_by_opponent = !candidate_controls;
                break;
            }
        }
        assert_eq!(response["response_type"], "terminal");
        assert!(terminal_was_reached_by_opponent);

        let rows = teacher_rows_v1(&bytes);
        assert!(rows.len() >= 3);
        for (expected_ordinal, row) in rows.iter().enumerate() {
            assert_eq!(row["record_ordinal"], expected_ordinal as u64);
        }
        let decision = &rows[rows.len() - 2];
        let terminal = &rows[rows.len() - 1];
        assert_eq!(decision["record_type"], "decision");
        assert_eq!(terminal["record_type"], "terminal");
        assert_eq!(
            terminal["record_ordinal"].as_u64().unwrap(),
            decision["record_ordinal"].as_u64().unwrap() + 1
        );
        let preceding_teacher_decisions = rows[..rows.len() - 2]
            .iter()
            .filter(|row| row["record_type"] == "decision")
            .count() as u64;
        assert_eq!(
            decision["teacher_decision_ordinal"],
            preceding_teacher_decisions
        );
        assert!(terminal.get("teacher_decision_ordinal").is_none());
    }

    #[test]
    fn outcome_export_records_only_accepted_candidate_tensor_action_v1() {
        let (mut service, bytes) =
            service_with_outcome_export_v1(Box::new(DeterministicTestModelV1));
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("outcome-reset")));
        assert_eq!(before["decision"]["candidate_controls_current_actor"], true);
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("candidate selection");
        let accepted = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"outcome-candidate\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_ne!(accepted["response_type"], "error");

        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["record_type"], "header");
        assert_eq!(
            rows[0]["export_contract"],
            XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1
        );
        assert_eq!(rows[0]["selection_source"], "candidate_checkpoint_policy");
        assert_eq!(
            rows[0]["model_input_commitment"],
            CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1
        );

        let row = &rows[1];
        assert_eq!(row["record_type"], "decision");
        assert_eq!(row["record_ordinal"], 1);
        assert_eq!(row["outcome_decision_ordinal"], 0);
        assert_eq!(row["acting_player"], row["candidate_seat"]);
        assert_eq!(row["selected_index"], selected);
        assert_eq!(
            row["selected_semantic"],
            before["decision"]["action_semantics"][selected as usize]
        );
        assert_eq!(
            row["old_policy_logits_f32_bits"],
            before["decision"]["logits_f32_bits"]
        );
        assert_eq!(
            row["old_policy_logits_f32_bits"][selected as usize],
            before["decision"]["logits_f32_bits"][selected as usize]
        );
        assert_eq!(
            row["old_value_f32_bits"],
            before["decision"]["value_f32_bits"]
        );
        assert_eq!(
            row["model_input_sha256"],
            before["decision"]["model_input_sha256"]
        );
        for field in [
            "state_f32_bits",
            "object_features_f32_bits",
            "object_card_ids",
            "object_groups",
            "object_node_ids",
            "edge_features_f32_bits",
            "edge_source_indices",
            "edge_target_indices",
            "action_features_f32_bits",
            "action_ref_features_f32_bits",
            "action_ref_card_ids",
            "action_ref_action_indices",
            "action_ref_node_indices",
        ] {
            assert!(
                row["tensor"][field].is_array(),
                "missing tensor field {field}"
            );
        }
    }

    #[test]
    fn outcome_export_binds_recurrent_structured_parent_inputs_v1() {
        let (mut service, bytes) =
            service_with_outcome_export_v1(Box::new(StructuredParentExportTestModelV1));
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("parent-export-reset")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("candidate selection");
        let accepted = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"parent-export-step\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_ne!(accepted["response_type"], "error");

        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len(), 2);
        let row = &rows[1];
        let legal = before["decision"]["legal_action_count"]
            .as_u64()
            .expect("legal action count") as usize;
        let expected = (0..legal)
            .map(|index| serde_json::json!((index as f32 * 0.25 - 0.75).to_bits()))
            .collect::<Vec<_>>();
        assert_eq!(
            row["structured_parent_policy_logits_f32_bits"],
            serde_json::Value::Array(expected)
        );
        assert_eq!(
            row["structured_parent_value_f32_bits"],
            serde_json::json!((-0.5f32).to_bits())
        );
        assert_ne!(
            row["structured_parent_policy_logits_f32_bits"],
            row["old_policy_logits_f32_bits"]
        );
        assert_ne!(
            row["structured_parent_value_f32_bits"],
            row["old_value_f32_bits"]
        );
    }

    #[test]
    fn outcome_export_write_failure_poisoning_prevents_retry_v1() {
        let (mut service, bytes, fail_flush) =
            service_with_failing_outcome_export_v1(Box::new(DeterministicTestModelV1));
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("outcome-poison-reset")));
        assert_eq!(before["decision"]["candidate_controls_current_actor"], true);
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("candidate selection");
        *fail_flush.lock().unwrap() = true;

        let request = format!(
            "{{\"request_type\":\"step\",\"request_id\":\"outcome-poison-step\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        );
        let failed = value_v1(&service.handle_line_v1(&request));
        assert_eq!(failed["error_code"], "outcome_export_write_failed");
        assert!(service.export_poisoned);
        assert_eq!(outcome_rows_v1(&bytes).len(), 2);

        let retry = value_v1(&service.handle_line_v1(&request));
        assert_eq!(retry["request_id"], "outcome-poison-step");
        assert_eq!(retry["error_code"], "export_poisoned");
        assert_eq!(outcome_rows_v1(&bytes).len(), 2);
    }

    #[test]
    fn iterative_outcome_export_repeats_exact_parent_identity_on_all_rows_v2() {
        let (mut service, bytes) =
            service_with_iterative_outcome_export_v1(Box::new(FirstActionTestModelV1));
        service.max_physical_decisions = 4_096;
        service.max_policy_steps = 8_192;
        let mut response = value_v1(&service.handle_line_v1(&reset_line_v1("iterative-outcome")));
        for ordinal in 0..8_192_u64 {
            assert_eq!(response["response_type"], "decision");
            let step = response["decision"]["step"].as_u64().unwrap();
            let selected = response["decision"]["selected_action_index"]
                .as_u64()
                .unwrap_or(0);
            response = value_v1(&service.handle_line_v1(&format!(
                "{{\"request_type\":\"step\",\"request_id\":\"iterative-outcome-{ordinal}\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
            )));
            assert_ne!(response["response_type"], "error");
            if response["response_type"] == "terminal" {
                break;
            }
        }
        assert_eq!(response["response_type"], "terminal");
        let rows = outcome_rows_v1(&bytes);
        let header = &rows[0];
        assert_eq!(header["schema_version"], 2);
        assert_eq!(
            header["export_contract"],
            XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2
        );
        assert_eq!(
            header["checkpoint"]["loaded_checkpoint_sha256"],
            "1".repeat(64)
        );
        assert!(rows[1..]
            .iter()
            .all(|row| row["schema_version"] == 2 && row["checkpoint"] == header["checkpoint"]));
        assert_eq!(rows.last().unwrap()["record_type"], "terminal");
    }

    #[test]
    fn structured_successor_outcome_export_uses_verified_v2_contract() {
        let mut service =
            ShadowScorerServiceV1::with_test_model_v1(Box::new(FirstActionTestModelV1));
        service.identity.authority_kind =
            "xmage-cp7-outcome-structured-policy-successor-v8".to_owned();
        service.identity.loaded_generation = 1;
        let bytes = SharedBytesV1::default();
        XmageCp7OutcomeJsonlWriterV1::from_writer_v1(Box::new(bytes.clone()), &service.identity)
            .unwrap();
        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["schema_version"], 2);
        assert_eq!(
            rows[0]["export_contract"],
            XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2
        );
    }

    #[test]
    fn population_store_outcome_export_uses_verified_v2_contract() {
        let mut service =
            ShadowScorerServiceV1::with_test_model_v1(Box::new(FirstActionTestModelV1));
        service.identity.authority_kind = "population-store-validated-generation".to_owned();
        service.identity.source_generation = 1024;
        service.identity.loaded_generation = 1024;
        service.identity.environment_trajectory_contract =
            POPULATION_STORE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1;
        let bytes = SharedBytesV1::default();
        XmageCp7OutcomeJsonlWriterV1::from_writer_v1(Box::new(bytes.clone()), &service.identity)
            .unwrap();
        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["schema_version"], 2);
        assert_eq!(rows[0]["checkpoint"]["source_generation"], 1024);

        service.identity.loaded_generation = 1028;
        assert!(XmageCp7OutcomeJsonlWriterV1::from_writer_v1(
            Box::new(io::sink()),
            &service.identity,
        )
        .is_err());
    }

    #[test]
    fn outcome_terminal_binds_reward_and_contiguous_candidate_decisions_v1() {
        let (mut service, bytes) = service_with_outcome_export_v1(Box::new(FirstActionTestModelV1));
        service.max_physical_decisions = 4_096;
        service.max_policy_steps = 8_192;
        let mut response = value_v1(&service.handle_line_v1(&reset_line_v1("outcome-terminal")));
        let mut accepted_candidate_decisions = 0_u64;
        let mut terminal_was_reached_by_opponent = false;
        for ordinal in 0..8_192_u64 {
            assert_eq!(response["response_type"], "decision");
            let candidate_controls = response["decision"]["candidate_controls_current_actor"]
                .as_bool()
                .expect("candidate control bit");
            let step = response["decision"]["step"].as_u64().expect("step");
            let selected = response["decision"]["selected_action_index"]
                .as_u64()
                .unwrap_or(0);
            response = value_v1(&service.handle_line_v1(&format!(
                "{{\"request_type\":\"step\",\"request_id\":\"outcome-terminal-{ordinal}\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
            )));
            assert_ne!(response["response_type"], "error");
            accepted_candidate_decisions += u64::from(candidate_controls);
            if response["response_type"] == "terminal" {
                terminal_was_reached_by_opponent = !candidate_controls;
                break;
            }
        }
        assert_eq!(response["response_type"], "terminal");
        assert!(terminal_was_reached_by_opponent);

        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len() as u64, accepted_candidate_decisions + 2);
        for (record_ordinal, row) in rows.iter().enumerate() {
            assert_eq!(row["record_ordinal"], record_ordinal as u64);
        }
        let decisions = &rows[1..rows.len() - 1];
        for (outcome_ordinal, row) in decisions.iter().enumerate() {
            assert_eq!(row["record_type"], "decision");
            assert_eq!(row["outcome_decision_ordinal"], outcome_ordinal as u64);
            assert_eq!(row["acting_player"], row["candidate_seat"]);
        }
        let terminal = rows.last().expect("terminal row");
        assert_eq!(terminal["record_type"], "terminal");
        assert_eq!(terminal["first_outcome_decision_ordinal"], 0);
        assert_eq!(
            terminal["outcome_decision_count"],
            accepted_candidate_decisions
        );
        assert_eq!(terminal["terminal"]["terminal_classification"], "natural");
        assert_eq!(terminal["terminal"]["terminal_code"], "natural_game_over");
        let reward_index = if terminal["candidate_seat"] == "p0" {
            0
        } else {
            1
        };
        assert_eq!(
            terminal["candidate_terminal_reward"],
            terminal["terminal"]["terminal_reward"][reward_index]
        );
    }

    #[test]
    fn outcome_export_rejects_reset_while_episode_is_open_v1() {
        let (mut service, bytes) =
            service_with_outcome_export_v1(Box::new(DeterministicTestModelV1));
        let first = value_v1(&service.handle_line_v1(&reset_line_v1("outcome-first")));
        assert_eq!(first["response_type"], "decision");
        let second = value_v1(&service.handle_line_v1(&reset_line_v1("outcome-second")));
        assert_eq!(second["error_code"], "outcome_export_episode_incomplete");
        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["record_type"], "header");
    }

    #[test]
    fn rejected_terminal_transition_does_not_export_outcome_rows_v1() {
        let (mut service, bytes) =
            service_with_outcome_export_v1(Box::new(DeterministicTestModelV1));
        service.max_physical_decisions = 1;
        service.max_policy_steps = 128;
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("outcome-cap")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("candidate selection");
        let rejected = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"outcome-cap-step\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_eq!(rejected["error_code"], "native_terminal_validation_failed");
        let rows = outcome_rows_v1(&bytes);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["record_type"], "header");
    }

    #[test]
    fn rejected_terminal_transition_does_not_export_teacher_rows_v1() {
        let (mut service, bytes) =
            service_with_teacher_export_v1(Box::new(DeterministicTestModelV1));
        service.max_physical_decisions = 1;
        service.max_policy_steps = 128;
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("teacher-cap")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .unwrap();
        let rejected = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"teacher-cap-step\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_eq!(rejected["error_code"], "native_terminal_validation_failed");
        let rows = teacher_rows_v1(&bytes);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["record_type"], "header");
    }

    #[test]
    fn cap_terminal_is_rejected_and_step_rolls_back_v1() {
        let mut service = service_v1();
        service.max_physical_decisions = 1;
        service.max_policy_steps = 128;
        let before = value_v1(&service.handle_line_v1(&reset_line_v1("reset-cap")));
        let selected = before["decision"]["selected_action_index"]
            .as_u64()
            .expect("candidate selected index");
        let rejected = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"step\",\"request_id\":\"step-cap\",\"episode_id\":2,\"expected_step\":0,\"selected_index\":{selected}}}"
        )));
        assert_eq!(rejected["error_code"], "native_terminal_validation_failed");
        let after = value_v1(&service.handle_line_v1(
            "{\"request_type\":\"score_current\",\"request_id\":\"after-cap\",\"episode_id\":2,\"expected_step\":0}",
        ));
        assert_eq!(
            decision_projection_v1(&before),
            decision_projection_v1(&after)
        );
    }

    #[test]
    fn score_current_terminal_enforces_episode_and_policy_step_v1() {
        let mut service =
            ShadowScorerServiceV1::with_test_model_v1(Box::new(FirstActionTestModelV1));
        service.max_physical_decisions = 4_096;
        service.max_policy_steps = 8_192;
        let mut response = value_v1(&service.handle_line_v1(&reset_line_v1("reset-terminal")));
        for ordinal in 0..8_192_u64 {
            if response["response_type"] == "terminal" {
                break;
            }
            assert_eq!(response["response_type"], "decision");
            let step = response["decision"]["step"]
                .as_u64()
                .expect("decision step");
            let selected = response["decision"]["selected_action_index"]
                .as_u64()
                .unwrap_or(0);
            response = value_v1(&service.handle_line_v1(&format!(
                "{{\"request_type\":\"step\",\"request_id\":\"drive-{ordinal}\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
            )));
        }
        assert_eq!(response["response_type"], "terminal");
        assert_eq!(
            response["terminal"]["terminal"]["terminal_classification"],
            "natural"
        );
        let terminal_step = response["terminal"]["terminal"]["policy_step_count"]
            .as_u64()
            .expect("terminal policy step");

        let wrong_episode = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"score_current\",\"request_id\":\"terminal-wrong-episode\",\"episode_id\":3,\"expected_step\":{terminal_step}}}"
        )));
        assert_eq!(wrong_episode["error_code"], "episode_id_mismatch");
        let wrong_step = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"score_current\",\"request_id\":\"terminal-wrong-step\",\"episode_id\":2,\"expected_step\":{}}}",
            terminal_step + 1
        )));
        assert_eq!(wrong_step["error_code"], "expected_step_mismatch");
        let current = value_v1(&service.handle_line_v1(&format!(
            "{{\"request_type\":\"score_current\",\"request_id\":\"terminal-current\",\"episode_id\":2,\"expected_step\":{terminal_step}}}"
        )));
        assert_eq!(current["response_type"], "terminal");
    }

    #[test]
    fn jsonl_loop_emits_one_lf_response_per_request_v1() {
        let mut service = service_v1();
        let input = format!(
            "{}\n{{\"request_type\":\"score_current\",\"request_id\":\"score-1\",\"episode_id\":2,\"expected_step\":0}}\n",
            reset_line_v1("reset-1")
        );
        let mut output = Vec::new();
        run_jsonl_v1(&mut service, input.as_bytes(), &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.ends_with('\n'));
        assert_eq!(output.lines().count(), 2);
        assert!(output
            .lines()
            .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok()));
    }

    #[test]
    fn cp7_behavior_clone_derivative_authority_fails_closed_on_this_lineage_v1() {
        // This lineage deliberately omits the behavior-clone scorer subsystem
        // (see the port commit); the authority must error loudly, never fall
        // through to a default scorer.
        let result = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative {
                root: PathBuf::from("unused"),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn xmage_cp7_outcome_reinforce_variant_fails_closed_on_this_lineage_v1() {
        // The outcome-REINFORCE sub-variant was trimmed in the port; only the
        // fixed_native_state sub-path remains supported. The trimmed variant
        // must error loudly.
        let result = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative {
                root: PathBuf::from("unused"),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "reads the portable promoted2 weights authority from an external evidence root"]
    fn real_portable_authority_loads_and_scores_v1() {
        let root = std::env::var_os("MTG_KERNEL_SHADOW_PORTABLE_ROOT_V1")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(
                    "/mnt/d/mtg-kernel-action-ingress-microrung-v1-retry-06-20260728/runs/slot-08-seed-940003-arm-A/store",
                )
            });
        let mut service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { root },
        )
        .unwrap();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("portable")));
        assert_eq!(response["response_type"], "decision");
        assert_eq!(
            response["checkpoint"]["model_parameter_sha256"],
            SOURCE_MODEL_PARAMETER_SHA256_V1
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "walks the original external promoted2 generation-384 Store"]
    fn real_original_generation384_authority_loads_and_scores_v1() {
        let root = std::env::var_os("MTG_KERNEL_SHADOW_ORIGINAL_ROOT_V1")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary"));
        let mut service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { root },
        )
        .unwrap();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("original")));
        assert_eq!(response["response_type"], "decision");
        assert_eq!(response["checkpoint"]["loaded_generation"], 384);
        assert_eq!(
            response["checkpoint"]["loaded_checkpoint_sha256"],
            SOURCE_CHECKPOINT_SHA256_V1
        );
    }

    #[test]
    #[ignore = "reads the original external promoted2 Store at selected generation 0"]
    fn real_original_selected_generation0_authority_loads_and_reports_exact_identity_v1() {
        let root = std::env::var_os("MTG_KERNEL_SHADOW_ORIGINAL_ROOT_V1")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    PathBuf::from(r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary")
                } else {
                    PathBuf::from("/mnt/d/mtg-kernel-ladder-pilot-20260725/pool3/primary")
                }
            });
        let mut service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration {
                root,
                generation: 0,
            },
        )
        .unwrap();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("selected-g0")));
        assert_eq!(response["response_type"], "decision");
        let checkpoint = &response["checkpoint"];
        assert_eq!(
            checkpoint["authority_kind"],
            "original-promoted2-validated-store-generation"
        );
        assert_eq!(checkpoint["source_run_sha256"], SOURCE_RUN_SHA256_V1);
        assert_eq!(checkpoint["source_generation"], 0);
        assert_eq!(checkpoint["loaded_generation"], 0);
        assert_eq!(
            checkpoint["source_checkpoint_sha256"],
            "82b25bbcf340015d2f353dbf3bd877c44e5e57ef53fb4b2cc4d3aa9ff54bd8af"
        );
        assert_eq!(
            checkpoint["source_sidecar_sha256"],
            "7e64b53e7d74f82a5a6746e98cc99ef6859afd6ceae43fd708d0adca02866a4e"
        );
        assert_eq!(
            checkpoint["source_payload_sha256"],
            "795dc4245d02a9b10702c3df282e7819e8e6657896e1db53c05e711ee32c9c9a"
        );
        assert_eq!(
            checkpoint["source_train_state_sha256"],
            "95b1c20acbd09ba4d65b80aed953811618858e6e182d98e3904026decf1fcf82"
        );
        assert_eq!(
            checkpoint["model_parameter_sha256"],
            "0635d2defb8facd700ede34789434956fc4a2fd3b5058cc2df5dd820398b4c22"
        );
        assert_eq!(
            checkpoint["source_checkpoint_sha256"],
            checkpoint["loaded_checkpoint_sha256"]
        );
        assert_eq!(
            checkpoint["source_payload_sha256"],
            checkpoint["loaded_payload_sha256"]
        );
        assert_eq!(
            checkpoint["source_train_state_sha256"],
            checkpoint["loaded_train_state_sha256"]
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "walks the original external promoted2 Store to selected generation 256"]
    fn real_original_selected_generation256_authority_loads_and_reports_exact_identity_v1() {
        let root = std::env::var_os("MTG_KERNEL_SHADOW_ORIGINAL_ROOT_V1")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    PathBuf::from(r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary")
                } else {
                    PathBuf::from("/mnt/d/mtg-kernel-ladder-pilot-20260725/pool3/primary")
                }
            });
        let mut service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration {
                root,
                generation: 256,
            },
        )
        .unwrap();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("selected-g256")));
        assert_eq!(response["response_type"], "decision");
        let checkpoint = &response["checkpoint"];
        assert_eq!(
            checkpoint["authority_kind"],
            "original-promoted2-validated-store-generation"
        );
        assert_eq!(checkpoint["source_run_sha256"], SOURCE_RUN_SHA256_V1);
        assert_eq!(checkpoint["source_generation"], 256);
        assert_eq!(checkpoint["loaded_generation"], 256);
        assert_eq!(
            checkpoint["source_checkpoint_sha256"],
            "9b538d3c7b2d70ef6cecf812610ecb177930efd846c04ebf5ccdeb72e967dd3a"
        );
        assert_eq!(
            checkpoint["source_sidecar_sha256"],
            "f57448b4552ac4da59ec7ee266c3b81564461b1f62b0b2670ad792cd1bf082a3"
        );
        assert_eq!(
            checkpoint["source_payload_sha256"],
            "cd46a1d0bc3f73e0620f8179f0e049f6529ccd44a1d05e9a6577c85810c6a153"
        );
        assert_eq!(
            checkpoint["source_train_state_sha256"],
            "1fd307374184d03eb89568c3de1d6f1012aa480c0b16f852e4e83f63167b1504"
        );
        assert_eq!(
            checkpoint["model_parameter_sha256"],
            "8a123fbc5deaa12a0840513a6f0fc9c4280357396e76109757424fd7d4e9b999"
        );
        assert_eq!(
            checkpoint["source_checkpoint_sha256"],
            checkpoint["loaded_checkpoint_sha256"]
        );
        assert_eq!(
            checkpoint["source_payload_sha256"],
            checkpoint["loaded_payload_sha256"]
        );
        assert_eq!(
            checkpoint["source_train_state_sha256"],
            checkpoint["loaded_train_state_sha256"]
        );
    }
}
