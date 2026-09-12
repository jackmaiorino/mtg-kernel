//! Explicit, inference-only transfer of a frozen Net8 export into the card lane.
//!
//! This is a research adapter, not a Store loader or continuation authority.
//! Original export and Store bytes stay untouched. The caller independently
//! pins the export and its source registry; the receipt declares the changed
//! card database and the preserved card-id prefix. Appended embedding rows are
//! retained exactly as exported and are not claimed to have been trained.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::fast_sampler::{FastCategoricalScratch, FAST_CATEGORICAL_SAMPLER_VERSION};
use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatDecisionEncoderV2,
    FlatEffectSubtypeChangeV2, FlatGlobalsV2, FlatObjectAbilityUseV2, FlatObjectCoreV2,
    FlatObjectGoadV2, FlatObjectSubtypeV2, FlatRelationV2, FlatScorerActionCoreV2,
    FlatScorerActionRefV2, FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
};
use crate::flat_policy_v3::{
    FlatDecisionEncoderV3, FlatScoringDecisionViewV3, FlatScoringExtensionsV3,
};
use crate::native_checkpoint_inference_v1::encoded_decision_view_v1;
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
use crate::native_flat_tensorizer_v3::{
    encoded_decision_view_v3, NativeFlatDecisionTensorV3, NativeFlatTensorizerV3,
    FEATURES_SOURCE_SHA256_V3, FEATURE_CONTRACT_DIGEST_V3, FEATURE_DESCRIPTOR_SHA256_V3,
    FEATURE_ENCODING_DIGEST_V3,
};
use crate::native_policy_train_step_v1::native_train_state_parameter_layout_v1;
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1, FEATURE_CONTRACT_DIGEST_V1,
    FEATURE_ENCODING_DIGEST_V1, MODEL_ARCHITECTURE_VERSION_V1, MODEL_CONFIG_FINGERPRINT_V1,
    PARAMETER_COUNT_V1,
};
use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, PairedBo1PolicyV1};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{
    FastActorResponseV1, FastActorSessionV1, RlSessionError, RlSessionErrorCode,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const DESTINATION_REGISTRY: &[u8] = include_bytes!("../../data/cards_v1.json");
const EXPORT_SCHEMA: &str = "mtg-kernel-native-inference-export/v1";
const PARAMETER_ENCODING: &str = "native-train-state-parameters-section-f32le/v1";
const EXPORT_VALIDATION: &str =
    "full-store-chain-checkpoint-payload-and-inference-validated-by-exporter";
const PARAMETER_BYTES: usize = PARAMETER_COUNT_V1 * 4;

/// Independent input pins. Obtain source registry bytes from the exporter's
/// exact Git commit and retain them alongside this manifest. The registry SHA
/// binds that explicit compatibility evidence, not an inferred current checkout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenPlayPolicyImportV1 {
    pub export_directory: PathBuf,
    pub expected_metadata_sha256: String,
    pub expected_model_parameter_sha256: String,
    pub source_run_path: PathBuf,
    pub source_registry_path: PathBuf,
    pub expected_source_registry_sha256: String,
    pub source_registry_git_commit: String,
    pub expected_destination_card_db_hash: String,
}

/// Required opt-in pins for the successor feature mapping. The source export
/// is still read with its original strict contract by `load_v1`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenPlayObservationTransferV3 {
    pub expected_feature_contract_digest: String,
    pub expected_feature_encoding_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenPlayObservationReceiptV3 {
    pub schema: String,
    pub source_feature_contract_digest: String,
    pub source_feature_encoding_digest: String,
    pub destination: FrozenPlayObservationTransferV3,
    pub features_source_sha256: String,
    pub feature_descriptor_sha256: String,
    pub semantics: String,
}

/// Evidence of weight identity and the explicitly changed environment. This
/// receipt gives no optimizer/resume authority and no cross-deck strength claim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrozenPlayPolicyIdentityV1 {
    pub schema: String,
    pub source_export_schema: String,
    pub source_metadata_sha256: String,
    pub weights_sha256: String,
    pub model_parameter_sha256: String,
    pub source_run_sha256: String,
    pub source_generation: u64,
    pub source_git_commit: String,
    pub source_card_db_hash: String,
    pub destination_card_db_hash: String,
    pub source_registry_sha256: String,
    pub destination_registry_sha256: String,
    pub source_card_count: usize,
    pub destination_card_count: usize,
    pub source_training_deck_ids: Vec<String>,
    pub namespace_rule: String,
    pub appended_rows: String,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub sampler_identity: String,
    pub reader_revalidated_store_chain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_successor: Option<FrozenPlayObservationReceiptV3>,
}

