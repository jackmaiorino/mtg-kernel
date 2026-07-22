//! Pure segment-continuation-v2 authority for Native Training Store V2.
//!
//! A continuation file is not independently authoritative: the frozen
//! largest-prefix rule can only be proved against every complete update group
//! in the segment. Public construction and decoding therefore operate on the
//! complete continuation chain, consume an opaque parent evidence context, and
//! return the uniquely advanced context. This module owns no filesystem path,
//! segment manifest, conservative pre-rollout planner, publisher, receipt, or
//! executor mutation.

use crate::canonical_json_v1::{
    count_canonical_json_bytes_v1, from_canonical_json_bytes_v1, to_canonical_json_bytes_v1,
    CanonicalJsonClosedMaxErrorV1, CanonicalJsonClosedMaxV1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_update_group_v1::{
    maximum_update_group_json_shape_v2, validate_embedded_update_group_wire_v1,
    validate_update_evidence_chain_context_v1, UpdateEvidenceChainContextV1, UpdateGroupV1Error,
    UpdateGroupV1ErrorKind, UpdateGroupWireV1, ValidatedUpdateGroupV1,
};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};
use std::alloc::Layout;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const SEGMENT_CONTINUATION_SCHEMA_V2: &str = "mtg_kernel_native_train_segment_continuation/v2";
// Widened 2026-07-21 (ledger #307) from 268_435_456 bytes / 262_144 rows to
// admit K=256+ batch schedules: the worst-case per-segment evidence bound at
// realistic per-episode caps exceeded the original limits while actual
// continuation files stay far smaller (reads and validation buffers size to
// actual content, never to these admission ceilings).
pub const SEGMENT_CONTINUATION_MAX_BYTES_V2: u64 = 2_147_483_648;
pub const SEGMENT_CONTINUATION_MAX_LOGICAL_ROWS_V2: u64 = 4_194_304;
pub const SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2: u64 = 99_999_999;
pub const SEGMENT_CONTINUATION_RECORD_CONTRACT_SHA256_V2: &str =
    crate::native_training_store_checkpoint_v3::NATIVE_TRAINING_STORE_RECORD_CONTRACT_SHA256_V1;

const U63_MAX_V2: u64 = (1_u64 << 63) - 1;

const PREVIOUS_CONTINUATION_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "previous_continuation_sha256",
    )];
const PREVIOUS_UPDATE_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("update_groups"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("previous_update_evidence_sha256"),
];
const EPISODE_WINNER_NULL_PATH_V2: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("update_groups"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("evidence"),
    CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
];
const CONTINUATION_NULL_PATHS_V2: &[&[CanonicalJsonNullPathSegmentV1]] = &[
    PREVIOUS_CONTINUATION_NULL_PATH_V2,
    PREVIOUS_UPDATE_NULL_PATH_V2,
    EPISODE_WINNER_NULL_PATH_V2,
];
const CONTINUATION_NULL_POLICY_V2: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(CONTINUATION_NULL_PATHS_V2);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SegmentContinuationDecodeWireV2 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    parent_generation_index: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    continuation_index: u64,
    previous_continuation_sha256: Option<String>,
    update_group_start_ordinal: u64,
    update_group_count: u64,
    logical_row_count: u64,
    update_groups: Vec<UpdateGroupWireV1>,
}

struct UpdateGroupSequenceV2<'a>(&'a [SegmentContinuationUpdateGroupV2]);

impl Serialize for UpdateGroupSequenceV2<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for group in self.0 {
            sequence.serialize_element(&group.wire)?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct SegmentContinuationEmitWireV2<'a> {
    schema: &'static str,
    run_sha256: &'a str,
    identity_bundle_sha256: &'a str,
    segment_ordinal: u64,
    parent_generation_index: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    continuation_index: u64,
    previous_continuation_sha256: Option<&'a str>,
    update_group_start_ordinal: u64,
    update_group_count: u64,
    logical_row_count: u64,
    update_groups: UpdateGroupSequenceV2<'a>,
}

/// Closed-grammar maximum for one continuation containing exactly one maximal
/// complete update group. This is the indivisible fallback admitted by the
/// frozen largest-prefix partition rule.
pub(crate) fn maximum_one_group_continuation_cj_bytes_v2(
    episode_count: u64,
    physical_term_count: u64,
    gauge_bound_count: u64,
) -> std::result::Result<u64, CanonicalJsonClosedMaxErrorV1> {
    let u63 = CanonicalJsonClosedMaxV1::max_u63_v1();
    let digest = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(64)?;
    let group =
        maximum_update_group_json_shape_v2(episode_count, physical_term_count, gauge_bound_count)?;
    CanonicalJsonClosedMaxV1::object_v1(&[
        ("batch_episodes", u63),
        ("checkpoint_segment_updates", u63),
        ("continuation_index", u63),
        ("generation_index", u63),
        ("identity_bundle_sha256", digest),
        ("logical_row_count", u63),
        ("parent_generation_index", u63),
        (
            "previous_continuation_sha256",
            CanonicalJsonClosedMaxV1::choice_v1(CanonicalJsonClosedMaxV1::null_v1(), digest)?,
        ),
        ("run_sha256", digest),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(SEGMENT_CONTINUATION_SCHEMA_V2)?,
        ),
        ("segment_ordinal", u63),
        ("update_group_count", u63),
        ("update_group_start_ordinal", u63),
        (
            "update_groups",
            CanonicalJsonClosedMaxV1::array_v1(1, group)?,
        ),
    ])?
    .canonical_document_bytes_v1()
}

/// One validated complete update embedded in a continuation.
pub struct SegmentContinuationUpdateGroupV2 {
    wire: UpdateGroupWireV1,
    update_index: u64,
    previous_update_evidence_sha256: Option<[u8; 32]>,
    update_evidence_sha256: [u8; 32],
    logical_row_count: u64,
    standalone_token_byte_count: u64,
}

impl std::fmt::Debug for SegmentContinuationUpdateGroupV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SegmentContinuationUpdateGroupV2")
            .field("update_index", &self.update_index)
            .field("logical_row_count", &self.logical_row_count)
            .field(
                "update_evidence_sha256",
                &lower_hex_raw32_v1(self.update_evidence_sha256),
            )
            .finish_non_exhaustive()
    }
}

impl SegmentContinuationUpdateGroupV2 {
    pub const fn update_index(&self) -> u64 {
        self.update_index
    }

    pub const fn previous_update_evidence_sha256(&self) -> Option<[u8; 32]> {
        self.previous_update_evidence_sha256
    }

    pub const fn update_evidence_sha256(&self) -> [u8; 32] {
        self.update_evidence_sha256
    }

