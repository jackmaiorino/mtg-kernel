use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1};
use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use mtgo_blackbox_v1::*;

fn digest(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn valid_record() -> MtgoObservedDecisionV1 {
    let decision = (1..=128)
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
            Some((decision, pass, land))
        })
        .expect("a deterministic opening exposes Pass and PlayLand");
    let source = match &decision.2 {
        ActionSemanticV1::PlayLand { source, .. } => source.clone(),
        _ => unreachable!(),
    };
    let payload = MtgoSemanticDecisionPayloadV1 {
        observation: (*decision.0.observation).clone(),
        legal_actions: vec![decision.1, decision.2],
        object_bindings: vec![MtgoObjectBindingV1 {
            adapter_object_id: "hand:visible-slot-0".to_owned(),
            kernel_ref: source,
        }],
    };
    let provenance = payload_leaf_inventory_v1(&payload)
        .unwrap()
        .into_iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .enumerate()
        .map(|(index, leaf)| MtgoLeafProvenanceV1 {
            json_pointer: leaf.json_pointer,
            value_sha256: leaf.value_sha256,
            evidence_ids: vec![[20, 30, 40, 50][index % 4]],
            confidence_bps: 10_000,
        })
        .collect();
    let local_metadata_sha256 = local_metadata_commitment_v1(&payload).unwrap();
    MtgoObservedDecisionV1 {
        schema_version: MTGO_OBSERVED_DECISION_SCHEMA_V1,
        decision_id: "action-resolution-priority-1".to_owned(),
        frame_id: 1,
        payload,
        frames: vec![MtgoMockFrameV1 {
            frame_id: 1,
            sequence: 1,
            sha256: digest('1'),
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
                    content_sha256: digest('2'),
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
            region_evidence(30, 3, 0, 0, 100, 50, '3'),
            region_evidence(40, 4, 0, 100, 100, 50, '4'),
            region_evidence(50, 5, 200, 100, 100, 100, '5'),
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

fn region_evidence(
    evidence_id: u64,
    sequence: u64,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    hash_character: char,
) -> MtgoVisibleEvidenceV1 {
    MtgoVisibleEvidenceV1 {
        evidence_id,
        sequence,
        source: MtgoEvidenceSourceV1::FrameRegion {
            frame_id: 1,
            rect: MtgoRectPxV1 {
                x,
                y,
                width,
                height,
            },
            content_sha256: digest(hash_character),
        },
    }
}

fn deployment() -> MtgoExpectedModelDeploymentV1 {
    MtgoExpectedModelDeploymentV1 {
        schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
        deployment_id: "test-deployment".to_owned(),
        checkpoint: MtgoNativeCheckpointIdentityV1 {
            run_sha256: digest('a'),
            checkpoint_manifest_sha256: digest('b'),
            checkpoint_payload_sha256: digest('c'),
            train_state_sha256: digest('d'),
            model_parameter_sha256: digest('e'),
            generation_index: 640,
        },
        scorer_contract_sha256: digest('f'),
    }
}

fn selection(decision: &ValidatedMtgoObservedDecisionV1) -> CheckedUntrustedMtgoModelSelectionV1 {
    let deployment = deployment();
    let request = build_external_scoring_request_v1(decision, &deployment).unwrap();
    let response = MtgoExternalModelScoreResponseV1 {
        schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
        request_commitment_sha256: scoring_request_commitment_v1(&request).unwrap(),
        logits_f32_bits: vec![0.0_f32.to_bits(), 1.0_f32.to_bits()],
        value_f32_bits: 0.0_f32.to_bits(),
    };
    validate_external_model_score_response_v1(decision, &deployment, response).unwrap()
}

fn control_set(decision: &ValidatedMtgoObservedDecisionV1) -> MtgoVisibleActionControlSetV1 {
    MtgoVisibleActionControlSetV1 {
        schema_version: MTGO_VISIBLE_ACTION_CONTROL_SET_SCHEMA_V1,
        decision_commitment_sha256: decision.decision_commitment_sha256().to_owned(),
        frame_id: decision.frame_id(),
        frame_sequence: decision.frame_sequence(),
        prompt_frame_region_evidence_id: 30,
        prompt_reconciled: true,
        candidate_set_complete: true,
        controls: vec![
            MtgoVisibleActionControlCandidateV1 {
                control_id: "priority-pass".to_owned(),
                control_kind: MtgoVisibleControlKindV1::PhaseButton,
                frame_region_evidence_id: 40,
                semantic: decision.legal_actions()[0].clone(),
                confidence_bps: 10_000,
                visibly_enabled: true,
            },
            MtgoVisibleActionControlCandidateV1 {
                control_id: "hand-card-0".to_owned(),
                control_kind: MtgoVisibleControlKindV1::Card,
                frame_region_evidence_id: 50,
                semantic: decision.legal_actions()[1].clone(),
                confidence_bps: 10_000,
                visibly_enabled: true,
            },
        ],
    }
}

#[test]
fn selected_semantic_resolves_to_one_current_visible_control() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let selection = selection(&decision);
    let resolved =
        resolve_selected_visible_control_v1(&decision, &selection, control_set(&decision)).unwrap();
    assert_eq!(resolved.control_id(), "hand-card-0");
    assert_eq!(resolved.frame_id(), decision.frame_id());
    assert_eq!(resolved.frame_sequence(), decision.frame_sequence());
    assert_eq!(resolved.frame_region_evidence_id(), 50);
    assert_eq!(resolved.resolution_commitment_sha256().len(), 64);
    assert!(!resolved.safe_for_live_input());
}

#[test]
fn selected_control_match_must_be_unique() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let selection = selection(&decision);
    let mut ambiguous = control_set(&decision);
    ambiguous.controls[0].semantic = decision.legal_actions()[1].clone();
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, ambiguous)
            .err()
            .unwrap()
            .code(),
        "visible_control_selected_match_count"
    );

    let mut missing = control_set(&decision);
    missing.controls.pop();
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, missing)
            .err()
            .unwrap()
            .code(),
        "visible_control_selected_match_count"
    );
}

