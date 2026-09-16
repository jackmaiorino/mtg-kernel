//! Explicit V4 scorer producer for the fresh-lineage (V7 observation-schema)
//! extension to the rich V6 observation. Additive sibling of
//! `flat_policy_v3.rs`; the V3 producer (`encode_current_flat_scoring_decision_owned_v3`,
//! `build_scoring_owned_v3`, `register_extensions_v3`) is never called from
//! here and stays byte-identical.
//!
//! Row layouts for every extension kind except `historical_public_sources`
//! are reused directly from `flat_policy_v3.rs` (`FlatPendingCastObjectCostV3`,
//! `FlatDecisionLocalLibraryV3`, `FlatPendingChosenCreatureCostV3`,
//! `FlatFinalizedChosenCreatureCostV3`, `FlatWardPaymentV3`,
//! `FlatQueuedWardPaymentV3`): nothing about those fields changes in this
//! fork. Only the historical-source row's `context` field widens from
//! `HistoricalSourceContextV6` to `HistoricalSourceContextV7`.

use crate::flat_policy_v2::{
    FlatDecisionEncoderV2, FlatDecisionErrorV2, FlatGlobalsV2, FlatScoringDecisionViewV2,
    FlatScoringOwnedBuffersV2,
};
use crate::flat_policy_v3::{
    FlatDecisionLocalLibraryV3, FlatFinalizedChosenCreatureCostV3, FlatPendingCastObjectCostV3,
    FlatPendingChosenCreatureCostV3, FlatQueuedWardPaymentV3, FlatWardPaymentV3,
};
use crate::policy_observation_v7::HistoricalSourceContextV7;
use crate::rl::StackItemKindV2;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1, FlatActionDecisionBindingV3};

pub const FLAT_POLICY_TYPED_LAYOUT_VERSION_V4: u32 = 4;
pub const FLAT_SCORER_PACKET_VERSION_V4: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatHistoricalPublicSourceV4 {
    pub context: HistoricalSourceContextV7,
    pub stack_item_kind: StackItemKindV2,
    pub model_object_index: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FlatScoringExtensionsV4 {
    /// Only newly introduced physical rows, in V4 registration order. Rows
    /// reused from common public or legitimately known groups are excluded.
    pub appended_object_indices: Vec<u32>,
    pub pending_cast_object_cost: Option<FlatPendingCastObjectCostV3>,
    pub decision_local_library: Option<FlatDecisionLocalLibraryV3>,
    pub historical_public_sources: Vec<FlatHistoricalPublicSourceV4>,
    pub pending_chosen_creature_cost: Option<FlatPendingChosenCreatureCostV3>,
    pub finalized_chosen_creature_costs: Vec<FlatFinalizedChosenCreatureCostV3>,
    pub pending_ward_payment: Option<FlatWardPaymentV3>,
    pub queued_ward_payments: Vec<FlatQueuedWardPaymentV3>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatDecisionV4 {
    pub binding: FlatActionDecisionBindingV3,
    pub globals: FlatGlobalsV2,
    pub extensions: FlatScoringExtensionsV4,
}

/// The common value types are storage layouts, not a V2 identity assertion.
/// Callers cannot construct or extract this view outside the crate.
#[derive(Clone, Copy)]
pub(crate) struct FlatScoringDecisionViewV4<'a> {
    common: FlatScoringDecisionViewV2<'a>,
    extensions: &'a FlatScoringExtensionsV4,
}

impl<'a> FlatScoringDecisionViewV4<'a> {
    pub(crate) fn new(
        common: FlatScoringDecisionViewV2<'a>,
        extensions: &'a FlatScoringExtensionsV4,
    ) -> Self {
        Self { common, extensions }
    }

    pub(crate) fn common(self) -> FlatScoringDecisionViewV2<'a> {
        self.common
    }

    pub(crate) fn extensions(self) -> &'a FlatScoringExtensionsV4 {
        self.extensions
    }
}

#[derive(Default)]
pub(crate) struct FlatDecisionEncoderV4 {
    pub(crate) common: FlatDecisionEncoderV2,
}

