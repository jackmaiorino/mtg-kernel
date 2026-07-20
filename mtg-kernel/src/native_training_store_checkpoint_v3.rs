//! Pure checkpoint-v3 record authority for Native Training Store V2.
//!
//! This module performs only in-memory canonical record and payload
//! validation.  It owns no filesystem path, publication, recovery, receipt,
//! executor mutation, or trained-evidence workflow.  Genesis authority is
//! available now.  A syntactically valid trained record fails closed until a
//! later complete Episode/UpdateGroup validator can supply a sealed cumulative
//! evidence boundary.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::common_model_snapshot_v1::{
    PARAMETER_ELEMENT_COUNT_V1, PARAMETER_TENSOR_COUNT_V1, PAYLOAD_BYTE_COUNT_V1,
};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_v1, decode_native_train_state_payload_verified_v1,
    NativeDecodedTrainStatePayloadV1, NativeTrainStatePayloadDigestFieldV1,
    NativeTrainStatePayloadDigestsV1, NativeTrainStatePayloadErrorV1,
    NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1, NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1,
    NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1, NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const CHECKPOINT_MANIFEST_SCHEMA_V3: &str = "mtg_kernel_native_train_checkpoint/v3";
pub const CHECKPOINT_MANIFEST_MAX_BYTES_V3: usize = 2 * 1024 * 1024;
pub const CHECKPOINT_LOGICAL_STATE_IDENTITY_V1: &str =
    "mtg-kernel-native-training-logical-state-sha256-v1";
pub const NATIVE_POLICY_VALUE_TRAIN_STATE_SCHEMA_V1: &str =
    "mtg_kernel_native_policy_value_train_state/v1";

/// Exact full-document digest of the frozen revision-5 Store contract that
/// defines this pure record authority.  This is source provenance, not a wire
/// field and not a filesystem-publication claim.
pub const NATIVE_TRAINING_STORE_RECORD_CONTRACT_SHA256_V1: &str =
    "53d5e4f8585e28e95870c54407e7a8a6ce6e292d9d85a30ba53197c04cd0ee0d";

const U63_MAX_V3: u64 = (1_u64 << 63) - 1;
const PARAMETER_TENSOR_COUNT_U64_V3: u64 = 33;
const PARAMETER_ELEMENT_COUNT_U64_V3: u64 = 1_230_994;
const TRAIN_STATE_PAYLOAD_BYTE_COUNT_U64_V1: u64 = 14_771_928;

const _: () = assert!(PARAMETER_TENSOR_COUNT_V1 == 33);
const _: () = assert!(PARAMETER_ELEMENT_COUNT_V1 == 1_230_994);
const _: () = assert!(PAYLOAD_BYTE_COUNT_V1 == 4_923_976);
const _: () = assert!(NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 == 14_771_928);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointOutcomeCountsV3 {
    pub(crate) win: u64,
    pub(crate) loss: u64,
    pub(crate) draw: u64,
}

impl CheckpointOutcomeCountsV3 {
    pub const fn win(&self) -> u64 {
        self.win
    }

    pub const fn loss(&self) -> u64 {
        self.loss
    }

