//! Allocation-free pre-rollout representability authority for one trained
//! Native Training Store V2 segment.
//!
//! This module derives every count, index, and canonical-JSON maximum from a
//! sealed run and lineage-complete parent. It allocates no represented arrays
//! and performs no trainer clone, rollout, optimizer work, or artifact build.

use crate::canonical_json_v1::{
    canonical_json_tree_allocation_layout_bytes_v1, CanonicalJsonClosedMaxErrorV1,
};
use crate::native_train_state_payload_v1::NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1;
use crate::native_training_executor_v1::{
    NativeTrainingEpisodeObservationV1, NativeTrainingGaugeSubstepObservationV1,
    NativeTrainingPhysicalTermObservationV1, NativeTrainingSelectedOutputObservationV1,
};
use crate::native_training_store_boundary_v2::{
    maximum_trained_checkpoint_sidecar_cj_bytes_v2, maximum_trained_head_record_cj_bytes_v2,
    ValidatedNativeTrainingBoundaryV2, CHECKPOINT_SIDECAR_MAX_BYTES_V2, HEAD_RECORD_MAX_BYTES_V2,
};
use crate::native_training_store_checkpoint_v3::{
    maximum_checkpoint_manifest_cj_bytes_v3, CHECKPOINT_MANIFEST_MAX_BYTES_V3,
};
use crate::native_training_store_digest_v1::parse_lower_hex_raw32_v1;
use crate::native_training_store_reference_latest_v2::{
    maximum_latest_record_cj_bytes_v2, maximum_trained_checkpoint_reference_cj_bytes_v2,
    CHECKPOINT_REFERENCE_MAX_BYTES_V2, LATEST_RECORD_MAX_BYTES_V2,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_continuation_v2::{
    maximum_one_group_continuation_cj_bytes_v2, segment_continuation_allocation_layout_bytes_v2,
    SEGMENT_CONTINUATION_MAX_BYTES_V2, SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2,
    SEGMENT_CONTINUATION_MAX_LOGICAL_ROWS_V2,
};
use crate::native_training_store_segment_manifest_v2::{
    maximum_genesis_segment_manifest_cj_bytes_v2, maximum_trained_segment_manifest_cj_bytes_v2,
    segment_manifest_allocation_layout_bytes_v2, SEGMENT_MANIFEST_MAX_BYTES_V2,
};
use crate::native_training_store_update_group_v1::{
    maximum_update_group_json_shape_v2, update_group_allocation_layout_bytes_v2,
    ValidatedUpdateGroupV1,
};
use std::alloc::Layout;
use std::error::Error;
use std::fmt::{Display, Formatter};

const U63_MAX_V2: u64 = (1_u64 << 63) - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SegmentRepresentabilityPlanErrorKindV2 {
    InputBinding,
    BoundExceeded,
}

impl SegmentRepresentabilityPlanErrorKindV2 {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InputBinding => "native-training-segment-input-binding-invalid",
            Self::BoundExceeded => "segment-representability-bound-exceeded",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentRepresentabilityPlanErrorV2 {
    kind: SegmentRepresentabilityPlanErrorKindV2,
}

impl SegmentRepresentabilityPlanErrorV2 {
    const fn new(kind: SegmentRepresentabilityPlanErrorKindV2) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind(self) -> SegmentRepresentabilityPlanErrorKindV2 {
        self.kind
    }
}

impl Display for SegmentRepresentabilityPlanErrorV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.kind.code())
    }
}

impl Error for SegmentRepresentabilityPlanErrorV2 {}

type Result<T> = std::result::Result<T, SegmentRepresentabilityPlanErrorV2>;

/// Sealed, move-only proof that the complete next segment is representable on
/// this runtime before any trainer state is cloned or mutated.
#[derive(Debug)]
pub(crate) struct SegmentRepresentabilityPlanV2 {
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    parent_generation_index: u64,
    generation_index: u64,
    segment_ordinal: u64,
    update_start_index: u64,
    episode_start: u64,
    episode_count: u64,
    episode_end_exclusive: u64,
    total_episode_budget: u64,
    max_physical_terms: u64,
    max_gauge_bounds: u64,
    max_physical_terms_per_segment: u64,
    max_gauge_bounds_per_segment: u64,
    max_logical_rows_per_update: u64,
    max_logical_rows_per_segment: u64,
    segment_update_capacity: usize,
    batch_episode_capacity: usize,
    max_physical_term_capacity: usize,
    max_gauge_bound_capacity: usize,
    max_continuation_capacity: usize,
    max_update_group_json_token_bytes: u64,
    max_segment_update_group_json_token_bytes: u64,
    max_update_group_cj_bytes: u64,
    max_segment_update_group_cj_bytes: u64,
    max_one_group_continuation_cj_bytes: u64,
    max_segment_continuation_cj_bytes: u64,
    max_trained_segment_manifest_cj_bytes: u64,
    max_genesis_segment_manifest_cj_bytes: u64,
    max_checkpoint_manifest_cj_bytes: u64,
    max_checkpoint_sidecar_cj_bytes: u64,
    max_head_record_cj_bytes: u64,
    max_checkpoint_reference_cj_bytes: u64,
    max_latest_record_cj_bytes: u64,
    max_publication_plan_entries: u64,
    max_immutable_final_entries: u64,
    max_runtime_allocation_product_bytes: u64,
}