#[derive(Debug, Clone)]
pub struct FrozenPlayDecisionScoresV1 {
    pub logits: Vec<f32>,
    pub value: f32,
}

/// The only input to the model is the existing actor-relative flat projection.
/// This type exposes immutable copied embeddings, never mutable play weights.
pub struct FrozenPlayPolicyV1 {
    model: NativePolicyValueNetV1,
    embeddings: Vec<f32>,
    identity: FrozenPlayPolicyIdentityV1,
    encoder: FlatDecisionEncoderV2,
    owned: OwnedScoringV1,
    tensorizer: NativeFlatTensorizerV2,
    tensor: NativeFlatDecisionTensorV2,
    sampler: FastCategoricalScratch,
    seat_rng: [SplitMix64; 2],
    sampling_initialized: bool,
    successor: Option<FrozenPlaySuccessorStateV3>,
}

#[derive(Default)]
struct FrozenPlaySuccessorStateV3 {
    encoder: FlatDecisionEncoderV3,
    extensions: FlatScoringExtensionsV3,
    tensorizer: NativeFlatTensorizerV3,
    tensor: NativeFlatDecisionTensorV3,
}

impl FrozenPlayPolicyV1 {
    pub fn load_v1(input: &FrozenPlayPolicyImportV1) -> Result<Self, String> {
        let metadata_bytes = read_bounded(&input.export_directory.join("metadata.json"), 65_536)?;
        require(
            hash(&metadata_bytes) == input.expected_metadata_sha256,
            "export metadata SHA differs",
        )?;
        let metadata: Value = serde_json::from_slice(&metadata_bytes).map_err(|e| e.to_string())?;
        require(
            string(&metadata, "/schema")? == EXPORT_SCHEMA,
            "export schema differs",
        )?;
        require(
            string(&metadata, "/parameter_encoding")? == PARAMETER_ENCODING,
            "parameter encoding differs",
        )?;
        require(
            number(&metadata, "/parameter_byte_count")? == PARAMETER_BYTES as u64,
            "parameter count differs",
        )?;
        for (path, expected) in [
            ("/architecture_identity", MODEL_ARCHITECTURE_VERSION_V1),
            ("/model_config_fingerprint", MODEL_CONFIG_FINGERPRINT_V1),
            ("/feature_contract_digest", FEATURE_CONTRACT_DIGEST_V1),
            ("/feature_encoding_digest", FEATURE_ENCODING_DIGEST_V1),
            ("/source_validation", EXPORT_VALIDATION),
        ] {
            require(
                string(&metadata, path)? == expected,
                &format!("inference contract differs: {path}"),
            )?;
        }
        require(
            string(&metadata, "/identity/model_parameter_sha256")?
                == input.expected_model_parameter_sha256,
            "independent model identity differs",
        )?;
        require(
            string(&metadata, "/source/checkpoint/expected_model_sha256")?
                == input.expected_model_parameter_sha256,
            "export source model identity differs",
        )?;
        require(
            string(&metadata, "/exporter_build/source_git_commit")?
                == input.source_registry_git_commit,
            "registry provenance does not name exporter commit",
        )?;
        require(
            input.expected_destination_card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}"),
            "destination card database differs from the intended transfer",
        )?;

        let source_run_bytes = read_bounded(&input.source_run_path, 1_048_576)?;
        let source_run_sha = string(&metadata, "/identity/loaded_run_sha256")?;
        require(
            hash(&source_run_bytes) == source_run_sha,
            "source run SHA differs from export",
        )?;
        let source_run: Value =
            serde_json::from_slice(&source_run_bytes).map_err(|e| e.to_string())?;
        require(
            string(&source_run, "/contracts/model/architecture_identity")?
                == MODEL_ARCHITECTURE_VERSION_V1,
            "source run model architecture differs",
        )?;
        require(
            string(&source_run, "/contracts/tensorizer/feature_contract_digest")?
                == FEATURE_CONTRACT_DIGEST_V1
                && string(&source_run, "/contracts/tensorizer/feature_encoding_digest")?
                    == FEATURE_ENCODING_DIGEST_V1,
            "source run feature contracts differ",
        )?;

