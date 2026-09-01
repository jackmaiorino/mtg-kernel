//! Checkpoint-manifest v4 sibling schema: adds the cell-centered advantage
//! baseline (`native_policy_baseline_state_v4`) to the checkpoint-v3 wire
//! shape.
//!
//! Contract: `docs/native_trainer_terminal_reinforce_value_v4_candidate_v1.md`
//! section 5 ("Baseline-state persistence"). The wire shape mirrors
//! `native_training_store_checkpoint_v3` exactly EXCEPT the train-state
//! binding, which additionally carries `baseline_schema` and
//! `baseline_cells`; the payload's frozen three-section `f32le` layout, the
//! progress grammar, and the logical-state atom list are the unchanged v3
//! forms (`CheckpointProgressV3` / `CheckpointPayloadBindingV1` reused
//! directly, unmodified).
//!
//! v4 semantics pinned by the contract: the manifest's `train_state_sha256`
//! (the `train_state.state_sha256` wire field, and this module's
//! `train_state_sha256()` accessor) holds the COMPOSED v4 hash --
//! `NativeBaselineStateV4::compose_train_state_sha256_v4(core_state_sha256)`
//! -- not the raw payload-derived snapshot hash. `logical_state_sha256`
//! keeps the identical v1 atom formula, folding that composed hash, so it
//! covers the baseline without any atom-list change.
//!
//! This is the pure checkpoint-manifest v4 SCHEMA sibling of
//! `native_training_store_checkpoint_v3`, not the store's run-bound
//! validator: it owns no `ValidatedTrainRunV2`/evidence-chain/candidate
//! cross-binding (those types have no v4 counterpart yet). A v4
//! update-group/store validation path (contract section 5, point 3) is
//! separate follow-on work.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::native_policy_baseline_state_v4::{
    BaselineCellWireV4, NativeBaselineErrorKindV4, NativeBaselineStateV4,
    NATIVE_BASELINE_STATE_SCHEMA_V4,
};
use crate::native_training_store_checkpoint_v3::{
    CheckpointPayloadBindingV1, CheckpointProgressV3, CHECKPOINT_LOGICAL_STATE_IDENTITY_V1,
    NATIVE_POLICY_VALUE_TRAIN_STATE_SCHEMA_V1,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const CHECKPOINT_MANIFEST_SCHEMA_V4: &str = "mtg_kernel_native_train_checkpoint/v4";
/// Mirrors `native_training_store_checkpoint_v3::CHECKPOINT_MANIFEST_MAX_BYTES_V3`.
/// The baseline cap (256 cells, each well under 200 wire bytes) leaves this
/// cap the same headroom it had before the baseline existed.
pub(crate) const CHECKPOINT_MANIFEST_MAX_BYTES_V4: usize = 2 * 1024 * 1024;

const U63_MAX_V4: u64 = (1_u64 << 63) - 1;

/// Sibling of `CheckpointTrainStateBindingV3`: identical fields, plus the
/// baseline schema tag and cell list (contract section 5). `state_sha256`
/// holds the COMPOSED v4 hash, not the raw payload snapshot hash -- see the
/// module doc comment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckpointTrainStateBindingV4 {
    pub(crate) schema: String,
    pub(crate) adam_step: u64,
    pub(crate) scorer_bias_anchor_f32_bits: u64,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) parameter_tensor_count: u64,
    pub(crate) parameter_element_count: u64,
    pub(crate) model_parameter_sha256: String,
    pub(crate) state_sha256: String,
    pub(crate) baseline_schema: String,
    pub(crate) baseline_cells: Vec<BaselineCellWireV4>,
}

impl CheckpointTrainStateBindingV4 {
    pub(crate) fn adam_step(&self) -> u64 {
        self.adam_step
    }

    pub(crate) fn scorer_bias_anchor_f32_bits(&self) -> u64 {
        self.scorer_bias_anchor_f32_bits
    }

    pub(crate) fn model_parameter_sha256(&self) -> &str {
        &self.model_parameter_sha256
    }

    /// The COMPOSED v4 train-state hash, hex-encoded.
    pub(crate) fn state_sha256(&self) -> &str {
        &self.state_sha256
    }

    pub(crate) fn baseline_schema(&self) -> &str {
        &self.baseline_schema
    }

