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
use crate::native_checkpoint_inference_v1::{
    NativeCheckpointInferenceOutputV1, NativeCheckpointInferenceV1,
};
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2,
    NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2, NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
};
use crate::native_ladder_pool_resolution_v1::{
    resolve_ladder_checkpoint_authority_v1, stage_ladder_checkpoint_ref_v1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::native_training_store_run_v2::{
    NativeRunEnvironmentTrajectoryContractV1, OpponentLadderCheckpointRefV1, ValidatedTrainRunV2,
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
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};

pub const CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1: &str = "mtg-kernel-checkpoint-shadow-stdio/v1";
pub const CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1: u32 = 1;
pub const CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1: &str =
    "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1";
pub const CHECKPOINT_SHADOW_MAX_REQUEST_BYTES_V1: usize = 1_048_576;
pub const XMAGE_CP7_TEACHER_JSONL_CONTRACT_V1: &str = "mtg-kernel-xmage-cp7-teacher-jsonl/v1";
pub const XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1: u32 = 1;

const XMAGE_CP7_TEACHER_SELECTION_SOURCE_V1: &str = "xmage_rally_cp7_mapper";

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
    /// A generation-0, weights-only derivative whose model parameters are
    /// bit-identical to promoted(2) g384. This is portable, but is never
    /// reported as the original checkpoint payload or manifest.
    PortablePromoted2WeightsGenesis { root: PathBuf },
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
        ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => Ok(fixed(
            PORTABLE_RUN_SHA256_V1,
            PORTABLE_GENERATION_V1,
            PORTABLE_CHECKPOINT_SHA256_V1,
            PORTABLE_SIDECAR_SHA256_V1,
            PORTABLE_PAYLOAD_SHA256_V1,
        )),
    }
}

