//! Public trigger programs indexed by the ordinary stack and pending rows.
use crate::{
    card_def::TargetSpec,
    effect::{EffectOp, PlayerRef},
    rl::{PendingTriggerKindV2, PublicObservationProjectionV2, StackItemKindV2},
    state::{GameState, Zone},
    trigger::{triggers_for, TriggerCondition},
};
use serde::{Deserialize, Serialize};

pub(crate) const NAME: &str = "Scrawling Crawler";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerV1 {
    ControllerUpkeep,
    OpponentDraw,
    /// The existing granting effect supplies the meaning for this row.
    Granted,
}

impl TriggerV1 {
    fn from_program(program: &EffectOp) -> Option<Self> {
        match program {
            EffectOp::Sequence(ops)
                if matches!(ops.as_slice(), [
                    EffectOp::DrawCards { player: PlayerRef::Controller, count: 1 },
                    EffectOp::DrawCards { player: PlayerRef::Opponent, count: 1 },
                ]) => Some(Self::ControllerUpkeep),
            EffectOp::LoseLife { player: PlayerRef::Opponent, amount: 1 } => Some(Self::OpponentDraw),
            _ => None,
        }
    }
}

pub(crate) fn exact_triggers(id: u16) -> bool {
    let [upkeep, draw] = triggers_for(id) else {
        return false;
    };
    upkeep.condition == (TriggerCondition::BeginningOfUpkeep { controller_only: true })
        && draw.condition == TriggerCondition::OpponentDraws
        && [upkeep, draw].iter().all(|ability| {
            ability.home_zone == Zone::Battlefield
                && ability.face_index == 0
                && !ability.intervening_if_kicked
                && !ability.intervening_if_controls_another_source_card
        })
        && TriggerV1::from_program(&(upkeep.effect)()) == Some(TriggerV1::ControllerUpkeep)
        && TriggerV1::from_program(&(draw.effect)()) == Some(TriggerV1::OpponentDraw)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedTriggerV1 {
    pub index: usize,
    pub trigger: TriggerV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub stack: Vec<IndexedTriggerV1>,
    pub pending: Vec<IndexedTriggerV1>,
}

pub(crate) fn pending_printing(id: u16) -> bool {
    crate::card_def::card_id_by_name(NAME) == Some(id)
}

impl FactsV1 {
    pub(crate) fn capture(
        state: &GameState,
        common: &PublicObservationProjectionV2,
    ) -> Result<Self, String> {
        let mut facts = Self { stack: Vec::new(), pending: Vec::new() };
        for (index, row) in common.stack.iter().enumerate() {
            if row.stack_item_kind != StackItemKindV2::TriggeredAbility || !pending_printing(row.source.card_db_id) {
                continue;
            }
            let item = state.stack.get(index).ok_or("Crawler stack row lacks a native item")?;
            let source = item.v4.ability_source_contract.ok_or("Crawler trigger lacks its native source contract")?;
            if source.card_def != row.source.card_db_id
                || source.source != item.source
                || source.source.0 != row.source.arena_id
                || source.zone_change_count != row.source.zone_change_count
                || source.zone != Zone::Battlefield
                || row.source.zone != source.zone
                || row.source.owner != source.owner.into()
                || source.controller != item.controller
                || row.controller != item.controller.into()
            {
                return Err("Crawler stack row differs from its native source".into());
            }
            let trigger = if item.v4.granted_by.is_some() {
                TriggerV1::Granted
            } else {
                if !item.targets.is_empty() {
                    return Err("Crawler's printed trigger has targets".into());
                }
                item.inline_effect.as_ref().and_then(TriggerV1::from_program)
                    .ok_or("Crawler stack program is outside its printed triggers")?
            };
            facts.stack.push(IndexedTriggerV1 { index, trigger });
        }
        let native_pending = state.engine.pending_triggers.iter().chain(
            state.pending_legend_rule_v1.iter().flat_map(|pending| pending.waiting_triggers())
        );
        for (index, (row, pending)) in common.engine_context.pending_triggers.iter().zip(native_pending).enumerate() {
            let Some(public_source) = row.source.as_ref().filter(|source| pending_printing(source.card_db_id)) else {
                continue;
            };
            if row.trigger_kind != PendingTriggerKindV2::TriggeredAbility {
                continue;
            }
            let source = pending.source_contract.ok_or("Crawler pending trigger lacks its native source contract")?;
            if source.card_def != public_source.card_db_id
                || source.source != pending.source
                || source.source.0 != public_source.arena_id
                || source.zone != Zone::Battlefield
                || public_source.owner != source.owner.into()
                || source.controller != pending.controller
                || row.controller != pending.controller.into()
            {
                return Err("Crawler pending row differs from its native source".into());
            }
            let trigger = if pending.granted_by.is_some() {
                TriggerV1::Granted
            } else {
                if pending.target_spec != TargetSpec::None || !pending.targets.is_empty() {
                    return Err("Crawler's printed pending trigger has targets".into());
                }
                TriggerV1::from_program(&pending.effect)
                    .ok_or("Crawler pending program is outside its printed triggers")?
            };
            facts.pending.push(IndexedTriggerV1 { index, trigger });
        }
        facts.validate(common)?;
        Ok(facts)
    }

    pub(crate) fn validate(&self, common: &PublicObservationProjectionV2) -> Result<(), String> {
        let stack = common.stack.iter().enumerate().filter_map(|(index, row)| {
            (row.stack_item_kind == StackItemKindV2::TriggeredAbility && pending_printing(row.source.card_db_id))
                .then_some(index)
        });
        let pending = common.engine_context.pending_triggers.iter().enumerate().filter_map(|(index, row)| {
            (row.trigger_kind == PendingTriggerKindV2::TriggeredAbility
                && row.source.as_ref().is_some_and(|source| pending_printing(source.card_db_id)))
                .then_some(index)
        });
        if self.stack.iter().map(|row| row.index).ne(stack)
            || self.pending.iter().map(|row| row.index).ne(pending)
        {
            return Err("Crawler trigger facts differ from the public row roster".into());
        }
        Ok(())
    }
}
