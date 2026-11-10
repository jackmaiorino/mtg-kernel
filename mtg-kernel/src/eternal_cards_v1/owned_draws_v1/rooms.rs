//! The two drawing room abilities and the Mad Wizard's actual drawn cards.

use super::{completed_attempts, has_started, record, InstructionV1, JournalV1, SourceV1};
use crate::effect::{EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, EffectTargetSelectionPurpose, PendingEffectChoice};
use crate::eternal_cards_v1::e3::{
    dungeons::{self, occurrences},
    E3ChoiceV1,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, StackItem, Target, Zone};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum KindV1 {
    Temple,
    MadWizard,
}

impl KindV1 {
    pub(super) fn from_room(dungeon: u16, room: u16) -> Option<Self> {
        match (dungeon, room) {
            (dungeons::LOST_MINE_OF_PHANDELVER, 7) => Some(Self::Temple),
            (dungeons::DUNGEON_OF_THE_MAD_MAGE, 9) => Some(Self::MadWizard),
            _ => None,
        }
    }

    pub(super) fn count(self) -> u32 {
        match self {
            Self::Temple => 1,
            Self::MadWizard => 3,
        }
    }
}

fn recipe(item: &StackItem) -> Option<(KindV1, usize)> {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::RoomBoundV1 {
        entered_at, dungeon, room,
    })) = item.inline_effect.as_ref()?
    else {
        return None;
    };
    Some((KindV1::from_room(*dungeon, *room)?, *entered_at))
}

pub(super) fn owns(state: &GameState, item: &StackItem) -> bool {
    let Some((kind, entered_at)) = recipe(item) else {
        return false;
    };
    occurrences::published(state, item).is_some_and(|entry| {
        entry.entered_at == entered_at
            && KindV1::from_room(entry.dungeon, entry.room) == Some(kind)
            && entry.player == item.controller
            && Some(entry.source) == item.v4.ability_source_contract
    })
}

pub(super) fn tail_started(state: &GameState, item: StackItemId) -> bool {
    state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .any(|event| matches!(event, CommittedEvent::EternalOwnedDrawV1 { journal }
            if journal.room_tail_item() == Some(item)))
}

fn actual_draws(
    state: &GameState,
    player: PlayerId,
    range: Range<usize>,
) -> Option<impl Iterator<Item = (usize, ObjectId)> + '_> {
    let start = range.start;
    Some(state.engine.event_history.get(range)?.iter().enumerate().filter_map(
        move |(offset, event)| match event {
            CommittedEvent::Draw { player: drawer, object: Some(card) } if *drawer == player => {
                Some((start + offset, *card))
            }
            _ => None,
        },
    ))
}

fn live_hand(state: &GameState, player: PlayerId, card: EffectObjectBinding) -> bool {
    card.expected_zone == Zone::Hand
        && state.players[player.index()].hand.contains(&card.object)
        && state.objects.try_get(card.object).is_some_and(|live| {
            live.owner == player && live.zone == Zone::Hand
                && live.zone_change_count == card.expected_zone_change_count
        })
}

pub(crate) fn live_drawn(
    state: &GameState,
    player: PlayerId,
    range: Range<usize>,
) -> Result<Vec<(usize, EffectObjectBinding)>, String> {
    actual_draws(state, player, range)
        .ok_or("Dungeon draw history is out of bounds")?
        .map(|(at, object)| {
            let live = state.objects.try_get(object).ok_or("Dungeon lost its actual draw")?;
            let card = EffectObjectBinding {
                object,
                expected_zone: Zone::Hand,
                expected_zone_change_count: live.zone_change_count,
            };
            live_hand(state, player, card)
                .then_some((at, card))
                .ok_or_else(|| "Dungeon draw left its original hand".into())
        })
        .collect()
}

pub(super) fn finish(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    started_at: usize,
) -> Result<(), String> {
    let item = pending.resolving_item.v4.stack_item_id;
    let player = pending.ctx.controller;
    let draws_end = state.engine.event_history.len();
    let drawn = live_drawn(state, player, started_at + 1..draws_end)?;
    for &(draw_at, card) in &drawn {
        let card_def = state.objects.get(card.object).card_def;
        // spellbench-public-reveal: only actual cards drawn by this room.
        state.reveal_distinct_hand_card(player.opponent(), player, card.object)?;
        record(state, JournalV1::RoomDrawRevealedV1 { item, draw_at, card, card_def });
    }
    record(state, JournalV1::RoomDrawFinishedV1 { item, started_at, draws_end });
    let drawn = drawn.into_iter().map(|(_, card)| card).collect();
    if let Some(choice) = dungeons::mad_wizard_choice(state, pending, drawn, Vec::new()) {
        crate::eternal_cards_v1::e3::offer(pending, choice);
    }
    Ok(())
}

fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else {
        return None;
    };
    Some(**journal)
}

struct RevealedV1 {
    card: EffectObjectBinding,
    card_def: u16,
}

