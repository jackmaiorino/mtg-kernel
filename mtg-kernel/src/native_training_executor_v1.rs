//! Public, non-serializing execution facade for the native trainer.
//!
//! This module is the narrow production seam used by the future strict CLI and
//! training store. It can bootstrap from the frozen common model snapshot,
//! execute one complete update at a time, export a verified full-state payload,
//! and resume from that payload. It intentionally defines no command-line
//! grammar, JSON schema, filesystem publication, recovery, or benchmark record;
//! those contracts remain separate layers.
//!
//! `NativeTrainingUpdateObservationV2::update_elapsed_ns` measures the native
//! trainer call from validation entry through its single in-memory commit. It
//! excludes checkpoint encoding, filesystem publication, source attestation,
//! and caller work. An authoritative end-to-end wrapper rate must time its
//! entire call and required persistence boundary independently.

use crate::common_model_snapshot_v1::CommonModelSnapshotRecordV1;
pub use crate::native_full_episode_trajectory_v1::NativeFullEpisodeTrajectoryReceiptV1 as NativeTrainingTrajectoryReceiptV1;
pub use crate::native_policy_train_step_v1::{
    NativeGaugeSubstepBoundV1 as NativeTrainingGaugeSubstepObservationV1,
    NativeScorerBiasGaugeRecordV1 as NativeTrainingScorerBiasGaugeObservationV1,
};
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_verified_v1, encode_native_train_state_payload_v1,
    NativeTrainStatePayloadDigestsV1, NativeTrainStatePayloadErrorV1,
};
use crate::native_trainer_v1::{
    validate_resumed_parts_v2, validate_update_config_v2, NativeTrainerBootstrapErrorV1,
    NativeTrainerErrorV1, NativeTrainerProgressV2, NativeTrainerStateV2,
    NativeTrainerUpdateConfigV2, NATIVE_TRAINER_CONTRACT_IDENTITY_V2,
};
pub use crate::native_trainer_v1::{
    NativeTrainerEpisodeEvidenceV1 as NativeTrainingEpisodeObservationV1,
    NativeTrainerPhysicalTermEvidenceV1 as NativeTrainingPhysicalTermObservationV1,
    NativeTrainerSelectedOutputEvidenceV1 as NativeTrainingSelectedOutputObservationV1,
    NativeTrainerUpdateEvidenceV2 as NativeTrainingUpdateObservationV2,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::time::Duration;

/// In-process update configuration, not a serialized or CLI contract.
///
/// `batch_episodes` is the immutable K for the executor and must be even in
/// `2..=10_000`. Counts are exact integers; `scheduler_timeout` is a wall-clock
/// duration; coefficients are raw IEEE-754 binary32 bits. Topology, caps, deck
/// identifiers, coefficient finiteness, and K are validated before snapshot
/// loading or state ownership. Strict callers must separately bind these values
/// into their immutable run record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTrainingExecutionConfigV1 {
    pub run_base_seed: u64,
    pub batch_episodes: u64,
    pub deck_ids: [String; 2],
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub worker_count: usize,
    pub sessions_per_worker: usize,
    pub broker_batch_target: usize,
    pub scheduler_timeout: Duration,
    pub measure_broker_service_time: bool,
    pub value_coefficient_bits: u32,
    pub learning_rate_bits: u32,
}

/// Non-serializing projection of the validated common-snapshot loader receipt.
/// The future store can bind these values without reopening private loader
/// internals or treating this Rust type as its JSON schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTrainingSnapshotReceiptV1 {
    pub schema: String,
    pub identity: String,
    pub snapshot_sha256: String,
    pub manifest_file_sha256: String,
    pub manifest_core_sha256: String,
    pub payload_sha256: String,
    pub payload_byte_count: u64,
    pub parameter_layout_sha256: String,
    pub named_parameter_stream_sha256: String,
    pub loaded_named_parameter_stream_sha256: String,
    pub parameter_tensor_count: u64,
    pub parameter_element_count: u64,
    pub model_config_fingerprint: String,
    pub model_architecture_version: String,
    pub feature_contract_digest: String,
    pub feature_encoding_digest: String,
    pub initializer_identity: String,
    pub base_seed: u64,
    pub model_init_seed: u64,
    pub trainer_schedule_version: String,
    pub python_reference_seed_version: String,
    pub schedule_goldens_sha256: String,
    pub authority_source_bundle_sha256: String,
    pub authority_runtime_identity: String,
    pub loader_identity: String,
    pub optimizer_identity: String,
    pub adam_step_initial: u64,
    pub moment_initialization: String,
    pub canonical_gauge_parameters: Vec<String>,
    /// Manifest JSON integer carrying a binary32 bit pattern. The loader uses
    /// u64 for strict JSON parsing; [`Self::scorer_bias_anchor_u32_v1`] exposes
    /// the checked in-memory width used by checkpoints.
    pub scorer_bias_anchor_f32_bits: u64,
    pub snapshot_load_completed_before_trial_start: bool,
    pub snapshot_load_timed: bool,
    pub rust_seeded_initializer_reproduced: bool,
    pub nonclaim: String,
}

