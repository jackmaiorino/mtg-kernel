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
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
use crate::native_ladder_pool_resolution_v1::resolve_ladder_checkpoint_authority_v1;
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
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

pub const CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1: &str = "mtg-kernel-checkpoint-shadow-stdio/v1";
pub const CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1: u32 = 1;
pub const CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1: &str =
    "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1";
pub const CHECKPOINT_SHADOW_MAX_REQUEST_BYTES_V1: usize = 1_048_576;

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
    /// A generation-0, weights-only derivative whose model parameters are
    /// bit-identical to promoted(2) g384. This is portable, but is never
    /// reported as the original checkpoint payload or manifest.
    PortablePromoted2WeightsGenesis { root: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ShadowCheckpointIdentityV1 {
    pub authority_kind: &'static str,
    pub source_run_sha256: &'static str,
    pub source_generation: u64,
    pub source_checkpoint_sha256: &'static str,
    pub source_sidecar_sha256: &'static str,
    pub source_payload_sha256: &'static str,
    pub source_train_state_sha256: &'static str,
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

fn fixed_ref_v1(authority: &ShadowCheckpointAuthorityV1) -> OpponentLadderCheckpointRefV1 {
    let (source_run_sha256, generation, checkpoint_sha256, sidecar_sha256, state_sha256) =
        match authority {
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. } => (
                SOURCE_RUN_SHA256_V1,
                SOURCE_GENERATION_V1,
                SOURCE_CHECKPOINT_SHA256_V1,
                SOURCE_SIDECAR_SHA256_V1,
                SOURCE_PAYLOAD_SHA256_V1,
            ),
            ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => (
                PORTABLE_RUN_SHA256_V1,
                PORTABLE_GENERATION_V1,
                PORTABLE_CHECKPOINT_SHA256_V1,
                PORTABLE_SIDECAR_SHA256_V1,
                PORTABLE_PAYLOAD_SHA256_V1,
            ),
        };
    OpponentLadderCheckpointRefV1 {
        source_run_sha256: source_run_sha256.to_owned(),
        generation,
        checkpoint_sha256: checkpoint_sha256.to_owned(),
        sidecar_sha256: sidecar_sha256.to_owned(),
        state_sha256: state_sha256.to_owned(),
    }
}

fn authority_root_v1(authority: &ShadowCheckpointAuthorityV1) -> &Path {
    match authority {
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { root }
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
) -> Result<(), ShadowScorerStartupErrorV1> {
    if lower_hex_raw32_v1(inference.run_sha256()) != expected_run
        || inference.generation_index() != expected_generation
        || lower_hex_raw32_v1(inference.checkpoint_manifest_sha256()) != expected_checkpoint
        || lower_hex_raw32_v1(inference.checkpoint_payload_sha256()) != expected_payload
        || lower_hex_raw32_v1(inference.train_state_sha256()) != expected_train_state
        || lower_hex_raw32_v1(inference.model_parameter_sha256())
            != SOURCE_MODEL_PARAMETER_SHA256_V1
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
    let fixed_ref = fixed_ref_v1(&requested);
    let authority =
        resolve_ladder_checkpoint_authority_v1(authority_root_v1(&requested), &fixed_ref).map_err(
            |_error| {
                #[cfg(test)]
                eprintln!("shadow checkpoint authority resolution failed: {_error:?}");
                ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
            },
        )?;
    validate_run_limits_v1(authority.run())?;
    if authority.run().environment_trajectory_contract_v1()
        != NativeRunEnvironmentTrajectoryContractV1::LegacyV1
    {
        return Err(ShadowScorerStartupErrorV1::new(
            ShadowScorerStartupErrorKindV1::CheckpointIdentity,
        ));
    }

    let (
        authority_kind,
        expected_run,
        expected_generation,
        expected_checkpoint,
        expected_payload,
        expected_train_state,
    ) = match requested {
        ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. } => (
            "original-promoted2-generation384-store",
            SOURCE_RUN_SHA256_V1,
            SOURCE_GENERATION_V1,
            SOURCE_CHECKPOINT_SHA256_V1,
            SOURCE_PAYLOAD_SHA256_V1,
            SOURCE_TRAIN_STATE_SHA256_V1,
        ),
        ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. } => {
            require_portable_source_binding_v1(authority.run())?;
            (
                "portable-promoted2-weights-generation0",
                PORTABLE_RUN_SHA256_V1,
                PORTABLE_GENERATION_V1,
                PORTABLE_CHECKPOINT_SHA256_V1,
                PORTABLE_PAYLOAD_SHA256_V1,
                PORTABLE_TRAIN_STATE_SHA256_V1,
            )
        }
    };
    let inference = authority.load_handle_v1().map_err(|_| {
        ShadowScorerStartupErrorV1::new(ShadowScorerStartupErrorKindV1::CheckpointAuthority)
    })?;
    require_inference_identity_v1(
        &inference,
        expected_run,
        expected_generation,
        expected_checkpoint,
        expected_payload,
        expected_train_state,
    )?;
    let identity = ShadowCheckpointIdentityV1 {
        authority_kind,
        source_run_sha256: SOURCE_RUN_SHA256_V1,
        source_generation: SOURCE_GENERATION_V1,
        source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1,
        source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1,
        source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1,
        source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1,
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
        })
    }

    #[cfg(test)]
    fn with_test_model_v1(model: Box<dyn ShadowModelScorerV1>) -> Self {
        Self {
            model,
            identity: ShadowCheckpointIdentityV1 {
                authority_kind: "test-only",
                source_run_sha256: SOURCE_RUN_SHA256_V1,
                source_generation: SOURCE_GENERATION_V1,
                source_checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1,
                source_sidecar_sha256: SOURCE_SIDECAR_SHA256_V1,
                source_payload_sha256: SOURCE_PAYLOAD_SHA256_V1,
                source_train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1,
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
        }
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
    let mut service = ShadowScorerServiceV1::load_v1(authority)?;
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
}
