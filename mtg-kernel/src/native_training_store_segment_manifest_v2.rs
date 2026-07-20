//! Pure SegmentManifestV2 authority for Native Training Store V2.
//!
//! Genesis authority is rooted directly in a validated update-zero checkpoint.
//! Trained authority additionally requires a sealed logical parent boundary,
//! the complete continuation-chain advance, and its exact trained checkpoint.
//! This module owns no filesystem path, publisher, recovery, receipt, latest,
//! reference, or executor mutation.

use crate::canonical_json_v1::{
    count_canonical_json_bytes_v1, from_canonical_json_bytes_v1, to_canonical_json_bytes_v1,
    CanonicalJsonErrorKindV1, CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1,
    CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_continuation_v2::{
    ValidatedSegmentContinuationChainAdvanceV2, SEGMENT_CONTINUATION_MAX_BYTES_V2,
    SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const SEGMENT_MANIFEST_SCHEMA_V2: &str = "mtg_kernel_native_train_checkpoint_segment/v2";
pub const SEGMENT_MANIFEST_MAX_BYTES_V2: u64 = 4_194_304;
pub const ORDERED_UPDATE_EVIDENCE_LIST_DIGEST_IDENTITY_V1: &str =
    "mtg-kernel-native-training-ordered-update-evidence-list-sha256-v1";
pub const SEGMENT_MANIFEST_RECORD_CONTRACT_SHA256_V2: &str =
    crate::native_training_store_checkpoint_v3::NATIVE_TRAINING_STORE_RECORD_CONTRACT_SHA256_V1;

const U63_MAX_V2: u64 = (1_u64 << 63) - 1;

const PARENT_GENERATION_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "parent_generation_index",
    )];
const PARENT_HEAD_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "parent_head_sha256",
    )];
const PARENT_LAST_EVIDENCE_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "parent_last_update_evidence_sha256",
    )];
const FIRST_CONTINUATION_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("continuation_chain"),
    CanonicalJsonNullPathSegmentV1::ObjectKey("first_continuation_sha256"),
];
const LAST_CONTINUATION_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("continuation_chain"),
    CanonicalJsonNullPathSegmentV1::ObjectKey("last_continuation_sha256"),
];
const DESCRIPTOR_PREDECESSOR_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("continuation_chain"),
    CanonicalJsonNullPathSegmentV1::ObjectKey("continuations"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("previous_continuation_sha256"),
];
const SEGMENT_MANIFEST_NULL_PATHS_V2: &[&[CanonicalJsonNullPathSegmentV1]] = &[
    PARENT_GENERATION_NULL_PATH_V2,
    PARENT_HEAD_NULL_PATH_V2,
    PARENT_LAST_EVIDENCE_NULL_PATH_V2,
    FIRST_CONTINUATION_NULL_PATH_V2,
    LAST_CONTINUATION_NULL_PATH_V2,
    DESCRIPTOR_PREDECESSOR_NULL_PATH_V2,
];
const SEGMENT_MANIFEST_NULL_POLICY_V2: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(SEGMENT_MANIFEST_NULL_PATHS_V2);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OrderedUpdateEvidenceRowWireV1 {
    update_index: u64,
    update_evidence_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ContinuationDescriptorWireV2 {
    continuation_index: u64,
    relative_name: String,
    byte_count: u64,
    sha256: String,
    previous_continuation_sha256: Option<String>,
    update_group_start_ordinal: u64,
    update_group_count: u64,
    logical_row_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ContinuationChainWireV2 {
    continuation_count: u64,
    update_group_count: u64,
    logical_row_count: u64,
    first_continuation_sha256: Option<String>,
    last_continuation_sha256: Option<String>,
    continuations: Vec<ContinuationDescriptorWireV2>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FinalCheckpointBindingWireV2 {
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    logical_state_sha256: String,
    model_parameter_sha256: String,
    train_state_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SegmentManifestWireV2 {
    schema: String,
    kind: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    parent_generation_index: Option<u64>,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    update_start_index: u64,
    update_count: u64,
    episode_start: u64,
    episode_count: u64,
    episode_end_exclusive: u64,
    parent_head_sha256: Option<String>,
    parent_last_update_evidence_sha256: Option<String>,
    ordered_update_evidence_count: u64,
    ordered_update_evidence: Vec<OrderedUpdateEvidenceRowWireV1>,
    ordered_update_evidence_list_sha256: String,
    continuation_chain: ContinuationChainWireV2,
    final_checkpoint: FinalCheckpointBindingWireV2,
}

struct DerivedTrainedManifestV2 {
    wire: SegmentManifestWireV2,
    ordered_update_evidence_list_sha256: [u8; 32],
}

/// Fully validated pure SegmentManifestV2 authority.
///
/// The authority is move-only and has neither an unchecked constructor nor a
/// serde deserializer:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_segment_manifest_v2::SegmentManifestV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<SegmentManifestV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_segment_manifest_v2::SegmentManifestV2;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<SegmentManifestV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_segment_manifest_v2::SegmentManifestV2;
/// let _ = SegmentManifestV2 {};
/// ```
pub struct SegmentManifestV2 {
    wire: SegmentManifestWireV2,
    canonical_bytes: Vec<u8>,
    segment_manifest_sha256: [u8; 32],
    ordered_update_evidence_list_sha256: [u8; 32],
    parent_head_sha256: Option<[u8; 32]>,
    parent_last_update_evidence_sha256: Option<[u8; 32]>,
    last_update_evidence_sha256: Option<[u8; 32]>,
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    logical_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
}

/// Narrow crate-internal projection for the later sidecar/head boundary.
///
/// This is not a parent capability and is never accepted by a public
/// constructor. It only projects already-validated facts from the sealed
/// manifest authority.
#[allow(dead_code)]
pub(crate) struct SegmentManifestBoundaryFactsV2<'a> {
    pub(crate) kind: &'a str,
    pub(crate) run_sha256: &'a str,
    pub(crate) identity_bundle_sha256: &'a str,
    pub(crate) segment_ordinal: u64,
    pub(crate) parent_generation_index: Option<u64>,
    pub(crate) generation_index: u64,
    pub(crate) batch_episodes: u64,
    pub(crate) checkpoint_segment_updates: u64,
    pub(crate) parent_head_sha256: Option<[u8; 32]>,
    pub(crate) parent_last_update_evidence_sha256: Option<[u8; 32]>,
    pub(crate) last_update_evidence_sha256: Option<[u8; 32]>,
    pub(crate) segment_manifest_sha256: [u8; 32],
    pub(crate) ordered_update_evidence_list_sha256: [u8; 32],
    pub(crate) checkpoint_manifest_sha256: [u8; 32],
    pub(crate) checkpoint_payload_sha256: [u8; 32],
    pub(crate) logical_state_sha256: [u8; 32],
    pub(crate) model_parameter_sha256: [u8; 32],
    pub(crate) train_state_sha256: [u8; 32],
}

impl std::fmt::Debug for SegmentManifestV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SegmentManifestV2")
            .field("kind", &self.wire.kind)
            .field("segment_ordinal", &self.wire.segment_ordinal)
            .field("generation_index", &self.wire.generation_index)
            .field(
                "segment_manifest_sha256",
                &lower_hex_raw32_v1(self.segment_manifest_sha256),
            )
            .finish_non_exhaustive()
    }
}

impl SegmentManifestV2 {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub const fn segment_manifest_sha256(&self) -> [u8; 32] {
        self.segment_manifest_sha256
    }

    #[allow(dead_code)]
    pub(crate) fn boundary_facts_v2(&self) -> SegmentManifestBoundaryFactsV2<'_> {
        SegmentManifestBoundaryFactsV2 {
            kind: &self.wire.kind,
            run_sha256: &self.wire.run_sha256,
            identity_bundle_sha256: &self.wire.identity_bundle_sha256,
            segment_ordinal: self.wire.segment_ordinal,
            parent_generation_index: self.wire.parent_generation_index,
            generation_index: self.wire.generation_index,
            batch_episodes: self.wire.batch_episodes,
            checkpoint_segment_updates: self.wire.checkpoint_segment_updates,
            parent_head_sha256: self.parent_head_sha256,
            parent_last_update_evidence_sha256: self.parent_last_update_evidence_sha256,
            last_update_evidence_sha256: self.last_update_evidence_sha256,
            segment_manifest_sha256: self.segment_manifest_sha256,
            ordered_update_evidence_list_sha256: self.ordered_update_evidence_list_sha256,
            checkpoint_manifest_sha256: self.checkpoint_manifest_sha256,
            checkpoint_payload_sha256: self.checkpoint_payload_sha256,
            logical_state_sha256: self.logical_state_sha256,
            model_parameter_sha256: self.model_parameter_sha256,
            train_state_sha256: self.train_state_sha256,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentManifestV2ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidKind,
    InvalidDigest,
    InvalidScalar,
    InvalidArithmetic,
    RunBinding,
    GenesisInvariant,
    TrainedInvariant,
    ParentBoundaryBinding,
    ContinuationBinding,
    FinalCheckpointBinding,
    OrderedEvidenceDigestMismatch,
    TrainedAuthorityRequired,
}

impl SegmentManifestV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_checkpoint_segment_v2_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_train_checkpoint_segment_v2_invalid_schema",
            Self::InvalidKind => "native_train_checkpoint_segment_v2_invalid_kind",
            Self::InvalidDigest => "native_train_checkpoint_segment_v2_invalid_digest",
            Self::InvalidScalar => "native_train_checkpoint_segment_v2_invalid_scalar",
            Self::InvalidArithmetic => "native_train_checkpoint_segment_v2_invalid_arithmetic",
            Self::RunBinding => "native_train_checkpoint_segment_v2_run_binding",
            Self::GenesisInvariant => "native_train_checkpoint_segment_v2_genesis_invariant",
            Self::TrainedInvariant => "native_train_checkpoint_segment_v2_trained_invariant",
            Self::ParentBoundaryBinding => {
                "native_train_checkpoint_segment_v2_parent_boundary_binding"
            }
            Self::ContinuationBinding => "native_train_checkpoint_segment_v2_continuation_binding",
            Self::FinalCheckpointBinding => {
                "native_train_checkpoint_segment_v2_final_checkpoint_binding"
            }
            Self::OrderedEvidenceDigestMismatch => {
                "native_train_checkpoint_segment_v2_ordered_evidence_digest_mismatch"
            }
            Self::TrainedAuthorityRequired => "trained_segment_manifest_context_required",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentManifestV2Error {
    kind: SegmentManifestV2ErrorKind,
}

impl SegmentManifestV2Error {
    const fn new(kind: SegmentManifestV2ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> SegmentManifestV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for SegmentManifestV2Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(SegmentManifestV2ErrorKind::CanonicalJson(error.kind()))
    }
}

impl Display for SegmentManifestV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for SegmentManifestV2Error {}

type Result<T> = std::result::Result<T, SegmentManifestV2Error>;

/// Builds the exact genesis SegmentManifestV2 from an independently validated
/// update-zero checkpoint. No trained input or parent capability is accepted.
pub fn build_genesis_segment_manifest_v2(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<SegmentManifestV2> {
    validate_genesis_checkpoint_v2(run, checkpoint)?;
    let ordered_update_evidence_list_sha256 =
        ordered_update_evidence_list_sha256_v2(run.run_sha256(), 0, 0, &[])?;
    let wire = SegmentManifestWireV2 {
        schema: SEGMENT_MANIFEST_SCHEMA_V2.to_owned(),
        kind: "genesis".to_owned(),
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        segment_ordinal: 0,
        parent_generation_index: None,
        generation_index: 0,
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates: run.checkpoint_segment_updates(),
        update_start_index: 0,
        update_count: 0,
        episode_start: 0,
        episode_count: 0,
        episode_end_exclusive: 0,
        parent_head_sha256: None,
        parent_last_update_evidence_sha256: None,
        ordered_update_evidence_count: 0,
        ordered_update_evidence: Vec::new(),
        ordered_update_evidence_list_sha256: lower_hex_raw32_v1(
            ordered_update_evidence_list_sha256,
        ),
        continuation_chain: ContinuationChainWireV2 {
            continuation_count: 0,
            update_group_count: 0,
            logical_row_count: 0,
            first_continuation_sha256: None,
            last_continuation_sha256: None,
            continuations: Vec::new(),
        },
        final_checkpoint: FinalCheckpointBindingWireV2 {
            checkpoint_manifest_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256()),
            checkpoint_payload_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()),
            logical_state_sha256: lower_hex_raw32_v1(checkpoint.logical_state_sha256()),
            model_parameter_sha256: lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
            train_state_sha256: lower_hex_raw32_v1(checkpoint.train_state_sha256()),
        },
    };
    let canonical_bytes = encode_segment_manifest_wire_v2(&wire)?;
    decode_genesis_segment_manifest_v2(&canonical_bytes, run, checkpoint)
}

/// Builds one trained SegmentManifestV2 from the sealed logical parent,
/// complete continuation advance, and exact final trained checkpoint.
pub fn build_trained_segment_manifest_v2(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    continuations: &ValidatedSegmentContinuationChainAdvanceV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<SegmentManifestV2> {
    let derived = derive_trained_manifest_v2(run, parent, continuations, checkpoint)?;
    let manifest_cj = encode_segment_manifest_wire_v2(&derived.wire)?;
    decode_trained_segment_manifest_v2(&manifest_cj, run, parent, continuations, checkpoint)
}

/// Decodes and validates only the genesis SegmentManifestV2 variant.
/// Canonical trained bytes require the separate explicit parent-bound API.
pub fn decode_genesis_segment_manifest_v2(
    manifest_cj: &[u8],
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<SegmentManifestV2> {
    let byte_count = u64::try_from(manifest_cj.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::RecordTooLarge))?;
    if byte_count > SEGMENT_MANIFEST_MAX_BYTES_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::RecordTooLarge,
        ));
    }
    let wire: SegmentManifestWireV2 =
        from_canonical_json_bytes_v1(manifest_cj, SEGMENT_MANIFEST_NULL_POLICY_V2)?;
    validate_common_wire_v2(&wire)?;
    validate_genesis_wire_v2(&wire, run)?;
    validate_genesis_checkpoint_v2(run, checkpoint)?;
    validate_final_checkpoint_binding_v2(&wire.final_checkpoint, checkpoint)?;

    let ordered_update_evidence_list_sha256 = ordered_update_evidence_list_sha256_v2(
        &wire.run_sha256,
        wire.generation_index,
        wire.update_count,
        &wire.ordered_update_evidence,
    )?;
    if wire.ordered_update_evidence_list_sha256
        != lower_hex_raw32_v1(ordered_update_evidence_list_sha256)
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::OrderedEvidenceDigestMismatch,
        ));
    }

    let checkpoint_manifest_sha256 =
        parse_digest_v2(&wire.final_checkpoint.checkpoint_manifest_sha256)?;
    let checkpoint_payload_sha256 =
        parse_digest_v2(&wire.final_checkpoint.checkpoint_payload_sha256)?;
    let logical_state_sha256 = parse_digest_v2(&wire.final_checkpoint.logical_state_sha256)?;
    let model_parameter_sha256 = parse_digest_v2(&wire.final_checkpoint.model_parameter_sha256)?;
    let train_state_sha256 = parse_digest_v2(&wire.final_checkpoint.train_state_sha256)?;
    let parent_head_sha256 = wire
        .parent_head_sha256
        .as_deref()
        .map(parse_digest_v2)
        .transpose()?;
    let parent_last_update_evidence_sha256 = wire
        .parent_last_update_evidence_sha256
        .as_deref()
        .map(parse_digest_v2)
        .transpose()?;
    let last_update_evidence_sha256 = wire
        .ordered_update_evidence
        .last()
        .map(|row| parse_digest_v2(&row.update_evidence_sha256))
        .transpose()?;

    Ok(SegmentManifestV2 {
        wire,
        canonical_bytes: manifest_cj.to_vec(),
        segment_manifest_sha256: sha256_v1(manifest_cj),
        ordered_update_evidence_list_sha256,
        parent_head_sha256,
        parent_last_update_evidence_sha256,
        last_update_evidence_sha256,
        checkpoint_manifest_sha256,
        checkpoint_payload_sha256,
        logical_state_sha256,
        model_parameter_sha256,
        train_state_sha256,
    })
}

