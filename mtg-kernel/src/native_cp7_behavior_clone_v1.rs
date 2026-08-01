//! Narrow native behavior cloning path for XMage Rally CP7 teacher exports.
//!
//! This intentionally does not mint a canonical Native Training Store run.
//! It produces the existing exact native train-state payload plus a small
//! derivative manifest that this module can load and verify.

use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2,
    NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2, NATIVE_FLAT_TENSORIZER_IDENTITY_V2,
};
use crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_ref_v1;
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1,
    NativePolicyValueNetV1,
};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_verified_v1, encode_native_train_state_payload_v1,
    NativeTrainStatePayloadDigestsV1, NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::native_training_store_layout_v2::NativeTrainingStoreFinalNameV2;
use crate::rl::{parse_strict_json_value, ActionSemanticV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const TEACHER_EXPORT_CONTRACT_V1: &str = "mtg-kernel-xmage-cp7-teacher-jsonl/v1";
const TEACHER_SELECTION_SOURCE_V1: &str = "xmage_rally_cp7_mapper";
const MODEL_INPUT_COMMITMENT_V1: &str = "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1";
const DERIVATIVE_SCHEMA_V1: &str = "mtg-kernel-cp7-behavior-clone-derivative/v1";
const DERIVATIVE_PAYLOAD_FILENAME_V1: &str = "checkpoint.state.f32le";
const DERIVATIVE_MANIFEST_FILENAME_V1: &str = "checkpoint.json";
const OBJECTIVE_V1: &str = "cp7_behavior_clone_policy_cross_entropy/v1";
const SPLIT_IDENTITY_V1: &str = "pair_index_modulo/v1";

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

type DynResultV1<T> = Result<T, Box<dyn Error>>;

fn invalid_data_v1(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Clone, Debug, Deserialize)]
struct TeacherCheckpointIdentityV1 {
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
}

#[derive(Clone, Debug, Deserialize)]
struct TeacherHeaderV1 {
    schema_version: u32,
    record_ordinal: u64,
    export_contract: String,
    selection_source: String,
    tensorizer_identity: String,
    tensorizer_features_source_sha256: String,
    model_input_commitment: String,
    checkpoint: TeacherCheckpointIdentityV1,
}

