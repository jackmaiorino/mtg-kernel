//! Native trainer persistence boundary shared by the executor and the strict
//! generation store.
//!
//! This module owns the move-only generation receipt and the durable
//! generation publisher. Receipt construction remains private to this module
//! and happens only from independently recaptured published bytes, after
//! immutable publication in the frozen order, complete-generation
//! revalidation, latest-last replacement, and the post-latest referenced
//! revalidation pass. The publisher also proves, under its exclusive lock and
//! before mutation, that the supplied parent is still the Store's current
//! latest boundary (or that this exact candidate is an idempotent retry whose
//! latest pointer already committed). Recovery, resume orchestration, and the
//! replay witness remain later layers.

use crate::durable_move_publication_v2::{
    publish_immutable_file_by_move_v2, replace_file_by_move_v2,
};
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, verify_existing_publication_v1,
    DurableFileExpectationV1, DurablePublicationErrorKindV1, DurablePublicationErrorV1,
    ValidatedPublicationParentV1,
};
use crate::native_training_store_boundary_v2::{
    decode_genesis_native_training_boundary_v2, decode_trained_native_training_boundary_v2,
    ValidatedNativeTrainingBoundaryV2,
};
use crate::native_training_store_checkpoint_v3::{
    decode_checkpoint_manifest_v3, decode_trained_checkpoint_manifest_v3, CheckpointManifestV3,
};
use crate::native_training_store_digest_v1::sha256_v1;
use crate::native_training_store_layout_v2::{
    classify_store_leaf_v2, NativeTrainingStoreDirectoryV2, NativeTrainingStoreFinalNameV2,
    NativeTrainingStoreLeafV2, NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2,
};
use crate::native_training_store_prepared_segment_v2::NativeTrainingPreparedSegmentV2;
use crate::native_training_store_reference_latest_v2::{
    build_latest_v2, decode_checkpoint_reference_v2, decode_latest_v2,
    ValidatedCheckpointReferenceV2, ValidatedLatestRecordV2,
};
use crate::native_training_store_resume_v2::{
    validate_native_training_store_for_publication_v2, NativeTrainingStoreResumeV2ErrorKind,
    ValidatedNativeTrainingStoreStateV2,
};
use crate::native_training_store_root_v2::{
    NativeTrainingStoreRootV2Error, NativeTrainingStoreRootV2ErrorKind,
    ValidatedNativeTrainingStoreRootV2,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_continuation_v2::decode_segment_continuations_v2;
use crate::native_training_store_segment_manifest_v2::{
    decode_genesis_segment_manifest_v2, decode_trained_segment_manifest_v2, SegmentManifestV2,
};
use crate::native_training_store_update_group_v1::resume_update_evidence_chain_v1;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Non-forgeable witness that one exact native training generation was
/// durably published and independently recaptured by the V2 store.
///
/// The type deliberately implements neither [`Clone`] nor a public constructor.
/// Read-only accessors support the executor's final receipt comparison and
/// audit diagnostics without allowing a caller to manufacture a witness.
///
/// External construction is rejected because every field is private:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
///
/// let _forged = NativeTrainingPersistenceReceiptV2 {
///     generation_index: 1,
///     checkpoint_payload_sha256: [0; 32],
///     checkpoint_manifest_sha256: [0; 32],
/// };
/// ```
///
/// The witness is move-only:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
///
/// fn duplicate(receipt: NativeTrainingPersistenceReceiptV2) {
///     let _copy = receipt.clone();
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
#[must_use = "a persistence receipt must be consumed by the prepared update commit"]
pub struct NativeTrainingPersistenceReceiptV2 {
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
}

impl NativeTrainingPersistenceReceiptV2 {
    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub const fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.checkpoint_payload_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }
}

// The production constructor is private to the publisher below: it runs only
// after the post-latest referenced revalidation pass, strictly from
// independently recaptured published bytes.
const fn production_persistence_receipt_v2(
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
) -> NativeTrainingPersistenceReceiptV2 {
    NativeTrainingPersistenceReceiptV2 {
        generation_index,
        checkpoint_payload_sha256,
        checkpoint_manifest_sha256,
    }
}

#[cfg(test)]
pub(crate) const fn test_persistence_receipt_v2(
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
) -> NativeTrainingPersistenceReceiptV2 {
    NativeTrainingPersistenceReceiptV2 {
        generation_index,
        checkpoint_payload_sha256,
        checkpoint_manifest_sha256,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStorePublisherV2ErrorKind {
    UnsupportedPlatform,
    StoreBusy,
    RootInvalid,
    InputInvalid,
    StageCorruption,
    PublicationFailed,
    ImmutableFinalMismatchCorruption,
    GenerationInvalid,
    LatestInvalid,
}

impl NativeTrainingStorePublisherV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "native-training-store-v2-unsupported-platform",
            Self::StoreBusy => "native-training-store-busy",
            Self::RootInvalid => "native-training-store-publisher-root-invalid",
            Self::InputInvalid => "native-training-store-publisher-input-invalid",
            Self::StageCorruption => "native-training-store-publisher-stage-corruption",
            Self::PublicationFailed => "native-training-store-publisher-publication-failed",
            Self::ImmutableFinalMismatchCorruption => "immutable-final-mismatch-corruption",
            Self::GenerationInvalid => "native-training-store-publisher-generation-invalid",
            Self::LatestInvalid => "native-training-store-publisher-latest-invalid",
        }
    }
}

/// Redacted publisher error carrying only its classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingStorePublisherV2Error {
    kind: NativeTrainingStorePublisherV2ErrorKind,
}

impl NativeTrainingStorePublisherV2Error {
    pub const fn kind(self) -> NativeTrainingStorePublisherV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeTrainingStorePublisherV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingStorePublisherV2Error {}

type PublisherResult<T> = std::result::Result<T, NativeTrainingStorePublisherV2Error>;

const fn publisher_error_v2(
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> NativeTrainingStorePublisherV2Error {
    NativeTrainingStorePublisherV2Error { kind }
}

fn map_root_error_v2(error: NativeTrainingStoreRootV2Error) -> NativeTrainingStorePublisherV2Error {
    publisher_error_v2(match error.kind() {
        NativeTrainingStoreRootV2ErrorKind::UnsupportedPlatform => {
            NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform
        }
        NativeTrainingStoreRootV2ErrorKind::StoreBusy => {
            NativeTrainingStorePublisherV2ErrorKind::StoreBusy
        }
        _ => NativeTrainingStorePublisherV2ErrorKind::RootInvalid,
    })
}

fn map_publication_error_v2(
    error: &DurablePublicationErrorV1,
) -> NativeTrainingStorePublisherV2Error {
    publisher_error_v2(match error.kind() {
        DurablePublicationErrorKindV1::UnsupportedPlatform => {
            NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform
        }
        _ => NativeTrainingStorePublisherV2ErrorKind::PublicationFailed,
    })
}

/// Fault-injection boundaries of the generation publication algorithm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublisherBoundaryV2 {
    AfterStageSweep,
    AfterRunAuthority,
    AfterImmutableFinal(usize),
    AfterGenerationRevalidation,
    BeforeLatestReplacement,
    AfterLatestReplacement,
    AfterLatestReopenRevalidation,
    BeforeReceiptConstruction,
}

/// Borrowed byte view of one complete generation to publish.
struct GenerationPublicationInputV2<'bytes> {
    generation_index: u64,
    checkpoint_payload: &'bytes [u8],
    checkpoint_manifest: &'bytes [u8],
    continuations: Vec<&'bytes [u8]>,
    segment_manifest: &'bytes [u8],
    checkpoint_sidecar: &'bytes [u8],
    head_record: &'bytes [u8],
    checkpoint_reference: &'bytes [u8],
    latest: &'bytes [u8],
}

/// Parent authorities for the generation being published.
enum PublisherParentV2<'authority> {
    Genesis,
    Trained {
        parent: &'authority ValidatedNativeTrainingBoundaryV2,
        parent_checkpoint: &'authority CheckpointManifestV3,
    },
}

/// Publish a prepared trained segment and privately construct its receipt.
///
/// The live executor is left untouched by this function; the caller consumes
/// the returned receipt through the prepared segment's `commit_v2`.
pub fn publish_prepared_segment_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    parent_checkpoint: &CheckpointManifestV3,
    prepared: &NativeTrainingPreparedSegmentV2<'_>,
) -> PublisherResult<NativeTrainingPersistenceReceiptV2> {
    publish_prepared_segment_with_hook_v2(
        root,
        run,
        parent,
        parent_checkpoint,
        prepared,
        |_| Ok(()),
    )
}

fn publish_prepared_segment_with_hook_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &ValidatedNativeTrainingBoundaryV2,
    parent_checkpoint: &CheckpointManifestV3,
    prepared: &NativeTrainingPreparedSegmentV2<'_>,
    hook: impl FnMut(PublisherBoundaryV2) -> PublisherResult<()>,
) -> PublisherResult<NativeTrainingPersistenceReceiptV2> {
    let view = prepared.publication_view_v2();
    let continuations = (0..view.continuation_count_v2())
        .map(|index| {
            view.continuation_canonical_bytes_v2(index)
                .ok_or(publisher_error_v2(
                    NativeTrainingStorePublisherV2ErrorKind::InputInvalid,
                ))
        })
        .collect::<PublisherResult<Vec<&[u8]>>>()?;
    if continuations.is_empty() {
        return Err(publisher_error_v2(
            NativeTrainingStorePublisherV2ErrorKind::InputInvalid,
        ));
    }
    let input = GenerationPublicationInputV2 {
        generation_index: prepared.expected_generation_index(),
        checkpoint_payload: view.checkpoint_payload_v2(),
        checkpoint_manifest: view.checkpoint_manifest_canonical_bytes_v2(),
        continuations,
        segment_manifest: view.segment_manifest_canonical_bytes_v2(),
        checkpoint_sidecar: view.checkpoint_sidecar_canonical_bytes_v2(),
        head_record: view.head_record_canonical_bytes_v2(),
        checkpoint_reference: view.checkpoint_reference_canonical_bytes_v2(),
        latest: view.latest_canonical_bytes_v2(),
    };
    publish_generation_v2(
        root,
        run,
        PublisherParentV2::Trained {
            parent,
            parent_checkpoint,
        },
        &input,
        hook,
    )
}

