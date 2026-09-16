//! Explicit successor observation for object costs, temporary library access,
//! and source incarnations established by public stack records.
//!
//! Stable references below are producer/transport authority. A scorer must
//! replace them with actor-visible row handles, never consume arena identities,
//! incarnation counters, or infer hidden library positions from them.

use crate::engine::{ChosenCreatureCostZoneV1, CostKind};
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
    pub pending_chosen_creature_cost: Option<PendingChosenCreatureCostV6>,
    pub finalized_chosen_creature_costs: Vec<FinalizedChosenCreatureCostV6>,
    /// Optional successor fields are absent from the canonical representation
    /// outside Ward states, preserving the earlier V6 bytes and hash features.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_ward_payment: Option<WardPaymentV6>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_ward_payments: Vec<QueuedWardPaymentV6>,
}

/// Public relation to one exact, currently live targeting stack item. The
/// ordinal indexes the existing public stack vector, including nonspell items;
/// two abilities from one source therefore remain distinct. No internal stack
/// id or incarnation counter is part of this relation. For a pending payment,
/// the Ward source uses the existing pending-resolution context. The engine
/// currently retains that resolver at the live stack top; this record does not
/// duplicate its identity as a queued trigger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WardPaymentV6 {
    pub targeting_stack_index: u32,
    pub payer: PlayerSeatV1,
    pub generic: u8,
}

/// A live Ward trigger and its live bound targeter before trigger resolution.
/// A trigger whose targeter has already departed resolves as a no-op and has
/// no payment relation, while its ordinary public stack row remains present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedWardPaymentV6 {
    pub stack_index: u32,
    pub payment: WardPaymentV6,
}

/// The controller's already selected cost branch, before choosing/revealing a
/// card. This record grants no other player access to an unfinished hand cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingChosenCreatureCostV6 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub selected_zone: ChosenCreatureCostZoneV1,
}

