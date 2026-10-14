//! Eaten Alive mandatory additional-cost facts. Original executable receipts stay private.
use crate::ids::PlayerId;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{GameState, StackSourceContractV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/eaten-public/v46";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BranchV46 {
    SacrificeCreature,
    AdditionalMana3B,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV46 {
    pub object: u32,
    pub zone_change_count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingV46 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub selected_branch: Option<BranchV46>,
    pub payable_branches: Vec<BranchV46>,
    pub selected_creature: Option<CardStableRefV1>,
    pub sacrifice_candidates: Vec<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV46 {
    pub ordinal: u32,
    pub source_at_payment: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub branch: BranchV46,
    pub sacrificed_incarnation: Option<IncarnationV46>,
    pub sacrifice_destination: Option<Zone>,
    pub stack_ordinal: Option<u32>,
    /// Actual original Stack departure, not an assertion about counter versus fizzle.
    pub original_spell_departed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV46 {
    pub schema: String,
    pub producer_sha256: String,
    pub meanings: Vec<crate::rules_meaning_eaten_alive_v1::MeaningV1>,
    pub pending: Option<PendingV46>,
    pub payments: Vec<PaymentV46>,
}
fn branch(pay: bool) -> BranchV46 {
    if pay {
        BranchV46::AdditionalMana3B
    } else {
        BranchV46::SacrificeCreature
    }
}
fn original(source: &StackSourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
pub(crate) fn selected(id: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|d| d.name == "Eaten Alive")
}
pub fn meanings() -> Result<Vec<crate::rules_meaning_eaten_alive_v1::MeaningV1>, RlContractError> {
    crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .filter(|(_, d)| d.name == "Eaten Alive")
        .map(|(id, _)| {
            crate::rules_meaning_eaten_alive_v1::printed_meaning(id as u16).map_err(RlContractError)
        })
        .collect()
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::engine::mandatory_cast_cost_v1::validate_state(state)?;
    if let Some(p) = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|p| p.mandatory_additional_cost.is_some())
    {
        crate::eaten_alive_v1::exact_recipe(p.source_contract.card_def)?;
        crate::engine::validate_pending_cast(state, p)?;
    }
    for event in &state.engine.event_history {
        if let crate::event::CommittedEvent::MandatoryCastPaymentV1 { source, .. } = event {
            crate::eaten_alive_v1::exact_recipe(source.card_def)?;
        }
    }
    Ok(())
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for text in [
        SCHEMA,
        include_str!("eaten_public_v46.rs"),
        include_str!("engine/mandatory_cast_cost_v1.rs"),
        include_str!("engine.rs"),
        include_str!("spell_costs_v1.rs"),
        include_str!("state.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
    ] {
        h.update([0]);
        h.update(text.as_bytes());
    }
    h.update(crate::rules_meaning_eaten_alive_v1::producer_sha256().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn capture(state: &GameState, actor: PlayerId) -> Result<FactsV46, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let pending = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|p| p.mandatory_additional_cost.is_some())
        .map(|p| -> Result<PendingV46, RlContractError> {
            let choice = p
                .mandatory_additional_cost
                .as_ref()
                .ok_or_else(|| RlContractError("mandatory choice erased".into()))?;
            let payable_branches =
                crate::engine::mandatory_cast_cost_v1::consumer_branches(state, p)
                    .map_err(RlContractError)?
                    .into_iter()
                    .map(branch)
                    .collect();
            let sacrifice_candidates = if p.controller == actor && choice.pay_mana != Some(true) {
                state.players[p.controller.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|id| {
                        let object = state.objects.get(*id);
                        object.zone == Zone::Battlefield
                            && object.controller == p.controller
                            && crate::engine::object_has_type(
                                state,
                                *id,
                                crate::card_def::CardType::Creature,
                            )
                    })
                    .map(|id| {
                        crate::sower_restriction_capture_v1::masked_ref(id, state.objects.get(id))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };
            let selected_creature = choice
                .creature
                .map(|b| {
                    crate::sower_restriction_capture_v1::masked_ref(
                        b.object,
                        state.objects.get(b.object),
                    )
                })
                .transpose()?;
            Ok(PendingV46 {
                source: original(&p.source_contract),
                controller: p.controller.into(),
                selected_branch: choice.pay_mana.map(branch),
                payable_branches,
                selected_creature,
                sacrifice_candidates,
            })
        })
        .transpose()?;
    let mut payments = Vec::new();
    for event in &state.engine.event_history {
        if let crate::event::CommittedEvent::MandatoryCastPaymentV1 {
            source,
            choice,
            item,
        } = event
        {
            let stack_ordinal = state
                .stack
                .iter()
                .position(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                .map(|n| u32::try_from(n))
                .transpose()
                .map_err(|_| RlContractError("stack ordinal overflow".into()))?;
            payments.push(PaymentV46 {
                ordinal: u32::try_from(payments.len())
                    .map_err(|_| RlContractError("payment ordinal overflow".into()))?,
                source_at_payment: original(source),
                controller: source.controller.into(),
                branch: branch(choice.pay_mana),
                sacrificed_incarnation: choice.original_creature.map(|b| IncarnationV46 {
                    object: b.object.0,
                    zone_change_count: b.expected_zone_change_count,
                }),
                sacrifice_destination: choice.paid_creature.map(|r| r.zone),
                stack_ordinal,
                original_spell_departed: stack_ordinal.is_none(),
            });
        }
    }
    Ok(FactsV46 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        meanings: meanings()?,
        pending,
        payments,
    })
}