/// Publish the genesis generation, bootstrapping `run.json` first, and
/// privately construct the generation-zero receipt.
#[allow(clippy::too_many_arguments)]
pub fn publish_genesis_generation_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    genesis_payload: &[u8],
    genesis_checkpoint: &CheckpointManifestV3,
    genesis_segment: &SegmentManifestV2,
    genesis_boundary: &ValidatedNativeTrainingBoundaryV2,
    genesis_reference: &ValidatedCheckpointReferenceV2,
    genesis_latest: &ValidatedLatestRecordV2,
) -> PublisherResult<NativeTrainingPersistenceReceiptV2> {
    publish_genesis_generation_with_hook_v2(
        root,
        run,
        genesis_payload,
        genesis_checkpoint,
        genesis_segment,
        genesis_boundary,
        genesis_reference,
        genesis_latest,
        |_| Ok(()),
    )
}

#[allow(clippy::too_many_arguments)]
fn publish_genesis_generation_with_hook_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    genesis_payload: &[u8],
    genesis_checkpoint: &CheckpointManifestV3,
    genesis_segment: &SegmentManifestV2,
    genesis_boundary: &ValidatedNativeTrainingBoundaryV2,
    genesis_reference: &ValidatedCheckpointReferenceV2,
    genesis_latest: &ValidatedLatestRecordV2,
    hook: impl FnMut(PublisherBoundaryV2) -> PublisherResult<()>,
) -> PublisherResult<NativeTrainingPersistenceReceiptV2> {
    if genesis_checkpoint.generation_index() != 0 {
        return Err(publisher_error_v2(
            NativeTrainingStorePublisherV2ErrorKind::InputInvalid,
        ));
    }
    let input = GenerationPublicationInputV2 {
        generation_index: 0,
        checkpoint_payload: genesis_payload,
        checkpoint_manifest: genesis_checkpoint.canonical_bytes(),
        continuations: Vec::new(),
        segment_manifest: genesis_segment.canonical_bytes(),
        checkpoint_sidecar: genesis_boundary.checkpoint_sidecar_canonical_bytes(),
        head_record: genesis_boundary.head_record_canonical_bytes(),
        checkpoint_reference: genesis_reference.canonical_bytes(),
        latest: genesis_latest.canonical_bytes(),
    };
    publish_generation_v2(root, run, PublisherParentV2::Genesis, &input, hook)
}

/// One immutable artifact scheduled in the frozen publication order.
struct ScheduledImmutableV2<'bytes> {
    final_name: NativeTrainingStoreFinalNameV2,
    bytes: &'bytes [u8],
}

#[derive(Default)]
struct PublicationInventoryV2 {
    run_present: bool,
    latest_present: bool,
    candidate_finals: Vec<NativeTrainingStoreFinalNameV2>,
    stage_finals: Vec<NativeTrainingStoreFinalNameV2>,
}

fn scheduled_immutables_v2<'bytes>(
    input: &GenerationPublicationInputV2<'bytes>,
) -> PublisherResult<Vec<ScheduledImmutableV2<'bytes>>> {
    let generation_index = input.generation_index;
    let mut scheduled = Vec::with_capacity(6 + input.continuations.len());
    scheduled.push(ScheduledImmutableV2 {
        final_name: NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        bytes: input.checkpoint_payload,
    });
    scheduled.push(ScheduledImmutableV2 {
        final_name: NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index },
        bytes: input.checkpoint_manifest,
    });
    for (index, continuation) in input.continuations.iter().enumerate() {
        let continuation_index = u64::try_from(index).map_err(|_| {
            publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::InputInvalid)
        })?;
        scheduled.push(ScheduledImmutableV2 {
            final_name: NativeTrainingStoreFinalNameV2::SegmentContinuation {
                generation_index,
                continuation_index,
            },
            bytes: continuation,
        });
    }
    scheduled.push(ScheduledImmutableV2 {
        final_name: NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index },
        bytes: input.segment_manifest,
    });
    scheduled.push(ScheduledImmutableV2 {
        final_name: NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index },
        bytes: input.checkpoint_sidecar,
    });
    scheduled.push(ScheduledImmutableV2 {
        final_name: NativeTrainingStoreFinalNameV2::HeadRecord { generation_index },
        bytes: input.head_record,
    });
    scheduled.push(ScheduledImmutableV2 {
        final_name: NativeTrainingStoreFinalNameV2::CheckpointReference { generation_index },
        bytes: input.checkpoint_reference,
    });
    Ok(scheduled)
}

/// Captured same-parent publication parents for every Store directory.
struct PublicationParentsV2 {
    root: ValidatedPublicationParentV1,
    segments: ValidatedPublicationParentV1,
    checkpoints: ValidatedPublicationParentV1,
    heads: ValidatedPublicationParentV1,
    refs: ValidatedPublicationParentV1,
}

impl PublicationParentsV2 {
    fn capture_v2(root: &ValidatedNativeTrainingStoreRootV2) -> PublisherResult<Self> {
        let capture = |directory| {
            capture_existing_publication_parent_v1(root.directory_path_v2(directory))
                .map_err(|error| map_publication_error_v2(&error))
        };
        Ok(Self {
            root: capture(NativeTrainingStoreDirectoryV2::Root)?,
            segments: capture(NativeTrainingStoreDirectoryV2::Segments)?,
            checkpoints: capture(NativeTrainingStoreDirectoryV2::Checkpoints)?,
            heads: capture(NativeTrainingStoreDirectoryV2::Heads)?,
            refs: capture(NativeTrainingStoreDirectoryV2::Refs)?,
        })
    }

    const fn parent_v2(
        &self,
        directory: NativeTrainingStoreDirectoryV2,
    ) -> &ValidatedPublicationParentV1 {
        match directory {
            NativeTrainingStoreDirectoryV2::Root => &self.root,
            NativeTrainingStoreDirectoryV2::Segments => &self.segments,
            NativeTrainingStoreDirectoryV2::Checkpoints => &self.checkpoints,
            NativeTrainingStoreDirectoryV2::Heads => &self.heads,
            NativeTrainingStoreDirectoryV2::Refs => &self.refs,
        }
    }
}

fn publish_generation_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
    mut hook: impl FnMut(PublisherBoundaryV2) -> PublisherResult<()>,
) -> PublisherResult<NativeTrainingPersistenceReceiptV2> {
    validate_publication_input_v2(run, &parent, input)?;
    let scheduled = scheduled_immutables_v2(input)?;

    root.recapture_v2().map_err(map_root_error_v2)?;
    let lock = root.lock_exclusive_v2().map_err(map_root_error_v2)?;
    let parents = PublicationParentsV2::capture_v2(root)?;

    let inventory =
        validate_publication_inventory_v2(root, &parent, input.generation_index, &scheduled, true)?;
    require_current_publication_authority_v2(
        root, run, &parent, input, &scheduled, &parents, &inventory,
    )?;
    prevalidate_publication_candidate_v2(run, &parent, input)?;

    sweep_recognized_stages_v2(root)?;
    hook(PublisherBoundaryV2::AfterStageSweep)?;

    establish_run_authority_v2(root, run, &parent, &parents)?;
    hook(PublisherBoundaryV2::AfterRunAuthority)?;

    for (ordinal, immutable) in scheduled.iter().enumerate() {
        publish_or_resume_immutable_v2(root, &parents, immutable)?;
        hook(PublisherBoundaryV2::AfterImmutableFinal(ordinal))?;
    }

    let _ = validate_publication_inventory_v2(
        root,
        &parent,
        input.generation_index,
        &scheduled,
        false,
    )?;

    revalidate_generation_from_disk_v2(root, run, &parent, input)?;
    hook(PublisherBoundaryV2::AfterGenerationRevalidation)?;

    hook(PublisherBoundaryV2::BeforeLatestReplacement)?;
    let latest_expectation = DurableFileExpectationV1::from_bytes(input.latest)
        .map_err(|error| map_publication_error_v2(&error))?;
    let latest_final = NativeTrainingStoreFinalNameV2::Latest;
    replace_file_by_move_v2(
        &parents.root,
        stage_basename_v2(latest_final)?,
        final_basename_v2(latest_final)?,
        input.latest,
        latest_expectation,
    )
    .map_err(|error| map_publication_error_v2(&error))?;
    hook(PublisherBoundaryV2::AfterLatestReplacement)?;

    let observed = revalidate_latest_and_referenced_v2(root, run, &parent, input)?;
    hook(PublisherBoundaryV2::AfterLatestReopenRevalidation)?;

    hook(PublisherBoundaryV2::BeforeReceiptConstruction)?;
    let receipt = production_persistence_receipt_v2(
        observed.generation_index,
        observed.checkpoint_payload_sha256,
        observed.checkpoint_manifest_sha256,
    );
    drop(lock);
    Ok(receipt)
}