    pub const fn draw(&self) -> u64 {
        self.draw
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointOutcomesByLearnerSeatV3 {
    pub(crate) p0: CheckpointOutcomeCountsV3,
    pub(crate) p1: CheckpointOutcomeCountsV3,
}

impl CheckpointOutcomesByLearnerSeatV3 {
    pub const fn p0(&self) -> &CheckpointOutcomeCountsV3 {
        &self.p0
    }

    pub const fn p1(&self) -> &CheckpointOutcomeCountsV3 {
        &self.p1
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointLearnerSeatCountersV3 {
    pub(crate) p0: u64,
    pub(crate) p1: u64,
}

impl CheckpointLearnerSeatCountersV3 {
    pub const fn p0(&self) -> u64 {
        self.p0
    }

    pub const fn p1(&self) -> u64 {
        self.p1
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointProgressV3 {
    pub(crate) batch_episodes: u64,
    pub(crate) checkpoint_segment_updates: u64,
    pub(crate) next_episode_index: u64,
    pub(crate) successful_update_count: u64,
    pub(crate) completed_episode_count: u64,
    pub(crate) outcomes_by_learner_seat: CheckpointOutcomesByLearnerSeatV3,
    pub(crate) learner_policy_steps_by_seat: CheckpointLearnerSeatCountersV3,
    pub(crate) learner_physical_decisions_by_seat: CheckpointLearnerSeatCountersV3,
}

impl CheckpointProgressV3 {
    pub const fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub const fn checkpoint_segment_updates(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub const fn next_episode_index(&self) -> u64 {
        self.next_episode_index
    }

    pub const fn successful_update_count(&self) -> u64 {
        self.successful_update_count
    }

    pub const fn completed_episode_count(&self) -> u64 {
        self.completed_episode_count
    }

    pub const fn outcomes_by_learner_seat(&self) -> &CheckpointOutcomesByLearnerSeatV3 {
        &self.outcomes_by_learner_seat
    }

    pub const fn learner_policy_steps_by_seat(&self) -> &CheckpointLearnerSeatCountersV3 {
        &self.learner_policy_steps_by_seat
    }

    pub const fn learner_physical_decisions_by_seat(&self) -> &CheckpointLearnerSeatCountersV3 {
        &self.learner_physical_decisions_by_seat
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointTrainStateBindingV3 {
    pub(crate) schema: String,
    pub(crate) adam_step: u64,
    pub(crate) scorer_bias_anchor_f32_bits: u64,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) parameter_tensor_count: u64,
    pub(crate) parameter_element_count: u64,
    pub(crate) model_parameter_sha256: String,
    pub(crate) state_sha256: String,
}

impl CheckpointTrainStateBindingV3 {
    pub fn adam_step(&self) -> u64 {
        self.adam_step
    }

    pub fn scorer_bias_anchor_f32_bits(&self) -> u64 {
        self.scorer_bias_anchor_f32_bits
    }

    pub fn model_parameter_sha256(&self) -> &str {
        &self.model_parameter_sha256
    }

    pub fn state_sha256(&self) -> &str {
        &self.state_sha256
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPayloadSectionBindingV1 {
    pub(crate) name: String,
    pub(crate) offset_bytes: u64,
    pub(crate) byte_count: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPayloadBindingV1 {
    pub(crate) schema: String,
    pub(crate) encoding: String,
    pub(crate) byte_count: u64,
    pub(crate) sha256: String,
    pub(crate) sections: [CheckpointPayloadSectionBindingV1; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CheckpointManifestWireV3 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    progress: CheckpointProgressV3,
    train_state: CheckpointTrainStateBindingV3,
    payload: CheckpointPayloadBindingV1,
    logical_state_sha256: String,
}

/// Fully validated pure checkpoint-v3 authority.
///
/// The authority deliberately has no public fields, serde deserializer, or
/// unchecked constructor:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_checkpoint_v3::CheckpointManifestV3;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<CheckpointManifestV3>();
/// ```
///
/// Direct construction is likewise impossible:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_checkpoint_v3::CheckpointManifestV3;
/// let _ = CheckpointManifestV3 {};
/// ```
pub struct CheckpointManifestV3 {
    wire: CheckpointManifestWireV3,
    canonical_bytes: Vec<u8>,
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    logical_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
}

impl std::fmt::Debug for CheckpointManifestV3 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CheckpointManifestV3")
            .field("segment_ordinal", &self.wire.segment_ordinal)
            .field("generation_index", &self.wire.generation_index)
            .field("batch_episodes", &self.wire.batch_episodes)
            .field(
                "checkpoint_segment_updates",
                &self.wire.checkpoint_segment_updates,
            )
            .field(
                "checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.checkpoint_manifest_sha256),
            )
            .finish_non_exhaustive()
    }
}

impl CheckpointManifestV3 {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub fn run_sha256(&self) -> &str {
        &self.wire.run_sha256
    }

    pub fn identity_bundle_sha256(&self) -> &str {
        &self.wire.identity_bundle_sha256
    }

    pub fn segment_ordinal(&self) -> u64 {
        self.wire.segment_ordinal
    }

    pub fn generation_index(&self) -> u64 {
        self.wire.generation_index
    }

    pub fn batch_episodes(&self) -> u64 {
        self.wire.batch_episodes
    }

    pub fn checkpoint_segment_updates(&self) -> u64 {
        self.wire.checkpoint_segment_updates
    }

    pub fn progress(&self) -> &CheckpointProgressV3 {
        &self.wire.progress
    }

    pub fn train_state(&self) -> &CheckpointTrainStateBindingV3 {
        &self.wire.train_state
    }

    pub fn payload(&self) -> &CheckpointPayloadBindingV1 {
        &self.wire.payload
    }

    pub fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }

    pub fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.checkpoint_payload_sha256
    }

    pub fn logical_state_sha256(&self) -> [u8; 32] {
        self.logical_state_sha256
    }

    pub fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub fn train_state_sha256(&self) -> [u8; 32] {
        self.train_state_sha256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointManifestV3ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidDigest,
    InvalidScalar,
    InvalidArithmetic,
    CrossBinding,
    PayloadExactLength,
    PayloadInvalid,
    PayloadDigestMismatch,
    GenesisSnapshotMismatch,
    LogicalStateDigestMismatch,
    TrainedEvidenceContextRequired,
}

impl CheckpointManifestV3ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_checkpoint_v3_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_train_checkpoint_v3_invalid_schema",
            Self::InvalidDigest => "native_train_checkpoint_v3_invalid_digest",
            Self::InvalidScalar => "native_train_checkpoint_v3_invalid_scalar",
            Self::InvalidArithmetic => "native_train_checkpoint_v3_invalid_arithmetic",
            Self::CrossBinding => "native_train_checkpoint_v3_cross_binding",
            Self::PayloadExactLength => "native_train_checkpoint_v3_payload_exact_length",
            Self::PayloadInvalid => "native_train_checkpoint_v3_payload_invalid",
            Self::PayloadDigestMismatch => "native_train_checkpoint_v3_payload_digest_mismatch",
            Self::GenesisSnapshotMismatch => "native_train_checkpoint_v3_genesis_snapshot_mismatch",
            Self::LogicalStateDigestMismatch => {
                "native_train_checkpoint_v3_logical_state_digest_mismatch"
            }
            Self::TrainedEvidenceContextRequired => "trained_evidence_context_required",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointManifestV3Error {
    kind: CheckpointManifestV3ErrorKind,
}

impl CheckpointManifestV3Error {
    const fn new(kind: CheckpointManifestV3ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> CheckpointManifestV3ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for CheckpointManifestV3Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(CheckpointManifestV3ErrorKind::CanonicalJson(error.kind()))
    }
}

impl Display for CheckpointManifestV3Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for CheckpointManifestV3Error {}

type Result<T> = std::result::Result<T, CheckpointManifestV3Error>;

/// Builds and validates the exact update-zero checkpoint authority from one
/// complete common-snapshot train-state payload.
pub fn build_genesis_checkpoint_manifest_v3(
    run: &ValidatedTrainRunV2,
    payload: &[u8],
) -> Result<CheckpointManifestV3> {
    if payload.len() != NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::PayloadExactLength,
        ));
    }
    let record = run.record();
    let anchor = u32::try_from(record.model_snapshot.scorer_bias_anchor_f32_bits)
        .map_err(|_| CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::CrossBinding))?;
    let decoded =
        decode_native_train_state_payload_v1(payload, 0, anchor).map_err(map_payload_error_v3)?;
    let progress =
        zero_checkpoint_progress_v3(run.batch_episodes(), run.checkpoint_segment_updates());
    let logical_state_sha256 = logical_state_sha256_v1(
        run.run_sha256(),
        0,
        &progress,
        decoded.digests.native_state_sha256,
    )?;
    let wire = CheckpointManifestWireV3 {
        schema: CHECKPOINT_MANIFEST_SCHEMA_V3.to_owned(),
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        segment_ordinal: 0,
        generation_index: 0,
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates: run.checkpoint_segment_updates(),
        progress,
        train_state: CheckpointTrainStateBindingV3 {
            schema: NATIVE_POLICY_VALUE_TRAIN_STATE_SCHEMA_V1.to_owned(),
            adam_step: 0,
            scorer_bias_anchor_f32_bits: u64::from(anchor),
            parameter_layout_sha256: record.model_snapshot.parameter_layout_sha256.clone(),
            parameter_tensor_count: PARAMETER_TENSOR_COUNT_U64_V3,
            parameter_element_count: PARAMETER_ELEMENT_COUNT_U64_V3,
            model_parameter_sha256: lower_hex_raw32_v1(decoded.digests.model_parameter_sha256),
            state_sha256: lower_hex_raw32_v1(decoded.digests.native_state_sha256),
        },
        payload: payload_binding_v1(&decoded.digests)?,
        logical_state_sha256: lower_hex_raw32_v1(logical_state_sha256),
    };
    let canonical_bytes = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    decode_checkpoint_manifest_v3(&canonical_bytes, payload, run)
}

/// Decodes a pure checkpoint-v3 authority.  The current public authority is
/// deliberately genesis-only; any nonzero generation requires the future
/// sealed cumulative Episode/UpdateGroup evidence context.
pub fn decode_checkpoint_manifest_v3(
    manifest_cj: &[u8],
    payload: &[u8],
    run: &ValidatedTrainRunV2,
) -> Result<CheckpointManifestV3> {
    if manifest_cj.len() > CHECKPOINT_MANIFEST_MAX_BYTES_V3 {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::RecordTooLarge,
        ));
    }
    let wire: CheckpointManifestWireV3 =
        from_canonical_json_bytes_v1(manifest_cj, CanonicalJsonNullPolicyV1::Forbid)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != manifest_cj {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NonCanonicalBytes,
            ),
        ));
    }
    if wire.schema != CHECKPOINT_MANIFEST_SCHEMA_V3 {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::InvalidSchema,
        ));
    }
    if !is_u63_v3(wire.generation_index) {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::InvalidScalar,
        ));
    }
    if wire.generation_index != 0 {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::TrainedEvidenceContextRequired,
        ));
    }

    validate_genesis_wire_v3(&wire, run)?;
    let decoded = validate_payload_v3(&wire, payload, run)?;
    validate_genesis_snapshot_v3(&wire, &decoded, run)?;

    let logical_state_sha256 = logical_state_sha256_v1(
        &wire.run_sha256,
        wire.generation_index,
        &wire.progress,
        decoded.digests.native_state_sha256,
    )?;
    let declared_logical = parse_digest_v3(&wire.logical_state_sha256)?;
    if logical_state_sha256 != declared_logical {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::LogicalStateDigestMismatch,
        ));
    }

    Ok(CheckpointManifestV3 {
        checkpoint_manifest_sha256: sha256_v1(manifest_cj),
        checkpoint_payload_sha256: decoded.digests.payload_sha256,
        logical_state_sha256,
        model_parameter_sha256: decoded.digests.model_parameter_sha256,
        train_state_sha256: decoded.digests.native_state_sha256,
        wire,
        canonical_bytes: reencoded,
    })
}

