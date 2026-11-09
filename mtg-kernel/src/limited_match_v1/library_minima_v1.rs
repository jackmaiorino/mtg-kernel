//! Positionless public library minima for the FullPool closed-list owner.
use crate::{
    curator_receipts_v1::JournalV1 as Curator,
    etali_primal_storm_receipts_v1::JournalV1 as Etali,
    event::CommittedEvent as Event,
    event_history_v1::EventKindV1,
    fiend_artisan_receipts_v1::JournalV1 as FiendArtisan,
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1::IdentityV1,
    progenitus_public_v1::PRINTING,
    state::{GameState, Zone},
};
use std::collections::BTreeSet;

fn public_departure_name(state: &GameState, at: usize, object: ObjectId) -> Option<u16> {
    // These typed witnesses follow the actual committed move and already
    // belong to the verified FullPool history. None names an unseen slot.
    match state.engine.event_history.get(at + 1)? {
        Event::CuratorReceiptV1 { journal } => match journal.as_ref() {
            Curator::ArrivalVisibility {
                event_at,
                original,
                mask: 3,
                disclosed_printing,
                ..
            } if *event_at == at as u64 && original.object == object => *disclosed_printing,
            _ => None,
        },
        Event::EtaliPrimalStormReceiptV1 { journal } => match journal.as_ref() {
            Etali::TopExileVisibility { arrival, .. } if arrival.source == object => {
                Some(arrival.card_def)
            }
            _ => None,
        },
        Event::FiendArtisanConsumerReceiptV1 { journal } => match journal.as_ref() {
            FiendArtisan::ArrivalVisibility {
                object: actual,
                observed: Some(card),
                ..
            } if *actual == object => match card.identity {
                IdentityV1::Known { card_id } => Some(card_id),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Reduce only native public operations. A same-library replacement can be
/// the same previously seen copy, so it establishes presence, not arrival.
/// A shuffled-away physical alias never participates in the count.
pub(crate) fn minimum_names(state: &GameState, owner: PlayerId) -> Vec<u16> {
    let history = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::ProgenitusShuffledV1]);
    let mut copies = 0usize;
    let mut tokens = BTreeSet::new();
    for (at, event) in history.iter().enumerate() {
        match event {
            Event::CreateToken { object, .. } => {
                tokens.insert(*object);
            }
            Event::ProgenitusShuffledV1 {
                move_history_index,
                source,
                ..
            } if source.owner == owner
                && source.card_def == PRINTING
                && !tokens.contains(&source.source)
                && state
                    .objects
                    .try_get(source.source)
                    .is_some_and(|card| card.spell_copy_origin.is_none()) =>
            {
                if move_history_index.is_some() {
                    copies += 1;
                } else {
                    copies = copies.max(1);
                }
            }
            Event::Draw {
                player,
                object: Some(_),
            } if *player == owner => {
                copies = copies.saturating_sub(1);
            }
            Event::ZoneChange {
                object,
                controller_before,
                from: Zone::Library,
                to,
            } if *controller_before == owner && *to != Zone::Library && copies > 0 => {
                // The ordinary library-to-graveyard route cannot move a
                // Progenitus because its mandatory replacement retains it.
                if *to != Zone::Graveyard
                    && public_departure_name(state, at, *object).is_none_or(|name| name == PRINTING)
                {
                    copies -= 1;
                }
            }
            _ => {}
        }
    }
    vec![PRINTING; copies]
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
#[path = "library_minima_v1_tests.rs"]
mod tests;