fn verify_existing_final_exact_v2(
    parents: &PublicationParentsV2,
    final_name: NativeTrainingStoreFinalNameV2,
    bytes: &[u8],
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> PublisherResult<()> {
    let expectation = DurableFileExpectationV1::from_bytes(bytes)
        .map_err(|error| map_publication_error_v2(&error))?;
    verify_existing_publication_v1(
        parents.parent_v2(final_name.directory()),
        &final_basename_v2(final_name)?,
        expectation,
    )
    .map(|_| ())
    .map_err(|_| publisher_error_v2(kind))
}

fn verify_preexisting_candidate_finals_v2(
    parents: &PublicationParentsV2,
    scheduled: &[ScheduledImmutableV2<'_>],
    inventory: &PublicationInventoryV2,
) -> PublisherResult<()> {
    for final_name in &inventory.candidate_finals {
        let expected = scheduled
            .iter()
            .find(|scheduled| scheduled.final_name == *final_name)
            .ok_or(publisher_error_v2(
                NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid,
            ))?;
        verify_existing_final_exact_v2(
            parents,
            *final_name,
            expected.bytes,
            NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
        )?;
    }
    Ok(())
}

fn boundary_authority_matches_v2(
    observed: &ValidatedNativeTrainingBoundaryV2,
    supplied: &ValidatedNativeTrainingBoundaryV2,
) -> bool {
    observed.checkpoint_sidecar_canonical_bytes() == supplied.checkpoint_sidecar_canonical_bytes()
        && observed.head_record_canonical_bytes() == supplied.head_record_canonical_bytes()
}

fn verify_walked_final_expectations_v2(
    parents: &PublicationParentsV2,
    walked: &ValidatedNativeTrainingStoreStateV2,
) -> PublisherResult<()> {
    for observed in walked.final_expectations_v2().iter().copied() {
        let final_name = observed.final_name();
        if let Err(error) = verify_existing_publication_v1(
            parents.parent_v2(final_name.directory()),
            &final_basename_v2(final_name)?,
            observed.expectation(),
        ) {
            let kind = match error.kind() {
                DurablePublicationErrorKindV1::UnsupportedPlatform => {
                    NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform
                }
                DurablePublicationErrorKindV1::InvalidParent
                | DurablePublicationErrorKindV1::ParentChanged => {
                    NativeTrainingStorePublisherV2ErrorKind::RootInvalid
                }
                _ => match final_name {
                    NativeTrainingStoreFinalNameV2::Run => {
                        NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption
                    }
                    NativeTrainingStoreFinalNameV2::Latest => {
                        NativeTrainingStorePublisherV2ErrorKind::LatestInvalid
                    }
                    _ => NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid,
                },
            };
            return Err(publisher_error_v2(kind));
        }
    }
    Ok(())
}

fn walk_current_store_through_parents_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parents: &PublicationParentsV2,
) -> PublisherResult<ValidatedNativeTrainingStoreStateV2> {
    let walked = validate_native_training_store_for_publication_v2(root, run)
        .map_err(map_store_validation_error_v2)?;
    verify_walked_final_expectations_v2(parents, &walked)?;
    Ok(walked)
}

/// Prove, before any cleanup or publication, that every existing candidate
/// final is byte-equal, the complete committed Store is valid, and the supplied
/// parent is either the exact current disk boundary or the exact parent of an
/// already-current idempotent candidate.
#[allow(clippy::too_many_arguments)]
fn require_current_publication_authority_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
    scheduled: &[ScheduledImmutableV2<'_>],
    parents: &PublicationParentsV2,
    inventory: &PublicationInventoryV2,
) -> PublisherResult<()> {
    let invalid = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid);
    match parent {
        PublisherParentV2::Genesis => {
            if !inventory.run_present {
                if inventory.latest_present || !inventory.candidate_finals.is_empty() {
                    return Err(invalid);
                }
                if inventory
                    .stage_finals
                    .iter()
                    .any(|final_name| *final_name != NativeTrainingStoreFinalNameV2::Run)
                {
                    return Err(publisher_error_v2(
                        NativeTrainingStorePublisherV2ErrorKind::StageCorruption,
                    ));
                }
                return Ok(());
            }

            verify_existing_final_exact_v2(
                parents,
                NativeTrainingStoreFinalNameV2::Run,
                run.canonical_bytes(),
                NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
            )?;
            // Preserve immutable-mismatch precedence when an idempotent
            // retry's candidate is already current. Repeat after the walk so
            // neither semantic validation nor path drift can bless new bytes.
            verify_preexisting_candidate_finals_v2(parents, scheduled, inventory)?;
            if !inventory.latest_present {
                return Ok(());
            }
            let walked = walk_current_store_through_parents_v2(root, run, parents)?;
            verify_preexisting_candidate_finals_v2(parents, scheduled, inventory)?;
            let current_latest =
                build_latest_v2(walked.latest_boundary(), walked.latest_reference()).map_err(
                    |_| publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::LatestInvalid),
                )?;
            if walked.latest_generation_index() != 0
                || current_latest.canonical_bytes() != input.latest
            {
                return Err(invalid);
            }
            Ok(())
        }
        PublisherParentV2::Trained {
            parent: supplied_parent,
            parent_checkpoint,
        } => {
            if !inventory.run_present {
                return Err(invalid);
            }
            verify_existing_final_exact_v2(
                parents,
                NativeTrainingStoreFinalNameV2::Run,
                run.canonical_bytes(),
                NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
            )?;
            verify_preexisting_candidate_finals_v2(parents, scheduled, inventory)?;
            let walked = walk_current_store_through_parents_v2(root, run, parents)?;
            verify_preexisting_candidate_finals_v2(parents, scheduled, inventory)?;
            let current_latest =
                build_latest_v2(walked.latest_boundary(), walked.latest_reference()).map_err(
                    |_| publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::LatestInvalid),
                )?;

            if walked.latest_generation_index() == parent_checkpoint.generation_index() {
                if boundary_authority_matches_v2(walked.latest_boundary(), supplied_parent)
                    && walked.latest_checkpoint().canonical_bytes()
                        == parent_checkpoint.canonical_bytes()
                {
                    return Ok(());
                }
                return Err(invalid);
            }
            if walked.latest_generation_index() == input.generation_index
                && current_latest.canonical_bytes() == input.latest
            {
                return Ok(());
            }
            Err(invalid)
        }
    }
}

fn map_store_validation_error_v2(
    error: crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2Error,
) -> NativeTrainingStorePublisherV2Error {
    publisher_error_v2(match error.kind() {
        NativeTrainingStoreResumeV2ErrorKind::UnsupportedPlatform => {
            NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform
        }
        NativeTrainingStoreResumeV2ErrorKind::StoreBusy => {
            NativeTrainingStorePublisherV2ErrorKind::StoreBusy
        }
        NativeTrainingStoreResumeV2ErrorKind::RootInvalid => {
            NativeTrainingStorePublisherV2ErrorKind::RootInvalid
        }
        NativeTrainingStoreResumeV2ErrorKind::LatestInvalid => {
            NativeTrainingStorePublisherV2ErrorKind::LatestInvalid
        }
        NativeTrainingStoreResumeV2ErrorKind::StageCorruption => {
            NativeTrainingStorePublisherV2ErrorKind::StageCorruption
        }
        NativeTrainingStoreResumeV2ErrorKind::RunInvalid
        | NativeTrainingStoreResumeV2ErrorKind::ScheduleInvalid
        | NativeTrainingStoreResumeV2ErrorKind::GenerationInvalid
        | NativeTrainingStoreResumeV2ErrorKind::ReconstructionFailed
        | NativeTrainingStoreResumeV2ErrorKind::MutationFailed => {
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        }
    })
}

/// Validate the complete directory grammar without mutation and require exact
/// membership for every candidate-generation final. For genesis, where no
/// latest pointer exists for the complete Store walker to anchor, all other
/// generation finals are rejected as well. A trained preflight is paired with
/// the whole-Store walk above, which validates every prior generation and
/// admits at most this one partial next boundary.
fn validate_publication_inventory_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    parent: &PublisherParentV2<'_>,
    generation_index: u64,
    scheduled: &[ScheduledImmutableV2<'_>],
    stages_allowed: bool,
) -> PublisherResult<PublicationInventoryV2> {
    let invalid = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid);
    let corruption = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::StageCorruption);
    let mut inventory = PublicationInventoryV2::default();
    for directory in [
        NativeTrainingStoreDirectoryV2::Root,
        NativeTrainingStoreDirectoryV2::Segments,
        NativeTrainingStoreDirectoryV2::Checkpoints,
        NativeTrainingStoreDirectoryV2::Heads,
        NativeTrainingStoreDirectoryV2::Refs,
    ] {
        let entries =
            std::fs::read_dir(root.directory_path_v2(directory)).map_err(|_| corruption)?;
        for entry in entries {
            let entry = entry.map_err(|_| corruption)?;
            let leaf = entry.file_name();
            let leaf = leaf.to_str().ok_or(corruption)?;
            let file_type = entry.file_type().map_err(|_| corruption)?;
            if matches!(directory, NativeTrainingStoreDirectoryV2::Root)
                && NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
                    .iter()
                    .any(|subdirectory| subdirectory.basename() == Some(leaf))
            {
                if !file_type.is_dir() || file_type.is_symlink() {
                    return Err(corruption);
                }
                continue;
            }
            let final_name = match classify_store_leaf_v2(directory, leaf) {
                Ok(NativeTrainingStoreLeafV2::Lock) => {
                    if !file_type.is_file() || file_type.is_symlink() {
                        return Err(corruption);
                    }
                    continue;
                }
                Ok(NativeTrainingStoreLeafV2::Stage(final_name)) => {
                    if !stages_allowed || !file_type.is_file() || file_type.is_symlink() {
                        return Err(corruption);
                    }
                    inventory.stage_finals.push(final_name);
                    continue;
                }
                Ok(NativeTrainingStoreLeafV2::Final(final_name)) => {
                    if !file_type.is_file() || file_type.is_symlink() {
                        return Err(corruption);
                    }
                    final_name
                }
                Err(_) => return Err(corruption),
            };
            match final_name {
                NativeTrainingStoreFinalNameV2::Run => {
                    inventory.run_present = true;
                    continue;
                }
                NativeTrainingStoreFinalNameV2::Latest => {
                    inventory.latest_present = true;
                    continue;
                }
                _ => {}
            }
            let observed_generation = match final_name {
                NativeTrainingStoreFinalNameV2::SegmentManifest {
                    generation_index: observed,
                }
                | NativeTrainingStoreFinalNameV2::SegmentContinuation {
                    generation_index: observed,
                    ..
                }
                | NativeTrainingStoreFinalNameV2::CheckpointManifest {
                    generation_index: observed,
                }
                | NativeTrainingStoreFinalNameV2::StatePayload {
                    generation_index: observed,
                }
                | NativeTrainingStoreFinalNameV2::CheckpointSidecar {
                    generation_index: observed,
                }
                | NativeTrainingStoreFinalNameV2::HeadRecord {
                    generation_index: observed,
                }
                | NativeTrainingStoreFinalNameV2::CheckpointReference {
                    generation_index: observed,
                } => Some(observed),
                NativeTrainingStoreFinalNameV2::Run | NativeTrainingStoreFinalNameV2::Latest => {
                    None
                }
            };
            let Some(observed_generation) = observed_generation else {
                continue;
            };
            if observed_generation == generation_index {
                if !scheduled
                    .iter()
                    .any(|expected| expected.final_name == final_name)
                {
                    return Err(invalid);
                }
                inventory.candidate_finals.push(final_name);
            } else if matches!(parent, PublisherParentV2::Genesis) {
                return Err(invalid);
            }
        }
    }
    Ok(inventory)
}