    pub const fn logical_row_count(&self) -> u64 {
        self.logical_row_count
    }
}

/// One validated continuation file inside a complete segment chain.
pub struct ValidatedSegmentContinuationV2 {
    canonical_bytes: Vec<u8>,
    continuation_sha256: [u8; 32],
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    parent_generation_index: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    continuation_index: u64,
    previous_continuation_sha256: Option<[u8; 32]>,
    update_group_start_ordinal: u64,
    logical_row_count: u64,
    update_groups: Vec<SegmentContinuationUpdateGroupV2>,
}

impl std::fmt::Debug for ValidatedSegmentContinuationV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedSegmentContinuationV2")
            .field("segment_ordinal", &self.segment_ordinal)
            .field("generation_index", &self.generation_index)
            .field("continuation_index", &self.continuation_index)
            .field("update_group_count", &self.update_groups.len())
            .field("logical_row_count", &self.logical_row_count)
            .field(
                "continuation_sha256",
                &lower_hex_raw32_v1(self.continuation_sha256),
            )
            .finish_non_exhaustive()
    }
}

impl ValidatedSegmentContinuationV2 {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub const fn continuation_sha256(&self) -> [u8; 32] {
        self.continuation_sha256
    }

    pub fn run_sha256(&self) -> &str {
        &self.run_sha256
    }

    pub fn identity_bundle_sha256(&self) -> &str {
        &self.identity_bundle_sha256
    }

    pub const fn segment_ordinal(&self) -> u64 {
        self.segment_ordinal
    }

    pub const fn parent_generation_index(&self) -> u64 {
        self.parent_generation_index
    }

    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub const fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub const fn checkpoint_segment_updates(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub const fn continuation_index(&self) -> u64 {
        self.continuation_index
    }

    pub const fn previous_continuation_sha256(&self) -> Option<[u8; 32]> {
        self.previous_continuation_sha256
    }

    pub const fn update_group_start_ordinal(&self) -> u64 {
        self.update_group_start_ordinal
    }

    pub fn update_group_count(&self) -> usize {
        self.update_groups.len()
    }

    pub const fn logical_row_count(&self) -> u64 {
        self.logical_row_count
    }

    pub fn update_groups(&self) -> &[SegmentContinuationUpdateGroupV2] {
        &self.update_groups
    }
}

/// Complete, uniquely partitioned continuation authority for one trained
/// segment.
///
/// The authority has no unchecked constructor or deserializer:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_segment_continuation_v2::ValidatedSegmentContinuationChainV2;
/// let _ = ValidatedSegmentContinuationChainV2 {};
/// ```
pub struct ValidatedSegmentContinuationChainV2 {
    segment_ordinal: u64,
    parent_generation_index: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    continuations: Vec<ValidatedSegmentContinuationV2>,
    ordered_update_evidence: Vec<(u64, [u8; 32])>,
}

impl std::fmt::Debug for ValidatedSegmentContinuationChainV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedSegmentContinuationChainV2")
            .field("segment_ordinal", &self.segment_ordinal)
            .field("parent_generation_index", &self.parent_generation_index)
            .field("generation_index", &self.generation_index)
            .field("continuation_count", &self.continuations.len())
            .field(
                "ordered_update_evidence_count",
                &self.ordered_update_evidence.len(),
            )
            .finish_non_exhaustive()
    }
}

impl ValidatedSegmentContinuationChainV2 {
    pub const fn segment_ordinal(&self) -> u64 {
        self.segment_ordinal
    }

    pub const fn parent_generation_index(&self) -> u64 {
        self.parent_generation_index
    }

    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub const fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub const fn checkpoint_segment_updates(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub fn continuations(&self) -> &[ValidatedSegmentContinuationV2] {
        &self.continuations
    }

    pub fn ordered_update_evidence(&self) -> &[(u64, [u8; 32])] {
        &self.ordered_update_evidence
    }
}

/// Complete segment authority paired with the only context that can validate
/// its successor.
pub struct ValidatedSegmentContinuationChainAdvanceV2 {
    chain: ValidatedSegmentContinuationChainV2,
    advanced_context: UpdateEvidenceChainContextV1,
}

impl std::fmt::Debug for ValidatedSegmentContinuationChainAdvanceV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedSegmentContinuationChainAdvanceV2")
            .field("chain", &self.chain)
            .field("advanced_context", &self.advanced_context)
            .finish()
    }
}

impl ValidatedSegmentContinuationChainAdvanceV2 {
    pub const fn chain(&self) -> &ValidatedSegmentContinuationChainV2 {
        &self.chain
    }

    pub const fn advanced_context(&self) -> &UpdateEvidenceChainContextV1 {
        &self.advanced_context
    }

    pub fn into_parts(
        self,
    ) -> (
        ValidatedSegmentContinuationChainV2,
        UpdateEvidenceChainContextV1,
    ) {
        (self.chain, self.advanced_context)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentContinuationV2ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidDigest,
    InvalidScalar,
    InvalidArithmetic,
    RunBinding,
    BoundaryBinding,
    GroupCount,
    UpdateGroup(UpdateGroupV1ErrorKind),
    ContinuationChain,
    LogicalRowCount,
    PartitionMismatch,
    Unrepresentable,
}

impl SegmentContinuationV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_segment_continuation_v2_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_train_segment_continuation_v2_invalid_schema",
            Self::InvalidDigest => "native_train_segment_continuation_v2_invalid_digest",
            Self::InvalidScalar => "native_train_segment_continuation_v2_invalid_scalar",
            Self::InvalidArithmetic => "native_train_segment_continuation_v2_invalid_arithmetic",
            Self::RunBinding => "native_train_segment_continuation_v2_run_binding",
            Self::BoundaryBinding => "native_train_segment_continuation_v2_boundary_binding",
            Self::GroupCount => "native_train_segment_continuation_v2_group_count",
            Self::UpdateGroup(kind) => kind.code(),
            Self::ContinuationChain => "native_train_segment_continuation_v2_chain",
            Self::LogicalRowCount => "native_train_segment_continuation_v2_logical_row_count",
            Self::PartitionMismatch => "native_train_segment_continuation_v2_partition_mismatch",
            Self::Unrepresentable => "native_train_segment_continuation_v2_unrepresentable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentContinuationV2Error {
    kind: SegmentContinuationV2ErrorKind,
}

impl SegmentContinuationV2Error {
    const fn new(kind: SegmentContinuationV2ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> SegmentContinuationV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for SegmentContinuationV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for SegmentContinuationV2Error {}

impl From<CanonicalJsonErrorV1> for SegmentContinuationV2Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(SegmentContinuationV2ErrorKind::CanonicalJson(error.kind()))
    }
}

