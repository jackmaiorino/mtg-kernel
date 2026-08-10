use mtg_kernel::rl::ActionSemanticV1;
use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use mtgo_blackbox_v1::*;

fn digest(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn deployment() -> MtgoExpectedModelDeploymentV1 {
    MtgoExpectedModelDeploymentV1 {
        schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
        deployment_id: "promoted-2-mtgo-shadow".to_owned(),
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
        decision_id: "model-score-priority-1".to_owned(),
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

fn response_for(
    decision: &ValidatedMtgoObservedDecisionV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    logits: &[f32],
    value: f32,
) -> MtgoExternalModelScoreResponseV1 {
    let request = build_external_scoring_request_v1(decision, deployment).unwrap();
    MtgoExternalModelScoreResponseV1 {
        schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
        request_commitment_sha256: scoring_request_commitment_v1(&request).unwrap(),
        logits_f32_bits: logits.iter().map(|value| value.to_bits()).collect(),
        value_f32_bits: value.to_bits(),
    }
}

struct MockScorer {
    logits: Vec<f32>,
    value: f32,
    called: bool,
}

impl MtgoExternalObservationScorerV1 for MockScorer {
    fn score_observation_v1(
        &mut self,
        request: &MtgoExternalScoringRequestV1,
        observation: &mtg_kernel::rl::ObservationV5,
        ordered_legal_actions: &[ActionSemanticV1],
    ) -> Result<MtgoExternalModelScoreResponseV1, MtgoContractErrorV1> {
        self.called = true;
        assert_eq!(request.action_count as usize, ordered_legal_actions.len());
        let ActionSemanticV1::Pass { actor } = &ordered_legal_actions[0] else {
            panic!("fixture action zero must remain Pass");
        };
        assert_eq!(observation.acting_player, *actor);
        Ok(MtgoExternalModelScoreResponseV1 {
            schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
            request_commitment_sha256: scoring_request_commitment_v1(request)?,
            logits_f32_bits: self.logits.iter().map(|value| value.to_bits()).collect(),
            value_f32_bits: self.value.to_bits(),
        })
    }
}

#[test]
fn exact_validated_decision_scores_selects_and_produces_offline_intent() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let deployment = deployment();
    let request = build_external_scoring_request_v1(&decision, &deployment).unwrap();
    assert_eq!(
        request.deployment_commitment_sha256,
        model_deployment_commitment_v1(&deployment).unwrap()
    );
    let mut scorer = MockScorer {
        logits: vec![0.25, 1.5],
        value: -0.125,
        called: false,
    };
    let selection =
        score_and_select_external_model_v1(&decision, &deployment, &mut scorer).unwrap();
    assert!(scorer.called);
    assert_eq!(selection.selected_index(), 1);
    assert_eq!(selection.selected_logit_f32_bits(), 1.5_f32.to_bits());
    assert_eq!(selection.value_f32_bits(), (-0.125_f32).to_bits());
    assert_eq!(selection.selection_commitment_sha256().len(), 64);
    assert!(!selection.safe_for_live_input());

    let intent = make_scored_offline_intent_v1(&decision, &selection).unwrap();
    assert_eq!(intent.selected_index, 1);
    assert_eq!(intent.semantic, decision.legal_actions()[1]);
    assert_eq!(
        intent.decision_commitment_sha256,
        decision.decision_commitment_sha256()
    );
}

#[test]
fn response_is_bound_to_decision_actions_observation_and_deployment() {
    let first = validate_observed_decision_v1(valid_record()).unwrap();
    let deployment = deployment();
    let response = response_for(&first, &deployment, &[0.0, 1.0], 0.0);

    let mut second_record = valid_record();
    second_record.decision_id = "model-score-priority-2".to_owned();
    let second = validate_observed_decision_v1(second_record).unwrap();
    assert_eq!(
        validate_external_model_score_response_v1(&second, &deployment, response.clone())
            .err()
            .unwrap()
            .code(),
        "external_scoring_response_request_mismatch"
    );

    let mut changed_deployment = deployment.clone();
    changed_deployment.checkpoint.model_parameter_sha256 = digest('0');
    assert_eq!(
        validate_external_model_score_response_v1(&first, &changed_deployment, response)
            .err()
            .unwrap()
            .code(),
        "external_scoring_response_request_mismatch"
    );
}

#[test]
fn logit_shape_and_every_numeric_output_are_finite() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let deployment = deployment();
    let short = response_for(&decision, &deployment, &[1.0], 0.0);
    assert_eq!(
        validate_external_model_score_response_v1(&decision, &deployment, short)
            .err()
            .unwrap()
            .code(),
        "external_scoring_logit_count_mismatch"
    );

    let nan_logit = response_for(&decision, &deployment, &[f32::NAN, 0.0], 0.0);
    assert_eq!(
        validate_external_model_score_response_v1(&decision, &deployment, nan_logit)
            .err()
            .unwrap()
            .code(),
        "external_scoring_logit_nonfinite"
    );

    let infinite_value = response_for(&decision, &deployment, &[0.0, 1.0], f32::INFINITY);
    assert_eq!(
        validate_external_model_score_response_v1(&decision, &deployment, infinite_value)
            .err()
            .unwrap()
            .code(),
        "external_scoring_value_nonfinite"
    );
}

#[test]
fn argmax_uses_lower_index_for_equal_finite_logits() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let deployment = deployment();
    let response = response_for(&decision, &deployment, &[2.0, 2.0], 0.0);
    let selection =
        validate_external_model_score_response_v1(&decision, &deployment, response).unwrap();
    assert_eq!(selection.selected_index(), 0);
}

