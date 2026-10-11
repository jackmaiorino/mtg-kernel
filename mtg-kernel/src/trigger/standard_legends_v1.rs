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

fn shanna() -> EffectOp {
    EffectOp::StandardLegendV1(LegendEffectV1::ShannaPayAndDraw)
}
pub(super) const SHANNA: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningControllerEndStep,
    ..etb_trigger(shanna)
}];

fn djeru() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: crate::effect::PlayerRef::Controller,
        count: crate::effect::LibraryLookCount::Fixed(6),
        rule: crate::effect::LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(crate::effect::LibraryPickSelectionV1 {
                filter: crate::effect::LibraryPickFilterV1::LegendaryCreature,
                optional: true,
                destination: crate::state::Zone::Exile,
                reveal_selected: false,
                without_mana_cost: crate::engine::FreeCastV1(true),
            }),
        },
        pick_x: false,
    }
}
pub(super) const DJERU: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(djeru)
}];
