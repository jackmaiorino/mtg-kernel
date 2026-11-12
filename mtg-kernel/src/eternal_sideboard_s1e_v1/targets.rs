//! Target filter of the wave S1e cards, carried as a `StandardTargetV1`
//! variant. `standard_cards_v1::legal_targets` and `target_matches` ask it
//! first.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::UpToOneOpponentArtifactCreatureOrPlaneswalkerV1
    )
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    owns(target).then_some((1, 0))
}

/// Azure Beastbinder: "up to one target artifact, creature, or planeswalker
/// an opponent controls". Whether `object` satisfies this module's filter
/// `target` for `controller`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    if !owns(target) {
        return None;
    }
    Some(state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.controller != controller
            && [
                CardType::Artifact,
                CardType::Creature,
                CardType::Planeswalker,
            ]
            .into_iter()
            .any(|card_type| crate::engine::object_has_type(state, object, card_type))
    }))
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
        [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&id| target_matches(target, controller, id, state) == Some(true))
            .map(Target::Object)
            .collect(),
    )
}
