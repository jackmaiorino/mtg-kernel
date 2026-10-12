//! Committed ownership of the bounded mill-then-mandatory-return lifecycle.
//! Predecessor programs emit no receipts and retain their existing behavior.
use super::*;
use crate::event::CommittedEvent;
use crate::state::AbilitySourceContractV4;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InspirationResolutionReceiptV1 {
    pub history_index: u32,
    pub previous_history_index: Option<u32>,
    pub ordinal: u32,
    pub stack_item_id: StackItemId,
    pub source: AbilitySourceContractV4,
    /// Independent committed snapshot of the engine-published boundary.
    /// None closes the resolution, forbidding a restart of this stack item.
    pub pending: Option<Box<EffectContinuation>>,
}

fn owned_root(op: &EffectOp) -> bool {
    matches!(op, EffectOp::Sequence(parts) if matches!(parts.as_slice(),
    [EffectOp::MillCards { player: PlayerRef::Controller, count: 3 }, EffectOp::SelectObjectsV1 { rule }]
        if *rule == ObjectSelectionRuleV1 {
            player: PlayerRef::Controller, zone: Zone::Graveyard, any_player: false,
            filter: ObjectSelectionFilterV1::InstantOrSorcery, min: 1, max: 1,
            action: ObjectSelectionActionV1::MoveTo(Zone::Hand),
        }))
}

/// Only the engine can acquire this loan while the complete resolving row
/// is live. It cannot be manufactured by deserializing a continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspirationResolutionOwnerV1 {
    original: StackItem,
    initial: bool,
}

impl InspirationResolutionOwnerV1 {
    pub(crate) fn completion_matches(&self, item: &StackItem) -> bool {
        self.original == *item
    }
}

pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
    initial: bool,
) -> Result<Option<InspirationResolutionOwnerV1>, String> {
    if item.kind != crate::state::StackItemKind::Spell {
        return Ok(None);
    }
    let source = state
        .objects
        .try_get(item.source)
        .ok_or("Inspiration acquisition source is absent")?;
    let Some(root) = CARD_DEFS
        .get(usize::from(source.card_def))
        .and_then(|definition| (definition.spell_effect)())
        .filter(|root| owned_root(root))
    else {
        return Ok(None);
    };
    validate(state)?;
    crate::engine::validated_stack_item_target_spec(item, state)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|row| row == item)
        || (initial && state.stack.last() != Some(item))
    {
        return Err("Inspiration acquisition lost its genuine complete live row".into());
    }
    if initial {
        if state.engine.pending_effect.is_some()
            || history(state)?
                .iter()
                .any(|receipt| receipt.stack_item_id == item.v4.stack_item_id)
        {
            return Err("Inspiration initial acquisition cannot restart a resolution".into());
        }
    } else {
        let pending = state
            .engine
            .pending_effect
            .as_ref()
            .ok_or("Inspiration resumed acquisition lost its continuation")?;
        if pending.resolving_item != *item
            || *validated_definition_owned_root_effect(state, pending)? != root
        {
            return Err("Inspiration resumed acquisition changed its owned root or row".into());
        }
    }
    Ok(Some(InspirationResolutionOwnerV1 {
        original: item.clone(),
        initial,
    }))
}

fn with_row_loan(
    state: &mut GameState,
    owner: &InspirationResolutionOwnerV1,
    operation: impl FnOnce(&mut GameState) -> Result<ResumableProgress, String>,
) -> Result<ResumableProgress, String> {
    if state
        .stack
        .iter()
        .any(|row| row.v4.stack_item_id == owner.original.v4.stack_item_id)
    {
        return Err("Inspiration private loan requires the actual engine removal".into());
    }
    // The physical spell remains in Stack during its non-target choices.
    // Reinsert only this engine-owned row for synchronous validation; the
    // engine publishes the continuation row after the loan is returned.
    state.stack.push(owner.original.clone());
    let result = operation(state);
    let index = state
        .stack
        .iter()
        .position(|row| row.v4.stack_item_id == owner.original.v4.stack_item_id)
        .ok_or("Inspiration private row disappeared before loan return")?;
    let returned = state.stack.remove(index);
    if returned != owner.original {
        return Err("Inspiration private row changed before loan return".into());
    }
    result
}

