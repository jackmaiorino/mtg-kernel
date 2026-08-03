//! Strict additive stack loader for complete-history structured residual stages.

use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2, NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
    NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2, NATIVE_FLAT_EDGE_FEATURE_DIM_V2,
    NATIVE_FLAT_OBJECT_FEATURE_DIM_V2, NATIVE_FLAT_STATE_FEATURE_DIM_V2,
};
use crate::native_structured_policy_residual_v1::{
    average_structured_residuals_v1, decode_structured_residual_parameters_v1,
    history_residual_parameter_layout_sha256_v1, lower_hex_v1, parse_lower_hex32_v1, raw_sha256_v1,
    structured_policy_residual_output_v1, structured_residual_v1, NativeStructuredHistoryEntryV1,
    NativeStructuredPolicyResidualOutputV1, CARD_EMBEDDING_DIM_V1, CARD_VOCAB_V1,
    GROUP_EMBEDDING_DIM_V1, HIDDEN_DIM_V1, HISTORY_FEATURE_DIM_V1, HISTORY_GROUP_VOCAB_V1,
    HISTORY_LENGTH_V1, HISTORY_PARAMETER_COUNT_V1, HISTORY_ROLE_DIM_V1, PARENT_ADAM_STEP_V1,
    PARENT_DIRECTORY_V1, PARENT_MANIFEST_FILENAME_V1, PARENT_MANIFEST_SHA256_V1,
    PARENT_MODEL_PARAMETER_SHA256_V1, PARENT_NATIVE_STATE_SHA256_V1, PARENT_PAYLOAD_SHA256_V1,
    PARENT_STATE_FILENAME_V1, PUBLICATION_ENCODING_V1, WEIGHTS_ENCODING_V1,
};
use crate::native_xmage_cp7_outcome_reinforce_v1::{
    load_xmage_cp7_outcome_inference_v1, NativeXmageCp7OutcomeInferenceV1,
};
use crate::rl::parse_strict_json_value;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::io;
use std::mem::size_of;
use std::path::Path;

const STACK_MANIFEST_FILENAME_V1: &str = "structured_history_stack.json";
const STACK_SCHEMA_V1: &str = "mtg-kernel-structured-history-stack-candidate/v1";
const STACK_ARCHITECTURE_V1: &str =
    "complete-public-history-structured-object-action-attention-policy-value-additive-stack/v1";
const STACK_MEMBER_ARCHITECTURE_V1: &str =
    "complete-public-history-structured-object-action-attention-policy-value-residual/v1";
