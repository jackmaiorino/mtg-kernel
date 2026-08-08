//! Pure sidecar/head boundary authority for Native Training Store V2.
//!
//! Genesis authority starts at update zero. Trained authority additionally
//! requires a sealed logical parent plus the exact trained segment and
//! checkpoint. This module performs no durable-store I/O or publication.

use crate::canonical_json_v1::{
    count_canonical_json_bytes_v1, from_canonical_json_bytes_v1, to_canonical_json_bytes_v1,
    CanonicalJsonClosedMaxErrorV1, CanonicalJsonClosedMaxV1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_manifest_v2::SegmentManifestV2;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const CHECKPOINT_SIDECAR_SCHEMA_V2: &str = "mtg_kernel_native_train_checkpoint_sidecar/v2";
pub const HEAD_RECORD_SCHEMA_V2: &str = "mtg_kernel_native_train_head/v2";
pub const CHECKPOINT_SIDECAR_MAX_BYTES_V2: u64 = 65_536;
pub const HEAD_RECORD_MAX_BYTES_V2: u64 = 65_536;
pub const HEAD_DIGEST_IDENTITY_V2: &str = "mtg-kernel-native-training-head-sha256-v2";
pub const NATIVE_TRAINING_BOUNDARY_RECORD_CONTRACT_SHA256_V2: &str =
    crate::native_training_store_checkpoint_v3::NATIVE_TRAINING_STORE_RECORD_CONTRACT_SHA256_V1;

const U63_MAX_V2: u64 = (1_u64 << 63) - 1;

const PARENT_HEAD_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "parent_head_sha256",
    )];
const LAST_EVIDENCE_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "last_update_evidence_sha256",
    )];
const BOUNDARY_NULL_PATHS_V2: &[&[CanonicalJsonNullPathSegmentV1]] =
    &[PARENT_HEAD_NULL_PATH_V2, LAST_EVIDENCE_NULL_PATH_V2];
const BOUNDARY_NULL_POLICY_V2: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(BOUNDARY_NULL_PATHS_V2);

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CheckpointSidecarWireV2 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    parent_head_sha256: Option<String>,
    segment_manifest_sha256: String,
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    logical_state_sha256: String,
    model_parameter_sha256: String,
    train_state_sha256: String,
    last_update_evidence_sha256: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct HeadRecordWireV2 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    parent_head_sha256: Option<String>,
    segment_manifest_sha256: String,
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    checkpoint_sidecar_sha256: String,
    logical_state_sha256: String,
    model_parameter_sha256: String,
    train_state_sha256: String,
    last_update_evidence_sha256: Option<String>,
    head_sha256: String,
}

fn maximum_boundary_common_fields_v2() -> std::result::Result<
    (CanonicalJsonClosedMaxV1, CanonicalJsonClosedMaxV1),
    CanonicalJsonClosedMaxErrorV1,
> {
    let digest = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(64)?;
    let optional_digest =
        CanonicalJsonClosedMaxV1::choice_v1(CanonicalJsonClosedMaxV1::null_v1(), digest)?;
    Ok((digest, optional_digest))
}

pub(crate) fn maximum_trained_checkpoint_sidecar_cj_bytes_v2(
) -> std::result::Result<u64, CanonicalJsonClosedMaxErrorV1> {
    let u63 = CanonicalJsonClosedMaxV1::max_u63_v1();
    let (digest, optional_digest) = maximum_boundary_common_fields_v2()?;
    CanonicalJsonClosedMaxV1::object_v1(&[
        ("batch_episodes", u63),
        ("checkpoint_manifest_sha256", digest),
        ("checkpoint_payload_sha256", digest),
        ("checkpoint_segment_updates", u63),
        ("generation_index", u63),
        ("identity_bundle_sha256", digest),
        ("last_update_evidence_sha256", optional_digest),
        ("logical_state_sha256", digest),
        ("model_parameter_sha256", digest),
        ("parent_head_sha256", optional_digest),
        ("run_sha256", digest),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(CHECKPOINT_SIDECAR_SCHEMA_V2)?,
        ),
        ("segment_manifest_sha256", digest),
        ("segment_ordinal", u63),
        ("train_state_sha256", digest),
    ])?
    .canonical_document_bytes_v1()
}

pub(crate) fn maximum_trained_head_record_cj_bytes_v2(
) -> std::result::Result<u64, CanonicalJsonClosedMaxErrorV1> {
    let u63 = CanonicalJsonClosedMaxV1::max_u63_v1();
    let (digest, optional_digest) = maximum_boundary_common_fields_v2()?;
    CanonicalJsonClosedMaxV1::object_v1(&[
        ("batch_episodes", u63),
        ("checkpoint_manifest_sha256", digest),
        ("checkpoint_payload_sha256", digest),
        ("checkpoint_segment_updates", u63),
        ("checkpoint_sidecar_sha256", digest),
        ("generation_index", u63),
        ("head_sha256", digest),
        ("identity_bundle_sha256", digest),
        ("last_update_evidence_sha256", optional_digest),
        ("logical_state_sha256", digest),
        ("model_parameter_sha256", digest),
        ("parent_head_sha256", optional_digest),
        ("run_sha256", digest),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(HEAD_RECORD_SCHEMA_V2)?,
        ),
        ("segment_manifest_sha256", digest),
        ("segment_ordinal", u63),
        ("train_state_sha256", digest),
    ])?
    .canonical_document_bytes_v1()
}

struct ExpectedBoundaryFactsV2 {
    run_sha256: String,
    identity_bundle_sha256: String,
    run_sha256_raw: [u8; 32],
    identity_bundle_sha256_raw: [u8; 32],
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    parent_head_sha256: Option<[u8; 32]>,
    segment_manifest_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    logical_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    last_update_evidence_sha256: Option<[u8; 32]>,
}

#[derive(Clone, Copy)]
enum BoundaryVariantV2 {
    Genesis,
    Trained,
}

/// Fully validated sidecar/head boundary authority.
///
/// Genesis is the root authority. Every trained value is lineage-complete by
/// induction: its only construction path requires one already validated
/// concrete parent and binds that parent's logical head and final evidence, so
/// the resulting capability attests the complete ancestry back to genesis
/// without storing or exposing a parent chain.
///
/// The capability is move-only and has no serde surface or unchecked public
/// constructor:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<ValidatedNativeTrainingBoundaryV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<ValidatedNativeTrainingBoundaryV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<ValidatedNativeTrainingBoundaryV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
/// let _ = ValidatedNativeTrainingBoundaryV2 {};
/// ```
pub struct ValidatedNativeTrainingBoundaryV2 {
    facts: ExpectedBoundaryFactsV2,
    checkpoint_sidecar_canonical_bytes: Vec<u8>,
    head_record_canonical_bytes: Vec<u8>,
    checkpoint_sidecar_sha256: [u8; 32],
    head_sha256: [u8; 32],
    head_record_sha256: [u8; 32],
}

/// Narrow crate-internal projection for later parent-bound validation.
///
/// It can only be obtained from an already validated boundary and is not a
/// caller-constructible parent capability.
#[allow(dead_code)]
pub(crate) struct NativeTrainingBoundaryFactsV2<'a> {
    pub(crate) run_sha256: &'a str,
    pub(crate) identity_bundle_sha256: &'a str,
    pub(crate) segment_ordinal: u64,
    pub(crate) generation_index: u64,
    pub(crate) batch_episodes: u64,
    pub(crate) checkpoint_segment_updates: u64,
    pub(crate) parent_head_sha256: Option<[u8; 32]>,
    pub(crate) segment_manifest_sha256: [u8; 32],
    pub(crate) checkpoint_manifest_sha256: [u8; 32],
    pub(crate) checkpoint_payload_sha256: [u8; 32],
    pub(crate) checkpoint_sidecar_sha256: [u8; 32],
    pub(crate) logical_state_sha256: [u8; 32],
    pub(crate) model_parameter_sha256: [u8; 32],
    pub(crate) train_state_sha256: [u8; 32],
    pub(crate) last_update_evidence_sha256: Option<[u8; 32]>,
    pub(crate) head_sha256: [u8; 32],
    pub(crate) head_record_sha256: [u8; 32],
}

impl std::fmt::Debug for ValidatedNativeTrainingBoundaryV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedNativeTrainingBoundaryV2")
            .field("segment_ordinal", &self.facts.segment_ordinal)
            .field("generation_index", &self.facts.generation_index)
            .finish_non_exhaustive()
    }
}

impl ValidatedNativeTrainingBoundaryV2 {
    pub fn checkpoint_sidecar_canonical_bytes(&self) -> &[u8] {
        &self.checkpoint_sidecar_canonical_bytes
    }

    pub fn head_record_canonical_bytes(&self) -> &[u8] {
        &self.head_record_canonical_bytes
    }

    pub const fn checkpoint_sidecar_sha256(&self) -> [u8; 32] {
        self.checkpoint_sidecar_sha256
    }

    pub const fn head_sha256(&self) -> [u8; 32] {
        self.head_sha256
    }

    pub const fn head_record_sha256(&self) -> [u8; 32] {
        self.head_record_sha256
    }

