//! Target filters of the wave P3b cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets` and `target_matches` ask
//! these first.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(target, StandardTargetV1::NonblackCreatureManaValueV1(_))
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    owns(target).then_some((1, 1))
}

/// Plaguebearer: "target nonblack creature with mana value X". Whether
/// `object` satisfies this module's filter `target` using `excluded` as the
/// disallowed color word, or `None` for every other filter.
fn target_matches_with_color(
    target: StandardTargetV1,
    object: ObjectId,
    excluded: ManaColor,
    state: &GameState,
) -> Option<bool> {
    let StandardTargetV1::NonblackCreatureManaValueV1(x) = target else {
        return None;
    };
    let Some(live) = state.objects.try_get(object) else {
        return Some(false);
    };
    let excluded_mask = crate::card_def::mana_color_mask(excluded);
    Some(
        live.zone == Zone::Battlefield
            && crate::engine::object_has_type(state, object, CardType::Creature)
            && crate::engine::object_color_mask(state, object) & excluded_mask == 0
            && crate::engine::object_mana_value(state, object) == u16::from(x),
    )
}

/// Plaguebearer: "target nonblack creature with mana value X". Whether
/// `object` satisfies this module's filter `target`, or `None` for every
/// other filter. Its current colors and mana value count.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    target_matches_with_color(target, object, ManaColor::B, state)
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    text_color_origin: Option<crate::text_color_change_v1::TargetOriginV1>,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    if !prefix.is_empty() {
        return Some(Vec::new());
    }
    let excluded =
        crate::text_color_change_v1::target_origin_color(state, text_color_origin, ManaColor::B);
    Some(
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&id| target_matches_with_color(target, id, excluded, state) == Some(true))
            .map(Target::Object)
            .collect(),
    )
}
