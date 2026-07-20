//! One-clone, one-final-payload preparation authority for a trained Store V2
//! segment.
//!
//! Preparation validates the exact live parent and all representability bounds
//! before cloning the trainer. The returned move-only guard owns every byte a
//! later publisher needs, while the exclusively borrowed live executor remains
//! unchanged until an independently constructed persistence receipt is
//! consumed by `commit_v2`.

#[cfg(test)]
use crate::native_training_executor_v1::NativeTrainingIntrinsicFactMutationForTestV2;
use crate::native_training_executor_v1::{
    checkpoint_matches_intrinsic_facts_v2, NativeTrainingCheckpointCandidateV1,
    NativeTrainingExecutorV1, NativeTrainingIntrinsicCheckpointFactsV2,
    NativeTrainingNumericalBackendV1, NativeTrainingSegmentCandidateV2,
};
use crate::native_training_store_boundary_v2::{
    build_trained_native_training_boundary_v2, ValidatedNativeTrainingBoundaryV2,
};
use crate::native_training_store_checkpoint_v3::{
    build_trained_checkpoint_manifest_v3, CheckpointManifestV3, CheckpointProgressV3,
};
use crate::native_training_store_digest_v1::sha256_v1;
use crate::native_training_store_reference_latest_v2::{
    build_checkpoint_reference_v2, build_latest_v2, ValidatedCheckpointReferenceV2,
    ValidatedLatestRecordV2, CHECKPOINT_REFERENCE_MAX_BYTES_V2, LATEST_RECORD_MAX_BYTES_V2,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_continuation_v2::{
    build_segment_continuations_v2, ValidatedSegmentContinuationChainAdvanceV2,
    SEGMENT_CONTINUATION_MAX_BYTES_V2,
};
use crate::native_training_store_segment_manifest_v2::{
    build_trained_segment_manifest_v2, SegmentManifestV2, SEGMENT_MANIFEST_MAX_BYTES_V2,
};
use crate::native_training_store_segment_representability_v2::{
    plan_trained_segment_representability_v2, SegmentRepresentabilityPlanErrorKindV2,
    SegmentRepresentabilityPlanV2,
};
use crate::native_training_store_update_group_v1::{
    build_compact_update_group_v2, resume_update_evidence_chain_v1,
    validate_prepared_execution_config_v1, UpdateEvidenceChainContextV1, ValidatedUpdateGroupV1,
};
use crate::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
use std::error::Error;
use std::fmt::{Display, Formatter};

const U63_MAX_V2: u64 = (1_u64 << 63) - 1;

/// Stable, input-independent failure classes for prepared segment creation and
/// receipt-gated commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingPreparedSegmentV2ErrorKind {
    InputBindingInvalid,
    RepresentabilityBoundExceeded,
    ResourceExhausted,
    UpdateFailed,
    EvidenceInvalid,
    ArtifactInvalid,
    PersistenceReceiptMismatch,
}

impl NativeTrainingPreparedSegmentV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InputBindingInvalid => "native-training-segment-input-binding-invalid",
            Self::RepresentabilityBoundExceeded => "segment-representability-bound-exceeded",
            Self::ResourceExhausted => "native-training-segment-resource-exhausted",
            Self::UpdateFailed => "native-training-segment-update-failed",
            Self::EvidenceInvalid => "native-training-segment-evidence-invalid",
            Self::ArtifactInvalid => "native-training-segment-artifact-invalid",
            Self::PersistenceReceiptMismatch => "persistence_receipt_mismatch",
        }
    }
}

/// Redacted prepared-segment failure. No source value, path, digest, or nested
/// diagnostic is retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingPreparedSegmentV2Error {
    kind: NativeTrainingPreparedSegmentV2ErrorKind,
}

impl NativeTrainingPreparedSegmentV2Error {
    const fn new(kind: NativeTrainingPreparedSegmentV2ErrorKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> NativeTrainingPreparedSegmentV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeTrainingPreparedSegmentV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingPreparedSegmentV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingPreparedSegmentV2Error>;

struct NativeTrainingCompactSuccessorFactsV2 {
    intrinsic: NativeTrainingIntrinsicCheckpointFactsV2,
    run_sha256: [u8; 32],
    identity_bundle_sha256: [u8; 32],
    checkpoint_segment_updates: u64,
    progress: CheckpointProgressV3,
}

impl NativeTrainingCompactSuccessorFactsV2 {
    fn bind_v2(
        run: &ValidatedTrainRunV2,
        context: &UpdateEvidenceChainContextV1,
        intrinsic: NativeTrainingIntrinsicCheckpointFactsV2,
    ) -> Result<Self> {
        let progress = *context.progress();
        if !intrinsic_matches_context_v2(run, &intrinsic, context, &progress) {
            return Err(evidence_error_v2());
        }
        Ok(Self {
            intrinsic,
            run_sha256: context.run_sha256_raw_v1(),
            identity_bundle_sha256: context.identity_bundle_sha256_raw_v1(),
            checkpoint_segment_updates: context.checkpoint_segment_updates_v1(),
            progress,
        })
    }

    fn into_intrinsic_v2(self) -> NativeTrainingIntrinsicCheckpointFactsV2 {
        self.intrinsic
    }

    fn matches_context_v2(
        &self,
        run: &ValidatedTrainRunV2,
        context: &UpdateEvidenceChainContextV1,
    ) -> bool {
        self.run_sha256 == context.run_sha256_raw_v1()
            && self.identity_bundle_sha256 == context.identity_bundle_sha256_raw_v1()
            && self.checkpoint_segment_updates == context.checkpoint_segment_updates_v1()
            && self.progress == *context.progress()
            && intrinsic_matches_context_v2(run, &self.intrinsic, context, &self.progress)
    }
}

struct ExpectedPersistenceReceiptFactsV2 {
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
}

/// Move-only prepared segment. Its public surface deliberately exposes only
/// the parent generation, expected generation, and receipt-gated commit.
///
/// The guard cannot be cloned:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_prepared_segment_v2::NativeTrainingPreparedSegmentV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<NativeTrainingPreparedSegmentV2<'static>>();
/// ```
///
/// Its candidate is private, which also prevents external construction:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_prepared_segment_v2::NativeTrainingPreparedSegmentV2;
/// fn extract_candidate(prepared: NativeTrainingPreparedSegmentV2<'_>) {
///     let _ = prepared.candidate;
/// }
/// ```
///
/// It is neither serializable nor deserializable:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_prepared_segment_v2::NativeTrainingPreparedSegmentV2;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<NativeTrainingPreparedSegmentV2<'static>>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_prepared_segment_v2::NativeTrainingPreparedSegmentV2;
/// fn require_deserialize<'de, T: serde::Deserialize<'de>>() {}
/// fn probe_matching_lifetime<'de>() {
///     require_deserialize::<'de, NativeTrainingPreparedSegmentV2<'de>>();
/// }
/// ```
///
/// Crate-private publication bytes are not part of the public surface:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_prepared_segment_v2::NativeTrainingPreparedSegmentV2;
/// fn extract_publication(prepared: &NativeTrainingPreparedSegmentV2<'_>) {
///     let _ = prepared.publication_view_v2();
/// }
/// ```
///
/// A successful commit returns only `()`:
///
/// ```
/// use mtg_kernel::native_training_store_prepared_segment_v2::{
///     NativeTrainingPreparedSegmentV2, NativeTrainingPreparedSegmentV2Error,
/// };
/// use mtg_kernel::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
/// fn commit_result<'a>(
///     prepared: NativeTrainingPreparedSegmentV2<'a>,
///     receipt: NativeTrainingPersistenceReceiptV2,
/// ) -> Result<(), NativeTrainingPreparedSegmentV2Error> {
///     prepared.commit_v2(receipt)
/// }
/// ```
#[must_use = "dropping a prepared segment aborts it without advancing the executor"]
pub struct NativeTrainingPreparedSegmentV2<'executor> {
    candidate: NativeTrainingSegmentCandidateV2<'executor>,
    plan: SegmentRepresentabilityPlanV2,
    final_compact: NativeTrainingCompactSuccessorFactsV2,
    continuations: ValidatedSegmentContinuationChainAdvanceV2,
    final_checkpoint: NativeTrainingCheckpointCandidateV1,
    checkpoint_manifest: CheckpointManifestV3,
    segment_manifest: SegmentManifestV2,
    boundary: ValidatedNativeTrainingBoundaryV2,
    reference: ValidatedCheckpointReferenceV2,
    latest: ValidatedLatestRecordV2,
    expected_receipt: ExpectedPersistenceReceiptFactsV2,
}

impl NativeTrainingPreparedSegmentV2<'_> {
    pub const fn parent_generation_index(&self) -> u64 {
        self.plan.parent_generation_index_v2()
    }

