//! Native ownership of the atomic Niv-Mizzet draw and counter routes.
use crate::{
    effect::{EffectOp, ExecCtx},
    event::{self, CommittedEvent as E},
    event_history_v1::EventKindV1,
    ids::{PlayerId, StackItemId},
    state::{GameState, StackItem, StackItemKind},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Native-only receipt. Public facts publish the derived count and outcome,
/// never history offsets, private stack IDs, or the drawn objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EndV1 {
    Resolved {
        first_history_index: u64,
        cards_drawn: u32,
    },
    Countered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawOutcomeV1 {
    Resolved { cards_drawn: u32 },
    Countered,
}

pub(super) struct RemovedDrawV1 {
    id: StackItemId,
    player: PlayerId,
    commanded: i32,
    original_stack_len: usize,
}

pub(super) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovedDrawV1>, String> {
    let Some(EffectOp::NivMizzetVisionaryDrawV1 {
        damage_history_index,
    }) = &item.inline_effect
    else {
        return Ok(None);
    };
    let id = item.v4.stack_item_id;
    let mut matching = state.stack.iter().filter(|row| row.v4.stack_item_id == id);
    if item.kind != StackItemKind::TriggeredAbility
        || matching.next() != Some(item)
        || matching.next().is_some()
        || ended(state).any(|(_, found, _)| found == id)
    {
        return Err("Niv draw needs its original unconsumed stack row".into());
    }
    let mut ctx = ExecCtx::no_targets(item.source, item.controller);
    ctx.stack_item_id = Some(id);
    ctx.ability_source_contract = item.v4.ability_source_contract;
    let commanded =
        event::resolve_niv_mizzet_visionary_draw_count_v1(state, &ctx, *damage_history_index)?;
    Ok(Some(RemovedDrawV1 {
        id,
        player: item.controller,
        commanded,
        original_stack_len: state.stack.len(),
    }))
}

fn require_removed(state: &GameState, owner: &RemovedDrawV1) -> Result<(), String> {
    if state.stack.len().checked_add(1) != Some(owner.original_stack_len)
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == owner.id)
        || ended(state).any(|(_, id, _)| id == owner.id)
        || state.engine.halted.is_some()
    {
        return Err("Niv draw lost its native removal".into());
    }
    Ok(())
}

fn draw_counts(history: &[E], player: PlayerId) -> Result<(i32, u32), String> {
    let mut attempted = 0i32;
    let mut drawn = 0u32;
    for event in history {
        if let E::Draw {
            player: drawer,
            object,
        } = event
        {
            if *drawer != player {
                return Err("Niv draw span contains another player's draw".into());
            }
            attempted = attempted
                .checked_add(1)
                .ok_or("Niv draw attempts overflow")?;
            if object.is_some() {
                drawn = drawn.checked_add(1).ok_or("Niv draw count overflow")?;
            }
        }
    }
    Ok((attempted, drawn))
}

pub(super) fn resolve(state: &mut GameState, owner: RemovedDrawV1) -> Result<(), String> {
    require_removed(state, &owner)?;
    let first = state.engine.event_history.len();
    let first_history_index = u64::try_from(first).map_err(|_| "Niv draw index overflow")?;
    for _ in 0..owner.commanded {
        event::propose_and_commit(state, event::ProposedEvent::draw(owner.player));
        if state.engine.halted.is_some() {
            return Err("Niv draw was interrupted before completion".into());
        }
    }
    let (attempted, cards_drawn) = draw_counts(&state.engine.event_history[first..], owner.player)?;
    if attempted != owner.commanded {
        return Err("Niv completed draw span changed its attempt count".into());
    }
    state
        .engine
        .event_history
        .push(E::NivMizzetVisionaryDrawEndedV1 {
            stack_item_id: owner.id,
            outcome: EndV1::Resolved {
                first_history_index,
                cards_drawn,
            },
        });
    Ok(())
}

pub(super) fn counter(state: &mut GameState, owner: RemovedDrawV1) -> Result<(), String> {
    require_removed(state, &owner)?;
    state
        .engine
        .event_history
        .push(E::NivMizzetVisionaryDrawEndedV1 {
            stack_item_id: owner.id,
            outcome: EndV1::Countered,
        });
    Ok(())
}

fn ended(state: &GameState) -> impl Iterator<Item = (usize, StackItemId, EndV1)> + '_ {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::NivMizzetVisionaryDrawEndedV1])
        .iter()
        .enumerate()
        .filter_map(|(at, row)| match row {
            E::NivMizzetVisionaryDrawEndedV1 {
                stack_item_id,
                outcome,
            } => Some((at, *stack_item_id, *outcome)),
            _ => None,
        })
}

/// Recapture only verified native outcomes. Earlier histories that lack an
/// ended receipt, and removals outside the counter and resolution routes,
/// carry no outcome claim.
pub(crate) fn outcomes(state: &GameState) -> Result<BTreeMap<StackItemId, DrawOutcomeV1>, String> {
    let history = &state.engine.event_history;
    let mut values = BTreeMap::new();
    let mut previous_end = None;
    for (end, id, outcome) in ended(state) {
        let mut publications = history
            .iter()
            .enumerate()
            .filter_map(|(at, row)| match row {
                E::NivMizzetVisionaryTriggerStackedV1 {
                    stack_item_id,
                    observer,
                    damage_history_index,
                } if *stack_item_id == id => Some((at, *observer, *damage_history_index)),
                _ => None,
            });
        let (publication, observer, damage) = publications
            .next()
            .ok_or("Niv outcome lost its exact publication")?;
        if publication >= end
            || publications.next().is_some()
            || state.stack.iter().any(|row| row.v4.stack_item_id == id)
            || state
                .engine
                .pending_effect
                .as_ref()
                .is_some_and(|pending| pending.resolving_item.v4.stack_item_id == id)
        {
            return Err("Niv outcome overlaps another native row".into());
        }
        let mut ctx = ExecCtx::no_targets(observer.source, observer.controller);
        ctx.stack_item_id = Some(id);
        ctx.ability_source_contract = Some(observer);
        let commanded = event::resolve_niv_mizzet_visionary_draw_count_v1(state, &ctx, damage)?;
        let value = match outcome {
            EndV1::Countered => DrawOutcomeV1::Countered,
            EndV1::Resolved {
                first_history_index,
                cards_drawn,
            } => {
                let first =
                    usize::try_from(first_history_index).map_err(|_| "Niv draw index overflow")?;
                if first <= publication
                    || first >= end
                    || previous_end.is_some_and(|at| first <= at)
                {
                    return Err(
                        "Niv draw span is out of order or overlaps an earlier outcome".into(),
                    );
                }
                if draw_counts(&history[first..end], observer.controller)?
                    != (commanded, cards_drawn)
                {
                    return Err("Niv outcome does not match its actual committed draws".into());
                }
                DrawOutcomeV1::Resolved { cards_drawn }
            }
        };
        previous_end = Some(end);
        if values.insert(id, value).is_some() {
            return Err("Niv draw ended more than once".into());
        }
    }
    Ok(values)
}
