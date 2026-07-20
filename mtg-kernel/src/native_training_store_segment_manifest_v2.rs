//! Pure SegmentManifestV2 authority for Native Training Store V2.
//!
//! This first slice intentionally authorizes only the genesis variant.  The
//! complete frozen tagged-union wire is represented here so a later trained
//! entry point can reuse it, but trained bytes cannot become authoritative
//! without an explicit parent/head and complete continuation boundary.  This
//! module owns no filesystem path, publisher, recovery, receipt, head, latest,
//! reference, or executor mutation.

use crate::canonical_json_v1::{
    count_canonical_json_bytes_v1, from_canonical_json_bytes_v1, to_canonical_json_bytes_v1,
    CanonicalJsonErrorKindV1, CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1,
    CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
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
    let count = count_canonical_json_bytes_v1(&wire, SEGMENT_MANIFEST_NULL_POLICY_V2)?;
    if count > SEGMENT_MANIFEST_MAX_BYTES_V2 {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::RecordTooLarge,
        ));
    }
    let canonical_bytes = to_canonical_json_bytes_v1(&wire, SEGMENT_MANIFEST_NULL_POLICY_V2)?;
    if u64::try_from(canonical_bytes.len())
        .map_err(|_| SegmentManifestV2Error::new(SegmentManifestV2ErrorKind::InvalidArithmetic))?
        != count
    {
        return Err(SegmentManifestV2Error::new(
            SegmentManifestV2ErrorKind::InvalidArithmetic,
        ));
    }
    decode_genesis_segment_manifest_v2(&canonical_bytes, run, checkpoint)
}

/// Decodes and validates only the genesis SegmentManifestV2 variant.
/// Canonical trained bytes always require a later explicit parent-bound API.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3;
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use std::time::Duration;

    struct FixtureV2 {
        run: ValidatedTrainRunV2,
        checkpoint: CheckpointManifestV3,
        segment: SegmentManifestV2,
    }

    static FIXTURE_V2: OnceLock<FixtureV2> = OnceLock::new();

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

    fn fixture_v2() -> &'static FixtureV2 {
        FIXTURE_V2.get_or_init(|| {
            let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
            let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
            let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
                execution_config_v2(&run),
                &snapshot_manifest,
                &snapshot_payload,
            )
            .unwrap();
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
        ] {
            assert!(
                !production.contains(forbidden),
                "production source unexpectedly contains {forbidden}"
            );
        }
    }
}
