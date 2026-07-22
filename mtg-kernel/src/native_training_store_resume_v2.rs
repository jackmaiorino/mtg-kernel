//! Native training Store V2 currentness validation and resume orchestration.
//!
//! The reader path takes the shared range lock and fully validates `run.json`,
//! every reachable boundary generation `0, S, 2S, ..., latest` in order, the
//! global evidence chain, and the latest pointer against the walked
//! authorities; hash-link walking alone is never sufficient. The mutator path
//! takes the exclusive range lock, applies only the complete prevalidated
//! recognized-stage deletion plan, and either proves the exact `P = N` no-op
//! or reconstructs a private candidate from the latest checkpoint and swaps it
//! into a fresh executor for the next `S`-update window. Resume accepts no
//! overrides for snapshot, seed, deck, optimizer, `K`, `S`, target `N`,
//! cadence, caps, topology, or runtime tuple. Any failure preserves latest and
//! every unknown or mismatching object. Generation publication and the
//! product CLI remain separate layers.

use crate::durable_publication_v1::DurableFileExpectationV1;
use crate::native_train_state_payload_v1::NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1;
use crate::native_training_executor_v1::{
    NativeTrainingCheckpointCandidateV1, NativeTrainingCheckpointDigestsV1,
    NativeTrainingCheckpointMetadataV1, NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
    NativeTrainingProgressV1,
};
use crate::native_training_store_boundary_v2::{
    decode_genesis_native_training_boundary_v2, decode_trained_native_training_boundary_v2,
    ValidatedNativeTrainingBoundaryV2, CHECKPOINT_SIDECAR_MAX_BYTES_V2, HEAD_RECORD_MAX_BYTES_V2,
};
use crate::native_training_store_checkpoint_v3::{
    decode_checkpoint_manifest_v3, decode_trained_checkpoint_manifest_v3, CheckpointManifestV3,
    CHECKPOINT_MANIFEST_MAX_BYTES_V3,
};
use crate::native_training_store_digest_v1::parse_lower_hex_raw32_v1;
use crate::native_training_store_layout_v2::{
    classify_store_leaf_v2, NativeTrainingStoreDirectoryV2, NativeTrainingStoreFinalNameV2,
    NativeTrainingStoreLeafV2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
};
use crate::native_training_store_reference_latest_v2::{
    decode_checkpoint_reference_v2, decode_latest_v2, peek_latest_generation_index_v2,
    ValidatedCheckpointReferenceV2, CHECKPOINT_REFERENCE_MAX_BYTES_V2, LATEST_RECORD_MAX_BYTES_V2,
};
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_continuation_v2::{
    decode_segment_continuations_v2, SEGMENT_CONTINUATION_MAX_BYTES_V2,
};
use crate::native_training_store_segment_manifest_v2::{
    decode_genesis_segment_manifest_v2, decode_trained_segment_manifest_v2,
    SEGMENT_MANIFEST_MAX_BYTES_V2,
};
use crate::native_training_store_update_group_v1::{
    resume_update_evidence_chain_v1, validate_prepared_execution_config_v1,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

const RUN_RECORD_MAX_BYTES_V2: u64 = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreResumeV2ErrorKind {
    UnsupportedPlatform,
    StoreBusy,
    RootInvalid,
    RunInvalid,
    ScheduleInvalid,
    GenerationInvalid,
    LatestInvalid,
    StageCorruption,
    ReconstructionFailed,
    MutationFailed,
}

impl NativeTrainingStoreResumeV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "native-training-store-v2-unsupported-platform",
            Self::StoreBusy => "native-training-store-busy",
            Self::RootInvalid => "native-training-store-resume-root-invalid",
            Self::RunInvalid => "native-training-store-resume-run-invalid",
            Self::ScheduleInvalid => "native-training-store-resume-schedule-invalid",
            Self::GenerationInvalid => "native-training-store-resume-generation-invalid",
            Self::LatestInvalid => "native-training-store-resume-latest-invalid",
            Self::StageCorruption => "native-training-store-resume-stage-corruption",
            Self::ReconstructionFailed => "native-training-store-resume-reconstruction-failed",
            Self::MutationFailed => "native-training-store-resume-mutation-failed",
        }
    }
}

/// Redacted resume error carrying only its classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingStoreResumeV2Error {
    kind: NativeTrainingStoreResumeV2ErrorKind,
}

