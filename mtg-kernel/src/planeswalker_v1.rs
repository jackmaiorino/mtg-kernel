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

/// Scoped Human V12 recipient facts, not printed planeswalker admission.
/// Inspect every stored ledger row so stale/hidden/foreign recipients cannot
/// be silently omitted from the public observation.
pub(crate) fn validate_heartfire_recipients_v12(state: &GameState) -> Result<(), String> {
    validate_recipients_inner(state, false)
}
pub(crate) fn validate_full_ajani_recipients_v42(state: &GameState) -> Result<(), String> {
    validate_recipients_inner(state, true)
}
fn validate_recipients_inner(state: &GameState, accept_full: bool) -> Result<(), String> {
    let recipient_ids = || {
        state
            .objects
            .iter()
            .filter(|(id, object)| {
                object.zone == Zone::Battlefield
                    && engine::object_has_type(state, *id, CardType::Planeswalker)
            })
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
    };
    let Some(ledger) = &state.planeswalkers_v1 else {
        return if recipient_ids().is_empty() {
            Ok(())
        } else {
            Err("Heartfire V12 missing loyalty recipient ledger".into())
        };
    };
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || ledger.loyalty.is_empty()
    {
        return Err("Heartfire V12 unsupported planeswalker recipient catalog".into());
    }
    for (id, entry) in &ledger.loyalty {
        let live = state
            .objects
            .try_get(*id)
            .ok_or("Heartfire V12 missing loyalty recipient")?;
        let definition = CARD_DEFS
            .get(usize::from(live.card_def))
            .ok_or("Heartfire V12 unknown loyalty recipient definition")?;
        if live.zone != Zone::Battlefield
            || live.v4.face_index != 0
            || entry.permanent.object != *id
            || entry.permanent.zone_change_count != live.zone_change_count
            || definition.name != "Ajani, Caller of the Pride"
            || !(definition.capability == crate::card_def::CardCapability::Partial
                || accept_full && crate::ajani_full_v42::validate_definition(live.card_def).is_ok())
            || definition.starting_loyalty != Some(4)
            || !engine::object_has_type(state, *id, CardType::Planeswalker)
        {
            return Err("Heartfire V12 unsupported or stale loyalty recipient".into());
        }
    }
    let recipients = recipient_ids();
    if recipients.len() != ledger.loyalty.len()
        || recipients.iter().any(|id| !ledger.loyalty.contains_key(id))
    {
        return Err("Heartfire V12 incomplete loyalty recipient ledger".into());
    }
    Ok(())
}