#[test]
fn stale_or_incomplete_control_sets_fail_closed() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let selection = selection(&decision);
    let mut stale = control_set(&decision);
    stale.frame_sequence += 1;
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, stale)
            .err()
            .unwrap()
            .code(),
        "visible_control_set_stale_frame"
    );

    for field in 0..2 {
        let mut incomplete = control_set(&decision);
        if field == 0 {
            incomplete.prompt_reconciled = false;
        } else {
            incomplete.candidate_set_complete = false;
        }
        assert_eq!(
            resolve_selected_visible_control_v1(&decision, &selection, incomplete)
                .err()
                .unwrap()
                .code(),
            "visible_control_set_incomplete"
        );
    }
}

#[test]
fn controls_require_current_distinct_pixel_evidence() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let selection = selection(&decision);
    let mut unknown = control_set(&decision);
    unknown.controls[1].frame_region_evidence_id = 999;
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, unknown)
            .err()
            .unwrap()
            .code(),
        "visible_control_evidence_invalid"
    );

    let mut prompt_reuse = control_set(&decision);
    prompt_reuse.controls[1].frame_region_evidence_id = 30;
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, prompt_reuse)
            .err()
            .unwrap()
            .code(),
        "visible_control_overlaps_prompt_region"
    );

    let mut duplicate = control_set(&decision);
    duplicate.controls[1].frame_region_evidence_id = 40;
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, duplicate)
            .err()
            .unwrap()
            .code(),
        "visible_control_evidence_duplicate"
    );
}

#[test]
fn disabled_low_confidence_or_illegal_controls_fail_closed() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let selection = selection(&decision);
    let mut disabled = control_set(&decision);
    disabled.controls[1].visibly_enabled = false;
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, disabled)
            .err()
            .unwrap()
            .code(),
        "visible_control_disabled"
    );

    let mut confidence = control_set(&decision);
    confidence.controls[1].confidence_bps = MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1 - 1;
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, confidence)
            .err()
            .unwrap()
            .code(),
        "visible_control_confidence_invalid"
    );

    let actor = decision.observation().acting_player;
    let other = match actor {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    };
    let mut illegal = control_set(&decision);
    illegal.controls[0].semantic = ActionSemanticV1::Pass { actor: other };
    assert_eq!(
        resolve_selected_visible_control_v1(&decision, &selection, illegal)
            .err()
            .unwrap()
            .code(),
        "visible_control_semantic_not_legal"
    );
}