const STACK_STAGE_WEIGHTING_V1: &str = "equal-average/v1";
const STACK_WEIGHTS_DIRECTORY_V1: &str = "weights";
const STACK_STAGE_DIRECTORY_PREFIX_V1: &str = "stage-";
const STACK_MEMBER_FILENAME_PREFIX_V1: &str = "member-";
const STACK_MEMBER_COUNT_V1: usize = 4;
const STACK_COMPOSITE_DOMAIN_V1: &[u8] = b"mtg-kernel-structured-history-stack-composite-model/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackManifestV1 {
    schema: String,
    publication_encoding: String,
    parent: StackParentBindingV1,
    architecture: StackArchitectureBindingV1,
    weights: StackWeightsBindingV1,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackParentBindingV1 {
    directory: String,
    manifest_sha256: String,
    payload_sha256: String,
    native_state_sha256: String,
    model_parameter_sha256: String,
    adam_step: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackArchitectureBindingV1 {
    identity: String,
    member_identity: String,
    state_dim: usize,
    object_dim: usize,
    edge_dim: usize,
    action_dim: usize,
    ref_dim: usize,
    hidden_dim: usize,
    card_vocab: usize,
    card_embedding_dim: usize,
    group_vocab: usize,
    group_embedding_dim: usize,
    history_length: usize,
    history_feature_dim: usize,
    history_role_dim: usize,
    stage_member_count: usize,
    stage_weighting: String,
    value_model: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackWeightsBindingV1 {
    directory: String,
    encoding: String,
    sha256: String,
    parameter_count: usize,
    parameter_layout_sha256: String,
    stages: Vec<StackStageBindingV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackStageBindingV1 {
    ordinal: usize,
    directory: String,
    #[serde(default)]
    scale: Option<f32>,
    members: Vec<StackMemberBindingV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackMemberBindingV1 {
    ordinal: usize,
    filename: String,
    sha256: String,
    byte_count: usize,
}

#[derive(Debug)]
struct StackMemberV1 {
    ordinal: usize,
    sha256: [u8; 32],
    parameters: BTreeMap<String, crate::native_structured_policy_residual_v1::TensorV1>,
}

#[derive(Debug)]
struct StackStageV1 {
    ordinal: usize,
    scale: Option<f32>,
    members: Vec<StackMemberV1>,
}

pub(crate) struct NativeStructuredHistoryStackInferenceV1 {
    parent: NativeXmageCp7OutcomeInferenceV1,
    manifest_sha256: [u8; 32],
    weights_sha256: [u8; 32],
    composite_model_parameter_sha256: [u8; 32],
    stages: Vec<StackStageV1>,
}

impl NativeStructuredHistoryStackInferenceV1 {
    pub(crate) const fn manifest_sha256_v1(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) const fn weights_sha256_v1(&self) -> [u8; 32] {
        self.weights_sha256
    }

    pub(crate) const fn composite_model_parameter_sha256_v1(&self) -> [u8; 32] {
        self.composite_model_parameter_sha256
    }

    pub(crate) const fn parent_adam_step_v1(&self) -> u64 {
        self.parent.adam_step_v1()
    }

    pub(crate) fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
        self.score_decision_with_history_v1(decision, &[], 0)
    }

    pub(crate) fn score_decision_with_history_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
        let action_count = decision.actions().len();
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
        let parent = self
            .parent
            .score_encoded_decision_v1(tensor_view_v1(&tensor))?;
        if parent.logits_v1().len() != action_count {
            return Err(());
        }
        self.score_tensor_v1(parent, tensor, action_count, history, acting_player)
    }

    fn score_tensor_v1(
        &self,
        parent: crate::native_xmage_cp7_outcome_reinforce_v1::NativeXmageCp7OutcomeInferenceOutputV1,
        tensor: NativeFlatDecisionTensorV2,
        action_count: usize,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
        if acting_player > 1 {
            return Err(());
        }
        let parent_output =
            structured_policy_residual_output_v1(parent.logits_v1().to_vec(), parent.value_v1())?;
        let mut stage_outputs = Vec::with_capacity(self.stages.len());
        for stage in &self.stages {
            let stage_output =
                stage.score_stage_v1(&tensor, action_count, history, acting_player)?;
            stage_outputs.push(stage_output);
        }
        combine_stack_outputs_v1(&parent_output, &stage_outputs)
    }
}

impl StackStageV1 {
    fn score_stage_v1(
        &self,
        tensor: &NativeFlatDecisionTensorV2,
        action_count: usize,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
        if self.members.len() != STACK_MEMBER_COUNT_V1 {
            return Err(());
        }
        let mut residuals = Vec::with_capacity(self.members.len());
        for member in &self.members {
            let residual = structured_residual_v1(
                &member.parameters,
                tensor,
                Some((history, acting_player)),
                HISTORY_GROUP_VOCAB_V1,
            )?;
            if residual.logits_v1().len() != action_count {
                return Err(());
            }
            residuals.push(residual);
        }
        average_structured_residuals_v1(&residuals)
    }
}

fn tensor_view_v1(
    tensor: &NativeFlatDecisionTensorV2,
) -> crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1<'_> {
    crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1::from_slices_unvalidated(
        crate::native_policy_value_net_v1::NativeEncodedDecisionSchemaV1::contract_v1(),
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

fn atom_v1(hasher: &mut Sha256, tag: &[u8], payload: &[u8]) {
    hasher.update((tag.len() as u32).to_be_bytes());
    hasher.update(tag);
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
}

fn exact_directory_inventory_v1(
    root: &Path,
    expected_files: &[&str],
    expected_directories: &[&str],
    label: &'static str,
) -> Result<(), Box<dyn Error>> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("non-UTF8 {label} inventory"),
            )
        })?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            files.insert(name);
        } else if file_type.is_dir() {
            directories.insert(name);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid {label} inventory type"),
            )
            .into());
        }
    }
    let expected_files = expected_files
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    let expected_directories = expected_directories
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if files != expected_files || directories != expected_directories {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} inventory is not exact"),
        )
        .into());
    }
    Ok(())
}

