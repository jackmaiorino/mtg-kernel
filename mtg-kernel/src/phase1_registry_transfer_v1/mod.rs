//! Explicit, append-only card-registry transfer of real expanded Net8 checkpoints.
//!
//! No filesystem writes, registry changes, training dispatch, or implicit resume.
//! New cards use already allocated embedding rows; feature changes are rejected.

use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::expanded_deck_training_v1::PinnedFileV1;
use crate::native_flat_tensorizer_v3::*;
use crate::native_policy_train_step_v1::{
    native_train_state_parameter_layout_v1, NativePolicyValueTrainSnapshotV1,
    NativePolicyValueTrainStateV1, ADAM_BETA1_V1, ADAM_BETA2_V1, ADAM_EPSILON_V1,
    ADAM_WEIGHT_DECAY_V1, NATIVE_OPTIMIZER_IDENTITY_V1,
};
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1, MODEL_ARCHITECTURE_VERSION_V1,
};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyIdentityV1;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod fresh;
mod strict_registry_json;

pub use fresh::{
    transfer_fresh_expanded_checkpoint_to_current_registry_v1,
    verify_fresh_registry_transfer_artifact_v1, FreshRegistryTransferReceiptV1,
    FreshRegistryTransferRequestV1, FreshVerifiedRegistryTransferV1,
};

const CURRENT_REGISTRY: &[u8] = include_bytes!("../../../data/cards_v1.json");
const MAX_INPUT_BYTES: usize = 512 * 1024 * 1024;
const TRANSFER_SCHEMA: &str = "mtg-kernel-expanded-registry-transfer/v1";
const INITIALIZER: &str = "sha256-card-name-seed-signed24-div2pow28/v1";
const LOSS: &str = "terminal_reinforce_value/v3";

fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn bounded<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, String> {
    require(
        bytes.len() <= MAX_INPUT_BYTES,
        "transfer input exceeds size bound",
    )?;
    serde_json::from_slice(bytes).map_err(|e| format!("invalid transfer input: {e}"))
}

/// Exact existing feature meanings and order. V1 supports no feature remapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTransferFeaturesV1 {
    pub schema_version: String,
    pub registry_version: String,
    pub contract_digest: String,
    pub encoding_digest: String,
    pub source_sha256: String,
    pub descriptor_sha256: String,
}

impl RegistryTransferFeaturesV1 {
    pub fn current_v1() -> Self {
        Self {
            schema_version: FEATURE_SCHEMA_VERSION_V3.into(),
            registry_version: FEATURE_REGISTRY_VERSION_V3.into(),
            contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
            descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
        }
    }
}

