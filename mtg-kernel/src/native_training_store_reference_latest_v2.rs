//! Pure checkpoint-reference-v2 and latest-v2 record authorities.
//!
//! These authorities bind exact canonical record bytes to an already sealed
//! native training boundary. They make no filesystem-location, publication,
//! reopen, latest-last, currentness, or durability claim.

use crate::canonical_json_v1::{
    count_canonical_json_bytes_v1, from_canonical_json_bytes_v1, to_canonical_json_bytes_v1,
    CanonicalJsonClosedMaxErrorV1, CanonicalJsonClosedMaxV1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1, NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::{ValidatedTrainRunV2, NATIVE_TRAINING_STORE_IDENTITY_V2};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const CHECKPOINT_REFERENCE_SCHEMA_V2: &str = "mtg_kernel_native_checkpoint_ref/v2";
pub const LATEST_RECORD_SCHEMA_V2: &str = "mtg_kernel_native_train_latest/v2";
pub const CHECKPOINT_REFERENCE_MAX_BYTES_V2: u64 = 16_384;
pub const LATEST_RECORD_MAX_BYTES_V2: u64 = 65_536;

const U63_MAX_V2: u64 = (1_u64 << 63) - 1;
const MAX_FIXED_DECIMAL_V2: u64 = 99_999_999;

const REFERENCE_PARENT_HEAD_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "parent_head_sha256",
    )];
const REFERENCE_LAST_EVIDENCE_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "last_update_evidence_sha256",
    )];
const REFERENCE_NULL_PATHS_V2: &[&[CanonicalJsonNullPathSegmentV1]] = &[
    REFERENCE_PARENT_HEAD_NULL_PATH_V2,
    REFERENCE_LAST_EVIDENCE_NULL_PATH_V2,
];
const REFERENCE_NULL_POLICY_V2: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(REFERENCE_NULL_PATHS_V2);

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CheckpointReferenceWireV2 {
    schema: String,
    store_identity: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    standalone_semantics_sha256: String,
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    parent_head_sha256: Option<String>,
    head_sha256: String,
    head_record_sha256: String,
    segment_manifest_sha256: String,
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    checkpoint_sidecar_sha256: String,
    logical_state_sha256: String,
    model_parameter_sha256: String,
    train_state_sha256: String,
    last_update_evidence_sha256: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LatestRecordWireV2 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    generation_index: u64,
    checkpoint_segment_updates: u64,
    head_sha256: String,
    head_record_sha256: String,
    checkpoint_ref_sha256: String,
}

pub(crate) fn maximum_trained_checkpoint_reference_cj_bytes_v2(
) -> std::result::Result<u64, CanonicalJsonClosedMaxErrorV1> {
    let u63 = CanonicalJsonClosedMaxV1::max_u63_v1();
    let digest = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(64)?;
    let optional_digest =
        CanonicalJsonClosedMaxV1::choice_v1(CanonicalJsonClosedMaxV1::null_v1(), digest)?;
    CanonicalJsonClosedMaxV1::object_v1(&[
        ("batch_episodes", u63),
        ("checkpoint_manifest_sha256", digest),
        ("checkpoint_payload_sha256", digest),
        ("checkpoint_segment_updates", u63),
        ("checkpoint_sidecar_sha256", digest),
        ("generation_index", u63),
        ("head_record_sha256", digest),
        ("head_sha256", digest),
        ("identity_bundle_sha256", digest),
        ("last_update_evidence_sha256", optional_digest),
        ("logical_state_sha256", digest),
        ("model_parameter_sha256", digest),
        ("parent_head_sha256", optional_digest),
        ("run_sha256", digest),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(CHECKPOINT_REFERENCE_SCHEMA_V2)?,
        ),
        ("segment_manifest_sha256", digest),
        ("segment_ordinal", u63),
        ("standalone_semantics_sha256", digest),
        (
            "store_identity",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(NATIVE_TRAINING_STORE_IDENTITY_V2)?,
        ),
        ("train_state_sha256", digest),
    ])?
    .canonical_document_bytes_v1()
}

pub(crate) fn maximum_latest_record_cj_bytes_v2(
) -> std::result::Result<u64, CanonicalJsonClosedMaxErrorV1> {
    let u63 = CanonicalJsonClosedMaxV1::max_u63_v1();
    let digest = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(64)?;
    CanonicalJsonClosedMaxV1::object_v1(&[
        ("checkpoint_ref_sha256", digest),
        ("checkpoint_segment_updates", u63),
        ("generation_index", u63),
        ("head_record_sha256", digest),
        ("head_sha256", digest),
        ("identity_bundle_sha256", digest),
        ("run_sha256", digest),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(LATEST_RECORD_SCHEMA_V2)?,
        ),
    ])?
    .canonical_document_bytes_v1()
}

struct CheckpointReferenceFactsV2 {
    store_identity: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    standalone_semantics_sha256: String,
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    parent_head_sha256: Option<[u8; 32]>,
    head_sha256: [u8; 32],
    head_record_sha256: [u8; 32],
    segment_manifest_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_sidecar_sha256: [u8; 32],
    logical_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    last_update_evidence_sha256: Option<[u8; 32]>,
}

struct LatestRecordFactsV2 {
    run_sha256: String,
    identity_bundle_sha256: String,
    generation_index: u64,
    checkpoint_segment_updates: u64,
    head_sha256: [u8; 32],
    head_record_sha256: [u8; 32],
    checkpoint_ref_sha256: [u8; 32],
}

/// Fully validated pure CheckpointReferenceV2 authority.
///
/// It is move-only and has no unchecked constructor or serde surface:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedCheckpointReferenceV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<ValidatedCheckpointReferenceV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedCheckpointReferenceV2;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<ValidatedCheckpointReferenceV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedCheckpointReferenceV2;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<ValidatedCheckpointReferenceV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedCheckpointReferenceV2;
/// let _ = ValidatedCheckpointReferenceV2 {};
/// ```
pub struct ValidatedCheckpointReferenceV2 {
    facts: CheckpointReferenceFactsV2,
    canonical_bytes: Vec<u8>,
    checkpoint_ref_sha256: [u8; 32],
}

impl std::fmt::Debug for ValidatedCheckpointReferenceV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedCheckpointReferenceV2")
            .field("segment_ordinal", &self.facts.segment_ordinal)
            .field("generation_index", &self.facts.generation_index)
            .finish_non_exhaustive()
    }
}

impl ValidatedCheckpointReferenceV2 {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub const fn checkpoint_ref_sha256(&self) -> [u8; 32] {
        self.checkpoint_ref_sha256
    }

    pub fn store_identity(&self) -> &str {
        &self.facts.store_identity
    }

    pub fn run_sha256(&self) -> &str {
        &self.facts.run_sha256
    }

    pub fn identity_bundle_sha256(&self) -> &str {
        &self.facts.identity_bundle_sha256
    }

    pub fn standalone_semantics_sha256(&self) -> &str {
        &self.facts.standalone_semantics_sha256
    }

    pub const fn segment_ordinal(&self) -> u64 {
        self.facts.segment_ordinal
    }

    pub const fn generation_index(&self) -> u64 {
        self.facts.generation_index
    }

    pub const fn batch_episodes(&self) -> u64 {
        self.facts.batch_episodes
    }

    pub const fn checkpoint_segment_updates(&self) -> u64 {
        self.facts.checkpoint_segment_updates
    }

    pub const fn parent_head_sha256(&self) -> Option<[u8; 32]> {
        self.facts.parent_head_sha256
    }

    pub const fn head_sha256(&self) -> [u8; 32] {
        self.facts.head_sha256
    }

    pub const fn head_record_sha256(&self) -> [u8; 32] {
        self.facts.head_record_sha256
    }

    pub const fn segment_manifest_sha256(&self) -> [u8; 32] {
        self.facts.segment_manifest_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.facts.checkpoint_manifest_sha256
    }

    pub const fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.facts.checkpoint_payload_sha256
    }

    pub const fn checkpoint_sidecar_sha256(&self) -> [u8; 32] {
        self.facts.checkpoint_sidecar_sha256
    }

    pub const fn logical_state_sha256(&self) -> [u8; 32] {
        self.facts.logical_state_sha256
    }

    pub const fn model_parameter_sha256(&self) -> [u8; 32] {
        self.facts.model_parameter_sha256
    }

    pub const fn train_state_sha256(&self) -> [u8; 32] {
        self.facts.train_state_sha256
    }

    pub const fn last_update_evidence_sha256(&self) -> Option<[u8; 32]> {
        self.facts.last_update_evidence_sha256
    }
}

/// Fully validated pure latest-v2 record authority.
///
/// This proves only exact record equality to the supplied sealed boundary and
/// reference. It does not prove that a Store's `latest.json` points here.
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedLatestRecordV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<ValidatedLatestRecordV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedLatestRecordV2;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<ValidatedLatestRecordV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedLatestRecordV2;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<ValidatedLatestRecordV2>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_reference_latest_v2::ValidatedLatestRecordV2;
/// let _ = ValidatedLatestRecordV2 {};
/// ```
pub struct ValidatedLatestRecordV2 {
    facts: LatestRecordFactsV2,
    canonical_bytes: Vec<u8>,
    latest_record_sha256: [u8; 32],
}

impl std::fmt::Debug for ValidatedLatestRecordV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedLatestRecordV2")
            .field("generation_index", &self.facts.generation_index)
            .finish_non_exhaustive()
    }
}

impl ValidatedLatestRecordV2 {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub const fn latest_record_sha256(&self) -> [u8; 32] {
        self.latest_record_sha256
    }

    pub fn run_sha256(&self) -> &str {
        &self.facts.run_sha256
    }

    pub fn identity_bundle_sha256(&self) -> &str {
        &self.facts.identity_bundle_sha256
    }

    pub const fn generation_index(&self) -> u64 {
        self.facts.generation_index
    }

    pub const fn checkpoint_segment_updates(&self) -> u64 {
        self.facts.checkpoint_segment_updates
    }

    pub const fn head_sha256(&self) -> [u8; 32] {
        self.facts.head_sha256
    }

    pub const fn head_record_sha256(&self) -> [u8; 32] {
        self.facts.head_record_sha256
    }

