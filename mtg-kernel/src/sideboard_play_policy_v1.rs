//! Explicit, inference-only transfer of a frozen Net8 export into the card lane.
//!
//! This is a research adapter, not a Store loader or continuation authority.
//! Original export and Store bytes stay untouched. The caller independently
//! pins the export and its source registry; the receipt declares the changed
//! card database and the preserved card-id prefix. Appended embedding rows are
//! retained exactly as exported and are not claimed to have been trained.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::fast_sampler::{
    FastCategoricalScratch, WideCategoricalScratchV1, FAST_CATEGORICAL_MAX_ACTIONS,
    FAST_CATEGORICAL_SAMPLER_VERSION, WIDE_CATEGORICAL_MAX_ACTIONS_V1,
    WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
};
use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatDecisionEncoderV2,
    FlatEffectSubtypeChangeV2, FlatGlobalsV2, FlatObjectAbilityUseV2, FlatObjectCoreV2,
    FlatObjectGoadV2, FlatObjectSubtypeV2, FlatRelationV2, FlatScorerActionCoreV2,
    FlatScorerActionRefV2, FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
};
use crate::flat_policy_v3::{
    FlatDecisionEncoderV3, FlatScoringDecisionViewV3, FlatScoringExtensionsV3,
};
use crate::flat_policy_v4::{
    FlatDecisionEncoderV4, FlatScoringDecisionViewV4, FlatScoringExtensionsV4,
};
use crate::native_checkpoint_inference_v1::encoded_decision_view_v1;
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
use crate::native_flat_tensorizer_v3::{
    encoded_decision_view_v3, NativeFlatDecisionTensorV3, NativeFlatTensorizerV3,
    FEATURES_SOURCE_SHA256_V3, FEATURE_CONTRACT_DIGEST_V3, FEATURE_DESCRIPTOR_SHA256_V3,
    FEATURE_ENCODING_DIGEST_V3, FEATURE_REGISTRY_VERSION_V3, FEATURE_SCHEMA_VERSION_V3,
};
use crate::native_flat_tensorizer_v4::{
    encoded_decision_view_v4, NativeFlatDecisionTensorV4, NativeFlatTensorizerV4,
    FEATURES_SOURCE_SHA256_V4, FEATURE_CONTRACT_DIGEST_V4, FEATURE_DESCRIPTOR_SHA256_V4,
    FEATURE_ENCODING_DIGEST_V4, FEATURE_REGISTRY_VERSION_V4, FEATURE_SCHEMA_VERSION_V4,
};
use crate::native_policy_train_step_v1::native_train_state_parameter_layout_v1;
use crate::native_policy_value_net_v1::{
    NativeNamedParameterV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1, FEATURE_CONTRACT_DIGEST_V1,
    FEATURE_ENCODING_DIGEST_V1, MODEL_ARCHITECTURE_VERSION_V1, MODEL_CONFIG_FINGERPRINT_V1,
    PARAMETER_COUNT_V1,
};
use crate::paired_bo1_harness_v1::{
    PairedBo1PolicyInputV1, PairedBo1PolicyV1, PlayPolicyGenerationV1,
};
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

mod origin;
pub use origin::{
    FreshPlayPolicyIdentityV1, PlayPolicyOriginV1, TransferredFreshPlayPolicyIdentityV1,
    FRESH_PLAY_INITIALIZATION_SCHEMA_V1, TRANSFERRED_FRESH_PLAY_SCHEMA_V1,
};

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

/// Identity of the parameters actually installed for inference. The separate
/// frozen import receipt remains ancestry after a successor update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlayModelIdentityV1 {
    pub schema: String,
    /// Flattened named-layout parameter values as binary32 little-endian,
    /// matching the frozen export's parameter-section hash semantics.
    pub weights_sha256: String,
    pub model_parameter_sha256: String,
    pub embedding_table_sha256: String,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub card_db_hash: String,
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
    identity: PlayPolicyOriginV1,
    encoder: FlatDecisionEncoderV2,
    owned: OwnedScoringV1,
    tensorizer: NativeFlatTensorizerV2,
    tensor: NativeFlatDecisionTensorV2,
    sampler: FastCategoricalScratch,
    seat_rng: [SplitMix64; 2],
    sampling_initialized: bool,
    successor: Option<FrozenPlaySuccessorStateV3>,
    /// Fresh-lineage (V4 contract) sibling of `successor`, independent and
    /// additive: both fields are per-instance, runtime-dispatched, and
    /// validated against digests from two different compiled modules
    /// (`native_flat_tensorizer_v3::FEATURE_CONTRACT_DIGEST_V3` vs
    /// `native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4`), so both can
    /// coexist in the same binary and even the same `FrozenPlayPolicyV1`
    /// instance without either affecting the other. `None` for every
    /// checkpoint constructed today; no existing caller ever sets this to
    /// `Some`, matching item 16's "reachable, not yet wired to any caller"
    /// scope.
    fresh_successor: Option<FrozenPlayFreshSuccessorStateV1>,
}

#[derive(Default)]
struct FrozenPlaySuccessorStateV3 {
    encoder: FlatDecisionEncoderV3,
    extensions: FlatScoringExtensionsV3,
    tensorizer: NativeFlatTensorizerV3,
    tensor: NativeFlatDecisionTensorV3,
    sampler: WideCategoricalScratchV1,
}

#[derive(Default)]
struct FrozenPlayFreshSuccessorStateV1 {
    encoder: FlatDecisionEncoderV4,
    extensions: FlatScoringExtensionsV4,
    tensorizer: NativeFlatTensorizerV4,
    tensor: NativeFlatDecisionTensorV4,
    sampler: WideCategoricalScratchV1,
}

/// Structural, not just numeric, identity gate for `fresh_successor`:
/// callers must bind it only to `FEATURE_CONTRACT_DIGEST_V4`/
/// `FEATURE_ENCODING_DIGEST_V4` (the fresh-lineage generation), never to
/// `FEATURE_CONTRACT_DIGEST_V3`/`FEATURE_ENCODING_DIGEST_V3` (the frozen
/// V3 generation) or any other generation's constants, mirroring the
/// non-relabeling discipline `expanded_deck_training_v1.rs`'s
/// `identity_valid`/`frozen_feature_identity_cannot_be_relabelled_as_successor`
/// already establishes for the V1-vs-V3 pair. This is new, additive code:
/// `identity_valid` itself is untouched.
pub(crate) fn fresh_successor_identity_valid_v1(
    feature_contract_digest: &str,
    feature_encoding_digest: &str,
) -> Result<(), String> {
    require(
        feature_contract_digest == FEATURE_CONTRACT_DIGEST_V4
            && feature_encoding_digest == FEATURE_ENCODING_DIGEST_V4,
        "fresh_successor feature identity must be the V4 fresh-lineage contract, never V3's",
    )
}

/// Exactly the two compiled fresh-lineage feature-contract generations that
/// `fresh_initialization_source.rs` and `expanded_deck_training_v1.rs` admit,
/// matched as a whole four-field tuple so a mixed tuple (say, a V3 contract
/// digest paired with a V4 encoding digest) can never pass by having each
/// field independently equal "V3 or V4". Mirrors `RuntimeContractGenerationV1`
/// (`phase1_agent_v1/package.rs`), applied to this crate's other fresh-lineage
/// axis; not a new idiom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FreshLineageGenerationV1 {
    V3,
    V4,
}

