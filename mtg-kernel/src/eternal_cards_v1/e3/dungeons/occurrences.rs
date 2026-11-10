//! Native venture and publication identity for Forgotten Realms room abilities.

use super::{leads_to, room_of, room_target_spec, DUNGEONS};
use crate::card_def::TargetSpec;
use crate::effect::{EffectContinuation, EffectOp};
use crate::eternal_cards_v1::owned_draws_v1::{record, JournalV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::ids::{PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, GameState, InitiativeTriggerBindingV1, InitiativeTriggerKindV1,
    StackItem, StackItemKind, UndercityRoomV1, UNDERCITY_DUNGEON_ID_V1,
};
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
    Removed { last_room: usize, occurrence: usize, colors: u8 },
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
    let (dungeon, room) = (marker.dungeon_id?, marker.room_id?);
    let (at, entry) = state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, event)| match event {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::DungeonRoomV1(RoomJournalV1::Entered(entry))
                    if entry.player == player => Some((at, entry)),
                _ => None,
            },
            _ => None,
        })?;
    (entry.entered_at == at && entry.dungeon == dungeon && entry.room == room).then_some(entry)
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

pub(crate) fn is_bound(op: &EffectOp) -> bool {
    matches!(op, EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::RoomBoundV1 { .. })))
}

/// The room belongs to the dungeon. The venturing card is only provenance.
pub(crate) fn source_colors(state: &GameState, effect: &EffectOp) -> Result<u8, &'static str> {
    let entry = bound_entry(state, effect).ok_or("room source lost its native entry")?;
    let active = active_entry(state, entry.player)
        .is_some_and(|active| active.occurrence == entry.occurrence);
    let start = entry.entered_at + 1;
    let mut departures = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| match journal(event) {
            Some(RoomJournalV1::Removed { last_room, occurrence, colors })
                if occurrence == entry.occurrence => Some((start + offset, last_room, colors)),
            _ => None,
        });
    let Some((at, last_room, colors)) = departures.next() else {
        return if active {
            Ok(crate::eternal_legacy_e9a_v1::statics::painter_mask(state))
        } else {
            Err("room source lost its dungeon removal")
        };
    };
    if active
        || departures.next().is_some()
        || colors & !0x1f != 0
        || entry.entered_at > last_room
        || last_room >= at
        || !entry_at(state, last_room).is_some_and(|last| {
            last.occurrence == entry.occurrence && last.player == entry.player
                && last.dungeon == entry.dungeon
                && leads_to(last.dungeon, last.room).is_some_and(<[u16]>::is_empty)
        })
    {
        return Err("room source has malformed dungeon removal");
    }
    Ok(colors)
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
    item: Option<StackItemId>,
    entered_at: Option<usize>,
) -> impl Iterator<Item = (usize, usize, StackItemId)> + '_ {
    state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .filter_map(move |(at, event)| match event {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::DungeonRoomV1(RoomJournalV1::Published { entered_at: entry, item: id })
                    if Some(id) == item || Some(entry) == entered_at => Some((at, entry, id)),
                _ => None,
            },
            _ => None,
        })
}

pub(super) fn retained_room_entry(
    state: &GameState,
    player: PlayerId,
    source: AbilitySourceContractV4,
    dungeon: u16,
    room: u16,
) -> Option<RoomEntryV1> {
    state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, event)| match event {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::DungeonRoomV1(RoomJournalV1::Entered(entry))
                    if entry.entered_at == at && entry.player == player && entry.source == source
                        && entry.dungeon == dungeon && entry.room == room => Some(entry),
                _ => None,
            },
            _ => None,
        })
}

