//! Curie's original combat trigger and base-power last known information.

use super::{own_definition, queue_trigger, E7aOpV1, CURIE};
use crate::card_def::CARD_DEFS;
use crate::effect::ExecCtx;
use crate::eternal_cards_v1::owned_draws_v1::{record, JournalV1};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::event_history_v1::EventKindV1;
use crate::ids::ObjectId;
use crate::state::{AbilitySourceContractV4, GameState, Zone};

fn base_power(state: &GameState, object: ObjectId) -> u32 {
    crate::engine::effective_base_power(state, object).unwrap_or(0).max(0) as u32
}

fn curie(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && own_definition(state, object).is_some_and(|id| {
            CARD_DEFS.get(usize::from(id)).is_some_and(|definition| definition.name == CURIE)
        })
}

pub(crate) fn record_combat(state: &mut GameState, combat_at: usize) {
    let Some(CommittedEvent::CombatDamageToPlayer {
        source,
        source_zone_change_count,
        amount,
        ..
    }) = state.engine.event_history.get(combat_at)
    else {
        return;
    };
    let Some(live) = state.objects.try_get(*source) else {
        return;
    };
    if *amount <= 0
        || live.zone != Zone::Battlefield
        || live.zone_change_count != *source_zone_change_count
        || !curie(state, *source)
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, *source)
    {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, *source);
    let journal = JournalV1::CurieCombat { combat_at, source };
    let event = CommittedEvent::EternalOwnedDrawV1 {
        journal: Box::new(journal),
    };
    state.engine.event_log.push(event.clone());
    state.engine.event_history.push(event);
}

pub(super) fn collect(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::EternalOwnedDrawV1 { journal } = event else {
            continue;
        };
        if let JournalV1::CurieCombat { combat_at, source } = **journal {
            if combat_source(state, combat_at) == Some(source) {
                queue_trigger(
                    state,
                    source,
                    source.controller,
                    E7aOpV1::CurieDrawBoundV1 { combat_at },
                );
            }
        }
    }
}

pub(crate) fn combat_source(state: &GameState, combat_at: usize) -> Option<AbilitySourceContractV4> {
    let history = &state.engine.event_history;
    let CommittedEvent::EternalOwnedDrawV1 { journal } = history.get(combat_at.checked_add(1)?)?
    else {
        return None;
    };
    let JournalV1::CurieCombat { combat_at: original, source } = **journal else {
        return None;
    };
    (original == combat_at && source.zone == Zone::Battlefield
        && matches!(history.get(combat_at), Some(CommittedEvent::CombatDamageToPlayer {
            source: object, source_zone_change_count, amount, ..
        }) if *object == source.source
            && *source_zone_change_count == source.zone_change_count && *amount > 0))
        .then_some(source)
}

#[derive(Clone, Copy)]
pub(crate) struct DepartureV1 {
    pub(crate) source: AbilitySourceContractV4,
    pub(crate) power: u32,
}

pub(crate) fn departure(state: &GameState, object: ObjectId) -> Option<DepartureV1> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield || !curie(state, object) {
        return None;
    }
    let original_combat = state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1]).iter().any(|event| {
            matches!(event, CommittedEvent::EternalOwnedDrawV1 { journal }
                if matches!(**journal, JournalV1::CurieCombat { combat_at, source }
                    if source.source == object && source.zone_change_count == live.zone_change_count
                        && combat_source(state, combat_at) == Some(source)))
        });
    original_combat.then(|| DepartureV1 {
        source: AbilitySourceContractV4::capture(state, object),
        power: base_power(state, object),
    })
}

pub(crate) fn capture_batch(state: &GameState, events: &[ProposedEvent]) -> Vec<DepartureV1> {
    events.iter().filter_map(|event| match event {
        ProposedEvent::ZoneChange(change) => departure(state, change.object),
        _ => None,
    }).collect()
}

pub(crate) fn record_departure(state: &mut GameState, at: usize, departure: DepartureV1) {
    record(state, JournalV1::CurieDeparture {
        at,
        source: departure.source,
        power: departure.power,
    });
}

pub(crate) fn draw_count(state: &GameState, ctx: &ExecCtx, combat_at: usize) -> Option<u32> {
    let source = combat_source(state, combat_at)?;
    if ctx.ability_source_contract != Some(source) || ctx.controller != source.controller
        || ctx.source != source.source
    {
        return None;
    }
    let live = state.objects.try_get(source.source)?;
    if live.owner != source.owner {
        return None;
    }
    if live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count {
        return Some(base_power(state, source.source));
    }
    departed_count_before(state, combat_at, source, state.engine.event_history.len())?
}

pub(crate) fn departed_count_before(
    state: &GameState,
    combat_at: usize,
    source: AbilitySourceContractV4,
    end: usize,
) -> Option<Option<u32>> {
    let history = &state.engine.event_history;
    let tail = combat_at.checked_add(2)?;
    let first_departure = history.get(tail..end)?.iter().enumerate().find_map(|(offset, event)| {
        match event {
            CommittedEvent::ZoneChange {
                object, from: Zone::Battlefield, to, controller_before,
            } if *object == source.source && *to != Zone::Battlefield => {
                Some((tail + offset, *controller_before))
            }
            _ => None,
        }
    });
    let Some((departure_at, controller)) = first_departure else {
        return Some(None);
    };
    let receipt_at = departure_at.checked_add(1)?;
    if receipt_at >= end {
        return None;
    }
    let CommittedEvent::EternalOwnedDrawV1 { journal } = history.get(receipt_at)?
    else {
        return None;
    };
    match **journal {
        JournalV1::CurieDeparture { at, source: departed, power }
            if at == departure_at && departed.source == source.source
                && departed.owner == source.owner && departed.controller == controller
                && departed.zone == Zone::Battlefield
                && departed.zone_change_count == source.zone_change_count => Some(Some(power)),
        _ => None,
    }
}
