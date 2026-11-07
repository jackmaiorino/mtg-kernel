//! Static abilities and engine hooks of the wave P2b cards: echo and fading
//! counters entering with their permanents, Deranged Hermit's Squirrel lord,
//! Vintara Snapper's conditional shroud, Survival of the Fittest's discard
//! restriction and Nantuko Vigilante's morph.

use super::{
    p2b_state, BLASTODERM, DERANGED_HERMIT, FADE_COUNTERS, MULTANIS_ACOLYTE, NANTUKO_VIGILANTE,
    SURVIVAL_OF_THE_FITTEST, VINTARA_SNAPPER, YAVIMAYA_GRANGER,
};
use crate::card_def::{CardType, Keywords, Subtype};
use crate::eternal_legacy_e9a_v1::{
    active_named, active_permanents, battlefield, definition, definition_name, live_link,
};
use crate::ids::ObjectId;
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{GameState, Zone};

const G: ManaColor = ManaColor::G;

/// The echo cost of `name`, as its colored pips and its generic mana:
/// Deranged Hermit {3}{G}{G}, Multani's Acolyte {G}{G}, Yavimaya Granger
/// {2}{G}.
pub(crate) fn echo_cost(name: &str) -> Option<(&'static [ManaColor], u8)> {
    match name {
        DERANGED_HERMIT => Some((&[G, G], 3)),
        MULTANIS_ACOLYTE => Some((&[G, G], 0)),
        YAVIMAYA_GRANGER => Some((&[G], 2)),
        _ => None,
    }
}

// ---- Entering the battlefield ----------------------------------------------------------

/// What a permanent of this wave carries as it enters: the echo it owes
/// (702.30a) and Blastoderm's three fade counters (fading 3).
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 || live.v4.face_down_v1.is_some()
    {
        return;
    }
    let Some(name) = definition_name(state, object) else {
        return;
    };
    let echo = echo_cost(name).is_some();
    if !echo && name != BLASTODERM {
        return;
    }
    if !active_named(state, object, name) {
        return;
    }
    let link = live_link(state, object);
    let p2b = p2b_state(state);
    if echo {
        p2b.echo_owed.retain(|owed| owed.object != object);
        p2b.echo_owed.push(link);
    } else {
        p2b.fade_counters.retain(|(host, _)| host.object != object);
        p2b.fade_counters.push((link, FADE_COUNTERS));
    }
}

// ---- Keywords and power -----------------------------------------------------------------

/// Vintara Snapper: "This creature has shroud as long as you control no
/// untapped lands."
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    let Some(live) = state.objects.try_get(object) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield || !active_named(state, object, VINTARA_SNAPPER) {
        return Keywords::NONE;
    }
    let controller = live.controller;
    let untapped_land = battlefield(state).any(|permanent| {
        let live = state.objects.get(permanent);
        live.controller == controller
            && !live.tapped
            && crate::engine::object_has_type(state, permanent, CardType::Land)
    });
    if untapped_land {
        Keywords::NONE
    } else {
        Keywords::SHROUD
    }
}

/// Deranged Hermit: "Squirrel creatures get +1/+1." Each Hermit on the
/// battlefield adds one to every Squirrel, whoever controls it.
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
        || !crate::engine::has_effective_subtype(state, object, Subtype::Squirrel)
    {
        return (0, 0);
    }
    let hermits = active_permanents(state, DERANGED_HERMIT, None).count();
    let hermits = i128::try_from(hermits).unwrap_or(i128::MAX);
    (hermits, hermits)
}

// ---- Costs -------------------------------------------------------------------------------

/// Survival of the Fittest: "Discard a creature card". The ability's cost is
/// an ordinary one-card discard; whether `card` may be the card discarded for
/// `source`'s cost.
pub(crate) fn discard_cost_allows(state: &GameState, source: ObjectId, card: ObjectId) -> bool {
    if definition_name(state, source) != Some(SURVIVAL_OF_THE_FITTEST) {
        return true;
    }
    definition(state, card).is_some_and(|def| def.has_type(CardType::Creature))
}

// ---- Morph ---------------------------------------------------------------------------

/// Nantuko Vigilante: "Morph {1}{G}".
pub(crate) fn is_morph(name: &str) -> bool {
    name == NANTUKO_VIGILANTE
}

/// The morph cost of the card named `name`, if it has one.
pub(crate) fn morph_cost_of(name: &str) -> Option<Cost> {
    (name == NANTUKO_VIGILANTE).then_some(Cost {
        pips: &[Pip::Colored(G)],
        generic: 1,
        x_count: 0,
    })
}

// ---- Counters ----------------------------------------------------------------------------

/// The fade counters on `object`'s battlefield incarnation.
pub fn fade_counters(state: &GameState, object: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    let link = crate::state::ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    super::read_state(state).map_or(0, |p2b| {
        p2b.fade_counters
            .iter()
            .find(|(host, _)| *host == link)
            .map_or(0, |(_, count)| *count)
    })
}