impl NativeTrainingStoreResumeV2Error {
    pub const fn kind(self) -> NativeTrainingStoreResumeV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeTrainingStoreResumeV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingStoreResumeV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingStoreResumeV2Error>;

const fn resume_error_v2(
    kind: NativeTrainingStoreResumeV2ErrorKind,
) -> NativeTrainingStoreResumeV2Error {
    NativeTrainingStoreResumeV2Error { kind }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeTrainingStoreFinalExpectationV2 {
    final_name: NativeTrainingStoreFinalNameV2,
    expectation: DurableFileExpectationV1,
}

impl NativeTrainingStoreFinalExpectationV2 {
    pub(crate) const fn final_name(self) -> NativeTrainingStoreFinalNameV2 {
        self.final_name
    }

    pub(crate) const fn expectation(self) -> DurableFileExpectationV1 {
        self.expectation
    }
}

/// Sealed proof that the whole Store validated as one coherent chain.
#[derive(Debug)]
pub struct ValidatedNativeTrainingStoreStateV2 {
    latest_generation_index: u64,
    latest_checkpoint: CheckpointManifestV3,
    latest_boundary: ValidatedNativeTrainingBoundaryV2,
    latest_reference: ValidatedCheckpointReferenceV2,
    latest_payload: Vec<u8>,
    recognized_stage_paths: Vec<PathBuf>,
    final_expectations: Vec<NativeTrainingStoreFinalExpectationV2>,
}

impl ValidatedNativeTrainingStoreStateV2 {
    pub const fn latest_generation_index(&self) -> u64 {
        self.latest_generation_index
    }

    /// Exact reopened state-payload bytes of the latest boundary.
    pub fn latest_payload(&self) -> &[u8] {
        &self.latest_payload
    }

    pub const fn latest_checkpoint(&self) -> &CheckpointManifestV3 {
        &self.latest_checkpoint
    }

    pub const fn latest_boundary(&self) -> &ValidatedNativeTrainingBoundaryV2 {
        &self.latest_boundary
    }

    pub const fn latest_reference(&self) -> &ValidatedCheckpointReferenceV2 {
        &self.latest_reference
    }

    pub(crate) fn final_expectations_v2(&self) -> &[NativeTrainingStoreFinalExpectationV2] {
        &self.final_expectations
    }
}

/// Resume decision after the exclusive validation and cleanup pass.
#[derive(Debug)]
pub enum NativeTrainingStoreResumeV2 {
    /// `P = N`: the exact no-op. No stage or final was created, latest was
    /// not replaced, no executor was reconstructed, and no live state moved.
    Complete { latest_generation_index: u64 },
    /// `P < N`: the boxed continuation holds a fresh executor with the
    /// reconstructed latest candidate plus the walked parent authorities.
    Continue(Box<NativeTrainingStoreResumeContinueV2>),
}

/// Reconstructed continuation state for the next `S`-update window.
#[derive(Debug)]
pub struct NativeTrainingStoreResumeContinueV2 {
    pub executor: NativeTrainingExecutorV1,
    pub parent_checkpoint: CheckpointManifestV3,
    pub parent_boundary: ValidatedNativeTrainingBoundaryV2,
    pub parent_generation_index: u64,
    pub target_generation_index: u64,
}

/// Validate the complete Store under the shared reader lock.
///
/// This deletes nothing and mutates nothing: recognized stage leaves are
/// reported valid-for-cleanup, while unknown or malformed leaves fail closed.
pub fn validate_native_training_store_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
) -> Result<ValidatedNativeTrainingStoreStateV2> {
    root.recapture_v2()
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::RootInvalid))?;
    let _shared = root.lock_shared_v2().map_err(map_lock_error_v2)?;
    walk_complete_store_v2(root, run)
}

/// One named boundary generation loaded strictly from validated Store bytes.
#[derive(Debug)]
pub struct LoadedNativeTrainingBoundaryV2 {
    generation_index: u64,
    checkpoint: CheckpointManifestV3,
    payload: Vec<u8>,
}

impl LoadedNativeTrainingBoundaryV2 {
    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub const fn checkpoint(&self) -> &CheckpointManifestV3 {
        &self.checkpoint
    }

    /// Exact reopened state-payload bytes of the named boundary.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Load one named boundary generation under the shared reader lock.
///
/// The complete Store is first validated through the full walk, the named
/// generation must not exceed the proven latest pointer, and the boundary is
/// then rewalked from genesis so its complete ancestry, evidence chain,
/// checkpoint reference, and payload all revalidate before any byte is
/// returned. Runner and evaluator loads consume exactly this authority; the
/// locator is never persisted.
pub fn load_native_training_boundary_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    generation_index: u64,
) -> Result<LoadedNativeTrainingBoundaryV2> {
    root.recapture_v2()
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::RootInvalid))?;
    let _shared = root.lock_shared_v2().map_err(map_lock_error_v2)?;
    let state = walk_complete_store_v2(root, run)?;
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    if generation_index > state.latest_generation_index
        || !(generation_index == 0 || generation_index.is_multiple_of(checkpoint_segment_updates))
    {
        return Err(resume_error_v2(
            NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid,
        ));
    }
    if generation_index == state.latest_generation_index {
        return Ok(LoadedNativeTrainingBoundaryV2 {
            generation_index,
            checkpoint: state.latest_checkpoint,
            payload: state.latest_payload,
        });
    }
    let schedule_invalid = resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::ScheduleInvalid);
    let mut walked: Option<WalkedGenerationV2> = None;
    let mut current = 0_u64;
    loop {
        let generation = load_generation_v2(root, run, walked.as_ref(), current)?;
        if current == generation_index {
            return Ok(LoadedNativeTrainingBoundaryV2 {
                generation_index,
                checkpoint: generation.checkpoint,
                payload: generation.payload,
            });
        }
        walked = Some(generation);
        current = current
            .checked_add(checkpoint_segment_updates)
            .ok_or(schedule_invalid)?;
    }
}

