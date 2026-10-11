// Included in the existing tensorizer test module to reuse its frozen fixtures.

#[test]
fn streamed_observation_categories_match_all_frozen_fixture_bytes() {
    for case in full_golden().cases {
        let fixture = &case.rust_fixture;
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            fixture.episode_id,
            fixture.environment_seed,
            256,
            32_768,
            fixture.deck_ids.clone(),
        )
        .unwrap();
        for &selected in &fixture.replay_selected_indices {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("{} ended before its fixture", case.name);
            };
            session
                .step(expected.episode_id, expected.step, selected)
                .unwrap();
        }
        let mut owned = OwnedScoringDecisionV2::from_session(&session);
        let observation = serde_json::from_value(fixture.observation.clone()).unwrap();
        apply_owned_fixture_transform_v2(&mut owned, &fixture.fixture_transform, &observation);
        let projection = build_object_projection_v2(&owned.objects).unwrap();
        let mut bytes = Vec::new();
        write_canonical_observation_v2(owned.view(), &projection, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            case.canonical_observation_json.as_bytes(),
            "{}",
            case.name
        );
    }
}

#[test]
fn streamed_successor_extensions_preserve_all_members_and_ward_omission() {
    use crate::engine::{ChosenCreatureCostZoneV1, CostKind};
    use crate::flat_policy_v3::*;
    use crate::flat_policy_v4::{
        FlatHistoricalPublicSourceV4, FlatScoringDecisionViewV4, FlatScoringExtensionsV4,
    };
    use crate::policy_observation_v6::HistoricalSourceContextV6;
    use crate::policy_observation_v7::HistoricalSourceContextV7;
    use crate::rl::StackItemKindV2;
    use crate::state::CastMethodV4;

    let globals = FlatGlobalsV2::default();
    let objects = [
        FlatObjectCoreV1 {
            card_token: 11,
            zone: Some(FlatZoneV1::Battlefield),
            ..FlatObjectCoreV1::default()
        },
        FlatObjectCoreV1 {
            card_token: NATIVE_FLAT_MAX_CARD_TOKEN_V2,
            owner: FlatRelativePlayerV1::Opponent,
            controller: FlatRelativePlayerV1::Opponent,
            zone: Some(FlatZoneV1::Hand),
            ..FlatObjectCoreV1::default()
        },
    ];
    let common = view(&globals, &objects, &[], &[]);
    let payment = FlatWardPaymentV3 {
        targeting_stack_index: u32::MAX,
        targeting_source_object: 0,
        ward_source_object: 1,
        payer: FlatRelativePlayerV1::Opponent,
        generic: u8::MAX,
    };
    let full = FlatScoringExtensionsV3 {
        pending_cast_object_cost: Some(FlatPendingCastObjectCostV3 {
            source_object: 0,
            controller: FlatRelativePlayerV1::SelfPlayer,
            cast_method: CastMethodV4::Escape,
            cost_kind: CostKind::ExileFromGraveyard,
            required_count: 3,
            selected_objects: vec![1, 0, 1],
            remaining_count: 0,
        }),
        decision_local_library: Some(FlatDecisionLocalLibraryV3 {
            chooser: FlatRelativePlayerV1::SelfPlayer,
            library_owner: FlatRelativePlayerV1::Opponent,
            object_indices: vec![1, 0, 1],
            public_class_ordinals: vec![1, 0, 1],
        }),
        historical_public_sources: vec![
            FlatHistoricalPublicSourceV3 {
                context: HistoricalSourceContextV6::Stack { stack_index: 10 },
                stack_item_kind: StackItemKindV2::ActivatedAbility,
                model_object_index: 0,
            },
            FlatHistoricalPublicSourceV3 {
                context: HistoricalSourceContextV6::PendingEffect,
                stack_item_kind: StackItemKindV2::MadnessOffer,
                model_object_index: 1,
            },
        ],
        pending_chosen_creature_cost: Some(FlatPendingChosenCreatureCostV3 {
            source_object: 1,
            controller: FlatRelativePlayerV1::Opponent,
            selected_zone: ChosenCreatureCostZoneV1::Hand,
        }),
        finalized_chosen_creature_costs: vec![
            FlatFinalizedChosenCreatureCostV3 {
                stack_index: 2,
                source_object: 0,
                chosen_object: 1,
                power_lki: i32::MIN,
            },
            FlatFinalizedChosenCreatureCostV3 {
                stack_index: 1,
                source_object: 1,
                chosen_object: 0,
                power_lki: i32::MAX,
            },
        ],
        pending_ward_payment: Some(payment.clone()),
        queued_ward_payments: vec![FlatQueuedWardPaymentV3 {
            stack_index: 10,
            payment,
        }],
        ..FlatScoringExtensionsV3::default()
    };
    let mut bytes = Vec::new();
    // Alternate empty, full and shrinking inputs in the same output buffer.
    for extensions in [
        FlatScoringExtensionsV3::default(),
        full.clone(),
        FlatScoringExtensionsV3::default(),
        full,
    ] {
        let v3 = FlatScoringDecisionViewV3::new(common, &extensions);
        bytes.clear();
        streaming_v3::write_extensions_v3(v3, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            serde_json::to_vec(&canonical_extensions_v3(v3).unwrap()).unwrap()
        );
        if extensions.pending_ward_payment.is_none() {
            let parsed: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(parsed.get("pending_ward_payment").is_none());
            assert!(parsed.get("queued_ward_payments").is_none());
        }
        let mut v4_extensions = FlatScoringExtensionsV4 {
            pending_cast_object_cost: extensions.pending_cast_object_cost.clone(),
            decision_local_library: extensions.decision_local_library.clone(),
            historical_public_sources: extensions
                .historical_public_sources
                .iter()
                .map(|source| FlatHistoricalPublicSourceV4 {
                    context: source.context.into(),
                    stack_item_kind: source.stack_item_kind,
                    model_object_index: source.model_object_index,
                })
                .collect(),
            pending_chosen_creature_cost: extensions.pending_chosen_creature_cost.clone(),
            finalized_chosen_creature_costs: extensions.finalized_chosen_creature_costs.clone(),
            pending_ward_payment: extensions.pending_ward_payment.clone(),
            queued_ward_payments: extensions.queued_ward_payments.clone(),
            ..FlatScoringExtensionsV4::default()
        };
        v4_extensions
            .historical_public_sources
            .push(FlatHistoricalPublicSourceV4 {
                context: HistoricalSourceContextV7::PendingTrigger { position: u32::MAX },
                stack_item_kind: StackItemKindV2::TriggeredAbility,
                model_object_index: 1,
            });
        let v4 = FlatScoringDecisionViewV4::new(common, &v4_extensions);
        bytes.clear();
        streaming_v3::write_extensions_v4(v4, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            serde_json::to_vec(&canonical_extensions_v4(v4).unwrap()).unwrap()
        );
    }
}