#[test]
fn response_schema_and_request_commitment_fail_closed() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let deployment = deployment();
    let mut schema = response_for(&decision, &deployment, &[0.0, 1.0], 0.0);
    schema.schema_version += 1;
    assert_eq!(
        validate_external_model_score_response_v1(&decision, &deployment, schema)
            .err()
            .unwrap()
            .code(),
        "external_scoring_response_schema_mismatch"
    );

    let mut wrong_request = response_for(&decision, &deployment, &[0.0, 1.0], 0.0);
    wrong_request.request_commitment_sha256 = digest('9');
    assert_eq!(
        validate_external_model_score_response_v1(&decision, &deployment, wrong_request)
            .err()
            .unwrap()
            .code(),
        "external_scoring_response_request_mismatch"
    );
}

#[test]
fn deployment_identity_and_wire_json_are_strict() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let mut invalid = deployment();
    invalid.checkpoint.run_sha256 = "ABC".to_owned();
    assert_eq!(
        build_external_scoring_request_v1(&decision, &invalid)
            .err()
            .unwrap()
            .code(),
        "external_scoring_deployment_hash_invalid"
    );

    let response = response_for(&decision, &deployment(), &[0.0, 1.0], 0.0);
    let json = serde_json::to_string(&response).unwrap();
    let with_unknown = json.replacen('{', "{\"selected_index\":0,", 1);
    assert!(serde_json::from_str::<MtgoExternalModelScoreResponseV1>(&with_unknown).is_err());
}

#[test]
fn selection_cannot_be_reused_for_another_validated_decision() {
    let first = validate_observed_decision_v1(valid_record()).unwrap();
    let deployment = deployment();
    let response = response_for(&first, &deployment, &[0.0, 1.0], 0.0);
    let selection =
        validate_external_model_score_response_v1(&first, &deployment, response).unwrap();

    let mut second_record = valid_record();
    second_record.decision_id = "different-selection-target".to_owned();
    let second = validate_observed_decision_v1(second_record).unwrap();
    assert_eq!(
        make_scored_offline_intent_v1(&second, &selection)
            .err()
            .unwrap()
            .code(),
        "external_scoring_selection_decision_mismatch"
    );
}