/// Explicit genesis spelling for callers that never accept trained input.
pub fn decode_genesis_checkpoint_manifest_v3(
    manifest_cj: &[u8],
    payload: &[u8],
    run: &ValidatedTrainRunV2,
) -> Result<CheckpointManifestV3> {
    decode_checkpoint_manifest_v3(manifest_cj, payload, run)
}

fn validate_genesis_wire_v3(
    wire: &CheckpointManifestWireV3,
    run: &ValidatedTrainRunV2,
) -> Result<()> {
    let record = run.record();
    let progress = &wire.progress;
    let outcomes = &progress.outcomes_by_learner_seat;
    let counters = [
        wire.segment_ordinal,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
        progress.batch_episodes,
        progress.checkpoint_segment_updates,
        progress.next_episode_index,
        progress.successful_update_count,
        progress.completed_episode_count,
        outcomes.p0.win,
        outcomes.p0.loss,
        outcomes.p0.draw,
        outcomes.p1.win,
        outcomes.p1.loss,
        outcomes.p1.draw,
        progress.learner_policy_steps_by_seat.p0,
        progress.learner_policy_steps_by_seat.p1,
        progress.learner_physical_decisions_by_seat.p0,
        progress.learner_physical_decisions_by_seat.p1,
        wire.train_state.adam_step,
        wire.train_state.parameter_tensor_count,
        wire.train_state.parameter_element_count,
        wire.payload.byte_count,
    ];
    if counters.into_iter().any(|value| !is_u63_v3(value))
        || wire.train_state.scorer_bias_anchor_f32_bits > u64::from(u32::MAX)
    {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::InvalidScalar,
        ));
    }
    validate_all_digest_encodings_v3(wire)?;

    if wire.run_sha256 != run.run_sha256()
        || wire.identity_bundle_sha256 != run.identity_bundle_sha256()
        || wire.batch_episodes != run.batch_episodes()
        || wire.checkpoint_segment_updates != run.checkpoint_segment_updates()
        || progress.batch_episodes != wire.batch_episodes
        || progress.checkpoint_segment_updates != wire.checkpoint_segment_updates
        || wire.generation_index > run.requested_successful_updates()
        || wire.schema != record.artifact_schemas.checkpoint
        || wire.payload.schema != record.artifact_schemas.state_payload
        || wire.payload.encoding != record.publication.state_payload
        || wire.train_state.schema != NATIVE_POLICY_VALUE_TRAIN_STATE_SCHEMA_V1
        || wire.train_state.parameter_layout_sha256
            != record.contracts.model.parameter_layout_sha256
        || wire.train_state.parameter_layout_sha256 != record.model_snapshot.parameter_layout_sha256
        || wire.train_state.parameter_tensor_count != PARAMETER_TENSOR_COUNT_U64_V3
        || wire.train_state.parameter_tensor_count != record.contracts.model.parameter_tensor_count
        || wire.train_state.parameter_tensor_count != record.model_snapshot.parameter_tensor_count
        || wire.train_state.parameter_element_count != PARAMETER_ELEMENT_COUNT_U64_V3
        || wire.train_state.parameter_element_count
            != record.contracts.model.parameter_element_count
        || wire.train_state.parameter_element_count != record.model_snapshot.parameter_element_count
        || wire.train_state.scorer_bias_anchor_f32_bits
            != record.model_snapshot.scorer_bias_anchor_f32_bits
        || wire.payload.byte_count != TRAIN_STATE_PAYLOAD_BYTE_COUNT_U64_V1
    {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::CrossBinding,
        ));
    }

    let s = wire.checkpoint_segment_updates;
    if s == 0 {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::InvalidScalar,
        ));
    }
    if !wire.generation_index.is_multiple_of(s) {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::CrossBinding,
        ));
    }
    let expected_segment_ordinal = wire.generation_index / s;
    let expected_episode_count = checked_u63_mul_v3(wire.generation_index, wire.batch_episodes)?;
    let expected_seat_episodes = expected_episode_count / 2;
    let p0_outcomes = checked_outcome_sum_v3(&outcomes.p0)?;
    let p1_outcomes = checked_outcome_sum_v3(&outcomes.p1)?;
    if wire.segment_ordinal != expected_segment_ordinal
        || wire.train_state.adam_step != wire.generation_index
        || progress.successful_update_count != wire.generation_index
        || progress.next_episode_index != expected_episode_count
        || progress.completed_episode_count != expected_episode_count
        || p0_outcomes != expected_seat_episodes
        || p1_outcomes != expected_seat_episodes
        || progress.learner_policy_steps_by_seat.p0 < progress.learner_physical_decisions_by_seat.p0
        || progress.learner_policy_steps_by_seat.p1 < progress.learner_physical_decisions_by_seat.p1
    {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::CrossBinding,
        ));
    }
    if wire.generation_index == 0
        && (wire.segment_ordinal != 0
            || progress.next_episode_index != 0
            || progress.successful_update_count != 0
            || progress.completed_episode_count != 0
            || p0_outcomes != 0
            || p1_outcomes != 0
            || progress.learner_policy_steps_by_seat.p0 != 0
            || progress.learner_policy_steps_by_seat.p1 != 0
            || progress.learner_physical_decisions_by_seat.p0 != 0
            || progress.learner_physical_decisions_by_seat.p1 != 0)
    {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::CrossBinding,
        ));
    }
    validate_payload_layout_v1(&wire.payload)
}