/// Resume the Store under the exclusive mutator lock.
///
/// Applies only the complete prevalidated recognized-stage deletion plan,
/// then either proves the `P = N` no-op or reconstructs the latest candidate
/// into a fresh executor for the next window.
pub fn resume_native_training_store_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    config: NativeTrainingExecutionConfigV1,
) -> Result<NativeTrainingStoreResumeV2> {
    validate_prepared_execution_config_v1(run, &config)
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::RunInvalid))?;
    root.recapture_v2()
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::RootInvalid))?;
    let _exclusive = root.lock_exclusive_v2().map_err(map_lock_error_v2)?;
    let state = walk_complete_store_v2(root, run)?;

    // Apply only the complete prevalidated recognized-stage deletion plan.
    for stage_path in &state.recognized_stage_paths {
        std::fs::remove_file(stage_path)
            .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::MutationFailed))?;
    }
    if !state.recognized_stage_paths.is_empty() {
        // Rescan to require stage absence after the plan is applied.
        let rescanned = walk_complete_store_v2(root, run)?;
        if !rescanned.recognized_stage_paths.is_empty() {
            return Err(resume_error_v2(
                NativeTrainingStoreResumeV2ErrorKind::StageCorruption,
            ));
        }
    }

    let target = run.requested_successful_updates();
    let latest = state.latest_generation_index;
    if latest > target {
        return Err(resume_error_v2(
            NativeTrainingStoreResumeV2ErrorKind::ScheduleInvalid,
        ));
    }
    if latest == target {
        // The no-op revalidates the unchanged latest boundary hashes and
        // performs no reconstruction, publication, or live mutation.
        let reread = walk_complete_store_v2(root, run)?;
        if reread.latest_generation_index != latest
            || reread.latest_boundary.head_record_sha256()
                != state.latest_boundary.head_record_sha256()
            || reread.latest_boundary.head_sha256() != state.latest_boundary.head_sha256()
        {
            return Err(resume_error_v2(
                NativeTrainingStoreResumeV2ErrorKind::LatestInvalid,
            ));
        }
        return Ok(NativeTrainingStoreResumeV2::Complete {
            latest_generation_index: latest,
        });
    }

    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    latest
        .checked_add(checkpoint_segment_updates)
        .filter(|next| *next <= target)
        .ok_or(resume_error_v2(
            NativeTrainingStoreResumeV2ErrorKind::ScheduleInvalid,
        ))?;
    let executor = reconstruct_executor_v2(run, &state, config)?;
    Ok(NativeTrainingStoreResumeV2::Continue(Box::new(
        NativeTrainingStoreResumeContinueV2 {
            executor,
            parent_generation_index: latest,
            target_generation_index: target,
            parent_checkpoint: state.latest_checkpoint,
            parent_boundary: state.latest_boundary,
        },
    )))
}

fn map_lock_error_v2(
    error: crate::native_training_store_root_v2::NativeTrainingStoreRootV2Error,
) -> NativeTrainingStoreResumeV2Error {
    use crate::native_training_store_root_v2::NativeTrainingStoreRootV2ErrorKind;
    resume_error_v2(match error.kind() {
        NativeTrainingStoreRootV2ErrorKind::StoreBusy => {
            NativeTrainingStoreResumeV2ErrorKind::StoreBusy
        }
        NativeTrainingStoreRootV2ErrorKind::UnsupportedPlatform => {
            NativeTrainingStoreResumeV2ErrorKind::UnsupportedPlatform
        }
        _ => NativeTrainingStoreResumeV2ErrorKind::RootInvalid,
    })
}

fn read_bounded_final_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    final_name: NativeTrainingStoreFinalNameV2,
    max_bytes: u64,
    kind: NativeTrainingStoreResumeV2ErrorKind,
) -> Result<Vec<u8>> {
    let error = resume_error_v2(kind);
    let basename = final_name.final_basename().map_err(|_| error)?;
    let path = root
        .directory_path_v2(final_name.directory())
        .join(basename);
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        return Err(error);
    }
    std::fs::read(&path).map_err(|_| error)
}

