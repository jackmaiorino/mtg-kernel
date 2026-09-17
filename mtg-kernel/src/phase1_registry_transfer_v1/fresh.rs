//! Fresh-origin (no imported ancestry rewrite) explicit registry transfer.
//!
//! Template: the sibling imported-only transfer in this module's parent file.
//! A fresh transfer retains every preallocated parameter and moment,
//! including newly registered rows, and never samples a new embedding
//! initializer. The read-only zero-moment check on newly registered rows is
//! kept as a consistency gate: it never mutates state, it only refuses a
//! declared source registry that does not match this checkpoint's actual
//! training history.

use super::{
    hex_digest, mapping, native_tensors, registry_cards, require, restore, sha, tensor_bits,
    RegistryCardMappingV1, RegistryTransferFeaturesV1, RegistryTransferScalarsV1, TensorBitsV1,
    CURRENT_REGISTRY, LOSS, MAX_INPUT_BYTES,
};
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::expanded_deck_training_v1::PinnedFileV1;
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1, ADAM_BETA1_V1, ADAM_BETA2_V1,
    ADAM_EPSILON_V1, ADAM_WEIGHT_DECAY_V1, NATIVE_OPTIMIZER_IDENTITY_V1,
};
use crate::native_policy_value_net_v1::{
    CARD_EMBEDDING_DIM_V1, MODEL_ARCHITECTURE_VERSION_V1, NativePolicyValueNetV1,
};
use crate::sideboard_play_policy_v1::FreshPlayPolicyIdentityV1;
use serde::{Deserialize, Serialize};

const FRESH_CHECKPOINT_SCHEMA: &str = "mtg-kernel-expanded-deck-fresh-checkpoint/v1";
const FRESH_TRANSFER_SCHEMA: &str = "mtg-kernel-expanded-fresh-registry-transfer/v1";
const MAPPING_RULE: &str = "exact-shared-card-record-prefix; card_token=card_db_id+1; all-feature-identities-unchanged; preallocated-rows-preserved-unmodified";

/// Mirrors `RegistryTransferRequestV1` with no initialization seed: a fresh
/// transfer samples nothing new, so there is no seed to bind or ignore.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshRegistryTransferRequestV1 {
    pub source_checkpoint_sha256: String,
    pub source_registry_sha256: String,
    pub source_state_sha256: String,
    pub source_adam_step: u64,
    pub source_card_db_hash: String,
    pub destination_registry_sha256: String,
    pub destination_card_db_hash: String,
    pub features: RegistryTransferFeaturesV1,
}

/// Mirror of the read-only fresh expanded checkpoint wire fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshExpandedCheckpointInputV1 {
    schema: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    source_import: FreshPlayPolicyIdentityV1,
    state_sha256: String,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<TensorBitsV1>,
    first_moments: Vec<TensorBitsV1>,
    second_moments: Vec<TensorBitsV1>,
    trajectories: Vec<PinnedFileV1>,
    loss_identity: String,
    // See `phase1_registry_transfer_v1::ExpandedCheckpointInputV1`'s
    // identical fields for why these exist here and are read, then ignored
    // (registry transfer stays v3-only; `loss_identity == LOSS` below is
    // the actual, clear rejection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gamma_bits: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gae_lambda_bits: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entropy_coefficient_bits: Option<u32>,
    learning_rate_bits: u32,
    value_coefficient_bits: u32,
}

/// No `initializer` field: nothing is overwritten, so there is no per-row
/// initializer identity to record. `preserved_new_embedding_scalar_count`
/// replaces the imported receipt's "initialized" count with an honest count
/// of scalars belonging to newly registered rows that were left untouched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshRegistryTransferReceiptV1 {
    pub model_architecture: String,
    pub mapping_rule: String,
    pub features: RegistryTransferFeaturesV1,
    pub cards: Vec<RegistryCardMappingV1>,
    pub scalars: RegistryTransferScalarsV1,
    pub source_state_sha256: String,
    pub destination_state_sha256: String,
    pub destination_model_parameter_sha256: String,
    pub copied_scalar_count_per_state_section: usize,
    pub preserved_new_embedding_scalar_count: usize,
    pub destination_build_git_head: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshTransferEnvelopeV1 {
    schema: String,
    request: FreshRegistryTransferRequestV1,
    receipt: FreshRegistryTransferReceiptV1,
    /// Retained as ancestry, never relabeled as destination-runtime provenance.
    source_import: FreshPlayPolicyIdentityV1,
    source_trajectories: Vec<PinnedFileV1>,
    parameters: Vec<TensorBitsV1>,
    first_moments: Vec<TensorBitsV1>,
    second_moments: Vec<TensorBitsV1>,
}

