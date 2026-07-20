//! Pure EpisodeV1, UpdateEvidenceV1, and UpdateGroupV1 record authority.
//!
//! This module validates exactly one complete native training update in
//! memory. It has no filesystem, continuation partitioner, publisher, receipt,
//! live-executor mutation, or checkpoint-manifest construction. The 256 MiB
//! standalone decode ceiling is only a conservative memory-safety ceiling; it
//! is not the Store continuation file cap or representability authority.

use crate::async_flat_scored_rollout_v2::ASYNC_FLAT_SCORED_MEMBERSHIP_DIGEST_IDENTITY_V1;
use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use crate::native_policy_train_step_v1::NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1;
use crate::native_training_executor_v1::{
    native_training_episode_schedule_v1, NativeTrainingCheckpointCandidateV1,
    NativeTrainingUpdateObservationV2,
};
use crate::native_training_store_checkpoint_v3::{CheckpointManifestV3, CheckpointProgressV3};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::rl::{PlayerSeatV1, TerminalOutcomeV1};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const EPISODE_SCHEMA_V1: &str = "mtg_kernel_native_train_episode/v1";
pub const UPDATE_EVIDENCE_SCHEMA_V1: &str = "mtg_kernel_native_train_update_evidence/v1";
pub const UPDATE_EVIDENCE_SHA256_IDENTITY_V1: &str =
    "mtg-kernel-native-training-update-evidence-sha256-v1";
pub const BATCH_MEMBERSHIP_DIGEST_IDENTITY_V1: &str =
    ASYNC_FLAT_SCORED_MEMBERSHIP_DIGEST_IDENTITY_V1;
/// Exact full-document digest of frozen Store contract revision 5.
pub const UPDATE_GROUP_RECORD_CONTRACT_SHA256_V1: &str =
    crate::native_training_store_checkpoint_v3::NATIVE_TRAINING_STORE_RECORD_CONTRACT_SHA256_V1;

const U63_MAX_V1: u64 = (1_u64 << 63) - 1;
const MAX_LOGICAL_ROWS_V1: u64 = 262_144;
const CONSERVATIVE_STANDALONE_GROUP_CJ_CEILING_V1: usize = 256 * 1024 * 1024;
const MAX_LEGAL_ACTION_COUNT_V1: u64 = 64;

const PREVIOUS_UPDATE_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "previous_update_evidence_sha256",
    )];
const EPISODE_WINNER_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("evidence"),
    CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
];
const GROUP_NULL_PATHS_V1: &[&[CanonicalJsonNullPathSegmentV1]] =
    &[PREVIOUS_UPDATE_NULL_PATH_V1, EPISODE_WINNER_NULL_PATH_V1];