fn finished(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<Option<(usize, Vec<RevealedV1>)>, String> {
    let id = pending.resolving_item.v4.stack_item_id;
    let history = &state.engine.event_history;
    let mut endings = history.scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter().enumerate().filter_map(|(at, event)| match event {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::RoomDrawFinishedV1 { item, started_at, draws_end } if item == id =>
                    Some((at, started_at, draws_end)),
                _ => None,
            },
            _ => None,
        });
    let Some((finish_at, started_at, draws_end)) = endings.next() else {
        return Ok(None);
    };
    let invalid = "Mad Wizard lost its original drawn-card receipt";
    let Some((KindV1::MadWizard, entered_at)) = recipe(&pending.resolving_item) else {
        return Err(invalid.into());
    };
    let (started, attempts) = journal(state, started_at)
        .and_then(JournalV1::start).ok_or(invalid)?;
    let player = pending.ctx.controller;
    let expected_source = pending.ctx.ability_source_contract.map(SourceV1::Ability);
    if endings.next().is_some()
        || started.item != id
        || started.controller != player
        || Some(started.source) != expected_source
        || started.instruction != (InstructionV1::Room { kind: KindV1::MadWizard, entered_at })
        || started.total != KindV1::MadWizard.count()
        || started.initial_library_len.is_some()
        || started_at >= draws_end
        || draws_end > finish_at
        || completed_attempts(&history[started_at + 1..draws_end], started_at) != Some(attempts)
        || journal(state, draws_end - 1)
            != Some(JournalV1::Completed { started_at, ordinal: attempts - 1 })
    {
        return Err(invalid.into());
    }
    let mut actual = actual_draws(state, player, started_at + 1..draws_end).ok_or(invalid)?;
    let drawn = (draws_end..finish_at).map(|at| {
        let Some(JournalV1::RoomDrawRevealedV1 { item, draw_at, card, card_def }) = journal(state, at) else {
            return Err(invalid);
        };
        if item != id || card.expected_zone != Zone::Hand || actual.next() != Some((draw_at, card.object)) {
            return Err(invalid);
        }
        Ok(RevealedV1 { card, card_def })
    }).collect::<Result<Vec<_>, _>>()?;
    if actual.next().is_some() {
        return Err(invalid.into());
    }
    Ok(Some((finish_at, drawn)))
}

fn validate_menu(
    state: &GameState,
    pending: &EffectContinuation,
    offered: &E3ChoiceV1,
    drawn: Vec<RevealedV1>,
) -> Result<(), String> {
    if drawn.iter().any(|reveal| !live_hand(state, pending.ctx.controller, reveal.card)
        || state.objects.get(reveal.card.object).card_def != reveal.card_def)
        || dungeons::mad_wizard_choice(
            state, pending, drawn.into_iter().map(|reveal| reveal.card).collect(), Vec::new(),
        ).as_ref() != Some(offered)
    {
        return Err("Mad Wizard cast menu differs from its actual draws".into());
    }
    Ok(())
}

fn answered(
    state: &GameState,
    finish_at: usize,
    id: StackItemId,
) -> Option<Option<EffectObjectBinding>> {
    match journal(state, finish_at + 1)? {
        JournalV1::RoomDrawAnsweredV1 { item, finished_at, selected }
            if item == id && finished_at == finish_at => Some(selected),
        _ => None,
    }
}

pub(super) fn validate_idle(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<bool, String> {
    if recipe(&pending.resolving_item).is_none() {
        return Ok(false);
    }
    if !owns(state, &pending.resolving_item) {
        return Err("Dungeon draw lost its actual room publication".into());
    }
    let id = pending.resolving_item.v4.stack_item_id;
    let Some((finish_at, drawn)) = finished(state, pending)? else {
        if has_started(state, id) {
            return Err("Dungeon draw already started its original instruction".into());
        }
        if matches!(
            (&pending.choice, pending.frames.as_slice()),
            (None, [EffectFrame::Program { op, path }])
                if path.is_empty() && pending.resolving_item.inline_effect.as_ref() == Some(op)
        ) {
            return Ok(true);
        }
        return Err("Dungeon draw lost its original room instruction".into());
    };
    match (&pending.choice, pending.frames.as_slice(), answered(state, finish_at, id)) {
        (Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::EternalV1 { choice }, ..
        }), [], None) => validate_menu(state, pending, choice, drawn)?,
        (None, [EffectFrame::EternalSelectionV1 { choice, .. }], None) => {
            let mut tail = pending.clone();
            tail.frames.clear();
            validate_menu(state, &tail, choice, drawn)?;
        }
        (None, [], Some(selected))
            if selected.is_none_or(|card| drawn.iter().any(|reveal| reveal.card == card)) => {}
        _ => return Err("Mad Wizard lost its actual cast choice".into()),
    }
    Ok(true)
}

pub(crate) fn record_answer(
    state: &mut GameState,
    pending: &EffectContinuation,
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Result<(), String> {
    if recipe(&pending.resolving_item).is_none() {
        return Ok(());
    }
    let id = pending.resolving_item.v4.stack_item_id;
    let (finish_at, drawn) = finished(state, pending)?
        .ok_or("Mad Wizard answer lost its actual draw stage")?;
    validate_menu(state, pending, choice, drawn)?;
    let card = match selected {
        [] => None,
        [Target::Object(object)] if choice.candidates.contains(&selected[0]) => Some(
            choice.bindings.iter().copied().find(|card| card.object == *object)
                .ok_or("Mad Wizard selected an unoffered card")?,
        ),
        _ => return Err("Mad Wizard changed its actual cast answer".into()),
    };
    if state.engine.event_history.len() != finish_at + 1 {
        return Err("Mad Wizard already consumed its cast choice".into());
    }
    record(state, JournalV1::RoomDrawAnsweredV1 { item: id, finished_at: finish_at, selected: card });
    Ok(())
}

pub(crate) fn validate_child(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &E3ChoiceV1,
    card: EffectObjectBinding,
) -> Result<(), String> {
    if recipe(&pending.resolving_item).is_none() {
        return Ok(());
    }
    let (finish_at, drawn) = finished(state, pending)?
        .ok_or("Mad Wizard child lost its actual draw stage")?;
    validate_menu(state, pending, choice, drawn)?;
    if answered(state, finish_at, pending.resolving_item.v4.stack_item_id) != Some(Some(card)) {
        return Err("Mad Wizard child changed its actual cast answer".into());
    }
    Ok(())
}
