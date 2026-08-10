use crate::{
    validate_observed_decision_v1, CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    CheckedUntrustedMtgoObservationReconstructionAuditV1, MtgoCalibrationCaptureRoleV1,
    MtgoContractErrorV1, MtgoDxgiCaptureRoleV2, MtgoObservedDecisionV1,
    MtgoReconstructionTopologyV1, ValidatedMtgoObservedDecisionV1,
};
use sha2::{Digest, Sha256};

const DXGI_OBSERVED_DECISION_CANDIDATE_DOMAIN_V1: &[u8] =
    b"mtgo-dxgi-observed-decision-candidate-v1";

/// Exact source-chain binding for a structurally valid acting-player duel
/// decision candidate.
///
/// The wrapper intentionally implements neither `Debug`, `Clone`, nor serde.
/// It exposes commitments and fixed false authority flags only. In particular,
/// it has no observation, legal-action, model-request, coordinate, or input
/// accessor.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1;
/// fn cannot_score(value: &CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1) {
///     let _ = value.observation();
///     let _ = value.legal_actions();
/// }
/// ```
pub struct CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1 {
    frame_id: u64,
    frame_sequence: u64,
    source_manifest_sha256: String,
    source_canonical_bgra8_sha256: String,
    source_output_identity_sha256: String,
    reconstruction_audit_commitment_sha256: String,
    perception_profile_commitment_sha256: String,
    base_decision_commitment_sha256: String,
    candidate_commitment_sha256: String,
    validated_decision: ValidatedMtgoObservedDecisionV1,
}

impl CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1 {
    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_canonical_bgra8_sha256(&self) -> &str {
        &self.source_canonical_bgra8_sha256
    }

    pub fn source_output_identity_sha256(&self) -> &str {
        &self.source_output_identity_sha256
    }

    pub fn reconstruction_audit_commitment_sha256(&self) -> &str {
        &self.reconstruction_audit_commitment_sha256
    }

    pub fn perception_profile_commitment_sha256(&self) -> &str {
        &self.perception_profile_commitment_sha256
    }

    pub fn base_decision_commitment_sha256(&self) -> &str {
        &self.base_decision_commitment_sha256
    }

    pub fn candidate_commitment_sha256(&self) -> &str {
        &self.candidate_commitment_sha256
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }

    pub(crate) fn validated_decision_v1(&self) -> &ValidatedMtgoObservedDecisionV1 {
        &self.validated_decision
    }
}