    #[allow(dead_code)]
    pub(crate) fn boundary_facts_v2(&self) -> NativeTrainingBoundaryFactsV2<'_> {
        NativeTrainingBoundaryFactsV2 {
            run_sha256: &self.facts.run_sha256,
            identity_bundle_sha256: &self.facts.identity_bundle_sha256,
            segment_ordinal: self.facts.segment_ordinal,
            generation_index: self.facts.generation_index,
            batch_episodes: self.facts.batch_episodes,
            checkpoint_segment_updates: self.facts.checkpoint_segment_updates,
            parent_head_sha256: self.facts.parent_head_sha256,
            segment_manifest_sha256: self.facts.segment_manifest_sha256,
            checkpoint_manifest_sha256: self.facts.checkpoint_manifest_sha256,
            checkpoint_payload_sha256: self.facts.checkpoint_payload_sha256,
            checkpoint_sidecar_sha256: self.checkpoint_sidecar_sha256,
            logical_state_sha256: self.facts.logical_state_sha256,
            model_parameter_sha256: self.facts.model_parameter_sha256,
            train_state_sha256: self.facts.train_state_sha256,
            last_update_evidence_sha256: self.facts.last_update_evidence_sha256,
            head_sha256: self.head_sha256,
            head_record_sha256: self.head_record_sha256,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingBoundaryV2ErrorKind {
    SidecarRecordTooLarge,
    HeadRecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidDigest,
    InvalidScalar,
    InvalidArithmetic,
    CheckpointBinding,
    SegmentBinding,
    GenesisInvariant,
    TrainedInvariant,
    ParentBoundaryBinding,
    SidecarBinding,
    HeadBinding,
    LogicalHeadDigestMismatch,
}

impl NativeTrainingBoundaryV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::SidecarRecordTooLarge => "native_train_checkpoint_sidecar_v2_record_too_large",
            Self::HeadRecordTooLarge => "native_train_head_v2_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_training_boundary_v2_invalid_schema",
            Self::InvalidDigest => "native_training_boundary_v2_invalid_digest",
            Self::InvalidScalar => "native_training_boundary_v2_invalid_scalar",
            Self::InvalidArithmetic => "native_training_boundary_v2_invalid_arithmetic",
            Self::CheckpointBinding => "native_training_boundary_v2_checkpoint_binding",
            Self::SegmentBinding => "native_training_boundary_v2_segment_binding",
            Self::GenesisInvariant => "native_training_boundary_v2_genesis_invariant",
            Self::TrainedInvariant => "native_training_boundary_v2_trained_invariant",
            Self::ParentBoundaryBinding => "native_training_boundary_v2_parent_boundary_binding",
            Self::SidecarBinding => "native_training_boundary_v2_sidecar_binding",
            Self::HeadBinding => "native_training_boundary_v2_head_binding",
            Self::LogicalHeadDigestMismatch => {
                "native_training_boundary_v2_logical_head_digest_mismatch"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingBoundaryV2Error {
    kind: NativeTrainingBoundaryV2ErrorKind,
}

impl NativeTrainingBoundaryV2Error {
    const fn new(kind: NativeTrainingBoundaryV2ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> NativeTrainingBoundaryV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for NativeTrainingBoundaryV2Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
            error.kind(),
        ))
    }
}

impl Display for NativeTrainingBoundaryV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingBoundaryV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingBoundaryV2Error>;

/// Builds the exact update-zero sidecar and head records, then routes those
/// emitted bytes through the public decoder before returning authority.
pub fn build_genesis_native_training_boundary_v2(
    run: &ValidatedTrainRunV2,
    genesis_segment: &SegmentManifestV2,
    genesis_checkpoint: &CheckpointManifestV3,
) -> Result<ValidatedNativeTrainingBoundaryV2> {
    let facts = derive_expected_genesis_facts_v2(run, genesis_segment, genesis_checkpoint)?;
    let sidecar_wire = sidecar_wire_v2(run, &facts);
    let sidecar_cj = encode_sidecar_v2(&sidecar_wire)?;
    let checkpoint_sidecar_sha256 = sha256_v1(&sidecar_cj);
    let head_sha256 = logical_head_sha256_v2(&facts, checkpoint_sidecar_sha256)?;
    let head_wire = head_wire_v2(run, &facts, checkpoint_sidecar_sha256, head_sha256);
    let head_cj = encode_head_v2(&head_wire)?;
    decode_genesis_native_training_boundary_v2(
        &sidecar_cj,
        &head_cj,
        run,
        genesis_segment,
        genesis_checkpoint,
    )
}

/// Builds one trained sidecar/head pair from a sealed logical parent and the
/// exact trained segment/checkpoint authorities, then routes the emitted bytes
/// through the public trained decoder. Accepting only a validated concrete
/// parent makes the returned authority inductively lineage-complete to genesis.
pub fn build_trained_native_training_boundary_v2(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    trained_segment: &SegmentManifestV2,
    trained_checkpoint: &CheckpointManifestV3,
) -> Result<ValidatedNativeTrainingBoundaryV2> {
    let facts = derive_expected_trained_facts_v2(run, parent, trained_segment, trained_checkpoint)?;
    let sidecar_wire = sidecar_wire_v2(run, &facts);
    let sidecar_cj = encode_sidecar_v2(&sidecar_wire)?;
    let checkpoint_sidecar_sha256 = sha256_v1(&sidecar_cj);
    let head_sha256 = logical_head_sha256_v2(&facts, checkpoint_sidecar_sha256)?;
    let head_wire = head_wire_v2(run, &facts, checkpoint_sidecar_sha256, head_sha256);
    let head_cj = encode_head_v2(&head_wire)?;
    decode_trained_native_training_boundary_v2(
        &sidecar_cj,
        &head_cj,
        run,
        parent,
        trained_segment,
        trained_checkpoint,
    )
}

/// Decodes and cross-validates the exact update-zero sidecar/head pair against
/// three independently sealed authorities.
pub fn decode_genesis_native_training_boundary_v2(
    sidecar_cj: &[u8],
    head_cj: &[u8],
    run: &ValidatedTrainRunV2,
    genesis_segment: &SegmentManifestV2,
    genesis_checkpoint: &CheckpointManifestV3,
) -> Result<ValidatedNativeTrainingBoundaryV2> {
    require_cap_v2(
        sidecar_cj,
        CHECKPOINT_SIDECAR_MAX_BYTES_V2,
        NativeTrainingBoundaryV2ErrorKind::SidecarRecordTooLarge,
    )?;
    require_cap_v2(
        head_cj,
        HEAD_RECORD_MAX_BYTES_V2,
        NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge,
    )?;

    let sidecar_wire: CheckpointSidecarWireV2 =
        from_canonical_json_bytes_v1(sidecar_cj, BOUNDARY_NULL_POLICY_V2)?;
    let head_wire: HeadRecordWireV2 =
        from_canonical_json_bytes_v1(head_cj, BOUNDARY_NULL_POLICY_V2)?;
    let facts = derive_expected_genesis_facts_v2(run, genesis_segment, genesis_checkpoint)?;

    validate_sidecar_wire_v2(&sidecar_wire, run, &facts, BoundaryVariantV2::Genesis)?;
    let checkpoint_sidecar_sha256 = sha256_v1(sidecar_cj);
    let supplied_head_sha256 = validate_head_wire_v2(
        &head_wire,
        run,
        &facts,
        checkpoint_sidecar_sha256,
        BoundaryVariantV2::Genesis,
    )?;
    let head_sha256 = logical_head_sha256_v2(&facts, checkpoint_sidecar_sha256)?;
    if supplied_head_sha256 != head_sha256 {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::LogicalHeadDigestMismatch,
        ));
    }
    let head_record_sha256 = sha256_v1(head_cj);

    Ok(ValidatedNativeTrainingBoundaryV2 {
        facts,
        checkpoint_sidecar_canonical_bytes: sidecar_cj.to_vec(),
        head_record_canonical_bytes: head_cj.to_vec(),
        checkpoint_sidecar_sha256,
        head_sha256,
        head_record_sha256,
    })
}

/// Decodes and cross-validates one trained sidecar/head pair against its
/// concrete sealed logical parent, trained segment, and trained checkpoint.
/// Successful decoding extends the parent's complete genesis-rooted lineage by
/// exactly one segment.
pub fn decode_trained_native_training_boundary_v2(
    sidecar_cj: &[u8],
    head_cj: &[u8],
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    trained_segment: &SegmentManifestV2,
    trained_checkpoint: &CheckpointManifestV3,
) -> Result<ValidatedNativeTrainingBoundaryV2> {
    require_cap_v2(
        sidecar_cj,
        CHECKPOINT_SIDECAR_MAX_BYTES_V2,
        NativeTrainingBoundaryV2ErrorKind::SidecarRecordTooLarge,
    )?;
    require_cap_v2(
        head_cj,
        HEAD_RECORD_MAX_BYTES_V2,
        NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge,
    )?;

    let sidecar_wire: CheckpointSidecarWireV2 =
        from_canonical_json_bytes_v1(sidecar_cj, BOUNDARY_NULL_POLICY_V2)?;
    let head_wire: HeadRecordWireV2 =
        from_canonical_json_bytes_v1(head_cj, BOUNDARY_NULL_POLICY_V2)?;
    let facts = derive_expected_trained_facts_v2(run, parent, trained_segment, trained_checkpoint)?;

    validate_sidecar_wire_v2(&sidecar_wire, run, &facts, BoundaryVariantV2::Trained)?;
    let checkpoint_sidecar_sha256 = sha256_v1(sidecar_cj);
    let supplied_head_sha256 = validate_head_wire_v2(
        &head_wire,
        run,
        &facts,
        checkpoint_sidecar_sha256,
        BoundaryVariantV2::Trained,
    )?;
    let head_sha256 = logical_head_sha256_v2(&facts, checkpoint_sidecar_sha256)?;
    if supplied_head_sha256 != head_sha256 {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::LogicalHeadDigestMismatch,
        ));
    }
    let head_record_sha256 = sha256_v1(head_cj);

    Ok(ValidatedNativeTrainingBoundaryV2 {
        facts,
        checkpoint_sidecar_canonical_bytes: sidecar_cj.to_vec(),
        head_record_canonical_bytes: head_cj.to_vec(),
        checkpoint_sidecar_sha256,
        head_sha256,
        head_record_sha256,
    })
}

fn sidecar_wire_v2(
    run: &ValidatedTrainRunV2,
    facts: &ExpectedBoundaryFactsV2,
) -> CheckpointSidecarWireV2 {
    CheckpointSidecarWireV2 {
        schema: run.record().artifact_schemas.sidecar.clone(),
        run_sha256: facts.run_sha256.clone(),
        identity_bundle_sha256: facts.identity_bundle_sha256.clone(),
        segment_ordinal: facts.segment_ordinal,
        generation_index: facts.generation_index,
        batch_episodes: facts.batch_episodes,
        checkpoint_segment_updates: facts.checkpoint_segment_updates,
        parent_head_sha256: facts.parent_head_sha256.map(lower_hex_raw32_v1),
        segment_manifest_sha256: lower_hex_raw32_v1(facts.segment_manifest_sha256),
        checkpoint_manifest_sha256: lower_hex_raw32_v1(facts.checkpoint_manifest_sha256),
        checkpoint_payload_sha256: lower_hex_raw32_v1(facts.checkpoint_payload_sha256),
        logical_state_sha256: lower_hex_raw32_v1(facts.logical_state_sha256),
        model_parameter_sha256: lower_hex_raw32_v1(facts.model_parameter_sha256),
        train_state_sha256: lower_hex_raw32_v1(facts.train_state_sha256),
        last_update_evidence_sha256: facts.last_update_evidence_sha256.map(lower_hex_raw32_v1),
    }
}

fn head_wire_v2(
    run: &ValidatedTrainRunV2,
    facts: &ExpectedBoundaryFactsV2,
    checkpoint_sidecar_sha256: [u8; 32],
    head_sha256: [u8; 32],
) -> HeadRecordWireV2 {
    HeadRecordWireV2 {
        schema: run.record().artifact_schemas.head.clone(),
        run_sha256: facts.run_sha256.clone(),
        identity_bundle_sha256: facts.identity_bundle_sha256.clone(),
        segment_ordinal: facts.segment_ordinal,
        generation_index: facts.generation_index,
        batch_episodes: facts.batch_episodes,
        checkpoint_segment_updates: facts.checkpoint_segment_updates,
        parent_head_sha256: facts.parent_head_sha256.map(lower_hex_raw32_v1),
        segment_manifest_sha256: lower_hex_raw32_v1(facts.segment_manifest_sha256),
        checkpoint_manifest_sha256: lower_hex_raw32_v1(facts.checkpoint_manifest_sha256),
        checkpoint_payload_sha256: lower_hex_raw32_v1(facts.checkpoint_payload_sha256),
        checkpoint_sidecar_sha256: lower_hex_raw32_v1(checkpoint_sidecar_sha256),
        logical_state_sha256: lower_hex_raw32_v1(facts.logical_state_sha256),
        model_parameter_sha256: lower_hex_raw32_v1(facts.model_parameter_sha256),
        train_state_sha256: lower_hex_raw32_v1(facts.train_state_sha256),
        last_update_evidence_sha256: facts.last_update_evidence_sha256.map(lower_hex_raw32_v1),
        head_sha256: lower_hex_raw32_v1(head_sha256),
    }
}

fn encode_sidecar_v2(wire: &CheckpointSidecarWireV2) -> Result<Vec<u8>> {
    let count = count_canonical_json_bytes_v1(wire, BOUNDARY_NULL_POLICY_V2)?;
    if count > CHECKPOINT_SIDECAR_MAX_BYTES_V2 {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::SidecarRecordTooLarge,
        ));
    }
    let bytes = to_canonical_json_bytes_v1(wire, BOUNDARY_NULL_POLICY_V2)?;
    require_emitted_count_v2(count, &bytes)?;
    Ok(bytes)
}

fn encode_head_v2(wire: &HeadRecordWireV2) -> Result<Vec<u8>> {
    let count = count_canonical_json_bytes_v1(wire, BOUNDARY_NULL_POLICY_V2)?;
    if count > HEAD_RECORD_MAX_BYTES_V2 {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge,
        ));
    }
    let bytes = to_canonical_json_bytes_v1(wire, BOUNDARY_NULL_POLICY_V2)?;
    require_emitted_count_v2(count, &bytes)?;
    Ok(bytes)
}

