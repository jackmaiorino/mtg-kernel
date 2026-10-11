use super::{
    fill_native_flat_decision_tensors_v3, fill_native_flat_decision_tensors_v3_with_scratch,
    fill_native_flat_decision_tensors_v4, fill_native_flat_decision_tensors_v4_with_scratch,
    NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2, NativeFlatTensorScratchV3,
    NATIVE_FLAT_OBJECT_FEATURE_DIM_V2,
};
use crate::flat_policy_v2::*;
use crate::flat_policy_v3::{
    FlatDecisionEncoderV3, FlatScoringDecisionViewV3, FlatScoringExtensionsV3,
};
use crate::flat_policy_v4::{
    FlatDecisionEncoderV4, FlatScoringDecisionViewV4, FlatScoringExtensionsV4,
};
use crate::native_flat_tensorizer_v3::{
    monstrous_emergence_cost_fixture_v3, monstrous_emergence_paid_fixture_v3,
    NativeFlatDecisionTensorV3, NativeFlatTensorizerV3,
};
use crate::native_flat_tensorizer_v4::{NativeFlatDecisionTensorV4, NativeFlatTensorizerV4};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::GameState;

#[derive(Clone, Default)]
struct OwnedDecision {
    globals: FlatGlobalsV2,
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    action_refs: Vec<FlatScorerActionRefV2>,
    extensions_v3: FlatScoringExtensionsV3,
    extensions_v4: FlatScoringExtensionsV4,
}

impl OwnedDecision {
    fn buffers(&mut self) -> FlatScoringOwnedBuffersV2<'_> {
        FlatScoringOwnedBuffersV2 {
            objects: &mut self.objects,
            relations: &mut self.relations,
            object_subtypes: &mut self.object_subtypes,
            ability_uses: &mut self.ability_uses,
            goads: &mut self.goads,
            completed_dungeons: &mut self.completed_dungeons,
            effect_subtype_changes: &mut self.effect_subtype_changes,
            context_path_elements: &mut self.context_path_elements,
            actions: &mut self.actions,
            action_refs: &mut self.action_refs,
        }
    }

    fn from_state(state: GameState, v4: bool) -> Self {
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("fixture must have a live decision");
        };
        let mut owned = Self::default();
        if v4 {
            let encoded = session
                .encode_current_flat_scoring_decision_owned_v4(
                    decision,
                    &mut FlatDecisionEncoderV4::default(),
                    &mut owned.buffers(),
                )
                .unwrap();
            owned.globals = encoded.globals;
            owned.extensions_v4 = encoded.extensions;
        } else {
            let encoded = session
                .encode_current_flat_scoring_decision_owned_v3(
                    decision,
                    &mut FlatDecisionEncoderV3::default(),
                    &mut owned.buffers(),
                )
                .unwrap();
            owned.globals = encoded.globals;
            owned.extensions_v3 = encoded.extensions;
        }
        owned
    }

    // A valid scorer packet with no objects, references, or relations exercises
    // the mandatory zero-object sentinel after buffers have held real rows.
    fn empty_pass() -> Self {
        Self {
            actions: vec![FlatScorerActionCoreV2::default()],
            ..Self::default()
        }
    }

    fn common(&self) -> FlatScoringDecisionViewV2<'_> {
        FlatScoringDecisionViewV2::new(
            &self.globals,
            &self.objects,
            &self.relations,
            &self.object_subtypes,
            &self.ability_uses,
            &self.goads,
            &self.completed_dungeons,
            &self.effect_subtype_changes,
            &self.context_path_elements,
            &self.actions,
            &self.action_refs,
        )
    }

    fn v3(&self) -> FlatScoringDecisionViewV3<'_> {
        FlatScoringDecisionViewV3::new(self.common(), &self.extensions_v3)
    }

    fn v4(&self) -> FlatScoringDecisionViewV4<'_> {
        FlatScoringDecisionViewV4::new(self.common(), &self.extensions_v4)
    }
}

fn assert_tensor_bits(actual: &NativeFlatDecisionTensorV2, expected: &NativeFlatDecisionTensorV2) {
    macro_rules! floats {
        ($($field:ident),+ $(,)?) => {$(
            assert_eq!(
                actual.$field.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                expected.$field.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                stringify!($field),
            );
        )+};
    }
    macro_rules! integers {
        ($($field:ident),+ $(,)?) => {$(
            assert_eq!(actual.$field, expected.$field, stringify!($field));
        )+};
    }
    floats!(
        state,
        object_features,
        edge_features,
        action_features,
        action_ref_features
    );
    integers!(
        object_card_ids,
        object_groups,
        object_node_ids,
        edge_source_indices,
        edge_target_indices,
        action_ref_card_ids,
        action_ref_action_indices,
        action_ref_node_indices,
    );
}