pub fn check_untrusted_dxgi_observed_decision_candidate_v1(
    source: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    audit: &CheckedUntrustedMtgoObservationReconstructionAuditV1,
    perception_profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    record: MtgoObservedDecisionV1,
) -> Result<CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1, MtgoContractErrorV1> {
    let perception_profile_commitment_sha256 = perception_profile.profile_commitment_sha256();
    if source.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerDuel
        || audit.topology() != MtgoReconstructionTopologyV1::TwoPlayerDuel
        || audit.capture_role() != MtgoCalibrationCaptureRoleV1::ActingPlayerDuel
    {
        return Err(error_v1(
            "dxgi_observed_decision_role",
            "an acting-player duel source and source-bound duel audit are required",
        ));
    }
    if source.executable_sha256() != perception_profile.executable_sha256()
        || source.signer_thumbprint() != perception_profile.signer_thumbprint()
        || source.signer_subject_sha256() != perception_profile.signer_subject_sha256()
        || source.dpi() != perception_profile.dpi()
        || source.client_size_px() != perception_profile.client_size_px()
        || source.output_identity_sha256() != perception_profile.output_identity_sha256()
        || source.game_format() != Some(perception_profile.game_format())
    {
        return Err(error_v1(
            "dxgi_observed_decision_perception_profile_source",
            "DXGI source identity, geometry, output, and format must match the perception profile",
        ));
    }
    if audit.source_manifest_sha256() != source.manifest_sha256()
        || audit.source_frame_sha256() != source.canonical_bgra8_sha256()
        || audit.source_client_size_px() != source.client_size_px()
        || audit.source_frame_sequence() == 0
    {
        return Err(error_v1(
            "dxgi_observed_decision_audit_source",
            "reconstruction audit must bind the exact checked DXGI source",
        ));
    }
    if !audit.observation_complete()
        || !audit.legal_action_set_complete()
        || !audit.blocking_groups().is_empty()
        || audit.ready_for_model_scoring()
    {
        return Err(error_v1(
            "dxgi_observed_decision_audit_incomplete",
            "the complete untrusted readiness inventory is required without a scoring claim",
        ));
    }
    if record.frames.len() != 1 {
        return Err(error_v1(
            "dxgi_observed_decision_frame_count",
            "v1 source binding requires exactly one current DXGI frame",
        ));
    }
    let frame = &record.frames[0];
    if record.frame_id != frame.frame_id
        || frame.sequence != audit.source_frame_sequence()
        || frame.sha256 != source.canonical_bgra8_sha256()
        || frame.client_bounds.x != 0
        || frame.client_bounds.y != 0
        || frame.client_bounds.width != source.client_size_px().width
        || frame.client_bounds.height != source.client_size_px().height
    {
        return Err(error_v1(
            "dxgi_observed_decision_frame_source",
            "decision frame identity, sequence, hash, and client bounds must match the DXGI source",
        ));
    }

    let frame_id = frame.frame_id;
    let frame_sequence = frame.sequence;
    let validated = validate_observed_decision_v1(record)?;
    let parts: [&[u8]; 9] = [
        source.manifest_sha256().as_bytes(),
        source.canonical_bgra8_sha256().as_bytes(),
        source.output_identity_sha256().as_bytes(),
        b"acting_player_duel",
        &source.captured_at_unix_millis().to_le_bytes(),
        audit.audit_commitment_sha256().as_bytes(),
        perception_profile_commitment_sha256.as_bytes(),
        validated.decision_commitment_sha256().as_bytes(),
        b"checked_untrusted_no_scoring_or_input_authority",
    ];
    let mut hasher = Sha256::new();
    hasher.update(DXGI_OBSERVED_DECISION_CANDIDATE_DOMAIN_V1);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }

    Ok(CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1 {
        frame_id,
        frame_sequence,
        source_manifest_sha256: source.manifest_sha256().to_owned(),
        source_canonical_bgra8_sha256: source.canonical_bgra8_sha256().to_owned(),
        source_output_identity_sha256: source.output_identity_sha256().to_owned(),
        reconstruction_audit_commitment_sha256: audit.audit_commitment_sha256().to_owned(),
        perception_profile_commitment_sha256: perception_profile_commitment_sha256.to_owned(),
        base_decision_commitment_sha256: validated.decision_commitment_sha256().to_owned(),
        candidate_commitment_sha256: format!("{:x}", hasher.finalize()),
        validated_decision: validated,
    })
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
pub(crate) fn dxgi_observed_decision_record_for_test_v1(
    source: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    sequence: u64,
) -> MtgoObservedDecisionV1 {
    use crate::{
        local_metadata_commitment_v1, payload_leaf_inventory_v1, MtgoDecisionReadinessV1,
        MtgoEvidenceSourceV1, MtgoLeafProvenanceV1, MtgoMockFrameV1, MtgoObjectBindingV1,
        MtgoPublicDerivationV1, MtgoRectPxV1, MtgoSemanticDecisionPayloadV1, MtgoVisibleEvidenceV1,
        MTGO_OBSERVED_DECISION_SCHEMA_V1,
    };
    use mtg_kernel::rl::ActionSemanticV1;
    use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};

    let (observation, pass, land) = (1..=128)
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
            Some(((*decision.observation).clone(), pass, land))
        })
        .expect("a deterministic test seed supplies Pass and PlayLand");
    let source_ref = match &land {
        ActionSemanticV1::PlayLand { source, .. } => source.clone(),
        _ => unreachable!(),
    };
    let payload = MtgoSemanticDecisionPayloadV1 {
        observation,
        legal_actions: vec![pass, land],
        object_bindings: vec![MtgoObjectBindingV1 {
            adapter_object_id: "dxgi-candidate:hand:0".to_owned(),
            kernel_ref: source_ref,
        }],
    };
    let provenance = payload_leaf_inventory_v1(&payload)
        .expect("test payload inventory is valid")
        .into_iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .map(|leaf| MtgoLeafProvenanceV1 {
            json_pointer: leaf.json_pointer,
            value_sha256: leaf.value_sha256,
            evidence_ids: vec![20, 30, 40, 50],
            confidence_bps: 10_000,
        })
        .collect();
    let local_metadata_sha256 =
        local_metadata_commitment_v1(&payload).expect("test local metadata is valid");
    MtgoObservedDecisionV1 {
        schema_version: MTGO_OBSERVED_DECISION_SCHEMA_V1,
        decision_id: "dxgi-source-bound-decision-candidate-v1".to_owned(),
        frame_id: 1,
        payload,
        frames: vec![MtgoMockFrameV1 {
            frame_id: 1,
            sequence,
            sha256: source.canonical_bgra8_sha256().to_owned(),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: source.client_size_px().width,
                height: source.client_size_px().height,
            },
        }],
        evidence: vec![
            MtgoVisibleEvidenceV1 {
                evidence_id: 10,
                sequence,
                source: MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: 1,
                    rect: MtgoRectPxV1 {
                        x: 0,
                        y: 0,
                        width: source.client_size_px().width,
                        height: source.client_size_px().height,
                    },
                    content_sha256: "5".repeat(64),
                },
            },
            MtgoVisibleEvidenceV1 {
                evidence_id: 20,
                sequence: sequence + 1,
                source: MtgoEvidenceSourceV1::DerivedPublicFact {
                    parent_evidence_ids: vec![10],
                    derivation: MtgoPublicDerivationV1::PublicStateProjection,
                },
            },
            MtgoVisibleEvidenceV1 {
                evidence_id: 30,
                sequence: sequence + 2,
                source: MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: 1,
                    rect: MtgoRectPxV1 {
                        x: 10,
                        y: 10,
                        width: 100,
                        height: 40,
                    },
                    content_sha256: "6".repeat(64),
                },
            },
            MtgoVisibleEvidenceV1 {
                evidence_id: 40,
                sequence: sequence + 3,
                source: MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: 1,
                    rect: MtgoRectPxV1 {
                        x: 120,
                        y: 10,
                        width: 80,
                        height: 40,
                    },
                    content_sha256: "7".repeat(64),
                },
            },
            MtgoVisibleEvidenceV1 {
                evidence_id: 50,
                sequence: sequence + 4,
                source: MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: 1,
                    rect: MtgoRectPxV1 {
                        x: 210,
                        y: 10,
                        width: 80,
                        height: 40,
                    },
                    content_sha256: "8".repeat(64),
                },
            },
        ],
        provenance,
        local_metadata_sha256,
        readiness: MtgoDecisionReadinessV1 {
            observation_complete: true,
            legal_action_set_complete: true,
            client_prompt_reconciled: true,
        },
    }
}