const GROUP_NULL_POLICY_V1: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(GROUP_NULL_PATHS_V1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SeatWireV1 {
    P0,
    P1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum OutcomeWireV1 {
    P0Win,
    P1Win,
    Draw,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct EpisodeWireV1 {
    schema: String,
    episode_index: u64,
    environment_seed_u64_hex: String,
    deck_ids: [String; 2],
    deck_hashes_u64_hex: [String; 2],
    learner_seat: SeatWireV1,
    learner_return: i8,
    terminal_outcome: OutcomeWireV1,
    winner: Option<SeatWireV1>,
    terminal_classification: String,
    terminal_code: String,
    policy_step_count: u64,
    physical_decision_count: u64,
    learner_policy_step_count: u64,
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_physical_decision_count: u64,
    trajectory_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PhysicalLossTermWireV1 {
    joint_log_probability_f32_bits: String,
    value_f32_bits: String,
    terminal_return_i8: i8,
    substep_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LossWireV1 {
    policy_sum_f32_bits: String,
    value_sum_f32_bits: String,
    total_f32_bits: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct GaugeSubstepBoundWireV1 {
    action_count: u64,
    abs_policy_coefficient_f64_bits: String,
    gamma_operation_count: u64,
    gamma_f64_bits: String,
    bound_component_f64_bits: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct GaugeWireV1 {
    identity: String,
    parameter_name: String,
    substep_count: u64,
    total_action_count: u64,
    max_action_count: u64,
    sum_abs_policy_coefficients_f64_bits: String,
    substep_bounds: Vec<GaugeSubstepBoundWireV1>,
    per_substep_bound_sum_f64_bits: String,
    cross_substep_bound_f64_bits: String,
    raw_gradient_residual_f32_bits: String,
    derived_absolute_bound_f64_bits: String,
    high_precision_residual_f64_bits: String,
    canonical_gradient_f32_bits: String,
    parameter_before_f32_bits: u32,
    parameter_after_f32_bits: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RolloutCountsWireV1 {
    complete_round_count: u64,
    scorer_batch_count: u64,
    scored_decision_count: u64,
    scored_action_logit_count: u64,
    sampled_action_count: u64,
    terminal_notification_count: u64,
    batch_width_sum: u64,
    max_batch_width: u64,
    full_target_batch_count: u64,
    short_batch_count: u64,
    batch_membership_digest_identity: String,
    batch_membership_digest_hex: String,
    natural_terminal_count: u64,
    halted_count: u64,
    truncated_count: u64,
    apply_error_count: u64,
    partial_group_count: u64,
    association_failure_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateEvidenceWireV1 {
    schema: String,
    run_sha256: String,
    identity_bundle_sha256: String,
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    update_index: u64,
    episode_start: u64,
    episode_count: u64,
    episode_end_exclusive: u64,
    optimizer_step: bool,
    adam_step_before: u64,
    adam_step_after: u64,
    learner_group_count: u64,
    learner_policy_step_count: u64,
    learner_physical_decision_count: u64,
    physical_terms: Vec<PhysicalLossTermWireV1>,
    loss: LossWireV1,
    gauge: GaugeWireV1,
    rollout_counts: RolloutCountsWireV1,
    episodes: Vec<EpisodeWireV1>,
    model_parameter_sha256_before: String,
    model_parameter_sha256_after: String,
    train_state_sha256_after: String,
    progress_after: CheckpointProgressV3,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateGroupWireV1 {
    update_index: u64,
    previous_update_evidence_sha256: Option<String>,
    evidence: UpdateEvidenceWireV1,
    update_evidence_sha256: String,
    logical_row_count: u64,
}

/// Fully validated, canonical one-update authority.
///
/// It has no public fields, serde decoder, or unchecked constructor:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_update_group_v1::ValidatedUpdateGroupV1;
/// use serde::de::DeserializeOwned;
/// fn require_deserialize<T: DeserializeOwned>() {}
/// require_deserialize::<ValidatedUpdateGroupV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_update_group_v1::ValidatedUpdateGroupV1;
/// let _ = ValidatedUpdateGroupV1 {};
/// ```
pub struct ValidatedUpdateGroupV1 {
    wire: UpdateGroupWireV1,
    canonical_bytes: Vec<u8>,
    update_evidence_sha256: [u8; 32],
}

impl std::fmt::Debug for ValidatedUpdateGroupV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedUpdateGroupV1")
            .field("update_index", &self.wire.update_index)
            .field("logical_row_count", &self.wire.logical_row_count)
            .field(
                "update_evidence_sha256",
                &lower_hex_raw32_v1(self.update_evidence_sha256),
            )
            .finish_non_exhaustive()
    }
}

impl ValidatedUpdateGroupV1 {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub const fn update_index(&self) -> u64 {
        self.wire.update_index
    }

    pub const fn logical_row_count(&self) -> u64 {
        self.wire.logical_row_count
    }

    pub const fn update_evidence_sha256(&self) -> [u8; 32] {
        self.update_evidence_sha256
    }

    pub fn previous_update_evidence_sha256(&self) -> Option<&str> {
        self.wire.previous_update_evidence_sha256.as_deref()
    }
}

/// Move-only, private-field evidence-chain authority rooted in a validated
/// generation-zero checkpoint.
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_update_group_v1::UpdateEvidenceChainContextV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<UpdateEvidenceChainContextV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_update_group_v1::UpdateEvidenceChainContextV1;
/// let _ = UpdateEvidenceChainContextV1 {};
/// ```
pub struct UpdateEvidenceChainContextV1 {
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    next_update_index: u64,
    previous_update_evidence_sha256: Option<[u8; 32]>,
    progress: CheckpointProgressV3,
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    scorer_bias_anchor_bits: u32,
}

impl std::fmt::Debug for UpdateEvidenceChainContextV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UpdateEvidenceChainContextV1")
            .field("next_update_index", &self.next_update_index)
            .field("progress", &self.progress)
            .finish_non_exhaustive()
    }
}

impl UpdateEvidenceChainContextV1 {
    pub const fn next_update_index(&self) -> u64 {
        self.next_update_index
    }

    pub const fn progress(&self) -> &CheckpointProgressV3 {
        &self.progress
    }

    pub const fn previous_update_evidence_sha256(&self) -> Option<[u8; 32]> {
        self.previous_update_evidence_sha256
    }

    pub const fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub const fn train_state_sha256(&self) -> [u8; 32] {
        self.train_state_sha256
    }
}

/// Validated group paired with the only context that may validate its
/// successor. Destructuring consumes the pair.
#[derive(Debug)]
pub struct ValidatedUpdateGroupAdvanceV1 {
    group: ValidatedUpdateGroupV1,
    advanced_context: UpdateEvidenceChainContextV1,
}

impl ValidatedUpdateGroupAdvanceV1 {
    pub const fn group(&self) -> &ValidatedUpdateGroupV1 {
        &self.group
    }

    pub const fn advanced_context(&self) -> &UpdateEvidenceChainContextV1 {
        &self.advanced_context
    }

    pub fn into_parts(self) -> (ValidatedUpdateGroupV1, UpdateEvidenceChainContextV1) {
        (self.group, self.advanced_context)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateGroupV1ErrorKind {
    RecordTooLarge,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidDigest,
    InvalidScalar,
    InvalidArithmetic,
    RunBinding,
    ScheduleBinding,
    EpisodeBinding,
    PhysicalLattice,
    LossMismatch,
    GaugeMismatch,
    RolloutMismatch,
    ProgressMismatch,
    CheckpointMismatch,
    ChainMismatch,
}

impl UpdateGroupV1ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RecordTooLarge => "native_train_update_group_v1_record_too_large",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_train_update_group_v1_invalid_schema",
            Self::InvalidDigest => "native_train_update_group_v1_invalid_digest",
            Self::InvalidScalar => "native_train_update_group_v1_invalid_scalar",
            Self::InvalidArithmetic => "native_train_update_group_v1_invalid_arithmetic",
            Self::RunBinding => "native_train_update_group_v1_run_binding",
            Self::ScheduleBinding => "native_train_update_group_v1_schedule_binding",
            Self::EpisodeBinding => "native_train_update_group_v1_episode_binding",
            Self::PhysicalLattice => "native_train_update_group_v1_physical_lattice",
            Self::LossMismatch => "native_train_update_group_v1_loss_mismatch",
            Self::GaugeMismatch => "native_train_update_group_v1_gauge_mismatch",
            Self::RolloutMismatch => "native_train_update_group_v1_rollout_mismatch",
            Self::ProgressMismatch => "native_train_update_group_v1_progress_mismatch",
            Self::CheckpointMismatch => "native_train_update_group_v1_checkpoint_mismatch",
            Self::ChainMismatch => "native_train_update_group_v1_chain_mismatch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateGroupV1Error {
    kind: UpdateGroupV1ErrorKind,
}

impl UpdateGroupV1Error {
    const fn new(kind: UpdateGroupV1ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> UpdateGroupV1ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl From<CanonicalJsonErrorV1> for UpdateGroupV1Error {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(UpdateGroupV1ErrorKind::CanonicalJson(error.kind()))
    }
}

impl Display for UpdateGroupV1Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for UpdateGroupV1Error {}

type Result<T> = std::result::Result<T, UpdateGroupV1Error>;

/// Establishes the only public root constructor for the evidence chain.
pub fn begin_update_evidence_chain_v1(
    run: &ValidatedTrainRunV2,
    genesis: &CheckpointManifestV3,
) -> Result<UpdateEvidenceChainContextV1> {
    let run_sha256 = parse_digest_v1(run.run_sha256())?;
    let identity_bundle_sha256 = parse_digest_v1(run.identity_bundle_sha256())?;
    let anchor = u32::try_from(genesis.train_state().scorer_bias_anchor_f32_bits())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch))?;
    if genesis.generation_index() != 0
        || genesis.segment_ordinal() != 0
        || genesis.run_sha256() != run.run_sha256()
        || genesis.identity_bundle_sha256() != run.identity_bundle_sha256()
        || genesis.batch_episodes() != run.batch_episodes()
        || genesis.checkpoint_segment_updates() != run.checkpoint_segment_updates()
        || genesis.progress().successful_update_count() != 0
        || genesis.progress().next_episode_index() != 0
        || genesis.progress().completed_episode_count() != 0
        || genesis.train_state().adam_step() != 0
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch));
    }
    Ok(UpdateEvidenceChainContextV1 {
        run_sha256,
        identity_bundle_sha256,
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates: run.checkpoint_segment_updates(),
        next_update_index: 1,
        previous_update_evidence_sha256: None,
        progress: *genesis.progress(),
        model_parameter_sha256: genesis.model_parameter_sha256(),
        train_state_sha256: genesis.train_state_sha256(),
        scorer_bias_anchor_bits: anchor,
    })
}

/// Builds a complete group from one prepared observation/checkpoint pair and
/// advances the consumed evidence context without mutating either producer.
pub fn build_update_group_v1(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    observation: &NativeTrainingUpdateObservationV2,
    checkpoint: &NativeTrainingCheckpointCandidateV1,
) -> Result<ValidatedUpdateGroupAdvanceV1> {
    validate_context_run_v1(run, &context)?;
    validate_observation_checkpoint_v1(run, &context, observation, checkpoint)?;
    preflight_observation_cardinality_v1(observation)?;
    let evidence = evidence_from_observation_v1(run, &context, observation, checkpoint)?;
    let previous_update_evidence_sha256 = context
        .previous_update_evidence_sha256
        .map(lower_hex_raw32_v1);
    let logical_row_count = logical_row_count_v1(&evidence)?;
    let evidence_cj = to_canonical_json_bytes_v1(&evidence, episode_null_policy_v1())?;
    let update_evidence_sha256 = update_evidence_sha256_v1(
        context.run_sha256,
        context.next_update_index,
        context.previous_update_evidence_sha256,
        &evidence_cj,
    )?;
    let wire = UpdateGroupWireV1 {
        update_index: context.next_update_index,
        previous_update_evidence_sha256,
        evidence,
        update_evidence_sha256: lower_hex_raw32_v1(update_evidence_sha256),
        logical_row_count,
    };
    let canonical_bytes = to_canonical_json_bytes_v1(&wire, GROUP_NULL_POLICY_V1)?;
    if canonical_bytes.len() > CONSERVATIVE_STANDALONE_GROUP_CJ_CEILING_V1 {
        return Err(error_v1(UpdateGroupV1ErrorKind::RecordTooLarge));
    }
    validate_and_advance_wire_v1(run, context, wire, canonical_bytes)
}

fn preflight_observation_cardinality_v1(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<()> {
    let episodes = u64::try_from(observation.episodes.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let physical_terms = u64::try_from(observation.physical_terms.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let gauge_bounds = u64::try_from(observation.scorer_bias_gauge.substep_bounds.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    1_u64
        .checked_add(episodes)
        .and_then(|value| value.checked_add(physical_terms))
        .and_then(|value| value.checked_add(gauge_bounds))
        .filter(|value| *value > 0 && is_u63_v1(*value) && *value <= MAX_LOGICAL_ROWS_V1)
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    Ok(())
}

/// Decodes canonical standalone group bytes and advances the consumed context.
/// The ceiling here is only defensive; continuation planning owns Store caps.
pub fn decode_update_group_v1(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    canonical_group_bytes: &[u8],
) -> Result<ValidatedUpdateGroupAdvanceV1> {
    if canonical_group_bytes.len() > CONSERVATIVE_STANDALONE_GROUP_CJ_CEILING_V1 {
        return Err(error_v1(UpdateGroupV1ErrorKind::RecordTooLarge));
    }
    validate_context_run_v1(run, &context)?;
    let wire: UpdateGroupWireV1 =
        from_canonical_json_bytes_v1(canonical_group_bytes, GROUP_NULL_POLICY_V1)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, GROUP_NULL_POLICY_V1)?;
    if reencoded != canonical_group_bytes {
        return Err(error_v1(UpdateGroupV1ErrorKind::CanonicalJson(
            CanonicalJsonErrorKindV1::NonCanonicalBytes,
        )));
    }
    validate_and_advance_wire_v1(run, context, wire, reencoded)
}

fn episode_null_policy_v1() -> CanonicalJsonNullPolicyV1 {
    const WINNER: &[CanonicalJsonNullPathSegmentV1] = &[
        CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
        CanonicalJsonNullPathSegmentV1::AnyArrayElement,
        CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
    ];
    CanonicalJsonNullPolicyV1::AllowOnly(&[WINNER])
}

fn validate_context_run_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
) -> Result<()> {
    if context.run_sha256 != parse_digest_v1(run.run_sha256())?
        || context.identity_bundle_sha256 != parse_digest_v1(run.identity_bundle_sha256())?
        || context.batch_episodes != run.batch_episodes()
        || context.checkpoint_segment_updates != run.checkpoint_segment_updates()
        || context.next_update_index == 0
        || !is_u63_v1(context.next_update_index)
        || context.next_update_index > run.requested_successful_updates()
        || context.progress.batch_episodes() != context.batch_episodes
        || context.progress.checkpoint_segment_updates() != context.checkpoint_segment_updates
        || context.progress.successful_update_count()
            != context
                .next_update_index
                .checked_sub(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RunBinding));
    }
    let expected_prior_episodes =
        checked_u63_mul_v1(context.batch_episodes, context.next_update_index - 1)?;
    if context.progress.next_episode_index() != expected_prior_episodes
        || context.progress.completed_episode_count() != expected_prior_episodes
        || context.scorer_bias_anchor_bits
            != u32::try_from(run.record().model_snapshot.scorer_bias_anchor_f32_bits)
                .map_err(|_| error_v1(UpdateGroupV1ErrorKind::RunBinding))?
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RunBinding));
    }
    validate_progress_shape_v1(&context.progress, context.next_update_index - 1)
}

fn validate_observation_checkpoint_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
    observation: &NativeTrainingUpdateObservationV2,
    checkpoint: &NativeTrainingCheckpointCandidateV1,
) -> Result<()> {
    let record = run.record();
    let expected_before = context.next_update_index - 1;
    let expected_end =
        checked_u63_add_v1(context.progress.next_episode_index(), run.batch_episodes())?;
    let topology = &record.topology;
    if observation.trainer_contract_identity != record.contracts.trainer_identity
        || observation.first_episode_index != context.progress.next_episode_index()
        || observation.episode_count != run.batch_episodes()
        || observation.adam_step_before != expected_before
        || observation.adam_step_after != context.next_update_index
        || checkpoint.base_seed() != record.schedule.base_seed
        || checkpoint.batch_episodes() != run.batch_episodes()
        || checkpoint.adam_step() != context.next_update_index
        || checkpoint.scorer_bias_anchor_bits() != context.scorer_bias_anchor_bits
        || checkpoint.progress().successful_update_count != context.next_update_index
        || checkpoint.progress().next_episode_index != expected_end
        || checkpoint.progress().completed_episode_count != expected_end
        || u64::try_from(observation.worker_count).ok() != Some(topology.worker_count)
        || u64::try_from(observation.sessions_per_worker).ok() != Some(topology.sessions_per_worker)
        || u64::try_from(observation.logical_actor_count).ok() != Some(topology.logical_actor_count)
        || u64::try_from(observation.broker_batch_target).ok() != Some(topology.broker_batch_target)
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch));
    }
    let model_before = parse_digest_v1(&observation.model_digest_before)?;
    let model_after = parse_digest_v1(&observation.model_digest_after)?;
    if model_before != context.model_parameter_sha256
        || model_after != checkpoint.digests().model_parameter_sha256
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch));
    }
    Ok(())
}

