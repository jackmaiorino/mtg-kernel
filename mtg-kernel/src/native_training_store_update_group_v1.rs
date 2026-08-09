//! Pure EpisodeV1, UpdateEvidenceV1, and UpdateGroupV1 record authority.
//!
//! This module validates exactly one complete native training update in
//! memory. It has no filesystem, continuation partitioner, publisher, receipt,
//! live-executor mutation, or checkpoint-manifest construction. The 256 MiB
//! standalone decode ceiling is only a conservative memory-safety ceiling; it
//! is not the Store continuation file cap or representability authority.

use crate::async_flat_scored_rollout_v2::ASYNC_FLAT_SCORED_MEMBERSHIP_DIGEST_IDENTITY_V1;
use crate::bounded_staleness_async_v1::{check_staleness_bound_v1, StalenessLedgerEntryV1};
use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonClosedMaxErrorV1,
    CanonicalJsonClosedMaxV1, CanonicalJsonErrorKindV1, CanonicalJsonErrorV1,
    CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use crate::native_policy_train_step_v1::{
    CANONICAL_GAUGE_PARAMETERS_V1, NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1,
};
use crate::native_population_opponent_v1::POPULATION_OPPONENT_SLOT_COUNT_V1;
use crate::native_training_executor_v1::{
    native_training_episode_schedule_v1, NativeTrainingCheckpointCandidateV1,
    NativeTrainingExecutionConfigV1, NativeTrainingIntrinsicCheckpointFactsV2,
    NativeTrainingNumericalBackendV1, NativeTrainingPreparedTransitionV2,
    NativeTrainingPreparedUpdateV2, NativeTrainingProgressV1, NativeTrainingUpdateObservationV2,
};
use crate::native_training_store_boundary_v2::ValidatedNativeTrainingBoundaryV2;
use crate::native_training_store_checkpoint_v3::{
    maximum_checkpoint_progress_json_shape_v3, CheckpointManifestV3, CheckpointProgressV3,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, NativeTrainingStoreAtomSha256V1,
    NativeTrainingStoreDigestErrorV1,
};
use crate::native_training_store_run_v2::{
    NativeRunEnvironmentTrajectoryContractV1, ValidatedTrainRunV2,
};
use crate::rl::{PlayerSeatV1, TerminalOutcomeV1};
use serde::{Deserialize, Serialize};
use std::alloc::Layout;
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

// Zero-side-effect ordering instrumentation: caller-thread counters proving
// whether evidence-context construction or Store evidence projection ran
// before a rejection. Test-only.
#[cfg(test)]
thread_local! {
    static EVIDENCE_CONTEXT_CONSTRUCTION_COUNT_V2: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    static STORE_EVIDENCE_PROJECTION_COUNT_V2: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// Run-local RAII counting scope: entry zeroes the calling thread's counters
/// after saving them; drop restores the saved values on every exit path,
/// including panics, so stale evidence can never leak into a later test on a
/// reused harness thread and nested scopes stay isolated.
#[cfg(test)]
pub(crate) struct StoreEvidenceCountScopeV2 {
    saved: (u64, u64),
    thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl StoreEvidenceCountScopeV2 {
    /// `(evidence_context_constructions, store_evidence_projections)`
    /// observed on the calling thread inside this scope.
    pub(crate) fn counts(&self) -> (u64, u64) {
        (
            EVIDENCE_CONTEXT_CONSTRUCTION_COUNT_V2.with(std::cell::Cell::get),
            STORE_EVIDENCE_PROJECTION_COUNT_V2.with(std::cell::Cell::get),
        )
    }
}

#[cfg(test)]
impl Drop for StoreEvidenceCountScopeV2 {
    fn drop(&mut self) {
        EVIDENCE_CONTEXT_CONSTRUCTION_COUNT_V2.with(|count| count.set(self.saved.0));
        STORE_EVIDENCE_PROJECTION_COUNT_V2.with(|count| count.set(self.saved.1));
    }
}

#[cfg(test)]
pub(crate) fn store_evidence_count_scope_v2() -> StoreEvidenceCountScopeV2 {
    let saved = (
        EVIDENCE_CONTEXT_CONSTRUCTION_COUNT_V2.with(|count| count.replace(0)),
        STORE_EVIDENCE_PROJECTION_COUNT_V2.with(|count| count.replace(0)),
    );
    StoreEvidenceCountScopeV2 {
        saved,
        thread_bound: std::marker::PhantomData,
    }
}
// Widened 2026-07-21 (ledger #307) in lockstep with the segment continuation
// row bound to admit K=256+ update groups.
const MAX_LOGICAL_ROWS_V1: u64 = 4_194_304;
const CONSERVATIVE_STANDALONE_GROUP_CJ_CEILING_V1: usize = 256 * 1024 * 1024;
const MAX_LEGAL_ACTION_COUNT_V1: u64 = 64;

#[derive(Clone, Copy)]
struct UpdateCheckpointFactsV1 {
    base_seed: u64,
    batch_episodes: u64,
    numerical_backend: NativeTrainingNumericalBackendV1,
    backward_worker_limit: usize,
    progress: NativeTrainingProgressV1,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    model_parameter_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
}

impl UpdateCheckpointFactsV1 {
    fn from_checkpoint_v1(checkpoint: &NativeTrainingCheckpointCandidateV1) -> Self {
        let digests = checkpoint.digests();
        Self {
            base_seed: checkpoint.base_seed(),
            batch_episodes: checkpoint.batch_episodes(),
            numerical_backend: checkpoint.numerical_backend(),
            backward_worker_limit: checkpoint.backward_worker_limit(),
            progress: checkpoint.progress(),
            adam_step: checkpoint.adam_step(),
            scorer_bias_anchor_bits: checkpoint.scorer_bias_anchor_bits(),
            model_parameter_sha256: digests.model_parameter_sha256,
            train_state_sha256: digests.native_state_sha256,
        }
    }

    fn from_intrinsic_v2(facts: &NativeTrainingIntrinsicCheckpointFactsV2) -> Self {
        Self {
            base_seed: facts.base_seed_v2(),
            batch_episodes: facts.batch_episodes_v2(),
            numerical_backend: facts.numerical_backend_v2(),
            backward_worker_limit: facts.backward_worker_limit_v2(),
            progress: facts.progress_v2(),
            adam_step: facts.adam_step_v2(),
            scorer_bias_anchor_bits: facts.scorer_bias_anchor_bits_v2(),
            model_parameter_sha256: facts.model_parameter_sha256_v2(),
            train_state_sha256: facts.train_state_sha256_v2(),
        }
    }
}

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
    /// Population-opponent slot (0-7) this episode's opponent seat used.
    /// `None` for records written before this field existed, and for
    /// episodes with no population opponent installed (ladder opponent or
    /// plain self-play). Omitted from the wire (not written as `null`) in
    /// both cases: `#[serde(default)]` lets a pre-existing segment
    /// continuation file, which never wrote this key, keep decoding, and
    /// `skip_serializing_if` re-emits that same absence on the way back out
    /// so the canonical round-trip byte check still passes for old records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opponent_population_slot: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opponent_run_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opponent_checkpoint_manifest_sha256: Option<String>,
    /// Bounded-staleness async-inference provenance: which trainer weight
    /// version scored this episode's rollout, and which trainer update its
    /// data is destined to train. Both `None` for every record written
    /// before this field existed and for every synchronous-mode run (the
    /// structural default): the synchronous path never dispatches through
    /// `BoundedStalenessSchedulerV1`, so it has nothing to stamp here.
    /// `#[serde(default)]` and `skip_serializing_if` give the identical
    /// decode-then-reencode round-trip guarantee the opponent-identity
    /// fields above already rely on. Present together only, never singly
    /// (see `validate_episodes_v1`), and always satisfy
    /// `consuming_update_version >= scoring_weight_version` (the same
    /// causality rule `check_staleness_bound_v1` enforces at the scheduler).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scoring_weight_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    consuming_update_version: Option<u64>,
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
pub(crate) struct UpdateGroupWireV1 {
    update_index: u64,
    previous_update_evidence_sha256: Option<String>,
    evidence: UpdateEvidenceWireV1,
    update_evidence_sha256: String,
    logical_row_count: u64,
}

/// Complete allocation-free maximum for one embedded `UpdateGroupV1` JSON
/// token. The standalone document is this value plus one final LF; a segment
/// continuation embeds the token itself.
///
/// Cardinalities are supplied only by the sealed representability planner:
/// exactly `K` episodes, `G_MAX` physical terms, and `P_MAX` gauge bounds.
pub(crate) fn maximum_update_group_json_shape_v2(
    episode_count: u64,
    physical_term_count: u64,
    gauge_bound_count: u64,
) -> std::result::Result<CanonicalJsonClosedMaxV1, CanonicalJsonClosedMaxErrorV1> {
    let u63 = CanonicalJsonClosedMaxV1::max_u63_v1();
    let u32_value = CanonicalJsonClosedMaxV1::max_u32_v1();
    let zero = CanonicalJsonClosedMaxV1::exact_unsigned_decimal_digits_v1(1)?;
    let hex8 = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(8)?;
    let hex16 = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(16)?;
    let digest = CanonicalJsonClosedMaxV1::fixed_ascii_string_bytes_v1(64)?;
    let seat = CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("p0")?;

    let episode = CanonicalJsonClosedMaxV1::object_v1(&[
        ("consuming_update_version", u63),
        (
            "deck_hashes_u64_hex",
            CanonicalJsonClosedMaxV1::array_v1(2, hex16)?,
        ),
        (
            "deck_ids",
            CanonicalJsonClosedMaxV1::array_v1(
                2,
                CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("Rally")?,
            )?,
        ),
        ("environment_seed_u64_hex", hex16),
        ("episode_index", u63),
        ("learner_physical_decision_count", u63),
        ("learner_policy_step_count", u63),
        (
            "learner_return",
            CanonicalJsonClosedMaxV1::terminal_return_i8_v1(),
        ),
        ("learner_seat", seat),
        ("opponent_checkpoint_manifest_sha256", digest),
        ("opponent_physical_decision_count", u63),
        ("opponent_policy_step_count", u63),
        ("opponent_population_slot", u32_value),
        ("opponent_run_sha256", digest),
        ("physical_decision_count", u63),
        ("policy_step_count", u63),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(EPISODE_SCHEMA_V1)?,
        ),
        ("scoring_weight_version", u63),
        (
            "terminal_classification",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("natural")?,
        ),
        (
            "terminal_code",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("natural-game-over")?,
        ),
        (
            "terminal_outcome",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("p0_win")?,
        ),
        ("trajectory_sha256", digest),
        (
            "winner",
            CanonicalJsonClosedMaxV1::choice_v1(CanonicalJsonClosedMaxV1::null_v1(), seat)?,
        ),
    ])?;
    let physical_term = CanonicalJsonClosedMaxV1::object_v1(&[
        ("joint_log_probability_f32_bits", hex8),
        ("substep_count", u32_value),
        (
            "terminal_return_i8",
            CanonicalJsonClosedMaxV1::terminal_return_i8_v1(),
        ),
        ("value_f32_bits", hex8),
    ])?;
    let loss = CanonicalJsonClosedMaxV1::object_v1(&[
        ("policy_sum_f32_bits", hex8),
        ("total_f32_bits", hex8),
        ("value_sum_f32_bits", hex8),
    ])?;
    let gauge_bound = CanonicalJsonClosedMaxV1::object_v1(&[
        ("abs_policy_coefficient_f64_bits", hex16),
        ("action_count", u63),
        ("bound_component_f64_bits", hex16),
        ("gamma_f64_bits", hex16),
        ("gamma_operation_count", u63),
    ])?;
    let gauge = CanonicalJsonClosedMaxV1::object_v1(&[
        (
            "canonical_gradient_f32_bits",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("00000000")?,
        ),
        ("cross_substep_bound_f64_bits", hex16),
        ("derived_absolute_bound_f64_bits", hex16),
        ("high_precision_residual_f64_bits", hex16),
        (
            "identity",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(
                NATIVE_SCORER_BIAS_GAUGE_EVIDENCE_IDENTITY_V1,
            )?,
        ),
        ("max_action_count", u63),
        ("parameter_after_f32_bits", u32_value),
        ("parameter_before_f32_bits", u32_value),
        (
            "parameter_name",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(CANONICAL_GAUGE_PARAMETERS_V1[0])?,
        ),
        ("per_substep_bound_sum_f64_bits", hex16),
        ("raw_gradient_residual_f32_bits", hex8),
        (
            "substep_bounds",
            CanonicalJsonClosedMaxV1::array_v1(gauge_bound_count, gauge_bound)?,
        ),
        ("substep_count", u63),
        ("sum_abs_policy_coefficients_f64_bits", hex16),
        ("total_action_count", u63),
    ])?;
    let rollout_counts = CanonicalJsonClosedMaxV1::object_v1(&[
        ("apply_error_count", zero),
        ("association_failure_count", zero),
        ("batch_membership_digest_hex", digest),
        (
            "batch_membership_digest_identity",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(BATCH_MEMBERSHIP_DIGEST_IDENTITY_V1)?,
        ),
        ("batch_width_sum", u63),
        ("complete_round_count", u63),
        ("full_target_batch_count", u63),
        ("halted_count", zero),
        ("max_batch_width", u63),
        ("natural_terminal_count", u63),
        ("partial_group_count", zero),
        ("sampled_action_count", u63),
        ("scored_action_logit_count", u63),
        ("scored_decision_count", u63),
        ("scorer_batch_count", u63),
        ("short_batch_count", u63),
        ("terminal_notification_count", u63),
        ("truncated_count", zero),
    ])?;
    let evidence = CanonicalJsonClosedMaxV1::object_v1(&[
        ("adam_step_after", u63),
        ("adam_step_before", u63),
        ("batch_episodes", u63),
        ("checkpoint_segment_updates", u63),
        ("episode_count", u63),
        ("episode_end_exclusive", u63),
        ("episode_start", u63),
        (
            "episodes",
            CanonicalJsonClosedMaxV1::array_v1(episode_count, episode)?,
        ),
        ("gauge", gauge),
        ("identity_bundle_sha256", digest),
        ("learner_group_count", u63),
        ("learner_physical_decision_count", u63),
        ("learner_policy_step_count", u63),
        ("loss", loss),
        ("model_parameter_sha256_after", digest),
        ("model_parameter_sha256_before", digest),
        ("optimizer_step", CanonicalJsonClosedMaxV1::bool_v1(true)),
        (
            "physical_terms",
            CanonicalJsonClosedMaxV1::array_v1(physical_term_count, physical_term)?,
        ),
        (
            "progress_after",
            maximum_checkpoint_progress_json_shape_v3()?,
        ),
        ("rollout_counts", rollout_counts),
        ("run_sha256", digest),
        (
            "schema",
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(UPDATE_EVIDENCE_SCHEMA_V1)?,
        ),
        ("train_state_sha256_after", digest),
        ("update_index", u63),
    ])?;
    CanonicalJsonClosedMaxV1::object_v1(&[
        ("evidence", evidence),
        ("logical_row_count", u63),
        (
            "previous_update_evidence_sha256",
            CanonicalJsonClosedMaxV1::choice_v1(CanonicalJsonClosedMaxV1::null_v1(), digest)?,
        ),
        ("update_evidence_sha256", digest),
        ("update_index", u63),
    ])
}

/// Exact architecture-dependent allocation products for the private vectors
/// created while validating one maximal update group.
///
/// Keeping this walk beside the private wire types prevents a preflight from
/// substituting an unrelated public type with a different layout.
pub(crate) fn update_group_allocation_layout_bytes_v2(
    retained_episode_count: usize,
    retained_physical_term_count: usize,
    retained_gauge_bound_count: usize,
    physical_term_scratch_count: usize,
) -> Option<[u64; 4]> {
    Some([
        allocation_layout_bytes_v2::<EpisodeWireV1>(retained_episode_count)?,
        allocation_layout_bytes_v2::<PhysicalLossTermWireV1>(retained_physical_term_count)?,
        allocation_layout_bytes_v2::<GaugeSubstepBoundWireV1>(retained_gauge_bound_count)?,
        allocation_layout_bytes_v2::<usize>(physical_term_scratch_count)?,
    ])
}

fn allocation_layout_bytes_v2<T>(count: usize) -> Option<u64> {
    u64::try_from(Layout::array::<T>(count).ok()?.size()).ok()
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

    pub(crate) fn into_embedded_wire_v1(self) -> UpdateGroupWireV1 {
        self.wire
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

    pub(crate) const fn run_sha256_raw_v1(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub(crate) const fn identity_bundle_sha256_raw_v1(&self) -> [u8; 32] {
        self.identity_bundle_sha256
    }

    pub(crate) const fn batch_episodes_v1(&self) -> u64 {
        self.batch_episodes
    }

    pub(crate) const fn checkpoint_segment_updates_v1(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub(crate) const fn scorer_bias_anchor_bits_v1(&self) -> u32 {
        self.scorer_bias_anchor_bits
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

/// Reconstructs a move-only evidence-chain context from one lineage-complete
/// sealed boundary and its exact concrete checkpoint.
///
/// This is crate-private because Store currentness and resume orchestration are
/// not record-layer authority. The constructor accepts no raw parent facts and
/// independently rechecks every checkpoint fact needed by the next update.
pub(crate) fn resume_update_evidence_chain_v1(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    parent_checkpoint: &CheckpointManifestV3,
) -> Result<UpdateEvidenceChainContextV1> {
    #[cfg(test)]
    EVIDENCE_CONTEXT_CONSTRUCTION_COUNT_V2.with(|count| count.set(count.get() + 1));
    let parent_facts = parent.boundary_facts_v2();
    let generation_index = parent_facts.generation_index;
    let segment_ordinal = parent_facts.segment_ordinal;
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    let expected_generation = segment_ordinal
        .checked_mul(checkpoint_segment_updates)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let next_update_index = generation_index
        .checked_add(1)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let scorer_bias_anchor_bits = u32::try_from(
        parent_checkpoint
            .train_state()
            .scorer_bias_anchor_f32_bits(),
    )
    .map_err(|_| error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch))?;
    let expected_parent_options = generation_index != 0;
    let progress = parent_checkpoint.progress();

    if parent_facts.run_sha256 != run.run_sha256()
        || parent_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || parent_facts.batch_episodes != run.batch_episodes()
        || parent_facts.checkpoint_segment_updates != checkpoint_segment_updates
        || parent_checkpoint.run_sha256() != run.run_sha256()
        || parent_checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
        || parent_checkpoint.batch_episodes() != run.batch_episodes()
        || parent_checkpoint.checkpoint_segment_updates() != checkpoint_segment_updates
        || parent_checkpoint.segment_ordinal() != segment_ordinal
        || parent_checkpoint.generation_index() != generation_index
        || expected_generation != generation_index
        || generation_index >= run.requested_successful_updates()
        || progress.batch_episodes() != run.batch_episodes()
        || progress.checkpoint_segment_updates() != checkpoint_segment_updates
        || progress.successful_update_count() != generation_index
        || progress.next_episode_index()
            != checked_u63_mul_v1(run.batch_episodes(), generation_index)?
        || progress.completed_episode_count() != progress.next_episode_index()
        || parent_facts.parent_head_sha256.is_some() != expected_parent_options
        || parent_facts.last_update_evidence_sha256.is_some() != expected_parent_options
        || parent_facts.checkpoint_manifest_sha256 != parent_checkpoint.checkpoint_manifest_sha256()
        || parent_facts.checkpoint_payload_sha256 != parent_checkpoint.checkpoint_payload_sha256()
        || parent_facts.logical_state_sha256 != parent_checkpoint.logical_state_sha256()
        || parent_facts.model_parameter_sha256 != parent_checkpoint.model_parameter_sha256()
        || parent_facts.train_state_sha256 != parent_checkpoint.train_state_sha256()
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch));
    }

    let context = UpdateEvidenceChainContextV1 {
        run_sha256: parse_digest_v1(run.run_sha256())?,
        identity_bundle_sha256: parse_digest_v1(run.identity_bundle_sha256())?,
        batch_episodes: run.batch_episodes(),
        checkpoint_segment_updates,
        next_update_index,
        previous_update_evidence_sha256: parent_facts.last_update_evidence_sha256,
        progress: *progress,
        model_parameter_sha256: parent_checkpoint.model_parameter_sha256(),
        train_state_sha256: parent_checkpoint.train_state_sha256(),
        scorer_bias_anchor_bits,
    };
    validate_context_run_v1(run, &context)?;
    Ok(context)
}

/// Builds a complete group from one opaque prepared-update guard and advances
/// the consumed evidence context without mutating the live executor.
///
/// The guard is the sole public producer authority: its private fields bind the
/// observation and successor checkpoint to the same isolated execution, while
/// its exclusive live-executor borrow supplies the actual configuration and
/// verified predecessor state. Raw observation/checkpoint parts are never a
/// public construction path.
///
/// ```compile_fail
/// use mtg_kernel::native_training_executor_v1::{
///     NativeTrainingCheckpointCandidateV1, NativeTrainingUpdateObservationV2,
/// };
/// use mtg_kernel::native_training_store_run_v2::ValidatedTrainRunV2;
/// use mtg_kernel::native_training_store_update_group_v1::{
///     build_update_group_v1, UpdateEvidenceChainContextV1,
/// };
/// fn forged_raw_parts(
///     run: &ValidatedTrainRunV2,
///     context: UpdateEvidenceChainContextV1,
///     observation: &NativeTrainingUpdateObservationV2,
///     checkpoint: &NativeTrainingCheckpointCandidateV1,
/// ) {
///     let _ = build_update_group_v1(run, context, observation, checkpoint);
/// }
/// ```
pub fn build_update_group_v1(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    prepared: &NativeTrainingPreparedUpdateV2<'_>,
) -> Result<ValidatedUpdateGroupAdvanceV1> {
    validate_context_run_v1(run, &context)?;
    validate_prepared_execution_config_v1(run, prepared.execution_config_v1())?;
    validate_run_transition_environment_diagonal_v1(
        run,
        prepared.environment_trajectory_contract_v1(),
    )?;
    preflight_receipt_environment_diagonal_v1(run, prepared.observation())?;
    let predecessor_checkpoint = prepared
        .pre_update_checkpoint_candidate_v1()
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch))?;
    let predecessor = UpdateCheckpointFactsV1::from_checkpoint_v1(&predecessor_checkpoint);
    let successor = UpdateCheckpointFactsV1::from_checkpoint_v1(prepared.checkpoint_candidate());
    build_update_group_from_parts_v1(
        run,
        context,
        &predecessor,
        prepared.observation(),
        &successor,
    )
}