#[cfg(test)]
pub(crate) fn checked_untrusted_dxgi_decision_candidate_for_test_v1(
    profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
) -> CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1 {
    use crate::{
        checked_untrusted_dxgi_artifact_for_test_v1,
        complete_acting_player_duel_audit_record_for_test_v1,
        validate_dxgi_bound_observation_reconstruction_audit_v1,
    };

    let source =
        checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
    let audit = validate_dxgi_bound_observation_reconstruction_audit_v1(
        &source,
        complete_acting_player_duel_audit_record_for_test_v1(&source),
    )
    .expect("test duel reconstruction audit is valid");
    let record = dxgi_observed_decision_record_for_test_v1(&source, audit.source_frame_sequence());
    check_untrusted_dxgi_observed_decision_candidate_v1(&source, &audit, profile, record)
        .expect("test DXGI decision candidate is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        check_untrusted_duel_perception_runtime_profile_v1,
        checked_untrusted_dxgi_artifact_for_test_v1,
        complete_acting_player_duel_audit_record_for_test_v1,
        duel_perception_runtime_profile_for_test_v1,
        duel_perception_runtime_profile_payload_for_test_v1,
        validate_dxgi_bound_observation_reconstruction_audit_v1, MtgoReconstructionStatusV1,
    };

    fn source_and_audit_v1() -> (
        CheckedUntrustedMtgoDxgiCaptureArtifactV1,
        CheckedUntrustedMtgoObservationReconstructionAuditV1,
    ) {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let audit = validate_dxgi_bound_observation_reconstruction_audit_v1(
            &source,
            complete_acting_player_duel_audit_record_for_test_v1(&source),
        )
        .unwrap();
        (source, audit)
    }

    #[test]
    fn exact_source_audit_and_decision_bind_without_scoring_authority() {
        let (source, audit) = source_and_audit_v1();
        let profile = duel_perception_runtime_profile_for_test_v1();
        let record =
            dxgi_observed_decision_record_for_test_v1(&source, audit.source_frame_sequence());
        let candidate = check_untrusted_dxgi_observed_decision_candidate_v1(
            &source,
            &audit,
            &profile,
            record.clone(),
        )
        .unwrap();

        assert_eq!(candidate.frame_id(), 1);
        assert_eq!(candidate.frame_sequence(), audit.source_frame_sequence());
        assert_eq!(candidate.source_manifest_sha256(), source.manifest_sha256());
        assert_eq!(
            candidate.source_canonical_bgra8_sha256(),
            source.canonical_bgra8_sha256()
        );
        assert_eq!(candidate.candidate_commitment_sha256().len(), 64);
        assert_eq!(
            candidate.perception_profile_commitment_sha256(),
            profile.profile_commitment_sha256()
        );
        let mut mismatched_profile = duel_perception_runtime_profile_payload_for_test_v1();
        mismatched_profile.dpi += 1;
        let mismatched_profile =
            check_untrusted_duel_perception_runtime_profile_v1(mismatched_profile).unwrap();
        assert_eq!(
            check_untrusted_dxgi_observed_decision_candidate_v1(
                &source,
                &audit,
                &mismatched_profile,
                record,
            )
            .err()
            .unwrap()
            .code(),
            "dxgi_observed_decision_perception_profile_source"
        );
        assert!(!candidate.safe_for_model_scoring());
        assert!(!candidate.safe_for_input());
    }

    #[test]
    fn frame_identity_sequence_hash_and_geometry_substitution_fail() {
        let (source, audit) = source_and_audit_v1();
        let profile = duel_perception_runtime_profile_for_test_v1();
        let baseline =
            dxgi_observed_decision_record_for_test_v1(&source, audit.source_frame_sequence());
        let mut mutations = Vec::new();
        let mut value = baseline.clone();
        value.frame_id = 2;
        mutations.push(value);
        let mut value = baseline.clone();
        value.frames[0].sequence += 1;
        mutations.push(value);
        let mut value = baseline.clone();
        value.frames[0].sha256 = "9".repeat(64);
        mutations.push(value);
        let mut value = baseline.clone();
        value.frames[0].client_bounds.width += 1;
        mutations.push(value);
        let mut value = baseline;
        value.frames.push(value.frames[0].clone());
        mutations.push(value);

        for mutation in mutations {
            assert!(check_untrusted_dxgi_observed_decision_candidate_v1(
                &source, &audit, &profile, mutation,
            )
            .is_err());
        }
    }

    #[test]
    fn non_duel_source_and_incomplete_audit_fail() {
        let source = checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::Navigation);
        let profile = duel_perception_runtime_profile_for_test_v1();
        let acting_source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let audit = validate_dxgi_bound_observation_reconstruction_audit_v1(
            &acting_source,
            complete_acting_player_duel_audit_record_for_test_v1(&acting_source),
        )
        .unwrap();
        assert_eq!(
            check_untrusted_dxgi_observed_decision_candidate_v1(
                &source,
                &audit,
                &profile,
                dxgi_observed_decision_record_for_test_v1(&source, audit.source_frame_sequence(),),
            )
            .err()
            .unwrap()
            .code(),
            "dxgi_observed_decision_role"
        );

        let mut incomplete_record =
            complete_acting_player_duel_audit_record_for_test_v1(&acting_source);
        incomplete_record.groups[0].status = MtgoReconstructionStatusV1::Incomplete;
        incomplete_record.groups[0].missing_reason_codes =
            vec!["duel_participants_unreconciled".to_owned()];
        incomplete_record.observation_complete = false;
        let incomplete_audit = validate_dxgi_bound_observation_reconstruction_audit_v1(
            &acting_source,
            incomplete_record,
        )
        .unwrap();
        assert_eq!(
            check_untrusted_dxgi_observed_decision_candidate_v1(
                &acting_source,
                &incomplete_audit,
                &profile,
                dxgi_observed_decision_record_for_test_v1(
                    &acting_source,
                    incomplete_audit.source_frame_sequence(),
                ),
            )
            .err()
            .unwrap()
            .code(),
            "dxgi_observed_decision_audit_incomplete"
        );
    }
}