impl SegmentRepresentabilityPlanV2 {
    pub(crate) const fn run_sha256_v2(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub(crate) const fn identity_bundle_sha256_v2(&self) -> [u8; 32] {
        self.identity_bundle_sha256
    }

    pub(crate) const fn parent_generation_index_v2(&self) -> u64 {
        self.parent_generation_index
    }

    pub(crate) const fn generation_index_v2(&self) -> u64 {
        self.generation_index
    }

    pub(crate) const fn segment_ordinal_v2(&self) -> u64 {
        self.segment_ordinal
    }

    pub(crate) const fn batch_episodes_v2(&self) -> u64 {
        self.batch_episodes
    }

    pub(crate) const fn checkpoint_segment_updates_v2(&self) -> u64 {
        self.checkpoint_segment_updates
    }

    pub(crate) const fn requested_successful_updates_v2(&self) -> u64 {
        self.requested_successful_updates
    }

    pub(crate) const fn max_physical_decisions_v2(&self) -> u64 {
        self.max_physical_decisions
    }

    pub(crate) const fn max_policy_steps_v2(&self) -> u64 {
        self.max_policy_steps
    }

    pub(crate) const fn update_start_index_v2(&self) -> u64 {
        self.update_start_index
    }

    pub(crate) const fn episode_start_v2(&self) -> u64 {
        self.episode_start
    }

    pub(crate) const fn episode_count_v2(&self) -> u64 {
        self.episode_count
    }

    pub(crate) const fn episode_end_exclusive_v2(&self) -> u64 {
        self.episode_end_exclusive
    }

    pub(crate) const fn total_episode_budget_v2(&self) -> u64 {
        self.total_episode_budget
    }

    pub(crate) const fn max_physical_terms_v2(&self) -> u64 {
        self.max_physical_terms
    }

    pub(crate) const fn max_gauge_bounds_v2(&self) -> u64 {
        self.max_gauge_bounds
    }

    pub(crate) const fn max_physical_terms_per_segment_v2(&self) -> u64 {
        self.max_physical_terms_per_segment
    }

    pub(crate) const fn max_gauge_bounds_per_segment_v2(&self) -> u64 {
        self.max_gauge_bounds_per_segment
    }

    pub(crate) const fn max_logical_rows_per_update_v2(&self) -> u64 {
        self.max_logical_rows_per_update
    }

    pub(crate) const fn max_logical_rows_per_segment_v2(&self) -> u64 {
        self.max_logical_rows_per_segment
    }

    pub(crate) const fn segment_update_capacity_v2(&self) -> usize {
        self.segment_update_capacity
    }

    pub(crate) const fn batch_episode_capacity_v2(&self) -> usize {
        self.batch_episode_capacity
    }

    pub(crate) const fn max_physical_term_capacity_v2(&self) -> usize {
        self.max_physical_term_capacity
    }

    pub(crate) const fn max_gauge_bound_capacity_v2(&self) -> usize {
        self.max_gauge_bound_capacity
    }

    pub(crate) const fn max_continuation_capacity_v2(&self) -> usize {
        self.max_continuation_capacity
    }

    pub(crate) const fn max_update_group_json_token_bytes_v2(&self) -> u64 {
        self.max_update_group_json_token_bytes
    }

    pub(crate) const fn max_segment_update_group_json_token_bytes_v2(&self) -> u64 {
        self.max_segment_update_group_json_token_bytes
    }

    pub(crate) const fn max_update_group_cj_bytes_v2(&self) -> u64 {
        self.max_update_group_cj_bytes
    }

    pub(crate) const fn max_segment_update_group_cj_bytes_v2(&self) -> u64 {
        self.max_segment_update_group_cj_bytes
    }

    pub(crate) const fn max_one_group_continuation_cj_bytes_v2(&self) -> u64 {
        self.max_one_group_continuation_cj_bytes
    }

    pub(crate) const fn max_segment_continuation_cj_bytes_v2(&self) -> u64 {
        self.max_segment_continuation_cj_bytes
    }

    pub(crate) const fn max_trained_segment_manifest_cj_bytes_v2(&self) -> u64 {
        self.max_trained_segment_manifest_cj_bytes
    }

    pub(crate) const fn max_genesis_segment_manifest_cj_bytes_v2(&self) -> u64 {
        self.max_genesis_segment_manifest_cj_bytes
    }

    pub(crate) const fn max_checkpoint_manifest_cj_bytes_v2(&self) -> u64 {
        self.max_checkpoint_manifest_cj_bytes
    }

    pub(crate) const fn max_checkpoint_sidecar_cj_bytes_v2(&self) -> u64 {
        self.max_checkpoint_sidecar_cj_bytes
    }

    pub(crate) const fn max_head_record_cj_bytes_v2(&self) -> u64 {
        self.max_head_record_cj_bytes
    }

    pub(crate) const fn max_checkpoint_reference_cj_bytes_v2(&self) -> u64 {
        self.max_checkpoint_reference_cj_bytes
    }

    pub(crate) const fn max_latest_record_cj_bytes_v2(&self) -> u64 {
        self.max_latest_record_cj_bytes
    }

    pub(crate) const fn max_publication_plan_entries_v2(&self) -> u64 {
        self.max_publication_plan_entries
    }

    pub(crate) const fn max_immutable_final_entries_v2(&self) -> u64 {
        self.max_immutable_final_entries
    }

    pub(crate) const fn max_runtime_allocation_product_bytes_v2(&self) -> u64 {
        self.max_runtime_allocation_product_bytes
    }
}

#[derive(Clone, Copy)]
struct PlanInputsV2 {
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    batch_episodes: u64,
    checkpoint_segment_updates: u64,
    requested_successful_updates: u64,
    parent_generation_index: u64,
    parent_segment_ordinal: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
}

#[derive(Clone, Copy)]
struct PlannerLimitsV2 {
    max_logical_rows: u64,
    max_continuation_bytes: u64,
    max_segment_manifest_bytes: u64,
    max_fixed_decimal: u64,
    max_runtime_usize: u64,
}

impl PlannerLimitsV2 {
    fn production_v2() -> Self {
        Self {
            max_logical_rows: SEGMENT_CONTINUATION_MAX_LOGICAL_ROWS_V2,
            max_continuation_bytes: SEGMENT_CONTINUATION_MAX_BYTES_V2,
            max_segment_manifest_bytes: SEGMENT_MANIFEST_MAX_BYTES_V2,
            max_fixed_decimal: SEGMENT_CONTINUATION_MAX_FIXED_DECIMAL_V2,
            max_runtime_usize: u64::try_from(usize::MAX).unwrap_or(u64::MAX),
        }
    }
}

pub(crate) fn plan_trained_segment_representability_v2(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
) -> Result<SegmentRepresentabilityPlanV2> {
    let parent_facts = parent.boundary_facts_v2();
    if parent_facts.run_sha256 != run.run_sha256()
        || parent_facts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || parent_facts.batch_episodes != run.batch_episodes()
        || parent_facts.checkpoint_segment_updates != run.checkpoint_segment_updates()
    {
        return Err(input_error_v2());
    }
    let run_sha256 = parse_lower_hex_raw32_v1(run.run_sha256()).map_err(|_| input_error_v2())?;
    let identity_bundle_sha256 =
        parse_lower_hex_raw32_v1(run.identity_bundle_sha256()).map_err(|_| input_error_v2())?;
    plan_from_inputs_v2(
        PlanInputsV2 {
            run_sha256,
            identity_bundle_sha256,
            batch_episodes: run.batch_episodes(),
            checkpoint_segment_updates: run.checkpoint_segment_updates(),
            requested_successful_updates: run.requested_successful_updates(),
            parent_generation_index: parent_facts.generation_index,
            parent_segment_ordinal: parent_facts.segment_ordinal,
            max_physical_decisions: run.record().limits().max_physical_decisions(),
            max_policy_steps: run.record().limits().max_policy_steps(),
        },
        PlannerLimitsV2::production_v2(),
    )
}

fn plan_from_inputs_v2(
    input: PlanInputsV2,
    limits: PlannerLimitsV2,
) -> Result<SegmentRepresentabilityPlanV2> {
    let k = input.batch_episodes;
    let s = input.checkpoint_segment_updates;
    let n = input.requested_successful_updates;
    let p = input.parent_generation_index;
    if k == 0 || s == 0 || n == 0 || p >= n || !p.is_multiple_of(s) {
        return Err(bound_error_v2());
    }

    let generation_index = checked_u63_add_v2(p, s)?;
    let segment_ordinal = checked_u63_add_v2(input.parent_segment_ordinal, 1)?;
    if generation_index > n
        || generation_index != checked_u63_mul_v2(segment_ordinal, s)?
        || s > limits.max_fixed_decimal
        || n > limits.max_fixed_decimal
        || p > limits.max_fixed_decimal
        || generation_index > limits.max_fixed_decimal
        || segment_ordinal > limits.max_fixed_decimal
        || s.checked_sub(1).ok_or_else(bound_error_v2)? > limits.max_fixed_decimal
    {
        return Err(bound_error_v2());
    }

    let episode_count = checked_u63_mul_v2(k, s)?;
    let total_episode_budget = checked_u63_mul_v2(k, n)?;
    let episode_start = checked_u63_mul_v2(k, p)?;
    let episode_end_exclusive = checked_u63_mul_v2(k, generation_index)?;
    if checked_u63_add_v2(episode_start, episode_count)? != episode_end_exclusive {
        return Err(bound_error_v2());
    }
    let update_start_index = checked_u63_add_v2(p, 1)?;
    let next_update_index = checked_u63_add_v2(generation_index, 1)?;
    let max_publication_plan_entries = checked_u63_add_v2(s, 7)?;
    let max_immutable_final_entries = checked_u63_add_v2(s, 6)?;
    let max_physical_terms = checked_u63_mul_v2(k, input.max_physical_decisions)?;
    let max_gauge_bounds = checked_u63_mul_v2(k, input.max_policy_steps)?;
    let max_physical_terms_per_segment = checked_u63_mul_v2(s, max_physical_terms)?;
    let max_gauge_bounds_per_segment = checked_u63_mul_v2(s, max_gauge_bounds)?;
    let max_logical_rows_per_update = checked_u63_add_v2(
        checked_u63_add_v2(checked_u63_add_v2(1, k)?, max_physical_terms)?,
        max_gauge_bounds,
    )?;
    if max_logical_rows_per_update > limits.max_logical_rows {
        return Err(bound_error_v2());
    }
    let max_logical_rows_per_segment = checked_u63_mul_v2(s, max_logical_rows_per_update)?;

    let update_group_shape =
        maximum_update_group_json_shape_v2(k, max_physical_terms, max_gauge_bounds)
            .map_err(map_closed_grammar_error_v2)?;
    let max_update_group_json_token_bytes = update_group_shape.token_bytes();
    let max_segment_update_group_json_token_bytes =
        checked_u64_mul_v2(s, max_update_group_json_token_bytes)?;
    let max_update_group_cj_bytes = update_group_shape
        .canonical_document_bytes_v1()
        .map_err(map_closed_grammar_error_v2)?;
    let max_segment_update_group_cj_bytes = checked_u64_mul_v2(s, max_update_group_cj_bytes)?;
    let max_one_group_continuation_cj_bytes =
        maximum_one_group_continuation_cj_bytes_v2(k, max_physical_terms, max_gauge_bounds)
            .map_err(map_closed_grammar_error_v2)?;
    let max_segment_continuation_cj_bytes =
        checked_u64_mul_v2(s, max_one_group_continuation_cj_bytes)?;
    let max_trained_segment_manifest_cj_bytes =
        maximum_trained_segment_manifest_cj_bytes_v2(s).map_err(map_closed_grammar_error_v2)?;
    let max_genesis_segment_manifest_cj_bytes =
        maximum_genesis_segment_manifest_cj_bytes_v2().map_err(map_closed_grammar_error_v2)?;
    let max_checkpoint_manifest_cj_bytes =
        maximum_checkpoint_manifest_cj_bytes_v3().map_err(map_closed_grammar_error_v2)?;
    let max_checkpoint_sidecar_cj_bytes =
        maximum_trained_checkpoint_sidecar_cj_bytes_v2().map_err(map_closed_grammar_error_v2)?;
    let max_head_record_cj_bytes =
        maximum_trained_head_record_cj_bytes_v2().map_err(map_closed_grammar_error_v2)?;
    let max_checkpoint_reference_cj_bytes =
        maximum_trained_checkpoint_reference_cj_bytes_v2().map_err(map_closed_grammar_error_v2)?;
    let max_latest_record_cj_bytes =
        maximum_latest_record_cj_bytes_v2().map_err(map_closed_grammar_error_v2)?;

    if max_one_group_continuation_cj_bytes > limits.max_continuation_bytes
        || max_trained_segment_manifest_cj_bytes > limits.max_segment_manifest_bytes
        || max_genesis_segment_manifest_cj_bytes > limits.max_segment_manifest_bytes
        || max_checkpoint_manifest_cj_bytes
            > u64::try_from(CHECKPOINT_MANIFEST_MAX_BYTES_V3).map_err(|_| bound_error_v2())?
        || max_checkpoint_sidecar_cj_bytes > CHECKPOINT_SIDECAR_MAX_BYTES_V2
        || max_head_record_cj_bytes > HEAD_RECORD_MAX_BYTES_V2
        || max_checkpoint_reference_cj_bytes > CHECKPOINT_REFERENCE_MAX_BYTES_V2
        || max_latest_record_cj_bytes > LATEST_RECORD_MAX_BYTES_V2
    {
        return Err(bound_error_v2());
    }

    let payload_bytes =
        u64::try_from(NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1).map_err(|_| bound_error_v2())?;
    let values_requiring_usize = [
        k,
        s,
        n,
        p,
        generation_index,
        segment_ordinal,
        update_start_index,
        next_update_index,
        episode_start,
        episode_count,
        episode_end_exclusive,
        total_episode_budget,
        input.max_physical_decisions,
        input.max_policy_steps,
        max_physical_terms,
        max_gauge_bounds,
        max_physical_terms_per_segment,
        max_gauge_bounds_per_segment,
        max_logical_rows_per_update,
        max_logical_rows_per_segment,
        max_update_group_json_token_bytes,
        max_segment_update_group_json_token_bytes,
        max_update_group_cj_bytes,
        max_segment_update_group_cj_bytes,
        max_one_group_continuation_cj_bytes,
        max_segment_continuation_cj_bytes,
        max_trained_segment_manifest_cj_bytes,
        max_genesis_segment_manifest_cj_bytes,
        max_checkpoint_manifest_cj_bytes,
        max_checkpoint_sidecar_cj_bytes,
        max_head_record_cj_bytes,
        max_checkpoint_reference_cj_bytes,
        max_latest_record_cj_bytes,
        max_publication_plan_entries,
        max_immutable_final_entries,
        payload_bytes,
    ];
    for value in values_requiring_usize {
        require_exact_usize_v2(value, limits.max_runtime_usize)?;
    }

    let segment_update_capacity = require_exact_usize_v2(s, limits.max_runtime_usize)?;
    let batch_episode_capacity = require_exact_usize_v2(k, limits.max_runtime_usize)?;
    let max_physical_term_capacity =
        require_exact_usize_v2(max_physical_terms, limits.max_runtime_usize)?;
    let max_gauge_bound_capacity =
        require_exact_usize_v2(max_gauge_bounds, limits.max_runtime_usize)?;
    let max_continuation_capacity = require_exact_usize_v2(s, limits.max_runtime_usize)?;
    let retained_episode_capacity =
        require_exact_usize_v2(episode_count, limits.max_runtime_usize)?;
    let retained_physical_term_capacity =
        require_exact_usize_v2(max_physical_terms_per_segment, limits.max_runtime_usize)?;
    let retained_gauge_bound_capacity =
        require_exact_usize_v2(max_gauge_bounds_per_segment, limits.max_runtime_usize)?;
    let mut max_runtime_allocation_product_bytes = 0_u64;

    for allocation_bytes in [
        require_layout_v2::<ValidatedUpdateGroupV1>(
            segment_update_capacity,
            limits.max_runtime_usize,
        )?,
        require_layout_v2::<NativeTrainingEpisodeObservationV1>(
            batch_episode_capacity,
            limits.max_runtime_usize,
        )?,
        require_layout_v2::<NativeTrainingPhysicalTermObservationV1>(
            max_physical_term_capacity,
            limits.max_runtime_usize,
        )?,
        require_layout_v2::<NativeTrainingSelectedOutputObservationV1>(
            max_gauge_bound_capacity,
            limits.max_runtime_usize,
        )?,
        require_layout_v2::<NativeTrainingGaugeSubstepObservationV1>(
            max_gauge_bound_capacity,
            limits.max_runtime_usize,
        )?,
    ] {
        max_runtime_allocation_product_bytes =
            max_runtime_allocation_product_bytes.max(allocation_bytes);
    }
    for allocation_bytes in update_group_allocation_layout_bytes_v2(
        retained_episode_capacity,
        retained_physical_term_capacity,
        retained_gauge_bound_capacity,
        max_physical_term_capacity,
    )
    .ok_or_else(bound_error_v2)?
    .into_iter()
    .chain(
        segment_continuation_allocation_layout_bytes_v2(
            segment_update_capacity,
            max_continuation_capacity,
        )
        .ok_or_else(bound_error_v2)?,
    )
    .chain(
        segment_manifest_allocation_layout_bytes_v2(segment_update_capacity)
            .ok_or_else(bound_error_v2)?,
    ) {
        require_exact_usize_v2(allocation_bytes, limits.max_runtime_usize)?;
        max_runtime_allocation_product_bytes =
            max_runtime_allocation_product_bytes.max(allocation_bytes);
    }
    let max_canonical_json_token_bytes = [
        max_update_group_json_token_bytes,
        max_one_group_continuation_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_trained_segment_manifest_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_genesis_segment_manifest_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_checkpoint_manifest_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_checkpoint_sidecar_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_head_record_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_checkpoint_reference_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
        max_latest_record_cj_bytes
            .checked_sub(1)
            .ok_or_else(bound_error_v2)?,
    ]
    .into_iter()
    .max()
    .ok_or_else(bound_error_v2)?;
    let canonical_node_capacity =
        require_exact_usize_v2(max_canonical_json_token_bytes, limits.max_runtime_usize)?;
    for allocation_bytes in canonical_json_tree_allocation_layout_bytes_v1(canonical_node_capacity)
        .ok_or_else(bound_error_v2)?
    {
        require_exact_usize_v2(allocation_bytes, limits.max_runtime_usize)?;
        max_runtime_allocation_product_bytes =
            max_runtime_allocation_product_bytes.max(allocation_bytes);
    }
    for byte_count in [
        max_segment_update_group_json_token_bytes,
        max_update_group_cj_bytes,
        max_segment_update_group_cj_bytes,
        max_one_group_continuation_cj_bytes,
        max_segment_continuation_cj_bytes,
        max_trained_segment_manifest_cj_bytes,
        max_genesis_segment_manifest_cj_bytes,
        max_checkpoint_manifest_cj_bytes,
        max_checkpoint_sidecar_cj_bytes,
        max_head_record_cj_bytes,
        max_checkpoint_reference_cj_bytes,
        max_latest_record_cj_bytes,
        payload_bytes,
    ] {
        let allocation_bytes = require_layout_v2::<u8>(
            require_exact_usize_v2(byte_count, limits.max_runtime_usize)?,
            limits.max_runtime_usize,
        )?;
        max_runtime_allocation_product_bytes =
            max_runtime_allocation_product_bytes.max(allocation_bytes);
    }

    Ok(SegmentRepresentabilityPlanV2 {
        run_sha256: input.run_sha256,
        identity_bundle_sha256: input.identity_bundle_sha256,
        batch_episodes: k,
        checkpoint_segment_updates: s,
        requested_successful_updates: n,
        max_physical_decisions: input.max_physical_decisions,
        max_policy_steps: input.max_policy_steps,
        parent_generation_index: p,
        generation_index,
        segment_ordinal,
        update_start_index,
        episode_start,
        episode_count,
        episode_end_exclusive,
        total_episode_budget,
        max_physical_terms,
        max_gauge_bounds,
        max_physical_terms_per_segment,
        max_gauge_bounds_per_segment,
        max_logical_rows_per_update,
        max_logical_rows_per_segment,
        segment_update_capacity,
        batch_episode_capacity,
        max_physical_term_capacity,
        max_gauge_bound_capacity,
        max_continuation_capacity,
        max_update_group_json_token_bytes,
        max_segment_update_group_json_token_bytes,
        max_update_group_cj_bytes,
        max_segment_update_group_cj_bytes,
        max_one_group_continuation_cj_bytes,
        max_segment_continuation_cj_bytes,
        max_trained_segment_manifest_cj_bytes,
        max_genesis_segment_manifest_cj_bytes,
        max_checkpoint_manifest_cj_bytes,
        max_checkpoint_sidecar_cj_bytes,
        max_head_record_cj_bytes,
        max_checkpoint_reference_cj_bytes,
        max_latest_record_cj_bytes,
        max_publication_plan_entries,
        max_immutable_final_entries,
        max_runtime_allocation_product_bytes,
    })
}

fn checked_u63_add_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(bound_error_v2)
}

