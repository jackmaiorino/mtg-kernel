//! Offline terminal REINFORCE/value updates over strict XMage-versus-CP7
//! candidate-policy outcome exports.
//!
//! This is deliberately a narrow derivative path. It accepts either promoted
//! g384 or an exactly bound outcome derivative as its source, consumes one
//! whole-file-SHA-bound JSONL contract, and publishes a create-new raw
//! train-state bundle that this module can independently reload and verify.
//! Legacy terminal-REINFORCE contracts remain intact; the iterative path also
//! supports frozen standardized one-update training and parent-only,
//! full-batch PPO clipping over joint physical-decision likelihood ratios.

use crate::fast_sampler::{
    FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256, FAST_CATEGORICAL_SAMPLER_VERSION,
};
use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2,
    NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2, NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
};
use crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_ref_v1;
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyFrozenObjectiveTermV1, NativePolicyPhysicalDecisionV1,
    NativePolicySubstepV1, NativePolicyValueTrainStateV1, TRAINER_ALGORITHM_V1,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1,
    NativePolicyValueNetV1, HIDDEN_DIM_V1,
};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_verified_v1, encode_native_train_state_payload_v1,
    NativeTrainStatePayloadDigestsV1, NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::native_training_store_layout_v2::NativeTrainingStoreFinalNameV2;
