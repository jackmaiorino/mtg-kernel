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
use crate::native_bilinear_policy_residual_v1::{
    load_native_rank1_policy_residual_inference_v1, NativeRank1PolicyResidualInferenceV1,
};
use crate::native_checkpoint_inference_v1::{
    NativeCheckpointInferenceOutputV1, NativeCheckpointInferenceV1,
};
use crate::native_cp7_behavior_clone_v1::{
    load_cp7_behavior_clone_inference_v1, NativeCp7BehaviorCloneInferenceV1,
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
use crate::native_structured_history_stack_v1::{
    load_native_structured_history_stack_inference_v1, NativeStructuredHistoryStackInferenceV1,
};
use crate::native_structured_policy_residual_v1::{
    load_native_structured_policy_residual_inference_v1, NativeStructuredHistoryEntryV1,
    NativeStructuredPolicyResidualInferenceV1, CARD_VOCAB_V1, HISTORY_LENGTH_V1,
    PARENT_NATIVE_STATE_SHA256_V1,
};
use crate::native_structured_policy_successor_v1::{
    load_native_structured_policy_successor_inference_v1,
    NativeStructuredPolicySuccessorInferenceV1,
    CANDIDATE_FILENAME_V1 as STRUCTURED_POLICY_SUCCESSOR_CANDIDATE_FILENAME_V1,
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
use crate::native_xmage_cp7_outcome_reinforce_v1::{
    load_xmage_cp7_outcome_inference_v1, NativeXmageCp7OutcomeInferenceV1,
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
use std::sync::Mutex;

pub const CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1: &str = "mtg-kernel-checkpoint-shadow-stdio/v1";
pub const CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1: u32 = 1;
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

struct LoadedShadowCheckpointV1 {
    inference: NativeCheckpointInferenceV1,
    identity: ShadowCheckpointIdentityV1,
    max_physical_decisions: u64,
    max_policy_steps: u64,
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

fn load_checkpoint_v1(
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

trait ShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        false
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

impl ShadowModelScorerV1 for NativeShadowModelScorerV1 {
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

impl ShadowModelScorerV1 for FixedNativeStateShadowModelScorerV1 {
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

struct Cp7BehaviorCloneShadowModelScorerV1 {
    inference: NativeCp7BehaviorCloneInferenceV1,
}

struct XmageCp7OutcomeShadowModelScorerV1 {
    inference: NativeXmageCp7OutcomeInferenceV1,
}

struct NativeRank1PolicyResidualShadowModelScorerV1 {
    inference: NativeRank1PolicyResidualInferenceV1,
}

struct NativeStructuredPolicyResidualShadowModelScorerV1 {
    inference: NativeStructuredPolicyResidualInferenceV1,
}

struct NativeStructuredPolicySuccessorShadowModelScorerV1 {
    inference: NativeStructuredPolicySuccessorInferenceV1,
}

struct NativeStructuredHistoryStackShadowModelScorerV1 {
    inference: NativeStructuredHistoryStackInferenceV1,
}

struct NativeQualifiedPolicyBoundedValueShadowModelScorerV1 {
    policy: NativeStructuredPolicySuccessorInferenceV1,
    value: NativeStructuredPolicyResidualInferenceV1,
}

impl ShadowModelScorerV1 for XmageCp7OutcomeShadowModelScorerV1 {
    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        _history: &[NativeStructuredHistoryEntryV1],
        _acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output = self.inference.score_decision_v1(decision)?;
        Ok(ShadowModelOutputV1 {
            logits: output.logits_v1().to_vec(),
            value: output.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

impl ShadowModelScorerV1 for NativeRank1PolicyResidualShadowModelScorerV1 {
    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        _history: &[NativeStructuredHistoryEntryV1],
        _acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output = self.inference.score_decision_v1(decision)?;
        Ok(ShadowModelOutputV1 {
            logits: output.logits_v1().to_vec(),
            value: output.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

impl ShadowModelScorerV1 for NativeStructuredPolicyResidualShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        self.inference.is_history_aware_v1()
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output =
            self.inference
                .score_decision_with_history_v1(decision, history, acting_player)?;
        Ok(ShadowModelOutputV1 {
            logits: output.logits_v1().to_vec(),
            value: output.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

impl ShadowModelScorerV1 for NativeStructuredPolicySuccessorShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        true
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output =
            self.inference
                .score_decision_with_history_v1(decision, history, acting_player)?;
        Ok(ShadowModelOutputV1 {
            logits: output.logits_v1().to_vec(),
            value: output.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

impl ShadowModelScorerV1 for NativeQualifiedPolicyBoundedValueShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        true
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let policy =
            self.policy
                .score_decision_with_history_v1(decision, history, acting_player)?;
        let value = self
            .value
            .score_decision_with_history_v1(decision, history, acting_player)?;
        if policy.logits_v1().len() != value.logits_v1().len() {
            return Err(());
        }
        Ok(ShadowModelOutputV1 {
            logits: policy.logits_v1().to_vec(),
            value: value.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

impl ShadowModelScorerV1 for NativeStructuredHistoryStackShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        true
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output =
            self.inference
                .score_decision_with_history_v1(decision, history, acting_player)?;
        Ok(ShadowModelOutputV1 {
            logits: output.logits_v1().to_vec(),
            value: output.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
    }
}

impl ShadowModelScorerV1 for Cp7BehaviorCloneShadowModelScorerV1 {
    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        _history: &[NativeStructuredHistoryEntryV1],
        _acting_player: u8,
        _substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        let output = self.inference.score_decision_v1(decision)?;
        Ok(ShadowModelOutputV1 {
            logits: output.logits_v1().to_vec(),
            value: output.value_v1(),
            structured_parent_logits: None,
            structured_parent_value: None,
        })
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

fn player_seat_index_v1(seat: PlayerSeatV1) -> u8 {
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

struct RecurrentCp7ShadowModelScorerV1 {
    parent: NativeStructuredPolicySuccessorInferenceV1,
    worker: Mutex<RecurrentCp7WorkerV1>,
}

impl ShadowModelScorerV1 for RecurrentCp7ShadowModelScorerV1 {
    fn uses_structured_history_v1(&self) -> bool {
        true
    }

    fn score_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
        substep_count: u32,
    ) -> Result<ShadowModelOutputV1, ()> {
        if acting_player > 1 || substep_count == 0 || history.len() > HISTORY_LENGTH_V1 {
            return Err(());
        }
        let parent =
            self.parent
                .score_decision_with_history_v1(decision, history, acting_player)?;
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
        let mut history_f32_bits = Vec::with_capacity(history.len() * 237);
        for entry in history {
            history_f32_bits.extend(
                entry
                    .actor_relative_features_v1(acting_player)?
                    .into_iter()
                    .map(f32::to_bits),
            );
        }
        let mut worker = self.worker.lock().map_err(|_| ())?;
        let request = RecurrentCp7RequestV1 {
            schema: RECURRENT_CP7_REQUEST_SCHEMA_V1,
            sequence: worker.sequence,
            acting_player,
            substep_count,
            tensor: XmageCp7TeacherTensorV1::from_native_v1(&tensor),
            history_f32_bits,
            parent_logits_f32_bits: parent
                .logits_v1()
                .iter()
                .map(|value| value.to_bits())
                .collect(),
            parent_value_f32_bits: parent.value_v1().to_bits(),
        };
        let (logits, recurrent_value) = worker.exchange_v1(&request).map_err(|_| ())?;
        if logits.len() != parent.logits_v1().len() {
            return Err(());
        }
        Ok(ShadowModelOutputV1 {
            logits,
            value: recurrent_value.unwrap_or_else(|| parent.value_v1()),
            structured_parent_logits: Some(parent.logits_v1().to_vec()),
            structured_parent_value: Some(parent.value_v1()),
        })
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
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "record ordinal exhausted"))?;
        self.next_teacher_decision_ordinal = self
            .next_teacher_decision_ordinal
            .checked_add(1)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "teacher decision ordinal exhausted")
            })?;
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
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "record ordinal exhausted"))?;
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
        episode.outcome_decision_count =
            episode
                .outcome_decision_count
                .checked_add(1)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Other, "episode decision count exhausted")
                })?;
        self.next_record_ordinal = self
            .next_record_ordinal
            .checked_add(1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "record ordinal exhausted"))?;
        self.next_outcome_decision_ordinal = self
            .next_outcome_decision_ordinal
            .checked_add(1)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "outcome decision ordinal exhausted")
            })?;
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
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "record ordinal exhausted"))?;
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

struct ShadowScorerServiceV1 {
    model: Box<dyn ShadowModelScorerV1>,
    opponent_model: Option<Box<dyn ShadowModelScorerV1>>,
    population_opponent: Option<LadderOpponentEngineV1>,
    identity: ShadowCheckpointIdentityV1,
    candidate_selector: ShadowCandidateSelectorV1,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    active: Option<ActiveShadowSessionV1>,
    teacher_export: Option<XmageCp7TeacherJsonlWriterV1>,
    outcome_export: Option<XmageCp7OutcomeJsonlWriterV1>,
    export_poisoned: bool,
}

impl ShadowScorerServiceV1 {
    fn load_recurrent_cp7_v1(
        root: PathBuf,
        python_executable: PathBuf,
    ) -> Result<Self, ShadowScorerStartupErrorV1> {
        let authority_error =
            || ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority);
        let identity_error =
            || ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointIdentity);
        if !root.is_dir() || !python_executable.is_file() {
            return Err(authority_error());
        }
        let inventory = fs::read_dir(&root)
            .map_err(|_| authority_error())?
            .map(|entry| {
                let entry = entry.map_err(|_| ())?;
                let file_type = entry.file_type().map_err(|_| ())?;
                if file_type.is_symlink() {
                    return Err(());
                }
                Ok((
                    entry.file_name().into_string().map_err(|_| ())?,
                    file_type.is_dir(),
                ))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, ()>>()
            .map_err(|_| authority_error())?;
        if inventory
            != std::collections::BTreeMap::from([
                ("model.pt".to_owned(), false),
                ("model_v1.py".to_owned(), false),
                ("parent".to_owned(), true),
                (RECURRENT_CP7_MANIFEST_FILENAME_V1.to_owned(), false),
                ("worker_v1.py".to_owned(), false),
            ])
        {
            return Err(authority_error());
        }
        let manifest_path = root.join(RECURRENT_CP7_MANIFEST_FILENAME_V1);
        let manifest_bytes = fs::read(&manifest_path).map_err(|_| authority_error())?;
        if !manifest_bytes.ends_with(b"\n") || manifest_bytes.contains(&b'\r') {
            return Err(authority_error());
        }
        let manifest_text = std::str::from_utf8(&manifest_bytes).map_err(|_| authority_error())?;
        let manifest_value =
            parse_strict_json_value(manifest_text).map_err(|_| authority_error())?;
        let package: RecurrentCp7PackageV1 =
            serde_json::from_value(manifest_value).map_err(|_| authority_error())?;
        let cp7_package = package.schema == RECURRENT_CP7_PACKAGE_SCHEMA_V1;
        let terminal_package = package.schema == RECURRENT_TERMINAL_PACKAGE_SCHEMA_V1;
        if (!cp7_package && !terminal_package)
            || package.architecture != "width128-two-layer-gru-structured-cp7-residual/v1"
            || package.files.model.path != "model.pt"
            || package.files.model_definition.path != "model_v1.py"
            || package.files.worker.path != "worker_v1.py"
            || package.parent.path != "parent"
            || package.git_commit.len() != 40
            || package.source.full_refit_report_sha256
                != "7c333e8bec2d332eb5dfba764f29df39d801211e74c0052bb2fd8555c68455f4"
            || package.source.deployment_calibration_report_sha256
                != "f3fc251dfcda2e742b02bca5d92e4eb38c2e5afe3f203a00b9a2bebfa7fe3b82"
            || (cp7_package
                && (package.identity.authority_kind != "recurrent-cp7-deployment-v1"
                    || package.deployment_scale.to_bits() != 0.97f64.to_bits()
                    || package.log_ratio_budget.to_bits() != 0.49f64.to_bits()
                    || package.source.terminal_training_report_sha256.is_some()
                    || package.non_claims
                        != [
                            "CP7 label fit is not playing strength".to_owned(),
                            "terminal win or loss remains the only promotion measure".to_owned(),
                        ]))
            || (terminal_package
                && (package.identity.authority_kind != "recurrent-terminal-policy-deployment-v1"
                    || package.deployment_scale.to_bits() != 1.0f64.to_bits()
                    || package.log_ratio_budget.to_bits() != 0.20f64.to_bits()
                    || package
                        .source
                        .terminal_training_report_sha256
                        .as_deref()
                        .is_none_or(|digest| !recurrent_cp7_is_sha256_v1(digest))
                    || package.non_claims
                        != [
                            "training diagnostics are not playing strength".to_owned(),
                            "terminal win or loss remains the only promotion measure".to_owned(),
                        ]))
        {
            return Err(identity_error());
        }
        for digest in [
            package.files.model.sha256.as_str(),
            package.files.model_definition.sha256.as_str(),
            package.files.worker.sha256.as_str(),
            package.model_state_sha256.as_str(),
            package.parent.candidate_sha256.as_str(),
            package.parent.weights_sha256.as_str(),
            package.parent.report_sha256.as_str(),
            package.parent.composite_model_parameter_sha256.as_str(),
            package.identity.model_parameter_sha256.as_str(),
        ] {
            if !recurrent_cp7_is_sha256_v1(digest) {
                return Err(authority_error());
            }
        }
        for (relative, expected) in [
            ("model.pt", package.files.model.sha256.as_str()),
            (
                "model_v1.py",
                package.files.model_definition.sha256.as_str(),
            ),
            ("worker_v1.py", package.files.worker.sha256.as_str()),
        ] {
            if recurrent_cp7_sha256_v1(&root.join(relative))? != expected {
                return Err(identity_error());
            }
        }
        if recurrent_cp7_composite_v1(&package) != package.identity.model_parameter_sha256 {
            return Err(identity_error());
        }
        let parent = load_native_structured_policy_successor_inference_v1(&root.join("parent"))
            .map_err(|_| authority_error())?;
        if parent.parent_adam_step_v1() != package.parent.adam_step
            || lower_hex_raw32_v1(parent.candidate_json_sha256_v1())
                != package.parent.candidate_sha256
            || lower_hex_raw32_v1(parent.weights_sha256_v1()) != package.parent.weights_sha256
            || lower_hex_raw32_v1(parent.report_sha256_v1()) != package.parent.report_sha256
            || lower_hex_raw32_v1(parent.composite_model_parameter_sha256_v1())
                != package.parent.composite_model_parameter_sha256
        {
            return Err(identity_error());
        }
        let worker = RecurrentCp7WorkerV1::launch_v1(
            &python_executable,
            &root,
            &package.files.model.sha256,
            &package.model_state_sha256,
            package.deployment_scale as f32,
            package.log_ratio_budget as f32,
            terminal_package,
        )
        .map_err(|_| authority_error())?;
        let manifest_sha256 = lower_hex_raw32_v1(Sha256::digest(&manifest_bytes).into());
        let identity = ShadowCheckpointIdentityV1 {
            authority_kind: package.identity.authority_kind.clone(),
            source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
            source_generation: SOURCE_GENERATION_V1,
            source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
            source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
            source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
            source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
            loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
            loaded_generation: package.parent.adam_step,
            loaded_checkpoint_sha256: manifest_sha256.clone(),
            loaded_payload_sha256: package.files.model.sha256.clone(),
            loaded_train_state_sha256: package.model_state_sha256.clone(),
            model_parameter_sha256: package.identity.model_parameter_sha256.clone(),
            environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
            sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
            sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        };
        eprintln!(
            "RECURRENT_DEPLOYMENT authority_kind={} manifest_sha256={} model_file_sha256={} model_state_sha256={} deployment_scale={} log_ratio_budget={} device=cpu",
            package.identity.authority_kind,
            manifest_sha256,
            package.files.model.sha256,
            package.model_state_sha256,
            package.deployment_scale,
            package.log_ratio_budget,
        );
        Ok(Self {
            model: Box::new(RecurrentCp7ShadowModelScorerV1 {
                parent,
                worker: Mutex::new(worker),
            }),
            opponent_model: None,
            population_opponent: None,
            identity,
            candidate_selector: ShadowCandidateSelectorV1::PolicySample,
            max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
            max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
            active: None,
            teacher_export: None,
            outcome_export: None,
            export_poisoned: false,
        })
    }

    fn load_v1(authority: ShadowCheckpointAuthorityV1) -> Result<Self, ShadowScorerStartupErrorV1> {
        if let ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { root } = &authority {
            let inference = load_cp7_behavior_clone_inference_v1(root).map_err(|_| {
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            })?;
            let identity = ShadowCheckpointIdentityV1 {
                authority_kind: "cp7-behavior-clone-derivative-v1".to_owned(),
                source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                source_generation: SOURCE_GENERATION_V1,
                source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                loaded_generation: inference.adam_step_v1(),
                loaded_checkpoint_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
                loaded_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
                loaded_train_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
                model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
                environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
            };
            return Ok(Self {
                model: Box::new(Cp7BehaviorCloneShadowModelScorerV1 { inference }),
                opponent_model: None,
                population_opponent: None,
                identity,
                candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                active: None,
                teacher_export: None,
                outcome_export: None,
                export_poisoned: false,
            });
        }
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
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
            let bounded_value_search = root
                .join(BOUNDED_VALUE_SEARCH_MANIFEST_FILENAME_V1)
                .try_exists()
                .map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
            if bounded_value_search {
                let inventory = fs::read_dir(root)
                    .map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?
                    .map(|entry| {
                        let entry = entry.map_err(|_| ())?;
                        let name = entry.file_name().into_string().map_err(|_| ())?;
                        let file_type = entry.file_type().map_err(|_| ())?;
                        if file_type.is_symlink() {
                            return Err(());
                        }
                        Ok((name, file_type.is_dir()))
                    })
                    .collect::<Result<std::collections::BTreeMap<_, _>, ()>>()
                    .map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
                if inventory
                    != std::collections::BTreeMap::from([
                        (BOUNDED_VALUE_SEARCH_MANIFEST_FILENAME_V1.to_owned(), false),
                        ("policy".to_owned(), true),
                        ("value".to_owned(), true),
                    ])
                {
                    return Err(ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    ));
                }
                let manifest_bytes = fs::read(root.join(BOUNDED_VALUE_SEARCH_MANIFEST_FILENAME_V1))
                    .map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
                let manifest_sha256: [u8; 32] = Sha256::digest(&manifest_bytes).into();
                if lower_hex_raw32_v1(manifest_sha256) != BOUNDED_VALUE_SEARCH_MANIFEST_SHA256_V1 {
                    return Err(ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointIdentity,
                    ));
                }
                let manifest_text = std::str::from_utf8(&manifest_bytes).map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
                let manifest = parse_strict_json_value(manifest_text).map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
                let policy =
                    load_native_structured_policy_successor_inference_v1(&root.join("policy"))
                        .map_err(|_| {
                            ShadowScorerStartupErrorV1::new(
                                ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                            )
                        })?;
                let value =
                    load_native_structured_policy_residual_inference_v1(&root.join("value"))
                        .map_err(|_| {
                            ShadowScorerStartupErrorV1::new(
                                ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                            )
                        })?;
                let exact_string = |pointer: &str, expected: &str| {
                    manifest
                        .pointer(pointer)
                        .and_then(serde_json::Value::as_str)
                        == Some(expected)
                };
                let exact_u64 = |pointer: &str, expected: u64| {
                    manifest
                        .pointer(pointer)
                        .and_then(serde_json::Value::as_u64)
                        == Some(expected)
                };
                let exact_f64 = |pointer: &str, expected: f64| {
                    manifest
                        .pointer(pointer)
                        .and_then(serde_json::Value::as_f64)
                        .map(f64::to_bits)
                        == Some(expected.to_bits())
                };
                if !exact_string("/schema", BOUNDED_VALUE_SEARCH_SCHEMA_V1)
                    || !exact_string("/selector", "candidate-turn-only-one-step-bounded-value/v1")
                    || !exact_string("/policy/directory", "policy")
                    || !exact_string(
                        "/policy/candidate_json_sha256",
                        BOUNDED_VALUE_SEARCH_POLICY_CANDIDATE_SHA256_V1,
                    )
                    || !exact_string(
                        "/policy/candidate_json_sha256",
                        &lower_hex_raw32_v1(policy.candidate_json_sha256_v1()),
                    )
                    || !exact_string(
                        "/policy/composite_model_parameter_sha256",
                        BOUNDED_VALUE_SEARCH_POLICY_COMPOSITE_SHA256_V1,
                    )
                    || !exact_string(
                        "/policy/composite_model_parameter_sha256",
                        &lower_hex_raw32_v1(policy.composite_model_parameter_sha256_v1()),
                    )
                    || !exact_string("/value/directory", "value")
                    || !exact_string(
                        "/value/candidate_json_sha256",
                        BOUNDED_VALUE_SEARCH_VALUE_CANDIDATE_SHA256_V1,
                    )
                    || !exact_string(
                        "/value/candidate_json_sha256",
                        &lower_hex_raw32_v1(value.candidate_json_sha256_v1()),
                    )
                    || !exact_string(
                        "/value/composite_model_parameter_sha256",
                        BOUNDED_VALUE_SEARCH_VALUE_COMPOSITE_SHA256_V1,
                    )
                    || !exact_string(
                        "/value/composite_model_parameter_sha256",
                        &lower_hex_raw32_v1(value.composite_model_parameter_sha256_v1()),
                    )
                    || !exact_u64("/information_set_samples", 4)
                    || !exact_u64("/minimum_actor_physical_decision", 20)
                    || !exact_u64("/minimum_legal_actions", 2)
                    || !exact_u64("/maximum_legal_actions", 8)
                    || !exact_f64("/override_margin", 0.25)
                    || !exact_string("/opponent_successor", "root-ineligible-retain-fallback")
                {
                    return Err(ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointIdentity,
                    ));
                }
                let mut composite_hasher = Sha256::new();
                composite_hasher.update(BOUNDED_VALUE_SEARCH_COMPOSITE_DOMAIN_V1);
                composite_hasher.update(policy.composite_model_parameter_sha256_v1());
                composite_hasher.update(value.composite_model_parameter_sha256_v1());
                let composite_sha256: [u8; 32] = composite_hasher.finalize().into();
                let identity = ShadowCheckpointIdentityV1 {
                    authority_kind: "qualified-policy-bounded-value-search-v1".to_owned(),
                    source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    source_generation: SOURCE_GENERATION_V1,
                    source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                    source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                    source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                    source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                    loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    loaded_generation: policy.parent_adam_step_v1(),
                    loaded_checkpoint_sha256: lower_hex_raw32_v1(manifest_sha256),
                    loaded_payload_sha256: lower_hex_raw32_v1(composite_sha256),
                    loaded_train_state_sha256: lower_hex_raw32_v1(value.report_sha256_v1()),
                    model_parameter_sha256: lower_hex_raw32_v1(composite_sha256),
                    environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                    sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                    sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                };
                return Ok(Self {
                    model: Box::new(NativeQualifiedPolicyBoundedValueShadowModelScorerV1 {
                        policy,
                        value,
                    }),
                    opponent_model: None,
                    population_opponent: None,
                    identity,
                    candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
            let structured_policy_successor = root
                .join(STRUCTURED_POLICY_SUCCESSOR_CANDIDATE_FILENAME_V1)
                .try_exists()
                .map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
            if structured_policy_successor {
                let inference = load_native_structured_policy_successor_inference_v1(root)
                    .map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
                let identity = ShadowCheckpointIdentityV1 {
                    authority_kind: inference.authority_kind_v1().to_owned(),
                    source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    source_generation: SOURCE_GENERATION_V1,
                    source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                    source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                    source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                    source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                    loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    loaded_generation: inference.parent_adam_step_v1(),
                    loaded_checkpoint_sha256: lower_hex_raw32_v1(
                        inference.candidate_json_sha256_v1(),
                    ),
                    loaded_payload_sha256: lower_hex_raw32_v1(inference.weights_sha256_v1()),
                    loaded_train_state_sha256: lower_hex_raw32_v1(inference.report_sha256_v1()),
                    model_parameter_sha256: lower_hex_raw32_v1(
                        inference.composite_model_parameter_sha256_v1(),
                    ),
                    environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                    sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                    sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                };
                return Ok(Self {
                    model: Box::new(NativeStructuredPolicySuccessorShadowModelScorerV1 {
                        inference,
                    }),
                    opponent_model: None,
                    population_opponent: None,
                    identity,
                    candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
            let structured_history_stack = root
                .join("structured_history_stack.json")
                .try_exists()
                .map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?;
            if structured_history_stack {
                let inference =
                    load_native_structured_history_stack_inference_v1(root).map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
                let identity = ShadowCheckpointIdentityV1 {
                    authority_kind: "xmage-cp7-outcome-structured-history-stack-v1".to_owned(),
                    source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    source_generation: SOURCE_GENERATION_V1,
                    source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                    source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                    source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                    source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                    loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    loaded_generation: inference.parent_adam_step_v1(),
                    loaded_checkpoint_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
                    loaded_payload_sha256: lower_hex_raw32_v1(inference.weights_sha256_v1()),
                    loaded_train_state_sha256: PARENT_NATIVE_STATE_SHA256_V1.to_owned(),
                    model_parameter_sha256: lower_hex_raw32_v1(
                        inference.composite_model_parameter_sha256_v1(),
                    ),
                    environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                    sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                    sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                };
                return Ok(Self {
                    model: Box::new(NativeStructuredHistoryStackShadowModelScorerV1 { inference }),
                    opponent_model: None,
                    population_opponent: None,
                    identity,
                    candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
            let structured_candidate = root
                .join("structured_candidate.json")
                .try_exists()
                .map_err(|_| {
                    ShadowScorerStartupErrorV1::new(
                        ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                    )
                })?
                || root
                    .join("structured_history_candidate.json")
                    .try_exists()
                    .map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
            if structured_candidate {
                let inference =
                    load_native_structured_policy_residual_inference_v1(root).map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
                let identity = ShadowCheckpointIdentityV1 {
                    authority_kind: "xmage-cp7-outcome-reinforce-derivative-v1".to_owned(),
                    source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    source_generation: SOURCE_GENERATION_V1,
                    source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                    source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                    source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                    source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                    loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    loaded_generation: inference.parent_adam_step_v1(),
                    loaded_checkpoint_sha256: lower_hex_raw32_v1(
                        inference.candidate_json_sha256_v1(),
                    ),
                    loaded_payload_sha256: lower_hex_raw32_v1(inference.weights_sha256_v1()),
                    loaded_train_state_sha256: lower_hex_raw32_v1(inference.report_sha256_v1()),
                    model_parameter_sha256: lower_hex_raw32_v1(
                        inference.composite_model_parameter_sha256_v1(),
                    ),
                    environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                    sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                    sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                };
                return Ok(Self {
                    model: Box::new(NativeStructuredPolicyResidualShadowModelScorerV1 {
                        inference,
                    }),
                    opponent_model: None,
                    population_opponent: None,
                    identity,
                    candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
            let rank1_candidate = root.join("candidate.json").try_exists().map_err(|_| {
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            })?;
            if rank1_candidate {
                let inference =
                    load_native_rank1_policy_residual_inference_v1(root).map_err(|_| {
                        ShadowScorerStartupErrorV1::new(
                            ShadowScorerStartupErrorKindV1::CheckpointAuthority,
                        )
                    })?;
                let identity = ShadowCheckpointIdentityV1 {
                    // Reuse the existing Java outcome-root transport. The
                    // four loaded hashes below identify the stricter residual
                    // bundle and are supplied explicitly by the harness.
                    authority_kind: "xmage-cp7-outcome-reinforce-derivative-v1".to_owned(),
                    source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    source_generation: SOURCE_GENERATION_V1,
                    source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                    source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                    source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                    source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                    loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                    loaded_generation: inference.parent_adam_step_v1(),
                    loaded_checkpoint_sha256: lower_hex_raw32_v1(
                        inference.candidate_json_sha256_v1(),
                    ),
                    loaded_payload_sha256: lower_hex_raw32_v1(inference.weights_sha256_v1()),
                    loaded_train_state_sha256: lower_hex_raw32_v1(inference.report_sha256_v1()),
                    model_parameter_sha256: lower_hex_raw32_v1(
                        inference.composite_model_parameter_sha256_v1(),
                    ),
                    environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                    sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                    sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
                };
                return Ok(Self {
                    model: Box::new(NativeRank1PolicyResidualShadowModelScorerV1 { inference }),
                    opponent_model: None,
                    population_opponent: None,
                    identity,
                    candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                    max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                    max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                    active: None,
                    teacher_export: None,
                    outcome_export: None,
                    export_poisoned: false,
                });
            }
            let inference = load_xmage_cp7_outcome_inference_v1(root).map_err(|_| {
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            })?;
            let identity = ShadowCheckpointIdentityV1 {
                authority_kind: "xmage-cp7-outcome-reinforce-derivative-v1".to_owned(),
                source_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                source_generation: SOURCE_GENERATION_V1,
                source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
                source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
                source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
                source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
                loaded_run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
                loaded_generation: inference.adam_step_v1(),
                loaded_checkpoint_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
                loaded_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
                loaded_train_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
                model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
                environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
                sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
                sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
            };
            return Ok(Self {
                model: Box::new(XmageCp7OutcomeShadowModelScorerV1 { inference }),
                opponent_model: None,
                population_opponent: None,
                identity,
                candidate_selector: ShadowCandidateSelectorV1::PolicySample,
                max_physical_decisions: FIXED_MAX_PHYSICAL_DECISIONS_V1,
                max_policy_steps: FIXED_MAX_POLICY_STEPS_V1,
                active: None,
                teacher_export: None,
                outcome_export: None,
                export_poisoned: false,
            });
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
        opponent_model: Option<&dyn ShadowModelScorerV1>,
        session: &FastActorSessionV1,
        scored: &ScoredCurrentDecisionV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
        candidate_seat: PlayerSeatV1,
        fallback_selected_index: u32,
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
        let action_count = usize::try_from(expected.legal_action_count)
            .map_err(|_| "depth8_search_action_count_invalid")?;
        let fallback = usize::try_from(fallback_selected_index)
            .map_err(|_| "depth8_search_fallback_index_invalid")?;
        if fallback >= action_count {
            return Err("depth8_search_fallback_index_invalid");
        }
        let mut value_sums = vec![0.0f64; action_count];
        let mut sampled_hashes = Vec::with_capacity(ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1);
        let mut natural_terminal_branch_count = 0usize;
        let mut critic_bootstrap_branch_count = 0usize;
        for sample_index in 0..ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1 {
            let mut seed_rng = SplitMix64::seed(
                DEPTH8_VALUE_REDETERMINIZATION_DOMAIN_V1
                    ^ expected.episode_id.wrapping_mul(0x9e37_79b1_85eb_ca87)
                    ^ expected
                        .physical_decision_id
                        .wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
                    ^ (sample_index as u64).wrapping_mul(0x1656_67b1_9e37_79f9),
            );
            let redeterminization_seed = seed_rng.next_u64();
            let snapshot = session
                .snapshot_current_actor_information_set_v1(redeterminization_seed)
                .map_err(|_| "depth8_search_redeterminization_failed")?;
            let mut sampled_root = session.clone();
            sampled_root.restore_v1(&snapshot);
            sampled_hashes.push(u64_hex_v1(sampled_root.privileged_core_environment_hash()));
            let (sampled_expected, sampled_output) =
                Self::score_current_model_output_v1(model, &sampled_root, structured_history)?;
            if sampled_expected != expected
                || sampled_output.value.to_bits() != scored.value_f32_bits
                || sampled_output
                    .logits
                    .iter()
                    .map(|value| value.to_bits())
                    .ne(scored.logits_f32_bits.iter().copied())
            {
                return Err("depth8_search_redeterminization_observation_drift");
            }
            for (action_index, value_sum) in value_sums.iter_mut().enumerate() {
                let selected = u32::try_from(action_index)
                    .map_err(|_| "depth8_search_action_index_invalid")?;
                let branch_value = Self::depth8_branch_value_v1(
                    model,
                    opponent_model,
                    &sampled_root,
                    scored,
                    structured_history,
                    candidate_seat,
                    selected,
                    sample_index,
                )?;
                if !branch_value.value.is_finite() {
                    return Err("depth8_search_nonfinite_successor_value");
                }
                natural_terminal_branch_count += usize::from(branch_value.reached_natural_terminal);
                critic_bootstrap_branch_count +=
                    usize::from(!branch_value.reached_natural_terminal);
                *value_sum += f64::from(branch_value.value);
            }
        }
        let values = value_sums
            .into_iter()
            .map(|sum| (sum / ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1 as f64) as f32)
            .collect::<Vec<_>>();
        let fallback_value = values[fallback];
        let mut best = fallback;
        let mut best_value = fallback_value;
        for (index, value) in values.iter().copied().enumerate() {
            if value > best_value {
                best = index;
                best_value = value;
            }
        }
        let margin = best_value - fallback_value;
        let selected = if best != fallback && margin >= ONE_STEP_VALUE_OVERRIDE_MARGIN_V1 {
            best
        } else {
            fallback
        };
        let value_bits = values
            .iter()
            .map(|value| format!("{:08x}", value.to_bits()))
            .collect::<Vec<_>>()
            .join(",");
        let (diagnostic, opponent_policy) = if opponent_model.is_some() {
            (
                "NATIVE_DEPTH8_CP7_OPPONENT_HISTORY_VALUE",
                "cp7_behavior_clone_sample",
            )
        } else {
            ("NATIVE_DEPTH8_HISTORY_VALUE", "candidate_argmax")
        };
        eprintln!(
            "{} episode={} step={} physical_decision={} actions={} continuation_steps={} opponent_policy={} information_set_samples={} sampled_hashes={} fallback={} best={} selected={} margin_bits={:08x} values_f32_bits={} override={}",
            diagnostic,
            expected.episode_id,
            expected.step,
            expected.physical_decision_id,
            action_count,
            DEPTH8_VALUE_CONTINUATION_STEPS_V1,
            opponent_policy,
            ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1,
            sampled_hashes.join(","),
            fallback,
            best,
            selected,
            margin.to_bits(),
            value_bits,
            selected != fallback,
        );
        let teacher_selected_index =
            u32::try_from(selected).map_err(|_| "depth8_search_selected_index_invalid")?;
        Ok(Some(Depth8TeacherDiagnosticV1 {
            schema: "mtg-kernel-depth8-bounded-value-search-teacher/v1",
            pair_index: expected.episode_id / 2,
            episode_id: expected.episode_id,
            step: expected.step,
            environment_revision: expected.environment_revision,
            physical_decision_id: expected.physical_decision_id,
            substep_index: expected.substep_index,
            substep_count: expected.substep_count,
            actor_physical_decision_ordinal: scored.actor_physical_decision_ordinal,
            candidate_seat,
            legal_action_count: expected.legal_action_count,
            continuation_steps: DEPTH8_VALUE_CONTINUATION_STEPS_V1,
            information_set_samples: ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1,
            information_set_sample_hashes_u64_hex: sampled_hashes,
            fallback_selected_index,
            best_search_index: u32::try_from(best)
                .map_err(|_| "depth8_search_selected_index_invalid")?,
            teacher_selected_index,
            teacher_differs_from_fallback: selected != fallback,
            search_margin_f32_bits_hex: format!("{:08x}", margin.to_bits()),
            action_values_f32_bits_hex: values
                .iter()
                .map(|value| format!("{:08x}", value.to_bits()))
                .collect(),
            branch_count: action_count * ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1,
            natural_terminal_branch_count,
            critic_bootstrap_branch_count,
            candidate_action_seed_u64_hex: scored.candidate_action_seed_u64_hex.clone(),
            candidate_order_commitment_128_hex: lower_hex_bytes_v1(
                &scored.binding.action_binding.candidate_order_commitment,
            ),
            model_input_sha256: scored.model_input_sha256.clone(),
            public_history_sha256: public_history_sha256_v1(
                structured_history,
                player_seat_index_v1(candidate_seat),
            )
            .map_err(|_| "depth8_search_public_history_hash_failed")?,
            diagnostic_state_hash_u64_hex: scored.diagnostic_state_hash_u64_hex.clone(),
            core_environment_hash_u64_hex: scored.core_environment_hash_u64_hex.clone(),
            action_semantics: scored.action_semantics.clone(),
        }))
    }

    fn one_step_history_value_selection_v1(
        model: &dyn ShadowModelScorerV1,
        session: &FastActorSessionV1,
        scored: &ScoredCurrentDecisionV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
        candidate_seat: PlayerSeatV1,
        fallback_selected_index: u32,
        candidate_turn_only: bool,
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
        let action_count = usize::try_from(expected.legal_action_count)
            .map_err(|_| "value_search_action_count_invalid")?;
        let fallback = usize::try_from(fallback_selected_index)
            .map_err(|_| "value_search_fallback_index_invalid")?;
        if fallback >= action_count {
            return Err("value_search_fallback_index_invalid");
        }
        let candidate_index = usize::from(player_seat_index_v1(candidate_seat));
        let mut value_sums = vec![0.0f64; action_count];
        let mut sampled_hashes = Vec::with_capacity(ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1);
        let mut candidate_successors = 0usize;
        let mut opponent_successors = 0usize;
        let mut terminal_successors = 0usize;
        for sample_index in 0..ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1 {
            let mut seed_rng = SplitMix64::seed(
                ONE_STEP_VALUE_REDETERMINIZATION_DOMAIN_V1
                    ^ expected.episode_id.wrapping_mul(0x9e37_79b1_85eb_ca87)
                    ^ expected
                        .physical_decision_id
                        .wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
                    ^ (sample_index as u64).wrapping_mul(0x1656_67b1_9e37_79f9),
            );
            let redeterminization_seed = seed_rng.next_u64();
            let snapshot = session
                .snapshot_current_actor_information_set_v1(redeterminization_seed)
                .map_err(|_| "value_search_redeterminization_failed")?;
            let mut sampled_root = session.clone();
            sampled_root.restore_v1(&snapshot);
            sampled_hashes.push(u64_hex_v1(sampled_root.privileged_core_environment_hash()));
            let (sampled_expected, sampled_output) =
                Self::score_current_model_output_v1(model, &sampled_root, structured_history)?;
            if sampled_expected != expected
                || sampled_output.value.to_bits() != scored.value_f32_bits
                || sampled_output
                    .logits
                    .iter()
                    .map(|value| value.to_bits())
                    .ne(scored.logits_f32_bits.iter().copied())
            {
                return Err("value_search_redeterminization_observation_drift");
            }
            for (action_index, value_sum) in value_sums.iter_mut().enumerate() {
                let selected =
                    u32::try_from(action_index).map_err(|_| "value_search_action_index_invalid")?;
                let mut branch = sampled_root.clone();
                let mut branch_history = StructuredHistoryStateV1 {
                    completed: structured_history.to_vec(),
                    pending: None,
                };
                branch_history
                    .accept_selected_v1(scored, selected)
                    .map_err(|_| "value_search_history_update_failed")?;
                let next = branch
                    .consume_current_flat_action_slice_v2(scored.binding.action_binding, selected)
                    .map_err(|_| "value_search_branch_consume_failed")?;
                let value = match next {
                    FastActorResponseV1::Terminal(terminal) => {
                        terminal_successors += 1;
                        terminal.terminal_reward[candidate_index] as f32
                    }
                    FastActorResponseV1::Decision(next_expected) => {
                        if candidate_turn_only && next_expected.acting_player != candidate_seat {
                            opponent_successors += 1;
                            continue;
                        }
                        let (observed_expected, output) = Self::score_current_model_output_v1(
                            model,
                            &branch,
                            &branch_history.completed,
                        )?;
                        if observed_expected != next_expected {
                            return Err("value_search_successor_binding_mismatch");
                        }
                        if next_expected.acting_player == candidate_seat {
                            candidate_successors += 1;
                            output.value
                        } else {
                            opponent_successors += 1;
                            -output.value
                        }
                    }
                };
                if !value.is_finite() {
                    return Err("value_search_nonfinite_successor_value");
                }
                *value_sum += f64::from(value);
            }
        }
        if candidate_turn_only && opponent_successors > 0 {
            eprintln!(
                "NATIVE_BOUNDED_CANDIDATE_TURN_VALUE episode={} step={} physical_decision={} actions={} information_set_samples={} sampled_hashes={} eligible=false candidate_successors={} opponent_successors={} terminal_successors={} fallback={} best={} selected={} margin_bits={:08x} values_f32_bits=none override=false",
                expected.episode_id,
                expected.step,
                expected.physical_decision_id,
                action_count,
                ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1,
                sampled_hashes.join(","),
                candidate_successors,
                opponent_successors,
                terminal_successors,
                fallback,
                fallback,
                fallback,
                0.0f32.to_bits(),
            );
            return Ok(None);
        }
        let values = value_sums
            .into_iter()
            .map(|sum| (sum / ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1 as f64) as f32)
            .collect::<Vec<_>>();
        let fallback_value = values[fallback];
        let mut best = fallback;
        let mut best_value = fallback_value;
        for (index, value) in values.iter().copied().enumerate() {
            if value > best_value {
                best = index;
                best_value = value;
            }
        }
        let margin = best_value - fallback_value;
        let selected = if best != fallback && margin >= ONE_STEP_VALUE_OVERRIDE_MARGIN_V1 {
            best
        } else {
            fallback
        };
        let value_bits = values
            .iter()
            .map(|value| format!("{:08x}", value.to_bits()))
            .collect::<Vec<_>>()
            .join(",");
        let diagnostic_prefix = if candidate_turn_only {
            "NATIVE_BOUNDED_CANDIDATE_TURN_VALUE"
        } else {
            "NATIVE_ONE_STEP_HISTORY_VALUE"
        };
        eprintln!(
            "{} episode={} step={} physical_decision={} actions={} information_set_samples={} sampled_hashes={} eligible=true candidate_successors={} opponent_successors={} terminal_successors={} fallback={} best={} selected={} margin_bits={:08x} values_f32_bits={} override={}",
            diagnostic_prefix,
            expected.episode_id,
            expected.step,
            expected.physical_decision_id,
            action_count,
            ONE_STEP_VALUE_INFORMATION_SET_SAMPLES_V1,
            sampled_hashes.join(","),
            candidate_successors,
            opponent_successors,
            terminal_successors,
            fallback,
            best,
            selected,
            margin.to_bits(),
            value_bits,
            selected != fallback,
        );
        Ok(Some(
            u32::try_from(selected).map_err(|_| "value_search_selected_index_invalid")?,
        ))
    }

    fn score_session_v1(
        model: &dyn ShadowModelScorerV1,
        opponent_model: Option<&dyn ShadowModelScorerV1>,
        population_opponent: Option<&LadderOpponentEngineV1>,
        population_opponent_member: Option<OpponentLadderPoolMemberV2>,
        candidate_selector: ShadowCandidateSelectorV1,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        base_seed: u64,
        session: &FastActorSessionV1,
        schedule: &mut NativeLaneScheduleStateV1,
        candidate_seat: PlayerSeatV1,
        structured_history: &[NativeStructuredHistoryEntryV1],
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
            )
        {
            let fallback = scored
                .selected_action_index
                .ok_or("value_search_fallback_selection_missing")?;
            let selected = match candidate_selector {
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
            self.max_physical_decisions,
            self.max_policy_steps,
            base_seed,
            &session,
            &mut schedule,
            candidate_seat,
            &structured_history.completed,
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
                        .map_err(|()| ())
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
        let body = match active.session.current_response() {
            FastActorResponseV1::Decision(_) => match Self::decision_body_v1(&active, true) {
                Ok(decision) => ShadowScorerResponseBodyV1::Decision {
                    decision,
                    applied_action: None,
                },
                Err(()) => error_body_v1("internal_protocol_error", "decision cache missing"),
            },
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
                let body = match Self::decision_body_v1(active, false) {
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
    ) -> ShadowScorerResponseV1 {
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
                self.max_physical_decisions,
                self.max_policy_steps,
                active.base_seed,
                &active.session,
                &mut active.schedule,
                active.candidate_seat,
                &structured_history_after.completed,
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
        let body = match next {
            FastActorResponseV1::Decision(_) => match Self::decision_body_v1(active, false) {
                Ok(decision) => ShadowScorerResponseBodyV1::Decision {
                    decision,
                    applied_action: Some(applied),
                },
                Err(()) => error_body_v1("internal_protocol_error", "decision cache missing"),
            },
            FastActorResponseV1::Terminal(terminal) => ShadowScorerResponseBodyV1::Terminal {
                terminal: Self::terminal_body_v1(active, terminal, false),
                applied_action: Some(applied),
            },
        };
        response_v1(Some(request_id), &self.identity, body)
    }

    fn handle_line_v1(&mut self, line: &str) -> String {
        if self.export_poisoned {
            let request_id = parse_strict_json_value(line)
                .ok()
                .and_then(|value| request_id_from_value_v1(&value));
            return serialize_response_v1(&response_v1(
                request_id,
                &self.identity,
                error_body_v1(
                    "export_poisoned",
                    "a prior export write failed and this scorer cannot continue",
                ),
            ));
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
                    return serialize_response_v1(&response_v1(
                        None,
                        &self.identity,
                        error_body_v1(code, "request line is not valid strict JSON"),
                    ));
                }
            };
            let recoverable_request_id = request_id_from_value_v1(&value);
            match serde_json::from_value::<ShadowScorerRequestV1>(value) {
                Ok(request) if valid_request_id_v1(request.request_id()) => match request {
                    ShadowScorerRequestV1::Reset {
                        request_id,
                        episode_id,
                        base_seed,
                    } => self.handle_reset_v1(request_id, episode_id, base_seed),
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
                    } => self.handle_step_v1(request_id, episode_id, expected_step, selected_index),
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
        serialize_response_v1(&response)
    }
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

/// Opt-in deployment bridge for the calibrated width-128 recurrent CP7 policy.
/// The package owns the exact parent authority, model, inference worker, and
/// identity hashes. Inference is CPU-only and terminal outcomes remain outside
/// this transport boundary.
pub fn run_checkpoint_shadow_stdio_with_recurrent_cp7_v1(
    root: PathBuf,
    python_executable: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_v1(root, python_executable, None, None)
}

/// Opt-in on-policy terminal export for the calibrated recurrent CP7 policy.
/// The exported reward is still only the natural terminal result.
pub fn run_checkpoint_shadow_stdio_with_recurrent_cp7_outcome_jsonl_v1(
    root: PathBuf,
    python_executable: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_v1(
        root,
        python_executable,
        None,
        Some(outcome_jsonl),
    )
}

/// Opt-in paired public-action and terminal export for on-policy recurrent data.
pub fn run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_jsonl_v1(
    root: PathBuf,
    python_executable: PathBuf,
    teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_v1(
        root,
        python_executable,
        Some(teacher_jsonl),
        Some(outcome_jsonl),
    )
}

fn run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_v1(
    root: PathBuf,
    python_executable: PathBuf,
    teacher_jsonl: Option<PathBuf>,
    outcome_jsonl: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let mut service = ShadowScorerServiceV1::load_recurrent_cp7_v1(root, python_executable)?;
    if let Some(path) = teacher_jsonl {
        let export = XmageCp7TeacherJsonlWriterV1::create_v1(&path, &service.identity)?;
        service.install_teacher_export_v1(export).map_err(|()| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "recurrent CP7 teacher export must be installed before reset",
            )
        })?;
    }
    if let Some(path) = outcome_jsonl {
        let export = XmageCp7OutcomeJsonlWriterV1::create_v1(&path, &service.identity)?;
        service.install_outcome_export_v1(export).map_err(|()| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "recurrent CP7 outcome export must be installed before reset",
            )
        })?;
    }
    run_jsonl_v1(&mut service, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
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

/// Experimental depth-8 selector whose opponent branches sample from one
/// verified CP7 behavior-clone package. The candidate model still supplies
/// candidate actions and every horizon value.
pub fn run_checkpoint_shadow_stdio_with_cp7_opponent_selector_v1(
    authority: ShadowCheckpointAuthorityV1,
    cp7_opponent_root: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
    if !service.model.uses_structured_history_v1() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "CP7-opponent history-value selection requires a structured-history candidate",
        )
        .into());
    }
    let inference = load_cp7_behavior_clone_inference_v1(&cp7_opponent_root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "CP7 opponent behavior-clone authority failed verification",
        )
    })?;
    eprintln!(
        "NATIVE_CP7_OPPONENT_MODEL manifest_sha256={} payload_sha256={} native_state_sha256={} model_parameter_sha256={} adam_step={}",
        lower_hex_raw32_v1(inference.manifest_sha256_v1()),
        lower_hex_raw32_v1(inference.payload_sha256_v1()),
        lower_hex_raw32_v1(inference.native_state_sha256_v1()),
        lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
        inference.adam_step_v1(),
    );
    service.opponent_model = Some(Box::new(Cp7BehaviorCloneShadowModelScorerV1 { inference }));
    service.candidate_selector = ShadowCandidateSelectorV1::Depth8Cp7OpponentHistoryValueBootstrap;
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
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
    install_native_population_exports_v1(&mut service, pool_root, teacher_jsonl, outcome_jsonl)?;
    run_jsonl_v1(&mut service, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}

/// Opt-in native Pool3 corpus generation for a verified recurrent package.
/// Python performs CPU inference only. The native engine owns trajectories,
/// the frozen opponent mixture, and natural terminal adjudication.
pub fn run_checkpoint_shadow_stdio_with_recurrent_native_population_exports_jsonl_v1(
    root: PathBuf,
    python_executable: PathBuf,
    pool_root: PathBuf,
    teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let mut service = ShadowScorerServiceV1::load_recurrent_cp7_v1(root, python_executable)?;
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

fn run_checkpoint_shadow_stdio_configured_v1(
    authority: ShadowCheckpointAuthorityV1,
    selector: ShadowCandidateSelectorV1,
    teacher_jsonl: Option<PathBuf>,
    outcome_jsonl: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
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
        writeln!(writer, "{}", service.handle_line_v1(&line))?;
        writer.flush()?;
        if service.export_poisoned {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "checkpoint shadow export is poisoned after a write failure",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "shared writer poisoned"))?
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
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "flush control poisoned"))?
            {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "injected export flush failure",
                ))
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

    #[test]
    fn depth8_teacher_shadow_evaluates_search_without_changing_live_actions_v1() {
        let baseline_calls = Arc::new(Mutex::new(0usize));
        let shadow_calls = Arc::new(Mutex::new(0usize));
        let mut baseline = ShadowScorerServiceV1::with_test_model_v1(Box::new(
            CountingStructuredDeterministicTestModelV1 {
                calls: Arc::clone(&baseline_calls),
            },
        ));
        let mut shadow = ShadowScorerServiceV1::with_test_model_v1(Box::new(
            CountingStructuredDeterministicTestModelV1 {
                calls: Arc::clone(&shadow_calls),
            },
        ));
        shadow.candidate_selector = ShadowCandidateSelectorV1::Depth8BoundedValueTeacherShadow;

        let mut baseline_response =
            value_v1(&baseline.handle_line_v1(&reset_line_v1("teacher-shadow-reset")));
        let mut shadow_response =
            value_v1(&shadow.handle_line_v1(&reset_line_v1("teacher-shadow-reset")));
        assert_eq!(
            decision_projection_v1(&baseline_response),
            decision_projection_v1(&shadow_response)
        );
        let mut search_was_evaluated = *shadow_calls.lock().unwrap() > 1;

        for ordinal in 0..512_u64 {
            if baseline_response["response_type"] == "terminal" {
                break;
            }
            assert_eq!(baseline_response["response_type"], "decision");
            assert_eq!(shadow_response["response_type"], "decision");
            assert_eq!(
                decision_projection_v1(&baseline_response),
                decision_projection_v1(&shadow_response)
            );

            let step = baseline_response["decision"]["step"]
                .as_u64()
                .expect("decision step");
            let selected = if baseline_response["decision"]["candidate_controls_current_actor"]
                .as_bool()
                .expect("candidate ownership")
            {
                baseline_response["decision"]["selected_action_index"]
                    .as_u64()
                    .expect("candidate selection")
            } else {
                0
            };
            let request = format!(
                "{{\"request_type\":\"step\",\"request_id\":\"teacher-shadow-{ordinal}\",\"episode_id\":2,\"expected_step\":{step},\"selected_index\":{selected}}}"
            );
            let calls_before = *shadow_calls.lock().unwrap();
            baseline_response = value_v1(&baseline.handle_line_v1(&request));
            shadow_response = value_v1(&shadow.handle_line_v1(&request));
            let calls_after = *shadow_calls.lock().unwrap();
            let search_this_decision = calls_after.saturating_sub(calls_before) > 1;
            search_was_evaluated |= search_this_decision;
            if search_this_decision {
                break;
            }
        }

        assert!(search_was_evaluated);
        assert_eq!(baseline_response, shadow_response);
    }

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
    #[ignore = "reads the external CP7 behavior-clone derivative"]
    fn real_cp7_behavior_clone_derivative_loads_scores_and_reports_identity_v1() {
        let root = std::env::var_os("MTG_KERNEL_CP7_BEHAVIOR_CLONE_ROOT_V1")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    PathBuf::from(r"D:\mtg-kernel-cp7-bc-train-base970001-grid-strict-v1")
                } else {
                    PathBuf::from("/mnt/d/mtg-kernel-cp7-bc-train-base970001-grid-strict-v1")
                }
            });
        let mut service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { root },
        )
        .unwrap();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("cp7-bc")));
        assert_eq!(response["response_type"], "decision");
        assert_eq!(
            response["checkpoint"]["authority_kind"],
            "cp7-behavior-clone-derivative-v1"
        );
        assert_eq!(response["checkpoint"]["loaded_generation"], 141);
        assert_eq!(
            response["checkpoint"]["loaded_checkpoint_sha256"],
            "6ba733fead0d36c26cd24630245fa6f2a1216ae60c73f46d45e83b4cc714676c"
        );
        assert_eq!(
            response["checkpoint"]["loaded_payload_sha256"],
            "de1132f6b8b55975154133b91a2f2ea90bc1159676a041057fd827e728eca4e1"
        );
        assert_eq!(
            response["checkpoint"]["loaded_train_state_sha256"],
            "64df1692fae7f78d0d4d4a4d6489325d253125276ca578c94912c9bd12374b56"
        );
        assert_eq!(
            response["checkpoint"]["model_parameter_sha256"],
            "3f4da9d761771cf0d7cfe2da19b52dd93dd0bc59466d92318cc11fc850d8c4dc"
        );
    }

    #[test]
    #[ignore = "reads the external XMage CP7 outcome derivative"]
    fn real_xmage_cp7_outcome_derivative_loads_scores_and_reports_identity_v1() {
        let root = std::env::var_os("MTG_KERNEL_XMAGE_CP7_OUTCOME_ROOT_V1")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    PathBuf::from(r"D:\mtg-kernel-xmage-cp7-outcome-base1010001-lr1e-4-vc0p5-v1")
                } else {
                    PathBuf::from("/mnt/d/mtg-kernel-xmage-cp7-outcome-base1010001-lr1e-4-vc0p5-v1")
                }
            });
        let mut service = ShadowScorerServiceV1::load_v1(
            ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root },
        )
        .unwrap();
        let response = value_v1(&service.handle_line_v1(&reset_line_v1("xmage-cp7-outcome")));
        assert_eq!(response["response_type"], "decision");
        assert_eq!(
            response["checkpoint"]["authority_kind"],
            "xmage-cp7-outcome-reinforce-derivative-v1"
        );
        assert_eq!(response["checkpoint"]["loaded_generation"], 1);
        assert_eq!(
            response["checkpoint"]["loaded_checkpoint_sha256"],
            "b02c16c403fa09bd435b46b3763a819635644dc61a02330ccd12861ebb5244e0"
        );
        assert_eq!(
            response["checkpoint"]["loaded_payload_sha256"],
            "a61084a0e505a4aecdf84123dff6dfc8d1ba2296eb54c71f4b3fedb5f25c9b7b"
        );
        assert_eq!(
            response["checkpoint"]["loaded_train_state_sha256"],
            "06a8bdc8f3a3173d9ff0aaf2e1bb42c2d44571b3093be08b3419c68d0c627170"
        );
        assert_eq!(
            response["checkpoint"]["model_parameter_sha256"],
            "aeeb6f6e51131e983743814f59494b799e43898c5e06da339f4d6649e72f5b74"
        );
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