impl From<CommonModelSnapshotRecordV1> for NativeTrainingSnapshotReceiptV1 {
    fn from(record: CommonModelSnapshotRecordV1) -> Self {
        Self {
            schema: record.schema,
            identity: record.identity,
            snapshot_sha256: record.snapshot_sha256,
            manifest_file_sha256: record.manifest_file_sha256,
            manifest_core_sha256: record.manifest_core_sha256,
            payload_sha256: record.payload_sha256,
            payload_byte_count: record.payload_byte_count,
            parameter_layout_sha256: record.parameter_layout_sha256,
            named_parameter_stream_sha256: record.named_parameter_stream_sha256,
            loaded_named_parameter_stream_sha256: record.loaded_named_parameter_stream_sha256,
            parameter_tensor_count: record.parameter_tensor_count,
            parameter_element_count: record.parameter_element_count,
            model_config_fingerprint: record.model_config_fingerprint,
            model_architecture_version: record.model_architecture_version,
            feature_contract_digest: record.feature_contract_digest,
            feature_encoding_digest: record.feature_encoding_digest,
            initializer_identity: record.initializer_identity,
            base_seed: record.base_seed,
            model_init_seed: record.model_init_seed,
            trainer_schedule_version: record.trainer_schedule_version,
            python_reference_seed_version: record.python_reference_seed_version,
            schedule_goldens_sha256: record.schedule_goldens_sha256,
            authority_source_bundle_sha256: record.authority_source_bundle_sha256,
            authority_runtime_identity: record.authority_runtime_identity,
            loader_identity: record.loader_identity,
            optimizer_identity: record.optimizer_identity,
            adam_step_initial: record.adam_step_initial,
            moment_initialization: record.moment_initialization,
            canonical_gauge_parameters: record.canonical_gauge_parameters,
            scorer_bias_anchor_f32_bits: record.scorer_bias_anchor_f32_bits,
            snapshot_load_completed_before_trial_start: record
                .snapshot_load_completed_before_trial_start,
            snapshot_load_timed: record.snapshot_load_timed,
            rust_seeded_initializer_reproduced: record.rust_seeded_initializer_reproduced,
            nonclaim: record.nonclaim,
        }
    }
}

impl NativeTrainingSnapshotReceiptV1 {
    pub fn scorer_bias_anchor_u32_v1(&self) -> Option<u32> {
        u32::try_from(self.scorer_bias_anchor_f32_bits).ok()
    }
}

impl NativeTrainingExecutionConfigV1 {
    fn trainer_update_config_v2(&self) -> NativeTrainerUpdateConfigV2 {
        NativeTrainerUpdateConfigV2 {
            deck_ids: self.deck_ids.clone(),
            batch_episodes: self.batch_episodes,
            max_physical_decisions: self.max_physical_decisions,
            max_policy_steps: self.max_policy_steps,
            worker_count: self.worker_count,
            sessions_per_worker: self.sessions_per_worker,
            broker_batch_target: self.broker_batch_target,
            scheduler_timeout: self.scheduler_timeout,
            measure_broker_service_time: self.measure_broker_service_time,
            value_coefficient_bits: self.value_coefficient_bits,
            learning_rate_bits: self.learning_rate_bits,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainingProgressV1 {
    pub next_episode_index: u64,
    pub successful_update_count: u64,
    pub completed_episode_count: u64,
    pub learner_physical_decision_count: u64,
    pub learner_policy_step_count: u64,
}

impl From<NativeTrainerProgressV2> for NativeTrainingProgressV1 {
    fn from(progress: NativeTrainerProgressV2) -> Self {
        Self {
            next_episode_index: progress.next_episode_index,
            successful_update_count: progress.successful_update_count,
            completed_episode_count: progress.completed_episode_count,
            learner_physical_decision_count: progress.learner_physical_decision_count,
            learner_policy_step_count: progress.learner_policy_step_count,
        }
    }
}

impl From<NativeTrainingProgressV1> for NativeTrainerProgressV2 {
    fn from(progress: NativeTrainingProgressV1) -> Self {
        Self {
            next_episode_index: progress.next_episode_index,
            successful_update_count: progress.successful_update_count,
            completed_episode_count: progress.completed_episode_count,
            learner_physical_decision_count: progress.learner_physical_decision_count,
            learner_policy_step_count: progress.learner_policy_step_count,
        }
    }
}

/// Complete, verified checkpoint material held in memory. Persistence layers
/// must add their own immutable metadata, atomic-publication, and recovery
/// contracts; this candidate is only the payload/progress handoff.
#[derive(Clone, PartialEq, Eq)]
pub struct NativeTrainingCheckpointCandidateV1 {
    base_seed: u64,
    batch_episodes: u64,
    progress: NativeTrainingProgressV1,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    payload: Vec<u8>,
    digests: NativeTrainStatePayloadDigestsV1,
}

impl std::fmt::Debug for NativeTrainingCheckpointCandidateV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTrainingCheckpointCandidateV1")
            .field("base_seed", &self.base_seed)
            .field("batch_episodes", &self.batch_episodes)
            .field("progress", &self.progress)
            .field("adam_step", &self.adam_step)
            .field("scorer_bias_anchor_bits", &self.scorer_bias_anchor_bits)
            .field("payload_byte_count", &self.payload.len())
            .field(
                "payload_sha256",
                &digest_hex_v1(self.digests.payload_sha256),
            )
            .finish_non_exhaustive()
    }
}

/// Raw digest values used by the in-process checkpoint handoff. This is not a
/// text encoding or serialized record contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainingCheckpointDigestsV1 {
    pub payload_sha256: [u8; 32],
    pub parameters_sha256: [u8; 32],
    pub first_moments_sha256: [u8; 32],
    pub second_moments_sha256: [u8; 32],
    pub model_parameter_sha256: [u8; 32],
    pub native_state_sha256: [u8; 32],
}

impl From<NativeTrainStatePayloadDigestsV1> for NativeTrainingCheckpointDigestsV1 {
    fn from(digests: NativeTrainStatePayloadDigestsV1) -> Self {
        Self {
            payload_sha256: digests.payload_sha256,
            parameters_sha256: digests.parameters_sha256,
            first_moments_sha256: digests.first_moments_sha256,
            second_moments_sha256: digests.second_moments_sha256,
            model_parameter_sha256: digests.model_parameter_sha256,
            native_state_sha256: digests.native_state_sha256,
        }
    }
}

