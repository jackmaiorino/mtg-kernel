//! Target filters of the wave P2d cards, carried as `StandardTargetV1`
//! variants. `standard_cards_v1::legal_targets` and `target_matches` ask
//! these first.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is one of this module's filters.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    matches!(
        target,
        StandardTargetV1::ArtifactOrLandV1 | StandardTargetV1::ControlledPermanentV1
    )
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    owns(target).then_some((1, 1))
}

/// Whether `object` satisfies this module's filter `target` for
/// `controller`, or `None` for every other filter.
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
    if live.zone != Zone::Battlefield {
        return Some(false);
    }
    Some(match target {
        // Pillage: "target artifact or land".
        StandardTargetV1::ArtifactOrLandV1 => {
            crate::engine::object_has_type(state, object, CardType::Artifact)
                || crate::engine::object_has_type(state, object, CardType::Land)
        }
        // Devoted Caretaker: "target permanent you control".
        StandardTargetV1::ControlledPermanentV1 => live.controller == controller,
        _ => false,
    })
}

/// The legal choices for this module's filter `target` after the already
/// chosen `prefix`, or `None` for every other filter.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    if !prefix.is_empty() {
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
