//! Explicit V3 scorer producer for rich V6 observations.
//!
//! Shared row layouts retain Net8 dimensions, while this entry point and its
//! action binding own a distinct identity. Private source/choice authority is
//! resolved to model row indices before any scorer receives these extensions.

use crate::engine::{ChosenCreatureCostZoneV1, CostKind};
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
    pub pending_chosen_creature_cost: Option<FlatPendingChosenCreatureCostV3>,
    pub finalized_chosen_creature_costs: Vec<FlatFinalizedChosenCreatureCostV3>,
    pub pending_ward_payment: Option<FlatWardPaymentV3>,
    pub queued_ward_payments: Vec<FlatQueuedWardPaymentV3>,
}

/// Public stack positions retain the instance relation even when two
/// activated abilities share the same source object row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatWardPaymentV3 {
    pub targeting_stack_index: u32,
    pub targeting_source_object: u32,
    pub ward_source_object: u32,
    pub payer: FlatRelativePlayerV2,
    pub generic: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatQueuedWardPaymentV3 {
    pub stack_index: u32,
    pub payment: FlatWardPaymentV3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatPendingChosenCreatureCostV3 {
    pub source_object: u32,
    pub controller: FlatRelativePlayerV2,
    pub selected_zone: ChosenCreatureCostZoneV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatFinalizedChosenCreatureCostV3 {
    pub stack_index: u32,
    pub source_object: u32,
    pub chosen_object: u32,
    pub power_lki: i32,
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
            .map_err(|error| {
                // Opt-in backend evidence only. Preserve the first failure and
                // capture only the projection belonging to the acting player.
                if let Some(path) = std::env::var_os("MTG_KERNEL_V3_SCORING_ERROR_CAPTURE") {
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                    {
                        let observation = self.flat_policy_observation_v3(expected);
                        let value = serde_json::json!({"schema": "v3-scoring-error/v1",
                            "error": format!("{error:?}"), "decision": format!("{expected:?}"),
                            "observation": observation.as_ref().ok(),
                            "observation_error": observation.as_ref().err().map(|e| format!("{e:?}"))});
                        let _ = serde_json::to_writer(&mut file, &value);
                    }
                }
                error
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::flat_policy_v2::*;
    use crate::native_flat_tensorizer_v3::{NativeFlatDecisionTensorV3, NativeFlatTensorizerV3};
    use crate::policy_observation_v6::tests::{
        escape_prefix_state, forest_search_state, forest_search_state_with_hidden_renumbering,
        initiative_transfer_state, map_choice_state,
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

    #[test]
    fn reexiled_card_keeps_only_current_generation_permissions_in_v3_scoring() {
        use crate::engine::{PlayOrCast, PlayPermission, PlayPermissionExpiry};
        use crate::event::{self, ProposedEvent};
        use crate::ids::PlayerId;
        use crate::mana::ManaColor;
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::Zone;

        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            state.starting_player = actor;
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 4;
            let card = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Exile));
            let old = PlayPermission {
                object: card,
                holder: actor,
                zone_change_generation: state.objects.get(card).zone_change_count,
                play_or_cast: PlayOrCast::Cast,
                expiry: PlayPermissionExpiry::UntilHoldersNextTurn {
                    holder_turn_started: true,
                },
            };
            state.engine.exile_play_permissions.push(old.clone());
            for zone in [Zone::Hand, Zone::Graveyard, Zone::Library, Zone::Exile] {
                event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, zone));
            }
            // A later exile alone must not resurrect the old permission.
            assert!(engine::active_permission_for(actor, card, &state).is_none());
            let observe = |state: &GameState, seat| {
                crate::rl::observe_policy_v6(
                    state,
                    &crate::policy_surface_v5::PolicySurfaceV5::new(),
                    seat,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap()
            };
            for seat in [PlayerId::P0, PlayerId::P1] {
                assert!(observe(&state, seat)
                    .projection
                    .surface
                    .exile_play_permissions
                    .is_empty());
            }
            let current_generation = state.objects.get(card).zone_change_count;
            assert_eq!(old.zone_change_generation, 1);
            assert_eq!(current_generation, 5);
            state.engine.exile_play_permissions.push(PlayPermission {
                zone_change_generation: current_generation,
                expiry: PlayPermissionExpiry::EndOfTurn,
                ..old.clone()
            });
            // Two grants for the same current incarnation may coexist. Keep
            // both; removing stale grants is not deduplication or action pruning.
            state.engine.exile_play_permissions.push(PlayPermission {
                zone_change_generation: current_generation,
                ..old
            });
            for seat in [PlayerId::P0, PlayerId::P1] {
                let observation = observe(&state, seat);
                let permissions = &observation.projection.surface.exile_play_permissions;
                assert_eq!(permissions.len(), 2);
                assert!(permissions
                    .iter()
                    .all(|permission| permission.zone_change_generation == 5
                        && permission.object.zone_change_count == 5));
            }
            assert_eq!(state.engine.exile_play_permissions.len(), 3);
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("current permission must permit a real decision");
            };
            assert_eq!(decision.acting_player, actor.into());
            assert!(decision.legal_action_count > 1);
            let mut owned = OwnedScoringV3::default();
            let encoded = owned.encode(&session);
            let mut tensor = NativeFlatDecisionTensorV3::default();
            NativeFlatTensorizerV3::default()
                .fill(owned.view(&encoded), &mut tensor)
                .unwrap();
        }
    }

    /// Regression test for the panel-stopping crash: a formal five-deck BO3
    /// evaluator run hit `StaleEnvironmentBinding: frozen sideboard play
    /// policy: V3 actor-visible encoding: InvalidReference` on a decision
    /// with no pending cast, discard, or stack item at all -- the reference
    /// that failed to resolve came from `combat` residue instead.
    ///
    /// `state.engine.combat` is only reset at the next `Step::BeginCombat`
    /// (see the `CombatState` doc comment in `engine.rs`: "This turn's
    /// combat. Reset at every `Step::BeginCombat`."), so it keeps reporting
    /// the last combat's attackers/blockers all the way through the rest of
    /// that combat phase, both main phases, both end steps, and the whole of
    /// the next turn up to its own `BeginCombat`. `combat_public_v2`
    /// (`rl.rs`) used to re-derive each participant's *current* zone with
    /// `object_is_live_in_zone_index`, which only asks whether the id still
    /// resolves to some zone slot -- not whether that zone is one the V3
    /// object registry (`register_objects`/`resolve_live` in
    /// `flat_policy_v2.rs`) can ever contain. A blocker that had since been
    /// shuffled into its owner's library (a hidden zone, never registered)
    /// therefore reached the encoder as an unresolvable stable reference.
    ///
    /// Also covers the fix's own equivalence requirement: the registry
    /// additionally admits an opponent's hand card once it has been
    /// revealed to the acting player (`known_hand_cards_v4`), so the
    /// combat-participant gate must keep that case resolvable (own hand,
    /// and a revealed opponent-hand incarnation) while only dropping the
    /// truly unresolvable ones (library, unrevealed opponent hand).
    #[test]
    fn stale_combat_residue_pointing_at_a_hidden_zone_does_not_break_v3_scoring() {
        use crate::event::{self, ProposedEvent};
        use crate::ids::PlayerId;
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::Zone;

        let mut state = ready_state();
        // Build the stale residue directly rather than playing out a whole
        // combat: the code under test only looks at `state.engine.combat`
        // and each named object's current zone, so this reproduces exactly
        // what a real post-combat, hidden-zone-bound decision presents.
        let attacker = put(&mut state, PlayerId::P1, "Tolarian Terror", Zone::Battlefield);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(attacker, Zone::Graveyard));
        let blocker_visible = put(&mut state, PlayerId::P0, "Tolarian Terror", Zone::Battlefield);
        let blocker_hidden = put(&mut state, PlayerId::P0, "Tolarian Terror", Zone::Battlefield);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(blocker_hidden, Zone::Library),
        );
        // The registry (`register_objects`, `flat_policy_v2.rs`) also admits
        // an opponent's hand card through `known_hand_cards_v4` when this
        // exact incarnation was previously revealed to the acting player
        // (`state.reveal_hand_card`) and is still sitting in that hand. The
        // combat-participant gate must match that exactly: the acting
        // player's own hand is always visible, the opponent's hand only
        // when revealed, never otherwise.
        let blocker_own_hand = put(&mut state, PlayerId::P0, "Tolarian Terror", Zone::Hand);
        let blocker_opp_hand_unrevealed =
            put(&mut state, PlayerId::P1, "Tolarian Terror", Zone::Hand);
        let blocker_opp_hand_revealed = put(&mut state, PlayerId::P1, "Tolarian Terror", Zone::Hand);
        state
            .reveal_hand_card(PlayerId::P0, PlayerId::P1, blocker_opp_hand_revealed)
            .unwrap();
        state.engine.combat.attackers_declared = true;
        state.engine.combat.blockers_declared = true;
        state.engine.combat.attackers = vec![attacker];
        state.engine.combat.blocked_by = vec![(
            attacker,
            vec![
                blocker_visible,
                blocker_hidden,
                blocker_own_hand,
                blocker_opp_hand_unrevealed,
                blocker_opp_hand_revealed,
            ],
        )];

        // The projection itself must silently drop the now-hidden blockers
        // (the same treatment a truly-gone id already got) rather than hand
        // the encoder a reference it can never resolve, while keeping every
        // blocker the registry can actually resolve -- including the
        // revealed opponent-hand incarnation, which must not regress just
        // because the library-bound case now gets dropped.
        let observation = crate::rl::observe_policy_v6(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            PlayerId::P0,
            0,
            0,
            0,
            1,
        )
        .unwrap();
        let combat = &observation.projection.surface.combat;
        assert_eq!(combat.ordered_attackers.len(), 1, "dead attacker stays visible in its graveyard");
        assert_eq!(combat.attacker_to_ordered_blockers.len(), 1);
        let (_, blockers) = &combat.attacker_to_ordered_blockers[0];
        assert_eq!(
            blockers.iter().map(|b| b.arena_id).collect::<Vec<_>>(),
            vec![
                blocker_visible.0,
                blocker_own_hand.0,
                blocker_opp_hand_revealed.0,
            ],
            "library-bound and unrevealed-opponent-hand blockers must be dropped; \
             battlefield, own-hand, and revealed-opponent-hand blockers must survive"
        );

        // End-to-end: this exact fixture used to panic V3 scoring encode
        // with `InvalidReference` once the dropped-here reference reached
        // `resolve_live`. `OwnedScoringV3::encode` unwraps internally, so a
        // regression here fails this test with that same error.
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        assert!(matches!(
            session.current_response(),
            FastActorResponseV1::Decision(_)
        ));
        let mut owned = OwnedScoringV3::default();
        let _ = owned.encode(&session);
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

    fn initiative_transfer_pending_state() -> GameState {
        let mut state = initiative_transfer_state();
        crate::policy_observation_v6::tests::put(
            &mut state,
            crate::ids::PlayerId::P0,
            "Forest",
            crate::state::Zone::Library,
        );
        for _ in 0..64 {
            match engine::advance_until_decision(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::OrderTriggers { pending, .. } if pending.len() == 1 => {
                    engine::step(&mut state, Action::OrderTriggers(vec![0])).unwrap();
                }
                Decision::ChooseEffectTargets { .. } | Decision::ChooseEffectOption { .. } => {
                    return state
                }
                other => panic!("initiative pending fixture: {other:?}"),
            }
        }
        panic!("initiative transfer did not reach its pending room effect");
    }

    fn blood_fountain_graveyard_target_state(detached: bool) -> GameState {
        use crate::ids::PlayerId;
        use crate::mana::ManaColor;
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::{Target, Zone};

        let mut state = ready_state();
        state.active_player = PlayerId::P1;
        state.priority_player = PlayerId::P1;
        let fountain = put(
            &mut state,
            PlayerId::P1,
            "Blood Fountain",
            Zone::Battlefield,
        );
        let creature = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Graveyard);
        put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
        state.players[1].mana_pool[ManaColor::B.pool_index()] = 4;
        let macabre = detached.then(|| put(&mut state, PlayerId::P0, "Faerie Macabre", Zone::Hand));
        engine::step(&mut state, Action::ActivateAbility(fountain, 0)).unwrap();
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(creature)),
        )
        .unwrap();
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        assert!(state.stack.iter().any(|item| item.source == fountain));
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass {
                player: PlayerId::P1,
                ..
            }
        ));
        engine::step(&mut state, Action::Pass).unwrap();
        if let Some(macabre) = macabre {
            // A real response exiles the chosen card while Fountain's captured
            // Graveyard target stays on the stack in its original incarnation.
            engine::step(&mut state, Action::ActivateAbility(macabre, 0)).unwrap();
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(creature)),
            )
            .unwrap();
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            for _ in 0..32 {
                let decision = engine::advance_until_decision(&mut state);
                if state.objects.get(creature).zone == Zone::Exile
                    && state.stack.iter().any(|item| item.source == fountain)
                {
                    return state;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("Blood Fountain response fixture: {other:?}"),
                }
            }
            panic!("Faerie Macabre response did not leave Fountain's captured target");
        }
        state
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

    #[test]
    fn flat_v3_initiative_transfer_keeps_live_and_captured_controllers_distinct() {
        for state in [
            initiative_transfer_state(),
            initiative_transfer_pending_state(),
        ] {
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("initiative fixture must retain an actual policy choice");
            };
            let observation = session.flat_policy_observation_v3(expected).unwrap();
            let mut owned = OwnedScoringV3::default();
            let decision = owned.encode(&session);
            let captured = decision
                .extensions
                .historical_public_sources
                .iter()
                .find(|row| owned.objects[row.model_object_index as usize].card_token == 2)
                .expect("captured Avenging Hunter initiative source");
            let historical = &owned.objects[captured.model_object_index as usize];
            assert_eq!(historical.controller, FlatRelativePlayerV2::SelfPlayer);
            assert_eq!(historical.group, FlatObjectGroupV2::PendingContext);
            let live_index = owned
                .objects
                .iter()
                .position(|object| {
                    object.card_token == 2 && object.group == FlatObjectGroupV2::OpponentBattlefield
                })
                .unwrap();
            assert_ne!(live_index as u32, captured.model_object_index);
            assert_eq!(
                owned.objects[live_index].controller,
                FlatRelativePlayerV2::Opponent
            );
            let mut output = NativeFlatDecisionTensorV3::default();
            NativeFlatTensorizerV3::default()
                .fill(owned.view(&decision), &mut output)
                .unwrap();

            // A controller distinction is authorized by the declared public
            // source context. A changed extension alone cannot create that view.
            let mut forged = observation;
            let record = forged
                .extensions
                .historical_public_sources
                .iter_mut()
                .find(|row| row.source.card_db_id == 1)
                .unwrap();
            record.source.controller = crate::rl::PlayerSeatV1::P1;
            assert!(matches!(
                encode_observation_owned_tables_for_fixture_v3(&forged),
                Err(FlatDecisionErrorV2::InconsistentReference)
            ));
        }
    }

    #[test]
    fn flat_v3_blood_fountain_live_and_detached_graveyard_targets_preserve_v2_rejection() {
        use crate::ids::PlayerId;
        use crate::policy_surface_v5::PolicySurfaceV5;
        use crate::rl::{observe_policy_v5, TargetRefV1};
        use crate::state::Zone;
        for detached in [false, true] {
            let state = blood_fountain_graveyard_target_state(detached);
            let old_observation =
                observe_policy_v5(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1)
                    .unwrap();
            assert!(matches!(
                encode_observation_owned_tables_for_fixture_v2(&old_observation),
                Err(FlatDecisionErrorV2::InvalidReference)
            ));
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("Blood Fountain detached={detached} needs an actual response choice");
            };
            let observation = session.flat_policy_observation_v3(expected).unwrap();
            let mut owned = OwnedScoringV3::default();
            let decision = owned.encode(&session);
            let target = owned
                .objects
                .iter()
                .find(|object| {
                    object.card_token == 79 && object.zone == Some(FlatZoneV2::Graveyard)
                })
                .unwrap();
            assert_eq!(
                target.group,
                if detached {
                    FlatObjectGroupV2::HistoricalStackTarget
                } else {
                    FlatObjectGroupV2::OpponentGraveyard
                }
            );
            let mut output = NativeFlatDecisionTensorV3::default();
            NativeFlatTensorizerV3::default()
                .fill(owned.view(&decision), &mut output)
                .unwrap();
            for hidden_zone in [Zone::Hand, Zone::Library] {
                let mut forged = observation.clone();
                let reference = forged
                    .projection
                    .surface
                    .stack
                    .iter_mut()
                    .flat_map(|item| item.targets.iter_mut())
                    .find_map(|target| match target {
                        TargetRefV1::Object { object } if object.card_db_id == 78 => Some(object),
                        _ => None,
                    })
                    .unwrap();
                reference.zone = hidden_zone;
                assert!(encode_observation_owned_tables_for_fixture_v3(&forged).is_err());
            }
        }
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

    fn ward_priority_fixture_state(abilities: bool) -> GameState {
        use crate::ids::PlayerId;
        use crate::mana::ManaColor;
        use crate::policy_observation_v6::tests::{
            put, ward_multi_targeter_state, ward_same_source_abilities_state,
        };
        use crate::state::Zone;
        let mut state = if abilities {
            ward_same_source_abilities_state().0
        } else {
            ward_multi_targeter_state().0
        };
        // Fast sessions consume forced Pass actions. Keep a real response
        // choice available so the queued fixture still contains both triggers
        // before the engine begins resolving either Ward payment.
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            state.players[player.index()].mana_pool[ManaColor::R.pool_index()] += 1;
        }
        state
    }

    #[test]
    fn ward_bound_stack_instances_reach_flat_and_numeric_inputs() {
        use crate::policy_observation_v6::tests::reach_ward_payment;
        for abilities in [false, true] {
            let mut state = ward_priority_fixture_state(abilities);
            let queued_session = FastActorSessionV1::from_v3_fixture_state(state.clone());
            let mut queued_owned = OwnedScoringV3::default();
            let queued = queued_owned.encode(&queued_session);
            assert_eq!(queued.extensions.queued_ward_payments.len(), 2);
            assert!(queued.extensions.pending_ward_payment.is_none());
            let candidates = queued
                .extensions
                .queued_ward_payments
                .iter()
                .map(|item| item.payment.targeting_stack_index)
                .collect::<Vec<_>>();
            assert_ne!(candidates[0], candidates[1]);
            if abilities {
                assert_eq!(
                    queued.extensions.queued_ward_payments[0]
                        .payment
                        .targeting_source_object,
                    queued.extensions.queued_ward_payments[1]
                        .payment
                        .targeting_source_object
                );
            }
            reach_ward_payment(&mut state);
            let session = FastActorSessionV1::from_v3_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("Ward fixture needs an actual payment choice");
            };
            let observation = session.flat_policy_observation_v3(expected).unwrap();
            let mut owned = OwnedScoringV3::default();
            let decision = owned.encode(&session);
            let current = decision.extensions.pending_ward_payment.as_ref().unwrap();
            let different_index = *candidates
                .iter()
                .find(|&&i| i != current.targeting_stack_index)
                .unwrap();
            let mut alternative = observation.clone();
            alternative
                .extensions
                .pending_ward_payment
                .as_mut()
                .unwrap()
                .targeting_stack_index = different_index;
            let (_, extensions) =
                encode_observation_owned_tables_for_fixture_v3(&alternative).unwrap();
            let mut different = decision.clone();
            different.extensions = extensions;
            let mut first = NativeFlatDecisionTensorV3::default();
            let mut second = NativeFlatDecisionTensorV3::default();
            NativeFlatTensorizerV3::default()
                .fill(owned.view(&decision), &mut first)
                .unwrap();
            NativeFlatTensorizerV3::default()
                .fill(owned.view(&different), &mut second)
                .unwrap();
            assert_ne!(first.common.state, second.common.state);
            assert_ne!(first.common.edge_features, second.common.edge_features);
            if abilities {
                assert_eq!(
                    first.common.edge_target_indices, second.common.edge_target_indices,
                    "shared source rows must still retain distinct stack-instance features"
                );
            }
            different
                .extensions
                .pending_ward_payment
                .as_mut()
                .unwrap()
                .generic += 1;
            let mut other_cost = NativeFlatDecisionTensorV3::default();
            NativeFlatTensorizerV3::default()
                .fill(owned.view(&different), &mut other_cost)
                .unwrap();
            assert_ne!(second.common.state, other_cost.common.state);
            assert_ne!(second.common.edge_features, other_cost.common.edge_features);
        }
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
        for abilities in [false, true] {
            use crate::policy_observation_v6::tests::reach_ward_payment;
            let mut state = ward_priority_fixture_state(abilities);
            emit_fixture(
                if abilities {
                    "ward-queued-shared-ability-source"
                } else {
                    "ward-queued-two-spells"
                },
                &FastActorSessionV1::from_v3_fixture_state(state.clone()),
            );
            reach_ward_payment(&mut state);
            emit_fixture(
                if abilities {
                    "ward-pending-shared-ability-source"
                } else {
                    "ward-pending-two-spells"
                },
                &FastActorSessionV1::from_v3_fixture_state(state),
            );
        }
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
        emit_fixture(
            "initiative-transfer",
            &FastActorSessionV1::from_v3_fixture_state(initiative_transfer_state()),
        );
        emit_fixture(
            "initiative-transfer-pending",
            &FastActorSessionV1::from_v3_fixture_state(initiative_transfer_pending_state()),
        );
        emit_fixture(
            "blood-fountain-graveyard-target-live",
            &FastActorSessionV1::from_v3_fixture_state(blood_fountain_graveyard_target_state(
                false,
            )),
        );
        emit_fixture(
            "blood-fountain-graveyard-target-detached",
            &FastActorSessionV1::from_v3_fixture_state(blood_fountain_graveyard_target_state(true)),
        );
        emit_fixture(
            "monstrous-emergence-zone-choice",
            &FastActorSessionV1::from_v3_fixture_state(
                crate::native_flat_tensorizer_v3::monstrous_emergence_zone_fixture_v3(),
            ),
        );
        for own_hand in [false, true] {
            let (state, _) =
                crate::native_flat_tensorizer_v3::monstrous_emergence_cost_fixture_v3(own_hand);
            emit_fixture(
                if own_hand {
                    "monstrous-emergence-reveal-own-hand-cost"
                } else {
                    "monstrous-emergence-public-creature-cost"
                },
                &FastActorSessionV1::from_v3_fixture_state(state),
            );
        }
        for goad_first in [true, false] {
            let (state, _, _) = crate::rl_session::goaded_attacker_fixture_state_v3(goad_first);
            let mut session = FastActorSessionV1::from_v3_fixture_state(state);
            emit_fixture(
                if goad_first {
                    "goad-required-first"
                } else {
                    "ordinary-before-required"
                },
                &session,
            );
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("goad fixture must start with an attacker inclusion");
            };
            session.step(decision.episode_id, decision.step, 0).unwrap();
            emit_fixture(
                if goad_first {
                    "ordinary-after-required"
                } else {
                    "goad-required-last"
                },
                &session,
            );
        }
        // Completing the third selection advances payment automatically, so
        // prefix 3 is tested above as rich state rather than a policy example.
        for (name, state) in chosen_creature_value_states() {
            emit_fixture(&name, &FastActorSessionV1::from_v3_fixture_state(state));
        }
        for (name, state) in escape_prefix_states().into_iter().take(3) {
            emit_fixture(&name, &FastActorSessionV1::from_v3_fixture_state(state));
        }
    }

    fn chosen_creature_value_states() -> Vec<(String, GameState)> {
        use crate::native_flat_tensorizer_v3::{
            monstrous_emergence_paid_fixture_v3, monstrous_emergence_zone_fixture_v3,
        };
        let mut states = Vec::new();
        let base = monstrous_emergence_zone_fixture_v3();
        for option in [0, 1] {
            let mut state = base.clone();
            engine::step(&mut state, Action::ChooseEffectOption(option)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseCostTargets { .. }
            ));
            states.push((format!("monstrous-selected-zone-{option}"), state));
        }
        for (name, hand, bonus) in [
            ("monstrous-paid-live", false, None),
            ("monstrous-paid-departed-power4", false, Some(0)),
            ("monstrous-paid-departed-power7", false, Some(3)),
            ("monstrous-paid-revealed-hand", true, None),
        ] {
            states.push((
                name.into(),
                monstrous_emergence_paid_fixture_v3(hand, bonus).0,
            ));
        }
        states
    }

    #[test]
    fn v3_chosen_creature_branch_and_refreshed_lki_reach_value_state_without_width_change() {
        let values = chosen_creature_value_states()
            .into_iter()
            .map(|(_, state)| tensors(&FastActorSessionV1::from_v3_fixture_state(state)).common)
            .collect::<Vec<_>>();
        assert_ne!(
            values[0].state, values[1].state,
            "selected zone must reach the value input"
        );
        assert_ne!(
            values[3].state, values[4].state,
            "departed creature LKI must reach the value input"
        );
        for value in &values {
            assert_eq!(value.state.len(), 219);
            assert_eq!(value.object_features.len() % 98, 0);
            assert_eq!(value.action_features.len() % 195, 0);
        }
        let mut left = values[3].clone();
        left.state.clone_from(&values[4].state);
        assert_eq!(
            left, values[4],
            "LKI-only fixture difference must not rewrite live objects/actions"
        );
    }
}