fn evidence_from_observation_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
    observation: &NativeTrainingUpdateObservationV2,
    checkpoint: &NativeTrainingCheckpointCandidateV1,
) -> Result<UpdateEvidenceWireV1> {
    let expected_k = usize::try_from(run.batch_episodes())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    if observation.episodes.len() != expected_k {
        return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
    }
    let run_deck_hashes = [
        parse_u64_hex_v1(&run.record().environment.deck_hashes_u64_hex[0])?,
        parse_u64_hex_v1(&run.record().environment.deck_hashes_u64_hex[1])?,
    ];
    let mut episodes = Vec::with_capacity(expected_k);
    let mut total_policy_steps = 0_u64;
    let mut total_physical_decisions = 0_u64;
    let mut learner_policy_steps = 0_u64;
    let mut learner_physical_decisions = 0_u64;
    for (offset, observed) in observation.episodes.iter().enumerate() {
        let offset = u64::try_from(offset)
            .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        let expected_episode_index = checked_u63_add_v1(observation.first_episode_index, offset)?;
        let schedule = native_training_episode_schedule_v1(
            run.record().schedule.base_seed,
            expected_episode_index,
        )
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::ScheduleBinding))?;
        let receipt = observed.full_trajectory_receipt;
        if observed.episode_index != expected_episode_index
            || receipt.episode_index != expected_episode_index
            || schedule.episode_index != expected_episode_index
            || schedule.learner_seat != observed.learner_seat
            || receipt.learner_seat != observed.learner_seat
            || receipt.environment_seed != schedule.environment_seed
            || receipt.deck_hashes != run_deck_hashes
            || observed.learner_group_count != receipt.learner_physical_decision_count
            || observed.learner_policy_step_count != receipt.learner_policy_step_count
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        let (terminal_outcome, winner) = natural_outcome_wire_v1(observed.terminal_outcome)?;
        let expected_return = learner_return_v1(observed.learner_seat, terminal_outcome);
        if observed.learner_return != expected_return {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        total_policy_steps = checked_u63_add_v1(total_policy_steps, receipt.policy_step_count)?;
        total_physical_decisions =
            checked_u63_add_v1(total_physical_decisions, receipt.physical_decision_count)?;
        learner_policy_steps =
            checked_u63_add_v1(learner_policy_steps, receipt.learner_policy_step_count)?;
        learner_physical_decisions = checked_u63_add_v1(
            learner_physical_decisions,
            receipt.learner_physical_decision_count,
        )?;
        let policy_parts = checked_u63_add_v1(
            receipt.learner_policy_step_count,
            receipt.opponent_policy_step_count,
        )?;
        let physical_parts = checked_u63_add_v1(
            receipt.learner_physical_decision_count,
            receipt.opponent_physical_decision_count,
        )?;
        if receipt.policy_step_count != policy_parts
            || receipt.physical_decision_count != physical_parts
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        episodes.push(EpisodeWireV1 {
            schema: EPISODE_SCHEMA_V1.to_owned(),
            episode_index: expected_episode_index,
            environment_seed_u64_hex: format!("{:016x}", receipt.environment_seed),
            deck_ids: run.record().environment.deck_ids.clone(),
            deck_hashes_u64_hex: run.record().environment.deck_hashes_u64_hex.clone(),
            learner_seat: seat_wire_v1(observed.learner_seat),
            learner_return: observed.learner_return,
            terminal_outcome,
            winner,
            terminal_classification: "natural".to_owned(),
            terminal_code: "natural-game-over".to_owned(),
            policy_step_count: receipt.policy_step_count,
            physical_decision_count: receipt.physical_decision_count,
            learner_policy_step_count: receipt.learner_policy_step_count,
            opponent_policy_step_count: receipt.opponent_policy_step_count,
            learner_physical_decision_count: receipt.learner_physical_decision_count,
            opponent_physical_decision_count: receipt.opponent_physical_decision_count,
            trajectory_sha256: lower_hex_raw32_v1(receipt.trajectory_sha256),
        });
    }
    if total_policy_steps != observation.policy_step_count
        || total_physical_decisions != observation.physical_decision_count
        || learner_policy_steps != observation.learner_policy_step_count
        || learner_physical_decisions != observation.learner_group_count
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
    }

    let physical_terms = observation
        .physical_terms
        .iter()
        .map(|term| PhysicalLossTermWireV1 {
            joint_log_probability_f32_bits: format!("{:08x}", term.joint_log_probability_bits),
            value_f32_bits: format!("{:08x}", term.value_bits),
            terminal_return_i8: term.terminal_return,
            substep_count: term.substep_count,
        })
        .collect::<Vec<_>>();
    validate_direct_physical_lattice_v1(observation, &episodes, &physical_terms)?;

    let gauge = gauge_from_observation_v1(observation)?;
    let rollout_counts = rollout_from_observation_v1(observation)?;
    let progress_after = fold_progress_v1(&context.progress, &episodes)?;
    validate_candidate_progress_v1(checkpoint, &progress_after)?;
    let episode_end_exclusive =
        checked_u63_add_v1(observation.first_episode_index, observation.episode_count)?;

    Ok(UpdateEvidenceWireV1 {
        schema: UPDATE_EVIDENCE_SCHEMA_V1.to_owned(),
        run_sha256: run.run_sha256().to_owned(),
        identity_bundle_sha256: run.identity_bundle_sha256().to_owned(),
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates: run.checkpoint_segment_updates(),
        update_index: context.next_update_index,
        episode_start: observation.first_episode_index,
        episode_count: observation.episode_count,
        episode_end_exclusive,
        optimizer_step: true,
        adam_step_before: observation.adam_step_before,
        adam_step_after: observation.adam_step_after,
        learner_group_count: observation.learner_group_count,
        learner_policy_step_count: observation.learner_policy_step_count,
        learner_physical_decision_count: learner_physical_decisions,
        physical_terms,
        loss: LossWireV1 {
            policy_sum_f32_bits: format!("{:08x}", observation.policy_sum_bits),
            value_sum_f32_bits: format!("{:08x}", observation.value_sum_bits),
            total_f32_bits: format!("{:08x}", observation.loss_bits),
        },
        gauge,
        rollout_counts,
        episodes,
        model_parameter_sha256_before: observation.model_digest_before.clone(),
        model_parameter_sha256_after: observation.model_digest_after.clone(),
        train_state_sha256_after: checkpoint.native_state_sha256_hex(),
        progress_after,
    })
}