impl FastActorSessionV1 {
    /// V4 sibling of `encode_current_flat_scoring_decision_owned_v3`,
    /// reachable (compiles, callable, tested) from the same
    /// `expanded_deck_training_v1`-family scoring entry point shape without
    /// switching any existing caller to it: no production call site invokes
    /// this yet.
    pub(crate) fn encode_current_flat_scoring_decision_owned_v4(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut FlatDecisionEncoderV4,
        buffers: &mut FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<FlatDecisionV4, FlatDecisionErrorV2> {
        encoder.common.build_scoring_owned_v4(self, expected, buffers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::native_flat_tensorizer_v4::{NativeFlatDecisionTensorV4, NativeFlatTensorizerV4};
    use crate::policy_observation_v7::HistoricalSourceContextV7;
    use crate::rl_session::{
        avenging_hunter_undercity_arena_choose_targets_state_v1, hidden_order_triggers_state_v1,
        shuffle_trigger_source_into_library_v1, FastActorResponseV1, FastActorSessionV1,
    };

    #[derive(Default)]
    struct OwnedScoringV4 {
        objects: Vec<crate::flat_policy_v2::FlatObjectCoreV2>,
        relations: Vec<crate::flat_policy_v2::FlatRelationV2>,
        object_subtypes: Vec<crate::flat_policy_v2::FlatObjectSubtypeV2>,
        ability_uses: Vec<crate::flat_policy_v2::FlatObjectAbilityUseV2>,
        goads: Vec<crate::flat_policy_v2::FlatObjectGoadV2>,
        completed_dungeons: Vec<crate::flat_policy_v2::FlatCompletedDungeonV2>,
        effect_subtype_changes: Vec<crate::flat_policy_v2::FlatEffectSubtypeChangeV2>,
        context_path_elements: Vec<crate::flat_policy_v2::FlatContextPathElementV2>,
        actions: Vec<crate::flat_policy_v2::FlatScorerActionCoreV2>,
        action_refs: Vec<crate::flat_policy_v2::FlatScorerActionRefV2>,
    }

    impl OwnedScoringV4 {
        fn encode(&mut self, session: &FastActorSessionV1) -> Result<FlatDecisionV4, FlatDecisionErrorV2> {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!(
                    "fixture is not a live decision: {:?}",
                    session.current_response()
                );
            };
            session.encode_current_flat_scoring_decision_owned_v4(
                expected,
                &mut FlatDecisionEncoderV4::default(),
                &mut FlatScoringOwnedBuffersV2 {
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
                },
            )
        }

        fn view<'a>(&'a self, decision: &'a FlatDecisionV4) -> FlatScoringDecisionViewV4<'a> {
            FlatScoringDecisionViewV4::new(
                FlatScoringDecisionViewV2::new(
                    &decision.globals,
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
                ),
                &decision.extensions,
            )
        }
    }

    /// Hidden `ChooseTargets` fixture (item 14's regression list) driven
    /// through the V4 producer/registry/tensorizer end to end: the row
    /// produced must be a real `PendingTrigger { position: 0 }` historical
    /// source, not the V3-only ad hoc `append_pending_trigger_frozen_source_authority_v3`
    /// mechanism (which item 15 confirms the V4 path never calls -- it does
    /// not exist as a code path here at all).
    #[test]
    fn v4_hidden_choose_targets_produces_a_pending_trigger_row_not_an_ad_hoc_one() {
        let (mut state, hunter, _goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);
        let session = FastActorSessionV1::from_v3_fixture_state(state);

        let extensions_v7 = crate::policy_observation_v7::policy_observation_extensions_v7(
            session.game_state(),
            PlayerId::P0,
        )
        .unwrap();
        let trigger_rows: Vec<_> = extensions_v7
            .historical_public_sources
            .iter()
            .filter(|row| matches!(row.context, HistoricalSourceContextV7::PendingTrigger { .. }))
            .collect();
        assert_eq!(trigger_rows.len(), 1);
        assert_eq!(
            trigger_rows[0].context,
            HistoricalSourceContextV7::PendingTrigger { position: 0 }
        );

        let mut owned = OwnedScoringV4::default();
        let decision = owned.encode(&session).expect("V4 fixture production encoding");
        let hist = &decision.extensions.historical_public_sources;
        assert_eq!(hist.len(), 1);
        assert!(matches!(
            hist[0].context,
            HistoricalSourceContextV7::PendingTrigger { position: 0 }
        ));

        let view = owned.view(&decision);
        let mut tensor = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(view, &mut tensor)
            .expect("V4 tensor fill must succeed for the hidden ChooseTargets fixture");
    }

    fn multi_hidden_scoring_case(count: usize) {
        let (state, _sources) = hidden_order_triggers_state_v1(count);
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut owned = OwnedScoringV4::default();
        let decision = owned
            .encode(&session)
            .unwrap_or_else(|e| panic!("V4 scoring must succeed for {count} simultaneous hidden triggers: {e:?}"));
        let hist = &decision.extensions.historical_public_sources;
        assert_eq!(hist.len(), count);
        let mut positions: Vec<u32> = hist
            .iter()
            .map(|row| match row.context {
                HistoricalSourceContextV7::PendingTrigger { position } => position,
                other => panic!("expected PendingTrigger, found {other:?}"),
            })
            .collect();
        positions.sort_unstable();
        assert_eq!(positions, (0..count as u32).collect::<Vec<_>>());

        // Collision-freedom at the registry layer: every row's registered
        // object index is distinct, and (via the tensorizer, which enforces
        // (group, visible_ordinal) uniqueness through
        // `build_object_projection_for_rows_v2`'s `ObjectOrder` check) the
        // full pipeline accepts all `count` rows without error.
        let object_indices: std::collections::BTreeSet<_> =
            hist.iter().map(|row| row.model_object_index).collect();
        assert_eq!(object_indices.len(), count);

        let view = owned.view(&decision);
        let mut tensor = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(view, &mut tensor)
            .unwrap_or_else(|e| panic!("V4 tensor fill must succeed for {count} simultaneous hidden triggers: {e:?}"));
    }

    #[test]
    fn v4_registry_two_simultaneously_hidden_triggers_are_collision_free() {
        multi_hidden_scoring_case(2);
    }

    #[test]
    fn v4_registry_seven_simultaneously_hidden_triggers_are_collision_free() {
        multi_hidden_scoring_case(7);
    }

    /// Dimension-stability check (plan section 3, step 7): encoding a
    /// common, non-hidden fixture through V3 and V4 must produce numerically
    /// identical object/edge feature widths -- proving mechanically, not
    /// just by reading `feature_contract_v4.json`, that adding the
    /// `PendingTrigger` context arm did not widen the tensor.
    #[test]
    fn v4_and_v3_tensor_widths_are_identical_for_a_shared_fixture() {
        use crate::flat_policy_v3::{FlatDecisionEncoderV3, FlatScoringDecisionViewV3};
        use crate::native_flat_tensorizer_v3::{NativeFlatDecisionTensorV3, NativeFlatTensorizerV3};

        // A real, working ChooseTargets decision with actual objects and no
        // hidden/extension content -- `ready_state()` alone (no objects at
        // all) does not reach an active decision through
        // `FastActorSessionV1::from_v3_fixture_state`'s
        // `advance_to_decision_or_terminal`.
        let (state, _hunter, _goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        let session = FastActorSessionV1::from_v3_fixture_state(state);

        #[derive(Default)]
        struct OwnedScoringV3Local {
            objects: Vec<crate::flat_policy_v2::FlatObjectCoreV2>,
            relations: Vec<crate::flat_policy_v2::FlatRelationV2>,
            object_subtypes: Vec<crate::flat_policy_v2::FlatObjectSubtypeV2>,
            ability_uses: Vec<crate::flat_policy_v2::FlatObjectAbilityUseV2>,
            goads: Vec<crate::flat_policy_v2::FlatObjectGoadV2>,
            completed_dungeons: Vec<crate::flat_policy_v2::FlatCompletedDungeonV2>,
            effect_subtype_changes: Vec<crate::flat_policy_v2::FlatEffectSubtypeChangeV2>,
            context_path_elements: Vec<crate::flat_policy_v2::FlatContextPathElementV2>,
            actions: Vec<crate::flat_policy_v2::FlatScorerActionCoreV2>,
            action_refs: Vec<crate::flat_policy_v2::FlatScorerActionRefV2>,
        }
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("fixture must have an active decision");
        };
        let mut v3_owned = OwnedScoringV3Local::default();
        let v3_decision = session
            .encode_current_flat_scoring_decision_owned_v3(
                expected,
                &mut FlatDecisionEncoderV3::default(),
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut v3_owned.objects,
                    relations: &mut v3_owned.relations,
                    object_subtypes: &mut v3_owned.object_subtypes,
                    ability_uses: &mut v3_owned.ability_uses,
                    goads: &mut v3_owned.goads,
                    completed_dungeons: &mut v3_owned.completed_dungeons,
                    effect_subtype_changes: &mut v3_owned.effect_subtype_changes,
                    context_path_elements: &mut v3_owned.context_path_elements,
                    actions: &mut v3_owned.actions,
                    action_refs: &mut v3_owned.action_refs,
                },
            )
            .unwrap();
        let v3_view = FlatScoringDecisionViewV3::new(
            FlatScoringDecisionViewV2::new(
                &v3_decision.globals,
                &v3_owned.objects,
                &v3_owned.relations,
                &v3_owned.object_subtypes,
                &v3_owned.ability_uses,
                &v3_owned.goads,
                &v3_owned.completed_dungeons,
                &v3_owned.effect_subtype_changes,
                &v3_owned.context_path_elements,
                &v3_owned.actions,
                &v3_owned.action_refs,
            ),
            &v3_decision.extensions,
        );
        let mut v3_tensor = NativeFlatDecisionTensorV3::default();
        NativeFlatTensorizerV3::default()
            .fill(v3_view, &mut v3_tensor)
            .unwrap();

        let mut v4_owned = OwnedScoringV4::default();
        let v4_decision = v4_owned.encode(&session).unwrap();
        let v4_view = v4_owned.view(&v4_decision);
        let mut v4_tensor = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(v4_view, &mut v4_tensor)
            .unwrap();

        assert_eq!(v3_tensor.common.object_features.len(), v4_tensor.common.object_features.len());
        assert_eq!(v3_tensor.common.edge_features.len(), v4_tensor.common.edge_features.len());
        assert_eq!(v3_tensor.common.state.len(), v4_tensor.common.state.len());
        assert_eq!(v3_tensor.common.action_features.len(), v4_tensor.common.action_features.len());
        assert_eq!(
            v3_tensor.common.action_ref_features.len(),
            v4_tensor.common.action_ref_features.len()
        );
    }
}
