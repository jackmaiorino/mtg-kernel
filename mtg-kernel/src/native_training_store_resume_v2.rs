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
use crate::native_train_state_payload_v1::{
    NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1, W_NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
};
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
    decode_genesis_checkpoint_manifest_dispatch_v2_v3, decode_trained_checkpoint_manifest_v3,
    CheckpointManifestV3, CHECKPOINT_MANIFEST_MAX_BYTES_V3,
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

/// StoreV3 port (D1) risk register: test-only instrumentation proving the
/// O(1) shortcut path never falls back to per-generation decoding. Uses
/// `thread_local!` rather than a process-wide counter deliberately: `cargo
/// test`'s default runner gives each `#[test]` function its own OS thread,
/// so a thread-local counter is immune to interference from unrelated tests
/// running concurrently, whereas a shared `static` would not be.
#[cfg(test)]
pub(crate) mod call_counters_v1 {
    use std::cell::Cell;

    thread_local! {
        static LOAD_GENERATION_CALLS_V1: Cell<u64> = const { Cell::new(0) };
    }

    /// Resets this test thread's counter to zero. Call before the
    /// in-process behavior under test begins.
    pub(crate) fn reset_load_generation_calls_v1() {
        LOAD_GENERATION_CALLS_V1.with(|cell| cell.set(0));
    }

    /// The number of `load_generation_v2` calls on this test thread since
    /// the last reset.
    pub(crate) fn load_generation_calls_v1() -> u64 {
        LOAD_GENERATION_CALLS_V1.with(Cell::get)
    }