fn gauge_from_observation_v1(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<GaugeWireV1> {
    let gauge = &observation.scorer_bias_gauge;
    Ok(GaugeWireV1 {
        identity: NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1.to_owned(),
        parameter_name: gauge.parameter_name.to_owned(),
        substep_count: checked_usize_u63_v1(gauge.substep_count)?,
        total_action_count: checked_usize_u63_v1(gauge.total_action_count)?,
        max_action_count: checked_usize_u63_v1(gauge.max_action_count)?,
        sum_abs_policy_coefficients_f64_bits: format!(
            "{:016x}",
            gauge.sum_abs_policy_coefficients.to_bits()
        ),
        substep_bounds: gauge
            .substep_bounds
            .iter()
            .map(|bound| {
                Ok(GaugeSubstepBoundWireV1 {
                    action_count: checked_usize_u63_v1(bound.action_count)?,
                    abs_policy_coefficient_f64_bits: format!(
                        "{:016x}",
                        bound.abs_policy_coefficient.to_bits()
                    ),
                    gamma_operation_count: checked_usize_u63_v1(bound.gamma_operation_count)?,
                    gamma_f64_bits: format!("{:016x}", bound.gamma.to_bits()),
                    bound_component_f64_bits: format!("{:016x}", bound.bound_component.to_bits()),
                })
            })
            .collect::<Result<Vec<_>>>()?,
        per_substep_bound_sum_f64_bits: format!("{:016x}", gauge.per_substep_bound_sum.to_bits()),
        cross_substep_bound_f64_bits: format!("{:016x}", gauge.cross_substep_bound.to_bits()),
        raw_gradient_residual_f32_bits: format!("{:08x}", gauge.raw_gradient_residual.to_bits()),
        derived_absolute_bound_f64_bits: format!("{:016x}", gauge.derived_absolute_bound.to_bits()),
        high_precision_residual_f64_bits: format!(
            "{:016x}",
            gauge.high_precision_residual.to_bits()
        ),
        canonical_gradient_f32_bits: format!("{:08x}", gauge.canonical_gradient.to_bits()),
        parameter_before_f32_bits: gauge.parameter_before_bits,
        parameter_after_f32_bits: gauge.parameter_after_bits,
    })
}

fn rollout_from_observation_v1(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<RolloutCountsWireV1> {
    let metrics = observation.rollout_metrics;
    Ok(RolloutCountsWireV1 {
        complete_round_count: checked_u63_v1(metrics.complete_round_count)?,
        scorer_batch_count: checked_u63_v1(metrics.scorer_batch_count)?,
        scored_decision_count: checked_u63_v1(metrics.scored_decision_count)?,
        scored_action_logit_count: checked_u63_v1(metrics.scored_action_logit_count)?,
        sampled_action_count: checked_u63_v1(metrics.sampled_action_count)?,
        terminal_notification_count: checked_u63_v1(metrics.terminal_notification_count)?,
        batch_width_sum: checked_u63_v1(metrics.batch_width_sum)?,
        max_batch_width: u64::from(metrics.max_batch_width),
        full_target_batch_count: checked_u63_v1(metrics.full_target_batch_count)?,
        short_batch_count: checked_u63_v1(metrics.short_batch_count)?,
        batch_membership_digest_identity: BATCH_MEMBERSHIP_DIGEST_IDENTITY_V1.to_owned(),
        batch_membership_digest_hex: lower_hex_raw32_v1(metrics.batch_membership_digest),
        natural_terminal_count: observation.episode_count,
        halted_count: 0,
        truncated_count: 0,
        apply_error_count: 0,
        partial_group_count: 0,
        association_failure_count: 0,
    })
}

fn validate_direct_physical_lattice_v1(
    observation: &NativeTrainingUpdateObservationV2,
    episodes: &[EpisodeWireV1],
    physical_terms: &[PhysicalLossTermWireV1],
) -> Result<()> {
    let group_count = usize::try_from(observation.learner_group_count)
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
    if group_count == 0
        || physical_terms.len() != group_count
        || observation.physical_terms.len() != group_count
        || observation.learner_group_count
            != episodes.iter().try_fold(0_u64, |sum, episode| {
                checked_u63_add_v1(sum, episode.learner_physical_decision_count)
            })?
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
    }

    let mut group_index = 0_usize;
    let mut output_index = 0_usize;
    let mut policy_count = 0_u64;
    for episode in episodes {
        let episode_groups = usize::try_from(episode.learner_physical_decision_count)
            .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
        for _ in 0..episode_groups {
            let term = physical_terms
                .get(group_index)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
            if term.terminal_return_i8 != episode.learner_return || term.substep_count == 0 {
                return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
            }
            let substeps = usize::try_from(term.substep_count)
                .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
            let mut joint: Option<f32> = None;
            for substep_index in 0..substeps {
                let output = observation
                    .selected_outputs
                    .get(output_index)
                    .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
                let selected_probability = f32::from_bits(output.selected_log_probability_bits);
                let selected_logit = f32::from_bits(output.selected_logit_bits);
                let value = f32::from_bits(output.value_bits);
                if output.group_index != group_index
                    || output.substep_index != substep_index
                    || !selected_probability.is_finite()
                    || !selected_logit.is_finite()
                    || !value.is_finite()
                    || (substep_index == 0
                        && output.value_bits != parse_f32_hex_v1(&term.value_f32_bits)?.to_bits())
                {
                    return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
                }
                joint = Some(match joint {
                    None => selected_probability,
                    Some(active) => active + selected_probability,
                });
                output_index = output_index
                    .checked_add(1)
                    .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
            }
            let expected_joint = parse_f32_hex_v1(&term.joint_log_probability_f32_bits)?;
            if joint.map(f32::to_bits) != Some(expected_joint.to_bits()) {
                return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
            }
            policy_count = checked_u63_add_v1(policy_count, u64::from(term.substep_count))?;
            group_index = group_index
                .checked_add(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        }
    }
    if group_index != physical_terms.len()
        || output_index != observation.selected_outputs.len()
        || policy_count != observation.learner_policy_step_count
        || policy_count
            != episodes.iter().try_fold(0_u64, |sum, episode| {
                checked_u63_add_v1(sum, episode.learner_policy_step_count)
            })?
        || policy_count
            != u64::try_from(observation.scorer_bias_gauge.substep_count)
                .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?
        || policy_count
            != u64::try_from(observation.scorer_bias_gauge.substep_bounds.len())
                .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?
        || policy_count != observation.rollout_metrics.scored_decision_count
        || policy_count != observation.rollout_metrics.sampled_action_count
        || policy_count != observation.rollout_metrics.batch_width_sum
        || policy_count != observation.scorer_accepted_decision_count
        || observation.scorer_accepted_batch_count != observation.rollout_metrics.scorer_batch_count
        || observation.learner_group_count > policy_count
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
    }
    let mut group_output_start = Vec::with_capacity(physical_terms.len());
    let mut next_output_start = 0_usize;
    for term in physical_terms {
        group_output_start.push(next_output_start);
        next_output_start = next_output_start
            .checked_add(
                usize::try_from(term.substep_count)
                    .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?,
            )
            .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    }
    let mut reverse_row_index = 0_usize;
    for group_index in (0..physical_terms.len()).rev() {
        let substeps = usize::try_from(physical_terms[group_index].substep_count)
            .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
        for substep_index in (0..substeps).rev() {
            let output_index = group_output_start[group_index]
                .checked_add(substep_index)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
            let output = &observation.selected_outputs[output_index];
            let bound = observation
                .scorer_bias_gauge
                .substep_bounds
                .get(reverse_row_index)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
            if bound.action_count == 0
                || bound.action_count > 64
                || output.selected_action_index >= bound.action_count
            {
                return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
            }
            reverse_row_index = reverse_row_index
                .checked_add(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        }
    }
    if reverse_row_index != observation.scorer_bias_gauge.substep_bounds.len() {
        return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
    }
    Ok(())
}

fn fold_progress_v1(
    prior: &CheckpointProgressV3,
    episodes: &[EpisodeWireV1],
) -> Result<CheckpointProgressV3> {
    let mut next = *prior;
    for episode in episodes {
        let seat = match episode.learner_seat {
            SeatWireV1::P0 => 0,
            SeatWireV1::P1 => 1,
        };
        let outcomes = if seat == 0 {
            &mut next.outcomes_by_learner_seat.p0
        } else {
            &mut next.outcomes_by_learner_seat.p1
        };
        match episode.learner_return {
            1 => outcomes.win = checked_u63_add_v1(outcomes.win, 1)?,
            -1 => outcomes.loss = checked_u63_add_v1(outcomes.loss, 1)?,
            0 => outcomes.draw = checked_u63_add_v1(outcomes.draw, 1)?,
            _ => return Err(error_v1(UpdateGroupV1ErrorKind::ProgressMismatch)),
        }
        let policy = if seat == 0 {
            &mut next.learner_policy_steps_by_seat.p0
        } else {
            &mut next.learner_policy_steps_by_seat.p1
        };
        *policy = checked_u63_add_v1(*policy, episode.learner_policy_step_count)?;
        let physical = if seat == 0 {
            &mut next.learner_physical_decisions_by_seat.p0
        } else {
            &mut next.learner_physical_decisions_by_seat.p1
        };
        *physical = checked_u63_add_v1(*physical, episode.learner_physical_decision_count)?;
    }
    let episode_count = u64::try_from(episodes.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    next.next_episode_index = checked_u63_add_v1(next.next_episode_index, episode_count)?;
    next.completed_episode_count = checked_u63_add_v1(next.completed_episode_count, episode_count)?;
    next.successful_update_count = checked_u63_add_v1(next.successful_update_count, 1)?;
    validate_progress_shape_v1(&next, next.successful_update_count)?;
    Ok(next)
}

fn validate_candidate_progress_v1(
    checkpoint: &NativeTrainingCheckpointCandidateV1,
    expected: &CheckpointProgressV3,
) -> Result<()> {
    let progress = checkpoint.progress();
    let expected_policy = checked_u63_add_v1(
        expected.learner_policy_steps_by_seat().p0(),
        expected.learner_policy_steps_by_seat().p1(),
    )?;
    let expected_physical = checked_u63_add_v1(
        expected.learner_physical_decisions_by_seat().p0(),
        expected.learner_physical_decisions_by_seat().p1(),
    )?;
    if progress.next_episode_index != expected.next_episode_index()
        || progress.successful_update_count != expected.successful_update_count()
        || progress.completed_episode_count != expected.completed_episode_count()
        || progress.learner_policy_step_count != expected_policy
        || progress.learner_physical_decision_count != expected_physical
        || checkpoint.adam_step() != expected.successful_update_count()
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::ProgressMismatch));
    }
    Ok(())
}

fn validate_and_advance_wire_v1(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    wire: UpdateGroupWireV1,
    canonical_bytes: Vec<u8>,
) -> Result<ValidatedUpdateGroupAdvanceV1> {
    validate_group_bindings_v1(run, &context, &wire)?;
    let evidence_cj = to_canonical_json_bytes_v1(&wire.evidence, episode_null_policy_v1())?;
    let expected_update_sha256 = update_evidence_sha256_v1(
        context.run_sha256,
        context.next_update_index,
        context.previous_update_evidence_sha256,
        &evidence_cj,
    )?;
    if parse_digest_v1(&wire.update_evidence_sha256)? != expected_update_sha256
        || wire.logical_row_count != logical_row_count_v1(&wire.evidence)?
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::ChainMismatch));
    }
    let next_update_index = context
        .next_update_index
        .checked_add(1)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let model_parameter_sha256 = parse_digest_v1(&wire.evidence.model_parameter_sha256_after)?;
    let train_state_sha256 = parse_digest_v1(&wire.evidence.train_state_sha256_after)?;
    let progress = wire.evidence.progress_after;
    let advanced_context = UpdateEvidenceChainContextV1 {
        run_sha256: context.run_sha256,
        identity_bundle_sha256: context.identity_bundle_sha256,
        batch_episodes: context.batch_episodes,
        checkpoint_segment_updates: context.checkpoint_segment_updates,
        next_update_index,
        previous_update_evidence_sha256: Some(expected_update_sha256),
        progress,
        model_parameter_sha256,
        train_state_sha256,
        scorer_bias_anchor_bits: context.scorer_bias_anchor_bits,
    };
    Ok(ValidatedUpdateGroupAdvanceV1 {
        group: ValidatedUpdateGroupV1 {
            wire,
            canonical_bytes,
            update_evidence_sha256: expected_update_sha256,
        },
        advanced_context,
    })
}