    pub(crate) fn baseline_cells(&self) -> &[BaselineCellWireV4] {
        &self.baseline_cells
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CheckpointManifestWireV4 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    segment_ordinal: u64,
    generation_index: u64,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    progress: CheckpointProgressV3,
    train_state: CheckpointTrainStateBindingV4,
    payload: CheckpointPayloadBindingV1,
    logical_state_sha256: String,
}

/// Fully validated pure checkpoint-v4 authority. No public fields, no
/// `Deserialize`, no unchecked constructor -- only [`build_checkpoint_manifest_v4`]
/// and [`decode_checkpoint_manifest_v4`] can produce one.
pub(crate) struct CheckpointManifestV4 {
    wire: CheckpointManifestWireV4,
    canonical_bytes: Vec<u8>,
    checkpoint_manifest_sha256: [u8; 32],
    logical_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    baseline_state: NativeBaselineStateV4,
}

impl std::fmt::Debug for CheckpointManifestV4 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CheckpointManifestV4")
            .field("segment_ordinal", &self.wire.segment_ordinal)
            .field("generation_index", &self.wire.generation_index)
            .field("batch_episodes", &self.wire.batch_episodes)
            .field(
                "checkpoint_segment_updates",
                &self.wire.checkpoint_segment_updates,
            )
            .field("baseline_cell_count", &self.baseline_state.cell_count_v4())
            .field(
                "checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.checkpoint_manifest_sha256),
            )
            .finish_non_exhaustive()
    }
}

impl CheckpointManifestV4 {
    pub(crate) fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub(crate) fn run_sha256(&self) -> &str {
        &self.wire.run_sha256
    }

    pub(crate) fn identity_bundle_sha256(&self) -> &str {
        &self.wire.identity_bundle_sha256
    }

    pub(crate) fn segment_ordinal(&self) -> u64 {
        self.wire.segment_ordinal
    }

    pub(crate) fn generation_index(&self) -> u64 {
        self.wire.generation_index
    }

    pub(crate) fn batch_episodes(&self) -> u64 {
        self.wire.batch_episodes
    }

    pub(crate) fn checkpoint_segment_updates(&self) -> u64 {
        self.wire.checkpoint_segment_updates
    }

    pub(crate) fn progress(&self) -> &CheckpointProgressV3 {
        &self.wire.progress
    }

    pub(crate) fn train_state(&self) -> &CheckpointTrainStateBindingV4 {
        &self.wire.train_state
    }

    pub(crate) fn payload(&self) -> &CheckpointPayloadBindingV1 {
        &self.wire.payload
    }

    pub(crate) fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }

    pub(crate) fn logical_state_sha256(&self) -> [u8; 32] {
        self.logical_state_sha256
    }

    pub(crate) fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    /// The COMPOSED v4 hash: `NativeBaselineStateV4::compose_train_state_sha256_v4`
    /// applied to the unchanged v3-style core snapshot hash. This is what
    /// `logical_state_sha256` folds, so the logical hash covers the baseline.
    pub(crate) fn train_state_sha256(&self) -> [u8; 32] {
        self.train_state_sha256
    }

    /// The decoded, fully validated baseline state: cell order, wire
    /// format, finiteness, counts, and the 256-cell cap are all already
    /// enforced by `NativeBaselineStateV4::from_wire_v4`.
    pub(crate) fn baseline_state(&self) -> NativeBaselineStateV4 {
        self.baseline_state.clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckpointManifestV4ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidBaselineSchema,
    InvalidBaseline(NativeBaselineErrorKindV4),
    InvalidDigest,
    InvalidScalar,
    LogicalStateDigestMismatch,
}