pub(crate) fn begin_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: &InspirationResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    if !owner.initial
        || ctx.source != owner.original.source
        || ctx.controller != owner.original.controller
        || ctx.stack_item_id != Some(owner.original.v4.stack_item_id)
    {
        return Err("Inspiration initial private ownership changed".into());
    }
    with_row_loan(state, owner, |state| {
        super::begin_resumable_resolution_with_inspiration_owner(
            op,
            ctx,
            owner.original.clone(),
            state,
            Some(owner),
        )
    })
}

pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: &InspirationResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    if owner.initial
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_none_or(|pending| pending.resolving_item != owner.original)
    {
        return Err("Inspiration resumed private ownership changed".into());
    }
    with_row_loan(state, owner, |state| drive_owned(state, owner))
}

fn tracked(state: &GameState) -> Result<bool, String> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(false);
    };
    let may_be_owned = pending.resolving_item.kind == crate::state::StackItemKind::Spell
        && state
            .objects
            .try_get(pending.ctx.source)
            .and_then(|source| CARD_DEFS.get(usize::from(source.card_def)))
            .and_then(|definition| (definition.spell_effect)())
            .is_some_and(|root| owned_root(&root));
    if !may_be_owned {
        return Ok(false);
    }
    Ok(owned_root(&validated_definition_owned_root_effect(
        state, pending,
    )?))
}

fn history(state: &GameState) -> Result<Vec<&InspirationResolutionReceiptV1>, String> {
    let mut receipts: Vec<&InspirationResolutionReceiptV1> = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::InspirationResolutionBoundaryV1 { receipt } = event else {
            continue;
        };
        if usize::try_from(receipt.history_index).ok() != Some(index) {
            return Err("mandatory return resolution receipt moved in committed history".into());
        }
        let previous = receipts
            .iter()
            .rev()
            .find(|r| r.stack_item_id == receipt.stack_item_id);
        match previous {
            None => {
                if receipt.ordinal != 0 || receipt.previous_history_index.is_some() {
                    return Err("mandatory return resolution lost its initial receipt".into());
                }
                let pending = receipt
                    .pending
                    .as_ref()
                    .ok_or("initial receipt lost its resolving program")?;
                let root = CARD_DEFS
                    .get(usize::from(receipt.source.card_def))
                    .and_then(|def| (def.spell_effect)())
                    .ok_or("receipt definition has no spell program")?;
                if !owned_root(&root)
                    || pending.choice.is_some()
                    || pending.answered_choice_guard.is_some()
                    || pending.frames
                        != vec![EffectFrame::Program {
                            op: root,
                            path: Vec::new(),
                        }]
                {
                    return Err("initial mandatory return receipt is not the generated root".into());
                }
            }
            Some(previous) => {
                if previous.pending.is_none()
                    || previous.source != receipt.source
                    || previous.ordinal.checked_add(1) != Some(receipt.ordinal)
                    || receipt.previous_history_index != Some(previous.history_index)
                {
                    return Err(
                        "mandatory return resolution receipt chain changed or replayed".into(),
                    );
                }
            }
        }
        if let Some(pending) = receipt.pending.as_ref() {
            if pending.resolving_item.v4.stack_item_id != receipt.stack_item_id
                || pending.ctx.stack_item_id != Some(receipt.stack_item_id)
                || pending.ctx.source != receipt.source.source
                || pending.ctx.controller != receipt.source.controller
                || pending.resolving_item.source != receipt.source.source
                || pending.resolving_item.controller != receipt.source.controller
            {
                return Err("mandatory return receipt source or stack identity changed".into());
            }
        }
        receipts.push(receipt);
    }
    Ok(receipts)
}

pub(super) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::InspirationResolutionBoundaryV1 { .. }
        )
    }) {
        return Err("mandatory return receipt cannot enter the public trigger event log".into());
    }
    let receipts = history(state)?;
    let mut latest = Vec::new();
    for receipt in receipts.iter().rev() {
        if !latest
            .iter()
            .any(|r: &&InspirationResolutionReceiptV1| r.stack_item_id == receipt.stack_item_id)
        {
            latest.push(*receipt);
        }
    }
    let open: Vec<_> = latest.into_iter().filter(|r| r.pending.is_some()).collect();
    if open.len() > 1 {
        return Err("multiple mandatory return resolutions claim ownership".into());
    }
    if let Some(receipt) = open.first() {
        if state.engine.pending_effect.as_ref() != receipt.pending.as_deref() {
            return Err(
                "mandatory return resolution lost or rewrote its committed boundary".into(),
            );
        }
        let live = state
            .objects
            .try_get(receipt.source.source)
            .ok_or("resolution source no longer exists")?;
        if live.card_def != receipt.source.card_def
            || live.owner != receipt.source.owner
            || live.controller != receipt.source.controller
            || live.zone != receipt.source.zone
            || live.zone_change_count != receipt.source.zone_change_count
        {
            return Err("mandatory return resolution source incarnation changed".into());
        }
        u32::try_from(state.engine.event_history.len())
            .map_err(|_| "resolution history exceeds u32")?;
    } else if tracked(state)? {
        return Err("mandatory return resolution has no open committed boundary".into());
    }
    Ok(())
}