fn final_exists_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    final_name: NativeTrainingStoreFinalNameV2,
) -> bool {
    final_name
        .final_basename()
        .map(|basename| {
            root.directory_path_v2(final_name.directory())
                .join(basename)
                .symlink_metadata()
                .is_ok()
        })
        .unwrap_or(false)
}

struct WalkedGenerationV2 {
    checkpoint: CheckpointManifestV3,
    boundary: ValidatedNativeTrainingBoundaryV2,
    reference: ValidatedCheckpointReferenceV2,
    payload: Vec<u8>,
    continuation_count: u64,
    final_expectations: Vec<NativeTrainingStoreFinalExpectationV2>,
}

fn final_expectation_v2(
    final_name: NativeTrainingStoreFinalNameV2,
    bytes: &[u8],
) -> Result<NativeTrainingStoreFinalExpectationV2> {
    let expectation = DurableFileExpectationV1::from_bytes(bytes)
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid))?;
    Ok(NativeTrainingStoreFinalExpectationV2 {
        final_name,
        expectation,
    })
}

fn load_generation_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: Option<&WalkedGenerationV2>,
    generation_index: u64,
) -> Result<WalkedGenerationV2> {
    let kind = NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid;
    let error = resume_error_v2(kind);
    let payload = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 as u64,
        kind,
    )?;
    let manifest = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index },
        CHECKPOINT_MANIFEST_MAX_BYTES_V3 as u64,
        kind,
    )?;
    let segment_manifest = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index },
        SEGMENT_MANIFEST_MAX_BYTES_V2,
        kind,
    )?;
    let sidecar = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index },
        CHECKPOINT_SIDECAR_MAX_BYTES_V2,
        kind,
    )?;
    let head = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::HeadRecord { generation_index },
        HEAD_RECORD_MAX_BYTES_V2,
        kind,
    )?;
    let reference_bytes = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::CheckpointReference { generation_index },
        CHECKPOINT_REFERENCE_MAX_BYTES_V2,
        kind,
    )?;

    let mut continuation_bytes: Vec<Vec<u8>> = Vec::new();
    let (checkpoint, boundary, continuation_count) = match parent {
        None => {
            let checkpoint =
                decode_checkpoint_manifest_v3(&manifest, &payload, run).map_err(|_| error)?;
            let segment = decode_genesis_segment_manifest_v2(&segment_manifest, run, &checkpoint)
                .map_err(|_| error)?;
            let boundary = decode_genesis_native_training_boundary_v2(
                &sidecar,
                &head,
                run,
                &segment,
                &checkpoint,
            )
            .map_err(|_| error)?;
            (checkpoint, boundary, 0_u64)
        }
        Some(parent) => {
            loop {
                let continuation_index =
                    u64::try_from(continuation_bytes.len()).map_err(|_| error)?;
                let final_name = NativeTrainingStoreFinalNameV2::SegmentContinuation {
                    generation_index,
                    continuation_index,
                };
                if !final_exists_v2(root, final_name) {
                    break;
                }
                continuation_bytes.push(read_bounded_final_v2(
                    root,
                    final_name,
                    SEGMENT_CONTINUATION_MAX_BYTES_V2,
                    kind,
                )?);
            }
            if continuation_bytes.is_empty() {
                return Err(error);
            }
            let parent_context =
                resume_update_evidence_chain_v1(run, &parent.boundary, &parent.checkpoint)
                    .map_err(|_| error)?;
            let continuations =
                decode_segment_continuations_v2(run, parent_context, &continuation_bytes)
                    .map_err(|_| error)?;
            let checkpoint = decode_trained_checkpoint_manifest_v3(
                &manifest,
                &payload,
                run,
                continuations.advanced_context(),
            )
            .map_err(|_| error)?;
            let segment = decode_trained_segment_manifest_v2(
                &segment_manifest,
                run,
                &parent.boundary,
                &continuations,
                &checkpoint,
            )
            .map_err(|_| error)?;
            let boundary = decode_trained_native_training_boundary_v2(
                &sidecar,
                &head,
                run,
                &parent.boundary,
                &segment,
                &checkpoint,
            )
            .map_err(|_| error)?;
            let continuation_count = u64::try_from(continuation_bytes.len()).map_err(|_| error)?;
            (checkpoint, boundary, continuation_count)
        }
    };
    if checkpoint.generation_index() != generation_index {
        return Err(error);
    }
    let reference =
        decode_checkpoint_reference_v2(&reference_bytes, run, &boundary).map_err(|_| error)?;
    let mut final_expectations = Vec::with_capacity(6 + continuation_bytes.len());
    final_expectations.push(final_expectation_v2(
        NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        &payload,
    )?);
    final_expectations.push(final_expectation_v2(
        NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index },
        &manifest,
    )?);
    for (continuation_index, continuation) in continuation_bytes.iter().enumerate() {
        let continuation_index = u64::try_from(continuation_index).map_err(|_| error)?;
        final_expectations.push(final_expectation_v2(
            NativeTrainingStoreFinalNameV2::SegmentContinuation {
                generation_index,
                continuation_index,
            },
            continuation,
        )?);
    }
    final_expectations.push(final_expectation_v2(
        NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index },
        &segment_manifest,
    )?);
    final_expectations.push(final_expectation_v2(
        NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index },
        &sidecar,
    )?);
    final_expectations.push(final_expectation_v2(
        NativeTrainingStoreFinalNameV2::HeadRecord { generation_index },
        &head,
    )?);
    final_expectations.push(final_expectation_v2(
        NativeTrainingStoreFinalNameV2::CheckpointReference { generation_index },
        &reference_bytes,
    )?);
    Ok(WalkedGenerationV2 {
        checkpoint,
        boundary,
        reference,
        payload,
        continuation_count,
        final_expectations,
    })
}