/// Decodes and validates one trained SegmentManifestV2 against only sealed
/// parent, continuation-advance, and checkpoint authorities.
pub fn decode_trained_segment_manifest_v2(
    manifest_cj: &[u8],
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    continuations: &ValidatedSegmentContinuationChainAdvanceV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<SegmentManifestV2> {
    require_manifest_cap_v2(manifest_cj)?;
    let wire: SegmentManifestWireV2 =
        from_canonical_json_bytes_v1(manifest_cj, SEGMENT_MANIFEST_NULL_POLICY_V2)?;
    validate_trained_common_wire_v2(&wire)?;
    let derived = derive_trained_manifest_v2(run, parent, continuations, checkpoint)?;

    validate_trained_run_and_equations_v2(&wire, run, &derived.wire)?;
    validate_trained_parent_fields_v2(&wire, &derived.wire)?;
    let supplied_ordered_digest = ordered_update_evidence_list_sha256_v2(
        &wire.run_sha256,
        wire.generation_index,
        wire.update_count,
        &wire.ordered_update_evidence,
    )?;
    if wire.ordered_update_evidence_list_sha256 != lower_hex_raw32_v1(supplied_ordered_digest) {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::OrderedEvidenceDigestMismatch,
        ));
    }
    if wire.ordered_update_evidence != derived.wire.ordered_update_evidence
        || supplied_ordered_digest != derived.ordered_update_evidence_list_sha256
        || wire.continuation_chain != derived.wire.continuation_chain
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ContinuationBinding,
        ));
    }
    if wire.final_checkpoint != derived.wire.final_checkpoint {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::FinalCheckpointBinding,
        ));
    }

    authority_from_wire_v2(wire, manifest_cj, supplied_ordered_digest)
}

fn encode_segment_manifest_wire_v2(wire: &SegmentManifestWireV2) -> Result<Vec<u8>> {
    let count = count_canonical_json_bytes_v1(wire, SEGMENT_MANIFEST_NULL_POLICY_V2)?;
    if count > SEGMENT_MANIFEST_MAX_BYTES_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::RecordTooLarge,
        ));
    }
    let canonical_bytes = to_canonical_json_bytes_v1(wire, SEGMENT_MANIFEST_NULL_POLICY_V2)?;
    let emitted = u64::try_from(canonical_bytes.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    if emitted != count {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidArithmetic,
        ));
    }
    Ok(canonical_bytes)
}

fn require_manifest_cap_v2(manifest_cj: &[u8]) -> Result<()> {
    let byte_count = u64::try_from(manifest_cj.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::RecordTooLarge))?;
    if byte_count > SEGMENT_MANIFEST_MAX_BYTES_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::RecordTooLarge,
        ));
    }
    Ok(())
}

fn authority_from_wire_v2(
    wire: SegmentManifestWireV2,
    manifest_cj: &[u8],
    ordered_update_evidence_list_sha256: [u8; 32],
) -> Result<SegmentManifestV2> {
    let checkpoint_manifest_sha256 =
        parse_digest_v2(&wire.final_checkpoint.checkpoint_manifest_sha256)?;
    let checkpoint_payload_sha256 =
        parse_digest_v2(&wire.final_checkpoint.checkpoint_payload_sha256)?;
    let logical_state_sha256 = parse_digest_v2(&wire.final_checkpoint.logical_state_sha256)?;
    let model_parameter_sha256 = parse_digest_v2(&wire.final_checkpoint.model_parameter_sha256)?;
    let train_state_sha256 = parse_digest_v2(&wire.final_checkpoint.train_state_sha256)?;
    let parent_head_sha256 = wire
        .parent_head_sha256
        .as_deref()
        .map(parse_digest_v2)
        .transpose()?;
    let parent_last_update_evidence_sha256 = wire
        .parent_last_update_evidence_sha256
        .as_deref()
        .map(parse_digest_v2)
        .transpose()?;
    let last_update_evidence_sha256 = wire
        .ordered_update_evidence
        .last()
        .map(|row| parse_digest_v2(&row.update_evidence_sha256))
        .transpose()?;

    Ok(SegmentManifestV2 {
        wire,
        canonical_bytes: manifest_cj.to_vec(),
        segment_manifest_sha256: sha256_v1(manifest_cj),
        ordered_update_evidence_list_sha256,
        parent_head_sha256,
        parent_last_update_evidence_sha256,
        last_update_evidence_sha256,
        checkpoint_manifest_sha256,
        checkpoint_payload_sha256,
        logical_state_sha256,
        model_parameter_sha256,
        train_state_sha256,
    })
}

fn validate_trained_common_wire_v2(wire: &SegmentManifestWireV2) -> Result<()> {
    if wire.schema != SEGMENT_MANIFEST_SCHEMA_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidSchema,
        ));
    }
    match wire.kind.as_str() {
        "trained" => {}
        "genesis" => {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::TrainedInvariant,
            ));
        }
        _ => {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::InvalidKind,
            ));
        }
    }
    validate_scalars_v2(wire)?;
    validate_digest_encodings_v2(wire)?;
    let ordered_count = u64::try_from(wire.ordered_update_evidence.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    let continuation_count = u64::try_from(wire.continuation_chain.continuations.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    if wire.ordered_update_evidence_count != wire.update_count
        || wire.ordered_update_evidence_count != ordered_count
        || wire.continuation_chain.continuation_count != continuation_count
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::TrainedInvariant,
        ));
    }
    Ok(())
}