pub(super) fn preflight_begin(
    state: &GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    item: &StackItem,
    owner: Option<&InspirationResolutionOwnerV1>,
) -> Result<bool, String> {
    if !owned_root(op) {
        return Ok(false);
    }
    if owner.is_none_or(|owner| !owner.initial || owner.original != *item) {
        return Err("Inspiration begin requires its genuine private pre-pop owner".into());
    }
    validate(state)?;
    let tentative = EffectContinuation {
        resolving_item: item.clone(),
        ctx: ctx.clone(),
        frames: vec![EffectFrame::Program {
            op: op.clone(),
            path: Vec::new(),
        }],
        choice: None,
        answered_choice_guard: None,
    };
    if *validated_definition_owned_root_effect(state, &tentative)? != *op
        || ctx.source != item.source
        || ctx.controller != item.controller
        || ctx.stack_item_id != Some(item.v4.stack_item_id)
        || crate::engine::resolution_cast_v1::resolving_index(state, item.v4.stack_item_id)
            .and_then(|index| state.stack.get(index))
            != Some(item)
    {
        return Err("mandatory return begin lost its finalized definition-owned stack item".into());
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    let receipts = history(state)?;
    if receipts
        .iter()
        .any(|r| r.stack_item_id == item.v4.stack_item_id)
    {
        return Err("mandatory return stack resolution cannot restart".into());
    }
    u32::try_from(state.engine.event_history.len())
        .map_err(|_| "resolution history exceeds u32")?;
    Ok(true)
}

pub(super) fn record_begin(state: &mut GameState) -> Result<(), String> {
    let pending = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("initial resolution is absent")?;
    let receipt = InspirationResolutionReceiptV1 {
        history_index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "resolution history exceeds u32")?,
        previous_history_index: None,
        ordinal: 0,
        stack_item_id: pending.resolving_item.v4.stack_item_id,
        source: AbilitySourceContractV4::capture(state, pending.ctx.source),
        pending: Some(Box::new(pending.clone())),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::InspirationResolutionBoundaryV1 { receipt });
    Ok(())
}

pub(super) fn record_boundary(state: &mut GameState) -> Result<(), String> {
    let receipts = history(state)?;
    let Some(previous) = receipts.iter().rev().find(|r| {
        r.pending.is_some()
            && !receipts
                .iter()
                .any(|later| later.stack_item_id == r.stack_item_id && later.ordinal > r.ordinal)
    }) else {
        return Ok(());
    };
    let receipt = InspirationResolutionReceiptV1 {
        history_index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "resolution history exceeds u32")?,
        previous_history_index: Some(previous.history_index),
        ordinal: previous
            .ordinal
            .checked_add(1)
            .ok_or("resolution receipt ordinal overflow")?,
        stack_item_id: previous.stack_item_id,
        source: previous.source,
        pending: state.engine.pending_effect.clone().map(Box::new),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::InspirationResolutionBoundaryV1 { receipt });
    Ok(())
}

pub(super) fn drive(state: &mut GameState) -> Result<ResumableProgress, String> {
    if tracked(state)? {
        return Err("Inspiration resumption requires its genuine private removed-row owner".into());
    }
    super::vampire_gourmand_resolution_v1::drive(state)
}

pub(super) fn drive_owned(
    state: &mut GameState,
    owner: &InspirationResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    validate(state)?;
    if !tracked(state)?
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_none_or(|pending| pending.resolving_item != owner.original)
        || !state.stack.iter().any(|row| row == &owner.original)
    {
        return Err("Inspiration driver lost its exact private row loan".into());
    }
    let result = super::vampire_gourmand_resolution_v1::drive(state);
    if result.is_ok() {
        record_boundary(state)?;
    }
    result
}