fn validate_group_bindings_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
    group: &UpdateGroupWireV1,
) -> Result<()> {
    let evidence = &group.evidence;
    let expected_previous = context
        .previous_update_evidence_sha256
        .map(lower_hex_raw32_v1);
    if group.update_index != context.next_update_index
        || group.previous_update_evidence_sha256 != expected_previous
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::ChainMismatch));
    }
    if evidence.schema != UPDATE_EVIDENCE_SCHEMA_V1
        || evidence.schema != run.record().artifact_schemas.update_evidence
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidSchema));
    }
    if evidence.run_sha256 != run.run_sha256()
        || evidence.identity_bundle_sha256 != run.identity_bundle_sha256()
        || evidence.batch_episodes != run.batch_episodes()
        || evidence.checkpoint_segment_updates != run.checkpoint_segment_updates()
        || evidence.update_index != context.next_update_index
        || evidence.episode_start != context.progress.next_episode_index()
        || evidence.episode_count != run.batch_episodes()
        || !evidence.optimizer_step
        || evidence.adam_step_before != context.next_update_index - 1
        || evidence.adam_step_after != context.next_update_index
        || evidence.model_parameter_sha256_before
            != lower_hex_raw32_v1(context.model_parameter_sha256)
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RunBinding));
    }
    let expected_episode_end = checked_u63_add_v1(evidence.episode_start, evidence.episode_count)?;
    if evidence.episode_end_exclusive != expected_episode_end
        || parse_digest_v1(&evidence.model_parameter_sha256_after).is_err()
        || parse_digest_v1(&evidence.train_state_sha256_after).is_err()
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidDigest));
    }
    validate_episodes_v1(run, evidence)?;
    validate_physical_and_loss_v1(run, evidence)?;
    validate_gauge_v1(run, context, evidence)?;
    validate_rollout_v1(run, evidence)?;
    let expected_progress = fold_progress_v1(&context.progress, &evidence.episodes)?;
    if evidence.progress_after != expected_progress {
        return Err(error_v1(UpdateGroupV1ErrorKind::ProgressMismatch));
    }
    Ok(())
}

fn validate_episodes_v1(run: &ValidatedTrainRunV2, evidence: &UpdateEvidenceWireV1) -> Result<()> {
    let expected_len = usize::try_from(run.batch_episodes())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    if evidence.episodes.len() != expected_len {
        return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
    }
    let mut total_policy = 0_u64;
    let mut total_physical = 0_u64;
    let mut learner_policy = 0_u64;
    let mut learner_physical = 0_u64;
    for (offset, episode) in evidence.episodes.iter().enumerate() {
        let offset = u64::try_from(offset)
            .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        let expected_index = checked_u63_add_v1(evidence.episode_start, offset)?;
        let schedule =
            native_training_episode_schedule_v1(run.record().schedule.base_seed, expected_index)
                .map_err(|_| error_v1(UpdateGroupV1ErrorKind::ScheduleBinding))?;
        let fields = [
            episode.episode_index,
            episode.policy_step_count,
            episode.physical_decision_count,
            episode.learner_policy_step_count,
            episode.opponent_policy_step_count,
            episode.learner_physical_decision_count,
            episode.opponent_physical_decision_count,
        ];
        if episode.schema != EPISODE_SCHEMA_V1
            || episode.schema != run.record().artifact_schemas.episode
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::InvalidSchema));
        }
        if fields.into_iter().any(|value| !is_u63_v1(value))
            || episode.episode_index != expected_index
            || episode.environment_seed_u64_hex != format!("{:016x}", schedule.environment_seed)
            || episode.deck_ids != run.record().environment.deck_ids
            || episode.deck_hashes_u64_hex != run.record().environment.deck_hashes_u64_hex
            || episode.learner_seat != seat_wire_v1(schedule.learner_seat)
            || episode.terminal_classification != "natural"
            || episode.terminal_code != "natural-game-over"
            || parse_digest_v1(&episode.trajectory_sha256).is_err()
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        let environment_seed = parse_u64_hex_v1(&episode.environment_seed_u64_hex)?;
        if environment_seed != schedule.environment_seed {
            return Err(error_v1(UpdateGroupV1ErrorKind::ScheduleBinding));
        }
        let expected_winner = match episode.terminal_outcome {
            OutcomeWireV1::P0Win => Some(SeatWireV1::P0),
            OutcomeWireV1::P1Win => Some(SeatWireV1::P1),
            OutcomeWireV1::Draw => None,
        };
        if episode.winner != expected_winner
            || episode.learner_return
                != learner_return_wire_v1(episode.learner_seat, episode.terminal_outcome)
            || checked_u63_add_v1(
                episode.learner_policy_step_count,
                episode.opponent_policy_step_count,
            )? != episode.policy_step_count
            || checked_u63_add_v1(
                episode.learner_physical_decision_count,
                episode.opponent_physical_decision_count,
            )? != episode.physical_decision_count
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        total_policy = checked_u63_add_v1(total_policy, episode.policy_step_count)?;
        total_physical = checked_u63_add_v1(total_physical, episode.physical_decision_count)?;
        learner_policy = checked_u63_add_v1(learner_policy, episode.learner_policy_step_count)?;
        learner_physical =
            checked_u63_add_v1(learner_physical, episode.learner_physical_decision_count)?;
    }
    if learner_policy != evidence.learner_policy_step_count
        || learner_physical != evidence.learner_physical_decision_count
        || learner_physical != evidence.learner_group_count
        || total_policy < learner_policy
        || total_physical < learner_physical
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
    }
    Ok(())
}

fn validate_physical_and_loss_v1(
    run: &ValidatedTrainRunV2,
    evidence: &UpdateEvidenceWireV1,
) -> Result<()> {
    if evidence.learner_group_count == 0
        || !is_u63_v1(evidence.learner_group_count)
        || !is_u63_v1(evidence.learner_policy_step_count)
        || !is_u63_v1(evidence.learner_physical_decision_count)
        || u64::try_from(evidence.physical_terms.len()).ok() != Some(evidence.learner_group_count)
        || evidence.learner_group_count != evidence.learner_physical_decision_count
        || evidence.learner_group_count > evidence.learner_policy_step_count
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
    }
    let mut term_index = 0_usize;
    let mut policy_count = 0_u64;
    for episode in &evidence.episodes {
        let episode_groups = usize::try_from(episode.learner_physical_decision_count)
            .map_err(|_| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
        for _ in 0..episode_groups {
            let term = evidence
                .physical_terms
                .get(term_index)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::PhysicalLattice))?;
            let q = parse_f32_hex_v1(&term.joint_log_probability_f32_bits)?;
            let value = parse_f32_hex_v1(&term.value_f32_bits)?;
            if !q.is_finite()
                || !value.is_finite()
                || term.substep_count == 0
                || term.terminal_return_i8 != episode.learner_return
                || !matches!(term.terminal_return_i8, -1..=1)
            {
                return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
            }
            policy_count = checked_u63_add_v1(policy_count, u64::from(term.substep_count))?;
            term_index = term_index
                .checked_add(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        }
    }
    if term_index != evidence.physical_terms.len()
        || policy_count != evidence.learner_policy_step_count
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
    }

    let group_f32 = exact_u64_as_f32_v1(evidence.learner_group_count)?;
    let value_coefficient =
        parse_f32_hex_v1(&run.record().optimization.value_coefficient_f32_bits)?;
    if !value_coefficient.is_finite() {
        return Err(error_v1(UpdateGroupV1ErrorKind::LossMismatch));
    }
    let mut policy_sum = 0.0_f32;
    let mut value_sum = 0.0_f32;
    for term in &evidence.physical_terms {
        let q = parse_f32_hex_v1(&term.joint_log_probability_f32_bits)?;
        let value = parse_f32_hex_v1(&term.value_f32_bits)?;
        let target = f32::from(term.terminal_return_i8);
        let advantage = target - value;
        let policy_term = (-q) * advantage;
        let value_error = value - target;
        let value_term = value_error * value_error;
        policy_sum += policy_term;
        value_sum += value_term;
        if !advantage.is_finite()
            || !policy_term.is_finite()
            || !value_error.is_finite()
            || !value_term.is_finite()
            || !policy_sum.is_finite()
            || !value_sum.is_finite()
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::LossMismatch));
        }
    }
    let weighted_value = value_coefficient * value_sum;
    let numerator = policy_sum + weighted_value;
    let total = numerator / group_f32;
    if !weighted_value.is_finite() || !numerator.is_finite() || !total.is_finite() {
        return Err(error_v1(UpdateGroupV1ErrorKind::LossMismatch));
    }
    let declared_policy = parse_f32_hex_v1(&evidence.loss.policy_sum_f32_bits)?;
    let declared_value = parse_f32_hex_v1(&evidence.loss.value_sum_f32_bits)?;
    let declared_total = parse_f32_hex_v1(&evidence.loss.total_f32_bits)?;
    if declared_policy.to_bits() != policy_sum.to_bits()
        || declared_value.to_bits() != value_sum.to_bits()
        || declared_total.to_bits() != total.to_bits()
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::LossMismatch));
    }
    Ok(())
}