type Result<T> = std::result::Result<T, SegmentContinuationV2Error>;

#[derive(Clone, Copy)]
struct SegmentBoundsV2 {
    segment_ordinal: u64,
    parent_generation_index: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
}

#[derive(Clone, Copy)]
struct ContinuationLimitsV2 {
    max_bytes: u64,
    max_logical_rows: u64,
}

const PRODUCTION_LIMITS_V2: ContinuationLimitsV2 = ContinuationLimitsV2 {
    max_bytes: SEGMENT_CONTINUATION_MAX_BYTES_V2,
    max_logical_rows: SEGMENT_CONTINUATION_MAX_LOGICAL_ROWS_V2,
};

struct PlannedContinuationV2 {
    continuation_index: u64,
    previous_continuation_sha256: Option<[u8; 32]>,
    update_group_start_ordinal: u64,
    update_group_count: usize,
    logical_row_count: u64,
    canonical_bytes: Vec<u8>,
    continuation_sha256: [u8; 32],
}

struct ParsedContinuationV2 {
    canonical_bytes: Vec<u8>,
    update_group_start_ordinal: usize,
    update_group_count: usize,
    logical_row_count: u64,
}

/// Exact architecture-dependent allocation products for every private vector
/// element type retained by the trained continuation builder.
pub(crate) fn segment_continuation_allocation_layout_bytes_v2(
    update_group_count: usize,
    continuation_count: usize,
) -> Option<[u64; 6]> {
    Some([
        allocation_layout_bytes_v2::<SegmentContinuationUpdateGroupV2>(update_group_count)?,
        allocation_layout_bytes_v2::<PlannedContinuationV2>(continuation_count)?,
        allocation_layout_bytes_v2::<ValidatedSegmentContinuationV2>(continuation_count)?,
        allocation_layout_bytes_v2::<(u64, [u8; 32])>(update_group_count)?,
        allocation_layout_bytes_v2::<ParsedContinuationV2>(continuation_count)?,
        allocation_layout_bytes_v2::<Vec<u8>>(continuation_count)?,
    ])
}

fn allocation_layout_bytes_v2<T>(count: usize) -> Option<u64> {
    u64::try_from(Layout::array::<T>(count).ok()?.size()).ok()
}

/// Builds the exact complete continuation chain from already validated update
/// groups and independently revalidates them against the consumed parent
/// context before any continuation bytes become authoritative.
pub fn build_segment_continuations_v2(
    run: &ValidatedTrainRunV2,
    parent_context: UpdateEvidenceChainContextV1,
    groups: Vec<ValidatedUpdateGroupV1>,
) -> Result<ValidatedSegmentContinuationChainAdvanceV2> {
    build_segment_continuations_with_limits_v2(run, parent_context, groups, PRODUCTION_LIMITS_V2)
}

/// Test-only access to the unchanged production planner under strictly
/// reduced limits, used to force a genuine multi-continuation authority.
#[cfg(test)]
pub(crate) fn build_segment_continuations_with_test_limits_v2(
    run: &ValidatedTrainRunV2,
    parent_context: UpdateEvidenceChainContextV1,
    groups: Vec<ValidatedUpdateGroupV1>,
    max_bytes: u64,
    max_logical_rows: u64,
) -> Result<ValidatedSegmentContinuationChainAdvanceV2> {
    if max_bytes == 0
        || max_logical_rows == 0
        || max_bytes > PRODUCTION_LIMITS_V2.max_bytes
        || max_logical_rows > PRODUCTION_LIMITS_V2.max_logical_rows
        || (max_bytes == PRODUCTION_LIMITS_V2.max_bytes
            && max_logical_rows == PRODUCTION_LIMITS_V2.max_logical_rows)
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::InvalidScalar));
    }
    build_segment_continuations_with_limits_v2(
        run,
        parent_context,
        groups,
        ContinuationLimitsV2 {
            max_bytes,
            max_logical_rows,
        },
    )
}

/// Decodes and re-partitions a complete set of continuation files. A single
/// file has no public decoder because it cannot prove largest-prefix
/// maximality in isolation.
pub fn decode_segment_continuations_v2(
    run: &ValidatedTrainRunV2,
    parent_context: UpdateEvidenceChainContextV1,
    continuation_cjs: &[Vec<u8>],
) -> Result<ValidatedSegmentContinuationChainAdvanceV2> {
    decode_segment_continuations_with_limits_v2(
        run,
        parent_context,
        continuation_cjs,
        PRODUCTION_LIMITS_V2,
    )
}

fn build_segment_continuations_with_limits_v2(
    run: &ValidatedTrainRunV2,
    mut context: UpdateEvidenceChainContextV1,
    groups: Vec<ValidatedUpdateGroupV1>,
    limits: ContinuationLimitsV2,
) -> Result<ValidatedSegmentContinuationChainAdvanceV2> {
    let bounds = validate_segment_start_v2(run, &context)?;
    require_exact_group_count_v2(groups.len(), bounds.checkpoint_segment_updates)?;
    let mut embedded = Vec::with_capacity(groups.len());
    for group in groups {
        let wire = group.into_embedded_wire_v1();
        let advance = validate_embedded_update_group_wire_v1(run, context, wire)
            .map_err(map_update_group_error_v2)?;
        let (validated, advanced) = advance.into_parts();
        embedded.push(SegmentContinuationUpdateGroupV2::from_validated(validated)?);
        context = advanced;
    }
    validate_final_context_v2(&context, bounds)?;
    let plans = plan_continuations_v2(run, bounds, &embedded, limits)?;
    assemble_chain_v2(run, bounds, embedded, plans, context)
}