        let source_registry_bytes = read_bounded(&input.source_registry_path, 4_194_304)?;
        require(
            hash(&source_registry_bytes) == input.expected_source_registry_sha256,
            "source registry SHA differs",
        )?;
        let source_registry: Value =
            serde_json::from_slice(&source_registry_bytes).map_err(|e| e.to_string())?;
        let destination_registry: Value =
            serde_json::from_slice(DESTINATION_REGISTRY).map_err(|e| e.to_string())?;
        let (source_card_count, destination_card_count) =
            validate_card_namespace(&source_registry, &destination_registry)?;

        let raw = read_bounded(
            &input.export_directory.join("parameters.f32le"),
            PARAMETER_BYTES,
        )?;
        require(raw.len() == PARAMETER_BYTES, "parameter byte count differs")?;
        let weights_sha = hash(&raw);
        require(
            weights_sha == string(&metadata, "/parameter_section_sha256")?,
            "parameter SHA differs",
        )?;
        let parameters = decode_parameters(&raw)?;
        let embeddings = parameters
            .iter()
            .find(|p| p.name == "card_embedding.weight")
            .ok_or("card embedding tensor missing")?
            .values
            .clone();
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .map_err(|e| format!("model construction: {e:?}"))?;
        model
            .replace_parameter_snapshot_v1(&parameters)
            .map_err(|e| format!("parameter validation: {e:?}"))?;
        require(
            model.parameter_manifest_sha256_v1() == input.expected_model_parameter_sha256,
            "named-parameter digest differs",
        )?;
        let training_decks = source_run
            .pointer("/environment/deck_ids")
            .and_then(Value::as_array)
            .ok_or("source run deck ids missing")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "invalid source deck id".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let identity = FrozenPlayPolicyIdentityV1 {
            schema: "mtg-kernel-frozen-sideboard-play-transfer/v1".into(),
            source_export_schema: EXPORT_SCHEMA.into(),
            source_metadata_sha256: input.expected_metadata_sha256.clone(),
            weights_sha256: weights_sha,
            model_parameter_sha256: input.expected_model_parameter_sha256.clone(),
            source_run_sha256: source_run_sha.into(),
            source_generation: number(&metadata, "/identity/loaded_generation")?,
            source_git_commit: input.source_registry_git_commit.clone(),
            source_card_db_hash: string(&source_run, "/environment/card_db_hash_u64_hex")?.into(),
            destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            source_registry_sha256: input.expected_source_registry_sha256.clone(),
            destination_registry_sha256: hash(DESTINATION_REGISTRY),
            source_card_count,
            destination_card_count,
            source_training_deck_ids: training_decks,
            namespace_rule: "card-token=id+1; original registry entries identical except deck membership; append-only".into(),
            appended_rows: "unchanged exported rows; no training or strength claim for appended cards".into(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1.into(),
            sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION.into(),
            reader_revalidated_store_chain: false,
            observation_successor: None,
        };
        Ok(Self {
            model,
            embeddings,
            identity,
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: None,
        })
    }

    pub fn load_feature_transfer_v3(
        input: &FrozenPlayPolicyImportV1,
        transfer: &FrozenPlayObservationTransferV3,
    ) -> Result<Self, String> {
        require(
            transfer.expected_feature_contract_digest == FEATURE_CONTRACT_DIGEST_V3
                && transfer.expected_feature_encoding_digest == FEATURE_ENCODING_DIGEST_V3,
            "explicit V3 destination feature identity differs",
        )?;
        let mut policy = Self::load_v1(input)?;
        policy.identity.observation_successor = Some(FrozenPlayObservationReceiptV3 {
            schema: "mtg-kernel-frozen-play-observation-transfer/v3".into(),
            source_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1.into(),
            source_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1.into(),
            destination: transfer.clone(),
            features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
            feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
            semantics: "rich V6 / flat V3; exact public historical sources, chooser-only unordered library candidates, typed object-cost prefixes; unchanged weights and dimensions; inference transfer only, no learned competence claim".into(),
        });
        policy.identity.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into();
        policy.identity.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V3.into();
        policy.successor = Some(FrozenPlaySuccessorStateV3::default());
        Ok(policy)
    }

