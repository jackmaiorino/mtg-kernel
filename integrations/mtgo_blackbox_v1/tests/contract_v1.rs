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
        .expect("at least one deterministic opening should expose Pass and PlayLand");

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
        decision_id: "mock-priority-1".to_owned(),
        frame_id: 1,
        payload,
        frames: vec![MtgoMockFrameV1 {
            frame_id: 1,
            sequence: 1,
            sha256: digest('a'),
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
                    content_sha256: digest('b'),
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

fn other_player(player: PlayerSeatV1) -> PlayerSeatV1 {
    match player {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    }
}

fn refresh_payload_commitments(record: &mut MtgoObservedDecisionV1) {
    record.payload.observation.visible_projection_hash =
        compute_visible_projection_hash_v5_v1(&record.payload.observation).unwrap();
    record.provenance = payload_leaf_inventory_v1(&record.payload)
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
    record.local_metadata_sha256 = local_metadata_commitment_v1(&record.payload).unwrap();
}

fn observation_postcondition(
    before: &MtgoObservedDecisionV1,
    after: &MtgoObservedDecisionV1,
) -> MtgoMockPostconditionV1 {
    let pointer = "/observation/projection/turn";
    let before_leaf = payload_leaf_inventory_v1(&before.payload)
        .unwrap()
        .into_iter()
        .find(|leaf| leaf.json_pointer == pointer && leaf.requires_visible_evidence)
        .unwrap();
    let after_leaf = payload_leaf_inventory_v1(&after.payload)
        .unwrap()
        .into_iter()
        .find(|leaf| leaf.json_pointer == pointer && leaf.requires_visible_evidence)
        .unwrap();
    MtgoMockPostconditionV1 {
        required_visible_leaf: MtgoExpectedVisibleLeafV1 {
            json_pointer: pointer.to_owned(),
            before_value_sha256: before_leaf.value_sha256,
            after_value_sha256: after_leaf.value_sha256,
        },
    }
}

#[test]
fn valid_visible_priority_decision_produces_exact_offline_intent() {
    let validated = validate_observed_decision_v1(valid_record()).unwrap();
    let intent = make_offline_intent_v1(&validated, 1).unwrap();
    assert_eq!(intent.selected_index, 1);
    assert_eq!(intent.semantic, validated.legal_actions()[1]);
    assert_eq!(
        intent.decision_commitment_sha256,
        validated.decision_commitment_sha256()
    );
}

#[test]
fn missing_duplicate_extra_and_wrong_leaf_provenance_fail_closed() {
    let mut missing = valid_record();
    missing.provenance.pop();
    assert_eq!(
        validate_observed_decision_v1(missing).unwrap_err().code(),
        "provenance_coverage"
    );

    let mut duplicate = valid_record();
    duplicate.provenance[1] = duplicate.provenance[0].clone();
    assert_eq!(
        validate_observed_decision_v1(duplicate).unwrap_err().code(),
        "provenance_pointer"
    );

    let mut extra = valid_record();
    let mut extra_row = extra.provenance[0].clone();
    extra_row.json_pointer = "/not/a/payload/leaf".to_owned();
    extra.provenance.push(extra_row);
    assert_eq!(
        validate_observed_decision_v1(extra).unwrap_err().code(),
        "provenance_coverage"
    );

    let mut wrong = valid_record();
    wrong.provenance[0].value_sha256 = digest('0');
    assert_eq!(
        validate_observed_decision_v1(wrong).unwrap_err().code(),
        "provenance_value"
    );
}

#[test]
fn unknown_forward_and_cyclic_evidence_fail_closed() {
    let mut unknown = valid_record();
    unknown.provenance[0].evidence_ids = vec![999];
    assert_eq!(
        validate_observed_decision_v1(unknown).unwrap_err().code(),
        "provenance_evidence"
    );

    let mut forward = valid_record();
    forward.evidence[0].sequence = 3;
    assert_eq!(
        validate_observed_decision_v1(forward).unwrap_err().code(),
        "evidence_parent_order"
    );

    let mut cyclic = valid_record();
    cyclic.evidence[1].source = MtgoEvidenceSourceV1::DerivedPublicFact {
        parent_evidence_ids: vec![20],
        derivation: MtgoPublicDerivationV1::PublicStateProjection,
    };
    assert_eq!(
        validate_observed_decision_v1(cyclic).unwrap_err().code(),
        "evidence_parent_order"
    );
}

#[test]
fn out_of_bounds_region_and_uncorroborated_accessibility_fail_closed() {
    let mut outside = valid_record();
    let MtgoEvidenceSourceV1::FrameRegion { rect, .. } = &mut outside.evidence[0].source else {
        unreachable!()
    };
    rect.x = 1_919;
    rect.width = 2;
    assert_eq!(
        validate_observed_decision_v1(outside).unwrap_err().code(),
        "evidence_rect"
    );

    let mut accessibility = valid_record();
    accessibility.evidence.push(MtgoVisibleEvidenceV1 {
        evidence_id: 30,
        sequence: 3,
        source: MtgoEvidenceSourceV1::VisibleAccessibilityText {
            frame_region_evidence_id: 20,
            frame_id: 1,
            control_bounds: MtgoRectPxV1 {
                x: 10,
                y: 10,
                width: 100,
                height: 30,
            },
            is_offscreen: false,
            automation_id_sha256: digest('c'),
            text_sha256: digest('d'),
        },
    });
    for row in &mut accessibility.provenance {
        row.evidence_ids = vec![30];
    }
    assert_eq!(
        validate_observed_decision_v1(accessibility)
            .unwrap_err()
            .code(),
        "visual_corroboration"
    );

    let mut offscreen = valid_record();
    offscreen.evidence.push(MtgoVisibleEvidenceV1 {
        evidence_id: 30,
        sequence: 3,
        source: MtgoEvidenceSourceV1::VisibleAccessibilityText {
            frame_region_evidence_id: 10,
            frame_id: 1,
            control_bounds: MtgoRectPxV1 {
                x: 10,
                y: 10,
                width: 100,
                height: 30,
            },
            is_offscreen: true,
            automation_id_sha256: digest('c'),
            text_sha256: digest('d'),
        },
    });
    for row in &mut offscreen.provenance {
        row.evidence_ids = vec![30];
    }
    assert_eq!(
        validate_observed_decision_v1(offscreen).unwrap_err().code(),
        "accessibility_visibility"
    );

    let mut visible = valid_record();
    visible.evidence.pop();
    visible.evidence.push(MtgoVisibleEvidenceV1 {
        evidence_id: 30,
        sequence: 2,
        source: MtgoEvidenceSourceV1::VisibleAccessibilityText {
            frame_region_evidence_id: 10,
            frame_id: 1,
            control_bounds: MtgoRectPxV1 {
                x: 10,
                y: 10,
                width: 100,
                height: 30,
            },
            is_offscreen: false,
            automation_id_sha256: digest('c'),
            text_sha256: digest('d'),
        },
    });
    for row in &mut visible.provenance {
        row.evidence_ids = vec![30];
    }
    validate_observed_decision_v1(visible).unwrap();
}

#[test]
fn legal_actions_cannot_be_supported_only_by_a_stale_frame() {
    let mut stale = valid_record();
    stale.frames[0].sequence = 2;
    stale.frames.push(MtgoMockFrameV1 {
        frame_id: 2,
        sequence: 1,
        sha256: digest('e'),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1_920,
            height: 1_080,
        },
    });
    let MtgoEvidenceSourceV1::FrameRegion { frame_id, .. } = &mut stale.evidence[0].source else {
        unreachable!()
    };
    *frame_id = 2;
    assert_eq!(
        validate_observed_decision_v1(stale).unwrap_err().code(),
        "current_frame_evidence"
    );
}

#[test]
fn malformed_kernel_observation_and_action_shapes_fail_closed() {
    let mut schema = valid_record();
    schema.payload.observation.schema_version = u32::MAX;
    schema.local_metadata_sha256 = local_metadata_commitment_v1(&schema.payload).unwrap();
    assert_eq!(
        validate_observed_decision_v1(schema).unwrap_err().code(),
        "observation_metadata"
    );

    let mut projection_hash = valid_record();
    projection_hash.payload.observation.visible_projection_hash ^= 1;
    projection_hash.local_metadata_sha256 =
        local_metadata_commitment_v1(&projection_hash.payload).unwrap();
    assert_eq!(
        validate_observed_decision_v1(projection_hash)
            .unwrap_err()
            .code(),
        "visible_projection_hash"
    );

    let mut action = valid_record();
    let source = action.payload.object_bindings[0].kernel_ref.clone();
    action.payload.legal_actions[0] = ActionSemanticV1::ChooseEffectNumber {
        actor: action.payload.observation.acting_player,
        source,
        number: 7,
        minimum: 10,
        maximum: 2,
    };
    assert_eq!(
        validate_observed_decision_v1(action).unwrap_err().code(),
        "action_shape"
    );
}

#[test]
fn actor_ambiguous_and_aggregate_combat_actions_fail_closed() {
    let mut actor = valid_record();
    let wrong_actor = other_player(actor.payload.observation.acting_player);
    actor.payload.legal_actions[0] = ActionSemanticV1::Pass { actor: wrong_actor };
    assert_eq!(
        validate_observed_decision_v1(actor).unwrap_err().code(),
        "action_actor"
    );

    let mut ambiguous = valid_record();
    ambiguous.payload.legal_actions[0] = ActionSemanticV1::Ambiguous {
        reason: "mock".to_owned(),
    };
    assert_eq!(
        validate_observed_decision_v1(ambiguous).unwrap_err().code(),
        "unsupported_action"
    );

    let mut aggregate = valid_record();
    aggregate.payload.legal_actions[0] = ActionSemanticV1::DeclareAttackers {
        actor: aggregate.payload.observation.acting_player,
        attackers: Vec::new(),
    };
    assert_eq!(
        validate_observed_decision_v1(aggregate).unwrap_err().code(),
        "unsupported_action"
    );
}

#[test]
fn action_objects_require_unique_exact_bindings() {
    let mut missing = valid_record();
    missing.payload.object_bindings.clear();
    assert_eq!(
        validate_observed_decision_v1(missing).unwrap_err().code(),
        "unbound_action_object"
    );

    let mut duplicate = valid_record();
    duplicate
        .payload
        .object_bindings
        .push(duplicate.payload.object_bindings[0].clone());
    assert_eq!(
        validate_observed_decision_v1(duplicate).unwrap_err().code(),
        "object_binding_id"
    );
}

#[test]
fn commitment_is_stable_under_evidence_and_provenance_record_reordering() {
    let original = validate_observed_decision_v1(valid_record()).unwrap();
    let mut reordered = valid_record();
    reordered.evidence.reverse();
    reordered.provenance.reverse();
    let reordered = validate_observed_decision_v1(reordered).unwrap();
    assert_eq!(
        original.decision_commitment_sha256(),
        reordered.decision_commitment_sha256()
    );
}

#[test]
fn mock_actuator_allows_one_intent_until_a_visible_postcondition() {
    let source = valid_record();
    let mut next = valid_record();
    next.payload.observation.projection.surface.turn += 1;
    refresh_payload_commitments(&mut next);
    let postcondition = observation_postcondition(&source, &next);
    let validated = validate_observed_decision_v1(source).unwrap();
    let mut actuator = MtgoMockActuatorV1::default();
    let intent = actuator
        .submit(&validated, 0, postcondition.clone())
        .unwrap();
    assert_eq!(intent.semantic, validated.legal_actions()[0]);
    assert_eq!(
        actuator
            .submit(&validated, 0, postcondition)
            .unwrap_err()
            .code(),
        "action_pending"
    );

    next.decision_id = "mock-priority-2".to_owned();
    next.frame_id = 2;
    next.frames[0].frame_id = 2;
    next.frames[0].sequence = 2;
    next.evidence[0].sequence = 2;
    next.evidence[1].sequence = 3;
    let MtgoEvidenceSourceV1::FrameRegion { frame_id, .. } = &mut next.evidence[0].source else {
        unreachable!()
    };
    *frame_id = 2;
    let next = validate_observed_decision_v1(next).unwrap();
    actuator.confirm(&next).unwrap();
    assert!(!actuator.has_pending_action());
    assert!(!actuator.is_halted());
}

#[test]
fn missing_mock_postcondition_halts() {
    let record = valid_record();
    let mut hypothetical = record.clone();
    hypothetical.payload.observation.projection.surface.turn += 1;
    refresh_payload_commitments(&mut hypothetical);
    let postcondition = observation_postcondition(&record, &hypothetical);
    let validated = validate_observed_decision_v1(record).unwrap();
    let mut actuator = MtgoMockActuatorV1::default();
    assert_eq!(
        actuator
            .submit(
                &validated,
                0,
                MtgoMockPostconditionV1 {
                    required_visible_leaf: MtgoExpectedVisibleLeafV1 {
                        json_pointer: "/legal_actions/0/action_kind".to_owned(),
                        before_value_sha256: digest('a'),
                        after_value_sha256: digest('b'),
                    },
                },
            )
            .unwrap_err()
            .code(),
        "postcondition_invalid"
    );
    actuator.submit(&validated, 0, postcondition).unwrap();
    assert_eq!(
        actuator.confirm(&validated).unwrap_err().code(),
        "postcondition_missing"
    );
    assert!(actuator.is_halted());
}

#[test]
fn authorization_defaults_deny_leagues_challenges_and_prize_events() {
    let scope = MtgoAuthorizationScopeV1::default();
    assert!(validate_authorization_for_mode_v1(&scope, MtgoRuntimeModeV1::OfflineReplay).is_ok());
    assert!(validate_authorization_for_mode_v1(&scope, MtgoRuntimeModeV1::LeagueInput).is_err());
    assert!(validate_authorization_for_mode_v1(&scope, MtgoRuntimeModeV1::ChallengeInput).is_err());
    assert!(
        validate_authorization_for_mode_v1(&scope, MtgoRuntimeModeV1::OtherPrizeEventInput)
            .is_err()
    );
    let mut hidden_offline = scope.clone();
    hidden_offline.visible_channels_only = false;
    assert_eq!(
        validate_authorization_for_mode_v1(&hidden_offline, MtgoRuntimeModeV1::OfflineReplay)
            .unwrap_err()
            .code(),
        "hidden_channel_forbidden"
    );

    let league_scope = MtgoAuthorizationScopeV1 {
        account_alias_sha256: digest('e'),
        written_permission_sha256: digest('f'),
        league_input: true,
        ..MtgoAuthorizationScopeV1::default()
    };
    assert!(
        validate_authorization_for_mode_v1(&league_scope, MtgoRuntimeModeV1::LeagueInput).is_ok()
    );
    assert_eq!(
        validate_authorization_for_mode_v1(&league_scope, MtgoRuntimeModeV1::ChallengeInput)
            .unwrap_err()
            .code(),
        "mode_not_authorized"
    );
}

#[test]
fn strict_json_rejects_unknown_fields_and_hidden_sources() {
    let value = serde_json::to_value(valid_record()).unwrap();

    let mut unknown = value.clone();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::json!(true));
    assert!(serde_json::from_value::<MtgoObservedDecisionV1>(unknown).is_err());

    let mut hidden = value.clone();
    hidden["evidence"][0]["source"]["source_kind"] = serde_json::json!("process_memory");
    assert!(serde_json::from_value::<MtgoObservedDecisionV1>(hidden).is_err());
}