fn checked_u63_mul_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right)
        .filter(|value| *value <= U63_MAX_V2)
        .ok_or_else(bound_error_v2)
}

fn checked_u64_mul_v2(left: u64, right: u64) -> Result<u64> {
    left.checked_mul(right).ok_or_else(bound_error_v2)
}

fn require_exact_usize_v2(value: u64, max_runtime_usize: u64) -> Result<usize> {
    if value > max_runtime_usize {
        return Err(bound_error_v2());
    }
    let converted = usize::try_from(value).map_err(|_| bound_error_v2())?;
    if u64::try_from(converted).ok() != Some(value) {
        return Err(bound_error_v2());
    }
    Ok(converted)
}

fn require_layout_v2<T>(count: usize, max_runtime_usize: u64) -> Result<u64> {
    let layout = Layout::array::<T>(count).map_err(|_| bound_error_v2())?;
    let byte_count = u64::try_from(layout.size()).map_err(|_| bound_error_v2())?;
    require_exact_usize_v2(byte_count, max_runtime_usize)?;
    Ok(byte_count)
}

fn map_closed_grammar_error_v2(
    _: CanonicalJsonClosedMaxErrorV1,
) -> SegmentRepresentabilityPlanErrorV2 {
    bound_error_v2()
}

const fn input_error_v2() -> SegmentRepresentabilityPlanErrorV2 {
    SegmentRepresentabilityPlanErrorV2::new(SegmentRepresentabilityPlanErrorKindV2::InputBinding)
}

