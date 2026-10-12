//! Selected Garruk's Uprising static characteristics. Trigger ownership is
//! prepared separately; this unregistered source is not capability admission.
use crate::card_def::{CardType, Keywords, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|def| def.name == "Garruk's Uprising" && def.is_executable())
}

pub(crate) fn active(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && selected_definition(live.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}

/// A continuously granted keyword uses the source's current controller and
/// ordinary layer timestamp. Losing abilities after that timestamp removes
/// the grant; a later source's grant can still survive the same removal.
pub(crate) fn grants_trample(state: &GameState, creature: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(creature) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, creature, CardType::Creature)
        && state.objects.iter().any(|(source, observer)| {
            observer.controller == live.controller
                && active(state, source)
                && crate::continuous_characteristics_v1::grant_survives(
                    state,
                    creature,
                    observer.v4.layer_timestamp.unwrap_or(0),
                )
        })
}

pub(crate) fn grants_keyword(state: &GameState, creature: ObjectId, keyword: Keywords) -> bool {
    keyword == Keywords::TRAMPLE && grants_trample(state, creature)
}
