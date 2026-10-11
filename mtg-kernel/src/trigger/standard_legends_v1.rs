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

pub(super) fn ertai_modes() -> Vec<(crate::card_def::TargetSpec, EffectOp)> {
    use crate::card_def::TargetSpec;
    vec![
        (
            TargetSpec::StackObject,
            EffectOp::StandardLegendV1(LegendEffectV1::CounterStackTargetAndDraw),
        ),
        (
            TargetSpec::AnotherCreatureOrPlaneswalker,
            EffectOp::StandardLegendV1(LegendEffectV1::DestroyTargetAndDraw),
        ),
        (TargetSpec::None, EffectOp::Sequence(vec![])),
    ]
}
pub(super) fn ertai_effect() -> EffectOp {
    EffectOp::Choice {
        controller: crate::effect::PlayerRef::Controller,
        options: ertai_modes().into_iter().map(|(_, op)| op).collect(),
    }
}
pub(super) const ERTAI: [TriggeredAbilityDef; 1] = [etb_trigger(ertai_effect)];

fn lagrella() -> EffectOp {
    EffectOp::StandardLegendV1(LegendEffectV1::LagrellaExileTargets)
}
pub(super) const LAGRELLA: [TriggeredAbilityDef; 1] = [etb_trigger(lagrella)];