const fn bound_error_v2() -> SegmentRepresentabilityPlanErrorV2 {
    SegmentRepresentabilityPlanErrorV2::new(SegmentRepresentabilityPlanErrorKindV2::BoundExceeded)
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
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_v2, test_fixture_bytes_with_base_seed_v2,
    };
    use crate::native_training_store_segment_continuation_v2::build_segment_continuations_v2;
    use crate::native_training_store_segment_manifest_v2::{
        build_genesis_segment_manifest_v2, build_trained_segment_manifest_v2,
    };
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1,
    };
    use std::sync::OnceLock;
    use std::time::Duration;

    struct SealedPlannerFixtureV2 {
        run: ValidatedTrainRunV2,
        genesis_boundary: ValidatedNativeTrainingBoundaryV2,
        trained_boundary: ValidatedNativeTrainingBoundaryV2,
    }

    struct AlternateGenesisFixtureV2 {
        run: ValidatedTrainRunV2,
        boundary: ValidatedNativeTrainingBoundaryV2,
    }

    static SEALED_FIXTURE_V2: OnceLock<SealedPlannerFixtureV2> = OnceLock::new();
    static ALTERNATE_FIXTURE_V2: OnceLock<AlternateGenesisFixtureV2> = OnceLock::new();

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
        let (manifest, payload) = common_model_snapshot_paths_v1();
        NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v2(run),
            &manifest,
            &payload,
        )
        .unwrap()
    }

    fn sealed_fixture_v2() -> &'static SealedPlannerFixtureV2 {
        SEALED_FIXTURE_V2.get_or_init(|| {
            let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
            let mut executor = fresh_executor_v2(&run);
            let genesis_candidate = executor.checkpoint_candidate_v1().unwrap();
            let genesis_checkpoint =
                build_genesis_checkpoint_manifest_v3(&run, genesis_candidate.payload()).unwrap();
            let genesis_segment =
                build_genesis_segment_manifest_v2(&run, &genesis_checkpoint).unwrap();
            let genesis_boundary = build_genesis_native_training_boundary_v2(
                &run,
                &genesis_segment,
                &genesis_checkpoint,
            )
            .unwrap();

            let continuation_context =
                begin_update_evidence_chain_v1(&run, &genesis_checkpoint).unwrap();
            let mut group_context =
                begin_update_evidence_chain_v1(&run, &genesis_checkpoint).unwrap();
            let mut groups = Vec::with_capacity(4);
            let mut final_candidate = None;
            for ordinal in 0..4 {
                let prepared = executor.prepare_update_v2().unwrap();
                let advance = build_update_group_v1(&run, group_context, &prepared).unwrap();
                if ordinal == 3 {
                    final_candidate = Some(prepared.checkpoint_candidate().clone());
                }
                let (group, advanced) = advance.into_parts();
                groups.push(group);
                group_context = advanced;
                drop(prepared);
                if ordinal < 3 {
                    executor.run_update_v2().unwrap();
                }
            }
            let continuations =
                build_segment_continuations_v2(&run, continuation_context, groups).unwrap();
            assert_eq!(
                continuations.advanced_context().progress(),
                group_context.progress()
            );
            let trained_checkpoint = build_trained_checkpoint_manifest_v3(
                &run,
                continuations.advanced_context(),
                final_candidate.as_ref().unwrap(),
            )
            .unwrap();
            let trained_segment = build_trained_segment_manifest_v2(
                &run,
                &genesis_boundary,
                &continuations,
                &trained_checkpoint,
            )
            .unwrap();
            let trained_boundary = build_trained_native_training_boundary_v2(
                &run,
                &genesis_boundary,
                &trained_segment,
                &trained_checkpoint,
            )
            .unwrap();
            SealedPlannerFixtureV2 {
                run,
                genesis_boundary,
                trained_boundary,
            }
        })
    }

    fn alternate_fixture_v2() -> &'static AlternateGenesisFixtureV2 {
        ALTERNATE_FIXTURE_V2.get_or_init(|| {
            let run = decode_train_run_v2(&test_fixture_bytes_with_base_seed_v2(71_502)).unwrap();
            let executor = fresh_executor_v2(&run);
            let candidate = executor.checkpoint_candidate_v1().unwrap();
            let checkpoint =
                build_genesis_checkpoint_manifest_v3(&run, candidate.payload()).unwrap();
            let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
            let boundary =
                build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
            AlternateGenesisFixtureV2 { run, boundary }
        })
    }

    fn current_inputs_v2() -> PlanInputsV2 {
        PlanInputsV2 {
            run_sha256: [0x11; 32],
            identity_bundle_sha256: [0x22; 32],
            batch_episodes: 2,
            checkpoint_segment_updates: 4,
            requested_successful_updates: 8,
            parent_generation_index: 0,
            parent_segment_ordinal: 0,
            max_physical_decisions: 32_768,
            max_policy_steps: 65_536,
        }
    }

    fn assert_bound_error_v2(result: Result<SegmentRepresentabilityPlanV2>) {
        assert_eq!(
            result.unwrap_err().kind(),
            SegmentRepresentabilityPlanErrorKindV2::BoundExceeded
        );
    }

    #[test]
    fn sealed_entrypoint_derives_genesis_and_trained_windows_and_rejects_cross_run_parent() {
        let fixture = sealed_fixture_v2();
        let genesis =
            plan_trained_segment_representability_v2(&fixture.run, &fixture.genesis_boundary)
                .unwrap();
        assert_eq!(
            genesis.run_sha256_v2(),
            parse_lower_hex_raw32_v1(fixture.run.run_sha256()).unwrap()
        );
        assert_eq!(
            genesis.identity_bundle_sha256_v2(),
            parse_lower_hex_raw32_v1(fixture.run.identity_bundle_sha256()).unwrap()
        );
        assert_eq!(genesis.parent_generation_index_v2(), 0);
        assert_eq!(genesis.generation_index_v2(), 4);
        assert_eq!(genesis.segment_ordinal_v2(), 1);

        let trained =
            plan_trained_segment_representability_v2(&fixture.run, &fixture.trained_boundary)
                .unwrap();
        assert_eq!(trained.parent_generation_index_v2(), 4);
        assert_eq!(trained.generation_index_v2(), 8);
        assert_eq!(trained.segment_ordinal_v2(), 2);

        let alternate = alternate_fixture_v2();
        assert_eq!(
            plan_trained_segment_representability_v2(&fixture.run, &alternate.boundary)
                .unwrap_err()
                .kind(),
            SegmentRepresentabilityPlanErrorKindV2::InputBinding
        );
        assert_eq!(
            plan_trained_segment_representability_v2(&alternate.run, &fixture.genesis_boundary)
                .unwrap_err()
                .kind(),
            SegmentRepresentabilityPlanErrorKindV2::InputBinding
        );
    }

    #[test]
    fn current_fixture_and_resumed_window_have_exact_golden_plan() {
        let limits = PlannerLimitsV2::production_v2();
        let first = plan_from_inputs_v2(current_inputs_v2(), limits).unwrap();
        assert_eq!(first.parent_generation_index_v2(), 0);
        assert_eq!(first.generation_index_v2(), 4);
        assert_eq!(first.segment_ordinal_v2(), 1);
        assert_eq!(first.max_physical_decisions_v2(), 32_768);
        assert_eq!(first.max_policy_steps_v2(), 65_536);
        assert_eq!(first.episode_start_v2(), 0);
        assert_eq!(first.episode_count_v2(), 8);
        assert_eq!(first.episode_end_exclusive_v2(), 8);
        assert_eq!(first.max_physical_terms_v2(), 65_536);
        assert_eq!(first.max_gauge_bounds_v2(), 131_072);
        assert_eq!(first.max_physical_terms_per_segment_v2(), 262_144);
        assert_eq!(first.max_gauge_bounds_per_segment_v2(), 524_288);
        assert_eq!(first.max_logical_rows_per_update_v2(), 196_611);
        assert_eq!(first.max_update_group_json_token_bytes_v2(), 36_508_556);
        assert_eq!(
            first.max_segment_update_group_json_token_bytes_v2(),
            146_034_224
        );
        assert_eq!(first.max_update_group_cj_bytes_v2(), 36_508_557);
        assert_eq!(first.max_segment_update_group_cj_bytes_v2(), 146_034_228);
        assert_eq!(first.max_one_group_continuation_cj_bytes_v2(), 36_509_286);
        assert_eq!(first.max_segment_continuation_cj_bytes_v2(), 146_037_144);
        assert_eq!(first.max_trained_segment_manifest_cj_bytes_v2(), 4_144);
        assert_eq!(first.max_genesis_segment_manifest_cj_bytes_v2(), 1_401);
        assert_eq!(first.max_checkpoint_manifest_cj_bytes_v2(), 2_204);
        assert_eq!(first.max_checkpoint_sidecar_cj_bytes_v2(), 1_133);
        assert_eq!(first.max_head_record_cj_bytes_v2(), 1_295);
        assert_eq!(first.max_checkpoint_reference_cj_bytes_v2(), 1_539);
        assert_eq!(first.max_latest_record_cj_bytes_v2(), 567);
        assert_eq!(first.max_publication_plan_entries_v2(), 11);
        assert_eq!(first.max_immutable_final_entries_v2(), 10);
        assert!(
            first.max_runtime_allocation_product_bytes_v2()
                >= first.max_segment_continuation_cj_bytes_v2()
        );

        let mut resumed = current_inputs_v2();
        resumed.parent_generation_index = 4;
        resumed.parent_segment_ordinal = 1;
        let resumed = plan_from_inputs_v2(resumed, limits).unwrap();
        assert_eq!(resumed.parent_generation_index_v2(), 4);
        assert_eq!(resumed.generation_index_v2(), 8);
        assert_eq!(resumed.segment_ordinal_v2(), 2);
        assert_eq!(resumed.episode_start_v2(), 8);
        assert_eq!(resumed.episode_end_exclusive_v2(), 16);
    }

    #[test]
    fn row_continuation_and_manifest_boundaries_fail_one_step_late() {
        let production = PlannerLimitsV2::production_v2();
        let mut row = current_inputs_v2();
        row.batch_episodes = 1;
        row.checkpoint_segment_updates = 1;
        row.requested_successful_updates = 1;
        row.max_physical_decisions = 131_071;
        row.max_policy_steps = 131_071;
        let accepted = plan_from_inputs_v2(row, production).unwrap();
        assert_eq!(accepted.max_logical_rows_per_update_v2(), 262_144);
        row.max_physical_decisions += 1;
        assert_bound_error_v2(plan_from_inputs_v2(row, production));

        let current = plan_from_inputs_v2(current_inputs_v2(), production).unwrap();
        let exact_continuation = current.max_one_group_continuation_cj_bytes_v2();
        let mut reduced = production;
        reduced.max_continuation_bytes = exact_continuation;
        assert!(plan_from_inputs_v2(current_inputs_v2(), reduced).is_ok());
        reduced.max_continuation_bytes -= 1;
        assert_bound_error_v2(plan_from_inputs_v2(current_inputs_v2(), reduced));

        let mut manifest = current_inputs_v2();
        manifest.batch_episodes = 2;
        manifest.checkpoint_segment_updates = 7_342;
        manifest.requested_successful_updates = 7_342;
        manifest.max_physical_decisions = 1;
        manifest.max_policy_steps = 1;
        let accepted = plan_from_inputs_v2(manifest, production).unwrap();
        assert_eq!(
            accepted.max_trained_segment_manifest_cj_bytes_v2(),
            4_194_142
        );
        manifest.checkpoint_segment_updates = 7_343;
        manifest.requested_successful_updates = 7_343;
        assert_bound_error_v2(plan_from_inputs_v2(manifest, production));
    }

    #[test]
    fn basename_arithmetic_and_runtime_usize_boundaries_fail_closed() {
        let production = PlannerLimitsV2::production_v2();
        let mut basename = current_inputs_v2();
        basename.checkpoint_segment_updates = 1;
        basename.requested_successful_updates = 99_999_999;
        basename.parent_generation_index = 99_999_998;
        basename.parent_segment_ordinal = 99_999_998;
        basename.max_physical_decisions = 1;
        basename.max_policy_steps = 1;
        assert!(plan_from_inputs_v2(basename, production).is_ok());
        basename.requested_successful_updates = 100_000_000;
        basename.parent_generation_index = 99_999_999;
        basename.parent_segment_ordinal = 99_999_999;
        assert_bound_error_v2(plan_from_inputs_v2(basename, production));

        let mut overflow = current_inputs_v2();
        overflow.batch_episodes = u64::MAX;
        assert_bound_error_v2(plan_from_inputs_v2(overflow, production));

        assert_eq!(checked_u63_add_v2(U63_MAX_V2, 0).unwrap(), U63_MAX_V2);
        assert_eq!(
            checked_u63_add_v2(U63_MAX_V2, 1).unwrap_err().kind(),
            SegmentRepresentabilityPlanErrorKindV2::BoundExceeded
        );
        assert_eq!(checked_u63_mul_v2(U63_MAX_V2, 1).unwrap(), U63_MAX_V2);
        assert_eq!(
            checked_u63_mul_v2(U63_MAX_V2, 2).unwrap_err().kind(),
            SegmentRepresentabilityPlanErrorKindV2::BoundExceeded
        );

        let exact_allocation_limit = plan_from_inputs_v2(current_inputs_v2(), production)
            .unwrap()
            .max_runtime_allocation_product_bytes_v2();
        let mut narrow = production;
        narrow.max_runtime_usize = exact_allocation_limit - 1;
        assert_bound_error_v2(plan_from_inputs_v2(current_inputs_v2(), narrow));
        narrow.max_runtime_usize = exact_allocation_limit;
        assert!(plan_from_inputs_v2(current_inputs_v2(), narrow).is_ok());
    }
}
