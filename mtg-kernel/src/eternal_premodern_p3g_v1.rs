//! Card programs for the eternal catalog's Premodern sideboard wave P3g
//! (`docs/design/eternal_formats_support_v1.md`): the four cards that
//! complete the sideboards of Landstill and BW Control (Akroma's Vengeance,
//! Arcane Laboratory, Circle of Protection: Red and Infest).
//!
//! Circle of Protection: Red is wave S1d's Circle of Protection: Blue with
//! red, so its menu, shield and departed sources live in
//! `eternal_sideboard_s1d_v1`. Arcane Laboratory joins Archon of Emeria's
//! limit (`eternal_vintage_v1::cast_prohibited`). Infest is Drown in
//! Sorrow's boost without its scry, and Akroma's Vengeance is Nevinyrral's
//! Disk's destruction. Every name here occurs only in the eternal registry.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::effect::EffectOp;
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_sideboard_s1d_v1::{CircleColorV1, S1dOpV1};
use crate::standard_cards_v1::StandardOpV1;

/// Arcane Laboratory: "Each player can't cast more than one spell each
/// turn." Archon of Emeria's limit reads this name.
pub(crate) const ARCANE_LABORATORY: &str = "Arcane Laboratory";

/// Circle of Protection: Red: "{1}: The next time a red source of your
/// choice would deal damage to you this turn, prevent that damage."
pub fn circle_of_protection_red() -> EffectOp {
    crate::eternal_sideboard_s1d_v1::circle_choose(CircleColorV1::Red)
}

/// Infest: "All creatures get -2/-2 until end of turn."
pub fn infest() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(
        S1dOpV1::BoostAllV1 {
            power: -2,
            toughness: -2,
            nonwhite_only: false,
        },
    )))
}

/// Akroma's Vengeance: "Destroy all artifacts, creatures, and enchantments."
/// One simultaneous destruction: regeneration applies, and indestructible
/// permanents stay. Its cycling is an ordinary cycling ability.
pub fn akromas_vengeance() -> EffectOp {
    crate::eternal_premodern_p2a_v1::nevinyrrals_disk()
}