    pub const fn checkpoint_ref_sha256(&self) -> [u8; 32] {
        self.facts.checkpoint_ref_sha256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingReferenceLatestV2ErrorKind {
    ReferenceRecordTooLarge,
    LatestRecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidStoreIdentity,
    InvalidDigest,
    InvalidScalar,
    InvalidArithmetic,
    RunBinding,
    BoundaryBinding,
    BoundaryCadence,
    ReferenceBinding,
    ReferenceBoundaryBinding,
    LatestBinding,
}

impl NativeTrainingReferenceLatestV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::ReferenceRecordTooLarge => "native_checkpoint_ref_v2_record_too_large",
            Self::LatestRecordTooLarge => "native_train_latest_v2_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_training_reference_latest_v2_invalid_schema",
            Self::InvalidStoreIdentity => {
                "native_training_reference_latest_v2_invalid_store_identity"
            }
            Self::InvalidDigest => "native_training_reference_latest_v2_invalid_digest",
            Self::InvalidScalar => "native_training_reference_latest_v2_invalid_scalar",
            Self::InvalidArithmetic => "native_training_reference_latest_v2_invalid_arithmetic",
            Self::RunBinding => "native_training_reference_latest_v2_run_binding",
            Self::BoundaryBinding => "native_training_reference_latest_v2_boundary_binding",
            Self::BoundaryCadence => "native_training_reference_latest_v2_boundary_cadence",
            Self::ReferenceBinding => "native_training_reference_latest_v2_reference_binding",
            Self::ReferenceBoundaryBinding => {
                "native_training_reference_latest_v2_reference_boundary_binding"
            }
            Self::LatestBinding => "native_training_reference_latest_v2_latest_binding",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingReferenceLatestV2Error {
    kind: NativeTrainingReferenceLatestV2ErrorKind,
}

impl NativeTrainingReferenceLatestV2Error {
    const fn new(kind: NativeTrainingReferenceLatestV2ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> NativeTrainingReferenceLatestV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for NativeTrainingReferenceLatestV2Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
            error.kind(),
        ))
    }
}

impl Display for NativeTrainingReferenceLatestV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingReferenceLatestV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingReferenceLatestV2Error>;

/// Builds an exact checkpoint-reference-v2 candidate from a validated run and
/// one sealed boundary, routing emitted bytes through the public decoder.
pub fn build_checkpoint_reference_v2(
    run: &ValidatedTrainRunV2,
    boundary: &ValidatedNativeTrainingBoundaryV2,
) -> Result<ValidatedCheckpointReferenceV2> {
    let facts = derive_reference_facts_v2(run, boundary)?;
    let wire = reference_wire_v2(&facts);
    let canonical_bytes = encode_with_cap_v2(
        &wire,
        REFERENCE_NULL_POLICY_V2,
        CHECKPOINT_REFERENCE_MAX_BYTES_V2,
        NativeTrainingReferenceLatestV2ErrorKind::ReferenceRecordTooLarge,
    )?;
    decode_checkpoint_reference_v2(&canonical_bytes, run, boundary)
}

/// Decodes an exact checkpoint-reference-v2 record against independent sealed
/// run and boundary authorities.
pub fn decode_checkpoint_reference_v2(
    checkpoint_ref_cj: &[u8],
    run: &ValidatedTrainRunV2,
    boundary: &ValidatedNativeTrainingBoundaryV2,
) -> Result<ValidatedCheckpointReferenceV2> {
    require_cap_v2(
        checkpoint_ref_cj,
        CHECKPOINT_REFERENCE_MAX_BYTES_V2,
        NativeTrainingReferenceLatestV2ErrorKind::ReferenceRecordTooLarge,
    )?;
    let wire: CheckpointReferenceWireV2 =
        from_canonical_json_bytes_v1(checkpoint_ref_cj, REFERENCE_NULL_POLICY_V2)?;
    validate_reference_encodings_v2(&wire)?;
    let facts = derive_reference_facts_v2(run, boundary)?;
    validate_reference_wire_v2(&wire, &facts)?;
    Ok(ValidatedCheckpointReferenceV2 {
        facts,
        canonical_bytes: checkpoint_ref_cj.to_vec(),
        checkpoint_ref_sha256: sha256_v1(checkpoint_ref_cj),
    })
}

/// Builds an exact latest-v2 record candidate from independently sealed
/// boundary and reference authorities, routing bytes through the decoder.
pub fn build_latest_v2(
    boundary: &ValidatedNativeTrainingBoundaryV2,
    reference: &ValidatedCheckpointReferenceV2,
) -> Result<ValidatedLatestRecordV2> {
    let facts = derive_latest_facts_v2(boundary, reference)?;
    let wire = latest_wire_v2(&facts);
    let canonical_bytes = encode_with_cap_v2(
        &wire,
        CanonicalJsonNullPolicyV1::Forbid,
        LATEST_RECORD_MAX_BYTES_V2,
        NativeTrainingReferenceLatestV2ErrorKind::LatestRecordTooLarge,
    )?;
    decode_latest_v2(&canonical_bytes, boundary, reference)
}

/// Parse only the durable generation index out of exact canonical latest
/// bytes, with no binding or authority claim. Resume orchestration uses this
/// to learn the walk target before the full decode proves the pointer.
pub(crate) fn peek_latest_generation_index_v2(
    latest_cj: &[u8],
) -> std::result::Result<u64, NativeTrainingReferenceLatestV2Error> {
    require_cap_v2(
        latest_cj,
        LATEST_RECORD_MAX_BYTES_V2,
        NativeTrainingReferenceLatestV2ErrorKind::LatestRecordTooLarge,
    )?;
    let wire: LatestRecordWireV2 =
        from_canonical_json_bytes_v1(latest_cj, CanonicalJsonNullPolicyV1::Forbid)?;
    if wire.schema != LATEST_RECORD_SCHEMA_V2 {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidSchema,
        ));
    }
    Ok(wire.generation_index)
}

/// Decodes an exact latest-v2 record against independent sealed boundary and
/// reference authorities. This does not establish Store pointer currentness.
pub fn decode_latest_v2(
    latest_cj: &[u8],
    boundary: &ValidatedNativeTrainingBoundaryV2,
    reference: &ValidatedCheckpointReferenceV2,
) -> Result<ValidatedLatestRecordV2> {
    require_cap_v2(
        latest_cj,
        LATEST_RECORD_MAX_BYTES_V2,
        NativeTrainingReferenceLatestV2ErrorKind::LatestRecordTooLarge,
    )?;
    let wire: LatestRecordWireV2 =
        from_canonical_json_bytes_v1(latest_cj, CanonicalJsonNullPolicyV1::Forbid)?;
    validate_latest_encodings_v2(&wire)?;
    let facts = derive_latest_facts_v2(boundary, reference)?;
    validate_latest_wire_v2(&wire, &facts)?;
    Ok(ValidatedLatestRecordV2 {
        facts,
        canonical_bytes: latest_cj.to_vec(),
        latest_record_sha256: sha256_v1(latest_cj),
    })
}

fn derive_reference_facts_v2(
    run: &ValidatedTrainRunV2,
    boundary: &ValidatedNativeTrainingBoundaryV2,
) -> Result<CheckpointReferenceFactsV2> {
    if run.record().artifact_schemas.checkpoint_ref != CHECKPOINT_REFERENCE_SCHEMA_V2
        || run.record().artifact_schemas.latest != LATEST_RECORD_SCHEMA_V2
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidSchema,
        ));
    }
    if run.record().store_identity() != NATIVE_TRAINING_STORE_IDENTITY_V2 {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidStoreIdentity,
        ));
    }
    if run.record().contracts.standalone_semantics.sha256 != run.standalone_semantics_sha256() {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::RunBinding,
        ));
    }
    parse_digest_v2(run.run_sha256())?;
    parse_digest_v2(run.identity_bundle_sha256())?;
    parse_digest_v2(run.standalone_semantics_sha256())?;

    let boundary_facts = boundary.boundary_facts_v2();
    if boundary_facts.run_sha256 != run.run_sha256()
        || boundary_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || boundary_facts.batch_episodes != run.batch_episodes()
        || boundary_facts.checkpoint_segment_updates != run.checkpoint_segment_updates()
        || boundary_facts.checkpoint_sidecar_sha256 != boundary.checkpoint_sidecar_sha256()
        || boundary_facts.head_sha256 != boundary.head_sha256()
        || boundary_facts.head_record_sha256 != boundary.head_record_sha256()
        || sha256_v1(boundary.checkpoint_sidecar_canonical_bytes())
            != boundary_facts.checkpoint_sidecar_sha256
        || sha256_v1(boundary.head_record_canonical_bytes()) != boundary_facts.head_record_sha256
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::BoundaryBinding,
        ));
    }

    let batch_episodes = run.batch_episodes();
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    let requested_successful_updates = run.requested_successful_updates();
    require_positive_u63_v2(batch_episodes)?;
    require_positive_u63_v2(checkpoint_segment_updates)?;
    require_positive_u63_v2(requested_successful_updates)?;
    require_u63_v2(boundary_facts.segment_ordinal)?;
    require_u63_v2(boundary_facts.generation_index)?;
    checked_u63_mul_v2(batch_episodes, checkpoint_segment_updates)?;
    checked_u63_mul_v2(batch_episodes, requested_successful_updates)?;
    let expected_generation =
        checked_u63_mul_v2(boundary_facts.segment_ordinal, checkpoint_segment_updates)?;
    if checkpoint_segment_updates > requested_successful_updates
        || !requested_successful_updates.is_multiple_of(checkpoint_segment_updates)
        || expected_generation != boundary_facts.generation_index
        || boundary_facts.generation_index > requested_successful_updates
        || boundary_facts.generation_index > MAX_FIXED_DECIMAL_V2
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::BoundaryCadence,
        ));
    }
    let is_genesis = boundary_facts.generation_index == 0;
    if (boundary_facts.segment_ordinal == 0) != is_genesis
        || boundary_facts.parent_head_sha256.is_none() != is_genesis
        || boundary_facts.last_update_evidence_sha256.is_none() != is_genesis
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::BoundaryCadence,
        ));
    }

    Ok(CheckpointReferenceFactsV2 {
        store_identity: NATIVE_TRAINING_STORE_IDENTITY_V2.to_owned(),
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        standalone_semantics_sha256: run.standalone_semantics_sha256().to_owned(),
        segment_ordinal: boundary_facts.segment_ordinal,
        generation_index: boundary_facts.generation_index,
        batch_episodes,
        checkpoint_segment_updates,
        parent_head_sha256: boundary_facts.parent_head_sha256,
        head_sha256: boundary_facts.head_sha256,
        head_record_sha256: boundary_facts.head_record_sha256,
        segment_manifest_sha256: boundary_facts.segment_manifest_sha256,
        checkpoint_manifest_sha256: boundary_facts.checkpoint_manifest_sha256,
        checkpoint_payload_sha256: boundary_facts.checkpoint_payload_sha256,
        checkpoint_sidecar_sha256: boundary_facts.checkpoint_sidecar_sha256,
        logical_state_sha256: boundary_facts.logical_state_sha256,
        model_parameter_sha256: boundary_facts.model_parameter_sha256,
        train_state_sha256: boundary_facts.train_state_sha256,
        last_update_evidence_sha256: boundary_facts.last_update_evidence_sha256,
    })
}