/// Caller-selected input pins. These bind bytes, not independent source-history
/// certification; the caller must obtain them from its preserved checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTransferRequestV1 {
    pub source_checkpoint_sha256: String,
    pub source_registry_sha256: String,
    pub source_state_sha256: String,
    pub source_adam_step: u64,
    pub source_card_db_hash: String,
    pub destination_registry_sha256: String,
    pub destination_card_db_hash: String,
    pub features: RegistryTransferFeaturesV1,
    pub initialization_seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCardMappingV1 {
    /// Exact unique registry name, including distinct token/front-face labels.
    pub name: String,
    pub source_card_id: Option<u16>,
    pub destination_card_id: u16,
    pub destination_embedding_row: u32,
    pub definition_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTransferScalarsV1 {
    pub optimizer_identity: String,
    pub adam_beta1_bits: u32,
    pub adam_beta2_bits: u32,
    pub adam_epsilon_bits: u32,
    pub adam_weight_decay_bits: u32,
    pub adam_step: u64,
    pub scorer_bias_anchor_bits: u32,
    pub loss_identity: String,
    pub learning_rate_bits: u32,
    pub value_coefficient_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTransferReceiptV1 {
    pub model_architecture: String,
    pub initializer: String,
    pub mapping_rule: String,
    pub features: RegistryTransferFeaturesV1,
    pub cards: Vec<RegistryCardMappingV1>,
    pub scalars: RegistryTransferScalarsV1,
    pub source_state_sha256: String,
    pub destination_state_sha256: String,
    pub destination_model_parameter_sha256: String,
    pub copied_scalar_count_per_state_section: usize,
    pub initialized_embedding_scalar_count: usize,
    pub destination_build_git_head: String,
}

/// Mirror of the existing *read-only* expanded checkpoint wire fields. It does
/// not loosen the private production reader or publish the old schema anew.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpandedCheckpointInputV1 {
    schema: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    source_import: FrozenPlayPolicyIdentityV1,
    state_sha256: String,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<TensorBitsV1>,
    first_moments: Vec<TensorBitsV1>,
    second_moments: Vec<TensorBitsV1>,
    trajectories: Vec<PinnedFileV1>,
    loss_identity: String,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TensorBitsV1 {
    name: String,
    shape: Vec<usize>,
    values: Vec<u32>,
}

fn native_tensors(saved: &[TensorBitsV1]) -> Result<Vec<NativeNamedParameterV1>, String> {
    let layout = native_train_state_parameter_layout_v1();
    require(saved.len() == layout.len(), "Net8 tensor count differs")?;
    saved
        .iter()
        .zip(layout)
        .map(|(tensor, (name, shape))| {
            require(
                tensor.name == name
                    && tensor.shape == shape
                    && tensor.values.len() == shape.iter().product::<usize>(),
                "Net8 tensor order/name/shape differs",
            )?;
            Ok(NativeNamedParameterV1 {
                name,
                shape: shape.to_vec(),
                values: tensor
                    .values
                    .iter()
                    .map(|bits| f32::from_bits(*bits))
                    .collect(),
            })
        })
        .collect()
}

fn tensor_bits(tensors: &[NativeNamedParameterV1]) -> Vec<TensorBitsV1> {
    tensors
        .iter()
        .map(|tensor| TensorBitsV1 {
            name: tensor.name.into(),
            shape: tensor.shape.clone(),
            values: tensor.values.iter().map(|v| v.to_bits()).collect(),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferEnvelopeV1 {
    schema: String,
    request: RegistryTransferRequestV1,
    receipt: RegistryTransferReceiptV1,
    /// Retained as ancestry, never relabeled as destination-runtime provenance.
    source_import: FrozenPlayPolicyIdentityV1,
    source_trajectories: Vec<PinnedFileV1>,
    parameters: Vec<TensorBitsV1>,
    first_moments: Vec<TensorBitsV1>,
    second_moments: Vec<TensorBitsV1>,
}

/// Opaque validated candidate. No filesystem publication or model installation
/// occurs. The crate-only state accessor is a future explicit trainer seam.
pub struct VerifiedRegistryTransferV1 {
    envelope: TransferEnvelopeV1,
    state: NativePolicyValueTrainStateV1,
}

impl VerifiedRegistryTransferV1 {
    pub(crate) fn model_v1(&self) -> &NativePolicyValueNetV1 {
        self.state.model_v1()
    }

    pub(crate) fn source_import_v1(&self) -> &FrozenPlayPolicyIdentityV1 {
        &self.envelope.source_import
    }

    pub fn receipt_v1(&self) -> &RegistryTransferReceiptV1 {
        &self.envelope.receipt
    }
    pub fn request_v1(&self) -> &RegistryTransferRequestV1 {
        &self.envelope.request
    }

    pub fn artifact_bytes_v1(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(&self.envelope).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Consumes validated state and preserves settings with it. This is not
    /// connected to the current training command or default checkpoint reader.
    #[allow(dead_code)]
    pub(crate) fn into_train_state_v1(
        self,
    ) -> (NativePolicyValueTrainStateV1, RegistryTransferScalarsV1) {
        (self.state, self.envelope.receipt.scalars)
    }
}

fn registry_cards(bytes: &[u8]) -> Result<Vec<Value>, String> {
    require(
        bytes.len() <= MAX_INPUT_BYTES,
        "registry input exceeds size bound",
    )?;
    let value = strict_registry_json::read(bytes)?;
    let cards = value
        .get("cards")
        .and_then(Value::as_array)
        .ok_or("registry cards absent")?;
    require(
        !cards.is_empty() && cards.len() < CARD_VOCAB_SIZE_V1,
        "registry exceeds the existing u16 card vocabulary",
    )?;
    let mut names = BTreeSet::new();
    for card in cards {
        let name = card
            .get("name")
            .and_then(Value::as_str)
            .ok_or("card name absent")?;
        require(
            !name.is_empty() && name.len() <= 1024 && name.trim() == name && names.insert(name),
            "ambiguous or duplicate stable card name",
        )?;
    }
    Ok(cards.clone())
}

fn mapping(source: &[Value], destination: &[Value]) -> Result<Vec<RegistryCardMappingV1>, String> {
    require(
        source.len() <= destination.len(),
        "card removal is unsupported",
    )?;
    let mut result = Vec::with_capacity(destination.len());
    for (id, card) in destination.iter().enumerate() {
        if let Some(old) = source.get(id) {
            require(
                old == card,
                "shared card reordered, renamed, or definition changed",
            )?;
        }
        let name = card["name"].as_str().ok_or("card name absent")?;
        result.push(RegistryCardMappingV1 {
            name: name.into(),
            source_card_id: (id < source.len()).then_some(id as u16),
            destination_card_id: id as u16,
            destination_embedding_row: id as u32 + 1,
            definition_sha256: sha(&serde_json::to_vec(card).map_err(|e| e.to_string())?),
        });
    }
    Ok(result)
}

/// Stable per-card initializer, independent of registry length and placement.
/// Exactly representable signed 24-bit integer divided by 2^28, in [-1/32,1/32).
/// Uses no host RNG, transcendental function, or default model initializer.
fn initial_card_row(name: &str, seed: u64) -> [f32; CARD_EMBEDDING_DIM_V1] {
    std::array::from_fn(|column| {
        let mut digest = Sha256::new();
        digest.update(INITIALIZER.as_bytes());
        digest.update([0]);
        digest.update(seed.to_le_bytes());
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((column as u64).to_le_bytes());
        let bits = digest.finalize();
        let integer = i32::from_le_bytes([bits[0], bits[1], bits[2], 0]) - (1 << 23);
        integer as f32 * (1.0 / 268_435_456.0)
    })
}

fn restore(
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> Result<NativePolicyValueTrainStateV1, String> {
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|e| e.to_string())?;
    // Set the source model before reconstruction so a valid non-default gauge
    // anchor is preserved. Snapshot validation still enforces the exact gauge.
    model
        .replace_parameter_snapshot_v1(&snapshot.parameters)
        .map_err(|e| e.to_string())?;
    NativePolicyValueTrainStateV1::from_snapshot_v1(model, snapshot).map_err(|e| e.to_string())
}

/// Transfer a pinned existing V3 expanded checkpoint to this build's registry.
/// Shared card records and all feature meanings must be exactly unchanged.
pub fn transfer_expanded_checkpoint_to_current_registry_v1(
    checkpoint_bytes: &[u8],
    source_registry_bytes: &[u8],
    request: &RegistryTransferRequestV1,
) -> Result<VerifiedRegistryTransferV1, String> {
    require(
        checkpoint_bytes.len() <= MAX_INPUT_BYTES && source_registry_bytes.len() <= MAX_INPUT_BYTES,
        "transfer input exceeds size bound",
    )?;
    require(
        sha(checkpoint_bytes) == request.source_checkpoint_sha256,
        "source checkpoint SHA256 differs",
    )?;
    require(
        sha(source_registry_bytes) == request.source_registry_sha256,
        "source registry SHA256 differs",
    )?;
    require(
        request.destination_registry_sha256 == sha(CURRENT_REGISTRY)
            && request.destination_card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}"),
        "destination is not the compiled registry",
    )?;
    require(
        request.features == RegistryTransferFeaturesV1::current_v1(),
        "feature migration is unsupported; exact current V3 identities required",
    )?;

    let saved: ExpandedCheckpointInputV1 = bounded(checkpoint_bytes)?;
    require(
        saved.schema == "mtg-kernel-expanded-deck-checkpoint/v1" && saved.loss_identity == LOSS,
        "unsupported checkpoint or training objective",
    )?;
    require(
        saved.state_sha256 == request.source_state_sha256
            && saved.adam_step == request.source_adam_step
            && saved.card_db_hash == request.source_card_db_hash,
        "source state, optimizer step, or engine identity differs",
    )?;
    let source = registry_cards(source_registry_bytes)?;
    let destination = registry_cards(CURRENT_REGISTRY)?;
    require(
        destination.len() == CARD_DEFS.len()
            && destination
                .iter()
                .zip(CARD_DEFS.iter())
                .all(|(card, def)| card["name"] == def.name),
        "compiled card IDs differ from embedded registry",
    )?;
    require(
        saved.source_import.destination_registry_sha256 == request.source_registry_sha256
            && saved.source_import.destination_card_db_hash == saved.card_db_hash
            && saved.source_import.destination_card_count == source.len(),
        "checkpoint is not bound to the supplied source card registry",
    )?;
    require(
        saved.feature_contract_digest == request.features.contract_digest
            && saved.feature_encoding_digest == request.features.encoding_digest
            && saved.source_import.feature_contract_digest == request.features.contract_digest
            && saved.source_import.feature_encoding_digest == request.features.encoding_digest,
        "source feature contract or encoding differs",
    )?;
    let observation = saved
        .source_import
        .observation_successor
        .as_ref()
        .ok_or("source checkpoint lacks explicit V3 observation transfer provenance")?;
    require(
        observation.schema == "mtg-kernel-frozen-play-observation-transfer/v3"
            && observation.destination.expected_feature_contract_digest
                == request.features.contract_digest
            && observation.destination.expected_feature_encoding_digest
                == request.features.encoding_digest
            && observation.features_source_sha256 == request.features.source_sha256
            && observation.feature_descriptor_sha256 == request.features.descriptor_sha256,
        "source feature source/descriptor differs",
    )?;
    let cards = mapping(&source, &destination)?;
    let learning_rate = f32::from_bits(saved.learning_rate_bits);
    let value_coefficient = f32::from_bits(saved.value_coefficient_bits);
    require(
        learning_rate.is_finite()
            && learning_rate > 0.0
            && value_coefficient.is_finite()
            && value_coefficient > 0.0,
        "invalid saved optimizer scalar settings",
    )?;
    let mut snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters: native_tensors(&saved.parameters)?,
        first_moments: native_tensors(&saved.first_moments)?,
        second_moments: native_tensors(&saved.second_moments)?,
    };
    // The existing native digest validates every float, second moment, padding
    // row, gauge and optimizer-step bound before a transfer is permitted.
    let source_state_digest = snapshot.state_sha256_v1().map_err(|e| e.to_string())?;
    require(
        hex_digest(&source_state_digest) == saved.state_sha256,
        "source state digest differs",
    )?;
    for card in cards.iter().filter(|card| card.source_card_id.is_none()) {
        let begin = card.destination_embedding_row as usize * CARD_EMBEDDING_DIM_V1;
        let end = begin + CARD_EMBEDDING_DIM_V1;
        require(
            snapshot.first_moments[0].values[begin..end]
                .iter()
                .chain(&snapshot.second_moments[0].values[begin..end])
                .all(|v| v.to_bits() == 0),
            "new card row contains learned optimizer state; refusing to discard it",
        )?;
        snapshot.parameters[0].values[begin..end]
            .copy_from_slice(&initial_card_row(&card.name, request.initialization_seed));
        snapshot.first_moments[0].values[begin..end].fill(0.0);
        snapshot.second_moments[0].values[begin..end].fill(0.0);
    }
    let state = restore(&snapshot)?;
    let initialized = (destination.len() - source.len()) * CARD_EMBEDDING_DIM_V1;
    let scalars = RegistryTransferScalarsV1 {
        optimizer_identity: NATIVE_OPTIMIZER_IDENTITY_V1.into(),
        adam_beta1_bits: ADAM_BETA1_V1.to_bits(),
        adam_beta2_bits: ADAM_BETA2_V1.to_bits(),
        adam_epsilon_bits: ADAM_EPSILON_V1.to_bits(),
        adam_weight_decay_bits: ADAM_WEIGHT_DECAY_V1.to_bits(),
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        loss_identity: saved.loss_identity,
        learning_rate_bits: saved.learning_rate_bits,
        value_coefficient_bits: saved.value_coefficient_bits,
    };
    let receipt = RegistryTransferReceiptV1 {
        model_architecture: MODEL_ARCHITECTURE_VERSION_V1.into(), initializer: INITIALIZER.into(),
        mapping_rule: "exact-shared-card-record-prefix; card_token=card_db_id+1; all-feature-identities-unchanged".into(),
        features: request.features.clone(), cards, scalars,
        source_state_sha256: saved.state_sha256,
        destination_state_sha256: hex_digest(&state.state_sha256_v1().map_err(|e| e.to_string())?),
        destination_model_parameter_sha256: state.model_v1().parameter_manifest_sha256_v1(),
        copied_scalar_count_per_state_section: snapshot.parameters.iter().map(|p| p.values.len()).sum::<usize>() - initialized,
        initialized_embedding_scalar_count: initialized,
        destination_build_git_head: option_env!("MTG_KERNEL_BUILD_GIT_HEAD").unwrap_or("unavailable").into(),
    };
    let envelope = TransferEnvelopeV1 {
        schema: TRANSFER_SCHEMA.into(),
        request: request.clone(),
        receipt,
        source_import: saved.source_import,
        source_trajectories: saved.trajectories,
        parameters: tensor_bits(&snapshot.parameters),
        first_moments: tensor_bits(&snapshot.first_moments),
        second_moments: tensor_bits(&snapshot.second_moments),
    };
    Ok(VerifiedRegistryTransferV1 { envelope, state })
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Recreate the transfer from its pinned preserved inputs before accepting a
/// persisted candidate. Edited mappings, scalars, parameters or provenance fail.
pub fn verify_registry_transfer_artifact_v1(
    artifact_bytes: &[u8],
    expected_artifact_sha256: &str,
    source_checkpoint_bytes: &[u8],
    source_registry_bytes: &[u8],
) -> Result<VerifiedRegistryTransferV1, String> {
    require(
        artifact_bytes.len() <= MAX_INPUT_BYTES && sha(artifact_bytes) == expected_artifact_sha256,
        "transfer artifact SHA256 differs",
    )?;
    let envelope: TransferEnvelopeV1 = bounded(artifact_bytes)?;
    require(
        envelope.schema == TRANSFER_SCHEMA,
        "transfer artifact schema differs",
    )?;
    let verified = transfer_expanded_checkpoint_to_current_registry_v1(
        source_checkpoint_bytes,
        source_registry_bytes,
        &envelope.request,
    )?;
    require(
        envelope == verified.envelope,
        "transfer artifact differs from reproduced state or provenance",
    )?;
    Ok(verified)
}

#[cfg(test)]
mod tests;