pub(crate) fn validate_pending(state: &GameState, pending: &PendingTrigger) -> Result<(), String> {
    if !is_bound(&pending.effect) {
        if let (Some((dungeon, room)), Some(source)) =
            (room_of(&pending.effect), pending.source_contract)
        {
            if retained_room_entry(state, pending.controller, source, dungeon, room).is_some() {
                return Err("native room entry cannot use the legacy trigger".into());
            }
        }
        return Ok(());
    }
    let entry = bound_entry(state, &pending.effect).ok_or("room trigger lost its native entry")?;
    source_colors(state, &pending.effect)?;
    if pending.controller != entry.player
        || pending.source != entry.source.source
        || pending.source_contract != Some(entry.source)
        || pending.target_spec != room_target_spec(entry.dungeon, entry.room)
        || published_rows(state, None, Some(entry.entered_at)).next().is_some()
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
    if published_rows(state, Some(item), Some(entry.entered_at)).next().is_some() {
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
    let mut rows = published_rows(state, Some(id), Some(entry.entered_at));
    let (at, entered_at, published_id) = rows.next()?;
    (at > entered_at && entered_at == entry.entered_at && published_id == id && rows.next().is_none())
        .then_some(entry)
}

fn validated_published_room(state: &GameState, item: &StackItem) -> Result<Option<RoomEntryV1>, String> {
    if let Some(effect) = item.inline_effect.as_ref().filter(|effect| is_bound(effect)) {
        let entry = published(state, item).ok_or("room source lost its native publication")?;
        source_colors(state, effect)?;
        return Ok(Some(entry));
    }
    if item.v4.stack_item_id != StackItemId::default()
        && published_rows(state, Some(item.v4.stack_item_id), None).next().is_some()
    {
        return Err("native room publication lost its bound effect".into());
    }
    Ok(None)
}

pub(crate) fn validate_stack_target_source(state: &GameState, item: &StackItem) -> Result<(), String> {
    validated_published_room(state, item).map(|_| ())
}

pub(crate) fn validate_resolving(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let Some(entry) = validated_published_room(state, &pending.resolving_item)? else {
        return Ok(());
    };
    if pending.ctx.source != entry.source.source
        || pending.ctx.controller != entry.player
        || pending.ctx.ability_source_contract != Some(entry.source)
        || pending.ctx.stack_item_id != Some(pending.resolving_item.v4.stack_item_id)
    {
        return Err("room resolution does not match its original trigger".into());
    }
    Ok(())
}

enum BottommostV1 {
    ForgottenRealms { entry: RoomEntryV1, includes_legacy_rooms: bool },
    Undercity { player: PlayerId, first: u32, bottom: u32 },
}

fn undercity_bottommost(state: &GameState, player: PlayerId) -> Option<BottommostV1> {
    let mut rooms = state.engine.event_history
        .scan_for_v1(&[EventKindV1::InitiativeTrigger])
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(at, event)| match event {
            CommittedEvent::InitiativeTrigger { binding }
                if binding.player == player && usize::try_from(binding.history_index).ok() == Some(at) =>
            {
                match binding.kind {
                    InitiativeTriggerKindV1::UndercityRoom(room) => Some((binding.history_index, room)),
                    _ => None,
                }
            }
            _ => None,
        });
    let (bottom, room) = rooms.next()?;
    if room != UndercityRoomV1::ThroneOfTheDeadThree {
        return None;
    }
    let first = rooms.find_map(|(at, room)| (room == UndercityRoomV1::SecretEntrance).then_some(at));
    Some(BottommostV1::Undercity { player, first: first.unwrap_or(bottom), bottom })
}

fn bottommost(state: &GameState, player: PlayerId) -> Option<BottommostV1> {
    let marker = &state.players[player.index()].dungeon;
    let (dungeon, room) = (marker.dungeon_id?, marker.room_id?);
    if dungeon == UNDERCITY_DUNGEON_ID_V1 {
        return if room == UndercityRoomV1::ThroneOfTheDeadThree.stable_id() {
            undercity_bottommost(state, player)
        } else {
            None
        };
    }
    if !leads_to(dungeon, room).is_some_and(<[u16]>::is_empty) {
        return None;
    }
    let entry = active_entry(state, player)?;
    let first = entry_at(state, entry.occurrence)?;
    if first.player != player || first.dungeon != dungeon || first.occurrence != first.entered_at {
        return None;
    }
    Some(BottommostV1::ForgottenRealms { entry, includes_legacy_rooms: first.room != 1 })
}

fn remove(state: &mut GameState, player: PlayerId, bottom: BottommostV1) {
    let dungeon = bottom.dungeon();
    if let BottommostV1::ForgottenRealms { entry, .. } = bottom {
        let colors = crate::eternal_legacy_e9a_v1::statics::painter_mask(state);
        record(state, JournalV1::DungeonRoomV1(RoomJournalV1::Removed {
            last_room: entry.entered_at,
            occurrence: entry.occurrence,
            colors,
        }));
    }
    let marker = &mut state.players[player.index()].dungeon;
    if let Err(index) = marker.completed_dungeons.binary_search(&dungeon) {
        marker.completed_dungeons.insert(index, dungeon);
    }
    marker.dungeon_id = None;
    marker.room_id = None;
}

