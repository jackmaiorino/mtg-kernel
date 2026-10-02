//! Loyalty state for the planeswalker recipient of a damage spell.
//! Loyalty abilities and attacking a planeswalker remain unsupported.

use crate::card_def::{CardType, CARD_DEFS};
use crate::engine;
use crate::ids::ObjectId;
use crate::state::{GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoyaltyV1 {
    permanent: ObjectLinkV4,
    counters: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneswalkersV1 {
    loyalty: BTreeMap<ObjectId, LoyaltyV1>,
}

pub fn loyalty(state: &GameState, object: ObjectId) -> Option<u32> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield
        || !engine::object_has_type(state, object, CardType::Planeswalker)
    {
        return None;
    }
    Some(
        state
            .planeswalkers_v1
            .as_ref()
            .and_then(|state| state.loyalty.get(&object))
            .filter(|entry| entry.permanent.zone_change_count == live.zone_change_count)
            .map_or(0, |entry| entry.counters),
    )
}

pub(crate) fn after_zone_change(state: &mut GameState, object: ObjectId) {
    if let Some(planeswalkers) = &mut state.planeswalkers_v1 {
        planeswalkers.loyalty.remove(&object);
        if planeswalkers.loyalty.is_empty() {
            state.planeswalkers_v1 = None;
        }
    }
    let live = state.objects.get(object);
    if live.zone == Zone::Battlefield
        && engine::object_has_type(state, object, CardType::Planeswalker)
    {
        let counters = u32::from(
            CARD_DEFS[live.card_def as usize]
                .starting_loyalty
                .unwrap_or(0),
        );
        let entry = LoyaltyV1 {
            permanent: ObjectLinkV4 {
                object,
                zone_change_count: live.zone_change_count,
            },
            counters,
        };
        state
            .planeswalkers_v1
            .get_or_insert_with(Default::default)
            .loyalty
            .insert(object, entry);
    }
}

pub(crate) fn damage(state: &mut GameState, object: ObjectId, amount: i32) {
    let Some(counters) = loyalty(state, object) else {
        return;
    };
    let entry = LoyaltyV1 {
        permanent: ObjectLinkV4 {
            object,
            zone_change_count: state.objects.get(object).zone_change_count,
        },
        counters: counters.saturating_sub(amount.max(0) as u32),
    };
    state
        .planeswalkers_v1
        .get_or_insert_with(Default::default)
        .loyalty
        .insert(object, entry);
}

pub(crate) fn zero_loyalty(state: &GameState, object: ObjectId) -> bool {
    loyalty(state, object) == Some(0)
}