impl From<NativeTrainingCheckpointDigestsV1> for NativeTrainStatePayloadDigestsV1 {
    fn from(digests: NativeTrainingCheckpointDigestsV1) -> Self {
        Self {
            payload_sha256: digests.payload_sha256,
            parameters_sha256: digests.parameters_sha256,
            first_moments_sha256: digests.first_moments_sha256,
            second_moments_sha256: digests.second_moments_sha256,
            model_parameter_sha256: digests.model_parameter_sha256,
            native_state_sha256: digests.native_state_sha256,
        }
    }
}

/// Scalar checkpoint metadata supplied by a validated persistence layer.
/// Authenticity and ancestry remain that layer's responsibility; construction
/// below independently revalidates payload and train-state integrity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainingCheckpointMetadataV1 {
    pub base_seed: u64,
    pub batch_episodes: u64,
    pub progress: NativeTrainingProgressV1,
    pub adam_step: u64,
    pub scorer_bias_anchor_bits: u32,
}

impl NativeTrainingCheckpointCandidateV1 {
    pub fn import_verified_v1(
        metadata: NativeTrainingCheckpointMetadataV1,
        payload: &[u8],
        digests: NativeTrainingCheckpointDigestsV1,
    ) -> Result<Self, NativeTrainingExecutorErrorV1> {
        let internal_digests = digests.into();
        let decoded = decode_native_train_state_payload_verified_v1(
            payload,
            metadata.adam_step,
            metadata.scorer_bias_anchor_bits,
            &internal_digests,
        )
        .map_err(payload_executor_error_v1)?;
        let train_state = train_state_from_snapshot_v1(&decoded.snapshot)?;
        validate_resumed_parts_v2(
            metadata.base_seed,
            metadata.batch_episodes,
            &train_state,
            metadata.progress.into(),
        )
        .map_err(trainer_executor_error_v1)?;
        Ok(Self {
            base_seed: metadata.base_seed,
            batch_episodes: metadata.batch_episodes,
            progress: metadata.progress,
            adam_step: metadata.adam_step,
            scorer_bias_anchor_bits: metadata.scorer_bias_anchor_bits,
            // Ownership is acquired only after payload, train-state, progress,
            // K, Adam, and next-schedule validation all succeed.
            payload: payload.to_vec(),
            digests: internal_digests,
        })
    }

    pub fn base_seed(&self) -> u64 {
        self.base_seed
    }

    pub fn batch_episodes(&self) -> u64 {
        self.batch_episodes
    }

    pub fn progress(&self) -> NativeTrainingProgressV1 {
        self.progress
    }

    pub fn adam_step(&self) -> u64 {
        self.adam_step
    }

    pub fn scorer_bias_anchor_bits(&self) -> u32 {
        self.scorer_bias_anchor_bits
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn payload_byte_count(&self) -> usize {
        self.payload.len()
    }

    pub fn metadata(&self) -> NativeTrainingCheckpointMetadataV1 {
        NativeTrainingCheckpointMetadataV1 {
            base_seed: self.base_seed,
            batch_episodes: self.batch_episodes,
            progress: self.progress,
            adam_step: self.adam_step,
            scorer_bias_anchor_bits: self.scorer_bias_anchor_bits,
        }
    }

    pub fn digests(&self) -> NativeTrainingCheckpointDigestsV1 {
        self.digests.into()
    }

    pub fn payload_sha256_hex(&self) -> String {
        digest_hex_v1(self.digests.payload_sha256)
    }

    pub fn parameters_sha256_hex(&self) -> String {
        digest_hex_v1(self.digests.parameters_sha256)
    }

    pub fn first_moments_sha256_hex(&self) -> String {
        digest_hex_v1(self.digests.first_moments_sha256)
    }

    pub fn second_moments_sha256_hex(&self) -> String {
        digest_hex_v1(self.digests.second_moments_sha256)
    }

    pub fn model_parameter_sha256_hex(&self) -> String {
        digest_hex_v1(self.digests.model_parameter_sha256)
    }

    pub fn native_state_sha256_hex(&self) -> String {
        digest_hex_v1(self.digests.native_state_sha256)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTrainingExecutorErrorKindV1 {
    Configuration,
    Snapshot,
    CheckpointBinding,
    Resume,
    Schedule,
    Rollout,
    Scorer,
    Observer,
    Grouping,
    Training,
    Payload,
    Model,
    TrainState,
    Counter,
}

#[derive(Debug)]
struct NativeTrainingExecutorDiagnosticV1 {
    _message: String,
}

pub struct NativeTrainingExecutorErrorV1 {
    kind: NativeTrainingExecutorErrorKindV1,
    code: &'static str,
    _diagnostic: Option<NativeTrainingExecutorDiagnosticV1>,
}

impl NativeTrainingExecutorErrorV1 {
    /// Stable coarse classification for the V1 executor facade. It is not a
    /// process exit code or serialized record field.
    pub fn kind(&self) -> NativeTrainingExecutorErrorKindV1 {
        self.kind
    }

    /// Stable ASCII diagnostic code within the V1 executor facade. Callers may
    /// branch on this value; human-readable internal diagnostics remain private.
    pub fn code(&self) -> &'static str {
        self.code
    }

    fn redacted(kind: NativeTrainingExecutorErrorKindV1, code: &'static str) -> Self {
        Self {
            kind,
            code,
            _diagnostic: None,
        }
    }

    fn with_diagnostic(
        kind: NativeTrainingExecutorErrorKindV1,
        code: &'static str,
        diagnostic: impl std::fmt::Debug,
    ) -> Self {
        Self {
            kind,
            code,
            _diagnostic: Some(NativeTrainingExecutorDiagnosticV1 {
                _message: format!("{diagnostic:?}"),
            }),
        }
    }
}

impl std::fmt::Debug for NativeTrainingExecutorErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTrainingExecutorErrorV1")
            .field("kind", &self.kind)
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}

impl Display for NativeTrainingExecutorErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native training executor rejected input ({})",
            self.code
        )
    }
}

impl Error for NativeTrainingExecutorErrorV1 {}