/// Exhaustive Store admission of the run/transition mode diagonal: a Legacy
/// run admits only a Legacy-sealed producer and an environment randomization
/// V2 run admits only a V2-sealed producer. No wildcard; every receipt
/// variant is then rechecked against the same run mode before it is
/// projected into evidence.
fn validate_run_transition_environment_diagonal_v1(
    run: &ValidatedTrainRunV2,
    transition_environment: NativeRunEnvironmentTrajectoryContractV1,
) -> Result<()> {
    match (
        run.environment_trajectory_contract_v1(),
        transition_environment,
    ) {
        (
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
        )
        | (
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
        ) => Ok(()),
        (
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
        )
        | (
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1,
        ) => Err(error_v1(UpdateGroupV1ErrorKind::RunBinding)),
    }
}

/// Pure, allocation-free receipt-diagonal preflight: every receipt variant in
/// the observation must sit on the run-mode diagonal. Both Store entry points
/// run this before predecessor export, transition consumption, or any
/// evidence work, so a mixed or off-diagonal receipt vector rejects with zero
/// side effects. Exhaustive on purpose.
fn preflight_receipt_environment_diagonal_v1(
    run: &ValidatedTrainRunV2,
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<()> {
    let expected_v2 = match run.environment_trajectory_contract_v1() {
        NativeRunEnvironmentTrajectoryContractV1::LegacyV1 => false,
        NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2 => true,
    };
    for episode in &observation.episodes {
        if episode
            .full_trajectory_receipt
            .is_environment_randomization_v2()
            != expected_v2
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
    }
    Ok(())
}

pub(crate) fn build_compact_update_group_v2(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    transition: NativeTrainingPreparedTransitionV2,
) -> Result<(
    ValidatedUpdateGroupAdvanceV1,
    NativeTrainingIntrinsicCheckpointFactsV2,
    Option<NativeTrainingCheckpointCandidateV1>,
)> {
    validate_context_run_v1(run, &context)?;
    validate_prepared_execution_config_v1(run, transition.execution_config_v2())?;
    validate_run_transition_environment_diagonal_v1(
        run,
        transition.environment_trajectory_contract_v1(),
    )?;
    preflight_receipt_environment_diagonal_v1(run, transition.observation_v2())?;
    let (predecessor, successor, observation, final_checkpoint) = transition.into_parts_v2();
    let predecessor_view = UpdateCheckpointFactsV1::from_intrinsic_v2(&predecessor);
    let successor_view = UpdateCheckpointFactsV1::from_intrinsic_v2(&successor);
    let advance = build_update_group_from_parts_v1(
        run,
        context,
        &predecessor_view,
        &observation,
        &successor_view,
    )?;
    Ok((advance, successor, final_checkpoint))
}

fn build_update_group_from_parts_v1(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    predecessor: &UpdateCheckpointFactsV1,
    observation: &NativeTrainingUpdateObservationV2,
    successor: &UpdateCheckpointFactsV1,
) -> Result<ValidatedUpdateGroupAdvanceV1> {
    validate_predecessor_checkpoint_v1(run, &context, predecessor)?;
    validate_observation_checkpoint_v1(run, &context, observation, successor)?;
    preflight_observation_cardinality_v1(observation)?;
    let evidence = evidence_from_observation_v1(run, &context, observation, successor)?;
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

pub(crate) fn validate_prepared_execution_config_v1(
    run: &ValidatedTrainRunV2,
    config: &NativeTrainingExecutionConfigV1,
) -> Result<()> {
    // Since C2 the environment randomization V2 trajectory contract is live,
    // so this validator carries no mode gate: it compares only the ordinary
    // config/run facts. Mode admission is the exhaustive run/executor and
    // run/transition/receipt diagonals proven at `prepare_segment_v2` and at
    // both Store update-group entry points.
    let record = run.record();
    let worker_count = u64::try_from(config.worker_count)
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::RunBinding))?;
    let sessions_per_worker = u64::try_from(config.sessions_per_worker)
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::RunBinding))?;
    let broker_batch_target = u64::try_from(config.broker_batch_target)
        .map_err(|_| error_v1(UpdateGroupV1ErrorKind::RunBinding))?;
    let logical_actor_count = worker_count
        .checked_mul(sessions_per_worker)
        .filter(|value| is_u63_v1(*value))
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
    let expected_value_coefficient =
        parse_f32_hex_v1(&record.optimization.value_coefficient_f32_bits)?.to_bits();
    let expected_learning_rate =
        parse_f32_hex_v1(&record.optimization.learning_rate_f32_bits)?.to_bits();
    if config.run_base_seed != record.schedule.base_seed
        || config.batch_episodes != run.batch_episodes()
        || config.deck_ids != record.environment.deck_ids
        || config.max_physical_decisions != record.limits.max_physical_decisions
        || config.max_policy_steps != record.limits.max_policy_steps
        || worker_count != record.topology.worker_count
        || sessions_per_worker != record.topology.sessions_per_worker
        || logical_actor_count != record.topology.logical_actor_count
        || broker_batch_target != record.topology.broker_batch_target
        || config.scheduler_timeout
            != std::time::Duration::from_millis(record.topology.scheduler_timeout_ms)
        || config.measure_broker_service_time != record.topology.measure_broker_service_time
        || config.value_coefficient_bits != expected_value_coefficient
        || config.learning_rate_bits != expected_learning_rate
        || Some(config.numerical_backend) != run.store_numerical_backend_v2()
        || config.backward_worker_limit != 1
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RunBinding));
    }
    Ok(())
}

fn validate_predecessor_checkpoint_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
    predecessor: &UpdateCheckpointFactsV1,
) -> Result<()> {
    let progress = predecessor.progress;
    let expected_policy = checked_u63_add_v1(
        context.progress.learner_policy_steps_by_seat().p0(),
        context.progress.learner_policy_steps_by_seat().p1(),
    )?;
    let expected_physical = checked_u63_add_v1(
        context.progress.learner_physical_decisions_by_seat().p0(),
        context.progress.learner_physical_decisions_by_seat().p1(),
    )?;
    if predecessor.base_seed != run.record().schedule.base_seed
        || predecessor.batch_episodes != run.batch_episodes()
        || Some(predecessor.numerical_backend) != run.store_numerical_backend_v2()
        || predecessor.backward_worker_limit != 1
        || predecessor.adam_step != context.next_update_index - 1
        || predecessor.scorer_bias_anchor_bits != context.scorer_bias_anchor_bits
        || predecessor.model_parameter_sha256 != context.model_parameter_sha256
        || predecessor.train_state_sha256 != context.train_state_sha256
        || progress.next_episode_index != context.progress.next_episode_index()
        || progress.successful_update_count != context.progress.successful_update_count()
        || progress.completed_episode_count != context.progress.completed_episode_count()
        || progress.learner_policy_step_count != expected_policy
        || progress.learner_physical_decision_count != expected_physical
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch));
    }
    Ok(())
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

pub(crate) fn validate_embedded_update_group_wire_v1(
    run: &ValidatedTrainRunV2,
    context: UpdateEvidenceChainContextV1,
    wire: UpdateGroupWireV1,
) -> Result<ValidatedUpdateGroupAdvanceV1> {
    validate_context_run_v1(run, &context)?;
    let canonical_bytes = to_canonical_json_bytes_v1(&wire, GROUP_NULL_POLICY_V1)?;
    if canonical_bytes.len() > CONSERVATIVE_STANDALONE_GROUP_CJ_CEILING_V1 {
        return Err(error_v1(UpdateGroupV1ErrorKind::RecordTooLarge));
    }
    validate_and_advance_wire_v1(run, context, wire, canonical_bytes)
}