#[test]
fn selection_and_control_set_cannot_cross_decisions() {
    let first = validate_observed_decision_v1(valid_record()).unwrap();
    let selection = selection(&first);
    let mut second_record = valid_record();
    second_record.decision_id = "action-resolution-priority-2".to_owned();
    let second = validate_observed_decision_v1(second_record).unwrap();
    assert_eq!(
        resolve_selected_visible_control_v1(&second, &selection, control_set(&second))
            .err()
            .unwrap()
            .code(),
        "visible_control_set_decision_mismatch"
    );
}

#[test]
fn control_set_json_rejects_coordinates_and_unknown_fields() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let json = serde_json::to_string(&control_set(&decision)).unwrap();
    let with_coordinates = json.replacen(
        "\"control_id\":\"priority-pass\"",
        "\"control_id\":\"priority-pass\",\"x\":123,\"y\":456",
        1,
    );
    assert!(serde_json::from_str::<MtgoVisibleActionControlSetV1>(&with_coordinates).is_err());
}

#[test]
fn full_shadow_loop_scores_resolves_submits_and_confirms_one_action() {
    let source_record = valid_record();
    let mut next_record = valid_record();
    next_record.decision_id = "action-resolution-postcondition-2".to_owned();
    next_record.frame_id = 2;
    next_record.frames[0].frame_id = 2;
    next_record.frames[0].sequence = 2;
    next_record.frames[0].sha256 = digest('9');
    for evidence in &mut next_record.evidence {
        evidence.sequence += 1;
        if let MtgoEvidenceSourceV1::FrameRegion { frame_id, .. } = &mut evidence.source {
            *frame_id = 2;
        }
    }
    next_record.payload.observation.projection.surface.turn += 1;
    next_record.payload.observation.visible_projection_hash =
        compute_visible_projection_hash_v5_v1(&next_record.payload.observation).unwrap();
    next_record.provenance = payload_leaf_inventory_v1(&next_record.payload)
        .unwrap()
        .into_iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .enumerate()
        .map(|(index, leaf)| MtgoLeafProvenanceV1 {
            json_pointer: leaf.json_pointer,
            value_sha256: leaf.value_sha256,
            evidence_ids: vec![[20, 30, 40, 50][index % 4]],
            confidence_bps: 10_000,
        })
        .collect();
    next_record.local_metadata_sha256 = local_metadata_commitment_v1(&next_record.payload).unwrap();

    let pointer = "/observation/projection/turn";
    let before = payload_leaf_inventory_v1(&source_record.payload)
        .unwrap()
        .into_iter()
        .find(|leaf| leaf.json_pointer == pointer)
        .unwrap();
    let after = payload_leaf_inventory_v1(&next_record.payload)
        .unwrap()
        .into_iter()
        .find(|leaf| leaf.json_pointer == pointer)
        .unwrap();
    let postcondition = MtgoMockPostconditionV1 {
        required_visible_leaf: MtgoExpectedVisibleLeafV1 {
            json_pointer: pointer.to_owned(),
            before_value_sha256: before.value_sha256,
            after_value_sha256: after.value_sha256,
        },
    };

    let source = validate_observed_decision_v1(source_record).unwrap();
    let next = validate_observed_decision_v1(next_record).unwrap();
    let selection = selection(&source);
    let resolved =
        resolve_selected_visible_control_v1(&source, &selection, control_set(&source)).unwrap();
    assert_eq!(
        resolved.selection_commitment_sha256(),
        selection.selection_commitment_sha256()
    );

    let scored_intent = make_scored_offline_intent_v1(&source, &selection).unwrap();
    let mut actuator = MtgoMockActuatorV1::default();
    let submitted = actuator
        .submit(&source, selection.selected_index(), postcondition)
        .unwrap();
    assert_eq!(submitted, scored_intent);
    assert!(actuator.has_pending_action());
    actuator.confirm(&next).unwrap();
    assert!(!actuator.has_pending_action());
    assert!(!actuator.is_halted());
}