fn derive_latest_facts_v2(
    boundary: &ValidatedNativeTrainingBoundaryV2,
    reference: &ValidatedCheckpointReferenceV2,
) -> Result<LatestRecordFactsV2> {
    let boundary_facts = boundary.boundary_facts_v2();
    let reference_facts = &reference.facts;
    if reference_facts.run_sha256 != boundary_facts.run_sha256
        || reference_facts.identity_bundle_sha256 != boundary_facts.identity_bundle_sha256
        || reference_facts.segment_ordinal != boundary_facts.segment_ordinal
        || reference_facts.generation_index != boundary_facts.generation_index
        || reference_facts.batch_episodes != boundary_facts.batch_episodes
        || reference_facts.checkpoint_segment_updates != boundary_facts.checkpoint_segment_updates
        || reference_facts.parent_head_sha256 != boundary_facts.parent_head_sha256
        || reference_facts.head_sha256 != boundary_facts.head_sha256
        || reference_facts.head_record_sha256 != boundary_facts.head_record_sha256
        || reference_facts.segment_manifest_sha256 != boundary_facts.segment_manifest_sha256
        || reference_facts.checkpoint_manifest_sha256 != boundary_facts.checkpoint_manifest_sha256
        || reference_facts.checkpoint_payload_sha256 != boundary_facts.checkpoint_payload_sha256
        || reference_facts.checkpoint_sidecar_sha256 != boundary_facts.checkpoint_sidecar_sha256
        || reference_facts.logical_state_sha256 != boundary_facts.logical_state_sha256
        || reference_facts.model_parameter_sha256 != boundary_facts.model_parameter_sha256
        || reference_facts.train_state_sha256 != boundary_facts.train_state_sha256
        || reference_facts.last_update_evidence_sha256 != boundary_facts.last_update_evidence_sha256
        || sha256_v1(boundary.checkpoint_sidecar_canonical_bytes())
            != boundary_facts.checkpoint_sidecar_sha256
        || sha256_v1(boundary.head_record_canonical_bytes()) != boundary_facts.head_record_sha256
        || sha256_v1(reference.canonical_bytes()) != reference.checkpoint_ref_sha256
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::ReferenceBoundaryBinding,
        ));
    }
    Ok(LatestRecordFactsV2 {
        run_sha256: reference_facts.run_sha256.clone(),
        identity_bundle_sha256: reference_facts.identity_bundle_sha256.clone(),
        generation_index: reference_facts.generation_index,
        checkpoint_segment_updates: reference_facts.checkpoint_segment_updates,
        head_sha256: reference_facts.head_sha256,
        head_record_sha256: reference_facts.head_record_sha256,
        checkpoint_ref_sha256: reference.checkpoint_ref_sha256,
    })
}

fn reference_wire_v2(facts: &CheckpointReferenceFactsV2) -> CheckpointReferenceWireV2 {
    CheckpointReferenceWireV2 {
        schema: CHECKPOINT_REFERENCE_SCHEMA_V2.to_owned(),
        store_identity: facts.store_identity.clone(),
        run_sha256: facts.run_sha256.clone(),
        identity_bundle_sha256: facts.identity_bundle_sha256.clone(),
        standalone_semantics_sha256: facts.standalone_semantics_sha256.clone(),
        segment_ordinal: facts.segment_ordinal,
        generation_index: facts.generation_index,
        batch_episodes: facts.batch_episodes,
        checkpoint_segment_updates: facts.checkpoint_segment_updates,
        parent_head_sha256: facts.parent_head_sha256.map(lower_hex_raw32_v1),
        head_sha256: lower_hex_raw32_v1(facts.head_sha256),
        head_record_sha256: lower_hex_raw32_v1(facts.head_record_sha256),
        segment_manifest_sha256: lower_hex_raw32_v1(facts.segment_manifest_sha256),
        checkpoint_manifest_sha256: lower_hex_raw32_v1(facts.checkpoint_manifest_sha256),
        checkpoint_payload_sha256: lower_hex_raw32_v1(facts.checkpoint_payload_sha256),
        checkpoint_sidecar_sha256: lower_hex_raw32_v1(facts.checkpoint_sidecar_sha256),
        logical_state_sha256: lower_hex_raw32_v1(facts.logical_state_sha256),
        model_parameter_sha256: lower_hex_raw32_v1(facts.model_parameter_sha256),
        train_state_sha256: lower_hex_raw32_v1(facts.train_state_sha256),
        last_update_evidence_sha256: facts.last_update_evidence_sha256.map(lower_hex_raw32_v1),
    }
}

fn latest_wire_v2(facts: &LatestRecordFactsV2) -> LatestRecordWireV2 {
    LatestRecordWireV2 {
        schema: LATEST_RECORD_SCHEMA_V2.to_owned(),
        run_sha256: facts.run_sha256.clone(),
        identity_bundle_sha256: facts.identity_bundle_sha256.clone(),
        generation_index: facts.generation_index,
        checkpoint_segment_updates: facts.checkpoint_segment_updates,
        head_sha256: lower_hex_raw32_v1(facts.head_sha256),
        head_record_sha256: lower_hex_raw32_v1(facts.head_record_sha256),
        checkpoint_ref_sha256: lower_hex_raw32_v1(facts.checkpoint_ref_sha256),
    }
}

fn validate_reference_encodings_v2(wire: &CheckpointReferenceWireV2) -> Result<()> {
    for value in [
        wire.segment_ordinal,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
    ] {
        require_u63_v2(value)?;
    }
    require_positive_u63_v2(wire.batch_episodes)?;
    require_positive_u63_v2(wire.checkpoint_segment_updates)?;
    parse_digest_v2(&wire.run_sha256)?;
    parse_digest_v2(&wire.identity_bundle_sha256)?;
    parse_digest_v2(&wire.standalone_semantics_sha256)?;
    parse_optional_digest_v2(wire.parent_head_sha256.as_deref())?;
    parse_digest_v2(&wire.head_sha256)?;
    parse_digest_v2(&wire.head_record_sha256)?;
    parse_digest_v2(&wire.segment_manifest_sha256)?;
    parse_digest_v2(&wire.checkpoint_manifest_sha256)?;
    parse_digest_v2(&wire.checkpoint_payload_sha256)?;
    parse_digest_v2(&wire.checkpoint_sidecar_sha256)?;
    parse_digest_v2(&wire.logical_state_sha256)?;
    parse_digest_v2(&wire.model_parameter_sha256)?;
    parse_digest_v2(&wire.train_state_sha256)?;
    parse_optional_digest_v2(wire.last_update_evidence_sha256.as_deref())?;
    Ok(())
}

fn validate_reference_wire_v2(
    wire: &CheckpointReferenceWireV2,
    facts: &CheckpointReferenceFactsV2,
) -> Result<()> {
    if wire.schema != CHECKPOINT_REFERENCE_SCHEMA_V2 {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidSchema,
        ));
    }
    if wire.store_identity != facts.store_identity {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidStoreIdentity,
        ));
    }
    if wire.run_sha256 != facts.run_sha256
        || wire.identity_bundle_sha256 != facts.identity_bundle_sha256
        || wire.standalone_semantics_sha256 != facts.standalone_semantics_sha256
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::RunBinding,
        ));
    }
    if wire.segment_ordinal != facts.segment_ordinal
        || wire.generation_index != facts.generation_index
        || wire.batch_episodes != facts.batch_episodes
        || wire.checkpoint_segment_updates != facts.checkpoint_segment_updates
        || parse_optional_digest_v2(wire.parent_head_sha256.as_deref())? != facts.parent_head_sha256
        || parse_digest_v2(&wire.head_sha256)? != facts.head_sha256
        || parse_digest_v2(&wire.head_record_sha256)? != facts.head_record_sha256
        || parse_digest_v2(&wire.segment_manifest_sha256)? != facts.segment_manifest_sha256
        || parse_digest_v2(&wire.checkpoint_manifest_sha256)? != facts.checkpoint_manifest_sha256
        || parse_digest_v2(&wire.checkpoint_payload_sha256)? != facts.checkpoint_payload_sha256
        || parse_digest_v2(&wire.checkpoint_sidecar_sha256)? != facts.checkpoint_sidecar_sha256
        || parse_digest_v2(&wire.logical_state_sha256)? != facts.logical_state_sha256
        || parse_digest_v2(&wire.model_parameter_sha256)? != facts.model_parameter_sha256
        || parse_digest_v2(&wire.train_state_sha256)? != facts.train_state_sha256
        || parse_optional_digest_v2(wire.last_update_evidence_sha256.as_deref())?
            != facts.last_update_evidence_sha256
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding,
        ));
    }
    Ok(())
}

fn validate_latest_encodings_v2(wire: &LatestRecordWireV2) -> Result<()> {
    require_u63_v2(wire.generation_index)?;
    require_positive_u63_v2(wire.checkpoint_segment_updates)?;
    parse_digest_v2(&wire.run_sha256)?;
    parse_digest_v2(&wire.identity_bundle_sha256)?;
    parse_digest_v2(&wire.head_sha256)?;
    parse_digest_v2(&wire.head_record_sha256)?;
    parse_digest_v2(&wire.checkpoint_ref_sha256)?;
    Ok(())
}

fn validate_latest_wire_v2(wire: &LatestRecordWireV2, facts: &LatestRecordFactsV2) -> Result<()> {
    if wire.schema != LATEST_RECORD_SCHEMA_V2 {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidSchema,
        ));
    }
    if wire.run_sha256 != facts.run_sha256
        || wire.identity_bundle_sha256 != facts.identity_bundle_sha256
        || wire.generation_index != facts.generation_index
        || wire.checkpoint_segment_updates != facts.checkpoint_segment_updates
        || parse_digest_v2(&wire.head_sha256)? != facts.head_sha256
        || parse_digest_v2(&wire.head_record_sha256)? != facts.head_record_sha256
        || parse_digest_v2(&wire.checkpoint_ref_sha256)? != facts.checkpoint_ref_sha256
    {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::LatestBinding,
        ));
    }
    Ok(())
}