fn validate_gauge_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
    evidence: &UpdateEvidenceWireV1,
) -> Result<()> {
    let gauge = &evidence.gauge;
    let policy_count = evidence.learner_policy_step_count;
    if gauge.identity != NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1
        || gauge.identity != run.record().contracts.optimizer.gauge_evidence_identity
        || gauge.parameter_name != "scorer.2.bias"
        || run.record().contracts.optimizer.canonical_gauge_parameters
            != ["scorer.2.bias".to_owned()]
        || gauge.substep_count != policy_count
        || gauge.substep_count == 0
        || u64::try_from(gauge.substep_bounds.len()).ok() != Some(policy_count)
        || gauge.total_action_count == 0
        || gauge.max_action_count == 0
        || gauge.max_action_count > MAX_LEGAL_ACTION_COUNT_V1
        || gauge.parameter_before_f32_bits != context.scorer_bias_anchor_bits
        || gauge.parameter_after_f32_bits != context.scorer_bias_anchor_bits
        || parse_f32_hex_v1(&gauge.canonical_gradient_f32_bits)?.to_bits() != 0
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
    }
    let group_f32 = exact_u64_as_f32_v1(evidence.learner_group_count)?;
    let unit_roundoff = f64::from(f32::EPSILON) / 2.0;
    let mut row_index = 0_usize;
    let mut total_action_count = 0_u64;
    let mut max_action_count = 0_u64;
    let mut sum_abs_coefficients = 0.0_f64;
    let mut per_substep_bound_sum = 0.0_f64;
    for term in evidence.physical_terms.iter().rev() {
        let value = parse_f32_hex_v1(&term.value_f32_bits)?;
        let target = f32::from(term.terminal_return_i8);
        let advantage = target - value;
        let coefficient = (-advantage) / group_f32;
        let expected_abs_coefficient = f64::from(coefficient).abs();
        if !advantage.is_finite()
            || !coefficient.is_finite()
            || !expected_abs_coefficient.is_finite()
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
        }
        for _ in (0..term.substep_count).rev() {
            let row = gauge
                .substep_bounds
                .get(row_index)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::GaugeMismatch))?;
            if row.action_count == 0 || row.action_count > MAX_LEGAL_ACTION_COUNT_V1 {
                return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
            }
            let gamma_operation_count = row
                .action_count
                .checked_mul(8)
                .and_then(|value| value.checked_add(8))
                .filter(|value| is_u63_v1(*value))
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
            let x = gamma_operation_count as f64 * unit_roundoff;
            if !x.is_finite() || x >= 1.0 {
                return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
            }
            let gamma = x / (1.0 - x);
            let bound_component = expected_abs_coefficient * gamma;
            let declared_abs = parse_f64_hex_v1(&row.abs_policy_coefficient_f64_bits)?;
            let declared_gamma = parse_f64_hex_v1(&row.gamma_f64_bits)?;
            let declared_component = parse_f64_hex_v1(&row.bound_component_f64_bits)?;
            if row.gamma_operation_count != gamma_operation_count
                || declared_abs.to_bits() != expected_abs_coefficient.to_bits()
                || declared_gamma.to_bits() != gamma.to_bits()
                || declared_component.to_bits() != bound_component.to_bits()
                || !gamma.is_finite()
                || !bound_component.is_finite()
                || declared_abs < 0.0
                || declared_gamma < 0.0
                || declared_component < 0.0
            {
                return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
            }
            sum_abs_coefficients += expected_abs_coefficient;
            per_substep_bound_sum += bound_component;
            total_action_count = checked_u63_add_v1(total_action_count, row.action_count)?;
            max_action_count = max_action_count.max(row.action_count);
            if !sum_abs_coefficients.is_finite() || !per_substep_bound_sum.is_finite() {
                return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
            }
            row_index = row_index
                .checked_add(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        }
    }
    if row_index != gauge.substep_bounds.len() {
        return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
    }
    let cross_operations = policy_count - 1;
    let cross_x = cross_operations as f64 * unit_roundoff;
    if !cross_x.is_finite() || cross_x >= 1.0 {
        return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
    }
    let cross_gamma = cross_x / (1.0 - cross_x);
    let cross_twice = cross_gamma * 2.0;
    let cross_substep_bound = cross_twice * sum_abs_coefficients;
    let derived_absolute_bound = per_substep_bound_sum + cross_substep_bound;
    let declared_sum_abs = parse_f64_hex_v1(&gauge.sum_abs_policy_coefficients_f64_bits)?;
    let declared_per_substep = parse_f64_hex_v1(&gauge.per_substep_bound_sum_f64_bits)?;
    let declared_cross = parse_f64_hex_v1(&gauge.cross_substep_bound_f64_bits)?;
    let declared_bound = parse_f64_hex_v1(&gauge.derived_absolute_bound_f64_bits)?;
    let raw_residual = parse_f32_hex_v1(&gauge.raw_gradient_residual_f32_bits)?;
    let high_precision = parse_f64_hex_v1(&gauge.high_precision_residual_f64_bits)?;
    if gauge.total_action_count != total_action_count
        || gauge.max_action_count != max_action_count
        || declared_sum_abs.to_bits() != sum_abs_coefficients.to_bits()
        || declared_per_substep.to_bits() != per_substep_bound_sum.to_bits()
        || declared_cross.to_bits() != cross_substep_bound.to_bits()
        || declared_bound.to_bits() != derived_absolute_bound.to_bits()
        || !cross_gamma.is_finite()
        || !cross_substep_bound.is_finite()
        || !derived_absolute_bound.is_finite()
        || derived_absolute_bound < 0.0
        || !raw_residual.is_finite()
        || !high_precision.is_finite()
        || f64::from(raw_residual).abs() > derived_absolute_bound
        || high_precision.abs() > derived_absolute_bound
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::GaugeMismatch));
    }
    Ok(())
}

fn validate_rollout_v1(run: &ValidatedTrainRunV2, evidence: &UpdateEvidenceWireV1) -> Result<()> {
    let counts = &evidence.rollout_counts;
    let b = run.record().topology.broker_batch_target;
    let actors = run.record().topology.logical_actor_count;
    let all_counts = [
        counts.complete_round_count,
        counts.scorer_batch_count,
        counts.scored_decision_count,
        counts.scored_action_logit_count,
        counts.sampled_action_count,
        counts.terminal_notification_count,
        counts.batch_width_sum,
        counts.max_batch_width,
        counts.full_target_batch_count,
        counts.short_batch_count,
        counts.natural_terminal_count,
        counts.halted_count,
        counts.truncated_count,
        counts.apply_error_count,
        counts.partial_group_count,
        counts.association_failure_count,
    ];
    let full_width = counts
        .full_target_batch_count
        .checked_mul(b)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let short_width = counts
        .batch_width_sum
        .checked_sub(full_width)
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::RolloutMismatch))?;
    let maximum_short_width = counts
        .short_batch_count
        .checked_mul(b.saturating_sub(1))
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    if all_counts.into_iter().any(|value| !is_u63_v1(value))
        || counts.complete_round_count == 0
        || counts.scorer_batch_count
            != checked_u63_add_v1(counts.full_target_batch_count, counts.short_batch_count)?
        || counts.scored_decision_count != evidence.learner_policy_step_count
        || counts.sampled_action_count != evidence.learner_policy_step_count
        || counts.batch_width_sum != evidence.learner_policy_step_count
        || counts.scored_action_logit_count != evidence.gauge.total_action_count
        || counts.max_batch_width == 0
        || counts.max_batch_width > b
        || (counts.full_target_batch_count > 0 && counts.max_batch_width != b)
        || (counts.full_target_batch_count == 0 && counts.max_batch_width >= b)
        || b == 0
        || b > actors
        || actors > 1024
        || counts.terminal_notification_count != run.batch_episodes()
        || counts.natural_terminal_count != run.batch_episodes()
        || counts.halted_count != 0
        || counts.truncated_count != 0
        || counts.apply_error_count != 0
        || counts.partial_group_count != 0
        || counts.association_failure_count != 0
        || counts.batch_membership_digest_identity != BATCH_MEMBERSHIP_DIGEST_IDENTITY_V1
        || parse_digest_v1(&counts.batch_membership_digest_hex).is_err()
        || (counts.short_batch_count == 0 && short_width != 0)
        || (counts.short_batch_count > 0
            && (short_width < counts.short_batch_count || short_width > maximum_short_width))
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
    }
    Ok(())
}

fn validate_progress_shape_v1(progress: &CheckpointProgressV3, update_index: u64) -> Result<()> {
    let expected_episodes = checked_u63_mul_v1(progress.batch_episodes(), update_index)?;
    let p0 = progress.outcomes_by_learner_seat().p0();
    let p1 = progress.outcomes_by_learner_seat().p1();
    let counters = [
        progress.batch_episodes(),
        progress.checkpoint_segment_updates(),
        progress.next_episode_index(),
        progress.successful_update_count(),
        progress.completed_episode_count(),
        p0.win(),
        p0.loss(),
        p0.draw(),
        p1.win(),
        p1.loss(),
        p1.draw(),
        progress.learner_policy_steps_by_seat().p0(),
        progress.learner_policy_steps_by_seat().p1(),
        progress.learner_physical_decisions_by_seat().p0(),
        progress.learner_physical_decisions_by_seat().p1(),
    ];
    let p0_total = checked_u63_add_v1(checked_u63_add_v1(p0.win(), p0.loss())?, p0.draw())?;
    let p1_total = checked_u63_add_v1(checked_u63_add_v1(p1.win(), p1.loss())?, p1.draw())?;
    if counters.into_iter().any(|value| !is_u63_v1(value))
        || progress.batch_episodes() == 0
        || progress.checkpoint_segment_updates() == 0
        || progress.next_episode_index() != expected_episodes
        || progress.completed_episode_count() != expected_episodes
        || progress.successful_update_count() != update_index
        || p0_total != expected_episodes / 2
        || p1_total != expected_episodes / 2
        || progress.learner_policy_steps_by_seat().p0()
            < progress.learner_physical_decisions_by_seat().p0()
        || progress.learner_policy_steps_by_seat().p1()
            < progress.learner_physical_decisions_by_seat().p1()
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::ProgressMismatch));
    }
    Ok(())
}

