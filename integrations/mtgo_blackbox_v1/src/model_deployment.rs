use crate::{
    model_deployment_commitment_v1, score_and_select_external_model_v1,
    score_and_select_profile_bound_duel_candidate_v1, AdmittedMtgoDuelPerceptionProfileV1,
    CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1, CheckedUntrustedMtgoModelSelectionV1,
    CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1, MtgoContractErrorV1,
    MtgoExpectedModelDeploymentV1, MtgoNativeCheckpointObservationScorerV1,
    ValidatedMtgoObservedDecisionV1,
};
use mtg_kernel::native_checkpoint_inference_v1::{
    load_native_checkpoint_inference_v1, NativeCheckpointInferenceV1,
};
use mtg_kernel::native_training_store_resume_v2::load_native_training_boundary_v2;
use mtg_kernel::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use mtg_kernel::native_training_store_run_v2::decode_train_run_v2;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};
use std::fs;
use std::path::Path;

const MAX_RUN_JSON_BYTES_V1: u64 = 1_048_576;
const NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_DOMAIN_V1: &[u8] =
    b"mtgo-native-checkpoint-competitive-capabilities-v1";

pub const MTGO_NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_SCHEMA_V1: u32 = 1;

/// Code-derived inventory of the competitive decision heads exposed by one
/// exact loaded checkpoint deployment.
///
/// This record is telemetry only. It cannot load a checkpoint, score a
/// decision, create an operator, authorize input, or enter an event. The
/// opaque loaded deployment remains the authority for its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
    pub schema_version: u32,
    pub deployment_commitment_sha256: String,
    pub native_duel_action_interface_present: bool,
    pub native_pregame_interface_present: bool,
    pub terminal_outcome_trained_pregame_head_present: bool,
    pub native_sideboard_interface_present: bool,
    pub terminal_outcome_trained_sideboard_head_present: bool,
    pub native_changed_sideboard_action_present: bool,
    pub native_unchanged_sideboard_action_present: bool,
    pub capabilities_commitment_sha256: String,
}

impl MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
    pub fn pregame_head_ready_v1(&self) -> bool {
        self.native_pregame_interface_present && self.terminal_outcome_trained_pregame_head_present
    }

    pub fn sideboard_head_ready_v1(&self) -> bool {
        self.native_sideboard_interface_present
            && self.terminal_outcome_trained_sideboard_head_present
            && self.native_changed_sideboard_action_present
            && self.native_unchanged_sideboard_action_present
    }
}

/// One exact native checkpoint deployment loaded through the validated Store
/// walk and rebound to an independently supplied MTGO deployment identity.
///
/// This value is move-only and has no input, capture, account, event-entry, or
/// match authority. Its only operational surface is creation of the existing
/// observation scorer.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::LoadedMtgoNativeCheckpointDeploymentV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<LoadedMtgoNativeCheckpointDeploymentV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::LoadedMtgoNativeCheckpointDeploymentV1;
/// use serde::Serialize;
/// fn require_serialize<T: Serialize>() {}
/// require_serialize::<LoadedMtgoNativeCheckpointDeploymentV1>();
/// ```
pub struct LoadedMtgoNativeCheckpointDeploymentV1 {
    expected: MtgoExpectedModelDeploymentV1,
    deployment_commitment_sha256: String,
    competitive_capabilities: MtgoNativeCheckpointCompetitiveCapabilitiesV1,
    inference: NativeCheckpointInferenceV1,
}

impl Debug for LoadedMtgoNativeCheckpointDeploymentV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedMtgoNativeCheckpointDeploymentV1")
            .field("deployment_id", &self.expected.deployment_id)
            .field(
                "generation_index",
                &self.expected.checkpoint.generation_index,
            )
            .field(
                "deployment_commitment_sha256",
                &self.deployment_commitment_sha256,
            )
            .field(
                "competitive_capabilities_commitment_sha256",
                &self.competitive_capabilities.capabilities_commitment_sha256,
            )
            .finish_non_exhaustive()
    }
}

impl LoadedMtgoNativeCheckpointDeploymentV1 {
    pub fn deployment_id(&self) -> &str {
        &self.expected.deployment_id
    }

    pub fn generation_index(&self) -> u64 {
        self.expected.checkpoint.generation_index
    }