fn decode_segment_continuations_with_limits_v2(
    run: &ValidatedTrainRunV2,
    mut context: UpdateEvidenceChainContextV1,
    continuation_cjs: &[Vec<u8>],
    limits: ContinuationLimitsV2,
) -> Result<ValidatedSegmentContinuationChainAdvanceV2> {
    let bounds = validate_segment_start_v2(run, &context)?;
    let continuation_count = u64::try_from(continuation_cjs.len())
        .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
    if continuation_count == 0 || continuation_count > bounds.checkpoint_segment_updates {
        return Err(error_v2(SegmentContinuationV2ErrorKind::GroupCount));
    }

    let expected_group_capacity = usize::try_from(bounds.checkpoint_segment_updates)
        .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
    let mut raw_groups = Vec::new();
    let mut parsed_files = Vec::with_capacity(continuation_cjs.len());
    let mut previous_continuation_sha256 = None;
    for (continuation_index, canonical_bytes) in continuation_cjs.iter().enumerate() {
        let byte_count = u64::try_from(canonical_bytes.len())
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        if byte_count > limits.max_bytes {
            return Err(error_v2(SegmentContinuationV2ErrorKind::RecordTooLarge));
        }
        let wire: SegmentContinuationDecodeWireV2 =
            from_canonical_json_bytes_v1(canonical_bytes, CONTINUATION_NULL_POLICY_V2)?;
        let reencoded = to_canonical_json_bytes_v1(&wire, CONTINUATION_NULL_POLICY_V2)?;
        if reencoded.as_slice() != canonical_bytes.as_slice() {
            return Err(error_v2(SegmentContinuationV2ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NonCanonicalBytes,
            )));
        }
        let expected_continuation_index = u64::try_from(continuation_index)
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        let expected_start = u64::try_from(raw_groups.len())
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        let group_count = validate_decoded_header_v2(
            run,
            bounds,
            &wire,
            expected_continuation_index,
            expected_start,
            previous_continuation_sha256,
        )?;
        let end = raw_groups
            .len()
            .checked_add(group_count)
            .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        if end > expected_group_capacity {
            return Err(error_v2(SegmentContinuationV2ErrorKind::GroupCount));
        }
        parsed_files.push(ParsedContinuationV2 {
            canonical_bytes: reencoded,
            update_group_start_ordinal: raw_groups.len(),
            update_group_count: group_count,
            logical_row_count: wire.logical_row_count,
        });
        raw_groups.extend(wire.update_groups);
        previous_continuation_sha256 = Some(sha256_v1(canonical_bytes));
    }
    require_exact_group_count_v2(raw_groups.len(), bounds.checkpoint_segment_updates)?;

    let mut embedded = Vec::with_capacity(raw_groups.len());
    for wire in raw_groups {
        let advance = validate_embedded_update_group_wire_v1(run, context, wire)
            .map_err(map_update_group_error_v2)?;
        let (validated, advanced) = advance.into_parts();
        embedded.push(SegmentContinuationUpdateGroupV2::from_validated(validated)?);
        context = advanced;
    }
    validate_final_context_v2(&context, bounds)?;

    for parsed in &parsed_files {
        let end = parsed
            .update_group_start_ordinal
            .checked_add(parsed.update_group_count)
            .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        let rows = checked_group_row_sum_v2(&embedded[parsed.update_group_start_ordinal..end])?;
        if rows != parsed.logical_row_count || rows > limits.max_logical_rows {
            return Err(error_v2(SegmentContinuationV2ErrorKind::LogicalRowCount));
        }
    }

    let plans = plan_continuations_v2(run, bounds, &embedded, limits)?;
    if plans.len() != parsed_files.len()
        || plans
            .iter()
            .zip(&parsed_files)
            .any(|(planned, parsed)| planned.canonical_bytes != parsed.canonical_bytes)
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::PartitionMismatch));
    }
    assemble_chain_v2(run, bounds, embedded, plans, context)
}

impl SegmentContinuationUpdateGroupV2 {
    fn from_validated(group: ValidatedUpdateGroupV1) -> Result<Self> {
        let standalone_byte_count = u64::try_from(group.canonical_bytes().len())
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        let standalone_token_byte_count = standalone_byte_count
            .checked_sub(1)
            .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        let previous_update_evidence_sha256 = group
            .previous_update_evidence_sha256()
            .map(parse_digest_v2)
            .transpose()?;
        let update_index = group.update_index();
        let update_evidence_sha256 = group.update_evidence_sha256();
        let logical_row_count = group.logical_row_count();
        let wire = group.into_embedded_wire_v1();
        Ok(Self {
            wire,
            update_index,
            previous_update_evidence_sha256,
            update_evidence_sha256,
            logical_row_count,
            standalone_token_byte_count,
        })
    }
}

fn validate_segment_start_v2(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
) -> Result<SegmentBoundsV2> {
    validate_update_evidence_chain_context_v1(run, context).map_err(map_update_group_error_v2)?;
    let parent_generation_index = context
        .next_update_index()
        .checked_sub(1)
        .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    if checkpoint_segment_updates == 0
        || !parent_generation_index.is_multiple_of(checkpoint_segment_updates)
        || (parent_generation_index == 0) != context.previous_update_evidence_sha256().is_none()
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::BoundaryBinding));
    }
    let generation_index = checked_u63_add_v2(parent_generation_index, checkpoint_segment_updates)?;
    if generation_index > run.requested_successful_updates()
        || generation_index > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2
        || checkpoint_segment_updates > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::BoundaryBinding));
    }
    Ok(SegmentBoundsV2 {
        segment_ordinal: generation_index / checkpoint_segment_updates,
        parent_generation_index,
        generation_index,
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates,
    })
}

fn validate_final_context_v2(
    context: &UpdateEvidenceChainContextV1,
    bounds: SegmentBoundsV2,
) -> Result<()> {
    let expected_next = checked_u63_add_v2(bounds.generation_index, 1)?;
    if context.next_update_index() != expected_next
        || context.progress().successful_update_count() != bounds.generation_index
        || context.previous_update_evidence_sha256().is_none()
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::BoundaryBinding));
    }
    Ok(())
}

fn require_exact_group_count_v2(actual: usize, expected: u64) -> Result<()> {
    if u64::try_from(actual).ok() != Some(expected) || actual == 0 {
        return Err(error_v2(SegmentContinuationV2ErrorKind::GroupCount));
    }
    Ok(())
}

fn validate_decoded_header_v2(
    run: &ValidatedTrainRunV2,
    bounds: SegmentBoundsV2,
    wire: &SegmentContinuationDecodeWireV2,
    expected_continuation_index: u64,
    expected_start: u64,
    expected_previous_continuation_sha256: Option<[u8; 32]>,
) -> Result<usize> {
    let scalars = [
        wire.segment_ordinal,
        wire.parent_generation_index,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
        wire.continuation_index,
        wire.update_group_start_ordinal,
        wire.update_group_count,
        wire.logical_row_count,
    ];
    if scalars.into_iter().any(|value| !is_u63_v2(value))
        || wire.update_group_count == 0
        || wire.logical_row_count == 0
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::InvalidScalar));
    }
    if wire.schema != SEGMENT_CONTINUATION_SCHEMA_V2 {
        return Err(error_v2(SegmentContinuationV2ErrorKind::InvalidSchema));
    }
    parse_digest_v2(&wire.run_sha256)?;
    parse_digest_v2(&wire.identity_bundle_sha256)?;
    let previous = wire
        .previous_continuation_sha256
        .as_deref()
        .map(parse_digest_v2)
        .transpose()?;
    if wire.run_sha256 != run.run_sha256()
        || wire.identity_bundle_sha256 != run.identity_bundle_sha256()
        || wire.segment_ordinal != bounds.segment_ordinal
        || wire.parent_generation_index != bounds.parent_generation_index
        || wire.generation_index != bounds.generation_index
        || wire.batch_episodes != bounds.batch_episodes
        || wire.checkpoint_segment_updates != bounds.checkpoint_segment_updates
        || wire.continuation_index != expected_continuation_index
        || wire.update_group_start_ordinal != expected_start
    {
        return Err(error_v2(SegmentContinuationV2ErrorKind::RunBinding));
    }
    if previous != expected_previous_continuation_sha256 {
        return Err(error_v2(SegmentContinuationV2ErrorKind::ContinuationChain));
    }
    let group_count = usize::try_from(wire.update_group_count)
        .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
    if group_count != wire.update_groups.len() {
        return Err(error_v2(SegmentContinuationV2ErrorKind::GroupCount));
    }
    Ok(group_count)
}