    pub fn identity_v1(&self) -> &FrozenPlayPolicyIdentityV1 {
        &self.identity
    }
    pub fn embedding_rows_v1(&self) -> &[f32] {
        &self.embeddings
    }
    pub const fn embedding_dim_v1(&self) -> usize {
        CARD_EMBEDDING_DIM_V1
    }

    /// Kernel card ids map to embedding tokens by adding one. Token zero is
    /// the fixed padding/unknown row; all ids must exist in this destination.
    pub fn card_embedding_v1(&self, card_id: u16) -> Result<&[f32], String> {
        require(
            usize::from(card_id) < self.identity.destination_card_count,
            "card id outside destination registry",
        )?;
        let start = (usize::from(card_id) + 1) * CARD_EMBEDDING_DIM_V1;
        Ok(&self.embeddings[start..start + CARD_EMBEDDING_DIM_V1])
    }

    pub fn reset_sampling_v1(&mut self, seeds: [u64; 2]) {
        self.seat_rng = [SplitMix64::seed(seeds[0]), SplitMix64::seed(seeds[1])];
        self.sampling_initialized = true;
        self.encoder = FlatDecisionEncoderV2::default();
        self.owned = OwnedScoringV1::default();
        self.tensorizer = NativeFlatTensorizerV2::new();
        self.tensor = NativeFlatDecisionTensorV2::default();
        if self.successor.is_some() {
            self.successor = Some(FrozenPlaySuccessorStateV3::default());
        }
    }

    /// Scores only the actor-visible projection generated by the session.
    pub fn score_fast_session_v1(
        &mut self,
        session: &FastActorSessionV1,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err("cannot score a terminal session".into());
        };
        if let Some(successor) = &mut self.successor {
            let encoded = session
                .encode_current_flat_scoring_decision_owned_v3(
                    decision,
                    &mut successor.encoder,
                    &mut self.owned.buffers(),
                )
                .map_err(|e| format!("V3 actor-visible encoding: {e:?}; decision={decision:?}"))?;
            self.owned.globals = encoded.globals;
            successor.extensions = encoded.extensions;
            return self
                .score_owned()
                .map_err(|error| format!("{error}; decision={decision:?}"));
        }
        let encoded = session
            .encode_current_flat_scoring_decision_owned_v2(
                decision,
                &mut self.encoder,
                &mut self.owned.buffers(),
            )
            .map_err(|e| format!("actor-visible encoding: {e:?}; decision={decision:?}"))?;
        self.owned.globals = encoded.globals;
        self.score_owned()
    }

    pub fn select_fast_session_v1(&mut self, session: &FastActorSessionV1) -> Result<u32, String> {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err("cannot select from a terminal session".into());
        };
        let scores = self.score_fast_session_v1(session)?;
        self.sample_scores(
            &scores.logits,
            decision.acting_player,
            decision.legal_action_count,
        )
    }

    fn score_owned(&mut self) -> Result<FrozenPlayDecisionScoresV1, String> {
        let output = if let Some(successor) = &mut self.successor {
            successor
                .tensorizer
                .fill(
                    FlatScoringDecisionViewV3::new(self.owned.view(), &successor.extensions),
                    &mut successor.tensor,
                )
                .map_err(|e| format!("V3 visible tensorization: {e:?}"))?;
            self.model
                .forward_feature_transfer_v3(encoded_decision_view_v3(&successor.tensor))
                .map_err(|e| format!("explicit V3 frozen feature transfer: {e:?}"))?
        } else {
            self.tensorizer
                .fill(self.owned.view(), &mut self.tensor)
                .map_err(|e| format!("visible tensorization: {e:?}"))?;
            self.model
                .forward_v1(encoded_decision_view_v1(&self.tensor))
                .map_err(|e| format!("frozen scalar inference: {e:?}"))?
        };
        require(
            output.logits.len() == self.owned.actions.len()
                && !output.logits.is_empty()
                && output.logits.iter().all(|x| x.is_finite())
                && output.value.is_finite(),
            "invalid frozen inference output",
        )?;
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }

    fn sample_scores(
        &mut self,
        logits: &[f32],
        seat: PlayerSeatV1,
        legal_count: u32,
    ) -> Result<u32, String> {
        require(
            self.sampling_initialized,
            "reset both policy seeds before a game",
        )?;
        require(
            logits.len() == legal_count as usize,
            "scorer and legal action counts differ",
        )?;
        let index = match seat {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        };
        let seed = self.seat_rng[index].next_u64();
        let selected = self
            .sampler
            .sample(logits, seed)
            .map_err(|e| e.to_string())?;
        u32::try_from(selected).map_err(|e| e.to_string())
    }
}