    pub const fn expected_generation_index(&self) -> u64 {
        self.plan.generation_index_v2()
    }

    /// Performs the sole candidate-to-live assignment after exact receipt and
    /// retained-byte revalidation. A mismatch consumes and drops the guard,
    /// leaving the live executor untouched.
    pub fn commit_v2(self, receipt: NativeTrainingPersistenceReceiptV2) -> Result<()> {
        let expected_generation = self
            .plan
            .parent_generation_index_v2()
            .checked_add(self.plan.checkpoint_segment_updates_v2())
            .filter(|value| *value <= U63_MAX_V2)
            .ok_or_else(receipt_error_v2)?;
        let payload_sha256 = sha256_v1(self.final_checkpoint.payload());
        let manifest_sha256 = sha256_v1(self.checkpoint_manifest.canonical_bytes());
        if expected_generation != self.plan.generation_index_v2()
            || expected_generation != self.expected_receipt.generation_index
            || receipt.generation_index() != expected_generation
            || payload_sha256 != self.expected_receipt.checkpoint_payload_sha256
            || manifest_sha256 != self.expected_receipt.checkpoint_manifest_sha256
            || receipt.checkpoint_payload_sha256() != payload_sha256
            || receipt.checkpoint_manifest_sha256() != manifest_sha256
            || self.checkpoint_manifest.checkpoint_payload_sha256() != payload_sha256
            || self.checkpoint_manifest.checkpoint_manifest_sha256() != manifest_sha256
            || !checkpoint_matches_intrinsic_facts_v2(
                &self.final_checkpoint,
                &self.final_compact.intrinsic,
            )
        {
            return Err(receipt_error_v2());
        }
        let Self { candidate, .. } = self;
        candidate.install_infallibly_v2();
        Ok(())
    }

    pub(crate) const fn publication_view_v2(
        &self,
    ) -> NativeTrainingPreparedSegmentPublicationViewV2<'_, '_> {
        NativeTrainingPreparedSegmentPublicationViewV2 { prepared: self }
    }
}

/// Borrowed, allocation-free Store publisher projection. It is crate-private
/// so public callers cannot extract a candidate or bypass receipt construction.
pub(crate) struct NativeTrainingPreparedSegmentPublicationViewV2<'a, 'executor> {
    prepared: &'a NativeTrainingPreparedSegmentV2<'executor>,
}

#[allow(dead_code)]
impl NativeTrainingPreparedSegmentPublicationViewV2<'_, '_> {
    pub(crate) fn continuation_count_v2(&self) -> usize {
        self.prepared.continuations.chain().continuations().len()
    }

    pub(crate) fn continuation_canonical_bytes_v2(&self, index: usize) -> Option<&[u8]> {
        self.prepared
            .continuations
            .chain()
            .continuations()
            .get(index)
            .map(|continuation| continuation.canonical_bytes())
    }

    pub(crate) fn continuation_sha256_v2(&self, index: usize) -> Option<[u8; 32]> {
        self.prepared
            .continuations
            .chain()
            .continuations()
            .get(index)
            .map(|continuation| continuation.continuation_sha256())
    }

    pub(crate) fn checkpoint_payload_v2(&self) -> &[u8] {
        self.prepared.final_checkpoint.payload()
    }

    pub(crate) fn checkpoint_payload_sha256_v2(&self) -> [u8; 32] {
        self.prepared.expected_receipt.checkpoint_payload_sha256
    }

    pub(crate) fn checkpoint_manifest_canonical_bytes_v2(&self) -> &[u8] {
        self.prepared.checkpoint_manifest.canonical_bytes()
    }

    pub(crate) fn checkpoint_manifest_sha256_v2(&self) -> [u8; 32] {
        self.prepared.expected_receipt.checkpoint_manifest_sha256
    }

    pub(crate) fn segment_manifest_canonical_bytes_v2(&self) -> &[u8] {
        self.prepared.segment_manifest.canonical_bytes()
    }

    pub(crate) fn segment_manifest_sha256_v2(&self) -> [u8; 32] {
        self.prepared.segment_manifest.segment_manifest_sha256()
    }

    pub(crate) fn checkpoint_sidecar_canonical_bytes_v2(&self) -> &[u8] {
        self.prepared.boundary.checkpoint_sidecar_canonical_bytes()
    }

    pub(crate) fn checkpoint_sidecar_sha256_v2(&self) -> [u8; 32] {
        self.prepared.boundary.checkpoint_sidecar_sha256()
    }

    pub(crate) fn head_record_canonical_bytes_v2(&self) -> &[u8] {
        self.prepared.boundary.head_record_canonical_bytes()
    }

    pub(crate) fn head_record_sha256_v2(&self) -> [u8; 32] {
        self.prepared.boundary.head_record_sha256()
    }

    pub(crate) fn checkpoint_reference_canonical_bytes_v2(&self) -> &[u8] {
        self.prepared.reference.canonical_bytes()
    }

    pub(crate) fn checkpoint_reference_sha256_v2(&self) -> [u8; 32] {
        self.prepared.reference.checkpoint_ref_sha256()
    }

    pub(crate) fn latest_canonical_bytes_v2(&self) -> &[u8] {
        self.prepared.latest.canonical_bytes()
    }