#[test]
fn reused_successor_tensors_match_fresh_bits_across_shrinking_and_growing_decisions() {
    for v4 in [false, true] {
        let cost = OwnedDecision::from_state(monstrous_emergence_cost_fixture_v3(false).0, v4);
        let paid =
            OwnedDecision::from_state(monstrous_emergence_paid_fixture_v3(false, Some(3)).0, v4);
        let library = OwnedDecision::from_state(
            crate::policy_observation_v6::tests::forest_search_state(false, "Mountain"),
            v4,
        );
        let empty = OwnedDecision::empty_pass();
        let mut scratch = NativeFlatTensorScratchV3::default();
        let mut output = NativeFlatDecisionTensorV2::default();
        let mut shapes = std::collections::BTreeSet::new();
        // Revisit each shape after both scratch/output allocations have held
        // other shapes, including extension edges and decision-local objects.
        for owned in [
            &cost, &paid, &empty, &library, &empty, &cost, &library, &paid,
        ] {
            let expected = if v4 {
                fill_native_flat_decision_tensors_v4(owned.v4()).unwrap()
            } else {
                fill_native_flat_decision_tensors_v3(owned.v3()).unwrap()
            };
            if v4 {
                fill_native_flat_decision_tensors_v4_with_scratch(
                    owned.v4(),
                    &mut scratch,
                    &mut output,
                )
                .unwrap();
            } else {
                fill_native_flat_decision_tensors_v3_with_scratch(
                    owned.v3(),
                    &mut scratch,
                    &mut output,
                )
                .unwrap();
            }
            assert_tensor_bits(&output, &expected);
            shapes.insert((
                output.object_card_ids.len(),
                output.edge_source_indices.len(),
                output.action_features.len(),
            ));
            if std::ptr::eq(owned, &empty) {
                assert_eq!(output.object_card_ids, [0]);
                assert_eq!(output.object_groups, [0]);
                assert_eq!(output.object_node_ids, [0]);
                assert_eq!(
                    output.object_features,
                    vec![0.0; NATIVE_FLAT_OBJECT_FEATURE_DIM_V2]
                );
                assert!(output.edge_features.is_empty());
                assert!(output.edge_source_indices.is_empty());
                assert!(output.edge_target_indices.is_empty());
                assert!(output.action_ref_features.is_empty());
                assert!(output.action_ref_card_ids.is_empty());
                assert!(output.action_ref_action_indices.is_empty());
                assert!(output.action_ref_node_indices.is_empty());
            }
        }
        assert!(
            shapes.len() >= 3,
            "fixture must exercise different tensor sizes"
        );
    }
}

#[test]
fn reused_successor_tensorizers_preserve_last_output_on_late_error_and_poison() {
    for v4 in [false, true] {
        let valid = OwnedDecision::from_state(monstrous_emergence_cost_fixture_v3(false).0, v4);
        let mut malformed = valid.clone();
        // Observation and tensor rows can be built before the final action's
        // invalid reference range is rejected. This catches partial commits.
        malformed.actions.last_mut().unwrap().ref_start = u32::MAX;
        if v4 {
            let mut tensorizer = NativeFlatTensorizerV4::default();
            let mut output = NativeFlatDecisionTensorV4::default();
            tensorizer.fill(valid.v4(), &mut output).unwrap();
            let before = output.common.clone();
            let expected_error = fill_native_flat_decision_tensors_v4(malformed.v4()).unwrap_err();
            assert_ne!(expected_error, NativeFlatTensorErrorV2::Poisoned);
            assert_eq!(
                tensorizer.fill(malformed.v4(), &mut output),
                Err(expected_error)
            );
            assert_tensor_bits(&output.common, &before);
            assert_eq!(
                tensorizer.fill(valid.v4(), &mut output),
                Err(NativeFlatTensorErrorV2::Poisoned)
            );
            assert_tensor_bits(&output.common, &before);
            NativeFlatTensorizerV4::default()
                .fill(valid.v4(), &mut output)
                .unwrap();
            assert_tensor_bits(&output.common, &before);
        } else {
            let mut tensorizer = NativeFlatTensorizerV3::default();
            let mut output = NativeFlatDecisionTensorV3::default();
            tensorizer.fill(valid.v3(), &mut output).unwrap();
            let before = output.common.clone();
            let expected_error = fill_native_flat_decision_tensors_v3(malformed.v3()).unwrap_err();
            assert_ne!(expected_error, NativeFlatTensorErrorV2::Poisoned);
            assert_eq!(
                tensorizer.fill(malformed.v3(), &mut output),
                Err(expected_error)
            );
            assert_tensor_bits(&output.common, &before);
            assert_eq!(
                tensorizer.fill(valid.v3(), &mut output),
                Err(NativeFlatTensorErrorV2::Poisoned)
            );
            assert_tensor_bits(&output.common, &before);
            NativeFlatTensorizerV3::default()
                .fill(valid.v3(), &mut output)
                .unwrap();
            assert_tensor_bits(&output.common, &before);
        }
    }
}