fn plan_continuations_v2(
    run: &ValidatedTrainRunV2,
    bounds: SegmentBoundsV2,
    groups: &[SegmentContinuationUpdateGroupV2],
    limits: ContinuationLimitsV2,
) -> Result<Vec<PlannedContinuationV2>> {
    require_exact_group_count_v2(groups.len(), bounds.checkpoint_segment_updates)?;
    let mut plans = Vec::new();
    let mut start = 0_usize;
    let mut previous_continuation_sha256 = None;
    while start < groups.len() {
        let continuation_index = u64::try_from(plans.len())
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        if continuation_index > SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2 {
            return Err(error_v2(SegmentContinuationV2ErrorKind::InvalidScalar));
        }
        let previous_hex = previous_continuation_sha256.map(lower_hex_raw32_v1);
        let mut rows = 0_u64;
        let mut group_token_bytes = 0_u64;
        let mut best = None;
        let mut previous_candidate_bytes = 0_u64;
        for end in (start + 1)..=groups.len() {
            let group = &groups[end - 1];
            rows = checked_u63_add_v2(rows, group.logical_row_count)?;
            group_token_bytes = group_token_bytes
                .checked_add(group.standalone_token_byte_count)
                .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
            if rows > limits.max_logical_rows {
                break;
            }
            let group_count = end - start;
            let byte_count = predicted_continuation_byte_count_v2(
                run,
                bounds,
                continuation_index,
                previous_hex.as_deref(),
                start,
                group_count,
                rows,
                group_token_bytes,
            )?;
            if byte_count < previous_candidate_bytes {
                return Err(error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic));
            }
            previous_candidate_bytes = byte_count;
            if byte_count > limits.max_bytes {
                break;
            }
            best = Some((end, rows, byte_count));
        }
        let Some((end, logical_row_count, predicted_byte_count)) = best else {
            return Err(error_v2(SegmentContinuationV2ErrorKind::Unrepresentable));
        };
        let update_group_count = end - start;
        let update_group_start_ordinal = u64::try_from(start)
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        let emit = continuation_emit_wire_v2(
            run,
            bounds,
            continuation_index,
            previous_hex.as_deref(),
            update_group_start_ordinal,
            update_group_count,
            logical_row_count,
            &groups[start..end],
        )?;
        let counted = count_canonical_json_bytes_v1(&emit, CONTINUATION_NULL_POLICY_V2)?;
        let canonical_bytes = to_canonical_json_bytes_v1(&emit, CONTINUATION_NULL_POLICY_V2)?;
        let emitted = u64::try_from(canonical_bytes.len())
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
        if counted != predicted_byte_count
            || emitted != predicted_byte_count
            || emitted > limits.max_bytes
        {
            return Err(error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic));
        }
        let continuation_sha256 = sha256_v1(&canonical_bytes);
        plans.push(PlannedContinuationV2 {
            continuation_index,
            previous_continuation_sha256,
            update_group_start_ordinal,
            update_group_count,
            logical_row_count,
            canonical_bytes,
            continuation_sha256,
        });
        previous_continuation_sha256 = Some(continuation_sha256);
        start = end;
    }
    Ok(plans)
}

#[allow(clippy::too_many_arguments)]
fn predicted_continuation_byte_count_v2(
    run: &ValidatedTrainRunV2,
    bounds: SegmentBoundsV2,
    continuation_index: u64,
    previous_continuation_sha256: Option<&str>,
    update_group_start_ordinal: usize,
    update_group_count: usize,
    logical_row_count: u64,
    group_token_byte_count: u64,
) -> Result<u64> {
    let empty_groups: &[SegmentContinuationUpdateGroupV2] = &[];
    let emit = continuation_emit_wire_v2(
        run,
        bounds,
        continuation_index,
        previous_continuation_sha256,
        u64::try_from(update_group_start_ordinal)
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?,
        update_group_count,
        logical_row_count,
        empty_groups,
    )?;
    let empty_count = count_canonical_json_bytes_v1(&emit, CONTINUATION_NULL_POLICY_V2)?;
    let comma_count = u64::try_from(update_group_count - 1)
        .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?;
    empty_count
        .checked_add(group_token_byte_count)
        .and_then(|value| value.checked_add(comma_count))
        .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))
}

#[allow(clippy::too_many_arguments)]
fn continuation_emit_wire_v2<'a>(
    run: &'a ValidatedTrainRunV2,
    bounds: SegmentBoundsV2,
    continuation_index: u64,
    previous_continuation_sha256: Option<&'a str>,
    update_group_start_ordinal: u64,
    update_group_count: usize,
    logical_row_count: u64,
    groups: &'a [SegmentContinuationUpdateGroupV2],
) -> Result<SegmentContinuationEmitWireV2<'a>> {
    Ok(SegmentContinuationEmitWireV2 {
        schema: SEGMENT_CONTINUATION_SCHEMA_V2,
        run_sha256: run.run_sha256(),
        identity_bundle_sha256: run.identity_bundle_sha256(),
        segment_ordinal: bounds.segment_ordinal,
        parent_generation_index: bounds.parent_generation_index,
        generation_index: bounds.generation_index,
        batch_episodes: bounds.batch_episodes,
        checkpoint_segment_updates: bounds.checkpoint_segment_updates,
        continuation_index,
        previous_continuation_sha256,
        update_group_start_ordinal,
        update_group_count: u64::try_from(update_group_count)
            .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))?,
        logical_row_count,
        update_groups: UpdateGroupSequenceV2(groups),
    })
}

