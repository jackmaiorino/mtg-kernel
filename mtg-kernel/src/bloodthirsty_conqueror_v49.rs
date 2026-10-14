//! Original loss operations for the prepared Bloodthirsty Conqueror successor.
//! Private input worlds are never actor observations or trigger-log events.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LossV49 {
    pub history_index: u64,
    pub player: PlayerId,
    pub amount: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchV49 {
    pub history_start: u64,
    pub history_end: u64,
    pub losses: Vec<LossV49>,
    pub observers: Vec<AbilitySourceContractV4>,
    // The original input is private and excludes history to avoid recursion.
    original_world: Vec<u8>,
}
pub(crate) struct RecorderV49 {
    start: usize,
    observers: Vec<AbilitySourceContractV4>,
    original_world: Vec<u8>,
}
pub const PRINTED_COST: Cost = Cost {
    generic: 3,
    pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
    x_count: 0,
};
pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.has_full_support()
            && d.name == "Bloodthirsty Conqueror"
            && d.object_name == d.name
            && d.cost == PRINTED_COST
            && d.mana_value == 5
            && d.types == [CardType::Creature]
            && d.colors == [ManaColor::B]
            && d.subtypes == [Subtype::Vampire, Subtype::Knight]
            && d.supertypes.is_empty()
            && d.power == Some(5)
            && d.toughness == Some(5)
            && d.keywords == (Keywords::FLYING | Keywords::DEATHTOUCH)
            && d.target_spec == TargetSpec::None
            && !d.is_token
            && !d.is_land
            && d.activated_abilities.is_empty()
            && d.additional_cost.is_none()
            && d.optional_additional_cost.is_none()
            && d.alt_cost.is_none()
            && d.kicker_cost.is_none()
            && d.kicked_target_spec.is_none()
            && d.flashback.is_none()
            && d.escape.is_none()
            && d.plot_cost.is_none()
            && d.madness_cost.is_none()
            && d.mode2.is_none()
            && d.mode3.is_none()
            && d.additional_modes.is_empty()
            && d.omen.is_none()
            && d.adventure.is_none()
            && d.bestow.is_none()
            && d.saga.is_none()
            && d.transform_face.is_none()
            && d.ward_cost.is_none()
            && !d.changeling
            && d.animation.is_none()
            && d.controlled_creature_grant.is_none()
            && d.controlled_counter_keyword.is_none()
            && d.game_outcome_protection_v1.is_none()
            && d.damage_replacement_v1.is_none()
            && d.equipment.is_none()
            && d.attachment.is_none()
            && d.starting_loyalty.is_none()
    })
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .enumerate()
            .any(|(id, _)| owns_definition(id as u16))
    })
}
fn observers(state: &GameState) -> Vec<AbilitySourceContractV4> {
    state
        .objects
        .iter()
        .filter(|(id, object)| {
            object.zone == Zone::Battlefield
                && object.v4.face_index == 0
                && object.v4.face_down_v1.is_none()
                && owns_definition(object.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect()
}
fn losses(state: &GameState, start: usize, end: usize) -> Result<Vec<LossV49>, String> {
    let events = state
        .engine
        .event_history
        .get(start..end)
        .ok_or("Conqueror loss interval changed")?;
    Ok(events
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            let (player, amount) = match event {
                CommittedEvent::LifeLoss { player, amount } => (*player, *amount),
                CommittedEvent::Damage {
                    target: crate::state::Target::Player(player),
                    amount,
                    ..
                } => (*player, *amount),
                _ => return None,
            };
            (amount > 0).then_some(LossV49 {
                history_index: (start + offset) as u64,
                player,
                amount,
            })
        })
        .collect())
}
impl RecorderV49 {
    fn original(state: &GameState) -> Result<Self, String> {
        let mut original = state.clone();
        original.engine.event_history.clear();
        Ok(Self {
            start: state.engine.event_history.len(),
            observers: observers(state),
            original_world: serde_json::to_vec(&original).map_err(|e| e.to_string())?,
        })
    }
    pub(crate) fn capture(state: &GameState) -> Option<Self> {
        if !enabled() {
            return None;
        }
        // GameState consists solely of serializable typed engine state.
        Some(Self::original(state).expect("Conqueror original typed state serializes"))
    }
    pub(crate) fn finish(self, state: &mut GameState) {
        let end = state.engine.event_history.len();
        let actual = losses(state, self.start, end).expect("actual committed interval exists");
        if actual.is_empty() {
            return;
        }
        state
            .engine
            .event_history
            .push(CommittedEvent::ConquerorLossBatchV49 {
                batch: Box::new(BatchV49 {
                    history_start: self.start as u64,
                    history_end: end as u64,
                    losses: actual,
                    observers: self.observers,
                    original_world: self.original_world,
                }),
            });
    }
}
impl BatchV49 {
    pub(crate) fn validate(&self, state: &GameState, index: usize) -> Result<(), String> {
        let start =
            usize::try_from(self.history_start).map_err(|_| "Conqueror start exceeds usize")?;
        let end = usize::try_from(self.history_end).map_err(|_| "Conqueror end exceeds usize")?;
        if start >= end
            || end != index
            || self.losses.is_empty()
            || self.losses != losses(state, start, end)?
        {
            return Err("Conqueror actual loss operation changed".into());
        }
        let mut original: GameState =
            serde_json::from_slice(&self.original_world).map_err(|e| e.to_string())?;
        if !original.engine.event_history.is_empty() {
            return Err("Conqueror recursive original history".into());
        }
        original.engine.event_history = state.engine.event_history[..start].to_vec();
        if observers(&original) != self.observers {
            return Err("Conqueror original observers changed".into());
        }
        for source in &self.observers {
            crate::engine::validate_historical_ability_source_contract(state, *source)?;
        }
        for player in [PlayerId::P0, PlayerId::P1] {
            self.amount_for(player)?;
        }
        Ok(())
    }
    pub(crate) fn amount_for(&self, player: PlayerId) -> Result<i32, String> {
        self.losses
            .iter()
            .filter(|loss| loss.player == player)
            .try_fold(0i32, |amount, loss| {
                amount
                    .checked_add(loss.amount)
                    .ok_or_else(|| "Conqueror actual batch amount overflows".into())
            })
    }
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::ConquerorLossBatchV49 { .. }))
    {
        return Err("Conqueror private loss world entered public trigger log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ConquerorLossBatchV49 { batch } = event {
            batch.validate(state, index)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
