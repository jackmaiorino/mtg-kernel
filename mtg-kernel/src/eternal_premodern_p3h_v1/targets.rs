//! Target filters of the wave P3h cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets`, `target_matches`,
//! `in_graveyard` and `StandardTargetV1::counts` ask them first.

use crate::card_def::{CardType, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(target, StandardTargetV1::EnchantmentCardInOwnGraveyardV1)
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    match target {
        StandardTargetV1::EnchantmentCardInOwnGraveyardV1 => Some((1, 1)),
        _ => None,
    }
}

/// Whether this module's filter `target` takes graveyard cards.
pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
    owns(target)
}

/// Auramancer's "target enchantment card from your graveyard": whether
/// `object` satisfies this module's filter `target` for `controller`, or
/// `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    if !owns(target) {
        return None;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return Some(false);
    };
    Some(
        live.zone == Zone::Graveyard
            && live.owner == controller
            && !live.v4.is_token
            && live.spell_copy_origin.is_none()
            && def.has_type(CardType::Enchantment),
    )
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    Some(
        state.players[controller.index()]
            .graveyard
            .iter()
            .copied()
            .filter(|&id| target_matches(target, controller, id, state) == Some(true))
            .map(Target::Object)
            .filter(|candidate| !prefix.contains(candidate))
            .collect(),
    )
}