impl PairedBo1PolicyV1 for FrozenPlayPolicyV1 {
    fn uses_observation_successor_v3(&self) -> bool {
        self.successor.is_some()
    }
    fn reset_for_game_v1(&mut self, policy_seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.reset_sampling_v1(policy_seeds);
        Ok(())
    }

    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        let decision = input.decision();
        if let Some(successor) = &mut self.successor {
            let encoded = input
                .encode_scoring_owned_v3(&mut successor.encoder, &mut self.owned.buffers())
                .map_err(|e| {
                    policy_error(format!(
                        "V3 actor-visible encoding: {e:?}; decision={decision:?}"
                    ))
                })?;
            self.owned.globals = encoded.globals;
            successor.extensions = encoded.extensions;
        } else {
            let encoded = input
                .encode_scoring_owned_v2(&mut self.encoder, &mut self.owned.buffers())
                .map_err(|e| {
                    policy_error(format!(
                        "actor-visible encoding: {e:?}; decision={decision:?}"
                    ))
                })?;
            self.owned.globals = encoded.globals;
        }
        let scores = self
            .score_owned()
            .map_err(|error| policy_error(format!("{error}; decision={decision:?}")))?;
        self.sample_scores(
            &scores.logits,
            decision.acting_player,
            decision.legal_action_count,
        )
        .map_err(policy_error)
    }
}

fn policy_error(message: String) -> RlSessionError {
    RlSessionError {
        code: RlSessionErrorCode::StaleEnvironmentBinding,
        message: format!("frozen sideboard play policy: {message}"),
    }
}

fn validate_card_namespace(source: &Value, destination: &Value) -> Result<(usize, usize), String> {
    let source = source
        .get("cards")
        .and_then(Value::as_array)
        .ok_or("source cards missing")?;
    let destination = destination
        .get("cards")
        .and_then(Value::as_array)
        .ok_or("destination cards missing")?;
    require(
        !source.is_empty()
            && source.len() <= destination.len()
            && destination.len() < CARD_VOCAB_SIZE_V1,
        "card namespace is not an admitted append-only extension",
    )?;
    let mut names = std::collections::BTreeSet::new();
    for entry in destination {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or("card name missing")?;
        require(
            !name.is_empty() && names.insert(name),
            "duplicate or empty destination card name",
        )?;
    }
    for (id, (old, new)) in source.iter().zip(destination).enumerate() {
        let mut old = old
            .as_object()
            .ok_or("source card is not an object")?
            .clone();
        let mut new = new
            .as_object()
            .ok_or("destination card is not an object")?
            .clone();
        old.remove("decks");
        new.remove("decks");
        require(
            old == new,
            &format!("existing card-id {id} changed identity or registry semantics"),
        )?;
    }
    Ok((source.len(), destination.len()))
}

fn decode_parameters(raw: &[u8]) -> Result<Vec<NativeNamedParameterV1>, String> {
    require(raw.len() == PARAMETER_BYTES, "parameter byte count differs")?;
    let mut cursor = 0usize;
    let mut result = Vec::new();
    for (name, shape) in native_train_state_parameter_layout_v1() {
        let elements: usize = shape.iter().product();
        let end = cursor
            .checked_add(elements * 4)
            .ok_or("parameter layout overflow")?;
        let bytes = raw
            .get(cursor..end)
            .ok_or("parameter layout exceeds payload")?;
        let values = bytes
            .chunks_exact(4)
            .map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]]))
            .collect();
        result.push(NativeNamedParameterV1 {
            name,
            shape: shape.to_vec(),
            values,
        });
        cursor = end;
    }
    require(
        cursor == raw.len(),
        "parameter payload has an unconsumed suffix",
    )?;
    Ok(result)
}

fn read_bounded(path: &Path, cap: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    require(
        metadata.is_file() && metadata.len() <= cap as u64,
        "input must be a bounded regular file",
    )?;
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(bytes.len() <= cap, "input exceeded bound during read")?;
    Ok(bytes)
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn require(ok: bool, error: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(error.into())
    }
}
fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, String> {
    value
        .pointer(path)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string {path}"))
}
fn number(value: &Value, path: &str) -> Result<u64, String> {
    value
        .pointer(path)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing integer {path}"))
}

