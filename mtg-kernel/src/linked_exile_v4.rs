//! Resolve historical card identity independently of a later battlefield copy.
use crate::card_def::CARD_DEFS;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::event_history_v1::EventKindV1;
use crate::ids::PlayerId;
use crate::state::{GameObject, GameState, LinkedExileRecordV4, ObjectLinkV4, Zone};

fn fiend_or_journey(record: &LinkedExileRecordV4) -> bool {
    CARD_DEFS
        .get(usize::from(record.source.card_def))
        .is_some_and(|def| matches!(def.name, "Mesmeric Fiend" | "Journey to Nowhere"))
}

pub(crate) struct Departure {
    exiled: ObjectLinkV4,
    owner: PlayerId,
    card_def: u16,
}

pub(crate) fn capture_departure(state: &GameState, event: &ProposedEvent) -> Option<Departure> {
    if state.engine.linked_exile_records.is_empty() {
        return None;
    }
    let ProposedEvent::ZoneChange(change) = event else {
        return None;
    };
    let live = state.objects.get(change.object);
    if live.zone != Zone::Exile
        || live.v4.face_down_v1.is_some()
        || !state.engine.linked_exile_records.iter().any(|r| {
            r.exiled == change.object
                && r.exiled_zone_change_count == live.zone_change_count
                && fiend_or_journey(r)
        })
        || state
            .exile
            .iter()
            .filter(|&&id| id == change.object)
            .count()
            != 1
    {
        return None;
    }
    Some(Departure {
        exiled: ObjectLinkV4 {
            object: change.object,
            zone_change_count: live.zone_change_count,
        },
        owner: live.owner,
        card_def: live.card_def,
    })
}

impl Departure {
    pub(crate) fn record(self, state: &mut GameState, at: usize) {
        if matches!(state.engine.event_history.get(at), Some(CommittedEvent::ZoneChange { object, from: Zone::Exile, .. }) if *object == self.exiled.object)
            && self.exiled.zone_change_count.checked_add(1)
                == Some(state.objects.get(self.exiled.object).zone_change_count)
        {
            state
                .engine
                .event_history
                .push(CommittedEvent::LinkedExileDepartureV1 {
                    exiled: self.exiled,
                    owner: self.owner,
                    card_def: self.card_def,
                    move_history_index: at as u64,
                });
        }
    }
}

/// Ordinary moves and finalized casts have distinct entry points and historical markers.
fn departure_printing(state: &GameState, record: &LinkedExileRecordV4) -> Result<Option<u16>, ()> {
    let history = &state.engine.event_history;
    let mut moved = None;
    let mut cast = None;
    let kinds = [
        EventKindV1::LinkedExileDepartureV1,
        EventKindV1::PublishedCastIdentityV1,
    ];
    for (at, event) in history.of_kinds_v2(&kinds) {
        let (slot, definition) = match event {
            CommittedEvent::LinkedExileDepartureV1 {
                exiled,
                owner,
                card_def,
                move_history_index,
            } if exiled.object == record.exiled
                && exiled.zone_change_count == record.exiled_zone_change_count =>
            {
                let move_at = usize::try_from(*move_history_index).map_err(|_| ())?;
                if *owner != record.exiled_owner
                    || move_at.checked_add(1) != Some(at)
                    || !matches!(history.get(move_at), Some(CommittedEvent::ZoneChange { object, from: Zone::Exile, .. }) if *object == exiled.object)
                {
                    return Err(());
                }
                (&mut moved, *card_def)
            }
            CommittedEvent::PublishedCastIdentityV1 { receipt }
                if receipt.spell == record.exiled
                    && receipt.origin_zone == Zone::Exile
                    && receipt.origin_zone_change_count == record.exiled_zone_change_count =>
            {
                let printing = crate::cast_identity_v1::checked_printing(
                    state,
                    at,
                    receipt,
                    record.exiled_owner,
                )?
                .ok_or(())?;
                (&mut cast, printing)
            }
            _ => continue,
        };
        if CARD_DEFS.get(usize::from(definition)).is_none() || slot.replace(definition).is_some() {
            return Err(());
        }
    }
    if matches!((moved, cast), (Some(a), Some(b)) if a != b) {
        return Err(());
    }
    Ok(moved.or(cast))
}

pub(crate) fn target_matches(
    state: &GameState,
    record: &LinkedExileRecordV4,
    live: &GameObject,
) -> bool {
    if live.owner != record.exiled_owner || live.zone_change_count < record.exiled_zone_change_count
    {
        return false;
    }
    if live.zone_change_count == record.exiled_zone_change_count || !fiend_or_journey(record) {
        return live.card_def == record.exiled_card_def;
    }
    match departure_printing(state, record) {
        Ok(Some(printed)) => return printed == record.exiled_card_def,
        Err(()) => return false,
        Ok(None) => {}
    }

    // Older saves can predate the independent exile-departure receipt.
    if live.zone != Zone::Battlefield {
        return live.card_def == record.exiled_card_def;
    }
    match crate::cast_identity_v1::archived_battlefield_cast(state, record.exiled, live.owner) {
        Ok(Some(printed)) => printed == record.exiled_card_def,
        Err(()) => false,
        Ok(None) => {
            crate::standard_cards_v1::archived_copy_printing(
                state,
                ObjectLinkV4 {
                    object: record.exiled,
                    zone_change_count: live.zone_change_count,
                },
                live.card_def,
            ) == Some(record.exiled_card_def)
        }
    }
}