/// Opaque validated fresh-origin candidate. No filesystem publication or
/// model installation occurs. The crate-only state accessor is a future
/// explicit trainer seam.
pub struct FreshVerifiedRegistryTransferV1 {
    envelope: FreshTransferEnvelopeV1,
    state: NativePolicyValueTrainStateV1,
}

impl FreshVerifiedRegistryTransferV1 {
    pub(crate) fn model_v1(&self) -> &NativePolicyValueNetV1 {
        self.state.model_v1()
    }

    pub(crate) fn source_import_v1(&self) -> &FreshPlayPolicyIdentityV1 {
        &self.envelope.source_import
    }

    pub fn receipt_v1(&self) -> &FreshRegistryTransferReceiptV1 {
        &self.envelope.receipt
    }
    pub fn request_v1(&self) -> &FreshRegistryTransferRequestV1 {
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

/// Transfer a pinned existing fresh-origin V3 expanded checkpoint to this
/// build's registry. Every preallocated row, including not-yet-registered
/// ones, is preserved bit-for-bit; only ancestry consistency is checked.
pub fn transfer_fresh_expanded_checkpoint_to_current_registry_v1(
    checkpoint_bytes: &[u8],
    source_registry_bytes: &[u8],
    request: &FreshRegistryTransferRequestV1,
) -> Result<FreshVerifiedRegistryTransferV1, String> {
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

    let saved: FreshExpandedCheckpointInputV1 = super::bounded(checkpoint_bytes)?;
    require(
        saved.schema == FRESH_CHECKPOINT_SCHEMA && saved.loss_identity == LOSS,
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
    // FreshPlayPolicyIdentityV1 has no observation_successor field at all
    // (fresh identities are natively V3), so there is nothing nested to
    // unwrap here; the flat source-import feature digests are sufficient.
    require(
        saved.feature_contract_digest == request.features.contract_digest
            && saved.feature_encoding_digest == request.features.encoding_digest
            && saved.source_import.feature_contract_digest == request.features.contract_digest
            && saved.source_import.feature_encoding_digest == request.features.encoding_digest
            && saved.source_import.features_source_sha256 == request.features.source_sha256
            && saved.source_import.feature_descriptor_sha256 == request.features.descriptor_sha256,
        "source feature contract, encoding, source or descriptor differs",
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
    let snapshot = NativePolicyValueTrainSnapshotV1 {
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
    // Read-only consistency gate: a newly registered row must still carry
    // positive-zero moments. This never mutates state; it only refuses a
    // declared source registry that does not match this checkpoint's actual
    // training history. Every preallocated row, assigned or not, is already
    // preserved bit-for-bit in `snapshot` because this loop never writes.
    for card in cards.iter().filter(|card| card.source_card_id.is_none()) {
        let begin = card.destination_embedding_row as usize * CARD_EMBEDDING_DIM_V1;
        let end = begin + CARD_EMBEDDING_DIM_V1;
        require(
            snapshot.first_moments[0].values[begin..end]
                .iter()
                .chain(&snapshot.second_moments[0].values[begin..end])
                .all(|v| v.to_bits() == 0),
            "newly registered card row carries learned optimizer state; the declared source registry does not match this checkpoint's training history",
        )?;
    }
    let state = restore(&snapshot)?;
    let preserved_new = (destination.len() - source.len()) * CARD_EMBEDDING_DIM_V1;
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
    let receipt = FreshRegistryTransferReceiptV1 {
        model_architecture: MODEL_ARCHITECTURE_VERSION_V1.into(),
        mapping_rule: MAPPING_RULE.into(),
        features: request.features.clone(),
        cards,
        scalars,
        source_state_sha256: saved.state_sha256,
        destination_state_sha256: hex_digest(&state.state_sha256_v1().map_err(|e| e.to_string())?),
        destination_model_parameter_sha256: state.model_v1().parameter_manifest_sha256_v1(),
        copied_scalar_count_per_state_section: snapshot
            .parameters
            .iter()
            .map(|p| p.values.len())
            .sum::<usize>(),
        preserved_new_embedding_scalar_count: preserved_new,
        destination_build_git_head: option_env!("MTG_KERNEL_BUILD_GIT_HEAD")
            .unwrap_or("unavailable")
            .into(),
    };
    let envelope = FreshTransferEnvelopeV1 {
        schema: FRESH_TRANSFER_SCHEMA.into(),
        request: request.clone(),
        receipt,
        source_import: saved.source_import,
        source_trajectories: saved.trajectories,
        parameters: tensor_bits(&snapshot.parameters),
        first_moments: tensor_bits(&snapshot.first_moments),
        second_moments: tensor_bits(&snapshot.second_moments),
    };
    Ok(FreshVerifiedRegistryTransferV1 { envelope, state })
}

/// Recreate the fresh transfer from its pinned preserved inputs before
/// accepting a persisted candidate. Edited mappings, scalars, parameters or
/// provenance fail, even when only the artifact's outer byte hash is redone.
pub fn verify_fresh_registry_transfer_artifact_v1(
    artifact_bytes: &[u8],
    expected_artifact_sha256: &str,
    source_checkpoint_bytes: &[u8],
    source_registry_bytes: &[u8],
) -> Result<FreshVerifiedRegistryTransferV1, String> {
    require(
        artifact_bytes.len() <= MAX_INPUT_BYTES && sha(artifact_bytes) == expected_artifact_sha256,
        "transfer artifact SHA256 differs",
    )?;
    let envelope: FreshTransferEnvelopeV1 = super::bounded(artifact_bytes)?;
    require(
        envelope.schema == FRESH_TRANSFER_SCHEMA,
        "transfer artifact schema differs",
    )?;
    let verified = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
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
mod tests {
    use super::*;
    use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
    use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
    use crate::native_policy_train_step_v1::{
        NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    };
    use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
    use crate::sideboard_play_policy_v1::FRESH_PLAY_INITIALIZATION_SCHEMA_V1;
    use serde_json::Value;

    const LEARNING_RATE: f32 = 0.00003;
    const VALUE_COEFFICIENT: f32 = 0.75;

    // Real native forward/backward/Adam on a small, declared synthetic V3
    // decision. This verifies state continuation, not legal-game behavior.
    fn update(state: &mut NativePolicyValueTrainStateV1, token: i64) {
        let mut features = vec![0.0; 390];
        features[0] = 0.25;
        features[195] = -0.5;
        let tensor = NativeFlatDecisionTensorV3 {
            common: NativeFlatDecisionTensorV2 {
                state: vec![0.125; 219],
                object_features: vec![0.0625; 98],
                object_card_ids: vec![token],
                object_groups: vec![0],
                object_node_ids: vec![0],
                action_features: features,
                ..Default::default()
            },
        };
        let output = state
            .model_v1()
            .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
            .unwrap();
        let logits: Vec<u32> = output.logits.iter().map(|x| x.to_bits()).collect();
        let steps = [NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(&tensor))),
            selected_action_index: 1,
            expected_raw_action_logit_bits: &logits,
            expected_value_bits: output.value.to_bits(),
        }];
        state
            .train_step_feature_transfer_v3(
                &[NativePolicyPhysicalDecisionV1 {
                    substeps: &steps,
                    terminal_return: 1,
                    baseline_bits: 0,
                }],
                VALUE_COEFFICIENT,
                LEARNING_RATE,
            )
            .unwrap();
    }

    fn learned_state() -> NativePolicyValueTrainStateV1 {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        update(&mut state, 1);
        update(&mut state, 1);
        let snapshot = state.snapshot_v1().unwrap();
        assert!(snapshot
            .first_moments
            .iter()
            .flat_map(|p| &p.values)
            .any(|v| *v != 0.0));
        assert!(snapshot
            .second_moments
            .iter()
            .flat_map(|p| &p.values)
            .any(|v| *v > 0.0));
        state
    }

    fn source_registry(count: usize) -> Vec<u8> {
        let mut registry: Value = serde_json::from_slice(CURRENT_REGISTRY).unwrap();
        registry["cards"].as_array_mut().unwrap().truncate(count);
        serde_json::to_vec(&registry).unwrap()
    }

    /// Metadata-only synthetic fixture, hand-built per fresh_tests convention
    /// (origin.rs's own fresh identity test uses the same all-hex-repeat
    /// style). This does not attest to a generated model, producer execution,
    /// runtime compatibility or playing strength.
    fn fresh_identity(registry: &[u8]) -> FreshPlayPolicyIdentityV1 {
        let count = registry_cards(registry).unwrap().len();
        FreshPlayPolicyIdentityV1 {
            schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
            initialization_manifest_sha256: "a".repeat(64),
            lineage_id: "fresh-registry-transfer-test".into(),
            initializer: "trainer-seeded-v1".into(),
            base_seed: 0,
            model_init_seed: 6_443_515_232_517_447_393,
            seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
            producer_git_commit: "1".repeat(40),
            initial_weights_sha256: "b".repeat(64),
            initial_model_parameter_sha256: "c".repeat(64),
            parameter_layout_sha256: "d".repeat(64),
            destination_registry_sha256: sha(registry),
            destination_card_db_hash: "0123456789abcdef".into(),
            destination_card_count: count,
            feature_contract_digest: RegistryTransferFeaturesV1::current_v1().contract_digest,
            feature_encoding_digest: RegistryTransferFeaturesV1::current_v1().encoding_digest,
            features_source_sha256: RegistryTransferFeaturesV1::current_v1().source_sha256,
            feature_descriptor_sha256: RegistryTransferFeaturesV1::current_v1().descriptor_sha256,
            sampler_identity: crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        }
    }

    fn fresh_checkpoint(
        snapshot: &NativePolicyValueTrainSnapshotV1,
        registry: &[u8],
    ) -> FreshExpandedCheckpointInputV1 {
        let identity = fresh_identity(registry);
        FreshExpandedCheckpointInputV1 {
            schema: FRESH_CHECKPOINT_SCHEMA.into(),
            feature_contract_digest: identity.feature_contract_digest.clone(),
            feature_encoding_digest: identity.feature_encoding_digest.clone(),
            card_db_hash: identity.destination_card_db_hash.clone(),
            source_import: identity,
            state_sha256: hex_digest(&snapshot.state_sha256_v1().unwrap()),
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            parameters: tensor_bits(&snapshot.parameters),
            first_moments: tensor_bits(&snapshot.first_moments),
            second_moments: tensor_bits(&snapshot.second_moments),
            trajectories: vec![PinnedFileV1 {
                path: "preserved-test-trajectory.json".into(),
                sha256: "3".repeat(64),
            }],
            loss_identity: LOSS.into(),
            gamma_bits: None,
            gae_lambda_bits: None,
            entropy_coefficient_bits: None,
            learning_rate_bits: LEARNING_RATE.to_bits(),
            value_coefficient_bits: VALUE_COEFFICIENT.to_bits(),
        }
    }

    fn fresh_request(
        saved: &FreshExpandedCheckpointInputV1,
        bytes: &[u8],
        registry: &[u8],
    ) -> FreshRegistryTransferRequestV1 {
        FreshRegistryTransferRequestV1 {
            source_checkpoint_sha256: sha(bytes),
            source_registry_sha256: sha(registry),
            source_state_sha256: saved.state_sha256.clone(),
            source_adam_step: saved.adam_step,
            source_card_db_hash: saved.card_db_hash.clone(),
            destination_registry_sha256: sha(CURRENT_REGISTRY),
            destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            features: RegistryTransferFeaturesV1::current_v1(),
        }
    }

    fn fresh_transfer(
        snapshot: &NativePolicyValueTrainSnapshotV1,
        source_count: usize,
    ) -> FreshVerifiedRegistryTransferV1 {
        let registry = source_registry(source_count);
        let saved = fresh_checkpoint(snapshot, &registry);
        let bytes = serde_json::to_vec(&saved).unwrap();
        transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&saved, &bytes, &registry),
        )
        .unwrap()
    }

    #[test]
    fn phase1_fresh_registry_identity_preserves_nondefault_gauge_and_exact_adam_continuation() {
        let mut snapshot = learned_state().snapshot_v1().unwrap();
        let gauge = snapshot
            .parameters
            .iter_mut()
            .find(|p| p.name == "scorer.2.bias")
            .unwrap();
        gauge.values[0] = 0.3125;
        snapshot.scorer_bias_anchor_bits = 0.3125_f32.to_bits();
        let mut control = restore(&snapshot).unwrap();
        let candidate = fresh_transfer(&snapshot, CARD_DEFS.len());
        assert_eq!(snapshot, candidate.state.snapshot_v1().unwrap());
        assert_eq!(
            candidate.receipt_v1().preserved_new_embedding_scalar_count,
            0
        );
        let (mut resumed, settings) = candidate.into_train_state_v1();
        assert_eq!(settings.learning_rate_bits, LEARNING_RATE.to_bits());
        assert_eq!(settings.value_coefficient_bits, VALUE_COEFFICIENT.to_bits());
        update(&mut control, 1);
        update(&mut resumed, 1);
        assert_eq!(
            control.snapshot_v1().unwrap(),
            resumed.snapshot_v1().unwrap()
        );
        assert_eq!(resumed.adam_step_v1(), 3);
    }

    #[test]
    fn phase1_fresh_registry_append_preserves_every_tensor_at_every_index_including_new_rows() {
        let mut control = learned_state();
        let before = control.snapshot_v1().unwrap();
        let source_count = CARD_DEFS.len() - 2;
        let candidate = fresh_transfer(&before, source_count);
        let after = candidate.state.snapshot_v1().unwrap();
        // No carve-out: unlike imported, fresh preserves every preallocated
        // row bit-for-bit, including newly registered ones.
        assert_eq!(before, after);
        assert_eq!(
            candidate.receipt_v1().preserved_new_embedding_scalar_count,
            32
        );
        let (mut resumed, _) = candidate.into_train_state_v1();
        update(&mut control, 1);
        update(&mut resumed, 1);
        assert_eq!(
            control.snapshot_v1().unwrap(),
            resumed.snapshot_v1().unwrap()
        );
        assert_eq!(resumed.adam_step_v1(), 3);
    }

    #[test]
    fn phase1_fresh_registry_new_card_row_learns_after_transfer_without_resetting_step() {
        let before = learned_state().snapshot_v1().unwrap();
        let candidate = fresh_transfer(&before, CARD_DEFS.len() - 1);
        let initial = candidate.state.snapshot_v1().unwrap();
        let (mut state, _) = candidate.into_train_state_v1();
        update(&mut state, CARD_DEFS.len() as i64);
        let after = state.snapshot_v1().unwrap();
        let row =
            CARD_DEFS.len() * CARD_EMBEDDING_DIM_V1..(CARD_DEFS.len() + 1) * CARD_EMBEDDING_DIM_V1;
        assert_ne!(
            initial.parameters[0].values[row.clone()],
            after.parameters[0].values[row.clone()]
        );
        assert!(after.first_moments[0].values[row.clone()]
            .iter()
            .any(|v| *v != 0.0));
        assert!(after.second_moments[0].values[row].iter().any(|v| *v > 0.0));
        assert_eq!(after.adam_step, before.adam_step + 1);
    }

    #[test]
    fn phase1_fresh_registry_rejects_removal_reorder_definition_changes_and_duplicate_names() {
        let cards = registry_cards(CURRENT_REGISTRY).unwrap();
        assert!(mapping(&cards, &cards[..cards.len() - 1]).is_err());
        let mut changed = cards.clone();
        changed.swap(0, 1);
        assert!(mapping(&cards, &changed).is_err());
        let mut changed = cards.clone();
        changed[0]["mana_value"] = serde_json::json!(99);
        assert!(mapping(&cards, &changed).is_err());
        let mut duplicate: Value = serde_json::from_slice(CURRENT_REGISTRY).unwrap();
        duplicate["cards"][1]["name"] = duplicate["cards"][0]["name"].clone();
        assert!(registry_cards(&serde_json::to_vec(&duplicate).unwrap()).is_err());
        assert!(registry_cards(br#"{"cards":[{"name":"A","name":"B"}]}"#).is_err());
    }

    #[test]
    fn phase1_fresh_registry_rejects_feature_changes_wrong_pins_and_malformed_state() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len());
        let mut saved = fresh_checkpoint(&snapshot, &registry);
        let bytes = serde_json::to_vec(&saved).unwrap();
        let original = fresh_request(&saved, &bytes, &registry);
        let mut changed = original.clone();
        changed.features.encoding_digest = "0".repeat(64);
        assert!(transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes, &registry, &changed
        )
        .is_err());
        let mut changed = original.clone();
        changed.source_adam_step += 1;
        assert!(transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes, &registry, &changed
        )
        .is_err());
        let mut changed = original;
        changed.source_registry_sha256 = "0".repeat(64);
        assert!(transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes, &registry, &changed
        )
        .is_err());
        saved.second_moments[1].values[0] = (-0.1_f32).to_bits();
        let corrupt = serde_json::to_vec(&saved).unwrap();
        assert!(transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &corrupt,
            &registry,
            &fresh_request(&saved, &corrupt, &registry)
        )
        .is_err());
    }

    #[test]
    fn phase1_fresh_registry_rejects_feature_contract_encoding_source_or_descriptor_mismatches() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len());
        let saved = fresh_checkpoint(&snapshot, &registry);
        // request.features stays RegistryTransferFeaturesV1::current_v1(); each
        // corruption below sets exactly one operand of the require's six-way
        // equality to a different, still hex-valid value, and the outer
        // checkpoint-bytes pin is recomputed from the corrupted bytes so only
        // this one require can fire, never the earlier checkpoint SHA pin.
        let assert_rejected = |mutate: &dyn Fn(&mut FreshExpandedCheckpointInputV1)| {
            let mut changed = saved.clone();
            mutate(&mut changed);
            let bytes = serde_json::to_vec(&changed).unwrap();
            let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
                &bytes,
                &registry,
                &fresh_request(&changed, &bytes, &registry),
            )
            .err()
            .unwrap();
            assert_eq!(
                error,
                "source feature contract, encoding, source or descriptor differs"
            );
        };
        assert_rejected(&|c| c.feature_contract_digest = "9".repeat(64));
        // Not explicitly requested by name, but the same require's sixth
        // operand; included for full coverage of this branch.
        assert_rejected(&|c| c.feature_encoding_digest = "9".repeat(64));
        assert_rejected(&|c| c.source_import.feature_contract_digest = "9".repeat(64));
        assert_rejected(&|c| c.source_import.feature_encoding_digest = "9".repeat(64));
        assert_rejected(&|c| c.source_import.features_source_sha256 = "9".repeat(64));
        assert_rejected(&|c| c.source_import.feature_descriptor_sha256 = "9".repeat(64));
    }

    #[test]
    fn phase1_fresh_registry_rejects_checkpoint_and_destination_pin_mismatches() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len());
        let saved = fresh_checkpoint(&snapshot, &registry);
        let bytes = serde_json::to_vec(&saved).unwrap();
        let original = fresh_request(&saved, &bytes, &registry);

        let mut wrong_checkpoint_pin = original.clone();
        wrong_checkpoint_pin.source_checkpoint_sha256 = "0".repeat(64);
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &wrong_checkpoint_pin,
        )
        .err()
        .unwrap();
        assert_eq!(error, "source checkpoint SHA256 differs");

        let mut wrong_destination_registry = original.clone();
        wrong_destination_registry.destination_registry_sha256 = "0".repeat(64);
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &wrong_destination_registry,
        )
        .err()
        .unwrap();
        assert_eq!(error, "destination is not the compiled registry");

        let mut wrong_destination_card_db = original;
        wrong_destination_card_db.destination_card_db_hash = "0000000000000000".into();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &wrong_destination_card_db,
        )
        .err()
        .unwrap();
        assert_eq!(error, "destination is not the compiled registry");
    }

    #[test]
    fn phase1_fresh_registry_rejects_wrong_checkpoint_schema_and_loss_identity() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len());
        let saved = fresh_checkpoint(&snapshot, &registry);

        let mut wrong_schema = saved.clone();
        wrong_schema.schema = "mtg-kernel-expanded-deck-checkpoint/v1".into();
        let bytes = serde_json::to_vec(&wrong_schema).unwrap();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&wrong_schema, &bytes, &registry),
        )
        .err()
        .unwrap();
        assert_eq!(error, "unsupported checkpoint or training objective");

        let mut wrong_loss = saved;
        wrong_loss.loss_identity = "some-other-training-objective/v1".into();
        let bytes = serde_json::to_vec(&wrong_loss).unwrap();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&wrong_loss, &bytes, &registry),
        )
        .err()
        .unwrap();
        assert_eq!(error, "unsupported checkpoint or training objective");
    }

    #[test]
    fn phase1_fresh_registry_rejects_source_import_destination_binding_mismatches() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len());
        let saved = fresh_checkpoint(&snapshot, &registry);

        let mut wrong_registry_binding = saved.clone();
        wrong_registry_binding.source_import.destination_registry_sha256 = "0".repeat(64);
        let bytes = serde_json::to_vec(&wrong_registry_binding).unwrap();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&wrong_registry_binding, &bytes, &registry),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            "checkpoint is not bound to the supplied source card registry"
        );

        let mut wrong_card_db_binding = saved.clone();
        wrong_card_db_binding.source_import.destination_card_db_hash = "fedcba9876543210".into();
        let bytes = serde_json::to_vec(&wrong_card_db_binding).unwrap();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&wrong_card_db_binding, &bytes, &registry),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            "checkpoint is not bound to the supplied source card registry"
        );

        let mut wrong_card_count_binding = saved;
        wrong_card_count_binding.source_import.destination_card_count += 1;
        let bytes = serde_json::to_vec(&wrong_card_count_binding).unwrap();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&wrong_card_count_binding, &bytes, &registry),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            "checkpoint is not bound to the supplied source card registry"
        );
    }

    #[test]
    fn phase1_fresh_registry_rejects_non_positive_or_non_finite_optimizer_scalars() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len());
        let saved = fresh_checkpoint(&snapshot, &registry);
        let assert_rejected = |mutate: &dyn Fn(&mut FreshExpandedCheckpointInputV1)| {
            let mut changed = saved.clone();
            mutate(&mut changed);
            let bytes = serde_json::to_vec(&changed).unwrap();
            let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
                &bytes,
                &registry,
                &fresh_request(&changed, &bytes, &registry),
            )
            .err()
            .unwrap();
            assert_eq!(error, "invalid saved optimizer scalar settings");
        };
        assert_rejected(&|c| c.learning_rate_bits = 0.0_f32.to_bits());
        assert_rejected(&|c| c.learning_rate_bits = (-1.0_f32).to_bits());
        assert_rejected(&|c| c.learning_rate_bits = f32::NAN.to_bits());
        assert_rejected(&|c| c.value_coefficient_bits = 0.0_f32.to_bits());
        assert_rejected(&|c| c.value_coefficient_bits = (-0.5_f32).to_bits());
        assert_rejected(&|c| c.value_coefficient_bits = f32::INFINITY.to_bits());
    }

    #[test]
    fn phase1_fresh_registry_refuses_to_erase_preexisting_learning_in_new_card_row() {
        let mut state = learned_state();
        update(&mut state, CARD_DEFS.len() as i64);
        let registry = source_registry(CARD_DEFS.len() - 1);
        let saved = fresh_checkpoint(&state.snapshot_v1().unwrap(), &registry);
        let bytes = serde_json::to_vec(&saved).unwrap();
        let error = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&saved, &bytes, &registry),
        )
        .err()
        .unwrap();
        assert!(error.contains("learned optimizer state"), "{error}");
        assert!(
            error.contains("declared source registry does not match this checkpoint's training history"),
            "{error}"
        );
    }

    /// A minimal, valid fresh identity has no observation-successor concept
    /// at all; this proves the transfer function requires none and does not
    /// leave any orphaned requirement for it.
    #[test]
    fn phase1_fresh_registry_transfer_requires_no_observation_successor_provenance() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let candidate = fresh_transfer(&snapshot, CARD_DEFS.len());
        assert!(candidate.source_import_v1().schema == FRESH_PLAY_INITIALIZATION_SCHEMA_V1);
    }

    #[test]
    fn phase1_fresh_registry_artifact_replay_rejects_tampered_state_even_with_new_artifact_digest() {
        let snapshot = learned_state().snapshot_v1().unwrap();
        let registry = source_registry(CARD_DEFS.len() - 1);
        let saved = fresh_checkpoint(&snapshot, &registry);
        let bytes = serde_json::to_vec(&saved).unwrap();
        let candidate = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            &registry,
            &fresh_request(&saved, &bytes, &registry),
        )
        .unwrap();
        let artifact = candidate.artifact_bytes_v1().unwrap();
        let reloaded =
            verify_fresh_registry_transfer_artifact_v1(&artifact, &sha(&artifact), &bytes, &registry)
                .unwrap();
        assert_eq!(
            candidate.state.snapshot_v1().unwrap(),
            reloaded.state.snapshot_v1().unwrap()
        );
        assert_eq!(artifact, reloaded.artifact_bytes_v1().unwrap());
        let mut envelope: FreshTransferEnvelopeV1 = serde_json::from_slice(&artifact).unwrap();
        envelope.first_moments[1].values[0] ^= 1;
        let tampered = serde_json::to_vec(&envelope).unwrap();
        assert!(verify_fresh_registry_transfer_artifact_v1(
            &tampered,
            &sha(&tampered),
            &bytes,
            &registry
        )
        .is_err());
    }
}