impl BottommostV1 {
    fn dungeon(&self) -> u16 {
        match self {
            Self::ForgottenRealms { entry, .. } => entry.dungeon,
            Self::Undercity { .. } => UNDERCITY_DUNGEON_ID_V1,
        }
    }

    fn waits_for_initiative(&self, state: &GameState, binding: InitiativeTriggerBindingV1) -> bool {
        let Self::Undercity { player, first, bottom } = self else {
            return false;
        };
        binding.player == *player
            && (*first..=*bottom).contains(&binding.history_index)
            && matches!(binding.kind, InitiativeTriggerKindV1::UndercityRoom(_))
            && usize::try_from(binding.history_index).ok().is_some_and(|at| {
                state.engine.event_history.get(at) == Some(&CommittedEvent::InitiativeTrigger { binding })
            })
    }

    fn waits_for(
        &self,
        state: &GameState,
        controller: PlayerId,
        effect: &EffectOp,
        source: Option<AbilitySourceContractV4>,
    ) -> bool {
        match self {
            Self::ForgottenRealms { entry, includes_legacy_rooms } => {
                if let Some(room) = bound_entry(state, effect) {
                    return room.occurrence == entry.occurrence
                        && room.player == entry.player
                        && room.player == controller
                        && source == Some(room.source);
                }
                if is_bound(effect) || controller != entry.player {
                    return false;
                }
                let Some((dungeon, room)) = room_of(effect) else {
                    return false;
                };
                dungeon == entry.dungeon && (*includes_legacy_rooms || source.is_some_and(|source| {
                    retained_room_entry(state, controller, source, dungeon, room)
                        .is_some_and(|room| room.occurrence == entry.occurrence)
                }))
            }
            Self::Undercity { .. } => match effect {
                EffectOp::ResolveInitiativeTrigger { binding } => {
                    controller == binding.player
                        && source == Some(binding.source)
                        && self.waits_for_initiative(state, *binding)
                }
                _ => false,
            },
        }
    }

    fn has_waiting_room(&self, state: &GameState, protected: &[PendingTrigger]) -> bool {
        let waiting_stack = |item: &StackItem| {
            item.kind == StackItemKind::TriggeredAbility
                && item.inline_effect.as_ref().is_some_and(|effect| {
                    self.waits_for(state, item.controller, effect, item.v4.ability_source_contract)
                })
        };
        protected.iter().chain(&state.engine.pending_triggers).any(|trigger| {
            self.waits_for(state, trigger.controller, &trigger.effect, trigger.source_contract)
        }) || state.stack.iter().any(waiting_stack)
            || state.engine.pending_effect.as_ref().is_some_and(|pending| waiting_stack(&pending.resolving_item))
            || match self {
                Self::ForgottenRealms { entry, includes_legacy_rooms } => super::super::e3(state).is_some_and(|e3| {
                    e3.room_triggers.iter().any(|room| {
                        room.player == entry.player && room.dungeon == entry.dungeon
                            && room.entered_at.map_or(*includes_legacy_rooms, |at| {
                                entry_at(state, at).is_some_and(|room| room.occurrence == entry.occurrence)
                            })
                    })
                }),
                Self::Undercity { .. } => state.engine.event_log.iter().any(|event| {
                    matches!(event, CommittedEvent::InitiativeTrigger { binding }
                        if self.waits_for_initiative(state, *binding))
                }),
            }
    }
}

/// Another venture removes a bottommost dungeon before starting the next one,
/// even while room abilities from the old occurrence still wait.
pub(crate) fn complete_bottommost(state: &mut GameState, player: PlayerId) {
    if let Some(bottom) = bottommost(state, player) {
        remove(state, player, bottom);
    }
}

/// 704.5t. Protect triggered rooms before placement as well as published rooms
/// and a room whose resolution is suspended for a native choice.
pub(crate) fn complete_ready(state: &mut GameState, protected: &[PendingTrigger]) -> bool {
    let mut changed = false;
    for player in [PlayerId::P0, PlayerId::P1] {
        let Some(bottom) = bottommost(state, player) else {
            continue;
        };
        if !bottom.has_waiting_room(state, protected) {
            remove(state, player, bottom);
            changed = true;
        }
    }
    changed
}