fn authority_root_v1(authority: &ShadowCheckpointAuthorityV1) -> &Path {
    match authority {
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { root }
        | ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { root, .. }
        | ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { root } => root,
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
    let checkpoint_ref = checkpoint_ref_v1(&requested)?;
    let authority =
        resolve_ladder_checkpoint_authority_v1(authority_root_v1(&requested), &checkpoint_ref)
            .map_err(|_error| {
                #[cfg(test)]
                eprintln!("shadow checkpoint authority resolution failed: {_error:?}");
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            })?;
    validate_run_limits_v1(authority.run())?;
    if authority.run().environment_trajectory_contract_v1()
        != NativeRunEnvironmentTrajectoryContractV1::LegacyV1
    {
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
        ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => {
            require_portable_source_binding_v1(authority.run())?;
            (
                "portable-promoted2-weights-generation0",
                Some(PORTABLE_TRAIN_STATE_SHA256_V1),
                Some(SOURCE_MODEL_PARAMETER_SHA256_V1),
            )
        }
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
        | ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { .. } => (
            checkpoint_ref.source_run_sha256.clone(),
            checkpoint_ref.generation,
            checkpoint_ref.checkpoint_sha256.clone(),
            checkpoint_ref.sidecar_sha256.clone(),
            checkpoint_ref.state_sha256.clone(),
            expected_train_state.clone(),
        ),
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
        environment_trajectory_contract: SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1,
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
}

trait ShadowModelScorerV1 {
    fn score_v1(&self, decision: FlatScoringDecisionViewV2<'_>) -> Result<ShadowModelOutputV1, ()>;
}

struct NativeShadowModelScorerV1 {
    inference: NativeCheckpointInferenceV1,
}

impl ShadowModelScorerV1 for NativeShadowModelScorerV1 {
    fn score_v1(&self, decision: FlatScoringDecisionViewV2<'_>) -> Result<ShadowModelOutputV1, ()> {
        let output: NativeCheckpointInferenceOutputV1 =
            self.inference.score_decision_v1(decision).map_err(|_| ())?;
        Ok(ShadowModelOutputV1 {
            logits: output.action_logits().to_vec(),
            value: output.value(),
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
    model_input_sha256: String,
    diagnostic_state_hash_u64_hex: String,
    core_environment_hash_u64_hex: String,
    actor_physical_decision_ordinal: u64,
    candidate_action_seed_u64_hex: Option<String>,
    selected_action_index: Option<u32>,
}

struct ActiveShadowSessionV1 {
    session: FastActorSessionV1,
    schedule: NativeLaneScheduleStateV1,
    candidate_seat: PlayerSeatV1,
    deck_ids: SessionDeckIdsV1,
    base_seed: u64,
    pair_index: u64,
    pair_environment_seed: u64,
    initial_library_card_definition_ids: [Vec<u16>; 2],
    current: Option<ScoredCurrentDecisionV1>,
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
    next_record_ordinal: u64,
    next_teacher_decision_ordinal: u64,
}

impl XmageCp7TeacherJsonlWriterV1 {
    fn create_v1(
        path: &Path,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> Result<Self, Box<dyn Error>> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Self::from_writer_v1(Box::new(BufWriter::new(file)), checkpoint)
            .map_err(|error| Box::new(error) as Box<dyn Error>)
    }

    fn from_writer_v1(
        writer: Box<dyn Write>,
        checkpoint: &ShadowCheckpointIdentityV1,
    ) -> io::Result<Self> {
        let mut export = Self {
            writer,
            next_record_ordinal: 0,
            next_teacher_decision_ordinal: 0,
        };
        export.write_v1(&XmageCp7TeacherRecordV1::Header {
            schema_version: XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1,
            record_ordinal: 0,
            export_contract: XMAGE_CP7_TEACHER_JSONL_CONTRACT_V1,
            selection_source: XMAGE_CP7_TEACHER_SELECTION_SOURCE_V1,
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
            schema_version: XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1,
            record_ordinal: self.next_record_ordinal,
            teacher_decision_ordinal: self.next_teacher_decision_ordinal,
            selection_source: XMAGE_CP7_TEACHER_SELECTION_SOURCE_V1,
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
            schema_version: XMAGE_CP7_TEACHER_JSONL_SCHEMA_VERSION_V1,
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
    identity: ShadowCheckpointIdentityV1,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    active: Option<ActiveShadowSessionV1>,
    teacher_export: Option<XmageCp7TeacherJsonlWriterV1>,
}

impl ShadowScorerServiceV1 {
    fn load_v1(authority: ShadowCheckpointAuthorityV1) -> Result<Self, ShadowScorerStartupErrorV1> {
        let loaded = load_checkpoint_v1(authority)?;
        Ok(Self {
            model: Box::new(NativeShadowModelScorerV1 {
                inference: loaded.inference,
            }),
            identity: loaded.identity,
            max_physical_decisions: loaded.max_physical_decisions,
            max_policy_steps: loaded.max_policy_steps,
            active: None,
            teacher_export: None,
        })
    }

    #[cfg(test)]
    fn with_test_model_v1(model: Box<dyn ShadowModelScorerV1>) -> Self {
        Self {
            model,
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
            max_physical_decisions: 128,
            max_policy_steps: 16_384,
            active: None,
            teacher_export: None,
        }
    }

    fn install_teacher_export_v1(
        &mut self,
        writer: XmageCp7TeacherJsonlWriterV1,
    ) -> Result<(), ()> {
        if self.active.is_some() || self.teacher_export.is_some() {
            return Err(());
        }
        self.teacher_export = Some(writer);
        Ok(())
    }

    fn score_session_v1(
        model: &dyn ShadowModelScorerV1,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        session: &FastActorSessionV1,
        schedule: &mut NativeLaneScheduleStateV1,
        candidate_seat: PlayerSeatV1,
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
            .score_v1(view)
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
            } else {
                (None, None)
            };
        let action_semantics = session
            .diagnostic_current_action_semantics()
            .ok_or("action_semantics_missing")?;
        if action_semantics.len() != output.logits.len() {
            return Err("action_semantics_width_mismatch");
        }
        let scored = ScoredCurrentDecisionV1 {
            expected,
            binding,
            tensor,
            action_semantics,
            logits_f32_bits: output.logits.iter().map(|value| value.to_bits()).collect(),
            value_f32_bits: output.value.to_bits(),
            model_input_sha256,
            diagnostic_state_hash_u64_hex: u64_hex_v1(session.diagnostic_state_hash()),
            core_environment_hash_u64_hex: u64_hex_v1(session.privileged_core_environment_hash()),
            actor_physical_decision_ordinal: preflight.actor_physical_decision_ordinal,
            candidate_action_seed_u64_hex,
            selected_action_index,
        };
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
                )
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
                )
            }
        };
        let mut schedule =
            NativeLaneScheduleStateV1::new(base_seed, episode_id, candidate_seat, None);
        let current = match Self::score_session_v1(
            self.model.as_ref(),
            self.max_physical_decisions,
            self.max_policy_steps,
            &session,
            &mut schedule,
            candidate_seat,
        ) {
            Ok(current) => current,
            Err(code) => {
                return response_v1(
                    Some(request_id),
                    &self.identity,
                    error_body_v1(code, "initial decision scoring failed"),
                )
            }
        };
        let active = ActiveShadowSessionV1 {
            session,
            schedule,
            candidate_seat,
            deck_ids,
            base_seed,
            pair_index: episode_schedule.pair_index,
            pair_environment_seed: episode_schedule.environment_seed,
            initial_library_card_definition_ids,
            current,
        };
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
                )
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
                        )
                    }
                }
            }
            _ => None,
        };
        let session_before = active.session.snapshot_v1();
        let schedule_before = active.schedule;
        let expected = scored.expected;
        let binding = scored.binding;
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
                self.max_physical_decisions,
                self.max_policy_steps,
                &active.session,
                &mut active.schedule,
                active.candidate_seat,
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
                active.session.restore_v1(&session_before);
                active.schedule = schedule_before;
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
        active.current = next_scored;
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
    run_checkpoint_shadow_stdio_configured_v1(authority, None)
}