fn require_emitted_count_v2(expected: u64, bytes: &[u8]) -> Result<()> {
    let actual = u64::try_from(bytes.len()).map_err(|_| {
        NativeTrainingBoundaryV2Error::new(NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic)
    })?;
    if actual != expected {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic,
        ));
    }
    Ok(())
}

fn require_cap_v2(bytes: &[u8], cap: u64, kind: NativeTrainingBoundaryV2ErrorKind) -> Result<()> {
    let count = u64::try_from(bytes.len()).map_err(|_| NativeTrainingBoundaryV2Error::new(kind))?;
    if count > cap {
        return Err(NativeTrainingBoundaryV2Error::new(kind));
    }
    Ok(())
}

fn derive_expected_genesis_facts_v2(
    run: &ValidatedTrainRunV2,
    segment: &SegmentManifestV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<ExpectedBoundaryFactsV2> {
    let record = run.record();
    if record.artifact_schemas.sidecar != CHECKPOINT_SIDECAR_SCHEMA_V2
        || record.artifact_schemas.head != HEAD_RECORD_SCHEMA_V2
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema,
        ));
    }
    require_positive_u63_v2(run.batch_episodes())?;
    require_positive_u63_v2(run.checkpoint_segment_updates())?;
    let run_sha256_raw = parse_digest_v2(run.run_sha256())?;
    let identity_bundle_sha256_raw = parse_digest_v2(run.identity_bundle_sha256())?;

    validate_genesis_checkpoint_v2(run, checkpoint)?;
    let segment_facts = segment.boundary_facts_v2();
    if segment_facts.kind != "genesis"
        || segment_facts.segment_ordinal != 0
        || segment_facts.parent_generation_index.is_some()
        || segment_facts.generation_index != 0
        || segment_facts.parent_head_sha256.is_some()
        || segment_facts.parent_last_update_evidence_sha256.is_some()
        || segment_facts.last_update_evidence_sha256.is_some()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::GenesisInvariant,
        ));
    }
    if segment_facts.run_sha256 != run.run_sha256()
        || segment_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || segment_facts.batch_episodes != run.batch_episodes()
        || segment_facts.checkpoint_segment_updates != run.checkpoint_segment_updates()
        || sha256_v1(segment.canonical_bytes()) != segment.segment_manifest_sha256()
        || segment_facts.segment_manifest_sha256 != segment.segment_manifest_sha256()
        || segment_facts.checkpoint_manifest_sha256 != checkpoint.checkpoint_manifest_sha256()
        || segment_facts.checkpoint_payload_sha256 != checkpoint.checkpoint_payload_sha256()
        || segment_facts.logical_state_sha256 != checkpoint.logical_state_sha256()
        || segment_facts.model_parameter_sha256 != checkpoint.model_parameter_sha256()
        || segment_facts.train_state_sha256 != checkpoint.train_state_sha256()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::SegmentBinding,
        ));
    }

    Ok(ExpectedBoundaryFactsV2 {
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        run_sha256_raw,
        identity_bundle_sha256_raw,
        segment_ordinal: 0,
        generation_index: 0,
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates: run.checkpoint_segment_updates(),
        parent_head_sha256: None,
        segment_manifest_sha256: segment.segment_manifest_sha256(),
        checkpoint_manifest_sha256: checkpoint.checkpoint_manifest_sha256(),
        checkpoint_payload_sha256: checkpoint.checkpoint_payload_sha256(),
        logical_state_sha256: checkpoint.logical_state_sha256(),
        model_parameter_sha256: checkpoint.model_parameter_sha256(),
        train_state_sha256: checkpoint.train_state_sha256(),
        last_update_evidence_sha256: None,
    })
}

fn derive_expected_trained_facts_v2(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    segment: &SegmentManifestV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<ExpectedBoundaryFactsV2> {
    let record = run.record();
    if record.artifact_schemas.sidecar != CHECKPOINT_SIDECAR_SCHEMA_V2
        || record.artifact_schemas.head != HEAD_RECORD_SCHEMA_V2
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema,
        ));
    }
    let batch_episodes = run.batch_episodes();
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    require_positive_u63_v2(batch_episodes)?;
    require_positive_u63_v2(checkpoint_segment_updates)?;
    let run_sha256_raw = parse_digest_v2(run.run_sha256())?;
    let identity_bundle_sha256_raw = parse_digest_v2(run.identity_bundle_sha256())?;

    let parent_facts = parent.boundary_facts_v2();
    if parent_facts.run_sha256 != run.run_sha256()
        || parent_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || parent_facts.batch_episodes != batch_episodes
        || parent_facts.checkpoint_segment_updates != checkpoint_segment_updates
        || parent_facts.head_sha256 != parent.head_sha256()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::ParentBoundaryBinding,
        ));
    }
    require_u63_v2(parent_facts.segment_ordinal)?;
    require_u63_v2(parent_facts.generation_index)?;
    if parent_facts.last_update_evidence_sha256.is_none() != (parent_facts.generation_index == 0)
        || checked_u63_mul_v2(parent_facts.segment_ordinal, checkpoint_segment_updates)?
            != parent_facts.generation_index
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::ParentBoundaryBinding,
        ));
    }

    let segment_ordinal = checked_u63_add_v2(parent_facts.segment_ordinal, 1)?;
    let generation_index =
        checked_u63_add_v2(parent_facts.generation_index, checkpoint_segment_updates)?;
    if checked_u63_mul_v2(segment_ordinal, checkpoint_segment_updates)? != generation_index
        || generation_index > run.requested_successful_updates()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::TrainedInvariant,
        ));
    }
    let expected_completed_episodes = checked_u63_mul_v2(batch_episodes, generation_index)?;

    validate_trained_checkpoint_v2(
        run,
        checkpoint,
        segment_ordinal,
        generation_index,
        expected_completed_episodes,
    )?;
    let segment_facts = segment.boundary_facts_v2();
    if segment_facts.kind != "trained"
        || segment_facts.segment_ordinal != segment_ordinal
        || segment_facts.parent_generation_index != Some(parent_facts.generation_index)
        || segment_facts.generation_index != generation_index
        || segment_facts.parent_head_sha256 != Some(parent.head_sha256())
        || segment_facts.parent_last_update_evidence_sha256
            != parent_facts.last_update_evidence_sha256
        || segment_facts.last_update_evidence_sha256.is_none()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::TrainedInvariant,
        ));
    }
    if segment_facts.run_sha256 != run.run_sha256()
        || segment_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || segment_facts.batch_episodes != batch_episodes
        || segment_facts.checkpoint_segment_updates != checkpoint_segment_updates
        || sha256_v1(segment.canonical_bytes()) != segment.segment_manifest_sha256()
        || segment_facts.segment_manifest_sha256 != segment.segment_manifest_sha256()
        || segment_facts.checkpoint_manifest_sha256 != checkpoint.checkpoint_manifest_sha256()
        || segment_facts.checkpoint_payload_sha256 != checkpoint.checkpoint_payload_sha256()
        || segment_facts.logical_state_sha256 != checkpoint.logical_state_sha256()
        || segment_facts.model_parameter_sha256 != checkpoint.model_parameter_sha256()
        || segment_facts.train_state_sha256 != checkpoint.train_state_sha256()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::SegmentBinding,
        ));
    }

    Ok(ExpectedBoundaryFactsV2 {
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        run_sha256_raw,
        identity_bundle_sha256_raw,
        segment_ordinal,
        generation_index,
        batch_episodes,
        checkpoint_segment_updates,
        parent_head_sha256: Some(parent.head_sha256()),
        segment_manifest_sha256: segment.segment_manifest_sha256(),
        checkpoint_manifest_sha256: checkpoint.checkpoint_manifest_sha256(),
        checkpoint_payload_sha256: checkpoint.checkpoint_payload_sha256(),
        logical_state_sha256: checkpoint.logical_state_sha256(),
        model_parameter_sha256: checkpoint.model_parameter_sha256(),
        train_state_sha256: checkpoint.train_state_sha256(),
        last_update_evidence_sha256: segment_facts.last_update_evidence_sha256,
    })
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
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding,
        ));
    }
    Ok(())
}

fn validate_trained_checkpoint_v2(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    segment_ordinal: u64,
    generation_index: u64,
    expected_completed_episodes: u64,
) -> Result<()> {
    let progress = checkpoint.progress();
    if checkpoint.run_sha256() != run.run_sha256()
        || checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
        || checkpoint.segment_ordinal() != segment_ordinal
        || checkpoint.generation_index() != generation_index
        || checkpoint.batch_episodes() != run.batch_episodes()
        || checkpoint.checkpoint_segment_updates() != run.checkpoint_segment_updates()
        || progress.batch_episodes() != run.batch_episodes()
        || progress.checkpoint_segment_updates() != run.checkpoint_segment_updates()
        || progress.next_episode_index() != expected_completed_episodes
        || progress.successful_update_count() != generation_index
        || progress.completed_episode_count() != expected_completed_episodes
        || checkpoint.train_state().adam_step() != generation_index
        || sha256_v1(checkpoint.canonical_bytes()) != checkpoint.checkpoint_manifest_sha256()
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding,
        ));
    }
    Ok(())
}

fn validate_sidecar_wire_v2(
    wire: &CheckpointSidecarWireV2,
    run: &ValidatedTrainRunV2,
    facts: &ExpectedBoundaryFactsV2,
    variant: BoundaryVariantV2,
) -> Result<()> {
    if wire.schema != CHECKPOINT_SIDECAR_SCHEMA_V2
        || wire.schema != run.record().artifact_schemas.sidecar
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema,
        ));
    }
    validate_sidecar_scalars_v2(wire)?;
    let parent_head_sha256 = parse_optional_digest_v2(wire.parent_head_sha256.as_deref())?;
    let last_update_evidence_sha256 =
        parse_optional_digest_v2(wire.last_update_evidence_sha256.as_deref())?;
    validate_boundary_options_v2(parent_head_sha256, last_update_evidence_sha256, variant)?;
    if parse_digest_v2(&wire.run_sha256)? != facts.run_sha256_raw
        || parse_digest_v2(&wire.identity_bundle_sha256)? != facts.identity_bundle_sha256_raw
        || wire.segment_ordinal != facts.segment_ordinal
        || wire.generation_index != facts.generation_index
        || wire.batch_episodes != facts.batch_episodes
        || wire.checkpoint_segment_updates != facts.checkpoint_segment_updates
        || parent_head_sha256 != facts.parent_head_sha256
        || parse_digest_v2(&wire.segment_manifest_sha256)? != facts.segment_manifest_sha256
        || parse_digest_v2(&wire.checkpoint_manifest_sha256)? != facts.checkpoint_manifest_sha256
        || parse_digest_v2(&wire.checkpoint_payload_sha256)? != facts.checkpoint_payload_sha256
        || parse_digest_v2(&wire.logical_state_sha256)? != facts.logical_state_sha256
        || parse_digest_v2(&wire.model_parameter_sha256)? != facts.model_parameter_sha256
        || parse_digest_v2(&wire.train_state_sha256)? != facts.train_state_sha256
        || last_update_evidence_sha256 != facts.last_update_evidence_sha256
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::SidecarBinding,
        ));
    }
    Ok(())
}

