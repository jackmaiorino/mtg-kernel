//! MageZero Standard family G: creatures with triggered abilities. Kept in
//! its own file so parallel Standard batches touch `trigger.rs` only through
//! the `triggers_for` and target-spec tables.

use super::{etb_trigger, TriggerCondition, TriggeredAbilityDef};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};

fn create_named_token(name: &str) -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(name)
            .unwrap_or_else(|| panic!("{name} in CARD_DEFS")),
        controller: PlayerRef::Controller,
    }
}

fn investigate_effect() -> EffectOp {
    create_named_token("Clue Token")
}

fn create_map_effect() -> EffectOp {
    create_named_token("Map Token")
}

fn source_explores_effect() -> EffectOp {
    EffectOp::ExploreTarget {
        object: ObjectRef::ThisSource,
    }
}

/// Novice Inspector: "When this creature enters, investigate."
pub(super) const NOVICE_INSPECTOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(investigate_effect)];

/// Sentinel of the Nameless City: "Whenever this creature enters or attacks,
/// create a Map token."
pub(super) const SENTINEL_OF_THE_NAMELESS_CITY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb_trigger(create_map_effect),
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb_trigger(create_map_effect)
    },
];

/// Cenote Scout: "When this creature enters, it explores."
pub(super) const CENOTE_SCOUT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(source_explores_effect)];