    pub(crate) fn increment_load_generation_calls_v1() {
        LOAD_GENERATION_CALLS_V1.with(|cell| cell.set(cell.get() + 1));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreResumeV2ErrorKind {
    UnsupportedPlatform,
    StoreBusy,
    RootInvalid,
    RunInvalid,
    /// Dual-Profile Catalog Successor (collab CLAUDE #220): the supplied run
    /// classified as `NativeRunCatalogProfileV1::Historical` at decode time.
    /// Historical-profile records stay decodable and read-only-validatable
    /// forever (see `validate_native_training_store_v2`, which performs no
    /// such rejection), but the mutator path rejects them here, before the
    /// root is even recaptured.
    HistoricalCatalogProfile,
    /// Dual-Profile Catalog Successor fix round (panel finding 1, blocker:
    /// bypass): the supplied run classified `Current` at decode time, but its
    /// embedded catalog fields do not equal the crate's live build constants
    /// at this moment. Closes the bypass where a record merely claiming the
    /// pinned CURRENT literal (rather than being authored by a build whose
    /// live identity actually equals it right now) could still resume.
    CurrentCatalogProfileLiveMismatch,
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
            Self::HistoricalCatalogProfile => {
                "native-training-store-resume-historical-catalog-profile"
            }
            Self::CurrentCatalogProfileLiveMismatch => {
                "native-training-store-resume-current-catalog-profile-live-mismatch"
            }
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
    /// StoreV3 port (D1, `collab/CLAUDE-STOREV3-PORT-PLAN-V1.md` section 2):
    /// O(1)-sized proof that `parent_boundary`/`parent_checkpoint` were just
    /// confirmed current, carried forward so the publish call moments later
    /// in the same science-loop window can attempt the same O(1)
    /// reverification instead of walking the entire ancestry again. Always
    /// populated, on both the fast path and the full-walk fallback, so every
    /// caller of `publish_prepared_segment_with_session_v2` has it available
    /// regardless of which path this resume call took.
    pub(crate) tip_proof: NativeTrainingStoreTipProofV2,
    /// Consecutive windows since the last full walk, as of this resume call
    /// (see `NativeTrainingStoreContinuationSessionV2`'s own doc); `0` when
    /// this resume call itself just performed one. Publish uses this to
    /// derive the outgoing session's own counter.
    pub(crate) windows_since_full_walk: u32,
}

/// O(1)-sized proof that exactly one already-proven generation's own finals
/// (that generation's checkpoint manifest, state payload, segment manifest,
/// checkpoint sidecar, head record, checkpoint reference, and any segment
/// continuations, plus the Store's `latest.json`) were independently
/// confirmed current -- deliberately not the whole-history vector a full
/// walk produces, so re-verifying every entry here is O(1) in Store depth
/// rather than O(depth). `run.json` is deliberately not included: both
/// consumers of this bundle (the resume-side shortcut below and the
/// publish-side authority check in `native_training_store_v2.rs`)
/// independently reverify it unconditionally already, so carrying a
/// redundant copy here would add nothing.
#[derive(Clone, Debug)]
pub(crate) struct NativeTrainingStoreTipProofV2 {
    generation_index: u64,
    final_expectations: Vec<NativeTrainingStoreFinalExpectationV2>,
}

impl NativeTrainingStoreTipProofV2 {
    /// Constructed either by `tip_proof_from_walked_v1` (this module, from a
    /// full walk's result) or by `native_training_store_v2.rs`'s publisher
    /// (from the exact bytes it just durably published, already
    /// independently reopened and byte-compared before this is called).
    pub(crate) const fn new_v1(
        generation_index: u64,
        final_expectations: Vec<NativeTrainingStoreFinalExpectationV2>,
    ) -> Self {
        Self {
            generation_index,
            final_expectations,
        }
    }

    pub(crate) const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub(crate) fn final_expectations(&self) -> &[NativeTrainingStoreFinalExpectationV2] {
        &self.final_expectations
    }
}

/// Retained per-process proof that a Store's tip generation was already
/// fully proven current, carried from a successful publish into the
/// following science-loop window's resume call
/// (`resume_native_training_store_with_session_v2`) so it can attempt an
/// O(1) reverification instead of walking the entire ancestry again.
///
/// Move-only by construction (no `Clone`): every consumer takes it by value,
/// and only a full walk or a successful publish's own independent
/// revalidation ever produces one, so a caller can never resurrect a session
/// past a failed resume or a failed publish -- the failure path simply does
/// not hand one back, and (per the science loop's own `?`-propagation) no
/// further call happens against a session that was never returned.
#[derive(Debug)]
pub(crate) struct NativeTrainingStoreContinuationSessionV2 {
    tip_checkpoint: CheckpointManifestV3,
    tip_boundary: ValidatedNativeTrainingBoundaryV2,
    tip_reference: ValidatedCheckpointReferenceV2,
    tip_proof: NativeTrainingStoreTipProofV2,
    /// Consecutive windows (on either side) since the last full
    /// `walk_complete_store_v2` pass. Defense in depth (port plan section 6
    /// risk register, directory-inventory/foreign-writer risk): the O(1)
    /// freshness probe only reverifies the tip generation's own finals, not
    /// the full directory inventory a full walk scans, so an out-of-band
    /// foreign write elsewhere in the tree could otherwise go unnoticed for
    /// the life of a run. Forcing a full walk at least this often bounds
    /// that exposure even when every probe in between would have passed.
    windows_since_full_walk: u32,
}

impl NativeTrainingStoreContinuationSessionV2 {
    /// Constructed only from independently reopened/redecoded bytes: by the
    /// resume-side full walk (via `tip_proof_from_walked_v1`, this module),
    /// or by the publish side's own post-publication revalidation
    /// (`native_training_store_v2.rs`, which already reopens and redecodes
    /// every final of the generation it just published for receipt
    /// purposes).
    pub(crate) fn new_v1(
        tip_checkpoint: CheckpointManifestV3,
        tip_boundary: ValidatedNativeTrainingBoundaryV2,
        tip_reference: ValidatedCheckpointReferenceV2,
        tip_proof: NativeTrainingStoreTipProofV2,
        windows_since_full_walk: u32,
    ) -> Self {
        Self {
            tip_checkpoint,
            tip_boundary,
            tip_reference,
            tip_proof,
            windows_since_full_walk,
        }
    }
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
    #[cfg(test)]
    #[cfg_attr(not(windows), allow(dead_code))]
    boundary: ValidatedNativeTrainingBoundaryV2,
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

    /// Exact validated sidecar/head identity for the named boundary.
    #[cfg(test)]
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) const fn boundary(&self) -> &ValidatedNativeTrainingBoundaryV2 {
        &self.boundary
    }

    /// Exact reopened state-payload bytes of the named boundary.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the boundary, returning its owned checkpoint manifest and
    /// raw payload bytes. For callers (Self-Play Ladder Design Contract S2,
    /// Amendment 1 / Section 8A point 2's continual-initialization genesis
    /// authoring) that need to hold the resolved reference beyond this
    /// value's own lifetime, rather than only borrowing from it.
    pub fn into_checkpoint_and_payload(self) -> (CheckpointManifestV3, Vec<u8>) {
        (self.checkpoint, self.payload)
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
            #[cfg(test)]
            boundary: state.latest_boundary,
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
                #[cfg(test)]
                boundary: generation.boundary,
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
///
/// Always performs the full `walk_complete_store_v2` pass (no retained
/// session): byte-for-byte the same behavior every existing caller has
/// always had. See `resume_native_training_store_with_session_v2` for the
/// StoreV3 port's (D1) O(1) fast path.
pub fn resume_native_training_store_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    config: NativeTrainingExecutionConfigV1,
) -> Result<NativeTrainingStoreResumeV2> {
    resume_native_training_store_impl_v1(
        root,
        run,
        config,
        None,
        PERIODIC_FULL_WALK_CADENCE_WINDOWS_V1,
    )
}

/// StoreV3 port (D1, `collab/CLAUDE-STOREV3-PORT-PLAN-V1.md` section 2):
/// resume with an optional retained session from a prior window's publish.
/// A healthy in-process continuation (session present, its periodic-full-
/// walk cadence not yet elapsed, its O(1) freshness probe passing) takes the
/// shortcut instead of the full `walk_complete_store_v2` pass; any absence,
/// cadence expiry, or byte mismatch falls back to exactly today's full walk.
/// Identical to `resume_native_training_store_v2` in every other respect
/// (same error taxonomy, same deletion-plan/no-op/continuation logic) --
/// only which walk proves the state differs.
///
/// `pub(crate)`: this and `NativeTrainingStoreContinuationSessionV2` are
/// internal science-loop plumbing, not part of the crate's public resume
/// API surface (unlike `resume_native_training_store_v2`, which doc-tests
/// reference externally).
pub(crate) fn resume_native_training_store_with_session_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    config: NativeTrainingExecutionConfigV1,
    session: Option<NativeTrainingStoreContinuationSessionV2>,
) -> Result<NativeTrainingStoreResumeV2> {
    resume_native_training_store_impl_v1(
        root,
        run,
        config,
        session,
        PERIODIC_FULL_WALK_CADENCE_WINDOWS_V1,
    )
}

/// Test-only sibling of `resume_native_training_store_with_session_v2` that
/// takes the periodic-full-walk cadence as an explicit parameter instead of
/// the production constant, so the section 6 risk-register mitigation
/// itself (forcing a full walk after enough shortcut-only windows) can be
/// exercised deterministically against a small cadence in a fast unit test,
/// without touching any process-global state that could leak into other
/// tests running concurrently on a different thread.
#[cfg(test)]
pub(crate) fn resume_native_training_store_with_session_and_cadence_for_test_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    config: NativeTrainingExecutionConfigV1,
    session: Option<NativeTrainingStoreContinuationSessionV2>,
    cadence_windows: u32,
) -> Result<NativeTrainingStoreResumeV2> {
    resume_native_training_store_impl_v1(root, run, config, session, cadence_windows)
}

