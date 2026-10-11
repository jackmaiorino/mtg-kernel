//! Triggered abilities of the Standard legendary creatures.
use super::{etb_trigger, TriggerCondition, TriggeredAbilityDef};
use crate::effect::EffectOp;
use crate::standard_legends_v1::LegendEffectV1;
fn halana() -> EffectOp {
    EffectOp::StandardLegendV1(LegendEffectV1::SourcePowerCountersAndHaste)
}
pub(super) const HALANA: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfControllerCombat,
    ..etb_trigger(halana)
}];