fn stack_stage_directory_v1(stage_ordinal: usize) -> String {
    format!("{STACK_STAGE_DIRECTORY_PREFIX_V1}{stage_ordinal:03}")
}

fn stack_member_filename_v1(member_ordinal: usize) -> String {
    format!("{STACK_MEMBER_FILENAME_PREFIX_V1}{member_ordinal:03}.f32le")
}

fn validate_manifest_v1(manifest: &StackManifestV1) -> Result<[u8; 32], Box<dyn Error>> {
    let expected_layout_sha256 = history_residual_parameter_layout_sha256_v1();
    let expected_parameter_count = HISTORY_PARAMETER_COUNT_V1;
    if manifest.schema != STACK_SCHEMA_V1
        || manifest.publication_encoding != PUBLICATION_ENCODING_V1
        || manifest.parent.directory != PARENT_DIRECTORY_V1
        || manifest.parent.manifest_sha256 != PARENT_MANIFEST_SHA256_V1
        || manifest.parent.payload_sha256 != PARENT_PAYLOAD_SHA256_V1
        || manifest.parent.native_state_sha256 != PARENT_NATIVE_STATE_SHA256_V1
        || manifest.parent.model_parameter_sha256 != PARENT_MODEL_PARAMETER_SHA256_V1
        || manifest.parent.adam_step != PARENT_ADAM_STEP_V1
        || manifest.architecture.identity != STACK_ARCHITECTURE_V1
        || manifest.architecture.member_identity != STACK_MEMBER_ARCHITECTURE_V1
        || manifest.architecture.state_dim != NATIVE_FLAT_STATE_FEATURE_DIM_V2
        || manifest.architecture.object_dim != NATIVE_FLAT_OBJECT_FEATURE_DIM_V2
        || manifest.architecture.edge_dim != NATIVE_FLAT_EDGE_FEATURE_DIM_V2
        || manifest.architecture.action_dim != NATIVE_FLAT_ACTION_FEATURE_DIM_V2
        || manifest.architecture.ref_dim != NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2
        || manifest.architecture.hidden_dim != HIDDEN_DIM_V1
        || manifest.architecture.card_vocab != CARD_VOCAB_V1
        || manifest.architecture.card_embedding_dim != CARD_EMBEDDING_DIM_V1
        || manifest.architecture.group_vocab != HISTORY_GROUP_VOCAB_V1
        || manifest.architecture.group_embedding_dim != GROUP_EMBEDDING_DIM_V1
        || manifest.architecture.history_length != HISTORY_LENGTH_V1
        || manifest.architecture.history_feature_dim != HISTORY_FEATURE_DIM_V1
        || manifest.architecture.history_role_dim != HISTORY_ROLE_DIM_V1
        || manifest.architecture.stage_member_count != STACK_MEMBER_COUNT_V1
        || manifest.architecture.stage_weighting != STACK_STAGE_WEIGHTING_V1
        || manifest.architecture.value_model
            != crate::native_structured_policy_residual_v1::HISTORY_VALUE_MODEL_V1
        || manifest.weights.directory != STACK_WEIGHTS_DIRECTORY_V1
        || manifest.weights.encoding != WEIGHTS_ENCODING_V1
        || manifest.weights.parameter_count != expected_parameter_count
        || manifest.weights.parameter_layout_sha256 != lower_hex_v1(expected_layout_sha256)
        || manifest.weights.sha256.len() != 64
        || manifest.composite_model_parameter_sha256.len() != 64
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack manifest binding mismatch",
        )
        .into());
    }
    if manifest.weights.stages.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack requires at least one stage",
        )
        .into());
    }
    for (stage_index, stage) in manifest.weights.stages.iter().enumerate() {
        if stage.ordinal != stage_index
            || stage.directory != stack_stage_directory_v1(stage_index)
            || stage.members.len() != STACK_MEMBER_COUNT_V1
            || stage
                .scale
                .map(|scale| scale.to_bits() != 1.0f32.to_bits())
                .unwrap_or(false)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "structured history stack stage binding mismatch",
            )
            .into());
        }
        for (member_index, member) in stage.members.iter().enumerate() {
            if member.ordinal != member_index
                || member.filename != stack_member_filename_v1(member_index)
                || member.byte_count != expected_parameter_count * size_of::<f32>()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "structured history stack member binding mismatch",
                )
                .into());
            }
        }
    }
    Ok(expected_layout_sha256)
}