fn validate_publication_input_v2(
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
) -> PublisherResult<()> {
    let input_invalid = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::InputInvalid);
    let checkpoint_segment_updates = run.checkpoint_segment_updates();
    match parent {
        PublisherParentV2::Genesis => {
            if input.generation_index != 0 || !input.continuations.is_empty() {
                return Err(input_invalid);
            }
        }
        PublisherParentV2::Trained {
            parent_checkpoint, ..
        } => {
            let expected = parent_checkpoint
                .generation_index()
                .checked_add(checkpoint_segment_updates)
                .ok_or(input_invalid)?;
            if input.generation_index != expected
                || input.generation_index == 0
                || input.continuations.is_empty()
                || !input
                    .generation_index
                    .is_multiple_of(checkpoint_segment_updates)
            {
                return Err(input_invalid);
            }
        }
    }
    if input.checkpoint_payload.is_empty()
        || input.checkpoint_manifest.is_empty()
        || input.segment_manifest.is_empty()
        || input.checkpoint_sidecar.is_empty()
        || input.head_record.is_empty()
        || input.checkpoint_reference.is_empty()
        || input.latest.is_empty()
        || input.continuations.iter().any(|bytes| bytes.is_empty())
    {
        return Err(input_invalid);
    }
    Ok(())
}

fn final_basename_v2(final_name: NativeTrainingStoreFinalNameV2) -> PublisherResult<String> {
    final_name
        .final_basename()
        .map_err(|_| publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::InputInvalid))
}

fn stage_basename_v2(final_name: NativeTrainingStoreFinalNameV2) -> PublisherResult<String> {
    final_name
        .stage_basename()
        .map_err(|_| publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::InputInvalid))
}

/// Delete recognized stage leaves; any unknown or malformed leaf, nonregular
/// authoritative member, or non-unicode name fails closed and is preserved.
fn sweep_recognized_stages_v2(root: &ValidatedNativeTrainingStoreRootV2) -> PublisherResult<()> {
    let corruption = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::StageCorruption);
    for directory in [
        NativeTrainingStoreDirectoryV2::Root,
        NativeTrainingStoreDirectoryV2::Segments,
        NativeTrainingStoreDirectoryV2::Checkpoints,
        NativeTrainingStoreDirectoryV2::Heads,
        NativeTrainingStoreDirectoryV2::Refs,
    ] {
        let directory_path = root.directory_path_v2(directory);
        let entries = std::fs::read_dir(directory_path).map_err(|_| corruption)?;
        for entry in entries {
            let entry = entry.map_err(|_| corruption)?;
            let file_name = entry.file_name();
            let Some(leaf) = file_name.to_str() else {
                return Err(corruption);
            };
            let file_type = entry.file_type().map_err(|_| corruption)?;
            if matches!(directory, NativeTrainingStoreDirectoryV2::Root)
                && NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
                    .iter()
                    .any(|subdirectory| subdirectory.basename() == Some(leaf))
            {
                if !file_type.is_dir() || file_type.is_symlink() {
                    return Err(corruption);
                }
                continue;
            }
            match classify_store_leaf_v2(directory, leaf) {
                Ok(NativeTrainingStoreLeafV2::Lock) => {
                    if !file_type.is_file() || file_type.is_symlink() {
                        return Err(corruption);
                    }
                }
                Ok(NativeTrainingStoreLeafV2::Final(_)) => {
                    if !file_type.is_file() || file_type.is_symlink() {
                        return Err(corruption);
                    }
                }
                Ok(NativeTrainingStoreLeafV2::Stage(_)) => {
                    if !file_type.is_file() || file_type.is_symlink() {
                        return Err(corruption);
                    }
                    std::fs::remove_file(entry.path()).map_err(|_| {
                        publisher_error_v2(
                            NativeTrainingStorePublisherV2ErrorKind::PublicationFailed,
                        )
                    })?;
                }
                Err(_) => return Err(corruption),
            }
        }
    }
    Ok(())
}

/// Publish `run.json` on genesis bootstrap or require the exact existing run.
fn establish_run_authority_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    parents: &PublicationParentsV2,
) -> PublisherResult<()> {
    let run_final = NativeTrainingStoreFinalNameV2::Run;
    let run_basename = final_basename_v2(run_final)?;
    let run_path = root
        .directory_path_v2(NativeTrainingStoreDirectoryV2::Root)
        .join(&run_basename);
    let run_bytes = run.canonical_bytes();
    let expectation = DurableFileExpectationV1::from_bytes(run_bytes)
        .map_err(|error| map_publication_error_v2(&error))?;
    if std::fs::symlink_metadata(&run_path).is_ok() {
        return verify_existing_publication_v1(&parents.root, &run_basename, expectation)
            .map(|_| ())
            .map_err(|_| {
                publisher_error_v2(
                    NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
                )
            });
    }
    if !matches!(parent, PublisherParentV2::Genesis) {
        return Err(publisher_error_v2(
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid,
        ));
    }
    publish_immutable_file_by_move_v2(
        &parents.root,
        stage_basename_v2(run_final)?,
        &run_basename,
        run_bytes,
        expectation,
    )
    .map(|_| ())
    .map_err(|error| map_immutable_publish_error_v2(parents, run_final, run_bytes, &error))
}

/// Publish one immutable final, or resume when the existing final is exactly
/// candidate-equal; any divergence is immutable-final-mismatch corruption.
fn publish_or_resume_immutable_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    parents: &PublicationParentsV2,
    immutable: &ScheduledImmutableV2<'_>,
) -> PublisherResult<()> {
    let directory = immutable.final_name.directory();
    let final_basename = final_basename_v2(immutable.final_name)?;
    let final_path = root.directory_path_v2(directory).join(&final_basename);
    let expectation = DurableFileExpectationV1::from_bytes(immutable.bytes)
        .map_err(|error| map_publication_error_v2(&error))?;
    if std::fs::symlink_metadata(&final_path).is_ok() {
        return verify_existing_publication_v1(
            parents.parent_v2(directory),
            &final_basename,
            expectation,
        )
        .map(|_| ())
        .map_err(|_| {
            publisher_error_v2(
                NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
            )
        });
    }
    publish_immutable_file_by_move_v2(
        parents.parent_v2(directory),
        stage_basename_v2(immutable.final_name)?,
        &final_basename,
        immutable.bytes,
        expectation,
    )
    .map(|_| ())
    .map_err(|error| {
        map_immutable_publish_error_v2(parents, immutable.final_name, immutable.bytes, &error)
    })
}

/// A `FinalCollision` during an immutable publish is re-audited as a
/// candidate-equality check; every other backend failure maps directly.
fn map_immutable_publish_error_v2(
    parents: &PublicationParentsV2,
    final_name: NativeTrainingStoreFinalNameV2,
    bytes: &[u8],
    error: &DurablePublicationErrorV1,
) -> NativeTrainingStorePublisherV2Error {
    if error.kind() != DurablePublicationErrorKindV1::FinalCollision {
        return map_publication_error_v2(error);
    }
    let mismatch = publisher_error_v2(
        NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
    );
    let Ok(final_basename) = final_name.final_basename() else {
        return mismatch;
    };
    let Ok(expectation) = DurableFileExpectationV1::from_bytes(bytes) else {
        return mismatch;
    };
    match verify_existing_publication_v1(
        parents.parent_v2(final_name.directory()),
        &final_basename,
        expectation,
    ) {
        Ok(_) => publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::PublicationFailed),
        Err(_) => mismatch,
    }
}

fn read_exact_final_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    final_name: NativeTrainingStoreFinalNameV2,
    expected: &[u8],
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> PublisherResult<Vec<u8>> {
    let error = publisher_error_v2(kind);
    let basename = final_basename_v2(final_name)?;
    let path = root
        .directory_path_v2(final_name.directory())
        .join(&basename);
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| error)?;
    if !metadata.is_file() || metadata.len() != expected.len() as u64 {
        return Err(error);
    }
    let bytes = std::fs::read(&path).map_err(|_| error)?;
    if bytes != expected {
        return Err(error);
    }
    Ok(bytes)
}

/// Decoded authorities reconstructed strictly from reopened final bytes.
struct ReopenedGenerationV2 {
    boundary: ValidatedNativeTrainingBoundaryV2,
    reference: ValidatedCheckpointReferenceV2,
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
}

