//! Selected extra-turn scheduling. Legacy round numbering remains unchanged.
use crate::card_def::{self, TargetSpec};
use crate::effect::EffectOp;
use crate::event::CommittedEvent;
use crate::ids::{PlayerId, StackItemId};
use crate::state::{GameState, StackItem, StackItemKind, Step};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScheduledTurnV1 {
    pub item: Box<StackItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExtraTurnStateV1 {
    /// A distinct occurrence from promotion onward, separate from the round.
    pub occurrence: u32,
    pub own_turns_started: [u32; 2],
    pub previous_player: PlayerId,
    pub next_regular_player: PlayerId,
    pub current_extra: Option<StackItemId>,
    pub original_first_turn: bool,
    /// Most recently created extra turn is consumed first (CR 500.7).
    pub queued: Vec<ScheduledTurnV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV1 {
    Resolved,
    Countered,
    LeftStackWithoutResolving,
}

pub fn recipe() -> EffectOp {
    EffectOp::TakeExtraTurnAfterThisOneV1
}

fn selected(item: &StackItem, state: &GameState) -> bool {
    [
        item.v4.source_contract.map(|source| source.card_def),
        state
            .objects
            .try_get(item.source)
            .map(|object| object.card_def),
    ]
    .into_iter()
    .flatten()
    .any(|id| {
        card_def::CARD_DEFS
            .get(usize::from(id))
            .is_some_and(|definition| definition.name == "Temporal Manipulation")
    })
}

fn exact_recipe(item: &StackItem) -> bool {
    if item
        .v4
        .source_contract
        .is_none_or(|source| source.source != item.source || source.controller != item.controller)
    {
        return false;
    }
    item.v4
        .source_contract
        .and_then(|source| card_def::CARD_DEFS.get(usize::from(source.card_def)))
        .is_some_and(|definition| {
            definition.name == "Temporal Manipulation"
                && definition.target_spec == TargetSpec::None
                && (definition.spell_effect)() == Some(recipe())
        })
        && item.kind == StackItemKind::Spell
        && item.inline_effect.is_none()
        && item.targets.is_empty()
        && item.v4.target_contracts.is_empty()
        && item.mode_chosen == 0
}

/// Private ownership acquired while the exact original row is still live.
pub(crate) struct ResolutionOwnerV1 {
    item: StackItem,
}
/// Called only after real complete payment or actual virtual-copy creation.
pub(crate) fn record_publication(state: &mut GameState, id: StackItemId) {
    let Some(item) = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == id)
        .cloned()
    else {
        return;
    };
    if selected(&item, state) {
        state
            .engine
            .event_history
            .push(CommittedEvent::ExtraTurnOwnerPublishedV1 {
                item: Box::new(item),
            });
    }
}
fn publication(state: &GameState, item: &StackItem) -> Result<usize, String> {
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            if let CommittedEvent::ExtraTurnOwnerPublishedV1 { item: original } = event {
                (original.v4.stack_item_id == item.v4.stack_item_id).then_some((i, original))
            } else {
                None
            }
        })
        .collect();
    match rows.as_slice() {
        [(i, original)] if original.as_ref() == item && exact_recipe(item) => Ok(*i),
        _ => Err("extra turn lost its independent original paid/copy publication".into()),
    }
}
pub(crate) fn acquire_counter(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<ResolutionOwnerV1>, String> {
    acquire_live(state, item, false)
}
pub(crate) fn record_countered(state: &mut GameState, owner: Option<ResolutionOwnerV1>) {
    if let Some(owner) = owner {
        state
            .engine
            .event_history
            .push(CommittedEvent::ExtraTurnOwnerRemovedV1 {
                item: Box::new(owner.item),
                outcome: RemovalV1::Countered,
            });
    }
}
pub(crate) fn record_other_departure(state: &mut GameState, owner: Option<ResolutionOwnerV1>) {
    if let Some(owner) = owner {
        state
            .engine
            .event_history
            .push(CommittedEvent::ExtraTurnOwnerRemovedV1 {
                item: Box::new(owner.item),
                outcome: RemovalV1::LeftStackWithoutResolving,
            });
    }
}

pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<ResolutionOwnerV1>, String> {
    acquire_live(state, item, true)
}
fn acquire_live(
    state: &GameState,
    item: &StackItem,
    top: bool,
) -> Result<Option<ResolutionOwnerV1>, String> {
    if !selected(item, state) {
        return Ok(None);
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    validate(state)?;
    publication(state, item)?;
    if !exact_recipe(item)
        || state
            .engine
            .pending_cast
            .as_ref()
            .is_some_and(|pending| pending.spell == item.source)
        || (top && state.stack.last() != Some(item))
        || state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
        || state.engine.event_history.iter().any(|event| {
            matches!(event,
            CommittedEvent::ExtraTurnScheduledV1 { item: prior }
                if prior.v4.stack_item_id == item.v4.stack_item_id)
        })
    {
        return Err("extra turn lost its unique finalized printed spell owner".into());
    }
    Ok(Some(ResolutionOwnerV1 { item: item.clone() }))
}

pub(crate) fn schedule_owned(
    state: &mut GameState,
    owner: ResolutionOwnerV1,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|item| item.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("extra turn resolution did not remove its owned original row".into());
    }
    // The unique row was authenticated before removal. Publish its actual
    // private removed-item operation atomically with the scheduler effect.
    state
        .engine
        .event_history
        .push(CommittedEvent::ExtraTurnOwnerRemovedV1 {
            item: Box::new(owner.item.clone()),
            outcome: RemovalV1::Resolved,
        });
    if state.extra_turn_v1.is_none() {
        let initial = ExtraTurnStateV1 {
            occurrence: state.turn,
            own_turns_started: initial_own_turns(
                state.turn,
                state.active_player,
                state.starting_player,
            ),
            previous_player: state.active_player.opponent(),
            next_regular_player: state.active_player.opponent(),
            current_extra: None,
            original_first_turn: state.turn == 1 && state.active_player == state.starting_player,
            queued: vec![],
        };
        state
            .engine
            .event_history
            .push(CommittedEvent::ExtraTurnStartedV1 {
                initial: initial.clone(),
                round: state.turn,
                active_player: state.active_player,
            });
        state.extra_turn_v1 = Some(initial);
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::ExtraTurnScheduledV1 {
            item: Box::new(owner.item.clone()),
        });
    state
        .extra_turn_v1
        .as_mut()
        .expect("installed scheduler")
        .queued
        .push(ScheduledTurnV1 {
            item: Box::new(owner.item),
        });
    Ok(())
}

fn next(
    previous: &ExtraTurnStateV1,
    round: u32,
    starting: PlayerId,
    current: PlayerId,
) -> Result<(ExtraTurnStateV1, u32, PlayerId), String> {
    let mut after = previous.clone();
    after.occurrence = after
        .occurrence
        .checked_add(1)
        .ok_or("extra-turn occurrence exceeds u32")?;
    after.original_first_turn = false;
    after.previous_player = current;
    let (round, player) = if let Some(scheduled) = after.queued.pop() {
        after.current_extra = Some(scheduled.item.v4.stack_item_id);
        (round, scheduled.item.controller)
    } else {
        let player = after.next_regular_player;
        after.next_regular_player = player.opponent();
        after.current_extra = None;
        (
            if player == starting {
                round.checked_add(1).ok_or("round exceeds u32")?
            } else {
                round
            },
            player,
        )
    };
    after.own_turns_started[player.index()] = after.own_turns_started[player.index()]
        .checked_add(1)
        .ok_or("own turn count exceeds u32")?;
    Ok((after, round, player))
}

fn initial_own_turns(round: u32, active: PlayerId, starting: PlayerId) -> [u32; 2] {
    let mut counts = [round; 2];
    if active == starting {
        counts[active.opponent().index()] = round.saturating_sub(1);
    }
    counts
}

/// Called only at the real completed Cleanup boundary, before Untap work.
pub(crate) fn begin_next_turn(state: &mut GameState) -> Result<bool, String> {
    let Some(previous) = state.extra_turn_v1.as_ref() else {
        return Ok(false);
    };
    if state.step != Step::Cleanup
        || !state.stack.is_empty()
        || state.engine.pending_effect.is_some()
        || state.engine.pending_discard.is_some()
    {
        return Err("extra turn transition requires completed actual Cleanup".into());
    }
    validate(state)?;
    let (after, round, player) = next(
        previous,
        state.turn,
        state.starting_player,
        state.active_player,
    )?;
    state
        .engine
        .event_history
        .push(CommittedEvent::ExtraTurnBeganV1 {
            previous: previous.clone(),
            after: after.clone(),
            round,
            active_player: player,
        });
    state.extra_turn_v1 = Some(after);
    state.turn = round;
    state.active_player = player;
    Ok(true)
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let mut derived: Option<ExtraTurnStateV1> = None;
    let mut round = state.turn;
    let mut player = state.active_player;
    let mut scheduled_ids = Vec::new();
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::ExtraTurnStartedV1 { .. }
                | CommittedEvent::ExtraTurnScheduledV1 { .. }
                | CommittedEvent::ExtraTurnBeganV1 { .. }
                | CommittedEvent::ExtraTurnOwnerPublishedV1 { .. }
                | CommittedEvent::ExtraTurnOwnerRemovedV1 { .. }
                | CommittedEvent::ExtraTurnVirtualCeasedV1 { .. }
        )
    }) {
        return Err("extra turn history authority cannot enter the public event log".into());
    }
    for (history_index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::ExtraTurnStartedV1 {
                initial,
                round: original_round,
                active_player,
            } => {
                if derived.is_some()
                    || !initial.queued.is_empty()
                    || initial.current_extra.is_some()
                    || initial.occurrence != *original_round
                    || *original_round == 0
                    || initial.own_turns_started
                        != initial_own_turns(*original_round, *active_player, state.starting_player)
                    || initial.previous_player != active_player.opponent()
                    || initial.next_regular_player != active_player.opponent()
                    || initial.original_first_turn
                        != (*original_round == 1 && *active_player == state.starting_player)
                {
                    return Err("extra turn initial authority is malformed or duplicated".into());
                }
                derived = Some(initial.clone());
                round = *original_round;
                player = *active_player;
            }
            CommittedEvent::ExtraTurnScheduledV1 { item } => {
                let published = publication(state, item)?;
                let operations: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, event)| {
                        if let CommittedEvent::ExtraTurnOwnerRemovedV1 {
                            item: original,
                            outcome,
                        } = event
                        {
                            (original.v4.stack_item_id == item.v4.stack_item_id)
                                .then_some((i, original, outcome))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !matches!(operations.as_slice(),[(i,original,RemovalV1::Resolved)]
                    if published<*i && *i<history_index && original.as_ref()==item.as_ref())
                {
                    return Err("extra turn has no unique actual owned resolution".into());
                }
                if !exact_recipe(item)
                    || item.v4.stack_item_id.0 == 0
                    || scheduled_ids.contains(&item.v4.stack_item_id)
                    || state
                        .stack
                        .iter()
                        .any(|live| live.v4.stack_item_id == item.v4.stack_item_id)
                {
                    return Err(
                        "extra turn scheduling substituted or replayed its printed owner".into(),
                    );
                }
                scheduled_ids.push(item.v4.stack_item_id);
                derived
                    .as_mut()
                    .ok_or("extra turn schedule has no initial authority")?
                    .queued
                    .push(ScheduledTurnV1 { item: item.clone() });
            }
            CommittedEvent::ExtraTurnBeganV1 {
                previous,
                after,
                round: recorded_round,
                active_player,
            } => {
                if derived.as_ref() != Some(previous)
                    || next(previous, round, state.starting_player, player)?
                        != (after.clone(), *recorded_round, *active_player)
                {
                    return Err(
                        "extra turn transition changed queue order or regular-turn continuation"
                            .into(),
                    );
                }
                derived = Some(after.clone());
                round = *recorded_round;
                player = *active_player;
            }
            _ => {}
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::ExtraTurnOwnerPublishedV1 { item } => {
                if publication(state, item)? != index {
                    return Err("duplicate extra turn publication".into());
                }
                let removals: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, event)| {
                        if let CommittedEvent::ExtraTurnOwnerRemovedV1 {
                            item: original,
                            outcome,
                        } = event
                        {
                            (original.v4.stack_item_id == item.v4.stack_item_id)
                                .then_some((i, original, outcome))
                        } else {
                            None
                        }
                    })
                    .collect();
                let live: Vec<_> = state
                    .stack
                    .iter()
                    .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                    .collect();
                match removals.as_slice() {
                    [] if live.as_slice() == [item.as_ref()] => {}
                    [(i, original, outcome)]
                        if index < *i && original.as_ref() == item.as_ref() && live.is_empty() =>
                    {
                        let schedules=state.engine.event_history.iter().filter(|event|matches!(event,
                            CommittedEvent::ExtraTurnScheduledV1 {item:scheduled} if scheduled.v4.stack_item_id==item.v4.stack_item_id)).count();
                        if schedules != usize::from(**outcome == RemovalV1::Resolved) {
                            return Err(
                                "extra turn completion dropped or replayed its actual schedule"
                                    .into(),
                            );
                        }
                        let scheduled_at = state.engine.event_history.iter().position(|event| matches!(event,
                            CommittedEvent::ExtraTurnScheduledV1 {item:scheduled} if scheduled.v4.stack_item_id==item.v4.stack_item_id));
                        // A subsequent publication of this same physical card bounds
                        // the departure occurrence to the original spell incarnation.
                        let end = state.engine.event_history.iter().enumerate().find_map(|(at,event)|
                            (at > *i && matches!(event,CommittedEvent::ExtraTurnOwnerPublishedV1 {item:later}
                                if later.source==item.source)).then_some(at)).unwrap_or(state.engine.event_history.len());
                        let departures = state.engine.event_history.iter().enumerate().filter(|(at,event)| {
                            let ordered=match outcome {
                                RemovalV1::Resolved=>scheduled_at.is_some_and(|scheduled| *at>scheduled && *at<end),
                                RemovalV1::Countered if !item.is_copy=>*at>*i && *at<end,
                                _=>*at>index && *at<*i,
                            };
                            ordered && if item.is_copy {
                                matches!(event,CommittedEvent::ExtraTurnVirtualCeasedV1 {source,stack_item_id}
                                    if Some(*source)==item.v4.source_contract && *stack_item_id==item.v4.stack_item_id)
                            } else {
                                matches!(event,CommittedEvent::ZoneChange {object,from:crate::state::Zone::Stack,controller_before,..}
                                    if *object==item.source && *controller_before==item.controller)
                            }
                        }).count();
                        if departures != 1 {
                            return Err("removed Temporal spell lacks its unique independent actual Stack departure".into());
                        }
                    }
                    _ => return Err("extra turn paid owner was erased, changed or replayed".into()),
                }
            }
            CommittedEvent::ExtraTurnOwnerRemovedV1 { item, .. } => {
                publication(state, item)?;
            }
            CommittedEvent::ExtraTurnVirtualCeasedV1 {
                source,
                stack_item_id,
            } => {
                let owners: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(at, event)| {
                        if let CommittedEvent::ExtraTurnOwnerPublishedV1 { item } = event {
                            (item.v4.stack_item_id == *stack_item_id).then_some((at, item))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !matches!(owners.as_slice(),[(at,item)] if *at<index && item.is_copy && item.v4.source_contract==Some(*source))
                    || state.engine.event_history.iter().filter(|event|matches!(event,CommittedEvent::ExtraTurnVirtualCeasedV1 {stack_item_id:id,..} if id==stack_item_id)).count()!=1
                    || state.stack.iter().any(|item|item.v4.stack_item_id==*stack_item_id)
                {
                    return Err(
                        "virtual Temporal departure has no independent original copy publication"
                            .into(),
                    );
                }
            }
            _ => {}
        }
    }
    if derived != state.extra_turn_v1
        || (derived.is_some() && (round != state.turn || player != state.active_player))
    {
        return Err("extra turn queue or current turn differs from committed scheduling".into());
    }
    for item in &state.stack {
        if selected(item, state) {
            // Pending casts have their genuine placeholder contract; finalized
            // items use the normal complete source/method/target validator.
            if let Some(pending) = state
                .engine
                .pending_cast
                .as_ref()
                .filter(|pending| pending.spell == item.source)
            {
                crate::engine::validate_pending_cast(state, pending)?;
            } else {
                crate::engine::validated_stack_item_target_spec(item, state)?;
                publication(state, item)?;
                if !exact_recipe(item) {
                    return Err("Temporal spell root metadata changed".into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
