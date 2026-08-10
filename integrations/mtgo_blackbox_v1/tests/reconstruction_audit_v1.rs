use mtgo_blackbox_v1::{
    validate_observation_reconstruction_audit_v1, MtgoObservationReconstructionAuditV1,
    MtgoObservationReconstructionGroupV1, MtgoReconstructionStatusV1, MtgoReconstructionTopologyV1,
};

fn fixture() -> MtgoObservationReconstructionAuditV1 {
    serde_json::from_str(include_str!(
        "../fixtures/solitaire_observation_reconstruction_audit_v1.json"
    ))
    .expect("checked-in reconstruction audit must parse")
}

#[test]
fn solitaire_audit_records_exact_model_scoring_blockers() {
    let checked = validate_observation_reconstruction_audit_v1(fixture()).unwrap();
    assert_eq!(
        checked.topology(),
        MtgoReconstructionTopologyV1::SolitaireCalibration
    );
    assert!(!checked.observation_complete());
    assert!(!checked.legal_action_set_complete());
    assert!(!checked.ready_for_model_scoring());
    assert_eq!(
        checked.blocking_groups(),
        &[
            MtgoObservationReconstructionGroupV1::DuelParticipants,
            MtgoObservationReconstructionGroupV1::PlayerPublicState,
            MtgoObservationReconstructionGroupV1::PublicObjectsAndZones,
            MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext,
            MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb,
            MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions,
        ]
    );
    assert_eq!(checked.audit_commitment_sha256().len(), 64);
}

#[test]
fn solitaire_or_incomplete_audit_cannot_claim_readiness() {
    for field in 0..3 {
        let mut claimed = fixture();
        let expected_code = match field {
            0 => {
                claimed.observation_complete = true;
                "reconstruction_audit_readiness_mismatch"
            }
            1 => {
                claimed.legal_action_set_complete = true;
                "reconstruction_audit_readiness_mismatch"
            }
            _ => {
                claimed.ready_for_model_scoring = true;
                "reconstruction_audit_model_scoring_authority_forbidden"
            }
        };
        assert_eq!(
            validate_observation_reconstruction_audit_v1(claimed)
                .err()
                .expect("unsupported readiness claim must fail")
                .code(),
            expected_code
        );
    }
}

#[test]
fn canonical_group_inventory_order_and_status_rules_are_strict() {
    let mut missing = fixture();
    missing.groups.pop();
    assert_eq!(
        validate_observation_reconstruction_audit_v1(missing)
            .err()
            .expect("missing group must fail")
            .code(),
        "reconstruction_audit_group_count_invalid"
    );

    let mut reordered = fixture();
    reordered.groups.swap(0, 1);
    assert_eq!(
        validate_observation_reconstruction_audit_v1(reordered)
            .err()
            .expect("reordered groups must fail")
            .code(),
        "reconstruction_audit_group_order_invalid"
    );

    let mut no_evidence = fixture();
    no_evidence.groups[1].visible_regions.clear();
    assert_eq!(
        validate_observation_reconstruction_audit_v1(no_evidence)
            .err()
            .expect("visible-complete group without evidence must fail")
            .code(),
        "reconstruction_audit_visible_complete_invalid"
    );

    let mut no_reason = fixture();
    no_reason.groups[0].missing_reason_codes.clear();
    assert_eq!(
        validate_observation_reconstruction_audit_v1(no_reason)
            .err()
            .expect("incomplete group without reason must fail")
            .code(),
        "reconstruction_audit_incomplete_without_reason"
    );

    let mut local_as_visible = fixture();
    local_as_visible.groups[6].status = MtgoReconstructionStatusV1::VisibleComplete;
    local_as_visible.groups[6].visible_regions = fixture().groups[1].visible_regions.clone();
    local_as_visible.groups[6].missing_reason_codes.clear();
    assert_eq!(
        validate_observation_reconstruction_audit_v1(local_as_visible)
            .err()
            .expect("kernel history cannot be declared pixel-complete")
            .code(),
        "reconstruction_audit_visible_complete_invalid"
    );
}

#[test]
fn frame_geometry_hashes_and_preview_authority_fail_closed() {
    let mut rect = fixture();
    rect.groups[1].visible_regions[0].rect_client_px.width = 10_000;
    assert_eq!(
        validate_observation_reconstruction_audit_v1(rect)
            .err()
            .expect("out-of-bounds region must fail")
            .code(),
        "reconstruction_audit_region_rect_invalid"
    );

    let mut hash = fixture();
    hash.groups[1].visible_regions[0].bgra8_sha256 = "A".repeat(64);
    assert_eq!(
        validate_observation_reconstruction_audit_v1(hash)
            .err()
            .expect("non-canonical hash must fail")
            .code(),
        "reconstruction_audit_region_hash_invalid"
    );

    let mut authority = fixture();
    authority.frame.safe_for_policy_scoring = true;
    assert_eq!(
        validate_observation_reconstruction_audit_v1(authority)
            .err()
            .expect("preview authority claim must fail")
            .code(),
        "reconstruction_audit_preview_claims_authority"
    );
}

#[test]
fn complete_two_player_inventory_can_only_produce_readiness_not_an_observation() {
    let mut complete = fixture();
    complete.topology = MtgoReconstructionTopologyV1::TwoPlayerDuel;
    for group in &mut complete.groups {
        match group.group {
            MtgoObservationReconstructionGroupV1::DuelParticipants
            | MtgoObservationReconstructionGroupV1::PlayerPublicState
            | MtgoObservationReconstructionGroupV1::PublicObjectsAndZones
            | MtgoObservationReconstructionGroupV1::CompleteOrderedLegalActions => {
                group.status = MtgoReconstructionStatusV1::VisibleComplete;
                group.missing_reason_codes.clear();
            }
            MtgoObservationReconstructionGroupV1::KernelDecisionHistoryContext
            | MtgoObservationReconstructionGroupV1::ObjectIncarnationsAndCardDb => {
                group.status = MtgoReconstructionStatusV1::LocalDerivedComplete;
                group.visible_regions.clear();
                group.missing_reason_codes.clear();
            }
            _ => {}
        }
    }
    complete.observation_complete = true;
    complete.legal_action_set_complete = true;
    complete.ready_for_model_scoring = false;
    let checked = validate_observation_reconstruction_audit_v1(complete).unwrap();
    assert!(checked.observation_complete());
    assert!(checked.legal_action_set_complete());
    assert!(!checked.ready_for_model_scoring());
    assert!(checked.blocking_groups().is_empty());
}

#[test]
fn audit_json_rejects_embedded_observation_or_actions() {
    let source = include_str!("../fixtures/solitaire_observation_reconstruction_audit_v1.json");
    let altered = source.replacen(
        "\"ready_for_model_scoring\": false",
        "\"ready_for_model_scoring\": false, \"observation\": {}, \"legal_actions\": []",
        1,
    );
    assert!(serde_json::from_str::<MtgoObservationReconstructionAuditV1>(&altered).is_err());
}