fn validate_stack_parent_inventory_v1(parent: &Path) -> Result<(), Box<dyn Error>> {
    exact_directory_inventory_v1(
        parent,
        &[PARENT_MANIFEST_FILENAME_V1, PARENT_STATE_FILENAME_V1],
        &[],
        "structured history stack parent",
    )
}

fn validate_stack_weights_inventory_v1(
    weights_root: &Path,
    stages: &[StackStageBindingV1],
) -> Result<(), Box<dyn Error>> {
    let expected_directories = stages
        .iter()
        .enumerate()
        .map(|(stage_index, stage)| {
            if stage.ordinal != stage_index {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "structured history stack stage ordinal mismatch",
                ))
            } else {
                Ok(stage.directory.as_str())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    exact_directory_inventory_v1(
        weights_root,
        &[],
        &expected_directories,
        "structured history stack weights",
    )?;
    for stage in stages {
        let expected_files = stage
            .members
            .iter()
            .map(|member| member.filename.as_str())
            .collect::<Vec<_>>();
        exact_directory_inventory_v1(
            &weights_root.join(&stage.directory),
            &expected_files,
            &[],
            "structured history stack stage",
        )?;
    }
    Ok(())
}

fn load_stack_member_v1(
    stage_index: usize,
    member_index: usize,
    weights_root: &Path,
    binding: &StackMemberBindingV1,
    expected_parameter_count: usize,
    weights_hasher: &mut Sha256,
) -> Result<StackMemberV1, Box<dyn Error>> {
    let expected_filename = stack_member_filename_v1(member_index);
    if binding.ordinal != member_index || binding.filename != expected_filename {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack member order mismatch",
        )
        .into());
    }
    let member_path = weights_root
        .join(stack_stage_directory_v1(stage_index))
        .join(&binding.filename);
    let member_bytes = fs::read(member_path)?;
    let member_sha256 = raw_sha256_v1(&member_bytes);
    if lower_hex_v1(member_sha256) != binding.sha256 || member_bytes.len() != binding.byte_count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack member digest mismatch",
        )
        .into());
    }
    if member_bytes.len() != expected_parameter_count * size_of::<f32>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack member byte count mismatch",
        )
        .into());
    }
    weights_hasher.update(&member_bytes);
    Ok(StackMemberV1 {
        ordinal: member_index,
        sha256: member_sha256,
        parameters: decode_structured_residual_parameters_v1(&member_bytes)?,
    })
}

