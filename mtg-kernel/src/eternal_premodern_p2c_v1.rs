//! Card programs for the eternal catalog's Premodern wave P2c
//! (`docs/design/eternal_formats_support_v1.md`): the Reanimator and Full
//! English Breakfast cards built so far.
//!
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
//! `eternal_cards_v1` asks this module first, then falls through to the
//! earlier waves.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;

use crate::card_def::{card_id_by_name, TargetSpec};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::Zone;
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};

pub(crate) const PALINCHRON: &str = "Palinchron";
pub(crate) const TREVAS_RUINS: &str = "Treva's Ruins";
pub(crate) const VERDANT_FORCE: &str = "Verdant Force";
pub(crate) const AKROMA: &str = "Akroma, Angel of Wrath";
pub(crate) const TRISKELION: &str = "Triskelion";
const SAPROLING_TOKEN: &str = "Saproling Token";

// ---- Programs named by the card database ----------------------------------------------

/// Unearth: "Return target creature card with mana value 3 or less from your
/// graveyard to the battlefield."
pub fn unearth() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Battlefield,
    }
}

/// Careful Study: "Draw two cards, then discard two cards."
pub fn careful_study() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 2,
        },
    ])
}

/// Palinchron: "Return Palinchron to its owner's hand."
pub fn return_source_to_hand() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Hand,
    }
}

// ---- Triggered abilities ----------------------------------------------------------------

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

/// Palinchron: "When this creature enters, untap up to seven lands."
fn palinchron_enters() -> EffectOp {
    EffectOp::UntapUpToLands {
        chooser: PlayerRef::Controller,
        max_targets: 7,
    }
}

/// Treva's Ruins: "When this land enters, sacrifice it unless you return a
/// non-Lair land you control to its owner's hand." Rith's Grove's entry
/// (wave P1a) is not specific to that card.
fn ruins_enters() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(
        crate::eternal_premodern_p1a_v1::P1aOpV1::GroveV1,
    )))
}

/// Verdant Force: "At the beginning of each upkeep, create a 1/1 green
/// Saproling creature token."
fn force_upkeep() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name(SAPROLING_TOKEN)
            .unwrap_or_else(|| panic!("{SAPROLING_TOKEN} in CARD_DEFS")),
        controller: PlayerRef::Controller,
    }
}

const PALINCHRON_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, palinchron_enters)];
const RUINS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, ruins_enters)];
const FORCE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: false,
    },
    force_upkeep,
)];

/// Triggered abilities of this module's cards; empty for every other name.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        PALINCHRON => &PALINCHRON_TRIGGERS,
        TREVAS_RUINS => &RUINS_TRIGGERS,
        VERDANT_FORCE => &FORCE_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities: none take a target.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        .then_some(TargetSpec::None)
}