/// Validate `run.json`, walk every boundary generation in order, prove the
/// latest pointer, and inventory every leaf in the Store.
fn walk_complete_store_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
) -> Result<ValidatedNativeTrainingStoreStateV2> {
    // Schedule identities: K, S, N, checked K*S and K*N, S | N.
    let schedule_invalid = resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::ScheduleInvalid);
    let batch_episodes = run.batch_episodes();
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    let target = run.requested_successful_updates();
    if checkpoint_segment_updates == 0
        || checkpoint_segment_updates > target
        || !target.is_multiple_of(checkpoint_segment_updates)
        || batch_episodes
            .checked_mul(checkpoint_segment_updates)
            .is_none()
        || batch_episodes.checked_mul(target).is_none()
    {
        return Err(schedule_invalid);
    }

    // run.json must be byte-identical to the validated run authority.
    let run_bytes = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::Run,
        RUN_RECORD_MAX_BYTES_V2,
        NativeTrainingStoreResumeV2ErrorKind::RunInvalid,
    )?;
    if run_bytes != run.canonical_bytes() {
        return Err(resume_error_v2(
            NativeTrainingStoreResumeV2ErrorKind::RunInvalid,
        ));
    }

    // The latest pointer names the walk target; full binding is proven after
    // the walk against the walked authorities.
    let latest_bytes = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::Latest,
        LATEST_RECORD_MAX_BYTES_V2,
        NativeTrainingStoreResumeV2ErrorKind::LatestInvalid,
    )?;
    let latest_generation_index = peek_latest_generation_index_v2(&latest_bytes)
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::LatestInvalid))?;
    if latest_generation_index > target
        || !(latest_generation_index == 0
            || latest_generation_index.is_multiple_of(checkpoint_segment_updates))
    {
        return Err(resume_error_v2(
            NativeTrainingStoreResumeV2ErrorKind::LatestInvalid,
        ));
    }

    let mut final_expectations = vec![
        final_expectation_v2(NativeTrainingStoreFinalNameV2::Run, &run_bytes)?,
        final_expectation_v2(NativeTrainingStoreFinalNameV2::Latest, &latest_bytes)?,
    ];

    // Fully validate every reachable boundary generation in order.
    let mut walked: Option<WalkedGenerationV2> = None;
    let mut continuation_counts: BTreeMap<u64, u64> = BTreeMap::new();
    let mut generation_index = 0_u64;
    loop {
        let generation = load_generation_v2(root, run, walked.as_ref(), generation_index)?;
        continuation_counts.insert(generation_index, generation.continuation_count);
        final_expectations.extend(generation.final_expectations.iter().copied());
        walked = Some(generation);
        if generation_index == latest_generation_index {
            break;
        }
        generation_index = generation_index
            .checked_add(checkpoint_segment_updates)
            .ok_or(schedule_invalid)?;
    }
    let latest_walked = walked.expect("the walk always validates generation zero");

    // Prove the latest pointer against the walked authorities.
    decode_latest_v2(
        &latest_bytes,
        &latest_walked.boundary,
        &latest_walked.reference,
    )
    .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::LatestInvalid))?;

    // Inventory every leaf: walked finals, at most one partial next boundary,
    // recognized stages, the lock, and nothing else.
    let next_boundary = latest_generation_index.checked_add(checkpoint_segment_updates);
    let partial_allowed =
        next_boundary.filter(|next| *next <= target && latest_generation_index < target);
    let mut recognized_stage_paths = Vec::new();
    let generation_valid = |index: u64, continuation: Option<u64>| -> bool {
        if let Some(count) = continuation_counts.get(&index) {
            return match continuation {
                None => true,
                Some(continuation_index) => continuation_index < *count,
            };
        }
        if partial_allowed == Some(index) {
            // Partial finals for exactly the next expected boundary await
            // candidate-equality reuse or replay by the publisher.
            return true;
        }
        false
    };
    for directory in [
        NativeTrainingStoreDirectoryV2::Root,
        NativeTrainingStoreDirectoryV2::Segments,
        NativeTrainingStoreDirectoryV2::Checkpoints,
        NativeTrainingStoreDirectoryV2::Heads,
        NativeTrainingStoreDirectoryV2::Refs,
    ] {
        let corruption = resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::StageCorruption);
        let directory_path = root.directory_path_v2(directory);
        for entry in std::fs::read_dir(directory_path).map_err(|_| corruption)? {
            let entry = entry.map_err(|_| corruption)?;
            let file_name = entry.file_name();
            let Some(leaf) = file_name.to_str() else {
                return Err(corruption);
            };
            let file_type = entry.file_type().map_err(|_| corruption)?;
            if file_type.is_symlink() {
                return Err(corruption);
            }
            if matches!(directory, NativeTrainingStoreDirectoryV2::Root)
                && NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
                    .iter()
                    .any(|subdirectory| subdirectory.basename() == Some(leaf))
            {
                if !file_type.is_dir() {
                    return Err(corruption);
                }
                continue;
            }
            match classify_store_leaf_v2(directory, leaf) {
                Ok(NativeTrainingStoreLeafV2::Lock) => {}
                Ok(NativeTrainingStoreLeafV2::Stage(_)) => {
                    if !file_type.is_file() {
                        return Err(corruption);
                    }
                    recognized_stage_paths.push(entry.path());
                }
                Ok(NativeTrainingStoreLeafV2::Final(final_name)) => {
                    if !file_type.is_file() {
                        return Err(corruption);
                    }
                    let admitted = match final_name {
                        NativeTrainingStoreFinalNameV2::Run
                        | NativeTrainingStoreFinalNameV2::Latest => true,
                        NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index }
                        | NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index }
                        | NativeTrainingStoreFinalNameV2::StatePayload { generation_index }
                        | NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index }
                        | NativeTrainingStoreFinalNameV2::HeadRecord { generation_index }
                        | NativeTrainingStoreFinalNameV2::CheckpointReference {
                            generation_index,
                        } => generation_valid(generation_index, None),
                        NativeTrainingStoreFinalNameV2::SegmentContinuation {
                            generation_index,
                            continuation_index,
                        } => generation_valid(generation_index, Some(continuation_index)),
                    };
                    if !admitted {
                        return Err(resume_error_v2(
                            NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid,
                        ));
                    }
                }
                Err(_) => return Err(corruption),
            }
        }
    }

    Ok(ValidatedNativeTrainingStoreStateV2 {
        latest_generation_index,
        latest_checkpoint: latest_walked.checkpoint,
        latest_boundary: latest_walked.boundary,
        latest_reference: latest_walked.reference,
        latest_payload: latest_walked.payload,
        recognized_stage_paths,
        final_expectations,
    })
}