fn decode_generation_candidate_v2(
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> PublisherResult<ReopenedGenerationV2> {
    let error = publisher_error_v2(kind);
    let (checkpoint, boundary) = match parent {
        PublisherParentV2::Genesis => {
            let checkpoint = decode_checkpoint_manifest_v3(
                input.checkpoint_manifest,
                input.checkpoint_payload,
                run,
            )
            .map_err(|_| error)?;
            let segment =
                decode_genesis_segment_manifest_v2(input.segment_manifest, run, &checkpoint)
                    .map_err(|_| error)?;
            let boundary = decode_genesis_native_training_boundary_v2(
                input.checkpoint_sidecar,
                input.head_record,
                run,
                &segment,
                &checkpoint,
            )
            .map_err(|_| error)?;
            (checkpoint, boundary)
        }
        PublisherParentV2::Trained {
            parent,
            parent_checkpoint,
        } => {
            let parent_context = resume_update_evidence_chain_v1(run, parent, parent_checkpoint)
                .map_err(|_| error)?;
            let continuations =
                decode_segment_continuations_v2(run, parent_context, &input.continuations)
                    .map_err(|_| error)?;
            let checkpoint = decode_trained_checkpoint_manifest_v3(
                input.checkpoint_manifest,
                input.checkpoint_payload,
                run,
                continuations.advanced_context(),
            )
            .map_err(|_| error)?;
            let segment = decode_trained_segment_manifest_v2(
                input.segment_manifest,
                run,
                parent,
                &continuations,
                &checkpoint,
            )
            .map_err(|_| error)?;
            let boundary = decode_trained_native_training_boundary_v2(
                input.checkpoint_sidecar,
                input.head_record,
                run,
                parent,
                &segment,
                &checkpoint,
            )
            .map_err(|_| error)?;
            (checkpoint, boundary)
        }
    };
    if checkpoint.generation_index() != input.generation_index {
        return Err(error);
    }
    let reference = decode_checkpoint_reference_v2(input.checkpoint_reference, run, &boundary)
        .map_err(|_| error)?;
    Ok(ReopenedGenerationV2 {
        boundary,
        reference,
        generation_index: input.generation_index,
        checkpoint_payload_sha256: sha256_v1(input.checkpoint_payload),
        checkpoint_manifest_sha256: sha256_v1(input.checkpoint_manifest),
    })
}

fn prevalidate_publication_candidate_v2(
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
) -> PublisherResult<()> {
    let kind = NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid;
    let error = publisher_error_v2(kind);
    let candidate = decode_generation_candidate_v2(run, parent, input, kind)?;
    decode_latest_v2(input.latest, &candidate.boundary, &candidate.reference)
        .map(|_| ())
        .map_err(|_| error)
}

/// Reopen and independently revalidate the complete generation: every final
/// byte is reread, every hash recomputed, and the full decode chain rerun
/// against the run and parent authorities.
fn decode_generation_from_disk_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> PublisherResult<ReopenedGenerationV2> {
    let error = publisher_error_v2(kind);
    let generation_index = input.generation_index;
    let payload = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        input.checkpoint_payload,
        kind,
    )?;
    let manifest = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index },
        input.checkpoint_manifest,
        kind,
    )?;
    let segment_manifest = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index },
        input.segment_manifest,
        kind,
    )?;
    let sidecar = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index },
        input.checkpoint_sidecar,
        kind,
    )?;
    let head = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::HeadRecord { generation_index },
        input.head_record,
        kind,
    )?;
    let reference_bytes = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::CheckpointReference { generation_index },
        input.checkpoint_reference,
        kind,
    )?;

    let continuation_bytes = input
        .continuations
        .iter()
        .enumerate()
        .map(|(index, expected)| {
            let continuation_index = u64::try_from(index).map_err(|_| error)?;
            read_exact_final_v2(
                root,
                NativeTrainingStoreFinalNameV2::SegmentContinuation {
                    generation_index,
                    continuation_index,
                },
                expected,
                kind,
            )
        })
        .collect::<PublisherResult<Vec<Vec<u8>>>>()?;
    let continuation_refs = continuation_bytes.iter().map(Vec::as_slice).collect();
    let reopened_input = GenerationPublicationInputV2 {
        generation_index,
        checkpoint_payload: &payload,
        checkpoint_manifest: &manifest,
        continuations: continuation_refs,
        segment_manifest: &segment_manifest,
        checkpoint_sidecar: &sidecar,
        head_record: &head,
        checkpoint_reference: &reference_bytes,
        latest: input.latest,
    };
    decode_generation_candidate_v2(run, parent, &reopened_input, kind)
}

fn revalidate_generation_from_disk_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
) -> PublisherResult<()> {
    decode_generation_from_disk_v2(
        root,
        run,
        parent,
        input,
        NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid,
    )
    .map(|_| ())
}

/// Reopen `latest.json` after the replacement and independently revalidate it
/// plus every referenced immutable file again; only these observed bytes may
/// source the receipt.
fn revalidate_latest_and_referenced_v2(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    parent: &PublisherParentV2<'_>,
    input: &GenerationPublicationInputV2<'_>,
) -> PublisherResult<ReopenedGenerationV2> {
    let kind = NativeTrainingStorePublisherV2ErrorKind::LatestInvalid;
    let error = publisher_error_v2(kind);
    let reopened = decode_generation_from_disk_v2(root, run, parent, input, kind)?;
    let latest_bytes = read_exact_final_v2(
        root,
        NativeTrainingStoreFinalNameV2::Latest,
        input.latest,
        kind,
    )?;
    decode_latest_v2(&latest_bytes, &reopened.boundary, &reopened.reference).map_err(|_| error)?;
    Ok(reopened)
}

