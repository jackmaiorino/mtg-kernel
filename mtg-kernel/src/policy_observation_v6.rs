//! Explicit successor observation for object costs, temporary library access,
//! and source incarnations established by public stack records.
//!
//! Stable references below are producer/transport authority. A scorer must
//! replace them with actor-visible row handles, never consume arena identities,
//! incarnation counters, or infer hidden library positions from them.

use crate::engine::CostKind;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, StackItemKindV2,
};
use crate::state::CastMethodV4;
use serde::{Deserialize, Serialize};

pub const OBSERVATION_SCHEMA_VERSION_V6: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationV6 {
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    pub extensions: PolicyObservationExtensionsV6,
    pub visible_projection_hash: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyObservationExtensionsV6 {
    pub pending_cast_object_cost: Option<PendingCastObjectCostV6>,
    pub decision_local_library: Option<DecisionLocalLibraryV6>,
    pub historical_public_sources: Vec<HistoricalPublicSourceV6>,
}

/// A declared cost and its complete current selection prefix. Selected cards
/// are still in their original zone until payment commits atomically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingCastObjectCostV6 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub cast_method: CastMethodV4,
    pub cost_kind: CostKind,
    pub required_count: u32,
    pub selected: Vec<CardStableRefV1>,
    pub remaining_count: u32,
}

/// The active chooser's matching cards, including selected cards, during this
/// prompt only. This is a multiset of physical choices with no library position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionLocalLibraryV6 {
    pub chooser: PlayerSeatV1,
    pub library_owner: PlayerSeatV1,
    pub cards: Vec<CardPrivateV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoricalSourceContextV6 {
    Stack { stack_index: u32 },
    PendingEffect,
}