fn encode_with_cap_v2<T: Serialize + ?Sized>(
    value: &T,
    null_policy: CanonicalJsonNullPolicyV1,
    cap: u64,
    too_large: NativeTrainingReferenceLatestV2ErrorKind,
) -> Result<Vec<u8>> {
    let count = count_canonical_json_bytes_v1(value, null_policy)?;
    if count > cap {
        return Err(NativeTrainingReferenceLatestV2Error::new(too_large));
    }
    let canonical_bytes = to_canonical_json_bytes_v1(value, null_policy)?;
    let emitted = u64::try_from(canonical_bytes.len()).map_err(|_| {
        NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidArithmetic,
        )
    })?;
    if emitted != count {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidArithmetic,
        ));
    }
    Ok(canonical_bytes)
}

fn require_cap_v2(
    bytes: &[u8],
    cap: u64,
    too_large: NativeTrainingReferenceLatestV2ErrorKind,
) -> Result<()> {
    let count = u64::try_from(bytes.len())
        .map_err(|_| NativeTrainingReferenceLatestV2Error::new(too_large))?;
    if count > cap {
        return Err(NativeTrainingReferenceLatestV2Error::new(too_large));
    }
    Ok(())
}

fn parse_digest_v2(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value).map_err(map_digest_error_v2)
}

fn parse_optional_digest_v2(value: Option<&str>) -> Result<Option<[u8; 32]>> {
    value.map(parse_digest_v2).transpose()
}

fn map_digest_error_v2(
    error: NativeTrainingStoreDigestErrorV1,
) -> NativeTrainingReferenceLatestV2Error {
    let kind = match error {
        NativeTrainingStoreDigestErrorV1::InvalidRaw32 => {
            NativeTrainingReferenceLatestV2ErrorKind::InvalidDigest
        }
        NativeTrainingStoreDigestErrorV1::AtomTagLength
        | NativeTrainingStoreDigestErrorV1::AtomPayloadLength => {
            NativeTrainingReferenceLatestV2ErrorKind::InvalidArithmetic
        }
    };
    NativeTrainingReferenceLatestV2Error::new(kind)
}

fn require_u63_v2(value: u64) -> Result<()> {
    if value > U63_MAX_V2 {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidScalar,
        ));
    }
    Ok(())
}

fn require_positive_u63_v2(value: u64) -> Result<()> {
    if value == 0 {
        return Err(NativeTrainingReferenceLatestV2Error::new(
            NativeTrainingReferenceLatestV2ErrorKind::InvalidScalar,
        ));
    }
    require_u63_v2(value)
}