fn logical_row_count_v1(evidence: &UpdateEvidenceWireV1) -> Result<u64> {
    let episodes = u64::try_from(evidence.episodes.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let physical_terms = u64::try_from(evidence.physical_terms.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let gauge_bounds = u64::try_from(evidence.gauge.substep_bounds.len())
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let rows = 1_u64
        .checked_add(episodes)
        .and_then(|value| value.checked_add(physical_terms))
        .and_then(|value| value.checked_add(gauge_bounds))
        .filter(|value| *value > 0 && is_u63_v1(*value) && *value <= MAX_LOGICAL_ROWS_V1)
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    Ok(rows)
}

fn update_evidence_sha256_v1(
    run_sha256: [u8; 32],
    update_index: u64,
    previous_update_evidence_sha256: Option<[u8; 32]>,
    evidence_cj: &[u8],
) -> Result<[u8; 32]> {
    let mut digest = NativeTrainingStoreAtomSha256V1::new();
    digest
        .atom("domain", UPDATE_EVIDENCE_SHA256_IDENTITY_V1.as_bytes())
        .map_err(map_digest_error_v1)?;
    digest
        .atom("run_sha256", &run_sha256)
        .map_err(map_digest_error_v1)?;
    digest
        .atom("update_index_u64be", &update_index.to_be_bytes())
        .map_err(map_digest_error_v1)?;
    digest
        .atom(
            "previous_update_evidence_sha256",
            previous_update_evidence_sha256
                .as_ref()
                .map_or(&[][..], |value| value.as_slice()),
        )
        .map_err(map_digest_error_v1)?;
    digest
        .atom("evidence_canonical_json", evidence_cj)
        .map_err(map_digest_error_v1)?;
    Ok(digest.finalize())
}

fn natural_outcome_wire_v1(
    outcome: TerminalOutcomeV1,
) -> Result<(OutcomeWireV1, Option<SeatWireV1>)> {
    match outcome {
        TerminalOutcomeV1::P0Win => Ok((OutcomeWireV1::P0Win, Some(SeatWireV1::P0))),
        TerminalOutcomeV1::P1Win => Ok((OutcomeWireV1::P1Win, Some(SeatWireV1::P1))),
        TerminalOutcomeV1::Draw => Ok((OutcomeWireV1::Draw, None)),
        TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
            Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding))
        }
    }
}

const fn seat_wire_v1(seat: PlayerSeatV1) -> SeatWireV1 {
    match seat {
        PlayerSeatV1::P0 => SeatWireV1::P0,
        PlayerSeatV1::P1 => SeatWireV1::P1,
    }
}

const fn learner_return_v1(seat: PlayerSeatV1, outcome: OutcomeWireV1) -> i8 {
    learner_return_wire_v1(seat_wire_v1(seat), outcome)
}

const fn learner_return_wire_v1(seat: SeatWireV1, outcome: OutcomeWireV1) -> i8 {
    match (seat, outcome) {
        (_, OutcomeWireV1::Draw) => 0,
        (SeatWireV1::P0, OutcomeWireV1::P0Win) | (SeatWireV1::P1, OutcomeWireV1::P1Win) => 1,
        (SeatWireV1::P0, OutcomeWireV1::P1Win) | (SeatWireV1::P1, OutcomeWireV1::P0Win) => -1,
    }
}

fn exact_u64_as_f32_v1(value: u64) -> Result<f32> {
    let encoded = value as f32;
    if value == 0 || !encoded.is_finite() || (encoded as u128) != u128::from(value) {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidScalar));
    }
    Ok(encoded)
}

fn parse_digest_v1(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value).map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidDigest))
}

fn parse_f32_hex_v1(value: &str) -> Result<f32> {
    let bits = parse_fixed_lower_hex_v1(value, 8)?;
    let bits = u32::try_from(bits).map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidScalar))?;
    let decoded = f32::from_bits(bits);
    if !decoded.is_finite() {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidScalar));
    }
    Ok(decoded)
}

fn parse_f64_hex_v1(value: &str) -> Result<f64> {
    let bits = parse_fixed_lower_hex_v1(value, 16)?;
    let decoded = f64::from_bits(bits);
    if !decoded.is_finite() {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidScalar));
    }
    Ok(decoded)
}

fn parse_u64_hex_v1(value: &str) -> Result<u64> {
    parse_fixed_lower_hex_v1(value, 16)
}

fn parse_fixed_lower_hex_v1(value: &str, expected_len: usize) -> Result<u64> {
    if value.len() != expected_len
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidScalar));
    }
    u64::from_str_radix(value, 16).map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidScalar))
}

fn checked_usize_u63_v1(value: usize) -> Result<u64> {
    let converted =
        u64::try_from(value).map_err(|_| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    checked_u63_v1(converted)
}

fn checked_u63_v1(value: u64) -> Result<u64> {
    if !is_u63_v1(value) {
        return Err(error_v1(UpdateGroupV1ErrorKind::InvalidScalar));
    }
    Ok(value)
}

fn checked_u63_add_v1(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))
}

fn checked_u63_mul_v1(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))
}

const fn is_u63_v1(value: u64) -> bool {
    value <= U63_MAX_V1
}

const fn error_v1(kind: UpdateGroupV1ErrorKind) -> UpdateGroupV1Error {
    UpdateGroupV1Error::new(kind)
}

