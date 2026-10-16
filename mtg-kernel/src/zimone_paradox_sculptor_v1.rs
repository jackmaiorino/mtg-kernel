//! Genuine Zimone native Partial printing. No complete-consumer admission.
use crate::{
    effect::EffectOp,
    standard_cards_v1::StandardOpV1,
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};

pub(crate) fn combat_counters() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PutPlusOneCountersOnTargets)
}

pub(crate) fn double_counters() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::DoubleAllCountersOnTargets)
}

pub(crate) const TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfControllerCombat,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: combat_counters,
}];

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