fn stack_composite_sha256_v1(
    parent: &NativeXmageCp7OutcomeInferenceV1,
    layout_sha256: [u8; 32],
    weights_sha256: [u8; 32],
    stages: &[StackStageV1],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    atom_v1(&mut hasher, b"domain", STACK_COMPOSITE_DOMAIN_V1);
    atom_v1(
        &mut hasher,
        b"architecture_identity",
        STACK_ARCHITECTURE_V1.as_bytes(),
    );
    atom_v1(
        &mut hasher,
        b"parent_manifest_sha256",
        parent.manifest_sha256_v1().as_ref(),
    );
    atom_v1(
        &mut hasher,
        b"parent_payload_sha256",
        parent.payload_sha256_v1().as_ref(),
    );
    atom_v1(
        &mut hasher,
        b"parent_native_state_sha256",
        parent.native_state_sha256_v1().as_ref(),
    );
    atom_v1(
        &mut hasher,
        b"parent_model_parameter_sha256",
        parent.model_parameter_sha256_v1().as_ref(),
    );
    atom_v1(
        &mut hasher,
        b"parent_adam_step",
        &parent.adam_step_v1().to_be_bytes(),
    );
    atom_v1(
        &mut hasher,
        b"weights_parameter_layout_sha256",
        &layout_sha256,
    );
    atom_v1(&mut hasher, b"weights_sha256", &weights_sha256);
    atom_v1(
        &mut hasher,
        b"stage_count",
        &(stages.len() as u64).to_be_bytes(),
    );
    for stage in stages {
        atom_v1(
            &mut hasher,
            b"stage_ordinal",
            &(stage.ordinal as u64).to_be_bytes(),
        );
        let scale_bits = stage.scale.unwrap_or(1.0).to_bits();
        atom_v1(&mut hasher, b"stage_scale_f32le", &scale_bits.to_le_bytes());
        atom_v1(
            &mut hasher,
            b"member_count",
            &(stage.members.len() as u64).to_be_bytes(),
        );
        for member in &stage.members {
            atom_v1(
                &mut hasher,
                b"member_ordinal",
                &(member.ordinal as u64).to_be_bytes(),
            );
            atom_v1(&mut hasher, b"member_sha256", &member.sha256);
        }
    }
    hasher.finalize().into()
}

fn combine_stack_outputs_v1(
    parent: &NativeStructuredPolicyResidualOutputV1,
    stage_outputs: &[NativeStructuredPolicyResidualOutputV1],
) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
    let mut logits = parent.logits_v1().to_vec();
    let mut value = parent.value_v1();
    let action_count = logits.len();
    for stage in stage_outputs {
        if stage.logits_v1().len() != action_count {
            return Err(());
        }
        for (destination, source) in logits.iter_mut().zip(stage.logits_v1()) {
            *destination += source;
        }
        value += stage.value_v1();
    }
    if logits.iter().any(|value| !value.is_finite()) || !value.is_finite() {
        return Err(());
    }
    structured_policy_residual_output_v1(logits, value)
}