fn map_digest_error_v1(_error: NativeTrainingStoreDigestErrorV1) -> UpdateGroupV1Error {
    error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_json_v1::to_canonical_json_bytes_v1;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, decode_genesis_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use std::time::Duration;

    struct FixtureV1 {
        run_bytes: Vec<u8>,
        genesis_manifest_bytes: Vec<u8>,
        genesis_payload: Vec<u8>,
        group_bytes: Vec<u8>,
        second_group_bytes: Vec<u8>,
    }

    static FIXTURE_V1: OnceLock<FixtureV1> = OnceLock::new();

    fn fixture_v1() -> &'static FixtureV1 {
        FIXTURE_V1.get_or_init(|| {
            let run_bytes = test_fixture_bytes_v2();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
            let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
                NativeTrainingExecutionConfigV1 {
                    run_base_seed: run.record().schedule.base_seed,
                    batch_episodes: run.batch_episodes(),
                    deck_ids: run.record().environment.deck_ids.clone(),
                    max_physical_decisions: run.record().limits.max_physical_decisions,
                    max_policy_steps: run.record().limits.max_policy_steps,
                    worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
                    sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker)
                        .unwrap(),
                    broker_batch_target: usize::try_from(run.record().topology.broker_batch_target)
                        .unwrap(),
                    scheduler_timeout: Duration::from_millis(
                        run.record().topology.scheduler_timeout_ms,
                    ),
                    measure_broker_service_time: run.record().topology.measure_broker_service_time,
                    value_coefficient_bits: parse_f32_hex_v1(
                        &run.record().optimization.value_coefficient_f32_bits,
                    )
                    .unwrap()
                    .to_bits(),
                    learning_rate_bits: parse_f32_hex_v1(
                        &run.record().optimization.learning_rate_f32_bits,
                    )
                    .unwrap()
                    .to_bits(),
                },
                &snapshot_manifest,
                &snapshot_payload,
            )
            .unwrap();
            let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
            let genesis_payload = genesis_candidate.payload().to_vec();
            let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
            let genesis_manifest_bytes = genesis.canonical_bytes().to_vec();
            let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
            let group_bytes = {
                let prepared = executor.prepare_update_v2().unwrap();
                let mut mismatched_observation = prepared.observation().clone();
                mismatched_observation.model_digest_after = "00".repeat(32);
                assert_eq!(
                    build_update_group_v1(
                        &run,
                        begin_update_evidence_chain_v1(&run, &genesis).unwrap(),
                        &mismatched_observation,
                        prepared.checkpoint_candidate(),
                    )
                    .unwrap_err()
                    .kind(),
                    UpdateGroupV1ErrorKind::CheckpointMismatch
                );
                let built = build_update_group_v1(
                    &run,
                    context,
                    prepared.observation(),
                    prepared.checkpoint_candidate(),
                )
                .unwrap();
                built.group().canonical_bytes().to_vec()
            };
            let committed_context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
            let first_observation = executor.run_update_v2().unwrap();
            let first_checkpoint = executor
                .checkpoint_candidate_for_observation_v2(&first_observation)
                .unwrap();
            let committed_first = build_update_group_v1(
                &run,
                committed_context,
                &first_observation,
                &first_checkpoint,
            )
            .unwrap();
            assert_eq!(committed_first.group().canonical_bytes(), group_bytes);
            let (_, second_context) = committed_first.into_parts();
            let second_observation = executor.run_update_v2().unwrap();
            let second_checkpoint = executor
                .checkpoint_candidate_for_observation_v2(&second_observation)
                .unwrap();
            let second_group_bytes = build_update_group_v1(
                &run,
                second_context,
                &second_observation,
                &second_checkpoint,
            )
            .unwrap()
            .group()
            .canonical_bytes()
            .to_vec();
            FixtureV1 {
                run_bytes,
                genesis_manifest_bytes,
                genesis_payload,
                group_bytes,
                second_group_bytes,
            }
        })
    }

    fn run_and_context_v1() -> (ValidatedTrainRunV2, UpdateEvidenceChainContextV1) {
        let fixture = fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let genesis = decode_genesis_checkpoint_manifest_v3(
            &fixture.genesis_manifest_bytes,
            &fixture.genesis_payload,
            &run,
        )
        .unwrap();
        let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        (run, context)
    }

    fn group_value_v1() -> Value {
        serde_json::from_slice(fixture_v1().group_bytes.strip_suffix(b"\n").unwrap()).unwrap()
    }

    fn canonical_group_value_v1(value: &Value) -> Vec<u8> {
        to_canonical_json_bytes_v1(value, GROUP_NULL_POLICY_V1).unwrap()
    }

    fn decode_value_error_v1(value: &Value) -> UpdateGroupV1ErrorKind {
        let (run, context) = run_and_context_v1();
        decode_update_group_v1(&run, context, &canonical_group_value_v1(value))
            .unwrap_err()
            .kind()
    }

    fn reference_update_hash_v1(group: &Value, include_evidence_lf: bool) -> [u8; 32] {
        fn append_atom(bytes: &mut Vec<u8>, tag: &str, payload: &[u8]) {
            bytes.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
            bytes.extend_from_slice(tag.as_bytes());
            bytes.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            bytes.extend_from_slice(payload);
        }

        const WINNER: &[CanonicalJsonNullPathSegmentV1] = &[
            CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
            CanonicalJsonNullPathSegmentV1::AnyArrayElement,
            CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
        ];
        let mut evidence = to_canonical_json_bytes_v1(
            &group["evidence"],
            CanonicalJsonNullPolicyV1::AllowOnly(&[WINNER]),
        )
        .unwrap();
        if !include_evidence_lf {
            assert_eq!(evidence.pop(), Some(b'\n'));
        }
        let run_sha256 =
            parse_digest_v1(group["evidence"]["run_sha256"].as_str().unwrap()).unwrap();
        let previous = group["previous_update_evidence_sha256"]
            .as_str()
            .map(parse_digest_v1)
            .transpose()
            .unwrap();
        let update_index = group["update_index"].as_u64().unwrap();
        let mut framed = Vec::new();
        append_atom(
            &mut framed,
            "domain",
            UPDATE_EVIDENCE_SHA256_IDENTITY_V1.as_bytes(),
        );
        append_atom(&mut framed, "run_sha256", &run_sha256);
        append_atom(
            &mut framed,
            "update_index_u64be",
            &update_index.to_be_bytes(),
        );
        append_atom(
            &mut framed,
            "previous_update_evidence_sha256",
            previous.as_ref().map_or(&[][..], |value| value.as_slice()),
        );
        append_atom(&mut framed, "evidence_canonical_json", &evidence);
        Sha256::digest(framed).into()
    }

    #[test]
    fn real_k2_prepared_update_roundtrips_and_advances_exact_chain() {
        let fixture = fixture_v1();
        let (run, context) = run_and_context_v1();
        let decoded = decode_update_group_v1(&run, context, &fixture.group_bytes).unwrap();
        assert_eq!(decoded.group().canonical_bytes(), fixture.group_bytes);
        assert_eq!(decoded.group().update_index(), 1);
        assert!(decoded.group().previous_update_evidence_sha256().is_none());
        assert_eq!(
            decoded.group().logical_row_count(),
            1 + run.batch_episodes()
                + u64::try_from(decoded.group().wire.evidence.physical_terms.len()).unwrap()
                + u64::try_from(decoded.group().wire.evidence.gauge.substep_bounds.len(),).unwrap()
        );
        let group_value = group_value_v1();
        assert_eq!(
            decoded.group().update_evidence_sha256(),
            reference_update_hash_v1(&group_value, true)
        );
        assert_ne!(
            decoded.group().update_evidence_sha256(),
            reference_update_hash_v1(&group_value, false),
            "CJ(evidence) final LF is hash-significant"
        );
        assert_eq!(
            UPDATE_GROUP_RECORD_CONTRACT_SHA256_V1,
            "53d5e4f8585e28e95870c54407e7a8a6ce6e292d9d85a30ba53197c04cd0ee0d"
        );
        assert_eq!(decoded.advanced_context().next_update_index(), 2);
        assert_eq!(
            decoded.advanced_context().previous_update_evidence_sha256(),
            Some(decoded.group().update_evidence_sha256())
        );
        assert_eq!(
            decoded
                .advanced_context()
                .progress()
                .successful_update_count(),
            1
        );
        assert_eq!(
            decoded.advanced_context().progress().next_episode_index(),
            2
        );
        let first_hash = decoded.group().update_evidence_sha256();
        let (_, second_context) = decoded.into_parts();
        let second =
            decode_update_group_v1(&run, second_context, &fixture.second_group_bytes).unwrap();
        assert_eq!(second.group().update_index(), 2);
        assert_eq!(
            second.group().previous_update_evidence_sha256(),
            Some(lower_hex_raw32_v1(first_hash).as_str())
        );
        assert_eq!(second.advanced_context().next_update_index(), 3);
        assert_eq!(
            second
                .advanced_context()
                .progress()
                .successful_update_count(),
            2
        );
        assert_eq!(second.advanced_context().progress().next_episode_index(), 4);
    }

    #[test]
    fn closed_wire_and_exact_null_paths_fail_closed() {
        let mut schema = group_value_v1();
        schema["evidence"]["schema"] = Value::String("wrong".to_owned());
        assert_eq!(
            decode_value_error_v1(&schema),
            UpdateGroupV1ErrorKind::InvalidSchema
        );

        let mut unknown = group_value_v1();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), Value::Bool(true));
        assert_eq!(
            decode_value_error_v1(&unknown),
            UpdateGroupV1ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::Deserialization)
        );

        let mut allowed_winner_null = group_value_v1();
        allowed_winner_null["evidence"]["episodes"][0]["terminal_outcome"] =
            Value::String("p0_win".to_owned());
        allowed_winner_null["evidence"]["episodes"][0]["winner"] = Value::Null;
        assert_eq!(
            decode_value_error_v1(&allowed_winner_null),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );

        let original = fixture_v1().group_bytes.clone();
        let logical = group_value_v1()["logical_row_count"].as_u64().unwrap();
        let forbidden = String::from_utf8(original)
            .unwrap()
            .replace(
                &format!("\"logical_row_count\":{logical}"),
                "\"logical_row_count\":null",
            )
            .into_bytes();
        let (run, context) = run_and_context_v1();
        assert_eq!(
            decode_update_group_v1(&run, context, &forbidden)
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::CanonicalJson(CanonicalJsonErrorKindV1::NullForbidden)
        );
    }

    #[test]
    fn schedule_physical_loss_gauge_rollout_progress_and_chain_corruptions_reject() {
        let mut run_binding = group_value_v1();
        run_binding["evidence"]["batch_episodes"] = Value::from(4_u64);
        assert_eq!(
            decode_value_error_v1(&run_binding),
            UpdateGroupV1ErrorKind::RunBinding
        );

        let mut adam = group_value_v1();
        adam["evidence"]["adam_step_after"] = Value::from(2_u64);
        assert_eq!(
            decode_value_error_v1(&adam),
            UpdateGroupV1ErrorKind::RunBinding
        );

        let mut model_before = group_value_v1();
        model_before["evidence"]["model_parameter_sha256_before"] = Value::String("00".repeat(32));
        assert_eq!(
            decode_value_error_v1(&model_before),
            UpdateGroupV1ErrorKind::RunBinding
        );

        let mut train_state = group_value_v1();
        train_state["evidence"]["train_state_sha256_after"] = Value::String("bad".to_owned());
        assert_eq!(
            decode_value_error_v1(&train_state),
            UpdateGroupV1ErrorKind::InvalidDigest
        );

        let mut schedule = group_value_v1();
        schedule["evidence"]["episodes"][0]["environment_seed_u64_hex"] =
            Value::String("0000000000000000".to_owned());
        assert_eq!(
            decode_value_error_v1(&schedule),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );

        let mut physical = group_value_v1();
        physical["evidence"]["physical_terms"][0]["substep_count"] = Value::from(0_u64);
        assert_eq!(
            decode_value_error_v1(&physical),
            UpdateGroupV1ErrorKind::PhysicalLattice
        );

        let mut loss = group_value_v1();
        let loss_bits = loss["evidence"]["loss"]["total_f32_bits"].as_str().unwrap();
        let changed_loss_bits = u32::from_str_radix(loss_bits, 16).unwrap() ^ 1;
        loss["evidence"]["loss"]["total_f32_bits"] =
            Value::String(format!("{changed_loss_bits:08x}"));
        assert_eq!(
            decode_value_error_v1(&loss),
            UpdateGroupV1ErrorKind::LossMismatch
        );

        let mut gauge = group_value_v1();
        gauge["evidence"]["gauge"]["substep_bounds"][0]["gamma_f64_bits"] =
            Value::String("0000000000000000".to_owned());
        assert_eq!(
            decode_value_error_v1(&gauge),
            UpdateGroupV1ErrorKind::GaugeMismatch
        );

        let mut rollout = group_value_v1();
        let decisions = rollout["evidence"]["rollout_counts"]["scored_decision_count"]
            .as_u64()
            .unwrap();
        rollout["evidence"]["rollout_counts"]["scored_decision_count"] = Value::from(decisions + 1);
        assert_eq!(
            decode_value_error_v1(&rollout),
            UpdateGroupV1ErrorKind::RolloutMismatch
        );

        let mut progress = group_value_v1();
        progress["evidence"]["progress_after"]["successful_update_count"] = Value::from(2_u64);
        assert_eq!(
            decode_value_error_v1(&progress),
            UpdateGroupV1ErrorKind::ProgressMismatch
        );

        let mut previous = group_value_v1();
        previous["previous_update_evidence_sha256"] = Value::String("11".repeat(32));
        assert_eq!(
            decode_value_error_v1(&previous),
            UpdateGroupV1ErrorKind::ChainMismatch
        );

        let mut digest = group_value_v1();
        digest["update_evidence_sha256"] = Value::String("22".repeat(32));
        assert_eq!(
            decode_value_error_v1(&digest),
            UpdateGroupV1ErrorKind::ChainMismatch
        );

        let mut rows = group_value_v1();
        let row_count = rows["logical_row_count"].as_u64().unwrap();
        rows["logical_row_count"] = Value::from(row_count + 1);
        assert_eq!(
            decode_value_error_v1(&rows),
            UpdateGroupV1ErrorKind::ChainMismatch
        );
    }
}