fn validate_head_wire_v2(
    wire: &HeadRecordWireV2,
    run: &ValidatedTrainRunV2,
    facts: &ExpectedBoundaryFactsV2,
    checkpoint_sidecar_sha256: [u8; 32],
    variant: BoundaryVariantV2,
) -> Result<[u8; 32]> {
    if wire.schema != HEAD_RECORD_SCHEMA_V2 || wire.schema != run.record().artifact_schemas.head {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema,
        ));
    }
    validate_head_scalars_v2(wire)?;
    let parent_head_sha256 = parse_optional_digest_v2(wire.parent_head_sha256.as_deref())?;
    let last_update_evidence_sha256 =
        parse_optional_digest_v2(wire.last_update_evidence_sha256.as_deref())?;
    validate_boundary_options_v2(parent_head_sha256, last_update_evidence_sha256, variant)?;
    if parse_digest_v2(&wire.run_sha256)? != facts.run_sha256_raw
        || parse_digest_v2(&wire.identity_bundle_sha256)? != facts.identity_bundle_sha256_raw
        || wire.segment_ordinal != facts.segment_ordinal
        || wire.generation_index != facts.generation_index
        || wire.batch_episodes != facts.batch_episodes
        || wire.checkpoint_segment_updates != facts.checkpoint_segment_updates
        || parent_head_sha256 != facts.parent_head_sha256
        || parse_digest_v2(&wire.segment_manifest_sha256)? != facts.segment_manifest_sha256
        || parse_digest_v2(&wire.checkpoint_manifest_sha256)? != facts.checkpoint_manifest_sha256
        || parse_digest_v2(&wire.checkpoint_payload_sha256)? != facts.checkpoint_payload_sha256
        || parse_digest_v2(&wire.checkpoint_sidecar_sha256)? != checkpoint_sidecar_sha256
        || parse_digest_v2(&wire.logical_state_sha256)? != facts.logical_state_sha256
        || parse_digest_v2(&wire.model_parameter_sha256)? != facts.model_parameter_sha256
        || parse_digest_v2(&wire.train_state_sha256)? != facts.train_state_sha256
        || last_update_evidence_sha256 != facts.last_update_evidence_sha256
    {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::HeadBinding,
        ));
    }
    parse_digest_v2(&wire.head_sha256)
}

fn validate_boundary_options_v2(
    parent_head_sha256: Option<[u8; 32]>,
    last_update_evidence_sha256: Option<[u8; 32]>,
    variant: BoundaryVariantV2,
) -> Result<()> {
    let invalid = match variant {
        BoundaryVariantV2::Genesis => {
            parent_head_sha256.is_some() || last_update_evidence_sha256.is_some()
        }
        BoundaryVariantV2::Trained => {
            parent_head_sha256.is_none() || last_update_evidence_sha256.is_none()
        }
    };
    if invalid {
        let kind = match variant {
            BoundaryVariantV2::Genesis => NativeTrainingBoundaryV2ErrorKind::GenesisInvariant,
            BoundaryVariantV2::Trained => NativeTrainingBoundaryV2ErrorKind::TrainedInvariant,
        };
        return Err(NativeTrainingBoundaryV2Error::new(kind));
    }
    Ok(())
}

fn validate_sidecar_scalars_v2(wire: &CheckpointSidecarWireV2) -> Result<()> {
    for value in [
        wire.segment_ordinal,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
    ] {
        require_u63_v2(value)?;
    }
    Ok(())
}

fn validate_head_scalars_v2(wire: &HeadRecordWireV2) -> Result<()> {
    for value in [
        wire.segment_ordinal,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
    ] {
        require_u63_v2(value)?;
    }
    Ok(())
}

fn logical_head_sha256_v2(
    facts: &ExpectedBoundaryFactsV2,
    checkpoint_sidecar_sha256: [u8; 32],
) -> Result<[u8; 32]> {
    let mut digest = NativeTrainingStoreAtomSha256V1::new();
    digest
        .atom("domain", HEAD_DIGEST_IDENTITY_V2.as_bytes())
        .map_err(map_digest_error_v2)?;
    digest
        .atom(
            "parent_head_sha256",
            facts
                .parent_head_sha256
                .as_ref()
                .map_or(&[][..], |raw| raw.as_slice()),
        )
        .map_err(map_digest_error_v2)?;
    digest
        .atom("run_sha256", &facts.run_sha256_raw)
        .map_err(map_digest_error_v2)?;
    digest
        .atom("identity_bundle_sha256", &facts.identity_bundle_sha256_raw)
        .map_err(map_digest_error_v2)?;
    digest
        .atom(
            "generation_index_u64be",
            &facts.generation_index.to_be_bytes(),
        )
        .map_err(map_digest_error_v2)?;
    digest
        .atom(
            "checkpoint_segment_updates_u64be",
            &facts.checkpoint_segment_updates.to_be_bytes(),
        )
        .map_err(map_digest_error_v2)?;
    digest
        .atom("segment_manifest_sha256", &facts.segment_manifest_sha256)
        .map_err(map_digest_error_v2)?;
    digest
        .atom(
            "checkpoint_manifest_sha256",
            &facts.checkpoint_manifest_sha256,
        )
        .map_err(map_digest_error_v2)?;
    digest
        .atom(
            "checkpoint_payload_sha256",
            &facts.checkpoint_payload_sha256,
        )
        .map_err(map_digest_error_v2)?;
    digest
        .atom("checkpoint_sidecar_sha256", &checkpoint_sidecar_sha256)
        .map_err(map_digest_error_v2)?;
    digest
        .atom("logical_state_sha256", &facts.logical_state_sha256)
        .map_err(map_digest_error_v2)?;
    digest
        .atom(
            "last_update_evidence_sha256",
            facts
                .last_update_evidence_sha256
                .as_ref()
                .map_or(&[][..], |raw| raw.as_slice()),
        )
        .map_err(map_digest_error_v2)?;
    Ok(digest.finalize())
}

fn parse_digest_v2(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value).map_err(map_digest_error_v2)
}

fn parse_optional_digest_v2(value: Option<&str>) -> Result<Option<[u8; 32]>> {
    value.map(parse_digest_v2).transpose()
}

fn map_digest_error_v2(error: NativeTrainingStoreDigestErrorV1) -> NativeTrainingBoundaryV2Error {
    let kind = match error {
        NativeTrainingStoreDigestErrorV1::InvalidRaw32 => {
            NativeTrainingBoundaryV2ErrorKind::InvalidDigest
        }
        NativeTrainingStoreDigestErrorV1::AtomTagLength
        | NativeTrainingStoreDigestErrorV1::AtomPayloadLength => {
            NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic
        }
    };
    NativeTrainingBoundaryV2Error::new(kind)
}

fn require_u63_v2(value: u64) -> Result<()> {
    if value > U63_MAX_V2 {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidScalar,
        ));
    }
    Ok(())
}

fn require_positive_u63_v2(value: u64) -> Result<()> {
    if value == 0 {
        return Err(NativeTrainingBoundaryV2Error::new(
            NativeTrainingBoundaryV2ErrorKind::InvalidScalar,
        ));
    }
    require_u63_v2(value)
}

fn checked_u63_add_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(|| {
            NativeTrainingBoundaryV2Error::new(NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic)
        })
}