    pub fn deployment_commitment_sha256(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub fn competitive_capabilities_v1(&self) -> &MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
        &self.competitive_capabilities
    }

    pub fn scorer_v1(
        &self,
    ) -> Result<MtgoNativeCheckpointObservationScorerV1<'_>, MtgoContractErrorV1> {
        MtgoNativeCheckpointObservationScorerV1::new_v1(&self.inference, &self.expected)
    }

    /// Scores one exact validated observation and complete ordered legal-action
    /// vector through this deployment, then applies the adapter's deterministic
    /// selection rule. The result remains coordinate-free and has no live-input
    /// or match-entry authority.
    pub fn score_validated_decision_v1(
        &self,
        decision: &ValidatedMtgoObservedDecisionV1,
    ) -> Result<CheckedUntrustedMtgoModelSelectionV1, MtgoContractErrorV1> {
        let mut scorer = self.scorer_v1()?;
        score_and_select_external_model_v1(decision, &self.expected, &mut scorer)
    }

    /// Scores one exact profile-bound visible duel candidate through this
    /// loaded deployment. The independently supplied deployment identity never
    /// leaves the opaque loaded value, so callers cannot accidentally score a
    /// live candidate against a reconstructed or crossed deployment record.
    pub fn score_profile_bound_duel_candidate_v1(
        &self,
        candidate: CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
        profile: &AdmittedMtgoDuelPerceptionProfileV1,
    ) -> Result<CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1, MtgoContractErrorV1> {
        let mut scorer = self.scorer_v1()?;
        score_and_select_profile_bound_duel_candidate_v1(
            candidate,
            profile,
            &self.expected,
            &mut scorer,
        )
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_match_entry(&self) -> bool {
        false
    }
}

/// Loads one exact checkpoint from a complete native Store root.
///
/// The Store loader validates `run.json` and the complete Store through its
/// latest pointer, then rewalks every boundary through the requested
/// generation before returning its checkpoint manifest and complete payload.
/// The inference handle is constructed only after that walk. The final scorer
/// constructor then compares all six checkpoint identity fields and the
/// scorer-contract digest with the independently supplied deployment record.
pub fn load_mtgo_native_checkpoint_deployment_v1(
    store_root: impl AsRef<Path>,
    expected: MtgoExpectedModelDeploymentV1,
) -> Result<LoadedMtgoNativeCheckpointDeploymentV1, MtgoContractErrorV1> {
    let deployment_commitment_sha256 = model_deployment_commitment_v1(&expected)?;
    let competitive_capabilities =
        current_native_checkpoint_competitive_capabilities_v1(&deployment_commitment_sha256)?;
    let store_root = store_root.as_ref();
    let run_path = store_root.join("run.json");
    let run_metadata = fs::metadata(&run_path).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_run_metadata_failed",
            "the selected Store run.json could not be inspected",
        )
    })?;
    if !run_metadata.is_file() || run_metadata.len() > MAX_RUN_JSON_BYTES_V1 {
        return Err(error_v1(
            "mtgo_model_deployment_run_size_invalid",
            "the selected Store run.json is absent or exceeds the fixed byte cap",
        ));
    }
    let run_bytes = fs::read(&run_path).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_run_read_failed",
            "the selected Store run.json could not be read",
        )
    })?;
    let run = decode_train_run_v2(&run_bytes).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_run_invalid",
            "the selected Store run.json failed the native contract",
        )
    })?;
    let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_store_invalid",
            "the selected native Store root failed validation",
        )
    })?;
    let boundary =
        load_native_training_boundary_v2(&root, &run, expected.checkpoint.generation_index)
            .map_err(|_| {
                error_v1(
                    "mtgo_model_deployment_boundary_invalid",
                    "the selected checkpoint boundary failed the complete Store walk",
                )
            })?;
    let inference =
        load_native_checkpoint_inference_v1(&run, boundary.checkpoint(), boundary.payload())
            .map_err(|_| {
                error_v1(
                    "mtgo_model_deployment_inference_invalid",
                    "the selected checkpoint could not construct the native inference handle",
                )
            })?;
    MtgoNativeCheckpointObservationScorerV1::new_v1(&inference, &expected)?;

    Ok(LoadedMtgoNativeCheckpointDeploymentV1 {
        expected,
        deployment_commitment_sha256,
        competitive_capabilities,
        inference,
    })
}