impl CheckpointManifestV4ErrorKind {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_checkpoint_v4_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_train_checkpoint_v4_invalid_schema",
            Self::InvalidBaselineSchema => "native_train_checkpoint_v4_invalid_baseline_schema",
            Self::InvalidBaseline(_) => "native_train_checkpoint_v4_invalid_baseline",
            Self::InvalidDigest => "native_train_checkpoint_v4_invalid_digest",
            Self::InvalidScalar => "native_train_checkpoint_v4_invalid_scalar",
            Self::LogicalStateDigestMismatch => {
                "native_train_checkpoint_v4_logical_state_digest_mismatch"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckpointManifestV4Error {
    kind: CheckpointManifestV4ErrorKind,
}

impl CheckpointManifestV4Error {
    const fn new(kind: CheckpointManifestV4ErrorKind) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind(self) -> CheckpointManifestV4ErrorKind {
        self.kind
    }

    pub(crate) const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for CheckpointManifestV4Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(CheckpointManifestV4ErrorKind::CanonicalJson(error.kind()))
    }
}

impl Display for CheckpointManifestV4Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for CheckpointManifestV4Error {}

type Result<T> = std::result::Result<T, CheckpointManifestV4Error>;

/// Inputs to [`build_checkpoint_manifest_v4`]. Deliberately flat and
/// self-contained (no `ValidatedTrainRunV2`/evidence-chain dependency -- see
/// the module doc comment).
#[derive(Clone, Debug)]
pub(crate) struct CheckpointManifestPartsV4 {
    pub(crate) run_sha256: String,
    pub(crate) identity_bundle_sha256: String,
    pub(crate) segment_ordinal: u64,
    pub(crate) generation_index: u64,
    pub(crate) batch_episodes: u64,
    pub(crate) checkpoint_segment_updates: u64,
    pub(crate) progress: CheckpointProgressV3,
    pub(crate) adam_step: u64,
    pub(crate) scorer_bias_anchor_f32_bits: u64,
    pub(crate) parameter_layout_sha256: String,
    pub(crate) parameter_tensor_count: u64,
    pub(crate) parameter_element_count: u64,
    pub(crate) model_parameter_sha256: [u8; 32],
    /// The unchanged v3-style snapshot hash (the payload-derived
    /// `native_state_sha256`), BEFORE baseline composition. Never confused
    /// with the manifest's `train_state_sha256`, which is the COMPOSED v4
    /// hash derived from this plus `baseline`.
    pub(crate) core_state_sha256: [u8; 32],
    pub(crate) payload: CheckpointPayloadBindingV1,
    pub(crate) baseline: NativeBaselineStateV4,
}

fn build_wire_v4(parts: CheckpointManifestPartsV4) -> Result<CheckpointManifestWireV4> {
    let train_state_sha256 = parts
        .baseline
        .compose_train_state_sha256_v4(parts.core_state_sha256);
    let logical_state_sha256 = logical_state_sha256_v4(
        &parts.run_sha256,
        parts.generation_index,
        &parts.progress,
        train_state_sha256,
    )?;
    Ok(CheckpointManifestWireV4 {
        schema: CHECKPOINT_MANIFEST_SCHEMA_V4.to_owned(),
        run_sha256: parts.run_sha256,
        identity_bundle_sha256: parts.identity_bundle_sha256,
        segment_ordinal: parts.segment_ordinal,
        generation_index: parts.generation_index,
        batch_episodes: parts.batch_episodes,
        checkpoint_segment_updates: parts.checkpoint_segment_updates,
        progress: parts.progress,
        train_state: CheckpointTrainStateBindingV4 {
            schema: NATIVE_POLICY_VALUE_TRAIN_STATE_SCHEMA_V1.to_owned(),
            adam_step: parts.adam_step,
            scorer_bias_anchor_f32_bits: parts.scorer_bias_anchor_f32_bits,
            parameter_layout_sha256: parts.parameter_layout_sha256,
            parameter_tensor_count: parts.parameter_tensor_count,
            parameter_element_count: parts.parameter_element_count,
            model_parameter_sha256: lower_hex_raw32_v1(parts.model_parameter_sha256),
            state_sha256: lower_hex_raw32_v1(train_state_sha256),
            baseline_schema: NATIVE_BASELINE_STATE_SCHEMA_V4.to_owned(),
            baseline_cells: parts.baseline.to_wire_v4(),
        },
        payload: parts.payload,
        logical_state_sha256: lower_hex_raw32_v1(logical_state_sha256),
    })
}

/// Builds and validates a checkpoint-v4 authority from its parts: computes
/// the composed v4 train-state hash and the logical-state hash, canonically
/// encodes the wire form, then immediately round-trips it through
/// [`decode_checkpoint_manifest_v4`] so a built manifest can never diverge
/// from what its own decoder accepts.
pub(crate) fn build_checkpoint_manifest_v4(
    parts: CheckpointManifestPartsV4,
) -> Result<CheckpointManifestV4> {
    let wire = build_wire_v4(parts)?;
    let canonical_bytes = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    decode_checkpoint_manifest_v4(&canonical_bytes)
}

/// Decodes and fully validates a checkpoint-v4 authority: size cap,
/// canonical-JSON round trip (byte-identical re-encode), outer and
/// train-state schema strings, baseline schema and cell order/format/cap
/// (via `NativeBaselineStateV4::from_wire_v4`), digest encodings, scalar
/// bounds, and the logical-state digest recomputation.
pub(crate) fn decode_checkpoint_manifest_v4(manifest_cj: &[u8]) -> Result<CheckpointManifestV4> {
    if manifest_cj.len() > CHECKPOINT_MANIFEST_MAX_BYTES_V4 {
        return Err(CheckpointManifestV4Error::new(
            CheckpointManifestV4ErrorKind::RecordTooLarge,
        ));
    }
    let wire: CheckpointManifestWireV4 =
        from_canonical_json_bytes_v1(manifest_cj, CanonicalJsonNullPolicyV1::Forbid)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != manifest_cj {
        return Err(CheckpointManifestV4Error::new(
            CheckpointManifestV4ErrorKind::CanonicalJson(
                CanonicalJsonErrorKindV1::NonCanonicalBytes,
            ),
        ));
    }
    if wire.schema != CHECKPOINT_MANIFEST_SCHEMA_V4
        || wire.train_state.schema != NATIVE_POLICY_VALUE_TRAIN_STATE_SCHEMA_V1
    {
        return Err(CheckpointManifestV4Error::new(
            CheckpointManifestV4ErrorKind::InvalidSchema,
        ));
    }
    if wire.train_state.baseline_schema != NATIVE_BASELINE_STATE_SCHEMA_V4 {
        return Err(CheckpointManifestV4Error::new(
            CheckpointManifestV4ErrorKind::InvalidBaselineSchema,
        ));
    }

    let scalars = [
        wire.segment_ordinal,
        wire.generation_index,
        wire.batch_episodes,
        wire.checkpoint_segment_updates,
        wire.progress.batch_episodes(),
        wire.progress.checkpoint_segment_updates(),
        wire.progress.next_episode_index(),
        wire.progress.successful_update_count(),
        wire.progress.completed_episode_count(),
        wire.progress.outcomes_by_learner_seat().p0().win(),
        wire.progress.outcomes_by_learner_seat().p0().loss(),
        wire.progress.outcomes_by_learner_seat().p0().draw(),
        wire.progress.outcomes_by_learner_seat().p1().win(),
        wire.progress.outcomes_by_learner_seat().p1().loss(),
        wire.progress.outcomes_by_learner_seat().p1().draw(),
        wire.progress.learner_policy_steps_by_seat().p0(),
        wire.progress.learner_policy_steps_by_seat().p1(),
        wire.progress.learner_physical_decisions_by_seat().p0(),
        wire.progress.learner_physical_decisions_by_seat().p1(),
        wire.train_state.adam_step,
        wire.train_state.parameter_tensor_count,
        wire.train_state.parameter_element_count,
        wire.payload.byte_count,
    ];
    if scalars.into_iter().any(|value| !is_u63_v4(value))
        || wire.train_state.scorer_bias_anchor_f32_bits > u64::from(u32::MAX)
    {
        return Err(CheckpointManifestV4Error::new(
            CheckpointManifestV4ErrorKind::InvalidScalar,
        ));
    }

    let baseline_state = NativeBaselineStateV4::from_wire_v4(&wire.train_state.baseline_cells)
        .map_err(|error| {
            CheckpointManifestV4Error::new(CheckpointManifestV4ErrorKind::InvalidBaseline(
                error.kind_v4(),
            ))
        })?;

    validate_all_digest_encodings_v4(&wire)?;
    let train_state_sha256 = parse_digest_v4(&wire.train_state.state_sha256)?;
    let model_parameter_sha256 = parse_digest_v4(&wire.train_state.model_parameter_sha256)?;
    let logical_state_sha256 = logical_state_sha256_v4(
        &wire.run_sha256,
        wire.generation_index,
        &wire.progress,
        train_state_sha256,
    )?;
    let declared_logical = parse_digest_v4(&wire.logical_state_sha256)?;
    if logical_state_sha256 != declared_logical {
        return Err(CheckpointManifestV4Error::new(
            CheckpointManifestV4ErrorKind::LogicalStateDigestMismatch,
        ));
    }

    Ok(CheckpointManifestV4 {
        checkpoint_manifest_sha256: sha256_v1(manifest_cj),
        logical_state_sha256,
        model_parameter_sha256,
        train_state_sha256,
        baseline_state,
        wire,
        canonical_bytes: reencoded,
    })
}

/// Adapted from `native_training_store_checkpoint_v3::validate_all_digest_encodings_v3`
/// (private there): the same "every declared digest string parses as strict
/// lowercase raw32" sweep, over the v4 wire shape.
fn validate_all_digest_encodings_v4(wire: &CheckpointManifestWireV4) -> Result<()> {
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
        parse_digest_v4(digest)?;
    }
    Ok(())
}