fn checked_u63_mul_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(|| {
            NativeTrainingBoundaryV2Error::new(NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_continuation_v2::{
        build_segment_continuations_v2, ValidatedSegmentContinuationChainAdvanceV2,
    };
    use crate::native_training_store_segment_manifest_v2::{
        build_genesis_segment_manifest_v2, build_trained_segment_manifest_v2,
    };
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1, decode_update_group_v1,
        UpdateEvidenceChainContextV1, ValidatedUpdateGroupV1,
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use std::time::Duration;

    #[test]
    fn trained_boundary_closed_maxima_are_frozen() {
        assert_eq!(
            maximum_trained_checkpoint_sidecar_cj_bytes_v2().unwrap(),
            1_133
        );
        assert_eq!(maximum_trained_head_record_cj_bytes_v2().unwrap(), 1_295);
    }

    struct FixtureV2 {
        run: ValidatedTrainRunV2,
        genesis_payload: Vec<u8>,
        checkpoint: CheckpointManifestV3,
        segment: SegmentManifestV2,
        boundary: ValidatedNativeTrainingBoundaryV2,
    }

    struct TrainedBoundaryFixtureV2 {
        run: ValidatedTrainRunV2,
        genesis_checkpoint: CheckpointManifestV3,
        genesis_segment: SegmentManifestV2,
        genesis_boundary: ValidatedNativeTrainingBoundaryV2,
        first_checkpoint: CheckpointManifestV3,
        first_segment: SegmentManifestV2,
        first_boundary: ValidatedNativeTrainingBoundaryV2,
        second_checkpoint: CheckpointManifestV3,
        second_segment: SegmentManifestV2,
        second_boundary: ValidatedNativeTrainingBoundaryV2,
    }

    static FIXTURE_V2: OnceLock<FixtureV2> = OnceLock::new();
    static TRAINED_BOUNDARY_FIXTURE_V2: OnceLock<TrainedBoundaryFixtureV2> = OnceLock::new();

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
            let genesis_payload = candidate.payload().to_vec();
            let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
            let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
            let boundary =
                build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
            FixtureV2 {
                run,
                genesis_payload,
                checkpoint,
                segment,
                boundary,
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

    fn segment_advance_v2(
        run: &ValidatedTrainRunV2,
        genesis: &CheckpointManifestV3,
        group_bytes: &[Vec<u8>],
        start: usize,
        count: usize,
    ) -> ValidatedSegmentContinuationChainAdvanceV2 {
        build_segment_continuations_v2(
            run,
            context_from_group_bytes_v2(run, genesis, group_bytes, start),
            groups_from_bytes_v2(run, genesis, group_bytes, start, count),
        )
        .unwrap()
    }

    fn trained_boundary_fixture_v2() -> &'static TrainedBoundaryFixtureV2 {
        TRAINED_BOUNDARY_FIXTURE_V2.get_or_init(|| {
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
            let genesis_boundary = build_genesis_native_training_boundary_v2(
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
            let first_continuations =
                segment_advance_v2(&run, &genesis_checkpoint, &group_bytes, 0, segment_updates);
            let first_checkpoint = build_trained_checkpoint_manifest_v3(
                &run,
                first_continuations.advanced_context(),
                &boundary_candidates[0],
            )
            .unwrap();
            let first_segment = build_trained_segment_manifest_v2(
                &run,
                &genesis_boundary,
                &first_continuations,
                &first_checkpoint,
            )
            .unwrap();
            let first_boundary = build_trained_native_training_boundary_v2(
                &run,
                &genesis_boundary,
                &first_segment,
                &first_checkpoint,
            )
            .unwrap();

            let second_continuations = segment_advance_v2(
                &run,
                &genesis_checkpoint,
                &group_bytes,
                segment_updates,
                segment_updates,
            );
            let second_checkpoint = build_trained_checkpoint_manifest_v3(
                &run,
                second_continuations.advanced_context(),
                &boundary_candidates[1],
            )
            .unwrap();
            let second_segment = build_trained_segment_manifest_v2(
                &run,
                &first_boundary,
                &second_continuations,
                &second_checkpoint,
            )
            .unwrap();
            let second_boundary = build_trained_native_training_boundary_v2(
                &run,
                &first_boundary,
                &second_segment,
                &second_checkpoint,
            )
            .unwrap();

            TrainedBoundaryFixtureV2 {
                run,
                genesis_checkpoint,
                genesis_segment,
                genesis_boundary,
                first_checkpoint,
                first_segment,
                first_boundary,
                second_checkpoint,
                second_segment,
                second_boundary,
            }
        })
    }

    fn sidecar_value_v2() -> Value {
        serde_json::from_slice(
            fixture_v2()
                .boundary
                .checkpoint_sidecar_canonical_bytes()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap()
    }

    fn head_value_v2() -> Value {
        serde_json::from_slice(
            fixture_v2()
                .boundary
                .head_record_canonical_bytes()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap()
    }

    fn boundary_sidecar_value_v2(boundary: &ValidatedNativeTrainingBoundaryV2) -> Value {
        serde_json::from_slice(
            boundary
                .checkpoint_sidecar_canonical_bytes()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap()
    }

    fn boundary_head_value_v2(boundary: &ValidatedNativeTrainingBoundaryV2) -> Value {
        serde_json::from_slice(
            boundary
                .head_record_canonical_bytes()
                .strip_suffix(b"\n")
                .unwrap(),
        )
        .unwrap()
    }

    fn trained_sidecar_value_v2() -> Value {
        boundary_sidecar_value_v2(&trained_boundary_fixture_v2().first_boundary)
    }

    fn trained_head_value_v2() -> Value {
        boundary_head_value_v2(&trained_boundary_fixture_v2().first_boundary)
    }

    fn canonical_boundary_value_bytes_v2(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, BOUNDARY_NULL_POLICY_V2).unwrap()
    }

    fn decode_pair_error_v2(
        sidecar_cj: &[u8],
        head_cj: &[u8],
    ) -> NativeTrainingBoundaryV2ErrorKind {
        let fixture = fixture_v2();
        decode_genesis_native_training_boundary_v2(
            sidecar_cj,
            head_cj,
            &fixture.run,
            &fixture.segment,
            &fixture.checkpoint,
        )
        .unwrap_err()
        .kind()
    }

    fn decode_sidecar_value_error_v2(value: &Value) -> NativeTrainingBoundaryV2ErrorKind {
        decode_pair_error_v2(
            &canonical_boundary_value_bytes_v2(value),
            fixture_v2().boundary.head_record_canonical_bytes(),
        )
    }

    fn decode_head_value_error_v2(value: &Value) -> NativeTrainingBoundaryV2ErrorKind {
        decode_pair_error_v2(
            fixture_v2().boundary.checkpoint_sidecar_canonical_bytes(),
            &canonical_boundary_value_bytes_v2(value),
        )
    }

    fn decode_trained_pair_error_v2(
        sidecar_cj: &[u8],
        head_cj: &[u8],
    ) -> NativeTrainingBoundaryV2ErrorKind {
        let fixture = trained_boundary_fixture_v2();
        decode_trained_native_training_boundary_v2(
            sidecar_cj,
            head_cj,
            &fixture.run,
            &fixture.genesis_boundary,
            &fixture.first_segment,
            &fixture.first_checkpoint,
        )
        .unwrap_err()
        .kind()
    }

    fn decode_trained_sidecar_value_error_v2(value: &Value) -> NativeTrainingBoundaryV2ErrorKind {
        decode_trained_pair_error_v2(
            &canonical_boundary_value_bytes_v2(value),
            trained_boundary_fixture_v2()
                .first_boundary
                .head_record_canonical_bytes(),
        )
    }

    fn decode_trained_head_value_error_v2(value: &Value) -> NativeTrainingBoundaryV2ErrorKind {
        decode_trained_pair_error_v2(
            trained_boundary_fixture_v2()
                .first_boundary
                .checkpoint_sidecar_canonical_bytes(),
            &canonical_boundary_value_bytes_v2(value),
        )
    }

    fn independent_trained_head_sha256_v2(
        run: &ValidatedTrainRunV2,
        parent: &ValidatedNativeTrainingBoundaryV2,
        segment: &SegmentManifestV2,
        checkpoint: &CheckpointManifestV3,
        boundary: &ValidatedNativeTrainingBoundaryV2,
    ) -> [u8; 32] {
        fn atom(reference: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            reference.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(tag.as_bytes());
            reference.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(payload);
        }

        let segment_facts = segment.boundary_facts_v2();
        let mut reference = Vec::new();
        atom(&mut reference, "domain", HEAD_DIGEST_IDENTITY_V2.as_bytes());
        atom(&mut reference, "parent_head_sha256", &parent.head_sha256());
        atom(
            &mut reference,
            "run_sha256",
            &parse_lower_hex_raw32_v1(run.run_sha256()).unwrap(),
        );
        atom(
            &mut reference,
            "identity_bundle_sha256",
            &parse_lower_hex_raw32_v1(run.identity_bundle_sha256()).unwrap(),
        );
        atom(
            &mut reference,
            "generation_index_u64be",
            &checkpoint.generation_index().to_be_bytes(),
        );
        atom(
            &mut reference,
            "checkpoint_segment_updates_u64be",
            &run.checkpoint_segment_updates().to_be_bytes(),
        );
        atom(
            &mut reference,
            "segment_manifest_sha256",
            &segment.segment_manifest_sha256(),
        );
        atom(
            &mut reference,
            "checkpoint_manifest_sha256",
            &checkpoint.checkpoint_manifest_sha256(),
        );
        atom(
            &mut reference,
            "checkpoint_payload_sha256",
            &checkpoint.checkpoint_payload_sha256(),
        );
        atom(
            &mut reference,
            "checkpoint_sidecar_sha256",
            &boundary.checkpoint_sidecar_sha256(),
        );
        atom(
            &mut reference,
            "logical_state_sha256",
            &checkpoint.logical_state_sha256(),
        );
        atom(
            &mut reference,
            "last_update_evidence_sha256",
            &segment_facts.last_update_evidence_sha256.unwrap(),
        );
        Sha256::digest(reference).into()
    }

    fn alternate_genesis_authorities_v2(
    ) -> (ValidatedTrainRunV2, CheckpointManifestV3, SegmentManifestV2) {
        let run_bytes = test_fixture_bytes_v2();
        let mut run_value: Value =
            serde_json::from_slice(run_bytes.strip_suffix(b"\n").unwrap()).unwrap();
        let os_build = run_value["runtime"]["os_build"].as_u64().unwrap();
        run_value["runtime"]["os_build"] = json!(os_build.checked_add(1).unwrap());
        let alternate_run_bytes =
            to_canonical_json_bytes_v1(&run_value, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let run = decode_train_run_v2(&alternate_run_bytes).unwrap();
        let checkpoint =
            build_genesis_checkpoint_manifest_v3(&run, &fixture_v2().genesis_payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        (run, checkpoint, segment)
    }

    #[test]
    fn genuine_executor_genesis_has_exact_records_hashes_and_roundtrip() {
        let fixture = fixture_v2();
        let boundary = decode_genesis_native_training_boundary_v2(
            fixture.boundary.checkpoint_sidecar_canonical_bytes(),
            fixture.boundary.head_record_canonical_bytes(),
            &fixture.run,
            &fixture.segment,
            &fixture.checkpoint,
        )
        .unwrap();
        let facts = boundary.boundary_facts_v2();
        assert_eq!(facts.run_sha256, fixture.run.run_sha256());
        assert_eq!(
            facts.identity_bundle_sha256,
            fixture.run.identity_bundle_sha256()
        );
        assert_eq!(facts.segment_ordinal, 0);
        assert_eq!(facts.generation_index, 0);
        assert_eq!(facts.batch_episodes, 2);
        assert_eq!(facts.checkpoint_segment_updates, 4);
        assert_eq!(facts.parent_head_sha256, None);
        assert_eq!(facts.last_update_evidence_sha256, None);
        assert_eq!(
            facts.segment_manifest_sha256,
            fixture.segment.segment_manifest_sha256()
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
            facts.checkpoint_sidecar_sha256,
            boundary.checkpoint_sidecar_sha256()
        );
        assert_eq!(facts.head_sha256, boundary.head_sha256());
        assert_eq!(facts.head_record_sha256, boundary.head_record_sha256());
        assert_eq!(
            boundary.checkpoint_sidecar_canonical_bytes(),
            fixture.boundary.checkpoint_sidecar_canonical_bytes()
        );
        assert_eq!(
            boundary.head_record_canonical_bytes(),
            fixture.boundary.head_record_canonical_bytes()
        );

        let sidecar_expected = format!(
            concat!(
                "{{\"batch_episodes\":2,\"checkpoint_manifest_sha256\":\"{}\",",
                "\"checkpoint_payload_sha256\":\"{}\",",
                "\"checkpoint_segment_updates\":4,\"generation_index\":0,",
                "\"identity_bundle_sha256\":\"{}\",",
                "\"last_update_evidence_sha256\":null,",
                "\"logical_state_sha256\":\"{}\",",
                "\"model_parameter_sha256\":\"{}\",",
                "\"parent_head_sha256\":null,\"run_sha256\":\"{}\",",
                "\"schema\":\"mtg_kernel_native_train_checkpoint_sidecar/v2\",",
                "\"segment_manifest_sha256\":\"{}\",\"segment_ordinal\":0,",
                "\"train_state_sha256\":\"{}\"}}\n"
            ),
            lower_hex_raw32_v1(fixture.checkpoint.checkpoint_manifest_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.checkpoint_payload_sha256()),
            fixture.run.identity_bundle_sha256(),
            lower_hex_raw32_v1(fixture.checkpoint.logical_state_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.model_parameter_sha256()),
            fixture.run.run_sha256(),
            lower_hex_raw32_v1(fixture.segment.segment_manifest_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.train_state_sha256()),
        );
        assert_eq!(
            boundary.checkpoint_sidecar_canonical_bytes(),
            sidecar_expected.as_bytes()
        );

        let head_expected = format!(
            concat!(
                "{{\"batch_episodes\":2,\"checkpoint_manifest_sha256\":\"{}\",",
                "\"checkpoint_payload_sha256\":\"{}\",",
                "\"checkpoint_segment_updates\":4,",
                "\"checkpoint_sidecar_sha256\":\"{}\",",
                "\"generation_index\":0,\"head_sha256\":\"{}\",",
                "\"identity_bundle_sha256\":\"{}\",",
                "\"last_update_evidence_sha256\":null,",
                "\"logical_state_sha256\":\"{}\",",
                "\"model_parameter_sha256\":\"{}\",",
                "\"parent_head_sha256\":null,\"run_sha256\":\"{}\",",
                "\"schema\":\"mtg_kernel_native_train_head/v2\",",
                "\"segment_manifest_sha256\":\"{}\",\"segment_ordinal\":0,",
                "\"train_state_sha256\":\"{}\"}}\n"
            ),
            lower_hex_raw32_v1(fixture.checkpoint.checkpoint_manifest_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.checkpoint_payload_sha256()),
            lower_hex_raw32_v1(boundary.checkpoint_sidecar_sha256()),
            lower_hex_raw32_v1(boundary.head_sha256()),
            fixture.run.identity_bundle_sha256(),
            lower_hex_raw32_v1(fixture.checkpoint.logical_state_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.model_parameter_sha256()),
            fixture.run.run_sha256(),
            lower_hex_raw32_v1(fixture.segment.segment_manifest_sha256()),
            lower_hex_raw32_v1(fixture.checkpoint.train_state_sha256()),
        );
        assert_eq!(
            boundary.head_record_canonical_bytes(),
            head_expected.as_bytes()
        );

        let sidecar_reference: [u8; 32] = Sha256::digest(sidecar_expected.as_bytes()).into();
        let head_record_reference: [u8; 32] = Sha256::digest(head_expected.as_bytes()).into();
        assert_eq!(boundary.checkpoint_sidecar_sha256(), sidecar_reference);
        assert_eq!(boundary.head_record_sha256(), head_record_reference);
        assert_eq!(sidecar_value_v2().as_object().unwrap().len(), 15);
        assert_eq!(head_value_v2().as_object().unwrap().len(), 17);
        assert!(
            u64::try_from(boundary.checkpoint_sidecar_canonical_bytes().len()).unwrap()
                <= CHECKPOINT_SIDECAR_MAX_BYTES_V2
        );
        assert!(
            u64::try_from(boundary.head_record_canonical_bytes().len()).unwrap()
                <= HEAD_RECORD_MAX_BYTES_V2
        );
        assert_eq!(
            NATIVE_TRAINING_BOUNDARY_RECORD_CONTRACT_SHA256_V2,
            "53d5e4f8585e28e95870c54407e7a8a6ce6e292d9d85a30ba53197c04cd0ee0d"
        );
        assert_eq!(
            format!("{boundary:?}"),
            "ValidatedNativeTrainingBoundaryV2 { segment_ordinal: 0, generation_index: 0, .. }"
        );
    }

    #[test]
    fn build_output_equals_decoder_output_and_independent_atom_reference() {
        fn atom(reference: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            reference.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(tag.as_bytes());
            reference.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(payload);
        }

        let fixture = fixture_v2();
        let built = build_genesis_native_training_boundary_v2(
            &fixture.run,
            &fixture.segment,
            &fixture.checkpoint,
        )
        .unwrap();
        let decoded = decode_genesis_native_training_boundary_v2(
            built.checkpoint_sidecar_canonical_bytes(),
            built.head_record_canonical_bytes(),
            &fixture.run,
            &fixture.segment,
            &fixture.checkpoint,
        )
        .unwrap();
        assert_eq!(
            built.checkpoint_sidecar_canonical_bytes(),
            decoded.checkpoint_sidecar_canonical_bytes()
        );
        assert_eq!(
            built.head_record_canonical_bytes(),
            decoded.head_record_canonical_bytes()
        );
        assert_eq!(
            built.checkpoint_sidecar_sha256(),
            decoded.checkpoint_sidecar_sha256()
        );
        assert_eq!(built.head_sha256(), decoded.head_sha256());
        assert_eq!(built.head_record_sha256(), decoded.head_record_sha256());

        let mut reference = Vec::new();
        atom(&mut reference, "domain", HEAD_DIGEST_IDENTITY_V2.as_bytes());
        atom(&mut reference, "parent_head_sha256", &[]);
        atom(
            &mut reference,
            "run_sha256",
            &parse_lower_hex_raw32_v1(fixture.run.run_sha256()).unwrap(),
        );
        atom(
            &mut reference,
            "identity_bundle_sha256",
            &parse_lower_hex_raw32_v1(fixture.run.identity_bundle_sha256()).unwrap(),
        );
        atom(
            &mut reference,
            "generation_index_u64be",
            &0_u64.to_be_bytes(),
        );
        atom(
            &mut reference,
            "checkpoint_segment_updates_u64be",
            &fixture.run.checkpoint_segment_updates().to_be_bytes(),
        );
        atom(
            &mut reference,
            "segment_manifest_sha256",
            &fixture.segment.segment_manifest_sha256(),
        );
        atom(
            &mut reference,
            "checkpoint_manifest_sha256",
            &fixture.checkpoint.checkpoint_manifest_sha256(),
        );
        atom(
            &mut reference,
            "checkpoint_payload_sha256",
            &fixture.checkpoint.checkpoint_payload_sha256(),
        );
        atom(
            &mut reference,
            "checkpoint_sidecar_sha256",
            &built.checkpoint_sidecar_sha256(),
        );
        atom(
            &mut reference,
            "logical_state_sha256",
            &fixture.checkpoint.logical_state_sha256(),
        );
        atom(&mut reference, "last_update_evidence_sha256", &[]);
        let independent_head: [u8; 32] = Sha256::digest(&reference).into();
        assert_eq!(built.head_sha256(), independent_head);
    }

    #[test]
    fn genuine_k2_s4_gen4_then_gen8_boundaries_are_lineage_complete() {
        let fixture = trained_boundary_fixture_v2();
        for (parent, segment, checkpoint, boundary, segment_ordinal, generation_index) in [
            (
                &fixture.genesis_boundary,
                &fixture.first_segment,
                &fixture.first_checkpoint,
                &fixture.first_boundary,
                1,
                4,
            ),
            (
                &fixture.first_boundary,
                &fixture.second_segment,
                &fixture.second_checkpoint,
                &fixture.second_boundary,
                2,
                8,
            ),
        ] {
            let decoded = decode_trained_native_training_boundary_v2(
                boundary.checkpoint_sidecar_canonical_bytes(),
                boundary.head_record_canonical_bytes(),
                &fixture.run,
                parent,
                segment,
                checkpoint,
            )
            .unwrap();
            let rebuilt = build_trained_native_training_boundary_v2(
                &fixture.run,
                parent,
                segment,
                checkpoint,
            )
            .unwrap();
            let facts = decoded.boundary_facts_v2();
            assert_eq!(facts.segment_ordinal, segment_ordinal);
            assert_eq!(facts.generation_index, generation_index);
            assert_eq!(facts.batch_episodes, 2);
            assert_eq!(facts.checkpoint_segment_updates, 4);
            assert_eq!(facts.parent_head_sha256, Some(parent.head_sha256()));
            assert!(facts.last_update_evidence_sha256.is_some());
            assert_eq!(
                facts.last_update_evidence_sha256,
                segment.boundary_facts_v2().last_update_evidence_sha256
            );
            assert_eq!(
                facts.segment_manifest_sha256,
                segment.segment_manifest_sha256()
            );
            assert_eq!(
                facts.checkpoint_manifest_sha256,
                checkpoint.checkpoint_manifest_sha256()
            );
            assert_eq!(
                facts.checkpoint_payload_sha256,
                checkpoint.checkpoint_payload_sha256()
            );
            assert_eq!(
                facts.logical_state_sha256,
                checkpoint.logical_state_sha256()
            );
            assert_eq!(
                facts.model_parameter_sha256,
                checkpoint.model_parameter_sha256()
            );
            assert_eq!(facts.train_state_sha256, checkpoint.train_state_sha256());
            assert_eq!(
                decoded.checkpoint_sidecar_sha256(),
                sha256_v1(decoded.checkpoint_sidecar_canonical_bytes())
            );
            assert_eq!(
                decoded.head_record_sha256(),
                sha256_v1(decoded.head_record_canonical_bytes())
            );
            assert_eq!(
                rebuilt.checkpoint_sidecar_canonical_bytes(),
                decoded.checkpoint_sidecar_canonical_bytes()
            );
            assert_eq!(
                rebuilt.head_record_canonical_bytes(),
                decoded.head_record_canonical_bytes()
            );
            assert_eq!(rebuilt.head_sha256(), decoded.head_sha256());
            assert_eq!(rebuilt.head_record_sha256(), decoded.head_record_sha256());

            let sidecar = boundary_sidecar_value_v2(boundary);
            let head = boundary_head_value_v2(boundary);
            assert_eq!(sidecar.as_object().unwrap().len(), 15);
            assert_eq!(head.as_object().unwrap().len(), 17);
            assert_eq!(
                sidecar["parent_head_sha256"],
                json!(lower_hex_raw32_v1(parent.head_sha256()))
            );
            assert_eq!(
                head["parent_head_sha256"],
                json!(lower_hex_raw32_v1(parent.head_sha256()))
            );
            assert_eq!(
                head["checkpoint_sidecar_sha256"],
                json!(lower_hex_raw32_v1(boundary.checkpoint_sidecar_sha256()))
            );
            assert_eq!(
                head["head_sha256"],
                json!(lower_hex_raw32_v1(boundary.head_sha256()))
            );
        }

        let first_facts = fixture.first_boundary.boundary_facts_v2();
        let second_segment_facts = fixture.second_segment.boundary_facts_v2();
        assert_eq!(
            second_segment_facts.parent_head_sha256,
            Some(fixture.first_boundary.head_sha256())
        );
        assert_eq!(
            second_segment_facts.parent_last_update_evidence_sha256,
            first_facts.last_update_evidence_sha256
        );
        assert_eq!(second_segment_facts.parent_generation_index, Some(4));
        assert_ne!(
            fixture.first_boundary.head_sha256(),
            fixture.first_boundary.head_record_sha256()
        );
        assert_ne!(
            fixture.second_boundary.head_sha256(),
            fixture.second_boundary.head_record_sha256()
        );
    }

    #[test]
    fn both_trained_logical_heads_match_independent_atom_references() {
        let fixture = trained_boundary_fixture_v2();
        let first_reference = independent_trained_head_sha256_v2(
            &fixture.run,
            &fixture.genesis_boundary,
            &fixture.first_segment,
            &fixture.first_checkpoint,
            &fixture.first_boundary,
        );
        let second_reference = independent_trained_head_sha256_v2(
            &fixture.run,
            &fixture.first_boundary,
            &fixture.second_segment,
            &fixture.second_checkpoint,
            &fixture.second_boundary,
        );
        assert_eq!(fixture.first_boundary.head_sha256(), first_reference);
        assert_eq!(fixture.second_boundary.head_sha256(), second_reference);
        assert_ne!(first_reference, second_reference);
    }

    #[test]
    fn trained_parent_segment_checkpoint_and_hash_roles_fail_closed() {
        let fixture = trained_boundary_fixture_v2();
        let (alternate_run, alternate_checkpoint, alternate_segment) =
            alternate_genesis_authorities_v2();
        let alternate_boundary = build_genesis_native_training_boundary_v2(
            &alternate_run,
            &alternate_segment,
            &alternate_checkpoint,
        )
        .unwrap();
        assert_ne!(alternate_run.run_sha256(), fixture.run.run_sha256());

        assert_eq!(
            decode_trained_native_training_boundary_v2(
                fixture.first_boundary.checkpoint_sidecar_canonical_bytes(),
                fixture.first_boundary.head_record_canonical_bytes(),
                &fixture.run,
                &alternate_boundary,
                &fixture.first_segment,
                &fixture.first_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::ParentBoundaryBinding
        );
        assert_eq!(
            decode_trained_native_training_boundary_v2(
                fixture.first_boundary.checkpoint_sidecar_canonical_bytes(),
                fixture.first_boundary.head_record_canonical_bytes(),
                &alternate_run,
                &alternate_boundary,
                &fixture.first_segment,
                &fixture.first_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding
        );
        assert_eq!(
            build_trained_native_training_boundary_v2(
                &fixture.run,
                &fixture.genesis_boundary,
                &fixture.second_segment,
                &fixture.first_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::TrainedInvariant
        );
        assert_eq!(
            build_trained_native_training_boundary_v2(
                &fixture.run,
                &fixture.genesis_boundary,
                &fixture.first_segment,
                &fixture.second_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding
        );
        assert_eq!(
            build_trained_native_training_boundary_v2(
                &fixture.run,
                &fixture.genesis_boundary,
                &fixture.genesis_segment,
                &fixture.first_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::TrainedInvariant
        );
        assert_eq!(
            build_trained_native_training_boundary_v2(
                &fixture.run,
                &fixture.genesis_boundary,
                &fixture.first_segment,
                &fixture.genesis_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding
        );

        assert_ne!(
            fixture.genesis_boundary.head_sha256(),
            fixture.genesis_boundary.head_record_sha256()
        );
        let mut sidecar_role_swap = trained_sidecar_value_v2();
        sidecar_role_swap["parent_head_sha256"] = json!(lower_hex_raw32_v1(
            fixture.genesis_boundary.head_record_sha256()
        ));
        assert_eq!(
            decode_trained_sidecar_value_error_v2(&sidecar_role_swap),
            NativeTrainingBoundaryV2ErrorKind::SidecarBinding
        );
        let mut head_role_swap = trained_head_value_v2();
        head_role_swap["parent_head_sha256"] = json!(lower_hex_raw32_v1(
            fixture.genesis_boundary.head_record_sha256()
        ));
        assert_eq!(
            decode_trained_head_value_error_v2(&head_role_swap),
            NativeTrainingBoundaryV2ErrorKind::HeadBinding
        );
        let mut current_head_role_swap = trained_head_value_v2();
        current_head_role_swap["head_sha256"] = json!(lower_hex_raw32_v1(
            fixture.first_boundary.head_record_sha256()
        ));
        assert_eq!(
            decode_trained_head_value_error_v2(&current_head_role_swap),
            NativeTrainingBoundaryV2ErrorKind::LogicalHeadDigestMismatch
        );
    }

    #[test]
    fn every_trained_wire_digest_scalar_and_option_binding_fails_closed() {
        let fixture = trained_boundary_fixture_v2();
        assert_eq!(
            NativeTrainingBoundaryV2ErrorKind::TrainedInvariant.code(),
            "native_training_boundary_v2_trained_invariant"
        );
        assert_eq!(
            NativeTrainingBoundaryV2ErrorKind::ParentBoundaryBinding.code(),
            "native_training_boundary_v2_parent_boundary_binding"
        );
        for (field, replacement) in [
            ("run_sha256", json!("ff".repeat(32))),
            ("identity_bundle_sha256", json!("ff".repeat(32))),
            ("segment_ordinal", json!(2)),
            ("generation_index", json!(8)),
            ("batch_episodes", json!(4)),
            ("checkpoint_segment_updates", json!(2)),
        ] {
            let mut sidecar = trained_sidecar_value_v2();
            sidecar[field] = replacement.clone();
            assert_eq!(
                decode_trained_sidecar_value_error_v2(&sidecar),
                NativeTrainingBoundaryV2ErrorKind::SidecarBinding,
                "sidecar tuple field {field}"
            );
            let mut head = trained_head_value_v2();
            head[field] = replacement;
            assert_eq!(
                decode_trained_head_value_error_v2(&head),
                NativeTrainingBoundaryV2ErrorKind::HeadBinding,
                "head tuple field {field}"
            );
        }

        for field in [
            "segment_manifest_sha256",
            "checkpoint_manifest_sha256",
            "checkpoint_payload_sha256",
            "logical_state_sha256",
            "model_parameter_sha256",
            "train_state_sha256",
        ] {
            let mut sidecar = trained_sidecar_value_v2();
            sidecar[field] = json!("ff".repeat(32));
            assert_eq!(
                decode_trained_sidecar_value_error_v2(&sidecar),
                NativeTrainingBoundaryV2ErrorKind::SidecarBinding,
                "sidecar digest field {field}"
            );
            let mut head = trained_head_value_v2();
            head[field] = json!("ff".repeat(32));
            assert_eq!(
                decode_trained_head_value_error_v2(&head),
                NativeTrainingBoundaryV2ErrorKind::HeadBinding,
                "head digest field {field}"
            );
        }

        for field in ["parent_head_sha256", "last_update_evidence_sha256"] {
            let mut sidecar_null = trained_sidecar_value_v2();
            sidecar_null[field] = Value::Null;
            assert_eq!(
                decode_trained_sidecar_value_error_v2(&sidecar_null),
                NativeTrainingBoundaryV2ErrorKind::TrainedInvariant,
                "sidecar null option {field}"
            );
            let mut head_null = trained_head_value_v2();
            head_null[field] = Value::Null;
            assert_eq!(
                decode_trained_head_value_error_v2(&head_null),
                NativeTrainingBoundaryV2ErrorKind::TrainedInvariant,
                "head null option {field}"
            );

            let mut sidecar_wrong = trained_sidecar_value_v2();
            sidecar_wrong[field] = json!("ff".repeat(32));
            assert_eq!(
                decode_trained_sidecar_value_error_v2(&sidecar_wrong),
                NativeTrainingBoundaryV2ErrorKind::SidecarBinding,
                "sidecar wrong option {field}"
            );
            let mut head_wrong = trained_head_value_v2();
            head_wrong[field] = json!("ff".repeat(32));
            assert_eq!(
                decode_trained_head_value_error_v2(&head_wrong),
                NativeTrainingBoundaryV2ErrorKind::HeadBinding,
                "head wrong option {field}"
            );
        }

        let mut wrong_sidecar_hash = trained_head_value_v2();
        wrong_sidecar_hash["checkpoint_sidecar_sha256"] = json!("ff".repeat(32));
        assert_eq!(
            decode_trained_head_value_error_v2(&wrong_sidecar_hash),
            NativeTrainingBoundaryV2ErrorKind::HeadBinding
        );
        let mut wrong_head_hash = trained_head_value_v2();
        wrong_head_hash["head_sha256"] = json!("ff".repeat(32));
        assert_eq!(
            decode_trained_head_value_error_v2(&wrong_head_hash),
            NativeTrainingBoundaryV2ErrorKind::LogicalHeadDigestMismatch
        );

        let mut malformed = trained_sidecar_value_v2();
        malformed["run_sha256"] = json!("A".repeat(64));
        assert_eq!(
            decode_trained_sidecar_value_error_v2(&malformed),
            NativeTrainingBoundaryV2ErrorKind::InvalidDigest
        );
        let mut over_u63 = trained_head_value_v2();
        over_u63["generation_index"] = json!(U63_MAX_V2 + 1);
        assert_eq!(
            decode_trained_head_value_error_v2(&over_u63),
            NativeTrainingBoundaryV2ErrorKind::InvalidScalar
        );
        assert_eq!(
            checked_u63_add_v2(U63_MAX_V2, 1).unwrap_err().kind(),
            NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic
        );
        assert_eq!(
            checked_u63_mul_v2(U63_MAX_V2, 2).unwrap_err().kind(),
            NativeTrainingBoundaryV2ErrorKind::InvalidArithmetic
        );

        assert_eq!(
            trained_sidecar_value_v2()["last_update_evidence_sha256"],
            json!(lower_hex_raw32_v1(
                fixture
                    .first_segment
                    .boundary_facts_v2()
                    .last_update_evidence_sha256
                    .unwrap()
            ))
        );
    }

    #[test]
    fn trained_canonical_null_schema_and_preparse_caps_fail_closed() {
        let fixture = trained_boundary_fixture_v2();
        let sidecar = String::from_utf8(
            fixture
                .first_boundary
                .checkpoint_sidecar_canonical_bytes()
                .to_vec(),
        )
        .unwrap();
        let head = String::from_utf8(
            fixture
                .first_boundary
                .head_record_canonical_bytes()
                .to_vec(),
        )
        .unwrap();

        for (sidecar_cj, head_cj) in [
            (
                sidecar.replacen(":", ": ", 1).into_bytes(),
                fixture
                    .first_boundary
                    .head_record_canonical_bytes()
                    .to_vec(),
            ),
            (
                fixture
                    .first_boundary
                    .checkpoint_sidecar_canonical_bytes()
                    .to_vec(),
                head.replacen(":", ": ", 1).into_bytes(),
            ),
        ] {
            assert_eq!(
                decode_trained_pair_error_v2(&sidecar_cj, &head_cj),
                NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                    CanonicalJsonErrorKindV1::NonCanonicalBytes
                )
            );
        }
        assert_eq!(
            decode_trained_pair_error_v2(
                &fixture.first_boundary.checkpoint_sidecar_canonical_bytes()[..fixture
                    .first_boundary
                    .checkpoint_sidecar_canonical_bytes()
                    .len()
                    - 1],
                fixture.first_boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::MissingFinalLf
            )
        );
        assert_eq!(
            decode_trained_pair_error_v2(
                fixture.first_boundary.checkpoint_sidecar_canonical_bytes(),
                &fixture.first_boundary.head_record_canonical_bytes()
                    [..fixture.first_boundary.head_record_canonical_bytes().len() - 1],
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::MissingFinalLf
            )
        );

        let duplicate_sidecar = sidecar.replacen(
            "{",
            "{\"schema\":\"mtg_kernel_native_train_checkpoint_sidecar/v2\",",
            1,
        );
        assert_eq!(
            decode_trained_pair_error_v2(
                duplicate_sidecar.as_bytes(),
                fixture.first_boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::DuplicateObjectKey
            )
        );
        let mut unknown_head = trained_head_value_v2();
        unknown_head
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_trained_head_value_error_v2(&unknown_head),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        );

        let forbidden_sidecar_null = sidecar.replacen(
            &format!(
                "\"segment_manifest_sha256\":\"{}\"",
                lower_hex_raw32_v1(fixture.first_segment.segment_manifest_sha256())
            ),
            "\"segment_manifest_sha256\":null",
            1,
        );
        assert_eq!(
            decode_trained_pair_error_v2(
                forbidden_sidecar_null.as_bytes(),
                fixture.first_boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NullForbidden
            )
        );

        let mut sidecar_schema = trained_sidecar_value_v2();
        sidecar_schema["schema"] = json!("mtg_kernel_native_train_checkpoint_sidecar/v1");
        assert_eq!(
            decode_trained_sidecar_value_error_v2(&sidecar_schema),
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema
        );
        let mut head_schema = trained_head_value_v2();
        head_schema["schema"] = json!("mtg_kernel_native_train_head/v1");
        assert_eq!(
            decode_trained_head_value_error_v2(&head_schema),
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema
        );

        let oversized_sidecar =
            vec![b' '; usize::try_from(CHECKPOINT_SIDECAR_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            decode_trained_pair_error_v2(
                &oversized_sidecar,
                fixture.first_boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::SidecarRecordTooLarge
        );
        let oversized_head = vec![b' '; usize::try_from(HEAD_RECORD_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            decode_trained_pair_error_v2(
                fixture.first_boundary.checkpoint_sidecar_canonical_bytes(),
                &oversized_head,
            ),
            NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge
        );
        assert_eq!(
            decode_trained_pair_error_v2(b"not-json", &oversized_head),
            NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge,
            "both trained caps must be checked before either parse"
        );
    }

    #[test]
    fn every_tuple_and_bound_digest_corruption_fails_closed() {
        for (field, replacement) in [
            ("run_sha256", json!("ff".repeat(32))),
            ("identity_bundle_sha256", json!("ff".repeat(32))),
            ("segment_ordinal", json!(1)),
            ("generation_index", json!(1)),
            ("batch_episodes", json!(4)),
            ("checkpoint_segment_updates", json!(2)),
        ] {
            let mut sidecar = sidecar_value_v2();
            sidecar[field] = replacement.clone();
            assert_eq!(
                decode_sidecar_value_error_v2(&sidecar),
                NativeTrainingBoundaryV2ErrorKind::SidecarBinding,
                "sidecar tuple field {field}"
            );

            let mut head = head_value_v2();
            head[field] = replacement;
            assert_eq!(
                decode_head_value_error_v2(&head),
                NativeTrainingBoundaryV2ErrorKind::HeadBinding,
                "head tuple field {field}"
            );
        }

        for field in [
            "segment_manifest_sha256",
            "checkpoint_manifest_sha256",
            "checkpoint_payload_sha256",
            "logical_state_sha256",
            "model_parameter_sha256",
            "train_state_sha256",
        ] {
            let mut sidecar = sidecar_value_v2();
            sidecar[field] = json!("ff".repeat(32));
            assert_eq!(
                decode_sidecar_value_error_v2(&sidecar),
                NativeTrainingBoundaryV2ErrorKind::SidecarBinding,
                "sidecar digest field {field}"
            );

            let mut head = head_value_v2();
            head[field] = json!("ff".repeat(32));
            assert_eq!(
                decode_head_value_error_v2(&head),
                NativeTrainingBoundaryV2ErrorKind::HeadBinding,
                "head digest field {field}"
            );
        }

        let mut wrong_sidecar_hash = head_value_v2();
        wrong_sidecar_hash["checkpoint_sidecar_sha256"] = json!("ff".repeat(32));
        assert_eq!(
            decode_head_value_error_v2(&wrong_sidecar_hash),
            NativeTrainingBoundaryV2ErrorKind::HeadBinding
        );

        let mut wrong_head_hash = head_value_v2();
        wrong_head_hash["head_sha256"] = json!("ff".repeat(32));
        assert_eq!(
            decode_head_value_error_v2(&wrong_head_hash),
            NativeTrainingBoundaryV2ErrorKind::LogicalHeadDigestMismatch
        );

        let mut malformed_sidecar = sidecar_value_v2();
        malformed_sidecar["run_sha256"] = json!("A".repeat(64));
        assert_eq!(
            decode_sidecar_value_error_v2(&malformed_sidecar),
            NativeTrainingBoundaryV2ErrorKind::InvalidDigest
        );
        let mut malformed_head = head_value_v2();
        malformed_head["head_sha256"] = json!("0".repeat(63));
        assert_eq!(
            decode_head_value_error_v2(&malformed_head),
            NativeTrainingBoundaryV2ErrorKind::InvalidDigest
        );
        let mut over_u63 = sidecar_value_v2();
        over_u63["segment_ordinal"] = json!(U63_MAX_V2 + 1);
        assert_eq!(
            decode_sidecar_value_error_v2(&over_u63),
            NativeTrainingBoundaryV2ErrorKind::InvalidScalar
        );
    }

    #[test]
    fn logical_head_and_record_hash_roles_are_not_interchangeable() {
        let fixture = fixture_v2();
        assert_ne!(
            fixture.boundary.head_sha256(),
            fixture.boundary.head_record_sha256()
        );
        assert_ne!(
            fixture.boundary.checkpoint_sidecar_sha256(),
            fixture.boundary.head_sha256()
        );

        let mut head_record_as_logical = head_value_v2();
        head_record_as_logical["head_sha256"] =
            json!(lower_hex_raw32_v1(fixture.boundary.head_record_sha256()));
        assert_eq!(
            decode_head_value_error_v2(&head_record_as_logical),
            NativeTrainingBoundaryV2ErrorKind::LogicalHeadDigestMismatch
        );

        let mut logical_as_sidecar = head_value_v2();
        logical_as_sidecar["checkpoint_sidecar_sha256"] =
            json!(lower_hex_raw32_v1(fixture.boundary.head_sha256()));
        assert_eq!(
            decode_head_value_error_v2(&logical_as_sidecar),
            NativeTrainingBoundaryV2ErrorKind::HeadBinding
        );
    }

    #[test]
    fn exact_null_allowlists_admit_only_the_two_genesis_nulls() {
        for value in [sidecar_value_v2(), head_value_v2()] {
            assert!(value["parent_head_sha256"].is_null());
            assert!(value["last_update_evidence_sha256"].is_null());
        }

        for field in ["parent_head_sha256", "last_update_evidence_sha256"] {
            let mut sidecar = sidecar_value_v2();
            sidecar[field] = json!("00".repeat(32));
            assert_eq!(
                decode_sidecar_value_error_v2(&sidecar),
                NativeTrainingBoundaryV2ErrorKind::GenesisInvariant,
                "sidecar admitted-null field {field}"
            );
            let mut head = head_value_v2();
            head[field] = json!("00".repeat(32));
            assert_eq!(
                decode_head_value_error_v2(&head),
                NativeTrainingBoundaryV2ErrorKind::GenesisInvariant,
                "head admitted-null field {field}"
            );
        }

        let fixture = fixture_v2();
        let sidecar = String::from_utf8(
            fixture
                .boundary
                .checkpoint_sidecar_canonical_bytes()
                .to_vec(),
        )
        .unwrap();
        let forbidden_sidecar_null = sidecar.replacen(
            &format!(
                "\"segment_manifest_sha256\":\"{}\"",
                lower_hex_raw32_v1(fixture.segment.segment_manifest_sha256())
            ),
            "\"segment_manifest_sha256\":null",
            1,
        );
        assert_eq!(
            decode_pair_error_v2(
                forbidden_sidecar_null.as_bytes(),
                fixture.boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NullForbidden
            )
        );

        let head =
            String::from_utf8(fixture.boundary.head_record_canonical_bytes().to_vec()).unwrap();
        let forbidden_head_null = head.replacen(
            &format!(
                "\"checkpoint_sidecar_sha256\":\"{}\"",
                lower_hex_raw32_v1(fixture.boundary.checkpoint_sidecar_sha256())
            ),
            "\"checkpoint_sidecar_sha256\":null",
            1,
        );
        assert_eq!(
            decode_pair_error_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                forbidden_head_null.as_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NullForbidden
            )
        );
    }

    #[test]
    fn canonical_unknown_duplicate_missing_lf_and_caps_fail_closed_for_both_records() {
        let fixture = fixture_v2();
        let sidecar = String::from_utf8(
            fixture
                .boundary
                .checkpoint_sidecar_canonical_bytes()
                .to_vec(),
        )
        .unwrap();
        let head =
            String::from_utf8(fixture.boundary.head_record_canonical_bytes().to_vec()).unwrap();

        let noncanonical_sidecar = sidecar.replacen(":", ": ", 1);
        assert_eq!(
            decode_pair_error_v2(
                noncanonical_sidecar.as_bytes(),
                fixture.boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NonCanonicalBytes
            )
        );
        let noncanonical_head = head.replacen(":", ": ", 1);
        assert_eq!(
            decode_pair_error_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                noncanonical_head.as_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NonCanonicalBytes
            )
        );

        assert_eq!(
            decode_pair_error_v2(
                &fixture.boundary.checkpoint_sidecar_canonical_bytes()
                    [..fixture.boundary.checkpoint_sidecar_canonical_bytes().len() - 1],
                fixture.boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::MissingFinalLf
            )
        );
        assert_eq!(
            decode_pair_error_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                &fixture.boundary.head_record_canonical_bytes()
                    [..fixture.boundary.head_record_canonical_bytes().len() - 1],
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::MissingFinalLf
            )
        );

        let duplicate_sidecar = sidecar.replacen(
            "{",
            "{\"schema\":\"mtg_kernel_native_train_checkpoint_sidecar/v2\",",
            1,
        );
        assert_eq!(
            decode_pair_error_v2(
                duplicate_sidecar.as_bytes(),
                fixture.boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::DuplicateObjectKey
            )
        );
        let duplicate_head =
            head.replacen("{", "{\"schema\":\"mtg_kernel_native_train_head/v2\",", 1);
        assert_eq!(
            decode_pair_error_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                duplicate_head.as_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::DuplicateObjectKey
            )
        );

        let mut unknown_sidecar = sidecar_value_v2();
        unknown_sidecar
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_sidecar_value_error_v2(&unknown_sidecar),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        );
        let mut unknown_head = head_value_v2();
        unknown_head
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_head_value_error_v2(&unknown_head),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        );

        let mut missing_sidecar = sidecar_value_v2();
        missing_sidecar
            .as_object_mut()
            .unwrap()
            .remove("train_state_sha256");
        assert_eq!(
            decode_sidecar_value_error_v2(&missing_sidecar),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        );
        let mut missing_head = head_value_v2();
        missing_head.as_object_mut().unwrap().remove("head_sha256");
        assert_eq!(
            decode_head_value_error_v2(&missing_head),
            NativeTrainingBoundaryV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        );

        let oversized_sidecar =
            vec![b' '; usize::try_from(CHECKPOINT_SIDECAR_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            decode_pair_error_v2(
                &oversized_sidecar,
                fixture.boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::SidecarRecordTooLarge
        );
        let at_sidecar_cap = vec![b' '; usize::try_from(CHECKPOINT_SIDECAR_MAX_BYTES_V2).unwrap()];
        assert_ne!(
            decode_pair_error_v2(
                &at_sidecar_cap,
                fixture.boundary.head_record_canonical_bytes(),
            ),
            NativeTrainingBoundaryV2ErrorKind::SidecarRecordTooLarge,
            "the sidecar byte cap is inclusive"
        );
        let oversized_head = vec![b' '; usize::try_from(HEAD_RECORD_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            decode_pair_error_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                &oversized_head,
            ),
            NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge
        );
        let at_head_cap = vec![b' '; usize::try_from(HEAD_RECORD_MAX_BYTES_V2).unwrap()];
        assert_ne!(
            decode_pair_error_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                &at_head_cap,
            ),
            NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge,
            "the head byte cap is inclusive"
        );
        assert_eq!(
            decode_pair_error_v2(b"not-json", &oversized_head),
            NativeTrainingBoundaryV2ErrorKind::HeadRecordTooLarge,
            "both caps must be checked before either parse"
        );
    }

    #[test]
    fn schemas_and_cross_run_or_mismatched_sealed_authorities_fail_closed() {
        let mut sidecar_schema = sidecar_value_v2();
        sidecar_schema["schema"] = json!("mtg_kernel_native_train_checkpoint_sidecar/v1");
        assert_eq!(
            decode_sidecar_value_error_v2(&sidecar_schema),
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema
        );
        let mut head_schema = head_value_v2();
        head_schema["schema"] = json!("mtg_kernel_native_train_head/v1");
        assert_eq!(
            decode_head_value_error_v2(&head_schema),
            NativeTrainingBoundaryV2ErrorKind::InvalidSchema
        );

        let fixture = fixture_v2();
        let (alternate_run, alternate_checkpoint, alternate_segment) =
            alternate_genesis_authorities_v2();
        assert_ne!(alternate_run.run_sha256(), fixture.run.run_sha256());
        assert_eq!(
            alternate_run.identity_bundle_sha256(),
            fixture.run.identity_bundle_sha256()
        );

        assert_eq!(
            decode_genesis_native_training_boundary_v2(
                fixture.boundary.checkpoint_sidecar_canonical_bytes(),
                fixture.boundary.head_record_canonical_bytes(),
                &alternate_run,
                &alternate_segment,
                &alternate_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::SidecarBinding
        );
        assert_eq!(
            build_genesis_native_training_boundary_v2(
                &fixture.run,
                &alternate_segment,
                &fixture.checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::SegmentBinding
        );
        assert_eq!(
            build_genesis_native_training_boundary_v2(
                &fixture.run,
                &fixture.segment,
                &alternate_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding
        );
    }

    #[test]
    fn production_module_has_no_store_io_or_publish_surface() {
        let production = include_str!("native_training_store_boundary_v2.rs")
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
            "publish_",
            "PersistenceReceipt",
            "parent_chain",
            "Vec<ValidatedNativeTrainingBoundaryV2>",
            "pub struct NativeTrainingBoundaryFactsV2",
        ] {
            assert!(
                !production.contains(forbidden),
                "production source unexpectedly contains {forbidden}"
            );
        }
        for forbidden in ["from_parent_head", "from_parent_facts", "unchecked_parent"] {
            assert!(
                !production.contains(forbidden),
                "production source unexpectedly contains {forbidden}"
            );
        }
        let public_constructors = production
            .lines()
            .filter_map(|line| line.strip_prefix("pub fn "))
            .map(|line| line.split('(').next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            public_constructors,
            vec![
                "build_genesis_native_training_boundary_v2",
                "build_trained_native_training_boundary_v2",
                "decode_genesis_native_training_boundary_v2",
                "decode_trained_native_training_boundary_v2",
            ],
            "no alternate trained construction path may exist"
        );
        assert_eq!(
            production
                .matches("Ok(ValidatedNativeTrainingBoundaryV2 {")
                .count(),
            2,
            "only the genesis and trained decoders may mint boundary authority"
        );
        let trained_derivation = production
            .split("fn derive_expected_trained_facts_v2")
            .nth(1)
            .unwrap()
            .split("fn validate_genesis_checkpoint_v2")
            .next()
            .unwrap();
        assert!(!trained_derivation.contains("head_record_sha256"));
        assert!(production.contains("parent: &ValidatedNativeTrainingBoundaryV2"));
    }
}
