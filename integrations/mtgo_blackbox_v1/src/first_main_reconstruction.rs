use crate::{
    CheckedUntrustedMtgoFirstMainKernelCardCoverageV1, CheckedUntrustedMtgoFirstMainLegalActionsV1,
    CheckedUntrustedMtgoObservationReconstructionAuditV1,
    CheckedUntrustedMtgoVisibleObjectLedgerV1, MtgoContractErrorV1,
    MtgoObservationReconstructionGroupV1, MtgoReconstructionTopologyV1,
};
use sha2::{Digest, Sha256};

const FIRST_MAIN_RECONSTRUCTION_REFINEMENT_DOMAIN_V1: &[u8] =
    b"mtgo-first-main-reconstruction-refinement-v1";
const FIRST_MAIN_HAND_COUNT_V1: usize = 8;
const FIRST_MAIN_ACTION_COUNT_V1: usize = 9;

const EXPECTED_BASE_BLOCKERS_V1: [MtgoObservationReconstructionGroupV1; 6] = [
    MtgoObservationReconstructionGroupV1::DuelParticipants,
    MtgoObservationReconstructionGroupV1::PlayerPublicState,
    MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
    MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext,
    MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb,
    MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions,
];

const REMAINING_BLOCKERS_V1: [MtgoObservationReconstructionGroupV1; 4] = [
    MtgoObservationReconstructionGroupV1::DuelParticipants,
    MtgoObservationReconstructionGroupV1::PlayerPublicState,
    MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
    MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext,
];

const RESOLVED_GROUPS_V1: [MtgoObservationReconstructionGroupV1; 2] = [
    MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb,
    MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions,
];

/// Source-bound refinement of the fixed Solitaire reconstruction audit.
///
/// The complete supported basic-land coverage, unchanged hand-object ledger,
/// and exact first-main action set close two bookkeeping groups for the same
/// frame. Four observation groups remain blocked, including all distinct
/// opponent state and kernel-only decision history. The wrapper cannot expose
/// bindings, actions, an `ObservationV5`, a model request, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoFirstMainReconstructionRefinementV1;
/// fn cannot_extract_actions(
///     value: &CheckedUntrustedMtgoFirstMainReconstructionRefinementV1,
/// ) {
///     let _ = value.legal_actions();
/// }
/// ```
pub struct CheckedUntrustedMtgoFirstMainReconstructionRefinementV1 {
    source_manifest_sha256: String,
    source_frame_sha256: String,
    base_audit_commitment_sha256: String,
    coverage_commitment_sha256: String,
    object_ledger_commitment_sha256: String,
    action_set_commitment_sha256: String,
    refinement_commitment_sha256: String,
}

impl CheckedUntrustedMtgoFirstMainReconstructionRefinementV1 {
    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn base_audit_commitment_sha256(&self) -> &str {
        &self.base_audit_commitment_sha256
    }

    pub fn coverage_commitment_sha256(&self) -> &str {
        &self.coverage_commitment_sha256
    }

    pub fn object_ledger_commitment_sha256(&self) -> &str {
        &self.object_ledger_commitment_sha256
    }

    pub fn action_set_commitment_sha256(&self) -> &str {
        &self.action_set_commitment_sha256
    }

    pub fn resolved_groups(&self) -> &[MtgoObservationReconstructionGroupV1] {
        &RESOLVED_GROUPS_V1
    }

    pub fn remaining_blocking_groups(&self) -> &[MtgoObservationReconstructionGroupV1] {
        &REMAINING_BLOCKERS_V1
    }

    pub fn supported_first_main_action_set_complete(&self) -> bool {
        true
    }

    pub fn observation_complete(&self) -> bool {
        false
    }

    pub fn ready_for_model_scoring(&self) -> bool {
        false
    }