#[derive(Default)]
struct OwnedScoringV1 {
    globals: FlatGlobalsV2,
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    action_refs: Vec<FlatScorerActionRefV2>,
}

impl OwnedScoringV1 {
    fn buffers(&mut self) -> FlatScoringOwnedBuffersV2<'_> {
        FlatScoringOwnedBuffersV2 {
            objects: &mut self.objects,
            relations: &mut self.relations,
            object_subtypes: &mut self.object_subtypes,
            ability_uses: &mut self.ability_uses,
            goads: &mut self.goads,
            completed_dungeons: &mut self.completed_dungeons,
            effect_subtype_changes: &mut self.effect_subtype_changes,
            context_path_elements: &mut self.context_path_elements,
            actions: &mut self.actions,
            action_refs: &mut self.action_refs,
        }
    }
    fn view(&self) -> FlatScoringDecisionViewV2<'_> {
        FlatScoringDecisionViewV2::new(
            &self.globals,
            &self.objects,
            &self.relations,
            &self.object_subtypes,
            &self.ability_uses,
            &self.goads,
            &self.completed_dungeons,
            &self.effect_subtype_changes,
            &self.context_path_elements,
            &self.actions,
            &self.action_refs,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn namespace_accepts_membership_and_appends_but_rejects_reassigned_or_changed_cards() {
        let source = json!({"cards":[{"name":"A","mana_value":1,"decks":["Old"]},{"name":"B","mana_value":2}]});
        let destination = json!({"cards":[{"name":"A","mana_value":1,"decks":["Old","New"]},{"name":"B","mana_value":2},{"name":"C","mana_value":3}]});
        assert_eq!(
            validate_card_namespace(&source, &destination).unwrap(),
            (2, 3)
        );
        let mut changed = destination.clone();
        changed["cards"].as_array_mut().unwrap().swap(0, 1);
        assert!(validate_card_namespace(&source, &changed).is_err());
        changed = destination.clone();
        changed["cards"][0]["mana_value"] = json!(2);
        assert!(validate_card_namespace(&source, &changed).is_err());
        changed = destination;
        changed["cards"][2]["name"] = json!("A");
        assert!(validate_card_namespace(&source, &changed).is_err());
    }

    /// Run explicitly with MTG_SIDEB_PLAY_IMPORT_MANIFEST pointing to pinned
    /// engineering inputs. No terminal outcomes or CP7 information are read.
    #[test]
    #[ignore = "requires an explicit pinned real-checkpoint engineering manifest"]
    fn real_frozen_export_scores_and_replays_visible_decisions() {
        let path =
            std::env::var("MTG_SIDEB_PLAY_IMPORT_MANIFEST").expect("set import manifest path");
        let input: FrozenPlayPolicyImportV1 =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut policy = FrozenPlayPolicyV1::load_v1(&input).unwrap();
        let session =
            FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2_environment_v2(
                1,
                123,
                1024,
                2048,
                ["Rally".into(), "Rally".into()],
            )
            .unwrap();
        policy.reset_sampling_v1([777, 888]);
        let first_scores = policy.score_fast_session_v1(&session).unwrap();
        let first = policy.select_fast_session_v1(&session).unwrap();
        policy.reset_sampling_v1([777, 888]);
        let second_scores = policy.score_fast_session_v1(&session).unwrap();
        let second = policy.select_fast_session_v1(&session).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first_scores
                .logits
                .iter()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>(),
            second_scores
                .logits
                .iter()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(first_scores.value.to_bits(), second_scores.value.to_bits());
        assert_eq!(
            policy.identity_v1().model_parameter_sha256,
            input.expected_model_parameter_sha256
        );
        assert_eq!(
            policy.embedding_rows_v1().len(),
            CARD_VOCAB_SIZE_V1 * CARD_EMBEDDING_DIM_V1
        );
        assert!(policy.embedding_rows_v1()[..CARD_EMBEDDING_DIM_V1]
            .iter()
            .all(|x| x.to_bits() == 0));
        let mut tampered = input;
        tampered.expected_metadata_sha256 = "0".repeat(64);
        assert!(FrozenPlayPolicyV1::load_v1(&tampered).is_err());
    }
}
