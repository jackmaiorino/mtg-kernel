//! Target filters of the wave S1n cards, carried as `StandardTargetV1`
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
        StandardTargetV1::ControlledArtifactOrCreatureV1
            | StandardTargetV1::AnyNumberEnchantmentsV1
    )
}

/// How many targets this module's filter takes, and how many at least.
pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
    match target {
        StandardTargetV1::ControlledArtifactOrCreatureV1 => Some((1, 1)),
        StandardTargetV1::AnyNumberEnchantmentsV1 => Some((64, 0)),
        _ => None,
    }
}

/// Apostle's Blessing's "target artifact or creature you control" and Kiora's
/// Dismissal's "any number of target enchantments": whether `object` satisfies
/// this module's filter `target` for `controller`, or `None` for every other
/// filter.
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
        StandardTargetV1::ControlledArtifactOrCreatureV1 => {
            live.controller == controller
                && (crate::engine::object_has_type(state, object, CardType::Artifact)
                    || crate::engine::object_has_type(state, object, CardType::Creature))
        }
        StandardTargetV1::AnyNumberEnchantmentsV1 => {
            crate::engine::object_has_type(state, object, CardType::Enchantment)
        }
        _ => return None,
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
    let (maximum, _) = counts(target)?;
    if prefix.len() >= usize::from(maximum) {
        return Some(Vec::new());
    }
    Some(match target {
        StandardTargetV1::ControlledArtifactOrCreatureV1 => state.players[controller.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|&id| target_matches(target, controller, id, state) == Some(true))
            .map(Target::Object)
            .filter(|candidate| !prefix.contains(candidate))
            .collect(),
        StandardTargetV1::AnyNumberEnchantmentsV1 => [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&id| target_matches(target, controller, id, state) == Some(true))
            .map(Target::Object)
            .filter(|candidate| !prefix.contains(candidate))
            .collect(),
        _ => return None,
    })
}
