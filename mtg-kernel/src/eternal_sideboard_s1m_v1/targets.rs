//! Target filters of the wave S1m cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets`, `target_matches` and
//! `StandardTargetV1::counts` ask them first.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::NoncreaturePermanentV1 | StandardTargetV1::RepeatableArtifactsV1(_)
    )
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    match target {
        StandardTargetV1::NoncreaturePermanentV1 => Some((1, 1)),
        StandardTargetV1::RepeatableArtifactsV1(slots) => Some((slots, slots)),
        _ => None,
    }
}

/// Extinguisher Battleship's "target noncreature permanent" and Fiery
/// Confluence's "target artifact". Whether `object` satisfies this module's
/// filter `target`, or `None` for every other filter.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    object: ObjectId,
    state: &GameState,
) -> Option<bool> {
    if !owns(target) {
        return None;
    }
    let on_battlefield = state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield);
    Some(
        on_battlefield
            && if target == StandardTargetV1::NoncreaturePermanentV1 {
                !crate::engine::object_has_type(state, object, CardType::Creature)
            } else {
                crate::engine::object_has_type(state, object, CardType::Artifact)
            },
    )
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter. Each of Fiery
/// Confluence's "target artifact" slots belongs to a different mode, so it
/// may name an artifact another slot named (700.2d).
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
