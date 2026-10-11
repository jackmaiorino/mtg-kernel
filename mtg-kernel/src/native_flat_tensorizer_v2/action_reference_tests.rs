mod action_reference_reuse_tests {
    use super::*;

    type RefMutation = fn(&mut FlatScorerActionCoreV1, &mut [FlatScorerActionRefV1]);

    fn mixed_menu(
        cases: &[&GoldenCase],
    ) -> (
        FlatGlobalsV1,
        Vec<FlatObjectCoreV1>,
        Vec<FlatScorerActionCoreV1>,
        Vec<FlatScorerActionRefV1>,
    ) {
        let mut objects = Vec::new();
        let mut actions = Vec::new();
        let mut refs = Vec::new();
        let globals = FlatGlobalsV1 {
            acting_player: FlatRelativePlayerV1::SelfPlayer,
            ..FlatGlobalsV1::default()
        };
        for (index, case) in cases.iter().enumerate() {
            let (_, case_objects, mut case_actions, mut case_refs) = parts(&case.flat_input);
            case_actions[0].ref_start = refs.len() as u32;
            for reference in &mut case_refs {
                reference.action_index = index as u32;
                reference.model_object_index += objects.len() as u32;
            }
            objects.extend(case_objects);
            actions.extend(case_actions);
            refs.extend(case_refs);
        }
        (globals, objects, actions, refs)
    }

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    #[test]
    fn mixed_reference_counts_match_sealed_goldens_after_reordering_and_reuse() {
        let document = golden();
        let mut cases: Vec<_> = document.cases.iter().collect();
        let mut output = ActionHalfV1::default();
        // Changing order exercises large-to-small-to-empty transitions in the
        // per-action scratch, then reuses the aggregate allocation next fill.
        for reverse in [false, true] {
            if reverse {
                cases.reverse();
            }
            let (globals, objects, actions, refs) = mixed_menu(&cases);
            output = encode_action_half_reusing_v3(
                view(&globals, &objects, &actions, &refs),
                None,
                &mut Vec::new(),
                false,
                None,
                output,
            )
            .unwrap();
            let mut ref_start = 0;
            let mut object_offset = 0;
            for (index, case) in cases.iter().enumerate() {
                let action_start = index * NATIVE_FLAT_ACTION_FEATURE_DIM_V1;
                assert_eq!(
                    bits(
                        &output.action_features
                            [action_start..action_start + NATIVE_FLAT_ACTION_FEATURE_DIM_V1]
                    ),
                    u32_le_words_from_hex(&case.full_feature_f32_le_hex),
                    "{} action",
                    case.name,
                );
                let ref_end = ref_start + case.action_ref_card_ids.len();
                assert_eq!(
                    bits(
                        &output.action_ref_features[ref_start
                            * NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1
                            ..ref_end * NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1]
                    ),
                    u32_le_words_from_hex(&case.action_ref_feature_f32_le_hex),
                    "{} refs",
                    case.name,
                );
                assert_eq!(
                    &output.action_ref_card_ids[ref_start..ref_end],
                    case.action_ref_card_ids
                );
                assert_eq!(
                    &output.action_ref_action_indices[ref_start..ref_end],
                    vec![index as i64; ref_end - ref_start]
                );
                assert_eq!(
                    &output.action_ref_node_indices[ref_start..ref_end],
                    case.action_ref_node_indices
                        .iter()
                        .map(|node| node + object_offset as i64)
                        .collect::<Vec<_>>(),
                    "{} nodes",
                    case.name,
                );
                ref_start = ref_end;
                object_offset += case.flat_input.objects.len();
            }
            assert_eq!(output.action_ref_card_ids.len(), refs.len());
        }
    }

    #[test]
    fn projected_and_deferred_refs_match_independent_reference_encoder() {
        let document = golden();
        let cases: Vec<_> = document.cases.iter().collect();
        let (globals, objects, actions, refs) = mixed_menu(&cases);
        let projection = ObjectProjectionV2 {
            raw_to_node: (0..objects.len()).rev().map(Some).collect(),
            node_to_raw: (0..objects.len()).rev().collect(),
        };
        let decision = view(&globals, &objects, &actions, &refs);
        let mut json = Vec::new();
        let mut ranges = Vec::new();
        let output = encode_action_half_reusing_v3(
            decision,
            Some(&projection),
            &mut Vec::new(),
            false,
            Some(DeferredActionJsonV1 {
                json: &mut json,
                ranges: &mut ranges,
            }),
            ActionHalfV1::default(),
        )
        .unwrap();
        assert_eq!(ranges.len(), actions.len());
        let mut ref_start = 0;
        for (index, action) in actions.iter().enumerate() {
            let start = action.ref_start as usize;
            let end = start + action.ref_len as usize;
            // The preexisting skip-hash encoder keeps its own resolution and
            // ordering implementation, providing an independent row oracle.
            let expected = encode_action_with_scratch_skip_hash_v1(
                decision,
                index,
                action,
                &refs[start..end],
                Some(&projection),
            )
            .unwrap();
            let action_start = index * NATIVE_FLAT_ACTION_FEATURE_DIM_V1;
            assert_eq!(
                bits(
                    &output.action_features
                        [action_start..action_start + NATIVE_FLAT_ACTION_FEATURE_DIM_V1]
                ),
                bits(&expected.features)
            );
            let ref_end = ref_start + expected.ref_card_ids.len();
            assert_eq!(
                bits(
                    &output.action_ref_features[ref_start * NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1
                        ..ref_end * NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V1]
                ),
                expected
                    .ref_features
                    .iter()
                    .flatten()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                &output.action_ref_card_ids[ref_start..ref_end],
                expected.ref_card_ids
            );
            assert_eq!(
                &output.action_ref_node_indices[ref_start..ref_end],
                expected.ref_node_indices
            );
            let (json_start, json_end) = ranges[index];
            assert_eq!(
                &json[json_start..json_end],
                cases[index].canonical_json.as_bytes()
            );
            ref_start = ref_end;
        }
    }

    #[test]
    fn malformed_refs_keep_error_precedence_and_scratch_recovers() {
        let document = golden();
        let scenarios: [(&str, RefMutation, NativeFlatTensorErrorV1); 6] = [
            (
                "primary-play_land",
                |action, refs| {
                    action.flags = u16::MAX;
                    refs[0].card_token = 0;
                },
                NativeFlatTensorErrorV1::ActionReferenceObject,
            ),
            (
                "primary-play_land",
                |_, refs| {
                    refs[0].projection_role_id = ROLE_CANDIDATE_V1;
                },
                NativeFlatTensorErrorV1::ActionReferenceShape,
            ),
            (
                "primary-play_land",
                |_, refs| {
                    refs[0].model_object_index = u32::MAX;
                },
                NativeFlatTensorErrorV1::ActionReferenceObject,
            ),
            (
                "primary-play_land",
                |action, _| {
                    action.flags = u16::MAX;
                },
                NativeFlatTensorErrorV1::NonCanonicalActionCore,
            ),
            (
                "primary-discard",
                |_, refs| {
                    refs[0].order_index = u16::MAX;
                },
                NativeFlatTensorErrorV1::ActionReferenceShape,
            ),
            (
                "primary-order_triggers",
                |_, refs| {
                    refs[0].associated_order = u16::MAX;
                },
                NativeFlatTensorErrorV1::InvalidTriggerOrder,
            ),
        ];
        for (name, mutate, expected_error) in scenarios {
            let case = document
                .cases
                .iter()
                .find(|case| case.name == name)
                .unwrap();
            let (globals, objects, actions, refs) = parts(&case.flat_input);
            let mut bad_actions = actions.clone();
            let mut bad_refs = refs.clone();
            mutate(&mut bad_actions[0], &mut bad_refs);
            let bad = view(&globals, &objects, &bad_actions, &bad_refs);
            assert_eq!(
                encode_action_with_scratch_skip_hash_v1(bad, 0, &bad_actions[0], &bad_refs, None)
                    .err(),
                Some(expected_error)
            );
            let mut scratch = ActionReferenceScratchV1::default();
            let mut canonical = Vec::new();
            let good = view(&globals, &objects, &actions, &refs);
            encode_action_with_reference_scratch_contract_v3(
                good,
                0,
                &actions[0],
                &refs,
                None,
                &mut canonical,
                false,
                false,
                None,
                &mut scratch,
                None,
            )
            .unwrap();
            let mut aggregate = ActionHalfV1::default();
            assert_eq!(
                encode_action_with_reference_scratch_contract_v3(
                    bad,
                    0,
                    &bad_actions[0],
                    &bad_refs,
                    None,
                    &mut canonical,
                    false,
                    false,
                    None,
                    &mut scratch,
                    Some((&mut aggregate, 0)),
                )
                .err(),
                Some(expected_error)
            );
            assert!(aggregate.action_ref_card_ids.is_empty());
            let result = encode_action_with_reference_scratch_contract_v3(
                good,
                0,
                &actions[0],
                &refs,
                None,
                &mut canonical,
                true,
                false,
                None,
                &mut scratch,
                Some((&mut aggregate, 0)),
            )
            .unwrap();
            assert_eq!(result.canonical_json, case.canonical_json.as_bytes());
            assert_eq!(
                bits(&result.features),
                u32_le_words_from_hex(&case.full_feature_f32_le_hex)
            );
            assert_eq!(
                bits(&aggregate.action_ref_features),
                u32_le_words_from_hex(&case.action_ref_feature_f32_le_hex)
            );
            assert_eq!(aggregate.action_ref_card_ids, case.action_ref_card_ids);
            assert_eq!(
                aggregate.action_ref_node_indices,
                case.action_ref_node_indices
            );
        }
    }
}
