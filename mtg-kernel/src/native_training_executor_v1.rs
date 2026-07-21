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
    NativeTrainingNumericalBackendV1,
    FIXED_PARTITION_PARALLEL_BACKWARD_NUMERICAL_BACKEND_IDENTITY_V1 as NATIVE_TRAINING_FIXED_PARTITION_NUMERICAL_BACKEND_IDENTITY_V1,
    NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1 as NATIVE_TRAINING_NUMERICAL_BACKEND_IDENTITY_V1,
};
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_verified_v1, encode_native_train_state_payload_v1,
    NativeTrainStatePayloadDigestsV1, NativeTrainStatePayloadErrorV1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
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
pub use crate::native_training_phase_diagnostic_v1::{
    NativeTrainingPhaseProfileV1, NativeTrainingPhaseRecordV1, NativeTrainingPhaseV1,
};
use crate::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
use crate::rl::PlayerSeatV1;
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::Cell;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
thread_local! {
    static SEGMENT_CANDIDATE_CLONE_COUNT_V2: Cell<u64> = const { Cell::new(0) };
    static SEGMENT_CANDIDATE_UPDATE_ATTEMPT_COUNT_V2: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_segment_candidate_counts_for_test_v2() {
    SEGMENT_CANDIDATE_CLONE_COUNT_V2.with(|count| count.set(0));
    SEGMENT_CANDIDATE_UPDATE_ATTEMPT_COUNT_V2.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn segment_candidate_counts_for_test_v2() -> (u64, u64) {
    let clones = SEGMENT_CANDIDATE_CLONE_COUNT_V2.with(Cell::get);
    let update_attempts = SEGMENT_CANDIDATE_UPDATE_ATTEMPT_COUNT_V2.with(Cell::get);
    (clones, update_attempts)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTrainingPrecloneMutationForTestV2 {
    OptimizerMoment,
    ModelParameter,
    Progress,
    ScorerAnchor,
    BaseSeed,
    BatchEpisodes,
    NumericalBackend,
    BackwardWorkerLimit,
}

/// In-process update configuration, not a serialized or CLI contract.
///
/// `batch_episodes` is the immutable K for the executor and must be even in
/// `2..=10_000`. Counts are exact integers; `scheduler_timeout` is a wall-clock
/// duration; coefficients are raw IEEE-754 binary32 bits. The sequential
/// numerical backend requires `backward_worker_limit == 1`; the fixed-four
/// backend accepts `1..=4`, with that limit controlling physical execution but
/// not its frozen four-partition reduction order. Topology, caps, deck
/// identifiers, coefficient finiteness, backend, and K are validated before
/// snapshot loading or state ownership. Strict callers must separately bind
/// these values into their immutable run record.
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
    pub numerical_backend: NativeTrainingNumericalBackendV1,
    pub backward_worker_limit: usize,
}

/// Public read-only projection of the frozen production seed/seat schedule.
///
/// Evidence harnesses use this projection to recompute episode provenance
/// without duplicating the schedule algorithm outside the trainer crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainingEpisodeScheduleV1 {
    pub episode_index: u64,
    pub pair_index: u64,
    pub learner_seat: PlayerSeatV1,
    pub environment_seed: u64,
}

pub fn native_training_episode_schedule_v1(
    base_seed: u64,
    episode_index: u64,
) -> Result<NativeTrainingEpisodeScheduleV1, NativeTrainingExecutorErrorV1> {
    let schedule =
        native_trainer_episode_schedule_v1(base_seed, episode_index).map_err(|error| {
            NativeTrainingExecutorErrorV1::with_diagnostic(
                NativeTrainingExecutorErrorKindV1::Schedule,
                "trainer_schedule_rejected",
                error,
            )
        })?;
    Ok(NativeTrainingEpisodeScheduleV1 {
        episode_index: schedule.episode_index,
        pair_index: schedule.pair_index,
        learner_seat: schedule.learner_seat,
        environment_seed: schedule.environment_seed,
    })
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
            numerical_backend: self.numerical_backend,
            backward_worker_limit: self.backward_worker_limit,
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

/// Payload-free intrinsic identity of one validated live checkpoint state.
///
/// This is a sealed in-process authority, not a record schema. It deliberately
/// owns no checkpoint snapshot or payload and cannot be cloned or serialized.
/// Store orchestration may compare these facts before it permits candidate
/// cloning, rollout, or artifact allocation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainingIntrinsicCheckpointFactsV2 {
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

impl NativeTrainingIntrinsicCheckpointFactsV2 {
    pub(crate) const fn base_seed_v2(&self) -> u64 {
        self.base_seed
    }

    pub(crate) const fn batch_episodes_v2(&self) -> u64 {
        self.batch_episodes
    }

    pub(crate) const fn numerical_backend_v2(&self) -> NativeTrainingNumericalBackendV1 {
        self.numerical_backend
    }

    pub(crate) const fn backward_worker_limit_v2(&self) -> usize {
        self.backward_worker_limit
    }

    pub(crate) const fn progress_v2(&self) -> NativeTrainingProgressV1 {
        self.progress
    }

    pub(crate) const fn adam_step_v2(&self) -> u64 {
        self.adam_step
    }

    pub(crate) const fn scorer_bias_anchor_bits_v2(&self) -> u32 {
        self.scorer_bias_anchor_bits
    }

    pub(crate) const fn model_parameter_sha256_v2(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub(crate) const fn train_state_sha256_v2(&self) -> [u8; 32] {
        self.train_state_sha256
    }

    #[cfg(test)]
    pub(crate) fn mutate_for_test_v2(
        &mut self,
        mutation: NativeTrainingIntrinsicFactMutationForTestV2,
    ) {
        match mutation {
            NativeTrainingIntrinsicFactMutationForTestV2::BaseSeed => self.base_seed ^= 1,
            NativeTrainingIntrinsicFactMutationForTestV2::BatchEpisodes => self.batch_episodes ^= 1,
            NativeTrainingIntrinsicFactMutationForTestV2::NumericalBackend => {
                self.numerical_backend = match self.numerical_backend {
                    NativeTrainingNumericalBackendV1::Sequential => {
                        NativeTrainingNumericalBackendV1::FixedFourPartitions
                    }
                    NativeTrainingNumericalBackendV1::FixedFourPartitions
                    | NativeTrainingNumericalBackendV1::CudaBurnDense => {
                        NativeTrainingNumericalBackendV1::Sequential
                    }
                }
            }
            NativeTrainingIntrinsicFactMutationForTestV2::BackwardWorkerLimit => {
                self.backward_worker_limit ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::NextEpisodeIndex => {
                self.progress.next_episode_index ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::SuccessfulUpdateCount => {
                self.progress.successful_update_count ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::CompletedEpisodeCount => {
                self.progress.completed_episode_count ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::LearnerPhysicalDecisionCount => {
                self.progress.learner_physical_decision_count ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::LearnerPolicyStepCount => {
                self.progress.learner_policy_step_count ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::AdamStep => self.adam_step ^= 1,
            NativeTrainingIntrinsicFactMutationForTestV2::ScorerBiasAnchorBits => {
                self.scorer_bias_anchor_bits ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::ModelParameterSha256 => {
                self.model_parameter_sha256[0] ^= 1
            }
            NativeTrainingIntrinsicFactMutationForTestV2::TrainStateSha256 => {
                self.train_state_sha256[0] ^= 1
            }
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTrainingIntrinsicFactMutationForTestV2 {
    BaseSeed,
    BatchEpisodes,
    NumericalBackend,
    BackwardWorkerLimit,
    NextEpisodeIndex,
    SuccessfulUpdateCount,
    CompletedEpisodeCount,
    LearnerPhysicalDecisionCount,
    LearnerPolicyStepCount,
    AdamStep,
    ScorerBiasAnchorBits,
    ModelParameterSha256,
    TrainStateSha256,
}

/// Complete, verified checkpoint material held in memory. Persistence layers
/// must add their own immutable metadata, atomic-publication, and recovery
/// contracts; this candidate is only the payload/progress handoff. Numerical
/// backend and backward-worker topology are nevertheless bound here so a
/// checkpoint cannot silently cross arithmetic or run-identity boundaries.
#[derive(Clone, PartialEq, Eq)]
pub struct NativeTrainingCheckpointCandidateV1 {
    base_seed: u64,
    batch_episodes: u64,
    numerical_backend: NativeTrainingNumericalBackendV1,
    backward_worker_limit: usize,
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
            .field("numerical_backend", &self.numerical_backend)
            .field("backward_worker_limit", &self.backward_worker_limit)
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
    pub numerical_backend: NativeTrainingNumericalBackendV1,
    pub backward_worker_limit: usize,
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
        validate_checkpoint_backend_metadata_v1(
            metadata.numerical_backend,
            metadata.backward_worker_limit,
        )?;
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
            numerical_backend: metadata.numerical_backend,
            backward_worker_limit: metadata.backward_worker_limit,
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

    pub fn numerical_backend(&self) -> NativeTrainingNumericalBackendV1 {
        self.numerical_backend
    }

    pub fn backward_worker_limit(&self) -> usize {
        self.backward_worker_limit
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
            numerical_backend: self.numerical_backend,
            backward_worker_limit: self.backward_worker_limit,
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

/// One validated candidate transition carrying its exact intrinsic predecessor
/// and successor authorities. This crate-private value is move-only and owns
/// no public record or serialization surface.
#[derive(Debug)]
pub(crate) struct NativeTrainingPreparedTransitionV2 {
    execution_config: Arc<NativeTrainingExecutionConfigV1>,
    predecessor: NativeTrainingIntrinsicCheckpointFactsV2,
    successor: NativeTrainingIntrinsicCheckpointFactsV2,
    observation: NativeTrainingUpdateObservationV2,
    final_checkpoint: Option<NativeTrainingCheckpointCandidateV1>,
}

impl NativeTrainingPreparedTransitionV2 {
    /// Returns the exact immutable configuration authority owned by the
    /// candidate that produced this transition. Store construction must use
    /// this sealed projection rather than accepting a second raw config.
    pub(crate) fn execution_config_v2(&self) -> &NativeTrainingExecutionConfigV1 {
        &self.execution_config
    }

    pub(crate) fn into_parts_v2(
        self,
    ) -> (
        NativeTrainingIntrinsicCheckpointFactsV2,
        NativeTrainingIntrinsicCheckpointFactsV2,
        NativeTrainingUpdateObservationV2,
        Option<NativeTrainingCheckpointCandidateV1>,
    ) {
        (
            self.predecessor,
            self.successor,
            self.observation,
            self.final_checkpoint,
        )
    }
}

/// One isolated trainer clone used for an ordered sequence of ordinary native
/// updates. Dropping the guard discards every candidate mutation and leaves the
/// exclusively borrowed live executor unchanged.
#[must_use = "dropping a segment candidate aborts it without advancing the executor"]
pub(crate) struct NativeTrainingSegmentCandidateV2<'executor> {
    executor: &'executor mut NativeTrainingExecutorV1,
    config: Arc<NativeTrainingExecutionConfigV1>,
    update_config: NativeTrainerUpdateConfigV2,
    candidate_trainer: NativeTrainerStateV2,
}

/// A prepared update owns the complete candidate trainer, observation, and
/// checkpoint while exclusively borrowing the live executor.
///
/// Persistence layers inspect [`Self::observation`] and
/// [`Self::checkpoint_candidate`], publish that exact candidate, and call
/// [`Self::bind_manifest_bytes_v2`] with the exact manifest bytes before any
/// publication begins. Dropping this value at any point leaves the live
/// executor unchanged. Because the exclusive executor borrow remains inside
/// this value, another update cannot be prepared or committed while
/// publication is in flight.
#[must_use = "dropping a prepared update aborts it without advancing the executor"]
pub struct NativeTrainingPreparedUpdateV2<'executor> {
    candidate: NativeTrainingSegmentCandidateV2<'executor>,
    observation: NativeTrainingUpdateObservationV2,
    checkpoint: NativeTrainingCheckpointCandidateV1,
}

impl<'executor> NativeTrainingPreparedUpdateV2<'executor> {
    pub fn observation(&self) -> &NativeTrainingUpdateObservationV2 {
        &self.observation
    }

    pub fn checkpoint_candidate(&self) -> &NativeTrainingCheckpointCandidateV1 {
        &self.checkpoint
    }

    /// Returns the exact validated execution configuration owned by the live
    /// executor that this guard exclusively borrows.
    ///
    /// This crate-private projection lets the Store producer bind its frozen
    /// run record to the actual executor without exposing the executor itself
    /// or adding a second, caller-supplied configuration authority.
    pub(crate) fn execution_config_v1(&self) -> &NativeTrainingExecutionConfigV1 {
        self.candidate.executor.config()
    }

    /// Re-exports the unchanged live predecessor as a verified checkpoint.
    ///
    /// The prepared candidate was computed on an isolated trainer clone, so
    /// the exclusively borrowed executor is still the exact pre-update state.
    /// Store authority uses this checked export to bind the predecessor's full
    /// train-state digest, model, Adam step, progress, and gauge anchor before
    /// accepting this guard's observation and successor checkpoint.
    pub(crate) fn pre_update_checkpoint_candidate_v1(
        &self,
    ) -> Result<NativeTrainingCheckpointCandidateV1, NativeTrainingExecutorErrorV1> {
        self.candidate.executor.checkpoint_candidate_v1()
    }

    /// Binds the exact opaque manifest bytes that a future persistence layer
    /// must publish alongside this checkpoint payload.
    ///
    /// The executor does not interpret or serialize these bytes. It owns them
    /// only so a later publication receipt can be compared against the exact
    /// manifest SHA-256 before the live trainer is advanced. This typestate has
    /// deliberately no commit method until that non-forgeable receipt is
    /// supplied by the durable store path.
    pub fn bind_manifest_bytes_v2(
        self,
        manifest_bytes: Box<[u8]>,
    ) -> NativeTrainingBoundUpdateV2<'executor> {
        let expected_manifest_sha256 = Sha256::digest(&manifest_bytes).into();
        let expected_generation_index = self.checkpoint.adam_step();
        let expected_payload_sha256 = self.checkpoint.digests().payload_sha256;
        let Self {
            candidate,
            observation,
            checkpoint,
        } = self;
        NativeTrainingBoundUpdateV2 {
            candidate,
            observation,
            checkpoint,
            manifest_bytes,
            expected_generation_index,
            expected_payload_sha256,
            expected_manifest_sha256,
        }
    }
}

/// Prepared candidate plus the exact opaque manifest bytes intended for
/// publication. This is an intermediate typestate, not a persistence receipt.
///
/// The only live-commit operation consumes a private-construction, move-only V2
/// store receipt proving publication of this generation, payload digest, and
/// manifest digest. Dropping this value leaves the exclusively borrowed
/// executor unchanged.
#[must_use = "dropping a publication-bound update aborts it without advancing the executor"]
pub struct NativeTrainingBoundUpdateV2<'executor> {
    candidate: NativeTrainingSegmentCandidateV2<'executor>,
    observation: NativeTrainingUpdateObservationV2,
    checkpoint: NativeTrainingCheckpointCandidateV1,
    manifest_bytes: Box<[u8]>,
    expected_generation_index: u64,
    expected_payload_sha256: [u8; 32],
    expected_manifest_sha256: [u8; 32],
}

impl NativeTrainingBoundUpdateV2<'_> {
    pub fn observation(&self) -> &NativeTrainingUpdateObservationV2 {
        &self.observation
    }

    pub fn checkpoint_candidate(&self) -> &NativeTrainingCheckpointCandidateV1 {
        &self.checkpoint
    }

    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }

    pub const fn expected_generation_index(&self) -> u64 {
        self.expected_generation_index
    }

    pub const fn expected_payload_sha256(&self) -> [u8; 32] {
        self.expected_payload_sha256
    }

    pub const fn expected_manifest_sha256(&self) -> [u8; 32] {
        self.expected_manifest_sha256
    }

    /// Consumes a V2 store receipt, checks it against the exact bound candidate,
    /// and advances the live executor only after every fallible check passes.
    ///
    /// Receipt mismatch consumes and aborts the candidate while leaving the
    /// live executor unchanged. On success, the trainer assignment is the sole
    /// commit point; no fallible operation follows it.
    pub fn commit_v2(
        self,
        receipt: NativeTrainingPersistenceReceiptV2,
    ) -> Result<NativeTrainingUpdateObservationV2, NativeTrainingExecutorErrorV1> {
        if !persistence_receipt_matches_v2(
            &receipt,
            self.expected_generation_index,
            self.expected_payload_sha256,
            self.expected_manifest_sha256,
        ) {
            return Err(NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "persistence_receipt_mismatch",
            ));
        }

        let Self {
            candidate,
            observation,
            ..
        } = self;
        candidate.install_infallibly_v2();
        Ok(observation)
    }
}

fn persistence_receipt_matches_v2(
    receipt: &NativeTrainingPersistenceReceiptV2,
    expected_generation_index: u64,
    expected_payload_sha256: [u8; 32],
    expected_manifest_sha256: [u8; 32],
) -> bool {
    receipt.generation_index() == expected_generation_index
        && receipt.checkpoint_payload_sha256() == expected_payload_sha256
        && receipt.checkpoint_manifest_sha256() == expected_manifest_sha256
}

/// Stateful one-update-at-a-time trainer executor.
///
/// Durable callers must use [`Self::prepare_update_v2`] so a successful
/// persistence attempt precedes the live in-memory commit. The direct
/// [`Self::run_update_v2`] method remains available for non-persisting runners
/// and diagnostics.
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

impl<'executor> NativeTrainingSegmentCandidateV2<'executor> {
    /// Runs one ordinary K-episode update on the isolated candidate. The exact
    /// predecessor authority is consumed and independently rederived before
    /// any candidate mutation. Only a requested final step exports a payload.
    pub(crate) fn prepare_transition_v2(
        &mut self,
        expected_predecessor: NativeTrainingIntrinsicCheckpointFactsV2,
        export_final_checkpoint: bool,
    ) -> Result<NativeTrainingPreparedTransitionV2, NativeTrainingExecutorErrorV1> {
        let predecessor =
            intrinsic_checkpoint_facts_from_parts_v2(&self.config, &self.candidate_trainer)?;
        if predecessor != expected_predecessor {
            return Err(NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "segment_candidate_predecessor_mismatch",
            ));
        }
        #[cfg(test)]
        SEGMENT_CANDIDATE_UPDATE_ATTEMPT_COUNT_V2.with(|count| count.set(count.get() + 1));
        let observation = self
            .candidate_trainer
            .run_even_batch_update_v2(&self.update_config)
            .map_err(trainer_executor_error_v1)?;
        let successor =
            intrinsic_checkpoint_facts_from_parts_v2(&self.config, &self.candidate_trainer)?;
        validate_current_observation_from_parts_v2(
            &self.config,
            &self.candidate_trainer,
            &observation,
        )?;
        let final_checkpoint = if export_final_checkpoint {
            let checkpoint = checkpoint_candidate_from_parts_with_facts_v2(
                &self.config,
                &self.candidate_trainer,
                &successor,
            )?;
            if !checkpoint_matches_intrinsic_facts_v2(&checkpoint, &successor) {
                return Err(NativeTrainingExecutorErrorV1::redacted(
                    NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                    "segment_final_checkpoint_mismatch",
                ));
            }
            Some(checkpoint)
        } else {
            None
        };
        Ok(NativeTrainingPreparedTransitionV2 {
            execution_config: Arc::clone(&self.config),
            predecessor,
            successor,
            observation,
            final_checkpoint,
        })
    }

    fn into_single_update_v2(
        self,
        transition: NativeTrainingPreparedTransitionV2,
    ) -> Result<NativeTrainingPreparedUpdateV2<'executor>, NativeTrainingExecutorErrorV1> {
        let NativeTrainingPreparedTransitionV2 {
            execution_config: _,
            predecessor: _,
            successor: _,
            observation,
            final_checkpoint,
        } = transition;
        let checkpoint = final_checkpoint.ok_or_else(|| {
            NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "single_update_missing_checkpoint",
            )
        })?;
        Ok(NativeTrainingPreparedUpdateV2 {
            candidate: self,
            observation,
            checkpoint,
        })
    }

    /// The sole candidate-to-live assignment. Callers must complete every
    /// fallible persistence check before invoking this consuming operation.
    pub(crate) fn install_infallibly_v2(self) {
        let Self {
            executor,
            candidate_trainer,
            ..
        } = self;
        executor.trainer = candidate_trainer;
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
    /// Rejects config/K/base-seed/backend/backward-topology mismatch, payload
    /// drift, invalid train state, incoherent progress/Adam counters, or an
    /// invalid next schedule before returning an executor.
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
        if checkpoint.numerical_backend != config.numerical_backend {
            return Err(NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "checkpoint_numerical_backend_mismatch",
            ));
        }
        if checkpoint.backward_worker_limit != config.backward_worker_limit {
            return Err(NativeTrainingExecutorErrorV1::redacted(
                NativeTrainingExecutorErrorKindV1::CheckpointBinding,
                "checkpoint_backward_worker_limit_mismatch",
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

    #[cfg(test)]
    pub(crate) fn mutate_live_for_preclone_test_v2(
        &mut self,
        mutation: NativeTrainingPrecloneMutationForTestV2,
    ) {
        match mutation {
            NativeTrainingPrecloneMutationForTestV2::OptimizerMoment => {
                self.trainer.mutate_optimizer_moment_for_preclone_test_v2()
            }
            NativeTrainingPrecloneMutationForTestV2::ModelParameter => {
                self.trainer.mutate_model_parameter_for_preclone_test_v2()
            }
            NativeTrainingPrecloneMutationForTestV2::Progress => {
                self.trainer.mutate_progress_for_preclone_test_v2()
            }
            NativeTrainingPrecloneMutationForTestV2::ScorerAnchor => {
                self.trainer.mutate_scorer_anchor_for_preclone_test_v2()
            }
            NativeTrainingPrecloneMutationForTestV2::BaseSeed => self.config.run_base_seed ^= 1,
            NativeTrainingPrecloneMutationForTestV2::BatchEpisodes => {
                self.config.batch_episodes = self.config.batch_episodes.saturating_add(2)
            }
            NativeTrainingPrecloneMutationForTestV2::NumericalBackend => {
                self.config.numerical_backend =
                    NativeTrainingNumericalBackendV1::FixedFourPartitions
            }
            NativeTrainingPrecloneMutationForTestV2::BackwardWorkerLimit => {
                self.config.backward_worker_limit = 2
            }
        }
    }

    /// Derives the complete live checkpoint identity without materializing a
    /// train-state snapshot or payload.
    pub(crate) fn intrinsic_checkpoint_facts_v2(
        &self,
    ) -> Result<NativeTrainingIntrinsicCheckpointFactsV2, NativeTrainingExecutorErrorV1> {
        intrinsic_checkpoint_facts_from_parts_v2(&self.config, &self.trainer)
    }

    /// Clones one isolated candidate only after the caller has completed its
    /// payload-free parent and representability checks.
    pub(crate) fn begin_segment_candidate_v2(
        &mut self,
    ) -> Result<NativeTrainingSegmentCandidateV2<'_>, NativeTrainingExecutorErrorV1> {
        validate_store_preparation_config_v2(&self.config)?;
        let candidate_trainer = self.trainer.clone();
        #[cfg(test)]
        SEGMENT_CANDIDATE_CLONE_COUNT_V2.with(|count| count.set(count.get() + 1));
        Ok(NativeTrainingSegmentCandidateV2 {
            config: Arc::new(self.config.clone()),
            update_config: self.update_config.clone(),
            candidate_trainer,
            executor: self,
        })
    }

    /// Computes and validates one complete update against an isolated clone of
    /// the live trainer, then returns an exclusive prepared-update guard.
    ///
    /// The live executor remains unchanged throughout rollout, training,
    /// observation binding, and 14.7 MiB checkpoint encoding. A persistence
    /// layer may retry publication through the borrowed checkpoint without
    /// recomputing the update. This intermediate typestate deliberately cannot
    /// make the candidate live; the receipt-gated commit operation lands only
    /// with the durable V2 store. Dropping the guard aborts the candidate and
    /// releases the executor unchanged.
    ///
    /// # Errors
    ///
    /// Returns a classified error with the live executor unchanged for every
    /// candidate rollout, training, validation, or checkpoint-export failure.
    /// Frozen Store RunV2 is sequential-only, so this V2 preparation boundary
    /// rejects the fixed-four backend before cloning or rollout.
    pub fn prepare_update_v2(
        &mut self,
    ) -> Result<NativeTrainingPreparedUpdateV2<'_>, NativeTrainingExecutorErrorV1> {
        validate_store_preparation_config_v2(&self.config)?;
        let predecessor = self.intrinsic_checkpoint_facts_v2()?;
        let mut candidate = self.begin_segment_candidate_v2()?;
        let transition = candidate.prepare_transition_v2(predecessor, true)?;
        candidate.into_single_update_v2(transition)
    }

    /// Runs exactly one K-episode transactional update and moves out the complete
    /// owned V2 evidence without post-timer evidence copying.
    ///
    /// This method commits in memory before returning and is therefore for
    /// non-persisting runners and diagnostics. Durable callers must use
    /// [`Self::prepare_update_v2`].
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

    /// Runs one in-memory update while collecting non-authoritative phase wall
    /// timings. The returned profile is diagnostic only: it has no serializer,
    /// identity, Store mapping, or benchmark-record mapping. Training inputs,
    /// arithmetic, validation, and the committed observation are shared with
    /// [`Self::run_update_v2`].
    pub fn run_update_with_phase_profile_v1(
        &mut self,
    ) -> Result<
        (
            NativeTrainingUpdateObservationV2,
            NativeTrainingPhaseProfileV1,
        ),
        NativeTrainingExecutorErrorV1,
    > {
        self.trainer
            .run_even_batch_update_profiled_v2(&self.update_config)
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
    /// K, Adam, backend topology, next schedule, payload layout, or digest
    /// construction fails.
    pub fn checkpoint_candidate_v1(
        &self,
    ) -> Result<NativeTrainingCheckpointCandidateV1, NativeTrainingExecutorErrorV1> {
        let facts = self.intrinsic_checkpoint_facts_v2()?;
        checkpoint_candidate_from_parts_with_facts_v2(&self.config, &self.trainer, &facts)
    }

    fn validate_current_observation_v2(
        &self,
        observation: &NativeTrainingUpdateObservationV2,
    ) -> Result<(), NativeTrainingExecutorErrorV1> {
        validate_current_observation_from_parts_v2(&self.config, &self.trainer, observation)
    }
}

fn checkpoint_candidate_from_parts_with_facts_v2(
    config: &NativeTrainingExecutionConfigV1,
    trainer: &NativeTrainerStateV2,
    facts: &NativeTrainingIntrinsicCheckpointFactsV2,
) -> Result<NativeTrainingCheckpointCandidateV1, NativeTrainingExecutorErrorV1> {
    if facts.base_seed != config.run_base_seed
        || facts.batch_episodes != config.batch_episodes
        || facts.numerical_backend != config.numerical_backend
        || facts.backward_worker_limit != config.backward_worker_limit
        || facts.progress != NativeTrainingProgressV1::from(trainer.progress_v2())
    {
        return Err(NativeTrainingExecutorErrorV1::redacted(
            NativeTrainingExecutorErrorKindV1::CheckpointBinding,
            "checkpoint_intrinsic_facts_mismatch",
        ));
    }
    let snapshot = trainer.train_state_v1().snapshot_v1().map_err(|error| {
        NativeTrainingExecutorErrorV1::with_diagnostic(
            NativeTrainingExecutorErrorKindV1::TrainState,
            "live_train_state_invalid",
            error,
        )
    })?;
    let encoded =
        encode_native_train_state_payload_v1(&snapshot).map_err(payload_executor_error_v1)?;
    let decoded = decode_native_train_state_payload_verified_v1(
        &encoded.bytes,
        snapshot.adam_step,
        snapshot.scorer_bias_anchor_bits,
        &encoded.digests,
    )
    .map_err(payload_executor_error_v1)?;
    if snapshot.adam_step != facts.adam_step
        || snapshot.scorer_bias_anchor_bits != facts.scorer_bias_anchor_bits
        || encoded.digests.model_parameter_sha256 != facts.model_parameter_sha256
        || encoded.digests.native_state_sha256 != facts.train_state_sha256
        || decoded.digests.model_parameter_sha256 != facts.model_parameter_sha256
        || decoded.digests.native_state_sha256 != facts.train_state_sha256
    {
        return Err(NativeTrainingExecutorErrorV1::redacted(
            NativeTrainingExecutorErrorKindV1::CheckpointBinding,
            "checkpoint_intrinsic_facts_mismatch",
        ));
    }
    Ok(NativeTrainingCheckpointCandidateV1 {
        base_seed: facts.base_seed,
        batch_episodes: facts.batch_episodes,
        numerical_backend: facts.numerical_backend,
        backward_worker_limit: facts.backward_worker_limit,
        progress: facts.progress,
        adam_step: facts.adam_step,
        scorer_bias_anchor_bits: facts.scorer_bias_anchor_bits,
        payload: encoded.bytes,
        digests: encoded.digests,
    })
}

fn validate_current_observation_from_parts_v2(
    config: &NativeTrainingExecutionConfigV1,
    trainer: &NativeTrainerStateV2,
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<(), NativeTrainingExecutorErrorV1> {
    let progress = NativeTrainingProgressV1::from(trainer.progress_v2());
    let expected_first_episode_index = observation
        .adam_step_before
        .checked_mul(config.batch_episodes)
        .ok_or_else(checkpoint_observation_mismatch_v1)?;
    let expected_end_episode_index = observation
        .first_episode_index
        .checked_add(observation.episode_count)
        .ok_or_else(checkpoint_observation_mismatch_v1)?;
    let expected_logical_actor_count = config
        .worker_count
        .checked_mul(config.sessions_per_worker)
        .ok_or_else(checkpoint_observation_mismatch_v1)?;
    let expected_episode_len =
        usize::try_from(config.batch_episodes).map_err(|_| checkpoint_observation_mismatch_v1())?;

    if observation.trainer_contract_identity != NATIVE_TRAINER_CONTRACT_IDENTITY_V2
        || observation.episode_count != config.batch_episodes
        || observation.first_episode_index != expected_first_episode_index
        || expected_end_episode_index != progress.next_episode_index
        || progress.completed_episode_count != expected_end_episode_index
        || observation.adam_step_before.checked_add(1) != Some(observation.adam_step_after)
        || observation.adam_step_after != progress.successful_update_count
        || observation.worker_count != config.worker_count
        || observation.sessions_per_worker != config.sessions_per_worker
        || observation.logical_actor_count != expected_logical_actor_count
        || observation.broker_batch_target != config.broker_batch_target
        || observation.episodes.len() != expected_episode_len
        || observation.selected_outputs.len()
            != usize::try_from(observation.learner_policy_step_count)
                .map_err(|_| checkpoint_observation_mismatch_v1())?
        || observation.physical_terms.len()
            != usize::try_from(observation.learner_group_count)
                .map_err(|_| checkpoint_observation_mismatch_v1())?
        || trainer
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
            .checked_add(u64::try_from(offset).map_err(|_| checkpoint_observation_mismatch_v1())?)
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

fn intrinsic_checkpoint_facts_from_parts_v2(
    config: &NativeTrainingExecutionConfigV1,
    trainer: &NativeTrainerStateV2,
) -> Result<NativeTrainingIntrinsicCheckpointFactsV2, NativeTrainingExecutorErrorV1> {
    validate_resumed_parts_v2(
        config.run_base_seed,
        config.batch_episodes,
        trainer.train_state_v1(),
        trainer.progress_v2(),
    )
    .map_err(trainer_executor_error_v1)?;
    let train_state = trainer.train_state_v1();
    let train_state_sha256 = train_state.state_sha256_v1().map_err(|error| {
        NativeTrainingExecutorErrorV1::with_diagnostic(
            NativeTrainingExecutorErrorKindV1::TrainState,
            "live_train_state_invalid",
            error,
        )
    })?;
    Ok(NativeTrainingIntrinsicCheckpointFactsV2 {
        base_seed: config.run_base_seed,
        batch_episodes: config.batch_episodes,
        numerical_backend: config.numerical_backend,
        backward_worker_limit: config.backward_worker_limit,
        progress: trainer.progress_v2().into(),
        adam_step: train_state.adam_step_v1(),
        scorer_bias_anchor_bits: train_state.scorer_bias_anchor_f32_bits_v1(),
        model_parameter_sha256: train_state.model_v1().parameter_manifest_sha256_raw_v1(),
        train_state_sha256,
    })
}

fn validate_store_preparation_config_v2(
    config: &NativeTrainingExecutionConfigV1,
) -> Result<(), NativeTrainingExecutorErrorV1> {
    if config.numerical_backend != NativeTrainingNumericalBackendV1::Sequential
        || config.backward_worker_limit != 1
    {
        return Err(NativeTrainingExecutorErrorV1::redacted(
            NativeTrainingExecutorErrorKindV1::Configuration,
            "store_v2_requires_sequential_numerical_backend",
        ));
    }
    Ok(())
}

pub(crate) fn checkpoint_matches_intrinsic_facts_v2(
    checkpoint: &NativeTrainingCheckpointCandidateV1,
    facts: &NativeTrainingIntrinsicCheckpointFactsV2,
) -> bool {
    checkpoint.base_seed == facts.base_seed
        && checkpoint.batch_episodes == facts.batch_episodes
        && checkpoint.numerical_backend == facts.numerical_backend
        && checkpoint.backward_worker_limit == facts.backward_worker_limit
        && checkpoint.progress == facts.progress
        && checkpoint.adam_step == facts.adam_step
        && checkpoint.scorer_bias_anchor_bits == facts.scorer_bias_anchor_bits
        && checkpoint.digests.model_parameter_sha256 == facts.model_parameter_sha256
        && checkpoint.digests.native_state_sha256 == facts.train_state_sha256
}

fn checkpoint_observation_mismatch_v1() -> NativeTrainingExecutorErrorV1 {
    NativeTrainingExecutorErrorV1::redacted(
        NativeTrainingExecutorErrorKindV1::CheckpointBinding,
        "checkpoint_observation_mismatch",
    )
}

fn validate_checkpoint_backend_metadata_v1(
    numerical_backend: NativeTrainingNumericalBackendV1,
    backward_worker_limit: usize,
) -> Result<(), NativeTrainingExecutorErrorV1> {
    if numerical_backend.accepts_backward_worker_limit_v1(backward_worker_limit) {
        Ok(())
    } else {
        Err(NativeTrainingExecutorErrorV1::redacted(
            NativeTrainingExecutorErrorKindV1::CheckpointBinding,
            "checkpoint_backend_metadata_invalid",
        ))
    }
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
    use crate::native_policy_train_step_v1::{
        reset_train_state_snapshot_call_count_for_test_v1,
        train_state_snapshot_call_count_for_test_v1,
    };
    use crate::native_train_state_payload_v1::{
        payload_encode_counts_for_test_v1, reset_payload_encode_counts_for_test_v1,
    };

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
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
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
    fn intrinsic_checkpoint_facts_are_payload_free_and_match_full_export() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();

        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();
        let facts = executor.intrinsic_checkpoint_facts_v2().unwrap();
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
        assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));

        let checkpoint = executor.checkpoint_candidate_v1().unwrap();
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 1);
        assert_eq!(payload_encode_counts_for_test_v1(), (1, 1));
        assert_eq!(facts.base_seed, checkpoint.base_seed());
        assert_eq!(facts.batch_episodes, checkpoint.batch_episodes());
        assert_eq!(facts.numerical_backend, checkpoint.numerical_backend());
        assert_eq!(
            facts.backward_worker_limit,
            checkpoint.backward_worker_limit()
        );
        assert_eq!(facts.progress, checkpoint.progress());
        assert_eq!(facts.adam_step, checkpoint.adam_step());
        assert_eq!(
            facts.scorer_bias_anchor_bits,
            checkpoint.scorer_bias_anchor_bits()
        );
        assert_eq!(
            facts.model_parameter_sha256,
            checkpoint.digests().model_parameter_sha256
        );
        assert_eq!(
            facts.train_state_sha256,
            checkpoint.digests().native_state_sha256
        );
    }

    #[test]
    fn segment_candidate_runs_four_ordered_updates_with_only_one_final_export() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let mut executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();
        let predecessor = executor.intrinsic_checkpoint_facts_v2().unwrap();
        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();
        reset_segment_candidate_counts_for_test_v2();

        let mut candidate = executor.begin_segment_candidate_v2().unwrap();
        assert_eq!(segment_candidate_counts_for_test_v2(), (1, 0));
        let mut predecessor = predecessor;
        let mut initial_predecessor = None;
        let mut final_checkpoint = None;
        for step in 0..4u64 {
            let expected_progress = predecessor.progress;
            let expected_model_sha256 = predecessor.model_parameter_sha256;
            let expected_train_sha256 = predecessor.train_state_sha256;
            let transition = candidate
                .prepare_transition_v2(predecessor, step == 3)
                .unwrap();
            assert_eq!(segment_candidate_counts_for_test_v2(), (1, step + 1));
            let NativeTrainingPreparedTransitionV2 {
                execution_config: _,
                predecessor: actual_predecessor,
                successor,
                observation,
                final_checkpoint: exported,
            } = transition;
            assert_eq!(actual_predecessor.progress, expected_progress);
            assert_eq!(
                actual_predecessor.model_parameter_sha256,
                expected_model_sha256
            );
            assert_eq!(actual_predecessor.train_state_sha256, expected_train_sha256);
            assert_eq!(observation.adam_step_before, step);
            assert_eq!(observation.adam_step_after, step + 1);
            assert_eq!(successor.progress.successful_update_count, step + 1);
            assert_eq!(successor.adam_step, step + 1);
            if step == 0 {
                initial_predecessor = Some(actual_predecessor);
            }
            if step < 3 {
                assert!(exported.is_none());
                assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
                assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
            } else {
                let exported = exported.unwrap();
                assert!(checkpoint_matches_intrinsic_facts_v2(&exported, &successor));
                final_checkpoint = Some(exported);
                assert_eq!(train_state_snapshot_call_count_for_test_v1(), 1);
                assert_eq!(payload_encode_counts_for_test_v1(), (1, 1));
            }
            predecessor = successor;
        }
        assert_eq!(
            final_checkpoint.unwrap().progress().successful_update_count,
            4
        );
        assert_eq!(segment_candidate_counts_for_test_v2(), (1, 4));
        drop(candidate);

        let live_after_drop = executor.intrinsic_checkpoint_facts_v2().unwrap();
        assert_eq!(live_after_drop, initial_predecessor.unwrap());
    }

    #[test]
    fn segment_candidate_rejects_a_wrong_predecessor_before_candidate_mutation() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let mut executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();
        let mut candidate = executor.begin_segment_candidate_v2().unwrap();
        let baseline = intrinsic_checkpoint_facts_from_parts_v2(
            &candidate.config,
            &candidate.candidate_trainer,
        )
        .unwrap();
        let mut wrong = intrinsic_checkpoint_facts_from_parts_v2(
            &candidate.config,
            &candidate.candidate_trainer,
        )
        .unwrap();
        wrong.train_state_sha256[0] ^= 1;
        reset_train_state_snapshot_call_count_for_test_v1();
        reset_payload_encode_counts_for_test_v1();

        let error = candidate.prepare_transition_v2(wrong, false).unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(error.code(), "segment_candidate_predecessor_mismatch");
        assert_eq!(train_state_snapshot_call_count_for_test_v1(), 0);
        assert_eq!(payload_encode_counts_for_test_v1(), (0, 0));
        let after = intrinsic_checkpoint_facts_from_parts_v2(
            &candidate.config,
            &candidate.candidate_trainer,
        )
        .unwrap();
        assert_eq!(after, baseline);
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
    fn fixed_backend_checkpoint_binding_is_exact_and_store_v2_preparation_fails_closed() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let mut fixed_config = burn_config_v1(2);
        fixed_config.numerical_backend = NativeTrainingNumericalBackendV1::FixedFourPartitions;
        fixed_config.backward_worker_limit = 4;
        let mut fixed = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            fixed_config.clone(),
            &manifest,
            &payload,
        )
        .unwrap();
        fixed.run_update_v2().unwrap();
        let checkpoint = fixed.checkpoint_candidate_v1().unwrap();
        assert_eq!(
            checkpoint.numerical_backend(),
            NativeTrainingNumericalBackendV1::FixedFourPartitions
        );
        assert_eq!(checkpoint.backward_worker_limit(), 4);
        assert_eq!(
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(
                fixed_config.clone(),
                &checkpoint
            )
            .unwrap()
            .checkpoint_candidate_v1()
            .unwrap(),
            checkpoint
        );

        let sequential_error =
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(burn_config_v1(2), &checkpoint)
                .unwrap_err();
        assert_eq!(
            sequential_error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(
            sequential_error.code(),
            "checkpoint_numerical_backend_mismatch"
        );

        let mut other_topology = fixed_config.clone();
        other_topology.backward_worker_limit = 1;
        let topology_error =
            NativeTrainingExecutorV1::from_checkpoint_candidate_v1(other_topology, &checkpoint)
                .unwrap_err();
        assert_eq!(
            topology_error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(
            topology_error.code(),
            "checkpoint_backward_worker_limit_mismatch"
        );

        let mut invalid_metadata = checkpoint.metadata();
        invalid_metadata.backward_worker_limit = 0;
        let metadata_error = NativeTrainingCheckpointCandidateV1::import_verified_v1(
            invalid_metadata,
            &[],
            checkpoint.digests(),
        )
        .unwrap_err();
        assert_eq!(
            metadata_error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(metadata_error.code(), "checkpoint_backend_metadata_invalid");

        let mut fresh_fixed = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            fixed_config,
            &manifest,
            &payload,
        )
        .unwrap();
        let before = fresh_fixed.checkpoint_candidate_v1().unwrap();
        let prepare_error = fresh_fixed.prepare_update_v2().err().unwrap();
        assert_eq!(
            prepare_error.kind(),
            NativeTrainingExecutorErrorKindV1::Configuration
        );
        assert_eq!(
            prepare_error.code(),
            "store_v2_requires_sequential_numerical_backend"
        );
        assert_eq!(fresh_fixed.checkpoint_candidate_v1().unwrap(), before);
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
    fn prepared_and_manifest_bound_updates_abort_unchanged_and_retry_exactly() {
        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let mut executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();
        let before = executor.checkpoint_candidate_v1().unwrap();

        let (aborted_observation, aborted_candidate) = {
            let prepared = executor.prepare_update_v2().unwrap();
            assert_eq!(prepared.observation().adam_step_before, 0);
            assert_eq!(prepared.observation().adam_step_after, 1);
            assert_eq!(
                prepared
                    .checkpoint_candidate()
                    .progress()
                    .successful_update_count,
                1
            );
            assert_ne!(prepared.checkpoint_candidate(), &before);

            // A persistence implementation may attempt publication repeatedly
            // through this same borrowed candidate. No export or update is
            // repeated and the backing payload remains the same allocation.
            let first_payload = prepared.checkpoint_candidate().payload();
            let first_payload_address = first_payload.as_ptr();
            let first_payload_sha256 = prepared.checkpoint_candidate().payload_sha256_hex();
            assert_eq!(
                prepared.checkpoint_candidate().payload().as_ptr(),
                first_payload_address
            );
            assert_eq!(
                prepared.checkpoint_candidate().payload_sha256_hex(),
                first_payload_sha256
            );

            // Simulate a permanent publication failure by dropping the guard.
            (
                without_timing_v1(prepared.observation().clone()),
                prepared.checkpoint_candidate().clone(),
            )
        };

        assert_eq!(executor.progress().successful_update_count, 0);
        assert_eq!(executor.checkpoint_candidate_v1().unwrap(), before);

        let manifest_bytes = br#"{"checkpoint":"candidate-one"}
"#
        .to_vec()
        .into_boxed_slice();
        let expected_manifest_sha256: [u8; 32] = Sha256::digest(&manifest_bytes).into();
        {
            let prepared = executor.prepare_update_v2().unwrap();
            assert_eq!(
                without_timing_v1(prepared.observation().clone()),
                aborted_observation
            );
            assert_eq!(prepared.checkpoint_candidate(), &aborted_candidate);
            let bound = prepared.bind_manifest_bytes_v2(manifest_bytes.clone());
            assert_eq!(bound.expected_generation_index(), 1);
            assert_eq!(
                bound.expected_payload_sha256(),
                aborted_candidate.digests().payload_sha256
            );
            assert_eq!(bound.expected_manifest_sha256(), expected_manifest_sha256);
            assert_eq!(bound.manifest_bytes(), manifest_bytes.as_ref());
            assert_eq!(bound.checkpoint_candidate(), &aborted_candidate);
            assert_eq!(
                without_timing_v1(bound.observation().clone()),
                aborted_observation
            );
            // Dropping without a store receipt after a hypothetical
            // publication failure still aborts.
        }
        assert_eq!(executor.progress().successful_update_count, 0);
        assert_eq!(executor.checkpoint_candidate_v1().unwrap(), before);

        // The existing direct non-persisting update remains a useful oracle:
        // it must produce the exact candidate held by both aborted guards.
        let direct_observation = executor.run_update_v2().unwrap();
        assert_eq!(without_timing_v1(direct_observation), aborted_observation);
        assert_eq!(
            executor.checkpoint_candidate_v1().unwrap(),
            aborted_candidate
        );
    }

    #[test]
    fn persistence_receipt_is_nonforgeable_and_gates_the_only_live_commit() {
        use crate::native_training_store_v2::test_persistence_receipt_v2;

        let (manifest, payload) = common_model_snapshot_paths_v1();
        let config = burn_config_v1(2);
        let mut executor =
            NativeTrainingExecutorV1::from_common_model_snapshot_v1(config, &manifest, &payload)
                .unwrap();
        let before = executor.checkpoint_candidate_v1().unwrap();
        let manifest_bytes = br#"{"checkpoint":"receipt-gated-candidate"}
"#
        .to_vec()
        .into_boxed_slice();

        let (expected_observation, expected_candidate, generation, payload_sha, manifest_sha) = {
            let prepared = executor.prepare_update_v2().unwrap();
            let bound = prepared.bind_manifest_bytes_v2(manifest_bytes.clone());
            (
                without_timing_v1(bound.observation().clone()),
                bound.checkpoint_candidate().clone(),
                bound.expected_generation_index(),
                bound.expected_payload_sha256(),
                bound.expected_manifest_sha256(),
            )
        };
        assert_eq!(executor.checkpoint_candidate_v1().unwrap(), before);

        let wrong_generation =
            test_persistence_receipt_v2(generation + 1, payload_sha, manifest_sha);
        let wrong_payload = test_persistence_receipt_v2(
            generation,
            {
                let mut digest = payload_sha;
                digest[31] ^= 1;
                digest
            },
            manifest_sha,
        );
        let wrong_manifest = test_persistence_receipt_v2(generation, payload_sha, {
            let mut digest = manifest_sha;
            digest[31] ^= 1;
            digest
        });
        assert!(!persistence_receipt_matches_v2(
            &wrong_generation,
            generation,
            payload_sha,
            manifest_sha
        ));
        assert!(!persistence_receipt_matches_v2(
            &wrong_payload,
            generation,
            payload_sha,
            manifest_sha
        ));
        assert!(!persistence_receipt_matches_v2(
            &wrong_manifest,
            generation,
            payload_sha,
            manifest_sha
        ));

        let prepared = executor.prepare_update_v2().unwrap();
        let bound = prepared.bind_manifest_bytes_v2(manifest_bytes.clone());
        let error = bound.commit_v2(wrong_generation).unwrap_err();
        assert_eq!(
            error.kind(),
            NativeTrainingExecutorErrorKindV1::CheckpointBinding
        );
        assert_eq!(error.code(), "persistence_receipt_mismatch");
        assert_eq!(executor.progress().successful_update_count, 0);
        assert_eq!(executor.checkpoint_candidate_v1().unwrap(), before);

        let prepared = executor.prepare_update_v2().unwrap();
        let bound = prepared.bind_manifest_bytes_v2(manifest_bytes);
        let correct_receipt = test_persistence_receipt_v2(generation, payload_sha, manifest_sha);
        let committed_observation = bound.commit_v2(correct_receipt).unwrap();
        assert_eq!(
            without_timing_v1(committed_observation),
            expected_observation
        );
        assert_eq!(executor.progress().successful_update_count, 1);
        assert_eq!(
            executor.checkpoint_candidate_v1().unwrap(),
            expected_candidate
        );
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