use crate::rl::{
    parse_strict_json_value, ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1,
    TerminalOutcomeV1, TerminalSafeCodeV2,
};
use crate::rl_session::{RlSessionTerminalV1, RL_SESSION_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub const XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1: &str = "mtg-kernel-xmage-cp7-outcome-jsonl/v1";
pub const XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2: &str = "mtg-kernel-xmage-cp7-outcome-jsonl/v2";
pub const XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1: &str = "candidate_checkpoint_policy";
pub const XMAGE_CP7_OUTCOME_DERIVATIVE_SCHEMA_V1: &str =
    "mtg-kernel-xmage-cp7-outcome-reinforce-derivative/v1";

const MODEL_INPUT_COMMITMENT_V1: &str = "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1";
const RANDOMIZATION_IDENTITY_V1: &str = "legacy_v1";
const ENVIRONMENT_TRAJECTORY_CONTRACT_V1: &str = "legacy-v1";
const AUTHORITY_KIND_V1: &str = "original-promoted2-generation384-store";
const DERIVATIVE_PAYLOAD_FILENAME_V1: &str = "checkpoint.state.f32le";
const DERIVATIVE_MANIFEST_FILENAME_V1: &str = "checkpoint.json";
const TRAINING_ORDER_V1: &str = "jsonl-record-order-epoch-major-contiguous-batches/v1";
const OPTIMIZER_V1: &str = "native-adam-canonical-scorer-bias-gauge-v1";
const STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1: &str =
    "terminal_reinforce_frozen_source_value_standardized_episode_balanced/v1";
const PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1: &str =
    "ppo_clip_frozen_source_value_standardized_episode_balanced/v1";
const STANDARDIZED_EPISODE_BALANCED_TRANSFORM_V1: &str =
    "frozen-source-value-population-standardization-equal-episode-mass/v1";
const PPO_CLIP_TRANSFORM_V1: &str = "selected-joint-physical-group-likelihood-ratio-clipping/v1";
const PPO_OLD_POLICY_SOURCE_V1: &str = "corpus_old_policy_logits_f32_bits";
const PPO_LIKELIHOOD_RATIO_SCOPE_V1: &str = "joint_selected_autoregressive_physical_group";
const PPO_CLIPPING_SCOPE_V1: &str = "once_per_physical_group_after_joint_log_probability_sum";
const PPO_INITIAL_MAX_ABSOLUTE_JOINT_LOG_RATIO_V1: f64 = 2.0e-4;
const STANDARDIZED_EPISODE_BALANCED_CLI_V1: &str = "standardized-episode-balanced";
const PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_CLI_V1: &str =
    "ppo-clip-standardized-episode-balanced";
const RAW_ADVANTAGE_CLI_V1: &str = "raw";
const ADVANTAGE_STANDARD_DEVIATION_FLOOR_V1: f64 = 1.0e-6;

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
const SOURCE_PARAMETERS_SECTION_SHA256_V1: &str =
    "895a034ba57e6d35903d77e55a88a8a98b220b064320ff3e694eabcfed4c8224";
const SOURCE_FIRST_MOMENTS_SECTION_SHA256_V1: &str =
    "f5bfcdf4cdba9a758fb6c20c156abf4cfcf50b44c02b8f92672acf3e3e6baa9e";
const SOURCE_SECOND_MOMENTS_SECTION_SHA256_V1: &str =
    "5f5c466428a825f108566c0778b98dc011b7d2f83c739f90008a7af703587d92";
const SOURCE_SCORER_BIAS_ANCHOR_F32_BITS_V1: u32 = 3_141_403_366;
const RALLY_DECK_HASH_U64_V1: u64 = 909_447_583_901_160_127;

const TRANSPORT_ABSOLUTE_TOLERANCE_V1: f32 = 3.0e-5;
const TRANSPORT_RELATIVE_TOLERANCE_V1: f32 = 3.0e-5;

type DynResultV1<T> = Result<T, Box<dyn Error>>;

fn invalid_data_v1(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct CheckpointIdentityWireV1 {
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderWireV1 {
    record_type: String,
    schema_version: u32,
    record_ordinal: u64,
    export_contract: String,
    selection_source: String,
    tensorizer_identity: String,
    tensorizer_features_source_sha256: String,
    model_input_commitment: String,
    checkpoint: CheckpointIdentityWireV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TensorWireV1 {
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionWireV1 {
    record_type: String,
    schema_version: u32,
    record_ordinal: u64,
    outcome_decision_ordinal: u64,
    #[serde(default)]
    checkpoint: Option<CheckpointIdentityWireV1>,
    selection_source: String,
    deck_ids: Vec<String>,
    randomization_identity: String,
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
    decision_kind: String,
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
    tensor: TensorWireV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalWireV1 {
    record_type: String,
    schema_version: u32,
    record_ordinal: u64,
    #[serde(default)]
    checkpoint: Option<CheckpointIdentityWireV1>,
    deck_ids: Vec<String>,
    randomization_identity: String,
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
}

#[derive(Clone, Debug)]
struct OwnedDecisionTensorV1 {
    state: Vec<f32>,
    object_features: Vec<f32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<f32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features: Vec<f32>,
    action_ref_features: Vec<f32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

impl OwnedDecisionTensorV1 {
    fn from_wire_v1(wire: TensorWireV1) -> DynResultV1<Self> {
        Ok(Self {
            state: decode_f32_bits_v1("state_f32_bits", wire.state_f32_bits)?,
            object_features: decode_f32_bits_v1(
                "object_features_f32_bits",
                wire.object_features_f32_bits,
            )?,
            object_card_ids: wire.object_card_ids,
            object_groups: wire.object_groups,
            object_node_ids: wire.object_node_ids,
            edge_features: decode_f32_bits_v1(
                "edge_features_f32_bits",
                wire.edge_features_f32_bits,
            )?,
            edge_source_indices: wire.edge_source_indices,
            edge_target_indices: wire.edge_target_indices,
            action_features: decode_f32_bits_v1(
                "action_features_f32_bits",
                wire.action_features_f32_bits,
            )?,
            action_ref_features: decode_f32_bits_v1(
                "action_ref_features_f32_bits",
                wire.action_ref_features_f32_bits,
            )?,
            action_ref_card_ids: wire.action_ref_card_ids,
            action_ref_action_indices: wire.action_ref_action_indices,
            action_ref_node_indices: wire.action_ref_node_indices,
        })
    }

    fn view_v1(&self) -> NativeEncodedDecisionViewV1<'_> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            &self.state,
            &self.object_features,
            &self.object_card_ids,
            &self.object_groups,
            &self.object_node_ids,
            &self.edge_features,
            &self.edge_source_indices,
            &self.edge_target_indices,
            &self.action_features,
            &self.action_ref_features,
            &self.action_ref_card_ids,
            &self.action_ref_action_indices,
            &self.action_ref_node_indices,
        )
    }
}

fn decode_f32_bits_v1(field: &'static str, bits: Vec<u32>) -> DynResultV1<Vec<f32>> {
    bits.into_iter()
        .enumerate()
        .map(|(index, bits)| {
            let value = f32::from_bits(bits);
            if value.is_finite() {
                Ok(value)
            } else {
                Err(invalid_data_v1(format!("nonfinite outcome tensor {field}[{index}]")).into())
            }
        })
        .collect()
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

fn model_input_sha256_v1(tensor: &OwnedDecisionTensorV1) -> String {
    let mut hasher = Sha256::new();
    framed_atom_v1(
        &mut hasher,
        "schema",
        MODEL_INPUT_COMMITMENT_V1.len() as u64,
        MODEL_INPUT_COMMITMENT_V1.bytes(),
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
    let digest = hasher.finalize();
    let mut raw = [0_u8; 32];
    raw.copy_from_slice(&digest);
    lower_hex_raw32_v1(raw)
}

#[derive(Clone, Debug)]
struct OutcomeExampleV1 {
    record_ordinal: u64,
    outcome_decision_ordinal: u64,
    pair_index: u64,
    episode_id: u64,
    step: u64,
    environment_revision: u64,
    physical_decision_id: u64,
    actor_physical_decision_ordinal: u64,
    substep_index: u32,
    substep_count: u32,
    decision_kind: String,
    legal_action_count: usize,
    selected_index: usize,
    old_policy_logits_f32_bits: Vec<u32>,
    old_value_f32_bits: u32,
    tensor: OwnedDecisionTensorV1,
}

#[derive(Clone, Debug)]
struct OutcomePhysicalDecisionV1 {
    first_record_ordinal: u64,
    pair_index: u64,
    episode_id: u64,
    candidate_seat: PlayerSeatV1,
    terminal_return: i8,
    decision_kind: String,
    examples: Vec<OutcomeExampleV1>,
}

#[derive(Clone, Debug)]
struct PendingEpisodeV1 {
    pair_index: u64,
    episode_id: u64,
    candidate_seat: PlayerSeatV1,
    base_seed_u64_hex: String,
    pair_environment_seed_u64_hex: String,
    first_outcome_decision_ordinal: Option<u64>,
    decisions: Vec<OutcomeExampleV1>,
}

#[derive(Clone, Debug)]
struct PairObservationV1 {
    base_seed_u64_hex: String,
    pair_environment_seed_u64_hex: String,
    episode_ids: Vec<u64>,
    candidate_seats: BTreeSet<PlayerSeatV1>,
}

#[derive(Clone, Debug)]
struct OutcomeDatasetV1 {
    jsonl_sha256: String,
    export_contract: String,
    schema_version: u32,
    policy_checkpoint: CheckpointIdentityWireV1,
    decision_row_count: usize,
    terminal_row_count: usize,
    episode_count: usize,
    pair_indices: Vec<u64>,
    terminal_return_counts: [u64; 3],
    groups: Vec<OutcomePhysicalDecisionV1>,
}

fn valid_lower_hex_v1(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn rally_deck_ids_v1(value: &[String]) -> bool {
    value.len() == 2 && value[0] == "Rally" && value[1] == "Rally"
}

fn exact_g384_checkpoint_v1(checkpoint: &CheckpointIdentityWireV1) -> bool {
    checkpoint.authority_kind == AUTHORITY_KIND_V1
        && checkpoint.source_run_sha256 == SOURCE_RUN_SHA256_V1
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
        && checkpoint.model_parameter_sha256 == SOURCE_MODEL_PARAMETER_SHA256_V1
}

fn outcome_parent_checkpoint_v1(checkpoint: &CheckpointIdentityWireV1) -> bool {
    checkpoint.authority_kind == "xmage-cp7-outcome-reinforce-derivative-v1"
        && checkpoint.source_run_sha256 == SOURCE_RUN_SHA256_V1
        && checkpoint.source_generation == SOURCE_GENERATION_V1
        && checkpoint.source_checkpoint_sha256 == SOURCE_CHECKPOINT_SHA256_V1
        && checkpoint.source_sidecar_sha256 == SOURCE_SIDECAR_SHA256_V1
        && checkpoint.source_payload_sha256 == SOURCE_PAYLOAD_SHA256_V1
        && checkpoint.source_train_state_sha256 == SOURCE_TRAIN_STATE_SHA256_V1
        && checkpoint.loaded_run_sha256 == SOURCE_RUN_SHA256_V1
        && checkpoint.loaded_generation > 0
        && valid_lower_hex_v1(&checkpoint.loaded_checkpoint_sha256, 64)
        && valid_lower_hex_v1(&checkpoint.loaded_payload_sha256, 64)
        && valid_lower_hex_v1(&checkpoint.loaded_train_state_sha256, 64)
        && valid_lower_hex_v1(&checkpoint.model_parameter_sha256, 64)
}

fn validate_header_v1(header: &HeaderWireV1) -> DynResultV1<()> {
    let checkpoint = &header.checkpoint;
    let common_valid = header.record_type == "header"
        && header.record_ordinal == 0
        && header.selection_source == XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1
        && header.tensorizer_identity == NATIVE_FLAT_TENSORIZER_IDENTITY_V2
        && header.tensorizer_features_source_sha256
            == NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2
        && header.model_input_commitment == MODEL_INPUT_COMMITMENT_V1
        && checkpoint.environment_trajectory_contract == ENVIRONMENT_TRAJECTORY_CONTRACT_V1
        && checkpoint.sampler_identity == FAST_CATEGORICAL_SAMPLER_VERSION
        && checkpoint.sampler_contract_sha256 == FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256;
    let authority_valid = match (header.schema_version, header.export_contract.as_str()) {
        (1, XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1) => exact_g384_checkpoint_v1(checkpoint),
        (2, XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2) => outcome_parent_checkpoint_v1(checkpoint),
        _ => false,
    };
    if !common_valid || !authority_valid {
        return Err(invalid_data_v1("outcome header policy authority is invalid").into());
    }
    Ok(())
}

fn row_checkpoint_binding_valid_v1(
    schema_version: u32,
    expected: &CheckpointIdentityWireV1,
    actual: Option<&CheckpointIdentityWireV1>,
) -> bool {
    match schema_version {
        1 => actual.is_none() && exact_g384_checkpoint_v1(expected),
        2 => actual == Some(expected) && outcome_parent_checkpoint_v1(expected),
        _ => false,
    }
}

fn row_checkpoint_field_presence_valid_v1(schema_version: u32, row: &serde_json::Value) -> bool {
    match schema_version {
        1 => row
            .as_object()
            .is_some_and(|object| !object.contains_key("checkpoint")),
        2 => row
            .get("checkpoint")
            .is_some_and(|checkpoint| !checkpoint.is_null()),
        _ => false,
    }
}

fn validate_natural_terminal_v1(
    row: &TerminalWireV1,
    schema_version: u32,
    checkpoint: &CheckpointIdentityWireV1,
) -> DynResultV1<()> {
    let terminal = &row.terminal;
    let tuple_valid = match terminal.terminal_outcome {
        TerminalOutcomeV1::P0Win => {
            terminal.winner == Some(PlayerSeatV1::P0) && terminal.terminal_reward == [1, -1]
        }
        TerminalOutcomeV1::P1Win => {
            terminal.winner == Some(PlayerSeatV1::P1) && terminal.terminal_reward == [-1, 1]
        }
        TerminalOutcomeV1::Draw => terminal.winner.is_none() && terminal.terminal_reward == [0, 0],
        TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => false,
    };
    let candidate_index = match row.candidate_seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    };
    if row.record_type != "terminal"
        || row.schema_version != schema_version
        || !row_checkpoint_binding_valid_v1(schema_version, checkpoint, row.checkpoint.as_ref())
        || terminal.schema_version != RL_SESSION_SCHEMA_VERSION
        || !rally_deck_ids_v1(&terminal.deck_ids)
        || terminal.deck_hashes != [RALLY_DECK_HASH_U64_V1; 2]
        || terminal.episode_id != row.episode_id
        || terminal.terminal_classification != TerminalClassificationV1::Natural
        || terminal.terminal_code != TerminalSafeCodeV2::NaturalGameOver
        || !tuple_valid
        || !matches!(row.candidate_terminal_reward, -1..=1)
        || terminal.terminal_reward[candidate_index] != i32::from(row.candidate_terminal_reward)
        || terminal.terminal_reason != "game_over"
        || !rally_deck_ids_v1(&row.deck_ids)
        || !valid_lower_hex_v1(&row.base_seed_u64_hex, 16)
        || !valid_lower_hex_v1(&row.pair_environment_seed_u64_hex, 16)
        || !valid_lower_hex_v1(&row.diagnostic_state_hash_u64_hex, 16)
        || !valid_lower_hex_v1(&row.core_environment_hash_u64_hex, 16)
        || row.randomization_identity != RANDOMIZATION_IDENTITY_V1
    {
        return Err(invalid_data_v1(format!(
            "invalid natural outcome terminal row {}",
            row.record_ordinal
        ))
        .into());
    }
    Ok(())
}

fn decision_from_wire_v1(
    row: DecisionWireV1,
    expected_record_ordinal: u64,
    expected_outcome_decision_ordinal: u64,
    schema_version: u32,
    checkpoint: &CheckpointIdentityWireV1,
) -> DynResultV1<(OutcomeExampleV1, PlayerSeatV1, String, String)> {
    let legal_action_count = usize::try_from(row.legal_action_count)?;
    let selected_index = usize::try_from(row.selected_index)?;
    if row.record_type != "decision"
        || row.schema_version != schema_version
        || !row_checkpoint_binding_valid_v1(schema_version, checkpoint, row.checkpoint.as_ref())
        || row.record_ordinal != expected_record_ordinal
        || row.outcome_decision_ordinal != expected_outcome_decision_ordinal
        || row.selection_source != XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1
        || !rally_deck_ids_v1(&row.deck_ids)
        || row.randomization_identity != RANDOMIZATION_IDENTITY_V1
        || !valid_lower_hex_v1(&row.base_seed_u64_hex, 16)
        || !valid_lower_hex_v1(&row.pair_environment_seed_u64_hex, 16)
        || row.acting_player != row.candidate_seat
        || row.substep_count == 0
        || row.substep_index >= row.substep_count
        || legal_action_count == 0
        || legal_action_count != row.old_policy_logits_f32_bits.len()
        || legal_action_count != row.action_semantics.len()
        || selected_index >= legal_action_count
        || row.action_semantics.get(selected_index) != Some(&row.selected_semantic)
        || !valid_lower_hex_v1(&row.candidate_order_commitment_128_hex, 32)
        || !valid_lower_hex_v1(&row.model_input_sha256, 64)
        || !f32::from_bits(row.old_value_f32_bits).is_finite()
        || row
            .old_policy_logits_f32_bits
            .iter()
            .any(|bits| !f32::from_bits(*bits).is_finite())
    {
        return Err(invalid_data_v1(format!(
            "invalid outcome decision row {}",
            row.record_ordinal
        ))
        .into());
    }
    let tensor = OwnedDecisionTensorV1::from_wire_v1(row.tensor)?;
    if model_input_sha256_v1(&tensor) != row.model_input_sha256 {
        return Err(invalid_data_v1(format!(
            "outcome tensor commitment mismatch at row {}",
            row.record_ordinal
        ))
        .into());
    }
    let candidate_seat = row.candidate_seat;
    let base_seed = row.base_seed_u64_hex.clone();
    let environment_seed = row.pair_environment_seed_u64_hex.clone();
    Ok((
        OutcomeExampleV1 {
            record_ordinal: row.record_ordinal,
            outcome_decision_ordinal: row.outcome_decision_ordinal,
            pair_index: row.pair_index,
            episode_id: row.episode_id,
            step: row.step,
            environment_revision: row.environment_revision,
            physical_decision_id: row.physical_decision_id,
            actor_physical_decision_ordinal: row.actor_physical_decision_ordinal,
            substep_index: row.substep_index,
            substep_count: row.substep_count,
            decision_kind: row.decision_kind,
            legal_action_count,
            selected_index,
            old_policy_logits_f32_bits: row.old_policy_logits_f32_bits,
            old_value_f32_bits: row.old_value_f32_bits,
            tensor,
        },
        candidate_seat,
        base_seed,
        environment_seed,
    ))
}

fn finalize_episode_v1(
    pending: PendingEpisodeV1,
    terminal: &TerminalWireV1,
    groups: &mut Vec<OutcomePhysicalDecisionV1>,
) -> DynResultV1<usize> {
    let decision_count = u64::try_from(pending.decisions.len())?;
    if terminal.pair_index != pending.pair_index
        || terminal.episode_id != pending.episode_id
        || terminal.candidate_seat != pending.candidate_seat
        || terminal.base_seed_u64_hex != pending.base_seed_u64_hex
        || terminal.pair_environment_seed_u64_hex != pending.pair_environment_seed_u64_hex
        || terminal.outcome_decision_count != decision_count
        || terminal.first_outcome_decision_ordinal != pending.first_outcome_decision_ordinal
        || (decision_count == 0) != terminal.first_outcome_decision_ordinal.is_none()
        || terminal.terminal.policy_step_count < decision_count
    {
        return Err(invalid_data_v1(format!(
            "outcome terminal range mismatch for episode {}",
            terminal.episode_id
        ))
        .into());
    }

    let mut physical_count = 0_usize;
    let mut index = 0_usize;
    while index < pending.decisions.len() {
        let first = &pending.decisions[index];
        let substep_count = usize::try_from(first.substep_count)?;
        let end = index
            .checked_add(substep_count)
            .ok_or_else(|| invalid_data_v1("physical group length overflow"))?;
        if first.substep_index != 0 || end > pending.decisions.len() {
            return Err(invalid_data_v1("incomplete outcome physical decision").into());
        }
        let slice = &pending.decisions[index..end];
        if slice.iter().enumerate().any(|(offset, row)| {
            row.pair_index != pending.pair_index
                || row.episode_id != pending.episode_id
                || row.physical_decision_id != first.physical_decision_id
                || row.actor_physical_decision_ordinal != first.actor_physical_decision_ordinal
                || row.substep_count != first.substep_count
                || usize::try_from(row.substep_index).ok() != Some(offset)
                || row.decision_kind != first.decision_kind
                || (offset > 0
                    && (row.step != slice[offset - 1].step + 1
                        || row.environment_revision != slice[offset - 1].environment_revision + 1))
        }) {
            return Err(invalid_data_v1(format!(
                "noncontiguous outcome physical decision {}",
                first.physical_decision_id
            ))
            .into());
        }
        if physical_count > 0 {
            let previous = groups
                .last()
                .ok_or_else(|| invalid_data_v1("missing previous physical group"))?;
            if first.actor_physical_decision_ordinal
                != previous.examples[0].actor_physical_decision_ordinal + 1
            {
                return Err(invalid_data_v1("candidate physical ordinal is not contiguous").into());
            }
        } else if first.actor_physical_decision_ordinal != 0 {
            return Err(
                invalid_data_v1("candidate physical ordinal does not start at zero").into(),
            );
        }
        groups.push(OutcomePhysicalDecisionV1 {
            first_record_ordinal: first.record_ordinal,
            pair_index: pending.pair_index,
            episode_id: pending.episode_id,
            candidate_seat: pending.candidate_seat,
            terminal_return: terminal.candidate_terminal_reward,
            decision_kind: first.decision_kind.clone(),
            examples: slice.to_vec(),
        });
        physical_count += 1;
        index = end;
    }
    if terminal.terminal.physical_decision_count < u64::try_from(physical_count)? {
        return Err(invalid_data_v1("terminal physical count is below candidate count").into());
    }
    Ok(physical_count)
}

fn load_outcome_dataset_v1(path: &Path) -> DynResultV1<OutcomeDatasetV1> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut digest = Sha256::new();
    let mut expected_record_ordinal = 0_u64;
    let mut expected_outcome_decision_ordinal = 0_u64;
    let mut header_seen = false;
    let mut export_contract = None::<String>;
    let mut corpus_schema_version = None::<u32>;
    let mut policy_checkpoint = None::<CheckpointIdentityWireV1>;
    let mut pending: Option<PendingEpisodeV1> = None;
    let mut closed_episodes = BTreeSet::<u64>::new();
    let mut pairs = BTreeMap::<u64, PairObservationV1>::new();
    let mut groups = Vec::new();
    let mut terminal_row_count = 0_usize;
    let mut terminal_return_counts = [0_u64; 3];

    loop {
        line.clear();
        let byte_count = reader.read_line(&mut line)?;
        if byte_count == 0 {
            break;
        }
        if !line.ends_with('\n') || line.contains('\r') {
            return Err(
                invalid_data_v1("outcome JSONL rows must end in LF and contain no CR").into(),
            );
        }
        digest.update(line.as_bytes());
        let trimmed = line.strip_suffix('\n').expect("LF checked");
        if trimmed.is_empty() {
            return Err(invalid_data_v1("outcome JSONL contains an empty row").into());
        }
        let value = parse_strict_json_value(trimmed)?;
        let record_type = value
            .get("record_type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid_data_v1("outcome row lacks record_type"))?;
        match record_type {
            "header" => {
                if header_seen || expected_record_ordinal != 0 || pending.is_some() {
                    return Err(invalid_data_v1("outcome header must be the first row").into());
                }
                let header: HeaderWireV1 = serde_json::from_value(value)?;
                validate_header_v1(&header)?;
                export_contract = Some(header.export_contract.clone());
                corpus_schema_version = Some(header.schema_version);
                policy_checkpoint = Some(header.checkpoint.clone());
                header_seen = true;
                expected_record_ordinal = 1;
            }
            "decision" => {
                if !header_seen {
                    return Err(invalid_data_v1("outcome decision precedes header").into());
                }
                let schema_version = corpus_schema_version
                    .ok_or_else(|| invalid_data_v1("outcome header schema is missing"))?;
                if !row_checkpoint_field_presence_valid_v1(schema_version, &value) {
                    return Err(invalid_data_v1(
                        "outcome decision checkpoint field presence is invalid",
                    )
                    .into());
                }
                let row: DecisionWireV1 = serde_json::from_value(value)?;
                let checkpoint = policy_checkpoint
                    .as_ref()
                    .ok_or_else(|| invalid_data_v1("outcome header checkpoint is missing"))?;
                let (example, candidate_seat, base_seed, environment_seed) = decision_from_wire_v1(
                    row,
                    expected_record_ordinal,
                    expected_outcome_decision_ordinal,
                    schema_version,
                    checkpoint,
                )?;
                if closed_episodes.contains(&example.episode_id) {
                    return Err(invalid_data_v1("outcome decision follows its terminal").into());
                }
                match pending.as_mut() {
                    Some(active) => {
                        if active.pair_index != example.pair_index
                            || active.episode_id != example.episode_id
                            || active.candidate_seat != candidate_seat
                            || active.base_seed_u64_hex != base_seed
                            || active.pair_environment_seed_u64_hex != environment_seed
                        {
                            return Err(invalid_data_v1(
                                "outcome episodes are interleaved or metadata drifted",
                            )
                            .into());
                        }
                    }
                    None => {
                        pending = Some(PendingEpisodeV1 {
                            pair_index: example.pair_index,
                            episode_id: example.episode_id,
                            candidate_seat,
                            base_seed_u64_hex: base_seed,
                            pair_environment_seed_u64_hex: environment_seed,
                            first_outcome_decision_ordinal: Some(example.outcome_decision_ordinal),
                            decisions: Vec::new(),
                        });
                    }
                }
                pending
                    .as_mut()
                    .expect("pending created")
                    .decisions
                    .push(example);
                expected_record_ordinal = expected_record_ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("record ordinal exhausted"))?;
                expected_outcome_decision_ordinal = expected_outcome_decision_ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("outcome decision ordinal exhausted"))?;
            }
            "terminal" => {
                if !header_seen {
                    return Err(invalid_data_v1("outcome terminal precedes header").into());
                }
                let schema_version = corpus_schema_version
                    .ok_or_else(|| invalid_data_v1("outcome header schema is missing"))?;
                if !row_checkpoint_field_presence_valid_v1(schema_version, &value) {
                    return Err(invalid_data_v1(
                        "outcome terminal checkpoint field presence is invalid",
                    )
                    .into());
                }
                let terminal: TerminalWireV1 = serde_json::from_value(value)?;
                if terminal.record_ordinal != expected_record_ordinal {
                    return Err(invalid_data_v1("outcome terminal record ordinal mismatch").into());
                }
                validate_natural_terminal_v1(
                    &terminal,
                    schema_version,
                    policy_checkpoint
                        .as_ref()
                        .ok_or_else(|| invalid_data_v1("outcome header checkpoint is missing"))?,
                )?;
                if closed_episodes.contains(&terminal.episode_id) {
                    return Err(invalid_data_v1("duplicate outcome terminal episode").into());
                }
                let active = match pending.take() {
                    Some(active) => active,
                    None if terminal.outcome_decision_count == 0
                        && terminal.first_outcome_decision_ordinal.is_none() =>
                    {
                        PendingEpisodeV1 {
                            pair_index: terminal.pair_index,
                            episode_id: terminal.episode_id,
                            candidate_seat: terminal.candidate_seat,
                            base_seed_u64_hex: terminal.base_seed_u64_hex.clone(),
                            pair_environment_seed_u64_hex: terminal
                                .pair_environment_seed_u64_hex
                                .clone(),
                            first_outcome_decision_ordinal: None,
                            decisions: Vec::new(),
                        }
                    }
                    None => {
                        return Err(invalid_data_v1(
                            "outcome terminal claims decisions without an active episode",
                        )
                        .into())
                    }
                };
                for window in active.decisions.windows(2) {
                    if window[1].record_ordinal != window[0].record_ordinal + 1
                        || window[1].outcome_decision_ordinal
                            != window[0].outcome_decision_ordinal + 1
                        || window[1].step <= window[0].step
                        || window[1].environment_revision <= window[0].environment_revision
                    {
                        return Err(invalid_data_v1(
                            "outcome episode decision stream is not strictly ordered",
                        )
                        .into());
                    }
                }
                finalize_episode_v1(active, &terminal, &mut groups)?;
                let pair = pairs
                    .entry(terminal.pair_index)
                    .or_insert_with(|| PairObservationV1 {
                        base_seed_u64_hex: terminal.base_seed_u64_hex.clone(),
                        pair_environment_seed_u64_hex: terminal
                            .pair_environment_seed_u64_hex
                            .clone(),
                        episode_ids: Vec::new(),
                        candidate_seats: BTreeSet::new(),
                    });
                if pair.base_seed_u64_hex != terminal.base_seed_u64_hex
                    || pair.pair_environment_seed_u64_hex != terminal.pair_environment_seed_u64_hex
                    || !pair.candidate_seats.insert(terminal.candidate_seat)
                {
                    return Err(
                        invalid_data_v1("outcome pair metadata or seat swap mismatch").into(),
                    );
                }
                pair.episode_ids.push(terminal.episode_id);
                closed_episodes.insert(terminal.episode_id);
                terminal_return_counts[usize::try_from(terminal.candidate_terminal_reward + 1)?] +=
                    1;
                terminal_row_count += 1;
                expected_record_ordinal = expected_record_ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("record ordinal exhausted"))?;
            }
            _ => return Err(invalid_data_v1("unknown outcome record_type").into()),
        }
    }

    if !header_seen || pending.is_some() || groups.is_empty() || terminal_row_count == 0 {
        return Err(
            invalid_data_v1("outcome corpus is incomplete or has no trainable groups").into(),
        );
    }
    for (pair_index, pair) in &pairs {
        if pair.episode_ids.len() != 2
            || pair.candidate_seats.len() != 2
            || pair.episode_ids[0] == pair.episode_ids[1]
        {
            return Err(invalid_data_v1(format!(
                "outcome pair {pair_index} is not an exact seat-swapped pair"
            ))
            .into());
        }
    }
    groups.sort_by_key(|group| group.first_record_ordinal);
    Ok(OutcomeDatasetV1 {
        jsonl_sha256: lower_hex_raw32_v1(digest.finalize().into()),
        export_contract: export_contract
            .ok_or_else(|| invalid_data_v1("outcome export contract is missing"))?,
        schema_version: corpus_schema_version
            .ok_or_else(|| invalid_data_v1("outcome schema version is missing"))?,
        policy_checkpoint: policy_checkpoint
            .ok_or_else(|| invalid_data_v1("outcome policy checkpoint is missing"))?,
        decision_row_count: usize::try_from(expected_outcome_decision_ordinal)?,
        terminal_row_count,
        episode_count: closed_episodes.len(),
        pair_indices: pairs.into_keys().collect(),
        terminal_return_counts,
        groups,
    })
}

fn load_source_train_state_v1(source_root: &Path) -> DynResultV1<NativePolicyValueTrainStateV1> {
    let checkpoint_ref = stage_ladder_checkpoint_ref_v1(source_root, SOURCE_GENERATION_V1)?;
    if checkpoint_ref.source_run_sha256 != SOURCE_RUN_SHA256_V1
        || checkpoint_ref.generation != SOURCE_GENERATION_V1
        || checkpoint_ref.checkpoint_sha256 != SOURCE_CHECKPOINT_SHA256_V1
        || checkpoint_ref.sidecar_sha256 != SOURCE_SIDECAR_SHA256_V1
        || checkpoint_ref.state_sha256 != SOURCE_PAYLOAD_SHA256_V1
    {
        return Err(invalid_data_v1("source root is not promoted g384 authority").into());
    }
    let payload_name = NativeTrainingStoreFinalNameV2::StatePayload {
        generation_index: SOURCE_GENERATION_V1,
    };
    let payload_path = source_root
        .join(
            payload_name
                .directory()
                .basename()
                .ok_or_else(|| invalid_data_v1("source payload directory missing"))?,
        )
        .join(payload_name.final_basename()?);
    let payload_bytes = fs::read(payload_path)?;
    let expected = NativeTrainStatePayloadDigestsV1 {
        payload_sha256: parse_lower_hex_raw32_v1(SOURCE_PAYLOAD_SHA256_V1)?,
        parameters_sha256: parse_lower_hex_raw32_v1(SOURCE_PARAMETERS_SECTION_SHA256_V1)?,
        first_moments_sha256: parse_lower_hex_raw32_v1(SOURCE_FIRST_MOMENTS_SECTION_SHA256_V1)?,
        second_moments_sha256: parse_lower_hex_raw32_v1(SOURCE_SECOND_MOMENTS_SECTION_SHA256_V1)?,
        model_parameter_sha256: parse_lower_hex_raw32_v1(SOURCE_MODEL_PARAMETER_SHA256_V1)?,
        native_state_sha256: parse_lower_hex_raw32_v1(SOURCE_TRAIN_STATE_SHA256_V1)?,
    };
    let decoded = decode_native_train_state_payload_verified_v1(
        &payload_bytes,
        SOURCE_GENERATION_V1,
        SOURCE_SCORER_BIAS_ANCHOR_F32_BITS_V1,
        &expected,
    )?;
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    model.replace_parameter_snapshot_v1(&decoded.snapshot.parameters)?;
    if model.parameter_manifest_sha256_v1() != SOURCE_MODEL_PARAMETER_SHA256_V1 {
        return Err(invalid_data_v1("source model parameter digest mismatch").into());
    }
    Ok(NativePolicyValueTrainStateV1::new_v1(model)?)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TransportAuditV1 {
    identity: String,
    decision_row_count: u64,
    logit_value_count: u64,
    bit_exact_decision_row_count: u64,
    mismatched_decision_row_count: u64,
    value_mismatch_count: u64,
    logit_mismatch_count: u64,
    max_value_absolute_delta: f32,
    max_logit_absolute_delta: f32,
    absolute_tolerance: f32,
    relative_tolerance: f32,
}

fn transport_bound_v1(exported: f32) -> f32 {
    TRANSPORT_ABSOLUTE_TOLERANCE_V1 + TRANSPORT_RELATIVE_TOLERANCE_V1 * exported.abs()
}

fn audit_source_transport_v1(
    model: &NativePolicyValueNetV1,
    groups: &[OutcomePhysicalDecisionV1],
    parent_source: bool,
) -> DynResultV1<TransportAuditV1> {
    let mut audit = TransportAuditV1 {
        identity: if parent_source {
            "exported-loaded-outcome-parent-forward-envelope/v1".to_owned()
        } else {
            "exported-g384-forward-envelope/v1".to_owned()
        },
        decision_row_count: 0,
        logit_value_count: 0,
        bit_exact_decision_row_count: 0,
        mismatched_decision_row_count: 0,
        value_mismatch_count: 0,
        logit_mismatch_count: 0,
        max_value_absolute_delta: 0.0,
        max_logit_absolute_delta: 0.0,
        absolute_tolerance: TRANSPORT_ABSOLUTE_TOLERANCE_V1,
        relative_tolerance: TRANSPORT_RELATIVE_TOLERANCE_V1,
    };
    for group in groups {
        for example in &group.examples {
            let output = model.forward_v1(example.tensor.view_v1())?;
            if output.logits.len() != example.legal_action_count
                || output.logits.len() != example.old_policy_logits_f32_bits.len()
            {
                return Err(invalid_data_v1("outcome source forward width mismatch").into());
            }
            audit.decision_row_count += 1;
            audit.logit_value_count += u64::try_from(output.logits.len())?;
            let mut exact = true;
            let exported_value = f32::from_bits(example.old_value_f32_bits);
            if output.value.to_bits() != example.old_value_f32_bits {
                exact = false;
                audit.value_mismatch_count += 1;
                let delta = (output.value - exported_value).abs();
                if delta > transport_bound_v1(exported_value) {
                    return Err(invalid_data_v1(format!(
                        "outcome source value transport exceeds envelope at decision {}",
                        example.outcome_decision_ordinal
                    ))
                    .into());
                }
                audit.max_value_absolute_delta = audit.max_value_absolute_delta.max(delta);
            }
            for (action_index, (actual, expected_bits)) in output
                .logits
                .iter()
                .copied()
                .zip(example.old_policy_logits_f32_bits.iter().copied())
                .enumerate()
            {
                if actual.to_bits() == expected_bits {
                    continue;
                }
                exact = false;
                audit.logit_mismatch_count += 1;
                let expected = f32::from_bits(expected_bits);
                let delta = (actual - expected).abs();
                if delta > transport_bound_v1(expected) {
                    return Err(invalid_data_v1(format!(
                        "outcome source logit transport exceeds envelope at decision {} action {}",
                        example.outcome_decision_ordinal, action_index
                    ))
                    .into());
                }
                audit.max_logit_absolute_delta = audit.max_logit_absolute_delta.max(delta);
            }
            if exact {
                audit.bit_exact_decision_row_count += 1;
            } else {
                audit.mismatched_decision_row_count += 1;
            }
        }
    }
    Ok(audit)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdvantageModeV1 {
    Raw,
    StandardizedEpisodeBalanced,
    PpoClipStandardizedEpisodeBalanced,
}

impl Default for AdvantageModeV1 {
    fn default() -> Self {
        Self::Raw
    }
}

impl AdvantageModeV1 {
    fn cli_v1(self) -> &'static str {
        match self {
            Self::Raw => RAW_ADVANTAGE_CLI_V1,
            Self::StandardizedEpisodeBalanced => STANDARDIZED_EPISODE_BALANCED_CLI_V1,
            Self::PpoClipStandardizedEpisodeBalanced => {
                PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_CLI_V1
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AdvantageObservationV1 {
    episode_id: u64,
    terminal_return: i8,
    source_value: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdvantageTransformManifestV1 {
    identity: String,
    raw_advantage: String,
    source_value_baseline: String,
    centering_and_standardization_weighting: String,
    policy_objective_aggregation: String,
    value_objective_aggregation: String,
    standard_deviation_floor: f64,
    source_advantage_mean: f64,
    source_advantage_population_standard_deviation: f64,
    normalization_denominator: f64,
    policy_scale: f32,
    policy_scale_f32_bits: u32,
    physical_group_count: usize,
    contributing_episode_count: usize,
    zero_decision_episode_count: usize,
    uniform_core_value_weight_sum: f64,
    uniform_core_policy_advantage_sum: f64,
}

#[derive(Clone, Debug)]
struct PreparedAdvantagesV1 {
    terms: Vec<NativePolicyFrozenObjectiveTermV1>,
    manifest: AdvantageTransformManifestV1,
}

fn prepare_standardized_episode_balanced_advantages_v1(
    observations: &[AdvantageObservationV1],
    corpus_episode_count: usize,
    policy_scale: f32,
    source_value_baseline: &'static str,
) -> DynResultV1<PreparedAdvantagesV1> {
    if observations.is_empty()
        || corpus_episode_count == 0
        || !policy_scale.is_finite()
        || policy_scale <= 0.0
    {
        return Err(invalid_data_v1("invalid standardized advantage inputs").into());
    }
    let mut episode_group_counts = BTreeMap::<u64, usize>::new();
    for observation in observations {
        if !matches!(observation.terminal_return, -1..=1) || !observation.source_value.is_finite() {
            return Err(invalid_data_v1("invalid standardized advantage observation").into());
        }
        let count = episode_group_counts
            .entry(observation.episode_id)
            .or_default();
        *count = count
            .checked_add(1)
            .ok_or_else(|| invalid_data_v1("episode physical group count overflow"))?;
    }
    let contributing_episode_count = episode_group_counts.len();
    if contributing_episode_count > corpus_episode_count {
        return Err(invalid_data_v1("advantage episodes exceed corpus terminals").into());
    }
    let group_count = observations.len();
    let group_count_f64 = group_count as f64;
    let episode_count_f64 = contributing_episode_count as f64;
    let mut raw_advantages = Vec::with_capacity(group_count);
    let mut effective_weights = Vec::with_capacity(group_count);
    let mut weighted_raw_sum = 0.0_f64;
    for observation in observations {
        let episode_group_count = *episode_group_counts
            .get(&observation.episode_id)
            .expect("episode count was populated");
        let effective_weight = group_count_f64 / (episode_count_f64 * episode_group_count as f64);
        let raw_advantage =
            f64::from(observation.terminal_return) - f64::from(observation.source_value);
        raw_advantages.push(raw_advantage);
        effective_weights.push(effective_weight);
        weighted_raw_sum += effective_weight * raw_advantage;
    }
    let source_advantage_mean = weighted_raw_sum / group_count_f64;
    let weighted_variance_sum = raw_advantages
        .iter()
        .zip(&effective_weights)
        .map(|(advantage, weight)| {
            let centered = *advantage - source_advantage_mean;
            *weight * centered * centered
        })
        .sum::<f64>();
    let source_advantage_population_standard_deviation =
        (weighted_variance_sum / group_count_f64).sqrt();
    let normalization_denominator =
        source_advantage_population_standard_deviation.max(ADVANTAGE_STANDARD_DEVIATION_FLOOR_V1);
    if !source_advantage_mean.is_finite()
        || !source_advantage_population_standard_deviation.is_finite()
        || !normalization_denominator.is_finite()
    {
        return Err(invalid_data_v1("nonfinite standardized advantage statistics").into());
    }

    let mut terms = Vec::with_capacity(group_count);
    for ((observation, raw_advantage), effective_weight) in observations
        .iter()
        .zip(&raw_advantages)
        .zip(&effective_weights)
    {
        let policy_advantage =
            f64::from(policy_scale) * *effective_weight * (*raw_advantage - source_advantage_mean)
                / normalization_denominator;
        let value_weight = *effective_weight;
        let policy_advantage = policy_advantage as f32;
        let value_weight = value_weight as f32;
        if !policy_advantage.is_finite() || !value_weight.is_finite() || value_weight <= 0.0 {
            return Err(invalid_data_v1("standardized advantage coefficient is not finite").into());
        }
        terms.push(NativePolicyFrozenObjectiveTermV1 {
            policy_advantage,
            value_target: f32::from(observation.terminal_return),
            value_weight,
        });
    }
    let uniform_core_value_weight_sum = terms
        .iter()
        .map(|term| f64::from(term.value_weight))
        .sum::<f64>();
    let uniform_core_policy_advantage_sum = terms
        .iter()
        .map(|term| f64::from(term.policy_advantage))
        .sum::<f64>();
    Ok(PreparedAdvantagesV1 {
        terms,
        manifest: AdvantageTransformManifestV1 {
            identity: STANDARDIZED_EPISODE_BALANCED_TRANSFORM_V1.to_owned(),
            raw_advantage: "candidate_terminal_reward_minus_frozen_source_value".to_owned(),
            source_value_baseline: source_value_baseline.to_owned(),
            centering_and_standardization_weighting:
                "population_moments_with_each_contributing_episode_total_mass_one".to_owned(),
            policy_objective_aggregation: "mean_over_episodes_of_mean_over_episode_physical_groups"
                .to_owned(),
            value_objective_aggregation: "mean_over_episodes_of_mean_over_episode_physical_groups"
                .to_owned(),
            standard_deviation_floor: ADVANTAGE_STANDARD_DEVIATION_FLOOR_V1,
            source_advantage_mean,
            source_advantage_population_standard_deviation,
            normalization_denominator,
            policy_scale,
            policy_scale_f32_bits: policy_scale.to_bits(),
            physical_group_count: group_count,
            contributing_episode_count,
            zero_decision_episode_count: corpus_episode_count - contributing_episode_count,
            uniform_core_value_weight_sum,
            uniform_core_policy_advantage_sum,
        },
    })
}

fn prepare_dataset_advantages_v1(
    dataset: &OutcomeDatasetV1,
    policy_scale: f32,
    parent_source: bool,
) -> DynResultV1<PreparedAdvantagesV1> {
    let observations = dataset
        .groups
        .iter()
        .map(|group| {
            let first = group
                .examples
                .first()
                .ok_or_else(|| invalid_data_v1("empty outcome physical group"))?;
            Ok(AdvantageObservationV1 {
                episode_id: group.episode_id,
                terminal_return: group.terminal_return,
                source_value: f32::from_bits(first.old_value_f32_bits),
            })
        })
        .collect::<DynResultV1<Vec<_>>>()?;
    prepare_standardized_episode_balanced_advantages_v1(
        &observations,
        dataset.episode_count,
        policy_scale,
        if parent_source {
            "exported_loaded_outcome_parent_old_value_f32_bits_first_substep"
        } else {
            "exported_g384_old_value_f32_bits_first_substep"
        },
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ObjectiveMetricsV1 {
    physical_group_count: u64,
    substep_count: u64,
    mean_policy_surrogate: f64,
    mean_value_squared_error: f64,
    mean_total_objective: f64,
    mean_selected_nll_per_physical_group: f64,
    substep_top1_accuracy: f64,
    physical_top1_accuracy: f64,
    mean_absolute_value_error: f64,
    mean_terminal_return: f64,
}

fn selected_log_probability_and_top1_v1(
    logits: &[f32],
    selected: usize,
) -> DynResultV1<(f64, bool)> {
    if logits.is_empty() || selected >= logits.len() {
        return Err(invalid_data_v1("invalid logits for outcome metric").into());
    }
    let mut top1 = 0_usize;
    let mut maximum = logits[0];
    for (index, value) in logits.iter().copied().enumerate().skip(1) {
        if value > maximum {
            maximum = value;
            top1 = index;
        }
    }
    let sum_exp = logits
        .iter()
        .map(|value| f64::from(*value - maximum).exp())
        .sum::<f64>();
    let log_probability = f64::from(logits[selected] - maximum) - sum_exp.ln();
    if !log_probability.is_finite() {
        return Err(invalid_data_v1("nonfinite outcome log probability").into());
    }
    Ok((log_probability, top1 == selected))
}

fn stable_log_probabilities_f64_v1(logits: &[f32]) -> DynResultV1<Vec<f64>> {
    if logits.is_empty() || logits.iter().any(|value| !value.is_finite()) {
        return Err(invalid_data_v1("invalid logits for stable log softmax").into());
    }
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let shifted = logits
        .iter()
        .map(|value| f64::from(*value) - f64::from(maximum))
        .collect::<Vec<_>>();
    let log_normalizer = shifted.iter().map(|value| value.exp()).sum::<f64>().ln();
    let log_probabilities = shifted
        .into_iter()
        .map(|value| value - log_normalizer)
        .collect::<Vec<_>>();
    if !log_normalizer.is_finite() || log_probabilities.iter().any(|value| !value.is_finite()) {
        return Err(invalid_data_v1("nonfinite stable log softmax").into());
    }
    Ok(log_probabilities)
}

fn ppo_joint_likelihood_ratio_v1(
    current_selected_log_probabilities: &[f64],
    old_selected_log_probabilities: &[f64],
) -> DynResultV1<(f64, f64)> {
    if current_selected_log_probabilities.is_empty()
        || current_selected_log_probabilities.len() != old_selected_log_probabilities.len()
        || current_selected_log_probabilities
            .iter()
            .chain(old_selected_log_probabilities)
            .any(|value| !value.is_finite())
    {
        return Err(invalid_data_v1("invalid PPO selected log-probability sequence").into());
    }
    let joint_log_likelihood_ratio = current_selected_log_probabilities
        .iter()
        .zip(old_selected_log_probabilities)
        .map(|(current, old)| current - old)
        .sum::<f64>();
    let likelihood_ratio = joint_log_likelihood_ratio.exp();
    if !joint_log_likelihood_ratio.is_finite()
        || !likelihood_ratio.is_finite()
        || likelihood_ratio <= 0.0
    {
        return Err(invalid_data_v1("nonfinite PPO joint likelihood ratio").into());
    }
    Ok((joint_log_likelihood_ratio, likelihood_ratio))
}

fn ppo_clipped_surrogate_and_coefficient_v1(
    base_policy_advantage: f32,
    likelihood_ratio: f64,
    clip_epsilon: f32,
) -> DynResultV1<(f64, f32, bool)> {
    if !base_policy_advantage.is_finite()
        || !likelihood_ratio.is_finite()
        || likelihood_ratio <= 0.0
        || !clip_epsilon.is_finite()
        || clip_epsilon <= 0.0
        || clip_epsilon >= 1.0
    {
        return Err(invalid_data_v1("invalid PPO clipping input").into());
    }
    let lower = 1.0 - f64::from(clip_epsilon);
    let upper = 1.0 + f64::from(clip_epsilon);
    let advantage = f64::from(base_policy_advantage);
    let clipped_ratio = likelihood_ratio.clamp(lower, upper);
    let surrogate = (likelihood_ratio * advantage).min(clipped_ratio * advantage);
    let clipped = (base_policy_advantage > 0.0 && likelihood_ratio > upper)
        || (base_policy_advantage < 0.0 && likelihood_ratio < lower);
    let effective_coefficient = if clipped {
        0.0
    } else {
        (likelihood_ratio * advantage) as f32
    };
    if !surrogate.is_finite() || !effective_coefficient.is_finite() {
        return Err(invalid_data_v1("nonfinite PPO clipped surrogate").into());
    }
    Ok((surrogate, effective_coefficient, clipped))
}

fn evaluate_objective_v1(
    model: &NativePolicyValueNetV1,
    groups: &[OutcomePhysicalDecisionV1],
    value_coefficient: f32,
) -> DynResultV1<ObjectiveMetricsV1> {
    if groups.is_empty() {
        return Err(invalid_data_v1("outcome metric requires physical groups").into());
    }
    let mut policy_sum = 0.0_f64;
    let mut value_sum = 0.0_f64;
    let mut nll_sum = 0.0_f64;
    let mut value_absolute_sum = 0.0_f64;
    let mut terminal_return_sum = 0_i64;
    let mut substep_count = 0_u64;
    let mut substep_correct = 0_u64;
    let mut physical_correct = 0_u64;
    for group in groups {
        let mut joint_log_probability = 0.0_f64;
        let mut group_top1 = true;
        let mut value = None;
        for example in &group.examples {
            let output = model.forward_v1(example.tensor.view_v1())?;
            if output.logits.len() != example.legal_action_count {
                return Err(invalid_data_v1("outcome metric action width mismatch").into());
            }
            let (selected_log_probability, correct) =
                selected_log_probability_and_top1_v1(&output.logits, example.selected_index)?;
            joint_log_probability += selected_log_probability;
            group_top1 &= correct;
            substep_correct += u64::from(correct);
            substep_count += 1;
            if value.is_none() {
                value = Some(f64::from(output.value));
            }
        }
        let value = value.ok_or_else(|| invalid_data_v1("empty outcome physical group"))?;
        let target = f64::from(group.terminal_return);
        let advantage = target - value;
        policy_sum += -joint_log_probability * advantage;
        value_sum += (value - target) * (value - target);
        nll_sum += -joint_log_probability;
        value_absolute_sum += (value - target).abs();
        terminal_return_sum += i64::from(group.terminal_return);
        physical_correct += u64::from(group_top1);
    }
    let group_count = groups.len() as f64;
    let mean_policy = policy_sum / group_count;
    let mean_value = value_sum / group_count;
    Ok(ObjectiveMetricsV1 {
        physical_group_count: u64::try_from(groups.len())?,
        substep_count,
        mean_policy_surrogate: mean_policy,
        mean_value_squared_error: mean_value,
        mean_total_objective: mean_policy + f64::from(value_coefficient) * mean_value,
        mean_selected_nll_per_physical_group: nll_sum / group_count,
        substep_top1_accuracy: substep_correct as f64 / substep_count as f64,
        physical_top1_accuracy: physical_correct as f64 / group_count,
        mean_absolute_value_error: value_absolute_sum / group_count,
        mean_terminal_return: terminal_return_sum as f64 / group_count,
    })
}

fn evaluate_frozen_objective_v1(
    model: &NativePolicyValueNetV1,
    groups: &[OutcomePhysicalDecisionV1],
    terms: &[NativePolicyFrozenObjectiveTermV1],
    value_coefficient: f32,
) -> DynResultV1<ObjectiveMetricsV1> {
    if groups.is_empty() || groups.len() != terms.len() {
        return Err(invalid_data_v1("frozen outcome metric cardinality mismatch").into());
    }
    let mut policy_sum = 0.0_f64;
    let mut value_sum = 0.0_f64;
    let mut nll_sum = 0.0_f64;
    let mut value_absolute_sum = 0.0_f64;
    let mut terminal_return_sum = 0_i64;
    let mut substep_count = 0_u64;
    let mut substep_correct = 0_u64;
    let mut physical_correct = 0_u64;
    for (group, term) in groups.iter().zip(terms) {
        let mut joint_log_probability = 0.0_f64;
        let mut group_top1 = true;
        let mut value = None;
        for example in &group.examples {
            let output = model.forward_v1(example.tensor.view_v1())?;
            if output.logits.len() != example.legal_action_count {
                return Err(invalid_data_v1("frozen outcome metric action width mismatch").into());
            }
            let (selected_log_probability, correct) =
                selected_log_probability_and_top1_v1(&output.logits, example.selected_index)?;
            joint_log_probability += selected_log_probability;
            group_top1 &= correct;
            substep_correct += u64::from(correct);
            substep_count += 1;
            if value.is_none() {
                value = Some(f64::from(output.value));
            }
        }
        let value = value.ok_or_else(|| invalid_data_v1("empty frozen outcome physical group"))?;
        let target = f64::from(term.value_target);
        let value_weight = f64::from(term.value_weight);
        policy_sum += -joint_log_probability * f64::from(term.policy_advantage);
        value_sum += value_weight * (value - target) * (value - target);
        nll_sum += -joint_log_probability;
        value_absolute_sum += value_weight * (value - target).abs();
        terminal_return_sum += i64::from(group.terminal_return);
        physical_correct += u64::from(group_top1);
    }
    let group_count = groups.len() as f64;
    let mean_policy = policy_sum / group_count;
    let mean_value = value_sum / group_count;
    Ok(ObjectiveMetricsV1 {
        physical_group_count: u64::try_from(groups.len())?,
        substep_count,
        mean_policy_surrogate: mean_policy,
        mean_value_squared_error: mean_value,
        mean_total_objective: mean_policy + f64::from(value_coefficient) * mean_value,
        mean_selected_nll_per_physical_group: nll_sum / group_count,
        substep_top1_accuracy: substep_correct as f64 / substep_count as f64,
        physical_top1_accuracy: physical_correct as f64 / group_count,
        mean_absolute_value_error: value_absolute_sum / group_count,
        mean_terminal_return: terminal_return_sum as f64 / group_count,
    })
}

#[derive(Clone, Debug)]
struct PreparedPpoEpochV1 {
    terms: Vec<NativePolicyFrozenObjectiveTermV1>,
    objective_metrics: ObjectiveMetricsV1,
    ratio_metrics: PpoRatioMetricsManifestV1,
}

fn prepare_ppo_epoch_v1(
    model: &NativePolicyValueNetV1,
    groups: &[OutcomePhysicalDecisionV1],
    base_terms: &[NativePolicyFrozenObjectiveTermV1],
    clip_epsilon: f32,
    value_coefficient: f32,
) -> DynResultV1<PreparedPpoEpochV1> {
    if groups.is_empty() || groups.len() != base_terms.len() {
        return Err(invalid_data_v1("PPO outcome metric cardinality mismatch").into());
    }
    let mut terms = Vec::with_capacity(groups.len());
    let mut policy_sum = 0.0_f64;
    let mut value_sum = 0.0_f64;
    let mut nll_sum = 0.0_f64;
    let mut value_absolute_sum = 0.0_f64;
    let mut terminal_return_sum = 0_i64;
    let mut substep_count = 0_u64;
    let mut substep_correct = 0_u64;
    let mut physical_correct = 0_u64;
    let mut clipped_group_count = 0_usize;
    let mut minimum_likelihood_ratio = f64::INFINITY;
    let mut maximum_likelihood_ratio = 0.0_f64;
    let mut likelihood_ratio_sum = 0.0_f64;
    let mut absolute_log_likelihood_ratio_sum = 0.0_f64;
    let mut maximum_absolute_joint_log_likelihood_ratio = 0.0_f64;
    let mut old_to_current_forward_kl_sum = 0.0_f64;
    let mut action_total_variations = Vec::new();
    for (group, base_term) in groups.iter().zip(base_terms) {
        let mut current_selected_log_probabilities = Vec::with_capacity(group.examples.len());
        let mut old_selected_log_probabilities = Vec::with_capacity(group.examples.len());
        let mut group_top1 = true;
        let mut value = None;
        for example in &group.examples {
            let output = model.forward_v1(example.tensor.view_v1())?;
            if output.logits.len() != example.legal_action_count
                || example.old_policy_logits_f32_bits.len() != example.legal_action_count
            {
                return Err(invalid_data_v1("PPO outcome metric action width mismatch").into());
            }
            let (_, correct) =
                selected_log_probability_and_top1_v1(&output.logits, example.selected_index)?;
            let current_log_probabilities = stable_log_probabilities_f64_v1(&output.logits)?;
            let old_logits = example
                .old_policy_logits_f32_bits
                .iter()
                .copied()
                .map(f32::from_bits)
                .collect::<Vec<_>>();
            let old_log_probabilities = stable_log_probabilities_f64_v1(&old_logits)?;
            let current_selected_log_probability = current_log_probabilities
                .get(example.selected_index)
                .copied()
                .ok_or_else(|| invalid_data_v1("PPO selected current action is out of range"))?;
            let old_selected_log_probability = old_log_probabilities
                .get(example.selected_index)
                .copied()
                .ok_or_else(|| invalid_data_v1("PPO selected old action is out of range"))?;
            let mut row_forward_kl = 0.0_f64;
            let mut row_total_variation = 0.0_f64;
            for (old_log_probability, current_log_probability) in
                old_log_probabilities.iter().zip(&current_log_probabilities)
            {
                let old_probability = old_log_probability.exp();
                let current_probability = current_log_probability.exp();
                row_forward_kl += old_probability * (old_log_probability - current_log_probability);
                row_total_variation += (old_probability - current_probability).abs();
            }
            row_total_variation *= 0.5;
            if !row_forward_kl.is_finite()
                || row_forward_kl < -1.0e-12
                || !row_total_variation.is_finite()
                || !(0.0..=1.0 + 1.0e-12).contains(&row_total_variation)
            {
                return Err(invalid_data_v1("invalid PPO row distribution metric").into());
            }
            old_to_current_forward_kl_sum += row_forward_kl.max(0.0);
            action_total_variations.push(row_total_variation.min(1.0));
            current_selected_log_probabilities.push(current_selected_log_probability);
            old_selected_log_probabilities.push(old_selected_log_probability);
            group_top1 &= correct;
            substep_correct += u64::from(correct);
            substep_count += 1;
            if value.is_none() {
                value = Some(f64::from(output.value));
            }
        }
        let current_joint_log_probability = current_selected_log_probabilities.iter().sum::<f64>();
        let (joint_log_likelihood_ratio, likelihood_ratio) = ppo_joint_likelihood_ratio_v1(
            &current_selected_log_probabilities,
            &old_selected_log_probabilities,
        )?;
        let (surrogate, effective_coefficient, clipped) = ppo_clipped_surrogate_and_coefficient_v1(
            base_term.policy_advantage,
            likelihood_ratio,
            clip_epsilon,
        )?;
        terms.push(NativePolicyFrozenObjectiveTermV1 {
            policy_advantage: effective_coefficient,
            value_target: base_term.value_target,
            value_weight: base_term.value_weight,
        });
        let value = value.ok_or_else(|| invalid_data_v1("empty PPO outcome physical group"))?;
        let target = f64::from(base_term.value_target);
        let value_weight = f64::from(base_term.value_weight);
        policy_sum += -surrogate;
        value_sum += value_weight * (value - target) * (value - target);
        nll_sum += -current_joint_log_probability;
        value_absolute_sum += value_weight * (value - target).abs();
        terminal_return_sum += i64::from(group.terminal_return);
        physical_correct += u64::from(group_top1);
        clipped_group_count += usize::from(clipped);
        minimum_likelihood_ratio = minimum_likelihood_ratio.min(likelihood_ratio);
        maximum_likelihood_ratio = maximum_likelihood_ratio.max(likelihood_ratio);
        likelihood_ratio_sum += likelihood_ratio;
        absolute_log_likelihood_ratio_sum += joint_log_likelihood_ratio.abs();
        maximum_absolute_joint_log_likelihood_ratio =
            maximum_absolute_joint_log_likelihood_ratio.max(joint_log_likelihood_ratio.abs());
    }
    let group_count = groups.len() as f64;
    action_total_variations.sort_by(f64::total_cmp);
    let observed_row_count = action_total_variations.len();
    if observed_row_count == 0 || observed_row_count != usize::try_from(substep_count)? {
        return Err(invalid_data_v1("PPO row metric cardinality mismatch").into());
    }
    let p90_rank = observed_row_count
        .checked_mul(9)
        .and_then(|value| value.checked_add(9))
        .ok_or_else(|| invalid_data_v1("PPO p90 rank overflow"))?
        / 10;
    let p90_action_total_variation_nearest_rank = action_total_variations[p90_rank - 1];
    let mean_action_total_variation =
        action_total_variations.iter().sum::<f64>() / observed_row_count as f64;
    let mean_policy = policy_sum / group_count;
    let mean_value = value_sum / group_count;
    let objective_metrics = ObjectiveMetricsV1 {
        physical_group_count: u64::try_from(groups.len())?,
        substep_count,
        mean_policy_surrogate: mean_policy,
        mean_value_squared_error: mean_value,
        mean_total_objective: mean_policy + f64::from(value_coefficient) * mean_value,
        mean_selected_nll_per_physical_group: nll_sum / group_count,
        substep_top1_accuracy: substep_correct as f64 / substep_count as f64,
        physical_top1_accuracy: physical_correct as f64 / group_count,
        mean_absolute_value_error: value_absolute_sum / group_count,
        mean_terminal_return: terminal_return_sum as f64 / group_count,
    };
    let ratio_metrics = PpoRatioMetricsManifestV1 {
        physical_group_count: groups.len(),
        observed_row_count,
        clipped_group_count,
        minimum_likelihood_ratio,
        maximum_likelihood_ratio,
        mean_likelihood_ratio: likelihood_ratio_sum / group_count,
        mean_absolute_log_likelihood_ratio: absolute_log_likelihood_ratio_sum / group_count,
        maximum_absolute_joint_log_likelihood_ratio,
        mean_old_to_current_forward_kl: old_to_current_forward_kl_sum / observed_row_count as f64,
        mean_action_total_variation,
        p90_action_total_variation_nearest_rank,
        mean_policy_surrogate: mean_policy,
    };
    Ok(PreparedPpoEpochV1 {
        terms,
        objective_metrics,
        ratio_metrics,
    })
}

#[derive(Clone, Debug)]
struct ExpectedForwardBitsV1 {
    logits: Vec<u32>,
    value: u32,
}

fn train_batch_v1(
    state: &mut NativePolicyValueTrainStateV1,
    batch: &[OutcomePhysicalDecisionV1],
    learning_rate: f32,
    value_coefficient: f32,
) -> DynResultV1<()> {
    let expected = batch
        .iter()
        .map(|group| {
            group
                .examples
                .iter()
                .map(|example| {
                    let output = state.model_v1().forward_v1(example.tensor.view_v1())?;
                    Ok(ExpectedForwardBitsV1 {
                        logits: output.logits.iter().map(|value| value.to_bits()).collect(),
                        value: output.value.to_bits(),
                    })
                })
                .collect::<Result<Vec<_>, crate::native_policy_value_net_v1::NativePolicyValueErrorV1>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let substeps = batch
        .iter()
        .zip(&expected)
        .map(|(group, expected)| {
            group
                .examples
                .iter()
                .zip(expected)
                .map(|(example, expected)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        example.tensor.view_v1(),
                    )),
                    selected_action_index: example.selected_index,
                    expected_raw_action_logit_bits: &expected.logits,
                    expected_value_bits: expected.value,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let physical = batch
        .iter()
        .zip(&substeps)
        .map(|(group, substeps)| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: group.terminal_return,
        })
        .collect::<Vec<_>>();
    let result = state.train_step_v1(&physical, value_coefficient, learning_rate)?;
    if result.physical_terms.len() != physical.len()
        || result.adam_step != state.adam_step_v1()
        || !result.loss.is_finite()
        || !result.policy_sum.is_finite()
        || !result.value_sum.is_finite()
    {
        return Err(invalid_data_v1("outcome train step receipt is invalid").into());
    }
    Ok(())
}

fn train_frozen_batch_v1(
    state: &mut NativePolicyValueTrainStateV1,
    batch: &[OutcomePhysicalDecisionV1],
    terms: &[NativePolicyFrozenObjectiveTermV1],
    learning_rate: f32,
    value_coefficient: f32,
) -> DynResultV1<()> {
    if batch.len() != terms.len() {
        return Err(invalid_data_v1("frozen outcome train batch cardinality mismatch").into());
    }
    let expected = batch
        .iter()
        .map(|group| {
            group
                .examples
                .iter()
                .map(|example| {
                    let output = state.model_v1().forward_v1(example.tensor.view_v1())?;
                    Ok(ExpectedForwardBitsV1 {
                        logits: output.logits.iter().map(|value| value.to_bits()).collect(),
                        value: output.value.to_bits(),
                    })
                })
                .collect::<Result<Vec<_>, crate::native_policy_value_net_v1::NativePolicyValueErrorV1>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let substeps = batch
        .iter()
        .zip(&expected)
        .map(|(group, expected)| {
            group
                .examples
                .iter()
                .zip(expected)
                .map(|(example, expected)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        example.tensor.view_v1(),
                    )),
                    selected_action_index: example.selected_index,
                    expected_raw_action_logit_bits: &expected.logits,
                    expected_value_bits: expected.value,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let physical = batch
        .iter()
        .zip(&substeps)
        .map(|(group, substeps)| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: group.terminal_return,
        })
        .collect::<Vec<_>>();
    let result = state.train_step_with_frozen_objective_v1(
        &physical,
        terms,
        value_coefficient,
        learning_rate,
    )?;
    if result.physical_terms.len() != physical.len()
        || result.adam_step != state.adam_step_v1()
        || !result.loss.is_finite()
        || !result.policy_sum.is_finite()
        || !result.value_sum.is_finite()
    {
        return Err(invalid_data_v1("frozen outcome train step receipt is invalid").into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BatchGroupsV1 {
    All,
    Count(usize),
}

impl Default for BatchGroupsV1 {
    fn default() -> Self {
        Self::All
    }
}

impl BatchGroupsV1 {
    fn effective_v1(self, group_count: usize) -> usize {
        match self {
            Self::All => group_count,
            Self::Count(value) => value.min(group_count),
        }
    }

    fn wire_v1(self) -> String {
        match self {
            Self::All => "all".to_owned(),
            Self::Count(value) => value.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
struct TrainConfigV1 {
    outcome_jsonl: PathBuf,
    outcome_jsonl_sha256: String,
    source_store_root: Option<PathBuf>,
    source_outcome_root: Option<PathBuf>,
    output_dir: PathBuf,
    learning_rate: f32,
    value_coefficient: f32,
    advantage_mode: AdvantageModeV1,
    policy_scale: f32,
    ppo_clip_epsilon: Option<f32>,
    epochs: u32,
    batch_groups: BatchGroupsV1,
}

impl Default for TrainConfigV1 {
    fn default() -> Self {
        Self {
            outcome_jsonl: PathBuf::new(),
            outcome_jsonl_sha256: String::new(),
            source_store_root: None,
            source_outcome_root: None,
            output_dir: PathBuf::new(),
            learning_rate: f32::NAN,
            value_coefficient: f32::NAN,
            advantage_mode: AdvantageModeV1::Raw,
            policy_scale: 1.0,
            ppo_clip_epsilon: None,
            epochs: 0,
            batch_groups: BatchGroupsV1::All,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceBindingManifestV1 {
    run_sha256: String,
    generation: u64,
    checkpoint_sha256: String,
    sidecar_sha256: String,
    payload_sha256: String,
    train_state_sha256: String,
    model_parameter_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ParentBindingManifestV1 {
    authority_kind: String,
    manifest_sha256: String,
    payload_sha256: String,
    native_state_sha256: String,
    model_parameter_sha256: String,
    corpus_sha256: String,
    adam_step: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CorpusBindingManifestV1 {
    jsonl_sha256: String,
    export_contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy_parent: Option<ParentBindingManifestV1>,
    selection_source: String,
    model_input_commitment: String,
    decision_row_count: usize,
    terminal_row_count: usize,
    episode_count: usize,
    physical_group_count: usize,
    physical_group_dimensions: Vec<PhysicalGroupDimensionManifestV1>,
    pair_indices: Vec<u64>,
    terminal_return_counts_loss_draw_win: [u64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PhysicalGroupDimensionManifestV1 {
    pair_index: u64,
    episode_id: u64,
    decision_kind: String,
    physical_group_count: usize,
    decision_row_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PpoRatioMetricsManifestV1 {
    physical_group_count: usize,
    observed_row_count: usize,
    clipped_group_count: usize,
    minimum_likelihood_ratio: f64,
    maximum_likelihood_ratio: f64,
    mean_likelihood_ratio: f64,
    mean_absolute_log_likelihood_ratio: f64,
    maximum_absolute_joint_log_likelihood_ratio: f64,
    mean_old_to_current_forward_kl: f64,
    mean_action_total_variation: f64,
    p90_action_total_variation_nearest_rank: f64,
    mean_policy_surrogate: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PpoClipEpochManifestV1 {
    epoch_index: u32,
    adam_step_before: u64,
    adam_step_after: u64,
    before_update: PpoRatioMetricsManifestV1,
    after_update: PpoRatioMetricsManifestV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PpoClipManifestV1 {
    identity: String,
    old_policy_source: String,
    likelihood_ratio_scope: String,
    clipping_scope: String,
    clip_epsilon: f32,
    clip_epsilon_f32_bits: u32,
    initial_maximum_absolute_joint_log_likelihood_ratio_limit: f64,
    initial_maximum_absolute_joint_log_likelihood_ratio_limit_f64_bits: u64,
    epochs: Vec<PpoClipEpochManifestV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrainingManifestV1 {
    objective: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    advantage_transform: Option<AdvantageTransformManifestV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ppo_clip: Option<PpoClipManifestV1>,
    optimizer: String,
    optimizer_reset: bool,
    training_order: String,
    learning_rate: f32,
    learning_rate_f32_bits: u32,
    value_coefficient: f32,
    value_coefficient_f32_bits: u32,
    epochs: u32,
    requested_batch_groups: String,
    effective_batch_groups: usize,
    adam_update_count: u64,
    on_policy_g384_single_update: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    starting_adam_step: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ending_adam_step: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    on_policy_parent_single_update: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PayloadManifestV1 {
    filename: String,
    byte_count: usize,
    payload_sha256: String,
    parameters_sha256: String,
    first_moments_sha256: String,
    second_moments_sha256: String,
    model_parameter_sha256: String,
    native_state_sha256: String,
    adam_step: u64,
    scorer_bias_anchor_f32_bits: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DerivativeManifestV1 {
    schema: String,
    source: SourceBindingManifestV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<ParentBindingManifestV1>,
    corpus: CorpusBindingManifestV1,
    training: TrainingManifestV1,
    source_transport_audit: TransportAuditV1,
    initial_objective_metrics: ObjectiveMetricsV1,
    final_objective_metrics: ObjectiveMetricsV1,
    payload: PayloadManifestV1,
    validation_nonclaim: String,
}

fn ensure_output_absent_v1(path: &Path) -> DynResultV1<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "outcome derivative output already exists",
        )
        .into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn atomic_write_v1(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_data_v1("output filename is not UTF-8"))?;
    let temporary = path.with_file_name(format!(".{filename}.tmp"));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    fs::rename(temporary, path)
}

fn physical_group_dimensions_v1(
    groups: &[OutcomePhysicalDecisionV1],
) -> DynResultV1<Vec<PhysicalGroupDimensionManifestV1>> {
    let mut counts = BTreeMap::<(u64, u64, String), (usize, usize)>::new();
    for group in groups {
        if group.decision_kind.is_empty() || group.examples.is_empty() {
            return Err(invalid_data_v1("outcome physical group dimension is empty").into());
        }
        let entry = counts
            .entry((
                group.pair_index,
                group.episode_id,
                group.decision_kind.clone(),
            ))
            .or_default();
        entry.0 = entry
            .0
            .checked_add(1)
            .ok_or_else(|| invalid_data_v1("physical group dimension count overflow"))?;
        entry.1 = entry
            .1
            .checked_add(group.examples.len())
            .ok_or_else(|| invalid_data_v1("decision row dimension count overflow"))?;
    }
    Ok(counts
        .into_iter()
        .map(
            |(
                (pair_index, episode_id, decision_kind),
                (physical_group_count, decision_row_count),
            )| {
                PhysicalGroupDimensionManifestV1 {
                    pair_index,
                    episode_id,
                    decision_kind,
                    physical_group_count,
                    decision_row_count,
                }
            },
        )
        .collect())
}

fn publish_derivative_v1(
    config: &TrainConfigV1,
    dataset: &OutcomeDatasetV1,
    effective_batch_groups: usize,
    parent: Option<ParentBindingManifestV1>,
    starting_adam_step: u64,
    advantage_transform: Option<AdvantageTransformManifestV1>,
    ppo_clip: Option<PpoClipManifestV1>,
    source_transport_audit: TransportAuditV1,
    initial_objective_metrics: ObjectiveMetricsV1,
    final_objective_metrics: ObjectiveMetricsV1,
    state: &NativePolicyValueTrainStateV1,
) -> DynResultV1<DerivativeManifestV1> {
    fs::create_dir(&config.output_dir)?;
    let physical_group_dimensions = physical_group_dimensions_v1(&dataset.groups)?;
    let snapshot = state.snapshot_v1()?;
    let encoded = encode_native_train_state_payload_v1(&snapshot)?;
    let payload = PayloadManifestV1 {
        filename: DERIVATIVE_PAYLOAD_FILENAME_V1.to_owned(),
        byte_count: encoded.bytes.len(),
        payload_sha256: lower_hex_raw32_v1(encoded.digests.payload_sha256),
        parameters_sha256: lower_hex_raw32_v1(encoded.digests.parameters_sha256),
        first_moments_sha256: lower_hex_raw32_v1(encoded.digests.first_moments_sha256),
        second_moments_sha256: lower_hex_raw32_v1(encoded.digests.second_moments_sha256),
        model_parameter_sha256: lower_hex_raw32_v1(encoded.digests.model_parameter_sha256),
        native_state_sha256: lower_hex_raw32_v1(encoded.digests.native_state_sha256),
        adam_step: snapshot.adam_step,
        scorer_bias_anchor_f32_bits: snapshot.scorer_bias_anchor_bits,
    };
    let updates_per_epoch = dataset.groups.len().div_ceil(effective_batch_groups);
    let adam_update_count = u64::try_from(updates_per_epoch)?
        .checked_mul(u64::from(config.epochs))
        .ok_or_else(|| invalid_data_v1("Adam update count overflow"))?;
    let expected_ending_adam_step = starting_adam_step
        .checked_add(adam_update_count)
        .ok_or_else(|| invalid_data_v1("ending Adam step overflow"))?;
    if payload.adam_step != expected_ending_adam_step {
        return Err(invalid_data_v1("published Adam step disagrees with training schedule").into());
    }
    let parent_single_update = parent.as_ref().map(|_| {
        config.epochs == 1
            && effective_batch_groups == dataset.groups.len()
            && adam_update_count == 1
            && corpus_matches_training_source_v1(dataset, parent.as_ref())
    });
    let manifest = DerivativeManifestV1 {
        schema: XMAGE_CP7_OUTCOME_DERIVATIVE_SCHEMA_V1.to_owned(),
        source: SourceBindingManifestV1 {
            run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
            generation: SOURCE_GENERATION_V1,
            checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
            sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
            payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
            train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
            model_parameter_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1.to_owned(),
        },
        parent: parent.clone(),
        corpus: CorpusBindingManifestV1 {
            jsonl_sha256: dataset.jsonl_sha256.clone(),
            export_contract: dataset.export_contract.clone(),
            schema_version: parent.as_ref().map(|_| dataset.schema_version),
            policy_parent: parent.clone(),
            selection_source: XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1.to_owned(),
            model_input_commitment: MODEL_INPUT_COMMITMENT_V1.to_owned(),
            decision_row_count: dataset.decision_row_count,
            terminal_row_count: dataset.terminal_row_count,
            episode_count: dataset.episode_count,
            physical_group_count: dataset.groups.len(),
            physical_group_dimensions,
            pair_indices: dataset.pair_indices.clone(),
            terminal_return_counts_loss_draw_win: dataset.terminal_return_counts,
        },
        training: TrainingManifestV1 {
            objective: match config.advantage_mode {
                AdvantageModeV1::Raw => TRAINER_ALGORITHM_V1.to_owned(),
                AdvantageModeV1::StandardizedEpisodeBalanced => {
                    STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1.to_owned()
                }
                AdvantageModeV1::PpoClipStandardizedEpisodeBalanced => {
                    PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1.to_owned()
                }
            },
            advantage_transform,
            ppo_clip,
            optimizer: OPTIMIZER_V1.to_owned(),
            optimizer_reset: parent.is_none(),
            training_order: TRAINING_ORDER_V1.to_owned(),
            learning_rate: config.learning_rate,
            learning_rate_f32_bits: config.learning_rate.to_bits(),
            value_coefficient: config.value_coefficient,
            value_coefficient_f32_bits: config.value_coefficient.to_bits(),
            epochs: config.epochs,
            requested_batch_groups: config.batch_groups.wire_v1(),
            effective_batch_groups,
            adam_update_count,
            on_policy_g384_single_update: parent.is_none()
                && config.epochs == 1
                && effective_batch_groups == dataset.groups.len()
                && adam_update_count == 1,
            starting_adam_step: parent.as_ref().map(|_| starting_adam_step),
            ending_adam_step: parent.as_ref().map(|_| expected_ending_adam_step),
            on_policy_parent_single_update: parent_single_update,
        },
        source_transport_audit,
        initial_objective_metrics,
        final_objective_metrics,
        payload,
        validation_nonclaim: "offline XMage outcome optimization is not an independent play-strength estimate; any derivative requires a fresh matched CP7 evaluation".to_owned(),
    };
    atomic_write_v1(
        &config.output_dir.join(DERIVATIVE_PAYLOAD_FILENAME_V1),
        &encoded.bytes,
    )?;
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    manifest_bytes.push(b'\n');
    atomic_write_v1(
        &config.output_dir.join(DERIVATIVE_MANIFEST_FILENAME_V1),
        &manifest_bytes,
    )?;
    Ok(manifest)
}

fn expected_payload_digests_v1(
    payload: &PayloadManifestV1,
) -> DynResultV1<NativeTrainStatePayloadDigestsV1> {
    Ok(NativeTrainStatePayloadDigestsV1 {
        payload_sha256: parse_lower_hex_raw32_v1(&payload.payload_sha256)?,
        parameters_sha256: parse_lower_hex_raw32_v1(&payload.parameters_sha256)?,
        first_moments_sha256: parse_lower_hex_raw32_v1(&payload.first_moments_sha256)?,
        second_moments_sha256: parse_lower_hex_raw32_v1(&payload.second_moments_sha256)?,
        model_parameter_sha256: parse_lower_hex_raw32_v1(&payload.model_parameter_sha256)?,
        native_state_sha256: parse_lower_hex_raw32_v1(&payload.native_state_sha256)?,
    })
}

fn validate_metric_v1(
    metric: &ObjectiveMetricsV1,
    corpus: &CorpusBindingManifestV1,
    value_coefficient: f32,
) -> bool {
    metric.physical_group_count == corpus.physical_group_count as u64
        && metric.substep_count == corpus.decision_row_count as u64
        && metric.physical_group_count > 0
        && metric.substep_count > 0
        && [
            metric.mean_policy_surrogate,
            metric.mean_value_squared_error,
            metric.mean_total_objective,
            metric.mean_selected_nll_per_physical_group,
            metric.substep_top1_accuracy,
            metric.physical_top1_accuracy,
            metric.mean_absolute_value_error,
            metric.mean_terminal_return,
        ]
        .into_iter()
        .all(f64::is_finite)
        && (0.0..=1.0).contains(&metric.substep_top1_accuracy)
        && (0.0..=1.0).contains(&metric.physical_top1_accuracy)
        && metric.mean_value_squared_error >= 0.0
        && metric.mean_selected_nll_per_physical_group >= 0.0
        && metric.mean_absolute_value_error >= 0.0
        && (-1.0..=1.0).contains(&metric.mean_terminal_return)
        && metric.mean_total_objective.to_bits()
            == (metric.mean_policy_surrogate
                + f64::from(value_coefficient) * metric.mean_value_squared_error)
                .to_bits()
}

fn validate_group_dimensions_v1(corpus: &CorpusBindingManifestV1) -> bool {
    if corpus.physical_group_dimensions.is_empty() {
        return false;
    }
    let mut physical_group_count = 0_usize;
    let mut decision_row_count = 0_usize;
    let mut episode_pairs = BTreeMap::<u64, u64>::new();
    let mut previous = None::<(u64, u64, &str)>;
    for dimension in &corpus.physical_group_dimensions {
        let key = (
            dimension.pair_index,
            dimension.episode_id,
            dimension.decision_kind.as_str(),
        );
        if previous.is_some_and(|value| value >= key)
            || dimension.decision_kind.is_empty()
            || dimension.decision_kind.chars().any(char::is_control)
            || dimension.physical_group_count == 0
            || dimension.decision_row_count < dimension.physical_group_count
            || corpus
                .pair_indices
                .binary_search(&dimension.pair_index)
                .is_err()
        {
            return false;
        }
        previous = Some(key);
        if episode_pairs
            .insert(dimension.episode_id, dimension.pair_index)
            .is_some_and(|pair_index| pair_index != dimension.pair_index)
        {
            return false;
        }
        let Some(next_group_count) =
            physical_group_count.checked_add(dimension.physical_group_count)
        else {
            return false;
        };
        physical_group_count = next_group_count;
        let Some(next_row_count) = decision_row_count.checked_add(dimension.decision_row_count)
        else {
            return false;
        };
        decision_row_count = next_row_count;
    }
    physical_group_count == corpus.physical_group_count
        && decision_row_count == corpus.decision_row_count
        && episode_pairs.len() <= corpus.episode_count
}

fn validate_advantage_transform_v1(
    transform: &AdvantageTransformManifestV1,
    corpus: &CorpusBindingManifestV1,
    parent_source: bool,
) -> bool {
    let contributing_episode_count = corpus
        .physical_group_dimensions
        .iter()
        .map(|dimension| dimension.episode_id)
        .collect::<BTreeSet<_>>()
        .len();
    let expected_zero_decision_episode_count =
        corpus.episode_count.checked_sub(contributing_episode_count);
    let expected_value_weight_sum = corpus.physical_group_count as f64;
    let value_weight_tolerance = 1.0e-6 * expected_value_weight_sum.max(1.0);
    let policy_sum_tolerance =
        1.0e-5 * expected_value_weight_sum.max(1.0) * f64::from(transform.policy_scale).max(1.0);
    transform.identity == STANDARDIZED_EPISODE_BALANCED_TRANSFORM_V1
        && transform.raw_advantage == "candidate_terminal_reward_minus_frozen_source_value"
        && transform.source_value_baseline
            == if parent_source {
                "exported_loaded_outcome_parent_old_value_f32_bits_first_substep"
            } else {
                "exported_g384_old_value_f32_bits_first_substep"
            }
        && transform.centering_and_standardization_weighting
            == "population_moments_with_each_contributing_episode_total_mass_one"
        && transform.policy_objective_aggregation
            == "mean_over_episodes_of_mean_over_episode_physical_groups"
        && transform.value_objective_aggregation
            == "mean_over_episodes_of_mean_over_episode_physical_groups"
        && transform.standard_deviation_floor.to_bits()
            == ADVANTAGE_STANDARD_DEVIATION_FLOOR_V1.to_bits()
        && transform.source_advantage_mean.is_finite()
        && transform
            .source_advantage_population_standard_deviation
            .is_finite()
        && transform.source_advantage_population_standard_deviation >= 0.0
        && transform.normalization_denominator.to_bits()
            == transform
                .source_advantage_population_standard_deviation
                .max(ADVANTAGE_STANDARD_DEVIATION_FLOOR_V1)
                .to_bits()
        && transform.policy_scale.to_bits() == transform.policy_scale_f32_bits
        && transform.policy_scale.is_finite()
        && transform.policy_scale > 0.0
        && transform.policy_scale <= 100.0
        && transform.physical_group_count == corpus.physical_group_count
        && transform.contributing_episode_count == contributing_episode_count
        && transform.contributing_episode_count > 0
        && Some(transform.zero_decision_episode_count) == expected_zero_decision_episode_count
        && transform.uniform_core_value_weight_sum.is_finite()
        && (transform.uniform_core_value_weight_sum - expected_value_weight_sum).abs()
            <= value_weight_tolerance
        && transform.uniform_core_policy_advantage_sum.is_finite()
        && transform.uniform_core_policy_advantage_sum.abs() <= policy_sum_tolerance
}

fn validate_ppo_ratio_metrics_v1(
    metrics: &PpoRatioMetricsManifestV1,
    corpus: &CorpusBindingManifestV1,
) -> bool {
    let maximum_ratio_from_log = metrics.maximum_absolute_joint_log_likelihood_ratio.exp();
    let minimum_ratio_from_log = (-metrics.maximum_absolute_joint_log_likelihood_ratio).exp();
    let ratio_tolerance = 1.0e-12;
    metrics.physical_group_count == corpus.physical_group_count
        && metrics.observed_row_count == corpus.decision_row_count
        && metrics.clipped_group_count <= metrics.physical_group_count
        && metrics.minimum_likelihood_ratio.is_finite()
        && metrics.maximum_likelihood_ratio.is_finite()
        && metrics.mean_likelihood_ratio.is_finite()
        && metrics.mean_absolute_log_likelihood_ratio.is_finite()
        && metrics
            .maximum_absolute_joint_log_likelihood_ratio
            .is_finite()
        && metrics.mean_old_to_current_forward_kl.is_finite()
        && metrics.mean_action_total_variation.is_finite()
        && metrics.p90_action_total_variation_nearest_rank.is_finite()
        && metrics.mean_policy_surrogate.is_finite()
        && metrics.minimum_likelihood_ratio > 0.0
        && metrics.minimum_likelihood_ratio <= metrics.mean_likelihood_ratio
        && metrics.mean_likelihood_ratio <= metrics.maximum_likelihood_ratio
        && metrics.mean_absolute_log_likelihood_ratio >= 0.0
        && metrics.mean_absolute_log_likelihood_ratio
            <= metrics.maximum_absolute_joint_log_likelihood_ratio
        && metrics.maximum_absolute_joint_log_likelihood_ratio >= 0.0
        && maximum_ratio_from_log.is_finite()
        && minimum_ratio_from_log.is_finite()
        && metrics.minimum_likelihood_ratio >= minimum_ratio_from_log * (1.0 - ratio_tolerance)
        && metrics.maximum_likelihood_ratio <= maximum_ratio_from_log * (1.0 + ratio_tolerance)
        && metrics.mean_old_to_current_forward_kl >= 0.0
        && (0.0..=1.0).contains(&metrics.mean_action_total_variation)
        && (0.0..=1.0).contains(&metrics.p90_action_total_variation_nearest_rank)
}

fn validate_ppo_clip_manifest_v1(
    ppo: &PpoClipManifestV1,
    corpus: &CorpusBindingManifestV1,
    training: &TrainingManifestV1,
    initial_objective_metrics: &ObjectiveMetricsV1,
    final_objective_metrics: &ObjectiveMetricsV1,
    parent_adam_step: u64,
) -> bool {
    if ppo.identity != PPO_CLIP_TRANSFORM_V1
        || ppo.old_policy_source != PPO_OLD_POLICY_SOURCE_V1
        || ppo.likelihood_ratio_scope != PPO_LIKELIHOOD_RATIO_SCOPE_V1
        || ppo.clipping_scope != PPO_CLIPPING_SCOPE_V1
        || ppo.clip_epsilon.to_bits() != ppo.clip_epsilon_f32_bits
        || !ppo.clip_epsilon.is_finite()
        || ppo.clip_epsilon <= 0.0
        || ppo.clip_epsilon >= 1.0
        || ppo
            .initial_maximum_absolute_joint_log_likelihood_ratio_limit
            .to_bits()
            != ppo.initial_maximum_absolute_joint_log_likelihood_ratio_limit_f64_bits
        || ppo
            .initial_maximum_absolute_joint_log_likelihood_ratio_limit
            .to_bits()
            != PPO_INITIAL_MAX_ABSOLUTE_JOINT_LOG_RATIO_V1.to_bits()
        || ppo.epochs.len() != training.epochs as usize
        || ppo.epochs.is_empty()
    {
        return false;
    }
    let mut expected_adam_step = parent_adam_step;
    let mut previous_after = None::<&PpoRatioMetricsManifestV1>;
    for (index, epoch) in ppo.epochs.iter().enumerate() {
        let Some(expected_after) = expected_adam_step.checked_add(1) else {
            return false;
        };
        if epoch.epoch_index != u32::try_from(index + 1).unwrap_or(u32::MAX)
            || epoch.adam_step_before != expected_adam_step
            || epoch.adam_step_after != expected_after
            || !validate_ppo_ratio_metrics_v1(&epoch.before_update, corpus)
            || !validate_ppo_ratio_metrics_v1(&epoch.after_update, corpus)
            || previous_after.is_some_and(|previous| previous != &epoch.before_update)
        {
            return false;
        }
        previous_after = Some(&epoch.after_update);
        expected_adam_step = expected_after;
    }
    let Some(first) = ppo.epochs.first() else {
        return false;
    };
    let Some(last) = ppo.epochs.last() else {
        return false;
    };
    Some(expected_adam_step) == training.ending_adam_step
        && first
            .before_update
            .maximum_absolute_joint_log_likelihood_ratio
            <= ppo.initial_maximum_absolute_joint_log_likelihood_ratio_limit
        && first.before_update.mean_policy_surrogate.to_bits()
            == initial_objective_metrics.mean_policy_surrogate.to_bits()
        && last.after_update.mean_policy_surrogate.to_bits()
            == final_objective_metrics.mean_policy_surrogate.to_bits()
}

fn validate_parent_binding_v1(parent: &ParentBindingManifestV1) -> bool {
    parent.authority_kind == "xmage-cp7-outcome-reinforce-derivative-v1"
        && parent.adam_step > 0
        && [
            &parent.manifest_sha256,
            &parent.payload_sha256,
            &parent.native_state_sha256,
            &parent.model_parameter_sha256,
            &parent.corpus_sha256,
        ]
        .into_iter()
        .all(|value| valid_lower_hex_v1(value, 64))
}

fn load_derivative_bundle_v1(
    directory: &Path,
) -> DynResultV1<(
    NativePolicyValueTrainStateV1,
    DerivativeManifestV1,
    [u8; 32],
)> {
    let mut inventory = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(invalid_data_v1("outcome derivative contains a non-file entry").into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid_data_v1("outcome derivative filename is not UTF-8"))?;
        inventory.insert(name);
    }
    let expected_inventory = BTreeSet::from([
        DERIVATIVE_MANIFEST_FILENAME_V1.to_owned(),
        DERIVATIVE_PAYLOAD_FILENAME_V1.to_owned(),
    ]);
    if inventory != expected_inventory {
        return Err(invalid_data_v1("outcome derivative inventory is not exact").into());
    }

    let manifest_bytes = fs::read(directory.join(DERIVATIVE_MANIFEST_FILENAME_V1))?;
    if !manifest_bytes.ends_with(b"\n") || manifest_bytes.contains(&b'\r') {
        return Err(invalid_data_v1("outcome derivative manifest framing is invalid").into());
    }
    let manifest_text = std::str::from_utf8(&manifest_bytes)?;
    let manifest_value = parse_strict_json_value(manifest_text)?;
    let manifest: DerivativeManifestV1 = serde_json::from_value(manifest_value)?;
    let mut canonical = serde_json::to_vec_pretty(&manifest)?;
    canonical.push(b'\n');
    if canonical != manifest_bytes {
        return Err(invalid_data_v1("outcome derivative manifest is not canonical").into());
    }
    let manifest_sha256 = sha256_v1(&manifest_bytes);

    let source = &manifest.source;
    let corpus = &manifest.corpus;
    let training = &manifest.training;
    let payload = &manifest.payload;
    let parent = manifest.parent.as_ref();
    let source_valid = source.run_sha256 == SOURCE_RUN_SHA256_V1
        && source.generation == SOURCE_GENERATION_V1
        && source.checkpoint_sha256 == SOURCE_CHECKPOINT_SHA256_V1
        && source.sidecar_sha256 == SOURCE_SIDECAR_SHA256_V1
        && source.payload_sha256 == SOURCE_PAYLOAD_SHA256_V1
        && source.train_state_sha256 == SOURCE_TRAIN_STATE_SHA256_V1
        && source.model_parameter_sha256 == SOURCE_MODEL_PARAMETER_SHA256_V1;
    let corpus_authority_valid = match parent {
        None => {
            corpus.export_contract == XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1
                && corpus.schema_version.is_none()
                && corpus.policy_parent.is_none()
        }
        Some(parent) => {
            validate_parent_binding_v1(parent)
                && corpus.export_contract == XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2
                && corpus.schema_version == Some(2)
                && corpus.policy_parent.as_ref() == Some(parent)
        }
    };
    let corpus_valid = valid_lower_hex_v1(&corpus.jsonl_sha256, 64)
        && corpus_authority_valid
        && corpus.selection_source == XMAGE_CP7_OUTCOME_SELECTION_SOURCE_V1
        && corpus.model_input_commitment == MODEL_INPUT_COMMITMENT_V1
        && corpus.decision_row_count > 0
        && corpus.terminal_row_count > 0
        && corpus.terminal_row_count == corpus.episode_count
        && corpus.episode_count == corpus.pair_indices.len().saturating_mul(2)
        && corpus.physical_group_count > 0
        && corpus.physical_group_count <= corpus.decision_row_count
        && corpus
            .terminal_return_counts_loss_draw_win
            .iter()
            .copied()
            .sum::<u64>()
            == corpus.episode_count as u64
        && corpus.pair_indices.windows(2).all(|pair| pair[0] < pair[1]);
    let requested_batch_valid = training.requested_batch_groups == "all"
        || training
            .requested_batch_groups
            .parse::<usize>()
            .ok()
            .is_some_and(|value| value > 0 && value.to_string() == training.requested_batch_groups);
    let requested_effective = if training.requested_batch_groups == "all" {
        Some(corpus.physical_group_count)
    } else {
        training
            .requested_batch_groups
            .parse::<usize>()
            .ok()
            .map(|value| value.min(corpus.physical_group_count))
    };
    let expected_updates = requested_effective.and_then(|batch| {
        u64::try_from(corpus.physical_group_count.div_ceil(batch))
            .ok()?
            .checked_mul(u64::from(training.epochs))
    });
    let starting_adam_step = parent.map_or(0, |binding| binding.adam_step);
    let expected_ending_adam_step =
        expected_updates.and_then(|updates| starting_adam_step.checked_add(updates));
    let single_update = training.epochs == 1
        && training.effective_batch_groups == corpus.physical_group_count
        && training.adam_update_count == 1;
    let objective_valid = match (
        training.objective.as_str(),
        training.advantage_transform.as_ref(),
        training.ppo_clip.as_ref(),
    ) {
        (TRAINER_ALGORITHM_V1, None, None) => true,
        (STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1, Some(transform), None) => {
            training.epochs == 1
                && training.requested_batch_groups == "all"
                && training.effective_batch_groups == corpus.physical_group_count
                && training.adam_update_count == 1
                && validate_advantage_transform_v1(transform, corpus, parent.is_some())
        }
        (PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1, Some(transform), Some(ppo)) => {
            parent.is_some()
                && training.epochs >= 2
                && training.requested_batch_groups == "all"
                && training.effective_batch_groups == corpus.physical_group_count
                && training.adam_update_count == u64::from(training.epochs)
                && transform.policy_scale.to_bits() == 1.0_f32.to_bits()
                && validate_advantage_transform_v1(transform, corpus, true)
                && validate_ppo_clip_manifest_v1(
                    ppo,
                    corpus,
                    training,
                    &manifest.initial_objective_metrics,
                    &manifest.final_objective_metrics,
                    starting_adam_step,
                )
        }
        _ => false,
    };
    let ppo_parent_multi_update = training.objective
        == PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_OBJECTIVE_V1
        && training.epochs >= 2
        && training.requested_batch_groups == "all"
        && training.effective_batch_groups == corpus.physical_group_count
        && training.adam_update_count == u64::from(training.epochs);
    let source_schedule_valid = match parent {
        None => {
            training.optimizer_reset
                && training.starting_adam_step.is_none()
                && training.ending_adam_step.is_none()
                && training.on_policy_parent_single_update.is_none()
                && training.on_policy_g384_single_update == single_update
        }
        Some(binding) => {
            (single_update || ppo_parent_multi_update)
                && !training.optimizer_reset
                && training.starting_adam_step == Some(binding.adam_step)
                && training.ending_adam_step == expected_ending_adam_step
                && training.on_policy_parent_single_update == Some(single_update)
                && !training.on_policy_g384_single_update
        }
    };
    let training_valid = objective_valid
        && training.optimizer == OPTIMIZER_V1
        && source_schedule_valid
        && training.training_order == TRAINING_ORDER_V1
        && training.learning_rate.to_bits() == training.learning_rate_f32_bits
        && training.learning_rate.is_finite()
        && training.learning_rate > 0.0
        && training.learning_rate <= 1.0e-3
        && training.value_coefficient.to_bits() == training.value_coefficient_f32_bits
        && training.value_coefficient.is_finite()
        && training.value_coefficient > 0.0
        && training.value_coefficient <= 10.0
        && training.epochs > 0
        && requested_batch_valid
        && requested_effective == Some(training.effective_batch_groups)
        && expected_updates == Some(training.adam_update_count);
    let transport = &manifest.source_transport_audit;
    let transport_valid = transport.identity
        == if parent.is_some() {
            "exported-loaded-outcome-parent-forward-envelope/v1"
        } else {
            "exported-g384-forward-envelope/v1"
        }
        && transport.decision_row_count == corpus.decision_row_count as u64
        && transport.bit_exact_decision_row_count + transport.mismatched_decision_row_count
            == transport.decision_row_count
        && transport.absolute_tolerance.to_bits() == TRANSPORT_ABSOLUTE_TOLERANCE_V1.to_bits()
        && transport.relative_tolerance.to_bits() == TRANSPORT_RELATIVE_TOLERANCE_V1.to_bits()
        && transport.max_value_absolute_delta.is_finite()
        && transport.max_logit_absolute_delta.is_finite()
        && transport.max_value_absolute_delta >= 0.0
        && transport.max_logit_absolute_delta >= 0.0;
    let payload_valid = payload.filename == DERIVATIVE_PAYLOAD_FILENAME_V1
        && payload.byte_count == NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1
        && Some(payload.adam_step) == expected_ending_adam_step
        && [
            &payload.payload_sha256,
            &payload.parameters_sha256,
            &payload.first_moments_sha256,
            &payload.second_moments_sha256,
            &payload.model_parameter_sha256,
            &payload.native_state_sha256,
        ]
        .into_iter()
        .all(|value| valid_lower_hex_v1(value, 64));
    if manifest.schema != XMAGE_CP7_OUTCOME_DERIVATIVE_SCHEMA_V1
        || !source_valid
        || !corpus_valid
        || !training_valid
        || !transport_valid
        || !payload_valid
        || !validate_group_dimensions_v1(corpus)
        || !validate_metric_v1(
            &manifest.initial_objective_metrics,
            corpus,
            training.value_coefficient,
        )
        || !validate_metric_v1(
            &manifest.final_objective_metrics,
            corpus,
            training.value_coefficient,
        )
    {
        return Err(invalid_data_v1("invalid outcome derivative manifest binding").into());
    }

    let payload_bytes = fs::read(directory.join(&payload.filename))?;
    if payload_bytes.len() != payload.byte_count
        || sha256_v1(&payload_bytes) != parse_lower_hex_raw32_v1(&payload.payload_sha256)?
    {
        return Err(invalid_data_v1("outcome derivative payload digest mismatch").into());
    }
    let expected_digests = expected_payload_digests_v1(payload)?;
    let decoded = decode_native_train_state_payload_verified_v1(
        &payload_bytes,
        payload.adam_step,
        payload.scorer_bias_anchor_f32_bits,
        &expected_digests,
    )?;
    let mut template =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    template.replace_parameter_snapshot_v1(&decoded.snapshot.parameters)?;
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(template, &decoded.snapshot)?;
    if state.adam_step_v1() != payload.adam_step
        || state.model_v1().parameter_manifest_sha256_v1() != payload.model_parameter_sha256
        || state.state_sha256_v1()? != expected_digests.native_state_sha256
    {
        return Err(invalid_data_v1("loaded outcome derivative semantic digest mismatch").into());
    }
    Ok((state, manifest, manifest_sha256))
}

struct LoadedTrainingSourceV1 {
    state: NativePolicyValueTrainStateV1,
    parent: Option<ParentBindingManifestV1>,
}

fn load_training_source_v1(config: &TrainConfigV1) -> DynResultV1<LoadedTrainingSourceV1> {
    match (&config.source_store_root, &config.source_outcome_root) {
        (Some(root), None) => Ok(LoadedTrainingSourceV1 {
            state: load_source_train_state_v1(root)?,
            parent: None,
        }),
        (None, Some(root)) => {
            let (state, manifest, manifest_sha256) = load_derivative_bundle_v1(root)?;
            let parent = ParentBindingManifestV1 {
                authority_kind: "xmage-cp7-outcome-reinforce-derivative-v1".to_owned(),
                manifest_sha256: lower_hex_raw32_v1(manifest_sha256),
                payload_sha256: manifest.payload.payload_sha256.clone(),
                native_state_sha256: manifest.payload.native_state_sha256.clone(),
                model_parameter_sha256: manifest.payload.model_parameter_sha256.clone(),
                corpus_sha256: manifest.corpus.jsonl_sha256.clone(),
                adam_step: manifest.payload.adam_step,
            };
            if state.adam_step_v1() != parent.adam_step
                || lower_hex_raw32_v1(state.state_sha256_v1()?) != parent.native_state_sha256
                || state.model_v1().parameter_manifest_sha256_v1() != parent.model_parameter_sha256
            {
                return Err(invalid_data_v1("loaded outcome parent state binding mismatch").into());
            }
            Ok(LoadedTrainingSourceV1 {
                state,
                parent: Some(parent),
            })
        }
        _ => Err(invalid_data_v1(
            "exactly one of --source-store-root or --source-outcome-root is required",
        )
        .into()),
    }
}

fn corpus_matches_training_source_v1(
    dataset: &OutcomeDatasetV1,
    parent: Option<&ParentBindingManifestV1>,
) -> bool {
    match parent {
        None => {
            dataset.schema_version == 1
                && dataset.export_contract == XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V1
                && exact_g384_checkpoint_v1(&dataset.policy_checkpoint)
        }
        Some(parent) => {
            let checkpoint = &dataset.policy_checkpoint;
            dataset.schema_version == 2
                && dataset.export_contract == XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2
                && outcome_parent_checkpoint_v1(checkpoint)
                && checkpoint.authority_kind == parent.authority_kind
                && checkpoint.loaded_generation == parent.adam_step
                && checkpoint.loaded_checkpoint_sha256 == parent.manifest_sha256
                && checkpoint.loaded_payload_sha256 == parent.payload_sha256
                && checkpoint.loaded_train_state_sha256 == parent.native_state_sha256
                && checkpoint.model_parameter_sha256 == parent.model_parameter_sha256
        }
    }
}

fn flat_tensor_view_v1(tensor: &NativeFlatDecisionTensorV2) -> NativeEncodedDecisionViewV1<'_> {
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

pub(crate) struct NativeXmageCp7OutcomeInferenceOutputV1 {
    logits: Vec<f32>,
    value: f32,
}

impl NativeXmageCp7OutcomeInferenceOutputV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        &self.logits
    }

    pub(crate) fn value_v1(&self) -> f32 {
        self.value
    }
}

/// Frozen-parent scoring output with the exact encoder activations consumed by
/// a residual policy head. `action_hidden` is row-major
/// `[logits.len(), HIDDEN_DIM_V1]`.
pub(crate) struct NativeXmageCp7OutcomeInferenceLatentOutputV1 {
    output: NativeXmageCp7OutcomeInferenceOutputV1,
    state_hidden: [f32; HIDDEN_DIM_V1],
    action_hidden: Vec<f32>,
}

impl NativeXmageCp7OutcomeInferenceLatentOutputV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        self.output.logits_v1()
    }

    pub(crate) fn value_v1(&self) -> f32 {
        self.output.value_v1()
    }

    pub(crate) fn state_hidden_v1(&self) -> &[f32; HIDDEN_DIM_V1] {
        &self.state_hidden
    }

    pub(crate) fn action_hidden_v1(&self) -> &[f32] {
        &self.action_hidden
    }
}

/// One strict outcome-corpus substep converted to frozen-parent features.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeXmageCp7OutcomeBilinearSubstepV1 {
    pub(crate) parent_logits: Vec<f32>,
    pub(crate) parent_value: f32,
    pub(crate) selected_index: usize,
    pub(crate) state_hidden: [f32; HIDDEN_DIM_V1],
    pub(crate) action_hidden: Vec<f32>,
}

/// One autoregressive physical decision. The first-substep exported value is
/// retained separately because existing episode-balanced objectives use it as
/// their frozen baseline.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeXmageCp7OutcomeBilinearGroupV1 {
    pub(crate) pair_index: u64,
    pub(crate) episode_id: u64,
    pub(crate) candidate_seat: PlayerSeatV1,
    pub(crate) terminal_return: i8,
    pub(crate) decision_kind: String,
    pub(crate) first_substep_old_value: f32,
    pub(crate) substeps: Vec<NativeXmageCp7OutcomeBilinearSubstepV1>,
}

/// Strictly validated corpus metadata and frozen-parent latent rows for a
/// bilinear residual trainer. Hashes are raw SHA-256 bytes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeXmageCp7OutcomeBilinearDatasetV1 {
    pub(crate) jsonl_sha256: [u8; 32],
    pub(crate) export_contract: String,
    pub(crate) schema_version: u32,
    pub(crate) decision_row_count: usize,
    pub(crate) terminal_row_count: usize,
    pub(crate) episode_count: usize,
    pub(crate) pair_count: usize,
    pub(crate) pair_indices: Vec<u64>,
    pub(crate) physical_group_count: usize,
    pub(crate) terminal_return_counts: [u64; 3],
    pub(crate) parent_manifest_sha256: [u8; 32],
    pub(crate) parent_payload_sha256: [u8; 32],
    pub(crate) parent_native_state_sha256: [u8; 32],
    pub(crate) parent_model_parameter_sha256: [u8; 32],
    pub(crate) parent_corpus_sha256: [u8; 32],
    pub(crate) parent_adam_step: u64,
    pub(crate) groups: Vec<NativeXmageCp7OutcomeBilinearGroupV1>,
}

/// Move-only immutable scorer loaded through the strict outcome derivative
/// manifest and raw-state verification boundary.
pub(crate) struct NativeXmageCp7OutcomeInferenceV1 {
    state: NativePolicyValueTrainStateV1,
    manifest_sha256: [u8; 32],
    payload_sha256: [u8; 32],
    native_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    corpus_sha256: [u8; 32],
    adam_step: u64,
}

impl NativeXmageCp7OutcomeInferenceV1 {
    pub(crate) fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeXmageCp7OutcomeInferenceOutputV1, ()> {
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
        let output = self
            .state
            .model_v1()
            .forward_v1(flat_tensor_view_v1(&tensor))
            .map_err(|_| ())?;
        if output.logits.len() != decision.actions().len()
            || output.logits.is_empty()
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err(());
        }
        Ok(NativeXmageCp7OutcomeInferenceOutputV1 {
            logits: output.logits,
            value: output.value,
        })
    }

    pub(crate) fn score_decision_with_latents_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeXmageCp7OutcomeInferenceLatentOutputV1, ()> {
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
        let latent = self
            .state
            .model_v1()
            .forward_with_latents_v1(flat_tensor_view_v1(&tensor))
            .map_err(|_| ())?;
        let expected_action_hidden = decision
            .actions()
            .len()
            .checked_mul(HIDDEN_DIM_V1)
            .ok_or(())?;
        if latent.output.logits.len() != decision.actions().len()
            || latent.output.logits.is_empty()
            || latent.output.logits.iter().any(|value| !value.is_finite())
            || !latent.output.value.is_finite()
            || latent
                .state_hidden
                .iter()
                .chain(&latent.action_hidden)
                .any(|value| !value.is_finite())
            || latent.action_hidden.len() != expected_action_hidden
        {
            return Err(());
        }
        Ok(NativeXmageCp7OutcomeInferenceLatentOutputV1 {
            output: NativeXmageCp7OutcomeInferenceOutputV1 {
                logits: latent.output.logits,
                value: latent.output.value,
            },
            state_hidden: latent.state_hidden,
            action_hidden: latent.action_hidden,
        })
    }

    pub(crate) const fn manifest_sha256_v1(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) const fn payload_sha256_v1(&self) -> [u8; 32] {
        self.payload_sha256
    }

    pub(crate) const fn native_state_sha256_v1(&self) -> [u8; 32] {
        self.native_state_sha256
    }

    pub(crate) const fn model_parameter_sha256_v1(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub(crate) const fn corpus_sha256_v1(&self) -> [u8; 32] {
        self.corpus_sha256
    }

    pub(crate) const fn adam_step_v1(&self) -> u64 {
        self.adam_step
    }
}

pub(crate) fn load_xmage_cp7_outcome_inference_v1(
    directory: &Path,
) -> DynResultV1<NativeXmageCp7OutcomeInferenceV1> {
    let (state, manifest, manifest_sha256) = load_derivative_bundle_v1(directory)?;
    let expected_corpus_sha256 = parse_lower_hex_raw32_v1(&manifest.corpus.jsonl_sha256)?;
    let inference = NativeXmageCp7OutcomeInferenceV1 {
        manifest_sha256,
        payload_sha256: parse_lower_hex_raw32_v1(&manifest.payload.payload_sha256)?,
        native_state_sha256: parse_lower_hex_raw32_v1(&manifest.payload.native_state_sha256)?,
        model_parameter_sha256: parse_lower_hex_raw32_v1(&manifest.payload.model_parameter_sha256)?,
        corpus_sha256: expected_corpus_sha256,
        adam_step: manifest.payload.adam_step,
        state,
    };
    if inference.corpus_sha256_v1() != expected_corpus_sha256 {
        return Err(invalid_data_v1("loaded outcome corpus identity mismatch").into());
    }
    Ok(inference)
}

fn inference_parent_binding_v1(
    parent: &NativeXmageCp7OutcomeInferenceV1,
) -> ParentBindingManifestV1 {
    ParentBindingManifestV1 {
        authority_kind: "xmage-cp7-outcome-reinforce-derivative-v1".to_owned(),
        manifest_sha256: lower_hex_raw32_v1(parent.manifest_sha256),
        payload_sha256: lower_hex_raw32_v1(parent.payload_sha256),
        native_state_sha256: lower_hex_raw32_v1(parent.native_state_sha256),
        model_parameter_sha256: lower_hex_raw32_v1(parent.model_parameter_sha256),
        corpus_sha256: lower_hex_raw32_v1(parent.corpus_sha256),
        adam_step: parent.adam_step,
    }
}

fn validate_bilinear_parent_transport_v1(
    example: &OutcomeExampleV1,
    logits: &[f32],
    value: f32,
) -> DynResultV1<()> {
    if logits.len() != example.legal_action_count
        || logits.len() != example.old_policy_logits_f32_bits.len()
    {
        return Err(invalid_data_v1("bilinear parent forward width mismatch").into());
    }
    let exported_value = f32::from_bits(example.old_value_f32_bits);
    if value.to_bits() != example.old_value_f32_bits
        && (value - exported_value).abs() > transport_bound_v1(exported_value)
    {
        return Err(invalid_data_v1(format!(
            "bilinear parent value transport exceeds envelope at decision {}",
            example.outcome_decision_ordinal
        ))
        .into());
    }
    for (action_index, (actual, expected_bits)) in logits
        .iter()
        .copied()
        .zip(example.old_policy_logits_f32_bits.iter().copied())
        .enumerate()
    {
        if actual.to_bits() == expected_bits {
            continue;
        }
        let expected = f32::from_bits(expected_bits);
        if (actual - expected).abs() > transport_bound_v1(expected) {
            return Err(invalid_data_v1(format!(
                "bilinear parent logit transport exceeds envelope at decision {} action {}",
                example.outcome_decision_ordinal, action_index
            ))
            .into());
        }
    }
    Ok(())
}

/// Loads the existing strict outcome JSONL contract, binds it to the selected
/// frozen derivative parent, and converts every physical group to normal
/// parent outputs plus exact state/action encoder activations.
pub(crate) fn load_xmage_cp7_outcome_bilinear_dataset_v1(
    path: &Path,
    parent: &NativeXmageCp7OutcomeInferenceV1,
) -> DynResultV1<NativeXmageCp7OutcomeBilinearDatasetV1> {
    let dataset = load_outcome_dataset_v1(path)?;
    let parent_binding = inference_parent_binding_v1(parent);
    if !validate_parent_binding_v1(&parent_binding)
        || !corpus_matches_training_source_v1(&dataset, Some(&parent_binding))
    {
        return Err(invalid_data_v1(
            "bilinear outcome corpus does not match the selected frozen parent",
        )
        .into());
    }

    let mut converted_groups = Vec::with_capacity(dataset.groups.len());
    let mut converted_substep_count = 0_usize;
    for group in &dataset.groups {
        let first = group
            .examples
            .first()
            .ok_or_else(|| invalid_data_v1("empty bilinear outcome physical group"))?;
        let mut substeps = Vec::with_capacity(group.examples.len());
        for example in &group.examples {
            let latent = parent
                .state
                .model_v1()
                .forward_with_latents_v1(example.tensor.view_v1())?;
            validate_bilinear_parent_transport_v1(
                example,
                &latent.output.logits,
                latent.output.value,
            )?;
            if latent.action_hidden.len()
                != example
                    .legal_action_count
                    .checked_mul(HIDDEN_DIM_V1)
                    .ok_or_else(|| invalid_data_v1("bilinear action latent length overflow"))?
            {
                return Err(invalid_data_v1("bilinear action latent width mismatch").into());
            }
            substeps.push(NativeXmageCp7OutcomeBilinearSubstepV1 {
                parent_logits: latent.output.logits,
                parent_value: latent.output.value,
                selected_index: example.selected_index,
                state_hidden: latent.state_hidden,
                action_hidden: latent.action_hidden,
            });
            converted_substep_count = converted_substep_count
                .checked_add(1)
                .ok_or_else(|| invalid_data_v1("bilinear substep count overflow"))?;
        }
        converted_groups.push(NativeXmageCp7OutcomeBilinearGroupV1 {
            pair_index: group.pair_index,
            episode_id: group.episode_id,
            candidate_seat: group.candidate_seat,
            terminal_return: group.terminal_return,
            decision_kind: group.decision_kind.clone(),
            first_substep_old_value: f32::from_bits(first.old_value_f32_bits),
            substeps,
        });
    }
    if converted_substep_count != dataset.decision_row_count
        || converted_groups.len() != dataset.groups.len()
    {
        return Err(invalid_data_v1("bilinear outcome conversion count mismatch").into());
    }

    let jsonl_sha256 = parse_lower_hex_raw32_v1(&dataset.jsonl_sha256)?;
    Ok(NativeXmageCp7OutcomeBilinearDatasetV1 {
        jsonl_sha256,
        export_contract: dataset.export_contract,
        schema_version: dataset.schema_version,
        decision_row_count: dataset.decision_row_count,
        terminal_row_count: dataset.terminal_row_count,
        episode_count: dataset.episode_count,
        pair_count: dataset.pair_indices.len(),
        pair_indices: dataset.pair_indices,
        physical_group_count: converted_groups.len(),
        terminal_return_counts: dataset.terminal_return_counts,
        parent_manifest_sha256: parent.manifest_sha256,
        parent_payload_sha256: parent.payload_sha256,
        parent_native_state_sha256: parent.native_state_sha256,
        parent_model_parameter_sha256: parent.model_parameter_sha256,
        parent_corpus_sha256: parent.corpus_sha256,
        parent_adam_step: parent.adam_step,
        groups: converted_groups,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct XmageCp7OutcomeCheckpointSummaryV1 {
    pub schema: String,
    pub manifest_sha256: String,
    pub payload_sha256: String,
    pub model_parameter_sha256: String,
    pub native_state_sha256: String,
    pub corpus_sha256: String,
    pub adam_step: u64,
    pub learning_rate: f32,
    pub value_coefficient: f32,
    pub advantage_mode: String,
    pub policy_scale: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ppo_clip_epsilon: Option<f32>,
    pub epochs: u32,
    pub effective_batch_groups: usize,
    pub optimizer_reset: bool,
    pub on_policy_g384_single_update: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_manifest_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starting_adam_step: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_policy_parent_single_update: Option<bool>,
}

pub fn verify_xmage_cp7_outcome_checkpoint_v1(
    directory: impl AsRef<Path>,
) -> DynResultV1<XmageCp7OutcomeCheckpointSummaryV1> {
    let (state, manifest, manifest_sha256) = load_derivative_bundle_v1(directory.as_ref())?;
    Ok(XmageCp7OutcomeCheckpointSummaryV1 {
        schema: manifest.schema,
        manifest_sha256: lower_hex_raw32_v1(manifest_sha256),
        payload_sha256: manifest.payload.payload_sha256,
        model_parameter_sha256: state.model_v1().parameter_manifest_sha256_v1(),
        native_state_sha256: lower_hex_raw32_v1(state.state_sha256_v1()?),
        corpus_sha256: manifest.corpus.jsonl_sha256,
        adam_step: state.adam_step_v1(),
        learning_rate: manifest.training.learning_rate,
        value_coefficient: manifest.training.value_coefficient,
        advantage_mode: if manifest.training.ppo_clip.is_some() {
            PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_CLI_V1.to_owned()
        } else if manifest.training.advantage_transform.is_some() {
            STANDARDIZED_EPISODE_BALANCED_CLI_V1.to_owned()
        } else {
            RAW_ADVANTAGE_CLI_V1.to_owned()
        },
        policy_scale: manifest
            .training
            .advantage_transform
            .as_ref()
            .map_or(1.0, |transform| transform.policy_scale),
        ppo_clip_epsilon: manifest
            .training
            .ppo_clip
            .as_ref()
            .map(|ppo| ppo.clip_epsilon),
        epochs: manifest.training.epochs,
        effective_batch_groups: manifest.training.effective_batch_groups,
        optimizer_reset: manifest.training.optimizer_reset,
        on_policy_g384_single_update: manifest.training.on_policy_g384_single_update,
        parent_manifest_sha256: manifest
            .parent
            .as_ref()
            .map(|parent| parent.manifest_sha256.clone()),
        starting_adam_step: manifest.training.starting_adam_step,
        on_policy_parent_single_update: manifest.training.on_policy_parent_single_update,
    })
}

fn next_arg_v1(iterator: &mut impl Iterator<Item = OsString>, flag: &str) -> DynResultV1<OsString> {
    iterator
        .next()
        .ok_or_else(|| invalid_data_v1(format!("missing value after {flag}")))
        .map_err(Into::into)
}

fn parse_batch_groups_v1(value: &str) -> DynResultV1<BatchGroupsV1> {
    if value == "all" {
        return Ok(BatchGroupsV1::All);
    }
    let count = value.parse::<usize>()?;
    if count == 0 || count.to_string() != value {
        return Err(
            invalid_data_v1("--batch-groups must be all or a canonical positive integer").into(),
        );
    }
    Ok(BatchGroupsV1::Count(count))
}

fn parse_advantage_mode_v1(value: &str) -> DynResultV1<AdvantageModeV1> {
    match value {
        RAW_ADVANTAGE_CLI_V1 => Ok(AdvantageModeV1::Raw),
        STANDARDIZED_EPISODE_BALANCED_CLI_V1 => Ok(AdvantageModeV1::StandardizedEpisodeBalanced),
        PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_CLI_V1 => {
            Ok(AdvantageModeV1::PpoClipStandardizedEpisodeBalanced)
        }
        _ => Err(invalid_data_v1(
            "--advantage-mode must be raw, standardized-episode-balanced, or ppo-clip-standardized-episode-balanced",
        )
        .into()),
    }
}

fn parse_train_config_v1<I>(arguments: I) -> DynResultV1<TrainConfigV1>
where
    I: IntoIterator<Item = OsString>,
{
    let mut config = TrainConfigV1::default();
    let mut iterator = arguments.into_iter();
    let mut seen = BTreeSet::new();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| invalid_data_v1("CLI flag is not UTF-8"))?;
        if !seen.insert(flag.to_owned()) {
            return Err(invalid_data_v1(format!("duplicate CLI flag {flag}")).into());
        }
        match flag {
            "--outcome-jsonl" => config.outcome_jsonl = next_arg_v1(&mut iterator, flag)?.into(),
            "--outcome-jsonl-sha256" => {
                config.outcome_jsonl_sha256 = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("outcome JSONL SHA-256 is not UTF-8"))?
                    .to_owned()
            }
            "--source-store-root" => {
                config.source_store_root = Some(next_arg_v1(&mut iterator, flag)?.into())
            }
            "--source-outcome-root" => {
                config.source_outcome_root = Some(next_arg_v1(&mut iterator, flag)?.into())
            }
            "--output-dir" => config.output_dir = next_arg_v1(&mut iterator, flag)?.into(),
            "--learning-rate" => {
                config.learning_rate = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("learning rate is not UTF-8"))?
                    .parse()?;
            }
            "--value-coefficient" => {
                config.value_coefficient = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("value coefficient is not UTF-8"))?
                    .parse()?;
            }
            "--advantage-mode" => {
                config.advantage_mode = parse_advantage_mode_v1(
                    next_arg_v1(&mut iterator, flag)?
                        .to_str()
                        .ok_or_else(|| invalid_data_v1("advantage mode is not UTF-8"))?,
                )?;
            }
            "--policy-scale" => {
                config.policy_scale = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("policy scale is not UTF-8"))?
                    .parse()?;
            }
            "--ppo-clip-epsilon" => {
                config.ppo_clip_epsilon = Some(
                    next_arg_v1(&mut iterator, flag)?
                        .to_str()
                        .ok_or_else(|| invalid_data_v1("PPO clip epsilon is not UTF-8"))?
                        .parse()?,
                );
            }
            "--epochs" => {
                config.epochs = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("epochs is not UTF-8"))?
                    .parse()?;
            }
            "--batch-groups" => {
                config.batch_groups = parse_batch_groups_v1(
                    next_arg_v1(&mut iterator, flag)?
                        .to_str()
                        .ok_or_else(|| invalid_data_v1("batch groups is not UTF-8"))?,
                )?;
            }
            "--help" | "-h" => {
                return Err(invalid_data_v1(
                    "usage: xmage_cp7_outcome_reinforce_v1 train --outcome-jsonl PATH --outcome-jsonl-sha256 HEX64 (--source-store-root PATH | --source-outcome-root PATH) --output-dir NEW_PATH --learning-rate FLOAT --value-coefficient FLOAT --epochs U32 [--batch-groups all|N] [--advantage-mode raw|standardized-episode-balanced|ppo-clip-standardized-episode-balanced] [--policy-scale FLOAT] [--ppo-clip-epsilon FLOAT]",
                )
                .into())
            }
            _ => return Err(invalid_data_v1(format!("unknown CLI flag {flag}")).into()),
        }
    }
    if config.outcome_jsonl.as_os_str().is_empty()
        || config.outcome_jsonl_sha256.is_empty()
        || config.output_dir.as_os_str().is_empty()
        || (config.source_store_root.is_some() == config.source_outcome_root.is_some())
        || config
            .source_store_root
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        || config
            .source_outcome_root
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err(invalid_data_v1(
            "--outcome-jsonl, --outcome-jsonl-sha256, --output-dir, and exactly one source root are required",
        )
        .into());
    }
    parse_lower_hex_raw32_v1(&config.outcome_jsonl_sha256)?;
    if !config.learning_rate.is_finite()
        || config.learning_rate <= 0.0
        || config.learning_rate > 1.0e-3
        || !config.value_coefficient.is_finite()
        || config.value_coefficient <= 0.0
        || config.value_coefficient > 10.0
        || !config.policy_scale.is_finite()
        || config.policy_scale <= 0.0
        || config.policy_scale > 100.0
        || config.epochs == 0
    {
        return Err(invalid_data_v1(
            "training requires 0 < learning-rate <= 1e-3, 0 < value-coefficient <= 10, 0 < policy-scale <= 100, and positive epochs",
        )
        .into());
    }
    match config.advantage_mode {
        AdvantageModeV1::Raw
            if config.policy_scale.to_bits() != 1.0_f32.to_bits()
                || config.ppo_clip_epsilon.is_some() =>
        {
            return Err(invalid_data_v1(
                "raw advantage mode requires --policy-scale 1 and no PPO clip epsilon",
            )
            .into())
        }
        AdvantageModeV1::StandardizedEpisodeBalanced
            if config.epochs != 1
                || config.batch_groups != BatchGroupsV1::All
                || config.ppo_clip_epsilon.is_some() =>
        {
            return Err(invalid_data_v1(
                "standardized-episode-balanced requires --epochs 1, --batch-groups all, and no PPO clip epsilon",
            )
            .into())
        }
        AdvantageModeV1::PpoClipStandardizedEpisodeBalanced => {
            let Some(clip_epsilon) = config.ppo_clip_epsilon else {
                return Err(invalid_data_v1("PPO clip mode requires --ppo-clip-epsilon").into());
            };
            if config.source_outcome_root.is_none()
                || config.epochs < 2
                || config.batch_groups != BatchGroupsV1::All
                || config.policy_scale.to_bits() != 1.0_f32.to_bits()
                || !clip_epsilon.is_finite()
                || clip_epsilon <= 0.0
                || clip_epsilon >= 1.0
            {
                return Err(invalid_data_v1(
                    "PPO clip mode requires an outcome parent, --epochs >= 2, --batch-groups all, --policy-scale 1, and 0 < --ppo-clip-epsilon < 1",
                )
                .into());
            }
        }
        _ => {}
    }
    if config.source_outcome_root.is_some()
        && config.advantage_mode != AdvantageModeV1::PpoClipStandardizedEpisodeBalanced
        && (config.epochs != 1 || config.batch_groups != BatchGroupsV1::All)
    {
        return Err(invalid_data_v1(
            "--source-outcome-root requires --epochs 1 and --batch-groups all",
        )
        .into());
    }
    Ok(config)
}

fn run_training_v1(config: TrainConfigV1) -> DynResultV1<XmageCp7OutcomeCheckpointSummaryV1> {
    ensure_output_absent_v1(&config.output_dir)?;
    let dataset = load_outcome_dataset_v1(&config.outcome_jsonl)?;
    if dataset.jsonl_sha256 != config.outcome_jsonl_sha256 {
        return Err(invalid_data_v1("outcome JSONL SHA-256 authority mismatch").into());
    }
    let loaded_source = load_training_source_v1(&config)?;
    if !corpus_matches_training_source_v1(&dataset, loaded_source.parent.as_ref()) {
        return Err(
            invalid_data_v1("outcome corpus does not match selected training source").into(),
        );
    }
    let parent_source = loaded_source.parent.is_some();
    let starting_adam_step = loaded_source.state.adam_step_v1();
    let parent = loaded_source.parent;
    let mut state = loaded_source.state;
    let source_transport_audit =
        audit_source_transport_v1(state.model_v1(), &dataset.groups, parent_source)?;
    let prepared_advantages = match config.advantage_mode {
        AdvantageModeV1::Raw => None,
        AdvantageModeV1::StandardizedEpisodeBalanced
        | AdvantageModeV1::PpoClipStandardizedEpisodeBalanced => Some(
            prepare_dataset_advantages_v1(&dataset, config.policy_scale, parent_source)?,
        ),
    };
    let effective_batch_groups = config.batch_groups.effective_v1(dataset.groups.len());
    eprintln!(
        "loaded XMage CP7 outcomes rows={} episodes={} pairs={} physical_groups={} batch_groups={} epochs={} advantage_mode={} policy_scale={} ppo_clip_epsilon={}",
        dataset.decision_row_count,
        dataset.episode_count,
        dataset.pair_indices.len(),
        dataset.groups.len(),
        effective_batch_groups,
        config.epochs,
        config.advantage_mode.cli_v1(),
        config.policy_scale,
        config
            .ppo_clip_epsilon
            .map_or_else(|| "none".to_owned(), |value| value.to_string()),
    );
    let (initial_objective_metrics, final_objective_metrics, ppo_clip) = match config.advantage_mode
    {
        AdvantageModeV1::Raw => {
            let initial =
                evaluate_objective_v1(state.model_v1(), &dataset.groups, config.value_coefficient)?;
            for _ in 0..config.epochs {
                for batch in dataset.groups.chunks(effective_batch_groups) {
                    train_batch_v1(
                        &mut state,
                        batch,
                        config.learning_rate,
                        config.value_coefficient,
                    )?;
                }
            }
            let final_metrics =
                evaluate_objective_v1(state.model_v1(), &dataset.groups, config.value_coefficient)?;
            (initial, final_metrics, None)
        }
        AdvantageModeV1::StandardizedEpisodeBalanced => {
            let prepared = prepared_advantages
                .as_ref()
                .expect("standardized advantages were prepared");
            let initial = evaluate_frozen_objective_v1(
                state.model_v1(),
                &dataset.groups,
                &prepared.terms,
                config.value_coefficient,
            )?;
            train_frozen_batch_v1(
                &mut state,
                &dataset.groups,
                &prepared.terms,
                config.learning_rate,
                config.value_coefficient,
            )?;
            let final_metrics = evaluate_frozen_objective_v1(
                state.model_v1(),
                &dataset.groups,
                &prepared.terms,
                config.value_coefficient,
            )?;
            (initial, final_metrics, None)
        }
        AdvantageModeV1::PpoClipStandardizedEpisodeBalanced => {
            let prepared = prepared_advantages
                .as_ref()
                .expect("PPO advantages were prepared");
            let clip_epsilon = config
                .ppo_clip_epsilon
                .ok_or_else(|| invalid_data_v1("PPO clip epsilon was not configured"))?;
            let mut current = prepare_ppo_epoch_v1(
                state.model_v1(),
                &dataset.groups,
                &prepared.terms,
                clip_epsilon,
                config.value_coefficient,
            )?;
            if current
                .ratio_metrics
                .maximum_absolute_joint_log_likelihood_ratio
                > PPO_INITIAL_MAX_ABSOLUTE_JOINT_LOG_RATIO_V1
            {
                return Err(invalid_data_v1(
                    "PPO initial joint likelihood ratio exceeds the transport gate",
                )
                .into());
            }
            let initial = current.objective_metrics.clone();
            let mut epoch_metrics = Vec::with_capacity(config.epochs as usize);
            for epoch_index in 1..=config.epochs {
                let adam_step_before = state.adam_step_v1();
                let before_update = current.ratio_metrics.clone();
                train_frozen_batch_v1(
                    &mut state,
                    &dataset.groups,
                    &current.terms,
                    config.learning_rate,
                    config.value_coefficient,
                )?;
                let adam_step_after = adam_step_before
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("PPO Adam step overflow"))?;
                if state.adam_step_v1() != adam_step_after {
                    return Err(
                        invalid_data_v1("PPO Adam step did not advance exactly once").into(),
                    );
                }
                let next = prepare_ppo_epoch_v1(
                    state.model_v1(),
                    &dataset.groups,
                    &prepared.terms,
                    clip_epsilon,
                    config.value_coefficient,
                )?;
                epoch_metrics.push(PpoClipEpochManifestV1 {
                    epoch_index,
                    adam_step_before,
                    adam_step_after,
                    before_update,
                    after_update: next.ratio_metrics.clone(),
                });
                current = next;
            }
            let final_metrics = current.objective_metrics;
            (
                initial,
                final_metrics,
                Some(PpoClipManifestV1 {
                    identity: PPO_CLIP_TRANSFORM_V1.to_owned(),
                    old_policy_source: PPO_OLD_POLICY_SOURCE_V1.to_owned(),
                    likelihood_ratio_scope: PPO_LIKELIHOOD_RATIO_SCOPE_V1.to_owned(),
                    clipping_scope: PPO_CLIPPING_SCOPE_V1.to_owned(),
                    clip_epsilon,
                    clip_epsilon_f32_bits: clip_epsilon.to_bits(),
                    initial_maximum_absolute_joint_log_likelihood_ratio_limit:
                        PPO_INITIAL_MAX_ABSOLUTE_JOINT_LOG_RATIO_V1,
                    initial_maximum_absolute_joint_log_likelihood_ratio_limit_f64_bits:
                        PPO_INITIAL_MAX_ABSOLUTE_JOINT_LOG_RATIO_V1.to_bits(),
                    epochs: epoch_metrics,
                }),
            )
        }
    };
    publish_derivative_v1(
        &config,
        &dataset,
        effective_batch_groups,
        parent,
        starting_adam_step,
        prepared_advantages.map(|prepared| prepared.manifest),
        ppo_clip,
        source_transport_audit,
        initial_objective_metrics,
        final_objective_metrics,
        &state,
    )?;
    verify_xmage_cp7_outcome_checkpoint_v1(&config.output_dir)
}

pub fn run_xmage_cp7_outcome_reinforce_cli_v1<I>(arguments: I) -> DynResultV1<()>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let (command, tail) = arguments
        .split_first()
        .ok_or_else(|| invalid_data_v1("expected train or verify command"))?;
    let summary = match command.to_str() {
        Some("train") => run_training_v1(parse_train_config_v1(tail.iter().cloned())?)?,
        Some("verify") => {
            if tail.len() != 2 || tail[0] != "--output-dir" {
                return Err(invalid_data_v1(
                    "usage: xmage_cp7_outcome_reinforce_v1 verify --output-dir PATH",
                )
                .into());
            }
            verify_xmage_cp7_outcome_checkpoint_v1(PathBuf::from(&tail[1]))?
        }
        _ => return Err(invalid_data_v1("expected train or verify command").into()),
    };
    println!("{}", serde_json::to_string(&summary)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact_g384_checkpoint_wire_v1() -> CheckpointIdentityWireV1 {
        CheckpointIdentityWireV1 {
            authority_kind: AUTHORITY_KIND_V1.to_owned(),
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
            environment_trajectory_contract: ENVIRONMENT_TRAJECTORY_CONTRACT_V1.to_owned(),
            sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION.to_owned(),
            sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256.to_owned(),
        }
    }

    fn parent_binding_and_checkpoint_v1() -> (ParentBindingManifestV1, CheckpointIdentityWireV1) {
        let parent = ParentBindingManifestV1 {
            authority_kind: "xmage-cp7-outcome-reinforce-derivative-v1".to_owned(),
            manifest_sha256: "1".repeat(64),
            payload_sha256: "2".repeat(64),
            native_state_sha256: "3".repeat(64),
            model_parameter_sha256: "4".repeat(64),
            corpus_sha256: "5".repeat(64),
            adam_step: 7,
        };
        let mut checkpoint = exact_g384_checkpoint_wire_v1();
        checkpoint.authority_kind = parent.authority_kind.clone();
        checkpoint.loaded_generation = parent.adam_step;
        checkpoint.loaded_checkpoint_sha256 = parent.manifest_sha256.clone();
        checkpoint.loaded_payload_sha256 = parent.payload_sha256.clone();
        checkpoint.loaded_train_state_sha256 = parent.native_state_sha256.clone();
        checkpoint.model_parameter_sha256 = parent.model_parameter_sha256.clone();
        (parent, checkpoint)
    }

    fn complete_train_args_v1() -> Vec<OsString> {
        vec![
            "--outcome-jsonl".into(),
            "outcomes.jsonl".into(),
            "--outcome-jsonl-sha256".into(),
            "0".repeat(64).into(),
            "--source-store-root".into(),
            "source".into(),
            "--output-dir".into(),
            "new-output".into(),
            "--learning-rate".into(),
            "0.000001".into(),
            "--value-coefficient".into(),
            "0.5".into(),
            "--epochs".into(),
            "1".into(),
        ]
    }

    fn complete_ppo_train_args_v1() -> Vec<OsString> {
        let mut arguments = complete_train_args_v1();
        let source_flag = arguments
            .iter()
            .position(|value| value == "--source-store-root")
            .unwrap();
        arguments[source_flag] = "--source-outcome-root".into();
        let epochs = arguments
            .iter()
            .position(|value| value == "--epochs")
            .unwrap();
        arguments[epochs + 1] = "4".into();
        arguments.extend([
            "--advantage-mode".into(),
            PPO_CLIP_STANDARDIZED_EPISODE_BALANCED_CLI_V1.into(),
            "--ppo-clip-epsilon".into(),
            "0.2".into(),
        ]);
        arguments
    }

    #[test]
    fn first_screen_defaults_to_one_full_corpus_batch_v1() {
        let config = parse_train_config_v1(complete_train_args_v1()).unwrap();
        assert_eq!(config.batch_groups, BatchGroupsV1::All);
        assert_eq!(config.batch_groups.effective_v1(1_234), 1_234);
        assert_eq!(config.epochs, 1);
        assert_eq!(config.learning_rate.to_bits(), 1.0e-6_f32.to_bits());
    }

    #[test]
    fn batch_and_cli_numeric_admission_are_strict_v1() {
        assert_eq!(parse_batch_groups_v1("all").unwrap(), BatchGroupsV1::All);
        assert_eq!(
            parse_batch_groups_v1("64").unwrap(),
            BatchGroupsV1::Count(64)
        );
        assert!(parse_batch_groups_v1("0").is_err());
        assert!(parse_batch_groups_v1("064").is_err());
        let mut duplicate = complete_train_args_v1();
        duplicate.extend(["--epochs".into(), "2".into()]);
        assert!(parse_train_config_v1(duplicate).is_err());
        let mut excessive = complete_train_args_v1();
        let position = excessive
            .iter()
            .position(|value| value == "0.000001")
            .unwrap();
        excessive[position] = "0.01".into();
        assert!(parse_train_config_v1(excessive).is_err());
        let mut zero_value_coefficient = complete_train_args_v1();
        let position = zero_value_coefficient
            .iter()
            .position(|value| value == "0.5")
            .unwrap();
        zero_value_coefficient[position] = "0".into();
        assert!(parse_train_config_v1(zero_value_coefficient).is_err());

        let mut standardized = complete_train_args_v1();
        standardized.extend([
            "--advantage-mode".into(),
            STANDARDIZED_EPISODE_BALANCED_CLI_V1.into(),
            "--policy-scale".into(),
            "2.5".into(),
        ]);
        let config = parse_train_config_v1(standardized).unwrap();
        assert_eq!(
            config.advantage_mode,
            AdvantageModeV1::StandardizedEpisodeBalanced
        );
        assert_eq!(config.policy_scale.to_bits(), 2.5_f32.to_bits());

        let mut standardized_minibatch = complete_train_args_v1();
        standardized_minibatch.extend([
            "--advantage-mode".into(),
            STANDARDIZED_EPISODE_BALANCED_CLI_V1.into(),
            "--batch-groups".into(),
            "64".into(),
        ]);
        assert!(parse_train_config_v1(standardized_minibatch).is_err());

        let mut scaled_raw = complete_train_args_v1();
        scaled_raw.extend(["--policy-scale".into(), "2".into()]);
        assert!(parse_train_config_v1(scaled_raw).is_err());

        let mut parent_source = complete_train_args_v1();
        let source_flag = parent_source
            .iter()
            .position(|value| value == "--source-store-root")
            .unwrap();
        parent_source[source_flag] = "--source-outcome-root".into();
        assert!(parse_train_config_v1(parent_source).is_ok());

        let mut both_sources = complete_train_args_v1();
        both_sources.extend(["--source-outcome-root".into(), "parent".into()]);
        assert!(parse_train_config_v1(both_sources).is_err());

        let mut parent_minibatch = complete_train_args_v1();
        let source_flag = parent_minibatch
            .iter()
            .position(|value| value == "--source-store-root")
            .unwrap();
        parent_minibatch[source_flag] = "--source-outcome-root".into();
        parent_minibatch.extend(["--batch-groups".into(), "64".into()]);
        assert!(parse_train_config_v1(parent_minibatch).is_err());
    }

    #[test]
    fn ppo_cli_requires_exact_parent_full_batches_multiple_epochs_and_clip_v1() {
        let config = parse_train_config_v1(complete_ppo_train_args_v1()).unwrap();
        assert_eq!(
            config.advantage_mode,
            AdvantageModeV1::PpoClipStandardizedEpisodeBalanced
        );
        assert_eq!(config.epochs, 4);
        assert_eq!(config.batch_groups, BatchGroupsV1::All);
        assert_eq!(config.policy_scale.to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            config.ppo_clip_epsilon.unwrap().to_bits(),
            0.2_f32.to_bits()
        );
        assert!(config.source_store_root.is_none());
        assert!(config.source_outcome_root.is_some());

        let mut missing_clip = complete_ppo_train_args_v1();
        let clip_flag = missing_clip
            .iter()
            .position(|value| value == "--ppo-clip-epsilon")
            .unwrap();
        missing_clip.drain(clip_flag..=clip_flag + 1);
        assert!(parse_train_config_v1(missing_clip).is_err());

        let mut original_store = complete_ppo_train_args_v1();
        let source_flag = original_store
            .iter()
            .position(|value| value == "--source-outcome-root")
            .unwrap();
        original_store[source_flag] = "--source-store-root".into();
        assert!(parse_train_config_v1(original_store).is_err());

        let mut one_epoch = complete_ppo_train_args_v1();
        let epochs = one_epoch
            .iter()
            .position(|value| value == "--epochs")
            .unwrap();
        one_epoch[epochs + 1] = "1".into();
        assert!(parse_train_config_v1(one_epoch).is_err());

        let mut minibatch = complete_ppo_train_args_v1();
        minibatch.extend(["--batch-groups".into(), "64".into()]);
        assert!(parse_train_config_v1(minibatch).is_err());

        let mut non_ppo_parent_multi_epoch = complete_train_args_v1();
        let source_flag = non_ppo_parent_multi_epoch
            .iter()
            .position(|value| value == "--source-store-root")
            .unwrap();
        non_ppo_parent_multi_epoch[source_flag] = "--source-outcome-root".into();
        let epochs = non_ppo_parent_multi_epoch
            .iter()
            .position(|value| value == "--epochs")
            .unwrap();
        non_ppo_parent_multi_epoch[epochs + 1] = "4".into();
        assert!(parse_train_config_v1(non_ppo_parent_multi_epoch).is_err());
    }

    #[test]
    fn ppo_joint_ratio_multiplies_substeps_then_clips_once_v1() {
        let current = [0.4_f64.ln(), 0.3_f64.ln()];
        let old = [0.2_f64.ln(), 0.6_f64.ln()];
        let (joint_log_ratio, joint_ratio) = ppo_joint_likelihood_ratio_v1(&current, &old).unwrap();
        assert!(joint_log_ratio.abs() <= 1.0e-15);
        assert!((joint_ratio - 1.0).abs() <= 1.0e-15);
        assert!(((current[0] - old[0]).exp() - 2.0).abs() <= 1.0e-15);
        assert!(((current[1] - old[1]).exp() - 0.5).abs() <= 1.0e-15);

        let (surrogate, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(1.0, joint_ratio, 0.2).unwrap();
        assert!(!clipped);
        assert!((surrogate - 1.0).abs() <= 1.0e-15);
        assert_eq!(coefficient.to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn ppo_clipping_has_exact_positive_and_negative_gradient_branches_v1() {
        let (surrogate, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(2.0, 1.3, 0.2).unwrap();
        assert!(clipped);
        assert!((surrogate - 2.4).abs() <= 1.0e-7);
        assert_eq!(coefficient.to_bits(), 0.0_f32.to_bits());

        let (surrogate, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(2.0, 0.7, 0.2).unwrap();
        assert!(!clipped);
        assert!((surrogate - 1.4).abs() <= 1.0e-7);
        assert!((coefficient - 1.4).abs() <= 1.0e-6);

        let (surrogate, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(-2.0, 0.7, 0.2).unwrap();
        assert!(clipped);
        assert!((surrogate + 1.6).abs() <= 1.0e-7);
        assert_eq!(coefficient.to_bits(), 0.0_f32.to_bits());

        let (surrogate, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(-2.0, 1.3, 0.2).unwrap();
        assert!(!clipped);
        assert!((surrogate + 2.6).abs() <= 1.0e-7);
        assert!((coefficient + 2.6).abs() <= 1.0e-6);
    }

    #[test]
    fn ppo_effective_coefficient_matches_local_clipped_surrogate_gradient_v1() {
        fn policy_loss(log_ratio: f64, advantage: f32, epsilon: f32) -> f64 {
            let (surrogate, _, _) =
                ppo_clipped_surrogate_and_coefficient_v1(advantage, log_ratio.exp(), epsilon)
                    .unwrap();
            -surrogate
        }

        let log_ratio = 0.9_f64.ln();
        let (_, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(1.75, log_ratio.exp(), 0.2).unwrap();
        assert!(!clipped);
        let delta = 1.0e-6;
        let numerical = (policy_loss(log_ratio + delta, 1.75, 0.2)
            - policy_loss(log_ratio - delta, 1.75, 0.2))
            / (2.0 * delta);
        assert!((numerical + f64::from(coefficient)).abs() <= 1.0e-7);

        let clipped_log_ratio = 1.3_f64.ln();
        let (_, coefficient, clipped) =
            ppo_clipped_surrogate_and_coefficient_v1(1.75, clipped_log_ratio.exp(), 0.2).unwrap();
        assert!(clipped);
        assert_eq!(coefficient.to_bits(), 0.0_f32.to_bits());
        let numerical = (policy_loss(clipped_log_ratio + delta, 1.75, 0.2)
            - policy_loss(clipped_log_ratio - delta, 1.75, 0.2))
            / (2.0 * delta);
        assert!(numerical.abs() <= 1.0e-9);
    }

    #[test]
    fn standardized_episode_balancing_equalizes_episode_mass_and_centers_coefficients_v1() {
        let observations = [
            AdvantageObservationV1 {
                episode_id: 10,
                terminal_return: 1,
                source_value: 0.0,
            },
            AdvantageObservationV1 {
                episode_id: 20,
                terminal_return: -1,
                source_value: 0.0,
            },
            AdvantageObservationV1 {
                episode_id: 20,
                terminal_return: -1,
                source_value: 0.5,
            },
            AdvantageObservationV1 {
                episode_id: 20,
                terminal_return: -1,
                source_value: -0.5,
            },
        ];
        let prepared = prepare_standardized_episode_balanced_advantages_v1(
            &observations,
            3,
            2.0,
            "exported_g384_old_value_f32_bits_first_substep",
        )
        .unwrap();
        let short_episode_mass = f64::from(prepared.terms[0].value_weight);
        let long_episode_mass = prepared.terms[1..]
            .iter()
            .map(|term| f64::from(term.value_weight))
            .sum::<f64>();
        assert!((short_episode_mass - 2.0).abs() <= 1.0e-7);
        assert!((long_episode_mass - 2.0).abs() <= 2.0e-7);
        assert!((short_episode_mass - long_episode_mass).abs() <= 2.0e-7);

        let policy_coefficient_sum = prepared
            .terms
            .iter()
            .map(|term| f64::from(term.policy_advantage))
            .sum::<f64>();
        assert!(policy_coefficient_sum.abs() <= 1.0e-6);
        assert_eq!(prepared.manifest.contributing_episode_count, 2);
        assert_eq!(prepared.manifest.zero_decision_episode_count, 1);
        assert_eq!(prepared.manifest.physical_group_count, 4);
        assert!(prepared.manifest.source_advantage_mean.abs() <= f64::EPSILON);
    }

    #[test]
    fn natural_terminal_tuple_binds_candidate_reward_v1() {
        let terminal = TerminalWireV1 {
            record_type: "terminal".to_owned(),
            schema_version: 1,
            record_ordinal: 2,
            checkpoint: None,
            deck_ids: vec!["Rally".to_owned(), "Rally".to_owned()],
            randomization_identity: RANDOMIZATION_IDENTITY_V1.to_owned(),
            base_seed_u64_hex: "0".repeat(16),
            pair_index: 0,
            pair_environment_seed_u64_hex: "1".repeat(16),
            episode_id: 0,
            candidate_seat: PlayerSeatV1::P0,
            first_outcome_decision_ordinal: Some(0),
            outcome_decision_count: 1,
            candidate_terminal_reward: 1,
            terminal: RlSessionTerminalV1 {
                schema_version: RL_SESSION_SCHEMA_VERSION,
                deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
                deck_hashes: [RALLY_DECK_HASH_U64_V1; 2],
                episode_id: 0,
                terminal_outcome: TerminalOutcomeV1::P0Win,
                terminal_classification: TerminalClassificationV1::Natural,
                terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                winner: Some(PlayerSeatV1::P0),
                terminal_reward: [1, -1],
                terminal_reason: "game_over".to_owned(),
                policy_step_count: 2,
                physical_decision_count: 2,
            },
            diagnostic_state_hash_u64_hex: "2".repeat(16),
            core_environment_hash_u64_hex: "3".repeat(16),
        };
        let checkpoint = exact_g384_checkpoint_wire_v1();
        validate_natural_terminal_v1(&terminal, 1, &checkpoint).unwrap();
        let mut mismatch = terminal.clone();
        mismatch.candidate_terminal_reward = -1;
        assert!(validate_natural_terminal_v1(&mismatch, 1, &checkpoint).is_err());
    }

    #[test]
    fn row_checkpoint_field_presence_is_strict_across_schema_versions_v1() {
        for record_type in ["decision", "terminal"] {
            let legacy_missing = serde_json::json!({"record_type": record_type});
            let legacy_null = serde_json::json!({"record_type": record_type, "checkpoint": null});
            let iterative_missing = serde_json::json!({"record_type": record_type});
            let iterative_null =
                serde_json::json!({"record_type": record_type, "checkpoint": null});
            let iterative_present = serde_json::json!({
                "record_type": record_type,
                "checkpoint": {"authority_kind": "test"}
            });

            assert!(row_checkpoint_field_presence_valid_v1(1, &legacy_missing));
            assert!(!row_checkpoint_field_presence_valid_v1(1, &legacy_null));
            assert!(!row_checkpoint_field_presence_valid_v1(
                2,
                &iterative_missing
            ));
            assert!(!row_checkpoint_field_presence_valid_v1(2, &iterative_null));
            assert!(row_checkpoint_field_presence_valid_v1(
                2,
                &iterative_present
            ));
        }
    }

    #[test]
    fn iterative_corpus_requires_the_exact_selected_parent_v1() {
        let (parent, checkpoint) = parent_binding_and_checkpoint_v1();
        let mut dataset = OutcomeDatasetV1 {
            jsonl_sha256: "6".repeat(64),
            export_contract: XMAGE_CP7_OUTCOME_JSONL_CONTRACT_V2.to_owned(),
            schema_version: 2,
            policy_checkpoint: checkpoint,
            decision_row_count: 1,
            terminal_row_count: 2,
            episode_count: 2,
            pair_indices: vec![0],
            terminal_return_counts: [1, 0, 1],
            groups: Vec::new(),
        };
        assert!(corpus_matches_training_source_v1(&dataset, Some(&parent)));
        assert!(!corpus_matches_training_source_v1(&dataset, None));

        dataset.policy_checkpoint.loaded_checkpoint_sha256 = "7".repeat(64);
        assert!(!corpus_matches_training_source_v1(&dataset, Some(&parent)));
        dataset.policy_checkpoint.loaded_checkpoint_sha256 = parent.manifest_sha256.clone();
        let mut wrong_parent = parent.clone();
        wrong_parent.native_state_sha256 = "8".repeat(64);
        assert!(!corpus_matches_training_source_v1(
            &dataset,
            Some(&wrong_parent)
        ));
    }

    #[test]
    #[ignore = "requires MTG_KERNEL_XMAGE_CP7_OUTCOME_JSONL"]
    fn external_outcome_corpus_passes_strict_loader_v1() {
        let path = std::env::var_os("MTG_KERNEL_XMAGE_CP7_OUTCOME_JSONL")
            .expect("MTG_KERNEL_XMAGE_CP7_OUTCOME_JSONL is set");
        let dataset = load_outcome_dataset_v1(Path::new(&path)).unwrap();
        assert!(!dataset.groups.is_empty());
        assert_eq!(dataset.episode_count, dataset.terminal_row_count);
        assert_eq!(dataset.episode_count, dataset.pair_indices.len() * 2);
    }

    #[test]
    #[ignore = "requires MTG_KERNEL_XMAGE_CP7_OUTCOME_PARENT_ROOT"]
    fn external_outcome_parent_load_preserves_full_adam_state_v1() {
        let root = PathBuf::from(
            std::env::var_os("MTG_KERNEL_XMAGE_CP7_OUTCOME_PARENT_ROOT")
                .expect("MTG_KERNEL_XMAGE_CP7_OUTCOME_PARENT_ROOT is set"),
        );
        let mut config = TrainConfigV1::default();
        config.source_outcome_root = Some(root.clone());
        let loaded = load_training_source_v1(&config).unwrap();
        let (direct, manifest, _) = load_derivative_bundle_v1(&root).unwrap();
        assert_eq!(
            loaded.state.snapshot_v1().unwrap(),
            direct.snapshot_v1().unwrap()
        );
        assert_eq!(loaded.state.adam_step_v1(), manifest.payload.adam_step);
        assert_eq!(
            loaded.parent.as_ref().unwrap().adam_step,
            manifest.payload.adam_step
        );
        assert!(loaded
            .state
            .snapshot_v1()
            .unwrap()
            .first_moments
            .iter()
            .flat_map(|parameter| &parameter.values)
            .any(|value| value.to_bits() != 0));
    }

    #[test]
    #[ignore = "requires MTG_KERNEL_XMAGE_CP7_OUTCOME_JSONL and MTG_KERNEL_XMAGE_CP7_OUTCOME_PARENT_ROOT"]
    fn external_ppo_parent_corpus_passes_initial_transport_gate_v1() {
        let corpus_path = PathBuf::from(
            std::env::var_os("MTG_KERNEL_XMAGE_CP7_OUTCOME_JSONL")
                .expect("MTG_KERNEL_XMAGE_CP7_OUTCOME_JSONL is set"),
        );
        let parent_root = PathBuf::from(
            std::env::var_os("MTG_KERNEL_XMAGE_CP7_OUTCOME_PARENT_ROOT")
                .expect("MTG_KERNEL_XMAGE_CP7_OUTCOME_PARENT_ROOT is set"),
        );
        let dataset = load_outcome_dataset_v1(&corpus_path).unwrap();
        let mut config = TrainConfigV1::default();
        config.source_outcome_root = Some(parent_root);
        let loaded = load_training_source_v1(&config).unwrap();
        assert!(corpus_matches_training_source_v1(
            &dataset,
            loaded.parent.as_ref()
        ));
        audit_source_transport_v1(loaded.state.model_v1(), &dataset.groups, true).unwrap();
        let prepared = prepare_dataset_advantages_v1(&dataset, 1.0, true).unwrap();
        let ppo = prepare_ppo_epoch_v1(
            loaded.state.model_v1(),
            &dataset.groups,
            &prepared.terms,
            0.2,
            0.05,
        )
        .unwrap();
        eprintln!(
            "initial PPO transport: groups={} rows={} max_abs_joint_log_ratio={} mean_kl={} mean_tv={} p90_tv={}",
            ppo.ratio_metrics.physical_group_count,
            ppo.ratio_metrics.observed_row_count,
            ppo.ratio_metrics
                .maximum_absolute_joint_log_likelihood_ratio,
            ppo.ratio_metrics.mean_old_to_current_forward_kl,
            ppo.ratio_metrics.mean_action_total_variation,
            ppo.ratio_metrics
                .p90_action_total_variation_nearest_rank,
        );
        assert!(
            ppo.ratio_metrics
                .maximum_absolute_joint_log_likelihood_ratio
                <= PPO_INITIAL_MAX_ABSOLUTE_JOINT_LOG_RATIO_V1
        );
        assert_eq!(ppo.ratio_metrics.clipped_group_count, 0);
    }
}
