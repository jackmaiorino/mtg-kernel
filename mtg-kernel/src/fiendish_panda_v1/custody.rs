//! Panda-specific ownership of mandatory roots and their actual operations.
use super::*;
use crate::state::{GameObject, StackItem, StackItemKind, StackStateV4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV1 {
    Published {
        capture: u64,
        item: Box<StackItem>,
        world: WorldV1,
    },
    Unavailable {
        capture: u64,
        pending: Box<PendingTrigger>,
        input: u64,
    },
    DropInput {
        capture: u64,
        pending: Box<PendingTrigger>,
        world: WorldV1,
    },
    RemovalInput {
        publication: u64,
        item: Box<StackItem>,
        kind: RemovalV1,
        world: WorldV1,
    },
    Removed {
        publication: u64,
        item: Box<StackItem>,
        kind: RemovalV1,
        input: u64,
    },
    OperationInput {
        removal: u64,
        context: Box<ExecCtx>,
        before: Option<Box<GameObject>>,
        eligible: bool,
        world: WorldV1,
    },
    Operation {
        input: u64,
        after: Option<Box<GameObject>>,
    },
    Closed {
        removal: u64,
        operation: Option<u64>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV1 {
    Resolved,
    Countered,
    NoLegalTarget,
}

/// The input world excludes history so retained inputs never recursively copy
/// older journal payloads. Replay borrows the original immutable history prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldV1 {
    history_count: u64,
    bytes: Vec<u8>,
    linked_borrow: crate::linked_aura_operation_v1::PandaReplayBorrowV1,
}
impl WorldV1 {
    fn capture(state: &GameState) -> Result<Self, String> {
        let mut original = state.clone();
        let history_count = u64::try_from(original.engine.event_history.len())
            .map_err(|_| "Panda input history exceeds u64")?;
        original.engine.event_history.clear();
        Ok(Self {
            history_count,
            bytes: serde_json::to_vec(&original).map_err(|e| e.to_string())?,
            linked_borrow: crate::linked_aura_operation_v1::capture_panda_replay_borrow_v1(state),
        })
    }
    fn restore(&self, state: &GameState, before: u64) -> Result<GameState, String> {
        let count =
            usize::try_from(self.history_count).map_err(|_| "Panda input prefix exceeds usize")?;
        if self.history_count > before || count > state.engine.event_history.len() {
            return Err("Panda original input prefix moved forward".into());
        }
        let mut original: GameState =
            serde_json::from_slice(&self.bytes).map_err(|e| e.to_string())?;
        if !original.engine.event_history.is_empty() {
            return Err("Panda private world contains recursive history".into());
        }
        original.engine.event_history = state.engine.event_history[..count].to_vec();
        Ok(original)
    }
}

fn append(state: &mut GameState, journal: JournalV1) -> Result<u64, String> {
    let index =
        u64::try_from(state.engine.event_history.len()).map_err(|_| "Panda history exceeds u64")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::FiendishPandaJournalV1 {
            journal: Box::new(journal),
        });
    Ok(index)
}
fn journal(state: &GameState, index: u64) -> Result<&JournalV1, String> {
    match state
        .engine
        .event_history
        .get(usize::try_from(index).map_err(|_| "Panda index exceeds usize")?)
    {
        Some(CommittedEvent::FiendishPandaJournalV1 { journal }) => Ok(journal),
        _ => Err("Panda original custody input absent".into()),
    }
}
fn binding(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 {
            capture_history_index,
        }
        | EffectOp::FiendishPandaReturnFromCaptureV1 {
            capture_history_index,
            ..
        } => Some(*capture_history_index),
        _ => None,
    }
}
fn item_capture(item: &StackItem) -> Option<u64> {
    item.inline_effect.as_ref().and_then(binding)
}
fn captures(state: &GameState) -> Vec<u64> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            matches!(
                e,
                CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
                    | CommittedEvent::FiendishPandaDeathCaptureV1 { .. }
            )
            .then_some(at as u64)
        })
        .collect()
}
fn canonical_context(item: &StackItem) -> ExecCtx {
    let mut context = ExecCtx::no_targets(item.source, item.controller);
    context.stack_item_id = Some(item.v4.stack_item_id);
    context.ability_source_contract = item.v4.ability_source_contract;
    context.targets = item.targets.clone();
    context.target_contracts = item.v4.target_contracts.clone();
    context
}
fn validate_item(state: &GameState, item: &StackItem, capture: u64) -> Result<(), String> {
    let (source, effect, spec) = root_at(
        state,
        usize::try_from(capture).map_err(|_| "Panda capture exceeds usize")?,
    )?;
    let expected = StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: source.source,
        controller: source.controller,
        targets: item.targets.clone(),
        inline_effect: Some(effect),
        is_copy: false,
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: item.v4.stack_item_id,
            ability_source_contract: Some(source),
            target_spec: (spec != TargetSpec::None).then_some(spec),
            target_contracts: item.v4.target_contracts.clone(),
            ..Default::default()
        },
    };
    if *item != expected
        || item.v4.stack_item_id == Default::default()
        || item.targets.len() != usize::from(spec != TargetSpec::None)
        || item.targets.len() != item.v4.target_contracts.len()
    {
        return Err("Panda publication changed its complete original root".into());
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    Ok(())
}
fn validate_pending(
    state: &GameState,
    pending: &PendingTrigger,
    capture: u64,
) -> Result<(), String> {
    let (source, effect, spec) = root_at(
        state,
        usize::try_from(capture).map_err(|_| "Panda capture exceeds usize")?,
    )?;
    let expected = PendingTrigger {
        source: source.source,
        controller: source.controller,
        source_contract: Some(source),
        effect,
        target_spec: spec,
        targets: pending.targets.clone(),
        target_contracts: pending.target_contracts.clone(),
        placement_ordered: pending.placement_ordered,
        is_madness_offer: false,
        kicked: false,
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: Vec::new(),
    };
    if *pending != expected
        || pending.targets.len() > usize::from(spec != TargetSpec::None)
        || pending.targets.len() != pending.target_contracts.len()
    {
        return Err("Panda pending root changed its original capture/source/targets".into());
    }
    crate::engine::validate_pending_trigger_identity(state, pending)
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = item_capture(item) else {
        return Ok(());
    };
    validate_item(state, item, capture)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|row| row == item)
    {
        return Err("Panda publication did not acquire the actual complete unique row".into());
    }
    if state.engine.event_history.iter().any(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal }
        if matches!(journal.as_ref(), JournalV1::Published { capture: old, .. } | JournalV1::Unavailable { capture: old, .. } if *old == capture)))
    { return Err("Panda captured occurrence published or dropped twice".into()); }
    let world = WorldV1::capture(state)?;
    append(
        state,
        JournalV1::Published {
            capture,
            item: Box::new(item.clone()),
            world,
        },
    )?;
    Ok(())
}
fn validate_publication(state: &GameState, publication: u64) -> Result<(), String> {
    let JournalV1::Published {
        capture,
        item,
        world,
    } = journal(state, publication)?
    else {
        return Err("Panda original publication input absent".into());
    };
    if world.history_count != publication {
        return Err("Panda publication input lost its exact original boundary".into());
    }
    let original = world.restore(state, publication)?;
    if original
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !original.stack.iter().any(|row| row == item.as_ref())
    {
        return Err("Panda publication differs from its complete original input row".into());
    }
    // The original public graveyard target is checked in its actual publication
    // world. A later hidden incarnation cannot choose the public validator's
    // result by changing its current definition.
    validate_item(&original, item, *capture)
}
pub(crate) fn unavailable(state: &mut GameState, pending: &PendingTrigger) -> Result<(), String> {
    let Some(capture) = binding(&pending.effect) else {
        return Ok(());
    };
    validate_pending(state, pending, capture)?;
    if pending.target_spec == TargetSpec::None
        || crate::engine::pending_trigger_targets_can_complete(pending, state)
    {
        return Err("Panda targetless or completable mandatory root cannot be dropped".into());
    }
    let world = WorldV1::capture(state)?;
    let input = append(
        state,
        JournalV1::DropInput {
            capture,
            pending: Box::new(pending.clone()),
            world,
        },
    )?;
    append(
        state,
        JournalV1::Unavailable {
            capture,
            pending: Box::new(pending.clone()),
            input,
        },
    )?;
    Ok(())
}