#[cfg(all(test, windows))]
mod windows_publisher_tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
    };
    use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
    use crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3;
    use crate::native_training_store_layout_v2::NATIVE_TRAINING_STORE_LOCK_LEAF_V2;
    use crate::native_training_store_prepared_segment_v2::prepare_segment_v2;
    use crate::native_training_store_reference_latest_v2::{
        build_checkpoint_reference_v2, build_latest_v2,
    };
    use crate::native_training_store_root_v2::windows_store_root_v2::{
        identity_v2, open_no_follow_v2, FILE_SHARE_DELETE_V2, FILE_SHARE_READ_V2,
        FILE_SHARE_WRITE_V2, GENERIC_READ_V2,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    struct TestStoreV2 {
        root: PathBuf,
    }

    impl TestStoreV2 {
        fn with_skeleton(label: &str) -> Self {
            static ORDINAL: AtomicU64 = AtomicU64::new(0);
            let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "mtg-kernel-store-publisher-v2-{}-{label}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&root).expect("create test root");
            for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
                fs::create_dir(root.join(directory.basename().unwrap()))
                    .expect("create subdirectory");
            }
            fs::write(root.join(NATIVE_TRAINING_STORE_LOCK_LEAF_V2), []).expect("create lock leaf");
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TestStoreV2 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
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
        let (manifest, payload) = common_model_snapshot_paths_v1();
        NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v2(run),
            &manifest,
            &payload,
        )
        .unwrap()
    }

    struct GenesisAuthoritiesV2 {
        payload: Vec<u8>,
        checkpoint: CheckpointManifestV3,
        segment: SegmentManifestV2,
        boundary: ValidatedNativeTrainingBoundaryV2,
        reference: ValidatedCheckpointReferenceV2,
        latest: ValidatedLatestRecordV2,
    }

    fn genesis_authorities_v2(
        run: &ValidatedTrainRunV2,
        executor: &NativeTrainingExecutorV1,
    ) -> GenesisAuthoritiesV2 {
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        GenesisAuthoritiesV2 {
            payload,
            checkpoint,
            segment,
            boundary,
            reference,
            latest,
        }
    }

    fn publish_genesis_v2(
        root: &ValidatedNativeTrainingStoreRootV2,
        run: &ValidatedTrainRunV2,
        genesis: &GenesisAuthoritiesV2,
    ) -> PublisherResult<NativeTrainingPersistenceReceiptV2> {
        publish_genesis_generation_v2(
            root,
            run,
            &genesis.payload,
            &genesis.checkpoint,
            &genesis.segment,
            &genesis.boundary,
            &genesis.reference,
            &genesis.latest,
        )
    }

    fn final_path_v2(
        root: &ValidatedNativeTrainingStoreRootV2,
        final_name: NativeTrainingStoreFinalNameV2,
    ) -> PathBuf {
        root.directory_path_v2(final_name.directory())
            .join(final_name.final_basename().unwrap())
    }

    fn snapshot_store_tree_v2(
        root: &ValidatedNativeTrainingStoreRootV2,
    ) -> BTreeMap<PathBuf, Option<(u64, [u8; 16], Vec<u8>)>> {
        fn walk_v2(
            base: &Path,
            directory: &Path,
            snapshot: &mut BTreeMap<PathBuf, Option<(u64, [u8; 16], Vec<u8>)>>,
        ) {
            let mut entries: Vec<_> = fs::read_dir(directory)
                .expect("read Store directory")
                .map(|entry| entry.expect("read Store entry"))
                .collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let relative = path
                    .strip_prefix(base)
                    .expect("Store entry remains below root")
                    .to_path_buf();
                let file_type = entry.file_type().expect("read Store entry type");
                if file_type.is_dir() {
                    snapshot.insert(relative, None);
                    walk_v2(base, &path, snapshot);
                } else {
                    let handle = open_no_follow_v2(
                        &path,
                        GENERIC_READ_V2,
                        FILE_SHARE_READ_V2 | FILE_SHARE_WRITE_V2 | FILE_SHARE_DELETE_V2,
                        false,
                        NativeTrainingStoreRootV2ErrorKind::RootInvalid,
                    )
                    .expect("open Store file without following reparses");
                    let identity =
                        identity_v2(&handle, NativeTrainingStoreRootV2ErrorKind::RootInvalid)
                            .expect("capture Store file identity");
                    snapshot.insert(
                        relative,
                        Some((
                            identity.volume_serial_number,
                            identity.file_id,
                            fs::read(path).expect("read Store file"),
                        )),
                    );
                }
            }
        }

        let base = root.directory_path_v2(NativeTrainingStoreDirectoryV2::Root);
        let mut snapshot = BTreeMap::new();
        walk_v2(&base, &base, &mut snapshot);
        snapshot
    }

    /// Reload one trained generation strictly from published files.
    fn load_trained_generation_v2(
        root: &ValidatedNativeTrainingStoreRootV2,
        run: &ValidatedTrainRunV2,
        parent: &ValidatedNativeTrainingBoundaryV2,
        parent_checkpoint: &CheckpointManifestV3,
        generation_index: u64,
    ) -> (CheckpointManifestV3, ValidatedNativeTrainingBoundaryV2) {
        let payload = fs::read(final_path_v2(
            root,
            NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        ))
        .unwrap();
        let manifest = fs::read(final_path_v2(
            root,
            NativeTrainingStoreFinalNameV2::CheckpointManifest { generation_index },
        ))
        .unwrap();
        let segment_manifest = fs::read(final_path_v2(
            root,
            NativeTrainingStoreFinalNameV2::SegmentManifest { generation_index },
        ))
        .unwrap();
        let sidecar = fs::read(final_path_v2(
            root,
            NativeTrainingStoreFinalNameV2::CheckpointSidecar { generation_index },
        ))
        .unwrap();
        let head = fs::read(final_path_v2(
            root,
            NativeTrainingStoreFinalNameV2::HeadRecord { generation_index },
        ))
        .unwrap();
        let mut continuation_bytes = Vec::new();
        loop {
            let continuation_index = u64::try_from(continuation_bytes.len()).unwrap();
            let path = final_path_v2(
                root,
                NativeTrainingStoreFinalNameV2::SegmentContinuation {
                    generation_index,
                    continuation_index,
                },
            );
            if fs::symlink_metadata(&path).is_err() {
                break;
            }
            continuation_bytes.push(fs::read(&path).unwrap());
        }
        let parent_context =
            resume_update_evidence_chain_v1(run, parent, parent_checkpoint).unwrap();
        let continuations =
            decode_segment_continuations_v2(run, parent_context, &continuation_bytes).unwrap();
        let checkpoint = decode_trained_checkpoint_manifest_v3(
            &manifest,
            &payload,
            run,
            continuations.advanced_context(),
        )
        .unwrap();
        let segment = decode_trained_segment_manifest_v2(
            &segment_manifest,
            run,
            parent,
            &continuations,
            &checkpoint,
        )
        .unwrap();
        let boundary = decode_trained_native_training_boundary_v2(
            &sidecar,
            &head,
            run,
            parent,
            &segment,
            &checkpoint,
        )
        .unwrap();
        (checkpoint, boundary)
    }

    #[test]
    fn genesis_and_two_trained_generations_publish_commit_and_reload_exactly() {
        let store = TestStoreV2::with_skeleton("lifecycle");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);

        let genesis_receipt = publish_genesis_v2(&root, &run, &genesis).unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);
        assert_eq!(
            genesis_receipt.checkpoint_payload_sha256(),
            sha256_v1(&genesis.payload)
        );
        assert_eq!(
            genesis_receipt.checkpoint_manifest_sha256(),
            sha256_v1(genesis.checkpoint.canonical_bytes())
        );
        assert_eq!(
            fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Run)).unwrap(),
            run.canonical_bytes()
        );
        assert_eq!(
            fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest)).unwrap(),
            genesis.latest.canonical_bytes()
        );

        // Genesis publication is idempotent over candidate-equal finals.
        let repeated = publish_genesis_v2(&root, &run, &genesis).unwrap();
        assert_eq!(repeated, genesis_receipt);

        let prepared =
            prepare_segment_v2(&mut executor, &run, &genesis.boundary, &genesis.checkpoint)
                .unwrap();
        let first_generation = prepared.expected_generation_index();
        let receipt = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &prepared,
        )
        .unwrap();
        assert_eq!(receipt.generation_index(), first_generation);
        prepared.commit_v2(receipt).unwrap();

        let (checkpoint_s, boundary_s) = load_trained_generation_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            first_generation,
        );

        let prepared = prepare_segment_v2(&mut executor, &run, &boundary_s, &checkpoint_s).unwrap();
        let second_generation = prepared.expected_generation_index();
        assert_eq!(
            second_generation,
            first_generation + run.checkpoint_segment_updates()
        );
        let latest_bytes = prepared
            .publication_view_v2()
            .latest_canonical_bytes_v2()
            .to_vec();
        let receipt =
            publish_prepared_segment_v2(&root, &run, &boundary_s, &checkpoint_s, &prepared)
                .unwrap();
        prepared.commit_v2(receipt).unwrap();
        assert_eq!(
            fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest)).unwrap(),
            latest_bytes
        );
        let (checkpoint_2s, _) =
            load_trained_generation_v2(&root, &run, &boundary_s, &checkpoint_s, second_generation);
        assert_eq!(checkpoint_2s.generation_index(), second_generation);

        // A trained checkpoint can never masquerade as a genesis input.
        assert_eq!(
            publish_genesis_generation_v2(
                &root,
                &run,
                &genesis.payload,
                &checkpoint_2s,
                &genesis.segment,
                &genesis.boundary,
                &genesis.reference,
                &genesis.latest,
            )
            .unwrap_err()
            .kind(),
            NativeTrainingStorePublisherV2ErrorKind::InputInvalid
        );
    }

    /// Regression guard for the preflight itself, not proof of the repair:
    /// the pre-preflight publisher already recovered the run-only state, and
    /// this scenario stays recoverable only because the Genesis authority arm
    /// returns early when latest.json is absent (the lineage walk needs
    /// latest to anchor). Reverting the currentness fix does not flip this
    /// test; removing that early return would.
    #[test]
    fn genesis_retry_recovers_after_run_authority_without_latest() {
        let store = TestStoreV2::with_skeleton("genesis-run-only-retry");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let injected = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::StoreBusy);

        let interrupted = publish_genesis_generation_with_hook_v2(
            &root,
            &run,
            &genesis.payload,
            &genesis.checkpoint,
            &genesis.segment,
            &genesis.boundary,
            &genesis.reference,
            &genesis.latest,
            |reached| {
                if reached == PublisherBoundaryV2::AfterRunAuthority {
                    Err(injected)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            interrupted.unwrap_err().kind(),
            NativeTrainingStorePublisherV2ErrorKind::StoreBusy
        );
        assert_eq!(
            fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Run)).unwrap(),
            run.canonical_bytes()
        );
        assert!(
            fs::symlink_metadata(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest))
                .is_err(),
            "the injected boundary precedes every genesis immutable and latest"
        );

        let receipt = publish_genesis_v2(&root, &run, &genesis).unwrap();
        assert_eq!(receipt.generation_index(), 0);
        assert_eq!(
            fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest)).unwrap(),
            genesis.latest.canonical_bytes()
        );
    }

    #[test]
    fn genesis_rejects_future_final_without_mutating_store() {
        let store = TestStoreV2::with_skeleton("genesis-future-final");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let future_final = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload {
                generation_index: run.checkpoint_segment_updates(),
            },
        );
        fs::write(&future_final, b"future-final-evidence").unwrap();
        let before = snapshot_store_tree_v2(&root);

        let error = publish_genesis_v2(&root, &run, &genesis).unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
        assert_eq!(
            snapshot_store_tree_v2(&root),
            before,
            "genesis inventory rejection must precede run or candidate publication"
        );
    }

    #[test]
    fn semantic_candidate_rejection_precedes_every_store_mutation() {
        let store = TestStoreV2::with_skeleton("semantic-candidate-preflight");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();
        let prepared =
            prepare_segment_v2(&mut executor, &run, &genesis.boundary, &genesis.checkpoint)
                .unwrap();
        let view = prepared.publication_view_v2();
        let continuations = (0..view.continuation_count_v2())
            .map(|index| view.continuation_canonical_bytes_v2(index).unwrap())
            .collect();
        let mut corrupted_head = view.head_record_canonical_bytes_v2().to_vec();
        let digest_marker = b"sha256\":\"";
        let digest_start = corrupted_head
            .windows(digest_marker.len())
            .position(|window| window == digest_marker)
            .map(|index| index + digest_marker.len())
            .unwrap();
        corrupted_head[digest_start] = if corrupted_head[digest_start] == b'0' {
            b'1'
        } else {
            b'0'
        };
        let input = GenerationPublicationInputV2 {
            generation_index: prepared.expected_generation_index(),
            checkpoint_payload: view.checkpoint_payload_v2(),
            checkpoint_manifest: view.checkpoint_manifest_canonical_bytes_v2(),
            continuations,
            segment_manifest: view.segment_manifest_canonical_bytes_v2(),
            checkpoint_sidecar: view.checkpoint_sidecar_canonical_bytes_v2(),
            head_record: &corrupted_head,
            checkpoint_reference: view.checkpoint_reference_canonical_bytes_v2(),
            latest: view.latest_canonical_bytes_v2(),
        };
        let preserved_stage = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join(
                NativeTrainingStoreFinalNameV2::StatePayload {
                    generation_index: input.generation_index,
                }
                .stage_basename()
                .unwrap(),
            );
        fs::write(&preserved_stage, b"preserve-semantic-preflight-stage").unwrap();
        let before = snapshot_store_tree_v2(&root);

        let error = publish_generation_v2(
            &root,
            &run,
            PublisherParentV2::Trained {
                parent: &genesis.boundary,
                parent_checkpoint: &genesis.checkpoint,
            },
            &input,
            |_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
        assert_eq!(
            snapshot_store_tree_v2(&root),
            before,
            "semantic candidate rejection must precede stage cleanup and immutable publication"
        );
    }

    #[test]
    fn walked_parent_expectations_reject_same_length_content_drift() {
        let store = TestStoreV2::with_skeleton("walked-parent-drift");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();

        root.recapture_v2().unwrap();
        let _lock = root.lock_exclusive_v2().unwrap();
        let parents = PublicationParentsV2::capture_v2(&root).unwrap();
        let walked = validate_native_training_store_for_publication_v2(&root, &run).unwrap();
        let payload_path = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload {
                generation_index: 0,
            },
        );
        let original = fs::read(&payload_path).unwrap();
        let corrupted: Vec<_> = original.iter().map(|byte| byte ^ 0x5a).collect();
        fs::write(&payload_path, corrupted).unwrap();

        assert_eq!(
            verify_walked_final_expectations_v2(&parents, &walked)
                .unwrap_err()
                .kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
    }

    #[test]
    fn stale_parent_cannot_roll_latest_backward_or_sweep_stage_evidence() {
        let store = TestStoreV2::with_skeleton("stale-parent-currentness");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut stale_executor = fresh_executor_v2(&run);
        let mut advancing_executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &stale_executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();

        // Writer A reconstructs from genesis and prepares S, then pauses after
        // its resume lock has been released.
        let stale = prepare_segment_v2(
            &mut stale_executor,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
        )
        .unwrap();

        // Writer B advances the authoritative Store through S and 2S.
        let first = prepare_segment_v2(
            &mut advancing_executor,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
        )
        .unwrap();
        let first_generation = first.expected_generation_index();
        let receipt = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &first,
        )
        .unwrap();
        first.commit_v2(receipt).unwrap();
        let (checkpoint_s, boundary_s) = load_trained_generation_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            first_generation,
        );
        let second =
            prepare_segment_v2(&mut advancing_executor, &run, &boundary_s, &checkpoint_s).unwrap();
        let second_generation = second.expected_generation_index();
        let second_latest = second
            .publication_view_v2()
            .latest_canonical_bytes_v2()
            .to_vec();
        let receipt =
            publish_prepared_segment_v2(&root, &run, &boundary_s, &checkpoint_s, &second).unwrap();
        second.commit_v2(receipt).unwrap();

        // Currentness is checked before even recognized-stage cleanup, so a
        // rejected stale writer cannot mutate unrelated recovery evidence.
        let future_generation = second_generation + run.checkpoint_segment_updates();
        let stage_name = NativeTrainingStoreFinalNameV2::StatePayload {
            generation_index: future_generation,
        }
        .stage_basename()
        .unwrap();
        let stage_path = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join(stage_name);
        fs::write(&stage_path, b"preserve-stale-stage-evidence").unwrap();
        let before = snapshot_store_tree_v2(&root);

        let error = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &stale,
        )
        .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
        assert_eq!(
            snapshot_store_tree_v2(&root),
            before,
            "stale-parent rejection must preserve every Store object identity and byte"
        );
        assert_eq!(
            fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest)).unwrap(),
            second_latest,
            "a stale parent must not roll latest from 2S back to S"
        );
        assert_eq!(
            fs::read(stage_path).unwrap(),
            b"preserve-stale-stage-evidence",
            "currentness rejection must precede every publisher mutation"
        );
    }

    #[test]
    fn unscheduled_continuation_blocks_latest_and_receipt() {
        let store = TestStoreV2::with_skeleton("extra-continuation");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();
        let prepared =
            prepare_segment_v2(&mut executor, &run, &genesis.boundary, &genesis.checkpoint)
                .unwrap();
        let generation_index = prepared.expected_generation_index();
        let candidate_payload = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        );
        assert!(fs::symlink_metadata(&candidate_payload).is_err());

        let extra_final = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::SegmentContinuation {
                generation_index,
                continuation_index: 99_999_999,
            },
        );
        fs::write(&extra_final, b"unscheduled-continuation-evidence").unwrap();
        let before = snapshot_store_tree_v2(&root);

        let error = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &prepared,
        )
        .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
        assert_eq!(
            snapshot_store_tree_v2(&root),
            before,
            "an inexact target-generation inventory must not mutate any Store byte"
        );
        assert!(
            fs::symlink_metadata(&candidate_payload).is_err(),
            "preflight must reject before publishing the first scheduled final"
        );

        fs::remove_file(&extra_final).unwrap();
        let receipt = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &prepared,
        )
        .unwrap();
        prepared.commit_v2(receipt).unwrap();
        let (checkpoint_s, boundary_s) = load_trained_generation_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            generation_index,
        );
        let second = prepare_segment_v2(&mut executor, &run, &boundary_s, &checkpoint_s).unwrap();
        let second_generation = second.expected_generation_index();
        let second_payload = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload {
                generation_index: second_generation,
            },
        );
        assert!(fs::symlink_metadata(&second_payload).is_err());

        let past_orphan = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::SegmentContinuation {
                generation_index: 0,
                continuation_index: 0,
            },
        );
        fs::write(&past_orphan, b"past-orphan-evidence").unwrap();
        let before = snapshot_store_tree_v2(&root);
        let error = publish_prepared_segment_v2(&root, &run, &boundary_s, &checkpoint_s, &second)
            .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
        assert_eq!(
            snapshot_store_tree_v2(&root),
            before,
            "a past orphan must block publication before cleanup or candidate writes"
        );
        assert!(fs::symlink_metadata(&second_payload).is_err());
    }

    #[test]
    fn missing_parent_immutable_blocks_publication_without_mutation() {
        let store = TestStoreV2::with_skeleton("missing-parent-immutable");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();
        let first = prepare_segment_v2(&mut executor, &run, &genesis.boundary, &genesis.checkpoint)
            .unwrap();
        let first_generation = first.expected_generation_index();
        let receipt = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &first,
        )
        .unwrap();
        first.commit_v2(receipt).unwrap();
        let (checkpoint_s, boundary_s) = load_trained_generation_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            first_generation,
        );
        let second = prepare_segment_v2(&mut executor, &run, &boundary_s, &checkpoint_s).unwrap();
        let second_generation = second.expected_generation_index();
        let second_payload = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload {
                generation_index: second_generation,
            },
        );
        assert!(fs::symlink_metadata(&second_payload).is_err());

        let preserved_stage = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join(
                NativeTrainingStoreFinalNameV2::StatePayload {
                    generation_index: second_generation,
                }
                .stage_basename()
                .unwrap(),
            );
        fs::write(&preserved_stage, b"preserve-parent-corruption-stage").unwrap();
        let parent_sidecar = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::CheckpointSidecar {
                generation_index: first_generation,
            },
        );
        fs::remove_file(parent_sidecar).unwrap();
        let before = snapshot_store_tree_v2(&root);

        let error = publish_prepared_segment_v2(&root, &run, &boundary_s, &checkpoint_s, &second)
            .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid
        );
        assert_eq!(
            snapshot_store_tree_v2(&root),
            before,
            "a broken parent lineage must be rejected before every publisher mutation"
        );
        assert!(fs::symlink_metadata(&second_payload).is_err());
    }

    #[test]
    fn every_fault_boundary_returns_no_receipt_and_retry_resumes_exactly() {
        let store = TestStoreV2::with_skeleton("fault-matrix");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();
        let genesis_latest_bytes = genesis.latest.canonical_bytes().to_vec();

        let prepared =
            prepare_segment_v2(&mut executor, &run, &genesis.boundary, &genesis.checkpoint)
                .unwrap();
        let generation_index = prepared.expected_generation_index();
        let view = prepared.publication_view_v2();
        let trained_latest_bytes = view.latest_canonical_bytes_v2().to_vec();
        let payload_final = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        );
        let continuation_count = view.continuation_count_v2();
        assert!(continuation_count >= 1);

        let mut boundaries = vec![
            PublisherBoundaryV2::AfterStageSweep,
            PublisherBoundaryV2::AfterRunAuthority,
        ];
        for ordinal in 0..(6 + continuation_count) {
            boundaries.push(PublisherBoundaryV2::AfterImmutableFinal(ordinal));
        }
        boundaries.extend([
            PublisherBoundaryV2::AfterGenerationRevalidation,
            PublisherBoundaryV2::BeforeLatestReplacement,
            PublisherBoundaryV2::AfterLatestReplacement,
            PublisherBoundaryV2::AfterLatestReopenRevalidation,
            PublisherBoundaryV2::BeforeReceiptConstruction,
        ]);

        let injected = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::StoreBusy);
        let mut payload_first_seen: Option<Vec<u8>> = None;
        for &boundary in &boundaries {
            let result = publish_prepared_segment_with_hook_v2(
                &root,
                &run,
                &genesis.boundary,
                &genesis.checkpoint,
                &prepared,
                |reached| {
                    if reached == boundary {
                        Err(injected)
                    } else {
                        Ok(())
                    }
                },
            );
            assert_eq!(
                result.unwrap_err().kind(),
                NativeTrainingStorePublisherV2ErrorKind::StoreBusy,
                "boundary {boundary:?} must interrupt publication without a receipt"
            );
            let latest_now =
                fs::read(final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest)).unwrap();
            let latest_replaced = matches!(
                boundary,
                PublisherBoundaryV2::AfterLatestReplacement
                    | PublisherBoundaryV2::AfterLatestReopenRevalidation
                    | PublisherBoundaryV2::BeforeReceiptConstruction
            );
            if latest_replaced {
                assert_eq!(latest_now, trained_latest_bytes);
            } else {
                assert_eq!(
                    latest_now, genesis_latest_bytes,
                    "latest must stay authoritative until replaced last at {boundary:?}"
                );
            }
            if let Ok(bytes) = fs::read(&payload_final) {
                match &payload_first_seen {
                    None => payload_first_seen = Some(bytes),
                    Some(first) => assert_eq!(
                        &bytes, first,
                        "published immutable bytes must never change across retries"
                    ),
                }
            }
        }

        // A same-length corruption of a published immutable final is
        // detected as immutable-final-mismatch corruption and preserved.
        let published_payload = fs::read(&payload_final).unwrap();
        let corrupted: Vec<u8> = published_payload.iter().map(|byte| byte ^ 0xa5).collect();
        fs::write(&payload_final, &corrupted).unwrap();
        let mismatch = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &prepared,
        )
        .unwrap_err();
        assert_eq!(
            mismatch.kind(),
            NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption
        );
        assert_eq!(mismatch.code(), "immutable-final-mismatch-corruption");
        assert_eq!(
            fs::read(&payload_final).unwrap(),
            corrupted,
            "a mismatching final must never be overwritten, renamed, or promoted"
        );
        fs::write(&payload_final, &published_payload).unwrap();

        // The clean retry resumes every candidate-equal final and commits.
        let receipt = publish_prepared_segment_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &prepared,
        )
        .unwrap();
        assert_eq!(receipt.generation_index(), generation_index);
        assert_eq!(
            receipt.checkpoint_payload_sha256(),
            sha256_v1(&published_payload)
        );
        prepared.commit_v2(receipt).unwrap();
    }

    /// The mandatory replay witness: termination is injected at the exact
    /// point after all immutable finals for boundary `U` have passed the
    /// complete-generation revalidation but before latest replacement. The
    /// restarted process must treat parent `U-S` as authoritative, replay
    /// exactly the lost window from recorded facts, reproduce every immutable
    /// candidate byte, reuse the candidate-equal finals, publish latest, and
    /// resume successfully. Equality of only model parameters, losses, or
    /// logical progress is insufficient: the receipt hashes bind the full
    /// reopened train-state payload bytes.
    #[test]
    fn killed_before_latest_replay_witness_reproduces_every_candidate_byte() {
        use crate::native_training_store_resume_v2::{
            resume_native_training_store_v2, NativeTrainingStoreResumeV2,
        };

        let store = TestStoreV2::with_skeleton("replay-witness");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let mut executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();
        let genesis_latest_bytes = genesis.latest.canonical_bytes().to_vec();

        // Prepare the first trained window and record the test-only oracle
        // outside the store: every candidate hash plus the latest bytes.
        let prepared =
            prepare_segment_v2(&mut executor, &run, &genesis.boundary, &genesis.checkpoint)
                .unwrap();
        let generation_index = prepared.expected_generation_index();
        let view = prepared.publication_view_v2();
        let oracle_payload_sha256 = view.checkpoint_payload_sha256_v2();
        let oracle_manifest_sha256 = view.checkpoint_manifest_sha256_v2();
        let oracle_latest_bytes = view.latest_canonical_bytes_v2().to_vec();

        // Inject termination after the complete-generation revalidation but
        // strictly before latest replacement, then drop the guard and the
        // live executor to simulate the killed process.
        let injected = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::StoreBusy);
        let interrupted = publish_prepared_segment_with_hook_v2(
            &root,
            &run,
            &genesis.boundary,
            &genesis.checkpoint,
            &prepared,
            |reached| {
                if reached == PublisherBoundaryV2::BeforeLatestReplacement {
                    Err(injected)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            interrupted.unwrap_err().kind(),
            NativeTrainingStorePublisherV2ErrorKind::StoreBusy
        );
        drop(prepared);
        drop(executor);
        let latest_path = final_path_v2(&root, NativeTrainingStoreFinalNameV2::Latest);
        assert_eq!(
            fs::read(&latest_path).unwrap(),
            genesis_latest_bytes,
            "the prior latest must remain authoritative after the kill"
        );
        let payload_final = final_path_v2(
            &root,
            NativeTrainingStoreFinalNameV2::StatePayload { generation_index },
        );
        let partial_payload_bytes = fs::read(&payload_final).unwrap();
        assert_eq!(sha256_v1(&partial_payload_bytes), oracle_payload_sha256);

        // Restart: resume treats parent U-S as authoritative and hands back a
        // reconstructed executor for the exact lost window.
        let resumed = resume_native_training_store_v2(
            &root,
            &run,
            crate::native_training_store_resume_v2::test_execution_config_v2(&run),
        )
        .unwrap();
        let mut continuation = match resumed {
            NativeTrainingStoreResumeV2::Continue(continuation) => continuation,
            NativeTrainingStoreResumeV2::Complete { .. } => {
                panic!("a killed-before-latest store must resume, not no-op")
            }
        };
        assert_eq!(continuation.parent_generation_index, 0);

        // Replay the window and republish: every immutable candidate byte
        // must reproduce so the candidate-equal finals are reused, latest is
        // published, and the receipt binds the reopened payload bytes.
        let replayed = prepare_segment_v2(
            &mut continuation.executor,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
        )
        .unwrap();
        assert_eq!(replayed.expected_generation_index(), generation_index);
        let replay_view = replayed.publication_view_v2();
        assert_eq!(
            replay_view.checkpoint_payload_sha256_v2(),
            oracle_payload_sha256
        );
        assert_eq!(
            replay_view.checkpoint_manifest_sha256_v2(),
            oracle_manifest_sha256
        );
        let receipt = publish_prepared_segment_v2(
            &root,
            &run,
            &continuation.parent_boundary,
            &continuation.parent_checkpoint,
            &replayed,
        )
        .unwrap();
        assert_eq!(receipt.generation_index(), generation_index);
        assert_eq!(receipt.checkpoint_payload_sha256(), oracle_payload_sha256);
        assert_eq!(receipt.checkpoint_manifest_sha256(), oracle_manifest_sha256);
        replayed.commit_v2(receipt).unwrap();

        assert_eq!(
            fs::read(&payload_final).unwrap(),
            partial_payload_bytes,
            "the candidate-equal final must be reused, never rewritten"
        );
        assert_eq!(fs::read(&latest_path).unwrap(), oracle_latest_bytes);
    }

    #[test]
    fn stage_hygiene_and_run_authority_fail_closed_and_preserve_evidence() {
        let store = TestStoreV2::with_skeleton("hygiene");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store.path()).unwrap();
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let executor = fresh_executor_v2(&run);
        let genesis = genesis_authorities_v2(&run, &executor);

        // An unknown leaf fails closed and is preserved.
        let unknown = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            .join("unexpected.bin");
        fs::write(&unknown, b"evidence").unwrap();
        assert_eq!(
            publish_genesis_v2(&root, &run, &genesis)
                .unwrap_err()
                .kind(),
            NativeTrainingStorePublisherV2ErrorKind::StageCorruption
        );
        assert_eq!(fs::read(&unknown).unwrap(), b"evidence");
        fs::remove_file(&unknown).unwrap();

        // A malformed stage-like leaf fails closed and is preserved.
        let malformed = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Segments)
            .join(".segment-00000000.json.stage-v3");
        fs::write(&malformed, b"evidence").unwrap();
        assert_eq!(
            publish_genesis_v2(&root, &run, &genesis)
                .unwrap_err()
                .kind(),
            NativeTrainingStorePublisherV2ErrorKind::StageCorruption
        );
        assert_eq!(fs::read(&malformed).unwrap(), b"evidence");
        fs::remove_file(&malformed).unwrap();

        // Before run authority exists, a generation stage is out of state and
        // must be preserved rather than treated as cleanup input.
        let stale_stage = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Segments)
            .join(".segment-00000000.json.stage-v2");
        fs::write(&stale_stage, b"stale").unwrap();
        let before = snapshot_store_tree_v2(&root);
        assert_eq!(
            publish_genesis_v2(&root, &run, &genesis)
                .unwrap_err()
                .kind(),
            NativeTrainingStorePublisherV2ErrorKind::StageCorruption
        );
        assert_eq!(snapshot_store_tree_v2(&root), before);
        fs::remove_file(&stale_stage).unwrap();

        // Once exact run authority exists, the same recognized generation
        // stage is valid cleanup input for interrupted genesis recovery.
        let injected = publisher_error_v2(NativeTrainingStorePublisherV2ErrorKind::StoreBusy);
        let interrupted = publish_genesis_generation_with_hook_v2(
            &root,
            &run,
            &genesis.payload,
            &genesis.checkpoint,
            &genesis.segment,
            &genesis.boundary,
            &genesis.reference,
            &genesis.latest,
            |reached| {
                if reached == PublisherBoundaryV2::AfterRunAuthority {
                    Err(injected)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            interrupted.unwrap_err().kind(),
            NativeTrainingStorePublisherV2ErrorKind::StoreBusy
        );
        fs::write(&stale_stage, b"stale").unwrap();
        let _ = publish_genesis_v2(&root, &run, &genesis).unwrap();
        assert!(
            fs::symlink_metadata(&stale_stage).is_err(),
            "recognized stage leaves are non-authoritative and deleted"
        );

        // A mismatching run.json is corruption and is preserved untouched.
        let tampered_store = TestStoreV2::with_skeleton("tampered-run");
        let tampered_root =
            ValidatedNativeTrainingStoreRootV2::open_v2(tampered_store.path()).unwrap();
        let mut tampered_run_bytes = run.canonical_bytes().to_vec();
        let flip_index = tampered_run_bytes.len() / 2;
        tampered_run_bytes[flip_index] ^= 0x01;
        let tampered_run_path = final_path_v2(&tampered_root, NativeTrainingStoreFinalNameV2::Run);
        fs::write(&tampered_run_path, &tampered_run_bytes).unwrap();
        assert_eq!(
            publish_genesis_v2(&tampered_root, &run, &genesis)
                .unwrap_err()
                .kind(),
            NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption
        );
        assert_eq!(fs::read(&tampered_run_path).unwrap(), tampered_run_bytes);
        assert_eq!(
            fs::read_dir(
                tampered_root.directory_path_v2(NativeTrainingStoreDirectoryV2::Checkpoints)
            )
            .unwrap()
            .count(),
            0,
            "no generation content may exist under a mismatching run"
        );
    }
}