    pub(crate) fn latest_sha256_v2(&self) -> [u8; 32] {
        self.prepared.latest.latest_record_sha256()
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTrainingCompactFactMutationForTestV2 {
    Intrinsic(NativeTrainingIntrinsicFactMutationForTestV2),
    RunSha256,
    IdentityBundleSha256,
    CheckpointSegmentUpdates,
    ProgressBatchEpisodes,
    ProgressCheckpointSegmentUpdates,
    ProgressNextEpisodeIndex,
    ProgressSuccessfulUpdateCount,
    ProgressCompletedEpisodeCount,
    ProgressP0Win,
    ProgressP0Loss,
    ProgressP0Draw,
    ProgressP1Win,
    ProgressP1Loss,
    ProgressP1Draw,
    ProgressPolicyP0,
    ProgressPolicyP1,
    ProgressPhysicalP0,
    ProgressPhysicalP1,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTrainingPreparedAbortPointForTestV2 {
    AfterClone,
    BeforeUpdate(usize),
    AfterUpdate(usize),
    BeforeContinuations,
    AfterContinuations,
    BeforeCheckpointManifest,
    AfterCheckpointManifest,
    BeforeSegmentManifest,
    AfterSegmentManifest,
    BeforeBoundary,
    AfterBoundary,
    BeforeReference,
    AfterReference,
    BeforeLatest,
    AfterLatest,
    BeforeFinalValidation,
    AfterFinalValidation,
    BeforeReceiptSeal,
    AfterReceiptSeal,
}

#[cfg(test)]
thread_local! {
    static COMPACT_MUTATION_FOR_TEST_V2: std::cell::Cell<Option<NativeTrainingCompactFactMutationForTestV2>> = const { std::cell::Cell::new(None) };
    static PREDECESSOR_MUTATION_FOR_TEST_V2: std::cell::Cell<Option<(usize, NativeTrainingIntrinsicFactMutationForTestV2)>> = const { std::cell::Cell::new(None) };
    static ABORT_POINT_FOR_TEST_V2: std::cell::Cell<Option<NativeTrainingPreparedAbortPointForTestV2>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn inject_compact_mutation_for_test_v2(
    mutation: Option<NativeTrainingCompactFactMutationForTestV2>,
) {
    COMPACT_MUTATION_FOR_TEST_V2.with(|cell| cell.set(mutation));
}

#[cfg(test)]
fn pending_compact_mutation_for_test_v2() -> Option<NativeTrainingCompactFactMutationForTestV2> {
    COMPACT_MUTATION_FOR_TEST_V2.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn inject_predecessor_mutation_for_test_v2(
    mutation: Option<(usize, NativeTrainingIntrinsicFactMutationForTestV2)>,
) {
    PREDECESSOR_MUTATION_FOR_TEST_V2.with(|cell| cell.set(mutation));
}

#[cfg(test)]
fn pending_predecessor_mutation_for_test_v2(
) -> Option<(usize, NativeTrainingIntrinsicFactMutationForTestV2)> {
    PREDECESSOR_MUTATION_FOR_TEST_V2.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn inject_abort_point_for_test_v2(
    point: Option<NativeTrainingPreparedAbortPointForTestV2>,
) {
    ABORT_POINT_FOR_TEST_V2.with(|cell| cell.set(point));
}

#[cfg(test)]
fn pending_abort_point_for_test_v2() -> Option<NativeTrainingPreparedAbortPointForTestV2> {
    ABORT_POINT_FOR_TEST_V2.with(std::cell::Cell::get)
}

#[cfg(test)]
fn mutate_compact_for_test_v2(compact: &mut NativeTrainingCompactSuccessorFactsV2) {
    let mutation = COMPACT_MUTATION_FOR_TEST_V2.with(|cell| cell.replace(None));
    let Some(mutation) = mutation else {
        return;
    };
    match mutation {
        NativeTrainingCompactFactMutationForTestV2::Intrinsic(mutation) => {
            compact.intrinsic.mutate_for_test_v2(mutation)
        }
        NativeTrainingCompactFactMutationForTestV2::RunSha256 => compact.run_sha256[0] ^= 1,
        NativeTrainingCompactFactMutationForTestV2::IdentityBundleSha256 => {
            compact.identity_bundle_sha256[0] ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::CheckpointSegmentUpdates => {
            compact.checkpoint_segment_updates ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressBatchEpisodes => {
            compact.progress.batch_episodes ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressCheckpointSegmentUpdates => {
            compact.progress.checkpoint_segment_updates ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressNextEpisodeIndex => {
            compact.progress.next_episode_index ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressSuccessfulUpdateCount => {
            compact.progress.successful_update_count ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressCompletedEpisodeCount => {
            compact.progress.completed_episode_count ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressP0Win => {
            compact.progress.outcomes_by_learner_seat.p0.win ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressP0Loss => {
            compact.progress.outcomes_by_learner_seat.p0.loss ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressP0Draw => {
            compact.progress.outcomes_by_learner_seat.p0.draw ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressP1Win => {
            compact.progress.outcomes_by_learner_seat.p1.win ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressP1Loss => {
            compact.progress.outcomes_by_learner_seat.p1.loss ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressP1Draw => {
            compact.progress.outcomes_by_learner_seat.p1.draw ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressPolicyP0 => {
            compact.progress.learner_policy_steps_by_seat.p0 ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressPolicyP1 => {
            compact.progress.learner_policy_steps_by_seat.p1 ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressPhysicalP0 => {
            compact.progress.learner_physical_decisions_by_seat.p0 ^= 1
        }
        NativeTrainingCompactFactMutationForTestV2::ProgressPhysicalP1 => {
            compact.progress.learner_physical_decisions_by_seat.p1 ^= 1
        }
    }
}

#[cfg(test)]
fn mutate_predecessor_for_test_v2(
    offset: usize,
    predecessor: &mut NativeTrainingIntrinsicCheckpointFactsV2,
) {
    PREDECESSOR_MUTATION_FOR_TEST_V2.with(|cell| {
        if let Some((target, mutation)) = cell.get() {
            if target == offset {
                cell.set(None);
                predecessor.mutate_for_test_v2(mutation);
            }
        }
    });
}

#[cfg(test)]
fn injected_abort_for_test_v2(point: NativeTrainingPreparedAbortPointForTestV2) -> Result<()> {
    let should_abort = ABORT_POINT_FOR_TEST_V2.with(|cell| {
        if cell.get() == Some(point) {
            cell.set(None);
            true
        } else {
            false
        }
    });
    if should_abort {
        match point {
            NativeTrainingPreparedAbortPointForTestV2::AfterClone
            | NativeTrainingPreparedAbortPointForTestV2::BeforeUpdate(_)
            | NativeTrainingPreparedAbortPointForTestV2::AfterUpdate(_) => Err(update_error_v2()),
            _ => Err(artifact_error_v2()),
        }
    } else {
        Ok(())
    }
}

/// Prepares exactly the next trained segment from one concrete parent without
/// changing the live executor.
pub fn prepare_segment_v2<'executor>(
    executor: &'executor mut NativeTrainingExecutorV1,
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    parent_checkpoint: &CheckpointManifestV3,
) -> Result<NativeTrainingPreparedSegmentV2<'executor>> {
    validate_prepared_execution_config_v1(run, executor.config()).map_err(|_| input_error_v2())?;
    let live = executor
        .intrinsic_checkpoint_facts_v2()
        .map_err(|_| input_error_v2())?;
    if !live_parent_matches_v2(run, parent, parent_checkpoint, &live) {
        return Err(input_error_v2());
    }
    let plan =
        plan_trained_segment_representability_v2(run, parent).map_err(|error| {
            match error.kind() {
                SegmentRepresentabilityPlanErrorKindV2::InputBinding => input_error_v2(),
                SegmentRepresentabilityPlanErrorKindV2::BoundExceeded => {
                    representability_error_v2()
                }
            }
        })?;
    let building_context = resume_update_evidence_chain_v1(run, parent, parent_checkpoint)
        .map_err(|_| input_error_v2())?;
    let continuation_context = resume_update_evidence_chain_v1(run, parent, parent_checkpoint)
        .map_err(|_| input_error_v2())?;
    let mut groups = reserve_update_groups_v2(plan.segment_update_capacity_v2())?;
    let mut candidate = executor
        .begin_segment_candidate_v2()
        .map_err(|_| update_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterClone)?;
    let mut predecessor = Some(live);
    let mut active_context = Some(building_context);
    let mut final_compact = None;
    let mut final_checkpoint = None;

    for offset in 0..plan.segment_update_capacity_v2() {
        let is_final = offset + 1 == plan.segment_update_capacity_v2();
        #[cfg(test)]
        injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeUpdate(
            offset,
        ))?;
        let expected_predecessor = predecessor.take().ok_or_else(evidence_error_v2)?;
        #[cfg(test)]
        let expected_predecessor = {
            let mut mutated = expected_predecessor;
            mutate_predecessor_for_test_v2(offset, &mut mutated);
            mutated
        };
        let transition = candidate
            .prepare_transition_v2(expected_predecessor, is_final)
            .map_err(|_| update_error_v2())?;
        #[cfg(test)]
        injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterUpdate(
            offset,
        ))?;
        let (advance, successor, checkpoint) = build_compact_update_group_v2(
            run,
            active_context.take().ok_or_else(evidence_error_v2)?,
            transition,
        )
        .map_err(|_| evidence_error_v2())?;
        let (group, advanced_context) = advance.into_parts();
        let compact =
            NativeTrainingCompactSuccessorFactsV2::bind_v2(run, &advanced_context, successor)?;
        #[cfg(test)]
        let compact = {
            let mut mutated = compact;
            mutate_compact_for_test_v2(&mut mutated);
            mutated
        };
        if !compact.matches_context_v2(run, &advanced_context) {
            return Err(evidence_error_v2());
        }
        groups.push(group);
        if is_final {
            final_checkpoint = Some(checkpoint.ok_or_else(update_error_v2)?);
            final_compact = Some(compact);
        } else {
            if checkpoint.is_some() {
                return Err(update_error_v2());
            }
            predecessor = Some(compact.into_intrinsic_v2());
            active_context = Some(advanced_context);
        }
    }
    if groups.len() != plan.segment_update_capacity_v2() {
        return Err(evidence_error_v2());
    }
    let final_compact = final_compact.ok_or_else(update_error_v2)?;
    let final_checkpoint = final_checkpoint.ok_or_else(update_error_v2)?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeContinuations)?;
    let continuations = build_segment_continuations_v2(run, continuation_context, groups)
        .map_err(|_| evidence_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterContinuations)?;
    if !final_compact.matches_context_v2(run, continuations.advanced_context())
        || !checkpoint_matches_intrinsic_facts_v2(&final_checkpoint, &final_compact.intrinsic)
        || continuations.chain().generation_index() != plan.generation_index_v2()
        || continuations.chain().segment_ordinal() != plan.segment_ordinal_v2()
        || continuations.chain().continuations().len() > plan.max_continuation_capacity_v2()
        || continuations
            .chain()
            .continuations()
            .iter()
            .any(|continuation| {
                u64::try_from(continuation.canonical_bytes().len())
                    .map_or(true, |count| count > SEGMENT_CONTINUATION_MAX_BYTES_V2)
            })
    {
        return Err(artifact_error_v2());
    }

    #[cfg(test)]
    injected_abort_for_test_v2(
        NativeTrainingPreparedAbortPointForTestV2::BeforeCheckpointManifest,
    )?;
    let checkpoint_manifest = build_trained_checkpoint_manifest_v3(
        run,
        continuations.advanced_context(),
        &final_checkpoint,
    )
    .map_err(|_| artifact_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterCheckpointManifest)?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeSegmentManifest)?;
    let segment_manifest =
        build_trained_segment_manifest_v2(run, parent, &continuations, &checkpoint_manifest)
            .map_err(|_| artifact_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterSegmentManifest)?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeBoundary)?;
    let boundary = build_trained_native_training_boundary_v2(
        run,
        parent,
        &segment_manifest,
        &checkpoint_manifest,
    )
    .map_err(|_| artifact_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterBoundary)?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeReference)?;
    let reference =
        build_checkpoint_reference_v2(run, &boundary).map_err(|_| artifact_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterReference)?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeLatest)?;
    let latest = build_latest_v2(&boundary, &reference).map_err(|_| artifact_error_v2())?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterLatest)?;

    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeFinalValidation)?;
    if u64::try_from(checkpoint_manifest.canonical_bytes().len()).map_or(true, |count| {
        count > plan.max_checkpoint_manifest_cj_bytes_v2()
    }) || u64::try_from(segment_manifest.canonical_bytes().len()).map_or(true, |count| {
        count > plan.max_trained_segment_manifest_cj_bytes_v2()
            || count > SEGMENT_MANIFEST_MAX_BYTES_V2
    }) || u64::try_from(boundary.checkpoint_sidecar_canonical_bytes().len())
        .map_or(true, |count| {
            count > plan.max_checkpoint_sidecar_cj_bytes_v2()
        })
        || u64::try_from(boundary.head_record_canonical_bytes().len())
            .map_or(true, |count| count > plan.max_head_record_cj_bytes_v2())
        || u64::try_from(reference.canonical_bytes().len()).map_or(true, |count| {
            count > plan.max_checkpoint_reference_cj_bytes_v2()
                || count > CHECKPOINT_REFERENCE_MAX_BYTES_V2
        })
        || u64::try_from(latest.canonical_bytes().len()).map_or(true, |count| {
            count > plan.max_latest_record_cj_bytes_v2() || count > LATEST_RECORD_MAX_BYTES_V2
        })
    {
        return Err(artifact_error_v2());
    }
    let checkpoint_payload_sha256 = sha256_v1(final_checkpoint.payload());
    let checkpoint_manifest_sha256 = sha256_v1(checkpoint_manifest.canonical_bytes());
    if checkpoint_payload_sha256 != final_checkpoint.digests().payload_sha256
        || checkpoint_payload_sha256 != checkpoint_manifest.checkpoint_payload_sha256()
        || checkpoint_manifest_sha256 != checkpoint_manifest.checkpoint_manifest_sha256()
    {
        return Err(artifact_error_v2());
    }
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterFinalValidation)?;
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::BeforeReceiptSeal)?;
    let expected_receipt = ExpectedPersistenceReceiptFactsV2 {
        generation_index: plan.generation_index_v2(),
        checkpoint_payload_sha256,
        checkpoint_manifest_sha256,
    };
    #[cfg(test)]
    injected_abort_for_test_v2(NativeTrainingPreparedAbortPointForTestV2::AfterReceiptSeal)?;
    Ok(NativeTrainingPreparedSegmentV2 {
        candidate,
        plan,
        final_compact,
        continuations,
        final_checkpoint,
        checkpoint_manifest,
        segment_manifest,
        boundary,
        reference,
        latest,
        expected_receipt,
    })
}

fn intrinsic_matches_context_v2(
    run: &ValidatedTrainRunV2,
    intrinsic: &NativeTrainingIntrinsicCheckpointFactsV2,
    context: &UpdateEvidenceChainContextV1,
    progress: &CheckpointProgressV3,
) -> bool {
    let intrinsic_progress = intrinsic.progress_v2();
    let Some(learner_policy_steps) = checked_seat_sum_v2(
        progress.learner_policy_steps_by_seat().p0(),
        progress.learner_policy_steps_by_seat().p1(),
    ) else {
        return false;
    };
    let Some(learner_physical_decisions) = checked_seat_sum_v2(
        progress.learner_physical_decisions_by_seat().p0(),
        progress.learner_physical_decisions_by_seat().p1(),
    ) else {
        return false;
    };
    intrinsic.base_seed_v2() == run.record().schedule.base_seed
        && intrinsic.batch_episodes_v2() == run.batch_episodes()
        && intrinsic.numerical_backend_v2() == NativeTrainingNumericalBackendV1::Sequential
        && intrinsic.backward_worker_limit_v2() == 1
        && context.batch_episodes_v1() == run.batch_episodes()
        && context.checkpoint_segment_updates_v1() == run.checkpoint_segment_updates()
        && progress.batch_episodes() == run.batch_episodes()
        && progress.checkpoint_segment_updates() == run.checkpoint_segment_updates()
        && intrinsic_progress.next_episode_index == progress.next_episode_index()
        && intrinsic_progress.successful_update_count == progress.successful_update_count()
        && intrinsic_progress.completed_episode_count == progress.completed_episode_count()
        && intrinsic_progress.learner_policy_step_count == learner_policy_steps
        && intrinsic_progress.learner_physical_decision_count == learner_physical_decisions
        && intrinsic.adam_step_v2() == progress.successful_update_count()
        && progress.successful_update_count().checked_add(1) == Some(context.next_update_index())
        && intrinsic.scorer_bias_anchor_bits_v2() == context.scorer_bias_anchor_bits_v1()
        && intrinsic.model_parameter_sha256_v2() == context.model_parameter_sha256()
        && intrinsic.train_state_sha256_v2() == context.train_state_sha256()
}

fn live_parent_matches_v2(
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    parent_checkpoint: &CheckpointManifestV3,
    live: &NativeTrainingIntrinsicCheckpointFactsV2,
) -> bool {
    let facts = parent.boundary_facts_v2();
    let progress = parent_checkpoint.progress();
    let live_progress = live.progress_v2();
    let expected_generation = facts
        .segment_ordinal
        .checked_mul(run.checkpoint_segment_updates())
        .filter(|value| *value <= U63_MAX_V2);
    let expected_episode = run
        .batch_episodes()
        .checked_mul(facts.generation_index)
        .filter(|value| *value <= U63_MAX_V2);
    let learner_policy_steps = checked_seat_sum_v2(
        progress.learner_policy_steps_by_seat().p0(),
        progress.learner_policy_steps_by_seat().p1(),
    );
    let learner_physical_decisions = checked_seat_sum_v2(
        progress.learner_physical_decisions_by_seat().p0(),
        progress.learner_physical_decisions_by_seat().p1(),
    );
    let anchor = u32::try_from(
        parent_checkpoint
            .train_state()
            .scorer_bias_anchor_f32_bits(),
    )
    .ok();
    facts.run_sha256 == run.run_sha256()
        && facts.identity_bundle_sha256 == run.identity_bundle_sha256()
        && facts.batch_episodes == run.batch_episodes()
        && facts.checkpoint_segment_updates == run.checkpoint_segment_updates()
        && expected_generation == Some(facts.generation_index)
        && parent_checkpoint.run_sha256() == run.run_sha256()
        && parent_checkpoint.identity_bundle_sha256() == run.identity_bundle_sha256()
        && parent_checkpoint.segment_ordinal() == facts.segment_ordinal
        && parent_checkpoint.generation_index() == facts.generation_index
        && parent_checkpoint.batch_episodes() == run.batch_episodes()
        && parent_checkpoint.checkpoint_segment_updates() == run.checkpoint_segment_updates()
        && facts.checkpoint_manifest_sha256 == parent_checkpoint.checkpoint_manifest_sha256()
        && facts.checkpoint_payload_sha256 == parent_checkpoint.checkpoint_payload_sha256()
        && facts.logical_state_sha256 == parent_checkpoint.logical_state_sha256()
        && facts.model_parameter_sha256 == parent_checkpoint.model_parameter_sha256()
        && facts.train_state_sha256 == parent_checkpoint.train_state_sha256()
        && progress.batch_episodes() == run.batch_episodes()
        && progress.checkpoint_segment_updates() == run.checkpoint_segment_updates()
        && progress.successful_update_count() == facts.generation_index
        && expected_episode == Some(progress.next_episode_index())
        && progress.completed_episode_count() == progress.next_episode_index()
        && live.base_seed_v2() == run.record().schedule.base_seed
        && live.batch_episodes_v2() == run.batch_episodes()
        && live.numerical_backend_v2() == NativeTrainingNumericalBackendV1::Sequential
        && live.backward_worker_limit_v2() == 1
        && live_progress.next_episode_index == progress.next_episode_index()
        && live_progress.successful_update_count == progress.successful_update_count()
        && live_progress.completed_episode_count == progress.completed_episode_count()
        && Some(live_progress.learner_policy_step_count) == learner_policy_steps
        && Some(live_progress.learner_physical_decision_count) == learner_physical_decisions
        && live.adam_step_v2() == facts.generation_index
        && Some(live.scorer_bias_anchor_bits_v2()) == anchor
        && live.model_parameter_sha256_v2() == facts.model_parameter_sha256
        && live.train_state_sha256_v2() == facts.train_state_sha256
}

fn checked_seat_sum_v2(left: u64, right: u64) -> Option<u64> {
    left.checked_add(right).filter(|value| *value <= U63_MAX_V2)
}

fn reserve_update_groups_v2(capacity: usize) -> Result<Vec<ValidatedUpdateGroupV1>> {
    #[cfg(test)]
    if RESERVATION_FAILURE_FOR_TEST_V2.with(std::cell::Cell::get) {
        return Err(resource_error_v2());
    }
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(capacity)
        .map_err(|_| resource_error_v2())?;
    Ok(groups)
}

#[cfg(test)]
thread_local! {
    static RESERVATION_FAILURE_FOR_TEST_V2: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn inject_reservation_failure_for_test_v2(enabled: bool) {
    RESERVATION_FAILURE_FOR_TEST_V2.with(|cell| cell.set(enabled));
}

const fn input_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::InputBindingInvalid,
    )
}

const fn representability_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::RepresentabilityBoundExceeded,
    )
}

const fn resource_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::ResourceExhausted,
    )
}

const fn update_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::UpdateFailed,
    )
}

const fn evidence_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::EvidenceInvalid,
    )
}