fn derive_trained_manifest_v2(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    continuation_advance: &ValidatedSegmentContinuationChainAdvanceV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<DerivedTrainedManifestV2> {
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    let batch_episodes = run.batch_episodes();
    require_positive_u63_v2(checkpoint_segment_updates)?;
    require_positive_u63_v2(batch_episodes)?;
    let run_sha256_raw = parse_digest_v2(run.run_sha256())?;
    let identity_bundle_sha256_raw = parse_digest_v2(run.identity_bundle_sha256())?;

    let parent_facts = parent.boundary_facts_v2();
    if parent_facts.run_sha256 != run.run_sha256()
        || parent_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || parent_facts.batch_episodes != batch_episodes
        || parent_facts.checkpoint_segment_updates != checkpoint_segment_updates
        || parent_facts.head_sha256 != parent.head_sha256()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ParentBoundaryBinding,
        ));
    }
    let parent_generation_index = parent_facts.generation_index;
    require_u63_v2(parent_generation_index)?;
    if parent_facts.last_update_evidence_sha256.is_none() != (parent_generation_index == 0) {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ParentBoundaryBinding,
        ));
    }
    let expected_parent_generation =
        checked_u63_mul_v2(parent_facts.segment_ordinal, checkpoint_segment_updates)?;
    if expected_parent_generation != parent_generation_index {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ParentBoundaryBinding,
        ));
    }
    let generation_index = checked_u63_add_v2(parent_generation_index, checkpoint_segment_updates)?;
    let segment_ordinal = checked_u63_add_v2(parent_facts.segment_ordinal, 1)?;
    if checked_u63_mul_v2(segment_ordinal, checkpoint_segment_updates)? != generation_index
        || generation_index > run.requested_successful_updates()
        || generation_index > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::TrainedInvariant,
        ));
    }
    let update_start_index = checked_u63_add_v2(parent_generation_index, 1)?;
    let episode_start = checked_u63_mul_v2(batch_episodes, parent_generation_index)?;
    let episode_count = checked_u63_mul_v2(batch_episodes, checkpoint_segment_updates)?;
    let episode_end_exclusive = checked_u63_mul_v2(batch_episodes, generation_index)?;

    let chain = continuation_advance.chain();
    if chain.segment_ordinal() != segment_ordinal
        || chain.parent_generation_index() != parent_generation_index
        || chain.generation_index() != generation_index
        || chain.batch_episodes() != batch_episodes
        || chain.checkpoint_segment_updates() != checkpoint_segment_updates
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ContinuationBinding,
        ));
    }
    let expected_group_count = usize::try_from(checkpoint_segment_updates)
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    if chain.ordered_update_evidence().len() != expected_group_count
        || chain.continuations().is_empty()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ContinuationBinding,
        ));
    }

    let continuation_count = u64::try_from(chain.continuations().len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    require_positive_u63_v2(continuation_count)?;
    let mut descriptors = Vec::with_capacity(chain.continuations().len());
    let mut ordered_rows = Vec::with_capacity(expected_group_count);
    let mut expected_previous_continuation = None;
    let mut expected_previous_update = parent_facts.last_update_evidence_sha256;
    let mut cumulative_group_count = 0_u64;
    let mut total_logical_rows = 0_u64;

    for (continuation_position, continuation) in chain.continuations().iter().enumerate() {
        let continuation_index = u64::try_from(continuation_position).map_err(|_| {
            SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic)
        })?;
        if continuation_index > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2
            || continuation.run_sha256() != run.run_sha256()
            || continuation.identity_bundle_sha256() != run.identity_bundle_sha256()
            || continuation.segment_ordinal() != segment_ordinal
            || continuation.parent_generation_index() != parent_generation_index
            || continuation.generation_index() != generation_index
            || continuation.batch_episodes() != batch_episodes
            || continuation.checkpoint_segment_updates() != checkpoint_segment_updates
            || continuation.continuation_index() != continuation_index
            || continuation.previous_continuation_sha256() != expected_previous_continuation
            || continuation.update_group_start_ordinal() != cumulative_group_count
            || continuation.update_groups().is_empty()
        {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::ContinuationBinding,
            ));
        }

        let byte_count = u64::try_from(continuation.canonical_bytes().len()).map_err(|_| {
            SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic)
        })?;
        require_positive_u63_v2(byte_count)?;
        if byte_count > SEGMENT_CONTINUATION_MAX_BYTES_V2
            || sha256_v1(continuation.canonical_bytes()) != continuation.continuation_sha256()
        {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::ContinuationBinding,
            ));
        }

        let update_group_count =
            u64::try_from(continuation.update_group_count()).map_err(|_| {
                SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic)
            })?;
        require_positive_u63_v2(update_group_count)?;
        let mut continuation_logical_rows = 0_u64;
        for group in continuation.update_groups() {
            let global_group_ordinal = u64::try_from(ordered_rows.len()).map_err(|_| {
                SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic)
            })?;
            if global_group_ordinal >= checkpoint_segment_updates {
                return Err(SegmentManifestV2Error::new(
                    SegmentManifestV2ErrorKind::ContinuationBinding,
                ));
            }
            let expected_update_index = checked_u63_add_v2(
                checked_u63_add_v2(parent_generation_index, global_group_ordinal)?,
                1,
            )?;
            let logical_row_count = group.logical_row_count();
            require_positive_u63_v2(logical_row_count)?;
            if group.update_index() != expected_update_index
                || group.previous_update_evidence_sha256() != expected_previous_update
            {
                return Err(SegmentManifestV2Error::new(
                    SegmentManifestV2ErrorKind::ContinuationBinding,
                ));
            }
            let evidence_sha256 = group.update_evidence_sha256();
            if chain.ordered_update_evidence()[ordered_rows.len()]
                != (expected_update_index, evidence_sha256)
            {
                return Err(SegmentManifestV2Error::new(
                    SegmentManifestV2ErrorKind::ContinuationBinding,
                ));
            }
            ordered_rows.push(OrderedUpdateEvidenceRowWireV1 {
                update_index: expected_update_index,
                update_evidence_sha256: lower_hex_raw32_v1(evidence_sha256),
            });
            expected_previous_update = Some(evidence_sha256);
            continuation_logical_rows =
                checked_u63_add_v2(continuation_logical_rows, logical_row_count)?;
        }
        if continuation_logical_rows != continuation.logical_row_count() {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::ContinuationBinding,
            ));
        }
        total_logical_rows = checked_u63_add_v2(total_logical_rows, continuation_logical_rows)?;
        descriptors.push(ContinuationDescriptorWireV2 {
            continuation_index,
            relative_name: continuation_relative_name_v2(generation_index, continuation_index)?,
            byte_count,
            sha256: lower_hex_raw32_v1(continuation.continuation_sha256()),
            previous_continuation_sha256: expected_previous_continuation.map(lower_hex_raw32_v1),
            update_group_start_ordinal: cumulative_group_count,
            update_group_count,
            logical_row_count: continuation_logical_rows,
        });
        cumulative_group_count = checked_u63_add_v2(cumulative_group_count, update_group_count)?;
        expected_previous_continuation = Some(continuation.continuation_sha256());
    }

    if ordered_rows.len() != expected_group_count
        || cumulative_group_count != checkpoint_segment_updates
        || total_logical_rows == 0
        || expected_previous_update != chain.ordered_update_evidence().last().map(|row| row.1)
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ContinuationBinding,
        ));
    }
    let first_continuation_sha256 = descriptors
        .first()
        .map(|descriptor| descriptor.sha256.clone());
    let last_continuation_sha256 = descriptors
        .last()
        .map(|descriptor| descriptor.sha256.clone());

    let advanced = continuation_advance.advanced_context();
    let expected_next_update_index = checked_u63_add_v2(generation_index, 1)?;
    if advanced.run_sha256_raw_v1() != run_sha256_raw
        || advanced.identity_bundle_sha256_raw_v1() != identity_bundle_sha256_raw
        || advanced.batch_episodes_v1() != batch_episodes
        || advanced.checkpoint_segment_updates_v1() != checkpoint_segment_updates
        || advanced.next_update_index() != expected_next_update_index
        || advanced.previous_update_evidence_sha256() != expected_previous_update
        || advanced.progress().batch_episodes() != batch_episodes
        || advanced.progress().checkpoint_segment_updates() != checkpoint_segment_updates
        || advanced.progress().next_episode_index() != episode_end_exclusive
        || advanced.progress().successful_update_count() != generation_index
        || advanced.progress().completed_episode_count() != episode_end_exclusive
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ContinuationBinding,
        ));
    }

    if checkpoint.run_sha256() != run.run_sha256()
        || checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
        || checkpoint.segment_ordinal() != segment_ordinal
        || checkpoint.generation_index() != generation_index
        || checkpoint.batch_episodes() != batch_episodes
        || checkpoint.checkpoint_segment_updates() != checkpoint_segment_updates
        || checkpoint.progress() != advanced.progress()
        || checkpoint.train_state().adam_step() != generation_index
        || checkpoint.train_state().scorer_bias_anchor_f32_bits()
            != u64::from(advanced.scorer_bias_anchor_bits_v1())
        || checkpoint.model_parameter_sha256() != advanced.model_parameter_sha256()
        || checkpoint.train_state_sha256() != advanced.train_state_sha256()
        || sha256_v1(checkpoint.canonical_bytes()) != checkpoint.checkpoint_manifest_sha256()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::FinalCheckpointBinding,
        ));
    }

    let ordered_update_evidence_list_sha256 = ordered_update_evidence_list_sha256_v2(
        run.run_sha256(),
        generation_index,
        checkpoint_segment_updates,
        &ordered_rows,
    )?;
    Ok(DerivedTrainedManifestV2 {
        wire: SegmentManifestWireV2 {
            schema: SEGMENT_MANIFEST_SCHEMA_V2.to_owned(),
            kind: "trained".to_owned(),
            run_sha256: run.run_sha256().to_owned(),
            identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
            segment_ordinal,
            parent_generation_index: Some(parent_generation_index),
            generation_index,
            batch_episodes,
            checkpoint_segment_updates,
            update_start_index,
            update_count: checkpoint_segment_updates,
            episode_start,
            episode_count,
            episode_end_exclusive,
            parent_head_sha256: Some(lower_hex_raw32_v1(parent.head_sha256())),
            parent_last_update_evidence_sha256: parent_facts
                .last_update_evidence_sha256
                .map(lower_hex_raw32_v1),
            ordered_update_evidence_count: checkpoint_segment_updates,
            ordered_update_evidence: ordered_rows,
            ordered_update_evidence_list_sha256: lower_hex_raw32_v1(
                ordered_update_evidence_list_sha256,
            ),
            continuation_chain: ContinuationChainWireV2 {
                continuation_count,
                update_group_count: checkpoint_segment_updates,
                logical_row_count: total_logical_rows,
                first_continuation_sha256,
                last_continuation_sha256,
                continuations: descriptors,
            },
            final_checkpoint: FinalCheckpointBindingWireV2 {
                checkpoint_manifest_sha256: lower_hex_raw32_v1(
                    checkpoint.checkpoint_manifest_sha256(),
                ),
                checkpoint_payload_sha256: lower_hex_raw32_v1(
                    checkpoint.checkpoint_payload_sha256(),
                ),
                logical_state_sha256: lower_hex_raw32_v1(checkpoint.logical_state_sha256()),
                model_parameter_sha256: lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
                train_state_sha256: lower_hex_raw32_v1(checkpoint.train_state_sha256()),
            },
        },
        ordered_update_evidence_list_sha256,
    })
}

fn validate_trained_run_and_equations_v2(
    wire: &SegmentManifestWireV2,
    run: &ValidatedTrainRunV2,
    expected: &SegmentManifestWireV2,
) -> Result<()> {
    if wire.schema != run.record().artifact_schemas.segment {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidSchema,
        ));
    }
    if wire.run_sha256 != run.run_sha256()
        || wire.identity_bundle_sha256 != run.identity_bundle_sha256()
        || wire.batch_episodes != run.batch_episodes()
        || wire.checkpoint_segment_updates != run.checkpoint_segment_updates()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::RunBinding,
        ));
    }
    if wire.kind != "trained"
        || wire.segment_ordinal != expected.segment_ordinal
        || wire.generation_index != expected.generation_index
        || wire.update_start_index != expected.update_start_index
        || wire.update_count != expected.update_count
        || wire.episode_start != expected.episode_start
        || wire.episode_count != expected.episode_count
        || wire.episode_end_exclusive != expected.episode_end_exclusive
        || wire.ordered_update_evidence_count != expected.ordered_update_evidence_count
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::TrainedInvariant,
        ));
    }
    Ok(())
}

fn validate_trained_parent_fields_v2(
    wire: &SegmentManifestWireV2,
    expected: &SegmentManifestWireV2,
) -> Result<()> {
    if wire.parent_generation_index != expected.parent_generation_index
        || wire.parent_head_sha256 != expected.parent_head_sha256
        || wire.parent_last_update_evidence_sha256 != expected.parent_last_update_evidence_sha256
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::ParentBoundaryBinding,
        ));
    }
    Ok(())
}

fn continuation_relative_name_v2(generation_index: u64, continuation_index: u64) -> Result<String> {
    if generation_index > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2
        || continuation_index > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidScalar,
        ));
    }
    Ok(format!(
        "segment-{generation_index:08}.continuation-{continuation_index:08}.json"
    ))
}

