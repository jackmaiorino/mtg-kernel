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

impl ObservationV8 {
    pub(crate) fn from_base(
        state: &GameState,
        actor: PlayerId,
        base: ObservationV6,
    ) -> Result<Self> {
        let v7 = crate::policy_observation_v7::policy_observation_extensions_v7_from_v6(
            state,
            actor,
            base.extensions,
        )?;
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
            let mut value = serde_json::to_value(&new).unwrap();
            assert!(value["extensions"]
                .get("pending_ward_life_payment")
                .is_none());
            assert!(value["extensions"]
                .get("queued_ward_life_payments")
                .is_none());
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
