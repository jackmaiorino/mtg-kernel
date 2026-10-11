//! Attack abilities of the Standard creature lands.
use super::{etb_trigger, TriggerCondition, TriggeredAbilityDef};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};
use crate::state::Zone;

fn bivouac() -> EffectOp {
    EffectOp::AddCountersToTarget {
        target_index: 0,
        optional: false,
        plus1_plus1: 1,
        lifelink: 0,
        stun: 0,
    }
}
fn cottage() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: crate::card_def::card_id_by_name("Food Token").unwrap(),
            controller: PlayerRef::Controller,
        },
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        },
    ])
}
fn fortress() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Opponent,
            amount: 2,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}
fn reef() -> EffectOp {
    EffectOp::MillCards {
        player: PlayerRef::Target(0),
        count: 4,
    }
}
fn ridgeline() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: crate::effect::TargetRef::Target(0),
            power: crate::card_def::DynamicValueDef::Fixed(2),
            toughness: crate::card_def::DynamicValueDef::Fixed(0),
        },
        EffectOp::UntapObject {
            object: ObjectRef::Target(0),
        },
    ])
}
pub(super) const BIVOUAC: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(bivouac)
}];
pub(super) const COTTAGE: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(cottage)
}];
pub(super) const FORTRESS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(fortress)
}];
pub(super) const REEF: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(reef)
}];
pub(super) const RIDGELINE: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(ridgeline)
}];

fn prairie() -> EffectOp {
    EffectOp::BoostOtherControlledCreaturesUntilEndOfTurn {
        power: 1,
        toughness: 1,
    }
}
fn vinestalk() -> EffectOp {
    EffectOp::SetTargetBasePowerToughnessUntilEndOfTurn {
        index: 0,
        power: 3,
        toughness: 3,
    }
}
pub(super) const PRAIRIE: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(prairie)
}];
pub(super) const VINESTALK: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb_trigger(vinestalk)
}];