fn payload_executor_error_v1(
    error: NativeTrainStatePayloadErrorV1,
) -> NativeTrainingExecutorErrorV1 {
    let code = match &error {
        NativeTrainStatePayloadErrorV1::ExactLength { .. } => "payload_exact_length",
        NativeTrainStatePayloadErrorV1::LayoutInvariant(_) => "payload_layout_invariant",
        NativeTrainStatePayloadErrorV1::TrainState(_) => "payload_train_state_invalid",
        NativeTrainStatePayloadErrorV1::DigestMismatch(_) => "payload_digest_mismatch",
    };
    NativeTrainingExecutorErrorV1::with_diagnostic(
        NativeTrainingExecutorErrorKindV1::Payload,
        code,
        error,
    )
}

fn trainer_executor_error_v1(error: NativeTrainerErrorV1) -> NativeTrainingExecutorErrorV1 {
    let (kind, code) = match &error {
        NativeTrainerErrorV1::Schedule(_) => (
            NativeTrainingExecutorErrorKindV1::Schedule,
            "trainer_schedule_rejected",
        ),
        NativeTrainerErrorV1::InvalidUpdateConfig(_) => (
            NativeTrainingExecutorErrorKindV1::Configuration,
            "trainer_update_config_invalid",
        ),
        NativeTrainerErrorV1::ResumeInvariant(_)
        | NativeTrainerErrorV1::ProgressOutsideU63 { .. } => (
            NativeTrainingExecutorErrorKindV1::Resume,
            "trainer_resume_invariant",
        ),
        NativeTrainerErrorV1::ObserverConstruction(_)
        | NativeTrainerErrorV1::ObserverFailed { .. }
        | NativeTrainerErrorV1::ObserverPanicked { .. } => (
            NativeTrainingExecutorErrorKindV1::Observer,
            "trainer_observer_rejected",
        ),
        NativeTrainerErrorV1::Scorer(_) => (
            NativeTrainingExecutorErrorKindV1::Scorer,
            "trainer_scorer_rejected",
        ),
        NativeTrainerErrorV1::Rollout(_) => (
            NativeTrainingExecutorErrorKindV1::Rollout,
            "trainer_rollout_rejected",
        ),
        NativeTrainerErrorV1::GroupingInvariant(_)
        | NativeTrainerErrorV1::TerminalReturnRange { .. }
        | NativeTrainerErrorV1::RecomputedOutputMismatch { .. } => (
            NativeTrainingExecutorErrorKindV1::Grouping,
            "trainer_grouping_invariant",
        ),
        NativeTrainerErrorV1::Train(_) => (
            NativeTrainingExecutorErrorKindV1::Training,
            "trainer_train_step_rejected",
        ),
        NativeTrainerErrorV1::CounterOverflow => (
            NativeTrainingExecutorErrorKindV1::Counter,
            "trainer_counter_overflow",
        ),
    };
    NativeTrainingExecutorErrorV1::with_diagnostic(kind, code, error)
}

fn bootstrap_executor_error_v1(
    error: NativeTrainerBootstrapErrorV1,
) -> NativeTrainingExecutorErrorV1 {
    let (kind, code) = match &error {
        NativeTrainerBootstrapErrorV1::PlaceholderModel(_) => (
            NativeTrainingExecutorErrorKindV1::Model,
            "snapshot_placeholder_model_rejected",
        ),
        NativeTrainerBootstrapErrorV1::OptimizerBootstrap(_) => (
            NativeTrainingExecutorErrorKindV1::TrainState,
            "snapshot_optimizer_bootstrap_rejected",
        ),
        NativeTrainerBootstrapErrorV1::Trainer(_) => (
            NativeTrainingExecutorErrorKindV1::Snapshot,
            "snapshot_trainer_bootstrap_rejected",
        ),
        NativeTrainerBootstrapErrorV1::Snapshot(_) => (
            NativeTrainingExecutorErrorKindV1::Snapshot,
            "common_snapshot_rejected",
        ),
        NativeTrainerBootstrapErrorV1::RunSeedMatchesSnapshotAuthority { .. } => (
            NativeTrainingExecutorErrorKindV1::Snapshot,
            "run_seed_matches_snapshot_authority",
        ),
    };
    NativeTrainingExecutorErrorV1::with_diagnostic(kind, code, error)
}

/// Stateful one-update-at-a-time trainer executor. A successful update is
/// transactional inside the trainer; callers decide when and how to publish a
/// checkpoint candidate.
pub struct NativeTrainingExecutorV1 {
    config: NativeTrainingExecutionConfigV1,
    update_config: NativeTrainerUpdateConfigV2,
    trainer: NativeTrainerStateV2,
    snapshot_receipt: Option<NativeTrainingSnapshotReceiptV1>,
}

impl std::fmt::Debug for NativeTrainingExecutorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTrainingExecutorV1")
            .field("config", &self.config)
            .field("progress", &self.progress())
            .field("has_snapshot_receipt", &self.snapshot_receipt.is_some())
            .finish_non_exhaustive()
    }
}

impl NativeTrainingExecutorV1 {
    /// Validates the execution config and frozen snapshot before constructing
    /// an executor. The validated snapshot receipt is retained and returned by
    /// [`Self::snapshot_receipt`].
    ///
    /// # Errors
    ///
    /// Returns a classified, redacted error without a live executor when config,
    /// snapshot, model, optimizer, schedule, or seed-separation checks fail.
    pub fn from_common_model_snapshot_v1(
        config: NativeTrainingExecutionConfigV1,
        manifest_path: &Path,
        payload_path: &Path,
    ) -> Result<Self, NativeTrainingExecutorErrorV1> {
        let update_config = validated_update_config_v1(&config)?;
        let (trainer, snapshot_receipt) = NativeTrainerStateV2::from_common_model_snapshot_v2(
            config.run_base_seed,
            config.batch_episodes,
            manifest_path,
            payload_path,
        )
        .map_err(bootstrap_executor_error_v1)?;
        Ok(Self {
            config,
            update_config,
            trainer,
            snapshot_receipt: Some(snapshot_receipt.into()),
        })
    }