/// Rechecks a serialized capability inventory. Passing this check does not
/// prove that it came from a loaded checkpoint and grants no authority.
pub fn validate_native_checkpoint_competitive_capabilities_v1(
    value: &MtgoNativeCheckpointCompetitiveCapabilitiesV1,
) -> Result<(), MtgoContractErrorV1> {
    if value.schema_version != MTGO_NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_SCHEMA_V1 {
        return Err(error_v1(
            "mtgo_checkpoint_capabilities_schema_invalid",
            "the checkpoint capability inventory uses an unsupported schema",
        ));
    }
    if !is_lower_sha256_v1(&value.deployment_commitment_sha256)
        || !is_lower_sha256_v1(&value.capabilities_commitment_sha256)
    {
        return Err(error_v1(
            "mtgo_checkpoint_capabilities_digest_invalid",
            "the checkpoint capability inventory contains an invalid digest",
        ));
    }
    if !value.native_pregame_interface_present
        && value.terminal_outcome_trained_pregame_head_present
    {
        return Err(error_v1(
            "mtgo_checkpoint_pregame_capabilities_inconsistent",
            "a trained pregame head cannot be present without its native interface",
        ));
    }
    if !value.native_sideboard_interface_present
        && (value.terminal_outcome_trained_sideboard_head_present
            || value.native_changed_sideboard_action_present
            || value.native_unchanged_sideboard_action_present)
    {
        return Err(error_v1(
            "mtgo_checkpoint_sideboard_capabilities_inconsistent",
            "sideboard training or actions cannot be present without the native interface",
        ));
    }
    let expected = native_checkpoint_competitive_capabilities_commitment_v1(value)?;
    if value.capabilities_commitment_sha256 != expected {
        return Err(error_v1(
            "mtgo_checkpoint_capabilities_commitment_mismatch",
            "the checkpoint capability inventory commitment does not match its fields",
        ));
    }
    Ok(())
}

fn current_native_checkpoint_competitive_capabilities_v1(
    deployment_commitment_sha256: &str,
) -> Result<MtgoNativeCheckpointCompetitiveCapabilitiesV1, MtgoContractErrorV1> {
    if !is_lower_sha256_v1(deployment_commitment_sha256) {
        return Err(error_v1(
            "mtgo_checkpoint_capabilities_deployment_invalid",
            "the checkpoint capability inventory requires an exact deployment commitment",
        ));
    }
    let mut value = MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
        schema_version: MTGO_NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_SCHEMA_V1,
        deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
        native_duel_action_interface_present: true,
        native_pregame_interface_present: false,
        terminal_outcome_trained_pregame_head_present: false,
        native_sideboard_interface_present: false,
        terminal_outcome_trained_sideboard_head_present: false,
        native_changed_sideboard_action_present: false,
        native_unchanged_sideboard_action_present: false,
        capabilities_commitment_sha256: String::new(),
    };
    value.capabilities_commitment_sha256 =
        native_checkpoint_competitive_capabilities_commitment_v1(&value)?;
    validate_native_checkpoint_competitive_capabilities_v1(&value)?;
    Ok(value)
}