fn validate_payload_layout_v1(payload: &CheckpointPayloadBindingV1) -> Result<()> {
    if payload.schema != NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1
        || payload.encoding != NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1
        || payload.byte_count != TRAIN_STATE_PAYLOAD_BYTE_COUNT_U64_V1
    {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::CrossBinding,
        ));
    }
    for (declared, expected) in payload
        .sections
        .iter()
        .zip(NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1)
    {
        let expected_offset = u64::try_from(expected.offset_bytes).map_err(|_| {
            CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
        })?;
        let expected_count = u64::try_from(expected.byte_count).map_err(|_| {
            CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
        })?;
        if declared.name != expected.name
            || declared.offset_bytes != expected_offset
            || declared.byte_count != expected_count
        {
            return Err(CheckpointManifestV3Error::new(
                CheckpointManifestV3ErrorKind::CrossBinding,
            ));
        }
    }
    Ok(())
}

fn validate_payload_v3(
    wire: &CheckpointManifestWireV3,
    payload: &[u8],
    _run: &ValidatedTrainRunV2,
) -> Result<NativeDecodedTrainStatePayloadV1> {
    if payload.len() != NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::PayloadExactLength,
        ));
    }
    let expected = NativeTrainStatePayloadDigestsV1 {
        payload_sha256: parse_digest_v3(&wire.payload.sha256)?,
        parameters_sha256: parse_digest_v3(&wire.payload.sections[0].sha256)?,
        first_moments_sha256: parse_digest_v3(&wire.payload.sections[1].sha256)?,
        second_moments_sha256: parse_digest_v3(&wire.payload.sections[2].sha256)?,
        model_parameter_sha256: parse_digest_v3(&wire.train_state.model_parameter_sha256)?,
        native_state_sha256: parse_digest_v3(&wire.train_state.state_sha256)?,
    };
    let anchor = u32::try_from(wire.train_state.scorer_bias_anchor_f32_bits).map_err(|_| {
        CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidScalar)
    })?;
    decode_native_train_state_payload_verified_v1(
        payload,
        wire.train_state.adam_step,
        anchor,
        &expected,
    )
    .map_err(map_payload_error_v3)
}