/// Read-only whole-Store validation for a publisher that already owns the
/// exclusive mutator lock. The sealed walked state lets publication compare
/// its supplied parent with the current disk authority, while the cleanup plan
/// remains private and untouched. Every committed prior generation, the
/// current latest pointer, and the at-most-one partial next generation must
/// form an admissible inventory before the publisher mutates anything.
pub(crate) fn validate_native_training_store_for_publication_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
) -> std::result::Result<ValidatedNativeTrainingStoreStateV2, NativeTrainingStoreResumeV2Error> {
    walk_complete_store_v2(root, run)
}

/// Decode the latest checkpoint into a private candidate and swap it into a
/// fresh executor. Every metadata and digest fact comes from the validated
/// checkpoint authority; nothing is caller-overridable.
fn reconstruct_executor_v2(
    run: &ValidatedTrainRunV2,
    state: &ValidatedNativeTrainingStoreStateV2,
    config: NativeTrainingExecutionConfigV1,
) -> Result<NativeTrainingExecutorV1> {
    let failed = resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::ReconstructionFailed);
    let checkpoint = &state.latest_checkpoint;
    let progress = checkpoint.progress();
    let policy_steps = progress
        .learner_policy_steps_by_seat()
        .p0()
        .checked_add(progress.learner_policy_steps_by_seat().p1())
        .ok_or(failed)?;
    let physical_decisions = progress
        .learner_physical_decisions_by_seat()
        .p0()
        .checked_add(progress.learner_physical_decisions_by_seat().p1())
        .ok_or(failed)?;
    let train_state = checkpoint.train_state();
    let scorer_bias_anchor_bits =
        u32::try_from(train_state.scorer_bias_anchor_f32_bits()).map_err(|_| failed)?;
    let metadata = NativeTrainingCheckpointMetadataV1 {
        base_seed: run.record().schedule.base_seed,
        batch_episodes: run.batch_episodes(),
        numerical_backend: config.numerical_backend,
        backward_worker_limit: config.backward_worker_limit,
        progress: NativeTrainingProgressV1 {
            next_episode_index: progress.next_episode_index(),
            successful_update_count: progress.successful_update_count(),
            completed_episode_count: progress.completed_episode_count(),
            learner_physical_decision_count: physical_decisions,
            learner_policy_step_count: policy_steps,
        },
        adam_step: train_state.adam_step(),
        scorer_bias_anchor_bits,
    };
    let payload_binding = checkpoint.payload();
    let digests = NativeTrainingCheckpointDigestsV1 {
        payload_sha256: parse_lower_hex_raw32_v1(&payload_binding.sha256).map_err(|_| failed)?,
        parameters_sha256: parse_lower_hex_raw32_v1(&payload_binding.sections[0].sha256)
            .map_err(|_| failed)?,
        first_moments_sha256: parse_lower_hex_raw32_v1(&payload_binding.sections[1].sha256)
            .map_err(|_| failed)?,
        second_moments_sha256: parse_lower_hex_raw32_v1(&payload_binding.sections[2].sha256)
            .map_err(|_| failed)?,
        model_parameter_sha256: parse_lower_hex_raw32_v1(train_state.model_parameter_sha256())
            .map_err(|_| failed)?,
        native_state_sha256: parse_lower_hex_raw32_v1(train_state.state_sha256())
            .map_err(|_| failed)?,
    };
    let candidate = NativeTrainingCheckpointCandidateV1::import_verified_v1(
        metadata,
        &state.latest_payload,
        digests,
    )
    .map_err(|_| failed)?;
    NativeTrainingExecutorV1::from_checkpoint_candidate_v1(config, &candidate).map_err(|_| failed)
}