pub(crate) fn validate_update_evidence_chain_context_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
) -> Result<()> {
    validate_context_run_v1(run, context)
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
    successor: &UpdateCheckpointFactsV1,
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
        || successor.base_seed != record.schedule.base_seed
        || successor.batch_episodes != run.batch_episodes()
        || Some(successor.numerical_backend) != run.store_numerical_backend_v2()
        || successor.backward_worker_limit != 1
        || successor.adam_step != context.next_update_index
        || successor.scorer_bias_anchor_bits != context.scorer_bias_anchor_bits
        || successor.progress.successful_update_count != context.next_update_index
        || successor.progress.next_episode_index != expected_end
        || successor.progress.completed_episode_count != expected_end
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
        || model_after != successor.model_parameter_sha256
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::CheckpointMismatch));
    }
    Ok(())
}

fn evidence_from_observation_v1(
    run: &ValidatedTrainRunV2,
    context: &UpdateEvidenceChainContextV1,
    observation: &NativeTrainingUpdateObservationV2,
    successor: &UpdateCheckpointFactsV1,
) -> Result<UpdateEvidenceWireV1> {
    #[cfg(test)]
    STORE_EVIDENCE_PROJECTION_COUNT_V2.with(|count| count.set(count.get() + 1));
    // Exhaustive on purpose: a future third mode variant must fail
    // compilation here rather than silently map to Legacy.
    let expected_v2_receipts = match run.environment_trajectory_contract_v1() {
        NativeRunEnvironmentTrajectoryContractV1::LegacyV1 => false,
        NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2 => true,
    };
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
        // Every receipt variant must sit on the run-mode diagonal before any
        // common accessor is projected into evidence.
        if receipt.is_environment_randomization_v2() != expected_v2_receipts {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        // V2-only facts are validated, not merely present: the pair index
        // must be the episode's own pair and the catalog-resolved physical
        // bindings must equal the ordered run deck IDs, while a legacy
        // receipt must project no V2-only fact at all. This runs before the
        // run deck IDs are projected into the wire episode, so corrupt
        // V2-only facts cannot be laundered into a valid 18-field
        // EpisodeWire.
        if expected_v2_receipts {
            if receipt.pair_index_v2() != Some(expected_episode_index / 2) {
                return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
            }
            match receipt.deck_ids_v2() {
                Some(receipt_deck_ids) => {
                    let run_deck_ids = &run.record().environment.deck_ids;
                    if receipt_deck_ids[0] != run_deck_ids[0]
                        || receipt_deck_ids[1] != run_deck_ids[1]
                    {
                        return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
                    }
                }
                None => return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding)),
            }
        } else if receipt.pair_index_v2().is_some() || receipt.deck_ids_v2().is_some() {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        if observed.episode_index != expected_episode_index
            || receipt.episode_index() != expected_episode_index
            || schedule.episode_index != expected_episode_index
            || schedule.learner_seat != observed.learner_seat
            || receipt.learner_seat() != observed.learner_seat
            || receipt.environment_seed() != schedule.environment_seed
            || receipt.deck_hashes() != run_deck_hashes
            || observed.learner_group_count != receipt.learner_physical_decision_count()
            || observed.learner_policy_step_count != receipt.learner_policy_step_count()
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        let (terminal_outcome, winner) = natural_outcome_wire_v1(observed.terminal_outcome)?;
        let expected_return = learner_return_v1(observed.learner_seat, terminal_outcome);
        if observed.learner_return != expected_return {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        validate_episode_count_lattice_v1(
            run,
            receipt.policy_step_count(),
            receipt.physical_decision_count(),
            receipt.learner_policy_step_count(),
            receipt.learner_physical_decision_count(),
            receipt.opponent_policy_step_count(),
            receipt.opponent_physical_decision_count(),
        )?;
        total_policy_steps = checked_u63_add_v1(total_policy_steps, receipt.policy_step_count())?;
        total_physical_decisions =
            checked_u63_add_v1(total_physical_decisions, receipt.physical_decision_count())?;
        learner_policy_steps =
            checked_u63_add_v1(learner_policy_steps, receipt.learner_policy_step_count())?;
        learner_physical_decisions = checked_u63_add_v1(
            learner_physical_decisions,
            receipt.learner_physical_decision_count(),
        )?;
        episodes.push(EpisodeWireV1 {
            schema: EPISODE_SCHEMA_V1.to_owned(),
            episode_index: expected_episode_index,
            environment_seed_u64_hex: format!("{:016x}", receipt.environment_seed()),
            deck_ids: run.record().environment.deck_ids.clone(),
            deck_hashes_u64_hex: run.record().environment.deck_hashes_u64_hex.clone(),
            learner_seat: seat_wire_v1(observed.learner_seat),
            learner_return: observed.learner_return,
            terminal_outcome,
            winner,
            terminal_classification: "natural".to_owned(),
            terminal_code: "natural-game-over".to_owned(),
            policy_step_count: receipt.policy_step_count(),
            physical_decision_count: receipt.physical_decision_count(),
            learner_policy_step_count: receipt.learner_policy_step_count(),
            opponent_policy_step_count: receipt.opponent_policy_step_count(),
            learner_physical_decision_count: receipt.learner_physical_decision_count(),
            opponent_physical_decision_count: receipt.opponent_physical_decision_count(),
            // Legacy projects its V1 trajectory digest; environment V2
            // projects the inner V1 digest through the same compatibility
            // accessor. The V2 outer digest never enters EpisodeWire.
            trajectory_sha256: lower_hex_raw32_v1(receipt.trajectory_sha256()),
            opponent_population_slot: observed.opponent_population_slot.map(u32::from),
            opponent_run_sha256: observed.opponent_run_sha256.map(lower_hex_raw32_v1),
            opponent_checkpoint_manifest_sha256: observed
                .opponent_checkpoint_manifest_sha256
                .map(lower_hex_raw32_v1),
            scoring_weight_version: observed.scoring_weight_version,
            consuming_update_version: observed.consuming_update_version,
        });
    }
    if total_policy_steps == 0
        || total_physical_decisions == 0
        || total_physical_decisions > total_policy_steps
        || total_policy_steps != observation.policy_step_count
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
    validate_candidate_progress_v1(successor, &progress_after)?;
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
        train_state_sha256_after: lower_hex_raw32_v1(successor.train_state_sha256),
        progress_after,
    })
}

fn validate_episode_count_lattice_v1(
    run: &ValidatedTrainRunV2,
    policy_step_count: u64,
    physical_decision_count: u64,
    learner_policy_step_count: u64,
    learner_physical_decision_count: u64,
    opponent_policy_step_count: u64,
    opponent_physical_decision_count: u64,
) -> Result<()> {
    let counts = [
        policy_step_count,
        physical_decision_count,
        learner_policy_step_count,
        learner_physical_decision_count,
        opponent_policy_step_count,
        opponent_physical_decision_count,
    ];
    let policy_parts = checked_u63_add_v1(learner_policy_step_count, opponent_policy_step_count)?;
    let physical_parts = checked_u63_add_v1(
        learner_physical_decision_count,
        opponent_physical_decision_count,
    )?;
    if counts.into_iter().any(|value| !is_u63_v1(value))
        || policy_step_count == 0
        || physical_decision_count == 0
        || policy_parts != policy_step_count
        || physical_parts != physical_decision_count
        || physical_decision_count > policy_step_count
        || learner_physical_decision_count > learner_policy_step_count
        || opponent_physical_decision_count > opponent_policy_step_count
        || policy_step_count > run.record().limits.max_policy_steps
        || physical_decision_count > run.record().limits.max_physical_decisions
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
    }
    Ok(())
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
        let mut episode_policy_count = 0_u64;
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
            let substep_count = u64::from(term.substep_count);
            episode_policy_count = checked_u63_add_v1(episode_policy_count, substep_count)?;
            policy_count = checked_u63_add_v1(policy_count, substep_count)?;
            group_index = group_index
                .checked_add(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        }
        if episode_policy_count != episode.learner_policy_step_count {
            return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
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
    successor: &UpdateCheckpointFactsV1,
    expected: &CheckpointProgressV3,
) -> Result<()> {
    let progress = successor.progress;
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
        || successor.adam_step != expected.successful_update_count()
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
        // The three opponent-identity fields are recorded together or not at
        // all: a population opponent always names a slot and its checkpoint's
        // full identity, and every other opponent path (ladder, plain
        // self-play, or a record predating this field) leaves all three
        // absent.
        match (
            episode.opponent_population_slot,
            &episode.opponent_run_sha256,
            &episode.opponent_checkpoint_manifest_sha256,
        ) {
            (None, None, None) => {}
            (Some(slot), Some(run_sha256), Some(checkpoint_manifest_sha256)) => {
                if slot >= POPULATION_OPPONENT_SLOT_COUNT_V1 as u32
                    || parse_digest_v1(run_sha256).is_err()
                    || parse_digest_v1(checkpoint_manifest_sha256).is_err()
                {
                    return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
                }
            }
            _ => return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding)),
        }
        // Bounded-staleness async provenance: present together or not at
        // all (a synchronous run, or any record predating this field,
        // leaves both absent). When present, the Store has no independent
        // notion of the run's declared staleness bound K (only the
        // scheduler does), so it cannot reject on boundedness -- but it can
        // and does reject the causality invariant no K ever admits: an
        // episode can never be scored by weights newer than the update
        // consuming it. This calls the scheduler's own
        // `check_staleness_bound_v1` directly (rather than restating the
        // `scoring_weight_version > consuming_update_version` comparison
        // here) with `max_staleness_updates: u32::MAX`, a bound wide enough
        // that it can never itself be the reason for rejection, so the only
        // way this call returns `Err` is the causality check inside it --
        // the two call sites are provably checking the identical rule, not
        // just similar-looking duplicated logic.
        match (
            episode.scoring_weight_version,
            episode.consuming_update_version,
        ) {
            (None, None) => {}
            (Some(scoring_weight_version), Some(consuming_update_version)) => {
                if !is_u63_v1(scoring_weight_version) || !is_u63_v1(consuming_update_version) {
                    return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
                }
                let entry = StalenessLedgerEntryV1 {
                    episode_id: expected_index,
                    scoring_weight_version,
                    consuming_update_version,
                };
                if check_staleness_bound_v1(entry, u32::MAX).is_err() {
                    return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
                }
            }
            _ => return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding)),
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
        {
            return Err(error_v1(UpdateGroupV1ErrorKind::EpisodeBinding));
        }
        validate_episode_count_lattice_v1(
            run,
            episode.policy_step_count,
            episode.physical_decision_count,
            episode.learner_policy_step_count,
            episode.learner_physical_decision_count,
            episode.opponent_policy_step_count,
            episode.opponent_physical_decision_count,
        )?;
        total_policy = checked_u63_add_v1(total_policy, episode.policy_step_count)?;
        total_physical = checked_u63_add_v1(total_physical, episode.physical_decision_count)?;
        learner_policy = checked_u63_add_v1(learner_policy, episode.learner_policy_step_count)?;
        learner_physical =
            checked_u63_add_v1(learner_physical, episode.learner_physical_decision_count)?;
    }
    if total_policy == 0
        || total_physical == 0
        || total_physical > total_policy
        || learner_policy != evidence.learner_policy_step_count
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
        let mut episode_policy_count = 0_u64;
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
            let substep_count = u64::from(term.substep_count);
            episode_policy_count = checked_u63_add_v1(episode_policy_count, substep_count)?;
            policy_count = checked_u63_add_v1(policy_count, substep_count)?;
            term_index = term_index
                .checked_add(1)
                .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::InvalidArithmetic))?;
        }
        if episode_policy_count != episode.learner_policy_step_count {
            return Err(error_v1(UpdateGroupV1ErrorKind::PhysicalLattice));
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
    if all_counts.into_iter().any(|value| !is_u63_v1(value))
        || counts.complete_round_count == 0
        || counts.scored_decision_count != evidence.learner_policy_step_count
        || counts.sampled_action_count != evidence.learner_policy_step_count
        || counts.batch_width_sum != evidence.learner_policy_step_count
        || counts.scored_action_logit_count != evidence.gauge.total_action_count
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
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
    }
    validate_batch_width_shape_v1(
        b,
        counts.full_target_batch_count,
        counts.short_batch_count,
        counts.batch_width_sum,
        counts.max_batch_width,
        counts.scorer_batch_count,
    )?;
    Ok(())
}

