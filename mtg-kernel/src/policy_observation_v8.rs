//! Typed life-payment Ward transport. V6/V7 and their frozen tensor encodings
//! remain separate and continue to reject life Ward. Stable references here
//! are transport authority; scorer successors must resolve visible row handles.
use crate::ids::PlayerId;
use crate::policy_observation_v6::{
    DecisionLocalLibraryV6, FinalizedChosenCreatureCostV6, ObservationV6, PendingCastObjectCostV6,
    PendingChosenCreatureCostV6, QueuedWardPaymentV6, WardPaymentV6,
};
use crate::policy_observation_v7::HistoricalSourceContextV7;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError, StackItemKindV2,
};
use crate::state::{GameState, StackItem};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, RlContractError>;
pub const OBSERVATION_SCHEMA_VERSION_V8: u32 = 8;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationV8 {
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
    pub extensions: PolicyObservationExtensionsV8,
    pub visible_projection_hash: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyObservationExtensionsV8 {
    pub pending_cast_object_cost: Option<PendingCastObjectCostV6>,
    pub decision_local_library: Option<DecisionLocalLibraryV6>,
    pub historical_public_sources: Vec<HistoricalPublicSourceV8>,
    pub pending_chosen_creature_cost: Option<PendingChosenCreatureCostV6>,
    pub finalized_chosen_creature_costs: Vec<FinalizedChosenCreatureCostV6>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_ward_payment: Option<WardPaymentV6>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_ward_payments: Vec<QueuedWardPaymentV6>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_ward_life_payment: Option<WardLifePaymentV8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_ward_life_payments: Vec<QueuedWardLifePaymentV8>,
    /// Printed fee and current ability status, before any targeting spell.
    /// Only identities already visible to this actor can have a trait row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ward_life_card_traits: Vec<WardLifeCardTraitV8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WardLifePrintedFaceV8 {
    Front,
    Back,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WardLifeCardTraitV8 {
    /// Transport authority. Model and human successors resolve a visible row.
    pub source: CardStableRefV1,
    pub life: u8,
    pub printed_face: WardLifePrintedFaceV8,
    pub effective: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalPublicSourceV8 {
    pub context: HistoricalSourceContextV7,
    pub source: CardStableRefV1,
    pub stack_item_kind: StackItemKindV2,
}

/// Public ordinal in the complete stack, including nonspell items. This life
/// fee is separate from generic mana; no internal stack id or incarnation
/// counter is added to the relation. The pending resolver already has its
/// public historical source row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WardLifePaymentV8 {
    pub targeting_stack_index: u32,
    pub payer: PlayerSeatV1,
    pub life: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedWardLifePaymentV8 {
    pub stack_index: u32,
    pub payment: WardLifePaymentV8,
}

fn projected_card_sources(base: &ObservationV6) -> Vec<CardStableRefV1> {
    let public = &base.projection.surface;
    let mut sources: Vec<_> = public
        .battlefield
        .iter()
        .chain(public.graveyards.iter())
        .flat_map(|cards| cards.iter())
        .chain(public.exile.iter())
        .map(|card| card.stable.clone())
        .collect();
    sources.extend(public.stack.iter().map(|item| item.source.clone()));
    sources.extend(base.own_hand.iter().map(|card| card.stable.clone()));
    sources.extend(
        base.known_hand_cards
            .iter()
            .flatten()
            .map(|card| card.stable.clone()),
    );
    sources.extend(
        base.known_library_cards
            .iter()
            .flatten()
            .map(|known| known.card.stable.clone()),
    );
    let extensions = &base.extensions;
    if let Some(search) = &extensions.decision_local_library {
        sources.extend(search.cards.iter().map(|card| card.stable.clone()));
    }
    if let Some(cost) = &extensions.pending_cast_object_cost {
        sources.push(cost.source.clone());
        sources.extend(cost.selected.iter().cloned());
    }
    if let Some(cost) = &extensions.pending_chosen_creature_cost {
        sources.push(cost.source.clone());
    }
    for cost in &extensions.finalized_chosen_creature_costs {
        sources.extend([cost.source.clone(), cost.chosen.clone()]);
    }
    sources
}

fn life_ward_card_traits(
    state: &GameState,
    sources: impl IntoIterator<Item = CardStableRefV1>,
) -> Result<Vec<WardLifeCardTraitV8>> {
    use crate::card_def::WardCostDef;
    use crate::state::Zone;
    let mut seen = std::collections::HashSet::new();
    let mut traits = Vec::new();
    for source in sources {
        if !seen.insert(source.clone()) {
            continue;
        }
        let definition = crate::card_def::CARD_DEFS
            .get(usize::from(source.card_db_id))
            .ok_or_else(|| RlContractError("visible life Ward definition is missing".into()))?;
        let (life, printed_face) = match definition.ward_cost {
            Some(WardCostDef::PayLife(life)) => (life, WardLifePrintedFaceV8::Front),
            Some(WardCostDef::BackFacePayLife(life)) => (life, WardLifePrintedFaceV8::Back),
            _ => continue,
        };
        let id = crate::ids::ObjectId(source.arena_id);
        let effective = source.zone == Zone::Battlefield
            && state.objects.try_get(id).is_some_and(|object| {
                object.card_def == source.card_db_id
                    && PlayerSeatV1::from(object.owner) == source.owner
                    && PlayerSeatV1::from(object.controller) == source.controller
                    && object.zone == source.zone
                    && object.zone_change_count == source.zone_change_count
            })
            && crate::trigger::active_printed_ward_cost(state, id) == definition.ward_cost;
        traits.push(WardLifeCardTraitV8 {
            source,
            life,
            printed_face,
            effective,
        });
    }
    Ok(traits)
}

impl ObservationV8 {
    pub(crate) fn from_base(
        state: &GameState,
        actor: PlayerId,
        base: ObservationV6,
    ) -> Result<Self> {
        // Discover identities from the projection, never from hidden objects.
        let mut sources = projected_card_sources(&base);
        let v7 = crate::policy_observation_v7::policy_observation_extensions_v7_from_v6(
            state,
            actor,
            base.extensions,
        )?;
        sources.extend(
            v7.historical_public_sources
                .iter()
                .map(|row| row.source.clone()),
        );
        let ward_life_card_traits = life_ward_card_traits(state, sources)?;
        let payment = |trigger: &StackItem| -> Result<Option<WardLifePaymentV8>> {
            crate::effect::validated_life_ward_observation_targeter(state, trigger)
                .map_err(RlContractError)?
                .map(|(targeter, life)| {
                    let index = state
                        .stack
                        .iter()
                        .position(|live| live.v4.stack_item_id == targeter.v4.stack_item_id)
                        .ok_or_else(|| {
                            RlContractError("life Ward targeter lost public membership".into())
                        })?;
                    Ok(WardLifePaymentV8 {
                        targeting_stack_index: u32::try_from(index).map_err(|_| {
                            RlContractError("life Ward targeter index exceeds u32".into())
                        })?,
                        payer: targeter.controller.into(),
                        life,
                    })
                })
                .transpose()
        };
        let mut queued_ward_life_payments = Vec::new();
        for (index, trigger) in state.stack.iter().enumerate() {
            if state.engine.pending_effect.as_ref().is_some_and(|pending| {
                pending.resolving_item.v4.stack_item_id == trigger.v4.stack_item_id
            }) {
                continue;
            }
            if let Some(payment) = payment(trigger)? {
                queued_ward_life_payments.push(QueuedWardLifePaymentV8 {
                    stack_index: u32::try_from(index).map_err(|_| {
                        RlContractError("life Ward trigger index exceeds u32".into())
                    })?,
                    payment,
                });
            }
        }
        let mut pending_ward_life_payment = None;
        if let Some(pending) = &state.engine.pending_effect {
            crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
            if matches!(
                pending.choice.as_ref(),
                Some(crate::effect::PendingEffectChoice::ChooseBoolean {
                    purpose: crate::effect::EffectBooleanChoicePurpose::CounterUnlessPaysLife { .. },
                    ..
                })
            ) {
                pending_ward_life_payment =
                    Some(payment(&pending.resolving_item)?.ok_or_else(|| {
                        RlContractError("pending life Ward has no live targeter".into())
                    })?);
            }
        }
        Ok(Self {
            schema_version: OBSERVATION_SCHEMA_VERSION_V8,
            kernel_version: base.kernel_version,
            surface_version: base.surface_version,
            policy_surface_version: base.policy_surface_version,
            card_db_hash: base.card_db_hash,
            acting_player: base.acting_player,
            step_index: base.step_index,
            physical_decision_id: base.physical_decision_id,
            substep_index: base.substep_index,
            substep_count: base.substep_count,
            projection: base.projection,
            own_hand: base.own_hand,
            known_library_cards: base.known_library_cards,
            known_hand_cards: base.known_hand_cards,
            extensions: PolicyObservationExtensionsV8 {
                pending_cast_object_cost: v7.pending_cast_object_cost,
                decision_local_library: v7.decision_local_library,
                historical_public_sources: v7
                    .historical_public_sources
                    .into_iter()
                    .map(|row| HistoricalPublicSourceV8 {
                        context: row.context,
                        source: row.source,
                        stack_item_kind: row.stack_item_kind,
                    })
                    .collect(),
                pending_chosen_creature_cost: v7.pending_chosen_creature_cost,
                finalized_chosen_creature_costs: v7.finalized_chosen_creature_costs,
                pending_ward_payment: v7.pending_ward_payment,
                queued_ward_payments: v7.queued_ward_payments,
                pending_ward_life_payment,
                queued_ward_life_payments,
                ward_life_card_traits,
            },
            visible_projection_hash: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_surface_v5::PolicySurfaceV5;
    use crate::rl::{observe_policy_v6, observe_policy_v8};

    #[cfg(feature = "standard-magezero-fixtures")]
    fn observe(state: &GameState, actor: PlayerId) -> ObservationV8 {
        observe_policy_v8(state, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).unwrap()
    }

    #[cfg(feature = "standard-magezero-fixtures")]
    #[test]
    fn projected_life_ward_traits_follow_visibility_and_printed_face() {
        use crate::event::{propose_and_commit, ProposedEvent};
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::{ObjectStateV4, Zone};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let other = actor.opponent();
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let battlefield = put(&mut state, other, "Brutal Cathar", Zone::Battlefield);
            let hand = put(&mut state, actor, "Brutal Cathar", Zone::Hand);
            let hidden = put(&mut state, other, "Brutal Cathar", Zone::Hand);
            put(&mut state, actor, "Brutal Cathar", Zone::Graveyard);
            put(&mut state, actor, "Brutal Cathar", Zone::Library);
            put(&mut state, other, "Brutal Cathar", Zone::Library);
            let initial = observe(&state, actor);
            let traits = &initial.extensions.ward_life_card_traits;
            assert_eq!(traits.len(), 3);
            assert!(traits.iter().all(|row| {
                row.life == 3 && row.printed_face == WardLifePrintedFaceV8::Back && !row.effective
            }));
            assert!(traits.iter().any(|row| row.source.arena_id == hand.0));
            assert!(traits.iter().all(|row| row.source.arena_id != hidden.0));

            // A hidden card identity cannot add a row or alter the commitment.
            let mut changed = state.clone();
            let forest = crate::card_def::card_id_by_name("Forest").unwrap();
            let hidden_card = changed.objects.get_mut(hidden);
            hidden_card.card_def = forest;
            hidden_card.name = "Forest".into();
            hidden_card.v4 = ObjectStateV4::from_card_def(forest);
            assert_eq!(observe(&changed, actor), initial);

            state.reveal_hand_card(actor, other, hidden).unwrap();
            state.reveal_library_top(actor, actor, 1);
            state.reveal_library_top(actor, other, 1);
            assert_eq!(
                observe(&state, actor)
                    .extensions
                    .ward_life_card_traits
                    .len(),
                6
            );
            propose_and_commit(
                &mut state,
                ProposedEvent::transform_in_place(battlefield, 1),
            );
            let transformed = observe(&state, actor);
            let row = transformed
                .extensions
                .ward_life_card_traits
                .iter()
                .find(|row| row.source.arena_id == battlefield.0)
                .unwrap();
            assert!(row.effective);
            assert_eq!(
                crate::trigger::active_printed_ward_cost(&state, battlefield),
                Some(crate::card_def::WardCostDef::BackFacePayLife(3))
            );
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(observe(&restored, actor), transformed);
            let decoded: ObservationV8 =
                serde_json::from_slice(&serde_json::to_vec(&transformed).unwrap()).unwrap();
            assert_eq!(decoded, transformed);
        }
    }

    #[cfg(all(
        feature = "standard-magezero-fixtures",
        feature = "limited-fdn-fixtures"
    ))]
    #[test]
    fn effective_life_ward_uses_exact_live_incarnation_and_ability_status() {
        use crate::event::{propose_and_commit, ProposedEvent};
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::state::{ObjectLinkV4, Zone};
        let mut state = ready_state();
        let source = put(&mut state, PlayerId::P1, "Brutal Cathar", Zone::Battlefield);
        propose_and_commit(&mut state, ProposedEvent::transform_in_place(source, 1));
        let original = observe(&state, PlayerId::P0)
            .extensions
            .ward_life_card_traits[0]
            .clone();
        assert!(original.effective);
        assert_eq!(
            life_ward_card_traits(&state, [original.source.clone(), original.source.clone()])
                .unwrap()
                .len(),
            1
        );

        // A valid starting-board Aura removes the ability, not its printed fee.
        let aura = put(
            &mut state,
            PlayerId::P0,
            "Witness Protection",
            Zone::Battlefield,
        );
        let source_generation = state.objects.get(source).zone_change_count;
        state.objects.get_mut(aura).v4.attached_to = Some(ObjectLinkV4 {
            object: source,
            zone_change_count: source_generation,
        });
        state.objects.get_mut(aura).v4.layer_timestamp = Some(100);
        state.objects.get_mut(source).attachments.push(aura);
        let suppressed = observe(&state, PlayerId::P0);
        let row = &suppressed.extensions.ward_life_card_traits[0];
        assert_eq!(row.life, 3);
        assert!(!row.effective);
        assert!(crate::trigger::active_printed_ward_cost(&state, source).is_none());
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(aura, Zone::Graveyard),
        );
        assert!(
            observe(&state, PlayerId::P0)
                .extensions
                .ward_life_card_traits[0]
                .effective
        );

        propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        assert!(!life_ward_card_traits(&state, [original.source.clone()]).unwrap()[0].effective);
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        propose_and_commit(&mut state, ProposedEvent::transform_in_place(source, 1));
        let current = observe(&state, PlayerId::P0)
            .extensions
            .ward_life_card_traits[0]
            .clone();
        assert!(current.effective);
        assert_ne!(
            current.source.zone_change_count,
            original.source.zone_change_count
        );
        let rows = life_ward_card_traits(&state, [original.source, current.source]).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].effective && rows[1].effective);
    }

    #[test]
    fn ordinary_and_generic_ward_content_preserves_predecessor_projection() {
        let (queued, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
        let mut pending = queued.clone();
        crate::policy_observation_v6::tests::reach_ward_payment(&mut pending);
        for state in [
            crate::policy_observation_v6::tests::ready_state(),
            queued,
            pending,
        ] {
            let surface = PolicySurfaceV5::new();
            let old = observe_policy_v6(&state, &surface, PlayerId::P0, 0, 0, 0, 1).unwrap();
            let new = observe_policy_v8(&state, &surface, PlayerId::P0, 0, 0, 0, 1).unwrap();
            assert_eq!(new.schema_version, 8);
            assert!(new.extensions.pending_ward_life_payment.is_none());
            assert!(new.extensions.queued_ward_life_payments.is_empty());
            assert!(new.extensions.ward_life_card_traits.is_empty());
            let mut value = serde_json::to_value(&new).unwrap();
            assert!(value["extensions"]
                .get("pending_ward_life_payment")
                .is_none());
            assert!(value["extensions"]
                .get("queued_ward_life_payments")
                .is_none());
            assert!(value["extensions"].get("ward_life_card_traits").is_none());
            // Distinct schema and commitment identities, identical prior content.
            value["schema_version"] = serde_json::json!(old.schema_version);
            value["visible_projection_hash"] = serde_json::json!(old.visible_projection_hash);
            assert_eq!(value, serde_json::to_value(old).unwrap());
            let restored: ObservationV8 =
                serde_json::from_slice(&serde_json::to_vec(&new).unwrap()).unwrap();
            assert_eq!(new, restored);
        }
    }
}
