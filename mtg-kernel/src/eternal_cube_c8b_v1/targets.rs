//! Target filters of Vintage Cube wave C8b, carried as a `StandardTargetV1`
//! variant. `standard_cards_v1::legal_targets` and `target_matches` ask these
//! before falling through to their built-in match.

use crate::card_def::CardType;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, Target, Zone};

/// Whether `target` is owned by wave C8b.
pub(crate) fn owns(target: StandardTargetV1) -> bool {
    target == StandardTargetV1::ControlledArtifactThenAnotherControlledArtifactOrCreatureV1
}

/// Whether `object` matches any slot of Saheeli, Sublime Artificer's -2 ability
/// ("Target artifact you control becomes a copy of another target artifact or
/// creature you control until end of turn"). Slot-specific checks live in
/// `legal_targets` and `standard_cards_v1::StandardTargetV1::slot_kind`.
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
            && live.controller == controller
            && (crate::engine::object_has_type(state, object, CardType::Artifact)
                || crate::engine::object_has_type(state, object, CardType::Creature))
    }))
}

/// Legal choices for Saheeli, Sublime Artificer's -2 ability:
/// - Slot 0: an artifact `controller` controls on the battlefield.
/// - Slot 1: another artifact or creature `controller` controls (`!= slot 0`).
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    prefix: &[Target],
    state: &GameState,
) -> Option<Vec<Target>> {
    if !owns(target) {
        return None;
    }
    let controlled = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            state
                .objects
                .try_get(object)
                .is_some_and(|live| live.zone == Zone::Battlefield && live.controller == controller)
        });
    Some(match prefix.len() {
        0 => controlled
            .filter(|&object| crate::engine::object_has_type(state, object, CardType::Artifact))
            .map(Target::Object)
            .collect(),
        1 => {
            let first = prefix[0];
            controlled
                .filter(|&object| {
                    Target::Object(object) != first
                        && (crate::engine::object_has_type(state, object, CardType::Artifact)
                            || crate::engine::object_has_type(state, object, CardType::Creature))
                })
                .map(Target::Object)
                .collect()
        }
        _ => Vec::new(),
    })
}