/// Recomputes the domain-separated telemetry commitment. This does not prove
/// that a record came from a loaded checkpoint and grants no authority.
pub fn native_checkpoint_competitive_capabilities_commitment_v1(
    value: &MtgoNativeCheckpointCompetitiveCapabilitiesV1,
) -> Result<String, MtgoContractErrorV1> {
    let mut payload = value.clone();
    payload.capabilities_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&payload).map_err(|_| {
        error_v1(
            "mtgo_checkpoint_capabilities_serialize_failed",
            "the checkpoint capability inventory could not be serialized",
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_DOMAIN_V1);
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn error_v1(code: &'static str, detail: &'static str) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_external_scoring_request_v1, local_metadata_commitment_v1,
        make_scored_offline_intent_v1, payload_leaf_inventory_v1, validate_observed_decision_v1,
        MtgoEvidenceSourceV1, MtgoExternalObservationScorerV1, MtgoLeafProvenanceV1,
        MtgoMockFrameV1, MtgoObjectBindingV1, MtgoObservedDecisionV1, MtgoPublicDerivationV1,
        MtgoRectPxV1, MtgoSemanticDecisionPayloadV1, MtgoVisibleEvidenceV1,
        MTGO_OBSERVED_DECISION_SCHEMA_V1,
    };
    use mtg_kernel::rl::ActionSemanticV1;
    use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};

    fn provisional_deployment_v1() -> MtgoExpectedModelDeploymentV1 {
        serde_json::from_str(include_str!(
            "../fixtures/provisional_promoted2_mtgo_deployment_20260810_v1.json"
        ))
        .expect("checked-in provisional deployment must parse")
    }

    fn fixture_digest_v1(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn external_probe_record_v1() -> MtgoObservedDecisionV1 {
        let (seed, observation, pass, land) = (1..=128)
            .find_map(|seed| {
                let session = RlEpisodeSessionV1::reset_with_limits(7, seed, 128, 16_384);
                let RlSessionResponseV1::Decision(decision) = session.current_response() else {
                    return None;
                };
                let pass = decision
                    .legal_actions
                    .iter()
                    .find(|action| matches!(action.semantic, ActionSemanticV1::Pass { .. }))?
                    .semantic
                    .clone();
                let land = decision
                    .legal_actions
                    .iter()
                    .find(|action| matches!(action.semantic, ActionSemanticV1::PlayLand { .. }))?
                    .semantic
                    .clone();
                Some((seed, (*decision.observation).clone(), pass, land))
            })
            .expect("a deterministic opening must expose Pass and PlayLand");
        assert_eq!(seed, 1);
        let source = match &land {
            ActionSemanticV1::PlayLand { source, .. } => source.clone(),
            _ => unreachable!(),
        };
        let payload = MtgoSemanticDecisionPayloadV1 {
            observation,
            legal_actions: vec![pass, land],
            object_bindings: vec![MtgoObjectBindingV1 {
                adapter_object_id: "deployment-probe:hand:0".to_owned(),
                kernel_ref: source,
            }],
        };
        let provenance = payload_leaf_inventory_v1(&payload)
            .unwrap()
            .into_iter()
            .filter(|leaf| leaf.requires_visible_evidence)
            .map(|leaf| MtgoLeafProvenanceV1 {
                json_pointer: leaf.json_pointer,
                value_sha256: leaf.value_sha256,
                evidence_ids: vec![20],
                confidence_bps: 10_000,
            })
            .collect();
        let local_metadata_sha256 = local_metadata_commitment_v1(&payload).unwrap();
        MtgoObservedDecisionV1 {
            schema_version: MTGO_OBSERVED_DECISION_SCHEMA_V1,
            decision_id: "provisional-deployment-external-probe".to_owned(),
            frame_id: 1,
            payload,
            frames: vec![MtgoMockFrameV1 {
                frame_id: 1,
                sequence: 1,
                sha256: fixture_digest_v1('1'),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 1_920,
                    height: 1_080,
                },
            }],
            evidence: vec![
                MtgoVisibleEvidenceV1 {
                    evidence_id: 10,
                    sequence: 1,
                    source: MtgoEvidenceSourceV1::FrameRegion {
                        frame_id: 1,
                        rect: MtgoRectPxV1 {
                            x: 0,
                            y: 0,
                            width: 1_920,
                            height: 1_080,
                        },
                        content_sha256: fixture_digest_v1('2'),
                    },
                },
                MtgoVisibleEvidenceV1 {
                    evidence_id: 20,
                    sequence: 2,
                    source: MtgoEvidenceSourceV1::DerivedPublicFact {
                        parent_evidence_ids: vec![10],
                        derivation: MtgoPublicDerivationV1::PublicStateProjection,
                    },
                },
            ],
            provenance,
            local_metadata_sha256,
            readiness: crate::MtgoDecisionReadinessV1 {
                observation_complete: true,
                legal_action_set_complete: true,
                client_prompt_reconciled: true,
            },
        }
    }

    #[test]
    fn provisional_promoted2_identity_is_exact_and_non_actuating() {
        let deployment = provisional_deployment_v1();
        assert_eq!(
            deployment.deployment_id,
            "promoted2-generation384-provisional-mtgo-wiring"
        );
        assert_eq!(deployment.checkpoint.generation_index, 384);
        assert_eq!(
            deployment.checkpoint.run_sha256,
            "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae"
        );
        assert_eq!(
            deployment.checkpoint.model_parameter_sha256,
            "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d"
        );
        assert_eq!(
            model_deployment_commitment_v1(&deployment).unwrap(),
            "1a3288a47a96989b4f4eddd22b0fa2f8db6b083420fd6329e385a7f50641849e"
        );
    }

    #[test]
    fn malformed_expected_identity_fails_before_store_access() {
        let mut deployment = provisional_deployment_v1();
        deployment.checkpoint.run_sha256 = "not-a-digest".to_owned();
        let error =
            load_mtgo_native_checkpoint_deployment_v1("this-path-must-not-be-read", deployment)
                .expect_err("invalid expected identity must fail");
        assert_eq!(error.code(), "external_scoring_deployment_hash_invalid");
    }

    #[test]
    fn current_loaded_checkpoint_capability_inventory_is_duel_only_and_bound() {
        let deployment_commitment = fixture_digest_v1('a');
        let capabilities =
            current_native_checkpoint_competitive_capabilities_v1(&deployment_commitment).unwrap();
        assert_eq!(
            capabilities.deployment_commitment_sha256,
            deployment_commitment
        );
        assert!(capabilities.native_duel_action_interface_present);
        assert!(!capabilities.pregame_head_ready_v1());
        assert!(!capabilities.sideboard_head_ready_v1());
        validate_native_checkpoint_competitive_capabilities_v1(&capabilities).unwrap();

        let mut crossed = capabilities.clone();
        crossed.deployment_commitment_sha256 = fixture_digest_v1('b');
        assert_eq!(
            validate_native_checkpoint_competitive_capabilities_v1(&crossed)
                .unwrap_err()
                .code(),
            "mtgo_checkpoint_capabilities_commitment_mismatch"
        );

        let mut impossible = capabilities;
        impossible.terminal_outcome_trained_sideboard_head_present = true;
        assert_eq!(
            validate_native_checkpoint_competitive_capabilities_v1(&impossible)
                .unwrap_err()
                .code(),
            "mtgo_checkpoint_sideboard_capabilities_inconsistent"
        );
    }

    #[test]
    #[ignore = "requires MTGO_NATIVE_STORE_ROOT_V1 pointing to the exact 2.33 GB native Store"]
    fn real_provisional_checkpoint_scores_external_public_decision() {
        let store_root = std::env::var_os("MTGO_NATIVE_STORE_ROOT_V1")
            .expect("the opt-in real Store test requires MTGO_NATIVE_STORE_ROOT_V1");
        let deployment = provisional_deployment_v1();
        let loaded =
            load_mtgo_native_checkpoint_deployment_v1(store_root, deployment.clone()).unwrap();
        let decision = validate_observed_decision_v1(external_probe_record_v1()).unwrap();
        let request = build_external_scoring_request_v1(&decision, &deployment).unwrap();
        let mut scorer = loaded.scorer_v1().unwrap();
        let response = scorer
            .score_observation_v1(&request, decision.observation(), decision.legal_actions())
            .unwrap();
        assert_eq!(response.logits_f32_bits, [3_245_259_304, 1_067_419_264]);
        assert_eq!(response.value_f32_bits, 1_041_311_617);
        let selection = loaded.score_validated_decision_v1(&decision).unwrap();
        assert_eq!(selection.selected_index(), 1);
        assert_eq!(selection.selected_logit_f32_bits(), 1_067_419_264);
        assert_eq!(selection.value_f32_bits(), 1_041_311_617);
        assert_eq!(
            selection.decision_commitment_sha256(),
            decision.decision_commitment_sha256()
        );
        assert!(!selection.safe_for_live_input());
        let intent = make_scored_offline_intent_v1(&decision, &selection).unwrap();
        assert_eq!(intent.selected_index, 1);
        assert_eq!(intent.semantic, decision.legal_actions()[1]);
        assert!(!loaded.safe_for_live_input());
        assert!(!loaded.permits_match_entry());
    }
}
