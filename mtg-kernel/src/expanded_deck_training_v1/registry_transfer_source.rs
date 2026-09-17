//! Opt-in trainer continuation from a verified registry transfer and a fixed,
//! fully resolved episode schedule. Does not create or extend a schedule.
use super::*;
use crate::phase1_registry_transfer_v1::{
    verify_registry_transfer_artifact_v1, RegistryTransferScalarsV1,
};

pub(super) const SOURCE_SCHEMA: &str = "mtg-kernel-expanded-registry-transfer-source/v1";
pub(super) const CHECKPOINT_SCHEMA_TRANSFER: &str =
    "mtg-kernel-expanded-deck-checkpoint/v2-registry-transfer";
pub(super) const SCHEDULE_SCHEMA: &str = "mtg-kernel-expanded-registry-resolved-schedule/v1";
const CONTINUATION_SCHEMA: &str = "mtg-kernel-expanded-registry-continuation/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedRegistryTransferSourceV1 {
    pub schema: String,
    pub source_checkpoint: PinnedFileV1,
    pub source_registry: PinnedFileV1,
    pub transfer_envelope: PinnedFileV1,
    pub continuation_schedule: PinnedFileV1,
}

/// Existing episode type, already resolved to exact opponent sources. Future
/// checkpoint references cannot be represented and are explicitly rejected.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpandedRegistryTransferScheduleV1 {
    pub schema: String,
    pub initial_adam_step: u64,
    pub learning_rate_bits: u32,
    pub value_coefficient_bits: u32,
    pub batches: Vec<Vec<ExpandedEpisodeV1>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExpandedRegistryContinuationV1 {
    pub schema: String,
    pub source_descriptor_sha256: String,
    pub source_checkpoint_sha256: String,
    pub source_registry_sha256: String,
    pub transfer_envelope_sha256: String,
    pub continuation_schedule_sha256: String,
    pub transfer_state_sha256: String,
    pub transfer_adam_step: u64,
    pub completed_updates: u64,
    pub consumed_batch_sha256: String,
    pub preserved_scalars: RegistryTransferScalarsV1,
}

/// Absence retains the legacy wire shape; an explicitly present null is not
/// absence and remains invalid without parsing the large checkpoint twice.
pub(super) fn deserialize_present_continuation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ExpandedRegistryContinuationV1>, D::Error> {
    ExpandedRegistryContinuationV1::deserialize(deserializer).map(Some)
}

pub(super) struct TransferTrainingContextV1 {
    descriptor_pin: PinnedFileV1,
    descriptor: ExpandedRegistryTransferSourceV1,
    schedule: ExpandedRegistryTransferScheduleV1,
    transfer_state_sha256: String,
    scalars: RegistryTransferScalarsV1,
    completed_updates: u64,
}

impl TransferTrainingContextV1 {
    /// Mirrors the fields this module's own `initialize` entry point already
    /// builds inline (construct at zero, then set the resumed cursor after a
    /// successor checkpoint validates). Reused by the fresh-transfer sibling
    /// module so its own resolved-schedule/scalar/cursor logic and readback
    /// stay identical; `completed_updates` is a constructor parameter (not
    /// hardcoded at zero) so the sibling can build the final, already-correct
    /// context once, without needing a private-field mutator across modules.
    pub(super) fn new(
        descriptor_pin: PinnedFileV1,
        descriptor: ExpandedRegistryTransferSourceV1,
        schedule: ExpandedRegistryTransferScheduleV1,
        transfer_state_sha256: String,
        scalars: RegistryTransferScalarsV1,
        completed_updates: u64,
    ) -> Self {
        Self {
            descriptor_pin,
            descriptor,
            schedule,
            transfer_state_sha256,
            scalars,
            completed_updates,
        }
    }
}

pub(super) fn check_pin(pin: &PinnedFileV1) -> Result<(), String> {
    ensure(
        pin.path.is_absolute()
            && pin.sha256.len() == 64
            && pin
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "registry transfer requires absolute paths and lowercase SHA256 pins",
    )
}

pub(super) fn batch_sha(batch: &[ExpandedEpisodeV1]) -> Result<String, String> {
    Ok(sha(&serde_json::to_vec(batch).map_err(err)?))
}

