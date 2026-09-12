//! Explicit V3 scorer producer for rich V6 observations.
//!
//! Shared row layouts retain Net8 dimensions, while this entry point and its
//! action binding own a distinct identity. Private source/choice authority is
//! resolved to model row indices before any scorer receives these extensions.

use crate::engine::CostKind;
use crate::flat_policy_v2::{
    FlatDecisionEncoderV2, FlatDecisionErrorV2, FlatGlobalsV2, FlatRelativePlayerV2,
    FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
};
use crate::policy_observation_v6::HistoricalSourceContextV6;
use crate::rl::StackItemKindV2;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1, FlatActionDecisionBindingV3};
use crate::state::CastMethodV4;

pub const FLAT_POLICY_TYPED_LAYOUT_VERSION_V3: u32 = 3;
pub const FLAT_SCORER_PACKET_VERSION_V3: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatPendingCastObjectCostV3 {
    pub source_object: u32,
    pub controller: FlatRelativePlayerV2,
    pub cast_method: CastMethodV4,
    pub cost_kind: CostKind,
    pub required_count: u32,
    pub selected_objects: Vec<u32>,
    pub remaining_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatDecisionLocalLibraryV3 {
    pub chooser: FlatRelativePlayerV2,
    pub library_owner: FlatRelativePlayerV2,
    /// Canonical physical-choice order. Each entry is a model row handle,
    /// not a library position. Identical new rows share a public class ordinal.
    pub object_indices: Vec<u32>,
    /// Rank among distinct observable card classes, independent of hidden
    /// library order and of legitimately known positions on reused rows.
    pub public_class_ordinals: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatHistoricalPublicSourceV3 {
    pub context: HistoricalSourceContextV6,
    pub stack_item_kind: StackItemKindV2,
    pub model_object_index: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlatScoringExtensionsV3 {
    /// Only newly introduced physical rows, in V3 registration order. Rows
    /// reused from common public or legitimately known groups are excluded.
    pub appended_object_indices: Vec<u32>,
    pub pending_cast_object_cost: Option<FlatPendingCastObjectCostV3>,
    pub decision_local_library: Option<FlatDecisionLocalLibraryV3>,
    pub historical_public_sources: Vec<FlatHistoricalPublicSourceV3>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatDecisionV3 {
    pub binding: FlatActionDecisionBindingV3,
    pub globals: FlatGlobalsV2,
    pub extensions: FlatScoringExtensionsV3,
}

/// The common value types are storage layouts, not a V2 identity assertion.
/// Callers cannot construct or extract this view outside the crate.
#[derive(Clone, Copy)]
pub struct FlatScoringDecisionViewV3<'a> {
    common: FlatScoringDecisionViewV2<'a>,
    extensions: &'a FlatScoringExtensionsV3,
}

impl<'a> FlatScoringDecisionViewV3<'a> {
    pub(crate) fn new(
        common: FlatScoringDecisionViewV2<'a>,
        extensions: &'a FlatScoringExtensionsV3,
    ) -> Self {
        Self { common, extensions }
    }

    pub(crate) fn common(self) -> FlatScoringDecisionViewV2<'a> {
        self.common
    }

    pub(crate) fn extensions(self) -> &'a FlatScoringExtensionsV3 {
        self.extensions
    }
}

#[derive(Default)]
pub struct FlatDecisionEncoderV3 {
    common: FlatDecisionEncoderV2,
}

impl FastActorSessionV1 {
    pub(crate) fn encode_current_flat_scoring_decision_owned_v3(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut FlatDecisionEncoderV3,
        buffers: &mut FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<FlatDecisionV3, FlatDecisionErrorV2> {
        encoder
            .common
            .build_scoring_owned_v3(self, expected, buffers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::flat_policy_v2::*;
    use crate::native_flat_tensorizer_v3::{NativeFlatDecisionTensorV3, NativeFlatTensorizerV3};
    use crate::policy_observation_v6::tests::{
        escape_prefix_state, forest_search_state, forest_search_state_with_hidden_renumbering,
        map_choice_state,
    };
    use crate::rl::make_legal_action_v5;
    use crate::rl_session::FastActorResponseV1;
    use crate::state::GameState;

    #[derive(Default)]
    struct OwnedScoringV3 {
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
    }

    impl OwnedScoringV3 {
        fn encode(&mut self, session: &FastActorSessionV1) -> FlatDecisionV3 {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!(
                    "fixture is not a live decision: {:?}",
                    session.current_response()
                );
            };
            session
                .encode_current_flat_scoring_decision_owned_v3(
                    expected,
                    &mut FlatDecisionEncoderV3::default(),
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
                .expect("V3 fixture production encoding")
        }

        fn view<'a>(&'a self, decision: &'a FlatDecisionV3) -> FlatScoringDecisionViewV3<'a> {
            FlatScoringDecisionViewV3::new(
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

    fn escape_prefix_states() -> Vec<(String, GameState)> {
        let (mut state, _, picks) = escape_prefix_state();
        let mut states = Vec::new();
        for prefix in 0..=3 {
            states.push((format!("escape-prefix-{prefix}"), state.clone()));
            if prefix < 3 {
                engine::step(&mut state, Action::ChooseCostTarget(picks[prefix])).unwrap();
            }
        }
        states
    }

    fn tensors(session: &FastActorSessionV1) -> NativeFlatDecisionTensorV3 {
        let mut owned = OwnedScoringV3::default();
        let decision = owned.encode(session);
        let mut tensor = NativeFlatDecisionTensorV3::default();
        NativeFlatTensorizerV3::default()
            .fill(owned.view(&decision), &mut tensor)
            .expect("V3 fixture tensorization");
        tensor
    }

    #[test]
    fn flat_v3_real_search_tensor_is_invariant_to_hidden_order_and_unmatched_identity() {
        let baseline =
            FastActorSessionV1::from_v3_fixture_state(forest_search_state(false, "Lightning Bolt"));
        let permuted =
            FastActorSessionV1::from_v3_fixture_state(forest_search_state(true, "Lightning Bolt"));
        let hidden_changed =
            FastActorSessionV1::from_v3_fixture_state(forest_search_state(false, "Fireblast"));
        let renumbered = FastActorSessionV1::from_v3_fixture_state(
            forest_search_state_with_hidden_renumbering(),
        );
        assert_eq!(tensors(&baseline), tensors(&permuted));
        assert_eq!(tensors(&baseline), tensors(&hidden_changed));
        assert_eq!(tensors(&baseline), tensors(&renumbered));
    }

    #[test]
    fn flat_v3_real_map_and_each_escape_prefix_tensorize() {
        let map = FastActorSessionV1::from_v3_fixture_state(map_choice_state().0);
        assert!(
            matches!(map.current_response(), FastActorResponseV1::Decision(_)),
            "Map fixture response: {:?}",
            map.current_response()
        );
        assert_eq!(tensors(&map).common.action_features.len() / 195, 2);
        let mut prior = None;
        for (name, state) in escape_prefix_states().into_iter().take(3) {
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            assert!(
                matches!(session.current_response(), FastActorResponseV1::Decision(_)),
                "{name} fixture response: {:?}",
                session.current_response()
            );
            let tensor = tensors(&session);
            assert!(!tensor.common.action_features.is_empty());
            if let Some(previous) = prior {
                assert_ne!(previous, tensor.common.state);
            }
            prior = Some(tensor.common.state);
        }
    }

    #[test]
    fn flat_v3_completed_escape_prefix_is_rich_state_without_fabricated_action() {
        let (_, completed) = escape_prefix_states().pop().unwrap();
        let observation = crate::rl::observe_policy_v6(
            &completed,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            crate::ids::PlayerId::P0,
            0,
            0,
            0,
            1,
        )
        .unwrap();
        let (_, extensions) = encode_observation_owned_tables_for_fixture_v3(&observation).unwrap();
        let cost = extensions.pending_cast_object_cost.unwrap();
        assert_eq!(cost.selected_objects.len(), 3);
        assert_eq!(cost.remaining_count, 0);
    }

    fn emit_fixture(name: &str, session: &FastActorSessionV1) {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!(
                "emitter {name} is not a live decision: {:?}",
                session.current_response()
            );
        };
        let observation = session.flat_policy_observation_v3(expected).unwrap();
        let legal_actions = session
            .diagnostic_current_action_semantics()
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(index, semantic)| make_legal_action_v5(index as u32, semantic, None).unwrap())
            .collect::<Vec<_>>();
        let tensor = tensors(session).common;
        let bits = |values: &[f32]| {
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        };
        let fixture = serde_json::json!({
            "schema":"native-flat-v3-rust-fixture-v1", "name":name,
            "observation":observation, "legal_actions":legal_actions,
            "expected":{
                "state_bits":bits(&tensor.state),
                "object_features_bits":bits(&tensor.object_features),
                "object_card_ids":tensor.object_card_ids,
                "object_groups":tensor.object_groups,
                "object_node_ids":tensor.object_node_ids,
                "edge_features_bits":bits(&tensor.edge_features),
                "edge_source_indices":tensor.edge_source_indices,
                "edge_target_indices":tensor.edge_target_indices,
                "action_features_bits":bits(&tensor.action_features),
                "action_ref_features_bits":bits(&tensor.action_ref_features),
                "action_ref_card_ids":tensor.action_ref_card_ids,
                "action_ref_action_indices":tensor.action_ref_action_indices,
                "action_ref_node_indices":tensor.action_ref_node_indices,
            }
        });
        println!("NATIVE_FLAT_V3_FIXTURE={fixture}");
    }

    #[test]
    #[ignore = "explicit Rust/Python all-thirteen-tensor fixture emitter"]
    fn emit_native_flat_v3_fixtures() {
        let ordinary = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            63_002,
            0x63002,
            256,
            32768,
            ["Burn".into(), "Burn".into()],
        )
        .unwrap();
        emit_fixture("ordinary-opening", &ordinary);
        emit_fixture(
            "forestcycling-search",
            &FastActorSessionV1::from_v3_fixture_state(forest_search_state(
                false,
                "Lightning Bolt",
            )),
        );
        emit_fixture(
            "map-explore",
            &FastActorSessionV1::from_v3_fixture_state(map_choice_state().0),
        );
        emit_fixture(
            "forestcycling-search-renumbered",
            &FastActorSessionV1::from_v3_fixture_state(
                forest_search_state_with_hidden_renumbering(),
            ),
        );
        // Completing the third selection advances payment automatically, so
        // prefix 3 is tested above as rich state rather than a policy example.
        for (name, state) in escape_prefix_states().into_iter().take(3) {
            emit_fixture(&name, &FastActorSessionV1::from_v3_fixture_state(state));
        }
    }
}