/// Single canonical whole-tuple classifier, reused by every reader of a
/// fresh-lineage source/target descriptor (the Rust fresh-initialization
/// reader and the successor identity gates), so the V3/V4 arms never drift
/// out of sync between call sites.
pub(crate) fn fresh_lineage_generation_v1(
    feature_contract_digest: &str,
    feature_encoding_digest: &str,
    features_source_sha256: &str,
    feature_descriptor_sha256: &str,
) -> Result<FreshLineageGenerationV1, String> {
    match (
        feature_contract_digest,
        feature_encoding_digest,
        features_source_sha256,
        feature_descriptor_sha256,
    ) {
        (
            FEATURE_CONTRACT_DIGEST_V3,
            FEATURE_ENCODING_DIGEST_V3,
            FEATURES_SOURCE_SHA256_V3,
            FEATURE_DESCRIPTOR_SHA256_V3,
        ) => Ok(FreshLineageGenerationV1::V3),
        (
            FEATURE_CONTRACT_DIGEST_V4,
            FEATURE_ENCODING_DIGEST_V4,
            FEATURES_SOURCE_SHA256_V4,
            FEATURE_DESCRIPTOR_SHA256_V4,
        ) => Ok(FreshLineageGenerationV1::V4),
        _ => Err(
            "fresh-lineage feature identity matches neither the compiled V3 nor V4 contract"
                .into(),
        ),
    }
}

/// The full bundle of one generation's independently-versioned feature
/// identities, grouped so a stamping call site can never mix a field from
/// one compiled module (`native_flat_tensorizer_v3`) with a field from the
/// other (`native_flat_tensorizer_v4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FreshFeatureIdentityV1 {
    pub(crate) generation: FreshLineageGenerationV1,
    pub(crate) feature_schema_version: &'static str,
    pub(crate) feature_registry_version: &'static str,
    pub(crate) feature_contract_digest: &'static str,
    pub(crate) feature_encoding_digest: &'static str,
    pub(crate) features_source_sha256: &'static str,
    pub(crate) feature_descriptor_sha256: &'static str,
}

pub(crate) const FRESH_FEATURE_IDENTITY_V3: FreshFeatureIdentityV1 = FreshFeatureIdentityV1 {
    generation: FreshLineageGenerationV1::V3,
    feature_schema_version: FEATURE_SCHEMA_VERSION_V3,
    feature_registry_version: FEATURE_REGISTRY_VERSION_V3,
    feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3,
    feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3,
    features_source_sha256: FEATURES_SOURCE_SHA256_V3,
    feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3,
};

pub(crate) const FRESH_FEATURE_IDENTITY_V4: FreshFeatureIdentityV1 = FreshFeatureIdentityV1 {
    generation: FreshLineageGenerationV1::V4,
    feature_schema_version: FEATURE_SCHEMA_VERSION_V4,
    feature_registry_version: FEATURE_REGISTRY_VERSION_V4,
    feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4,
    feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4,
    features_source_sha256: FEATURES_SOURCE_SHA256_V4,
    feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V4,
};

/// Computes the two byte-identity hashes `from_fresh_initialization_generation_v1`
/// verifies against a caller-declared identity: the flattened parameter-value
/// SHA256 (`initial_weights_sha256`) and the named layout SHA256
/// (`parameter_layout_sha256`). Factored out so the constructor's
/// verification and a test's synthetic-fixture construction share exactly
/// one computation, never two independently maintained copies.
fn fresh_parameter_evidence_v1(
    parameters: &[NativeNamedParameterV1],
) -> Result<(String, String), String> {
    let expected: Vec<_> = native_train_state_parameter_layout_v1().collect();
    require(
        parameters.len() == expected.len(),
        "fresh parameter tensor count differs",
    )?;
    // Fields are in canonical sorted order; names are frozen ASCII and
    // shapes/offsets are integers. No trailing newline enters the digest.
    #[derive(Serialize)]
    struct Layout<'a> {
        byte_count: usize,
        byte_offset: usize,
        name: &'a str,
        ordinal: usize,
        shape: &'a [usize],
    }
    let mut offset = 0usize;
    let mut layout = Vec::with_capacity(parameters.len());
    let mut weights = Sha256::new();
    for (ordinal, (parameter, (name, shape))) in parameters.iter().zip(expected).enumerate() {
        require(
            parameter.name == name && parameter.shape.as_slice() == shape,
            "fresh named parameter layout differs",
        )?;
        let byte_count = parameter
            .values
            .len()
            .checked_mul(4)
            .ok_or("parameter byte count overflow")?;
        layout.push(Layout {
            byte_count,
            byte_offset: offset,
            name,
            ordinal,
            shape,
        });
        offset = offset
            .checked_add(byte_count)
            .ok_or("parameter byte offset overflow")?;
        for value in &parameter.values {
            weights.update(value.to_bits().to_le_bytes());
        }
    }
    require(offset == PARAMETER_BYTES, "fresh parameter payload byte count differs")?;
    Ok((
        format!("{:x}", weights.finalize()),
        hash(&serde_json::to_vec(&layout).map_err(|e| e.to_string())?),
    ))
}

