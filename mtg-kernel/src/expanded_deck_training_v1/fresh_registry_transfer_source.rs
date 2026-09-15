//! Opt-in trainer continuation from a verified FRESH-origin registry
//! transfer and a fixed, fully resolved episode schedule. Template: the
//! sibling imported-only `registry_transfer_source` module in this same
//! directory. Reuses its resolved-schedule reader, pin checks and
//! `TransferTrainingContextV1` cursor/scalar validation verbatim; only the
//! transfer verification and policy construction calls are fresh-specific.
//! Does not create or extend a schedule.
use super::*;
use crate::phase1_registry_transfer_v1::{
    verify_fresh_registry_transfer_artifact_v1, RegistryTransferScalarsV1,
};
use registry_transfer_source::{
    batch_sha, check_pin, read_schedule, ExpandedRegistryContinuationV1,
    ExpandedRegistryTransferScheduleV1, ExpandedRegistryTransferSourceV1, TransferTrainingContextV1,
};

pub(super) const SOURCE_SCHEMA: &str = "mtg-kernel-expanded-fresh-registry-transfer-source/v1";
pub(super) const CHECKPOINT_SCHEMA_TRANSFER: &str =
    "mtg-kernel-expanded-deck-fresh-checkpoint/v2-registry-transfer";
// Shared with the imported family: no distinct fresh continuation schema is
// defined (R5 names only the envelope, origin wrapper, trainer source
// descriptor and successor checkpoint as fresh-specific schemas). This
// literal mirrors `registry_transfer_source::CONTINUATION_SCHEMA`, which is
// private to that module and therefore cannot be imported directly.
const CONTINUATION_SCHEMA: &str = "mtg-kernel-expanded-registry-continuation/v1";

/// Reconstructs the same value `TransferTrainingContextV1::metadata` (private
/// to the imported sibling module) would compute for the given cursor, using
/// only the pieces already on hand before that context is constructed. This
/// lets the resumed-checkpoint provenance be verified before the final,
/// already-correct context is built via `TransferTrainingContextV1::new`.
fn continuation_metadata(
    descriptor_pin: &PinnedFileV1,
    descriptor: &ExpandedRegistryTransferSourceV1,
    schedule: &ExpandedRegistryTransferScheduleV1,
    transfer_state_sha256: &str,
    scalars: &RegistryTransferScalarsV1,
    completed_updates: u64,
) -> Result<ExpandedRegistryContinuationV1, String> {
    ensure(
        completed_updates > 0 && completed_updates <= schedule.batches.len() as u64,
        "registry-transfer completed cursor is outside the pinned schedule",
    )?;
    Ok(ExpandedRegistryContinuationV1 {
        schema: CONTINUATION_SCHEMA.into(),
        source_descriptor_sha256: descriptor_pin.sha256.clone(),
        source_checkpoint_sha256: descriptor.source_checkpoint.sha256.clone(),
        source_registry_sha256: descriptor.source_registry.sha256.clone(),
        transfer_envelope_sha256: descriptor.transfer_envelope.sha256.clone(),
        continuation_schedule_sha256: descriptor.continuation_schedule.sha256.clone(),
        transfer_state_sha256: transfer_state_sha256.into(),
        transfer_adam_step: scalars.adam_step,
        completed_updates,
        consumed_batch_sha256: batch_sha(&schedule.batches[(completed_updates - 1) as usize])?,
        preserved_scalars: scalars.clone(),
    })
}

pub(super) fn initialize(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
) -> Result<
    (
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        Option<TransferTrainingContextV1>,
    ),
    String,
> {
    check_pin(&source.play_import)?;
    ensure(
        source.feature_transfer.expected_feature_contract_digest == FEATURE_CONTRACT_DIGEST_V3
            && source.feature_transfer.expected_feature_encoding_digest
                == FEATURE_ENCODING_DIGEST_V3,
        "registry-transfer source feature identity differs",
    )?;
    let descriptor: ExpandedRegistryTransferSourceV1 =
        serde_json::from_slice(descriptor_bytes).map_err(err)?;
    ensure(
        descriptor.schema == SOURCE_SCHEMA,
        "registry-transfer source schema differs",
    )?;
    for pin in [
        &descriptor.source_checkpoint,
        &descriptor.source_registry,
        &descriptor.transfer_envelope,
        &descriptor.continuation_schedule,
    ] {
        check_pin(pin)?;
    }
    // Resolve/validate the bounded schedule before loading models or collecting.
    let schedule = read_schedule(&descriptor.continuation_schedule)?;
    let checkpoint_bytes = read_pinned_bytes(&descriptor.source_checkpoint)?;
    let registry_bytes = read_pinned_bytes(&descriptor.source_registry)?;
    let envelope_bytes = read_pinned_bytes(&descriptor.transfer_envelope)?;
    let verified = verify_fresh_registry_transfer_artifact_v1(
        &envelope_bytes,
        &descriptor.transfer_envelope.sha256,
        &checkpoint_bytes,
        &registry_bytes,
    )?;
    ensure(
        verified.request_v1().source_checkpoint_sha256 == descriptor.source_checkpoint.sha256
            && verified.request_v1().source_registry_sha256 == descriptor.source_registry.sha256,
        "registry-transfer envelope source pins differ",
    )?;
    let mut policy = FrozenPlayPolicyV1::from_fresh_registry_transfer_v1(
        &verified,
        &descriptor.transfer_envelope.sha256,
    )?;
    let receipt = verified.receipt_v1().clone();
    ensure(
        schedule.initial_adam_step == receipt.scalars.adam_step
            && schedule.learning_rate_bits == receipt.scalars.learning_rate_bits
            && schedule.value_coefficient_bits == receipt.scalars.value_coefficient_bits,
        "registry-transfer schedule changes restored step or optimizer settings",
    )?;
    let (mut state, scalars) = verified.into_train_state_v1();
    let descriptor_pin = source.play_import.clone();
    let transfer_state_sha256 = receipt.destination_state_sha256.clone();
    let mut completed_updates = 0u64;
    if let Some(pin) = &source.checkpoint {
        check_pin(pin)?;
        let saved: ExpandedCheckpointV1 = read_pinned(pin)?;
        ensure(
            saved.schema == CHECKPOINT_SCHEMA_TRANSFER,
            "explicit registry source requires a registry-transfer successor checkpoint",
        )?;
        let provenance = saved
            .registry_transfer
            .as_ref()
            .ok_or("registry successor transfer provenance absent")?;
        let expected = continuation_metadata(
            &descriptor_pin,
            &descriptor,
            &schedule,
            &transfer_state_sha256,
            &scalars,
            provenance.completed_updates,
        )?;
        ensure(
            provenance == &expected,
            "registry successor transfer provenance, schedule, or cursor differs",
        )?;
        ensure(
            scalars.adam_step.checked_add(provenance.completed_updates) == Some(saved.adam_step)
                && saved.learning_rate_bits == scalars.learning_rate_bits
                && saved.value_coefficient_bits == scalars.value_coefficient_bits,
            "registry successor optimizer step or scalar settings differ",
        )?;
        state = restore_checkpoint_fields_v1(&saved, &mut policy, state.model_v1().clone())?;
        completed_updates = provenance.completed_updates;
    }
    let context = TransferTrainingContextV1::new(
        descriptor_pin,
        descriptor,
        schedule,
        transfer_state_sha256,
        scalars,
        completed_updates,
    );
    Ok((policy, state, Some(context)))
}

#[cfg(test)]
mod tests;
