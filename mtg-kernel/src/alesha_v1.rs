//! Source-scoped rules for Alesha, Who Laughs at Fate.
//!
//! Oracle/source checkpoint: pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
//! `Mage.Sets/src/mage/cards/a/AleshaWhoLaughsAtFate.java`. The card remains
//! explicitly Partial while the surrounding pool qualification is pending.

use crate::{effect::EffectOp, trigger::TriggerCondition};

pub(crate) const fn attack_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

pub(crate) const fn raid_return() -> EffectOp {
    EffectOp::BindAleshaRaidReturn
}

pub(crate) const TRIGGERS: [crate::trigger::TriggeredAbilityDef; 2] = [
    crate::trigger::TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        home_zone: crate::state::Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: attack_counter,
    },
    crate::trigger::TriggeredAbilityDef {
        condition: TriggerCondition::BeginningControllerEndStepAfterControllerAttackedThisTurn,
        home_zone: crate::state::Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: raid_return,
    },
];

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