/// A visible paid creature bound to one exact validated spell on the stack.
/// Power is the engine's recorded LKI, refreshed immediately before that exact
/// battlefield incarnation leaves. It is never read from a later incarnation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalizedChosenCreatureCostV6 {
    pub stack_index: u32,
    pub source: CardStableRefV1,
    pub chosen: CardStableRefV1,
    pub power_lki: i32,
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

    /// Two simultaneous opponent spells target one Terror. Both Ward triggers
    /// are engine-created and the payer can afford either payment.
    pub(crate) fn ward_multi_targeter_state() -> (GameState, [ObjectId; 2], ObjectId) {
        let mut state = ready_state();
        let terror = put(&mut state, PlayerId::P1, "Tolarian Terror", Zone::Battlefield);
        let spells = ["Lightning Bolt", "Lightning Bolt"]
            .map(|name| put(&mut state, PlayerId::P0, name, Zone::Hand));
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 8;
        for spell in spells {
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
        }
        (state, spells, terror)
    }

    pub(crate) fn reach_ward_payment(state: &mut GameState) {
        for _ in 0..16 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::ChooseEffectBoolean { .. } => return,
                other => panic!("Ward payment expected, got {other:?}"),
            }
        }
        panic!("Ward payment did not suspend");
    }

    pub(crate) fn ward_same_source_abilities_state() -> (GameState, ObjectId) {
        let mut state = ready_state();
        let terror = put(&mut state, PlayerId::P1, "Tolarian Terror", Zone::Battlefield);
        let timberwatch = put(&mut state, PlayerId::P0, "Timberwatch Elf", Zone::Battlefield);
        let ranger = put(&mut state, PlayerId::P0, "Quirion Ranger", Zone::Battlefield);
        let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
        state.players[0].mana_pool[ManaColor::G.pool_index()] = 8;
        engine::step(&mut state, Action::ActivateAbility(timberwatch, 0)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        engine::step(&mut state, Action::ActivateAbility(ranger, 0)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(timberwatch))).unwrap();
        engine::step(&mut state, Action::ChooseCostTarget(forest)).unwrap();
        for _ in 0..8 {
            let decision = engine::advance_until_decision(&mut state);
            if !state.objects.get(timberwatch).tapped { break; }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(!state.objects.get(timberwatch).tapped);
        engine::step(&mut state, Action::ActivateAbility(timberwatch, 0)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        (state, timberwatch)
    }

    #[test]
    fn v6_ward_distinguishes_two_abilities_from_the_same_source() {
        let (mut state, source) = ward_same_source_abilities_state();
        let observation = observe(&state, PlayerId::P0);
        let queued = &observation.extensions.queued_ward_payments;
        assert_eq!(queued.len(), 2);
        assert_ne!(queued[0].payment.targeting_stack_index, queued[1].payment.targeting_stack_index);
        for entry in queued {
            assert_eq!(observation.projection.surface.stack[entry.payment.targeting_stack_index as usize].source.arena_id, source.0);
        }
        reach_ward_payment(&mut state);
        let pending = observe(&state, PlayerId::P0);
        assert_eq!(pending.extensions.pending_ward_payment.as_ref().unwrap().targeting_stack_index,
            queued[1].payment.targeting_stack_index);
        engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.iter().filter(|item| item.source == source).count(), 1);
    }

    #[test]
    fn v6_ward_source_zone_change_retains_the_original_trigger_incarnation() {
        let (mut state, _, terror) = ward_multi_targeter_state();
        let original_count = state.objects.get(terror).zone_change_count;
        crate::event::propose_and_commit(&mut state, crate::event::ProposedEvent::zone_change(terror, Zone::Graveyard));
        crate::event::propose_and_commit(&mut state, crate::event::ProposedEvent::zone_change(terror, Zone::Battlefield));
        reach_ward_payment(&mut state);
        let observation = observe(&state, PlayerId::P0);
        let historical = observation.extensions.historical_public_sources.iter()
            .find(|source| source.context == HistoricalSourceContextV6::PendingEffect).unwrap();
        assert_eq!(historical.source.zone_change_count, original_count);
        assert!(state.objects.get(terror).zone_change_count > original_count);
        assert_eq!(observation.extensions.pending_ward_payment.as_ref().unwrap().generic, 2);
        engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
    }

    /// Defect-2 fixture (`InvalidDecisionRelation`, campaign-001 block-1
    /// sweep offset 26, seed 3157112932801185247): Writhing Chrysalis's own
    /// `TriggerCondition::CastSelf` cast trigger left alone on the stack
    /// after Counterspell counters the spell that produced it (the spell
    /// departs to the graveyard; the trigger, stacked above it, is not yet
    /// resolved). Real self-play never needs the trigger to actually
    /// resolve to hit this: `stack_source_ref` (`rl.rs`) revalidates EVERY
    /// stack item's producer -- including this still-pending one -- every
    /// time any later decision's observation is built, well before the
    /// trigger is ever the top of the stack. Before the engine fix (see
    /// `engine::validate_spell_sourced_trigger`), that per-item
    /// revalidation itself failed with "spell-sourced trigger contract no
    /// longer matches its producer", which every caller of
    /// `policy_observation_extensions_v6` (both the V3 and the V4 actor-
    /// visible encoders alike -- see `rl_session::flat_action_v4`'s
    /// `encode_current_flat_action_slice_v4` and
    /// `flat_policy_observation_v3`/`v4`) folds into a generic
    /// `InvalidDecisionRelation`, for ANY later decision, not only one that
    /// otherwise involves Chrysalis at all. Returns the state with the
    /// bare trigger on the stack, and Chrysalis's own (now-departed) object
    /// id.
    pub(crate) fn spell_sourced_trigger_state_after_producer_departs_v1() -> (GameState, ObjectId) {
        let mut state = ready_state();
        let chrysalis = put(&mut state, PlayerId::P0, "Writhing Chrysalis", Zone::Hand);
        let counterspell = put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
        state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
        state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
        state.players[1].mana_pool[ManaColor::U.pool_index()] = 2;

        engine::step(&mut state, Action::CastSpell(chrysalis)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 2, "the spell and its cast trigger must both be on the stack");

        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
        engine::step(&mut state, Action::CastSpell(counterspell)).unwrap();
        match engine::advance_until_decision(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Object(chrysalis)]);
            }
            other => panic!("expected Counterspell's target decision, got {other:?}"),
        }
        engine::step(&mut state, Action::ChooseTarget(Target::Object(chrysalis))).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 3);

        // Pass until exactly one stack item resolves: Counterspell itself,
        // which also removes its target (the Chrysalis spell) as part of
        // that same resolution, dropping the stack from 3 items to 1 in one
        // step. Stop there, deliberately not letting the bare trigger
        // resolve, so it stays a still-pending, producer-departed stack
        // item for the fixture's caller to observe.
        let starting_len = state.stack.len();
        loop {
            match engine::advance_until_decision(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("unexpected decision while resolving Counterspell: {other:?}"),
            }
            if state.stack.len() < starting_len {
                break;
            }
        }
        assert_eq!(state.stack.len(), 1, "only the still-pending cast trigger should remain");
        assert_eq!(state.stack[0].kind, crate::state::StackItemKind::TriggeredAbility);
        assert_eq!(state.stack[0].source, chrysalis);
        assert!(
            state.players[0].graveyard.contains(&chrysalis),
            "Counterspell must have countered the Chrysalis spell into the graveyard"
        );
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { .. }));
        (state, chrysalis)
    }

    /// Defect-2 regression, always-run and self-contained: building the V6
    /// observation extensions for either player must not fail merely
    /// because a still-pending spell-sourced trigger's producing spell has
    /// already departed the stack. Confirmed to fail with `RlContractError`
    /// ("spell-sourced trigger contract no longer matches its producer")
    /// against the pre-fix `engine::validate_spell_sourced_trigger`.
    #[test]
    fn v6_spell_sourced_trigger_survives_producer_countered_while_still_pending() {
        let (state, chrysalis) = spell_sourced_trigger_state_after_producer_departs_v1();
        for actor in [PlayerId::P0, PlayerId::P1] {
            policy_observation_extensions_v6(&state, actor).unwrap_or_else(|e| {
                panic!("V6 observation extensions must not fail on a departed-producer trigger: {e}")
            });
        }
        let observation = observe(&state, PlayerId::P0);
        assert_eq!(observation.projection.surface.stack.len(), 1);
        assert_eq!(
            observation.projection.surface.stack[0].source.arena_id,
            chrysalis.0
        );
    }

    #[test]
    fn v6_ward_multiple_targeters_have_exact_queued_and_pending_bindings() {
        let (mut state, spells, terror) = ward_multi_targeter_state();
        let queued = observe(&state, PlayerId::P0);
        assert!(queued.extensions.pending_ward_payment.is_none());
        assert_eq!(queued.extensions.queued_ward_payments.len(), 2);
        for (payment, spell) in queued.extensions.queued_ward_payments.iter().zip(spells) {
            let targeter = &queued.projection.surface.stack[payment.payment.targeting_stack_index as usize];
            assert_eq!(targeter.source.arena_id, spell.0);
            assert_eq!(queued.projection.surface.stack[payment.stack_index as usize].source.arena_id, terror.0);
        }
        assert!(observe_policy_v5(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1)
            .unwrap_err().0.contains("Ward-bound"));
        reach_ward_payment(&mut state);
        let pending = observe(&state, PlayerId::P0);
        let payment = pending.extensions.pending_ward_payment.as_ref().unwrap();
        assert_eq!(payment.payer, PlayerSeatV1::P0);
        assert_eq!(payment.generic, 2);
        assert_eq!(pending.projection.surface.stack[payment.targeting_stack_index as usize].source.arena_id, spells[1].0);
        assert_eq!(pending.extensions.queued_ward_payments.len(), 1);
        assert!(crate::rl::observe_v2(&state, &crate::surface_v2::HarnessSurfaceV2::new(), PlayerId::P0, 0)
            .unwrap_err().0.contains("Ward-bound"));

        for pay in [false, true] {
            let mut answered = state.clone();
            engine::step(&mut answered, Action::ChooseEffectBoolean(pay)).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut answered), Decision::CastSpellOrPass { .. }));
            assert_eq!(answered.objects.get(spells[1]).zone, if pay { Zone::Stack } else { Zone::Graveyard });
            assert_eq!(answered.objects.get(spells[0]).zone, Zone::Stack);
            assert_eq!(answered.players[0].mana_pool[ManaColor::R.pool_index()], if pay { 4 } else { 6 });
        }
    }

    #[test]
    fn v6_ward_rejects_valid_but_foreign_targeter_and_changed_cost_or_incarnation() {
        let (mut state, spells, terror) = ward_multi_targeter_state();
        reach_ward_payment(&mut state);
        let foreign_id = state.stack.iter().find(|item| item.source == spells[0]).unwrap().v4.stack_item_id;
        for mutation in 0..4 {
            let mut forged = state.clone();
            if mutation == 3 {
                forged.objects.get_mut(terror).zone_change_count = u32::MAX;
                forged.engine.pending_effect.as_mut().unwrap().resolving_item.v4.ability_source_contract.as_mut().unwrap().zone_change_count = u32::MAX;
            } else {
                let crate::effect::PendingEffectChoice::ChooseBoolean { purpose, .. } = forged.engine.pending_effect.as_mut().unwrap().choice.as_mut().unwrap() else { panic!("Ward Boolean") };
                let crate::effect::EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { targeting_stack_item, generic, player, .. } = purpose else { panic!("Ward purpose") };
                match mutation { 0 => *targeting_stack_item = foreign_id, 1 => *generic = 1, 2 => *player = PlayerId::P1, _ => unreachable!() }
            }
            assert!(policy_observation_extensions_v6(&forged, PlayerId::P0).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn v6_ward_departed_targeter_has_no_queued_payment_and_resolves_as_noop() {
        let (mut state, spells, _) = ward_multi_targeter_state();
        let targeter_id = state.stack.iter().find(|item| item.source == spells[1]).unwrap().v4.stack_item_id;
        crate::engine::counter_stack_item_by_id(&mut state, targeter_id).unwrap().unwrap();
        let observation = observe(&state, PlayerId::P0);
        assert_eq!(observation.extensions.queued_ward_payments.len(), 1);
        reach_ward_payment(&mut state);
        let observation = observe(&state, PlayerId::P0);
        let payment = observation.extensions.pending_ward_payment.unwrap();
        assert_eq!(observation.projection.surface.stack[payment.targeting_stack_index as usize].source.arena_id, spells[0].0);
    }

    #[test]
    fn v6_ward_departed_targeter_does_not_hide_a_malformed_trigger_cost() {
        let (mut state, spells, _) = ward_multi_targeter_state();
        let targeter_id = state.stack.iter().find(|item| item.source == spells[1]).unwrap().v4.stack_item_id;
        crate::engine::counter_stack_item_by_id(&mut state, targeter_id).unwrap().unwrap();
        let trigger = state.stack.last_mut().unwrap();
        let Some(crate::effect::EffectOp::CounterUnlessPaysGeneric { generic, .. }) = trigger.inline_effect.as_mut() else { panic!("Ward trigger") };
        *generic = 1;
        assert!(policy_observation_extensions_v6(&state, PlayerId::P0).is_err());
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
    fn v6_chosen_creature_branch_is_private_and_distinguishes_equal_common_projections() {
        use crate::native_flat_tensorizer_v3::monstrous_emergence_zone_fixture_v3;
        let base = monstrous_emergence_zone_fixture_v3();
        let mut observations = Vec::new();
        for option in [0, 1] {
            let mut state = base.clone();
            engine::step(&mut state, Action::ChooseEffectOption(option)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseCostTargets { .. }
            ));
            let visible = observe(&state, PlayerId::P0);
            assert_eq!(
                visible
                    .extensions
                    .pending_chosen_creature_cost
                    .as_ref()
                    .unwrap()
                    .selected_zone,
                if option == 0 {
                    ChosenCreatureCostZoneV1::Battlefield
                } else {
                    ChosenCreatureCostZoneV1::Hand
                }
            );
            let opponent = observe(&state, PlayerId::P1);
            assert!(opponent.extensions.pending_chosen_creature_cost.is_none());
            assert!(opponent.known_hand_cards.iter().all(Vec::is_empty));
            observations.push(visible);
        }
        assert_eq!(observations[0].projection, observations[1].projection);
        assert_eq!(observations[0].own_hand, observations[1].own_hand);
        assert_ne!(
            observations[0].visible_projection_hash,
            observations[1].visible_projection_hash
        );
    }

    #[test]
    fn v6_chosen_creature_power_retains_refreshed_lki_and_rejects_foreign_power() {
        use crate::native_flat_tensorizer_v3::monstrous_emergence_paid_fixture_v3;
        let mut observations = Vec::new();
        for bonus in [0, 3] {
            let (state, chosen) = monstrous_emergence_paid_fixture_v3(false, Some(bonus));
            let visible = observe(&state, PlayerId::P0);
            let cost = &visible.extensions.finalized_chosen_creature_costs[0];
            assert_eq!(cost.power_lki, 4 + i32::from(bonus));
            assert_eq!(cost.chosen.arena_id, chosen.0);
            assert_eq!(cost.chosen.zone, Zone::Battlefield);
            assert!(state.objects.get(chosen).zone_change_count > cost.chosen.zone_change_count);
            assert_eq!(
                cost.chosen,
                visible.projection.surface.stack[cost.stack_index as usize].paid_cost_refs[0]
            );
            assert_eq!(
                observe(&state, PlayerId::P1)
                    .extensions
                    .finalized_chosen_creature_costs,
                visible.extensions.finalized_chosen_creature_costs
            );
            let mut forged = state.clone();
            forged.stack.last_mut().unwrap().v4.paid_cost_refs[0].power_lki = Some(99);
            assert!(policy_observation_extensions_v6(&forged, PlayerId::P0).is_err());
            observations.push(visible);
        }
        assert_eq!(observations[0].projection, observations[1].projection);
        assert_ne!(
            observations[0].visible_projection_hash,
            observations[1].visible_projection_hash
        );
        let (hand, chosen) = monstrous_emergence_paid_fixture_v3(true, None);
        for actor in [PlayerId::P0, PlayerId::P1] {
            let visible = observe(&hand, actor);
            let cost = &visible.extensions.finalized_chosen_creature_costs[0];
            assert_eq!(cost.chosen.arena_id, chosen.0);
            assert_eq!(cost.chosen.zone, Zone::Hand);
            assert_eq!(cost.power_lki, 4);
        }
    }

    #[test]
    fn v6_explicit_empty_extensions_round_trip_and_commit_to_hash() {
        let state = ready_state();
        let full = observe(&state, PlayerId::P0);
        let value = serde_json::to_value(&full).unwrap();
        assert!(value["extensions"].get("pending_ward_payment").is_none());
        assert!(value["extensions"].get("queued_ward_payments").is_none());
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
            value["extensions"]["pending_chosen_creature_cost"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["extensions"]["finalized_chosen_creature_costs"],
            serde_json::json!([])
        );
        let mut stale = value.clone();
        stale["extensions"]
            .as_object_mut()
            .unwrap()
            .remove("finalized_chosen_creature_costs");
        assert!(serde_json::from_value::<ObservationV6>(stale).is_err());
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
