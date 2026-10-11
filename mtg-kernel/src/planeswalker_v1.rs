//! Loyalty state for battlefield planeswalkers: starting loyalty, damage,
//! loyalty-ability costs and effects that add loyalty counters.

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
    if live.zone != Zone::Battlefield {
        return None;
    }
    state
        .planeswalkers_v1
        .as_ref()
        .and_then(|state| state.loyalty.get(&object))
        .filter(|entry| entry.permanent.zone_change_count == live.zone_change_count)
        .map(|entry| entry.counters)
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
        let counters = crate::standard_cards_v1::scale_counters(
            state,
            live.controller,
            counters as i32,
        )
        .max(0) as u32;
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
    change_loyalty(state, object, -amount.max(0));
}

/// Adds (positive) or removes (negative) loyalty counters on a battlefield
/// planeswalker. Removal stops at zero; the state-based action then puts it
/// into its owner's graveyard.
pub fn change_loyalty(state: &mut GameState, object: ObjectId, delta: i32) {
    let Some(counters) = loyalty(state, object) else {
        return;
    };
    let counters = if delta >= 0 {
        counters.saturating_add(
            crate::standard_cards_v1::scale_counters(
                state,
                state.objects.get(object).controller,
                delta,
            )
            .max(0) as u32,
        )
    } else {
        counters.saturating_sub(delta.unsigned_abs())
    };
    let entry = LoyaltyV1 {
        permanent: ObjectLinkV4 {
            object,
            zone_change_count: state.objects.get(object).zone_change_count,
        },
        counters,
    };
    state
        .planeswalkers_v1
        .get_or_insert_with(Default::default)
        .loyalty
        .insert(object, entry);
}

pub(crate) fn zero_loyalty(state: &GameState, object: ObjectId) -> bool {
    engine::object_has_type(state, object, CardType::Planeswalker)
        && loyalty(state, object).unwrap_or(0) == 0
}

/// Counters may be placed on a permanent even when it is not a planeswalker.
pub(crate) fn add_loyalty_counters(state: &mut GameState, object: ObjectId, amount: i32) {
    if amount <= 0 || state.objects.get(object).zone != Zone::Battlefield {
        return;
    }
    if loyalty(state, object).is_none() {
        let permanent = ObjectLinkV4 {
            object,
            zone_change_count: state.objects.get(object).zone_change_count,
        };
        state
            .planeswalkers_v1
            .get_or_insert_with(Default::default)
            .loyalty
            .insert(
                object,
                LoyaltyV1 {
                    permanent,
                    counters: 0,
                },
            );
    }
    change_loyalty(state, object, amount);
}