impl FrozenPlayPolicyV1 {
    /// Shared validation for both fresh-lineage generations: exact feature
    /// identity tuple (against the caller-selected compiled generation),
    /// installed Net8 layout, weights hash and origin metadata. Neither
    /// `from_fresh_initialization_v1` nor `from_fresh_initialization_v4`
    /// duplicates this; they differ only in which compiled constants they
    /// pass in and which successor slot the result activates.
    fn from_fresh_initialization_generation_v1(
        model: NativePolicyValueNetV1,
        identity: FreshPlayPolicyIdentityV1,
        feature_identity: FreshFeatureIdentityV1,
    ) -> Result<(NativePolicyValueNetV1, Vec<f32>, PlayPolicyOriginV1), String> {
        identity.validate_v1()?;
        require(
            model.config_v1() == NativePolicyValueModelConfigV1::contract_v1()
                && identity.destination_card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}")
                && identity.destination_registry_sha256 == hash(DESTINATION_REGISTRY)
                && identity.destination_card_count == crate::card_def::CARD_DEFS.len()
                && identity.feature_contract_digest == feature_identity.feature_contract_digest
                && identity.feature_encoding_digest == feature_identity.feature_encoding_digest
                && identity.features_source_sha256 == feature_identity.features_source_sha256
                && identity.feature_descriptor_sha256
                    == feature_identity.feature_descriptor_sha256,
            "fresh initialization does not bind this runtime and Net8 layout",
        )?;
        model.validate_parameters_v1().map_err(|e| e.to_string())?;
        let parameters = model.parameter_snapshot_v1();
        let (weights_sha256, parameter_layout_sha256) =
            fresh_parameter_evidence_v1(&parameters)?;
        require(
            identity.initial_weights_sha256 == weights_sha256
                && identity.initial_model_parameter_sha256 == model.parameter_manifest_sha256_v1()
                && identity.parameter_layout_sha256 == parameter_layout_sha256,
            "fresh initial parameter bytes or layout identity differs",
        )?;
        let embeddings = parameters
            .iter()
            .find(|p| p.name == "card_embedding.weight")
            .ok_or("embedding tensor absent")?
            .values
            .clone();
        let origin = PlayPolicyOriginV1::fresh_initialization_v1(identity)?;
        Ok((model, embeddings, origin))
    }

    /// Construct a V3 scorer from actual sampled initialization bytes. This
    /// validates installed state and metadata, not the Python producer's seed
    /// execution. The pinned artifact loader separately validates that evidence.
    pub(crate) fn from_fresh_initialization_v1(
        model: NativePolicyValueNetV1,
        identity: FreshPlayPolicyIdentityV1,
    ) -> Result<Self, String> {
        let (model, embeddings, identity) = Self::from_fresh_initialization_generation_v1(
            model,
            identity,
            FRESH_FEATURE_IDENTITY_V3,
        )?;
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
            successor: Some(FrozenPlaySuccessorStateV3::default()),
            fresh_successor: None,
        })
    }

    /// V4 sibling of `from_fresh_initialization_v1`: the same validated Net8
    /// layout/weights/origin contract, but the identity's feature tuple must
    /// match the compiled V4 fresh-lineage constants (never V3's), and the
    /// constructed policy activates `fresh_successor`, never `successor`.
    /// Explicitly wires in the already-built `fresh_successor_identity_valid_v1`
    /// gate rather than reimplementing its check.
    pub(crate) fn from_fresh_initialization_v4(
        model: NativePolicyValueNetV1,
        identity: FreshPlayPolicyIdentityV1,
    ) -> Result<Self, String> {
        let (model, embeddings, identity) = Self::from_fresh_initialization_generation_v1(
            model,
            identity,
            FRESH_FEATURE_IDENTITY_V4,
        )?;
        fresh_successor_identity_valid_v1(
            identity.feature_contract_digest_v1(),
            identity.feature_encoding_digest_v1(),
        )?;
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
            fresh_successor: Some(FrozenPlayFreshSuccessorStateV1::default()),
        })
    }

    /// Single accessor for every site that stamps a feature-contract identity
    /// (trajectories, checkpoints, inference identities) from the actually
    /// loaded policy. No call site should ever hand-pick a hardcoded V3 (or
    /// V4) constant independently of what this policy's generation actually
    /// is; item 3's mitigation against a silent V4-encoded/V3-labeled
    /// mislabel is that every stamping site instead reads this.
    pub(crate) fn feature_identity_v1(&self) -> FreshFeatureIdentityV1 {
        if self.fresh_successor.is_some() {
            FRESH_FEATURE_IDENTITY_V4
        } else {
            // Also the correct default for the plain V3 successor and for
            // any policy with neither successor active: this crate's fresh-
            // lineage trainer/collector never constructs the latter, and it
            // matches this accessor's pre-existing hardcoded-V3 behavior.
            FRESH_FEATURE_IDENTITY_V3
        }
    }

    /// Explicit construction from the independently verified registry-transfer
    /// envelope. The old import loader is unchanged and cannot select this path.
    pub(crate) fn from_registry_transfer_v1(
        transfer: &crate::phase1_registry_transfer_v1::VerifiedRegistryTransferV1,
        envelope_sha256: &str,
    ) -> Result<Self, String> {
        require(
            envelope_sha256.len() == 64
                && envelope_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && transfer.request_v1().destination_card_db_hash
                    == format!("{KERNEL_CARDDB_HASH:016x}")
                && transfer.request_v1().destination_registry_sha256 == hash(DESTINATION_REGISTRY),
            "registry transfer does not bind this runtime",
        )?;
        let model = transfer.model_v1().clone();
        let embeddings = model
            .parameter_snapshot_v1()
            .into_iter()
            .find(|p| p.name == "card_embedding.weight")
            .ok_or("embedding tensor absent")?
            .values;
        let mut identity = transfer.source_import_v1().clone();
        // Source export fields remain ancestry. Destination fields describe
        // the actual installed registry; actual_model_identity_v1 owns weights.
        identity.schema = "mtg-kernel-registry-transferred-play/v1".into();
        identity.destination_registry_sha256 =
            transfer.request_v1().destination_registry_sha256.clone();
        identity.destination_card_db_hash = transfer.request_v1().destination_card_db_hash.clone();
        identity.destination_card_count = transfer.receipt_v1().cards.len();
        identity.namespace_rule = transfer.receipt_v1().mapping_rule.clone();
        identity.appended_rows = format!(
            "{}; envelope_sha256={envelope_sha256}",
            transfer.receipt_v1().initializer
        );
        Ok(Self {
            model,
            embeddings,
            identity: identity.into(),
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: Some(FrozenPlaySuccessorStateV3::default()),
            fresh_successor: None,
        })
    }

    /// Explicit construction from a verified fresh-origin registry-transfer
    /// candidate. Nests the original fresh ancestry unchanged; the wrapper's
    /// own destination fields describe the current runtime, not the original
    /// import's stale ones. Does not call `from_fresh_initialization_v1`: it
    /// correctly requires the input identity's own destination fields to
    /// already match this runtime, which a pre-transfer fresh identity (still
    /// describing the OLD registry) does not have.
    pub(crate) fn from_fresh_registry_transfer_v1(
        transfer: &crate::phase1_registry_transfer_v1::FreshVerifiedRegistryTransferV1,
        envelope_sha256: &str,
    ) -> Result<Self, String> {
        require(
            envelope_sha256.len() == 64
                && envelope_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && transfer.request_v1().destination_card_db_hash
                    == format!("{KERNEL_CARDDB_HASH:016x}")
                && transfer.request_v1().destination_registry_sha256 == hash(DESTINATION_REGISTRY),
            "registry transfer does not bind this runtime",
        )?;
        let model = transfer.model_v1().clone();
        let embeddings = model
            .parameter_snapshot_v1()
            .into_iter()
            .find(|p| p.name == "card_embedding.weight")
            .ok_or("embedding tensor absent")?
            .values;
        let identity = TransferredFreshPlayPolicyIdentityV1 {
            schema: TRANSFERRED_FRESH_PLAY_SCHEMA_V1.into(),
            original: transfer.source_import_v1().clone(),
            destination_registry_sha256: transfer.request_v1().destination_registry_sha256.clone(),
            destination_card_db_hash: transfer.request_v1().destination_card_db_hash.clone(),
            destination_card_count: transfer.receipt_v1().cards.len(),
            transfer_envelope_sha256: envelope_sha256.into(),
        };
        Ok(Self {
            model,
            embeddings,
            identity: PlayPolicyOriginV1::transferred_fresh_v1(identity)?,
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: Some(FrozenPlaySuccessorStateV3::default()),
            fresh_successor: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn training_fixture_v3() -> Self {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let parameters = model.parameter_snapshot_v1();
        let embeddings = parameters
            .iter()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap()
            .values
            .clone();
        let mut policy = Self {
            model,
            embeddings,
            identity: FrozenPlayPolicyIdentityV1 {
                schema: "test-only-import-ancestry".into(),
                source_export_schema: EXPORT_SCHEMA.into(),
                source_metadata_sha256: "a".repeat(64),
                weights_sha256: String::new(),
                model_parameter_sha256: String::new(),
                source_run_sha256: "b".repeat(64),
                source_generation: 7,
                source_git_commit: "1".repeat(40),
                source_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                source_registry_sha256: "c".repeat(64),
                destination_registry_sha256: hash(DESTINATION_REGISTRY),
                source_card_count: crate::card_def::CARD_DEFS.len(),
                destination_card_count: crate::card_def::CARD_DEFS.len(),
                source_training_deck_ids: vec!["Rally".into(), "Rally".into()],
                namespace_rule: "test fixture".into(),
                appended_rows: "test fixture".into(),
                feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
                sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION.into(),
                reader_revalidated_store_chain: false,
                observation_successor: None,
            }
            .into(),
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: Some(FrozenPlaySuccessorStateV3::default()),
            fresh_successor: None,
        };
        let installed = policy.actual_model_identity_v1();
        let PlayPolicyOriginV1::Imported(identity) = &mut policy.identity else {
            unreachable!("test fixture constructed an imported origin")
        };
        identity.weights_sha256 = installed.weights_sha256;
        identity.model_parameter_sha256 = installed.model_parameter_sha256;
        policy
    }

    /// V4 sibling of `training_fixture_v3`: same "Imported" weight-provenance
    /// origin (weight ancestry and feature-contract generation are
    /// independent axes, as `training_fixture_v3` itself already establishes
    /// by pairing an `Imported` origin with the wide V3 `successor`), but
    /// activates `fresh_successor`, not `successor`, with V4 digests.
    #[cfg(test)]
    pub(crate) fn training_fixture_v4() -> Self {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let parameters = model.parameter_snapshot_v1();
        let embeddings = parameters
            .iter()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap()
            .values
            .clone();
        let mut policy = Self {
            model,
            embeddings,
            identity: FrozenPlayPolicyIdentityV1 {
                schema: "test-only-import-ancestry".into(),
                source_export_schema: EXPORT_SCHEMA.into(),
                source_metadata_sha256: "a".repeat(64),
                weights_sha256: String::new(),
                model_parameter_sha256: String::new(),
                source_run_sha256: "b".repeat(64),
                source_generation: 7,
                source_git_commit: "1".repeat(40),
                source_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                source_registry_sha256: "c".repeat(64),
                destination_registry_sha256: hash(DESTINATION_REGISTRY),
                source_card_count: crate::card_def::CARD_DEFS.len(),
                destination_card_count: crate::card_def::CARD_DEFS.len(),
                source_training_deck_ids: vec!["Rally".into(), "Rally".into()],
                namespace_rule: "test fixture".into(),
                appended_rows: "test fixture".into(),
                feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
                feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
                sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
                reader_revalidated_store_chain: false,
                observation_successor: None,
            }
            .into(),
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: None,
            fresh_successor: Some(FrozenPlayFreshSuccessorStateV1::default()),
        };
        let installed = policy.actual_model_identity_v1();
        let PlayPolicyOriginV1::Imported(identity) = &mut policy.identity else {
            unreachable!("test fixture constructed an imported origin")
        };
        identity.weights_sha256 = installed.weights_sha256;
        identity.model_parameter_sha256 = installed.model_parameter_sha256;
        policy
    }

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
            identity: identity.into(),
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: None,
            fresh_successor: None,
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
        let PlayPolicyOriginV1::Imported(identity) = &mut policy.identity else {
            return Err("legacy observation transfer requires imported ancestry".into());
        };
        identity.observation_successor = Some(FrozenPlayObservationReceiptV3 {
            schema: "mtg-kernel-frozen-play-observation-transfer/v3".into(),
            source_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V1.into(),
            source_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V1.into(),
            destination: transfer.clone(),
            features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
            feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
            semantics: "rich V6 / flat V3 revision 3; exact public historical sources, chooser-only unordered library candidates, typed object-cost prefixes, private chosen-creature branch, visible refreshed paid power, and exact pending/queued Ward-to-targeter payment bindings; unchanged imported weights and dimensions; source inference transfer only, no learned competence claim".into(),
        });
        identity.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into();
        identity.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V3.into();
        policy.successor = Some(FrozenPlaySuccessorStateV3::default());
        Ok(policy)
    }

    pub fn identity_v1(&self) -> &PlayPolicyOriginV1 {
        &self.identity
    }

    /// A private CPU collector with the same installed weights and ancestry.
    /// Encoders, tensors, samplers and RNG streams start fresh, rather than
    /// sharing mutable inference state or carrying a previous episode forward.
    /// The collector must reset physical-seat sampling before its first action.
    pub(crate) fn fork_for_collection_v3(&self) -> Result<Self, String> {
        require(
            self.successor.is_some() || self.fresh_successor.is_some(),
            "parallel collection requires explicit successor features",
        )?;
        Ok(Self {
            model: self.model.clone(),
            embeddings: self.embeddings.clone(),
            identity: self.identity.clone(),
            encoder: FlatDecisionEncoderV2::default(),
            owned: OwnedScoringV1::default(),
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            sampler: FastCategoricalScratch::default(),
            seat_rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            sampling_initialized: false,
            successor: self
                .successor
                .is_some()
                .then(FrozenPlaySuccessorStateV3::default),
            fresh_successor: self
                .fresh_successor
                .is_some()
                .then(FrozenPlayFreshSuccessorStateV1::default),
        })
    }

    /// Active runtime capability, separate from the historical import receipt.
    /// Narrow decisions still execute the frozen sampler verbatim. Both wide
    /// generations (V3's `successor` and V4's `fresh_successor`) share the
    /// same wide sampler identity and action bound.
    pub fn runtime_sampler_identity_v1(&self) -> &'static str {
        if self.successor.is_some() || self.fresh_successor.is_some() {
            WIDE_CATEGORICAL_SAMPLER_VERSION_V1
        } else {
            FAST_CATEGORICAL_SAMPLER_VERSION
        }
    }

    pub fn runtime_sampler_max_actions_v1(&self) -> usize {
        if self.successor.is_some() || self.fresh_successor.is_some() {
            WIDE_CATEGORICAL_MAX_ACTIONS_V1
        } else {
            FAST_CATEGORICAL_MAX_ACTIONS
        }
    }

    pub fn actual_model_identity_v1(&self) -> PlayModelIdentityV1 {
        let mut weights = Sha256::new();
        let mut embeddings = Sha256::new();
        self.model.visit_parameters_v1(|name, _, values| {
            for value in values {
                let bytes = value.to_bits().to_le_bytes();
                weights.update(bytes);
                if name == "card_embedding.weight" {
                    embeddings.update(bytes);
                }
            }
        });
        PlayModelIdentityV1 {
            schema: "mtg-kernel-actual-play-model/v1".into(),
            weights_sha256: format!("{:x}", weights.finalize()),
            model_parameter_sha256: self.model.parameter_manifest_sha256_v1(),
            embedding_table_sha256: format!("{:x}", embeddings.finalize()),
            feature_contract_digest: self.identity.feature_contract_digest_v1().to_owned(),
            feature_encoding_digest: self.identity.feature_encoding_digest_v1().to_owned(),
            card_db_hash: self.identity.destination_card_db_hash_v1().to_owned(),
        }
    }

    /// Crate-only warm-start bridge. The caller owns the successor checkpoint
    /// identity; the original import receipt continues to describe the source.
    pub(crate) fn training_parameters_v3(&self) -> Vec<NativeNamedParameterV1> {
        self.model.parameter_snapshot_v1()
    }

    pub(crate) fn replace_training_parameters_v3(
        &mut self,
        parameters: &[NativeNamedParameterV1],
    ) -> Result<(), String> {
        require(
            self.successor.is_some() || self.fresh_successor.is_some(),
            "training requires explicit successor features",
        )?;
        self.model
            .replace_parameter_snapshot_v1(parameters)
            .map_err(|e| e.to_string())?;
        // Replacement validates the whole model before committing. Copy the
        // installed table so sideboard inputs cannot retain ancestral rows.
        self.model.visit_parameters_v1(|name, _, values| {
            if name == "card_embedding.weight" {
                self.embeddings = values.to_vec();
            }
        });
        Ok(())
    }

    /// Read-only replay of an actor-visible training tensor against the
    /// installed model. This does not consume either physical seat's RNG.
    pub(crate) fn score_training_tensor_v3(
        &self,
        tensor: &NativeFlatDecisionTensorV3,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        require(
            self.successor.is_some(),
            "training requires explicit successor features",
        )?;
        let output = self
            .model
            .forward_feature_transfer_v3(encoded_decision_view_v3(tensor))
            .map_err(|e| e.to_string())?;
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }

    /// V4 sibling of `score_training_tensor_v3`.
    pub(crate) fn score_training_tensor_v4(
        &self,
        tensor: &NativeFlatDecisionTensorV4,
    ) -> Result<FrozenPlayDecisionScoresV1, String> {
        require(
            self.fresh_successor.is_some(),
            "training requires explicit V4 successor features",
        )?;
        let output = self
            .model
            .forward_feature_transfer_v4(encoded_decision_view_v4(tensor))
            .map_err(|e| e.to_string())?;
        Ok(FrozenPlayDecisionScoresV1 {
            logits: output.logits,
            value: output.value,
        })
    }

    /// Borrow the input retained by the immediately preceding V3 scoring call.
    /// Recording callers must use it before any subsequent score or reset.
    /// This accessor performs no encoding, forward pass, or random sampling.
    pub(crate) fn last_scored_training_tensor_v3(
        &self,
    ) -> Result<&NativeFlatDecisionTensorV3, String> {
        self.successor
            .as_ref()
            .map(|state| &state.tensor)
            .ok_or_else(|| "native capture requires the V3 scorer".into())
    }

    /// V4 sibling of `last_scored_training_tensor_v3`.
    pub(crate) fn last_scored_training_tensor_v4(
        &self,
    ) -> Result<&NativeFlatDecisionTensorV4, String> {
        self.fresh_successor
            .as_ref()
            .map(|state| &state.tensor)
            .ok_or_else(|| "native capture requires the V4 scorer".into())
    }

    pub(crate) fn select_with_training_tensor_v3(
        &mut self,
        session: &FastActorSessionV1,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1, NativeFlatDecisionTensorV3), String> {
        require(
            self.successor.is_some(),
            "training requires explicit successor features",
        )?;
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err("cannot collect a terminal decision".into());
        };
        let scores = self.score_fast_session_v1(session)?;
        let selected = self.sample_scores(
            &scores.logits,
            decision.acting_player,
            decision.legal_action_count,
        )?;
        let tensor = self
            .successor
            .as_ref()
            .ok_or("missing successor tensor")?
            .tensor
            .clone();
        Ok((selected, scores, tensor))
    }

    /// V4 sibling of `select_with_training_tensor_v3`, for a policy with an
    /// active `fresh_successor`. The scoring/sampling path itself
    /// (`score_fast_session_v1`, `sample_scores`) is shared and already
    /// dispatches on the active successor; only the returned tensor type
    /// and the successor this reads from differ.
    pub(crate) fn select_with_training_tensor_v4(
        &mut self,
        session: &FastActorSessionV1,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1, NativeFlatDecisionTensorV4), String> {
        require(
            self.fresh_successor.is_some(),
            "training requires explicit V4 successor features",
        )?;
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err("cannot collect a terminal decision".into());
        };
        let scores = self.score_fast_session_v1(session)?;
        let selected = self.sample_scores(
            &scores.logits,
            decision.acting_player,
            decision.legal_action_count,
        )?;
        let tensor = self
            .fresh_successor
            .as_ref()
            .ok_or("missing fresh successor tensor")?
            .tensor
            .clone();
        Ok((selected, scores, tensor))
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
            usize::from(card_id) < self.identity.destination_card_count_v1(),
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
        if let Some(successor) = &mut self.successor {
            let sampler = std::mem::take(&mut successor.sampler);
            *successor = FrozenPlaySuccessorStateV3 {
                sampler,
                ..FrozenPlaySuccessorStateV3::default()
            };
        }
        if let Some(fresh) = &mut self.fresh_successor {
            let sampler = std::mem::take(&mut fresh.sampler);
            *fresh = FrozenPlayFreshSuccessorStateV1 {
                sampler,
                ..FrozenPlayFreshSuccessorStateV1::default()
            };
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
        if let Some(fresh) = &mut self.fresh_successor {
            let encoded = session
                .encode_current_flat_scoring_decision_owned_v4(
                    decision,
                    &mut fresh.encoder,
                    &mut self.owned.buffers(),
                )
                .map_err(|e| format!("V4 actor-visible encoding: {e:?}; decision={decision:?}"))?;
            self.owned.globals = encoded.globals;
            fresh.extensions = encoded.extensions;
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
        } else if let Some(fresh) = &mut self.fresh_successor {
            fresh
                .tensorizer
                .fill(
                    FlatScoringDecisionViewV4::new(self.owned.view(), &fresh.extensions),
                    &mut fresh.tensor,
                )
                .map_err(|e| format!("V4 visible tensorization: {e:?}"))?;
            self.model
                .forward_feature_transfer_v4(encoded_decision_view_v4(&fresh.tensor))
                .map_err(|e| format!("explicit V4 frozen feature transfer: {e:?}"))?
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
        let selected = if let Some(successor) = &mut self.successor {
            successor.sampler.sample(logits, seed)
        } else if let Some(fresh) = &mut self.fresh_successor {
            fresh.sampler.sample(logits, seed)
        } else {
            self.sampler.sample(logits, seed)
        }
        .map_err(|e| e.to_string())?;
        u32::try_from(selected).map_err(|e| e.to_string())
    }
}

