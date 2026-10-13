//! Committed ownership of the bounded mill-then-mandatory-return lifecycle.
//! Predecessor programs emit no receipts and retain their existing behavior.
use super::*;
use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, InspirationStartedV1};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InspirationInputKindV1 {
    Drive,
    Target(Target),
    Finish,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InspirationInputV1 {
    pub index: u32,
    pub previous: u32,
    pub kind: InspirationInputKindV1,
    pub pending: Box<EffectContinuation>,
    pub mill: Option<Vec<EffectObjectBinding>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InspirationOperationV1 {
    pub index: u32,
    pub input: u32,
    pub after: Option<Box<EffectContinuation>>,
    pub mill_moves: Vec<u32>,
    pub return_moves: Vec<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InspirationDepartureV1 {
    pub index: u32,
    pub closed: u32,
    pub item: Box<StackItem>,
    pub movement: Option<u32>,
    pub virtual_ceased: bool,
}
pub(super) struct InspirationAnswerOwnerV1 {
    previous: u32,
    before: EffectContinuation,
    kind: InspirationInputKindV1,
}
fn next_index(state: &GameState) -> Result<u32, String> {
    u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Inspiration history exhausted".into())
}
fn latest_open(state: &GameState) -> Result<&InspirationResolutionReceiptV1, String> {
    history(state)?
        .into_iter()
        .rev()
        .find(|r| r.pending.is_some())
        .ok_or("Inspiration open boundary missing".into())
}
fn input(
    state: &mut GameState,
    previous: u32,
    kind: InspirationInputKindV1,
    before: EffectContinuation,
) -> Result<u32, String> {
    let mill = if kind == InspirationInputKindV1::Drive {
        match before.frames.last() {
            Some(EffectFrame::Program { op, path }) if owned_root(op) && path.is_empty() => {
                Some(bind_library_top(state, before.ctx.controller, 3))
            }
            Some(EffectFrame::MillLibraryBatch { objects, .. }) => Some(objects.clone()),
            _ => None,
        }
    } else {
        None
    };
    let index = next_index(state)?;
    state
        .engine
        .event_history
        .push(CommittedEvent::InspirationResolutionInputV1 {
            input: Box::new(InspirationInputV1 {
                index,
                previous,
                kind,
                pending: Box::new(before),
                mill,
            }),
        });
    Ok(index)
}
fn record_operation(state: &mut GameState, at: u32) -> Result<(), String> {
    let mut mill_moves = Vec::new();
    let mut return_moves = Vec::new();
    for (index, event) in state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(at as usize + 1)
    {
        match event {
            CommittedEvent::ZoneChange {
                from: Zone::Library,
                to: Zone::Graveyard,
                ..
            } => mill_moves.push(index as u32),
            CommittedEvent::ZoneChange {
                from: Zone::Graveyard,
                to: Zone::Hand,
                ..
            } => return_moves.push(index as u32),
            _ => {}
        }
    }
    let operation = InspirationOperationV1 {
        index: next_index(state)?,
        input: at,
        after: state.engine.pending_effect.clone().map(Box::new),
        mill_moves,
        return_moves,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::InspirationResolutionOperationV1 {
            operation: Box::new(operation),
        });
    record_boundary(state)
}
pub(super) fn prepare_answer(
    state: &GameState,
    kind: InspirationInputKindV1,
) -> Result<Option<InspirationAnswerOwnerV1>, String> {
    if !tracked(state)? {
        return Ok(None);
    }
    validate(state)?;
    let receipt = latest_open(state)?;
    let before = state
        .engine
        .pending_effect
        .clone()
        .ok_or("Inspiration answer lost pending")?;
    if before.choice.is_none() || !state.stack.iter().any(|row| row == &before.resolving_item) {
        return Err("Inspiration answer lacks actual live choice".into());
    }
    Ok(Some(InspirationAnswerOwnerV1 {
        previous: receipt.history_index,
        before,
        kind,
    }))
}
pub(super) fn record_answer(
    state: &mut GameState,
    owner: Option<InspirationAnswerOwnerV1>,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let at = input(state, owner.previous, owner.kind, owner.before)?;
    record_operation(state, at)
}
fn validate_operation(
    state: &GameState,
    receipt: &InspirationResolutionReceiptV1,
    previous: &InspirationResolutionReceiptV1,
) -> Result<(), String> {
    let op_at = receipt
        .history_index
        .checked_sub(1)
        .ok_or("Inspiration operation index underflow")?;
    let Some(CommittedEvent::InspirationResolutionOperationV1 { operation }) =
        state.engine.event_history.get(op_at as usize)
    else {
        return Err("Inspiration boundary lost independent actual execution".into());
    };
    let Some(CommittedEvent::InspirationResolutionInputV1 { input }) =
        state.engine.event_history.get(operation.input as usize)
    else {
        return Err("Inspiration execution lost actual owned input".into());
    };
    if operation.index != op_at
        || input.index != operation.input
        || input.index <= previous.history_index
        || input.index >= operation.index
        || input.previous != previous.history_index
        || previous.pending.as_deref() != Some(input.pending.as_ref())
        || operation.after != receipt.pending
    {
        return Err("Inspiration operation redirected original boundary or remainder".into());
    }
    let events = &state.engine.event_history[input.index as usize + 1..operation.index as usize];
    let actual = |from, to| {
        events
            .iter()
            .enumerate()
            .filter_map(|(offset, e)| match e {
                CommittedEvent::ZoneChange { from: f, to: t, .. } if *f == from && *t == to => {
                    Some(input.index + 1 + offset as u32)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    if actual(Zone::Library, Zone::Graveyard) != operation.mill_moves
        || actual(Zone::Graveyard, Zone::Hand) != operation.return_moves
    {
        return Err("Inspiration operation omitted actual movements".into());
    }
    let check_moves = |bindings: &[EffectObjectBinding],
                       moves: &[u32],
                       from,
                       to|
     -> Result<(), String> {
        if bindings.len() != moves.len() {
            return Err("Inspiration movement count changed".into());
        }
        for (binding, at) in bindings.iter().zip(moves) {
            if !matches!(state.engine.event_history.get(*at as usize), Some(CommittedEvent::ZoneChange {object, from: f, to: t, ..})
                if *object == binding.object && *f == from && *t == to)
            {
                return Err("Inspiration movement redirected selected incarnation".into());
            }
        }
        Ok(())
    };
    match input.kind {
        InspirationInputKindV1::Drive => {
            if input.pending.choice.is_some() {
                return Err("Inspiration execution has unanswered choice".into());
            }
            let ordering = matches!(
                operation.after.as_deref().and_then(|p| p.choice.as_ref()),
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::OrderMilledIntoGraveyard,
                    ..
                })
            );
            if ordering {
                if !operation.mill_moves.is_empty() {
                    return Err("Inspiration ordered mill moved early".into());
                }
            } else if let Some(mill) = &input.mill {
                check_moves(mill, &operation.mill_moves, Zone::Library, Zone::Graveyard)?;
            } else if !operation.mill_moves.is_empty() {
                return Err("Inspiration execution invented mill".into());
            }
            let returned = match input.pending.frames.last() {
                Some(EffectFrame::ApplySelectedObjectsV1 { rule, selected, .. })
                    if rule.filter == ObjectSelectionFilterV1::InstantOrSorcery =>
                {
                    Some(selected)
                }
                _ => None,
            };
            if let Some(returned) = returned {
                check_moves(
                    returned,
                    &operation.return_moves,
                    Zone::Graveyard,
                    Zone::Hand,
                )?;
            } else if !operation.return_moves.is_empty() {
                return Err("Inspiration return has no consumed answer".into());
            }
            if operation.after.is_none() && returned.is_none() {
                return Err("Inspiration closed before the mandatory return answer".into());
            }
        }
        InspirationInputKindV1::Target(_) | InspirationInputKindV1::Finish => {
            let mut expected = input.pending.as_ref().clone();
            let Some(PendingEffectChoice::SelectTargets { purpose, .. }) = expected.choice.as_ref()
            else {
                return Err("Inspiration answer lost its actual selection prompt".into());
            };
            if !matches!(
                purpose,
                EffectTargetSelectionPurpose::OrderMilledIntoGraveyard
            ) && !matches!(purpose, EffectTargetSelectionPurpose::SelectObjectsV1 { rule, .. } if rule.filter == ObjectSelectionFilterV1::InstantOrSorcery)
            {
                return Err("Inspiration answer changed its selection purpose".into());
            }
            match input.kind {
                InspirationInputKindV1::Target(target) => {
                    let PendingEffectChoice::SelectTargets {
                        selected,
                        legal,
                        min_targets,
                        max_targets,
                        ordered,
                        ..
                    } = expected.choice.as_mut().unwrap()
                    else {
                        unreachable!()
                    };
                    let position = legal
                        .iter()
                        .position(|candidate| candidate.target == target)
                        .ok_or("Inspiration input target was not actually offered")?;
                    selected.push(legal.remove(position));
                    let required = usize::from(*min_targets).saturating_sub(selected.len());
                    if (*ordered && required == 1 && legal.len() == 1)
                        || (!*ordered && required > 0 && required == legal.len())
                    {
                        selected.append(legal);
                    }
                    if selected.len() == usize::from(*max_targets) {
                        complete_resumable_target_selection(&mut expected)?;
                    }
                }
                InspirationInputKindV1::Finish => {
                    if !matches!(expected.choice.as_ref(), Some(PendingEffectChoice::SelectTargets { selected, min_targets, .. }) if selected.len() >= usize::from(*min_targets))
                    {
                        return Err("Inspiration finish skipped a mandatory available card".into());
                    }
                    complete_resumable_target_selection(&mut expected)?;
                }
                InspirationInputKindV1::Drive => unreachable!(),
            }
            if operation.after.as_deref() != Some(&expected)
                || !operation.mill_moves.is_empty()
                || !operation.return_moves.is_empty()
                || input.mill.is_some()
            {
                return Err(
                    "Inspiration accepted answer changed its exact before-to-after transition"
                        .into(),
                );
            }
        }
    }
    Ok(())
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
        let mut unmarked = item.clone();
        if self.initial {
            unmarked.v4.inspiration_started_v1 = InspirationStartedV1::default();
        }
        self.original == unmarked
    }
}

/// A genuine private loan, never serialized or hashed into predecessor state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InspirationRuntimeV1(pub(super) Option<InspirationResolutionOwnerV1>);
impl std::hash::Hash for InspirationRuntimeV1 {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}
fn locally_owned(state: &GameState) -> bool {
    state.engine.inspiration_runtime_v1.0.is_some()
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
        if !item.v4.inspiration_started_v1.is_empty()
            || state.engine.pending_effect.is_some()
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
    let mut expected = owner.original.clone();
    if owner.initial {
        expected.v4.inspiration_started_v1 = InspirationStartedV1(Some(
            u32::try_from(state.engine.event_history.len())
                .map_err(|_| "Inspiration history exhausted")?,
        ));
    }
    if locally_owned(state) {
        return Err("Inspiration private loans cannot nest".into());
    }
    state.engine.inspiration_runtime_v1.0 = Some(owner.clone());
    state.stack.push(expected.clone());
    let result = operation(state);
    state.engine.inspiration_runtime_v1.0 = None;
    let index = state
        .stack
        .iter()
        .position(|row| row.v4.stack_item_id == owner.original.v4.stack_item_id)
        .ok_or("Inspiration private row disappeared before loan return")?;
    let returned = state.stack.remove(index);
    if returned != expected {
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
            state
                .stack
                .last()
                .ok_or("Inspiration initial marked row absent")?
                .clone(),
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
                    || pending.resolving_item.v4.inspiration_started_v1.0
                        != Some(receipt.history_index)
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
                validate_operation(state, receipt, previous)?;
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
            let first = receipts
                .iter()
                .find(|r| r.stack_item_id == receipt.stack_item_id);
            if first.is_some_and(|first| {
                first
                    .pending
                    .as_ref()
                    .is_none_or(|initial| initial.resolving_item != pending.resolving_item)
            }) {
                return Err(
                    "Inspiration continuation changed its intrinsic full started row".into(),
                );
            }
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

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if locally_owned(state) {
        return Ok(());
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::InspirationResolutionBoundaryV1 { .. }
                | CommittedEvent::InspirationResolutionInputV1 { .. }
                | CommittedEvent::InspirationResolutionOperationV1 { .. }
                | CommittedEvent::InspirationEngineDepartureV1 { .. }
        )
    }) {
        return Err("mandatory return receipt cannot enter the public trigger event log".into());
    }
    let receipts = history(state)?;
    for item in &state.stack {
        if let Some(first) = item.v4.inspiration_started_v1.0 {
            let receipt = receipts
                .iter()
                .find(|r| r.history_index == first && r.ordinal == 0)
                .ok_or("Inspiration marked live row lost its original receipt")?;
            if receipt.stack_item_id != item.v4.stack_item_id
                || receipt
                    .pending
                    .as_ref()
                    .is_none_or(|p| p.resolving_item != *item)
            {
                return Err("Inspiration intrinsic started binding changed".into());
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::InspirationResolutionInputV1 { input } => {
                let uses = state.engine.event_history.iter().filter(|e| matches!(e,
                    CommittedEvent::InspirationResolutionOperationV1 { operation } if operation.input == input.index)).count();
                if input.index as usize != at || uses != 1 {
                    return Err("Inspiration actual input erased, duplicated or orphaned".into());
                }
            }
            CommittedEvent::InspirationResolutionOperationV1 { operation } => {
                if operation.index as usize != at
                    || !matches!(state.engine.event_history.get(at+1),
                    Some(CommittedEvent::InspirationResolutionBoundaryV1 { receipt }) if receipt.history_index as usize == at+1)
                {
                    return Err("Inspiration execution has no immediate boundary".into());
                }
            }
            CommittedEvent::InspirationEngineDepartureV1 { departure } => {
                if departure.index as usize != at
                    || departure.closed as usize >= at
                    || !matches!(state.engine.event_history.get(departure.closed as usize),
                        Some(CommittedEvent::InspirationResolutionBoundaryV1 { receipt })
                            if receipt.pending.is_none() && receipt.stack_item_id == departure.item.v4.stack_item_id)
                {
                    return Err("orphan Inspiration engine departure".into());
                }
            }
            _ => {}
        }
    }
    let mut latest = Vec::new();
    for receipt in receipts.iter().rev() {
        if !latest
            .iter()
            .any(|r: &&InspirationResolutionReceiptV1| r.stack_item_id == receipt.stack_item_id)
        {
            latest.push(*receipt);
        }
    }
    for receipt in &latest {
        if receipt.pending.is_none() {
            validate_departure(state, receipt)?;
        } else if state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == receipt.stack_item_id)
            .collect::<Vec<_>>()
            .as_slice()
            != [&receipt.pending.as_ref().unwrap().resolving_item]
        {
            return Err("Inspiration open boundary lost sole actual marked live row".into());
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

fn validate_departure(
    state: &GameState,
    receipt: &InspirationResolutionReceiptV1,
) -> Result<(), String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::InspirationEngineDepartureV1 { departure }
                if departure.closed == receipt.history_index =>
            {
                Some((at, departure.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, departure)] = rows.as_slice() else {
        return Err("Inspiration closure lacks unique actual engine departure".into());
    };
    let first = history(state)?
        .into_iter()
        .find(|r| r.stack_item_id == receipt.stack_item_id)
        .ok_or("Inspiration initial row missing")?;
    if departure.index as usize != *at
        || departure.index <= receipt.history_index
        || first
            .pending
            .as_ref()
            .is_none_or(|p| p.resolving_item != *departure.item)
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == receipt.stack_item_id)
    {
        return Err("Inspiration closed row was redirected or reintroduced".into());
    }
    match departure.movement {
        Some(index)
            if !departure.virtual_ceased
                && !departure.item.is_copy
                && index > receipt.history_index
                && index < departure.index =>
        {
            let destination = if departure.item.is_flashback {
                Zone::Exile
            } else {
                Zone::Graveyard
            };
            if !matches!(state.engine.event_history.get(index as usize), Some(CommittedEvent::ZoneChange {
                object, from: Zone::Stack, to, controller_before }) if *object == receipt.source.source && *to == destination && *controller_before == receipt.source.controller)
            {
                return Err("Inspiration closure lost its genuine source movement".into());
            }
        }
        None if departure.virtual_ceased && departure.item.is_copy => {}
        _ => return Err("Inspiration departure outcome changed".into()),
    }
    Ok(())
}
pub(crate) fn finish_owned(
    state: &mut GameState,
    owner: &InspirationResolutionOwnerV1,
    item: &StackItem,
    finish: impl FnOnce(&mut GameState) -> Result<(), String>,
) -> Result<(), String> {
    if !owner.completion_matches(item)
        || locally_owned(state)
        || state.engine.pending_effect.is_some()
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Inspiration final departure lacks genuine completed private owner".into());
    }
    let closed = history(state)?
        .into_iter()
        .rev()
        .find(|r| r.stack_item_id == item.v4.stack_item_id)
        .filter(|r| r.pending.is_none())
        .ok_or("Inspiration completion boundary missing")?
        .history_index;
    let marked = history(state)?
        .into_iter()
        .find(|r| r.stack_item_id == item.v4.stack_item_id)
        .and_then(|r| r.pending.as_ref())
        .ok_or("Inspiration original marked row missing")?
        .resolving_item
        .clone();
    if !owner.completion_matches(&marked) {
        return Err("Inspiration finalization changed original marked row".into());
    }
    let start = state.engine.event_history.len();
    state.engine.inspiration_runtime_v1.0 = Some(owner.clone());
    // Retain the genuine owner row for both the existing zone-change route and
    // actual virtual cease. No user decision can observe this synchronous loan.
    state.stack.push(marked.clone());
    let result = finish(state);
    state.engine.inspiration_runtime_v1.0 = None;
    result?;
    if state
        .stack
        .iter()
        .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Inspiration final engine operation retained its source row".into());
    }
    let moves = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(start)
        .filter_map(|(at, e)| match e {
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Stack,
                ..
            } if *object == item.source => Some(at as u32),
            _ => None,
        })
        .collect::<Vec<_>>();
    let movement = if item.is_copy {
        if !moves.is_empty() {
            return Err("Inspiration virtual copy entered a physical card zone".into());
        }
        None
    } else {
        let [movement] = moves.as_slice() else {
            return Err("Inspiration actual final movement missing or duplicated".into());
        };
        Some(*movement)
    };
    let departure = InspirationDepartureV1 {
        index: next_index(state)?,
        closed,
        item: Box::new(marked),
        movement,
        virtual_ceased: item.is_copy,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::InspirationEngineDepartureV1 {
            departure: Box::new(departure),
        });
    validate(state)
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
    if owner.is_none_or(|owner| !owner.initial || !owner.completion_matches(item)) {
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
    if pending.resolving_item.v4.inspiration_started_v1.0
        != Some(state.engine.event_history.len() as u32)
    {
        return Err("Inspiration initial row lost intrinsic started binding".into());
    }
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
            .is_none_or(|pending| !owner.completion_matches(&pending.resolving_item))
        || !state.stack.iter().any(|row| owner.completion_matches(row))
    {
        return Err("Inspiration driver lost its exact private row loan".into());
    }
    let previous = latest_open(state)?.history_index;
    let before = state
        .engine
        .pending_effect
        .clone()
        .ok_or("Inspiration owned input lost")?;
    let at = input(state, previous, InspirationInputKindV1::Drive, before)?;
    let result = super::vampire_gourmand_resolution_v1::drive(state);
    if result.is_ok() {
        record_operation(state, at)?;
    }
    result
}

#[cfg(test)]
mod absence_tests {
    use super::*;
    use std::hash::{Hash, Hasher};
    #[derive(Default)]
    struct Bytes(Vec<u8>);
    impl Hasher for Bytes {
        fn finish(&self) -> u64 {
            self.0.len() as u64
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }
    #[test]
    fn predecessor_none_marker_and_runtime_contribute_no_hash_or_serialized_fields() {
        let mut bytes = Bytes::default();
        InspirationStartedV1::default().hash(&mut bytes);
        InspirationRuntimeV1::default().hash(&mut bytes);
        assert!(bytes.0.is_empty());
        let stack = serde_json::to_value(crate::state::StackStateV4::default()).unwrap();
        assert!(stack.get("inspiration_started_v1").is_none());
        let state = crate::policy_observation_v6::tests::ready_state();
        let encoded = serde_json::to_value(&state.engine).unwrap();
        assert!(encoded.get("inspiration_runtime_v1").is_none());
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(state, restored);
        validate(&state).unwrap();
    }
}