pub(super) fn read_schedule(
    pin: &PinnedFileV1,
) -> Result<ExpandedRegistryTransferScheduleV1, String> {
    let bytes = read_pinned_bytes(pin)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(err)?;
    if let Some(batches) = value.get("batches").and_then(Value::as_array) {
        for batch in batches.iter().filter_map(Value::as_array) {
            for episode in batch {
                ensure(episode.get("opponent").and_then(|v| v.get("kind")).is_none(),
                    "registry transfer schedule requires resolved opponent model sources; dynamic historical references are unsupported")?;
            }
        }
    }
    let schedule: ExpandedRegistryTransferScheduleV1 =
        serde_json::from_slice(&bytes).map_err(err)?;
    ensure(
        schedule.schema == SCHEDULE_SCHEMA
            && !schedule.batches.is_empty()
            && schedule.batches.len() <= 4096,
        "invalid resolved registry-transfer schedule",
    )?;
    ensure(
        schedule
            .initial_adam_step
            .checked_add(schedule.batches.len() as u64)
            .is_some_and(|step| step <= i32::MAX as u64),
        "resolved schedule exceeds the native Adam step domain",
    )?;
    let mut ids = BTreeSet::new();
    for batch in &schedule.batches {
        ensure(
            !batch.is_empty() && batch.len() <= 1024,
            "invalid resolved schedule batch size",
        )?;
        for episode in batch {
            ensure(
                !episode.id.is_empty() && episode.id.len() <= 128 && ids.insert(&episode.id),
                "empty or duplicate resolved schedule episode ID",
            )?;
            ensure(
                episode.learner_seat < 2
                    && episode.starting_player < 2
                    && (1..=100_000).contains(&episode.max_physical_decisions)
                    && (1..=1_000_000).contains(&episode.max_policy_steps),
                "invalid resolved schedule episode bounds",
            )?;
            episode.configurations()?;
            if let Some(opponent) = &episode.opponent {
                check_pin(&opponent.play_import)?;
                if let Some(checkpoint) = &opponent.checkpoint {
                    check_pin(checkpoint)?;
                }
                ensure(
                    opponent.feature_transfer.expected_feature_contract_digest
                        == FEATURE_CONTRACT_DIGEST_V3
                        && opponent.feature_transfer.expected_feature_encoding_digest
                            == FEATURE_ENCODING_DIGEST_V3,
                    "resolved opponent features differ",
                )?;
            }
        }
    }
    Ok(schedule)
}

impl TransferTrainingContextV1 {
    pub(super) fn validate_scalars(
        &self,
        learning_rate: f32,
        value_coefficient: f32,
    ) -> Result<(), String> {
        ensure(
            learning_rate.to_bits() == self.scalars.learning_rate_bits
                && value_coefficient.to_bits() == self.scalars.value_coefficient_bits,
            "registry-transfer continuation must preserve exact optimizer scalar settings",
        )
    }

    pub(super) fn validate_batch(&self, episodes: &[ExpandedEpisodeV1]) -> Result<(), String> {
        let expected = self
            .schedule
            .batches
            .get(self.completed_updates as usize)
            .ok_or("registry-transfer continuation schedule is exhausted")?;
        ensure(
            batch_sha(expected)? == batch_sha(episodes)?,
            "registry-transfer batch differs from the pinned schedule or cursor",
        )
    }

    fn metadata(&self, completed_updates: u64) -> Result<ExpandedRegistryContinuationV1, String> {
        ensure(
            completed_updates > 0 && completed_updates <= self.schedule.batches.len() as u64,
            "registry-transfer completed cursor is outside the pinned schedule",
        )?;
        Ok(ExpandedRegistryContinuationV1 {
            schema: CONTINUATION_SCHEMA.into(),
            source_descriptor_sha256: self.descriptor_pin.sha256.clone(),
            source_checkpoint_sha256: self.descriptor.source_checkpoint.sha256.clone(),
            source_registry_sha256: self.descriptor.source_registry.sha256.clone(),
            transfer_envelope_sha256: self.descriptor.transfer_envelope.sha256.clone(),
            continuation_schedule_sha256: self.descriptor.continuation_schedule.sha256.clone(),
            transfer_state_sha256: self.transfer_state_sha256.clone(),
            transfer_adam_step: self.scalars.adam_step,
            completed_updates,
            consumed_batch_sha256: batch_sha(
                &self.schedule.batches[(completed_updates - 1) as usize],
            )?,
            preserved_scalars: self.scalars.clone(),
        })
    }

    pub(super) fn after_update(
        &self,
        adam_step: u64,
    ) -> Result<ExpandedRegistryContinuationV1, String> {
        let next = self
            .completed_updates
            .checked_add(1)
            .ok_or("transfer cursor overflow")?;
        ensure(
            self.scalars.adam_step.checked_add(next) == Some(adam_step),
            "registry-transfer update must advance exactly one optimizer step",
        )?;
        self.metadata(next)
    }
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
    let verified = verify_registry_transfer_artifact_v1(
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
    let mut policy = FrozenPlayPolicyV1::from_registry_transfer_v1(
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
    let mut context = TransferTrainingContextV1 {
        descriptor_pin: source.play_import.clone(),
        descriptor,
        schedule,
        transfer_state_sha256: receipt.destination_state_sha256,
        scalars,
        completed_updates: 0,
    };
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
        ensure(
            provenance == &context.metadata(provenance.completed_updates)?,
            "registry successor transfer provenance, schedule, or cursor differs",
        )?;
        ensure(
            context
                .scalars
                .adam_step
                .checked_add(provenance.completed_updates)
                == Some(saved.adam_step)
                && saved.learning_rate_bits == context.scalars.learning_rate_bits
                && saved.value_coefficient_bits == context.scalars.value_coefficient_bits,
            "registry successor optimizer step or scalar settings differ",
        )?;
        state = restore_checkpoint_fields_v1(&saved, &mut policy, state.model_v1().clone())?;
        context.completed_updates = provenance.completed_updates;
    }
    Ok((policy, state, Some(context)))
}

#[cfg(test)]
mod tests;
