//! Definition-owned continuous player hexproof and noncombat prevention.
//! No installed effect, receipt, or serialized state exists for this source.
use crate::card_def::{CardType, CARD_DEFS};
use crate::event::DamageProposed;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Target, Zone};

fn source_controller(state: &GameState, source: ObjectId) -> Option<PlayerId> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return None;
    }
    let object = state.objects.try_get(source)?;
    let definition = CARD_DEFS.get(usize::from(object.card_def))?;
    (object.zone == Zone::Battlefield
        && object.v4.face_index == 0
        && definition.is_executable()
        && definition.name == "Crystal Barricade"
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source))
    .then_some(object.controller)
}

pub(crate) fn controller_has_hexproof(state: &GameState, player: PlayerId) -> bool {
    state
        .objects
        .iter()
        .any(|(source, _)| source_controller(state, source) == Some(player))
}

pub(crate) fn active(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(source, _)| source_controller(state, source).is_some())
}

/// The recipient is evaluated at the replacement boundary, not announcement.
/// "Other" excludes only the currently active source permanent.
fn protects_creature(state: &GameState, source: ObjectId, target: ObjectId) -> bool {
    let Some(controller) = source_controller(state, source) else {
        return false;
    };
    let Some(recipient) = state.objects.try_get(target) else {
        return false;
    };
    source != target
        && recipient.zone == Zone::Battlefield
        && recipient.controller == controller
        && crate::engine::object_has_type(state, target, CardType::Creature)
}

pub(crate) fn prevents(state: &GameState, damage: &DamageProposed) -> bool {
    if damage.is_combat || damage.amount <= 0 {
        return false;
    }
    let Target::Object(target) = damage.target else {
        return false;
    };
    state
        .objects
        .iter()
        .any(|(source, _)| protects_creature(state, source, target))
}

#[cfg(test)]
#[path = "crystal_barricade_v1_tests.rs"]
mod tests;