#[test]
fn streamed_extensions_preserve_errors_before_common_observation_errors() {
    use crate::engine::CostKind;
    use crate::flat_policy_v3::{
        FlatPendingCastObjectCostV3, FlatScoringDecisionViewV3, FlatScoringExtensionsV3,
    };
    use crate::flat_policy_v4::{FlatScoringDecisionViewV4, FlatScoringExtensionsV4};
    use crate::state::CastMethodV4;
    let globals = FlatGlobalsV2::default();
    let objects = [FlatObjectCoreV1 {
        card_token: 0,
        visible_ordinal: 1,
        zone: Some(FlatZoneV1::Hand),
        ..FlatObjectCoreV1::default()
    }];
    let common = view(&globals, &objects, &[], &[]);
    let extensions = FlatScoringExtensionsV3 {
        pending_cast_object_cost: Some(FlatPendingCastObjectCostV3 {
            source_object: 0,
            controller: FlatRelativePlayerV1::None,
            cast_method: CastMethodV4::Normal,
            cost_kind: CostKind::DiscardCards,
            required_count: 1,
            selected_objects: vec![u32::MAX],
            remaining_count: 1,
        }),
        ..FlatScoringExtensionsV3::default()
    };
    let v3 = FlatScoringDecisionViewV3::new(common, &extensions);
    let v4_extensions = FlatScoringExtensionsV4 {
        pending_cast_object_cost: extensions.pending_cast_object_cost.clone(),
        ..FlatScoringExtensionsV4::default()
    };
    let v4 = FlatScoringDecisionViewV4::new(common, &v4_extensions);
    let expected = canonical_extensions_v3(v3).unwrap_err();
    assert_eq!(expected, NativeFlatTensorErrorV2::CardTokenRange);
    assert_eq!(
        streaming_v3::write_extensions_v3(v3, &mut Vec::new()),
        Err(expected)
    );
    assert_eq!(
        streaming_v3::write_extensions_v4(v4, &mut Vec::new()),
        canonical_extensions_v4(v4).map(|_| ())
    );
    let projection = ObjectProjectionV2 {
        raw_to_node: vec![Some(0)],
        node_to_raw: vec![0],
    };
    assert_eq!(
        write_canonical_observation_v2(common, &projection, &mut Vec::new()),
        Err(NativeFlatTensorErrorV2::ObjectOrder),
        "the malformed common observation must have a distinct error"
    );
    let mut output = Vec::new();
    let mut base = b"unwritten".to_vec();
    assert_eq!(
        write_canonical_observation_with_streamed_extensions_reusing_v2(
            common,
            &projection,
            |out| streaming_v3::write_extensions_v3(v3, out),
            &mut output,
            &mut base,
        ),
        Err(expected)
    );
    assert_eq!(
        base, b"unwritten",
        "extension errors must precede common writes"
    );
}