#[derive(Clone, Debug, Deserialize)]
struct TeacherTensorWireV1 {
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
struct TeacherDecisionWireV1 {
    schema_version: u32,
    record_ordinal: u64,
    teacher_decision_ordinal: u64,
    selection_source: String,
    pair_index: u64,
    episode_id: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    acting_player: String,
    decision_kind: String,
    candidate_seat: String,
    legal_action_count: u32,
    model_input_sha256: String,
    old_policy_logits_f32_bits: Vec<u32>,
    old_value_f32_bits: u32,
    action_semantics: Vec<ActionSemanticV1>,
    selected_index: u32,
    selected_semantic: ActionSemanticV1,
    tensor: TeacherTensorWireV1,
}

#[derive(Clone, Debug, Deserialize)]
struct TeacherTerminalInnerV1 {
    episode_id: u64,
    terminal_classification: String,
    terminal_code: String,
}

#[derive(Clone, Debug, Deserialize)]
struct TeacherTerminalWireV1 {
    schema_version: u32,
    record_ordinal: u64,
    pair_index: u64,
    episode_id: u64,
    candidate_seat: String,
    terminal: TeacherTerminalInnerV1,
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
    fn from_wire_v1(wire: TeacherTensorWireV1) -> DynResultV1<Self> {
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

/// Recomputes the exact commitment framing used by the shadow exporter. This
/// binds every supervised label to the tensor bytes that produced its menu.
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

fn decode_f32_bits_v1(field: &'static str, bits: Vec<u32>) -> DynResultV1<Vec<f32>> {
    bits.into_iter()
        .enumerate()
        .map(|(index, bits)| {
            let value = f32::from_bits(bits);
            if value.is_finite() {
                Ok(value)
            } else {
                Err(invalid_data_v1(format!("nonfinite teacher tensor {field}[{index}]")).into())
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct TeacherExampleV1 {
    record_ordinal: u64,
    teacher_decision_ordinal: u64,
    pair_index: u64,
    episode_id: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    decision_kind: String,
    legal_action_count: u32,
    selected_index: usize,
    old_policy_logits_f32_bits: Vec<u32>,
    old_value_f32_bits: u32,
    tensor: OwnedDecisionTensorV1,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EpisodeKeyV1 {
    pair_index: u64,
    episode_id: u64,
    candidate_seat: u8,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PhysicalKeyV1 {
    pair_index: u64,
    episode_id: u64,
    physical_decision_id: u64,
    candidate_seat: u8,
}

#[derive(Clone, Debug)]
struct TeacherPhysicalDecisionV1 {
    first_record_ordinal: u64,
    pair_index: u64,
    decision_kind: String,
    examples: Vec<TeacherExampleV1>,
}

#[derive(Clone, Debug)]
struct TeacherDatasetV1 {
    teacher_jsonl_sha256: String,
    decision_row_count: usize,
    terminal_row_count: usize,
    pair_indices: Vec<u64>,
    groups: Vec<TeacherPhysicalDecisionV1>,
}

fn validate_header_v1(header: &TeacherHeaderV1) -> DynResultV1<()> {
    let checkpoint = &header.checkpoint;
    let valid = header.schema_version == 1
        && header.record_ordinal == 0
        && header.export_contract == TEACHER_EXPORT_CONTRACT_V1
        && header.selection_source == TEACHER_SELECTION_SOURCE_V1
        && header.tensorizer_identity == NATIVE_FLAT_TENSORIZER_IDENTITY_V2
        && header.tensorizer_features_source_sha256
            == NATIVE_FLAT_TENSORIZER_FEATURES_SOURCE_SHA256_V2
        && header.model_input_commitment == MODEL_INPUT_COMMITMENT_V1
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
        && checkpoint.model_parameter_sha256 == SOURCE_MODEL_PARAMETER_SHA256_V1;
    if !valid {
        return Err(invalid_data_v1("teacher header is not promoted g384 CP7 authority").into());
    }
    Ok(())
}

fn load_teacher_dataset_v1(path: &Path) -> DynResultV1<TeacherDatasetV1> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut digest = Sha256::new();
    let mut expected_record_ordinal = 0_u64;
    let mut expected_teacher_decision_ordinal = 0_u64;
    let mut header_seen = false;
    let mut decisions = BTreeMap::<PhysicalKeyV1, Vec<TeacherExampleV1>>::new();
    let mut terminals = BTreeMap::<EpisodeKeyV1, bool>::new();
    let mut terminal_row_count = 0_usize;

    loop {
        line.clear();
        let byte_count = reader.read_line(&mut line)?;
        if byte_count == 0 {
            break;
        }
        digest.update(line.as_bytes());
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return Err(invalid_data_v1("teacher JSONL contains an empty row").into());
        }
        let value = parse_strict_json_value(trimmed)?;
        let record_type = value
            .get("record_type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid_data_v1("teacher row lacks record_type"))?;
        match record_type {
            "header" => {
                if header_seen || expected_record_ordinal != 0 {
                    return Err(invalid_data_v1("teacher header must be the first row").into());
                }
                let header: TeacherHeaderV1 = serde_json::from_value(value)?;
                validate_header_v1(&header)?;
                header_seen = true;
                expected_record_ordinal = 1;
            }
            "decision" => {
                if !header_seen {
                    return Err(invalid_data_v1("teacher decision precedes header").into());
                }
                let row: TeacherDecisionWireV1 = serde_json::from_value(value)?;
                let legal_action_count = usize::try_from(row.legal_action_count)?;
                let selected_index = usize::try_from(row.selected_index)?;
                if row.schema_version != 1
                    || row.record_ordinal != expected_record_ordinal
                    || row.teacher_decision_ordinal != expected_teacher_decision_ordinal
                    || row.selection_source != TEACHER_SELECTION_SOURCE_V1
                    || row.acting_player == row.candidate_seat
                    || !matches!(row.acting_player.as_str(), "p0" | "p1")
                    || !matches!(row.candidate_seat.as_str(), "p0" | "p1")
                    || !matches!(
                        row.decision_kind.as_str(),
                        "surface" | "attacker_inclusion" | "blocker_inclusion"
                    )
                    || row.substep_count == 0
                    || row.substep_index >= row.substep_count
                    || row.legal_action_count == 0
                    || row.old_policy_logits_f32_bits.len() != legal_action_count
                    || selected_index >= legal_action_count
                    || row
                        .old_policy_logits_f32_bits
                        .iter()
                        .any(|bits| !f32::from_bits(*bits).is_finite())
                    || !f32::from_bits(row.old_value_f32_bits).is_finite()
                {
                    return Err(invalid_data_v1(format!(
                        "invalid teacher decision row {}",
                        row.record_ordinal
                    ))
                    .into());
                }
                let key = PhysicalKeyV1 {
                    pair_index: row.pair_index,
                    episode_id: row.episode_id,
                    physical_decision_id: row.physical_decision_id,
                    candidate_seat: u8::from(row.candidate_seat == "p1"),
                };
                let tensor = OwnedDecisionTensorV1::from_wire_v1(row.tensor)?;
                if row.action_semantics.len() != legal_action_count
                    || row.action_semantics.get(selected_index) != Some(&row.selected_semantic)
                    || model_input_sha256_v1(&tensor) != row.model_input_sha256
                {
                    return Err(invalid_data_v1(format!(
                        "teacher label or tensor commitment mismatch at row {}",
                        row.record_ordinal
                    ))
                    .into());
                }
                decisions.entry(key).or_default().push(TeacherExampleV1 {
                    record_ordinal: row.record_ordinal,
                    teacher_decision_ordinal: row.teacher_decision_ordinal,
                    pair_index: row.pair_index,
                    episode_id: row.episode_id,
                    physical_decision_id: row.physical_decision_id,
                    substep_index: row.substep_index,
                    substep_count: row.substep_count,
                    decision_kind: row.decision_kind,
                    legal_action_count: row.legal_action_count,
                    selected_index,
                    old_policy_logits_f32_bits: row.old_policy_logits_f32_bits,
                    old_value_f32_bits: row.old_value_f32_bits,
                    tensor,
                });
                expected_record_ordinal = expected_record_ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("record ordinal exhausted"))?;
                expected_teacher_decision_ordinal = expected_teacher_decision_ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("teacher decision ordinal exhausted"))?;
            }
            "terminal" => {
                if !header_seen {
                    return Err(invalid_data_v1("teacher terminal precedes header").into());
                }
                let row: TeacherTerminalWireV1 = serde_json::from_value(value)?;
                if row.schema_version != 1
                    || row.record_ordinal != expected_record_ordinal
                    || row.episode_id != row.terminal.episode_id
                    || !matches!(row.candidate_seat.as_str(), "p0" | "p1")
                {
                    return Err(invalid_data_v1(format!(
                        "invalid teacher terminal row {}",
                        row.record_ordinal
                    ))
                    .into());
                }
                let natural = row.terminal.terminal_classification == "natural"
                    && row.terminal.terminal_code == "natural_game_over";
                let key = EpisodeKeyV1 {
                    pair_index: row.pair_index,
                    episode_id: row.episode_id,
                    candidate_seat: u8::from(row.candidate_seat == "p1"),
                };
                if terminals.insert(key, natural).is_some() {
                    return Err(invalid_data_v1("duplicate teacher terminal row").into());
                }
                terminal_row_count += 1;
                expected_record_ordinal = expected_record_ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid_data_v1("record ordinal exhausted"))?;
            }
            _ => return Err(invalid_data_v1("unknown teacher record_type").into()),
        }
    }
    if !header_seen || decisions.is_empty() {
        return Err(invalid_data_v1("teacher dataset is empty or lacks header").into());
    }

    let mut groups = Vec::with_capacity(decisions.len());
    let mut pair_indices = BTreeSet::new();
    let mut decision_row_count = 0_usize;
    for (key, mut examples) in decisions {
        let episode_key = EpisodeKeyV1 {
            pair_index: key.pair_index,
            episode_id: key.episode_id,
            candidate_seat: key.candidate_seat,
        };
        if terminals.get(&episode_key).copied() != Some(true) {
            return Err(invalid_data_v1(format!(
                "teacher episode pair={} episode={} is incomplete or non-natural",
                key.pair_index, key.episode_id
            ))
            .into());
        }
        examples.sort_by_key(|row| row.substep_index);
        let expected_count = examples[0].substep_count;
        if examples.len() != usize::try_from(expected_count)?
            || examples.iter().enumerate().any(|(index, row)| {
                row.pair_index != key.pair_index
                    || row.episode_id != key.episode_id
                    || row.physical_decision_id != key.physical_decision_id
                    || row.substep_count != expected_count
                    || usize::try_from(row.substep_index).ok() != Some(index)
                    || row.decision_kind != examples[0].decision_kind
            })
        {
            return Err(invalid_data_v1(format!(
                "teacher physical decision is incomplete: pair={} episode={} physical={}",
                key.pair_index, key.episode_id, key.physical_decision_id
            ))
            .into());
        }
        let first_record_ordinal = examples[0].record_ordinal;
        decision_row_count += examples.len();
        pair_indices.insert(key.pair_index);
        groups.push(TeacherPhysicalDecisionV1 {
            first_record_ordinal,
            pair_index: key.pair_index,
            decision_kind: examples[0].decision_kind.clone(),
            examples,
        });
    }
    groups.sort_by_key(|group| group.first_record_ordinal);
    Ok(TeacherDatasetV1 {
        teacher_jsonl_sha256: lower_hex_raw32_v1(digest.finalize().into()),
        decision_row_count,
        terminal_row_count,
        pair_indices: pair_indices.into_iter().collect(),
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
    // Nonzero canonical Store walks are Windows-only. This derivative path
    // needs only the exact promoted state, so after staging verifies run.json
    // and all three artifact hashes, decode the hash-pinned payload directly.
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
    // Objective changes from terminal REINFORCE/value to supervised policy CE.
    // Reset Adam while preserving every promoted g384 model parameter bit.
    Ok(NativePolicyValueTrainStateV1::new_v1(model)?)
}

#[derive(Clone, Debug, Default)]
struct MetricAccumulatorV1 {
    physical_group_count: u64,
    substep_count: u64,
    nll_sum: f64,
    substep_top1_correct: u64,
    physical_top1_correct: u64,
}

impl MetricAccumulatorV1 {
    fn finish_v1(&self) -> MetricSliceV1 {
        MetricSliceV1 {
            physical_group_count: self.physical_group_count,
            substep_count: self.substep_count,
            mean_nll_per_physical_group: self.nll_sum / self.physical_group_count as f64,
            mean_nll_per_substep: self.nll_sum / self.substep_count as f64,
            substep_top1_accuracy: self.substep_top1_correct as f64 / self.substep_count as f64,
            physical_top1_accuracy: self.physical_top1_correct as f64
                / self.physical_group_count as f64,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MetricSliceV1 {
    physical_group_count: u64,
    substep_count: u64,
    mean_nll_per_physical_group: f64,
    mean_nll_per_substep: f64,
    substep_top1_accuracy: f64,
    physical_top1_accuracy: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GroupedMetricsV1 {
    overall: MetricSliceV1,
    by_decision_kind: BTreeMap<String, MetricSliceV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SplitMetricsV1 {
    train: GroupedMetricsV1,
    heldout: GroupedMetricsV1,
}

fn selected_nll_and_top1_v1(logits: &[f32], selected: usize) -> DynResultV1<(f64, bool)> {
    if logits.is_empty() || selected >= logits.len() {
        return Err(invalid_data_v1("invalid logits for metric evaluation").into());
    }
    let mut max_logit = logits[0];
    let mut top1 = 0_usize;
    for (index, logit) in logits.iter().copied().enumerate().skip(1) {
        if logit > max_logit {
            max_logit = logit;
            top1 = index;
        }
    }
    let sum_exp = logits
        .iter()
        .map(|logit| f64::from(*logit - max_logit).exp())
        .sum::<f64>();
    let nll = f64::from(max_logit - logits[selected]) + sum_exp.ln();
    if !nll.is_finite() {
        return Err(invalid_data_v1("nonfinite policy NLL").into());
    }
    Ok((nll, top1 == selected))
}

const TRANSPORT_ABSOLUTE_TOLERANCE_V1: f32 = 3.0e-5;
const TRANSPORT_RELATIVE_TOLERANCE_V1: f32 = 3.0e-5;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ForwardTransportMismatchV1 {
    teacher_decision_ordinal: u64,
    decision_kind: String,
    channel: String,
    action_index: Option<usize>,
    exported_f32_bits: u32,
    recomputed_f32_bits: u32,
    absolute_delta: f32,
    ulp_delta: u32,
    object_count: usize,
    edge_count: usize,
    action_count: usize,
    action_ref_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ForwardTransportAuditV1 {
    identity: String,
    authority_platform: String,
    recompute_platform: String,
    absolute_tolerance: f32,
    relative_tolerance: f32,
    decision_row_count: u64,
    logit_value_count: u64,
    bit_exact_decision_row_count: u64,
    mismatched_decision_row_count: u64,
    value_mismatch_count: u64,
    logit_mismatch_count: u64,
    max_value_absolute_delta: f32,
    max_logit_absolute_delta: f32,
    max_value_ulp_delta: u32,
    max_logit_ulp_delta: u32,
    first_mismatch: Option<ForwardTransportMismatchV1>,
    inference_path_note: String,
}

fn ulp_distance_v1(left: f32, right: f32) -> u32 {
    fn ordered_v1(bits: u32) -> u32 {
        if bits & 0x8000_0000 == 0 {
            bits | 0x8000_0000
        } else {
            !bits
        }
    }
    ordered_v1(left.to_bits()).abs_diff(ordered_v1(right.to_bits()))
}

fn transport_bound_v1(exported: f32) -> f32 {
    TRANSPORT_ABSOLUTE_TOLERANCE_V1 + TRANSPORT_RELATIVE_TOLERANCE_V1 * exported.abs()
}

fn mismatch_record_v1(
    example: &TeacherExampleV1,
    channel: &'static str,
    action_index: Option<usize>,
    exported: f32,
    recomputed: f32,
) -> ForwardTransportMismatchV1 {
    ForwardTransportMismatchV1 {
        teacher_decision_ordinal: example.teacher_decision_ordinal,
        decision_kind: example.decision_kind.clone(),
        channel: channel.to_owned(),
        action_index,
        exported_f32_bits: exported.to_bits(),
        recomputed_f32_bits: recomputed.to_bits(),
        absolute_delta: (exported - recomputed).abs(),
        ulp_delta: ulp_distance_v1(exported, recomputed),
        object_count: example.tensor.object_card_ids.len(),
        edge_count: example.tensor.edge_source_indices.len(),
        action_count: usize::try_from(example.legal_action_count).unwrap_or(usize::MAX),
        action_ref_count: example.tensor.action_ref_card_ids.len(),
    }
}

fn audit_exported_g384_transport_v1(
    model: &NativePolicyValueNetV1,
    groups: &[&TeacherPhysicalDecisionV1],
) -> DynResultV1<ForwardTransportAuditV1> {
    let mut audit = ForwardTransportAuditV1 {
        identity: "windows-msvc-export-to-linux-gnu-native-forward-envelope/v1".to_owned(),
        authority_platform: "x86_64-pc-windows-msvc".to_owned(),
        recompute_platform: "x86_64-unknown-linux-gnu".to_owned(),
        absolute_tolerance: TRANSPORT_ABSOLUTE_TOLERANCE_V1,
        relative_tolerance: TRANSPORT_RELATIVE_TOLERANCE_V1,
        decision_row_count: 0,
        logit_value_count: 0,
        bit_exact_decision_row_count: 0,
        mismatched_decision_row_count: 0,
        value_mismatch_count: 0,
        logit_mismatch_count: 0,
        max_value_absolute_delta: 0.0,
        max_logit_absolute_delta: 0.0,
        max_value_ulp_delta: 0,
        max_logit_ulp_delta: 0,
        first_mismatch: None,
        inference_path_note: "NativeCheckpointInferenceV1 scores its retained tensor by calling the same NativePolicyValueNetV1::forward_v1 used by this audit".to_owned(),
    };
    for group in groups {
        for example in &group.examples {
            let output = model.forward_v1(example.tensor.view_v1())?;
            if output.logits.len() != example.old_policy_logits_f32_bits.len() {
                return Err(invalid_data_v1("transport audit logit count mismatch").into());
            }
            audit.decision_row_count += 1;
            audit.logit_value_count += u64::try_from(output.logits.len())?;
            let mut row_exact = true;
            let exported_value = f32::from_bits(example.old_value_f32_bits);
            if output.value.to_bits() != example.old_value_f32_bits {
                row_exact = false;
                audit.value_mismatch_count += 1;
                let mismatch =
                    mismatch_record_v1(example, "value", None, exported_value, output.value);
                if mismatch.absolute_delta > transport_bound_v1(exported_value) {
                    return Err(invalid_data_v1(format!(
                        "g384 transported value exceeds envelope at teacher decision {}: delta={} bound={}",
                        example.teacher_decision_ordinal,
                        mismatch.absolute_delta,
                        transport_bound_v1(exported_value)
                    ))
                    .into());
                }
                audit.max_value_absolute_delta =
                    audit.max_value_absolute_delta.max(mismatch.absolute_delta);
                audit.max_value_ulp_delta = audit.max_value_ulp_delta.max(mismatch.ulp_delta);
                if audit.first_mismatch.is_none() {
                    audit.first_mismatch = Some(mismatch);
                }
            }
            for (action_index, (exported_bits, recomputed)) in example
                .old_policy_logits_f32_bits
                .iter()
                .copied()
                .zip(output.logits.iter().copied())
                .enumerate()
            {
                if recomputed.to_bits() == exported_bits {
                    continue;
                }
                row_exact = false;
                audit.logit_mismatch_count += 1;
                let exported = f32::from_bits(exported_bits);
                let mismatch =
                    mismatch_record_v1(example, "logit", Some(action_index), exported, recomputed);
                if mismatch.absolute_delta > transport_bound_v1(exported) {
                    return Err(invalid_data_v1(format!(
                        "g384 transported logit exceeds envelope at teacher decision {} action {}: delta={} bound={}",
                        example.teacher_decision_ordinal,
                        action_index,
                        mismatch.absolute_delta,
                        transport_bound_v1(exported)
                    ))
                    .into());
                }
                audit.max_logit_absolute_delta =
                    audit.max_logit_absolute_delta.max(mismatch.absolute_delta);
                audit.max_logit_ulp_delta = audit.max_logit_ulp_delta.max(mismatch.ulp_delta);
                if audit.first_mismatch.is_none() {
                    audit.first_mismatch = Some(mismatch);
                }
            }
            if row_exact {
                audit.bit_exact_decision_row_count += 1;
            } else {
                audit.mismatched_decision_row_count += 1;
            }
        }
    }
    Ok(audit)
}

fn evaluate_groups_v1(
    model: &NativePolicyValueNetV1,
    groups: &[&TeacherPhysicalDecisionV1],
) -> DynResultV1<GroupedMetricsV1> {
    if groups.is_empty() {
        return Err(invalid_data_v1("metric split has no physical decisions").into());
    }
    let mut overall = MetricAccumulatorV1::default();
    let mut by_kind = BTreeMap::<String, MetricAccumulatorV1>::new();
    for group in groups {
        let mut group_nll = 0.0_f64;
        let mut group_correct = true;
        let mut substep_correct_count = 0_u64;
        for example in &group.examples {
            let output = model.forward_v1(example.tensor.view_v1())?;
            if output.logits.len() != usize::try_from(example.legal_action_count)? {
                return Err(invalid_data_v1("tensor action count differs from teacher row").into());
            }
            let (nll, correct) = selected_nll_and_top1_v1(&output.logits, example.selected_index)?;
            group_nll += nll;
            group_correct &= correct;
            substep_correct_count += u64::from(correct);
        }
        for accumulator in [
            &mut overall,
            by_kind.entry(group.decision_kind.clone()).or_default(),
        ] {
            accumulator.physical_group_count += 1;
            accumulator.substep_count += u64::try_from(group.examples.len())?;
            accumulator.nll_sum += group_nll;
            accumulator.substep_top1_correct += substep_correct_count;
            accumulator.physical_top1_correct += u64::from(group_correct);
        }
    }
    Ok(GroupedMetricsV1 {
        overall: overall.finish_v1(),
        by_decision_kind: by_kind
            .into_iter()
            .map(|(kind, accumulator)| (kind, accumulator.finish_v1()))
            .collect(),
    })
}

fn split_groups_v1<'a>(
    dataset: &'a TeacherDatasetV1,
    modulus: u64,
    remainder: u64,
) -> DynResultV1<(
    Vec<&'a TeacherPhysicalDecisionV1>,
    Vec<&'a TeacherPhysicalDecisionV1>,
    Vec<u64>,
    Vec<u64>,
)> {
    if modulus < 2 || remainder >= modulus {
        return Err(invalid_data_v1("invalid pair modulo heldout split").into());
    }
    let mut train = Vec::new();
    let mut heldout = Vec::new();
    let mut train_pairs = BTreeSet::new();
    let mut heldout_pairs = BTreeSet::new();
    for group in &dataset.groups {
        if group.pair_index % modulus == remainder {
            heldout.push(group);
            heldout_pairs.insert(group.pair_index);
        } else {
            train.push(group);
            train_pairs.insert(group.pair_index);
        }
    }
    if train.is_empty() || heldout.is_empty() {
        return Err(invalid_data_v1("pair split produced an empty partition").into());
    }
    Ok((
        train,
        heldout,
        train_pairs.into_iter().collect(),
        heldout_pairs.into_iter().collect(),
    ))
}

#[derive(Clone, Debug)]
struct ExpectedForwardBitsV1 {
    logits: Vec<u32>,
    value: u32,
}

fn train_batch_v1(
    state: &mut NativePolicyValueTrainStateV1,
    batch: &[&TeacherPhysicalDecisionV1],
    learning_rate: f32,
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
    let physical = substeps
        .iter()
        .map(|substeps| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: 0,
        })
        .collect::<Vec<_>>();
    let result = state.behavior_clone_step_v1(&physical, learning_rate)?;
    if result.value_sum.to_bits() & 0x7fff_ffff != 0 {
        return Err(invalid_data_v1("behavior clone step produced value loss").into());
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TrialReportV1 {
    learning_rate: f32,
    epochs: u32,
    final_adam_step: u64,
    model_parameter_sha256: String,
    native_state_sha256: String,
    metrics: SplitMetricsV1,
}

fn train_trial_v1(
    base: &NativePolicyValueTrainStateV1,
    train: &[&TeacherPhysicalDecisionV1],
    heldout: &[&TeacherPhysicalDecisionV1],
    learning_rate: f32,
    epochs: u32,
    batch_groups: usize,
) -> DynResultV1<(NativePolicyValueTrainStateV1, TrialReportV1)> {
    if !learning_rate.is_finite() || learning_rate <= 0.0 || epochs == 0 || batch_groups == 0 {
        return Err(invalid_data_v1("invalid behavior clone trial configuration").into());
    }
    let mut state = base.clone();
    for _ in 0..epochs {
        for batch in train.chunks(batch_groups) {
            train_batch_v1(&mut state, batch, learning_rate)?;
        }
    }
    let metrics = SplitMetricsV1 {
        train: evaluate_groups_v1(state.model_v1(), train)?,
        heldout: evaluate_groups_v1(state.model_v1(), heldout)?,
    };
    let report = TrialReportV1 {
        learning_rate,
        epochs,
        final_adam_step: state.adam_step_v1(),
        model_parameter_sha256: state.model_v1().parameter_manifest_sha256_v1(),
        native_state_sha256: lower_hex_raw32_v1(state.state_sha256_v1()?),
        metrics,
    };
    Ok((state, report))
}

#[derive(Clone, Debug)]
struct TrainConfigV1 {
    teacher_jsonl: PathBuf,
    teacher_jsonl_sha256: String,
    source_store_root: PathBuf,
    output_dir: PathBuf,
    learning_rates: Vec<f32>,
    epoch_grid: Vec<u32>,
    batch_groups: usize,
    holdout_modulus: u64,
    holdout_remainder: u64,
}

impl Default for TrainConfigV1 {
    fn default() -> Self {
        Self {
            teacher_jsonl: PathBuf::new(),
            teacher_jsonl_sha256: String::new(),
            source_store_root: PathBuf::new(),
            output_dir: PathBuf::new(),
            learning_rates: vec![1.0e-5, 3.0e-5],
            epoch_grid: vec![1, 3],
            batch_groups: 64,
            holdout_modulus: 4,
            holdout_remainder: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceBindingManifestV1 {
    run_sha256: String,
    generation: u64,
    checkpoint_sha256: String,
    sidecar_sha256: String,
    payload_sha256: String,
    train_state_sha256: String,
    model_parameter_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TeacherBindingManifestV1 {
    jsonl_sha256: String,
    export_contract: String,
    decision_row_count: usize,
    terminal_row_count: usize,
    pair_indices: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SplitManifestV1 {
    identity: String,
    modulus: u64,
    heldout_remainder: u64,
    train_pair_indices: Vec<u64>,
    heldout_pair_indices: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TrainingManifestV1 {
    objective: String,
    optimizer: String,
    optimizer_reset: bool,
    value_gradient: String,
    batch_physical_decisions: usize,
    learning_rate_grid: Vec<f32>,
    epoch_grid: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    teacher: TeacherBindingManifestV1,
    split: SplitManifestV1,
    training: TrainingManifestV1,
    source_transport_audit: ForwardTransportAuditV1,
    initial_metrics: SplitMetricsV1,
    trials: Vec<TrialReportV1>,
    selected_trial_index: usize,
    payload: PayloadManifestV1,
    validation_nonclaim: String,
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

fn publish_derivative_v1(
    config: &TrainConfigV1,
    dataset: &TeacherDatasetV1,
    train_pairs: Vec<u64>,
    heldout_pairs: Vec<u64>,
    source_transport_audit: ForwardTransportAuditV1,
    initial_metrics: SplitMetricsV1,
    trials: Vec<TrialReportV1>,
    selected_trial_index: usize,
    selected_state: &NativePolicyValueTrainStateV1,
) -> DynResultV1<DerivativeManifestV1> {
    fs::create_dir(&config.output_dir)?;
    let snapshot = selected_state.snapshot_v1()?;
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
    let manifest = DerivativeManifestV1 {
        schema: DERIVATIVE_SCHEMA_V1.to_owned(),
        source: SourceBindingManifestV1 {
            run_sha256: SOURCE_RUN_SHA256_V1.to_owned(),
            generation: SOURCE_GENERATION_V1,
            checkpoint_sha256: SOURCE_CHECKPOINT_SHA256_V1.to_owned(),
            sidecar_sha256: SOURCE_SIDECAR_SHA256_V1.to_owned(),
            payload_sha256: SOURCE_PAYLOAD_SHA256_V1.to_owned(),
            train_state_sha256: SOURCE_TRAIN_STATE_SHA256_V1.to_owned(),
            model_parameter_sha256: SOURCE_MODEL_PARAMETER_SHA256_V1.to_owned(),
        },
        teacher: TeacherBindingManifestV1 {
            jsonl_sha256: dataset.teacher_jsonl_sha256.clone(),
            export_contract: TEACHER_EXPORT_CONTRACT_V1.to_owned(),
            decision_row_count: dataset.decision_row_count,
            terminal_row_count: dataset.terminal_row_count,
            pair_indices: dataset.pair_indices.clone(),
        },
        split: SplitManifestV1 {
            identity: SPLIT_IDENTITY_V1.to_owned(),
            modulus: config.holdout_modulus,
            heldout_remainder: config.holdout_remainder,
            train_pair_indices: train_pairs,
            heldout_pair_indices: heldout_pairs,
        },
        training: TrainingManifestV1 {
            objective: OBJECTIVE_V1.to_owned(),
            optimizer: "native-adam-canonical-scorer-bias-gauge-v1".to_owned(),
            optimizer_reset: true,
            value_gradient: "exact_zero".to_owned(),
            batch_physical_decisions: config.batch_groups,
            learning_rate_grid: config.learning_rates.clone(),
            epoch_grid: config.epoch_grid.clone(),
        },
        source_transport_audit,
        initial_metrics,
        trials,
        selected_trial_index,
        payload,
        validation_nonclaim: "heldout pairs select the grid winner and are not an unbiased final play-strength estimate".to_owned(),
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

fn expected_payload_digests_from_manifest_v1(
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

fn load_cp7_behavior_clone_bundle_v1(
    directory: &Path,
) -> DynResultV1<(
    NativePolicyValueTrainStateV1,
    DerivativeManifestV1,
    [u8; 32],
)> {
    let manifest_bytes = fs::read(directory.join(DERIVATIVE_MANIFEST_FILENAME_V1))?;
    let manifest_sha256 = sha256_v1(&manifest_bytes);
    let manifest: DerivativeManifestV1 = serde_json::from_slice(&manifest_bytes)?;
    if manifest.schema != DERIVATIVE_SCHEMA_V1
        || manifest.source.run_sha256 != SOURCE_RUN_SHA256_V1
        || manifest.source.generation != SOURCE_GENERATION_V1
        || manifest.source.checkpoint_sha256 != SOURCE_CHECKPOINT_SHA256_V1
        || manifest.source.sidecar_sha256 != SOURCE_SIDECAR_SHA256_V1
        || manifest.source.payload_sha256 != SOURCE_PAYLOAD_SHA256_V1
        || manifest.source.train_state_sha256 != SOURCE_TRAIN_STATE_SHA256_V1
        || manifest.source.model_parameter_sha256 != SOURCE_MODEL_PARAMETER_SHA256_V1
        || manifest.teacher.export_contract != TEACHER_EXPORT_CONTRACT_V1
        || manifest.split.identity != SPLIT_IDENTITY_V1
        || manifest.training.objective != OBJECTIVE_V1
        || manifest.training.optimizer != "native-adam-canonical-scorer-bias-gauge-v1"
        || !manifest.training.optimizer_reset
        || manifest.training.value_gradient != "exact_zero"
        || manifest.payload.filename != DERIVATIVE_PAYLOAD_FILENAME_V1
        || manifest.payload.byte_count != NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1
        || manifest.selected_trial_index >= manifest.trials.len()
        || manifest.trials.iter().any(|trial| {
            parse_lower_hex_raw32_v1(&trial.model_parameter_sha256).is_err()
                || parse_lower_hex_raw32_v1(&trial.native_state_sha256).is_err()
        })
    {
        return Err(invalid_data_v1("invalid CP7 derivative manifest binding").into());
    }
    let selected = &manifest.trials[manifest.selected_trial_index];
    if selected.final_adam_step != manifest.payload.adam_step
        || selected.model_parameter_sha256 != manifest.payload.model_parameter_sha256
        || selected.native_state_sha256 != manifest.payload.native_state_sha256
    {
        return Err(invalid_data_v1("selected CP7 trial is not bound to payload").into());
    }
    let payload_bytes = fs::read(directory.join(&manifest.payload.filename))?;
    if payload_bytes.len() != manifest.payload.byte_count
        || sha256_v1(&payload_bytes) != parse_lower_hex_raw32_v1(&manifest.payload.payload_sha256)?
    {
        return Err(invalid_data_v1("CP7 derivative payload digest mismatch").into());
    }
    let expected = expected_payload_digests_from_manifest_v1(&manifest.payload)?;
    let decoded = decode_native_train_state_payload_verified_v1(
        &payload_bytes,
        manifest.payload.adam_step,
        manifest.payload.scorer_bias_anchor_f32_bits,
        &expected,
    )?;
    let mut template =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    // The promoted model's frozen scorer-bias gauge anchor is part of the
    // transported parameter snapshot and need not equal runner_fixed's local
    // template anchor. Install the verified parameters before reconstructing
    // optimizer state so from_snapshot validates against the transported gauge.
    template.replace_parameter_snapshot_v1(&decoded.snapshot.parameters)?;
    let state = NativePolicyValueTrainStateV1::from_snapshot_v1(template, &decoded.snapshot)?;
    if state.model_v1().parameter_manifest_sha256_v1() != manifest.payload.model_parameter_sha256
        || state.state_sha256_v1()? != expected.native_state_sha256
    {
        return Err(invalid_data_v1("loaded CP7 derivative semantic digest mismatch").into());
    }
    Ok((state, manifest, manifest_sha256))
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

pub(crate) struct NativeCp7BehaviorCloneInferenceOutputV1 {
    logits: Vec<f32>,
    value: f32,
}

impl NativeCp7BehaviorCloneInferenceOutputV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        &self.logits
    }

    pub(crate) fn value_v1(&self) -> f32 {
        self.value
    }
}

/// Move-only scorer loaded from a verified raw CP7 behavior-clone derivative.
/// It deliberately exposes only immutable scoring and identity accessors.
pub(crate) struct NativeCp7BehaviorCloneInferenceV1 {
    state: NativePolicyValueTrainStateV1,
    manifest_sha256: [u8; 32],
    payload_sha256: [u8; 32],
    native_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    adam_step: u64,
}

impl NativeCp7BehaviorCloneInferenceV1 {
    pub(crate) fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeCp7BehaviorCloneInferenceOutputV1, ()> {
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
        Ok(NativeCp7BehaviorCloneInferenceOutputV1 {
            logits: output.logits,
            value: output.value,
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

    pub(crate) const fn adam_step_v1(&self) -> u64 {
        self.adam_step
    }
}

pub(crate) fn load_cp7_behavior_clone_inference_v1(
    directory: &Path,
) -> DynResultV1<NativeCp7BehaviorCloneInferenceV1> {
    let (state, manifest, manifest_sha256) = load_cp7_behavior_clone_bundle_v1(directory)?;
    Ok(NativeCp7BehaviorCloneInferenceV1 {
        manifest_sha256,
        payload_sha256: parse_lower_hex_raw32_v1(&manifest.payload.payload_sha256)?,
        native_state_sha256: parse_lower_hex_raw32_v1(&manifest.payload.native_state_sha256)?,
        model_parameter_sha256: parse_lower_hex_raw32_v1(&manifest.payload.model_parameter_sha256)?,
        adam_step: manifest.payload.adam_step,
        state,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Cp7BehaviorCloneCheckpointSummaryV1 {
    pub schema: String,
    pub model_parameter_sha256: String,
    pub native_state_sha256: String,
    pub adam_step: u64,
    pub selected_learning_rate: f32,
    pub selected_epochs: u32,
    pub heldout_mean_nll_per_physical_group: f64,
    pub heldout_substep_top1_accuracy: f64,
}

pub fn verify_cp7_behavior_clone_checkpoint_v1(
    directory: impl AsRef<Path>,
) -> DynResultV1<Cp7BehaviorCloneCheckpointSummaryV1> {
    let directory = directory.as_ref();
    let (state, manifest, _) = load_cp7_behavior_clone_bundle_v1(directory)?;
    let selected = &manifest.trials[manifest.selected_trial_index];
    Ok(Cp7BehaviorCloneCheckpointSummaryV1 {
        schema: manifest.schema,
        model_parameter_sha256: state.model_v1().parameter_manifest_sha256_v1(),
        native_state_sha256: lower_hex_raw32_v1(state.state_sha256_v1()?),
        adam_step: state.adam_step_v1(),
        selected_learning_rate: selected.learning_rate,
        selected_epochs: selected.epochs,
        heldout_mean_nll_per_physical_group: selected
            .metrics
            .heldout
            .overall
            .mean_nll_per_physical_group,
        heldout_substep_top1_accuracy: selected.metrics.heldout.overall.substep_top1_accuracy,
    })
}

fn parse_csv_v1<T>(value: &str, name: &str) -> DynResultV1<Vec<T>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
{
    let parsed = value
        .split(',')
        .map(str::parse)
        .collect::<Result<Vec<T>, _>>()?;
    if parsed.is_empty() {
        return Err(invalid_data_v1(format!("{name} must not be empty")).into());
    }
    Ok(parsed)
}

fn next_arg_v1(iterator: &mut impl Iterator<Item = OsString>, flag: &str) -> DynResultV1<OsString> {
    iterator
        .next()
        .ok_or_else(|| invalid_data_v1(format!("missing value after {flag}")))
        .map_err(Into::into)
}

fn parse_train_config_v1<I>(arguments: I) -> DynResultV1<TrainConfigV1>
where
    I: IntoIterator<Item = OsString>,
{
    let mut config = TrainConfigV1::default();
    let mut iterator = arguments.into_iter();
    while let Some(argument) = iterator.next() {
        let flag = argument
            .to_str()
            .ok_or_else(|| invalid_data_v1("CLI flag is not UTF-8"))?;
        match flag {
            "--teacher-jsonl" => config.teacher_jsonl = next_arg_v1(&mut iterator, flag)?.into(),
            "--teacher-jsonl-sha256" => {
                config.teacher_jsonl_sha256 = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("teacher JSONL SHA-256 is not UTF-8"))?
                    .to_owned()
            }
            "--source-store-root" => {
                config.source_store_root = next_arg_v1(&mut iterator, flag)?.into()
            }
            "--output-dir" => config.output_dir = next_arg_v1(&mut iterator, flag)?.into(),
            "--learning-rates" => {
                let value = next_arg_v1(&mut iterator, flag)?;
                config.learning_rates = parse_csv_v1(
                    value
                        .to_str()
                        .ok_or_else(|| invalid_data_v1("learning rates are not UTF-8"))?,
                    flag,
                )?;
            }
            "--epoch-grid" => {
                let value = next_arg_v1(&mut iterator, flag)?;
                config.epoch_grid = parse_csv_v1(
                    value
                        .to_str()
                        .ok_or_else(|| invalid_data_v1("epoch grid is not UTF-8"))?,
                    flag,
                )?;
            }
            "--batch-groups" => {
                config.batch_groups = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("batch groups are not UTF-8"))?
                    .parse()?;
            }
            "--holdout-modulus" => {
                config.holdout_modulus = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("holdout modulus is not UTF-8"))?
                    .parse()?;
            }
            "--holdout-remainder" => {
                config.holdout_remainder = next_arg_v1(&mut iterator, flag)?
                    .to_str()
                    .ok_or_else(|| invalid_data_v1("holdout remainder is not UTF-8"))?
                    .parse()?;
            }
            "--help" | "-h" => {
                return Err(invalid_data_v1(
                    "usage: cp7_behavior_clone_v1 --teacher-jsonl PATH --teacher-jsonl-sha256 HEX --source-store-root PATH --output-dir PATH [--learning-rates 1e-5,3e-5] [--epoch-grid 1,3] [--batch-groups 64] [--holdout-modulus 4] [--holdout-remainder 0]",
                )
                .into())
            }
            _ => return Err(invalid_data_v1(format!("unknown CLI flag {flag}")).into()),
        }
    }
    if config.teacher_jsonl.as_os_str().is_empty()
        || config.teacher_jsonl_sha256.is_empty()
        || config.source_store_root.as_os_str().is_empty()
        || config.output_dir.as_os_str().is_empty()
    {
        return Err(invalid_data_v1(
            "--teacher-jsonl, --teacher-jsonl-sha256, --source-store-root, and --output-dir are required",
        )
        .into());
    }
    parse_lower_hex_raw32_v1(&config.teacher_jsonl_sha256)?;
    if config
        .learning_rates
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || config.epoch_grid.iter().any(|value| *value == 0)
        || config.batch_groups == 0
    {
        return Err(invalid_data_v1("invalid training grid").into());
    }
    Ok(config)
}

fn run_training_v1(config: TrainConfigV1) -> DynResultV1<Cp7BehaviorCloneCheckpointSummaryV1> {
    let dataset = load_teacher_dataset_v1(&config.teacher_jsonl)?;
    if dataset.teacher_jsonl_sha256 != config.teacher_jsonl_sha256 {
        return Err(invalid_data_v1("teacher JSONL SHA-256 authority mismatch").into());
    }
    let base = load_source_train_state_v1(&config.source_store_root)?;
    let (train, heldout, train_pairs, heldout_pairs) =
        split_groups_v1(&dataset, config.holdout_modulus, config.holdout_remainder)?;
    eprintln!(
        "loaded CP7 teacher rows={} physical_groups={} train_groups={} heldout_groups={} pairs={}",
        dataset.decision_row_count,
        dataset.groups.len(),
        train.len(),
        heldout.len(),
        dataset.pair_indices.len()
    );
    let all_groups = dataset.groups.iter().collect::<Vec<_>>();
    let source_transport_audit = audit_exported_g384_transport_v1(base.model_v1(), &all_groups)?;
    eprintln!(
        "g384 transport audit exact_rows={} mismatched_rows={} value_mismatches={} logit_mismatches={} max_value_delta={} max_logit_delta={}",
        source_transport_audit.bit_exact_decision_row_count,
        source_transport_audit.mismatched_decision_row_count,
        source_transport_audit.value_mismatch_count,
        source_transport_audit.logit_mismatch_count,
        source_transport_audit.max_value_absolute_delta,
        source_transport_audit.max_logit_absolute_delta
    );
    let initial_metrics = SplitMetricsV1 {
        train: evaluate_groups_v1(base.model_v1(), &train)?,
        heldout: evaluate_groups_v1(base.model_v1(), &heldout)?,
    };

    let mut trials: Vec<TrialReportV1> = Vec::new();
    let mut selected: Option<(NativePolicyValueTrainStateV1, usize)> = None;
    for learning_rate in &config.learning_rates {
        for epochs in &config.epoch_grid {
            eprintln!(
                "starting CP7 behavior-clone trial learning_rate={} epochs={}",
                learning_rate, epochs
            );
            let (state, report) = train_trial_v1(
                &base,
                &train,
                &heldout,
                *learning_rate,
                *epochs,
                config.batch_groups,
            )?;
            let candidate_index = trials.len();
            let candidate_nll = report.metrics.heldout.overall.mean_nll_per_physical_group;
            eprintln!(
                "finished CP7 behavior-clone trial learning_rate={} epochs={} heldout_mean_nll={} heldout_top1={}",
                learning_rate,
                epochs,
                candidate_nll,
                report.metrics.heldout.overall.substep_top1_accuracy
            );
            let replace = selected.as_ref().is_none_or(|(_, selected_index)| {
                let selected_nll = trials[*selected_index]
                    .metrics
                    .heldout
                    .overall
                    .mean_nll_per_physical_group;
                candidate_nll < selected_nll
            });
            trials.push(report);
            if replace {
                selected = Some((state, candidate_index));
            }
        }
    }
    let (selected_state, selected_trial_index) =
        selected.ok_or_else(|| invalid_data_v1("training grid produced no trials"))?;
    publish_derivative_v1(
        &config,
        &dataset,
        train_pairs,
        heldout_pairs,
        source_transport_audit,
        initial_metrics,
        trials,
        selected_trial_index,
        &selected_state,
    )?;
    verify_cp7_behavior_clone_checkpoint_v1(&config.output_dir)
}

pub fn run_cp7_behavior_clone_cli_v1<I>(arguments: I) -> DynResultV1<()>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let summary =
        if arguments.first().and_then(|value| value.to_str()) == Some("--verify-checkpoint") {
            if arguments.len() != 2 {
                return Err(
                    invalid_data_v1("--verify-checkpoint requires exactly one directory").into(),
                );
            }
            verify_cp7_behavior_clone_checkpoint_v1(PathBuf::from(&arguments[1]))?
        } else {
            run_training_v1(parse_train_config_v1(arguments)?)?
        };
    println!("{}", serde_json::to_string(&summary)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires MTG_KERNEL_PROMOTED_G384_STORE_ROOT"]
    fn external_promoted_g384_source_loads_with_adam_reset() {
        let root = std::env::var_os("MTG_KERNEL_PROMOTED_G384_STORE_ROOT")
            .expect("MTG_KERNEL_PROMOTED_G384_STORE_ROOT is set");
        let state = load_source_train_state_v1(Path::new(&root)).unwrap();
        assert_eq!(state.adam_step_v1(), 0);
        assert_eq!(
            state.model_v1().parameter_manifest_sha256_v1(),
            SOURCE_MODEL_PARAMETER_SHA256_V1
        );
        assert!(state
            .first_moment_snapshot_v1()
            .iter()
            .flat_map(|parameter| &parameter.values)
            .all(|value| value.to_bits() == 0));
        assert!(state
            .second_moment_snapshot_v1()
            .iter()
            .flat_map(|parameter| &parameter.values)
            .all(|value| value.to_bits() == 0));
    }
}