fn checked_group_row_sum_v2(groups: &[SegmentContinuationUpdateGroupV2]) -> Result<u64> {
    let mut rows = 0_u64;
    for group in groups {
        rows = checked_u63_add_v2(rows, group.logical_row_count)?;
    }
    if rows == 0 {
        return Err(error_v2(SegmentContinuationV2ErrorKind::LogicalRowCount));
    }
    Ok(rows)
}

fn assemble_chain_v2(
    run: &ValidatedTrainRunV2,
    bounds: SegmentBoundsV2,
    groups: Vec<SegmentContinuationUpdateGroupV2>,
    plans: Vec<PlannedContinuationV2>,
    advanced_context: UpdateEvidenceChainContextV1,
) -> Result<ValidatedSegmentContinuationChainAdvanceV2> {
    let ordered_update_evidence = groups
        .iter()
        .map(|group| (group.update_index, group.update_evidence_sha256))
        .collect();
    let mut group_iter = groups.into_iter();
    let mut continuations = Vec::with_capacity(plans.len());
    for plan in plans {
        let update_groups: Vec<_> = group_iter.by_ref().take(plan.update_group_count).collect();
        if update_groups.len() != plan.update_group_count {
            return Err(error_v2(SegmentContinuationV2ErrorKind::GroupCount));
        }
        continuations.push(ValidatedSegmentContinuationV2 {
            canonical_bytes: plan.canonical_bytes,
            continuation_sha256: plan.continuation_sha256,
            run_sha256: run.run_sha256().to_owned(),
            identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
            segment_ordinal: bounds.segment_ordinal,
            parent_generation_index: bounds.parent_generation_index,
            generation_index: bounds.generation_index,
            batch_episodes: bounds.batch_episodes,
            checkpoint_segment_updates: bounds.checkpoint_segment_updates,
            continuation_index: plan.continuation_index,
            previous_continuation_sha256: plan.previous_continuation_sha256,
            update_group_start_ordinal: plan.update_group_start_ordinal,
            logical_row_count: plan.logical_row_count,
            update_groups,
        });
    }
    if group_iter.next().is_some() {
        return Err(error_v2(SegmentContinuationV2ErrorKind::GroupCount));
    }
    Ok(ValidatedSegmentContinuationChainAdvanceV2 {
        chain: ValidatedSegmentContinuationChainV2 {
            segment_ordinal: bounds.segment_ordinal,
            parent_generation_index: bounds.parent_generation_index,
            generation_index: bounds.generation_index,
            batch_episodes: bounds.batch_episodes,
            checkpoint_segment_updates: bounds.checkpoint_segment_updates,
            continuations,
            ordered_update_evidence,
        },
        advanced_context,
    })
}

fn checked_u63_add_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .filter(|value| is_u63_v2(*value))
        .ok_or_else(|| error_v2(SegmentContinuationV2ErrorKind::InvalidArithmetic))
}

fn is_u63_v2(value: u64) -> bool {
    value <= U63_MAX_V2
}

fn parse_digest_v2(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value)
        .map_err(|_| error_v2(SegmentContinuationV2ErrorKind::InvalidDigest))
}

fn map_update_group_error_v2(error: UpdateGroupV1Error) -> SegmentContinuationV2Error {
    error_v2(SegmentContinuationV2ErrorKind::UpdateGroup(error.kind()))
}