/// A source is public because this exact validated stack record announced it,
/// even when its captured origin was Hand (cycling), it changed zones later,
/// or its token has ceased. It is not a statement about the current incarnation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalPublicSourceV6 {
    pub context: HistoricalSourceContextV6,
    pub source: CardStableRefV1,
    pub stack_item_kind: StackItemKindV2,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::engine::{self, Action, Decision};
    use crate::ids::{ObjectId, PlayerId};
    use crate::mana::ManaColor;
    use crate::policy_surface_v5::PolicySurfaceV5;
    use crate::rl::{
        card_name, observe_policy_v5, observe_policy_v6, policy_observation_extensions_v6,
    };
    use crate::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};

    pub(crate) fn ready_state() -> GameState {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 60_001);
        state.step = Step::Main1;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state
    }

    pub(crate) fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("fixture card: {name}"));
        let id = state.objects.push(GameObject {
            card_def,
            name: name.into(),
            owner,
            controller: owner,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Counters::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Hand => state.players[owner.index()].hand.push(id),
            Zone::Library => state.players[owner.index()].library.push(id),
            Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
            Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
            _ => panic!("unsupported fixture zone"),
        }
        id
    }

    fn observe(state: &GameState, actor: PlayerId) -> ObservationV6 {
        observe_policy_v6(state, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).unwrap()
    }

    pub(crate) fn escape_prefix_state() -> (GameState, ObjectId, [ObjectId; 3]) {
        let mut state = ready_state();
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
        let target = put(
            &mut state,
            PlayerId::P1,
            "Cryptic Serpent",
            Zone::Battlefield,
        );
        let spell = put(
            &mut state,
            PlayerId::P0,
            "Sleep of the Dead",
            Zone::Graveyard,
        );
        let picks = ["Lightning Bolt", "Fireblast", "Counterspell"]
            .map(|name| put(&mut state, PlayerId::P0, name, Zone::Graveyard));
        // Keep every incomplete cost prefix a real choice, including the last
        // pick. A singleton remainder is correctly suppressed by the session.
        put(&mut state, PlayerId::P0, "Brainstorm", Zone::Graveyard);
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        (state, spell, picks)
    }

    #[test]
    fn v6_escape_exposes_every_cost_prefix_and_v5_stays_rejected() {
        let (mut state, spell, picks) = escape_prefix_state();
        let mut hashes = Vec::new();
        for selected_count in 0..=3 {
            let observation = observe(&state, PlayerId::P0);
            let cost = observation
                .extensions
                .pending_cast_object_cost
                .as_ref()
                .unwrap();
            assert_eq!(cost.cast_method, CastMethodV4::Escape);
            assert_eq!(cost.cost_kind, CostKind::ExileFromGraveyard);
            assert_eq!(cost.required_count, 3);
            assert_eq!(cost.selected.len(), selected_count);
            assert_eq!(cost.remaining_count, 3 - selected_count as u32);
            assert_eq!(cost.source.arena_id, spell.0);
            assert!(cost
                .selected
                .iter()
                .all(|card| card.zone == Zone::Graveyard));
            assert!(observation
                .projection
                .surface
                .engine_context
                .pending_cast
                .as_ref()
                .unwrap()
                .sacrifice_chosen
                .is_empty());
            hashes.push(observation.visible_projection_hash);
            assert!(
                observe_policy_v5(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1)
                    .unwrap_err()
                    .0
                    .contains("staged Escape")
            );
            if selected_count < 3 {
                engine::step(&mut state, Action::ChooseCostTarget(picks[selected_count])).unwrap();
            }
        }
        assert!(hashes.windows(2).all(|pair| pair[0] != pair[1]));
        let mut duplicate = state.clone();
        duplicate
            .engine
            .pending_cast
            .as_mut()
            .unwrap()
            .sacrifice_chosen[1] = picks[0];
        assert!(policy_observation_extensions_v6(&duplicate, PlayerId::P0).is_err());
    }

    pub(crate) fn forest_search_state(reverse_library: bool, unmatched: &str) -> GameState {
        forest_search_state_inner(reverse_library, unmatched, false)
    }

    pub(crate) fn forest_search_state_with_hidden_renumbering() -> GameState {
        forest_search_state_inner(false, "Lightning Bolt", true)
    }

    fn forest_search_state_inner(
        reverse_library: bool,
        unmatched: &str,
        renumber: bool,
    ) -> GameState {
        let mut state = ready_state();
        let source = put(&mut state, PlayerId::P0, "Generous Ent", Zone::Hand);
        let names = if renumber {
            ["Snow-Covered Forest", "Forest", "Forest", unmatched]
        } else {
            ["Forest", unmatched, "Snow-Covered Forest", "Forest"]
        };
        for name in names {
            let id = put(&mut state, PlayerId::P0, name, Zone::Library);
            if renumber {
                state.objects.get_mut(id).zone_change_count = 31 + id.0;
            }
        }
        if reverse_library {
            state.players[0].library.reverse();
        }
        state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        for _ in 0..16 {
            match engine::advance_until_decision(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::ChooseEffectTargets { .. } => return state,
                other => panic!("expected forest search, got {other:?}"),
            }
        }
        panic!("forest search did not suspend");
    }

    #[test]
    fn v6_search_is_chooser_only_without_persistent_library_order() {
        let state = forest_search_state(false, "Lightning Bolt");
        let permuted = forest_search_state(true, "Lightning Bolt");
        let hidden_changed = forest_search_state(false, "Fireblast");
        let chooser = observe(&state, PlayerId::P0);
        let search = chooser.extensions.decision_local_library.as_ref().unwrap();
        assert_eq!(search.cards.len(), 3);
        assert_eq!(search.chooser, PlayerSeatV1::P0);
        assert_eq!(search.library_owner, PlayerSeatV1::P0);
        let crate::rl::PendingEffectChoiceSemanticV4::Targets { legal_targets, .. } = chooser
            .projection
            .surface
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .as_ref()
            .unwrap()
        else {
            panic!("search requires targets");
        };
        let canonical_targets = search
            .cards
            .iter()
            .map(|card| crate::rl::TargetRefV1::Object {
                object: card.stable.clone(),
            })
            .collect::<Vec<_>>();
        assert_eq!(legal_targets, &canonical_targets);
        assert!(chooser.known_library_cards.iter().all(Vec::is_empty));
        assert_eq!(
            search,
            observe(&permuted, PlayerId::P0)
                .extensions
                .decision_local_library
                .as_ref()
                .unwrap()
        );
        assert_eq!(
            search,
            observe(&hidden_changed, PlayerId::P0)
                .extensions
                .decision_local_library
                .as_ref()
                .unwrap()
        );
        let opponent = observe(&state, PlayerId::P1);
        assert!(opponent.extensions.decision_local_library.is_none());
        assert_eq!(opponent, observe(&hidden_changed, PlayerId::P1));
        let encoded = serde_json::to_value(search).unwrap();
        assert!(encoded["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|card| card.get("position").is_none()));
        let source = &chooser
            .extensions
            .historical_public_sources
            .iter()
            .find(|row| row.context == HistoricalSourceContextV6::PendingEffect)
            .unwrap()
            .source;
        let frozen = state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .v4
            .ability_source_contract
            .unwrap();
        assert_eq!(source.zone, frozen.zone);
        assert_eq!(source.zone_change_count, frozen.zone_change_count);
        let mut forged = state.clone();
        if let Some(crate::effect::PendingEffectChoice::SelectTargets { legal, .. }) =
            &mut forged.engine.pending_effect.as_mut().unwrap().choice
        {
            legal[0]
                .expected_object
                .as_mut()
                .unwrap()
                .expected_zone_change_count += 1;
        }
        assert!(policy_observation_extensions_v6(&forged, PlayerId::P0).is_err());
    }

    pub(crate) fn map_choice_state() -> (GameState, ObjectId) {
        let mut state = ready_state();
        put(
            &mut state,
            PlayerId::P0,
            "Fanatical Offering",
            Zone::Library,
        );
        let map = put(&mut state, PlayerId::P0, "Map Token", Zone::Battlefield);
        let creature = put(
            &mut state,
            PlayerId::P0,
            "Voldaren Epicure",
            Zone::Battlefield,
        );
        state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
        engine::step(&mut state, Action::ActivateAbility(map, 0)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
        let mut reached = false;
        for _ in 0..16 {
            match engine::advance_until_decision(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::ChooseEffectOption {
                    option_count: 2, ..
                } => {
                    reached = true;
                    break;
                }
                other => panic!("expected Map Explore, got {other:?}"),
            }
        }
        assert!(reached);
        (state, map)
    }

    /// A real combat-transfer trigger keeps the designation's Hunter source,
    /// while its controller is the player taking the Initiative. The Hunter
    /// itself stays on the other player's battlefield in the same incarnation.
    pub(crate) fn initiative_transfer_state() -> GameState {
        use crate::effect::EffectOp;
        use crate::state::{AbilitySourceContractV4, InitiativeTriggerKindV1};

        let mut state = ready_state();
        let hunter = put(
            &mut state,
            PlayerId::P1,
            "Avenging Hunter",
            Zone::Battlefield,
        );
        state.initiative = Some(PlayerId::P1);
        state.engine.initiative_source = Some(AbilitySourceContractV4::capture(&state, hunter));
        let attacker = put(
            &mut state,
            PlayerId::P0,
            "Voldaren Epicure",
            Zone::Battlefield,
        );
        // Retain an actual priority choice when the combat-transfer trigger
        // reaches the stack, rather than relying on suppressed passes.
        put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::DeclareAttackers {
                player: PlayerId::P0,
                ..
            }
        ));
        engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        for _ in 0..64 {
            let decision = engine::advance_until_decision(&mut state);
            if state.stack.iter().any(|item| {
                matches!(&item.inline_effect,
                Some(EffectOp::ResolveInitiativeTrigger { binding })
                    if binding.kind == InitiativeTriggerKindV1::CombatTransfer)
            }) {
                assert_eq!(state.objects.get(hunter).controller, PlayerId::P1);
                return state;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareBlockers {
                    player: PlayerId::P1,
                    ..
                } => engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap(),
                Decision::OrderTriggers { ref pending, .. } if pending.len() == 1 => {
                    engine::step(&mut state, Action::OrderTriggers(vec![0])).unwrap()
                }
                other => panic!("unexpected path to Initiative transfer: {other:?}"),
            }
        }
        panic!("combat did not create an Initiative transfer trigger");
    }

    #[test]
    fn v6_initiative_transfer_preserves_distinct_live_and_frozen_controllers() {
        use crate::effect::EffectOp;
        use crate::event::CommittedEvent;
        use crate::state::InitiativeTriggerKindV1;

        let state = initiative_transfer_state();
        let (stack_index, item, binding) = state
            .stack
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                let Some(EffectOp::ResolveInitiativeTrigger { binding }) = &item.inline_effect
                else {
                    return None;
                };
                (binding.kind == InitiativeTriggerKindV1::CombatTransfer)
                    .then_some((index, item, *binding))
            })
            .unwrap();
        assert_eq!(
            state
                .engine
                .event_history
                .get(binding.history_index as usize),
            Some(&CommittedEvent::InitiativeTrigger { binding })
        );
        assert!(state.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::CombatDamageToPlayer {
                player: PlayerId::P1,
                amount: 1,
                ..
            }
        )));
        let observation = observe(&state, PlayerId::P0);
        let historical = observation
            .extensions
            .historical_public_sources
            .iter()
            .find(|row| {
                row.context
                    == HistoricalSourceContextV6::Stack {
                        stack_index: stack_index as u32,
                    }
            })
            .unwrap();
        let live = observation.projection.surface.battlefield[1]
            .iter()
            .find(|card| card.stable.arena_id == item.source.0)
            .unwrap();
        assert_eq!(historical.source.arena_id, live.stable.arena_id);
        assert_eq!(
            historical.source.zone_change_count,
            live.stable.zone_change_count
        );
        assert_eq!(historical.source.card_db_id, live.stable.card_db_id);
        assert_eq!(historical.source.owner, live.stable.owner);
        assert_eq!(historical.source.zone, live.stable.zone);
        assert_eq!(historical.source.controller, PlayerSeatV1::P0);
        assert_eq!(live.stable.controller, PlayerSeatV1::P1);
        assert_eq!(
            historical.source,
            observation.projection.surface.stack[stack_index].source
        );
        let mut forged = state.clone();
        forged.stack[stack_index]
            .v4
            .ability_source_contract
            .as_mut()
            .unwrap()
            .controller = PlayerId::P1;
        assert!(policy_observation_extensions_v6(&forged, PlayerId::P0).is_err());
    }

    #[test]
    fn v6_historical_map_source_survives_token_cessation_and_rejects_forgery() {
        let (state, map) = map_choice_state();
        assert!(!state.players[0].graveyard.contains(&map));
        let observation = observe(&state, PlayerId::P0);
        let source = observation
            .extensions
            .historical_public_sources
            .iter()
            .find(|row| row.context == HistoricalSourceContextV6::PendingEffect)
            .unwrap();
        assert_eq!(source.source.arena_id, map.0);
        assert_eq!(
            source.source.card_db_id,
            card_id_by_name("Map Token").unwrap()
        );
        assert_eq!(source.stack_item_kind, StackItemKindV2::ActivatedAbility);
        let mut forged = state.clone();
        forged
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .resolving_item
            .v4
            .ability_source_contract
            .as_mut()
            .unwrap()
            .card_def = card_id_by_name("Forest").unwrap();
        assert!(policy_observation_extensions_v6(&forged, PlayerId::P0).is_err());
    }

    #[test]
    fn v6_explicit_empty_extensions_round_trip_and_commit_to_hash() {
        let state = ready_state();
        let full = observe(&state, PlayerId::P0);
        let value = serde_json::to_value(&full).unwrap();
        assert_eq!(value["schema_version"], 6);
        assert_eq!(
            value["extensions"]["pending_cast_object_cost"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["extensions"]["decision_local_library"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["extensions"]["historical_public_sources"],
            serde_json::json!([])
        );
        assert_eq!(
            serde_json::from_value::<ObservationV6>(value).unwrap(),
            full
        );
        let legacy =
            observe_policy_v5(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1).unwrap();
        assert_eq!(legacy.schema_version, 5);
        assert_eq!(legacy.projection, full.projection);
        assert_ne!(legacy.visible_projection_hash, full.visible_projection_hash);
    }
}