fn validate_genesis_snapshot_v3(
    wire: &CheckpointManifestWireV3,
    decoded: &NativeDecodedTrainStatePayloadV1,
    run: &ValidatedTrainRunV2,
) -> Result<()> {
    let snapshot = &run.record().model_snapshot;
    let expected_parameter_payload = parse_digest_v3(&snapshot.payload_sha256)?;
    let expected_named_parameters = parse_digest_v3(&snapshot.named_parameter_stream_sha256)?;
    let expected_loaded_parameters =
        parse_digest_v3(&snapshot.loaded_named_parameter_stream_sha256)?;
    let all_moments_positive_zero = decoded
        .snapshot
        .first_moments
        .iter()
        .chain(&decoded.snapshot.second_moments)
        .flat_map(|parameter| &parameter.values)
        .all(|value| value.to_bits() == 0);
    if wire.generation_index != 0
        || decoded.snapshot.adam_step != 0
        || decoded.digests.parameters_sha256 != expected_parameter_payload
        || decoded.digests.model_parameter_sha256 != expected_named_parameters
        || decoded.digests.model_parameter_sha256 != expected_loaded_parameters
        || !all_moments_positive_zero
    {
        return Err(CheckpointManifestV3Error::new(
            CheckpointManifestV3ErrorKind::GenesisSnapshotMismatch,
        ));
    }
    Ok(())
}

fn validate_all_digest_encodings_v3(wire: &CheckpointManifestWireV3) -> Result<()> {
    for digest in [
        &wire.run_sha256,
        &wire.identity_bundle_sha256,
        &wire.train_state.parameter_layout_sha256,
        &wire.train_state.model_parameter_sha256,
        &wire.train_state.state_sha256,
        &wire.payload.sha256,
        &wire.payload.sections[0].sha256,
        &wire.payload.sections[1].sha256,
        &wire.payload.sections[2].sha256,
        &wire.logical_state_sha256,
    ] {
        parse_digest_v3(digest)?;
    }
    Ok(())
}

fn payload_binding_v1(
    digests: &NativeTrainStatePayloadDigestsV1,
) -> Result<CheckpointPayloadBindingV1> {
    let section_digests = [
        digests.parameters_sha256,
        digests.first_moments_sha256,
        digests.second_moments_sha256,
    ];
    let sections = [
        payload_section_binding_v1(0, section_digests[0])?,
        payload_section_binding_v1(1, section_digests[1])?,
        payload_section_binding_v1(2, section_digests[2])?,
    ];
    Ok(CheckpointPayloadBindingV1 {
        schema: NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1.to_owned(),
        encoding: NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1.to_owned(),
        byte_count: TRAIN_STATE_PAYLOAD_BYTE_COUNT_U64_V1,
        sha256: lower_hex_raw32_v1(digests.payload_sha256),
        sections,
    })
}

fn payload_section_binding_v1(
    index: usize,
    digest: [u8; 32],
) -> Result<CheckpointPayloadSectionBindingV1> {
    let layout = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[index];
    Ok(CheckpointPayloadSectionBindingV1 {
        name: layout.name.to_owned(),
        offset_bytes: u64::try_from(layout.offset_bytes).map_err(|_| {
            CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
        })?,
        byte_count: u64::try_from(layout.byte_count).map_err(|_| {
            CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
        })?,
        sha256: lower_hex_raw32_v1(digest),
    })
}

fn zero_checkpoint_progress_v3(batch_episodes: u64, segment_updates: u64) -> CheckpointProgressV3 {
    let zero_outcomes = CheckpointOutcomeCountsV3 {
        win: 0,
        loss: 0,
        draw: 0,
    };
    let zero_seat_counters = CheckpointLearnerSeatCountersV3 { p0: 0, p1: 0 };
    CheckpointProgressV3 {
        batch_episodes,
        checkpoint_segment_updates: segment_updates,
        next_episode_index: 0,
        successful_update_count: 0,
        completed_episode_count: 0,
        outcomes_by_learner_seat: CheckpointOutcomesByLearnerSeatV3 {
            p0: zero_outcomes,
            p1: zero_outcomes,
        },
        learner_policy_steps_by_seat: zero_seat_counters,
        learner_physical_decisions_by_seat: zero_seat_counters,
    }
}

fn logical_state_sha256_v1(
    run_sha256: &str,
    generation_index: u64,
    progress: &CheckpointProgressV3,
    train_state_sha256: [u8; 32],
) -> Result<[u8; 32]> {
    let run_sha256 = parse_digest_v3(run_sha256)?;
    let progress_cj = to_canonical_json_bytes_v1(progress, CanonicalJsonNullPolicyV1::Forbid)?;
    let mut digest = NativeTrainingStoreAtomSha256V1::new();
    digest
        .atom("domain", CHECKPOINT_LOGICAL_STATE_IDENTITY_V1.as_bytes())
        .map_err(map_digest_error_v3)?;
    digest
        .atom("run_sha256", &run_sha256)
        .map_err(map_digest_error_v3)?;
    digest
        .atom("generation_index_u64be", &generation_index.to_be_bytes())
        .map_err(map_digest_error_v3)?;
    digest
        .atom("progress_canonical_json", &progress_cj)
        .map_err(map_digest_error_v3)?;
    digest
        .atom("train_state_sha256", &train_state_sha256)
        .map_err(map_digest_error_v3)?;
    Ok(digest.finalize())
}

fn checked_outcome_sum_v3(counts: &CheckpointOutcomeCountsV3) -> Result<u64> {
    counts
        .win
        .checked_add(counts.loss)
        .and_then(|sum| sum.checked_add(counts.draw))
        .filter(|sum| is_u63_v3(*sum))
        .ok_or_else(|| {
            CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
        })
}

fn checked_u63_mul_v3(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| is_u63_v3(*value))
        .ok_or_else(|| {
            CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
        })
}

fn is_u63_v3(value: u64) -> bool {
    value <= U63_MAX_V3
}

fn parse_digest_v3(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value)
        .map_err(|_| CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidDigest))
}

