//! Observation-to-checkpoint scoring seam for visible external game clients.
//!
//! This module accepts one complete public policy-v5 observation and its exact
//! ordered semantic legal-action set. It reconstructs the existing Flat V2
//! scorer view using only those public records, then calls the unchanged native
//! checkpoint inference path. It does not create executable kernel actions,
//! session bindings, coordinates, input authority, queue authority, or match
//! entry authority.

use crate::flat_policy_v2::encode_external_flat_scoring_decision_owned_v1;
use crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
use crate::rl::{ActionSemanticV1, ObservationV5};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Display, Formatter};

const EXTERNAL_SCORING_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtg-kernel/external-observation-scoring-input/v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeExternalObservationScoringErrorKindV1 {
    DecisionInvalid,
    InferenceInvalid,
    CommitmentInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeExternalObservationScoringErrorV1 {
    kind: NativeExternalObservationScoringErrorKindV1,
}

impl NativeExternalObservationScoringErrorV1 {
    pub const fn kind(&self) -> NativeExternalObservationScoringErrorKindV1 {
        self.kind
    }
}

impl Display for NativeExternalObservationScoringErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "external observation scoring failed: {:?}",
            self.kind
        )
    }
}

impl std::error::Error for NativeExternalObservationScoringErrorV1 {}

fn error_v1(
    kind: NativeExternalObservationScoringErrorKindV1,
) -> NativeExternalObservationScoringErrorV1 {
    NativeExternalObservationScoringErrorV1 { kind }
}

/// Immutable score packet bound to one exact public decision and checkpoint.
///
/// The ordered logits correspond one-for-one to the ordered semantic actions
/// passed to [`NativeCheckpointInferenceV1::score_external_observation_v1`].
/// The packet is model output only and is never an input or match-entry token.
#[derive(Clone, PartialEq)]
pub struct NativeExternalObservationScoreV1 {
    decision_commitment_sha256: [u8; 32],
    run_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    generation_index: u64,
    action_logits: Vec<f32>,
    value: f32,
}

impl Debug for NativeExternalObservationScoreV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeExternalObservationScoreV1")
            .field(
                "decision_commitment_sha256",
                &self.decision_commitment_sha256,
            )
            .field(
                "checkpoint_manifest_sha256",
                &self.checkpoint_manifest_sha256,
            )
            .field("generation_index", &self.generation_index)
            .field("action_logit_count", &self.action_logits.len())
            .finish_non_exhaustive()
    }
}

impl NativeExternalObservationScoreV1 {
    pub const fn decision_commitment_sha256(&self) -> [u8; 32] {
        self.decision_commitment_sha256
    }

    pub const fn run_sha256(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }

    pub const fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub fn action_logits(&self) -> &[f32] {
        &self.action_logits
    }

    pub const fn value(&self) -> f32 {
        self.value
    }

    pub const fn is_input_authority_v1(&self) -> bool {
        false
    }

    pub const fn permits_match_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Serialize)]
struct ExternalScoringCommitmentInputV1<'a> {
    observation: &'a ObservationV5,
    ordered_action_semantics: &'a [ActionSemanticV1],
}

fn external_scoring_decision_commitment_v1(
    observation: &ObservationV5,
    action_semantics: &[ActionSemanticV1],
) -> Result<[u8; 32], NativeExternalObservationScoringErrorV1> {
    let payload = serde_json::to_vec(&ExternalScoringCommitmentInputV1 {
        observation,
        ordered_action_semantics: action_semantics,
    })
    .map_err(|_| error_v1(NativeExternalObservationScoringErrorKindV1::CommitmentInvalid))?;
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| error_v1(NativeExternalObservationScoringErrorKindV1::CommitmentInvalid))?;
    let mut hash = Sha256::new();
    hash.update(EXTERNAL_SCORING_COMMITMENT_DOMAIN_V1);
    hash.update(payload_len.to_le_bytes());
    hash.update(payload);
    Ok(hash.finalize().into())
}

