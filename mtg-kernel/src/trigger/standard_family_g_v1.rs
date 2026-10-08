//! MageZero Standard family G: creatures with triggered abilities. Kept in
//! its own file so parallel Standard batches touch `trigger.rs` only through
//! the `triggers_for` and target-spec tables.

use super::{etb_trigger, TriggerCondition, TriggeredAbilityDef};
use crate::effect::{CreatureSacrificeFilter, EffectOp, ObjectRef, PlayerRef};
use crate::state::Zone;

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

fn target_player_sacrifices_creature_effect() -> EffectOp {
    EffectOp::SacrificeCreature {
        player: PlayerRef::Target(0),
        filter: CreatureSacrificeFilter::Any,
    }
}

fn exile_from_target_opponents_hand_effect() -> EffectOp {
    EffectOp::RevealHandChooseNonlandToLinkedExile {
        player: PlayerRef::Target(0),
    }
}

/// Gatekeeper of Malakir: "When this creature enters, if it was kicked,
/// target player sacrifices a creature."
pub(super) const GATEKEEPER_OF_MALAKIR_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: true,
    intervening_if_controls_another_source_card: false,
    effect: target_player_sacrifices_creature_effect,
}];

/// Deep-Cavern Bat: "When this creature enters, look at target opponent's
/// hand. You may exile a nonland card from it until this creature leaves the
/// battlefield." The return is the linked exile's own duration, not a
/// trigger (see `effect::LinkedHandExileKind`).
pub(super) const DEEP_CAVERN_BAT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [etb_trigger(exile_from_target_opponents_hand_effect)];
