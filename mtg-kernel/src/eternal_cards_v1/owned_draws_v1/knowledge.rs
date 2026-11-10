//! The paid physical Accumulated Knowledge spell owns two sequential instructions.
use super::{
    completed_attempts, has_started, record, start_instruction, InstructionV1,
    JournalV1 as DrawJournal, SourceV1, StartedV1,
};
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectContinuation, EffectFrame, EffectOp};
use crate::event::{self, CommittedEvent};
use crate::ids::StackItemId;
use crate::state::{GameState, StackItem, StackItemKind, StackSourceContractV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Paid {
        item: StackItemId,
        source: StackSourceContractV4,
        stack_sha256: [u8; 32],
    },
    Second {
        paid_at: usize,
        first_at: usize,
        count: u32,
    },
}
fn row(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else {
        return None;
    };
    let DrawJournal::KnowledgeV1(row) = **journal else {
        return None;
    };
    Some(row)
}
fn rows(state: &GameState) -> impl Iterator<Item = (usize, JournalV1)> + '_ {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, _)| row(state, at).map(|row| (at, row)))
}
fn note(state: &mut GameState, row: JournalV1) -> usize {
    let at = state.engine.event_history.len();
    record(state, DrawJournal::KnowledgeV1(row));
    at
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures")
        && *op == crate::eternal_premodern_p1b_v1::accumulated_knowledge()
}
fn source(item: &StackItem) -> Option<StackSourceContractV4> {
    let source = item.v4.source_contract?;
    (cfg!(feature = "eternal-fixtures")
        && item.kind == StackItemKind::Spell
        && !item.is_copy
        && item.inline_effect.is_none()
        && item.targets.is_empty()
        && item.v4.target_contracts.is_empty()
        && source.source == item.source
        && source.controller == item.controller
        && source.zone == Zone::Stack
        && item.v4.cast_method == Some(source.cast_method)
        && source.spell_copy_origin.is_none()
        && source
            .spell_cast_origin
            .is_some_and(|origin| origin.finalized_method == Some(source.cast_method))
        && CARD_DEFS.get(usize::from(source.card_def))?.object_name == "Accumulated Knowledge"
        && CARD_DEFS[usize::from(source.card_def)]
            .printed_mode_effect(item.mode_chosen)
            .as_ref()
            .is_some_and(recipe))
    .then_some(source)
}
fn digest(item: &StackItem) -> Option<[u8; 32]> {
    Some(Sha256::digest(serde_json::to_vec(item).ok()?).into())
}
/// This hook runs only after the native cast has paid and published its final row.
pub(crate) fn record_cast(state: &mut GameState, item: StackItemId) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let live = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == item)
        .ok_or("Knowledge cast lost its native row")?;
    let Some(source) = source(live) else {
        return Ok(());
    };
    if StackSourceContractV4::capture(state, live.source, source.cast_method) != source {
        return Err("Knowledge cast changed its physical source".into());
    }
    let stack_sha256 = digest(live).ok_or("Knowledge cast could not bind its paid row")?;
    note(
        state,
        JournalV1::Paid {
            item,
            source,
            stack_sha256,
        },
    );
    Ok(())
}
pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(usize, StackSourceContractV4)> {
    let item = &pending.resolving_item;
    let source = source(item)?;
    let ctx = &pending.ctx;
    if StackSourceContractV4::capture(state, item.source, source.cast_method) != source
        || ctx.source != item.source
        || ctx.controller != item.controller
        || ctx.stack_item_id != Some(item.v4.stack_item_id)
        || ctx.targets != item.targets
        || ctx.target_contracts != item.v4.target_contracts
        || ctx.discarded != item.discarded
        || ctx.paid_cost_refs != item.v4.paid_cost_refs
        || ctx.ability_source_contract != item.v4.ability_source_contract
        || ctx.hidden_ability_source != item.v4.hidden_ability_source
        || ctx.cauldron_grant != item.v4.cauldron_grant
        || ctx.kicked != item.kicked
        || ctx.optional_additional_cost_paid != item.v4.optional_additional_cost_paid
        || ctx.x_value != item.v4.x_value
    {
        return None;
    }
    let mut receipts = rows(state).filter_map(|(at, row)| match row {
        JournalV1::Paid {
            item: original,
            source: original_source,
            stack_sha256,
        } if original == item.v4.stack_item_id => Some((at, original_source, stack_sha256)),
        _ => None,
    });
    let (at, original, hash) = receipts.next()?;
    (receipts.next().is_none() && original == source && digest(item) == Some(hash))
        .then_some((at, source))
}
pub(super) fn validate_idle(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<bool, String> {
    let Some((_, _)) = selected(state, pending) else {
        return Ok(false);
    };
    if has_started(state, pending.resolving_item.v4.stack_item_id)
        || pending.choice.is_some()
        || pending.answered_choice_guard.is_some()
        || !matches!(pending.frames.as_slice(), [EffectFrame::Program { op, path }] if path.is_empty() && recipe(op))
    {
        return Err("Knowledge lost or replayed its original unresolved root".into());
    }
    Ok(true)
}
pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    path: &[u16],
) -> Result<(), String> {
    let (paid_at, source) =
        selected(state, pending).ok_or("Knowledge lacks its actual paid physical cast")?;
    if !path.is_empty()
        || !pending.frames.is_empty()
        || state.eternal_owned_draw_progress_v1.is_some()
        || has_started(state, pending.resolving_item.v4.stack_item_id)
        || state
            .stack
            .iter()
            .any(|item| item.v4.stack_item_id == pending.resolving_item.v4.stack_item_id)
    {
        return Err("Knowledge cannot replay or skip its first instruction".into());
    }
    let extra = event::draw_instruction_extra(state, pending.ctx.controller, 1);
    start_instruction(
        state,
        pending,
        InstructionV1::Knowledge {
            paid_at,
            second_at: None,
        },
        SourceV1::Spell(source),
        1,
        extra,
    );
    Ok(())
}
/// Reconstruct this instruction's count from public zone events. Later dredges
/// can change the current graveyards but cannot change the captured count.
fn graveyard_count_at(state: &GameState, boundary: usize) -> Option<u32> {
    let named = |object| {
        state.objects.try_get(object).is_some_and(|card| {
            CARD_DEFS[usize::from(card.card_def)].object_name == "Accumulated Knowledge"
        })
    };
    let mut cards: std::collections::HashSet<_> = state
        .players
        .iter()
        .flat_map(|p| p.graveyard.iter().copied())
        .filter(|&object| named(object))
        .collect();
    for event in state.engine.event_history.get(boundary..)?.iter().rev() {
        if let CommittedEvent::ZoneChange {
            object, from, to, ..
        } = event
        {
            if from != to && named(*object) {
                if *to == Zone::Graveyard {
                    if !cards.remove(object) {
                        return None;
                    }
                }
                if *from == Zone::Graveyard {
                    if !cards.insert(*object) {
                        return None;
                    }
                }
            }
        }
    }
    u32::try_from(cards.len()).ok()
}
pub(super) fn count(state: &GameState, second_at: Option<usize>) -> Option<u32> {
    let Some(at) = second_at else {
        return Some(1);
    };
    let JournalV1::Second { count, .. } = row(state, at)? else {
        return None;
    };
    (graveyard_count_at(state, at + 1) == Some(count)).then_some(count)
}
pub(super) fn finish_first(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    paid_at: usize,
    first_at: usize,
    source: SourceV1,
) -> Result<(), String> {
    let count = graveyard_count_at(state, state.engine.event_history.len())
        .ok_or("Knowledge cannot capture its second count")?;
    let second_at = note(
        state,
        JournalV1::Second {
            paid_at,
            first_at,
            count,
        },
    );
    if count > 0 {
        let extra = event::draw_instruction_extra(state, pending.ctx.controller, count);
        start_instruction(
            state,
            pending,
            InstructionV1::Knowledge {
                paid_at,
                second_at: Some(second_at),
            },
            source,
            count,
            extra,
        );
    }
    Ok(())
}
pub(super) fn matches_start(
    state: &GameState,
    pending: &EffectContinuation,
    started: StartedV1,
    started_at: usize,
) -> bool {
    let InstructionV1::Knowledge { paid_at, second_at } = started.instruction else {
        return false;
    };
    let Some((actual_paid, source)) = selected(state, pending) else {
        return false;
    };
    if paid_at != actual_paid
        || paid_at >= started_at
        || started.source != SourceV1::Spell(source)
        || started.item != pending.resolving_item.v4.stack_item_id
        || started.controller != source.controller
        || started.initial_library_len.is_some()
        || count(state, second_at) != Some(started.total)
    {
        return false;
    }
    let mut firsts = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| {
            let CommittedEvent::EternalOwnedDrawV1 { journal } = event else {
                return None;
            };
            let (first, attempts) = journal.start()?;
            (first.item == started.item
                && first.instruction
                    == InstructionV1::Knowledge {
                        paid_at,
                        second_at: None,
                    })
            .then_some((at, first, attempts))
        });
    let Some((first_at, first, attempts)) = firsts.next() else {
        return false;
    };
    if firsts.next().is_some()
        || first.source != started.source
        || first.controller != started.controller
        || first.total != 1
        || first.initial_library_len.is_some()
        || first_at <= paid_at
    {
        return false;
    }
    let Some(second_at) = second_at else {
        return started_at == first_at;
    };
    let Some(JournalV1::Second {
        paid_at: original_paid,
        first_at: original_first,
        ..
    }) = row(state, second_at)
    else {
        return false;
    };
    original_paid == paid_at && original_first == first_at && second_at > first_at && started_at == second_at + 1
        && rows(state).filter(|(_, row)| matches!(row, JournalV1::Second { paid_at: original, .. } if *original == paid_at)).count() == 1
        && completed_attempts(&state.engine.event_history[first_at + 1..second_at], first_at) == Some(attempts)
        && matches!(state.engine.event_history.get(second_at - 1), Some(CommittedEvent::EternalOwnedDrawV1 { journal }) if **journal == (DrawJournal::Completed { started_at: first_at, ordinal: attempts - 1 }))
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.eternal_owned_draw_progress_v1.is_some_and(|progress| {
        matches!(state.engine.event_history.get(progress.started_at), Some(CommittedEvent::EternalOwnedDrawV1 { journal })
            if journal.start().is_some_and(|(started, _)| matches!(started.instruction, InstructionV1::Knowledge { .. })))
    }) { return Err("Knowledge owned draw choices require an observation successor".into()); }
    Ok(())
}
