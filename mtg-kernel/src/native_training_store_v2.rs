//! Native trainer persistence boundary shared by the executor and the strict
//! generation store.
//!
//! This module owns the move-only generation receipt and the durable
//! generation publisher. Receipt construction remains private to this module
//! and happens only from independently recaptured published bytes, after
//! immutable publication in the frozen order, complete-generation
//! revalidation, latest-last replacement, and the post-latest referenced
//! revalidation pass. Recovery, currentness, resume orchestration, and the
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
    decode_checkpoint_reference_v2, decode_latest_v2, ValidatedCheckpointReferenceV2,
    ValidatedLatestRecordV2,
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

    sweep_recognized_stages_v2(root)?;
    hook(PublisherBoundaryV2::AfterStageSweep)?;

    establish_run_authority_v2(root, run, &parent, &parents)?;
    hook(PublisherBoundaryV2::AfterRunAuthority)?;

    for (ordinal, immutable) in scheduled.iter().enumerate() {
        publish_or_resume_immutable_v2(root, &parents, immutable)?;
        hook(PublisherBoundaryV2::AfterImmutableFinal(ordinal))?;
    }

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

    let (checkpoint, boundary) = match parent {
        PublisherParentV2::Genesis => {
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
            (checkpoint, boundary)
        }
        PublisherParentV2::Trained {
            parent,
            parent_checkpoint,
        } => {
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
            let parent_context = resume_update_evidence_chain_v1(run, parent, parent_checkpoint)
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
                parent,
                &continuations,
                &checkpoint,
            )
            .map_err(|_| error)?;
            let boundary = decode_trained_native_training_boundary_v2(
                &sidecar,
                &head,
                run,
                parent,
                &segment,
                &checkpoint,
            )
            .map_err(|_| error)?;
            (checkpoint, boundary)
        }
    };
    if checkpoint.generation_index() != generation_index {
        return Err(error);
    }
    let reference =
        decode_checkpoint_reference_v2(&reference_bytes, run, &boundary).map_err(|_| error)?;
    Ok(ReopenedGenerationV2 {
        boundary,
        reference,
        generation_index,
        checkpoint_payload_sha256: sha256_v1(&payload),
        checkpoint_manifest_sha256: sha256_v1(&manifest),
    })
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
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
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

        // Recognized stale stage leaves are deleted before reconstruction.
        let stale_stage = root
            .directory_path_v2(NativeTrainingStoreDirectoryV2::Segments)
            .join(".segment-00000000.json.stage-v2");
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