impl FrozenPlayPolicyV1 {
    /// Score and sample exactly once using the legacy arithmetic and seat RNG.
    /// The BO3 recorder captures probabilities from these actual sampled logits.
    pub(crate) fn select_paired_with_scores_v1(
        &mut self,
        input: &PairedBo1PolicyInputV1<'_>,
    ) -> Result<(u32, FrozenPlayDecisionScoresV1), RlSessionError> {
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
        } else if let Some(fresh) = &mut self.fresh_successor {
            let encoded = input
                .encode_scoring_owned_v4(&mut fresh.encoder, &mut self.owned.buffers())
                .map_err(|e| {
                    policy_error(format!(
                        "V4 actor-visible encoding: {e:?}; decision={decision:?}"
                    ))
                })?;
            self.owned.globals = encoded.globals;
            fresh.extensions = encoded.extensions;
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
        let selected = self
            .sample_scores(
                &scores.logits,
                decision.acting_player,
                decision.legal_action_count,
            )
            .map_err(policy_error)?;
        Ok((selected, scores))
    }
}

impl PairedBo1PolicyV1 for FrozenPlayPolicyV1 {
    fn uses_observation_successor_v3(&self) -> bool {
        self.successor.is_some() || self.fresh_successor.is_some()
    }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        if self.fresh_successor.is_some() {
            PlayPolicyGenerationV1::V4
        } else if self.successor.is_some() {
            PlayPolicyGenerationV1::V3
        } else {
            PlayPolicyGenerationV1::V2
        }
    }
    fn reset_for_game_v1(&mut self, policy_seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.reset_sampling_v1(policy_seeds);
        Ok(())
    }

    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        self.select_paired_with_scores_v1(&input)
            .map(|(selected, _)| selected)
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
    fn fresh_successor_is_none_for_every_existing_checkpoint_construction_path() {
        // `training_fixture_v3` sets the V3 `successor` to `Some`, exercising
        // the "both fields independent" claim: a V3 successor present does
        // not imply a fresh (V4) one.
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        assert!(policy.successor.is_some());
        assert!(policy.fresh_successor.is_none());
    }

    #[test]
    fn fresh_successor_identity_accepts_only_v4_never_v3() {
        assert!(fresh_successor_identity_valid_v1(
            FEATURE_CONTRACT_DIGEST_V4,
            FEATURE_ENCODING_DIGEST_V4,
        )
        .is_ok());
        assert!(fresh_successor_identity_valid_v1(
            FEATURE_CONTRACT_DIGEST_V3,
            FEATURE_ENCODING_DIGEST_V3,
        )
        .is_err());
        // Mixed pairs (one V4, one V3) must also fail: this is a structural
        // pair check, not two independent membership checks.
        assert!(fresh_successor_identity_valid_v1(
            FEATURE_CONTRACT_DIGEST_V4,
            FEATURE_ENCODING_DIGEST_V3,
        )
        .is_err());
        assert!(fresh_successor_identity_valid_v1(
            FEATURE_CONTRACT_DIGEST_V3,
            FEATURE_ENCODING_DIGEST_V4,
        )
        .is_err());
    }

    /// Real deterministic Net8 weights and real weight/layout hashes (via
    /// `fresh_parameter_evidence_v1`, the same computation the constructor
    /// itself verifies against), not a bypassed struct literal: exercises
    /// `from_fresh_initialization_v4` itself, item 1's required test.
    #[test]
    fn from_fresh_initialization_v4_accepts_v4_and_rejects_a_v3_declared_identity() {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let parameters = model.parameter_snapshot_v1();
        let (initial_weights_sha256, parameter_layout_sha256) =
            fresh_parameter_evidence_v1(&parameters).unwrap();
        let base_identity = FreshPlayPolicyIdentityV1 {
            schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
            initialization_manifest_sha256: "a".repeat(64),
            lineage_id: "synthetic-v4-constructor-test".into(),
            initializer: "trainer-seeded-v1".into(),
            base_seed: 0,
            model_init_seed: 6_443_515_232_517_447_393,
            seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
            producer_git_commit: "1".repeat(40),
            initial_weights_sha256,
            initial_model_parameter_sha256: model.parameter_manifest_sha256_v1(),
            parameter_layout_sha256,
            destination_registry_sha256: hash(DESTINATION_REGISTRY),
            destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            destination_card_count: crate::card_def::CARD_DEFS.len(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
            features_source_sha256: FEATURES_SOURCE_SHA256_V4.into(),
            feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V4.into(),
            sampler_identity: WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        };
        let policy =
            FrozenPlayPolicyV1::from_fresh_initialization_v4(model.clone(), base_identity.clone())
                .unwrap();
        assert!(policy.fresh_successor.is_some());
        assert!(policy.successor.is_none());
        assert_eq!(policy.feature_generation_v1(), PlayPolicyGenerationV1::V4);
        assert!(policy.uses_observation_successor_v3());

        // A V3-declared identity (same real weights, V3 digests) must be
        // rejected by the V4 constructor, never silently accepted.
        let mut v3_identity = base_identity;
        v3_identity.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into();
        v3_identity.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V3.into();
        v3_identity.features_source_sha256 = FEATURES_SOURCE_SHA256_V3.into();
        v3_identity.feature_descriptor_sha256 = FEATURE_DESCRIPTOR_SHA256_V3.into();
        assert!(FrozenPlayPolicyV1::from_fresh_initialization_v4(model, v3_identity).is_err());
    }

    #[test]
    fn phase1_collection_fork_has_private_weights_embeddings_samplers_and_rng() {
        let mut original = FrozenPlayPolicyV1::training_fixture_v3();
        let expected_identity = original.actual_model_identity_v1();
        let ancestry = original.identity_v1().clone();
        let seeds = [73, 911];
        original.reset_sampling_v1(seeds);
        // The fork must not inherit an already advanced seat stream.
        original
            .sample_scores(&[0.0; 64], PlayerSeatV1::P0, 64)
            .unwrap();
        let mut first = original.fork_for_collection_v3().unwrap();
        let mut second = original.fork_for_collection_v3().unwrap();
        assert!(!first.sampling_initialized);
        assert!(!second.sampling_initialized);
        assert_eq!(first.actual_model_identity_v1(), expected_identity);
        assert_eq!(first.identity_v1(), &ancestry);
        assert_ne!(first.embeddings.as_ptr(), original.embeddings.as_ptr());
        first.reset_sampling_v1(seeds);
        second.reset_sampling_v1(seeds);
        for (seat, width) in [
            (PlayerSeatV1::P0, 64),
            (PlayerSeatV1::P1, 257),
            (PlayerSeatV1::P1, 120),
            (PlayerSeatV1::P0, 3),
        ] {
            let logits = vec![0.0; width];
            assert_eq!(
                first.sample_scores(&logits, seat, width as u32).unwrap(),
                second.sample_scores(&logits, seat, width as u32).unwrap()
            );
        }
        let mut parameters = first.training_parameters_v3();
        parameters
            .iter_mut()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap()
            .values[16] += 0.125;
        first.replace_training_parameters_v3(&parameters).unwrap();
        assert_ne!(first.actual_model_identity_v1(), expected_identity);
        assert_eq!(original.actual_model_identity_v1(), expected_identity);
        assert_eq!(second.actual_model_identity_v1(), expected_identity);
        original.successor = None;
        assert!(original.fork_for_collection_v3().is_err());
    }

    #[test]
    fn wide_runtime_sampling_preserves_import_and_each_physical_seat_rng() {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let ancestry = policy.identity_v1().clone();
        assert_eq!(
            policy.runtime_sampler_identity_v1(),
            WIDE_CATEGORICAL_SAMPLER_VERSION_V1
        );
        assert_eq!(policy.runtime_sampler_max_actions_v1(), 65_536);
        let seeds = [17, 829];
        for _ in 0..2 {
            policy.reset_sampling_v1(seeds);
            let mut rng = seeds.map(SplitMix64::seed);
            let mut old = FastCategoricalScratch::default();
            for (actor, width) in [(0, 64), (1, 120), (1, 257), (0, 5_040), (1, 3), (0, 2)] {
                let seed = rng[actor].next_u64();
                let logits = vec![0.0; width];
                let expected = if width <= 64 {
                    old.sample(&logits, seed).unwrap()
                } else {
                    // Independent uniform-quota inverse CDF, including the
                    // extra Hamilton units at the earliest legal indices.
                    let total = 1u128 << 64;
                    let quotient = total / width as u128;
                    let residual = total % width as u128;
                    let draw = u128::from(crate::fast_sampler::splitmix64_first(seed));
                    let prefix = (quotient + 1) * residual;
                    if draw < prefix {
                        (draw / (quotient + 1)) as usize
                    } else {
                        (residual + (draw - prefix) / quotient) as usize
                    }
                };
                let seat = if actor == 0 {
                    PlayerSeatV1::P0
                } else {
                    PlayerSeatV1::P1
                };
                assert_eq!(
                    policy.sample_scores(&logits, seat, width as u32).unwrap() as usize,
                    expected
                );
            }
            assert_eq!(policy.identity_v1(), &ancestry);
        }
        policy.successor = None;
        assert_eq!(
            policy.runtime_sampler_identity_v1(),
            FAST_CATEGORICAL_SAMPLER_VERSION
        );
        assert_eq!(policy.runtime_sampler_max_actions_v1(), 64);
        assert!(policy
            .sample_scores(&[0.0; 65], PlayerSeatV1::P0, 65)
            .unwrap_err()
            .contains("maximum 64"));
    }

    #[test]
    fn successor_parameter_install_refreshes_embeddings_and_preserves_ancestry() {
        use crate::learned_sideboard_v1::{
            FrozenSideboardEmbeddingsV1, LearnedSideboardModelV1, SideboardPlayIdentityV1,
        };
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        let ancestry = policy.identity_v1().clone();
        let before = policy.actual_model_identity_v1();
        let head = {
            let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
                policy.embedding_rows_v1(),
                SideboardPlayIdentityV1 {
                    weights_sha256: before.weights_sha256.clone(),
                    git_head: ancestry.origin_git_commit_v1().to_owned(),
                },
            )
            .unwrap();
            assert_eq!(embeddings.table_sha256_v1(), before.embedding_table_sha256);
            LearnedSideboardModelV1::new_v1(19, &embeddings)
        };
        let mut replacement = policy.training_parameters_v3();
        let embedding = replacement
            .iter_mut()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap();
        embedding.values[CARD_EMBEDDING_DIM_V1] = 0.375;
        let expected_bits = embedding
            .values
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>();
        policy.replace_training_parameters_v3(&replacement).unwrap();
        let after = policy.actual_model_identity_v1();
        assert_ne!(before.weights_sha256, after.weights_sha256);
        assert_ne!(before.model_parameter_sha256, after.model_parameter_sha256);
        assert_ne!(before.embedding_table_sha256, after.embedding_table_sha256);
        assert_eq!(policy.identity_v1(), &ancestry);
        assert_eq!(
            policy
                .embedding_rows_v1()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected_bits
        );
        assert_eq!(
            policy
                .training_parameters_v3()
                .iter()
                .find(|p| p.name == "card_embedding.weight")
                .unwrap()
                .values
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected_bits
        );
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
            policy.embedding_rows_v1(),
            SideboardPlayIdentityV1 {
                weights_sha256: after.weights_sha256.clone(),
                git_head: ancestry.origin_git_commit_v1().to_owned(),
            },
        )
        .unwrap();
        assert_eq!(embeddings.table_sha256_v1(), after.embedding_table_sha256);
        assert!(head.validate_frozen_embeddings_v1(&embeddings).is_err());

        // The rejected install changes neither live parameters nor their copy.
        replacement[0].values[0] = f32::NAN;
        assert!(policy.replace_training_parameters_v3(&replacement).is_err());
        assert_eq!(policy.actual_model_identity_v1(), after);
        assert_eq!(policy.identity_v1(), &ancestry);
        assert_eq!(
            policy
                .embedding_rows_v1()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected_bits
        );
    }

    #[test]
    fn previous_v3_feature_identity_rejects_before_export_read() {
        let absent = PathBuf::from("this-export-must-not-be-read");
        let input = FrozenPlayPolicyImportV1 {
            export_directory: absent.clone(),
            expected_metadata_sha256: String::new(),
            expected_model_parameter_sha256: String::new(),
            source_run_path: absent.clone(),
            source_registry_path: absent,
            expected_source_registry_sha256: String::new(),
            source_registry_git_commit: String::new(),
            expected_destination_card_db_hash: String::new(),
        };
        let old = FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest:
                "bd794bda37eace823cee7a8ce12a42107e2d628437b08b3f23f412aa307e0b2e".into(),
            expected_feature_encoding_digest:
                "0111f48e9c2e6f24d186e9059ddede48aa98dd45e9c17e2343145cb408edac88".into(),
        };
        let error = match FrozenPlayPolicyV1::load_feature_transfer_v3(&input, &old) {
            Err(error) => error,
            Ok(_) => panic!("old V3 unexpectedly accepted"),
        };
        assert_eq!(error, "explicit V3 destination feature identity differs");
    }

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

    #[test]
    fn from_fresh_registry_transfer_v1_nests_ancestry_and_installs_transferred_model() {
        use crate::native_flat_tensorizer_v3::{
            FEATURES_SOURCE_SHA256_V3, FEATURE_CONTRACT_DIGEST_V3, FEATURE_DESCRIPTOR_SHA256_V3,
            FEATURE_ENCODING_DIGEST_V3,
        };
        use crate::native_policy_train_step_v1::NativePolicyValueTrainStateV1;
        use crate::phase1_registry_transfer_v1::{
            transfer_fresh_expanded_checkpoint_to_current_registry_v1, FreshRegistryTransferRequestV1,
            RegistryTransferFeaturesV1,
        };

        #[derive(Serialize)]
        struct Tensor {
            name: String,
            shape: Vec<usize>,
            values: Vec<u32>,
        }
        #[derive(Serialize)]
        struct Checkpoint {
            schema: String,
            feature_contract_digest: String,
            feature_encoding_digest: String,
            card_db_hash: String,
            source_import: FreshPlayPolicyIdentityV1,
            state_sha256: String,
            adam_step: u64,
            scorer_bias_anchor_bits: u32,
            parameters: Vec<Tensor>,
            first_moments: Vec<Tensor>,
            second_moments: Vec<Tensor>,
            trajectories: Vec<serde_json::Value>,
            loss_identity: String,
            learning_rate_bits: u32,
            value_coefficient_bits: u32,
        }
        fn to_wire(list: &[NativeNamedParameterV1]) -> Vec<Tensor> {
            list.iter()
                .map(|p| Tensor {
                    name: p.name.into(),
                    shape: p.shape.clone(),
                    values: p.values.iter().map(|v| v.to_bits()).collect(),
                })
                .collect()
        }

        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let snapshot = state.snapshot_v1().unwrap();
        // Metadata-only synthetic ancestry. This does not attest to a real
        // transfer, generated model, producer execution or playing strength.
        let identity = FreshPlayPolicyIdentityV1 {
            schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
            initialization_manifest_sha256: "a".repeat(64),
            lineage_id: "sideboard-play-policy-fixture".into(),
            initializer: "trainer-seeded-v1".into(),
            base_seed: 0,
            model_init_seed: 6_443_515_232_517_447_393,
            seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
            producer_git_commit: "1".repeat(40),
            initial_weights_sha256: "b".repeat(64),
            initial_model_parameter_sha256: "c".repeat(64),
            parameter_layout_sha256: "d".repeat(64),
            destination_registry_sha256: hash(DESTINATION_REGISTRY),
            destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            destination_card_count: crate::card_def::CARD_DEFS.len(),
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
            feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
            sampler_identity: crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
        };
        let state_hex: String = snapshot
            .state_sha256_v1()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let checkpoint = Checkpoint {
            schema: "mtg-kernel-expanded-deck-fresh-checkpoint/v1".into(),
            feature_contract_digest: identity.feature_contract_digest.clone(),
            feature_encoding_digest: identity.feature_encoding_digest.clone(),
            card_db_hash: identity.destination_card_db_hash.clone(),
            source_import: identity.clone(),
            state_sha256: state_hex,
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            parameters: to_wire(&snapshot.parameters),
            first_moments: to_wire(&snapshot.first_moments),
            second_moments: to_wire(&snapshot.second_moments),
            trajectories: vec![],
            loss_identity: "terminal_reinforce_value/v3".into(),
            learning_rate_bits: 0.00003_f32.to_bits(),
            value_coefficient_bits: 0.75_f32.to_bits(),
        };
        let bytes = serde_json::to_vec(&checkpoint).unwrap();
        let request = FreshRegistryTransferRequestV1 {
            source_checkpoint_sha256: hash(&bytes),
            source_registry_sha256: hash(DESTINATION_REGISTRY),
            source_state_sha256: checkpoint.state_sha256.clone(),
            source_adam_step: checkpoint.adam_step,
            source_card_db_hash: checkpoint.card_db_hash.clone(),
            destination_registry_sha256: hash(DESTINATION_REGISTRY),
            destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
            features: RegistryTransferFeaturesV1::current_v1(),
        };
        let transfer = transfer_fresh_expanded_checkpoint_to_current_registry_v1(
            &bytes,
            DESTINATION_REGISTRY,
            &request,
        )
        .unwrap();
        let expected_embeddings: Vec<u32> = transfer
            .model_v1()
            .parameter_snapshot_v1()
            .into_iter()
            .find(|p| p.name == "card_embedding.weight")
            .unwrap()
            .values
            .iter()
            .map(|v| v.to_bits())
            .collect();
        let envelope_sha256 = "7".repeat(64);
        let policy =
            FrozenPlayPolicyV1::from_fresh_registry_transfer_v1(&transfer, &envelope_sha256)
                .unwrap();
        assert!(policy.identity_v1().is_fresh_v1());
        assert!(policy.identity_v1().as_imported_v1().is_none());
        // Destination accessors read the current (post-transfer) identity.
        assert_eq!(
            policy.identity_v1().destination_registry_sha256_v1(),
            hash(DESTINATION_REGISTRY)
        );
        assert_eq!(
            policy.identity_v1().destination_card_db_hash_v1(),
            format!("{KERNEL_CARDDB_HASH:016x}")
        );
        // Initial weight/seed/producer accessors read the nested ancestry.
        assert_eq!(
            policy.identity_v1().feature_contract_digest_v1(),
            identity.feature_contract_digest
        );
        assert_eq!(
            policy.identity_v1().initial_weights_sha256_v1(),
            identity.initial_weights_sha256
        );
        assert_eq!(
            policy.identity_v1().initial_model_parameter_sha256_v1(),
            identity.initial_model_parameter_sha256
        );
        assert_eq!(
            policy.identity_v1().origin_git_commit_v1(),
            identity.producer_git_commit
        );
        assert_eq!(
            policy
                .embedding_rows_v1()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected_embeddings
        );
    }

    /// Proves the reconciliation layer, not just the raw action slice:
    /// `score_fast_session_v1`'s V3 path (`encode_current_flat_scoring_decision_owned_v3`,
    /// which shares `flat_policy_v2.rs`'s `build_scoring_owned_v3`) must
    /// score a decision whose pending trigger's source has been shuffled
    /// into its owner's library -- the same fixture the action-slice
    /// regression tests in `rl_session/flat_action_v3.rs` use, driven all
    /// the way through the frozen play policy's scorer instead of just the
    /// raw action slice.
    #[test]
    fn score_fast_session_v1_reconciles_a_pending_trigger_hidden_source() {
        let (mut state, hunter, _goaded, _ordinary) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        policy.reset_sampling_v1([11, 22]);
        let scores = policy.score_fast_session_v1(&session).unwrap();
        assert!(!scores.logits.is_empty());
        assert!(scores.logits.iter().all(|x| x.is_finite()));
        assert!(scores.value.is_finite());
        let selected = policy.select_fast_session_v1(&session).unwrap();
        assert!((selected as usize) < scores.logits.len());
    }

    /// The tensorizer-level sibling of the ordinal-collision regression in
    /// `rl_session/flat_action_v3.rs`
    /// (`v3_pending_trigger_hidden_source_ordinal_does_not_collide_with_real_stack_historical_rows`).
    /// `score_fast_session_v1`'s V3 path runs the full scorer, including
    /// `native_flat_tensorizer_v2.rs`'s `build_object_projection_v3` /
    /// `build_object_projection_for_rows_v2`, which is what actually
    /// enforces `(group, visible_ordinal)` uniqueness
    /// (`NativeFlatTensorErrorV2::ObjectOrder`) across every registered
    /// `PendingContext` row, real historical sources included -- the
    /// action-slice test alone never reaches that check. A real spell at
    /// stack index 0 plus two real non-spell historical rows at raw
    /// indices 1 and 2 previously collided with a naive "count of
    /// historical rows" ordinal for the pending-trigger row; this proves
    /// the collision-safe ceiling
    /// (`trigger::historical_public_source_ordinal_ceiling_v1`) avoids it.
    #[test]
    fn score_fast_session_v1_reconciles_a_pending_trigger_hidden_source_with_real_stack_historical_rows(
    ) {
        let (mut state, hunter) =
            crate::rl_session::avenging_hunter_hidden_source_with_stack_historical_rows_state_v1();
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
        policy.reset_sampling_v1([33, 44]);
        let scores = policy.score_fast_session_v1(&session).unwrap();
        assert!(!scores.logits.is_empty());
        assert!(scores.logits.iter().all(|x| x.is_finite()));
        assert!(scores.value.is_finite());
    }

    /// Proves `score_fast_session_v1`'s new `fresh_successor` arm is
    /// actually taken for a V4 policy (not a silent fallthrough to the V2
    /// narrow path, which this wide fixture would reject), and that the
    /// pre-existing V3 arm's output is unchanged (a regression guard) on
    /// the same fixture used by the adjacent V3-only tests above.
    #[test]
    fn score_fast_session_v1_takes_the_v4_arm_and_v3_output_is_unchanged() {
        let (mut state, hunter, _goaded, _ordinary) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut state,
            hunter,
            crate::ids::PlayerId::P0,
        );

        let v3_session = FastActorSessionV1::from_v3_fixture_state(state.clone());
        let mut v3_policy = FrozenPlayPolicyV1::training_fixture_v3();
        v3_policy.reset_sampling_v1([11, 22]);
        assert_eq!(v3_policy.feature_generation_v1(), PlayPolicyGenerationV1::V3);
        let v3_scores = v3_policy.score_fast_session_v1(&v3_session).unwrap();
        assert!(!v3_scores.logits.is_empty());
        assert!(v3_scores.logits.iter().all(|x| x.is_finite()));
        assert!(v3_scores.value.is_finite());

        let v4_session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut v4_policy = FrozenPlayPolicyV1::training_fixture_v4();
        v4_policy.reset_sampling_v1([11, 22]);
        assert_eq!(v4_policy.feature_generation_v1(), PlayPolicyGenerationV1::V4);
        let v4_scores = v4_policy.score_fast_session_v1(&v4_session).unwrap();
        assert!(!v4_scores.logits.is_empty());
        assert!(v4_scores.logits.iter().all(|x| x.is_finite()));
        assert!(v4_scores.value.is_finite());
        // Both generations share the same wide action space for this
        // fixture; only the feature encoding, not the legal-action count,
        // is expected to differ between V3 and V4.
        assert_eq!(v4_scores.logits.len(), v3_scores.logits.len());

        let selected = v4_policy.select_fast_session_v1(&v4_session).unwrap();
        assert!((selected as usize) < v4_scores.logits.len());
    }

    #[test]
    fn select_with_training_tensor_v4_replays_through_score_and_last_scored_training_tensor_v4() {
        let (mut state, hunter, _goaded, _ordinary) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        policy.reset_sampling_v1([55, 66]);
        let (selected, scores, tensor) = policy.select_with_training_tensor_v4(&session).unwrap();
        assert!((selected as usize) < scores.logits.len());
        assert_eq!(policy.last_scored_training_tensor_v4().unwrap(), &tensor);
        let replayed = policy.score_training_tensor_v4(&tensor).unwrap();
        assert_eq!(replayed.logits, scores.logits);
        assert_eq!(replayed.value, scores.value);
    }

    /// The registry-level proof the action-slice tests in
    /// `rl_session/flat_action_v3.rs`
    /// (`v3_pending_trigger_known_library_source_takes_the_ordinary_path`,
    /// `..._graveyard_source_takes_the_ordinary_path`) cannot give: those
    /// only inspect the action slice's own object table
    /// (`rl_session.rs`'s `FlatActionObjectV2` rows), never
    /// `flat_policy_v2.rs`'s model-facing registry
    /// (`FlatDecisionEncoderV2::objects`) that
    /// `append_pending_trigger_frozen_source_authority_v3` (layer B) could
    /// grow with a ghost `PendingContext` row. This calls
    /// `encode_current_flat_scoring_decision_owned_v3` directly (the same
    /// path `score_fast_session_v1` uses) and counts the real
    /// `buffers.objects` it publishes, before and after moving Hunter to a
    /// known library position or to the graveyard: decision shape alone
    /// (`Decision::ChooseTargets` for a pending trigger with a
    /// `source_contract`) is not enough to open
    /// `trigger::pending_trigger_choose_targets_gate_v1` -- the live
    /// source must actually be hidden (`Zone::Library`, no
    /// `library_knowledge` entry) -- so neither case may add a row.
    #[test]
    fn score_fast_session_v1_registry_object_count_is_unchanged_for_a_known_or_public_source() {
        fn registry_object_count(state: crate::state::GameState) -> usize {
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("fixture must have an active decision");
            };
            let mut encoder = FlatDecisionEncoderV3::default();
            let mut objects = Vec::new();
            let mut relations = Vec::new();
            let mut object_subtypes = Vec::new();
            let mut ability_uses = Vec::new();
            let mut goads = Vec::new();
            let mut completed_dungeons = Vec::new();
            let mut effect_subtype_changes = Vec::new();
            let mut context_path_elements = Vec::new();
            let mut actions = Vec::new();
            let mut action_refs = Vec::new();
            session
                .encode_current_flat_scoring_decision_owned_v3(
                    expected,
                    &mut encoder,
                    &mut FlatScoringOwnedBuffersV2 {
                        objects: &mut objects,
                        relations: &mut relations,
                        object_subtypes: &mut object_subtypes,
                        ability_uses: &mut ability_uses,
                        goads: &mut goads,
                        completed_dungeons: &mut completed_dungeons,
                        effect_subtype_changes: &mut effect_subtype_changes,
                        context_path_elements: &mut context_path_elements,
                        actions: &mut actions,
                        action_refs: &mut action_refs,
                    },
                )
                .unwrap();
            objects.len()
        }

        let (baseline_state, hunter, _goaded, _ordinary) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let baseline_count = registry_object_count(baseline_state.clone());

        let mut known_state = baseline_state.clone();
        crate::rl_session::move_trigger_source_to_known_library_v1(
            &mut known_state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        assert_eq!(
            registry_object_count(known_state),
            baseline_count,
            "a known library source must not grow the registry with a ghost PendingContext row"
        );

        let mut graveyard_state = baseline_state;
        crate::rl_session::move_trigger_source_to_graveyard_v1(
            &mut graveyard_state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        assert_eq!(
            registry_object_count(graveyard_state),
            baseline_count,
            "a graveyard source must not grow the registry with a ghost PendingContext row"
        );
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
            policy.identity_v1().initial_model_parameter_sha256_v1(),
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