pub(crate) fn load_native_structured_history_stack_inference_v1(
    root: &Path,
) -> Result<NativeStructuredHistoryStackInferenceV1, Box<dyn Error>> {
    exact_directory_inventory_v1(
        root,
        &[STACK_MANIFEST_FILENAME_V1],
        &[PARENT_DIRECTORY_V1, STACK_WEIGHTS_DIRECTORY_V1],
        "structured history stack root",
    )?;
    let root_manifest = root.join(STACK_MANIFEST_FILENAME_V1);
    let root_manifest_bytes = fs::read(&root_manifest)?;
    let manifest_sha256 = raw_sha256_v1(&root_manifest_bytes);
    let manifest: StackManifestV1 = serde_json::from_value(parse_strict_json_value(
        std::str::from_utf8(&root_manifest_bytes)?,
    )?)?;
    let layout_sha256 = validate_manifest_v1(&manifest)?;
    validate_stack_parent_inventory_v1(&root.join(PARENT_DIRECTORY_V1))?;
    validate_stack_weights_inventory_v1(
        &root.join(STACK_WEIGHTS_DIRECTORY_V1),
        &manifest.weights.stages,
    )?;
    let parent_directory = root.join(PARENT_DIRECTORY_V1);
    let parent = load_xmage_cp7_outcome_inference_v1(&parent_directory)?;
    if parent.manifest_sha256_v1() != parse_lower_hex32_v1(PARENT_MANIFEST_SHA256_V1)?
        || parent.payload_sha256_v1() != parse_lower_hex32_v1(PARENT_PAYLOAD_SHA256_V1)?
        || parent.native_state_sha256_v1() != parse_lower_hex32_v1(PARENT_NATIVE_STATE_SHA256_V1)?
        || parent.model_parameter_sha256_v1()
            != parse_lower_hex32_v1(PARENT_MODEL_PARAMETER_SHA256_V1)?
        || parent.adam_step_v1() != PARENT_ADAM_STEP_V1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack parent mismatch",
        )
        .into());
    }
    let expected_parameter_count = HISTORY_PARAMETER_COUNT_V1;
    let mut weights_hasher = Sha256::new();
    let mut stages = Vec::with_capacity(manifest.weights.stages.len());
    for (stage_index, stage_binding) in manifest.weights.stages.iter().enumerate() {
        let mut members = Vec::with_capacity(STACK_MEMBER_COUNT_V1);
        for (member_index, member_binding) in stage_binding.members.iter().enumerate() {
            let member = load_stack_member_v1(
                stage_index,
                member_index,
                &root.join(STACK_WEIGHTS_DIRECTORY_V1),
                member_binding,
                expected_parameter_count,
                &mut weights_hasher,
            )?;
            members.push(member);
        }
        stages.push(StackStageV1 {
            ordinal: stage_binding.ordinal,
            scale: stage_binding.scale,
            members,
        });
    }
    let weights_sha256: [u8; 32] = weights_hasher.finalize().into();
    if manifest.weights.sha256 != lower_hex_v1(weights_sha256) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack weights digest mismatch",
        )
        .into());
    }
    let composite_model_parameter_sha256 =
        stack_composite_sha256_v1(&parent, layout_sha256, weights_sha256, &stages);
    if manifest.composite_model_parameter_sha256 != lower_hex_v1(composite_model_parameter_sha256) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured history stack composite mismatch",
        )
        .into());
    }
    Ok(NativeStructuredHistoryStackInferenceV1 {
        parent,
        manifest_sha256,
        weights_sha256,
        composite_model_parameter_sha256,
        stages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_structured_policy_residual_v1::{
        average_structured_policy_residual_outputs_v1, structured_policy_residual_output_v1,
    };
    use serde_json::json;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir_v1() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("mtg-kernel-structured-stack-{unique}"))
    }

    fn write_json_v1(path: &Path, value: serde_json::Value) {
        let bytes = serde_json::to_vec_pretty(&value).expect("json");
        fs::write(path, bytes).expect("write json");
    }

    fn write_f32le_v1(path: &Path, values: &[f32]) {
        let mut file = File::create(path).expect("create weights");
        for value in values {
            file.write_all(&value.to_bits().to_le_bytes())
                .expect("write weight");
        }
    }

    fn base_manifest_v1(
        weights_sha256: &str,
        composite_sha256: &str,
        stages: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        json!({
            "schema": STACK_SCHEMA_V1,
            "publication_encoding": PUBLICATION_ENCODING_V1,
            "parent": {
                "directory": PARENT_DIRECTORY_V1,
                "manifest_sha256": PARENT_MANIFEST_SHA256_V1,
                "payload_sha256": PARENT_PAYLOAD_SHA256_V1,
                "native_state_sha256": PARENT_NATIVE_STATE_SHA256_V1,
                "model_parameter_sha256": PARENT_MODEL_PARAMETER_SHA256_V1,
                "adam_step": PARENT_ADAM_STEP_V1,
            },
            "architecture": {
                "identity": STACK_ARCHITECTURE_V1,
                "member_identity": STACK_MEMBER_ARCHITECTURE_V1,
                "state_dim": NATIVE_FLAT_STATE_FEATURE_DIM_V2,
                "object_dim": NATIVE_FLAT_OBJECT_FEATURE_DIM_V2,
                "edge_dim": NATIVE_FLAT_EDGE_FEATURE_DIM_V2,
                "action_dim": NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
                "ref_dim": NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2,
                "hidden_dim": HIDDEN_DIM_V1,
                "card_vocab": CARD_VOCAB_V1,
                "card_embedding_dim": CARD_EMBEDDING_DIM_V1,
                "group_vocab": HISTORY_GROUP_VOCAB_V1,
                "group_embedding_dim": GROUP_EMBEDDING_DIM_V1,
                "history_length": HISTORY_LENGTH_V1,
                "history_feature_dim": HISTORY_FEATURE_DIM_V1,
                "history_role_dim": HISTORY_ROLE_DIM_V1,
                "stage_member_count": STACK_MEMBER_COUNT_V1,
                "stage_weighting": STACK_STAGE_WEIGHTING_V1,
                "value_model": crate::native_structured_policy_residual_v1::HISTORY_VALUE_MODEL_V1,
            },
            "weights": {
                "directory": STACK_WEIGHTS_DIRECTORY_V1,
                "encoding": WEIGHTS_ENCODING_V1,
                "sha256": weights_sha256,
                "parameter_count": HISTORY_PARAMETER_COUNT_V1,
                "parameter_layout_sha256": lower_hex_v1(history_residual_parameter_layout_sha256_v1()),
                "stages": stages,
            },
            "composite_model_parameter_sha256": composite_sha256,
        })
    }

    fn minimal_stage_v1(member_sha256: &str, scale: Option<f32>) -> serde_json::Value {
        json!({
            "ordinal": 0,
            "directory": stack_stage_directory_v1(0),
            "scale": scale,
            "members": [
                {
                    "ordinal": 0,
                    "filename": stack_member_filename_v1(0),
                    "sha256": member_sha256,
                    "byte_count": HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
                {
                    "ordinal": 1,
                    "filename": stack_member_filename_v1(1),
                    "sha256": member_sha256,
                    "byte_count": HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
                {
                    "ordinal": 2,
                    "filename": stack_member_filename_v1(2),
                    "sha256": member_sha256,
                    "byte_count": HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
                {
                    "ordinal": 3,
                    "filename": stack_member_filename_v1(3),
                    "sha256": member_sha256,
                    "byte_count": HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
            ],
        })
    }

    #[test]
    fn identical_members_average_to_the_member_v1() {
        let member = structured_policy_residual_output_v1(vec![4.0, 8.0], 12.0).unwrap();
        let averaged = average_structured_policy_residual_outputs_v1(&[
            member.clone(),
            member.clone(),
            member.clone(),
            member.clone(),
        ])
        .unwrap();
        assert_eq!(averaged.logits_v1(), member.logits_v1());
        assert_eq!(averaged.value_v1(), member.value_v1());
    }

    #[test]
    fn two_stages_add_without_duplication_v1() {
        let parent = structured_policy_residual_output_v1(vec![3.0, 7.0], 11.0).unwrap();
        let stage_one = structured_policy_residual_output_v1(vec![1.0, 2.0], 5.0).unwrap();
        let stage_two = structured_policy_residual_output_v1(vec![-4.0, 6.0], -3.0).unwrap();
        let combined =
            combine_stack_outputs_v1(&parent, &[stage_one.clone(), stage_two.clone()]).unwrap();
        assert_eq!(combined.logits_v1(), &[0.0, 15.0]);
        assert_eq!(combined.value_v1(), 13.0);
    }

    #[test]
    fn manifest_rejects_scale_drift_v1() {
        let manifest = StackManifestV1 {
            schema: STACK_SCHEMA_V1.to_owned(),
            publication_encoding: PUBLICATION_ENCODING_V1.to_owned(),
            parent: StackParentBindingV1 {
                directory: PARENT_DIRECTORY_V1.to_owned(),
                manifest_sha256: PARENT_MANIFEST_SHA256_V1.to_owned(),
                payload_sha256: PARENT_PAYLOAD_SHA256_V1.to_owned(),
                native_state_sha256: PARENT_NATIVE_STATE_SHA256_V1.to_owned(),
                model_parameter_sha256: PARENT_MODEL_PARAMETER_SHA256_V1.to_owned(),
                adam_step: PARENT_ADAM_STEP_V1,
            },
            architecture: StackArchitectureBindingV1 {
                identity: STACK_ARCHITECTURE_V1.to_owned(),
                member_identity: STACK_MEMBER_ARCHITECTURE_V1.to_owned(),
                state_dim: NATIVE_FLAT_STATE_FEATURE_DIM_V2,
                object_dim: NATIVE_FLAT_OBJECT_FEATURE_DIM_V2,
                edge_dim: NATIVE_FLAT_EDGE_FEATURE_DIM_V2,
                action_dim: NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
                ref_dim: NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2,
                hidden_dim: HIDDEN_DIM_V1,
                card_vocab: CARD_VOCAB_V1,
                card_embedding_dim: CARD_EMBEDDING_DIM_V1,
                group_vocab: HISTORY_GROUP_VOCAB_V1,
                group_embedding_dim: GROUP_EMBEDDING_DIM_V1,
                history_length: HISTORY_LENGTH_V1,
                history_feature_dim: HISTORY_FEATURE_DIM_V1,
                history_role_dim: HISTORY_ROLE_DIM_V1,
                stage_member_count: STACK_MEMBER_COUNT_V1,
                stage_weighting: STACK_STAGE_WEIGHTING_V1.to_owned(),
                value_model: crate::native_structured_policy_residual_v1::HISTORY_VALUE_MODEL_V1
                    .to_owned(),
            },
            weights: StackWeightsBindingV1 {
                directory: STACK_WEIGHTS_DIRECTORY_V1.to_owned(),
                encoding: WEIGHTS_ENCODING_V1.to_owned(),
                sha256: "0".repeat(64),
                parameter_count: HISTORY_PARAMETER_COUNT_V1,
                parameter_layout_sha256: lower_hex_v1(history_residual_parameter_layout_sha256_v1()),
                stages: vec![StackStageBindingV1 {
                    ordinal: 0,
                    directory: stack_stage_directory_v1(0),
                    scale: Some(0.5),
                    members: vec![
                        StackMemberBindingV1 {
                            ordinal: 0,
                            filename: stack_member_filename_v1(0),
                            sha256: "0".repeat(64),
                            byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                        },
                        StackMemberBindingV1 {
                            ordinal: 1,
                            filename: stack_member_filename_v1(1),
                            sha256: "0".repeat(64),
                            byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                        },
                        StackMemberBindingV1 {
                            ordinal: 2,
                            filename: stack_member_filename_v1(2),
                            sha256: "0".repeat(64),
                            byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                        },
                        StackMemberBindingV1 {
                            ordinal: 3,
                            filename: stack_member_filename_v1(3),
                            sha256: "0".repeat(64),
                            byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                        },
                    ],
                }],
            },
            composite_model_parameter_sha256: "0".repeat(64),
        };
        assert!(validate_manifest_v1(&manifest).is_err());
    }

    #[test]
    fn root_inventory_rejects_missing_weights_directory_v1() {
        let root = unique_temp_dir_v1();
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join(STACK_MANIFEST_FILENAME_V1), b"{}").expect("write manifest");
        assert!(exact_directory_inventory_v1(
            &root,
            &[STACK_MANIFEST_FILENAME_V1],
            &[PARENT_DIRECTORY_V1, STACK_WEIGHTS_DIRECTORY_V1],
            "structured history stack root",
        )
        .is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stage_inventory_rejects_wrong_member_count_v1() {
        let root = unique_temp_dir_v1();
        let weights_root = root.join(STACK_WEIGHTS_DIRECTORY_V1);
        let stage_root = weights_root.join(stack_stage_directory_v1(0));
        fs::create_dir_all(&stage_root).expect("create stage");
        for index in 0..3 {
            write_f32le_v1(&stage_root.join(stack_member_filename_v1(index)), &[0.0]);
        }
        let stage = StackStageBindingV1 {
            ordinal: 0,
            directory: stack_stage_directory_v1(0),
            scale: None,
            members: vec![
                StackMemberBindingV1 {
                    ordinal: 0,
                    filename: stack_member_filename_v1(0),
                    sha256: "0".repeat(64),
                    byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
                StackMemberBindingV1 {
                    ordinal: 1,
                    filename: stack_member_filename_v1(1),
                    sha256: "0".repeat(64),
                    byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
                StackMemberBindingV1 {
                    ordinal: 2,
                    filename: stack_member_filename_v1(2),
                    sha256: "0".repeat(64),
                    byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
                StackMemberBindingV1 {
                    ordinal: 3,
                    filename: stack_member_filename_v1(3),
                    sha256: "0".repeat(64),
                    byte_count: HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>(),
                },
            ],
        };
        assert!(validate_stack_weights_inventory_v1(&weights_root, &[stage]).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