fn resume_native_training_store_impl_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    config: NativeTrainingExecutionConfigV1,
    session: Option<NativeTrainingStoreContinuationSessionV2>,
    cadence_windows: u32,
) -> Result<NativeTrainingStoreResumeV2> {
    // Dual-Profile Catalog Successor (collab CLAUDE #220), resume boundary:
    // reject a historical-profile run before any other check, lock, or store
    // mutation. `validate_native_training_store_v2` (the read-only walk used
    // to verify sealed evidence stores) deliberately performs no such
    // rejection; only this mutator path does. Exhaustive match (fix round,
    // panel finding 3): a future third profile variant fails this match at
    // compile time rather than silently resuming under it. The CURRENT arm
    // (fix round, panel finding 1, blocker: bypass) additionally requires the
    // record's own catalog fields to equal the crate's live build constants
    // at this moment, not merely the pinned CURRENT literal -- closing the
    // gap where a record merely claiming that literal, authored by a build
    // whose real identity has since moved past it, could still resume.
    use crate::native_training_store_run_v2::{
        current_profile_matches_live_build_identity_v1, NativeRunCatalogProfileV1,
    };
    match run.catalog_profile_v1() {
        NativeRunCatalogProfileV1::Historical => {
            return Err(resume_error_v2(
                NativeTrainingStoreResumeV2ErrorKind::HistoricalCatalogProfile,
            ));
        }
        NativeRunCatalogProfileV1::Current => {
            if !current_profile_matches_live_build_identity_v1(run.record().environment()) {
                return Err(resume_error_v2(
                    NativeTrainingStoreResumeV2ErrorKind::CurrentCatalogProfileLiveMismatch,
                ));
            }
        }
    }
    validate_prepared_execution_config_v1(run, &config)
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::RunInvalid))?;
    root.recapture_v2()
        .map_err(|_| resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::RootInvalid))?;
    let _exclusive = root.lock_exclusive_v2().map_err(map_lock_error_v2)?;
    let (state, windows_since_full_walk) =
        walk_complete_store_or_shortcut_v2(root, run, session, cadence_windows)?;

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
    let tip_proof = tip_proof_from_walked_v1(&state);
    let executor = reconstruct_executor_v2(run, &state, config)?;
    Ok(NativeTrainingStoreResumeV2::Continue(Box::new(
        NativeTrainingStoreResumeContinueV2 {
            executor,
            parent_generation_index: latest,
            target_generation_index: target,
            parent_checkpoint: state.latest_checkpoint,
            parent_boundary: state.latest_boundary,
            tip_proof,
            windows_since_full_walk,
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

/// `pub(crate)`: also used by `native_training_store_v2.rs` to build a
/// `NativeTrainingStoreTipProofV2` from bytes it independently reopens
/// during publication (StoreV3 port, D1).
pub(crate) fn final_expectation_v2(
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
    // StoreV3 port (D1) risk register: test-only call counter proving the
    // O(1) shortcut never reaches this function (only `walk_complete_store_v2`,
    // the O(depth) full walk, ever does), so a future regression that
    // silently reintroduces per-window walking on either the resume or the
    // publish side fails a test deterministically, independent of noisy
    // wall-clock timing. See `call_counters_v1` below.
    #[cfg(test)]
    call_counters_v1::increment_load_generation_calls_v1();
    let kind = NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid;
    let error = resume_error_v2(kind);
    // Capacity-experiment wide records carry a larger train-state payload;
    // the read bound dispatches on contracts.wide_model_experiment_v1 exactly
    // like the codec and wire validations (frozen path byte-for-byte).
    let payload_bound = if run.record().contracts.wide_model_experiment_v1.is_some() {
        W_NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1
    } else {
        NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1
    } as u64;
    let payload = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        payload_bound,
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
            // Design directive slice 2: the shared genesis-decode
            // chokepoint every walk/resume/publish path reaches -- see
            // `decode_genesis_checkpoint_manifest_dispatch_v2_v3`'s doc for
            // the dispatch rule.
            let checkpoint =
                decode_genesis_checkpoint_manifest_dispatch_v2_v3(&manifest, &payload, run)
                    .map_err(|_| error)?;
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

// --- StoreV3 port (D1): retained-session O(1) successor check --------------
//
// `collab/CLAUDE-STOREV3-PORT-PLAN-V1.md` section 2. `walk_complete_store_v2`
// above is the shared chokepoint both the resume side (this module) and the
// publish side (`native_training_store_v2.rs`, via
// `validate_native_training_store_for_publication_v2` just below) already
// fall through, so a single session-aware short-circuit here fixes both
// walks with one mechanism (plan section 1's "Note on the resume/publish
// relationship"). It is deliberately NOT built by making
// `walk_complete_store_v2` itself branch internally: that function, and its
// three unconditional resume-side call sites in
// `resume_native_training_store_v2` below, are left byte-for-byte as today,
// so every existing caller (this module's own tests, `native_training_store_v2.rs`'s
// tests, and the 20 other `publish_prepared_segment_v2` call sites) keeps the
// exact O(depth) behavior it has always had. Only the two NEW entry points
// (`resume_native_training_store_with_session_v2` below and
// `publish_prepared_segment_with_session_v2` in `native_training_store_v2.rs`)
// opt into the shortcut, per the plan's additive-API recommendation.

/// Periodic full-walk cadence (plan section 6 risk register, directory-
/// inventory/foreign-writer risk): the O(1) freshness probe below only
/// reverifies the tip generation's own finals plus `latest.json`, not the
/// full five-directory inventory scan `walk_complete_store_v2` performs --
/// that scan is what catches a foreign writer's leaf anywhere else in the
/// tree, and skipping it is exactly what makes the fast path O(1). Forcing a
/// full walk at least this often bounds how long such a foreign write could
/// otherwise go unnoticed, independent of whether every probe in between
/// would have passed. Chosen as a conservative, disclosed default (not
/// derived from the plan, which left the exact cadence to implementation);
/// not caller-configurable in production. `walk_complete_store_or_shortcut_v2`
/// takes the cadence as an explicit parameter (rather than reading this
/// constant directly) so a test can exercise the mitigation itself against a
/// small cadence, deterministically, without touching any process-global
/// state (an env var or a shared `static` would be read by every test
/// thread, including unrelated ones running concurrently) -- see
/// `resume_native_training_store_with_session_and_cadence_for_test_v1`.
const PERIODIC_FULL_WALK_CADENCE_WINDOWS_V1: u32 = 64;

/// The exact byte-count bound `load_generation_v2` already applies per final
/// kind (mirrored here, not refactored there, to avoid touching the existing
/// full-walk path at all).
fn max_bytes_for_final_v1(run: &ValidatedTrainRunV2, final_name: NativeTrainingStoreFinalNameV2) -> u64 {
    match final_name {
        NativeTrainingStoreFinalNameV2::Run => RUN_RECORD_MAX_BYTES_V2,
        NativeTrainingStoreFinalNameV2::Latest => LATEST_RECORD_MAX_BYTES_V2,
        NativeTrainingStoreFinalNameV2::StatePayload { .. } => {
            if run.record().contracts.wide_model_experiment_v1.is_some() {
                W_NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 as u64
            } else {
                NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 as u64
            }
        }
        NativeTrainingStoreFinalNameV2::CheckpointManifest { .. } => {
            CHECKPOINT_MANIFEST_MAX_BYTES_V3 as u64
        }
        NativeTrainingStoreFinalNameV2::SegmentManifest { .. } => SEGMENT_MANIFEST_MAX_BYTES_V2,
        NativeTrainingStoreFinalNameV2::CheckpointSidecar { .. } => CHECKPOINT_SIDECAR_MAX_BYTES_V2,
        NativeTrainingStoreFinalNameV2::HeadRecord { .. } => HEAD_RECORD_MAX_BYTES_V2,
        NativeTrainingStoreFinalNameV2::CheckpointReference { .. } => CHECKPOINT_REFERENCE_MAX_BYTES_V2,
        NativeTrainingStoreFinalNameV2::SegmentContinuation { .. } => SEGMENT_CONTINUATION_MAX_BYTES_V2,
    }
}

/// The generation a final name belongs to, or `None` for the two
/// generation-less finals (`Run`, `Latest`).
const fn final_name_generation_index_v1(final_name: NativeTrainingStoreFinalNameV2) -> Option<u64> {
    match final_name {
        NativeTrainingStoreFinalNameV2::Run | NativeTrainingStoreFinalNameV2::Latest => None,
        NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index }
        | NativeTrainingStoreFinalNameV2::SegmentContinuation {
            generation_index, ..
        }
        | NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index }
        | NativeTrainingStoreFinalNameV2::StatePayload { generation_index }
        | NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index }
        | NativeTrainingStoreFinalNameV2::HeadRecord { generation_index }
        | NativeTrainingStoreFinalNameV2::CheckpointReference { generation_index } => {
            Some(generation_index)
        }
    }
}

/// Projects a fully walked state's tip generation into an O(1)-sized proof:
/// `Latest` plus exactly the tip generation's own finals, dropping every
/// earlier generation's entries (and `Run`, per
/// `NativeTrainingStoreTipProofV2`'s own doc). Pure and read-only; `state` is
/// only borrowed.
fn tip_proof_from_walked_v1(state: &ValidatedNativeTrainingStoreStateV2) -> NativeTrainingStoreTipProofV2 {
    let tip = state.latest_generation_index();
    let final_expectations = state
        .final_expectations_v2()
        .iter()
        .copied()
        .filter(|expectation| match expectation.final_name() {
            NativeTrainingStoreFinalNameV2::Latest => true,
            NativeTrainingStoreFinalNameV2::Run => false,
            other => final_name_generation_index_v1(other) == Some(tip),
        })
        .collect();
    NativeTrainingStoreTipProofV2 {
        generation_index: tip,
        final_expectations,
    }
}

/// Attempts the O(1) successor check: re-reads exactly the retained
/// session's tip generation's own finals plus `latest.json` (never the full
/// five-directory inventory) and requires each to still byte-for-byte match
/// what was proven when the session was established. On success, reuses the
/// session's already-decoded authorities directly -- no re-walk, no
/// re-decode. Any I/O error or byte mismatch is `Err`, which the caller
/// (`walk_complete_store_or_shortcut_v2`) treats as "decline the shortcut,
/// fall back to a full walk," never as a hard failure of its own.
fn try_tip_shortcut_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    session: NativeTrainingStoreContinuationSessionV2,
) -> Result<ValidatedNativeTrainingStoreStateV2> {
    let declined = resume_error_v2(NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid);

    // run.json is always independently reverified here too, exactly like
    // `walk_complete_store_v2`'s own first check -- already O(1) regardless
    // of depth, so there is no shortcut to take for it, only to repeat it.
    let run_bytes = read_bounded_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::Run,
        RUN_RECORD_MAX_BYTES_V2,
        NativeTrainingStoreResumeV2ErrorKind::RunInvalid,
    )?;
    if run_bytes != run.canonical_bytes() {
        return Err(declined);
    }

    let tip = session.tip_proof.generation_index();
    let mut latest_payload: Option<Vec<u8>> = None;
    for expectation in session.tip_proof.final_expectations() {
        let final_name = expectation.final_name();
        let max_bytes = max_bytes_for_final_v1(run, final_name);
        let bytes = read_bounded_final_v2(root, final_name, max_bytes, declined.kind())?;
        if final_expectation_v2(final_name, &bytes)? != *expectation {
            return Err(declined);
        }
        if let NativeTrainingStoreFinalNameV2::StatePayload { generation_index } = final_name {
            if generation_index == tip {
                latest_payload = Some(bytes);
            }
        }
    }
    let latest_payload = latest_payload.ok_or(declined)?;

    Ok(ValidatedNativeTrainingStoreStateV2 {
        latest_generation_index: tip,
        latest_checkpoint: session.tip_checkpoint,
        latest_boundary: session.tip_boundary,
        latest_reference: session.tip_reference,
        latest_payload,
        recognized_stage_paths: Vec::new(),
        final_expectations: session.tip_proof.final_expectations,
    })
}

