//! Source-scoped, explicitly Partial implementation of Valkyrie's Call.
//!
//! Oracle source: pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
//! `Mage.Sets/src/mage/cards/v/ValkyriesCall.java`. The death condition is
//! captured from the pre-departure battlefield; resolution returns only the
//! exact graveyard incarnation under its physical owner's control. The entry
//! counter and characteristics are installed at the battlefield entry
//! boundary, so ordinary counter replacements apply once.
//! The current executable fixture pool has no supported action that animates
//! this enchantment into a creature, so the paid fixture cannot produce a
//! genuine self-death case. The source predicate itself has no source-identity
//! exclusion and captures self-death if a legal native animation path exists.
//! Soulstone Sanctuary's available animation is not a substitute: it grants
//! every creature type, including Angel, so it cannot satisfy this trigger's
//! non-Angel condition while animated.

use crate::{effect::EffectOp, trigger::TriggerCondition};

pub(crate) const fn death() -> EffectOp {
    EffectOp::BindValkyriesCallDeathV1
}

pub(crate) const TRIGGERS: [crate::trigger::TriggeredAbilityDef; 1] =
    [crate::trigger::TriggeredAbilityDef {
        condition: TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(
            crate::card_def::Subtype::Angel,
        ),
        home_zone: crate::state::Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: death,
    }];

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