impl NativeCheckpointInferenceV1 {
    /// Scores one complete public policy-v5 decision through the unchanged
    /// native Flat V2 checkpoint path.
    pub fn score_external_observation_v1(
        &self,
        observation: &ObservationV5,
        ordered_action_semantics: &[ActionSemanticV1],
    ) -> Result<NativeExternalObservationScoreV1, NativeExternalObservationScoringErrorV1> {
        let decision = encode_external_flat_scoring_decision_owned_v1(
            observation,
            ordered_action_semantics,
        )
        .map_err(|_| error_v1(NativeExternalObservationScoringErrorKindV1::DecisionInvalid))?;
        let decision_commitment_sha256 =
            external_scoring_decision_commitment_v1(observation, ordered_action_semantics)?;
        let output = self
            .score_decision_v1(decision.scoring_view_v1())
            .map_err(|_| error_v1(NativeExternalObservationScoringErrorKindV1::InferenceInvalid))?;
        if output.action_logits().len() != ordered_action_semantics.len()
            || output.action_logits().is_empty()
            || output
                .action_logits()
                .iter()
                .any(|value| !value.is_finite())
            || !output.value().is_finite()
        {
            return Err(error_v1(
                NativeExternalObservationScoringErrorKindV1::InferenceInvalid,
            ));
        }
        Ok(NativeExternalObservationScoreV1 {
            decision_commitment_sha256,
            run_sha256: self.run_sha256(),
            checkpoint_manifest_sha256: self.checkpoint_manifest_sha256(),
            model_parameter_sha256: self.model_parameter_sha256(),
            generation_index: self.generation_index(),
            action_logits: output.action_logits().to_vec(),
            value: output.value(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::flat_policy_v2::{
        FlatDecisionEncoderV2, FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
    };
    use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, decode_genesis_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1, CANONICAL_BURN_DECK_ID};
    use std::time::Duration;

    fn inference_handle_v1() -> NativeCheckpointInferenceV1 {
        let run_bytes = test_fixture_bytes_v2();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
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
            },
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let authority = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let checkpoint =
            decode_genesis_checkpoint_manifest_v3(authority.canonical_bytes(), &payload, &run)
                .unwrap();
        load_native_checkpoint_inference_v1(&run, &checkpoint, &payload).unwrap()
    }

    fn session_decision_v1() -> (
        FastActorSessionV1,
        crate::rl_session::FastActorDecisionV1,
        ObservationV5,
        Vec<ActionSemanticV1>,
    ) {
        let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            93_100,
            0x5eed_0200,
            128,
            16_384,
            [
                CANONICAL_BURN_DECK_ID.to_string(),
                CANONICAL_BURN_DECK_ID.to_string(),
            ],
        )
        .unwrap();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("expected current decision");
        };
        let semantics = session
            .diagnostic_current_action_semantics()
            .expect("current action semantics");
        let mut observation = session.flat_policy_observation_v2(expected).unwrap();
        observation.kernel_version = crate::KERNEL_VERSION.to_string();
        observation.visible_projection_hash =
            crate::rl::visible_projection_hash_v5(&observation).unwrap();
        (session, expected, observation, semantics)
    }

    #[test]
    fn external_observation_score_is_bit_exact_with_session_view() {
        let handle = inference_handle_v1();
        let (session, expected, observation, semantics) = session_decision_v1();
        let external = handle
            .score_external_observation_v1(&observation, &semantics)
            .unwrap();

        let mut encoder = FlatDecisionEncoderV2::default();
        let mut objects = Vec::new();
        let mut relations = Vec::new();
        let mut object_subtypes = Vec::new();
        let mut ability_uses = Vec::new();
        let mut goads = Vec::new();
        let mut completed_dungeons = Vec::new();
        let mut effect_subtype_changes = Vec::new();
        let mut context_path_elements = Vec::new();
        let mut actions = Vec::new();
        let mut action_refs = Vec::new();
        let decision = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected,
                &mut encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                },
            )
            .unwrap();
        let direct = handle
            .score_decision_v1(FlatScoringDecisionViewV2::new(
                &decision.globals,
                &objects,
                &relations,
                &object_subtypes,
                &ability_uses,
                &goads,
                &completed_dungeons,
                &effect_subtype_changes,
                &context_path_elements,
                &actions,
                &action_refs,
            ))
            .unwrap();

        assert_eq!(
            external
                .action_logits()
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            direct
                .action_logits()
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(external.value().to_bits(), direct.value().to_bits());
        assert_eq!(external.run_sha256(), handle.run_sha256());
        assert_eq!(
            external.checkpoint_manifest_sha256(),
            handle.checkpoint_manifest_sha256()
        );
        assert_eq!(
            external.model_parameter_sha256(),
            handle.model_parameter_sha256()
        );
        assert_eq!(external.generation_index(), handle.generation_index());
        assert!(!external.is_input_authority_v1());
        assert!(!external.permits_match_entry_v1());
    }

    #[test]
    fn external_score_commitment_binds_order_and_invalid_decisions_fail_closed() {
        let (_, _, observation, semantics) = session_decision_v1();
        let commitment = external_scoring_decision_commitment_v1(&observation, &semantics).unwrap();
        let mut reordered = semantics.clone();
        reordered.reverse();
        assert_ne!(
            commitment,
            external_scoring_decision_commitment_v1(&observation, &reordered).unwrap()
        );

        let handle = inference_handle_v1();
        let mut bad_hash = observation;
        bad_hash.visible_projection_hash ^= u64::MAX;
        assert_eq!(
            handle
                .score_external_observation_v1(&bad_hash, &semantics)
                .unwrap_err()
                .kind(),
            NativeExternalObservationScoringErrorKindV1::DecisionInvalid
        );
        assert_eq!(
            handle
                .score_external_observation_v1(&bad_hash, &[])
                .unwrap_err()
                .kind(),
            NativeExternalObservationScoringErrorKindV1::DecisionInvalid
        );
    }
}