/// Frozen production execution configuration for tests: every value is
/// derived from the validated run plus the fixed Sequential/one-worker
/// production tuple; nothing is overridable. Consumed only by the
/// Windows-gated store and resume test suites.
#[cfg(test)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn test_execution_config_v2(
    run: &ValidatedTrainRunV2,
) -> NativeTrainingExecutionConfigV1 {
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    NativeTrainingExecutionConfigV1 {
        run_base_seed: run.record().schedule.base_seed,
        batch_episodes: run.batch_episodes(),
        deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
        max_physical_decisions: run.record().limits.max_physical_decisions,
        max_policy_steps: run.record().limits.max_policy_steps,
        worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
        sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker).unwrap(),
        broker_batch_target: usize::try_from(run.record().topology.broker_batch_target).unwrap(),
        scheduler_timeout: std::time::Duration::from_secs(30),
        measure_broker_service_time: false,
        value_coefficient_bits: 0.5_f32.to_bits(),
        learning_rate_bits: 0.001_f32.to_bits(),
        numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
        backward_worker_limit: 1,
    }
}

#[cfg(all(test, windows))]
mod windows_resume_tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_training_store_bootstrap_v2::{
        bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapOutcomeV2,
    };
    use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
    use crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3;
    use crate::native_training_store_prepared_segment_v2::prepare_segment_v2;
    use crate::native_training_store_reference_latest_v2::{
        build_checkpoint_reference_v2, build_latest_v2,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
    use crate::native_training_store_v2::{
        publish_genesis_generation_v2, publish_prepared_segment_v2,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestParentV2 {
        parent: PathBuf,
    }

    impl TestParentV2 {
        fn new(label: &str) -> Self {
            static ORDINAL: AtomicU64 = AtomicU64::new(0);
            let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
            let parent = std::env::temp_dir().join(format!(
                "mtg-kernel-store-resume-v2-{}-{label}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&parent).expect("create test parent");
            Self { parent }
        }

        fn path(&self) -> &Path {
            &self.parent
        }
    }

    impl Drop for TestParentV2 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.parent);
        }
    }

    use super::test_execution_config_v2 as execution_config_v2;

    fn fresh_executor_v2(run: &ValidatedTrainRunV2) -> NativeTrainingExecutorV1 {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v2(run),
            &manifest,
            &payload,
        )
        .unwrap()
    }

    fn bootstrap_and_publish_genesis_v2(
        parent: &Path,
        run: &ValidatedTrainRunV2,
    ) -> ValidatedNativeTrainingStoreRootV2 {
        let bootstrapped = bootstrap_native_training_store_v2(parent, "store").unwrap();
        assert_eq!(
            bootstrapped.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        );
        let root = bootstrapped.into_root();
        let executor = fresh_executor_v2(run);
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

    #[test]
    fn resume_drives_the_full_run_from_reconstructed_executors_to_the_exact_no_op() {
        let parent = TestParentV2::new("lifecycle");
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let root = bootstrap_and_publish_genesis_v2(parent.path(), &run);
        let target = run.requested_successful_updates();
        let checkpoint_segment_updates = run.checkpoint_segment_updates();

        // Every window runs on a freshly reconstructed executor: the resume
        // path, not the original in-memory trainer, carries the run forward.
        let mut expected_parent = 0_u64;
        loop {
            match resume_native_training_store_v2(&root, &run, execution_config_v2(&run)).unwrap() {
                NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                } => {
                    assert_eq!(latest_generation_index, target);
                    break;
                }
                NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                    assert_eq!(continuation.parent_generation_index, expected_parent);
                    assert_eq!(continuation.target_generation_index, target);
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .unwrap();
                    let receipt = publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    expected_parent += checkpoint_segment_updates;
                }
            }
        }
        assert_eq!(expected_parent, target);

        let state = validate_native_training_store_v2(&root, &run).unwrap();
        assert_eq!(state.latest_generation_index(), target);
        assert_eq!(state.latest_checkpoint().generation_index(), target);

        // The exact no-op deletes recognized stages and nothing else.
        let stray_stage = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Segments)
            .join(".segment-00000000.json.stage-v2");
        fs::write(&stray_stage, b"stale").unwrap();
        match resume_native_training_store_v2(&root, &run, execution_config_v2(&run)).unwrap() {
            NativeTrainingStoreResumeV2::Complete {
                latest_generation_index,
            } => assert_eq!(latest_generation_index, target),
            NativeTrainingStoreResumeV2::Continue(_) => {
                panic!("a completed run must resume as the exact no-op")
            }
        }
        assert!(
            fs::symlink_metadata(&stray_stage).is_err(),
            "the recognized-stage deletion plan must run under the lock"
        );

        // An immutable final beyond the target is corruption and preserved.
        let beyond = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Refs)
            .join("update-00000016.ref.json");
        fs::write(&beyond, b"{}").unwrap();
        assert_eq!(
            validate_native_training_store_v2(&root, &run)
                .unwrap_err()
                .kind(),
            NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid
        );
        assert_eq!(fs::read(&beyond).unwrap(), b"{}");
        fs::remove_file(&beyond).unwrap();

        // A same-length corruption of a mid-chain final fails the walk.
        let sidecar_path = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join("update-00000004.sidecar.json");
        let original = fs::read(&sidecar_path).unwrap();
        let corrupted: Vec<u8> = original.iter().map(|byte| byte ^ 0x01).collect();
        fs::write(&sidecar_path, &corrupted).unwrap();
        assert_eq!(
            validate_native_training_store_v2(&root, &run)
                .unwrap_err()
                .kind(),
            NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid
        );
        assert_eq!(fs::read(&sidecar_path).unwrap(), corrupted);
        fs::write(&sidecar_path, &original).unwrap();
        let _ = validate_native_training_store_v2(&root, &run).unwrap();

        // An unknown leaf is corruption and preserved.
        let unknown = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Heads)
            .join("notes.txt");
        fs::write(&unknown, b"evidence").unwrap();
        assert_eq!(
            validate_native_training_store_v2(&root, &run)
                .unwrap_err()
                .kind(),
            NativeTrainingStoreResumeV2ErrorKind::StageCorruption
        );
        assert_eq!(fs::read(&unknown).unwrap(), b"evidence");
        fs::remove_file(&unknown).unwrap();

        // Another holder's exclusive lock reports store-busy to both paths.
        // The conflict is between distinct handles, as between processes.
        let other_holder = ValidatedNativeTrainingStoreRootV2::open_v2(root.root_path()).unwrap();
        let held = other_holder.lock_exclusive_v2().unwrap();
        assert_eq!(
            validate_native_training_store_v2(&root, &run)
                .unwrap_err()
                .kind(),
            NativeTrainingStoreResumeV2ErrorKind::StoreBusy
        );
        assert_eq!(
            resume_native_training_store_v2(&root, &run, execution_config_v2(&run))
                .unwrap_err()
                .kind(),
            NativeTrainingStoreResumeV2ErrorKind::StoreBusy
        );
        drop(held);
    }

    #[test]
    fn a_tampered_run_record_fails_closed_before_any_walk() {
        let parent = TestParentV2::new("tampered-run");
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let root = bootstrap_and_publish_genesis_v2(parent.path(), &run);
        let run_path = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Root)
            .join("run.json");
        let original = fs::read(&run_path).unwrap();
        let mut tampered = original.clone();
        let flip = tampered.len() / 2;
        tampered[flip] ^= 0x01;
        fs::write(&run_path, &tampered).unwrap();
        assert_eq!(
            validate_native_training_store_v2(&root, &run)
                .unwrap_err()
                .kind(),
            NativeTrainingStoreResumeV2ErrorKind::RunInvalid
        );
        assert_eq!(fs::read(&run_path).unwrap(), tampered);
    }
}