/// Opt-in XMage CP7 teacher export. The destination is created exclusively;
/// callers must promote only the file from a fully successful anchor run.
pub fn run_checkpoint_shadow_stdio_with_xmage_cp7_teacher_jsonl_v1(
    authority: ShadowCheckpointAuthorityV1,
    teacher_jsonl: PathBuf,
) -> Result<(), Box<dyn Error>> {
    run_checkpoint_shadow_stdio_configured_v1(authority, Some(teacher_jsonl))
}

fn run_checkpoint_shadow_stdio_configured_v1(
    authority: ShadowCheckpointAuthorityV1,
    teacher_jsonl: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
    if let Some(path) = teacher_jsonl {
        let export = XmageCp7TeacherJsonlWriterV1::create_v1(&path, &service.identity)?;
        service.install_teacher_export_v1(export).map_err(|()| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "CP7 teacher export must be installed before reset",
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

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

    struct DeterministicTestModelV1;

    impl ShadowModelScorerV1 for DeterministicTestModelV1 {
        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
        ) -> Result<ShadowModelOutputV1, ()> {
            Ok(ShadowModelOutputV1 {
                logits: (0..decision.actions().len())
                    .map(|index| index as f32 * 0.125 - 0.5)
                    .collect(),
                value: 0.25,
            })
        }
    }

    struct FirstActionTestModelV1;

    impl ShadowModelScorerV1 for FirstActionTestModelV1 {
        fn score_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
        ) -> Result<ShadowModelOutputV1, ()> {
            Ok(ShadowModelOutputV1 {
                logits: (0..decision.actions().len())
                    .map(|index| if index == 0 { 0.0 } else { -1_000.0 })
                    .collect(),
                value: 0.0,
            })
        }
    }

    fn service_v1() -> ShadowScorerServiceV1 {
        ShadowScorerServiceV1::with_test_model_v1(Box::new(DeterministicTestModelV1))
    }

    fn service_with_teacher_export_v1(
        model: Box<dyn ShadowModelScorerV1>,
    ) -> (ShadowScorerServiceV1, SharedBytesV1) {
        let mut service = ShadowScorerServiceV1::with_test_model_v1(model);
        let bytes = SharedBytesV1::default();
        let export = XmageCp7TeacherJsonlWriterV1::from_writer_v1(
            Box::new(bytes.clone()),
            &service.identity,
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
