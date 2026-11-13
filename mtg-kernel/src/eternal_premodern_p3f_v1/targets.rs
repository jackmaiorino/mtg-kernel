//! Target filter of the wave P3f cards, carried as a `StandardTargetV1`
//! variant. `standard_cards_v1::legal_targets` and `target_matches` ask it
//! first.
//!
//! Contagion's counters are divided as it is announced (601.2d), like
//! Pyrokinesis's damage: two picks among creatures, one counter each, a
//! creature picked again getting the other counter. `eternal_sideboard_s1b_v1`
//! owns the division of the picks.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(target, StandardTargetV1::DividedTwoCountersCreaturesV1)
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    owns(target).then_some((2, 2))
}

/// Contagion: "one or two target creatures". Whether `object` satisfies
/// this module's filter `target`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    if !owns(target) {
        return None;
    }
    Some(state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && crate::engine::object_has_type(state, object, CardType::Creature)
    }))
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter. A creature already
/// picked may be picked again.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    Some(
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&id| target_matches(target, id, state) == Some(true))
            .map(Target::Object)
            .collect(),
    )
}