/// Adapted from `native_training_store_checkpoint_v3::logical_state_sha256_v1`
/// (private there): identical atom framing, sharing the same domain constant
/// and progress type; the last atom is the v4-COMPOSED `train_state_sha256`
/// so the logical-state digest folds the baseline (contract section 5:
/// "`logical_state_sha256`'s atom list is unchanged because it already folds
/// `train_state_sha256`").
fn logical_state_sha256_v4(
    run_sha256: &str,
    generation_index: u64,
    progress: &CheckpointProgressV3,
    train_state_sha256: [u8; 32],
) -> Result<[u8; 32]> {
    let run_sha256 = parse_digest_v4(run_sha256)?;
    let progress_cj = to_canonical_json_bytes_v1(progress, CanonicalJsonNullPolicyV1::Forbid)?;
    let mut digest = NativeTrainingStoreAtomSha256V1::new();
    digest
        .atom("domain", CHECKPOINT_LOGICAL_STATE_IDENTITY_V1.as_bytes())
        .map_err(map_digest_error_v4)?;
    digest
        .atom("run_sha256", &run_sha256)
        .map_err(map_digest_error_v4)?;
    digest
        .atom("generation_index_u64be", &generation_index.to_be_bytes())
        .map_err(map_digest_error_v4)?;
    digest
        .atom("progress_canonical_json", &progress_cj)
        .map_err(map_digest_error_v4)?;
    digest
        .atom("train_state_sha256", &train_state_sha256)
        .map_err(map_digest_error_v4)?;
    Ok(digest.finalize())
}