fn checked_u63_mul_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(|| {
            NativeTrainingReferenceLatestV2Error::new(
                NativeTrainingReferenceLatestV2ErrorKind::InvalidArithmetic,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_boundary_v2::{
        build_genesis_native_training_boundary_v2, build_trained_native_training_boundary_v2,
        NativeTrainingBoundaryV2ErrorKind,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
        CheckpointManifestV3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_continuation_v2::{
        build_segment_continuations_v2, ValidatedSegmentContinuationChainAdvanceV2,
    };
    use crate::native_training_store_segment_manifest_v2::{
        build_genesis_segment_manifest_v2, build_trained_segment_manifest_v2, SegmentManifestV2,
    };
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1, decode_update_group_v1,
        UpdateEvidenceChainContextV1, ValidatedUpdateGroupV1,
    };
    use crate::native_training_store_v2::test_persistence_receipt_v2;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use std::time::Duration;

    #[test]
    fn reference_and_latest_closed_maxima_are_frozen() {
        assert_eq!(
            maximum_trained_checkpoint_reference_cj_bytes_v2().unwrap(),
            1_539
        );
        assert_eq!(maximum_latest_record_cj_bytes_v2().unwrap(), 567);
    }

    struct FixtureV2 {
        run: ValidatedTrainRunV2,
        genesis_payload: Vec<u8>,
        boundary: ValidatedNativeTrainingBoundaryV2,
        reference: ValidatedCheckpointReferenceV2,
        latest: ValidatedLatestRecordV2,
    }

    struct AlternateFixtureV2 {
        run: ValidatedTrainRunV2,
        boundary: ValidatedNativeTrainingBoundaryV2,
        reference: ValidatedCheckpointReferenceV2,
    }

    struct TrainedReferenceLatestFixtureV2 {
        run: ValidatedTrainRunV2,
        genesis_checkpoint: CheckpointManifestV3,
        genesis_segment: SegmentManifestV2,
        genesis_boundary: ValidatedNativeTrainingBoundaryV2,
        genesis_reference: ValidatedCheckpointReferenceV2,
        genesis_latest: ValidatedLatestRecordV2,
        first_checkpoint: CheckpointManifestV3,
        first_segment: SegmentManifestV2,
        first_boundary: ValidatedNativeTrainingBoundaryV2,
        first_reference: ValidatedCheckpointReferenceV2,
        first_latest: ValidatedLatestRecordV2,
        second_checkpoint: CheckpointManifestV3,
        second_segment: SegmentManifestV2,
        second_boundary: ValidatedNativeTrainingBoundaryV2,
        second_reference: ValidatedCheckpointReferenceV2,
        second_latest: ValidatedLatestRecordV2,
    }

    struct TrainedReferenceLatestCaseV2<'a> {
        parent: &'a ValidatedNativeTrainingBoundaryV2,
        segment: &'a SegmentManifestV2,
        checkpoint: &'a CheckpointManifestV3,
        boundary: &'a ValidatedNativeTrainingBoundaryV2,
        reference: &'a ValidatedCheckpointReferenceV2,
        latest: &'a ValidatedLatestRecordV2,
        expected_segment_ordinal: u64,
        expected_generation_index: u64,
    }

    static FIXTURE_V2: OnceLock<FixtureV2> = OnceLock::new();
    static ALTERNATE_FIXTURE_V2: OnceLock<AlternateFixtureV2> = OnceLock::new();
    static TRAINED_REFERENCE_LATEST_FIXTURE_V2: OnceLock<TrainedReferenceLatestFixtureV2> =
        OnceLock::new();

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
            let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
            let latest = build_latest_v2(&boundary, &reference).unwrap();
            FixtureV2 {
                run,
                genesis_payload,
                boundary,
                reference,
                latest,
            }
        })
    }

    fn alternate_fixture_v2() -> &'static AlternateFixtureV2 {
        ALTERNATE_FIXTURE_V2.get_or_init(|| {
            let mut run_value: Value =
                serde_json::from_slice(test_fixture_bytes_v2().strip_suffix(b"\n").unwrap())
                    .unwrap();
            let os_build = run_value["runtime"]["os_build"].as_u64().unwrap();
            run_value["runtime"]["os_build"] = json!(os_build.checked_add(1).unwrap());
            let run_bytes =
                to_canonical_json_bytes_v1(&run_value, CanonicalJsonNullPolicyV1::Forbid).unwrap();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let checkpoint =
                build_genesis_checkpoint_manifest_v3(&run, &fixture_v2().genesis_payload).unwrap();
            let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
            let boundary =
                build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
            let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
            AlternateFixtureV2 {
                run,
                boundary,
                reference,
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

    fn trained_reference_latest_fixture_v2() -> &'static TrainedReferenceLatestFixtureV2 {
        TRAINED_REFERENCE_LATEST_FIXTURE_V2.get_or_init(|| {
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
            let genesis_reference = build_checkpoint_reference_v2(&run, &genesis_boundary).unwrap();
            let genesis_latest = build_latest_v2(&genesis_boundary, &genesis_reference).unwrap();

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
                let group_canonical_bytes = group.canonical_bytes().to_vec();
                group_bytes.push(group_canonical_bytes.clone());
                context = advanced;
                let bound =
                    prepared.bind_manifest_bytes_v2(group_canonical_bytes.into_boxed_slice());
                let receipt = test_persistence_receipt_v2(
                    bound.expected_generation_index(),
                    bound.expected_payload_sha256(),
                    bound.expected_manifest_sha256(),
                );
                bound.commit_v2(receipt).unwrap();
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
            let first_reference = build_checkpoint_reference_v2(&run, &first_boundary).unwrap();
            let first_latest = build_latest_v2(&first_boundary, &first_reference).unwrap();

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
            let second_reference = build_checkpoint_reference_v2(&run, &second_boundary).unwrap();
            let second_latest = build_latest_v2(&second_boundary, &second_reference).unwrap();

            TrainedReferenceLatestFixtureV2 {
                run,
                genesis_checkpoint,
                genesis_segment,
                genesis_boundary,
                genesis_reference,
                genesis_latest,
                first_checkpoint,
                first_segment,
                first_boundary,
                first_reference,
                first_latest,
                second_checkpoint,
                second_segment,
                second_boundary,
                second_reference,
                second_latest,
            }
        })
    }

    fn reference_authority_value_v2(reference: &ValidatedCheckpointReferenceV2) -> Value {
        serde_json::from_slice(reference.canonical_bytes().strip_suffix(b"\n").unwrap()).unwrap()
    }

    fn latest_authority_value_v2(latest: &ValidatedLatestRecordV2) -> Value {
        serde_json::from_slice(latest.canonical_bytes().strip_suffix(b"\n").unwrap()).unwrap()
    }

    fn reference_value_v2() -> Value {
        reference_authority_value_v2(&fixture_v2().reference)
    }

    fn latest_value_v2() -> Value {
        latest_authority_value_v2(&fixture_v2().latest)
    }

    fn canonical_reference_value_bytes_v2(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, REFERENCE_NULL_POLICY_V2).unwrap()
    }

    fn canonical_latest_value_bytes_v2(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    fn reference_error_bytes_v2(bytes: &[u8]) -> NativeTrainingReferenceLatestV2ErrorKind {
        let fixture = fixture_v2();
        decode_checkpoint_reference_v2(bytes, &fixture.run, &fixture.boundary)
            .unwrap_err()
            .kind()
    }

    fn reference_error_value_v2(value: &Value) -> NativeTrainingReferenceLatestV2ErrorKind {
        reference_error_bytes_v2(&canonical_reference_value_bytes_v2(value))
    }

    fn latest_error_bytes_v2(bytes: &[u8]) -> NativeTrainingReferenceLatestV2ErrorKind {
        let fixture = fixture_v2();
        decode_latest_v2(bytes, &fixture.boundary, &fixture.reference)
            .unwrap_err()
            .kind()
    }

    fn latest_error_value_v2(value: &Value) -> NativeTrainingReferenceLatestV2ErrorKind {
        latest_error_bytes_v2(&canonical_latest_value_bytes_v2(value))
    }

    fn assert_sealed_boundary_record_roundtrip_v2(
        run: &ValidatedTrainRunV2,
        boundary: &ValidatedNativeTrainingBoundaryV2,
    ) {
        let reference = build_checkpoint_reference_v2(run, boundary).unwrap();
        let decoded_reference =
            decode_checkpoint_reference_v2(reference.canonical_bytes(), run, boundary).unwrap();
        assert_eq!(
            decoded_reference.canonical_bytes(),
            reference.canonical_bytes()
        );
        assert_eq!(
            decoded_reference.checkpoint_ref_sha256(),
            reference.checkpoint_ref_sha256()
        );
        let latest = build_latest_v2(boundary, &reference).unwrap();
        let decoded_latest =
            decode_latest_v2(latest.canonical_bytes(), boundary, &reference).unwrap();
        assert_eq!(decoded_latest.canonical_bytes(), latest.canonical_bytes());
        assert_eq!(
            decoded_latest.latest_record_sha256(),
            latest.latest_record_sha256()
        );
    }

    fn assert_trained_reference_latest_roundtrip_v2(
        run: &ValidatedTrainRunV2,
        case: TrainedReferenceLatestCaseV2<'_>,
    ) {
        let TrainedReferenceLatestCaseV2 {
            parent,
            segment,
            checkpoint,
            boundary,
            reference,
            latest,
            expected_segment_ordinal,
            expected_generation_index,
        } = case;
        let boundary_facts = boundary.boundary_facts_v2();
        let segment_facts = segment.boundary_facts_v2();
        assert_eq!(
            (
                boundary_facts.segment_ordinal,
                boundary_facts.generation_index,
                boundary_facts.batch_episodes,
                boundary_facts.checkpoint_segment_updates,
            ),
            (expected_segment_ordinal, expected_generation_index, 2, 4,)
        );
        assert_eq!(
            (
                reference.segment_ordinal(),
                reference.generation_index(),
                reference.batch_episodes(),
                reference.checkpoint_segment_updates(),
            ),
            (expected_segment_ordinal, expected_generation_index, 2, 4,)
        );
        assert_eq!(
            (
                latest.generation_index(),
                latest.checkpoint_segment_updates()
            ),
            (expected_generation_index, 4)
        );
        assert_eq!(
            reference.store_identity(),
            NATIVE_TRAINING_STORE_IDENTITY_V2
        );
        assert_eq!(reference.run_sha256(), run.run_sha256());
        assert_eq!(
            reference.identity_bundle_sha256(),
            run.identity_bundle_sha256()
        );
        assert_eq!(
            reference.standalone_semantics_sha256(),
            run.standalone_semantics_sha256()
        );
        assert_eq!(reference.parent_head_sha256(), Some(parent.head_sha256()));
        assert_eq!(
            reference.parent_head_sha256(),
            boundary_facts.parent_head_sha256
        );
        assert_eq!(
            reference.last_update_evidence_sha256(),
            segment_facts.last_update_evidence_sha256
        );
        assert_eq!(
            reference.last_update_evidence_sha256(),
            boundary_facts.last_update_evidence_sha256
        );
        assert!(reference.parent_head_sha256().is_some());
        assert!(reference.last_update_evidence_sha256().is_some());

        for (role, actual, expected) in [
            (
                "head_sha256",
                reference.head_sha256(),
                boundary_facts.head_sha256,
            ),
            (
                "head_record_sha256",
                reference.head_record_sha256(),
                boundary_facts.head_record_sha256,
            ),
            (
                "segment_manifest_sha256",
                reference.segment_manifest_sha256(),
                boundary_facts.segment_manifest_sha256,
            ),
            (
                "checkpoint_manifest_sha256",
                reference.checkpoint_manifest_sha256(),
                boundary_facts.checkpoint_manifest_sha256,
            ),
            (
                "checkpoint_payload_sha256",
                reference.checkpoint_payload_sha256(),
                boundary_facts.checkpoint_payload_sha256,
            ),
            (
                "checkpoint_sidecar_sha256",
                reference.checkpoint_sidecar_sha256(),
                boundary_facts.checkpoint_sidecar_sha256,
            ),
            (
                "logical_state_sha256",
                reference.logical_state_sha256(),
                boundary_facts.logical_state_sha256,
            ),
            (
                "model_parameter_sha256",
                reference.model_parameter_sha256(),
                boundary_facts.model_parameter_sha256,
            ),
            (
                "train_state_sha256",
                reference.train_state_sha256(),
                boundary_facts.train_state_sha256,
            ),
        ] {
            assert_eq!(actual, expected, "digest role {role}");
        }
        assert_eq!(
            reference.segment_manifest_sha256(),
            segment.segment_manifest_sha256()
        );
        assert_eq!(
            reference.checkpoint_manifest_sha256(),
            checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(
            reference.checkpoint_payload_sha256(),
            checkpoint.checkpoint_payload_sha256()
        );
        assert_eq!(
            reference.logical_state_sha256(),
            checkpoint.logical_state_sha256()
        );
        assert_eq!(
            reference.model_parameter_sha256(),
            checkpoint.model_parameter_sha256()
        );
        assert_eq!(
            reference.train_state_sha256(),
            checkpoint.train_state_sha256()
        );
        assert_eq!(
            reference.checkpoint_sidecar_sha256(),
            sha256_v1(boundary.checkpoint_sidecar_canonical_bytes())
        );
        assert_eq!(
            reference.head_record_sha256(),
            sha256_v1(boundary.head_record_canonical_bytes())
        );

        let reference_sha: [u8; 32] = Sha256::digest(reference.canonical_bytes()).into();
        assert_eq!(reference.checkpoint_ref_sha256(), reference_sha);
        assert_eq!(latest.checkpoint_ref_sha256(), reference_sha);
        assert_eq!(latest.run_sha256(), run.run_sha256());
        assert_eq!(
            latest.identity_bundle_sha256(),
            run.identity_bundle_sha256()
        );
        assert_eq!(latest.head_sha256(), reference.head_sha256());
        assert_eq!(latest.head_record_sha256(), reference.head_record_sha256());
        assert_ne!(reference.head_sha256(), reference.head_record_sha256());
        assert_ne!(parent.head_sha256(), parent.head_record_sha256());

        let built_reference = build_checkpoint_reference_v2(run, boundary).unwrap();
        assert_eq!(
            built_reference.canonical_bytes(),
            reference.canonical_bytes()
        );
        assert_eq!(built_reference.checkpoint_ref_sha256(), reference_sha);
        let decoded_reference =
            decode_checkpoint_reference_v2(reference.canonical_bytes(), run, boundary).unwrap();
        assert_eq!(
            decoded_reference.canonical_bytes(),
            reference.canonical_bytes()
        );
        assert_eq!(decoded_reference.checkpoint_ref_sha256(), reference_sha);

        let built_latest = build_latest_v2(boundary, reference).unwrap();
        assert_eq!(built_latest.canonical_bytes(), latest.canonical_bytes());
        let decoded_latest =
            decode_latest_v2(latest.canonical_bytes(), boundary, reference).unwrap();
        assert_eq!(decoded_latest.canonical_bytes(), latest.canonical_bytes());
        let latest_sha: [u8; 32] = Sha256::digest(latest.canonical_bytes()).into();
        assert_eq!(built_latest.latest_record_sha256(), latest_sha);
        assert_eq!(decoded_latest.latest_record_sha256(), latest_sha);
        let reference_wire = reference_authority_value_v2(reference);
        assert_eq!(reference_wire.as_object().unwrap().len(), 20);
        assert_eq!(
            reference_wire["segment_ordinal"],
            json!(expected_segment_ordinal)
        );
        assert_eq!(
            reference_wire["generation_index"],
            json!(expected_generation_index)
        );
        assert_eq!(reference_wire["batch_episodes"], json!(2));
        assert_eq!(reference_wire["checkpoint_segment_updates"], json!(4));
        assert_eq!(
            reference_wire["parent_head_sha256"],
            json!(lower_hex_raw32_v1(parent.head_sha256()))
        );
        assert_eq!(
            reference_wire["last_update_evidence_sha256"],
            json!(lower_hex_raw32_v1(
                segment_facts.last_update_evidence_sha256.unwrap()
            ))
        );
        let latest_wire = latest_authority_value_v2(latest);
        assert_eq!(latest_wire.as_object().unwrap().len(), 8);
        assert_eq!(
            latest_wire["generation_index"],
            json!(expected_generation_index)
        );
        assert_eq!(latest_wire["checkpoint_segment_updates"], json!(4));
        assert_eq!(
            latest_wire["checkpoint_ref_sha256"],
            json!(lower_hex_raw32_v1(reference_sha))
        );
    }

    #[test]
    fn genuine_genesis_exact_reference_latest_bytes_hashes_and_roundtrip() {
        let fixture = fixture_v2();
        assert_sealed_boundary_record_roundtrip_v2(&fixture.run, &fixture.boundary);
        let reference = &fixture.reference;
        let latest = &fixture.latest;

        let expected_reference = format!(
            concat!(
                "{{\"batch_episodes\":2,",
                "\"checkpoint_manifest_sha256\":\"{checkpoint_manifest}\",",
                "\"checkpoint_payload_sha256\":\"{checkpoint_payload}\",",
                "\"checkpoint_segment_updates\":4,",
                "\"checkpoint_sidecar_sha256\":\"{checkpoint_sidecar}\",",
                "\"generation_index\":0,",
                "\"head_record_sha256\":\"{head_record}\",",
                "\"head_sha256\":\"{head}\",",
                "\"identity_bundle_sha256\":\"{identity}\",",
                "\"last_update_evidence_sha256\":null,",
                "\"logical_state_sha256\":\"{logical_state}\",",
                "\"model_parameter_sha256\":\"{model_parameter}\",",
                "\"parent_head_sha256\":null,",
                "\"run_sha256\":\"{run}\",",
                "\"schema\":\"mtg_kernel_native_checkpoint_ref/v2\",",
                "\"segment_manifest_sha256\":\"{segment_manifest}\",",
                "\"segment_ordinal\":0,",
                "\"standalone_semantics_sha256\":\"{semantics}\",",
                "\"store_identity\":\"mtg-kernel-native-training-store-v2\",",
                "\"train_state_sha256\":\"{train_state}\"}}\n"
            ),
            checkpoint_manifest = lower_hex_raw32_v1(reference.checkpoint_manifest_sha256()),
            checkpoint_payload = lower_hex_raw32_v1(reference.checkpoint_payload_sha256()),
            checkpoint_sidecar = lower_hex_raw32_v1(reference.checkpoint_sidecar_sha256()),
            head_record = lower_hex_raw32_v1(reference.head_record_sha256()),
            head = lower_hex_raw32_v1(reference.head_sha256()),
            identity = reference.identity_bundle_sha256(),
            logical_state = lower_hex_raw32_v1(reference.logical_state_sha256()),
            model_parameter = lower_hex_raw32_v1(reference.model_parameter_sha256()),
            run = reference.run_sha256(),
            segment_manifest = lower_hex_raw32_v1(reference.segment_manifest_sha256()),
            semantics = reference.standalone_semantics_sha256(),
            train_state = lower_hex_raw32_v1(reference.train_state_sha256()),
        );
        assert_eq!(reference.canonical_bytes(), expected_reference.as_bytes());
        let reference_sha: [u8; 32] = Sha256::digest(expected_reference.as_bytes()).into();
        assert_eq!(reference.checkpoint_ref_sha256(), reference_sha);

        let expected_latest = format!(
            concat!(
                "{{\"checkpoint_ref_sha256\":\"{checkpoint_ref}\",",
                "\"checkpoint_segment_updates\":4,",
                "\"generation_index\":0,",
                "\"head_record_sha256\":\"{head_record}\",",
                "\"head_sha256\":\"{head}\",",
                "\"identity_bundle_sha256\":\"{identity}\",",
                "\"run_sha256\":\"{run}\",",
                "\"schema\":\"mtg_kernel_native_train_latest/v2\"}}\n"
            ),
            checkpoint_ref = lower_hex_raw32_v1(reference_sha),
            head_record = lower_hex_raw32_v1(reference.head_record_sha256()),
            head = lower_hex_raw32_v1(reference.head_sha256()),
            identity = reference.identity_bundle_sha256(),
            run = reference.run_sha256(),
        );
        assert_eq!(latest.canonical_bytes(), expected_latest.as_bytes());
        let latest_sha: [u8; 32] = Sha256::digest(expected_latest.as_bytes()).into();
        assert_eq!(latest.latest_record_sha256(), latest_sha);
        assert_eq!(reference_value_v2().as_object().unwrap().len(), 20);
        assert_eq!(latest_value_v2().as_object().unwrap().len(), 8);
        assert!(reference.canonical_bytes().len() <= CHECKPOINT_REFERENCE_MAX_BYTES_V2 as usize);
        assert!(latest.canonical_bytes().len() <= LATEST_RECORD_MAX_BYTES_V2 as usize);
    }

    #[test]
    fn genuine_k2_s4_gen4_and_gen8_reference_latest_records_are_exact() {
        let fixture = trained_reference_latest_fixture_v2();
        let genesis_segment_facts = fixture.genesis_segment.boundary_facts_v2();
        assert_eq!(fixture.genesis_checkpoint.generation_index(), 0);
        assert_eq!(
            (
                genesis_segment_facts.segment_ordinal,
                genesis_segment_facts.generation_index,
            ),
            (0, 0)
        );
        assert_eq!(fixture.genesis_reference.generation_index(), 0);
        assert_eq!(fixture.genesis_latest.generation_index(), 0);
        assert_sealed_boundary_record_roundtrip_v2(&fixture.run, &fixture.genesis_boundary);

        for case in [
            TrainedReferenceLatestCaseV2 {
                parent: &fixture.genesis_boundary,
                segment: &fixture.first_segment,
                checkpoint: &fixture.first_checkpoint,
                boundary: &fixture.first_boundary,
                reference: &fixture.first_reference,
                latest: &fixture.first_latest,
                expected_segment_ordinal: 1,
                expected_generation_index: 4,
            },
            TrainedReferenceLatestCaseV2 {
                parent: &fixture.first_boundary,
                segment: &fixture.second_segment,
                checkpoint: &fixture.second_checkpoint,
                boundary: &fixture.second_boundary,
                reference: &fixture.second_reference,
                latest: &fixture.second_latest,
                expected_segment_ordinal: 2,
                expected_generation_index: 8,
            },
        ] {
            assert_trained_reference_latest_roundtrip_v2(&fixture.run, case);
        }
    }

    #[test]
    fn same_run_cross_generation_records_are_rejected_in_both_directions() {
        let fixture = trained_reference_latest_fixture_v2();
        let generations = [
            (
                &fixture.genesis_boundary,
                &fixture.genesis_reference,
                &fixture.genesis_latest,
                0,
            ),
            (
                &fixture.first_boundary,
                &fixture.first_reference,
                &fixture.first_latest,
                4,
            ),
            (
                &fixture.second_boundary,
                &fixture.second_reference,
                &fixture.second_latest,
                8,
            ),
        ];
        for (
            source_index,
            &(source_boundary, source_reference, source_latest, source_generation),
        ) in generations.iter().enumerate()
        {
            assert_eq!(
                source_boundary.boundary_facts_v2().generation_index,
                source_generation
            );
            for (target_index, &(target_boundary, target_reference, _, target_generation)) in
                generations.iter().enumerate()
            {
                if source_index == target_index {
                    continue;
                }
                assert_ne!(source_generation, target_generation);
                assert_eq!(
                    decode_checkpoint_reference_v2(
                        source_reference.canonical_bytes(),
                        &fixture.run,
                        target_boundary,
                    )
                    .unwrap_err()
                    .kind(),
                    NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding
                );
                assert_eq!(
                    build_latest_v2(target_boundary, source_reference)
                        .unwrap_err()
                        .kind(),
                    NativeTrainingReferenceLatestV2ErrorKind::ReferenceBoundaryBinding
                );
                assert_eq!(
                    decode_latest_v2(
                        source_latest.canonical_bytes(),
                        target_boundary,
                        target_reference,
                    )
                    .unwrap_err()
                    .kind(),
                    NativeTrainingReferenceLatestV2ErrorKind::LatestBinding
                );
            }
        }
    }

    #[test]
    fn trained_null_and_digest_role_swaps_fail_closed() {
        let fixture = trained_reference_latest_fixture_v2();
        for (parent, boundary, reference, latest) in [
            (
                &fixture.genesis_boundary,
                &fixture.first_boundary,
                &fixture.first_reference,
                &fixture.first_latest,
            ),
            (
                &fixture.first_boundary,
                &fixture.second_boundary,
                &fixture.second_reference,
                &fixture.second_latest,
            ),
        ] {
            for field in [
                "run_sha256",
                "identity_bundle_sha256",
                "standalone_semantics_sha256",
                "parent_head_sha256",
                "head_sha256",
                "head_record_sha256",
                "segment_manifest_sha256",
                "checkpoint_manifest_sha256",
                "checkpoint_payload_sha256",
                "checkpoint_sidecar_sha256",
                "logical_state_sha256",
                "model_parameter_sha256",
                "train_state_sha256",
                "last_update_evidence_sha256",
            ] {
                let mut value = reference_authority_value_v2(reference);
                assert_ne!(value[field], json!("ff".repeat(32)));
                value[field] = json!("ff".repeat(32));
                assert!(
                    decode_checkpoint_reference_v2(
                        &canonical_reference_value_bytes_v2(&value),
                        &fixture.run,
                        boundary,
                    )
                    .is_err(),
                    "trained reference digest field {field}"
                );
            }
            for field in [
                "run_sha256",
                "identity_bundle_sha256",
                "head_sha256",
                "head_record_sha256",
                "checkpoint_ref_sha256",
            ] {
                let mut value = latest_authority_value_v2(latest);
                assert_ne!(value[field], json!("ff".repeat(32)));
                value[field] = json!("ff".repeat(32));
                assert!(
                    decode_latest_v2(
                        &canonical_latest_value_bytes_v2(&value),
                        boundary,
                        reference,
                    )
                    .is_err(),
                    "trained latest digest field {field}"
                );
            }
            for field in ["parent_head_sha256", "last_update_evidence_sha256"] {
                let mut value = reference_authority_value_v2(reference);
                assert!(!value[field].is_null());
                value[field] = Value::Null;
                assert_eq!(
                    decode_checkpoint_reference_v2(
                        &canonical_reference_value_bytes_v2(&value),
                        &fixture.run,
                        boundary,
                    )
                    .unwrap_err()
                    .kind(),
                    NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding,
                    "trained null field {field}"
                );
            }

            assert_ne!(parent.head_sha256(), parent.head_record_sha256());
            let mut parent_role_swap = reference_authority_value_v2(reference);
            parent_role_swap["parent_head_sha256"] =
                json!(lower_hex_raw32_v1(parent.head_record_sha256()));
            assert_eq!(
                decode_checkpoint_reference_v2(
                    &canonical_reference_value_bytes_v2(&parent_role_swap),
                    &fixture.run,
                    boundary,
                )
                .unwrap_err()
                .kind(),
                NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding
            );

            assert_ne!(reference.head_sha256(), reference.head_record_sha256());
            let mut reference_role_swap = reference_authority_value_v2(reference);
            let logical_head = reference_role_swap["head_sha256"].clone();
            let head_record = reference_role_swap["head_record_sha256"].clone();
            reference_role_swap["head_sha256"] = head_record;
            reference_role_swap["head_record_sha256"] = logical_head;
            assert_eq!(
                decode_checkpoint_reference_v2(
                    &canonical_reference_value_bytes_v2(&reference_role_swap),
                    &fixture.run,
                    boundary,
                )
                .unwrap_err()
                .kind(),
                NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding
            );

            assert_ne!(
                reference.checkpoint_sidecar_sha256(),
                reference.head_record_sha256()
            );
            let mut sidecar_role_swap = reference_authority_value_v2(reference);
            let sidecar = sidecar_role_swap["checkpoint_sidecar_sha256"].clone();
            let head_record = sidecar_role_swap["head_record_sha256"].clone();
            sidecar_role_swap["checkpoint_sidecar_sha256"] = head_record;
            sidecar_role_swap["head_record_sha256"] = sidecar;
            assert_eq!(
                decode_checkpoint_reference_v2(
                    &canonical_reference_value_bytes_v2(&sidecar_role_swap),
                    &fixture.run,
                    boundary,
                )
                .unwrap_err()
                .kind(),
                NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding
            );

            let mut latest_role_swap = latest_authority_value_v2(latest);
            let logical_head = latest_role_swap["head_sha256"].clone();
            let head_record = latest_role_swap["head_record_sha256"].clone();
            latest_role_swap["head_sha256"] = head_record;
            latest_role_swap["head_record_sha256"] = logical_head;
            assert_eq!(
                decode_latest_v2(
                    &canonical_latest_value_bytes_v2(&latest_role_swap),
                    boundary,
                    reference,
                )
                .unwrap_err()
                .kind(),
                NativeTrainingReferenceLatestV2ErrorKind::LatestBinding
            );

            assert_ne!(
                reference.checkpoint_ref_sha256(),
                reference.head_record_sha256()
            );
            let mut reference_file_role_swap = latest_authority_value_v2(latest);
            reference_file_role_swap["checkpoint_ref_sha256"] =
                json!(lower_hex_raw32_v1(reference.head_record_sha256()));
            assert_eq!(
                decode_latest_v2(
                    &canonical_latest_value_bytes_v2(&reference_file_role_swap),
                    boundary,
                    reference,
                )
                .unwrap_err()
                .kind(),
                NativeTrainingReferenceLatestV2ErrorKind::LatestBinding
            );
        }
    }

    #[test]
    fn generation_eight_requires_the_sealed_generation_four_parent() {
        let fixture = trained_reference_latest_fixture_v2();
        let first_facts = fixture.first_boundary.boundary_facts_v2();
        let second_segment_facts = fixture.second_segment.boundary_facts_v2();
        assert_eq!(first_facts.generation_index, 4);
        assert_eq!(
            fixture.second_boundary.boundary_facts_v2().generation_index,
            8
        );
        assert_eq!(
            second_segment_facts.parent_generation_index,
            Some(first_facts.generation_index)
        );
        assert_eq!(
            second_segment_facts.parent_head_sha256,
            Some(fixture.first_boundary.head_sha256())
        );
        assert_eq!(
            second_segment_facts.parent_last_update_evidence_sha256,
            first_facts.last_update_evidence_sha256
        );
        assert_eq!(
            build_trained_native_training_boundary_v2(
                &fixture.run,
                &fixture.genesis_boundary,
                &fixture.second_segment,
                &fixture.second_checkpoint,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingBoundaryV2ErrorKind::CheckpointBinding
        );
        let rebuilt = build_trained_native_training_boundary_v2(
            &fixture.run,
            &fixture.first_boundary,
            &fixture.second_segment,
            &fixture.second_checkpoint,
        )
        .unwrap();
        assert_eq!(
            rebuilt.checkpoint_sidecar_canonical_bytes(),
            fixture.second_boundary.checkpoint_sidecar_canonical_bytes()
        );
        assert_eq!(
            rebuilt.head_record_canonical_bytes(),
            fixture.second_boundary.head_record_canonical_bytes()
        );
    }

    #[test]
    fn every_reference_root_field_corruption_fails_closed() {
        let corruptions = [
            ("schema", json!("mtg_kernel_native_checkpoint_ref/v1")),
            ("store_identity", json!("other-store")),
            ("run_sha256", json!("ff".repeat(32))),
            ("identity_bundle_sha256", json!("ff".repeat(32))),
            ("standalone_semantics_sha256", json!("ff".repeat(32))),
            ("segment_ordinal", json!(1)),
            ("generation_index", json!(4)),
            ("batch_episodes", json!(4)),
            ("checkpoint_segment_updates", json!(2)),
            ("parent_head_sha256", json!("ff".repeat(32))),
            ("head_sha256", json!("ff".repeat(32))),
            ("head_record_sha256", json!("ff".repeat(32))),
            ("segment_manifest_sha256", json!("ff".repeat(32))),
            ("checkpoint_manifest_sha256", json!("ff".repeat(32))),
            ("checkpoint_payload_sha256", json!("ff".repeat(32))),
            ("checkpoint_sidecar_sha256", json!("ff".repeat(32))),
            ("logical_state_sha256", json!("ff".repeat(32))),
            ("model_parameter_sha256", json!("ff".repeat(32))),
            ("train_state_sha256", json!("ff".repeat(32))),
            ("last_update_evidence_sha256", json!("ff".repeat(32))),
        ];
        for (field, replacement) in corruptions {
            let mut value = reference_value_v2();
            value[field] = replacement;
            assert!(
                decode_checkpoint_reference_v2(
                    &canonical_reference_value_bytes_v2(&value),
                    &fixture_v2().run,
                    &fixture_v2().boundary,
                )
                .is_err(),
                "field {field}"
            );
        }
    }

    #[test]
    fn every_latest_root_field_corruption_fails_closed() {
        for (field, replacement) in [
            ("schema", json!("mtg_kernel_native_train_latest/v1")),
            ("run_sha256", json!("ff".repeat(32))),
            ("identity_bundle_sha256", json!("ff".repeat(32))),
            ("generation_index", json!(4)),
            ("checkpoint_segment_updates", json!(2)),
            ("head_sha256", json!("ff".repeat(32))),
            ("head_record_sha256", json!("ff".repeat(32))),
            ("checkpoint_ref_sha256", json!("ff".repeat(32))),
        ] {
            let mut value = latest_value_v2();
            value[field] = replacement;
            assert!(
                decode_latest_v2(
                    &canonical_latest_value_bytes_v2(&value),
                    &fixture_v2().boundary,
                    &fixture_v2().reference,
                )
                .is_err(),
                "field {field}"
            );
        }
    }

    #[test]
    fn logical_head_record_and_reference_file_roles_never_interchange() {
        let fixture = fixture_v2();
        assert_ne!(
            fixture.reference.head_sha256(),
            fixture.reference.head_record_sha256()
        );
        assert_ne!(
            fixture.reference.head_sha256(),
            fixture.reference.checkpoint_ref_sha256()
        );
        assert_ne!(
            fixture.reference.head_record_sha256(),
            fixture.reference.checkpoint_ref_sha256()
        );

        let mut reference = reference_value_v2();
        reference["head_sha256"] =
            json!(lower_hex_raw32_v1(fixture.reference.head_record_sha256()));
        reference["head_record_sha256"] =
            json!(lower_hex_raw32_v1(fixture.reference.head_sha256()));
        assert_eq!(
            reference_error_value_v2(&reference),
            NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding
        );

        let mut latest = latest_value_v2();
        latest["head_sha256"] = json!(lower_hex_raw32_v1(fixture.reference.head_record_sha256()));
        latest["head_record_sha256"] = json!(lower_hex_raw32_v1(fixture.reference.head_sha256()));
        assert_eq!(
            latest_error_value_v2(&latest),
            NativeTrainingReferenceLatestV2ErrorKind::LatestBinding
        );
        let mut latest = latest_value_v2();
        latest["checkpoint_ref_sha256"] =
            json!(lower_hex_raw32_v1(fixture.reference.head_record_sha256()));
        assert_eq!(
            latest_error_value_v2(&latest),
            NativeTrainingReferenceLatestV2ErrorKind::LatestBinding
        );
    }

    #[test]
    fn canonical_duplicate_bom_lf_unknown_missing_null_and_caps_fail_closed() {
        let fixture = fixture_v2();
        for canonical in [
            fixture.reference.canonical_bytes(),
            fixture.latest.canonical_bytes(),
        ] {
            assert!(canonical.ends_with(b"\n"));
        }

        let reference_text =
            String::from_utf8(fixture.reference.canonical_bytes().to_vec()).unwrap();
        let noncanonical_reference = reference_text.replacen(":", ": ", 1);
        assert!(matches!(
            reference_error_bytes_v2(noncanonical_reference.as_bytes()),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        assert!(matches!(
            reference_error_bytes_v2(
                &fixture.reference.canonical_bytes()
                    [..fixture.reference.canonical_bytes().len() - 1]
            ),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        let mut extra_lf = fixture.reference.canonical_bytes().to_vec();
        extra_lf.push(b'\n');
        assert!(matches!(
            reference_error_bytes_v2(&extra_lf),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        let mut bom = vec![0xef, 0xbb, 0xbf];
        bom.extend_from_slice(fixture.reference.canonical_bytes());
        assert!(matches!(
            reference_error_bytes_v2(&bom),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        let duplicate_reference = reference_text.replacen(
            "{",
            "{\"schema\":\"mtg_kernel_native_checkpoint_ref/v2\",",
            1,
        );
        assert!(matches!(
            reference_error_bytes_v2(duplicate_reference.as_bytes()),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::DuplicateObjectKey
            )
        ));

        let latest_text = String::from_utf8(fixture.latest.canonical_bytes().to_vec()).unwrap();
        let noncanonical_latest = latest_text.replacen(":", ": ", 1);
        assert!(matches!(
            latest_error_bytes_v2(noncanonical_latest.as_bytes()),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        assert!(matches!(
            latest_error_bytes_v2(
                &fixture.latest.canonical_bytes()[..fixture.latest.canonical_bytes().len() - 1]
            ),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        let mut latest_extra_lf = fixture.latest.canonical_bytes().to_vec();
        latest_extra_lf.push(b'\n');
        assert!(matches!(
            latest_error_bytes_v2(&latest_extra_lf),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        let mut latest_bom = vec![0xef, 0xbb, 0xbf];
        latest_bom.extend_from_slice(fixture.latest.canonical_bytes());
        assert!(matches!(
            latest_error_bytes_v2(&latest_bom),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(_)
        ));
        let duplicate_latest =
            latest_text.replacen("{", "{\"schema\":\"mtg_kernel_native_train_latest/v2\",", 1);
        assert!(matches!(
            latest_error_bytes_v2(duplicate_latest.as_bytes()),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::DuplicateObjectKey
            )
        ));

        let mut unknown_reference = reference_value_v2();
        unknown_reference["unknown"] = json!(1);
        assert!(matches!(
            reference_error_value_v2(&unknown_reference),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        ));
        let mut missing_reference = reference_value_v2();
        missing_reference
            .as_object_mut()
            .unwrap()
            .remove("head_sha256");
        assert!(matches!(
            reference_error_value_v2(&missing_reference),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        ));
        let mut unknown_latest = latest_value_v2();
        unknown_latest["unknown"] = json!(1);
        assert!(matches!(
            latest_error_value_v2(&unknown_latest),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        ));
        let mut missing_latest = latest_value_v2();
        missing_latest
            .as_object_mut()
            .unwrap()
            .remove("checkpoint_ref_sha256");
        assert!(matches!(
            latest_error_value_v2(&missing_latest),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::Deserialization
            )
        ));

        let forbidden_reference_null = reference_text.replacen(
            &format!(
                "\"head_sha256\":\"{}\"",
                lower_hex_raw32_v1(fixture.reference.head_sha256())
            ),
            "\"head_sha256\":null",
            1,
        );
        assert!(matches!(
            reference_error_bytes_v2(forbidden_reference_null.as_bytes()),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NullForbidden
            )
        ));
        let forbidden_latest_null = latest_text.replacen(
            &format!(
                "\"head_sha256\":\"{}\"",
                lower_hex_raw32_v1(fixture.latest.head_sha256())
            ),
            "\"head_sha256\":null",
            1,
        );
        assert!(matches!(
            latest_error_bytes_v2(forbidden_latest_null.as_bytes()),
            NativeTrainingReferenceLatestV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NullForbidden
            )
        ));

        let oversized_reference =
            vec![b' '; usize::try_from(CHECKPOINT_REFERENCE_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            reference_error_bytes_v2(&oversized_reference),
            NativeTrainingReferenceLatestV2ErrorKind::ReferenceRecordTooLarge
        );
        let exact_cap_reference =
            vec![b' '; usize::try_from(CHECKPOINT_REFERENCE_MAX_BYTES_V2).unwrap()];
        assert_ne!(
            reference_error_bytes_v2(&exact_cap_reference),
            NativeTrainingReferenceLatestV2ErrorKind::ReferenceRecordTooLarge
        );
        let oversized_latest = vec![b' '; usize::try_from(LATEST_RECORD_MAX_BYTES_V2).unwrap() + 1];
        assert_eq!(
            latest_error_bytes_v2(&oversized_latest),
            NativeTrainingReferenceLatestV2ErrorKind::LatestRecordTooLarge
        );
        let exact_cap_latest = vec![b' '; usize::try_from(LATEST_RECORD_MAX_BYTES_V2).unwrap()];
        assert_ne!(
            latest_error_bytes_v2(&exact_cap_latest),
            NativeTrainingReferenceLatestV2ErrorKind::LatestRecordTooLarge
        );
    }

    #[test]
    fn strict_digest_u63_cadence_store_semantics_and_genesis_nulls_fail_closed() {
        let value = reference_value_v2();
        assert!(value["parent_head_sha256"].is_null());
        assert!(value["last_update_evidence_sha256"].is_null());
        for (field, replacement) in [
            ("run_sha256", json!("A".repeat(64))),
            ("identity_bundle_sha256", json!("0".repeat(63))),
            ("head_sha256", json!("gg".repeat(32))),
        ] {
            let mut value = reference_value_v2();
            value[field] = replacement;
            assert_eq!(
                reference_error_value_v2(&value),
                NativeTrainingReferenceLatestV2ErrorKind::InvalidDigest,
                "field {field}"
            );
        }
        let mut over_u63 = reference_value_v2();
        over_u63["generation_index"] = json!(U63_MAX_V2 + 1);
        assert_eq!(
            reference_error_value_v2(&over_u63),
            NativeTrainingReferenceLatestV2ErrorKind::InvalidScalar
        );
        let mut zero_s = reference_value_v2();
        zero_s["checkpoint_segment_updates"] = json!(0);
        assert_eq!(
            reference_error_value_v2(&zero_s),
            NativeTrainingReferenceLatestV2ErrorKind::InvalidScalar
        );
        for (field, replacement) in [
            ("checkpoint_ref_sha256", json!("A".repeat(64))),
            ("head_record_sha256", json!("0".repeat(63))),
            ("head_sha256", json!("gg".repeat(32))),
        ] {
            let mut value = latest_value_v2();
            value[field] = replacement;
            assert_eq!(
                latest_error_value_v2(&value),
                NativeTrainingReferenceLatestV2ErrorKind::InvalidDigest,
                "field {field}"
            );
        }
        let mut latest_over_u63 = latest_value_v2();
        latest_over_u63["generation_index"] = json!(U63_MAX_V2 + 1);
        assert_eq!(
            latest_error_value_v2(&latest_over_u63),
            NativeTrainingReferenceLatestV2ErrorKind::InvalidScalar
        );
        let mut latest_zero_s = latest_value_v2();
        latest_zero_s["checkpoint_segment_updates"] = json!(0);
        assert_eq!(
            latest_error_value_v2(&latest_zero_s),
            NativeTrainingReferenceLatestV2ErrorKind::InvalidScalar
        );
        let mut wrong_store = reference_value_v2();
        wrong_store["store_identity"] = json!("mtg-kernel-native-training-store-v1");
        assert_eq!(
            reference_error_value_v2(&wrong_store),
            NativeTrainingReferenceLatestV2ErrorKind::InvalidStoreIdentity
        );
        let mut wrong_semantics = reference_value_v2();
        wrong_semantics["standalone_semantics_sha256"] = json!("ff".repeat(32));
        assert_eq!(
            reference_error_value_v2(&wrong_semantics),
            NativeTrainingReferenceLatestV2ErrorKind::RunBinding
        );
        for field in ["parent_head_sha256", "last_update_evidence_sha256"] {
            let mut nonnull_genesis = reference_value_v2();
            nonnull_genesis[field] = json!("ff".repeat(32));
            assert_eq!(
                reference_error_value_v2(&nonnull_genesis),
                NativeTrainingReferenceLatestV2ErrorKind::ReferenceBinding,
                "field {field}"
            );
        }
        assert_eq!(
            checked_u63_mul_v2(U63_MAX_V2, 2).unwrap_err().kind(),
            NativeTrainingReferenceLatestV2ErrorKind::InvalidArithmetic
        );
    }

    #[test]
    fn cross_run_boundary_and_reference_authorities_fail_closed() {
        let fixture = fixture_v2();
        let alternate = alternate_fixture_v2();
        assert_ne!(fixture.run.run_sha256(), alternate.run.run_sha256());
        assert_eq!(
            decode_checkpoint_reference_v2(
                fixture.reference.canonical_bytes(),
                &alternate.run,
                &alternate.boundary,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingReferenceLatestV2ErrorKind::RunBinding
        );
        assert_eq!(
            build_latest_v2(&alternate.boundary, &fixture.reference)
                .unwrap_err()
                .kind(),
            NativeTrainingReferenceLatestV2ErrorKind::ReferenceBoundaryBinding
        );
        assert_eq!(
            decode_latest_v2(
                fixture.latest.canonical_bytes(),
                &alternate.boundary,
                &alternate.reference,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingReferenceLatestV2ErrorKind::LatestBinding
        );
    }

    #[test]
    fn public_getters_are_exact_safe_record_facts() {
        let fixture = fixture_v2();
        let boundary = fixture.boundary.boundary_facts_v2();
        let reference = &fixture.reference;
        assert_eq!(
            reference.store_identity(),
            NATIVE_TRAINING_STORE_IDENTITY_V2
        );
        assert_eq!(reference.run_sha256(), boundary.run_sha256);
        assert_eq!(
            reference.identity_bundle_sha256(),
            boundary.identity_bundle_sha256
        );
        assert_eq!(
            reference.standalone_semantics_sha256(),
            fixture.run.standalone_semantics_sha256()
        );
        assert_eq!(reference.segment_ordinal(), boundary.segment_ordinal);
        assert_eq!(reference.generation_index(), boundary.generation_index);
        assert_eq!(reference.batch_episodes(), boundary.batch_episodes);
        assert_eq!(
            reference.checkpoint_segment_updates(),
            boundary.checkpoint_segment_updates
        );
        assert_eq!(reference.parent_head_sha256(), boundary.parent_head_sha256);
        assert_eq!(reference.head_sha256(), boundary.head_sha256);
        assert_eq!(reference.head_record_sha256(), boundary.head_record_sha256);
        assert_eq!(
            reference.segment_manifest_sha256(),
            boundary.segment_manifest_sha256
        );
        assert_eq!(
            reference.checkpoint_manifest_sha256(),
            boundary.checkpoint_manifest_sha256
        );
        assert_eq!(
            reference.checkpoint_payload_sha256(),
            boundary.checkpoint_payload_sha256
        );
        assert_eq!(
            reference.checkpoint_sidecar_sha256(),
            sha256_v1(fixture.boundary.checkpoint_sidecar_canonical_bytes())
        );
        assert_eq!(
            reference.logical_state_sha256(),
            boundary.logical_state_sha256
        );
        assert_eq!(
            reference.model_parameter_sha256(),
            boundary.model_parameter_sha256
        );
        assert_eq!(reference.train_state_sha256(), boundary.train_state_sha256);
        assert_eq!(
            reference.last_update_evidence_sha256(),
            boundary.last_update_evidence_sha256
        );
        assert_eq!(
            fixture.latest.checkpoint_ref_sha256(),
            sha256_v1(reference.canonical_bytes())
        );
        assert_eq!(fixture.latest.run_sha256(), boundary.run_sha256);
        assert_eq!(
            fixture.latest.identity_bundle_sha256(),
            boundary.identity_bundle_sha256
        );
        assert_eq!(fixture.latest.generation_index(), boundary.generation_index);
        assert_eq!(
            fixture.latest.checkpoint_segment_updates(),
            boundary.checkpoint_segment_updates
        );
        assert_eq!(fixture.latest.head_sha256(), boundary.head_sha256);
        assert_eq!(
            fixture.latest.head_record_sha256(),
            sha256_v1(fixture.boundary.head_record_canonical_bytes())
        );
    }

    #[test]
    fn production_source_has_no_filesystem_publisher_or_raw_authority_surface() {
        let production = include_str!("native_training_store_reference_latest_v2.rs")
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
            "MoveFileExW",
            "PersistenceReceipt",
            "NativeTrainingBoundaryFactsV2",
            "pub fn publish",
            "pub fn reopen",
            "latest_published_last",
        ] {
            assert!(
                !production.contains(forbidden),
                "production source unexpectedly contains {forbidden}"
            );
        }
        assert!(production.contains("&ValidatedNativeTrainingBoundaryV2"));
        assert!(production.contains("&ValidatedCheckpointReferenceV2"));
    }
}
