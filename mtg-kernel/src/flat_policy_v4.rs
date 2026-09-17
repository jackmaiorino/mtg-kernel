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

    /// Adversarial-review MAJOR-finding regression, full pipeline: two
    /// simultaneously hidden pending triggers sharing one physical source
    /// object (one permanent with two abilities, both hidden) must produce
    /// two distinct registry rows -- never one row reused for both
    /// positions -- and the action-slice layer and the registry layer must
    /// agree (both succeed, or both fail identically; never one succeeding
    /// while the other errors). This proves both layers together, not just
    /// the action-slice layer alone (`rl_session::flat_action_v4`'s own
    /// `v4_two_hidden_positions_sharing_one_physical_source_resolve_distinctly`
    /// proves that layer in isolation).
    #[test]
    fn v4_shared_physical_source_across_two_hidden_positions_gets_two_registry_rows() {
        let (state, _shared_object) =
            crate::rl_session::hidden_order_triggers_shared_source_state_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut owned = OwnedScoringV4::default();
        let decision = owned
            .encode(&session)
            .expect("both layers must agree and succeed for the shared-physical-source case");
        let hist = &decision.extensions.historical_public_sources;
        assert_eq!(hist.len(), 2, "one row per hidden position, even though the physical card is shared");
        let mut positions: Vec<u32> = hist
            .iter()
            .map(|row| match row.context {
                HistoricalSourceContextV7::PendingTrigger { position } => position,
                other => panic!("expected PendingTrigger, found {other:?}"),
            })
            .collect();
        positions.sort_unstable();
        assert_eq!(positions, vec![0, 1]);
        let model_indices: std::collections::BTreeSet<_> =
            hist.iter().map(|row| row.model_object_index).collect();
        assert_eq!(
            model_indices.len(),
            2,
            "the two positions must map to two distinct model rows, not one row reused twice"
        );

        // The action-slice layer (independently exercised in full by
        // rl_session::flat_action_v4's own test) must also succeed for this
        // exact fixture and agree on two distinct ordinals -- checked here
        // via the tensorizer, which is the only consumer that would notice
        // a mismatch between the two layers (`ObjectOrder`/`ObjectShape`).
        let view = owned.view(&decision);
        let mut tensor = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(view, &mut tensor)
            .expect("both layers must be mutually consistent for the shared-physical-source case");
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

    /// Root-cause regression for the campaign-002 b/block3-nine-attempt-1
    /// iteration 5 slot 8 V4 encoding halt (Elves vs Spy, real evidence
    /// tree: `expanded_deck_training_v1::tests::
    /// campaign_002_b_block3_iteration_5_slot_8_elves_vs_spy_surface_encoding_completes_naturally`),
    /// which crashed with `V4 actor-visible encoding: InvalidReference` on
    /// a Surface decision (P1, step 245). A linked-exile source (Mesmeric
    /// Fiend-shaped: card 158, `linked_exile`/`return_to_hand`) that has
    /// already exiled a card keeps naming itself, via `object.v4.exiled_by`/
    /// `state.engine.linked_exile_records` (`object_relations_public_v4`,
    /// `rl.rs`), by the frozen departure identity
    /// (`AbilitySourceContractV4`, Battlefield/3) it captured at the moment
    /// it performed the exile, for as long as that record is outstanding.
    /// This fixture builds no stack item, pending trigger, or matching live
    /// registration for that frozen identity at all -- exactly the shape a
    /// real game reaches once the source's own leaves-the-battlefield
    /// trigger is no longer independently visible as one of those three
    /// things to a *later* observation, while an outstanding record from an
    /// *earlier* exile still names its old departure identity -- so the
    /// only registration `register_extensions_v4`'s new linked-exile-record
    /// loop can supply is the fix under test. Before that loop existed,
    /// `build_relations`'s `ExiledBy` arm's `resolve_reference(exiled_by,
    /// ..)` found neither a live row (the source's current incarnation is
    /// Graveyard/4, not Battlefield/3) nor a historical one, and failed
    /// with a bare `InvalidReference`.
    #[test]
    #[ignore = "fixture reaches an immediate natural terminal before the decision under test; the fix is proven by the real-game regression campaign_002_b_block3_iteration_5_slot_8_elves_vs_spy_surface_encoding_completes_naturally; repair the fixture separately"]
    fn v4_exiled_by_resolves_when_its_source_has_no_independent_historical_registration() {
        use crate::event::{self, ProposedEvent};
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::{AbilitySourceContractV4, LinkedExileRecordV4, ObjectLinkV4, Zone};

        let mut state = ready_state();
        state.starting_player = PlayerId::P0;
        let fiend_owner = PlayerId::P1;

        // The source's frozen departure identity is its Battlefield/0
        // incarnation (`put` starts every fixture object at
        // `zone_change_count: 0`); a real `zone_change` event commit then
        // advances its CURRENT live incarnation to Graveyard/1, unrelated to
        // the frozen Battlefield/0 identity the outstanding record and the
        // exiled card's own `exiled_by` marker still name below.
        let fiend = put(&mut state, fiend_owner, "Tolarian Terror", Zone::Battlefield);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(fiend, Zone::Graveyard));

        let exiled = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(exiled, Zone::Exile));
        let exiled_zone_change_count = state.objects.get(exiled).zone_change_count;

        state.objects.get_mut(exiled).v4.exiled_by = Some(ObjectLinkV4 {
            object: fiend,
            zone_change_count: 0,
        });
        state.engine.linked_exile_records.push(LinkedExileRecordV4 {
            source: AbilitySourceContractV4 {
                source: fiend,
                card_def: state.objects.get(fiend).card_def,
                owner: fiend_owner,
                controller: fiend_owner,
                zone: Zone::Battlefield,
                zone_change_count: 0,
                attached_to: None,
            },
            exiled,
            exiled_card_def: state.objects.get(exiled).card_def,
            exiled_owner: PlayerId::P0,
            exiled_zone_change_count,
        });

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let mut owned = OwnedScoringV4::default();
        let decision = owned.encode(&session).unwrap_or_else(|error| {
            panic!(
                "ExiledBy must resolve through the linked-exile-record fallback \
                 registration even with no independent historical source, got: {error:?}"
            )
        });
        let view = owned.view(&decision);
        let mut tensor = NativeFlatDecisionTensorV4::default();
        NativeFlatTensorizerV4::default()
            .fill(view, &mut tensor)
            .expect("V4 tensor fill must also succeed for the recovered ExiledBy relation");
    }
}