/// This token can only be acquired from the full unique actual stack row.
pub(crate) struct OwnerV1 {
    original: StackItem,
    publication: u64,
    world: WorldV1,
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV1>, String> {
    let Some(capture) = item_capture(item) else {
        return Ok(None);
    };
    validate(state)?;
    if state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|i| i == item)
    {
        return Err("Panda owner did not acquire its actual full unique row".into());
    }
    let publications = state.engine.event_history.iter().enumerate().filter_map(|(at, e)| match e {
        CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Published { capture: old, item: original, .. } if *old == capture && original.as_ref() == item) => Some(at as u64),
        _ => None,
    }).collect::<Vec<_>>();
    let [publication] = publications.as_slice() else {
        return Err("Panda original full publication missing or duplicated".into());
    };
    Ok(Some(OwnerV1 {
        original: item.clone(),
        publication: *publication,
        world: WorldV1::capture(state)?,
    }))
}
fn record_removal(
    state: &mut GameState,
    owner: &OwnerV1,
    item: &StackItem,
    kind: RemovalV1,
) -> Result<u64, String> {
    if owner.original != *item || state.stack.iter().any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal }
            if matches!(journal.as_ref(), JournalV1::Removed { publication, .. } if *publication == owner.publication)))
    { return Err("Panda genuine removal lost its private loan or was replayed".into()); }
    let input = append(
        state,
        JournalV1::RemovalInput {
            publication: owner.publication,
            item: Box::new(item.clone()),
            kind,
            world: owner.world.clone(),
        },
    )?;
    append(
        state,
        JournalV1::Removed {
            publication: owner.publication,
            item: Box::new(item.clone()),
            kind,
            input,
        },
    )
}
pub(crate) fn counter_removed(
    state: &mut GameState,
    owner: Option<&OwnerV1>,
    item: &StackItem,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let removal = record_removal(state, owner, item, RemovalV1::Countered)?;
    append(
        state,
        JournalV1::Closed {
            removal,
            operation: None,
        },
    )?;
    Ok(())
}
pub(crate) fn resolve_owned(
    state: &mut GameState,
    owner: &OwnerV1,
    item: &StackItem,
    context: &ExecCtx,
    targets_legal: bool,
) -> Result<(), String> {
    if *context != canonical_context(&owner.original) {
        return Err("Panda execution context changed its original full publication".into());
    }
    let removal = record_removal(
        state,
        owner,
        item,
        if targets_legal {
            RemovalV1::Resolved
        } else {
            RemovalV1::NoLegalTarget
        },
    )?;
    if !targets_legal {
        if item.targets.is_empty() {
            return Err("Untargeted Panda counter cannot fizzle".into());
        }
        append(
            state,
            JournalV1::Closed {
                removal,
                operation: None,
            },
        )?;
        return Ok(());
    }
    let effect = item
        .inline_effect
        .as_ref()
        .ok_or("Panda owner lost bound program")?;
    let object = match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 { .. } => item.source,
        EffectOp::FiendishPandaReturnFromCaptureV1 { .. } => match item.targets.as_slice() {
            [Target::Object(object)] => *object,
            _ => return Err("Panda return lost its unique graveyard target".into()),
        },
        _ => return Err("Panda private loan contains a foreign program".into()),
    };
    let before = state.objects.try_get(object).cloned().map(Box::new);
    let eligible = match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 { .. } => {
            let source = item
                .v4
                .ability_source_contract
                .ok_or("Panda counter source absent")?;
            before.as_ref().is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.owner == source.owner
                    && o.zone_change_count == source.zone_change_count
            })
        }
        _ => true, // The real engine's complete target recheck precedes this private loan.
    };
    let world = WorldV1::capture(state)?;
    let input = append(
        state,
        JournalV1::OperationInput {
            removal,
            context: Box::new(context.clone()),
            before,
            eligible,
            world,
        },
    )?;
    super::execute_core(state, context, effect)?;
    if state.engine.halted.is_some() {
        return Err("Panda actual operation halted".into());
    }
    let after = state.objects.try_get(object).cloned().map(Box::new);
    let operation = append(state, JournalV1::Operation { input, after })?;
    append(
        state,
        JournalV1::Closed {
            removal,
            operation: Some(operation),
        },
    )?;
    validate_operation(state, operation)?;
    Ok(())
}
fn validate_operation(state: &GameState, operation: u64) -> Result<(), String> {
    let JournalV1::Operation { input, after } = journal(state, operation)? else {
        return Err("Panda completion lost its actual operation".into());
    };
    let JournalV1::OperationInput {
        removal,
        context,
        before,
        eligible,
        world,
    } = journal(state, *input)?
    else {
        return Err("Panda operation lost independent original input".into());
    };
    let JournalV1::Removed {
        item,
        kind: RemovalV1::Resolved,
        ..
    } = journal(state, *removal)?
    else {
        return Err("Panda operation changed genuine resolution removal".into());
    };
    if *input <= *removal || operation <= *input || context.as_ref() != &canonical_context(item) {
        return Err("Panda actual operation changed its context or history order".into());
    }
    let events = &state.engine.event_history[*input as usize + 1..operation as usize];
    if world.history_count != *input {
        return Err("Panda operation input lost its exact world/history boundary".into());
    }
    let mut replay = world.restore(state, *input)?;
    let object = match item
        .inline_effect
        .as_ref()
        .ok_or("Panda replay lost actual program")?
    {
        EffectOp::FiendishPandaCounterFromCaptureV1 { .. } => item.source,
        EffectOp::FiendishPandaReturnFromCaptureV1 { .. } => match item.targets.as_slice() {
            [Target::Object(object)] => *object,
            _ => return Err("Panda replay lost target incarnation".into()),
        },
        _ => return Err("Panda replay borrowed a foreign effect".into()),
    };
    if replay.objects.try_get(object) != before.as_deref() {
        return Err("Panda original object snapshot changed its independent input world".into());
    }
    if matches!(
        item.inline_effect,
        Some(EffectOp::FiendishPandaReturnFromCaptureV1 { .. })
    ) && !context.target_incarnation_matches(0, &replay)
    {
        return Err("Panda return input lost the original published target incarnation".into());
    }
    replay
        .engine
        .event_history
        .push(state.engine.event_history[*input as usize].clone());
    crate::linked_aura_operation_v1::with_panda_replay_borrow_v1(
        &mut replay,
        item,
        &world.linked_borrow,
        |replay| super::execute_core(replay, context, item.inline_effect.as_ref().unwrap()),
    )?;
    if replay.engine.halted.is_some()
        || replay.objects.try_get(object) != after.as_deref()
        || &replay.engine.event_history[*input as usize + 1..] != events
    {
        return Err(
            "Panda complete operation output differs from its genuine original-input replay".into(),
        );
    }
    match item
        .inline_effect
        .as_ref()
        .ok_or("Panda operation lost root")?
    {
        EffectOp::FiendishPandaCounterFromCaptureV1 { .. } => {
            let source = item
                .v4
                .ability_source_contract
                .ok_or("Panda operation lost source")?;
            let actual_eligible = before.as_ref().is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.owner == source.owner
                    && o.zone_change_count == source.zone_change_count
            });
            if *eligible != actual_eligible {
                return Err("Panda source-presence input changed".into());
            }
            let counters = events
                .iter()
                .filter_map(|e| match e {
                    CommittedEvent::PlusOneCountersAdded {
                        object,
                        zone_change_count,
                        player,
                        count,
                    } if *object == item.source
                        && *zone_change_count == source.zone_change_count
                        && *player == source.controller =>
                    {
                        Some(*count)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if *eligible {
                let ([count], Some(before), Some(after)) =
                    (counters.as_slice(), before.as_ref(), after.as_ref())
                else {
                    return Err("Panda counter lost its actual committed placement".into());
                };
                let mut expected = before.as_ref().clone();
                expected.counters.plus1_plus1 = expected
                    .counters
                    .plus1_plus1
                    .checked_add(*count)
                    .ok_or("Panda counter overflow")?;
                // The printed instruction is one counter; replacements may
                // change the actual placement. Exact original-world replay
                // above authenticates that amount, not a hardcoded final one.
                if *count <= 0 || expected != **after {
                    return Err("Panda actual counter-placement output changed".into());
                }
            } else if !counters.is_empty() || before != after {
                return Err("Absent original Panda incarnation received counters".into());
            }
        }
        EffectOp::FiendishPandaReturnFromCaptureV1 { .. } => {
            if !*eligible {
                return Err("Panda legal return was relabeled absent".into());
            }
        }
        _ => return Err("Foreign Panda operation".into()),
    }
    Ok(())
}

fn validate_drop(state: &GameState, at: u64) -> Result<(), String> {
    let JournalV1::Unavailable {
        capture,
        pending,
        input,
    } = journal(state, at)?
    else {
        return Err("Panda unavailable phase absent".into());
    };
    let JournalV1::DropInput {
        capture: original,
        pending: original_pending,
        world,
    } = journal(state, *input)?
    else {
        return Err("Panda unavailable lost its independent original placement input".into());
    };
    if at != input + 1
        || original != capture
        || original_pending != pending
        || world.history_count != *input
    {
        return Err("Panda unavailable changed genuine placement input".into());
    }
    let replay = world.restore(state, *input)?;
    validate_pending(&replay, pending, *capture)?;
    if pending.target_spec == TargetSpec::None
        || crate::engine::pending_trigger_targets_can_complete(pending, &replay)
    {
        return Err("Panda unavailable relabeled a genuinely completable mandatory root".into());
    }
    Ok(())
}
fn validate_removal(state: &GameState, at: u64) -> Result<(), String> {
    let JournalV1::Removed {
        publication,
        item,
        kind,
        input,
    } = journal(state, at)?
    else {
        return Err("Panda removal absent".into());
    };
    let JournalV1::RemovalInput {
        publication: original,
        item: original_item,
        kind: original_kind,
        world,
    } = journal(state, *input)?
    else {
        return Err("Panda removal lost independent original live input".into());
    };
    if at != input + 1 || original != publication || original_item != item || original_kind != kind
    {
        return Err("Panda actual removal changed its original phase/item/kind".into());
    }
    let replay = world.restore(state, *input)?;
    let JournalV1::Published {
        item: published, ..
    } = journal(&replay, *publication)?
    else {
        return Err("Panda original removal world lost publication".into());
    };
    if published != item
        || replay
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
        || !replay.stack.iter().any(|i| i == item.as_ref())
    {
        return Err("Panda removal input lost full original live owner".into());
    }
    let legal = crate::engine::stack_targets_still_legal(item, &replay)?;
    if *kind == RemovalV1::Resolved && !legal || *kind == RemovalV1::NoLegalTarget && legal {
        return Err("Panda resolution/fizzle changed original complete target legality".into());
    }
    Ok(())
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { .. }))
    {
        return Err("Private Panda custody entered mutable trigger log".into());
    }
    let waiting = state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        );
    let captures = captures(state);
    // Preserve predecessor paths without allocating a pending-owner vector.
    // This checks retained root shape, never opposing hidden card identities.
    if captures.is_empty()
        && !state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { .. }))
        && !waiting.clone().any(|p| binding(&p.effect).is_some())
        && !state.stack.iter().any(|item| item_capture(item).is_some())
    {
        return Ok(());
    }
    let waiting = waiting.collect::<Vec<_>>();
    for capture in &captures {
        root_at(state, *capture as usize)?;
        let collection = state.engine.event_history.iter().enumerate().filter_map(|(at, e)| matches!(e,
            CommittedEvent::FiendishPandaCaptureCollectedV1 { capture_history_index } if capture_history_index == capture).then_some(at as u64)).collect::<Vec<_>>();
        let fresh = state
            .engine
            .event_log
            .iter()
            .filter(|e| **e == state.engine.event_history[*capture as usize])
            .count();
        let held = waiting
            .iter()
            .filter(|p| binding(&p.effect) == Some(*capture))
            .collect::<Vec<_>>();
        let finals = state.engine.event_history.iter().enumerate().filter_map(|(at, e)| match e {
            CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Published { capture: old, .. } | JournalV1::Unavailable { capture: old, .. } if old == capture) => Some((at as u64, journal.as_ref())),
            _ => None,
        }).collect::<Vec<_>>();
        if collection.is_empty() {
            if fresh != 1 || !held.is_empty() || !finals.is_empty() {
                return Err("Panda fresh mandatory capture lost its collection owner".into());
            }
            continue;
        }
        if collection.len() != 1 || collection[0] <= *capture || fresh != 0 {
            return Err("Panda capture collection erased, duplicated or replayed".into());
        }
        for p in &held {
            validate_pending(state, p, *capture)?;
        }
        if finals.is_empty() && held.len() == 1 {
            continue;
        }
        let [(at, final_phase)] = finals.as_slice() else {
            return Err("Panda collected root lost its unique mandatory owner".into());
        };
        if !held.is_empty() || *at <= collection[0] {
            return Err("Panda publication/drop retained or preceded its pending owner".into());
        }
        match final_phase {
            JournalV1::Unavailable { pending, .. } => {
                validate_pending(state, pending, *capture)?;
                if pending.target_spec == TargetSpec::None {
                    return Err("Panda life counter falsely marked unavailable".into());
                }
                validate_drop(state, *at)?;
            }
            JournalV1::Published { item, .. } => {
                validate_publication(state, *at)?;
                let live = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
                    .collect::<Vec<_>>();
                let publication_at = *at;
                let removals = state.engine.event_history.iter().enumerate().filter_map(|(at, e)| match e {
                    CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Removed { publication, .. } if *publication == publication_at) => Some((at as u64, journal.as_ref())),
                    _ => None,
                }).collect::<Vec<_>>();
                if removals.is_empty() {
                    if live.as_slice() != [item.as_ref()] {
                        return Err(
                            "Panda whole full stack publication disappeared or changed".into()
                        );
                    }
                } else {
                    let [(
                        removed_at,
                        JournalV1::Removed {
                            item: removed,
                            kind,
                            ..
                        },
                    )] = removals.as_slice()
                    else {
                        return Err("Panda actual removal duplicated".into());
                    };
                    if *removed_at <= *at || removed != item || !live.is_empty() {
                        return Err("Panda removed root changed or was reintroduced".into());
                    }
                    validate_removal(state, *removed_at)?;
                    let closed = state.engine.event_history.iter().enumerate().filter_map(|(at, e)| match e {
                        CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Closed { removal, .. } if removal == removed_at) => Some((at as u64, journal.as_ref())),
                        _ => None,
                    }).collect::<Vec<_>>();
                    let [(closed_at, JournalV1::Closed { operation, .. })] = closed.as_slice()
                    else {
                        return Err(
                            "Panda genuine removal erased its required operation/completion".into(),
                        );
                    };
                    match kind {
                        RemovalV1::Resolved => {
                            let operation =
                                operation.ok_or("Panda resolved root lost mandatory operation")?;
                            if operation + 1 != *closed_at {
                                return Err("Panda closure separated from actual operation".into());
                            }
                            let JournalV1::Operation { input, .. } = journal(state, operation)?
                            else {
                                return Err("Panda closure refers to a foreign operation".into());
                            };
                            if !matches!(journal(state, *input)?, JournalV1::OperationInput { removal, .. } if removal == removed_at)
                            {
                                return Err(
                                    "Panda closure borrowed another occurrence's operation".into(),
                                );
                            }
                            validate_operation(state, operation)?;
                        }
                        RemovalV1::Countered | RemovalV1::NoLegalTarget => {
                            if operation.is_some()
                                || *closed_at != removed_at + 1
                                || *kind == RemovalV1::NoLegalTarget && item.targets.is_empty()
                            {
                                return Err(
                                    "Panda counter/fizzle closure changed genuine removal".into()
                                );
                            }
                        }
                    }
                }
            }
            _ => unreachable!("filtered Panda final phase"),
        }
    }
    for pending in waiting {
        if binding(&pending.effect).is_some_and(|capture| !captures.contains(&capture)) {
            return Err("Panda pending owner names a foreign occurrence".into());
        }
    }
    for item in &state.stack {
        if let Some(capture) = item_capture(item) {
            let publications = state
                .engine
                .event_history
                .iter()
                .filter_map(|e| match e {
                    CommittedEvent::FiendishPandaJournalV1 { journal } => match journal.as_ref() {
                        JournalV1::Published {
                            capture: original,
                            item,
                            ..
                        } if *original == capture => Some(item.as_ref()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !captures.contains(&capture)
                || publications.as_slice() != [item]
                || state
                    .stack
                    .iter()
                    .filter(|i| item_capture(i) == Some(capture))
                    .count()
                    != 1
            {
                return Err(
                    "Panda live owner changed, duplicated or lost its unique original publication"
                        .into(),
                );
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::FiendishPandaJournalV1 { journal: phase } = event else {
            continue;
        };
        let reference = match phase.as_ref() {
            JournalV1::Published { capture, .. } => {
                validate_publication(state, at as u64)?;
                *capture
            }
            JournalV1::DropInput { capture, .. } => {
                root_at(state, *capture as usize)?;
                if state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Unavailable { input, .. } if *input == at as u64))).count() != 1 {
                    return Err("Panda original placement input was erased or reused".into());
                }
                *capture
            }
            JournalV1::Unavailable { input, .. } => {
                validate_drop(state, at as u64)?;
                *input
            }
            JournalV1::RemovalInput { publication, .. } => {
                if !matches!(journal(state, *publication)?, JournalV1::Published { .. }) {
                    return Err("Panda removal lost original publication".into());
                }
                if state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Removed { input, .. } if *input == at as u64))).count() != 1 {
                    return Err("Panda independent removal input was erased or reused".into());
                }
                *publication
            }
            JournalV1::Removed { input, .. } => {
                validate_removal(state, at as u64)?;
                *input
            }
            JournalV1::OperationInput {
                removal, context, ..
            } => {
                let JournalV1::Removed {
                    item,
                    kind: RemovalV1::Resolved,
                    ..
                } = journal(state, *removal)?
                else {
                    return Err("Panda operation input lost genuine resolution removal".into());
                };
                if at as u64 != removal + 1 || context.as_ref() != &canonical_context(item)
                    || state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Operation { input, .. } if *input == at as u64))).count() != 1
                { return Err("Panda independent operation input was changed, erased or replayed".into()); }
                *removal
            }
            JournalV1::Closed { removal, .. } => {
                if !matches!(journal(state, *removal)?, JournalV1::Removed { .. }) {
                    return Err("Panda operation/closure lost original removal".into());
                }
                *removal
            }
            JournalV1::Operation { input, .. } => {
                validate_operation(state, at as u64)?;
                if state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), JournalV1::Closed { operation: Some(reference), .. } if *reference == at as u64))).count() != 1 {
                    return Err("Panda actual operation was unused or borrowed twice".into());
                }
                *input
            }
        };
        if reference >= at as u64 {
            return Err("Panda custody reordered original input".into());
        }
    }
    Ok(())
}