/// Copied from `native_training_store_checkpoint_v3::is_u63_v3` (private,
/// trivial).
fn is_u63_v4(value: u64) -> bool {
    value <= U63_MAX_V4
}

/// Copied from `native_training_store_checkpoint_v3::parse_digest_v3`
/// (private, trivial).
fn parse_digest_v4(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value)
        .map_err(|_| CheckpointManifestV4Error::new(CheckpointManifestV4ErrorKind::InvalidDigest))
}

/// Copied from `native_training_store_checkpoint_v3::map_digest_error_v3`
/// (private, trivial).
fn map_digest_error_v4(_error: NativeTrainingStoreDigestErrorV1) -> CheckpointManifestV4Error {
    CheckpointManifestV4Error::new(CheckpointManifestV4ErrorKind::InvalidScalar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_baseline_state_v4::{
        BaselineCellKeyV4, BaselineObservationV4, BaselineRoleV4,
    };
    use crate::native_training_store_checkpoint_v3::{
        CheckpointLearnerSeatCountersV3, CheckpointOutcomeCountsV3,
        CheckpointOutcomesByLearnerSeatV3, CheckpointPayloadSectionBindingV1,
        CheckpointTrainStateBindingV3, CHECKPOINT_MANIFEST_SCHEMA_V3,
    };
    use serde_json::Value;

    fn fake_digest_v4(tag: u8) -> String {
        format!("{tag:02x}").repeat(32)
    }

    fn sample_progress_v4() -> CheckpointProgressV3 {
        let zero_outcomes = CheckpointOutcomeCountsV3 {
            win: 0,
            loss: 0,
            draw: 0,
        };
        let zero_seat = CheckpointLearnerSeatCountersV3 { p0: 0, p1: 0 };
        CheckpointProgressV3 {
            batch_episodes: 64,
            checkpoint_segment_updates: 8,
            next_episode_index: 0,
            successful_update_count: 0,
            completed_episode_count: 0,
            outcomes_by_learner_seat: CheckpointOutcomesByLearnerSeatV3 {
                p0: zero_outcomes,
                p1: zero_outcomes,
            },
            learner_policy_steps_by_seat: zero_seat,
            learner_physical_decisions_by_seat: zero_seat,
        }
    }

    fn sample_payload_v4() -> CheckpointPayloadBindingV1 {
        let section = |index: u8| CheckpointPayloadSectionBindingV1 {
            name: format!("section-{index}"),
            offset_bytes: u64::from(index) * 1000,
            byte_count: 1000,
            sha256: fake_digest_v4(index),
        };
        CheckpointPayloadBindingV1 {
            schema: "test-native-train-state-payload-schema/v1".to_owned(),
            encoding: "f32le".to_owned(),
            byte_count: 3000,
            sha256: fake_digest_v4(9),
            sections: [section(0), section(1), section(2)],
        }
    }

    fn baseline_with_two_cells_v4() -> NativeBaselineStateV4 {
        NativeBaselineStateV4::empty_v4()
            .apply_update_v4(&[
                BaselineObservationV4 {
                    key: BaselineCellKeyV4::new_v4(fake_digest_v4(1), BaselineRoleV4::P0)
                        .expect("key"),
                    residual_sum_f64: 30.0,
                    decision_count: 60,
                    episode_count: 32,
                },
                BaselineObservationV4 {
                    key: BaselineCellKeyV4::new_v4(fake_digest_v4(2), BaselineRoleV4::P1)
                        .expect("key"),
                    residual_sum_f64: -12.0,
                    decision_count: 48,
                    episode_count: 30,
                },
            ])
            .expect("apply update")
    }

    fn sample_parts_v4(
        baseline: NativeBaselineStateV4,
        generation_index: u64,
    ) -> CheckpointManifestPartsV4 {
        CheckpointManifestPartsV4 {
            run_sha256: fake_digest_v4(10),
            identity_bundle_sha256: fake_digest_v4(11),
            segment_ordinal: 0,
            generation_index,
            batch_episodes: 64,
            checkpoint_segment_updates: 8,
            progress: sample_progress_v4(),
            adam_step: generation_index,
            scorer_bias_anchor_f32_bits: 0,
            parameter_layout_sha256: fake_digest_v4(12),
            parameter_tensor_count: 33,
            parameter_element_count: 1_230_994,
            model_parameter_sha256: [0x42; 32],
            core_state_sha256: [0x99; 32],
            payload: sample_payload_v4(),
            baseline,
        }
    }

    #[test]
    fn round_trip_with_nonempty_baseline_v4() {
        let baseline = baseline_with_two_cells_v4();
        let manifest =
            build_checkpoint_manifest_v4(sample_parts_v4(baseline.clone(), 8)).expect("build");
        assert_eq!(manifest.generation_index(), 8);
        assert_eq!(manifest.baseline_state().cell_count_v4(), 2);
        assert_eq!(manifest.train_state().baseline_cells().len(), 2);

        let redecoded = decode_checkpoint_manifest_v4(manifest.canonical_bytes()).expect("decode");
        assert_eq!(redecoded.canonical_bytes(), manifest.canonical_bytes());
        assert_eq!(
            redecoded.train_state_sha256(),
            manifest.train_state_sha256()
        );
        assert_eq!(redecoded.baseline_state(), baseline);
    }

    #[test]
    fn round_trip_with_empty_baseline_is_genesis_v4() {
        let baseline = NativeBaselineStateV4::empty_v4();
        let core = [0x99_u8; 32];
        let mut parts = sample_parts_v4(baseline.clone(), 0);
        parts.core_state_sha256 = core;
        parts.adam_step = 0;
        let manifest = build_checkpoint_manifest_v4(parts).expect("build");

        assert_eq!(manifest.generation_index(), 0);
        assert_eq!(manifest.baseline_state().cell_count_v4(), 0);
        assert!(manifest.train_state().baseline_cells().is_empty());
        assert_eq!(
            manifest.train_state_sha256(),
            baseline.compose_train_state_sha256_v4(core)
        );
        // The composed hash must not degenerate to the bare core hash even
        // when the baseline is empty (domain separation).
        assert_ne!(manifest.train_state_sha256(), core);

        let redecoded = decode_checkpoint_manifest_v4(manifest.canonical_bytes()).expect("decode");
        assert_eq!(redecoded.canonical_bytes(), manifest.canonical_bytes());
    }

    #[test]
    fn composed_hash_matches_baseline_module_function_v4() {
        let baseline = baseline_with_two_cells_v4();
        let core = [0x55_u8; 32];
        let mut parts = sample_parts_v4(baseline.clone(), 4);
        parts.core_state_sha256 = core;
        let manifest = build_checkpoint_manifest_v4(parts).expect("build");
        assert_eq!(
            manifest.train_state_sha256(),
            baseline.compose_train_state_sha256_v4(core)
        );
    }

    #[test]
    fn canonical_bytes_are_stable_across_redecoding_v4() {
        let baseline = baseline_with_two_cells_v4();
        let manifest = build_checkpoint_manifest_v4(sample_parts_v4(baseline, 4)).expect("build");
        let bytes_a = manifest.canonical_bytes().to_vec();
        let redecoded = decode_checkpoint_manifest_v4(&bytes_a).expect("redecode");
        assert_eq!(redecoded.canonical_bytes(), bytes_a.as_slice());
    }

    /// "A v3 manifest fails the v4 decoder with a schema error": a
    /// v4-shaped document whose outer `schema` value is v3's constant.
    #[test]
    fn v3_schema_string_rejected_by_v4_decoder_v4() {
        let baseline = baseline_with_two_cells_v4();
        let mut wire = build_wire_v4(sample_parts_v4(baseline, 4)).expect("wire");
        wire.schema = CHECKPOINT_MANIFEST_SCHEMA_V3.to_owned();
        let bytes =
            to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid).expect("encode");
        let error = decode_checkpoint_manifest_v4(&bytes).expect_err("schema mismatch");
        assert_eq!(error.kind(), CheckpointManifestV4ErrorKind::InvalidSchema);
    }

    /// The other direction: a genuine v4 manifest's `train_state` object
    /// carries `baseline_schema`/`baseline_cells`, which v3's own
    /// `deny_unknown_fields` `CheckpointTrainStateBindingV3` rejects.
    #[test]
    fn v4_train_state_shape_rejected_by_v3_binding_v4() {
        let baseline = baseline_with_two_cells_v4();
        let manifest = build_checkpoint_manifest_v4(sample_parts_v4(baseline, 4)).expect("build");
        let document: Value =
            serde_json::from_slice(manifest.canonical_bytes()).expect("parse json");
        let train_state_value = document.get("train_state").cloned().expect("train_state");
        assert!(
            serde_json::from_value::<CheckpointTrainStateBindingV3>(train_state_value).is_err()
        );
    }

    #[test]
    fn unknown_field_rejected_v4() {
        let baseline = baseline_with_two_cells_v4();
        let manifest = build_checkpoint_manifest_v4(sample_parts_v4(baseline, 4)).expect("build");
        let mut document: Value =
            serde_json::from_slice(manifest.canonical_bytes()).expect("parse json");
        document
            .as_object_mut()
            .expect("object")
            .insert("unexpected_field_v4".to_owned(), Value::Bool(true));
        let bytes = serde_json::to_vec(&document).expect("reserialize");
        let error = decode_checkpoint_manifest_v4(&bytes).expect_err("unknown field");
        assert!(matches!(
            error.kind(),
            CheckpointManifestV4ErrorKind::CanonicalJson(_)
        ));
    }

    #[test]
    fn out_of_order_baseline_cells_rejected_v4() {
        let baseline = baseline_with_two_cells_v4();
        let mut wire = build_wire_v4(sample_parts_v4(baseline, 4)).expect("wire");
        assert_eq!(wire.train_state.baseline_cells.len(), 2);
        wire.train_state.baseline_cells.reverse();
        let bytes =
            to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid).expect("encode");
        let error = decode_checkpoint_manifest_v4(&bytes).expect_err("out of order");
        assert_eq!(
            error.kind(),
            CheckpointManifestV4ErrorKind::InvalidBaseline(NativeBaselineErrorKindV4::InvalidWire)
        );
    }

    #[test]
    fn baseline_schema_mismatch_rejected_v4() {
        let baseline = baseline_with_two_cells_v4();
        let mut wire = build_wire_v4(sample_parts_v4(baseline, 4)).expect("wire");
        wire.train_state.baseline_schema = "wrong-baseline-schema/v1".to_owned();
        let bytes =
            to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid).expect("encode");
        let error = decode_checkpoint_manifest_v4(&bytes).expect_err("baseline schema mismatch");
        assert_eq!(
            error.kind(),
            CheckpointManifestV4ErrorKind::InvalidBaselineSchema
        );
    }
}