const fn artifact_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::ArtifactInvalid,
    )
}

const fn receipt_error_v2() -> NativeTrainingPreparedSegmentV2Error {
    NativeTrainingPreparedSegmentV2Error::new(
        NativeTrainingPreparedSegmentV2ErrorKind::PersistenceReceiptMismatch,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_json_v1::{
        to_canonical_json_bytes_v1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
    };
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_policy_train_step_v1::{
        reset_train_state_snapshot_call_count_for_test_v1,
        train_state_snapshot_call_count_for_test_v1,
    };
    use crate::native_train_state_payload_v1::{
        payload_encode_counts_for_test_v1, reset_payload_encode_counts_for_test_v1,
    };
    use crate::native_training_executor_v1::{
        reset_segment_candidate_counts_for_test_v2, segment_candidate_counts_for_test_v2,
        NativeTrainingExecutionConfigV1, NativeTrainingPrecloneMutationForTestV2,
    };
    use crate::native_training_store_boundary_v2::{
        build_genesis_native_training_boundary_v2, decode_trained_native_training_boundary_v2,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, decode_trained_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_continuation_v2::decode_segment_continuations_v2;
    use crate::native_training_store_segment_manifest_v2::{
        build_genesis_segment_manifest_v2, decode_trained_segment_manifest_v2,
    };
    use crate::native_training_store_update_group_v1::{
        build_update_group_v1, decode_update_group_v1,
    };
    use crate::native_training_store_v2::test_persistence_receipt_v2;
    use serde_json::Value;
    use std::sync::OnceLock;
    use std::time::Duration;

    struct GenesisFixtureV2 {
        run: ValidatedTrainRunV2,
        checkpoint: CheckpointManifestV3,
        boundary: ValidatedNativeTrainingBoundaryV2,
    }

    static GENESIS_FIXTURE_V2: OnceLock<GenesisFixtureV2> = OnceLock::new();

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

    fn genesis_fixture_v2() -> &'static GenesisFixtureV2 {
        GENESIS_FIXTURE_V2.get_or_init(|| {
            let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
            let executor = fresh_executor_v2(&run);
            let candidate = executor.checkpoint_candidate_v1().unwrap();
            let checkpoint =
                build_genesis_checkpoint_manifest_v3(&run, candidate.payload()).unwrap();
            let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
            let boundary =
                build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
            GenesisFixtureV2 {
                run,
                checkpoint,
                boundary,
            }
        })
    }

    fn reset_production_counts_v2() {
        reset_segment_candidate_counts_for_test_v2();
        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();
    }

    #[derive(Debug, Eq, PartialEq)]
    struct IntrinsicProjectionV2 {
        base_seed: u64,
        batch_episodes: u64,
        numerical_backend: NativeTrainingNumericalBackendV1,
        backward_worker_limit: usize,
        progress: crate::native_training_executor_v1::NativeTrainingProgressV1,
        adam_step: u64,
        scorer_bias_anchor_bits: u32,
        model_parameter_sha256: [u8; 32],
        train_state_sha256: [u8; 32],
    }

    fn intrinsic_projection_v2(
        facts: &NativeTrainingIntrinsicCheckpointFactsV2,
    ) -> IntrinsicProjectionV2 {
        IntrinsicProjectionV2 {
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

    #[derive(Debug, Eq, PartialEq)]
    struct ContextProjectionV2 {
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

    fn context_projection_v2(context: &UpdateEvidenceChainContextV1) -> ContextProjectionV2 {
        ContextProjectionV2 {
            run_sha256: context.run_sha256_raw_v1(),
            identity_bundle_sha256: context.identity_bundle_sha256_raw_v1(),
            batch_episodes: context.batch_episodes_v1(),
            checkpoint_segment_updates: context.checkpoint_segment_updates_v1(),
            next_update_index: context.next_update_index(),
            previous_update_evidence_sha256: context.previous_update_evidence_sha256(),
            progress: *context.progress(),
            model_parameter_sha256: context.model_parameter_sha256(),
            train_state_sha256: context.train_state_sha256(),
            scorer_bias_anchor_bits: context.scorer_bias_anchor_bits_v1(),
        }
    }

    fn checkpoint_matches_context_v2(
        run: &ValidatedTrainRunV2,
        checkpoint: &NativeTrainingCheckpointCandidateV1,
        context: &UpdateEvidenceChainContextV1,
    ) -> bool {
        let progress = context.progress();
        let aggregate = checkpoint.progress();
        checkpoint.base_seed() == run.record().schedule.base_seed
            && checkpoint.batch_episodes() == run.batch_episodes()
            && checkpoint.numerical_backend() == NativeTrainingNumericalBackendV1::Sequential
            && checkpoint.backward_worker_limit() == 1
            && aggregate.next_episode_index == progress.next_episode_index()
            && aggregate.successful_update_count == progress.successful_update_count()
            && aggregate.completed_episode_count == progress.completed_episode_count()
            && checked_seat_sum_v2(
                progress.learner_policy_steps_by_seat().p0(),
                progress.learner_policy_steps_by_seat().p1(),
            ) == Some(aggregate.learner_policy_step_count)
            && checked_seat_sum_v2(
                progress.learner_physical_decisions_by_seat().p0(),
                progress.learner_physical_decisions_by_seat().p1(),
            ) == Some(aggregate.learner_physical_decision_count)
            && checkpoint.adam_step() == progress.successful_update_count()
            && checkpoint.scorer_bias_anchor_bits() == context.scorer_bias_anchor_bits_v1()
            && checkpoint.digests().model_parameter_sha256 == context.model_parameter_sha256()
            && checkpoint.digests().native_state_sha256 == context.train_state_sha256()
    }

    fn standalone_groups_from_continuations_v2(continuations: &[Vec<u8>]) -> Vec<Vec<u8>> {
        const PREVIOUS_UPDATE: &[CanonicalJsonNullPathSegmentV1] =
            &[CanonicalJsonNullPathSegmentV1::ObjectKey(
                "previous_update_evidence_sha256",
            )];
        const WINNER: &[CanonicalJsonNullPathSegmentV1] = &[
            CanonicalJsonNullPathSegmentV1::ObjectKey("evidence"),
            CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
            CanonicalJsonNullPathSegmentV1::AnyArrayElement,
            CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
        ];
        let null_policy = CanonicalJsonNullPolicyV1::AllowOnly(&[PREVIOUS_UPDATE, WINNER]);
        let mut groups = Vec::new();
        for continuation in continuations {
            let value: Value = serde_json::from_slice(continuation).unwrap();
            for group in value["update_groups"].as_array().unwrap() {
                groups.push(to_canonical_json_bytes_v1(group, null_policy).unwrap());
            }
        }
        groups
    }

    fn assert_one_payload_path_matches_full_export_oracle_v2(
        run: &ValidatedTrainRunV2,
        parent: &ValidatedNativeTrainingBoundaryV2,
        parent_checkpoint: &CheckpointManifestV3,
        compact_executor: &mut NativeTrainingExecutorV1,
        full_executor: &mut NativeTrainingExecutorV1,
    ) {
        let compact_live_before = compact_executor.intrinsic_checkpoint_facts_v2().unwrap();
        let compact = prepare_segment_v2(compact_executor, run, parent, parent_checkpoint).unwrap();
        let view = compact.publication_view_v2();
        let compact_continuations = (0..view.continuation_count_v2())
            .map(|index| {
                view.continuation_canonical_bytes_v2(index)
                    .unwrap()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        let compact_continuation_hashes = (0..view.continuation_count_v2())
            .map(|index| view.continuation_sha256_v2(index).unwrap())
            .collect::<Vec<_>>();
        assert!(view
            .continuation_canonical_bytes_v2(view.continuation_count_v2())
            .is_none());
        assert!(view
            .continuation_sha256_v2(view.continuation_count_v2())
            .is_none());
        let compact_groups = standalone_groups_from_continuations_v2(&compact_continuations);
        let compact_payload = view.checkpoint_payload_v2().to_vec();
        let compact_checkpoint = view.checkpoint_manifest_canonical_bytes_v2().to_vec();
        let compact_segment = view.segment_manifest_canonical_bytes_v2().to_vec();
        let compact_sidecar = view.checkpoint_sidecar_canonical_bytes_v2().to_vec();
        let compact_head = view.head_record_canonical_bytes_v2().to_vec();
        let compact_reference = view.checkpoint_reference_canonical_bytes_v2().to_vec();
        let compact_latest = view.latest_canonical_bytes_v2().to_vec();
        let compact_payload_sha256 = view.checkpoint_payload_sha256_v2();
        let compact_checkpoint_sha256 = view.checkpoint_manifest_sha256_v2();
        let compact_segment_sha256 = view.segment_manifest_sha256_v2();
        let compact_sidecar_sha256 = view.checkpoint_sidecar_sha256_v2();
        let compact_head_sha256 = view.head_record_sha256_v2();
        let compact_reference_sha256 = view.checkpoint_reference_sha256_v2();
        let compact_latest_sha256 = view.latest_sha256_v2();
        for (bytes, expected) in compact_continuations
            .iter()
            .zip(&compact_continuation_hashes)
        {
            assert_eq!(sha256_v1(bytes), *expected);
        }
        assert_eq!(sha256_v1(&compact_payload), compact_payload_sha256);
        assert_eq!(sha256_v1(&compact_checkpoint), compact_checkpoint_sha256);
        assert_eq!(sha256_v1(&compact_segment), compact_segment_sha256);
        assert_eq!(sha256_v1(&compact_sidecar), compact_sidecar_sha256);
        assert_eq!(sha256_v1(&compact_head), compact_head_sha256);
        assert_eq!(sha256_v1(&compact_reference), compact_reference_sha256);
        assert_eq!(sha256_v1(&compact_latest), compact_latest_sha256);
        drop(compact);
        assert_eq!(
            compact_executor.intrinsic_checkpoint_facts_v2().unwrap(),
            compact_live_before
        );

        let continuation_context =
            resume_update_evidence_chain_v1(run, parent, parent_checkpoint).unwrap();
        let mut context = resume_update_evidence_chain_v1(run, parent, parent_checkpoint).unwrap();
        let mut full_groups = Vec::with_capacity(4);
        let mut full_group_bytes = Vec::with_capacity(4);
        let mut full_hashes = Vec::with_capacity(4);
        let mut full_contexts = Vec::with_capacity(4);
        let mut full_checkpoints = Vec::with_capacity(4);
        for ordinal in 0..4 {
            let prepared = full_executor.prepare_update_v2().unwrap();
            let checkpoint = prepared.checkpoint_candidate().clone();
            let advance = build_update_group_v1(run, context, &prepared).unwrap();
            let (group, advanced) = advance.into_parts();
            assert!(checkpoint_matches_context_v2(run, &checkpoint, &advanced));
            full_group_bytes.push(group.canonical_bytes().to_vec());
            full_hashes.push(group.update_evidence_sha256());
            full_contexts.push(context_projection_v2(&advanced));
            full_checkpoints.push(checkpoint);
            full_groups.push(group);
            context = advanced;
            drop(prepared);
            if ordinal < 3 {
                full_executor.run_update_v2().unwrap();
            }
        }
        assert_eq!(compact_groups, full_group_bytes);
        let mut compact_context =
            resume_update_evidence_chain_v1(run, parent, parent_checkpoint).unwrap();
        for (ordinal, bytes) in compact_groups.iter().enumerate() {
            let advance = decode_update_group_v1(run, compact_context, bytes).unwrap();
            assert_eq!(
                advance.group().update_evidence_sha256(),
                full_hashes[ordinal]
            );
            let (_, advanced) = advance.into_parts();
            assert_eq!(context_projection_v2(&advanced), full_contexts[ordinal]);
            assert!(checkpoint_matches_context_v2(
                run,
                &full_checkpoints[ordinal],
                &advanced
            ));
            compact_context = advanced;
        }

        let continuations =
            build_segment_continuations_v2(run, continuation_context, full_groups).unwrap();
        let oracle_checkpoint = build_trained_checkpoint_manifest_v3(
            run,
            continuations.advanced_context(),
            full_checkpoints.last().unwrap(),
        )
        .unwrap();
        let oracle_segment =
            build_trained_segment_manifest_v2(run, parent, &continuations, &oracle_checkpoint)
                .unwrap();
        let oracle_boundary = build_trained_native_training_boundary_v2(
            run,
            parent,
            &oracle_segment,
            &oracle_checkpoint,
        )
        .unwrap();
        let oracle_reference = build_checkpoint_reference_v2(run, &oracle_boundary).unwrap();
        let oracle_latest = build_latest_v2(&oracle_boundary, &oracle_reference).unwrap();
        let oracle_continuations = continuations
            .chain()
            .continuations()
            .iter()
            .map(|continuation| continuation.canonical_bytes().to_vec())
            .collect::<Vec<_>>();

        assert_eq!(compact_continuations, oracle_continuations);
        assert_eq!(
            compact_continuation_hashes,
            continuations
                .chain()
                .continuations()
                .iter()
                .map(|continuation| continuation.continuation_sha256())
                .collect::<Vec<_>>()
        );
        assert_eq!(compact_payload, full_checkpoints.last().unwrap().payload());
        assert_eq!(
            compact_payload_sha256,
            full_checkpoints.last().unwrap().digests().payload_sha256
        );
        assert_eq!(compact_checkpoint, oracle_checkpoint.canonical_bytes());
        assert_eq!(
            compact_checkpoint_sha256,
            oracle_checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(compact_segment, oracle_segment.canonical_bytes());
        assert_eq!(
            compact_sidecar,
            oracle_boundary.checkpoint_sidecar_canonical_bytes()
        );
        assert_eq!(compact_head, oracle_boundary.head_record_canonical_bytes());
        assert_eq!(compact_reference, oracle_reference.canonical_bytes());
        assert_eq!(compact_latest, oracle_latest.canonical_bytes());
        assert_eq!(
            compact_segment_sha256,
            oracle_segment.segment_manifest_sha256()
        );
        assert_eq!(
            compact_sidecar_sha256,
            oracle_boundary.checkpoint_sidecar_sha256()
        );
        assert_eq!(compact_head_sha256, oracle_boundary.head_record_sha256());
        assert_eq!(
            compact_reference_sha256,
            oracle_reference.checkpoint_ref_sha256()
        );
        assert_eq!(compact_latest_sha256, oracle_latest.latest_record_sha256());
    }

    #[test]
    fn one_payload_path_matches_full_export_after_every_update_oracle() {
        let fixture = genesis_fixture_v2();
        let mut genesis_compact = fresh_executor_v2(&fixture.run);
        let mut genesis_full = fresh_executor_v2(&fixture.run);
        assert_one_payload_path_matches_full_export_oracle_v2(
            &fixture.run,
            &fixture.boundary,
            &fixture.checkpoint,
            &mut genesis_compact,
            &mut genesis_full,
        );

        let mut resumed_compact = fresh_executor_v2(&fixture.run);
        let prepared = prepare_segment_v2(
            &mut resumed_compact,
            &fixture.run,
            &fixture.boundary,
            &fixture.checkpoint,
        )
        .unwrap();
        let view = prepared.publication_view_v2();
        let continuation_cjs = (0..view.continuation_count_v2())
            .map(|index| {
                view.continuation_canonical_bytes_v2(index)
                    .unwrap()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        let payload = view.checkpoint_payload_v2().to_vec();
        let checkpoint_cj = view.checkpoint_manifest_canonical_bytes_v2().to_vec();
        let segment_cj = view.segment_manifest_canonical_bytes_v2().to_vec();
        let sidecar_cj = view.checkpoint_sidecar_canonical_bytes_v2().to_vec();
        let head_cj = view.head_record_canonical_bytes_v2().to_vec();
        let receipt = test_persistence_receipt_v2(
            prepared.expected_generation_index(),
            view.checkpoint_payload_sha256_v2(),
            view.checkpoint_manifest_sha256_v2(),
        );
        prepared.commit_v2(receipt).unwrap();

        let parent_context =
            resume_update_evidence_chain_v1(&fixture.run, &fixture.boundary, &fixture.checkpoint)
                .unwrap();
        let continuations =
            decode_segment_continuations_v2(&fixture.run, parent_context, &continuation_cjs)
                .unwrap();
        let checkpoint = decode_trained_checkpoint_manifest_v3(
            &checkpoint_cj,
            &payload,
            &fixture.run,
            continuations.advanced_context(),
        )
        .unwrap();
        let segment = decode_trained_segment_manifest_v2(
            &segment_cj,
            &fixture.run,
            &fixture.boundary,
            &continuations,
            &checkpoint,
        )
        .unwrap();
        let boundary = decode_trained_native_training_boundary_v2(
            &sidecar_cj,
            &head_cj,
            &fixture.run,
            &fixture.boundary,
            &segment,
            &checkpoint,
        )
        .unwrap();

        let mut resumed_full = fresh_executor_v2(&fixture.run);
        for _ in 0..4 {
            resumed_full.run_update_v2().unwrap();
        }
        assert_eq!(
            resumed_full.checkpoint_candidate_v1().unwrap().payload(),
            payload
        );
        assert_one_payload_path_matches_full_export_oracle_v2(
            &fixture.run,
            &boundary,
            &checkpoint,
            &mut resumed_compact,
            &mut resumed_full,
        );
    }

    #[test]
    fn every_compact_fact_mutation_rejects_before_artifact_return_or_live_mutation() {
        use NativeTrainingCompactFactMutationForTestV2 as Compact;
        use NativeTrainingIntrinsicFactMutationForTestV2 as Intrinsic;

        let mutations = [
            Compact::Intrinsic(Intrinsic::BaseSeed),
            Compact::Intrinsic(Intrinsic::BatchEpisodes),
            Compact::Intrinsic(Intrinsic::NumericalBackend),
            Compact::Intrinsic(Intrinsic::BackwardWorkerLimit),
            Compact::Intrinsic(Intrinsic::NextEpisodeIndex),
            Compact::Intrinsic(Intrinsic::SuccessfulUpdateCount),
            Compact::Intrinsic(Intrinsic::CompletedEpisodeCount),
            Compact::Intrinsic(Intrinsic::LearnerPhysicalDecisionCount),
            Compact::Intrinsic(Intrinsic::LearnerPolicyStepCount),
            Compact::Intrinsic(Intrinsic::AdamStep),
            Compact::Intrinsic(Intrinsic::ScorerBiasAnchorBits),
            Compact::Intrinsic(Intrinsic::ModelParameterSha256),
            Compact::Intrinsic(Intrinsic::TrainStateSha256),
            Compact::RunSha256,
            Compact::IdentityBundleSha256,
            Compact::CheckpointSegmentUpdates,
            Compact::ProgressBatchEpisodes,
            Compact::ProgressCheckpointSegmentUpdates,
            Compact::ProgressNextEpisodeIndex,
            Compact::ProgressSuccessfulUpdateCount,
            Compact::ProgressCompletedEpisodeCount,
            Compact::ProgressP0Win,
            Compact::ProgressP0Loss,
            Compact::ProgressP0Draw,
            Compact::ProgressP1Win,
            Compact::ProgressP1Loss,
            Compact::ProgressP1Draw,
            Compact::ProgressPolicyP0,
            Compact::ProgressPolicyP1,
            Compact::ProgressPhysicalP0,
            Compact::ProgressPhysicalP1,
        ];
        let fixture = genesis_fixture_v2();
        for mutation in mutations {
            let mut executor = fresh_executor_v2(&fixture.run);
            let before = executor.intrinsic_checkpoint_facts_v2().unwrap();
            reset_production_counts_v2();
            inject_compact_mutation_for_test_v2(Some(mutation));
            let result = prepare_segment_v2(
                &mut executor,
                &fixture.run,
                &fixture.boundary,
                &fixture.checkpoint,
            );
            let pending = pending_compact_mutation_for_test_v2();
            inject_compact_mutation_for_test_v2(None);
            assert_eq!(pending, None, "compact hook was not consumed: {mutation:?}");
            let error = match result {
                Ok(prepared) => {
                    drop(prepared);
                    panic!("compact mutation was admitted: {mutation:?}");
                }
                Err(error) => error,
            };
            assert_eq!(
                error.kind(),
                NativeTrainingPreparedSegmentV2ErrorKind::EvidenceInvalid,
                "unexpected class for {mutation:?}"
            );
            assert_eq!(segment_candidate_counts_for_test_v2(), (1, 1));
            assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
            assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
            assert_eq!(
                executor.intrinsic_checkpoint_facts_v2().unwrap(),
                before,
                "live mutation for {mutation:?}"
            );
        }
    }

    #[test]
    fn optimizer_only_predecessor_drift_rejects_before_next_update() {
        let fixture = genesis_fixture_v2();
        let mut executor = fresh_executor_v2(&fixture.run);
        let mut nonvacuity = executor.intrinsic_checkpoint_facts_v2().unwrap();
        let model_before = nonvacuity.model_parameter_sha256_v2();
        let train_before = nonvacuity.train_state_sha256_v2();
        nonvacuity
            .mutate_for_test_v2(NativeTrainingIntrinsicFactMutationForTestV2::TrainStateSha256);
        assert_eq!(nonvacuity.model_parameter_sha256_v2(), model_before);
        assert_ne!(nonvacuity.train_state_sha256_v2(), train_before);

        let before = executor.intrinsic_checkpoint_facts_v2().unwrap();
        reset_production_counts_v2();
        inject_predecessor_mutation_for_test_v2(Some((
            1,
            NativeTrainingIntrinsicFactMutationForTestV2::TrainStateSha256,
        )));
        let result = prepare_segment_v2(
            &mut executor,
            &fixture.run,
            &fixture.boundary,
            &fixture.checkpoint,
        );
        let pending = pending_predecessor_mutation_for_test_v2();
        inject_predecessor_mutation_for_test_v2(None);
        assert_eq!(pending, None, "predecessor hook was not consumed");
        let error = match result {
            Ok(prepared) => {
                drop(prepared);
                panic!("optimizer-only predecessor drift was admitted");
            }
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            NativeTrainingPreparedSegmentV2ErrorKind::UpdateFailed
        );
        assert_eq!(segment_candidate_counts_for_test_v2(), (1, 1));
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
        assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
        assert_eq!(executor.intrinsic_checkpoint_facts_v2().unwrap(), before);
    }

    #[test]
    fn all_live_parent_mismatches_reject_before_clone_rollout_or_payload() {
        let fixture = genesis_fixture_v2();
        let mutations = [
            NativeTrainingPrecloneMutationForTestV2::OptimizerMoment,
            NativeTrainingPrecloneMutationForTestV2::ModelParameter,
            NativeTrainingPrecloneMutationForTestV2::Progress,
            NativeTrainingPrecloneMutationForTestV2::ScorerAnchor,
            NativeTrainingPrecloneMutationForTestV2::BaseSeed,
            NativeTrainingPrecloneMutationForTestV2::BatchEpisodes,
            NativeTrainingPrecloneMutationForTestV2::NumericalBackend,
            NativeTrainingPrecloneMutationForTestV2::BackwardWorkerLimit,
        ];
        for mutation in mutations {
            let mut executor = fresh_executor_v2(&fixture.run);
            let unmutated =
                intrinsic_projection_v2(&executor.intrinsic_checkpoint_facts_v2().unwrap());
            executor.mutate_live_for_preclone_test_v2(mutation);
            let before = executor.intrinsic_checkpoint_facts_v2().unwrap();
            let mutated = intrinsic_projection_v2(&before);
            assert_ne!(mutated, unmutated, "mutation was vacuous: {mutation:?}");
            if mutation == NativeTrainingPrecloneMutationForTestV2::OptimizerMoment {
                assert_eq!(mutated.base_seed, unmutated.base_seed);
                assert_eq!(mutated.batch_episodes, unmutated.batch_episodes);
                assert_eq!(mutated.numerical_backend, unmutated.numerical_backend);
                assert_eq!(
                    mutated.backward_worker_limit,
                    unmutated.backward_worker_limit
                );
                assert_eq!(mutated.progress, unmutated.progress);
                assert_eq!(mutated.adam_step, unmutated.adam_step);
                assert_eq!(
                    mutated.scorer_bias_anchor_bits,
                    unmutated.scorer_bias_anchor_bits
                );
                assert_eq!(
                    mutated.model_parameter_sha256,
                    unmutated.model_parameter_sha256
                );
                assert_ne!(mutated.train_state_sha256, unmutated.train_state_sha256);
            }
            reset_production_counts_v2();
            let result = prepare_segment_v2(
                &mut executor,
                &fixture.run,
                &fixture.boundary,
                &fixture.checkpoint,
            );
            let error = match result {
                Ok(prepared) => {
                    drop(prepared);
                    panic!("live-parent mismatch was admitted: {mutation:?}");
                }
                Err(error) => error,
            };
            assert_eq!(
                error.kind(),
                NativeTrainingPreparedSegmentV2ErrorKind::InputBindingInvalid,
                "unexpected class for {mutation:?}"
            );
            assert_eq!(segment_candidate_counts_for_test_v2(), (0, 0));
            assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
            assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
            assert_eq!(
                executor.intrinsic_checkpoint_facts_v2().unwrap(),
                before,
                "failed preparation changed live mismatch state: {mutation:?}"
            );
        }
    }

    #[test]
    fn every_update_and_artifact_abort_point_leaves_live_executor_exact() {
        use NativeTrainingPreparedAbortPointForTestV2 as Point;

        let mut points = vec![Point::AfterClone];
        for offset in 0..4 {
            points.push(Point::BeforeUpdate(offset));
            points.push(Point::AfterUpdate(offset));
        }
        points.extend([
            Point::BeforeContinuations,
            Point::AfterContinuations,
            Point::BeforeCheckpointManifest,
            Point::AfterCheckpointManifest,
            Point::BeforeSegmentManifest,
            Point::AfterSegmentManifest,
            Point::BeforeBoundary,
            Point::AfterBoundary,
            Point::BeforeReference,
            Point::AfterReference,
            Point::BeforeLatest,
            Point::AfterLatest,
            Point::BeforeFinalValidation,
            Point::AfterFinalValidation,
            Point::BeforeReceiptSeal,
            Point::AfterReceiptSeal,
        ]);

        let fixture = genesis_fixture_v2();
        for point in points {
            let mut executor = fresh_executor_v2(&fixture.run);
            let before = executor.intrinsic_checkpoint_facts_v2().unwrap();
            inject_abort_point_for_test_v2(Some(point));
            let result = prepare_segment_v2(
                &mut executor,
                &fixture.run,
                &fixture.boundary,
                &fixture.checkpoint,
            );
            let pending = pending_abort_point_for_test_v2();
            inject_abort_point_for_test_v2(None);
            assert_eq!(pending, None, "abort hook was not consumed: {point:?}");
            let expected_kind = match point {
                Point::AfterClone | Point::BeforeUpdate(_) | Point::AfterUpdate(_) => {
                    NativeTrainingPreparedSegmentV2ErrorKind::UpdateFailed
                }
                _ => NativeTrainingPreparedSegmentV2ErrorKind::ArtifactInvalid,
            };
            match result {
                Ok(prepared) => {
                    drop(prepared);
                    panic!("abort point was not reached: {point:?}");
                }
                Err(error) => assert_eq!(error.kind(), expected_kind),
            }
            assert_eq!(
                executor.intrinsic_checkpoint_facts_v2().unwrap(),
                before,
                "live mutation at {point:?}"
            );
        }
    }

    #[test]
    fn genuine_genesis_and_resumed_segments_use_one_clone_and_one_final_payload() {
        let fixture = genesis_fixture_v2();
        let mut executor = fresh_executor_v2(&fixture.run);
        reset_production_counts_v2();
        let prepared = prepare_segment_v2(
            &mut executor,
            &fixture.run,
            &fixture.boundary,
            &fixture.checkpoint,
        )
        .unwrap();
        assert_eq!(prepared.parent_generation_index(), 0);
        assert_eq!(prepared.expected_generation_index(), 4);
        assert_eq!(segment_candidate_counts_for_test_v2(), (1, 4));
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 1);
        assert_eq!(payload_encode_counts_for_test_v1(), (1, 1));

        let expected_installed = intrinsic_projection_v2(&prepared.final_compact.intrinsic);
        let view = prepared.publication_view_v2();
        let continuation_cjs = (0..view.continuation_count_v2())
            .map(|index| {
                view.continuation_canonical_bytes_v2(index)
                    .unwrap()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        let payload = view.checkpoint_payload_v2().to_vec();
        let checkpoint_cj = view.checkpoint_manifest_canonical_bytes_v2().to_vec();
        let segment_cj = view.segment_manifest_canonical_bytes_v2().to_vec();
        let sidecar_cj = view.checkpoint_sidecar_canonical_bytes_v2().to_vec();
        let head_cj = view.head_record_canonical_bytes_v2().to_vec();
        let payload_sha256 = view.checkpoint_payload_sha256_v2();
        let manifest_sha256 = view.checkpoint_manifest_sha256_v2();
        assert_eq!(sha256_v1(&payload), payload_sha256);
        assert_eq!(sha256_v1(&checkpoint_cj), manifest_sha256);
        let receipt = test_persistence_receipt_v2(4, payload_sha256, manifest_sha256);
        prepared.commit_v2(receipt).unwrap();
        assert_eq!(executor.progress().successful_update_count, 4);
        assert_eq!(
            intrinsic_projection_v2(&executor.intrinsic_checkpoint_facts_v2().unwrap()),
            expected_installed
        );
        let installed = executor.checkpoint_candidate_v1().unwrap();
        assert_eq!(installed.payload(), payload);
        assert_eq!(installed.digests().payload_sha256, payload_sha256);
        assert_eq!(installed.progress().successful_update_count, 4);

        let parent_context =
            resume_update_evidence_chain_v1(&fixture.run, &fixture.boundary, &fixture.checkpoint)
                .unwrap();
        let continuations =
            decode_segment_continuations_v2(&fixture.run, parent_context, &continuation_cjs)
                .unwrap();
        let checkpoint = decode_trained_checkpoint_manifest_v3(
            &checkpoint_cj,
            &payload,
            &fixture.run,
            continuations.advanced_context(),
        )
        .unwrap();
        let segment = decode_trained_segment_manifest_v2(
            &segment_cj,
            &fixture.run,
            &fixture.boundary,
            &continuations,
            &checkpoint,
        )
        .unwrap();
        let boundary = decode_trained_native_training_boundary_v2(
            &sidecar_cj,
            &head_cj,
            &fixture.run,
            &fixture.boundary,
            &segment,
            &checkpoint,
        )
        .unwrap();

        reset_production_counts_v2();
        let resumed_live_before = executor.intrinsic_checkpoint_facts_v2().unwrap();
        let resumed =
            prepare_segment_v2(&mut executor, &fixture.run, &boundary, &checkpoint).unwrap();
        assert_eq!(resumed.parent_generation_index(), 4);
        assert_eq!(resumed.expected_generation_index(), 8);
        assert_eq!(segment_candidate_counts_for_test_v2(), (1, 4));
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 1);
        assert_eq!(payload_encode_counts_for_test_v1(), (1, 1));
        drop(resumed);
        assert_eq!(
            executor.intrinsic_checkpoint_facts_v2().unwrap(),
            resumed_live_before
        );
    }

    #[test]
    fn every_wrong_receipt_role_consumes_guard_without_changing_live_executor() {
        let fixture = genesis_fixture_v2();
        for corrupted_role in 0..3 {
            let mut executor = fresh_executor_v2(&fixture.run);
            let before = executor.intrinsic_checkpoint_facts_v2().unwrap();
            let prepared = prepare_segment_v2(
                &mut executor,
                &fixture.run,
                &fixture.boundary,
                &fixture.checkpoint,
            )
            .unwrap();
            let view = prepared.publication_view_v2();
            let mut generation = prepared.expected_generation_index();
            let mut payload_sha256 = view.checkpoint_payload_sha256_v2();
            let mut manifest_sha256 = view.checkpoint_manifest_sha256_v2();
            match corrupted_role {
                0 => generation ^= 1,
                1 => payload_sha256[0] ^= 1,
                2 => manifest_sha256[0] ^= 1,
                _ => unreachable!(),
            }
            let receipt = test_persistence_receipt_v2(generation, payload_sha256, manifest_sha256);
            let error = prepared.commit_v2(receipt).unwrap_err();
            assert_eq!(
                error.kind(),
                NativeTrainingPreparedSegmentV2ErrorKind::PersistenceReceiptMismatch
            );
            assert_eq!(
                executor.intrinsic_checkpoint_facts_v2().unwrap(),
                before,
                "wrong receipt role {corrupted_role} changed the live executor"
            );
        }
    }

    #[test]
    fn injected_reservation_failure_precedes_clone_and_update() {
        let fixture = genesis_fixture_v2();
        let mut executor = fresh_executor_v2(&fixture.run);
        reset_production_counts_v2();
        inject_reservation_failure_for_test_v2(true);
        let result = prepare_segment_v2(
            &mut executor,
            &fixture.run,
            &fixture.boundary,
            &fixture.checkpoint,
        );
        inject_reservation_failure_for_test_v2(false);
        let error = match result {
            Ok(_) => panic!("expected injected reservation failure"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            NativeTrainingPreparedSegmentV2ErrorKind::ResourceExhausted
        );
        assert_eq!(segment_candidate_counts_for_test_v2(), (0, 0));
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
        assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
        assert_eq!(executor.progress().successful_update_count, 0);
    }
}