    /// Reconstructs an executor from an immutable verified candidate.
    ///
    /// Resumed executors return `None` from [`Self::snapshot_receipt`]; the
    /// persistence layer remains responsible for retaining and validating the
    /// original run-level snapshot receipt.
    ///
    /// # Errors
    ///
    /// Rejects config/K/base-seed mismatch, payload drift, invalid train state,
    /// incoherent progress/Adam counters, or an invalid next schedule before
    /// returning an executor.
    pub fn from_checkpoint_candidate_v1(
        config: NativeTrainingExecutionConfigV1,
        checkpoint: &NativeTrainingCheckpointCandidateV1,
    ) -> Result<Self, NativeTrainingExecutorErrorV1> {
        let update_config = validated_update_config_v1(&config)?;
        if checkpoint.base_seed != config.run_base_seed {
            return Err(NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "checkpoint_base_seed_mismatch",
            ));
        }
        if checkpoint.batch_episodes != config.batch_episodes {
            return Err(NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "checkpoint_batch_episodes_mismatch",
            ));
        }

        let decoded = decode_native_train_state_payload_verified_v1(
            &checkpoint.payload,
            checkpoint.adam_step,
            checkpoint.scorer_bias_anchor_bits,
            &checkpoint.digests,
        )
        .map_err(payload_executor_error_v1)?;
        let train_state = train_state_from_snapshot_v1(&decoded.snapshot)?;
        let trainer = NativeTrainerStateV2::from_resumed_parts_v2(
            config.run_base_seed,
            config.batch_episodes,
            &train_state,
            checkpoint.progress.into(),
        )
        .map_err(trainer_executor_error_v1)?;
        Ok(Self {
            config,
            update_config,
            trainer,
            snapshot_receipt: None,
        })
    }

    pub fn config(&self) -> &NativeTrainingExecutionConfigV1 {
        &self.config
    }

    /// Returns the loader receipt only for a fresh snapshot bootstrap. Resumed
    /// executors intentionally return `None`.
    pub fn snapshot_receipt(&self) -> Option<&NativeTrainingSnapshotReceiptV1> {
        self.snapshot_receipt.as_ref()
    }

    pub fn progress(&self) -> NativeTrainingProgressV1 {
        self.trainer.progress_v2().into()
    }

    /// Runs exactly one K-episode transactional update and moves out the complete
    /// owned V2 evidence without post-timer evidence copying.
    ///
    /// # Errors
    ///
    /// Returns a classified error with the live trainer unchanged for every
    /// recoverable schedule, rollout, observer, scorer, grouping, or train-step
    /// rejection.
    pub fn run_update_v2(
        &mut self,
    ) -> Result<NativeTrainingUpdateObservationV2, NativeTrainingExecutorErrorV1> {
        self.trainer
            .run_even_batch_update_v2(&self.update_config)
            .map_err(trainer_executor_error_v1)
    }

    /// Exports the current checkpoint only when `observation` is the exact
    /// successful update that produced the executor's current live state.
    ///
    /// The caller retains the observation on every error and may retry this
    /// method after a persistence-layer failure without running another update.
    /// A stale, mutated, or topology-mismatched observation fails before the
    /// 14.7 MiB payload is encoded.
    ///
    /// # Errors
    ///
    /// Rejects any observation/current-state mismatch or any checkpoint export
    /// failure without mutating the executor or taking observation ownership.
    pub fn checkpoint_candidate_for_observation_v2(
        &self,
        observation: &NativeTrainingUpdateObservationV2,
    ) -> Result<NativeTrainingCheckpointCandidateV1, NativeTrainingExecutorErrorV1> {
        self.validate_current_observation_v2(observation)?;
        let checkpoint = self.checkpoint_candidate_v1()?;
        if checkpoint.adam_step != observation.adam_step_after
            || checkpoint.progress.successful_update_count != observation.adam_step_after
            || checkpoint.progress.next_episode_index
                != observation
                    .first_episode_index
                    .checked_add(observation.episode_count)
                    .ok_or_else(checkpoint_observation_mismatch_v1)?
            || checkpoint.model_parameter_sha256_hex() != observation.model_digest_after
        {
            return Err(checkpoint_observation_mismatch_v1());
        }
        Ok(checkpoint)
    }

    /// Revalidates cross-component resume coherence, then exports a complete
    /// headerless train-state payload and raw digests.
    ///
    /// # Errors
    ///
    /// Returns a classified error without a candidate when live state, progress,
    /// K, Adam, next schedule, payload layout, or digest construction fails.
    pub fn checkpoint_candidate_v1(
        &self,
    ) -> Result<NativeTrainingCheckpointCandidateV1, NativeTrainingExecutorErrorV1> {
        validate_resumed_parts_v2(
            self.config.run_base_seed,
            self.config.batch_episodes,
            self.trainer.train_state_v1(),
            self.trainer.progress_v2(),
        )
        .map_err(trainer_executor_error_v1)?;
        let snapshot = self
            .trainer
            .train_state_v1()
            .snapshot_v1()
            .map_err(|error| {
                NativeTrainingExecutorErrorV1::with_diagnostic(
                    NativeTrainingExecutorErrorKindV1::TrainState,
                    "live_train_state_invalid",
                    error,
                )
            })?;
        let encoded =
            encode_native_train_state_payload_v1(&snapshot).map_err(payload_executor_error_v1)?;
        Ok(NativeTrainingCheckpointCandidateV1 {
            base_seed: self.config.run_base_seed,
            batch_episodes: self.config.batch_episodes,
            progress: self.progress(),
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            payload: encoded.bytes,
            digests: encoded.digests,
        })
    }

    fn validate_current_observation_v2(
        &self,
        observation: &NativeTrainingUpdateObservationV2,
    ) -> Result<(), NativeTrainingExecutorErrorV1> {
        let progress = self.progress();
        let expected_first_episode_index = observation
            .adam_step_before
            .checked_mul(self.config.batch_episodes)
            .ok_or_else(checkpoint_observation_mismatch_v1)?;
        let expected_end_episode_index = observation
            .first_episode_index
            .checked_add(observation.episode_count)
            .ok_or_else(checkpoint_observation_mismatch_v1)?;
        let expected_logical_actor_count = self
            .config
            .worker_count
            .checked_mul(self.config.sessions_per_worker)
            .ok_or_else(checkpoint_observation_mismatch_v1)?;
        let expected_episode_len = usize::try_from(self.config.batch_episodes)
            .map_err(|_| checkpoint_observation_mismatch_v1())?;

        if observation.trainer_contract_identity != NATIVE_TRAINER_CONTRACT_IDENTITY_V2
            || observation.episode_count != self.config.batch_episodes
            || observation.first_episode_index != expected_first_episode_index
            || expected_end_episode_index != progress.next_episode_index
            || progress.completed_episode_count != expected_end_episode_index
            || observation.adam_step_before.checked_add(1) != Some(observation.adam_step_after)
            || observation.adam_step_after != progress.successful_update_count
            || observation.worker_count != self.config.worker_count
            || observation.sessions_per_worker != self.config.sessions_per_worker
            || observation.logical_actor_count != expected_logical_actor_count
            || observation.broker_batch_target != self.config.broker_batch_target
            || observation.episodes.len() != expected_episode_len
            || observation.selected_outputs.len()
                != usize::try_from(observation.learner_policy_step_count)
                    .map_err(|_| checkpoint_observation_mismatch_v1())?
            || observation.physical_terms.len()
                != usize::try_from(observation.learner_group_count)
                    .map_err(|_| checkpoint_observation_mismatch_v1())?
            || self
                .trainer
                .train_state_v1()
                .model_v1()
                .parameter_manifest_sha256_v1()
                != observation.model_digest_after
        {
            return Err(checkpoint_observation_mismatch_v1());
        }

        let mut physical_decision_count = 0u64;
        let mut policy_step_count = 0u64;
        let mut learner_group_count = 0u64;
        let mut learner_policy_step_count = 0u64;
        for (offset, episode) in observation.episodes.iter().enumerate() {
            let expected_episode_index = observation
                .first_episode_index
                .checked_add(
                    u64::try_from(offset).map_err(|_| checkpoint_observation_mismatch_v1())?,
                )
                .ok_or_else(checkpoint_observation_mismatch_v1)?;
            let receipt = &episode.full_trajectory_receipt;
            if episode.episode_index != expected_episode_index
                || receipt.episode_index != expected_episode_index
                || receipt.learner_seat != episode.learner_seat
                || receipt.learner_policy_step_count != episode.learner_policy_step_count
                || receipt.learner_physical_decision_count != episode.learner_group_count
            {
                return Err(checkpoint_observation_mismatch_v1());
            }
            physical_decision_count = physical_decision_count
                .checked_add(receipt.physical_decision_count)
                .ok_or_else(checkpoint_observation_mismatch_v1)?;
            policy_step_count = policy_step_count
                .checked_add(receipt.policy_step_count)
                .ok_or_else(checkpoint_observation_mismatch_v1)?;
            learner_group_count = learner_group_count
                .checked_add(episode.learner_group_count)
                .ok_or_else(checkpoint_observation_mismatch_v1)?;
            learner_policy_step_count = learner_policy_step_count
                .checked_add(episode.learner_policy_step_count)
                .ok_or_else(checkpoint_observation_mismatch_v1)?;
        }
        if physical_decision_count != observation.physical_decision_count
            || policy_step_count != observation.policy_step_count
            || learner_group_count != observation.learner_group_count
            || learner_policy_step_count != observation.learner_policy_step_count
        {
            return Err(checkpoint_observation_mismatch_v1());
        }
        Ok(())
    }
}

