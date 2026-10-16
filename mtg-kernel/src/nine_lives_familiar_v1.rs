//! Source-scoped rules for Nine-Lives Familiar.
//!
//! Pinned Mage source: `a5c90fe180021e70e2a644ade00eeab07f857a40`,
//! `Mage.Sets/src/mage/cards/n/NineLivesFamiliar.java`.

use crate::{effect::EffectOp, trigger::TriggerCondition};

pub(crate) const fn death_trigger() -> EffectOp {
    EffectOp::BindNineLivesFamiliarReturnV1
}

pub(crate) const TRIGGERS: [crate::trigger::TriggeredAbilityDef; 1] =
    [crate::trigger::TriggeredAbilityDef {
        condition: TriggerCondition::DiesWithRevivalCounterV1,
        home_zone: crate::state::Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: death_trigger,
    }];

/// Frozen RL card rows have no revival-counter field. Refuse before projecting
/// any card carrying one through those older shapes.
pub(crate) fn require_legacy_public_counter_schema(
    state: &crate::state::GameState,
    object: crate::ids::ObjectId,
) -> Result<(), String> {
    if state
        .objects
        .try_get(object)
        .is_some_and(|live| live.counters.revival != 0)
    {
        Err("legacy public counter schema cannot represent revival counters".into())
    } else {
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