fn validate_common_wire_v2(wire: &SegmentManifestWireV2) -> Result<()> {
    if wire.schema != SEGMENT_MANIFEST_SCHEMA_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidSchema,
        ));
    }
    match wire.kind.as_str() {
        "genesis" => {}
        "trained" => {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::TrainedAuthorityRequired,
            ));
        }
        _ => {
            return Err(SegmentManifestV2Error::new(
                SegmentManifestV2ErrorKind::InvalidKind,
            ));
        }
    }
    validate_scalars_v2(wire)?;
    validate_digest_encodings_v2(wire)?;
    let ordered_count = u64::try_from(wire.ordered_update_evidence.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    let continuation_count = u64::try_from(wire.continuation_chain.continuations.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?;
    if wire.ordered_update_evidence_count != wire.update_count
        || wire.ordered_update_evidence_count != ordered_count
        || wire.continuation_chain.continuation_count != continuation_count
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::GenesisInvariant,
        ));
    }
    Ok(())
}

fn validate_scalars_v2(wire: &SegmentManifestWireV2) -> Result<()> {
    for value in [
        wire.segment_ordinal,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
        wire.update_start_index,
        wire.update_count,
        wire.episode_start,
        wire.episode_count,
        wire.episode_end_exclusive,
        wire.ordered_update_evidence_count,
        wire.continuation_chain.continuation_count,
        wire.continuation_chain.update_group_count,
        wire.continuation_chain.logical_row_count,
    ] {
        require_u63_v2(value)?;
    }
    if let Some(value) = wire.parent_generation_index {
        require_u63_v2(value)?;
    }
    for row in &wire.ordered_update_evidence {
        require_u63_v2(row.update_index)?;
    }
    for descriptor in &wire.continuation_chain.continuations {
        for value in [
            descriptor.continuation_index,
            descriptor.update_group_start_ordinal,
        ] {
            require_u63_v2(value)?;
        }
        for value in [
            descriptor.byte_count,
            descriptor.update_group_count,
            descriptor.logical_row_count,
        ] {
            require_positive_u63_v2(value)?;
        }
    }
    Ok(())
}

fn validate_digest_encodings_v2(wire: &SegmentManifestWireV2) -> Result<()> {
    parse_digest_v2(&wire.run_sha256)?;
    parse_digest_v2(&wire.identity_bundle_sha256)?;
    parse_digest_v2(&wire.ordered_update_evidence_list_sha256)?;
    if let Some(value) = wire.parent_head_sha256.as_deref() {
        parse_digest_v2(value)?;
    }
    if let Some(value) = wire.parent_last_update_evidence_sha256.as_deref() {
        parse_digest_v2(value)?;
    }
    for row in &wire.ordered_update_evidence {
        parse_digest_v2(&row.update_evidence_sha256)?;
    }
    if let Some(value) = wire.continuation_chain.first_continuation_sha256.as_deref() {
        parse_digest_v2(value)?;
    }
    if let Some(value) = wire.continuation_chain.last_continuation_sha256.as_deref() {
        parse_digest_v2(value)?;
    }
    for descriptor in &wire.continuation_chain.continuations {
        parse_digest_v2(&descriptor.sha256)?;
        if let Some(value) = descriptor.previous_continuation_sha256.as_deref() {
            parse_digest_v2(value)?;
        }
    }
    parse_digest_v2(&wire.final_checkpoint.checkpoint_manifest_sha256)?;
    parse_digest_v2(&wire.final_checkpoint.checkpoint_payload_sha256)?;
    parse_digest_v2(&wire.final_checkpoint.logical_state_sha256)?;
    parse_digest_v2(&wire.final_checkpoint.model_parameter_sha256)?;
    parse_digest_v2(&wire.final_checkpoint.train_state_sha256)?;
    Ok(())
}

fn validate_genesis_wire_v2(wire: &SegmentManifestWireV2, run: &ValidatedTrainRunV2) -> Result<()> {
    if wire.schema != run.record().artifact_schemas.segment {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidSchema,
        ));
    }
    if wire.run_sha256 != run.run_sha256()
        || wire.identity_bundle_sha256 != run.identity_bundle_sha256()
        || wire.batch_episodes != run.batch_episodes()
        || wire.checkpoint_segment_updates != run.checkpoint_segment_updates()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::RunBinding,
        ));
    }
    if wire.segment_ordinal != 0
        || wire.parent_generation_index.is_some()
        || wire.generation_index != 0
        || wire.update_start_index != 0
        || wire.update_count != 0
        || wire.episode_start != 0
        || wire.episode_count != 0
        || wire.episode_end_exclusive != 0
        || wire.parent_head_sha256.is_some()
        || wire.parent_last_update_evidence_sha256.is_some()
        || wire.ordered_update_evidence_count != 0
        || !wire.ordered_update_evidence.is_empty()
        || wire.continuation_chain.continuation_count != 0
        || wire.continuation_chain.update_group_count != 0
        || wire.continuation_chain.logical_row_count != 0
        || wire.continuation_chain.first_continuation_sha256.is_some()
        || wire.continuation_chain.last_continuation_sha256.is_some()
        || !wire.continuation_chain.continuations.is_empty()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::GenesisInvariant,
        ));
    }
    Ok(())
}

fn validate_genesis_checkpoint_v2(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<()> {
    let progress = checkpoint.progress();
    let outcomes = progress.outcomes_by_learner_seat();
    let p0 = outcomes.p0();
    let p1 = outcomes.p1();
    let policy_steps = progress.learner_policy_steps_by_seat();
    let physical_decisions = progress.learner_physical_decisions_by_seat();
    if checkpoint.run_sha256() != run.run_sha256()
        || checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
        || checkpoint.segment_ordinal() != 0
        || checkpoint.generation_index() != 0
        || checkpoint.batch_episodes() != run.batch_episodes()
        || checkpoint.checkpoint_segment_updates() != run.checkpoint_segment_updates()
        || progress.batch_episodes() != run.batch_episodes()
        || progress.checkpoint_segment_updates() != run.checkpoint_segment_updates()
        || progress.next_episode_index() != 0
        || progress.successful_update_count() != 0
        || progress.completed_episode_count() != 0
        || p0.win() != 0
        || p0.loss() != 0
        || p0.draw() != 0
        || p1.win() != 0
        || p1.loss() != 0
        || p1.draw() != 0
        || policy_steps.p0() != 0
        || policy_steps.p1() != 0
        || physical_decisions.p0() != 0
        || physical_decisions.p1() != 0
        || checkpoint.train_state().adam_step() != 0
        || sha256_v1(checkpoint.canonical_bytes()) != checkpoint.checkpoint_manifest_sha256()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::FinalCheckpointBinding,
        ));
    }
    Ok(())
}

fn validate_final_checkpoint_binding_v2(
    binding: &FinalCheckpointBindingWireV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<()> {
    if parse_digest_v2(&binding.checkpoint_manifest_sha256)?
        != checkpoint.checkpoint_manifest_sha256()
        || parse_digest_v2(&binding.checkpoint_payload_sha256)?
            != checkpoint.checkpoint_payload_sha256()
        || parse_digest_v2(&binding.logical_state_sha256)? != checkpoint.logical_state_sha256()
        || parse_digest_v2(&binding.model_parameter_sha256)? != checkpoint.model_parameter_sha256()
        || parse_digest_v2(&binding.train_state_sha256)? != checkpoint.train_state_sha256()
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::FinalCheckpointBinding,
        ));
    }
    Ok(())
}

fn ordered_update_evidence_list_sha256_v2(
    run_sha256: &str,
    generation_index: u64,
    update_count: u64,
    rows: &[OrderedUpdateEvidenceRowWireV1],
) -> Result<[u8; 32]> {
    require_u63_v2(generation_index)?;
    require_u63_v2(update_count)?;
    let run_sha256 = parse_digest_v2(run_sha256)?;
    let mut digest = NativeTrainingStoreAtomSha256V1::new();
    digest
        .atom(
            "domain",
            ORDERED_UPDATE_EVIDENCE_LIST_DIGEST_IDENTITY_V1.as_bytes(),
        )
        .map_err(map_digest_error_v2)?;
    digest
        .atom("run_sha256", &run_sha256)
        .map_err(map_digest_error_v2)?;
    digest
        .atom("generation_index_u64be", &generation_index.to_be_bytes())
        .map_err(map_digest_error_v2)?;
    digest
        .atom("update_count_u64be", &update_count.to_be_bytes())
        .map_err(map_digest_error_v2)?;
    for row in rows {
        require_u63_v2(row.update_index)?;
        let evidence_sha256 = parse_digest_v2(&row.update_evidence_sha256)?;
        digest
            .atom("update_index_u64be", &row.update_index.to_be_bytes())
            .map_err(map_digest_error_v2)?;
        digest
            .atom("update_evidence_sha256", &evidence_sha256)
            .map_err(map_digest_error_v2)?;
    }
    Ok(digest.finalize())
}

fn parse_digest_v2(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value).map_err(map_digest_error_v2)
}

fn map_digest_error_v2(error: NativeTrainingStoreDigestErrorV1) -> SegmentManifestV2Error {
    let kind = match error {
        NativeTrainingStoreDigestErrorV1::InvalidRaw32 => SegmentManifestV2ErrorKind::InvalidDigest,
        NativeTrainingStoreDigestErrorV1::AtomTagLength
        | NativeTrainingStoreDigestErrorV1::AtomPayloadLength => {
            SegmentManifestV2ErrorKind::InvalidArithmetic
        }
    };
    SegmentManifestV2Error::new(kind)
}

fn require_u63_v2(value: u64) -> Result<()> {
    if value > U63_MAX_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidScalar,
        ));
    }
    Ok(())
}

fn require_positive_u63_v2(value: u64) -> Result<()> {
    if value == 0 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidScalar,
        ));
    }
    require_u63_v2(value)
}

fn checked_u63_add_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(|| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))
}