/// One walk at start/restart (plan section 2, item 1): with a retained
/// session, a cadence that has not yet elapsed, and a freshness probe that
/// passes, returns in O(1); any absence, cadence expiry, or mismatch falls
/// back to exactly `walk_complete_store_v2`, unchanged. Returns the proven
/// state plus the `windows_since_full_walk` count to carry forward (`0`
/// whenever a full walk just ran, since that resets the cadence clock).
fn walk_complete_store_or_shortcut_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    session: Option<NativeTrainingStoreContinuationSessionV2>,
    cadence_windows: u32,
) -> Result<(ValidatedNativeTrainingStoreStateV2, u32)> {
    if let Some(session) = session {
        if session.windows_since_full_walk < cadence_windows {
            let windows_since_full_walk = session.windows_since_full_walk;
            if let Ok(state) = try_tip_shortcut_v1(root, run, session) {
                return Ok((state, windows_since_full_walk));
            }
            // Any decline (mismatch, I/O error) falls through to the full
            // walk below, fail-closed: the shortcut never substitutes a
            // weaker check for a stronger one it disagrees with.
        }
    }
    Ok((walk_complete_store_v2(root, run)?, 0))
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
    // Capacity Experiment Contract (Stage 3), Section 3: every resumed
    // training window reconstructs its executor here, so this record-driven
    // dispatch on `contracts.wide_model_experiment_v1` (the same signal
    // genesis authoring and the inference-authority chokepoint already
    // dispatch on) is what makes `MULTIRUN_WIDE=1` actually train past
    // generation zero. Absent, this reproduces the frozen resume path
    // byte-for-byte.
    // Both branches reconstruct through the crate-private run-bound
    // constructors: checkpoint bytes deliberately carry no mode, so the
    // sealed trajectory contract is rederived from the validated run's own
    // decode-time classification on every resumed window.
    if run.record().contracts.wide_model_experiment_v1.is_some() {
        let candidate = NativeTrainingCheckpointCandidateV1::import_verified_wide_v1(
            metadata,
            &state.latest_payload,
            digests,
        )
        .map_err(|_error| failed)?;
        NativeTrainingExecutorV1::from_checkpoint_candidate_run_bound_wide_v2(
            config, &candidate, run,
        )
        .map_err(|_error| failed)
    } else {
        let candidate = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            metadata,
            &state.latest_payload,
            digests,
        )
        .map_err(|_| failed)?;
        NativeTrainingExecutorV1::from_checkpoint_candidate_run_bound_v2(config, &candidate, run)
            .map_err(|_| failed)
    }
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
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_historical_v1, test_fixture_bytes_v2,
    };
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

    /// Dual-Profile Catalog Successor (collab CLAUDE #220), resume boundary:
    /// a historical-profile run is rejected with the specific
    /// `HistoricalCatalogProfile` kind before the root is even recaptured or
    /// locked. Deliberately bootstraps only a bare skeleton (no genesis --
    /// publishing genesis with a historical run is itself rejected by the
    /// publisher boundary, so this test cannot depend on it): the mutator
    /// path's own check must fire first regardless of store contents.
    #[test]
    fn resume_rejects_a_historical_catalog_profile_run_before_any_store_interaction() {
        let parent = TestParentV2::new("historical-catalog-profile");
        let bootstrapped = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
        assert_eq!(
            bootstrapped.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        );
        let root = bootstrapped.into_root();
        let run = decode_train_run_v2(&test_fixture_bytes_historical_v1()).unwrap();

        let result = resume_native_training_store_v2(&root, &run, execution_config_v2(&run));

        assert_eq!(
            result.unwrap_err().kind(),
            NativeTrainingStoreResumeV2ErrorKind::HistoricalCatalogProfile
        );
    }

    /// Dual-Profile Catalog Successor fix round (panel finding 1, blocker:
    /// bypass), resume boundary: a CURRENT-profile run whose embedded
    /// catalog fields do not equal the crate's live build constants at this
    /// moment is rejected with the specific `CurrentCatalogProfileLiveMismatch`
    /// kind before the root is even recaptured. The crate's real live
    /// constants cannot be changed from a test, so this simulates a future
    /// catalog move via the module's own per-thread test shim
    /// (`LiveCatalogBuildIdentityOverrideGuardV1`): the record still claims
    /// the pinned CURRENT literal (and so still classifies `Current`), but
    /// the shimmed "live" identity has moved past it.
    #[test]
    fn resume_rejects_a_current_catalog_profile_run_whose_live_identity_has_moved() {
        use crate::native_training_store_run_v2::LiveCatalogBuildIdentityOverrideGuardV1;

        let parent = TestParentV2::new("current-catalog-profile-live-mismatch");
        let bootstrapped = bootstrap_native_training_store_v2(parent.path(), "store").unwrap();
        assert_eq!(
            bootstrapped.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        );
        let root = bootstrapped.into_root();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();

        let _shim = LiveCatalogBuildIdentityOverrideGuardV1::install(
            "ffffffffffffffff",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        );

        let result = resume_native_training_store_v2(&root, &run, execution_config_v2(&run));

        assert_eq!(
            result.unwrap_err().kind(),
            NativeTrainingStoreResumeV2ErrorKind::CurrentCatalogProfileLiveMismatch
        );
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

    // --- StoreV3 port (D1) tests -------------------------------------------
    //
    // `collab/CLAUDE-STOREV3-PORT-PLAN-V1.md`. These exercise the new
    // session-aware entry points (`resume_native_training_store_with_session_v2`,
    // `publish_prepared_segment_with_session_v2`) side by side with the
    // unchanged sync path, using the `call_counters_v1` instrumentation
    // (never wall-clock timing, which is noisy) to prove the O(1)-per-window
    // property directly.

    /// Byte-for-byte identical directory tree, compared by relative path
    /// (the two roots live under different temp directories).
    fn assert_store_trees_equal_v1(
        a: &ValidatedNativeTrainingStoreRootV2,
        b: &ValidatedNativeTrainingStoreRootV2,
    ) {
        for directory in [
            NativeTrainingStoreDirectoryV2::Root,
            NativeTrainingStoreDirectoryV2::Segments,
            NativeTrainingStoreDirectoryV2::Checkpoints,
            NativeTrainingStoreDirectoryV2::Heads,
            NativeTrainingStoreDirectoryV2::Refs,
        ] {
            let a_dir = a.directory_path_v2(directory);
            let b_dir = b.directory_path_v2(directory);
            let mut a_names: Vec<_> = fs::read_dir(&a_dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            let mut b_names: Vec<_> = fs::read_dir(&b_dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            a_names.sort();
            b_names.sort();
            assert_eq!(a_names, b_names, "directory listing mismatch in {directory:?}");
            for name in a_names {
                let a_path = a_dir.join(&name);
                if fs::symlink_metadata(&a_path).unwrap().is_dir() {
                    continue; // the four fixed subdirectories, walked separately above
                }
                assert_eq!(
                    fs::read(&a_path).unwrap(),
                    fs::read(b_dir.join(&name)).unwrap(),
                    "file content mismatch: {directory:?}/{name:?}"
                );
            }
        }
    }

    /// Gates (a) and (d): the session-aware path drives an identical run to
    /// a byte-for-byte identical Store versus the unchanged sync path, while
    /// calling `load_generation_v2` (only ever reachable from inside the
    /// O(depth) `walk_complete_store_v2`) far fewer times -- proving the
    /// fast path is taken, and that it changes nothing observable.
    #[test]
    fn session_shortcut_matches_sync_path_bit_for_bit_and_avoids_full_walks() {
        use crate::native_training_store_v2::publish_prepared_segment_with_session_v2;

        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let target = run.requested_successful_updates();
        let checkpoint_segment_updates = run.checkpoint_segment_updates();

        // Sync path (unchanged): every window's own full walk, on both sides.
        call_counters_v1::reset_load_generation_calls_v1();
        let sync_parent = TestParentV2::new("session-vs-sync-old");
        let sync_root = bootstrap_and_publish_genesis_v2(sync_parent.path(), &run);
        loop {
            match resume_native_training_store_v2(&sync_root, &run, execution_config_v2(&run))
                .unwrap()
            {
                NativeTrainingStoreResumeV2::Complete { .. } => break,
                NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .unwrap();
                    let receipt = publish_prepared_segment_v2(
                        &sync_root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                }
            }
        }
        let sync_calls = call_counters_v1::load_generation_calls_v1();
        assert!(sync_calls > 0, "the sync path must call load_generation_v2 at all");

        // Session-aware path (StoreV3 port): identical run, fresh store.
        call_counters_v1::reset_load_generation_calls_v1();
        let session_parent = TestParentV2::new("session-vs-sync-new");
        let session_root = bootstrap_and_publish_genesis_v2(session_parent.path(), &run);
        let mut session: Option<NativeTrainingStoreContinuationSessionV2> = None;
        loop {
            let resumed = resume_native_training_store_with_session_v2(
                &session_root,
                &run,
                execution_config_v2(&run),
                session.take(),
            )
            .unwrap();
            match resumed {
                NativeTrainingStoreResumeV2::Complete { .. } => break,
                NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .unwrap();
                    let (receipt, next_session) = publish_prepared_segment_with_session_v2(
                        &session_root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                        &continuation.tip_proof,
                        continuation.windows_since_full_walk,
                    )
                    .unwrap();
                    session = Some(next_session);
                    prepared.commit_v2(receipt).unwrap();
                }
            }
        }
        let session_calls = call_counters_v1::load_generation_calls_v1();

        println!(
            "session_shortcut_test sync_load_generation_calls={sync_calls} \
             session_load_generation_calls={session_calls}"
        );
        assert!(
            session_calls < sync_calls,
            "session-aware path must call load_generation_v2 far less often: \
             sync={sync_calls} session={session_calls}"
        );
        // Exact pin: one bootstrap walk at depth 0 (1 generation) plus
        // exactly one full walk at the very end -- the `Complete` branch's
        // existing, unconditional no-op reread (unchanged by this port,
        // and itself unrelated to the per-window cost the port fixes, since
        // it fires once per run rather than once per window) -- walking
        // every boundary generation 0..=target.
        let expected_session_calls = 1 + (target / checkpoint_segment_updates + 1);
        assert_eq!(session_calls, expected_session_calls);

        assert_store_trees_equal_v1(&sync_root, &session_root);
    }

    /// Gate (c): with no retained session (a genuine restart, or the first
    /// call in a fresh process), the full walk always runs; with one, a
    /// healthy in-process continuation never calls `load_generation_v2` at
    /// all.
    #[test]
    fn session_shortcut_declines_after_a_genuine_restart() {
        use crate::native_training_store_v2::publish_prepared_segment_with_session_v2;

        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let parent = TestParentV2::new("session-restart");
        let root = bootstrap_and_publish_genesis_v2(parent.path(), &run);

        call_counters_v1::reset_load_generation_calls_v1();
        let resumed = resume_native_training_store_with_session_v2(
            &root,
            &run,
            execution_config_v2(&run),
            None,
        )
        .unwrap();
        let mut continuation = match resumed {
            NativeTrainingStoreResumeV2::Continue(continuation) => continuation,
            NativeTrainingStoreResumeV2::Complete { .. } => {
                panic!("the fixture's schedule must have a first window")
            }
        };
        assert!(
            call_counters_v1::load_generation_calls_v1() > 0,
            "the first resume call in a process must take the full walk"
        );

        let prepared = prepare_segment_v2(
            &mut continuation.executor,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
        )
        .unwrap();
        let (receipt, session) = publish_prepared_segment_with_session_v2(
            &root,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
            &prepared,
            &continuation.tip_proof,
            continuation.windows_since_full_walk,
        )
        .unwrap();
        prepared.commit_v2(receipt).unwrap();

        // In-process continuation: the shortcut is taken, adding zero more
        // load_generation_v2 calls.
        call_counters_v1::reset_load_generation_calls_v1();
        let _ = resume_native_training_store_with_session_v2(
            &root,
            &run,
            execution_config_v2(&run),
            Some(session),
        )
        .unwrap();
        assert_eq!(
            call_counters_v1::load_generation_calls_v1(),
            0,
            "a healthy in-process continuation must not call load_generation_v2 at all"
        );

        // Genuine restart (or first call in a fresh process): session=None,
        // exactly as if a new process had just opened this same on-disk
        // Store. The full walk must run -- proportional to the current
        // depth, never zero.
        call_counters_v1::reset_load_generation_calls_v1();
        let _ = resume_native_training_store_with_session_v2(
            &root,
            &run,
            execution_config_v2(&run),
            None,
        )
        .unwrap();
        assert!(
            call_counters_v1::load_generation_calls_v1() > 0,
            "a genuine restart (no retained session) must perform the full walk"
        );
    }

    /// Gate (b), part 1: a byte corruption of the retained session's own
    /// tip generation is still caught. The O(1) freshness probe declines the
    /// shortcut (byte mismatch), falls back to exactly `walk_complete_store_v2`,
    /// and that full walk -- unchanged -- fails exactly as it would for the
    /// unmodified sync path on the same corruption.
    #[test]
    fn session_shortcut_falls_back_and_fails_closed_on_tip_corruption() {
        use crate::native_training_store_v2::publish_prepared_segment_with_session_v2;

        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let parent = TestParentV2::new("session-tip-corruption");
        let root = bootstrap_and_publish_genesis_v2(parent.path(), &run);

        let resumed = resume_native_training_store_with_session_v2(
            &root,
            &run,
            execution_config_v2(&run),
            None,
        )
        .unwrap();
        let mut continuation = match resumed {
            NativeTrainingStoreResumeV2::Continue(continuation) => continuation,
            NativeTrainingStoreResumeV2::Complete { .. } => {
                panic!("the fixture's schedule must have a first window")
            }
        };
        let prepared = prepare_segment_v2(
            &mut continuation.executor,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
        )
        .unwrap();
        let (receipt, session) = publish_prepared_segment_with_session_v2(
            &root,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
            &prepared,
            &continuation.tip_proof,
            continuation.windows_since_full_walk,
        )
        .unwrap();
        prepared.commit_v2(receipt).unwrap();

        // Corrupt the new tip's own sidecar (a same-length flip, exactly the
        // "mid-chain final" corruption the sync-path test above exercises).
        let checkpoint_segment_updates = run.checkpoint_segment_updates();
        let sidecar_path = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join(format!(
                "update-{checkpoint_segment_updates:08}.sidecar.json"
            ));
        let original = fs::read(&sidecar_path).unwrap();
        let corrupted: Vec<u8> = original.iter().map(|byte| byte ^ 0x01).collect();
        fs::write(&sidecar_path, &corrupted).unwrap();

        call_counters_v1::reset_load_generation_calls_v1();
        let result =
            resume_native_training_store_with_session_v2(&root, &run, execution_config_v2(&run), Some(session));
        assert_eq!(
            result.unwrap_err().kind(),
            NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid,
            "a corrupted tip must fail exactly as the unmodified full walk does"
        );
        assert!(
            call_counters_v1::load_generation_calls_v1() > 0,
            "the freshness probe must decline and fall back to the full walk, which is what actually catches this"
        );
        assert_eq!(fs::read(&sidecar_path).unwrap(), corrupted, "no mutation on a failed resume");
    }

    /// Gate (b), part 2: the O(1) freshness probe only reverifies the
    /// retained session's own tip generation. It is NOT, however, the only
    /// existing scan on the publish side: `validate_publication_inventory_v2`
    /// (unchanged by this port) unconditionally classifies every leaf in all
    /// five directories on *every* publish call regardless of any session,
    /// which is why a stray/foreign leaf is actually caught immediately (an
    /// earlier version of this test wrongly assumed otherwise and was
    /// corrected). The real, disclosed blind spot is narrower: a *content*
    /// corruption of an older, no-longer-tip generation's final. Neither the
    /// freshness probe (tip-only) nor the inventory scan (filename
    /// classification only -- it never rereads a historical, non-candidate
    /// final's bytes) can see that. The periodic full-walk cadence bounds
    /// how long it can persist: it fully redecodes every generation from
    /// genesis, exactly like `resume_drives_the_full_run_from_reconstructed_executors_to_the_exact_no_op`'s
    /// own "same-length corruption of a mid-chain final" case. Uses the
    /// test-only cadence override so the mitigation is exercised in a
    /// handful of windows rather than needing the real (much larger)
    /// production cadence.
    #[test]
    fn periodic_full_walk_catches_mid_chain_corruption_the_shortcut_missed() {
        use crate::native_training_store_v2::publish_prepared_segment_with_session_v2;

        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let parent = TestParentV2::new("session-periodic-cadence");
        let root = bootstrap_and_publish_genesis_v2(parent.path(), &run);
        let cadence = 3_u32;
        let checkpoint_segment_updates = run.checkpoint_segment_updates();
        let target = run.requested_successful_updates();
        assert!(
            target >= checkpoint_segment_updates * 2,
            "fixture must schedule at least two windows so a generation can become non-tip"
        );

        // Window 1: gen 0 -> gen S.
        let resumed = resume_native_training_store_with_session_and_cadence_for_test_v1(
            &root,
            &run,
            execution_config_v2(&run),
            None,
            cadence,
        )
        .unwrap();
        let mut continuation = match resumed {
            NativeTrainingStoreResumeV2::Continue(continuation) => continuation,
            NativeTrainingStoreResumeV2::Complete { .. } => {
                panic!("the fixture's schedule must have a first window")
            }
        };
        let prepared = prepare_segment_v2(
            &mut continuation.executor,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
        )
        .unwrap();
        let (receipt, session_after_window1) = publish_prepared_segment_with_session_v2(
            &root,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
            &prepared,
            &continuation.tip_proof,
            continuation.windows_since_full_walk,
        )
        .unwrap();
        prepared.commit_v2(receipt).unwrap();

        // Window 2: gen S -> gen 2S. Generation S is no longer the tip.
        let resumed = resume_native_training_store_with_session_and_cadence_for_test_v1(
            &root,
            &run,
            execution_config_v2(&run),
            Some(session_after_window1),
            cadence,
        )
        .unwrap();
        let mut continuation = match resumed {
            NativeTrainingStoreResumeV2::Continue(continuation) => continuation,
            NativeTrainingStoreResumeV2::Complete { .. } => {
                panic!("the fixture's schedule must have a second window")
            }
        };
        let prepared = prepare_segment_v2(
            &mut continuation.executor,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
        )
        .unwrap();
        let (receipt, session_after_window2) = publish_prepared_segment_with_session_v2(
            &root,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
            &prepared,
            &continuation.tip_proof,
            continuation.windows_since_full_walk,
        )
        .unwrap();
        prepared.commit_v2(receipt).unwrap();
        let mut session = Some(session_after_window2);

        // Corrupt generation S's own sidecar: a same-length byte flip, exactly
        // the "mid-chain final" corruption the sync-path lifecycle test
        // exercises against the unmodified full walk. Generation S is no
        // longer the tip (generation 2S is).
        let sidecar_path = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join(format!(
                "update-{checkpoint_segment_updates:08}.sidecar.json"
            ));
        let original = fs::read(&sidecar_path).unwrap();
        let corrupted: Vec<u8> = original.iter().map(|byte| byte ^ 0x01).collect();
        fs::write(&sidecar_path, &corrupted).unwrap();

        // Drive further windows until either a resume call is caught by the
        // periodic full walk, or the schedule would otherwise complete
        // (bounded so a mispredicted cadence fails the test instead of
        // looping forever).
        let max_windows = target / checkpoint_segment_updates + 2;
        let mut succeeded_despite_corruption = false;
        let mut caught = false;
        for _ in 0..max_windows {
            match resume_native_training_store_with_session_and_cadence_for_test_v1(
                &root,
                &run,
                execution_config_v2(&run),
                session.take(),
                cadence,
            ) {
                Err(error) => {
                    assert_eq!(error.kind(), NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid);
                    caught = true;
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Complete { .. }) => {
                    panic!("must be caught by the periodic full walk before the schedule completes")
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut next_continuation)) => {
                    succeeded_despite_corruption = true;
                    let prepared = prepare_segment_v2(
                        &mut next_continuation.executor,
                        &run,
                        &next_continuation.parent_boundary,
                        &next_continuation.parent_checkpoint,
                    )
                    .unwrap();
                    let (receipt, next_session) = publish_prepared_segment_with_session_v2(
                        &root,
                        &run,
                        &next_continuation.parent_boundary,
                        &next_continuation.parent_checkpoint,
                        &prepared,
                        &next_continuation.tip_proof,
                        next_continuation.windows_since_full_walk,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    session = Some(next_session);
                }
            }
        }
        assert!(
            succeeded_despite_corruption,
            "at least one window must succeed despite the corruption -- the disclosed, accepted blind spot"
        );
        assert!(
            caught,
            "the periodic full-walk cadence must eventually catch the mid-chain corruption"
        );

        fs::write(&sidecar_path, &original).unwrap();
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

/// MEASUREMENT HARNESS ONLY (throughput remeasure task, 2026-08-25):
/// per-depth wall-clock timing for `walk_complete_store_v2`'s per-generation
/// loop, reusing `load_generation_v2` -- the exact function the production
/// walk calls once per boundary generation -- but stopping at an externally
/// chosen `target_depth` instead of always continuing to whatever
/// `latest.json` records. This lets one already-decodable Store copy yield
/// several depth points on the same wall-clock scaling curve, instead of
/// needing a distinct Store at every depth. Excluded relative to a full
/// `walk_complete_store_v2` call: the O(1) `run.json`/`latest.json` byte
/// reads before the loop and the O(1) latest-pointer proof plus leaf
/// inventory scan after it (see `walk_complete_store_v2` above); none of
/// those scale with depth, so the comparison this exists to support (cost
/// vs. depth) is unaffected by leaving them out. Read-only: takes the same
/// shared reader lock `validate_native_training_store_v2` takes and writes
/// nothing.
#[cfg(test)]
mod store_v2_partial_walk_timing_harness_v1 {
    use super::*;
    use crate::native_training_store_run_v2::decode_train_run_v2;
    use std::time::Instant;

    #[test]
    #[ignore = "measurement harness: needs MTG_KERNEL_TIMING_HARNESS_STORE_ROOT and \
                MTG_KERNEL_TIMING_HARNESS_TARGET_DEPTH set against a real Store copy"]
    fn measure_partial_walk_wall_time_v1() {
        let root_path = std::env::var("MTG_KERNEL_TIMING_HARNESS_STORE_ROOT")
            .expect("set MTG_KERNEL_TIMING_HARNESS_STORE_ROOT to a Store root directory");
        let target_depth: u64 = std::env::var("MTG_KERNEL_TIMING_HARNESS_TARGET_DEPTH")
            .expect("set MTG_KERNEL_TIMING_HARNESS_TARGET_DEPTH to a boundary generation index")
            .parse()
            .expect("MTG_KERNEL_TIMING_HARNESS_TARGET_DEPTH must be a u64");
        let repeats: u32 = std::env::var("MTG_KERNEL_TIMING_HARNESS_REPEATS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3);

        println!("partial_harness_store_root={root_path} target_depth={target_depth}");

        let run_json_path = std::path::Path::new(&root_path).join("run.json");
        let run_bytes = std::fs::read(&run_json_path).unwrap_or_else(|error| {
            panic!("harness_error=read_run_json path={run_json_path:?} error={error}")
        });
        let run = decode_train_run_v2(&run_bytes)
            .unwrap_or_else(|error| panic!("harness_error=decode_train_run_v2 error={error}"));
        let checkpoint_segment_updates = run.checkpoint_segment_updates();
        assert!(
            target_depth == 0 || target_depth.is_multiple_of(checkpoint_segment_updates),
            "target_depth must be 0 or a multiple of checkpoint_segment_updates ({checkpoint_segment_updates})"
        );

        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap_or_else(|error| {
            panic!("harness_error=open_v2 code={} error={error}", error.code())
        });

        for repeat_index in 0..repeats {
            root.recapture_v2().unwrap_or_else(|error| {
                panic!("harness_error=recapture_v2 code={} error={error}", error.code())
            });
            let _shared = root.lock_shared_v2().map_err(map_lock_error_v2).unwrap_or_else(|error| {
                panic!(
                    "harness_error=lock_shared_v2 code={} error={error}",
                    error.kind().code()
                )
            });
            let started = Instant::now();
            let mut walked: Option<WalkedGenerationV2> = None;
            let mut generation_index = 0_u64;
            loop {
                let generation =
                    load_generation_v2(&root, &run, walked.as_ref(), generation_index)
                        .unwrap_or_else(|error| {
                            panic!(
                                "harness_error=load_generation_v2 code={} generation_index={} error={error}",
                                error.kind().code(),
                                generation_index
                            )
                        });
                walked = Some(generation);
                if generation_index == target_depth {
                    break;
                }
                generation_index = generation_index
                    .checked_add(checkpoint_segment_updates)
                    .expect("generation_index overflow before reaching target_depth");
            }
            let elapsed = started.elapsed();
            println!(
                "partial_harness_result repeat={repeat_index} target_depth={target_depth} elapsed_micros={} elapsed_secs={:.6}",
                elapsed.as_micros(),
                elapsed.as_secs_f64(),
            );
        }
    }
}
