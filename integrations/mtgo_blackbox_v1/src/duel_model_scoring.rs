use crate::{
    model_deployment_commitment_v1, score_and_select_external_model_v1,
    AdmittedMtgoDuelPerceptionProfileV1, CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    CheckedUntrustedMtgoModelSelectionV1, MtgoContractErrorV1, MtgoExpectedModelDeploymentV1,
    MtgoExternalObservationScorerV1,
};
use mtg_kernel::rl::ActionSemanticV1;
use sha2::{Digest, Sha256};

const PROFILE_BOUND_DUEL_MODEL_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-profile-bound-duel-model-selection-v1";

/// A model selection over one exact source-bound duel candidate and one exact
/// admitted perception profile.
///
/// The candidate is still checked-untrusted because this offline crate cannot
/// prove that the in-process Windows capture object produced it. This wrapper
/// therefore exposes the selected semantic and commitments for wiring, but it
/// has no raw observation, coordinate, action-intent, input, or event-entry
/// conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1;
/// fn cannot_input(value: &CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1) {
///     let _ = value.input_command();
///     let _ = value.target_point_client_px();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1>();
/// ```
pub struct CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1 {
    candidate: CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    selection: CheckedUntrustedMtgoModelSelectionV1,
    perception_profile_admission_commitment_sha256: String,
    deployment_commitment_sha256: String,
    profile_bound_selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1 {
    pub fn source_candidate_commitment_sha256(&self) -> &str {
        self.candidate.candidate_commitment_sha256()
    }

    pub fn perception_profile_commitment_sha256(&self) -> &str {
        self.candidate.perception_profile_commitment_sha256()
    }

    pub fn perception_profile_admission_commitment_sha256(&self) -> &str {
        &self.perception_profile_admission_commitment_sha256
    }

    pub fn deployment_commitment_sha256(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub fn selection_commitment_sha256(&self) -> &str {
        self.selection.selection_commitment_sha256()
    }

    pub fn profile_bound_selection_commitment_sha256(&self) -> &str {
        &self.profile_bound_selection_commitment_sha256
    }

    pub fn selected_index(&self) -> usize {
        self.selection.selected_index()
    }

    pub fn selected_logit_f32_bits(&self) -> u32 {
        self.selection.selected_logit_f32_bits()
    }

    pub fn value_f32_bits(&self) -> u32 {
        self.selection.value_f32_bits()
    }

    pub fn decision_commitment_sha256(&self) -> &str {
        self.selection.decision_commitment_sha256()
    }

    pub fn selected_semantic(&self) -> &ActionSemanticV1 {
        self.selection.selected_semantic()
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_match_entry(&self) -> bool {
        false
    }

    pub(crate) fn validated_decision_v1(&self) -> &crate::ValidatedMtgoObservedDecisionV1 {
        self.candidate.validated_decision_v1()
    }

    pub(crate) fn base_selection_v1(&self) -> &CheckedUntrustedMtgoModelSelectionV1 {
        &self.selection
    }

    pub(crate) fn source_manifest_sha256_v1(&self) -> &str {
        self.candidate.source_manifest_sha256()
    }

    pub(crate) fn source_canonical_bgra8_sha256_v1(&self) -> &str {
        self.candidate.source_canonical_bgra8_sha256()
    }

    pub(crate) fn source_output_identity_sha256_v1(&self) -> &str {
        self.candidate.source_output_identity_sha256()
    }
}

/// Scores one exact source-bound acting-player duel decision through the
/// existing external-observation scorer. The profile, source candidate,
/// deployment, response, and deterministic selection are committed together.
///
/// This function does not elevate the checked-untrusted source to live capture
/// authority. A Windows-side consumer must separately preserve possession of
/// the opaque in-process DXGI frame through perception and action grounding.
pub fn score_and_select_profile_bound_duel_candidate_v1<S: MtgoExternalObservationScorerV1>(
    candidate: CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1, MtgoContractErrorV1> {
    if candidate.perception_profile_commitment_sha256()
        != profile.perception_profile_commitment_sha256()
    {
        return Err(MtgoContractErrorV1::new(
            "profile_bound_duel_scoring_profile_mismatch",
            "source candidate and admitted perception profile differ",
        ));
    }
    let deployment_commitment_sha256 = model_deployment_commitment_v1(deployment)?;
    let selection =
        score_and_select_external_model_v1(candidate.validated_decision_v1(), deployment, scorer)?;
    if selection.decision_commitment_sha256() != candidate.base_decision_commitment_sha256() {
        return Err(MtgoContractErrorV1::new(
            "profile_bound_duel_scoring_decision_mismatch",
            "model selection does not bind the source candidate decision",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(PROFILE_BOUND_DUEL_MODEL_SELECTION_DOMAIN_V1);
    for part in [
        candidate.candidate_commitment_sha256().as_bytes(),
        profile.perception_profile_commitment_sha256().as_bytes(),
        profile.admission_commitment_sha256().as_bytes(),
        deployment_commitment_sha256.as_bytes(),
        selection.selection_commitment_sha256().as_bytes(),
        b"checked_untrusted_no_input_or_event_entry_authority",
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    Ok(CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1 {
        candidate,
        selection,
        perception_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        deployment_commitment_sha256,
        profile_bound_selection_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

#[cfg(test)]
pub(crate) struct DeterministicProfileBoundTestScorerV1 {
    pub corrupt_request_commitment: bool,
}

#[cfg(test)]
impl MtgoExternalObservationScorerV1 for DeterministicProfileBoundTestScorerV1 {
    fn score_observation_v1(
        &mut self,
        request: &crate::MtgoExternalScoringRequestV1,
        _observation: &mtg_kernel::rl::ObservationV5,
        ordered_legal_actions: &[ActionSemanticV1],
    ) -> Result<crate::MtgoExternalModelScoreResponseV1, MtgoContractErrorV1> {
        let mut request_commitment_sha256 = crate::scoring_request_commitment_v1(request)?;
        if self.corrupt_request_commitment {
            let replacement = if request_commitment_sha256.starts_with('0') {
                "1"
            } else {
                "0"
            };
            request_commitment_sha256.replace_range(0..1, replacement);
        }
        Ok(crate::MtgoExternalModelScoreResponseV1 {
            schema_version: crate::MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
            request_commitment_sha256,
            logits_f32_bits: (0..ordered_legal_actions.len())
                .map(|index| (index as f32).to_bits())
                .collect(),
            value_f32_bits: 0.25_f32.to_bits(),
        })
    }
}

#[cfg(test)]
pub(crate) fn profile_bound_duel_model_selection_for_test_v1(
) -> CheckedUntrustedMtgoProfileBoundDuelModelSelectionV1 {
    let payload = crate::duel_perception_runtime_profile_payload_for_test_v1();
    let candidate_profile =
        crate::check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
    let candidate =
        crate::checked_untrusted_dxgi_decision_candidate_for_test_v1(&candidate_profile);
    let admitted = crate::duel_perception_profile_admitted_for_test_v1(
        crate::check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
    );
    let deployment: MtgoExpectedModelDeploymentV1 = serde_json::from_str(include_str!(
        "../fixtures/provisional_promoted2_mtgo_deployment_20260810_v1.json"
    ))
    .unwrap();
    let mut scorer = DeterministicProfileBoundTestScorerV1 {
        corrupt_request_commitment: false,
    };
    score_and_select_profile_bound_duel_candidate_v1(candidate, &admitted, &deployment, &mut scorer)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_external_scoring_request_v1, check_untrusted_duel_perception_runtime_profile_v1,
        checked_untrusted_dxgi_decision_candidate_for_test_v1,
        duel_perception_profile_admitted_for_test_v1,
        duel_perception_runtime_profile_payload_for_test_v1,
    };

    fn deployment_v1() -> MtgoExpectedModelDeploymentV1 {
        serde_json::from_str(include_str!(
            "../fixtures/provisional_promoted2_mtgo_deployment_20260810_v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn exact_profile_source_deployment_and_selection_bind_without_input_authority() {
        let payload = duel_perception_runtime_profile_payload_for_test_v1();
        let candidate_profile =
            check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
        let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&candidate_profile);
        let admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
        );
        let deployment = deployment_v1();
        let expected_deployment_commitment = model_deployment_commitment_v1(&deployment).unwrap();
        let mut scorer = DeterministicProfileBoundTestScorerV1 {
            corrupt_request_commitment: false,
        };
        let selected = score_and_select_profile_bound_duel_candidate_v1(
            candidate,
            &admitted,
            &deployment,
            &mut scorer,
        )
        .unwrap();

        assert_eq!(selected.selected_index(), 1);
        assert!(matches!(
            selected.selected_semantic(),
            ActionSemanticV1::PlayLand { .. }
        ));
        assert_eq!(
            selected.perception_profile_commitment_sha256(),
            admitted.perception_profile_commitment_sha256()
        );
        assert_eq!(
            selected.perception_profile_admission_commitment_sha256(),
            admitted.admission_commitment_sha256()
        );
        assert_eq!(
            selected.deployment_commitment_sha256(),
            expected_deployment_commitment
        );
        assert_eq!(
            selected.profile_bound_selection_commitment_sha256().len(),
            64
        );
        assert!(!selected.safe_for_live_input());
        assert!(!selected.permits_match_entry());
    }

    #[test]
    fn profile_and_score_response_substitution_fail_closed() {
        let payload = duel_perception_runtime_profile_payload_for_test_v1();
        let candidate_profile =
            check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
        let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&candidate_profile);
        let mut different_payload = payload.clone();
        different_payload.classifier_assets_manifest_sha256 = "8".repeat(64);
        let admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(different_payload).unwrap(),
        );
        let deployment = deployment_v1();
        let mut scorer = DeterministicProfileBoundTestScorerV1 {
            corrupt_request_commitment: false,
        };
        assert_eq!(
            score_and_select_profile_bound_duel_candidate_v1(
                candidate,
                &admitted,
                &deployment,
                &mut scorer,
            )
            .err()
            .unwrap()
            .code(),
            "profile_bound_duel_scoring_profile_mismatch"
        );

        let candidate_profile =
            check_untrusted_duel_perception_runtime_profile_v1(payload.clone()).unwrap();
        let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&candidate_profile);
        let admitted = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
        );
        let mut scorer = DeterministicProfileBoundTestScorerV1 {
            corrupt_request_commitment: true,
        };
        assert_eq!(
            score_and_select_profile_bound_duel_candidate_v1(
                candidate,
                &admitted,
                &deployment,
                &mut scorer,
            )
            .err()
            .unwrap()
            .code(),
            "external_scoring_response_request_mismatch"
        );
    }

    #[test]
    fn request_builder_still_binds_exact_validated_candidate_decision() {
        let profile = check_untrusted_duel_perception_runtime_profile_v1(
            duel_perception_runtime_profile_payload_for_test_v1(),
        )
        .unwrap();
        let candidate = checked_untrusted_dxgi_decision_candidate_for_test_v1(&profile);
        let deployment = deployment_v1();
        let request =
            build_external_scoring_request_v1(candidate.validated_decision_v1(), &deployment)
                .unwrap();
        assert_eq!(
            request.decision_commitment_sha256,
            candidate.base_decision_commitment_sha256()
        );
        assert_eq!(
            request.action_count as usize,
            candidate.validated_decision_v1().legal_actions().len()
        );
    }
}