fn checked_u63_mul_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(|| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_continuation_v2::{
        build_segment_continuations_v2, build_segment_continuations_with_test_limits_v2,
        ValidatedSegmentContinuationChainAdvanceV2, SEGMENT_CONTINUATION_MAX_BYTES_V2,
    };
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1, decode_update_group_v1,
        UpdateEvidenceChainContextV1, ValidatedUpdateGroupV1,
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use std::time::Duration;

    struct FixtureV2 {
        run: ValidatedTrainRunV2,
        checkpoint: CheckpointManifestV3,
        segment: SegmentManifestV2,
    }

    struct TrainedFixtureV2 {
        run: ValidatedTrainRunV2,
        genesis_checkpoint: CheckpointManifestV3,
        parent: ValidatedNativeTrainingBoundaryV2,
        group_bytes: Vec<Vec<u8>>,
        continuations: ValidatedSegmentContinuationChainAdvanceV2,
        checkpoint: CheckpointManifestV3,
        wrong_continuations: ValidatedSegmentContinuationChainAdvanceV2,
        wrong_checkpoint: CheckpointManifestV3,
        manifest: SegmentManifestV2,
    }

    static FIXTURE_V2: OnceLock<FixtureV2> = OnceLock::new();
    static TRAINED_FIXTURE_V2: OnceLock<TrainedFixtureV2> = OnceLock::new();

    fn execution_config_v2(run: &ValidatedTrainRunV2) -> NativeTrainingExecutionConfigV1 {
        NativeTrainingExecutionConfigV1 {
            run_base_seed: run.record().schedule.base_seed,
            batch_episodes: run.batch_episodes(),
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            max_physical_decisions: run.record().limits.max_physical_decisions,
            max_policy_steps: run.record().limits.max_policy_steps,
            worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
            sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker)
                .unwrap(),
            broker_batch_target: usize::try_from(run.record().topology.broker_batch_target)
                .unwrap(),
            scheduler_timeout: Duration::from_secs(30),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5_f32.to_bits(),
            learning_rate_bits: 0.001_f32.to_bits(),
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    fn fresh_executor_v2(run: &ValidatedTrainRunV2) -> NativeTrainingExecutorV1 {
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v2(run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap()
    }

    fn fixture_v2() -> &'static FixtureV2 {
        FIXTURE_V2.get_or_init(|| {
            let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
            let executor = fresh_executor_v2(&run);
            let candidate = executor.checkpoint_candidate_v1().unwrap();
            let checkpoint =
                build_genesis_checkpoint_manifest_v3(&run, candidate.payload()).unwrap();
            let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
            FixtureV2 {
                run,
                checkpoint,
                segment,
            }
        })
    }

    fn context_from_group_bytes_v2(
        run: &ValidatedTrainRunV2,
        genesis: &CheckpointManifestV3,
        group_bytes: &[Vec<u8>],
        update_count: usize,
    ) -> UpdateEvidenceChainContextV1 {
        let mut context = begin_update_evidence_chain_v1(run, genesis).unwrap();
        for bytes in group_bytes.iter().take(update_count) {
            context = decode_update_group_v1(run, context, bytes)
                .unwrap()
                .into_parts()
                .1;
        }
        context
    }

    fn groups_from_bytes_v2(
        run: &ValidatedTrainRunV2,
        genesis: &CheckpointManifestV3,
        group_bytes: &[Vec<u8>],
        start: usize,
        count: usize,
    ) -> Vec<ValidatedUpdateGroupV1> {
        let mut context = context_from_group_bytes_v2(run, genesis, group_bytes, start);
        let mut groups = Vec::with_capacity(count);
        for bytes in group_bytes.iter().skip(start).take(count) {
            let advance = decode_update_group_v1(run, context, bytes).unwrap();
            let (group, advanced) = advance.into_parts();
            groups.push(group);
            context = advanced;
        }
        groups
    }

    fn trained_fixture_v2() -> &'static TrainedFixtureV2 {
        TRAINED_FIXTURE_V2.get_or_init(|| {
            let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
            assert_eq!(run.batch_episodes(), 2);
            assert_eq!(run.checkpoint_segment_updates(), 4);
            assert!(run.requested_successful_updates() >= 8);
            let mut executor = fresh_executor_v2(&run);
            let genesis_payload = executor
                .checkpoint_candidate_v1()
                .unwrap()
                .payload()
                .to_vec();
            let genesis_checkpoint =
                build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
            let genesis_segment =
                build_genesis_segment_manifest_v2(&run, &genesis_checkpoint).unwrap();
            let parent = build_genesis_native_training_boundary_v2(
                &run,
                &genesis_segment,
                &genesis_checkpoint,
            )
            .unwrap();

            let mut context = begin_update_evidence_chain_v1(&run, &genesis_checkpoint).unwrap();
            let mut group_bytes = Vec::new();
            let mut boundary_candidates = Vec::new();
            for update_ordinal in 0..8 {
                let prepared = executor.prepare_update_v2().unwrap();
                let advance = build_update_group_v1(&run, context, &prepared).unwrap();
                if matches!(update_ordinal, 3 | 7) {
                    boundary_candidates.push(prepared.checkpoint_candidate().clone());
                }
                let (group, advanced) = advance.into_parts();
                group_bytes.push(group.canonical_bytes().to_vec());
                context = advanced;
                drop(prepared);
                if update_ordinal + 1 < 8 {
                    executor.run_update_v2().unwrap();
                }
            }

            let segment_updates = usize::try_from(run.checkpoint_segment_updates()).unwrap();
            let first_parent =
                context_from_group_bytes_v2(&run, &genesis_checkpoint, &group_bytes, 0);
            let first_groups =
                groups_from_bytes_v2(&run, &genesis_checkpoint, &group_bytes, 0, segment_updates);
            let continuations =
                build_segment_continuations_v2(&run, first_parent, first_groups).unwrap();
            let checkpoint = build_trained_checkpoint_manifest_v3(
                &run,
                continuations.advanced_context(),
                &boundary_candidates[0],
            )
            .unwrap();
            let manifest =
                build_trained_segment_manifest_v2(&run, &parent, &continuations, &checkpoint)
                    .unwrap();

            let second_parent = context_from_group_bytes_v2(
                &run,
                &genesis_checkpoint,
                &group_bytes,
                segment_updates,
            );
            let second_groups = groups_from_bytes_v2(
                &run,
                &genesis_checkpoint,
                &group_bytes,
                segment_updates,
                segment_updates,
            );
            let wrong_continuations =
                build_segment_continuations_v2(&run, second_parent, second_groups).unwrap();
            let wrong_checkpoint = build_trained_checkpoint_manifest_v3(
                &run,
                wrong_continuations.advanced_context(),
                &boundary_candidates[1],
            )
            .unwrap();

            TrainedFixtureV2 {
                run,
                genesis_checkpoint,
                parent,
                group_bytes,
                continuations,
                checkpoint,
                wrong_continuations,
                wrong_checkpoint,
                manifest,
            }
        })
    }

    fn segment_value_v2() -> Value {
        serde_json::from_slice(
            fixture_v2()
                .segment
                .canonical_bytes()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap()
    }

    fn canonical_value_bytes_v2(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, SEGMENT_MANIFEST_NULL_POLICY_V2).unwrap()
    }

    fn decode_value_error_v2(value: &Value) -> SegmentManifestV2ErrorKind {
        let fixture = fixture_v2();
        decode_genesis_segment_manifest_v2(
            &canonical_value_bytes_v2(value),
            &fixture.run,
            &fixture.checkpoint,
        )
        .unwrap_err()
        .kind()
    }

    fn trained_value_v2() -> Value {
        serde_json::from_slice(
            trained_fixture_v2()
                .manifest
                .canonical_bytes()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap()
    }

    fn decode_trained_value_error_v2(value: &Value) -> SegmentManifestV2ErrorKind {
        let fixture = trained_fixture_v2();
        decode_trained_segment_manifest_v2(
            &canonical_value_bytes_v2(value),
            &fixture.run,
            &fixture.parent,
            &fixture.continuations,
            &fixture.checkpoint,
        )
        .unwrap_err()
        .kind()
    }

    fn refresh_trained_ordered_digest_v2(value: &mut Value) {
        let wire: SegmentManifestWireV2 = serde_json::from_value(value.clone()).unwrap();
        let digest = ordered_update_evidence_list_sha256_v2(
            &wire.run_sha256,
            wire.generation_index,
            wire.update_count,
            &wire.ordered_update_evidence,
        )
        .unwrap();
        value["ordered_update_evidence_list_sha256"] = json!(lower_hex_raw32_v1(digest));
    }

    fn independent_ordered_digest_v2(
        run_sha256: &str,
        generation_index: u64,
        rows: &[(u64, [u8; 32])],
    ) -> [u8; 32] {
        fn atom(reference: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            reference.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(tag.as_bytes());
            reference.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(payload);
        }

        let mut reference = Vec::new();
        atom(
            &mut reference,
            "domain",
            ORDERED_UPDATE_EVIDENCE_LIST_DIGEST_IDENTITY_V1.as_bytes(),
        );
        atom(
            &mut reference,
            "run_sha256",
            &parse_lower_hex_raw32_v1(run_sha256).unwrap(),
        );
        atom(
            &mut reference,
            "generation_index_u64be",
            &generation_index.to_be_bytes(),
        );
        atom(
            &mut reference,
            "update_count_u64be",
            &u64::try_from(rows.len()).unwrap().to_be_bytes(),
        );
        for (update_index, evidence_sha256) in rows {
            atom(
                &mut reference,
                "update_index_u64be",
                &update_index.to_be_bytes(),
            );
            atom(&mut reference, "update_evidence_sha256", evidence_sha256);
        }
        Sha256::digest(reference).into()
    }

    #[test]
    fn genuine_executor_genesis_roundtrips_with_exact_wire_and_hash() {
        let fixture = fixture_v2();
        let authority = decode_genesis_segment_manifest_v2(
            fixture.segment.canonical_bytes(),
            &fixture.run,
            &fixture.checkpoint,
        )
        .unwrap();
        let facts = authority.boundary_facts_v2();
        assert_eq!(
            authority.canonical_bytes(),
            fixture.segment.canonical_bytes()
        );
        assert_eq!(facts.kind, "genesis");
        assert_eq!(facts.segment_ordinal, 0);
        assert_eq!(facts.parent_generation_index, None);
        assert_eq!(facts.generation_index, 0);
        assert_eq!(facts.batch_episodes, 2);
        assert_eq!(facts.checkpoint_segment_updates, 4);
        assert_eq!(authority.wire.update_count, 0);
        assert_eq!(facts.run_sha256, fixture.run.run_sha256());
        assert_eq!(
            facts.identity_bundle_sha256,
            fixture.run.identity_bundle_sha256()
        );
        assert_eq!(facts.parent_head_sha256, None);
        assert_eq!(facts.parent_last_update_evidence_sha256, None);
        assert_eq!(facts.last_update_evidence_sha256, None);
        assert_eq!(
            SEGMENT_MANIFEST_RECORD_CONTRACT_SHA256_V2,
            "53d5e4f8585e28e95870c54407e7a8a6ce6e292d9d85a30ba53197c04cd0ee0d"
        );

        let expected = format!(
            concat!(
                "{{\"batch_episodes\":2,\"checkpoint_segment_updates\":4,",
                "\"continuation_chain\":{{\"continuation_count\":0,",
                "\"continuations\":[],\"first_continuation_sha256\":null,",
                "\"last_continuation_sha256\":null,\"logical_row_count\":0,",
                "\"update_group_count\":0}},\"episode_count\":0,",
                "\"episode_end_exclusive\":0,\"episode_start\":0,",
                "\"final_checkpoint\":{{\"checkpoint_manifest_sha256\":\"{}\",",
                "\"checkpoint_payload_sha256\":\"{}\",",
                "\"logical_state_sha256\":\"{}\",",
                "\"model_parameter_sha256\":\"{}\",",
                "\"train_state_sha256\":\"{}\"}},\"generation_index\":0,",
                "\"identity_bundle_sha256\":\"{}\",\"kind\":\"genesis\",",
                "\"ordered_update_evidence\":[],",
                "\"ordered_update_evidence_count\":0,",
                "\"ordered_update_evidence_list_sha256\":\"{}\",",
                "\"parent_generation_index\":null,\"parent_head_sha256\":null,",
                "\"parent_last_update_evidence_sha256\":null,",
                "\"run_sha256\":\"{}\",\"schema\":",
                "\"mtg_kernel_native_train_checkpoint_segment/v2\",",
                "\"segment_ordinal\":0,\"update_count\":0,",
                "\"update_start_index\":0}}\n"
            ),
            lower_hex_raw32_v1(fixture.checkpoint.checkpoint_manifest_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.checkpoint_payload_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.logical_state_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.model_parameter_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.train_state_sha256()),
            fixture.run.identity_bundle_sha256(),
            lower_hex_raw32_v1(facts.ordered_update_evidence_list_sha256),
            fixture.run.run_sha256(),
        );
        assert_eq!(authority.canonical_bytes(), expected.as_bytes());
        let independently_hashed: [u8; 32] = Sha256::digest(expected.as_bytes()).into();
        assert_eq!(authority.segment_manifest_sha256(), independently_hashed);
        assert!(
            u64::try_from(authority.canonical_bytes().len()).unwrap()
                <= SEGMENT_MANIFEST_MAX_BYTES_V2
        );

        assert_eq!(
            facts.checkpoint_manifest_sha256,
            fixture.checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(
            facts.checkpoint_payload_sha256,
            fixture.checkpoint.checkpoint_payload_sha256()
        );
        assert_eq!(
            facts.logical_state_sha256,
            fixture.checkpoint.logical_state_sha256()
        );
        assert_eq!(
            facts.model_parameter_sha256,
            fixture.checkpoint.model_parameter_sha256()
        );
        assert_eq!(
            facts.train_state_sha256,
            fixture.checkpoint.train_state_sha256()
        );
        assert_eq!(
            facts.segment_manifest_sha256,
            authority.segment_manifest_sha256()
        );

        let value = segment_value_v2();
        assert_eq!(value.as_object().unwrap().len(), 21);
        assert_eq!(value["continuation_chain"].as_object().unwrap().len(), 6);
        assert_eq!(value["final_checkpoint"].as_object().unwrap().len(), 5);
        assert_eq!(
            serde_json::to_value(OrderedUpdateEvidenceRowWireV1 {
                update_index: 1,
                update_evidence_sha256: "00".repeat(32),
            })
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            serde_json::to_value(ContinuationDescriptorWireV2 {
                continuation_index: 0,
                relative_name: "segment-00000004.continuation-00000000.json".to_owned(),
                byte_count: 1,
                sha256: "11".repeat(32),
                previous_continuation_sha256: None,
                update_group_start_ordinal: 0,
                update_group_count: 1,
                logical_row_count: 1,
            })
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
            8
        );
    }

    #[test]
    fn genesis_ordered_evidence_digest_matches_independent_atom_reference() {
        fn atom(reference: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            reference.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(tag.as_bytes());
            reference.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(payload);
        }

        let fixture = fixture_v2();
        let mut reference = Vec::new();
        atom(
            &mut reference,
            "domain",
            ORDERED_UPDATE_EVIDENCE_LIST_DIGEST_IDENTITY_V1.as_bytes(),
        );
        atom(
            &mut reference,
            "run_sha256",
            &parse_lower_hex_raw32_v1(fixture.run.run_sha256()).unwrap(),
        );
        atom(
            &mut reference,
            "generation_index_u64be",
            &0_u64.to_be_bytes(),
        );
        atom(&mut reference, "update_count_u64be", &0_u64.to_be_bytes());
        let expected: [u8; 32] = Sha256::digest(&reference).into();
        assert_eq!(
            fixture
                .segment
                .boundary_facts_v2()
                .ordered_update_evidence_list_sha256,
            expected
        );
    }

    #[test]
    fn genuine_k2_s4_trained_manifest_roundtrips_exact_authority() {
        let fixture = trained_fixture_v2();
        let authority = decode_trained_segment_manifest_v2(
            fixture.manifest.canonical_bytes(),
            &fixture.run,
            &fixture.parent,
            &fixture.continuations,
            &fixture.checkpoint,
        )
        .unwrap();
        let rebuilt = build_trained_segment_manifest_v2(
            &fixture.run,
            &fixture.parent,
            &fixture.continuations,
            &fixture.checkpoint,
        )
        .unwrap();
        let facts = authority.boundary_facts_v2();
        let chain = fixture.continuations.chain();

        assert_eq!(
            authority.canonical_bytes(),
            fixture.manifest.canonical_bytes()
        );
        assert_eq!(rebuilt.canonical_bytes(), authority.canonical_bytes());
        assert_eq!(facts.kind, "trained");
        assert_eq!(facts.segment_ordinal, 1);
        assert_eq!(facts.parent_generation_index, Some(0));
        assert_eq!(facts.generation_index, 4);
        assert_eq!(facts.batch_episodes, 2);
        assert_eq!(facts.checkpoint_segment_updates, 4);
        assert_eq!(authority.wire.update_start_index, 1);
        assert_eq!(authority.wire.update_count, 4);
        assert_eq!(authority.wire.episode_start, 0);
        assert_eq!(authority.wire.episode_count, 8);
        assert_eq!(authority.wire.episode_end_exclusive, 8);
        assert_eq!(facts.parent_head_sha256, Some(fixture.parent.head_sha256()));
        assert_eq!(facts.parent_last_update_evidence_sha256, None);
        assert_eq!(
            facts.last_update_evidence_sha256,
            chain.ordered_update_evidence().last().map(|row| row.1)
        );
        assert_eq!(
            authority.segment_manifest_sha256(),
            sha256_v1(authority.canonical_bytes())
        );
        assert!(
            u64::try_from(authority.canonical_bytes().len()).unwrap()
                <= SEGMENT_MANIFEST_MAX_BYTES_V2
        );

        let value = trained_value_v2();
        assert_eq!(value.as_object().unwrap().len(), 21);
        assert_eq!(
            value["ordered_update_evidence"].as_array().unwrap().len(),
            4
        );
        assert_eq!(value["continuation_chain"].as_object().unwrap().len(), 6);
        assert_eq!(
            value["continuation_chain"]["continuations"][0]
                .as_object()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(value["final_checkpoint"].as_object().unwrap().len(), 5);

        assert_eq!(
            decode_genesis_segment_manifest_v2(
                authority.canonical_bytes(),
                &fixture.run,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::TrainedAuthorityRequired
        );
        assert_eq!(
            decode_trained_segment_manifest_v2(
                fixture_v2().segment.canonical_bytes(),
                &fixture.run,
                &fixture.parent,
                &fixture.continuations,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::TrainedInvariant
        );
    }

    #[test]
    fn trained_canonical_wire_and_ordered_digest_match_independent_references() {
        let fixture = trained_fixture_v2();
        let chain = fixture.continuations.chain();
        assert_eq!(chain.continuations().len(), 1);
        let continuation = &chain.continuations()[0];
        let rows = chain
            .ordered_update_evidence()
            .iter()
            .map(|(update_index, evidence_sha256)| {
                format!(
                    "{{\"update_evidence_sha256\":\"{}\",\"update_index\":{update_index}}}",
                    lower_hex_raw32_v1(*evidence_sha256)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let independent_digest = independent_ordered_digest_v2(
            fixture.run.run_sha256(),
            chain.generation_index(),
            chain.ordered_update_evidence(),
        );
        let descriptor = format!(
            concat!(
                "{{\"byte_count\":{},\"continuation_index\":0,",
                "\"logical_row_count\":{},\"previous_continuation_sha256\":null,",
                "\"relative_name\":\"segment-00000004.continuation-00000000.json\",",
                "\"sha256\":\"{}\",\"update_group_count\":{},",
                "\"update_group_start_ordinal\":0}}"
            ),
            continuation.canonical_bytes().len(),
            continuation.logical_row_count(),
            lower_hex_raw32_v1(continuation.continuation_sha256()),
            continuation.update_group_count(),
        );
        let continuation_sha256 = lower_hex_raw32_v1(continuation.continuation_sha256());
        let expected = format!(
            concat!(
                "{{\"batch_episodes\":2,\"checkpoint_segment_updates\":4,",
                "\"continuation_chain\":{{\"continuation_count\":1,",
                "\"continuations\":[{descriptor}],",
                "\"first_continuation_sha256\":\"{continuation_sha256}\",",
                "\"last_continuation_sha256\":\"{continuation_sha256}\",",
                "\"logical_row_count\":{logical_row_count},\"update_group_count\":4}},",
                "\"episode_count\":8,\"episode_end_exclusive\":8,",
                "\"episode_start\":0,\"final_checkpoint\":{{",
                "\"checkpoint_manifest_sha256\":\"{checkpoint_manifest}\",",
                "\"checkpoint_payload_sha256\":\"{checkpoint_payload}\",",
                "\"logical_state_sha256\":\"{logical_state}\",",
                "\"model_parameter_sha256\":\"{model_parameter}\",",
                "\"train_state_sha256\":\"{train_state}\"}},",
                "\"generation_index\":4,\"identity_bundle_sha256\":\"{identity}\",",
                "\"kind\":\"trained\",\"ordered_update_evidence\":[{rows}],",
                "\"ordered_update_evidence_count\":4,",
                "\"ordered_update_evidence_list_sha256\":\"{ordered_digest}\",",
                "\"parent_generation_index\":0,",
                "\"parent_head_sha256\":\"{parent_head}\",",
                "\"parent_last_update_evidence_sha256\":null,",
                "\"run_sha256\":\"{run}\",",
                "\"schema\":\"mtg_kernel_native_train_checkpoint_segment/v2\",",
                "\"segment_ordinal\":1,\"update_count\":4,",
                "\"update_start_index\":1}}\n"
            ),
            descriptor = descriptor,
            continuation_sha256 = continuation_sha256,
            logical_row_count = continuation.logical_row_count(),
            checkpoint_manifest =
                lower_hex_raw32_v1(fixture.checkpoint.checkpoint_manifest_sha256()),
            checkpoint_payload = lower_hex_raw32_v1(fixture.checkpoint.checkpoint_payload_sha256()),
            logical_state = lower_hex_raw32_v1(fixture.checkpoint.logical_state_sha256()),
            model_parameter = lower_hex_raw32_v1(fixture.checkpoint.model_parameter_sha256()),
            train_state = lower_hex_raw32_v1(fixture.checkpoint.train_state_sha256()),
            identity = fixture.run.identity_bundle_sha256(),
            rows = rows,
            ordered_digest = lower_hex_raw32_v1(independent_digest),
            parent_head = lower_hex_raw32_v1(fixture.parent.head_sha256()),
            run = fixture.run.run_sha256(),
        );

        assert_eq!(fixture.manifest.canonical_bytes(), expected.as_bytes());
        assert_eq!(
            fixture
                .manifest
                .boundary_facts_v2()
                .ordered_update_evidence_list_sha256,
            independent_digest
        );
        let independently_hashed: [u8; 32] = Sha256::digest(expected.as_bytes()).into();
        assert_eq!(
            fixture.manifest.segment_manifest_sha256(),
            independently_hashed
        );
    }

    #[test]
    fn forced_multi_continuation_manifest_binds_every_descriptor_and_link() {
        let fixture = trained_fixture_v2();
        let parent_context = context_from_group_bytes_v2(
            &fixture.run,
            &fixture.genesis_checkpoint,
            &fixture.group_bytes,
            0,
        );
        let groups = groups_from_bytes_v2(
            &fixture.run,
            &fixture.genesis_checkpoint,
            &fixture.group_bytes,
            0,
            usize::try_from(fixture.run.checkpoint_segment_updates()).unwrap(),
        );
        let max_single_group_rows = groups
            .iter()
            .map(ValidatedUpdateGroupV1::logical_row_count)
            .max()
            .unwrap();
        let continuations = build_segment_continuations_with_test_limits_v2(
            &fixture.run,
            parent_context,
            groups,
            SEGMENT_CONTINUATION_MAX_BYTES_V2,
            max_single_group_rows,
        )
        .unwrap();
        assert!(continuations.chain().continuations().len() > 1);
        let manifest = build_trained_segment_manifest_v2(
            &fixture.run,
            &fixture.parent,
            &continuations,
            &fixture.checkpoint,
        )
        .unwrap();
        let value: Value =
            serde_json::from_slice(manifest.canonical_bytes().strip_suffix(b"\n").unwrap())
                .unwrap();
        let descriptors = value["continuation_chain"]["continuations"]
            .as_array()
            .unwrap();
        let mut cumulative_group_count = 0_u64;
        let mut expected_previous = None;
        let mut total_logical_rows = 0_u64;
        for (position, (descriptor, continuation)) in descriptors
            .iter()
            .zip(continuations.chain().continuations())
            .enumerate()
        {
            assert_eq!(descriptor["continuation_index"], json!(position));
            assert_eq!(
                descriptor["relative_name"],
                json!(format!("segment-00000004.continuation-{position:08}.json"))
            );
            assert_eq!(
                descriptor["byte_count"],
                json!(continuation.canonical_bytes().len())
            );
            assert_eq!(
                descriptor["sha256"],
                json!(lower_hex_raw32_v1(sha256_v1(
                    continuation.canonical_bytes()
                )))
            );
            assert_eq!(
                descriptor["previous_continuation_sha256"],
                expected_previous
                    .map(|digest| json!(lower_hex_raw32_v1(digest)))
                    .unwrap_or(Value::Null)
            );
            assert_eq!(
                descriptor["update_group_start_ordinal"],
                json!(cumulative_group_count)
            );
            assert_eq!(
                descriptor["update_group_count"],
                json!(continuation.update_group_count())
            );
            assert_eq!(
                descriptor["logical_row_count"],
                json!(continuation.logical_row_count())
            );
            cumulative_group_count += u64::try_from(continuation.update_group_count()).unwrap();
            total_logical_rows += continuation.logical_row_count();
            expected_previous = Some(continuation.continuation_sha256());
        }
        assert_eq!(
            descriptors.len(),
            continuations.chain().continuations().len()
        );
        assert_eq!(
            value["continuation_chain"]["continuation_count"],
            json!(descriptors.len())
        );
        assert_eq!(
            value["continuation_chain"]["update_group_count"],
            json!(cumulative_group_count)
        );
        assert_eq!(
            value["continuation_chain"]["logical_row_count"],
            json!(total_logical_rows)
        );
        assert_eq!(
            value["continuation_chain"]["first_continuation_sha256"],
            descriptors[0]["sha256"]
        );
        assert_eq!(
            value["continuation_chain"]["last_continuation_sha256"],
            descriptors.last().unwrap()["sha256"]
        );
        let decoded = decode_trained_segment_manifest_v2(
            manifest.canonical_bytes(),
            &fixture.run,
            &fixture.parent,
            &continuations,
            &fixture.checkpoint,
        )
        .unwrap();
        assert_eq!(decoded.canonical_bytes(), manifest.canonical_bytes());

        let multi_error = |mutated: &Value| {
            decode_trained_segment_manifest_v2(
                &canonical_value_bytes_v2(mutated),
                &fixture.run,
                &fixture.parent,
                &continuations,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind()
        };
        let mut broken_link = value.clone();
        broken_link["continuation_chain"]["continuations"][1]["previous_continuation_sha256"] =
            json!("ff".repeat(32));
        assert_eq!(
            multi_error(&broken_link),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );
        let mut reordered = value.clone();
        reordered["continuation_chain"]["continuations"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert_eq!(
            multi_error(&reordered),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );
        let mut duplicated = value;
        let first_descriptor = duplicated["continuation_chain"]["continuations"][0].clone();
        duplicated["continuation_chain"]["continuations"][1] = first_descriptor;
        assert_eq!(
            multi_error(&duplicated),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );
    }

    #[test]
    fn trained_root_parent_options_and_checked_arithmetic_fail_closed() {
        let fixture = trained_fixture_v2();
        assert_ne!(
            fixture.parent.head_sha256(),
            fixture.parent.head_record_sha256()
        );
        let mut role_swap = trained_value_v2();
        role_swap["parent_head_sha256"] =
            json!(lower_hex_raw32_v1(fixture.parent.head_record_sha256()));
        assert_eq!(
            decode_trained_value_error_v2(&role_swap),
            SegmentManifestV2ErrorKind::ParentBoundaryBinding
        );

        for (field, replacement) in [
            ("segment_ordinal", json!(2)),
            ("generation_index", json!(3)),
            ("update_start_index", json!(0)),
            ("update_count", json!(3)),
            ("episode_start", json!(1)),
            ("episode_count", json!(7)),
            ("episode_end_exclusive", json!(7)),
            ("ordered_update_evidence_count", json!(3)),
        ] {
            let mut value = trained_value_v2();
            value[field] = replacement;
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::TrainedInvariant,
                "field {field}"
            );
        }

        for (field, replacement, expected) in [
            (
                "schema",
                json!("mtg_kernel_native_train_checkpoint_segment/v1"),
                SegmentManifestV2ErrorKind::InvalidSchema,
            ),
            (
                "kind",
                json!("TRAINED"),
                SegmentManifestV2ErrorKind::InvalidKind,
            ),
            (
                "run_sha256",
                json!("ff".repeat(32)),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "identity_bundle_sha256",
                json!("ff".repeat(32)),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "batch_episodes",
                json!(4),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "checkpoint_segment_updates",
                json!(2),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
        ] {
            let mut value = trained_value_v2();
            value[field] = replacement;
            assert_eq!(
                decode_trained_value_error_v2(&value),
                expected,
                "field {field}"
            );
        }

        for (field, replacement) in [
            ("parent_generation_index", Value::Null),
            ("parent_head_sha256", Value::Null),
            ("parent_last_update_evidence_sha256", json!("ff".repeat(32))),
        ] {
            let mut value = trained_value_v2();
            value[field] = replacement;
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::ParentBoundaryBinding,
                "field {field}"
            );
        }

        for field in ["first_continuation_sha256", "last_continuation_sha256"] {
            let mut value = trained_value_v2();
            value["continuation_chain"][field] = Value::Null;
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::ContinuationBinding,
                "field {field}"
            );
        }

        let mut over_u63 = trained_value_v2();
        over_u63["generation_index"] = json!(U63_MAX_V2 + 1);
        assert_eq!(
            decode_trained_value_error_v2(&over_u63),
            SegmentManifestV2ErrorKind::InvalidScalar
        );
        assert_eq!(
            checked_u63_add_v2(U63_MAX_V2, 1).unwrap_err().kind(),
            SegmentManifestV2ErrorKind::InvalidArithmetic
        );
        assert_eq!(
            checked_u63_mul_v2(U63_MAX_V2, 2).unwrap_err().kind(),
            SegmentManifestV2ErrorKind::InvalidArithmetic
        );
        assert_eq!(
            continuation_relative_name_v2(SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2 + 1, 0,)
                .unwrap_err()
                .kind(),
            SegmentManifestV2ErrorKind::InvalidScalar
        );

        let canonical = String::from_utf8(fixture.manifest.canonical_bytes().to_vec()).unwrap();
        let noncanonical = canonical.replacen(":", ": ", 1);
        assert_eq!(
            decode_trained_segment_manifest_v2(
                noncanonical.as_bytes(),
                &fixture.run,
                &fixture.parent,
                &fixture.continuations,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::NonCanonicalBytes)
        );
        let oversized = vec![b' '; usize::try_from(SEGMENT_MANIFEST_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            decode_trained_segment_manifest_v2(
                &oversized,
                &fixture.run,
                &fixture.parent,
                &fixture.continuations,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::RecordTooLarge
        );
    }

    #[test]
    fn trained_ordered_rows_are_exact_even_after_digest_recomputation() {
        let mut stale_digest = trained_value_v2();
        stale_digest["ordered_update_evidence"][0]["update_evidence_sha256"] =
            json!("ff".repeat(32));
        assert_eq!(
            decode_trained_value_error_v2(&stale_digest),
            SegmentManifestV2ErrorKind::OrderedEvidenceDigestMismatch
        );

        let mut shortened = trained_value_v2();
        shortened["ordered_update_evidence"]
            .as_array_mut()
            .unwrap()
            .pop();
        shortened["update_count"] = json!(3);
        shortened["ordered_update_evidence_count"] = json!(3);
        refresh_trained_ordered_digest_v2(&mut shortened);
        assert_eq!(
            decode_trained_value_error_v2(&shortened),
            SegmentManifestV2ErrorKind::TrainedInvariant
        );

        let mut reordered = trained_value_v2();
        reordered["ordered_update_evidence"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        refresh_trained_ordered_digest_v2(&mut reordered);
        assert_eq!(
            decode_trained_value_error_v2(&reordered),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );

        let mut duplicated = trained_value_v2();
        let first = duplicated["ordered_update_evidence"][0].clone();
        duplicated["ordered_update_evidence"][1] = first;
        refresh_trained_ordered_digest_v2(&mut duplicated);
        assert_eq!(
            decode_trained_value_error_v2(&duplicated),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );

        for (field, replacement) in [
            ("update_index", json!(2)),
            ("update_evidence_sha256", json!("ff".repeat(32))),
        ] {
            let mut value = trained_value_v2();
            value["ordered_update_evidence"][0][field] = replacement;
            refresh_trained_ordered_digest_v2(&mut value);
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::ContinuationBinding,
                "field {field}"
            );
        }
    }

    #[test]
    fn trained_descriptors_links_and_aggregate_counts_are_exact() {
        let baseline = trained_value_v2();
        let descriptor = &baseline["continuation_chain"]["continuations"][0];
        let byte_count = descriptor["byte_count"].as_u64().unwrap();
        let update_group_count = descriptor["update_group_count"].as_u64().unwrap();
        let logical_row_count = descriptor["logical_row_count"].as_u64().unwrap();
        for (field, replacement) in [
            ("continuation_index", json!(1)),
            (
                "relative_name",
                json!("segment-00000004.continuation-00000001.json"),
            ),
            ("byte_count", json!(byte_count + 1)),
            ("sha256", json!("ff".repeat(32))),
            ("previous_continuation_sha256", json!("ff".repeat(32))),
            ("update_group_start_ordinal", json!(1)),
            ("update_group_count", json!(update_group_count + 1)),
            ("logical_row_count", json!(logical_row_count + 1)),
        ] {
            let mut value = trained_value_v2();
            value["continuation_chain"]["continuations"][0][field] = replacement;
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::ContinuationBinding,
                "descriptor field {field}"
            );
        }

        for (field, replacement) in [
            ("update_group_count", json!(5)),
            ("logical_row_count", json!(logical_row_count + 1)),
            ("first_continuation_sha256", json!("ff".repeat(32))),
            ("last_continuation_sha256", json!("ff".repeat(32))),
        ] {
            let mut value = trained_value_v2();
            value["continuation_chain"][field] = replacement;
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::ContinuationBinding,
                "chain field {field}"
            );
        }

        let mut wrong_count = trained_value_v2();
        wrong_count["continuation_chain"]["continuation_count"] = json!(2);
        assert_eq!(
            decode_trained_value_error_v2(&wrong_count),
            SegmentManifestV2ErrorKind::TrainedInvariant
        );
        let mut empty = trained_value_v2();
        empty["continuation_chain"]["continuations"] = json!([]);
        empty["continuation_chain"]["continuation_count"] = json!(0);
        assert_eq!(
            decode_trained_value_error_v2(&empty),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );
    }

    #[test]
    fn trained_checkpoint_context_and_all_five_digest_bindings_fail_closed() {
        let fixture = trained_fixture_v2();
        assert_eq!(
            decode_trained_segment_manifest_v2(
                fixture.manifest.canonical_bytes(),
                &fixture.run,
                &fixture.parent,
                &fixture.wrong_continuations,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::ContinuationBinding
        );
        for wrong_checkpoint in [&fixture.genesis_checkpoint, &fixture.wrong_checkpoint] {
            assert_eq!(
                decode_trained_segment_manifest_v2(
                    fixture.manifest.canonical_bytes(),
                    &fixture.run,
                    &fixture.parent,
                    &fixture.continuations,
                    wrong_checkpoint,
                )
                .unwrap_err()
                .kind(),
                SegmentManifestV2ErrorKind::FinalCheckpointBinding
            );
            assert_eq!(
                build_trained_segment_manifest_v2(
                    &fixture.run,
                    &fixture.parent,
                    &fixture.continuations,
                    wrong_checkpoint,
                )
                .unwrap_err()
                .kind(),
                SegmentManifestV2ErrorKind::FinalCheckpointBinding
            );
        }

        for field in [
            "checkpoint_manifest_sha256",
            "checkpoint_payload_sha256",
            "logical_state_sha256",
            "model_parameter_sha256",
            "train_state_sha256",
        ] {
            let mut value = trained_value_v2();
            value["final_checkpoint"][field] = json!("ff".repeat(32));
            assert_eq!(
                decode_trained_value_error_v2(&value),
                SegmentManifestV2ErrorKind::FinalCheckpointBinding,
                "field {field}"
            );
        }
    }

    #[test]
    fn canonical_null_cap_unknown_duplicate_and_missing_fields_fail_closed() {
        let fixture = fixture_v2();
        let canonical = String::from_utf8(fixture.segment.canonical_bytes().to_vec()).unwrap();

        let noncanonical = canonical.replacen(":", ": ", 1);
        assert_eq!(
            decode_genesis_segment_manifest_v2(
                noncanonical.as_bytes(),
                &fixture.run,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::NonCanonicalBytes)
        );
        assert_eq!(
            decode_genesis_segment_manifest_v2(
                &fixture.segment.canonical_bytes()[..fixture.segment.canonical_bytes().len() - 1],
                &fixture.run,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::MissingFinalLf)
        );
        let mut trailing = fixture.segment.canonical_bytes().to_vec();
        trailing.push(b' ');
        assert_eq!(
            decode_genesis_segment_manifest_v2(&trailing, &fixture.run, &fixture.checkpoint,)
                .unwrap_err()
                .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::MissingFinalLf)
        );

        let duplicate = canonical.replacen(
            "{",
            "{\"schema\":\"mtg_kernel_native_train_checkpoint_segment/v2\",",
            1,
        );
        assert_eq!(
            decode_genesis_segment_manifest_v2(
                duplicate.as_bytes(),
                &fixture.run,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::DuplicateObjectKey)
        );
        let nested_duplicate = canonical.replacen(
            "\"continuation_chain\":{",
            "\"continuation_chain\":{\"continuation_count\":0,",
            1,
        );
        assert_eq!(
            decode_genesis_segment_manifest_v2(
                nested_duplicate.as_bytes(),
                &fixture.run,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::DuplicateObjectKey)
        );

        let forbidden_null = canonical.replacen(
            &format!(
                "\"checkpoint_manifest_sha256\":\"{}\"",
                lower_hex_raw32_v1(fixture.checkpoint.checkpoint_manifest_sha256())
            ),
            "\"checkpoint_manifest_sha256\":null",
            1,
        );
        assert_eq!(
            decode_genesis_segment_manifest_v2(
                forbidden_null.as_bytes(),
                &fixture.run,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::NullForbidden)
        );

        let mut unknown = segment_value_v2();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_value_error_v2(&unknown),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );
        let mut nested_unknown = segment_value_v2();
        nested_unknown["continuation_chain"]
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_value_error_v2(&nested_unknown),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );
        let mut nested_unknown = segment_value_v2();
        nested_unknown["final_checkpoint"]
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_value_error_v2(&nested_unknown),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let mut row_unknown = segment_value_v2();
        row_unknown["update_count"] = json!(1);
        row_unknown["ordered_update_evidence_count"] = json!(1);
        row_unknown["ordered_update_evidence"] = json!([{
            "update_index": 1,
            "update_evidence_sha256": "00".repeat(32),
            "unknown": 1,
        }]);
        assert_eq!(
            decode_value_error_v2(&row_unknown),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let descriptor = json!({
            "continuation_index": 0,
            "relative_name": "segment-00000004.continuation-00000000.json",
            "byte_count": 1,
            "sha256": "11".repeat(32),
            "previous_continuation_sha256": null,
            "update_group_start_ordinal": 0,
            "update_group_count": 1,
            "logical_row_count": 1,
            "unknown": 1,
        });
        let mut descriptor_unknown = segment_value_v2();
        descriptor_unknown["continuation_chain"]["continuation_count"] = json!(1);
        descriptor_unknown["continuation_chain"]["continuations"] = json!([descriptor]);
        assert_eq!(
            decode_value_error_v2(&descriptor_unknown),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );
        let mut missing = segment_value_v2();
        missing.as_object_mut().unwrap().remove("update_count");
        assert_eq!(
            decode_value_error_v2(&missing),
            SegmentManifestV2ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let oversized = vec![b' '; usize::try_from(SEGMENT_MANIFEST_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            decode_genesis_segment_manifest_v2(&oversized, &fixture.run, &fixture.checkpoint,)
                .unwrap_err()
                .kind(),
            SegmentManifestV2ErrorKind::RecordTooLarge
        );
    }

    #[test]
    fn kind_run_scalar_and_every_genesis_invariant_fail_closed() {
        let mut value = segment_value_v2();
        value["kind"] = json!("trained");
        assert_eq!(
            decode_value_error_v2(&value),
            SegmentManifestV2ErrorKind::TrainedAuthorityRequired
        );
        let mut value = segment_value_v2();
        value["kind"] = json!("GENESIS");
        assert_eq!(
            decode_value_error_v2(&value),
            SegmentManifestV2ErrorKind::InvalidKind
        );

        for (field, replacement, expected) in [
            (
                "schema",
                json!("mtg_kernel_native_train_checkpoint_segment/v1"),
                SegmentManifestV2ErrorKind::InvalidSchema,
            ),
            (
                "run_sha256",
                json!("00".repeat(32)),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "identity_bundle_sha256",
                json!("00".repeat(32)),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "batch_episodes",
                json!(4),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "checkpoint_segment_updates",
                json!(2),
                SegmentManifestV2ErrorKind::RunBinding,
            ),
            (
                "segment_ordinal",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "generation_index",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "update_start_index",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "update_count",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "episode_start",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "episode_count",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "episode_end_exclusive",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
            (
                "ordered_update_evidence_count",
                json!(1),
                SegmentManifestV2ErrorKind::GenesisInvariant,
            ),
        ] {
            let mut value = segment_value_v2();
            value[field] = replacement;
            assert_eq!(decode_value_error_v2(&value), expected, "field {field}");
        }

        let mut over_u63 = segment_value_v2();
        over_u63["segment_ordinal"] = json!(U63_MAX_V2 + 1);
        assert_eq!(
            decode_value_error_v2(&over_u63),
            SegmentManifestV2ErrorKind::InvalidScalar
        );

        for field in [
            "parent_generation_index",
            "parent_head_sha256",
            "parent_last_update_evidence_sha256",
        ] {
            let mut value = segment_value_v2();
            value[field] = if field == "parent_generation_index" {
                json!(0)
            } else {
                json!("00".repeat(32))
            };
            assert_eq!(
                decode_value_error_v2(&value),
                SegmentManifestV2ErrorKind::GenesisInvariant,
                "field {field}"
            );
        }

        for field in [
            "continuation_count",
            "update_group_count",
            "logical_row_count",
        ] {
            let mut value = segment_value_v2();
            value["continuation_chain"][field] = json!(1);
            assert_eq!(
                decode_value_error_v2(&value),
                SegmentManifestV2ErrorKind::GenesisInvariant,
                "field {field}"
            );
        }
        for field in ["first_continuation_sha256", "last_continuation_sha256"] {
            let mut value = segment_value_v2();
            value["continuation_chain"][field] = json!("00".repeat(32));
            assert_eq!(
                decode_value_error_v2(&value),
                SegmentManifestV2ErrorKind::GenesisInvariant,
                "field {field}"
            );
        }

        let row = OrderedUpdateEvidenceRowWireV1 {
            update_index: 1,
            update_evidence_sha256: "00".repeat(32),
        };
        let digest = ordered_update_evidence_list_sha256_v2(
            fixture_v2().run.run_sha256(),
            0,
            1,
            std::slice::from_ref(&row),
        )
        .unwrap();
        let mut value = segment_value_v2();
        value["update_count"] = json!(1);
        value["ordered_update_evidence_count"] = json!(1);
        value["ordered_update_evidence"] = json!([{
            "update_index": 1,
            "update_evidence_sha256": "00".repeat(32),
        }]);
        value["ordered_update_evidence_list_sha256"] = json!(lower_hex_raw32_v1(digest));
        assert_eq!(
            decode_value_error_v2(&value),
            SegmentManifestV2ErrorKind::GenesisInvariant
        );

        let descriptor = json!({
            "continuation_index": 0,
            "relative_name": "segment-00000004.continuation-00000000.json",
            "byte_count": 1,
            "sha256": "11".repeat(32),
            "previous_continuation_sha256": null,
            "update_group_start_ordinal": 0,
            "update_group_count": 1,
            "logical_row_count": 1,
        });
        let mut value = segment_value_v2();
        value["continuation_chain"]["continuation_count"] = json!(1);
        value["continuation_chain"]["update_group_count"] = json!(1);
        value["continuation_chain"]["logical_row_count"] = json!(1);
        value["continuation_chain"]["first_continuation_sha256"] = json!("11".repeat(32));
        value["continuation_chain"]["last_continuation_sha256"] = json!("11".repeat(32));
        value["continuation_chain"]["continuations"] = json!([descriptor]);
        assert_eq!(
            decode_value_error_v2(&value),
            SegmentManifestV2ErrorKind::GenesisInvariant
        );
    }

    #[test]
    fn digest_and_all_final_checkpoint_bindings_fail_closed() {
        for (path, replacement) in [
            (&["run_sha256"][..], json!("A".repeat(64))),
            (
                &["ordered_update_evidence_list_sha256"][..],
                json!("0".repeat(63)),
            ),
            (
                &["final_checkpoint", "checkpoint_manifest_sha256"][..],
                json!("A".repeat(64)),
            ),
        ] {
            let mut value = segment_value_v2();
            if path.len() == 1 {
                value[path[0]] = replacement;
            } else {
                value[path[0]][path[1]] = replacement;
            }
            assert_eq!(
                decode_value_error_v2(&value),
                SegmentManifestV2ErrorKind::InvalidDigest,
                "path {path:?}"
            );
        }

        let mut wrong_list_digest = segment_value_v2();
        wrong_list_digest["ordered_update_evidence_list_sha256"] = json!("ff".repeat(32));
        assert_eq!(
            decode_value_error_v2(&wrong_list_digest),
            SegmentManifestV2ErrorKind::OrderedEvidenceDigestMismatch
        );

        for field in [
            "checkpoint_manifest_sha256",
            "checkpoint_payload_sha256",
            "logical_state_sha256",
            "model_parameter_sha256",
            "train_state_sha256",
        ] {
            let mut value = segment_value_v2();
            value["final_checkpoint"][field] = json!("ff".repeat(32));
            assert_eq!(
                decode_value_error_v2(&value),
                SegmentManifestV2ErrorKind::FinalCheckpointBinding,
                "field {field}"
            );
        }
    }

    #[test]
    fn production_module_has_no_filesystem_or_path_surface() {
        let production = include_str!("native_training_store_segment_manifest_v2.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for forbidden in [
            "std::fs::",
            "std::path::",
            "PathBuf",
            "File::",
            "OpenOptions",
            "create_dir",
            "remove_file",
            "rename(",
            "head_record_sha256",
            "NativeTrainingBoundaryFactsV2",
            "ValidatedNativeTrainingBoundaryV2 {",
            "ValidatedSegmentContinuationChainV2",
        ] {
            assert!(
                !production.contains(forbidden),
                "production source unexpectedly contains {forbidden}"
            );
        }
        assert!(production.contains("&ValidatedSegmentContinuationChainAdvanceV2"));
        let continuation_source = include_str!("native_training_store_segment_continuation_v2.rs");
        assert!(continuation_source.contains(concat!(
            "#[cfg(test)]\n",
            "pub(crate) fn build_segment_continuations_with_test_limits_v2"
        )));
    }
}
