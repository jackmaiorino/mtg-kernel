//! Native venture and publication identity for Forgotten Realms room abilities.

use super::{room_target_spec, DUNGEONS};
use crate::card_def::TargetSpec;
use crate::effect::{EffectContinuation, EffectOp};
use crate::eternal_cards_v1::owned_draws_v1::{record, JournalV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::ids::{PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind};
use crate::trigger::PendingTrigger;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomEntryV1 {
    pub(crate) player: PlayerId,
    pub(crate) source: AbilitySourceContractV4,
    pub(crate) dungeon: u16,
    pub(crate) room: u16,
    pub(crate) entered_at: usize,
    pub(crate) occurrence: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RoomJournalV1 {
    Entered(RoomEntryV1),
    Published { entered_at: usize, item: StackItemId },
}

fn journal(event: &CommittedEvent) -> Option<RoomJournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = event else {
        return None;
    };
    match **journal {
        JournalV1::DungeonRoomV1(row) => Some(row),
        _ => None,
    }
}

fn entry_at(state: &GameState, at: usize) -> Option<RoomEntryV1> {
    let RoomJournalV1::Entered(entry) = journal(state.engine.event_history.get(at)?)? else {
        return None;
    };
    (entry.entered_at == at && DUNGEONS.contains(&entry.dungeon)).then_some(entry)
}

fn active_entry(state: &GameState, player: PlayerId) -> Option<RoomEntryV1> {
    let marker = &state.players[player.index()].dungeon;
    let (at, entry) = state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, event)| match journal(event) {
            Some(RoomJournalV1::Entered(entry)) if entry.player == player => Some((at, entry)),
            _ => None,
        })?;
    (entry.entered_at == at
        && marker.dungeon_id == Some(entry.dungeon)
        && marker.room_id == Some(entry.room)).then_some(entry)
}

pub(super) fn entered(
    state: &mut GameState,
    player: PlayerId,
    source: AbilitySourceContractV4,
    dungeon: u16,
    room: u16,
) -> Result<usize, String> {
    let entered_at = state.engine.event_history.len();
    let occurrence = active_entry(state, player).map_or(entered_at, |entry| entry.occurrence);
    let entry = RoomEntryV1 { player, source, dungeon, room, entered_at, occurrence };
    if source.controller != player || !DUNGEONS.contains(&dungeon) {
        return Err("venture room lost its native controller or dungeon".into());
    }
    record(state, JournalV1::DungeonRoomV1(RoomJournalV1::Entered(entry)));
    Ok(entered_at)
}

fn bound_entry(state: &GameState, op: &EffectOp) -> Option<RoomEntryV1> {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::RoomBoundV1 {
        entered_at, dungeon, room,
    })) = op else {
        return None;
    };
    let entry = entry_at(state, *entered_at)?;
    let first = entry_at(state, entry.occurrence)?;
    (entry.dungeon == *dungeon
        && entry.room == *room
        && first.occurrence == entry.occurrence
        && first.entered_at <= entry.entered_at
        && first.player == entry.player
        && first.dungeon == entry.dungeon)
        .then_some(entry)
}

fn is_bound(op: &EffectOp) -> bool {
    matches!(op, EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::RoomBoundV1 { .. })))
}

fn for_item(state: &GameState, item: &StackItem) -> Option<RoomEntryV1> {
    let entry = bound_entry(state, item.inline_effect.as_ref()?)?;
    let spec = room_target_spec(entry.dungeon, entry.room);
    (item.kind == StackItemKind::TriggeredAbility
        && item.source == entry.source.source
        && item.controller == entry.player
        && item.v4.ability_source_contract == Some(entry.source)
        && item.v4.target_spec == (spec != TargetSpec::None).then_some(spec)
        && item.v4.stack_item_id != StackItemId::default())
        .then_some(entry)
}

fn published_rows(
    state: &GameState,
    item: StackItemId,
    entered_at: usize,
) -> impl Iterator<Item = (usize, usize, StackItemId)> + '_ {
    state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .filter_map(move |(at, event)| match journal(event) {
            Some(RoomJournalV1::Published { entered_at: entry, item: id })
                if id == item || entry == entered_at => Some((at, entry, id)),
            _ => None,
        })
}

pub(crate) fn validate_pending(state: &GameState, pending: &PendingTrigger) -> Result<(), String> {
    if !is_bound(&pending.effect) {
        return Ok(());
    }
    let entry = bound_entry(state, &pending.effect).ok_or("room trigger lost its native entry")?;
    if pending.controller != entry.player
        || pending.source != entry.source.source
        || pending.source_contract != Some(entry.source)
        || pending.target_spec != room_target_spec(entry.dungeon, entry.room)
        || published_rows(state, StackItemId::default(), entry.entered_at).next().is_some()
    {
        return Err("room trigger does not match its unpublished entry".into());
    }
    Ok(())
}

pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if !item.inline_effect.as_ref().is_some_and(is_bound) {
        return Ok(());
    }
    let entry = for_item(state, item).ok_or("room publication lost its native entry")?;
    let item = item.v4.stack_item_id;
    if published_rows(state, item, entry.entered_at).next().is_some() {
        return Err("room entry has already published its trigger".into());
    }
    record(state, JournalV1::DungeonRoomV1(RoomJournalV1::Published {
        entered_at: entry.entered_at,
        item,
    }));
    Ok(())
}

pub(crate) fn published(state: &GameState, item: &StackItem) -> Option<RoomEntryV1> {
    let entry = for_item(state, item)?;
    let id = item.v4.stack_item_id;
    let mut rows = published_rows(state, id, entry.entered_at);
    let (at, entered_at, published_id) = rows.next()?;
    (at > entered_at && entered_at == entry.entered_at && published_id == id && rows.next().is_none())
        .then_some(entry)
}

pub(crate) fn validate_resolving(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    if !pending.resolving_item.inline_effect.as_ref().is_some_and(is_bound) {
        return Ok(());
    }
    let entry = published(state, &pending.resolving_item)
        .ok_or("room resolution lost its native publication")?;
    if pending.ctx.source != entry.source.source
        || pending.ctx.controller != entry.player
        || pending.ctx.ability_source_contract != Some(entry.source)
        || pending.ctx.stack_item_id != Some(pending.resolving_item.v4.stack_item_id)
    {
        return Err("room resolution does not match its original trigger".into());
    }
    Ok(())
}