#[test]
fn streamed_categories_preserve_reference_errors_for_malformed_rows() {
    let globals = FlatGlobalsV2::default();
    let objects = [FlatObjectCoreV1 {
        card_token: 1,
        zone: Some(FlatZoneV1::Battlefield),
        ..FlatObjectCoreV1::default()
    }];
    let projection = ObjectProjectionV2 {
        raw_to_node: vec![Some(0)],
        node_to_raw: vec![0],
    };
    // Multiple simultaneous faults distinguish construction order from JSON
    // field order; every category must report its original first error.
    for role in [
        FlatRelationRoleV2::StackTarget,
        FlatRelationRoleV2::CombatAttacker,
        FlatRelationRoleV2::EffectSource,
        FlatRelationRoleV2::Permission,
        FlatRelationRoleV2::AttachedTo,
    ] {
        let rows = [FlatRelationV2 {
            role,
            primary_order: 2,
            target_object: Some(u32::MAX),
            source_object: None,
            payload: FlatRelationPayloadV2::None,
            ..FlatRelationV2::default()
        }];
        let decision = FlatScoringDecisionViewV1::new(
            &globals,
            &objects,
            &rows,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
        );
        let mut bytes = Vec::new();
        let (actual, expected) = match role {
            FlatRelationRoleV2::StackTarget => (
                streaming_v3::stack(decision, &projection, &mut bytes),
                canonical_stack_v2(decision, &projection),
            ),
            FlatRelationRoleV2::CombatAttacker => (
                streaming_v3::combat(decision, &projection, &mut bytes),
                canonical_combat_v2(decision, &projection),
            ),
            FlatRelationRoleV2::EffectSource => (
                streaming_v3::effects(decision, &projection, &mut bytes),
                canonical_effects_v2(decision, &projection),
            ),
            FlatRelationRoleV2::Permission => (
                streaming_v3::permissions(decision, &projection, &mut bytes),
                canonical_permissions_v2(decision, &projection),
            ),
            FlatRelationRoleV2::AttachedTo => (
                streaming_v3::object_relations(decision, &projection, &mut bytes),
                canonical_object_relations_v2(decision, &projection),
            ),
            _ => unreachable!(),
        };
        assert!(expected.is_err());
        assert_eq!(actual, expected.map(|_| ()), "{role:?}");
    }
}