fn checkpoint_observation_mismatch_v1() -> NativeTrainingExecutorErrorV1 {
    NativeTrainingExecutorErrorV1::redacted(
        NativeTrainingExecutorErrorKindV1::CheckpointBinding,
        "checkpoint_observation_mismatch",
    )
}

fn validated_update_config_v1(
    config: &NativeTrainingExecutionConfigV1,
) -> Result<NativeTrainerUpdateConfigV2, NativeTrainingExecutorErrorV1> {
    let update_config = config.trainer_update_config_v2();
    validate_update_config_v2(&update_config).map_err(trainer_executor_error_v1)?;
    Ok(update_config)
}

fn train_state_from_snapshot_v1(
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> Result<NativePolicyValueTrainStateV1, NativeTrainingExecutorErrorV1> {
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|error| {
                NativeTrainingExecutorErrorV1::with_diagnostic(
                    NativeTrainingExecutorErrorKindV1::Model,
                    "model_template_rejected",
                    error,
                )
            })?;
    model
        .replace_parameter_snapshot_v1(&snapshot.parameters)
        .map_err(|error| {
            NativeTrainingExecutorErrorV1::with_diagnostic(
                NativeTrainingExecutorErrorKindV1::Model,
                "model_snapshot_rejected",
                error,
            )
        })?;
    NativePolicyValueTrainStateV1::from_snapshot_v1(model, snapshot).map_err(|error| {
        NativeTrainingExecutorErrorV1::with_diagnostic(
            NativeTrainingExecutorErrorKindV1::TrainState,
            "train_state_snapshot_rejected",
            error,
        )
    })
}