fn validate_batch_width_shape_v1(
    batch_target: u64,
    full_batch_count: u64,
    short_batch_count: u64,
    batch_width_sum: u64,
    max_batch_width: u64,
    scorer_batch_count: u64,
) -> Result<()> {
    if batch_target == 0
        || batch_width_sum == 0
        || max_batch_width == 0
        || max_batch_width > batch_target
    {
        return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
    }
    let expected_batch_count = checked_u63_add_v1(full_batch_count, short_batch_count)?;
    if scorer_batch_count != expected_batch_count {
        return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
    }
    let full_width = checked_u63_mul_v1(full_batch_count, batch_target)?;
    let short_width = batch_width_sum
        .checked_sub(full_width)
        .ok_or_else(|| error_v1(UpdateGroupV1ErrorKind::RolloutMismatch))?;
    if full_batch_count > 0 {
        if max_batch_width != batch_target {
            return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
        }
        if short_batch_count == 0 {
            if short_width != 0 {
                return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
            }
        } else {
            let maximum_short_width = checked_u63_mul_v1(short_batch_count, batch_target - 1)?;
            if short_width < short_batch_count || short_width > maximum_short_width {
                return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
            }
        }
    } else {
        if short_batch_count == 0 || max_batch_width >= batch_target {
            return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
        }
        let minimum_width = checked_u63_add_v1(max_batch_width, short_batch_count - 1)?;
        let maximum_width = checked_u63_mul_v1(short_batch_count, max_batch_width)?;
        if batch_width_sum < minimum_width || batch_width_sum > maximum_width {
            return Err(error_v1(UpdateGroupV1ErrorKind::RolloutMismatch));
        }
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
    use crate::native_policy_train_step_v1::{
        reset_train_state_snapshot_call_count_for_test_v1,
        train_state_snapshot_call_count_for_test_v1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_train_state_payload_v1::{
        payload_encode_counts_for_test_v1, reset_payload_encode_counts_for_test_v1,
    };
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
    };
    use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, decode_genesis_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_environment_randomization_v2, test_fixture_bytes_v2,
    };
    use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
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

    #[test]
    fn update_group_closed_maximum_matches_frozen_recurrence() {
        let one = maximum_update_group_json_shape_v2(1, 1, 1).unwrap();
        assert_eq!(one.token_bytes(), 3_820 + 754 + 125 + 216);
        assert_eq!(one.canonical_document_bytes_v1().unwrap(), 4_916);

        let current = maximum_update_group_json_shape_v2(2, 65_536, 131_072).unwrap();
        assert_eq!(current.token_bytes(), 36_509_204);
        assert_eq!(current.canonical_document_bytes_v1().unwrap(), 36_509_205);
    }

    fn execution_config_v1(run: &ValidatedTrainRunV2) -> NativeTrainingExecutionConfigV1 {
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
            scheduler_timeout: Duration::from_millis(run.record().topology.scheduler_timeout_ms),
            measure_broker_service_time: run.record().topology.measure_broker_service_time,
            value_coefficient_bits: parse_f32_hex_v1(
                &run.record().optimization.value_coefficient_f32_bits,
            )
            .unwrap()
            .to_bits(),
            learning_rate_bits: parse_f32_hex_v1(&run.record().optimization.learning_rate_f32_bits)
                .unwrap()
                .to_bits(),
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    fn fixture_v1() -> &'static FixtureV1 {
        FIXTURE_V1.get_or_init(|| {
            let run_bytes = test_fixture_bytes_v2();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
            let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
                execution_config_v1(&run),
                &snapshot_manifest,
                &snapshot_payload,
            )
            .unwrap();
            let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
            let genesis_payload = genesis_candidate.payload().to_vec();
            let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
            let genesis_manifest_bytes = genesis.canonical_bytes().to_vec();
            let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
            let (group_bytes, second_context) = {
                let prepared = executor.prepare_update_v2().unwrap();
                assert_eq!(
                    prepared.pre_update_checkpoint_candidate_v1().unwrap(),
                    genesis_candidate
                );
                assert_ne!(prepared.checkpoint_candidate(), &genesis_candidate);
                let mut mismatched_predecessor =
                    begin_update_evidence_chain_v1(&run, &genesis).unwrap();
                mismatched_predecessor.train_state_sha256 = [0_u8; 32];
                assert_eq!(
                    build_update_group_v1(&run, mismatched_predecessor, &prepared)
                        .unwrap_err()
                        .kind(),
                    UpdateGroupV1ErrorKind::CheckpointMismatch,
                    "the opaque prepared guard must re-attest the full live predecessor state"
                );
                let mut mismatched_model = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
                mismatched_model.model_parameter_sha256 = [0_u8; 32];
                assert_eq!(
                    build_update_group_v1(&run, mismatched_model, &prepared)
                        .unwrap_err()
                        .kind(),
                    UpdateGroupV1ErrorKind::CheckpointMismatch
                );
                let built = build_update_group_v1(&run, context, &prepared).unwrap();
                let (group, advanced_context) = built.into_parts();
                (group.canonical_bytes().to_vec(), advanced_context)
            };
            executor.run_update_v2().unwrap();
            let second_group_bytes = {
                let prepared = executor.prepare_update_v2().unwrap();
                build_update_group_v1(&run, second_context, &prepared)
                    .unwrap()
                    .group()
                    .canonical_bytes()
                    .to_vec()
            };
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

    #[test]
    fn sealed_genesis_boundary_reconstructs_the_exact_evidence_context() {
        let fixture = fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let genesis = decode_genesis_checkpoint_manifest_v3(
            &fixture.genesis_manifest_bytes,
            &fixture.genesis_payload,
            &run,
        )
        .unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &genesis).unwrap();
        let boundary = build_genesis_native_training_boundary_v2(&run, &segment, &genesis).unwrap();
        let expected = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let reconstructed = resume_update_evidence_chain_v1(&run, &boundary, &genesis).unwrap();

        assert_eq!(
            reconstructed.next_update_index(),
            expected.next_update_index()
        );
        assert_eq!(reconstructed.progress(), expected.progress());
        assert_eq!(
            reconstructed.previous_update_evidence_sha256(),
            expected.previous_update_evidence_sha256()
        );
        assert_eq!(
            reconstructed.model_parameter_sha256(),
            expected.model_parameter_sha256()
        );
        assert_eq!(
            reconstructed.train_state_sha256(),
            expected.train_state_sha256()
        );
        assert_eq!(
            reconstructed.run_sha256_raw_v1(),
            expected.run_sha256_raw_v1()
        );
        assert_eq!(
            reconstructed.identity_bundle_sha256_raw_v1(),
            expected.identity_bundle_sha256_raw_v1()
        );
        assert_eq!(
            reconstructed.batch_episodes_v1(),
            expected.batch_episodes_v1()
        );
        assert_eq!(
            reconstructed.checkpoint_segment_updates_v1(),
            expected.checkpoint_segment_updates_v1()
        );
        assert_eq!(
            reconstructed.scorer_bias_anchor_bits_v1(),
            expected.scorer_bias_anchor_bits_v1()
        );
    }

    #[test]
    fn compact_and_full_prepared_authorities_emit_identical_update_groups() {
        let fixture = fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let genesis = decode_genesis_checkpoint_manifest_v3(
            &fixture.genesis_manifest_bytes,
            &fixture.genesis_payload,
            &run,
        )
        .unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut full_executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let mut compact_executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();

        let full_context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let full_prepared = full_executor.prepare_update_v2().unwrap();
        let full = build_update_group_v1(&run, full_context, &full_prepared).unwrap();
        let full_checkpoint = full_prepared.checkpoint_candidate().clone();

        let compact_context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let predecessor = compact_executor.intrinsic_checkpoint_facts_v2().unwrap();
        let mut candidate = compact_executor.begin_segment_candidate_v2().unwrap();
        let transition = candidate.prepare_transition_v2(predecessor, true).unwrap();
        let (compact, successor, compact_checkpoint) =
            build_compact_update_group_v2(&run, compact_context, transition).unwrap();
        let compact_checkpoint = compact_checkpoint.unwrap();
        assert_eq!(compact_checkpoint, full_checkpoint);
        assert_eq!(
            compact_checkpoint.digests().model_parameter_sha256,
            successor.model_parameter_sha256_v2()
        );
        assert_eq!(
            compact_checkpoint.digests().native_state_sha256,
            successor.train_state_sha256_v2()
        );

        assert_eq!(
            compact.group().canonical_bytes(),
            full.group().canonical_bytes()
        );
        assert_eq!(
            compact.group().update_evidence_sha256(),
            full.group().update_evidence_sha256()
        );
        let compact_context = compact.advanced_context();
        let full_context = full.advanced_context();
        assert_eq!(
            compact_context.next_update_index(),
            full_context.next_update_index()
        );
        assert_eq!(compact_context.progress(), full_context.progress());
        assert_eq!(
            compact_context.previous_update_evidence_sha256(),
            full_context.previous_update_evidence_sha256()
        );
        assert_eq!(
            compact_context.model_parameter_sha256(),
            full_context.model_parameter_sha256()
        );
        assert_eq!(
            compact_context.train_state_sha256(),
            full_context.train_state_sha256()
        );
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
    fn prepared_authority_binds_every_execution_config_field() {
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let expected = execution_config_v1(&run);
        validate_prepared_execution_config_v1(&run, &expected).unwrap();

        let mut mismatches = Vec::new();
        let mut changed = expected.clone();
        changed.run_base_seed ^= 1;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.batch_episodes += 2;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.deck_ids[0].push_str("-wrong");
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.max_physical_decisions -= 1;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.max_policy_steps -= 1;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.worker_count += 1;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.sessions_per_worker += 1;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.broker_batch_target += 1;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.scheduler_timeout += Duration::from_nanos(1);
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.measure_broker_service_time = !changed.measure_broker_service_time;
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.value_coefficient_bits = 0.25_f32.to_bits();
        mismatches.push(changed);
        let mut changed = expected.clone();
        changed.learning_rate_bits = 0.002_f32.to_bits();
        mismatches.push(changed);

        for changed in mismatches {
            assert_eq!(
                validate_prepared_execution_config_v1(&run, &changed)
                    .unwrap_err()
                    .kind(),
                UpdateGroupV1ErrorKind::RunBinding
            );
        }

        let fixture = fixture_v1();
        let genesis = decode_genesis_checkpoint_manifest_v3(
            &fixture.genesis_manifest_bytes,
            &fixture.genesis_payload,
            &run,
        )
        .unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut wrong_config = expected;
        wrong_config.learning_rate_bits = 0.002_f32.to_bits();
        let mut wrong_executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            wrong_config,
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let prepared = wrong_executor.prepare_update_v2().unwrap();
        assert_eq!(
            build_update_group_v1(
                &run,
                begin_update_evidence_chain_v1(&run, &genesis).unwrap(),
                &prepared,
            )
            .unwrap_err()
            .kind(),
            UpdateGroupV1ErrorKind::RunBinding,
            "loss evidence alone cannot authorize an update made with the wrong learning rate"
        );
        drop(prepared);

        let predecessor = wrong_executor.intrinsic_checkpoint_facts_v2().unwrap();
        let mut candidate = wrong_executor.begin_segment_candidate_v2().unwrap();
        let transition = candidate.prepare_transition_v2(predecessor, true).unwrap();
        assert_eq!(
            build_compact_update_group_v2(
                &run,
                begin_update_evidence_chain_v1(&run, &genesis).unwrap(),
                transition,
            )
            .unwrap_err()
            .kind(),
            UpdateGroupV1ErrorKind::RunBinding,
            "compact evidence must use the sealed config that produced its transition"
        );
    }

    #[test]
    fn batch_width_maximum_is_exactly_feasible() {
        let pass = |full, short, width, maximum| {
            validate_batch_width_shape_v1(16, full, short, width, maximum, full + short).unwrap();
        };
        let reject = |full, short, width, maximum| {
            assert_eq!(
                validate_batch_width_shape_v1(16, full, short, width, maximum, full + short,)
                    .unwrap_err()
                    .kind(),
                UpdateGroupV1ErrorKind::RolloutMismatch
            );
        };

        pass(0, 3, 6, 4);
        pass(0, 3, 12, 4);
        pass(1, 0, 16, 16);
        pass(1, 3, 19, 16);
        pass(1, 3, 61, 16);
        reject(0, 3, 3, 2);
        reject(0, 3, 4, 1);
        reject(1, 0, 16, 15);
        reject(1, 3, 18, 16);
        reject(1, 3, 62, 16);
        assert!(validate_batch_width_shape_v1(1, 1, 0, 1, 1, 1).is_ok());
        assert_eq!(
            validate_batch_width_shape_v1(1, 0, 1, 1, 1, 1)
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::RolloutMismatch
        );
        assert_eq!(
            validate_batch_width_shape_v1(
                U63_MAX_V1, U63_MAX_V1, 0, U63_MAX_V1, U63_MAX_V1, U63_MAX_V1,
            )
            .unwrap_err()
            .kind(),
            UpdateGroupV1ErrorKind::InvalidArithmetic
        );
    }

    #[test]
    fn episode_count_lattice_and_per_episode_policy_partition_fail_closed() {
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let limits = &run.record().limits;
        validate_episode_count_lattice_v1(&run, 3, 2, 0, 0, 3, 2).unwrap();

        let rejected = [
            (0, 0, 0, 0, 0, 0),
            (1, 0, 1, 0, 0, 0),
            (1, 2, 1, 2, 0, 0),
            (2, 2, 1, 2, 1, 0),
            (2, 2, 1, 0, 1, 2),
            (
                limits.max_policy_steps + 1,
                1,
                0,
                0,
                limits.max_policy_steps + 1,
                1,
            ),
            (
                limits.max_physical_decisions + 1,
                limits.max_physical_decisions + 1,
                0,
                0,
                limits.max_physical_decisions + 1,
                limits.max_physical_decisions + 1,
            ),
        ];
        for (
            policy,
            physical,
            learner_policy,
            learner_physical,
            opponent_policy,
            opponent_physical,
        ) in rejected
        {
            assert_eq!(
                validate_episode_count_lattice_v1(
                    &run,
                    policy,
                    physical,
                    learner_policy,
                    learner_physical,
                    opponent_policy,
                    opponent_physical,
                )
                .unwrap_err()
                .kind(),
                UpdateGroupV1ErrorKind::EpisodeBinding
            );
        }

        let mut empty_episode = group_value_v1();
        for field in [
            "policy_step_count",
            "physical_decision_count",
            "learner_policy_step_count",
            "learner_physical_decision_count",
            "opponent_policy_step_count",
            "opponent_physical_decision_count",
        ] {
            empty_episode["evidence"]["episodes"][0][field] = Value::from(0_u64);
        }
        assert_eq!(
            decode_value_error_v1(&empty_episode),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );

        let mut actor_violation = group_value_v1();
        let learner_policy = actor_violation["evidence"]["episodes"][0]
            ["learner_policy_step_count"]
            .as_u64()
            .unwrap();
        let learner_physical = actor_violation["evidence"]["episodes"][0]
            ["learner_physical_decision_count"]
            .as_u64()
            .unwrap();
        actor_violation["evidence"]["episodes"][0]["opponent_policy_step_count"] =
            Value::from(0_u64);
        actor_violation["evidence"]["episodes"][0]["opponent_physical_decision_count"] =
            Value::from(1_u64);
        actor_violation["evidence"]["episodes"][0]["policy_step_count"] =
            Value::from(learner_policy);
        actor_violation["evidence"]["episodes"][0]["physical_decision_count"] =
            Value::from(learner_physical + 1);
        assert_eq!(
            decode_value_error_v1(&actor_violation),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );

        let mut limit_violation = group_value_v1();
        let learner_policy = limit_violation["evidence"]["episodes"][0]
            ["learner_policy_step_count"]
            .as_u64()
            .unwrap();
        limit_violation["evidence"]["episodes"][0]["opponent_policy_step_count"] =
            Value::from(limits.max_policy_steps);
        limit_violation["evidence"]["episodes"][0]["policy_step_count"] =
            Value::from(limits.max_policy_steps + learner_policy);
        assert_eq!(
            decode_value_error_v1(&limit_violation),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );

        let mut wire: UpdateGroupWireV1 = serde_json::from_value(group_value_v1()).unwrap();
        let first_episode_groups =
            usize::try_from(wire.evidence.episodes[0].learner_physical_decision_count).unwrap();
        let second_episode_groups =
            usize::try_from(wire.evidence.episodes[1].learner_physical_decision_count).unwrap();
        assert!(first_episode_groups > 0 && second_episode_groups > 0);
        let first_range = 0..first_episode_groups;
        let second_range = first_episode_groups..first_episode_groups + second_episode_groups;
        let transfer = first_range
            .clone()
            .find(|index| wire.evidence.physical_terms[*index].substep_count > 1)
            .map(|donor| (donor, second_range.start))
            .or_else(|| {
                second_range
                    .clone()
                    .find(|index| wire.evidence.physical_terms[*index].substep_count > 1)
                    .map(|donor| (donor, first_range.start))
            })
            .expect("the real K=2 fixture must exercise a multi-substep physical decision");
        let original_global_policy = wire.evidence.learner_policy_step_count;
        wire.evidence.physical_terms[transfer.0].substep_count -= 1;
        wire.evidence.physical_terms[transfer.1].substep_count += 1;
        assert_eq!(
            wire.evidence
                .physical_terms
                .iter()
                .map(|term| u64::from(term.substep_count))
                .sum::<u64>(),
            original_global_policy,
            "the corruption preserves the old update-wide P check"
        );
        assert_eq!(
            validate_physical_and_loss_v1(&run, &wire.evidence)
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::PhysicalLattice
        );
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

    // ---------------------------------------------------------------------
    // Live C2 environment-randomization-V2 mode-diagonal and frozen-byte
    // suites.
    //
    // These live in this module, not a sibling one, so they can reach the
    // private `execution_config_v1` helper above without widening any
    // visibility.
    // ---------------------------------------------------------------------

    use crate::native_checkpoint_runner_v1::NativeCheckpointRunnerConfigV1;
    use crate::native_science_loop_v1::{run_native_science_loop_v1, NativeScienceLoopV1ErrorKind};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(windows)]
    use crate::native_training_store_bootstrap_v2::{
        bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapOutcomeV2,
    };
    #[cfg(windows)]
    use crate::native_training_store_layout_v2::NativeTrainingStoreDirectoryV2;
    #[cfg(windows)]
    use crate::native_training_store_reference_latest_v2::{
        build_checkpoint_reference_v2, build_latest_v2,
    };
    #[cfg(windows)]
    use crate::native_training_store_resume_v2::{
        resume_native_training_store_v2, validate_native_training_store_v2,
    };
    #[cfg(windows)]
    use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
    #[cfg(windows)]
    use crate::native_training_store_v2::publish_genesis_generation_v2;

    /// Unique temporary parent, one per test, following the pattern the
    /// science-loop and resume suites already use. `std` only.
    struct StoreSuiteParentV1 {
        parent: PathBuf,
    }

    impl StoreSuiteParentV1 {
        fn new(label: &str) -> Self {
            static ORDINAL: AtomicU64 = AtomicU64::new(0);
            let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
            let parent = std::env::temp_dir().join(format!(
                "mtg-kernel-update-group-c1-{}-{label}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&parent).expect("create test parent");
            Self { parent }
        }

        fn path(&self) -> &Path {
            &self.parent
        }
    }

    impl Drop for StoreSuiteParentV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.parent);
        }
    }

    fn coherent_v2_run_v1() -> ValidatedTrainRunV2 {
        decode_train_run_v2(&test_fixture_bytes_environment_randomization_v2())
            .expect("the coherent V2 fixture decodes")
    }

    fn legacy_run_v1() -> ValidatedTrainRunV2 {
        decode_train_run_v2(&test_fixture_bytes_v2()).expect("the legacy fixture decodes")
    }

    /// The V2 fixture really does decode and classify as V2, so every
    /// diagonal admission and rejection below binds to a genuinely
    /// V2-classified run rather than an undecodable record.
    #[test]
    fn v2_fixture_decodes_and_classifies_as_v2() {
        assert_eq!(
            coherent_v2_run_v1().environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
    }

    /// The legacy fixture is unaffected by the new classification.
    #[test]
    fn legacy_fixture_still_classifies_as_legacy() {
        assert_eq!(
            legacy_run_v1().environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::LegacyV1
        );
    }

    /// Live C2: the shared prepared-execution validator carries no mode gate.
    /// Both classifications admit the same matching config, and ordinary
    /// config drift still rejects under both run modes.
    #[test]
    fn prepared_execution_config_admits_both_modes_and_still_binds_config() {
        let legacy = legacy_run_v1();
        let v2 = coherent_v2_run_v1();
        let config = execution_config_v1(&v2);

        validate_prepared_execution_config_v1(&legacy, &config)
            .expect("the legacy run admits the matching config");
        validate_prepared_execution_config_v1(&v2, &config)
            .expect("the V2 run admits the same matching config since C2");

        let mut drifted = execution_config_v1(&v2);
        drifted.run_base_seed ^= 1;
        for run in [&legacy, &v2] {
            assert_eq!(
                validate_prepared_execution_config_v1(run, &drifted)
                    .unwrap_err()
                    .kind(),
                UpdateGroupV1ErrorKind::RunBinding,
                "ordinary config binding must keep rejecting under both modes"
            );
        }
    }

    /// Live C2: the science loop's input validation no longer rejects a V2
    /// run. With deliberately missing snapshot paths the loop must fail
    /// somewhere past input validation, with a kind other than InputInvalid,
    /// which is exactly what proves the inactive gate is gone.
    #[test]
    fn v2_science_loop_proceeds_past_input_validation() {
        let parent = StoreSuiteParentV1::new("science-live");
        let v2 = coherent_v2_run_v1();
        let config = execution_config_v1(&v2);
        validate_prepared_execution_config_v1(&v2, &config)
            .expect("the V2 run admits its own config since C2");

        let missing_manifest = parent.path().join("missing-snapshot-manifest.json");
        let missing_payload = parent.path().join("missing-snapshot-payload.bin");
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 2,
            scheduler_timeout: Duration::from_secs(300),
            measure_broker_service_time: false,
            starting_player: None,
        };

        let error = run_native_science_loop_v1(
            parent.path(),
            "store",
            &v2,
            config,
            &missing_manifest,
            &missing_payload,
            runner_config,
            None,
            None,
        )
        .expect_err("missing snapshot paths cannot produce a complete run");
        assert_ne!(
            error.kind(),
            NativeScienceLoopV1ErrorKind::InputInvalid,
            "a V2 run must pass input validation and fail later"
        );
    }

    /// Frozen since C1 and preserved by live C2: `EpisodeWireV1`'s maximum
    /// shape and canonical output are unchanged, and its serialized value
    /// carries no environment randomization V2 section, no outer digest, and
    /// no receipt field.
    ///
    /// The first assertion is the real non-expansion proof. A maximum-width
    /// episode serialized standalone is one token plus one final LF; the
    /// planner's per-episode allowance is one token plus one joining comma. LF
    /// and comma are each one byte, so the two counts must be equal. Adding any
    /// field to `EpisodeWireV1` without also widening the planner breaks that
    /// equality, and widening the planner breaks the frozen totals pinned by
    /// `update_group_closed_maximum_matches_frozen_recurrence`.
    #[test]
    fn episode_wire_v1_maximum_shape_and_canonical_output_are_unchanged() {
        let maximum_episode = EpisodeWireV1 {
            schema: EPISODE_SCHEMA_V1.to_owned(),
            episode_index: U63_MAX_V1,
            environment_seed_u64_hex: "f".repeat(16),
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            deck_hashes_u64_hex: ["f".repeat(16), "f".repeat(16)],
            learner_seat: SeatWireV1::P0,
            learner_return: -1,
            terminal_outcome: OutcomeWireV1::P0Win,
            winner: Some(SeatWireV1::P0),
            terminal_classification: "natural".to_owned(),
            terminal_code: "natural-game-over".to_owned(),
            policy_step_count: U63_MAX_V1,
            physical_decision_count: U63_MAX_V1,
            learner_policy_step_count: U63_MAX_V1,
            opponent_policy_step_count: U63_MAX_V1,
            learner_physical_decision_count: U63_MAX_V1,
            opponent_physical_decision_count: U63_MAX_V1,
            trajectory_sha256: "f".repeat(64),
            opponent_population_slot: Some(u32::MAX),
            opponent_run_sha256: Some("f".repeat(64)),
            opponent_checkpoint_manifest_sha256: Some("f".repeat(64)),
            scoring_weight_version: Some(U63_MAX_V1),
            consuming_update_version: Some(U63_MAX_V1),
        };
        let bytes = to_canonical_json_bytes_v1(&maximum_episode, GROUP_NULL_POLICY_V1)
            .expect("the maximum episode is canonically serializable");

        // The planner's per-episode allowance is the difference between a
        // one-episode and a two-episode group: one episode token plus the one
        // comma that joins them. A canonical *document* is one token plus one
        // final LF. Comma and LF are both one byte, so the standalone
        // canonical byte count equals the allowance exactly.
        let one = maximum_update_group_json_shape_v2(1, 1, 1).unwrap();
        let two = maximum_update_group_json_shape_v2(2, 1, 1).unwrap();
        let per_episode_allowance = two.token_bytes() - one.token_bytes();
        assert_eq!(
            u64::try_from(bytes.len()).unwrap(),
            per_episode_allowance,
            "the maximum episode must exactly consume the closed per-episode allowance"
        );

        // Pin the token/comma decomposition against real bytes rather than
        // assuming it: wrapping the same maximum episode once and twice must
        // differ by exactly the allowance. The wrapper uses the production
        // episode null policy, whose only permitted null is
        // `episodes[*].winner`.
        let wrapped_once = to_canonical_json_bytes_v1(
            &serde_json::json!({ "episodes": [&maximum_episode] }),
            episode_null_policy_v1(),
        )
        .unwrap();
        let wrapped_twice = to_canonical_json_bytes_v1(
            &serde_json::json!({ "episodes": [&maximum_episode, &maximum_episode] }),
            episode_null_policy_v1(),
        )
        .unwrap();
        assert_eq!(
            u64::try_from(wrapped_twice.len() - wrapped_once.len()).unwrap(),
            per_episode_allowance,
            "one more episode must cost exactly one token plus one comma"
        );

        // These frozen totals are pinned by
        // `update_group_closed_maximum_matches_frozen_recurrence`; restate them
        // here so a planner widening cannot satisfy the allowance equality
        // above by moving both sides at once.
        assert_eq!(one.canonical_document_bytes_v1().unwrap(), 4_916);
        assert_eq!(
            maximum_update_group_json_shape_v2(2, 65_536, 131_072)
                .unwrap()
                .canonical_document_bytes_v1()
                .unwrap(),
            36_509_205
        );

        const BASE_EPISODE_KEYS_V1: [&str; 18] = [
            "deck_hashes_u64_hex",
            "deck_ids",
            "environment_seed_u64_hex",
            "episode_index",
            "learner_physical_decision_count",
            "learner_policy_step_count",
            "learner_return",
            "learner_seat",
            "opponent_physical_decision_count",
            "opponent_policy_step_count",
            "physical_decision_count",
            "policy_step_count",
            "schema",
            "terminal_classification",
            "terminal_code",
            "terminal_outcome",
            "trajectory_sha256",
            "winner",
        ];

        /// The three opponent-identity fields are omitted from the wire
        /// (never written as `null`) whenever they are `None`, so unlike the
        /// eighteen base fields the expected key set is per-episode: a
        /// population-opponent episode carries twenty-one keys, and every
        /// other episode (ladder opponent, plain self-play, or a record
        /// predating this field) carries the original eighteen.
        fn expected_episode_keys_v1(episode: &EpisodeWireV1) -> Vec<&'static str> {
            let mut keys = BASE_EPISODE_KEYS_V1.to_vec();
            if episode.opponent_population_slot.is_some() {
                keys.push("opponent_population_slot");
            }
            if episode.opponent_run_sha256.is_some() {
                keys.push("opponent_run_sha256");
            }
            if episode.opponent_checkpoint_manifest_sha256.is_some() {
                keys.push("opponent_checkpoint_manifest_sha256");
            }
            if episode.scoring_weight_version.is_some() {
                keys.push("scoring_weight_version");
            }
            if episode.consuming_update_version.is_some() {
                keys.push("consuming_update_version");
            }
            keys.sort_unstable();
            keys
        }
        const FORBIDDEN_EPISODE_SUBSTRINGS_V1: [&str; 6] = [
            "environment_randomization_v2",
            "trajectory_sha256_v2",
            "outer_trajectory_sha256",
            "outer_digest",
            "receipt",
            "_v2",
        ];

        /// The independent oracle: all eighteen wire fields written out by
        /// hand, including every `rename_all = "snake_case"` enum spelling and
        /// the `Option<SeatWireV1>` null. Built from the typed episode without
        /// going through `Serialize`, so comparing it to
        /// `serde_json::to_value` catches any renamed, added, or dropped field.
        fn manual_episode_value_v1(episode: &EpisodeWireV1) -> Value {
            let seat_name_v1 = |seat: SeatWireV1| match seat {
                SeatWireV1::P0 => "p0",
                SeatWireV1::P1 => "p1",
            };
            let mut value = serde_json::json!({
                "deck_hashes_u64_hex": [
                    episode.deck_hashes_u64_hex[0].clone(),
                    episode.deck_hashes_u64_hex[1].clone(),
                ],
                "deck_ids": [episode.deck_ids[0].clone(), episode.deck_ids[1].clone()],
                "environment_seed_u64_hex": episode.environment_seed_u64_hex.clone(),
                "episode_index": episode.episode_index,
                "learner_physical_decision_count": episode.learner_physical_decision_count,
                "learner_policy_step_count": episode.learner_policy_step_count,
                "learner_return": episode.learner_return,
                "learner_seat": seat_name_v1(episode.learner_seat),
                "opponent_physical_decision_count": episode.opponent_physical_decision_count,
                "opponent_policy_step_count": episode.opponent_policy_step_count,
                "physical_decision_count": episode.physical_decision_count,
                "policy_step_count": episode.policy_step_count,
                "schema": episode.schema.clone(),
                "terminal_classification": episode.terminal_classification.clone(),
                "terminal_code": episode.terminal_code.clone(),
                "terminal_outcome": match episode.terminal_outcome {
                    OutcomeWireV1::P0Win => "p0_win",
                    OutcomeWireV1::P1Win => "p1_win",
                    OutcomeWireV1::Draw => "draw",
                },
                "trajectory_sha256": episode.trajectory_sha256.clone(),
                "winner": match episode.winner {
                    Some(seat) => Value::from(seat_name_v1(seat)),
                    None => Value::Null,
                },
            });
            // Omitted (not written as `null`) whenever `None`, matching the
            // wire's `skip_serializing_if`: this keeps a pre-existing record
            // that never wrote these keys round-tripping byte for byte.
            let object = value.as_object_mut().expect("the oracle is a JSON object");
            if let Some(slot) = episode.opponent_population_slot {
                object.insert("opponent_population_slot".to_owned(), Value::from(slot));
            }
            if let Some(run_sha256) = &episode.opponent_run_sha256 {
                object.insert(
                    "opponent_run_sha256".to_owned(),
                    Value::from(run_sha256.clone()),
                );
            }
            if let Some(checkpoint_manifest_sha256) = &episode.opponent_checkpoint_manifest_sha256 {
                object.insert(
                    "opponent_checkpoint_manifest_sha256".to_owned(),
                    Value::from(checkpoint_manifest_sha256.clone()),
                );
            }
            if let Some(scoring_weight_version) = episode.scoring_weight_version {
                object.insert(
                    "scoring_weight_version".to_owned(),
                    Value::from(scoring_weight_version),
                );
            }
            if let Some(consuming_update_version) = episode.consuming_update_version {
                object.insert(
                    "consuming_update_version".to_owned(),
                    Value::from(consuming_update_version),
                );
            }
            value
        }

        fn assert_episode_oracle_v1(episode: &EpisodeWireV1, label: &str) {
            let manual = manual_episode_value_v1(episode);
            assert_eq!(
                serde_json::to_value(episode).unwrap(),
                manual,
                "{label}: EpisodeWireV1's serialized form must equal the manual oracle"
            );

            let object = manual.as_object().expect("the oracle is a JSON object");
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                expected_episode_keys_v1(episode),
                "{label}: EpisodeWireV1 must not gain or lose a field"
            );

            assert_eq!(episode.trajectory_sha256.len(), 64, "{label}: digest width");
            assert!(
                episode
                    .trajectory_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "{label}: digest must be lowercase hex"
            );

            let rendered = manual.to_string();
            for forbidden in FORBIDDEN_EPISODE_SUBSTRINGS_V1 {
                assert!(
                    !object.keys().any(|key| key.contains(forbidden)),
                    "{label}: EpisodeWireV1 must not carry a {forbidden} key"
                );
                assert!(
                    !rendered.contains(forbidden),
                    "{label}: EpisodeWireV1 must not carry {forbidden}"
                );
            }
        }

        assert_episode_oracle_v1(&maximum_episode, "maximum episode");
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            manual_episode_value_v1(&maximum_episode),
            "the canonical bytes must decode back to the oracle"
        );

        // The real emitted group, decoded through the production decoder so the
        // episodes are typed `EpisodeWireV1` values rather than loose JSON.
        // `UpdateGroupWireV1` nests them under `evidence`.
        let (fixture_run, fixture_context) = run_and_context_v1();
        let decoded =
            decode_update_group_v1(&fixture_run, fixture_context, &fixture_v1().group_bytes)
                .expect("the fixture group decodes");
        let fixture_episodes = &decoded.group().wire.evidence.episodes;
        assert!(
            !fixture_episodes.is_empty(),
            "the fixture group must emit real episodes"
        );
        for (index, episode) in fixture_episodes.iter().enumerate() {
            assert_episode_oracle_v1(episode, &format!("fixture episode {index}"));
        }

        // Canonical wrapped-episodes equality: the typed episodes the decoder
        // produced and the independently built manual values must serialize to
        // byte-identical canonical documents under the production episode null
        // policy. This covers ordering, the winner null, and every spelling at
        // once, not just the key set.
        let manual_values: Vec<Value> = fixture_episodes
            .iter()
            .map(manual_episode_value_v1)
            .collect();
        assert_eq!(
            to_canonical_json_bytes_v1(
                &serde_json::json!({ "episodes": fixture_episodes }),
                episode_null_policy_v1()
            )
            .unwrap(),
            to_canonical_json_bytes_v1(
                &serde_json::json!({ "episodes": manual_values }),
                episode_null_policy_v1()
            )
            .unwrap(),
            "the canonical wrapped episodes must match the manual oracle byte for byte"
        );

        assert_eq!(maximum_episode.schema, "mtg_kernel_native_train_episode/v1");
    }

    /// A training record written with a population opponent installed
    /// carries a correct, independently reproducible opponent identity, and
    /// the whole group round-trips through canonical bytes unchanged.
    #[test]
    fn population_opponent_identity_round_trips_through_the_written_record() {
        use crate::native_population_opponent_v1::{
            checkpoint_inference_handles_for_test_v1, population_slot_for_episode_v1,
            PopulationOpponentEngineV1, PopulationWeightVectorV1,
        };
        use std::sync::Arc;

        let run_bytes = test_fixture_bytes_v2();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();

        let weights = PopulationWeightVectorV1::new_v1([1, 1, 1, 1, 1, 1, 1, 1], 8).unwrap();
        let handles = checkpoint_inference_handles_for_test_v1::<8>();
        let population = Arc::new(PopulationOpponentEngineV1::new_v1(weights, handles));
        executor.set_population_opponent_v1(Some(population.clone()));

        let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
        let genesis_payload = genesis_candidate.payload().to_vec();
        let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
        let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let prepared = executor.prepare_update_v2().unwrap();
        let (group, _advanced_context) = build_update_group_v1(&run, context, &prepared)
            .unwrap()
            .into_parts();
        let group_bytes = group.canonical_bytes().to_vec();

        let decode_context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let decoded = decode_update_group_v1(&run, decode_context, &group_bytes)
            .expect("the population-opponent group decodes");
        let episodes = &decoded.group().wire.evidence.episodes;
        assert!(
            !episodes.is_empty(),
            "the population-opponent update must emit real episodes"
        );
        for episode in episodes {
            let slot = episode
                .opponent_population_slot
                .expect("a population opponent must record its slot");
            let run_sha256 = episode
                .opponent_run_sha256
                .as_deref()
                .expect("a population opponent must record its run sha256");
            let checkpoint_manifest_sha256 = episode
                .opponent_checkpoint_manifest_sha256
                .as_deref()
                .expect("a population opponent must record its checkpoint manifest sha256");

            let expected_slot = population_slot_for_episode_v1(
                run.record().schedule.base_seed,
                episode.episode_index,
                &weights,
            )
            .unwrap();
            assert_eq!(
                slot,
                u32::from(expected_slot.index_v1() as u8),
                "the recorded slot must match the deterministic selection the rollout used"
            );
            let (expected_run_sha256, expected_checkpoint_manifest_sha256) =
                population.checkpoint_identity_for_slot_v1(expected_slot);
            assert_eq!(run_sha256, lower_hex_raw32_v1(expected_run_sha256));
            assert_eq!(
                checkpoint_manifest_sha256,
                lower_hex_raw32_v1(expected_checkpoint_manifest_sha256)
            );
        }

        // The whole group, opponent identity included, round-trips through
        // canonical bytes exactly: re-encoding the decoded wire must
        // reproduce the bytes that were written.
        let reencoded = to_canonical_json_bytes_v1(&decoded.group().wire, GROUP_NULL_POLICY_V1)
            .expect("the decoded group re-serializes");
        assert_eq!(
            reencoded, group_bytes,
            "a population-opponent record must round-trip byte for byte"
        );
    }

    /// Exercises the bounded-staleness async provenance fields the way a
    /// future executor-integrated caller will: after `prepare_update_v2`
    /// returns and before the guard is handed to `build_update_group_v1`,
    /// using the crate-private, scope-narrowed `stamp_episode_provenance_v1`
    /// setter added for this integration (today's actual production
    /// integration, `bounded_staleness_async_production_v1::stamp_episode_
    /// provenance_v1`, stamps a freestanding `NativeTrainerUpdateEvidenceV2`
    /// directly and does not yet drive this executor guard at all -- see
    /// that module's doc). Confirms the stamped fields reach the written
    /// record and the whole group still round-trips byte for byte. Uses
    /// equal scoring/consuming versions (the boundary-legal zero-staleness
    /// case: scored by the exact weight version consuming it) so this same
    /// real, self-consistent group also covers that boundary; the
    /// strictly-nonzero-but-within-bound and the causality-violation cases
    /// are covered by the mutation tests below against the same underlying
    /// `validate_episodes_v1` branch.
    #[test]
    fn bounded_staleness_provenance_round_trips_through_the_written_record() {
        let run_bytes = test_fixture_bytes_v2();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();

        let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
        let genesis_payload = genesis_candidate.payload().to_vec();
        let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
        let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let mut prepared = executor.prepare_update_v2().unwrap();
        prepared.stamp_episode_provenance_v1(2, 2);
        let (group, _advanced_context) = build_update_group_v1(&run, context, &prepared)
            .unwrap()
            .into_parts();
        let group_bytes = group.canonical_bytes().to_vec();

        let decode_context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let decoded = decode_update_group_v1(&run, decode_context, &group_bytes)
            .expect("the stamped group decodes");
        let episodes = &decoded.group().wire.evidence.episodes;
        assert!(!episodes.is_empty(), "the update must emit real episodes");
        for episode in episodes {
            assert_eq!(episode.scoring_weight_version, Some(2));
            assert_eq!(episode.consuming_update_version, Some(2));
        }

        let reencoded = to_canonical_json_bytes_v1(&decoded.group().wire, GROUP_NULL_POLICY_V1)
            .expect("the decoded group re-serializes");
        assert_eq!(
            reencoded, group_bytes,
            "a bounded-staleness-stamped record must round-trip byte for byte"
        );
    }

    /// The synchronous path: no stamping at all. Confirms the two new
    /// fields stay fully absent (never emitted, not even as `null`) for a
    /// record built the ordinary way, exactly like every record written
    /// before this field existed.
    #[test]
    fn absent_bounded_staleness_provenance_round_trips_unchanged() {
        let run_bytes = test_fixture_bytes_v2();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();

        let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
        let genesis_payload = genesis_candidate.payload().to_vec();
        let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
        let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let prepared = executor.prepare_update_v2().unwrap();
        let (group, _advanced_context) = build_update_group_v1(&run, context, &prepared)
            .unwrap()
            .into_parts();
        let group_bytes = group.canonical_bytes().to_vec();
        let group_text = std::str::from_utf8(&group_bytes).unwrap();
        assert!(
            !group_text.contains("scoring_weight_version")
                && !group_text.contains("consuming_update_version"),
            "an ordinary synchronous-path update must never emit either new key"
        );

        let decode_context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let decoded = decode_update_group_v1(&run, decode_context, &group_bytes)
            .expect("the ordinary group decodes");
        for episode in &decoded.group().wire.evidence.episodes {
            assert_eq!(episode.scoring_weight_version, None);
            assert_eq!(episode.consuming_update_version, None);
        }
        let reencoded = to_canonical_json_bytes_v1(&decoded.group().wire, GROUP_NULL_POLICY_V1)
            .expect("the decoded group re-serializes");
        assert_eq!(reencoded, group_bytes);
    }

    /// Golden sync-identity proof required by the production-integration
    /// task: this exact fixed-seed, unstamped update group, run through
    /// this branch's synchronous path (no scheduler, no stamping, identical
    /// call sequence to `absent_bounded_staleness_provenance_round_trips_
    /// unchanged` above), must reproduce the byte length and sha256 that
    /// pristine `origin/main` (commit e930890, before this branch's merge
    /// or any of its own commits) produces for the identical fixture. The
    /// reference values were captured by adding the same construction
    /// (`test_fixture_bytes_v2` -> decode -> executor -> genesis ->
    /// `prepare_update_v2` -> `build_update_group_v1` -> sha256 of
    /// `canonical_bytes()`) to a throwaway `origin/main` worktree, running
    /// it there, and pasting back the printed digest -- not computed by
    /// hand, matching this file's own frozen-byte-total discipline.
    #[test]
    fn sync_path_reproduces_mains_golden_store_hash() {
        use sha2::Digest;

        const MAIN_GOLDEN_SHA256_V1: &str =
            "d0413353fbb7298c47646adfc56d6d43a22e83cb59f874731973177e4ad00f61";
        const MAIN_GOLDEN_LEN_V1: usize = 78_190;

        let run_bytes = test_fixture_bytes_v2();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
        let genesis_payload = genesis_candidate.payload().to_vec();
        let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
        let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        // No stamping: this is the plain synchronous call sequence,
        // unmodified from what `origin/main` itself runs.
        let prepared = executor.prepare_update_v2().unwrap();
        let (group, _advanced_context) = build_update_group_v1(&run, context, &prepared)
            .unwrap()
            .into_parts();
        let group_bytes = group.canonical_bytes().to_vec();

        assert_eq!(
            group_bytes.len(),
            MAIN_GOLDEN_LEN_V1,
            "the synchronous path's canonical byte length must match origin/main exactly"
        );
        let digest: [u8; 32] = sha2::Sha256::digest(&group_bytes).into();
        assert_eq!(
            lower_hex_raw32_v1(digest),
            MAIN_GOLDEN_SHA256_V1,
            "the synchronous path's Store hash must match origin/main exactly: this branch's \
             additive fields must not perturb a single byte of ordinary synchronous output"
        );
    }

    /// Mutation-rejection: the two staleness-provenance fields must be
    /// present together or not at all, exactly like the three
    /// opponent-identity fields above.
    #[test]
    fn staleness_provenance_present_only_singly_is_rejected() {
        let mut only_scoring = group_value_v1();
        only_scoring["evidence"]["episodes"][0]["scoring_weight_version"] = Value::from(0_u64);
        assert_eq!(
            decode_value_error_v1(&only_scoring),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );

        let mut only_consuming = group_value_v1();
        only_consuming["evidence"]["episodes"][0]["consuming_update_version"] = Value::from(1_u64);
        assert_eq!(
            decode_value_error_v1(&only_consuming),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );
    }

    /// Mutation-rejection: `scoring_weight_version > consuming_update_version`
    /// is a causality violation (an episode scored by weights newer than the
    /// update consuming it), and is rejected regardless of any staleness
    /// bound -- the same rule `check_staleness_bound_v1` enforces at the
    /// scheduler.
    #[test]
    fn staleness_provenance_causality_violation_is_rejected() {
        let mut violation = group_value_v1();
        violation["evidence"]["episodes"][0]["scoring_weight_version"] = Value::from(5_u64);
        violation["evidence"]["episodes"][0]["consuming_update_version"] = Value::from(4_u64);
        assert_eq!(
            decode_value_error_v1(&violation),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );
    }

    /// Proves the Store's causality gate inside `validate_episodes_v1` and
    /// the scheduler's own `check_staleness_bound_v1` (called there with
    /// `max_staleness_updates: u32::MAX`, see the comment at that call
    /// site) are the same rule, not just similar-looking duplicated logic:
    /// for a table of (scoring, consuming) pairs spanning both sides of the
    /// causality boundary, `check_staleness_bound_v1`'s own verdict is
    /// cross-checked against whether the Store's `EpisodeBinding` causality
    /// gate rejects. A mutated copy of the shared fixture can trip an
    /// unrelated chain-hash mismatch further down the decode pipeline even
    /// when the causality gate itself is satisfied (the fixture's other
    /// embedded hashes were computed over the original, unmutated episode),
    /// so this checks specifically whether `EpisodeBinding` was the
    /// rejection reason, not merely whether the whole decode succeeded --
    /// `staleness_provenance_causality_violation_is_rejected` and
    /// `staleness_provenance_present_only_singly_is_rejected` above already
    /// prove the true end-to-end rejection case; this test generalizes the
    /// specific causality comparison across more pairs and pins it against
    /// the scheduler's own function.
    #[test]
    fn staleness_provenance_causality_check_matches_check_staleness_bound_v1() {
        let cases: [(u64, u64); 5] = [(0, 0), (3, 5), (5, 5), (5, 3), (U63_MAX_V1, U63_MAX_V1)];
        for (scoring, consuming) in cases {
            let entry = StalenessLedgerEntryV1 {
                episode_id: 0,
                scoring_weight_version: scoring,
                consuming_update_version: consuming,
            };
            let scheduler_ok = check_staleness_bound_v1(entry, u32::MAX).is_ok();

            let mut mutated = group_value_v1();
            mutated["evidence"]["episodes"][0]["scoring_weight_version"] = Value::from(scoring);
            mutated["evidence"]["episodes"][0]["consuming_update_version"] = Value::from(consuming);
            let (run, context) = run_and_context_v1();
            let outcome =
                decode_update_group_v1(&run, context, &canonical_group_value_v1(&mutated));
            let store_rejected_via_causality_gate = matches!(
                outcome.as_ref().err().map(|error| error.kind()),
                Some(UpdateGroupV1ErrorKind::EpisodeBinding)
            );

            assert_eq!(
                !scheduler_ok, store_rejected_via_causality_gate,
                "scoring={scoring} consuming={consuming}: the Store's EpisodeBinding causality \
                 gate must reject exactly when check_staleness_bound_v1(entry, u32::MAX) rejects"
            );
        }
    }

    /// Mutation-rejection: a staleness-provenance value above the u63 domain
    /// is rejected, matching every other u63-domain counter in this schema.
    #[test]
    fn staleness_provenance_above_u63_domain_is_rejected() {
        let mut over_max = group_value_v1();
        over_max["evidence"]["episodes"][0]["scoring_weight_version"] = Value::from(U63_MAX_V1 + 1);
        over_max["evidence"]["episodes"][0]["consuming_update_version"] =
            Value::from(U63_MAX_V1 + 1);
        assert_eq!(
            decode_value_error_v1(&over_max),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );
    }

    /// Complete-genesis Store, recreated from the proven helper in
    /// `native_training_store_resume_v2.rs` (its tests around lines 1009
    /// through 1040), using only public crate APIs. A skeleton-only Store
    /// would make the cleanup proof vacuous, because there would be no
    /// walkable chain for resume to accept in the first place.
    #[cfg(windows)]
    fn bootstrap_and_publish_genesis_c1_v1(
        parent: &Path,
        run: &ValidatedTrainRunV2,
    ) -> ValidatedNativeTrainingStoreRootV2 {
        let bootstrapped = bootstrap_native_training_store_v2(parent, "store").unwrap();
        assert_eq!(
            bootstrapped.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        );
        let root = bootstrapped.into_root();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let receipt = publish_genesis_generation_v2(
            &root,
            run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(receipt.generation_index(), 0);
        root
    }

    /// Live C2 required test: V2 publish, commit, Windows resume, and
    /// next-update continuation preserve the sealed mode and evidence while
    /// checkpoint bytes carry no mode.
    ///
    /// The V2-bound Store is driven from genesis to the exact no-op through
    /// `resume_native_training_store_v2`: every continuation's reconstructed
    /// executor must rederive the environment randomization V2 seal from the
    /// validated run alone, every prepared segment must publish and commit,
    /// and a recognized stage must be deleted by the now-admitted cleanup
    /// plan.
    #[cfg(windows)]
    #[test]
    fn v2_resume_drives_publish_commit_and_continuation_with_rederived_mode() {
        use crate::native_training_store_prepared_segment_v2::prepare_segment_v2;
        use crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2;
        use crate::native_training_store_v2::publish_prepared_segment_v2;

        let parent = StoreSuiteParentV1::new("resume-live-v2");
        let v2 = coherent_v2_run_v1();
        let root = bootstrap_and_publish_genesis_c1_v1(parent.path(), &v2);

        // A recognized stage leaf: live C2 resume must now delete it under
        // the exclusive lock instead of rejecting the run.
        let stage = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Segments)
            .join(".segment-00000000.json.stage-v2");
        fs::write(&stage, b"live-c2-stage-sentinel").unwrap();

        let target = v2.requested_successful_updates();
        let mut expected_parent = 0_u64;
        loop {
            match resume_native_training_store_v2(&root, &v2, execution_config_v1(&v2)).unwrap() {
                NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                } => {
                    assert_eq!(latest_generation_index, target);
                    break;
                }
                NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                    assert!(
                        fs::symlink_metadata(&stage).is_err(),
                        "live C2 resume must apply the recognized-stage deletion plan"
                    );
                    assert_eq!(continuation.parent_generation_index, expected_parent);
                    // The reconstructed executor rederives the V2 seal from
                    // the validated run although checkpoint bytes carry no
                    // mode.
                    assert_eq!(
                        continuation.executor.environment_trajectory_contract_v1(),
                        NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
                    );
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &v2,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .unwrap();
                    let receipt = publish_prepared_segment_v2(
                        &root,
                        &v2,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    expected_parent += v2.checkpoint_segment_updates();
                }
            }
        }
        assert_eq!(expected_parent, target);
        let state = validate_native_training_store_v2(&root, &v2)
            .expect("the completed V2 Store walks cleanly");
        assert_eq!(state.latest_generation_index(), target);
    }

    /// Live C2 wide resume witness: a genuinely wide plus environment
    /// randomization V2 Store resume must reach the production wide
    /// reconstruction branch through the run-bound wide checkpoint
    /// constructor. Checkpoint bytes are mode-free, so the post-seal
    /// construction counter, scoped around the single resume call, is the
    /// witness that kills a reverted raw wide constructor. The wide
    /// Sequential continuation is deliberately not trained.
    #[cfg(windows)]
    #[test]
    fn wide_v2_resume_reconstructs_through_the_run_bound_wide_constructor() {
        use crate::native_training_executor_v1::{
            run_bound_checkpoint_construction_count_scope_v2, NativeTrainingExecutorV1,
        };
        use crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_environment_v2;

        let parent = StoreSuiteParentV1::new("resume-wide-v2");
        let wide_v2 = decode_train_run_v2(
            &test_fixture_bytes_with_schedule_and_base_seed_wide_environment_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                2,
                4,
                4,
                2,
                4,
                8,
                32_768,
                65_536,
                71_501,
            ),
        )
        .expect("the wide V2 fixture decodes");
        assert_eq!(
            wide_v2.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );

        // Publish the wide genesis; the payload is byte-identical to the
        // run-bound sibling's, so publication stays the production shape.
        let bootstrapped = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
        let root = bootstrapped.into_root();
        let (wide_manifest, wide_payload_path) =
            crate::common_model_snapshot_v1::wide_model_snapshot_paths_v1();
        let genesis_executor = NativeTrainingExecutorV1::from_common_model_snapshot_wide_v1(
            execution_config_v1(&wide_v2),
            &wide_manifest,
            &wide_payload_path,
        )
        .unwrap();
        let payload = genesis_executor
            .checkpoint_candidate_v1()
            .unwrap()
            .payload()
            .to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&wide_v2, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&wide_v2, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&wide_v2, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&wide_v2, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let _ = publish_genesis_generation_v2(
            &root,
            &wide_v2,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();

        // The single resume call, scoped exactly.
        let scope = run_bound_checkpoint_construction_count_scope_v2();
        let resumed =
            resume_native_training_store_v2(&root, &wide_v2, execution_config_v1(&wide_v2))
                .expect("the wide V2 store must resume since C2");
        assert_eq!(
            scope.counts(),
            (0, 1),
            "wide resume must reconstruct through the run-bound wide constructor"
        );
        match resumed {
            NativeTrainingStoreResumeV2::Continue(continuation) => {
                assert_eq!(continuation.parent_generation_index, 0);
                assert_eq!(
                    continuation.executor.environment_trajectory_contract_v1(),
                    NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2,
                    "the continuation executor must rederive the V2 seal"
                );
                assert!(continuation.executor.snapshot_receipt().is_none());
            }
            NativeTrainingStoreResumeV2::Complete { .. } => {
                panic!("generation zero of a target-four run cannot be complete")
            }
        }
    }

    /// Live C2 science-loop proof for the run-bound genesis callsite: the
    /// complete science loop drives a V2 run end to end from real snapshots,
    /// which can only succeed if the ordinary genesis branch constructs its
    /// executor through the run-bound snapshot constructor and every later
    /// window rederives the V2 seal.
    #[cfg(windows)]
    #[test]
    fn v2_science_loop_completes_end_to_end_from_real_snapshots() {
        let parent = StoreSuiteParentV1::new("science-live-v2");
        let v2 = coherent_v2_run_v1();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 2,
            scheduler_timeout: Duration::from_secs(600),
            measure_broker_service_time: false,
            starting_player: None,
        };
        let genesis_scope =
            crate::native_training_executor_v1::run_bound_snapshot_construction_count_scope_v2();
        let report = run_native_science_loop_v1(
            parent.path(),
            "store",
            &v2,
            execution_config_v1(&v2),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config,
            None,
            None,
        )
        .expect("the V2 science loop must complete end to end since C2");
        // Genesis bytes are mode-free, so byte equality cannot prove the
        // callsite; this counter can. A reverted raw genesis constructor
        // makes this exact assertion fail.
        assert_eq!(
            genesis_scope.counts(),
            (1, 0),
            "the ordinary narrow genesis site must construct run-bound exactly once"
        );
        assert_eq!(
            report.latest_generation_index(),
            v2.requested_successful_updates()
        );
        for run_result in [report.reference_run(), report.candidate_run()] {
            for binding in run_result.episode_bindings() {
                assert!(
                    binding.outer_trajectory_sha256_v2().is_some(),
                    "V2 evaluation bindings must retain the outer evidence"
                );
            }
        }
    }

    /// Live C2 frozen-byte proof against the pre-C2 baseline captured at
    /// `058d11f0dccf9e485cd6577bd52ca2b57e3253c9`: every target pins the
    /// float-free episode projection, and the x86_64 Linux-gnu target where
    /// that baseline was captured additionally pins the whole-group bytes
    /// (trained loss/gauge/model bits carry platform libm last bits, so the
    /// full-byte pin is per-target by construction).
    #[test]
    fn legacy_update_group_matches_pre_c2_projection_with_linux_gnu_full_byte_pin() {
        use sha2::Digest;

        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let run = legacy_run_v1();
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &manifest,
            &payload,
        )
        .unwrap();
        let genesis_payload = executor
            .checkpoint_candidate_v1()
            .unwrap()
            .payload()
            .to_vec();
        let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
        let context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let prepared = executor.prepare_update_v2().unwrap();
        let advance = build_update_group_v1(&run, context, &prepared).unwrap();
        let (group, _context) = advance.into_parts();
        // Trained loss/gauge/model bits carry platform libm last bits, so the
        // whole-group literal is a per-target witness in the same spirit as
        // the recorded burn-pair numerical witness; it was captured from the
        // pre-C2 baseline checkout on this exact target.
        #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
        {
            let canonical_sha256: [u8; 32] = Sha256::digest(group.canonical_bytes()).into();
            assert_eq!(
                lower_hex_raw32_v1(canonical_sha256),
                "0644682ac8697833c7498449c6f170019df70f2c9fbfba2bb73c283b7cc93dd3",
                "the legacy update group canonical bytes drifted from the pre-C2 baseline"
            );
            assert_eq!(
                lower_hex_raw32_v1(group.update_evidence_sha256()),
                "f3f2e325d2afebd3792ecbfd72c4e50cfb1f455d849918fa34a61db255cbbe37",
                "the legacy update evidence digest drifted from the pre-C2 baseline"
            );
            assert_eq!(group.canonical_bytes().len(), 78_190);
        }
        // The episode projection carries counts, seeds, deck bindings, and
        // trajectory digests but no float bits, so this pre-C2 pin is
        // platform-independent.
        let value: Value = serde_json::from_slice(group.canonical_bytes()).unwrap();
        let episodes_cj =
            to_canonical_json_bytes_v1(&value["evidence"]["episodes"], episode_null_policy_v1())
                .unwrap();
        let episodes_sha256: [u8; 32] = Sha256::digest(&episodes_cj).into();
        assert_eq!(
            lower_hex_raw32_v1(episodes_sha256),
            "a250bc19d5ce9a756e89c1e40abaaf59b532eead9ad8fd748c60b8b4221f70b3",
            "the legacy episode projection drifted from the pre-C2 baseline"
        );
    }

    /// Shared live-C2 authority builder: genesis checkpoint, boundary, and a
    /// prepared transition from an executor sealed to the requested mode,
    /// always over the same K=2 genesis window.
    struct LiveC2AuthoritiesV1 {
        run: ValidatedTrainRunV2,
        genesis: CheckpointManifestV3,
    }

    fn live_c2_authorities_v1(v2: bool) -> LiveC2AuthoritiesV1 {
        use crate::native_training_store_run_v2::test_fixture_bytes_environment_randomization_v2;
        let run = if v2 {
            decode_train_run_v2(&test_fixture_bytes_environment_randomization_v2()).unwrap()
        } else {
            legacy_run_v1()
        };
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&run),
            &manifest,
            &payload,
        )
        .unwrap();
        let genesis_payload = executor
            .checkpoint_candidate_v1()
            .unwrap()
            .payload()
            .to_vec();
        let genesis = build_genesis_checkpoint_manifest_v3(&run, &genesis_payload).unwrap();
        LiveC2AuthoritiesV1 { run, genesis }
    }

    fn sealed_transition_v1(
        authorities: &LiveC2AuthoritiesV1,
        v2_sealed: bool,
    ) -> crate::native_training_executor_v1::NativeTrainingPreparedTransitionV2 {
        use crate::native_training_store_run_v2::test_fixture_bytes_environment_randomization_v2;
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let seed_executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(&authorities.run),
            &manifest,
            &payload,
        )
        .unwrap();
        let genesis_candidate = seed_executor.checkpoint_candidate_v1().unwrap();
        let mut executor = if v2_sealed {
            let v2_run =
                decode_train_run_v2(&test_fixture_bytes_environment_randomization_v2()).unwrap();
            NativeTrainingExecutorV1::from_checkpoint_candidate_run_bound_v2(
                execution_config_v1(&authorities.run),
                &genesis_candidate,
                &v2_run,
            )
            .unwrap()
        } else {
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(
                execution_config_v1(&authorities.run),
                &genesis_candidate,
            )
            .unwrap()
        };
        let facts = executor.intrinsic_checkpoint_facts_v2().unwrap();
        let mut candidate = executor.begin_segment_candidate_v2().unwrap();
        let transition = candidate.prepare_transition_v2(facts, true).unwrap();
        transition
    }

    /// Live C2 Store diagonal: all eight homogeneous run/transition/receipt
    /// triples, with the evidence context always built from the tested run so
    /// an earlier context mismatch cannot make the oracle vacuous. Only LLL
    /// and VVV admit; run/transition off-diagonals reject as RunBinding and
    /// receipt off-variants reject as EpisodeBinding, each with zero evidence
    /// projections (the compact entry constructs no context of its own, so
    /// the projection counter is the meaningful zero here).
    #[test]
    fn store_admits_only_the_lll_and_vvv_triples() {
        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let legacy = live_c2_authorities_v1(false);
        let v2 = live_c2_authorities_v1(true);

        // Donor receipts per variant, harvested from genuine executions over
        // the same genesis window.
        let legacy_donor = sealed_transition_v1(&legacy, false);
        let v2_donor = sealed_transition_v1(&v2, true);
        let legacy_receipts: Vec<_> = legacy_donor
            .observation_v2()
            .episodes
            .iter()
            .map(|episode| episode.full_trajectory_receipt)
            .collect();
        let v2_receipts: Vec<_> = v2_donor
            .observation_v2()
            .episodes
            .iter()
            .map(|episode| episode.full_trajectory_receipt)
            .collect();
        assert!(legacy_receipts
            .iter()
            .all(|receipt| !receipt.is_environment_randomization_v2()));
        assert!(v2_receipts
            .iter()
            .all(|receipt| receipt.is_environment_randomization_v2()));

        for run_is_v2 in [false, true] {
            for transition_is_v2 in [false, true] {
                for receipts_are_v2 in [false, true] {
                    let authorities = if run_is_v2 { &v2 } else { &legacy };
                    let context =
                        begin_update_evidence_chain_v1(&authorities.run, &authorities.genesis)
                            .unwrap();
                    let mut transition = sealed_transition_v1(authorities, transition_is_v2);
                    if receipts_are_v2 != transition_is_v2 {
                        let donors = if receipts_are_v2 {
                            &v2_receipts
                        } else {
                            &legacy_receipts
                        };
                        for (index, donor) in donors.iter().enumerate() {
                            transition.swap_observation_receipt_for_test_v2(index, *donor);
                        }
                    }
                    let scope = store_evidence_count_scope_v2();
                    let outcome =
                        build_compact_update_group_v2(&authorities.run, context, transition);
                    match (run_is_v2, transition_is_v2, receipts_are_v2) {
                        (false, false, false) | (true, true, true) => {
                            let (advance, _successor, checkpoint) =
                                outcome.expect("the homogeneous diagonal must admit");
                            assert!(checkpoint.is_some());
                            let (_group, advanced) = advance.into_parts();
                            assert_eq!(advanced.next_update_index(), 2);
                            // Positive instrumentation control: the admitted
                            // diagonal projects exactly once and constructs
                            // no context of its own, so a deleted increment
                            // site cannot hide behind zero-only assertions.
                            assert_eq!(scope.counts(), (0, 1));
                        }
                        (run_v2, transition_v2, _) if run_v2 != transition_v2 => {
                            let error = outcome.map(|_| ()).unwrap_err();
                            assert_eq!(error.kind(), UpdateGroupV1ErrorKind::RunBinding);
                            assert_eq!(
                                scope.counts().1,
                                0,
                                "an off-diagonal transition must project zero evidence"
                            );
                        }
                        _ => {
                            let error = outcome.map(|_| ()).unwrap_err();
                            assert_eq!(error.kind(), UpdateGroupV1ErrorKind::EpisodeBinding);
                            assert_eq!(
                                scope.counts().1,
                                0,
                                "an off-variant receipt vector must reject before any \
                                 evidence projection"
                            );
                        }
                    }
                }
            }
        }

        // Mixed receipt vector: one swapped episode is enough to reject.
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let mut mixed = sealed_transition_v1(&v2, true);
        mixed.swap_observation_receipt_for_test_v2(1, legacy_receipts[1]);
        let scope = store_evidence_count_scope_v2();
        assert_eq!(
            build_compact_update_group_v2(&v2.run, context, mixed)
                .map(|_| ())
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );
        assert_eq!(scope.counts().1, 0, "zero evidence projections");

        // Cross-episode swap inside one genuine V2 observation: variants
        // stay on the diagonal, so the preflight passes and the projection
        // itself must reject the swapped episode binding.
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let mut crossed = sealed_transition_v1(&v2, true);
        let first = crossed.observation_v2().episodes[0].full_trajectory_receipt;
        let second = crossed.observation_v2().episodes[1].full_trajectory_receipt;
        crossed.swap_observation_receipt_for_test_v2(0, second);
        crossed.swap_observation_receipt_for_test_v2(1, first);
        assert_eq!(
            build_compact_update_group_v2(&v2.run, context, crossed)
                .map(|_| ())
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::EpisodeBinding
        );
    }

    /// Live C2 genuine V2 Store emission oracle: a genuinely V2-produced
    /// update group's every wire episode has exactly the frozen eighteen
    /// keys, `trajectory_sha256` is the inner compatibility digest hex and
    /// never the outer digest, no outer/V2 key or substring appears anywhere
    /// in the group bytes, and the update digest is domain-separated by the
    /// raw run SHA alone.
    #[test]
    fn genuine_v2_store_emission_keeps_episode_wire_frozen_and_domain_separated() {
        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let v2 = live_c2_authorities_v1(true);
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let transition = sealed_transition_v1(&v2, true);
        let recorded: Vec<([u8; 32], [u8; 32], u64)> = transition
            .observation_v2()
            .episodes
            .iter()
            .map(|episode| {
                let receipt = episode.full_trajectory_receipt;
                (
                    receipt.trajectory_sha256(),
                    receipt
                        .outer_trajectory_sha256_v2()
                        .expect("a genuine V2 receipt carries the outer digest"),
                    receipt.episode_index(),
                )
            })
            .collect();
        let (advance, _successor, _checkpoint) =
            build_compact_update_group_v2(&v2.run, context, transition).unwrap();
        let (group, _advanced) = advance.into_parts();

        let value: Value = serde_json::from_slice(group.canonical_bytes()).unwrap();
        let episodes = value["evidence"]["episodes"].as_array().unwrap();
        assert_eq!(episodes.len(), recorded.len());
        for (episode, (inner, outer, episode_index)) in episodes.iter().zip(&recorded) {
            let object = episode.as_object().unwrap();
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(
                keys, EPISODE_KEYS_ORACLE_V1,
                "a genuinely V2-produced wire episode must keep exactly the 18 frozen keys"
            );
            assert_eq!(episode["episode_index"].as_u64().unwrap(), *episode_index);
            assert_eq!(
                episode["trajectory_sha256"].as_str().unwrap(),
                lower_hex_raw32_v1(*inner),
                "the wire digest is the inner compatibility digest"
            );
            assert_ne!(
                episode["trajectory_sha256"].as_str().unwrap(),
                lower_hex_raw32_v1(*outer),
                "the wire digest must never be the outer digest"
            );
        }
        let rendered = String::from_utf8(group.canonical_bytes().to_vec()).unwrap();
        for forbidden in [
            "environment_randomization_v2",
            "trajectory_sha256_v2",
            "outer_trajectory_sha256",
            "outer_digest",
            "receipt",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "the V2 group bytes must not carry {forbidden}"
            );
        }
        for (_, outer, _) in &recorded {
            assert!(
                !rendered.contains(&lower_hex_raw32_v1(*outer)),
                "the outer digest hex must not appear anywhere in the group bytes"
            );
        }

        // Independent digest-framing oracle: the frozen five-atom framing is
        // reimplemented from scratch, proven equal to the production digest
        // for the real raw run SHA, and then rerun differing ONLY in the raw
        // run SHA to prove domain separation.
        fn independent_update_digest_v1(
            run_sha256: [u8; 32],
            update_index: u64,
            previous: Option<[u8; 32]>,
            evidence_cj: &[u8],
        ) -> [u8; 32] {
            fn append_atom(bytes: &mut Vec<u8>, tag: &str, payload: &[u8]) {
                bytes.extend_from_slice(&u32::try_from(tag.len()).unwrap().to_be_bytes());
                bytes.extend_from_slice(tag.as_bytes());
                bytes.extend_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
                bytes.extend_from_slice(payload);
            }
            let mut framed = Vec::new();
            // The identity is a frozen local literal on purpose: an oracle
            // importing the production constant could not catch its drift.
            append_atom(
                &mut framed,
                "domain",
                b"mtg-kernel-native-training-update-evidence-sha256-v1",
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
            append_atom(&mut framed, "evidence_canonical_json", evidence_cj);
            Sha256::digest(framed).into()
        }
        let evidence_cj =
            to_canonical_json_bytes_v1(&value["evidence"], episode_null_policy_v1()).unwrap();
        let run_sha = parse_digest_v1(v2.run.run_sha256()).unwrap();
        let independent = independent_update_digest_v1(run_sha, 1, None, &evidence_cj);
        assert_eq!(
            independent,
            group.update_evidence_sha256(),
            "the from-scratch framing must equal the production digest"
        );
        let mut other_run_sha = run_sha;
        other_run_sha[0] ^= 1;
        assert_ne!(
            independent_update_digest_v1(other_run_sha, 1, None, &evidence_cj),
            independent,
            "the update digest must be domain-separated by the raw run SHA alone"
        );

        // Direct checkpoint byte proof: the V2 genesis checkpoint manifest
        // carries no outer digest, no mode key, and no environment
        // randomization section string.
        let manifest_text = String::from_utf8(v2.genesis.canonical_bytes().to_vec()).unwrap();
        for forbidden in [
            "environment_randomization_v2",
            "trajectory_sha256_v2",
            "outer_trajectory_sha256",
            "outer_digest",
            "\"mode\"",
        ] {
            assert!(
                !manifest_text.contains(forbidden),
                "the checkpoint manifest must not carry {forbidden}"
            );
        }
    }

    const EPISODE_KEYS_ORACLE_V1: [&str; 18] = [
        "deck_hashes_u64_hex",
        "deck_ids",
        "environment_seed_u64_hex",
        "episode_index",
        "learner_physical_decision_count",
        "learner_policy_step_count",
        "learner_return",
        "learner_seat",
        "opponent_physical_decision_count",
        "opponent_policy_step_count",
        "physical_decision_count",
        "policy_step_count",
        "schema",
        "terminal_classification",
        "terminal_code",
        "terminal_outcome",
        "trajectory_sha256",
        "winner",
    ];

    /// Live C2 guard-path coverage: `build_update_group_v1`'s prepared-update
    /// entry admits a genuine V2 prepared update, its wire bytes are
    /// byte-identical to the compact path over the same window, and both
    /// off-diagonal run/producer pairings reject before predecessor export
    /// with zero evidence projections.
    #[test]
    fn full_prepared_guard_path_admits_v2_and_rejects_the_off_diagonals() {
        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let legacy = live_c2_authorities_v1(false);
        let v2 = live_c2_authorities_v1(true);
        let (manifest, payload) = common_model_snapshot_paths_v1();

        let sealed_executor = |authorities: &LiveC2AuthoritiesV1, v2_sealed: bool| {
            let seed_executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
                execution_config_v1(&authorities.run),
                &manifest,
                &payload,
            )
            .unwrap();
            let genesis_candidate = seed_executor.checkpoint_candidate_v1().unwrap();
            if v2_sealed {
                NativeTrainingExecutorV1::from_checkpoint_candidate_run_bound_v2(
                    execution_config_v1(&authorities.run),
                    &genesis_candidate,
                    &v2.run,
                )
                .unwrap()
            } else {
                NativeTrainingExecutorV1::from_checkpoint_candidate_v1(
                    execution_config_v1(&authorities.run),
                    &genesis_candidate,
                )
                .unwrap()
            }
        };

        // Genuine V2 admission through the guard path, byte-equal to the
        // compact path over the same genesis window. The positive
        // predecessor-export control: building through the guard exports the
        // unchanged predecessor (one extra snapshot and payload encode on
        // top of the prepared update's own final export).
        let mut v2_executor = sealed_executor(&v2, true);
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();
        let prepared = v2_executor.prepare_update_v2().unwrap();
        assert!(prepared.observation().episodes.iter().all(|episode| episode
            .full_trajectory_receipt
            .is_environment_randomization_v2()));
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 1);
        assert_eq!(payload_encode_counts_for_test_v1(), (1, 1));
        let advance = build_update_group_v1(&v2.run, context, &prepared).unwrap();
        assert_eq!(
            train_state_snapshot_call_count_for_test_v1(),
            2,
            "the admitted guard path exports the predecessor exactly once"
        );
        assert_eq!(payload_encode_counts_for_test_v1(), (2, 2));
        let (guard_group, _advanced) = advance.into_parts();
        drop(prepared);

        let compact_context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let compact_transition = sealed_transition_v1(&v2, true);
        let (compact_advance, _successor, _checkpoint) =
            build_compact_update_group_v2(&v2.run, compact_context, compact_transition).unwrap();
        let (compact_group, _compact_advanced) = compact_advance.into_parts();
        assert_eq!(
            guard_group.canonical_bytes(),
            compact_group.canonical_bytes(),
            "the guard and compact paths must emit identical V2 wire bytes"
        );
        assert_eq!(
            guard_group.update_evidence_sha256(),
            compact_group.update_evidence_sha256()
        );

        // Off-diagonal 1: V2 run against a Legacy-sealed prepared producer.
        // The rejection precedes predecessor export: zero train-state
        // snapshots and zero payload encodes, on top of zero projections.
        let mut legacy_producer = sealed_executor(&v2, false);
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let prepared = legacy_producer.prepare_update_v2().unwrap();
        let scope = store_evidence_count_scope_v2();
        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();
        assert_eq!(
            build_update_group_v1(&v2.run, context, &prepared)
                .map(|_| ())
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::RunBinding
        );
        assert_eq!(scope.counts().1, 0, "zero evidence projections");
        assert_eq!(
            train_state_snapshot_call_count_for_test_v1(),
            0,
            "the off-diagonal rejection must precede predecessor export"
        );
        assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
        drop(prepared);

        // Off-diagonal 2: legacy run against a V2-sealed prepared producer.
        let mut v2_producer = sealed_executor(&legacy, true);
        let context = begin_update_evidence_chain_v1(&legacy.run, &legacy.genesis).unwrap();
        let prepared = v2_producer.prepare_update_v2().unwrap();
        let scope = store_evidence_count_scope_v2();
        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();
        assert_eq!(
            build_update_group_v1(&legacy.run, context, &prepared)
                .map(|_| ())
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::RunBinding
        );
        assert_eq!(scope.counts().1, 0, "zero evidence projections");
        assert_eq!(
            train_state_snapshot_call_count_for_test_v1(),
            0,
            "the off-diagonal rejection must precede predecessor export"
        );
        assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
        drop(prepared);
    }

    /// Live C2 V2-fact mutation battery: every bound V2-only or common
    /// binding fact of a genuine V2 receipt, corrupted one at a time through
    /// the crate-private test seam, rejects Store evidence construction as an
    /// episode-binding failure. Deck mutations cover both indices; the pure
    /// order swap is representable-equal under the symmetric run fixture and
    /// is therefore proven at the asymmetric trainer layer instead.
    #[test]
    fn v2_receipt_fact_mutations_reject_store_evidence_construction() {
        use crate::native_full_episode_trajectory_v2::NativeV2ReceiptFactMutationForTestV2;

        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let v2 = live_c2_authorities_v1(true);
        for mutation in [
            NativeV2ReceiptFactMutationForTestV2::PairIndex,
            NativeV2ReceiptFactMutationForTestV2::DeckId0,
            NativeV2ReceiptFactMutationForTestV2::DeckId1,
            NativeV2ReceiptFactMutationForTestV2::EpisodeIndex,
            NativeV2ReceiptFactMutationForTestV2::PairRoot,
            NativeV2ReceiptFactMutationForTestV2::DeckHash0,
            NativeV2ReceiptFactMutationForTestV2::DeckHash1,
            NativeV2ReceiptFactMutationForTestV2::LearnerSeat,
        ] {
            let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
            let mut transition = sealed_transition_v1(&v2, true);
            let mut corrupted = transition.observation_v2().episodes[0].full_trajectory_receipt;
            corrupted.mutate_environment_fact_for_test_v2(mutation);
            transition.swap_observation_receipt_for_test_v2(0, corrupted);
            assert_eq!(
                build_compact_update_group_v2(&v2.run, context, transition)
                    .map(|_| ())
                    .unwrap_err()
                    .kind(),
                UpdateGroupV1ErrorKind::EpisodeBinding,
                "mutation {mutation:?} must reject evidence construction"
            );
        }

        // A pure order swap is representable-equal under the symmetric
        // Rally/Rally run fixture, so it is exercised where the bindings are
        // genuinely asymmetric: the trainer's Rally/Burn genuine pair pins
        // the ordered deck hashes and the order-binding outer envelope, and
        // the per-index DeckId0/DeckId1 mutations above prove each index is
        // compared on its own.

        // Observation scalar mutation: one episode's own index scalar drifts
        // while its receipt stays genuine.
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let mut scalar = sealed_transition_v1(&v2, true);
        scalar.mutate_observation_episode_index_for_test_v2(0);
        assert_eq!(
            build_compact_update_group_v2(&v2.run, context, scalar)
                .map(|_| ())
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::EpisodeBinding,
            "an observation scalar drift must reject evidence construction"
        );

        // Whole episode-wrapper swap: both coherent wrappers, wrong slots.
        let context = begin_update_evidence_chain_v1(&v2.run, &v2.genesis).unwrap();
        let mut swapped = sealed_transition_v1(&v2, true);
        swapped.swap_observation_episodes_for_test_v2(0, 1);
        assert_eq!(
            build_compact_update_group_v2(&v2.run, context, swapped)
                .map(|_| ())
                .unwrap_err()
                .kind(),
            UpdateGroupV1ErrorKind::EpisodeBinding,
            "a whole episode-wrapper swap must reject evidence construction"
        );
    }
}