fn error_v2(kind: SegmentContinuationV2ErrorKind) -> SegmentContinuationV2Error {
    SegmentContinuationV2Error::new(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, decode_genesis_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1, decode_update_group_v1,
    };
    use serde_json::{json, Value};
    use std::sync::OnceLock;
    use std::time::Duration;

    struct GenuineFixtureV2 {
        run_bytes: Vec<u8>,
        genesis_manifest: Vec<u8>,
        genesis_payload: Vec<u8>,
        group_bytes: Vec<Vec<u8>>,
    }

    static GENUINE_FIXTURE_V2: OnceLock<GenuineFixtureV2> = OnceLock::new();

    #[test]
    fn one_group_continuation_closed_maximum_matches_frozen_recurrence() {
        assert_eq!(
            maximum_one_group_continuation_cj_bytes_v2(1, 1, 1).unwrap(),
            4_591 + 730
        );
        assert_eq!(
            maximum_one_group_continuation_cj_bytes_v2(2, 65_536, 131_072).unwrap(),
            36_509_286
        );
    }

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

    fn fixture_v2() -> &'static GenuineFixtureV2 {
        GENUINE_FIXTURE_V2.get_or_init(|| {
            let run_bytes = test_fixture_bytes_v2();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            assert_eq!(run.batch_episodes(), 2);
            assert_eq!(run.checkpoint_segment_updates(), 4);
            assert!(run.requested_successful_updates() >= 8);
            let mut executor = fresh_executor_v2(&run);
            let genesis_payload = executor
                .checkpoint_candidate_v1()
                .unwrap()
                .payload()
                .to_vec();
            let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
            let genesis_manifest = genesis.canonical_bytes().to_vec();
            let mut context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
            let mut group_bytes = Vec::new();
            for update_ordinal in 0..8 {
                let prepared = executor.prepare_update_v2().unwrap();
                let advance = build_update_group_v1(&run, context, &prepared).unwrap();
                let (group, advanced_context) = advance.into_parts();
                group_bytes.push(group.canonical_bytes().to_vec());
                context = advanced_context;
                drop(prepared);
                if update_ordinal + 1 < 8 {
                    executor.run_update_v2().unwrap();
                }
            }
            GenuineFixtureV2 {
                run_bytes,
                genesis_manifest,
                genesis_payload,
                group_bytes,
            }
        })
    }

    fn run_v2() -> ValidatedTrainRunV2 {
        decode_train_run_v2(&fixture_v2().run_bytes).unwrap()
    }

    fn context_at_v2(update_count: usize) -> UpdateEvidenceChainContextV1 {
        let fixture = fixture_v2();
        let run = run_v2();
        let genesis = decode_genesis_checkpoint_manifest_v3(
            &fixture.genesis_manifest,
            &fixture.genesis_payload,
            &run,
        )
        .unwrap();
        let mut context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        for bytes in fixture.group_bytes.iter().take(update_count) {
            context = decode_update_group_v1(&run, context, bytes)
                .unwrap()
                .into_parts()
                .1;
        }
        context
    }

    fn segment_groups_v2(start: usize) -> Vec<ValidatedUpdateGroupV1> {
        let run = run_v2();
        let mut context = context_at_v2(start);
        let mut groups = Vec::new();
        for bytes in fixture_v2().group_bytes.iter().skip(start).take(4) {
            let advance = decode_update_group_v1(&run, context, bytes).unwrap();
            let (group, advanced) = advance.into_parts();
            groups.push(group);
            context = advanced;
        }
        groups
    }

    fn continuation_bytes_v2(chain: &ValidatedSegmentContinuationChainV2) -> Vec<Vec<u8>> {
        chain
            .continuations()
            .iter()
            .map(|continuation| continuation.canonical_bytes().to_vec())
            .collect()
    }

    fn canonical_value_bytes_v2(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, CONTINUATION_NULL_POLICY_V2).unwrap()
    }

    fn continuation_value_v2(bytes: &[u8]) -> Value {
        serde_json::from_slice(bytes.strip_suffix(b"\n").unwrap()).unwrap()
    }

    fn group_rows_v2(groups: &[Value]) -> u64 {
        groups
            .iter()
            .map(|group| group["logical_row_count"].as_u64().unwrap())
            .sum()
    }

    fn split_single_continuation_v2(bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut first = continuation_value_v2(bytes);
        let second_groups = first["update_groups"].as_array_mut().unwrap().split_off(2);
        let (first_group_count, first_logical_rows) = {
            let first_groups = first["update_groups"].as_array().unwrap();
            (first_groups.len(), group_rows_v2(first_groups))
        };
        first["update_group_count"] = json!(first_group_count);
        first["logical_row_count"] = json!(first_logical_rows);
        let first_bytes = canonical_value_bytes_v2(&first);

        let mut second = first;
        second["continuation_index"] = json!(1);
        second["previous_continuation_sha256"] = json!(lower_hex_raw32_v1(sha256_v1(&first_bytes)));
        second["update_group_start_ordinal"] = json!(2);
        second["update_group_count"] = json!(second_groups.len());
        second["logical_row_count"] = json!(group_rows_v2(&second_groups));
        second["update_groups"] = Value::Array(second_groups);
        vec![first_bytes, canonical_value_bytes_v2(&second)]
    }

    fn decode_error_v2(
        start: usize,
        continuation_cjs: &[Vec<u8>],
    ) -> SegmentContinuationV2ErrorKind {
        decode_segment_continuations_v2(&run_v2(), context_at_v2(start), continuation_cjs)
            .unwrap_err()
            .kind()
    }

    #[test]
    fn genuine_k2_s4_two_segment_roundtrip_preserves_the_global_update_chain() {
        let run = run_v2();
        let first =
            build_segment_continuations_v2(&run, context_at_v2(0), segment_groups_v2(0)).unwrap();
        assert_eq!(first.chain().parent_generation_index(), 0);
        assert_eq!(first.chain().generation_index(), 4);
        assert_eq!(first.chain().segment_ordinal(), 1);
        assert_eq!(first.chain().batch_episodes(), 2);
        assert_eq!(first.chain().checkpoint_segment_updates(), 4);
        assert_eq!(first.chain().ordered_update_evidence().len(), 4);
        assert_eq!(first.chain().continuations().len(), 1);
        assert_eq!(first.advanced_context().next_update_index(), 5);
        assert_eq!(
            first
                .advanced_context()
                .progress()
                .successful_update_count(),
            4
        );
        let first_bytes = continuation_bytes_v2(first.chain());
        let first_roundtrip =
            decode_segment_continuations_v2(&run, context_at_v2(0), &first_bytes).unwrap();
        assert_eq!(continuation_bytes_v2(first_roundtrip.chain()), first_bytes);
        assert_eq!(
            first_roundtrip.advanced_context().train_state_sha256(),
            first.advanced_context().train_state_sha256()
        );

        let final_update_four_hash = first.chain().ordered_update_evidence().last().unwrap().1;
        let second =
            build_segment_continuations_v2(&run, context_at_v2(4), segment_groups_v2(4)).unwrap();
        assert_eq!(second.chain().parent_generation_index(), 4);
        assert_eq!(second.chain().generation_index(), 8);
        assert_eq!(second.chain().segment_ordinal(), 2);
        assert_eq!(
            second.chain().continuations()[0].previous_continuation_sha256(),
            None,
            "the per-segment continuation chain resets"
        );
        assert_eq!(
            second.chain().continuations()[0].update_groups()[0].previous_update_evidence_sha256(),
            Some(final_update_four_hash),
            "the global update-evidence chain never resets"
        );
        assert_eq!(second.advanced_context().next_update_index(), 9);
        let second_bytes = continuation_bytes_v2(second.chain());
        let second_roundtrip =
            decode_segment_continuations_v2(&run, context_at_v2(4), &second_bytes).unwrap();
        assert_eq!(
            continuation_bytes_v2(second_roundtrip.chain()),
            second_bytes
        );

        assert!(matches!(
            build_segment_continuations_v2(&run, context_at_v2(0), segment_groups_v2(4))
                .unwrap_err()
                .kind(),
            SegmentContinuationV2ErrorKind::UpdateGroup(_)
        ));
    }

    #[test]
    fn exact_count_sink_and_largest_prefix_are_authoritative() {
        let run = run_v2();
        let production =
            build_segment_continuations_v2(&run, context_at_v2(0), segment_groups_v2(0)).unwrap();
        let production_bytes = continuation_bytes_v2(production.chain());
        assert_eq!(production_bytes.len(), 1);
        let parsed: SegmentContinuationDecodeWireV2 =
            from_canonical_json_bytes_v1(&production_bytes[0], CONTINUATION_NULL_POLICY_V2)
                .unwrap();
        assert_eq!(
            count_canonical_json_bytes_v1(&parsed, CONTINUATION_NULL_POLICY_V2).unwrap(),
            u64::try_from(production_bytes[0].len()).unwrap()
        );

        let alternative = split_single_continuation_v2(&production_bytes[0]);
        assert_eq!(alternative.len(), 2);
        assert_eq!(
            decode_error_v2(0, &alternative),
            SegmentContinuationV2ErrorKind::PartitionMismatch,
            "a parseable nonmaximal split is corruption"
        );

        let groups = segment_groups_v2(0);
        let max_group_rows = groups
            .iter()
            .map(ValidatedUpdateGroupV1::logical_row_count)
            .max()
            .unwrap();
        let row_limited = ContinuationLimitsV2 {
            max_bytes: SEGMENT_CONTINUATION_MAX_BYTES_V2,
            max_logical_rows: max_group_rows,
        };
        let limited =
            build_segment_continuations_with_limits_v2(&run, context_at_v2(0), groups, row_limited)
                .unwrap();
        assert!(limited.chain().continuations().len() > 1);
        let limited_bytes = continuation_bytes_v2(limited.chain());
        let limited_roundtrip = decode_segment_continuations_with_limits_v2(
            &run,
            context_at_v2(0),
            &limited_bytes,
            row_limited,
        )
        .unwrap();
        assert_eq!(
            continuation_bytes_v2(limited_roundtrip.chain()),
            limited_bytes
        );
        for pair in limited.chain().continuations().windows(2) {
            assert!(
                pair[0]
                    .logical_row_count()
                    .checked_add(pair[1].update_groups()[0].logical_row_count())
                    .unwrap()
                    > max_group_rows,
                "each nonfinal prefix is directly maximal under the row cap"
            );
        }

        let byte_limit = u64::try_from(production_bytes[0].len()).unwrap() - 1;
        let byte_limited_limits = ContinuationLimitsV2 {
            max_bytes: byte_limit,
            max_logical_rows: SEGMENT_CONTINUATION_MAX_LOGICAL_ROWS_V2,
        };
        let byte_limited = build_segment_continuations_with_limits_v2(
            &run,
            context_at_v2(0),
            segment_groups_v2(0),
            byte_limited_limits,
        )
        .unwrap();
        assert!(byte_limited.chain().continuations().len() > 1);
        assert!(byte_limited
            .chain()
            .continuations()
            .iter()
            .all(
                |continuation| u64::try_from(continuation.canonical_bytes().len()).unwrap()
                    <= byte_limit
            ));
        let byte_limited_bytes = continuation_bytes_v2(byte_limited.chain());
        decode_segment_continuations_with_limits_v2(
            &run,
            context_at_v2(0),
            &byte_limited_bytes,
            byte_limited_limits,
        )
        .unwrap();
    }

    #[test]
    fn continuation_headers_hash_links_and_nested_groups_fail_closed() {
        let run = run_v2();
        let valid =
            build_segment_continuations_v2(&run, context_at_v2(0), segment_groups_v2(0)).unwrap();
        let valid_bytes = continuation_bytes_v2(valid.chain());
        let base = continuation_value_v2(&valid_bytes[0]);

        for (field, replacement, expected) in [
            (
                "schema",
                json!("mtg_kernel_native_train_segment_continuation/v1"),
                SegmentContinuationV2ErrorKind::InvalidSchema,
            ),
            (
                "run_sha256",
                json!("00".repeat(32)),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "identity_bundle_sha256",
                json!("00".repeat(32)),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "segment_ordinal",
                json!(2),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "parent_generation_index",
                json!(4),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "generation_index",
                json!(8),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "batch_episodes",
                json!(4),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "checkpoint_segment_updates",
                json!(2),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "continuation_index",
                json!(1),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "update_group_start_ordinal",
                json!(1),
                SegmentContinuationV2ErrorKind::RunBinding,
            ),
            (
                "update_group_count",
                json!(3),
                SegmentContinuationV2ErrorKind::GroupCount,
            ),
            (
                "logical_row_count",
                json!(base["logical_row_count"].as_u64().unwrap() + 1),
                SegmentContinuationV2ErrorKind::LogicalRowCount,
            ),
        ] {
            let mut value = base.clone();
            value[field] = replacement;
            assert_eq!(
                decode_error_v2(0, &[canonical_value_bytes_v2(&value)]),
                expected,
                "field {field}"
            );
        }

        let mut reordered = base.clone();
        reordered["update_groups"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(matches!(
            decode_error_v2(0, &[canonical_value_bytes_v2(&reordered)]),
            SegmentContinuationV2ErrorKind::UpdateGroup(_)
        ));

        let split = split_single_continuation_v2(&valid_bytes[0]);
        let mut wrong_link = continuation_value_v2(&split[1]);
        wrong_link["previous_continuation_sha256"] = json!("00".repeat(32));
        assert_eq!(
            decode_error_v2(
                0,
                &[split[0].clone(), canonical_value_bytes_v2(&wrong_link)]
            ),
            SegmentContinuationV2ErrorKind::ContinuationChain
        );
    }

    #[test]
    fn truncation_appended_tail_caps_and_group_counts_fail_closed() {
        let run = run_v2();
        let valid =
            build_segment_continuations_v2(&run, context_at_v2(0), segment_groups_v2(0)).unwrap();
        let valid_bytes = continuation_bytes_v2(valid.chain());

        let mut mid_group_truncation = valid_bytes[0].clone();
        mid_group_truncation.truncate(mid_group_truncation.len() - 17);
        assert!(matches!(
            decode_error_v2(0, &[mid_group_truncation]),
            SegmentContinuationV2ErrorKind::CanonicalJson(_)
        ));

        let split = split_single_continuation_v2(&valid_bytes[0]);
        assert_eq!(
            decode_error_v2(0, &[split[0].clone()]),
            SegmentContinuationV2ErrorKind::GroupCount,
            "a canonical complete prefix is still a truncated segment"
        );

        let mut appended = continuation_value_v2(&valid_bytes[0]);
        let duplicate = appended["update_groups"].as_array().unwrap()[3].clone();
        appended["update_groups"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        let (appended_group_count, appended_logical_rows) = {
            let appended_groups = appended["update_groups"].as_array().unwrap();
            (appended_groups.len(), group_rows_v2(appended_groups))
        };
        appended["update_group_count"] = json!(appended_group_count);
        appended["logical_row_count"] = json!(appended_logical_rows);
        assert_eq!(
            decode_error_v2(0, &[canonical_value_bytes_v2(&appended)]),
            SegmentContinuationV2ErrorKind::GroupCount
        );

        assert_eq!(
            build_segment_continuations_v2(
                &run,
                context_at_v2(0),
                segment_groups_v2(0).into_iter().take(3).collect(),
            )
            .unwrap_err()
            .kind(),
            SegmentContinuationV2ErrorKind::GroupCount
        );
        assert_eq!(
            build_segment_continuations_with_limits_v2(
                &run,
                context_at_v2(0),
                segment_groups_v2(0),
                ContinuationLimitsV2 {
                    max_bytes: 0,
                    max_logical_rows: SEGMENT_CONTINUATION_MAX_LOGICAL_ROWS_V2,
                },
            )
            .unwrap_err()
            .kind(),
            SegmentContinuationV2ErrorKind::Unrepresentable
        );
        assert_eq!(
            build_segment_continuations_with_limits_v2(
                &run,
                context_at_v2(0),
                segment_groups_v2(0),
                ContinuationLimitsV2 {
                    max_bytes: SEGMENT_CONTINUATION_MAX_BYTES_V2,
                    max_logical_rows: 0,
                },
            )
            .unwrap_err()
            .kind(),
            SegmentContinuationV2ErrorKind::Unrepresentable
        );
    }
}