    pub fn refinement_commitment_sha256(&self) -> &str {
        &self.refinement_commitment_sha256
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn refine_checked_untrusted_first_main_reconstruction_v1(
    base: &CheckedUntrustedMtgoObservationReconstructionAuditV1,
    coverage: &CheckedUntrustedMtgoFirstMainKernelCardCoverageV1,
    ledger: &CheckedUntrustedMtgoVisibleObjectLedgerV1,
    actions: &CheckedUntrustedMtgoFirstMainLegalActionsV1,
) -> Result<CheckedUntrustedMtgoFirstMainReconstructionRefinementV1, MtgoContractErrorV1> {
    if base.topology() != MtgoReconstructionTopologyV1::SolitaireCalibration
        || base.blocking_groups() != EXPECTED_BASE_BLOCKERS_V1
        || base.observation_complete()
        || base.legal_action_set_complete()
        || base.ready_for_model_scoring()
    {
        return Err(error_v1(
            "first_main_reconstruction_base_audit_mismatch",
            "the canonical six-blocker Solitaire audit is required",
        ));
    }
    if coverage.source_manifest_sha256() != base.source_manifest_sha256()
        || coverage.source_frame_sha256() != base.source_frame_sha256()
        || !coverage.coverage_complete()
        || coverage.entries().len() != FIRST_MAIN_HAND_COUNT_V1
    {
        return Err(error_v1(
            "first_main_reconstruction_coverage_mismatch",
            "coverage must be complete and bind the exact audited frame",
        ));
    }
    if ledger.current_frame_sha256() != coverage.source_frame_sha256()
        || ledger.seed_source_commitment_sha256() != Some(coverage.coverage_commitment_sha256())
        || ledger.object_count() != FIRST_MAIN_HAND_COUNT_V1
        || ledger.transition_count() != 0
    {
        return Err(error_v1(
            "first_main_reconstruction_ledger_mismatch",
            "the unchanged eight-object ledger must bind the exact coverage result",
        ));
    }
    if actions.source_frame_sha256() != coverage.source_frame_sha256()
        || actions.coverage_commitment_sha256() != coverage.coverage_commitment_sha256()
        || actions.object_ledger_commitment_sha256() != ledger.ledger_commitment_sha256()
        || actions.play_land_action_count() != FIRST_MAIN_HAND_COUNT_V1 as u8
        || actions.action_count() != FIRST_MAIN_ACTION_COUNT_V1
    {
        return Err(error_v1(
            "first_main_reconstruction_action_set_mismatch",
            "the exact nine-action set must bind the same coverage and ledger",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(FIRST_MAIN_RECONSTRUCTION_REFINEMENT_DOMAIN_V1);
    for part in [
        base.source_manifest_sha256().as_bytes(),
        base.source_frame_sha256().as_bytes(),
        base.audit_commitment_sha256().as_bytes(),
        coverage.coverage_commitment_sha256().as_bytes(),
        ledger.ledger_commitment_sha256().as_bytes(),
        actions.action_set_commitment_sha256().as_bytes(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    for group in REMAINING_BLOCKERS_V1 {
        let encoded = serde_json::to_vec(&group).map_err(|error| {
            error_v1(
                "first_main_reconstruction_group_serialization",
                error.to_string(),
            )
        })?;
        hasher.update((encoded.len() as u64).to_le_bytes());
        hasher.update(encoded);
    }

    Ok(CheckedUntrustedMtgoFirstMainReconstructionRefinementV1 {
        source_manifest_sha256: base.source_manifest_sha256().to_owned(),
        source_frame_sha256: base.source_frame_sha256().to_owned(),
        base_audit_commitment_sha256: base.audit_commitment_sha256().to_owned(),
        coverage_commitment_sha256: coverage.coverage_commitment_sha256().to_owned(),
        object_ledger_commitment_sha256: ledger.ledger_commitment_sha256().to_owned(),
        action_set_commitment_sha256: actions.action_set_commitment_sha256().to_owned(),
        refinement_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        complete_island_coverage_for_test_v1, derive_checked_untrusted_first_main_legal_actions_v1,
        start_checked_untrusted_first_main_hand_object_ledger_v1,
        validate_observation_reconstruction_audit_v1, MtgoObservationReconstructionAuditV1,
        MtgoReconstructionStatusV1,
    };
    use mtg_kernel::rl::PlayerSeatV1;

    fn base_audit_v1() -> CheckedUntrustedMtgoObservationReconstructionAuditV1 {
        let mut record: MtgoObservationReconstructionAuditV1 = serde_json::from_str(include_str!(
            "../fixtures/solitaire_observation_reconstruction_audit_v1.json"
        ))
        .unwrap();
        record.audit_id = "supported_first_main_reconstruction_test_v1".to_owned();
        record.frame.manifest_sha256 = "2".repeat(64);
        record.frame.frame_sha256 = "3".repeat(64);
        validate_observation_reconstruction_audit_v1(record).unwrap()
    }

    #[test]
    fn exact_source_chain_resolves_only_objects_and_actions() {
        let base = base_audit_v1();
        let coverage = complete_island_coverage_for_test_v1();
        let ledger =
            start_checked_untrusted_first_main_hand_object_ledger_v1(&coverage, PlayerSeatV1::P0)
                .unwrap();
        let actions =
            derive_checked_untrusted_first_main_legal_actions_v1(&coverage, &ledger).unwrap();
        let refined = refine_checked_untrusted_first_main_reconstruction_v1(
            &base, &coverage, &ledger, &actions,
        )
        .unwrap();

        assert_eq!(refined.resolved_groups(), RESOLVED_GROUPS_V1);
        assert_eq!(refined.remaining_blocking_groups(), REMAINING_BLOCKERS_V1);
        assert!(refined.supported_first_main_action_set_complete());
        assert!(!refined.observation_complete());
        assert!(!refined.ready_for_model_scoring());
        assert!(!refined.safe_for_input());
        assert_eq!(refined.refinement_commitment_sha256().len(), 64);
    }

    #[test]
    fn cross_frame_or_cross_ledger_refinement_rejects() {
        let base = base_audit_v1();
        let coverage = complete_island_coverage_for_test_v1();
        let ledger =
            start_checked_untrusted_first_main_hand_object_ledger_v1(&coverage, PlayerSeatV1::P0)
                .unwrap();
        let actions =
            derive_checked_untrusted_first_main_legal_actions_v1(&coverage, &ledger).unwrap();

        let mut other_record: MtgoObservationReconstructionAuditV1 = serde_json::from_str(
            include_str!("../fixtures/solitaire_observation_reconstruction_audit_v1.json"),
        )
        .unwrap();
        other_record.audit_id = "other_first_main_reconstruction_test_v1".to_owned();
        other_record.frame.manifest_sha256 = "2".repeat(64);
        other_record.frame.frame_sha256 = "5".repeat(64);
        let other_base = validate_observation_reconstruction_audit_v1(other_record).unwrap();
        assert_eq!(
            refine_checked_untrusted_first_main_reconstruction_v1(
                &other_base,
                &coverage,
                &ledger,
                &actions,
            )
            .err()
            .unwrap()
            .code(),
            "first_main_reconstruction_coverage_mismatch"
        );

        let other_ledger =
            start_checked_untrusted_first_main_hand_object_ledger_v1(&coverage, PlayerSeatV1::P1)
                .unwrap();
        assert_eq!(
            refine_checked_untrusted_first_main_reconstruction_v1(
                &base,
                &coverage,
                &other_ledger,
                &actions,
            )
            .err()
            .unwrap()
            .code(),
            "first_main_reconstruction_action_set_mismatch"
        );
    }

    #[test]
    fn noncanonical_base_blocker_inventory_rejects() {
        let mut record: MtgoObservationReconstructionAuditV1 = serde_json::from_str(include_str!(
            "../fixtures/solitaire_observation_reconstruction_audit_v1.json"
        ))
        .unwrap();
        record.audit_id = "altered_first_main_reconstruction_test_v1".to_owned();
        record.frame.manifest_sha256 = "2".repeat(64);
        record.frame.frame_sha256 = "3".repeat(64);
        record.groups[7].status = MtgoReconstructionStatusV1::LocalDerivedComplete;
        record.groups[7].visible_regions.clear();
        record.groups[7].missing_reason_codes.clear();
        let altered = validate_observation_reconstruction_audit_v1(record).unwrap();

        let coverage = complete_island_coverage_for_test_v1();
        let ledger =
            start_checked_untrusted_first_main_hand_object_ledger_v1(&coverage, PlayerSeatV1::P0)
                .unwrap();
        let actions =
            derive_checked_untrusted_first_main_legal_actions_v1(&coverage, &ledger).unwrap();
        assert_eq!(
            refine_checked_untrusted_first_main_reconstruction_v1(
                &altered, &coverage, &ledger, &actions,
            )
            .err()
            .unwrap()
            .code(),
            "first_main_reconstruction_base_audit_mismatch"
        );
    }
}