fn digest_hex_v1(digest: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;

    fn burn_config_v1(batch_episodes: u64) -> NativeTrainingExecutionConfigV1 {
        NativeTrainingExecutionConfigV1 {
            run_base_seed: 71_501,
            batch_episodes,
            deck_ids: ["Burn".to_owned(), "Burn".to_owned()],
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count: 1,
            sessions_per_worker: 1,
            broker_batch_target: 1,
            scheduler_timeout: Duration::from_secs(600),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5f32.to_bits(),
            learning_rate_bits: 0.001f32.to_bits(),
        }
    }

    fn without_timing_v1(
        mut observation: NativeTrainingUpdateObservationV2,
    ) -> NativeTrainingUpdateObservationV2 {
        observation.update_elapsed_ns = 0;
        observation.rollout_metrics.total_elapsed_ns = 0;
        observation
    }

    #[test]
    fn fresh_update_checkpoint_resume_and_continuation_are_exact() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let mut uninterrupted = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            config.clone(),
            &manifest,
            &payload,
        )
        .unwrap();
        let snapshot_receipt = uninterrupted.snapshot_receipt().unwrap();
        assert_eq!(snapshot_receipt.snapshot_sha256.len(), 64);
        assert_eq!(snapshot_receipt.payload_sha256.len(), 64);
        assert!(snapshot_receipt.snapshot_load_completed_before_trial_start);
        assert!(!snapshot_receipt.snapshot_load_timed);

        let update_zero = uninterrupted.checkpoint_candidate_v1().unwrap();
        assert_eq!(
            update_zero.payload_byte_count(),
            crate::native_train_state_payload_v1::NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1
        );
        assert_eq!(update_zero.progress().successful_update_count, 0);
        assert_eq!(update_zero.adam_step(), 0);

        let first = uninterrupted.run_update_v2().unwrap();
        assert_eq!(first.first_episode_index, 0);
        assert_eq!(first.episode_count, 2);
        assert_eq!(first.adam_step_before, 0);
        assert_eq!(first.adam_step_after, 1);
        assert!(first.physical_decision_count > 0);
        assert!(first.policy_step_count >= first.physical_decision_count);
        assert_ne!(first.model_digest_before, first.model_digest_after);
        assert_eq!(first.episodes.len(), 2);
        assert_eq!(
            first
                .episodes
                .iter()
                .map(|episode| episode.full_trajectory_receipt.physical_decision_count)
                .sum::<u64>(),
            first.physical_decision_count
        );
        assert!(first
            .episodes
            .iter()
            .all(|episode| episode.full_trajectory_receipt.trajectory_sha256 != [0; 32]));
        assert!(!first.selected_outputs.is_empty());
        assert!(!first.physical_terms.is_empty());
        assert_eq!(first.scorer_bias_gauge.parameter_name, "scorer.2.bias");
        assert_eq!(
            first.rollout_metrics.scored_decision_count,
            first.scorer_accepted_decision_count
        );

        let checkpoint = uninterrupted.checkpoint_candidate_v1().unwrap();
        assert_eq!(checkpoint.progress().successful_update_count, 1);
        assert_eq!(checkpoint.progress().completed_episode_count, 2);
        assert_eq!(checkpoint.adam_step(), 1);
        assert_eq!(checkpoint.payload_sha256_hex().len(), 64);
        assert_eq!(checkpoint.native_state_sha256_hex().len(), 64);

        let imported = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            checkpoint.metadata(),
            checkpoint.payload(),
            checkpoint.digests(),
        )
        .unwrap();
        assert_eq!(imported, checkpoint);
        let mut resumed =
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(config, &imported).unwrap();
        assert!(resumed.snapshot_receipt().is_none());
        assert_eq!(resumed.checkpoint_candidate_v1().unwrap(), checkpoint);

        let uninterrupted_next = uninterrupted.run_update_v2().unwrap();
        let resumed_next = resumed.run_update_v2().unwrap();
        assert_eq!(
            without_timing_v1(resumed_next),
            without_timing_v1(uninterrupted_next)
        );
        assert_eq!(
            resumed.checkpoint_candidate_v1().unwrap(),
            uninterrupted.checkpoint_candidate_v1().unwrap()
        );
    }

    #[test]
    fn k4_checkpoint_resume_continuation_is_exact() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(4);
        let mut uninterrupted = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            config.clone(),
            &manifest,
            &payload,
        )
        .unwrap();

        let first = uninterrupted.run_update_v2().unwrap();
        assert_eq!(first.first_episode_index, 0);
        assert_eq!(first.episode_count, 4);
        assert_eq!(first.episodes.len(), 4);
        assert_eq!(first.adam_step_after, 1);

        let checkpoint = uninterrupted.checkpoint_candidate_v1().unwrap();
        assert_eq!(checkpoint.batch_episodes(), 4);
        assert_eq!(checkpoint.progress().next_episode_index, 4);
        assert_eq!(checkpoint.progress().completed_episode_count, 4);

        let imported = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            checkpoint.metadata(),
            checkpoint.payload(),
            checkpoint.digests(),
        )
        .unwrap();
        let mut resumed =
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(config, &imported).unwrap();

        let uninterrupted_next = uninterrupted.run_update_v2().unwrap();
        let resumed_next = resumed.run_update_v2().unwrap();
        assert_eq!(uninterrupted_next.first_episode_index, 4);
        assert_eq!(uninterrupted_next.episode_count, 4);
        assert_eq!(
            without_timing_v1(resumed_next),
            without_timing_v1(uninterrupted_next)
        );
        assert_eq!(
            resumed.checkpoint_candidate_v1().unwrap(),
            uninterrupted.checkpoint_candidate_v1().unwrap()
        );
    }

    #[test]
    fn observation_bound_checkpoint_is_retryable_and_rejects_stale_or_mutated_evidence() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let mut executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();

        let first_observation = executor.run_update_v2().unwrap();
        let first_candidate = executor
            .checkpoint_candidate_for_observation_v2(&first_observation)
            .unwrap();

        // A caller may fail after receiving the candidate but before durable
        // publication. Repeating the export must not advance training or alter
        // any byte of the candidate.
        let retry_candidate = executor
            .checkpoint_candidate_for_observation_v2(&first_observation)
            .unwrap();
        assert_eq!(retry_candidate, first_candidate);
        assert_eq!(executor.progress().successful_update_count, 1);

        let mut mutated_digest = first_observation.clone();
        mutated_digest.model_digest_after.replace_range(0..1, "0");
        if mutated_digest.model_digest_after == first_observation.model_digest_after {
            mutated_digest.model_digest_after.replace_range(0..1, "1");
        }
        let error = executor
            .checkpoint_candidate_for_observation_v2(&mutated_digest)
            .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(error.code(), "checkpoint_observation_mismatch");

        let mut mutated_episode = first_observation.clone();
        mutated_episode.episodes[0].learner_group_count += 1;
        let error = executor
            .checkpoint_candidate_for_observation_v2(&mutated_episode)
            .unwrap_err();
        assert_eq!(error.code(), "checkpoint_observation_mismatch");
        assert_eq!(executor.checkpoint_candidate_v1().unwrap(), first_candidate);

        let second_observation = executor.run_update_v2().unwrap();
        let error = executor
            .checkpoint_candidate_for_observation_v2(&first_observation)
            .unwrap_err();
        assert_eq!(error.code(), "checkpoint_observation_mismatch");
        let second_candidate = executor
            .checkpoint_candidate_for_observation_v2(&second_observation)
            .unwrap();
        assert_eq!(second_candidate.progress().successful_update_count, 2);
        assert_ne!(second_candidate, first_candidate);
    }

    #[test]
    fn checkpoint_binding_mismatch_rejects_before_decode() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            config.clone(),
            &manifest,
            &payload,
        )
        .unwrap();
        let checkpoint = executor.checkpoint_candidate_v1().unwrap();

        let mut wrong_seed = config.clone();
        wrong_seed.run_base_seed += 1;
        let error = NativeTrainingExecutorV1::from_checkpoint_candidate_v1(wrong_seed, &checkpoint)
            .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(error.code(), "checkpoint_base_seed_mismatch");

        let mut wrong_batch = config;
        wrong_batch.batch_episodes = 4;
        let error =
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(wrong_batch, &checkpoint)
                .unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(error.code(), "checkpoint_batch_episodes_mismatch");
    }

    #[test]
    fn corrupted_payload_and_progress_reject_without_consuming_candidate() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            config.clone(),
            &manifest,
            &payload,
        )
        .unwrap();
        let checkpoint = executor.checkpoint_candidate_v1().unwrap();

        let mut corrupted_payload = checkpoint.payload().to_vec();
        corrupted_payload[0] ^= 1;
        let retained_payload = corrupted_payload.clone();
        let error = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            checkpoint.metadata(),
            &corrupted_payload,
            checkpoint.digests(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeTrainingExecutorErrorKindV1::Payload);
        assert_eq!(error.code(), "payload_digest_mismatch");
        assert_eq!(corrupted_payload, retained_payload);

        let mut corrupted_metadata = checkpoint.metadata();
        corrupted_metadata.progress.successful_update_count = 1;
        let error = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            corrupted_metadata,
            checkpoint.payload(),
            checkpoint.digests(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeTrainingExecutorErrorKindV1::Resume);
        assert_eq!(error.code(), "trainer_resume_invariant");
        assert_eq!(checkpoint.metadata().progress.successful_update_count, 0);
    }

    #[test]
    fn corrupted_adam_anchor_and_declared_digest_reject_verified_import() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();
        let checkpoint = executor.checkpoint_candidate_v1().unwrap();

        let mut adam_metadata = checkpoint.metadata();
        adam_metadata.adam_step += 1;
        let error = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            adam_metadata,
            checkpoint.payload(),
            checkpoint.digests(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeTrainingExecutorErrorKindV1::Payload);
        assert_eq!(error.code(), "payload_digest_mismatch");

        let mut anchor_metadata = checkpoint.metadata();
        anchor_metadata.scorer_bias_anchor_bits ^= 1;
        let error = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            anchor_metadata,
            checkpoint.payload(),
            checkpoint.digests(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeTrainingExecutorErrorKindV1::Payload);
        assert_eq!(error.code(), "payload_train_state_invalid");

        let mut corrupted_digests = checkpoint.digests();
        corrupted_digests.payload_sha256[0] ^= 1;
        let error = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            checkpoint.metadata(),
            checkpoint.payload(),
            corrupted_digests,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeTrainingExecutorErrorKindV1::Payload);
        assert_eq!(error.code(), "payload_digest_mismatch");

        assert_eq!(checkpoint.progress().successful_update_count, 0);
        assert_eq!(checkpoint.adam_step(), 0);
    }

    #[test]
    fn public_error_and_debug_surfaces_are_compact_and_redacted() {
        let config = burn_config_v1(2);
        let sensitive_path = Path::new("C:\\private-benchmark\\secret-snapshot.json");
        let error = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            config.clone(),
            sensitive_path,
            sensitive_path,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeTrainingExecutorErrorKindV1::Snapshot);
        assert_eq!(error.code(), "common_snapshot_rejected");
        assert!(!error.to_string().contains("private-benchmark"));
        assert!(!format!("{error:?}").contains("private-benchmark"));
        assert!(std::error::Error::source(&error).is_none());

        let (manifest, payload) = common_model_snapshot_paths_v1();
        let executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();
        let checkpoint = executor.checkpoint_candidate_v1().unwrap();
        let checkpoint_debug = format!("{checkpoint:?}");
        let executor_debug = format!("{executor:?}");
        assert!(checkpoint_debug.len() < 512, "{checkpoint_debug}");
        assert!(executor_debug.len() < 1_024, "{executor_debug}");
        assert!(checkpoint_debug.contains("payload_byte_count"));
        assert!(!checkpoint_debug.contains("parameters"));
    }
}