fn map_digest_error_v3(_error: NativeTrainingStoreDigestErrorV1) -> CheckpointManifestV3Error {
    CheckpointManifestV3Error::new(CheckpointManifestV3ErrorKind::InvalidArithmetic)
}

fn map_payload_error_v3(error: NativeTrainStatePayloadErrorV1) -> CheckpointManifestV3Error {
    let kind = match error {
        NativeTrainStatePayloadErrorV1::ExactLength { .. } => {
            CheckpointManifestV3ErrorKind::PayloadExactLength
        }
        NativeTrainStatePayloadErrorV1::DigestMismatch(
            NativeTrainStatePayloadDigestFieldV1::Payload
            | NativeTrainStatePayloadDigestFieldV1::Parameters
            | NativeTrainStatePayloadDigestFieldV1::FirstMoments
            | NativeTrainStatePayloadDigestFieldV1::SecondMoments
            | NativeTrainStatePayloadDigestFieldV1::ModelParameters
            | NativeTrainStatePayloadDigestFieldV1::NativeState,
        ) => CheckpointManifestV3ErrorKind::PayloadDigestMismatch,
        NativeTrainStatePayloadErrorV1::LayoutInvariant(_)
        | NativeTrainStatePayloadErrorV1::TrainState(_) => {
            CheckpointManifestV3ErrorKind::PayloadInvalid
        }
    };
    CheckpointManifestV3Error::new(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_json_v1::to_canonical_json_bytes_v1;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_policy_train_step_v1::native_train_state_parameter_layout_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use std::time::Duration;

    const GENESIS_MANIFEST_SHA256_GOLDEN_V3: &str =
        "2ae7e16e7e1f52478b1d8fc12d88ff4bc1bfcfaa4855273db4fdffabb2bc8286";
    const GENESIS_PAYLOAD_SHA256_GOLDEN_V1: &str =
        "3c83802885e13c118ebcf870de2d3c9f2209e9e9c47b66a8dac5e5232d1c9c43";
    const GENESIS_LOGICAL_STATE_SHA256_GOLDEN_V1: &str =
        "4306c612de240410aaf5f1603562bf659a49102a740b1ff3de9b71adff68d0bd";
    const GENESIS_TRAIN_STATE_SHA256_GOLDEN_V1: &str =
        "5854b477e2ce22dda199b5c9442824a339acd15d7eb8666f19895aa0d7c53c26";

    struct FixtureV3 {
        run_bytes: Vec<u8>,
        payload: Vec<u8>,
        manifest: Vec<u8>,
    }

    static FIXTURE_V3: OnceLock<FixtureV3> = OnceLock::new();

    fn fixture_v3() -> &'static FixtureV3 {
        FIXTURE_V3.get_or_init(|| {
            let run_bytes = test_fixture_bytes_v2();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
            let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
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
                },
                &snapshot_manifest,
                &snapshot_payload,
            )
            .unwrap();
            let checkpoint = executor.checkpoint_candidate_v1().unwrap();
            let payload = checkpoint.payload().to_vec();
            let authority = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
            FixtureV3 {
                run_bytes,
                payload,
                manifest: authority.canonical_bytes().to_vec(),
            }
        })
    }

    fn run_v3() -> ValidatedTrainRunV2 {
        decode_train_run_v2(&fixture_v3().run_bytes).unwrap()
    }

    fn manifest_value_v3() -> Value {
        serde_json::from_slice(
            fixture_v3()
                .manifest
                .strip_suffix(b"\n")
                .expect("fixture is canonical JSON"),
        )
        .unwrap()
    }

    fn canonical_value_bytes_v3(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, CanonicalJsonNullPolicyV1::Forbid).unwrap()
    }

    fn decode_value_error_v3(value: &Value) -> CheckpointManifestV3ErrorKind {
        decode_genesis_checkpoint_manifest_v3(
            &canonical_value_bytes_v3(value),
            &fixture_v3().payload,
            &run_v3(),
        )
        .unwrap_err()
        .kind()
    }

    fn tensor_offset_v1(name: &str) -> usize {
        let mut offset = 0_usize;
        for (candidate, shape) in native_train_state_parameter_layout_v1() {
            if candidate == name {
                return offset;
            }
            offset = offset
                .checked_add(shape.iter().product::<usize>() * 4)
                .unwrap();
        }
        panic!("unknown tensor")
    }

    #[test]
    fn genesis_authority_roundtrips_and_matches_frozen_goldens() {
        let fixture = fixture_v3();
        let run = run_v3();
        let authority =
            decode_genesis_checkpoint_manifest_v3(&fixture.manifest, &fixture.payload, &run)
                .unwrap();
        assert_eq!(authority.canonical_bytes(), fixture.manifest);
        assert_eq!(authority.generation_index(), 0);
        assert_eq!(authority.segment_ordinal(), 0);
        assert_eq!(authority.batch_episodes(), 2);
        assert_eq!(authority.checkpoint_segment_updates(), 4);
        assert_eq!(authority.progress().successful_update_count(), 0);
        assert_eq!(authority.progress().next_episode_index(), 0);
        assert_eq!(
            lower_hex_raw32_v1(authority.checkpoint_manifest_sha256()),
            GENESIS_MANIFEST_SHA256_GOLDEN_V3
        );
        assert_eq!(
            lower_hex_raw32_v1(authority.checkpoint_payload_sha256()),
            GENESIS_PAYLOAD_SHA256_GOLDEN_V1
        );
        assert_eq!(
            lower_hex_raw32_v1(authority.logical_state_sha256()),
            GENESIS_LOGICAL_STATE_SHA256_GOLDEN_V1
        );
        assert_eq!(
            lower_hex_raw32_v1(authority.train_state_sha256()),
            GENESIS_TRAIN_STATE_SHA256_GOLDEN_V1
        );
        assert_eq!(
            authority.model_parameter_sha256(),
            parse_lower_hex_raw32_v1(&run.record().model_snapshot.named_parameter_stream_sha256)
                .unwrap()
        );
    }

    #[test]
    fn logical_state_digest_matches_an_independent_atom_reference() {
        fn atom(reference: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            reference.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(tag.as_bytes());
            reference.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            reference.extend_from_slice(payload);
        }

        let fixture = fixture_v3();
        let run = run_v3();
        let authority =
            decode_genesis_checkpoint_manifest_v3(&fixture.manifest, &fixture.payload, &run)
                .unwrap();
        let progress_cj =
            to_canonical_json_bytes_v1(authority.progress(), CanonicalJsonNullPolicyV1::Forbid)
                .unwrap();
        let mut reference = Vec::new();
        atom(
            &mut reference,
            "domain",
            CHECKPOINT_LOGICAL_STATE_IDENTITY_V1.as_bytes(),
        );
        atom(
            &mut reference,
            "run_sha256",
            &parse_lower_hex_raw32_v1(run.run_sha256()).unwrap(),
        );
        atom(
            &mut reference,
            "generation_index_u64be",
            &0_u64.to_be_bytes(),
        );
        atom(&mut reference, "progress_canonical_json", &progress_cj);
        atom(
            &mut reference,
            "train_state_sha256",
            &authority.train_state_sha256(),
        );
        let expected: [u8; 32] = Sha256::digest(&reference).into();
        assert_eq!(authority.logical_state_sha256(), expected);
    }

    #[test]
    fn canonical_closed_schema_and_no_null_policy_fail_closed() {
        let run = run_v3();
        let fixture = fixture_v3();
        let mut unknown = manifest_value_v3();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_value_error_v3(&unknown),
            CheckpointManifestV3ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let mut nested_unknown = manifest_value_v3();
        nested_unknown["progress"]
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), json!(1));
        assert_eq!(
            decode_value_error_v3(&nested_unknown),
            CheckpointManifestV3ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let mut missing = manifest_value_v3();
        missing
            .as_object_mut()
            .unwrap()
            .remove("logical_state_sha256");
        assert_eq!(
            decode_value_error_v3(&missing),
            CheckpointManifestV3ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let canonical = String::from_utf8(fixture.manifest.clone()).unwrap();
        let noncanonical = canonical.replacen(":", ": ", 1);
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(noncanonical.as_bytes(), &fixture.payload, &run,)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NonCanonicalBytes
            )
        );
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(
                &fixture.manifest[..fixture.manifest.len() - 1],
                &fixture.payload,
                &run,
            )
            .unwrap_err()
            .kind(),
            CheckpointManifestV3ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::MissingFinalLf)
        );
        let mut trailing = fixture.manifest.clone();
        trailing.push(b' ');
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(&trailing, &fixture.payload, &run)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::MissingFinalLf)
        );
        let duplicate = canonical.replacen(
            "{",
            "{\"schema\":\"mtg_kernel_native_train_checkpoint/v3\",",
            1,
        );
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(duplicate.as_bytes(), &fixture.payload, &run,)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::DuplicateObjectKey
            )
        );
        let null = canonical.replacen("\"segment_ordinal\":0", "\"segment_ordinal\":null", 1);
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(null.as_bytes(), &fixture.payload, &run)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::NullForbidden)
        );
        let float = canonical.replacen("\"segment_ordinal\":0", "\"segment_ordinal\":0.0", 1);
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(float.as_bytes(), &fixture.payload, &run)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::FloatingPointForbidden
            )
        );
    }

    #[test]
    fn record_cap_and_payload_exact_length_are_preconditions() {
        let run = run_v3();
        let fixture = fixture_v3();
        let oversized = vec![b' '; CHECKPOINT_MANIFEST_MAX_BYTES_V3 + 1];
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(&oversized, &fixture.payload, &run)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::RecordTooLarge
        );
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(
                &fixture.manifest,
                &fixture.payload[..fixture.payload.len() - 1],
                &run,
            )
            .unwrap_err()
            .kind(),
            CheckpointManifestV3ErrorKind::PayloadExactLength
        );
        let mut extended = fixture.payload.clone();
        extended.push(0);
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(&fixture.manifest, &extended, &run)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::PayloadExactLength
        );
    }

    #[test]
    fn digest_scalar_run_and_generation_corruption_fail_closed() {
        let mut value = manifest_value_v3();
        value["schema"] = json!("mtg_kernel_native_train_checkpoint/v2");
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::InvalidSchema
        );

        let mut value = manifest_value_v3();
        value["run_sha256"] = json!("A".repeat(64));
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::InvalidDigest
        );
        let mut value = manifest_value_v3();
        value["run_sha256"] = json!("0".repeat(63));
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::InvalidDigest
        );

        let mut value = manifest_value_v3();
        value["segment_ordinal"] = json!(U63_MAX_V3 + 1);
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::InvalidScalar
        );

        let mut value = manifest_value_v3();
        value["run_sha256"] = json!("00".repeat(32));
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::CrossBinding
        );

        let mut value = manifest_value_v3();
        value["identity_bundle_sha256"] = json!("00".repeat(32));
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::CrossBinding
        );

        let mut value = manifest_value_v3();
        value["generation_index"] = json!(4);
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::TrainedEvidenceContextRequired
        );
        assert_eq!(
            CheckpointManifestV3ErrorKind::TrainedEvidenceContextRequired.code(),
            "trained_evidence_context_required"
        );
    }

    #[test]
    fn progress_train_state_and_layout_cross_bindings_fail_closed() {
        for path in [
            &["segment_ordinal"][..],
            &["batch_episodes"],
            &["checkpoint_segment_updates"],
            &["progress", "batch_episodes"],
            &["progress", "checkpoint_segment_updates"],
            &["progress", "next_episode_index"],
            &["progress", "successful_update_count"],
            &["progress", "completed_episode_count"],
            &["progress", "outcomes_by_learner_seat", "p0", "win"],
            &["progress", "learner_policy_steps_by_seat", "p0"],
            &["train_state", "adam_step"],
        ] {
            let mut value = manifest_value_v3();
            let mut cursor = &mut value;
            for key in &path[..path.len() - 1] {
                cursor = &mut cursor[*key];
            }
            cursor[path[path.len() - 1]] = json!(1);
            assert_eq!(
                decode_value_error_v3(&value),
                CheckpointManifestV3ErrorKind::CrossBinding,
                "path {path:?}"
            );
        }

        let mut value = manifest_value_v3();
        value["train_state"]["scorer_bias_anchor_f32_bits"] = json!(u64::from(u32::MAX) + 1);
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::InvalidScalar
        );
        let mut value = manifest_value_v3();
        let anchor = value["train_state"]["scorer_bias_anchor_f32_bits"]
            .as_u64()
            .unwrap();
        value["train_state"]["scorer_bias_anchor_f32_bits"] = json!(anchor ^ 1);
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::CrossBinding
        );

        for (path, replacement) in [
            (
                &["train_state", "schema"][..],
                json!("mtg_kernel_native_policy_value_train_state/v0"),
            ),
            (
                &["train_state", "parameter_layout_sha256"][..],
                json!("00".repeat(32)),
            ),
            (&["train_state", "parameter_tensor_count"][..], json!(32)),
            (
                &["train_state", "parameter_element_count"][..],
                json!(1_230_993),
            ),
            (
                &["payload", "schema"][..],
                json!("mtg_kernel_native_train_state_payload/v0"),
            ),
            (&["payload", "encoding"][..], json!("wrong/v1")),
            (&["payload", "byte_count"][..], json!(14_771_927)),
        ] {
            let mut value = manifest_value_v3();
            value[path[0]][path[1]] = replacement;
            assert_eq!(
                decode_value_error_v3(&value),
                CheckpointManifestV3ErrorKind::CrossBinding,
                "path {path:?}"
            );
        }

        let mut value = manifest_value_v3();
        value["payload"]["sections"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::CrossBinding
        );
        let mut value = manifest_value_v3();
        value["payload"]["sections"][1]["offset_bytes"] = json!(0);
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::CrossBinding
        );
    }

    #[test]
    fn every_payload_and_logical_digest_is_enforced() {
        for path in [
            &["payload", "sha256"][..],
            &["payload", "sections", "0", "sha256"],
            &["payload", "sections", "1", "sha256"],
            &["payload", "sections", "2", "sha256"],
            &["train_state", "model_parameter_sha256"],
            &["train_state", "state_sha256"],
        ] {
            let mut value = manifest_value_v3();
            if path[1] == "sections" {
                let index = path[2].parse::<usize>().unwrap();
                value[path[0]][path[1]][index][path[3]] = json!("00".repeat(32));
            } else {
                value[path[0]][path[1]] = json!("00".repeat(32));
            }
            assert_eq!(
                decode_value_error_v3(&value),
                CheckpointManifestV3ErrorKind::PayloadDigestMismatch,
                "path {path:?}"
            );
        }
        let mut value = manifest_value_v3();
        value["logical_state_sha256"] = json!("00".repeat(32));
        assert_eq!(
            decode_value_error_v3(&value),
            CheckpointManifestV3ErrorKind::LogicalStateDigestMismatch
        );
    }

    #[test]
    fn genesis_rejects_self_consistent_parameter_and_moment_drift() {
        let run = run_v3();
        let fixture = fixture_v3();
        let mut changed_parameter = fixture.payload.clone();
        let parameter_offset = tensor_offset_v1("object_encoder.0.weight");
        let original = u32::from_le_bytes(
            changed_parameter[parameter_offset..parameter_offset + 4]
                .try_into()
                .unwrap(),
        );
        changed_parameter[parameter_offset..parameter_offset + 4]
            .copy_from_slice(&(original ^ 1).to_le_bytes());
        assert_eq!(
            build_genesis_checkpoint_manifest_v3(&run, &changed_parameter)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::GenesisSnapshotMismatch
        );

        let mut changed_moment = fixture.payload.clone();
        let moment_offset = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[1].offset_bytes
            + tensor_offset_v1("object_encoder.0.weight");
        changed_moment[moment_offset..moment_offset + 4]
            .copy_from_slice(&0.25_f32.to_bits().to_le_bytes());
        assert_eq!(
            build_genesis_checkpoint_manifest_v3(&run, &changed_moment)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::GenesisSnapshotMismatch
        );

        let mut invalid_second_moment = fixture.payload.clone();
        let second_moment_offset = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[2].offset_bytes
            + tensor_offset_v1("object_encoder.0.weight");
        invalid_second_moment[second_moment_offset..second_moment_offset + 4]
            .copy_from_slice(&(-f32::EPSILON).to_bits().to_le_bytes());
        assert_eq!(
            build_genesis_checkpoint_manifest_v3(&run, &invalid_second_moment)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::PayloadInvalid
        );
    }

    #[test]
    fn finite_payload_corruption_cannot_reuse_the_authoritative_manifest() {
        let fixture = fixture_v3();
        let run = run_v3();
        let mut corrupted = fixture.payload.clone();
        let offset = tensor_offset_v1("object_encoder.0.weight");
        corrupted[offset] ^= 1;
        assert_eq!(
            decode_genesis_checkpoint_manifest_v3(&fixture.manifest, &corrupted, &run)
                .unwrap_err()
                .kind(),
            CheckpointManifestV3ErrorKind::PayloadDigestMismatch
        );
    }
}
