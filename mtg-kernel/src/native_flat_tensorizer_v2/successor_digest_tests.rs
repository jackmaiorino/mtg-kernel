use super::*;
use crate::flat_policy_v2::FlatGlobalsV2;
use crate::flat_policy_v3::{FlatScoringDecisionViewV3, FlatScoringExtensionsV3};
use crate::flat_policy_v4::{FlatScoringDecisionViewV4, FlatScoringExtensionsV4};

// No-reference actions keep the fixture small while supplying distinct
// canonical message lengths and contents, including duplicate menu rows.
fn action_menu(order: &[usize]) -> Vec<FlatScorerActionCoreV1> {
    let mut choices = vec![
        FlatScorerActionCoreV1::default(),
        FlatScorerActionCoreV1 {
            kind: FlatScorerActionKindV1::ChooseOptionalCostUse,
            ..FlatScorerActionCoreV1::default()
        },
        FlatScorerActionCoreV1 {
            kind: FlatScorerActionKindV1::ChooseOptionalCostUse,
            flags: FLAT_ACTION_FLAG_USE_COST_V1,
            ..FlatScorerActionCoreV1::default()
        },
    ];
    choices.extend((1..=3).map(|optional_cost_choice| FlatScorerActionCoreV1 {
        kind: FlatScorerActionKindV1::ChooseOptionalCostWhich,
        optional_cost_choice,
        ..FlatScorerActionCoreV1::default()
    }));
    order.iter().map(|&index| choices[index]).collect()
}

fn common<'a>(
    globals: &'a FlatGlobalsV2,
    actions: &'a [FlatScorerActionCoreV1],
) -> FlatScoringDecisionViewV1<'a> {
    FlatScoringDecisionViewV1::new(
        globals,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        actions,
        &[],
    )
}

fn serial(
    decision: FlatScoringDecisionViewV1<'_>,
    v4: bool,
) -> Result<NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2> {
    if v4 {
        fill_native_flat_decision_tensors_v4(FlatScoringDecisionViewV4::new(
            decision,
            &FlatScoringExtensionsV4::default(),
        ))
    } else {
        fill_native_flat_decision_tensors_v3(FlatScoringDecisionViewV3::new(
            decision,
            &FlatScoringExtensionsV3::default(),
        ))
    }
}

fn batched(
    decision: FlatScoringDecisionViewV1<'_>,
    v4: bool,
    scratch: &mut NativeFlatTensorScratchV3,
    output: &mut NativeFlatDecisionTensorV2,
) -> Result<(), NativeFlatTensorErrorV2> {
    if v4 {
        fill_native_flat_decision_tensors_v4_with_scratch(
            FlatScoringDecisionViewV4::new(decision, &FlatScoringExtensionsV4::default()),
            scratch,
            output,
        )
    } else {
        fill_native_flat_decision_tensors_v3_with_scratch(
            FlatScoringDecisionViewV3::new(decision, &FlatScoringExtensionsV3::default()),
            scratch,
            output,
        )
    }
}

fn assert_tensor_bits(actual: &NativeFlatDecisionTensorV2, expected: &NativeFlatDecisionTensorV2) {
    assert_eq!(actual, expected);
    for (name, actual, expected) in [
        ("state", &actual.state, &expected.state),
        (
            "objects",
            &actual.object_features,
            &expected.object_features,
        ),
        ("edges", &actual.edge_features, &expected.edge_features),
        (
            "actions",
            &actual.action_features,
            &expected.action_features,
        ),
        (
            "action_refs",
            &actual.action_ref_features,
            &expected.action_ref_features,
        ),
    ] {
        assert_eq!(
            actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            "{name}",
        );
    }
}

#[test]
fn successor_batched_digests_match_serial_across_menu_orders_and_sizes() {
    let globals = FlatGlobalsV2::default();
    let mut scratch = NativeFlatTensorScratchV3::default();
    let mut output = NativeFlatDecisionTensorV2::default();
    for order in [
        &[0, 1, 2, 3, 4, 5][..],
        &[5][..],
        &[2, 0, 2][..],
        &[5, 4, 3, 2, 1, 0, 5][..],
        &[0][..],
    ] {
        let actions = action_menu(order);
        for v4 in [false, true] {
            let decision = common(&globals, &actions);
            let expected = serial(decision, v4).unwrap();
            batched(decision, v4, &mut scratch, &mut output).unwrap();
            assert_tensor_bits(&output, &expected);
            assert_eq!(scratch.slot.action_ranges.len(), actions.len());
            assert_eq!(
                scratch.digests.blocks.len(),
                ACTION_HASH_BLOCK_COUNT_V1 * (1 + actions.len()),
            );
            // Check every deferred message against the serial SHA-512 path,
            // including its six-counter ordering and feature-tail position.
            for (index, &(start, end)) in scratch.slot.action_ranges.iter().enumerate() {
                let (blocks, tail) = action_hash_features_v1(&scratch.slot.action_json[start..end]);
                let block_start = ACTION_HASH_BLOCK_COUNT_V1 * (index + 1);
                assert_eq!(
                    &scratch.digests.blocks[block_start..block_start + ACTION_HASH_BLOCK_COUNT_V1],
                    &blocks,
                );
                let row_start = index * NATIVE_FLAT_ACTION_FEATURE_DIM_V1;
                let actual_tail = &output.action_features[row_start
                    + NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V1
                    ..row_start + NATIVE_FLAT_ACTION_FEATURE_DIM_V1];
                assert_eq!(
                    actual_tail.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                    tail.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                );
            }
        }
    }
}

#[test]
fn successor_batched_digests_discard_partial_messages_after_late_error() {
    let globals = FlatGlobalsV2::default();
    for v4 in [false, true] {
        let mut scratch = NativeFlatTensorScratchV3::default();
        let mut output = NativeFlatDecisionTensorV2::default();
        let first = action_menu(&[0, 1, 2, 3, 4, 5]);
        batched(common(&globals, &first), v4, &mut scratch, &mut output).unwrap();
        let before = output.clone();
        let mut invalid = action_menu(&[5, 4, 0]);
        invalid.last_mut().unwrap().ref_start = u32::MAX;
        let expected_error = serial(common(&globals, &invalid), v4).unwrap_err();
        assert_eq!(
            batched(common(&globals, &invalid), v4, &mut scratch, &mut output),
            Err(expected_error),
        );
        assert_tensor_bits(&output, &before);
        // The internal scratch API can be reused after failure. Public
        // tensorizers retain their existing fail-closed poisoning behavior.
        let next = action_menu(&[1]);
        let decision = common(&globals, &next);
        batched(decision, v4, &mut scratch, &mut output).unwrap();
        assert_tensor_bits(&output, &serial(decision, v4).unwrap());
        assert_eq!(scratch.slot.action_ranges.len(), 1);
        assert_eq!(scratch.digests.blocks.len(), ACTION_HASH_BLOCK_COUNT_V1 * 2);
    }
}
