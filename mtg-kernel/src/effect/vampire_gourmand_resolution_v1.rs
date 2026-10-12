//! Committed ownership of the bounded optional creature sacrifice and draw/evasion lifecycle.
//! Predecessor programs emit no receipts and retain their existing behavior.
use super::*;
use crate::card_def::CARD_DEFS;
use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, VampireGourmandStartedBindingV1};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VampireGourmandResolutionReceiptV1 {
    pub history_index: u32,
    pub previous_history_index: Option<u32>,
    pub ordinal: u32,
    pub stack_item_id: StackItemId,
    pub source: AbilitySourceContractV4,
    /// Independent committed snapshot of the engine-published boundary.
    /// None closes the resolution, forbidding a restart of this stack item.
    pub pending: Option<Box<EffectContinuation>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VampireGourmandCostResultV2 {
    pub selected: Option<EffectObjectBinding>,
    pub source_before: Box<crate::state::GameObject>,
    pub sacrifice_history_index: Option<u32>,
    pub move_history_index: Option<u32>,
    pub draw_history_index: Option<u32>,
    pub installed: Option<crate::engine::UntilEndOfTurnEffect>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VampireGourmandCostOperationV2 {
    pub history_index: u32,
    pub boundary_history_index: u32,
    pub before: Box<EffectContinuation>,
    pub result: VampireGourmandCostResultV2,
}

fn owned_root(op: &EffectOp) -> bool {
    *op == crate::vampire_gourmand_v1::program()
}
fn source_owns_root(source: AbilitySourceContractV4, root: &EffectOp) -> bool {
    CARD_DEFS
        .get(usize::from(source.card_def))
        .is_some_and(|definition| {
            definition.name == "Vampire Gourmand"
                && crate::trigger::triggers_for(source.card_def)
                    .iter()
                    .any(|ability| (ability.effect)() == *root)
                && owned_root(root)
        })
}

/// A synchronous, nonserializable loan of an actually indexed stack item.
/// Only this module constructs it, before engine removal; external restore
/// validation cannot manufacture an absent-live resolution from pending data.
pub(crate) struct VampireGourmandResolutionOwnerV1 {
    original: StackItem,
    expected: StackItem,
    initial: bool,
}
impl VampireGourmandResolutionOwnerV1 {
    pub(crate) fn completion_matches(&self, item: &StackItem) -> bool {
        *item == self.expected
    }
}
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
    initial: bool,
) -> Result<Option<VampireGourmandResolutionOwnerV1>, String> {
    let owns = item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.v4.ability_source_contract.is_some_and(|source| {
            item.inline_effect
                .as_ref()
                .is_some_and(|root| source_owns_root(source, root))
        });
    if !owns {
        return Ok(None);
    }
    validate(state)?;
    crate::engine::validated_stack_item_target_spec(item, state)?;
    if state
        .stack
        .iter()
        .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|live| live == item)
    {
        return Err("optional sacrifice owner did not acquire the genuine live stack item".into());
    }
    let mut expected = item.clone();
    if initial {
        if state.engine.pending_effect.is_some()
            || !item.v4.vampire_gourmand_started_v1.is_empty()
            || history(state)?
                .iter()
                .any(|r| r.stack_item_id == item.v4.stack_item_id)
        {
            return Err("optional sacrifice initial owner cannot restart".into());
        }
        expected.v4.vampire_gourmand_started_v1.0 = Some(VampireGourmandStartedBindingV1 {
            stack_item_id: item.v4.stack_item_id,
            source: item.v4.ability_source_contract.unwrap(),
            first_receipt_history_index: u32::try_from(state.engine.event_history.len())
                .map_err(|_| "resolution history exceeds u32")?,
        });
    } else if state
        .engine
        .pending_effect
        .as_ref()
        .is_none_or(|pending| pending.resolving_item != *item)
    {
        return Err("optional sacrifice resumed owner changed its published item".into());
    }
    Ok(Some(VampireGourmandResolutionOwnerV1 {
        original: item.clone(),
        expected,
        initial,
    }))
}

pub(crate) fn begin_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: &VampireGourmandResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    if !owner.initial
        || state.engine.pending_effect.is_some()
        || state
            .stack
            .iter()
            .any(|item| item.v4.stack_item_id == owner.original.v4.stack_item_id)
        || owner.original.inline_effect.as_ref() != Some(op)
        || ctx.stack_item_id != Some(owner.original.v4.stack_item_id)
        || ctx.source != owner.original.source
        || ctx.controller != owner.original.controller
        || ctx.ability_source_contract != owner.original.v4.ability_source_contract
    {
        return Err("optional sacrifice initial removed-item ownership changed".into());
    }
    super::validate_resumable_program(op)?;
    let pending = EffectContinuation {
        resolving_item: owner.expected.clone(),
        ctx: ctx.clone(),
        frames: vec![EffectFrame::Program {
            op: op.clone(),
            path: Vec::new(),
        }],
        choice: None,
        answered_choice_guard: None,
    };
    if *super::validated_definition_owned_root_effect(state, &pending)? != *op {
        return Err("optional sacrifice removed owner lost its definition-owned context".into());
    }
    state.engine.pending_effect = Some(pending);
    record_begin(state)?;
    drive_owned(state, owner)
}

pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: &VampireGourmandResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    if owner.initial {
        return Err("initial owner cannot resume a published selection".into());
    }
    drive_owned(state, owner)
}
fn tracked(state: &GameState) -> Result<bool, String> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(false);
    };
    Ok(
        pending.resolving_item.kind == crate::state::StackItemKind::TriggeredAbility
            && pending
                .resolving_item
                .v4
                .ability_source_contract
                .is_some_and(|source| {
                    pending
                        .resolving_item
                        .inline_effect
                        .as_ref()
                        .is_some_and(|root| source_owns_root(source, root))
                }),
    )
}

fn history(state: &GameState) -> Result<Vec<&VampireGourmandResolutionReceiptV1>, String> {
    let mut receipts: Vec<&VampireGourmandResolutionReceiptV1> = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::VampireGourmandResolutionBoundaryV1 { receipt } = event else {
            continue;
        };
        if usize::try_from(receipt.history_index).ok() != Some(index) {
            return Err("optional sacrifice resolution receipt moved in committed history".into());
        }
        let previous = receipts
            .iter()
            .rev()
            .find(|r| r.stack_item_id == receipt.stack_item_id);
        match previous {
            None => {
                if receipt.ordinal != 0 || receipt.previous_history_index.is_some() {
                    return Err("optional sacrifice resolution lost its initial receipt".into());
                }
                let pending = receipt
                    .pending
                    .as_ref()
                    .ok_or("initial receipt lost its resolving program")?;
                let root = pending
                    .resolving_item
                    .inline_effect
                    .clone()
                    .ok_or("receipt definition has no activated program")?;
                if !source_owns_root(receipt.source, &root)
                    || pending.choice.is_some()
                    || pending.answered_choice_guard.is_some()
                    || pending.frames
                        != vec![EffectFrame::Program {
                            op: root,
                            path: Vec::new(),
                        }]
                {
                    return Err(
                        "initial optional sacrifice receipt is not the generated root".into(),
                    );
                }
            }
            Some(previous) => {
                if previous.pending.is_none()
                    || previous.source != receipt.source
                    || previous.ordinal.checked_add(1) != Some(receipt.ordinal)
                    || receipt.previous_history_index != Some(previous.history_index)
                {
                    return Err(
                        "optional sacrifice resolution receipt chain changed or replayed".into(),
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
                return Err("optional sacrifice receipt source or stack identity changed".into());
            }
        }
        receipts.push(receipt);
    }
    Ok(receipts)
}

fn validate_cost_operations(
    state: &GameState,
    receipts: &[&VampireGourmandResolutionReceiptV1],
) -> Result<(), String> {
    let mut used = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::VampireGourmandCostOperationV2 { operation } = event else {
            continue;
        };
        if operation.history_index as usize != index {
            return Err("Gourmand actual operation moved in history".into());
        }
        let previous = receipts
            .iter()
            .find(|r| r.history_index == operation.boundary_history_index)
            .ok_or("Gourmand operation lost accepted selection boundary")?;
        if operation.before.as_ref()
            != previous
                .pending
                .as_deref()
                .ok_or("Gourmand operation follows closed boundary")?
            || operation.boundary_history_index as usize >= index
            || operation.before.choice.is_some()
            || operation.before.answered_choice_guard.is_some()
        {
            return Err("Gourmand operation rewrote original accepted selection".into());
        }
        let [EffectFrame::ApplySelectedObjectsV1 {
            rule,
            original_candidates,
            selected,
            path,
        }] = operation.before.frames.as_slice()
        else {
            return Err("Gourmand operation lacks exact selected consequence".into());
        };
        if !path.is_empty()
            || crate::vampire_gourmand_v1::program() != (EffectOp::SelectObjectsV1 { rule: *rule })
            || selected.len() > 1
            || selected.first().copied() != operation.result.selected
            || selected.iter().any(|b| !original_candidates.contains(b))
        {
            return Err("Gourmand operation changed cost cardinality or candidate".into());
        }
        let source = previous.source;
        if operation.result.source_before.owner != source.owner {
            return Err("Gourmand operation changed source owner".into());
        }
        let events =
            &state.engine.event_history[operation.boundary_history_index as usize + 1..index];
        match operation.result.selected {
            None => {
                if operation.result.sacrifice_history_index.is_some()
                    || operation.result.move_history_index.is_some()
                    || operation.result.draw_history_index.is_some()
                    || operation.result.installed.is_some()
                    || events.iter().any(|e| {
                        matches!(
                            e,
                            CommittedEvent::Sacrificed { .. } | CommittedEvent::Draw { .. }
                        )
                    })
                {
                    return Err("Gourmand declined cost claimed consequences".into());
                }
            }
            Some(chosen) => {
                if chosen.expected_zone != Zone::Battlefield
                    || (chosen.object == source.source
                        && chosen.expected_zone_change_count == source.zone_change_count)
                {
                    return Err("Gourmand cost selected the original source".into());
                }
                let sacrifice = operation
                    .result
                    .sacrifice_history_index
                    .ok_or("Gourmand payment lost actual sacrifice")?;
                if sacrifice <= operation.boundary_history_index || sacrifice as usize >= index
                    || !matches!(state.engine.event_history.get(sacrifice as usize), Some(CommittedEvent::Sacrificed {object,controller_before,..})
                        if *object == chosen.object && *controller_before == source.controller)
                    || events.iter().filter(|e| matches!(e, CommittedEvent::Sacrificed { object,.. } if *object == chosen.object)).count() != 1 {
                    return Err("Gourmand cost lost exact actual sacrifice occurrence".into());
                }
                let moves: Vec<_> = events.iter().enumerate().filter_map(|(at,e)| matches!(e,
                    CommittedEvent::ZoneChange { object, from:Zone::Battlefield, .. } if *object == chosen.object)
                    .then_some(operation.boundary_history_index as usize + 1 + at)).collect();
                if moves.as_slice()
                    != operation
                        .result
                        .move_history_index
                        .map(|at| vec![at as usize])
                        .unwrap_or_default()
                {
                    return Err("Gourmand cost erased or duplicated actual payment move".into());
                }
                if let Some(moved) = operation.result.move_history_index {
                    let draw = operation
                        .result
                        .draw_history_index
                        .ok_or("Gourmand paid sacrifice skipped mandatory draw")?;
                    if moved <= sacrifice
                        || draw <= moved
                        || draw as usize >= index
                        || !matches!(state.engine.event_history.get(draw as usize), Some(CommittedEvent::Draw {player,..}) if *player == source.controller)
                    {
                        return Err("Gourmand draw lost actual ordered occurrence".into());
                    }
                    if events
                        .iter()
                        .filter(|e| {
                            matches!(e,
                        CommittedEvent::Draw { player, .. } if *player == source.controller)
                        })
                        .count()
                        != 1
                    {
                        return Err("Gourmand paid cost duplicated its draw".into());
                    }
                    let original_alive = operation.result.source_before.zone == Zone::Battlefield
                        && operation.result.source_before.zone_change_count
                            == source.zone_change_count;
                    if !original_alive {
                        let declaration = state.engine.event_history[..operation.boundary_history_index as usize]
                            .iter().rposition(|e| matches!(e,
                                CommittedEvent::DeclaredAttacker { source: id, source_zone_change_count, controller }
                                    if *id == source.source && *source_zone_change_count == source.zone_change_count
                                        && *controller == source.controller))
                            .ok_or("Gourmand result lost original attack occurrence")?;
                        if !state.engine.event_history[declaration + 1..index].iter().any(|e| matches!(e,
                            CommittedEvent::ZoneChange { object, from:Zone::Battlefield, .. } if *object == source.source)) {
                            return Err("Gourmand absent-source result lacks actual departure".into());
                        }
                    }
                    match operation.result.installed.as_ref() {
                        Some(
                            crate::engine::UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 {
                                object_id,
                                object_zone_change_count,
                                duration: crate::engine::EffectDuration::EndOfTurn,
                                ..
                            },
                        ) if original_alive
                            && *object_id == source.source
                            && *object_zone_change_count == source.zone_change_count => {}
                        None if !original_alive => {}
                        _ => {
                            return Err(
                                "Gourmand paid result skipped or redirected exact-source evasion"
                                    .into(),
                            )
                        }
                    }
                } else if operation.result.draw_history_index.is_some()
                    || operation.result.installed.is_some()
                {
                    return Err("Gourmand prevented sacrifice claimed paid consequences".into());
                }
            }
        }
        let next: Vec<_> = receipts
            .iter()
            .filter(|r| r.previous_history_index == Some(previous.history_index))
            .collect();
        if next.len() != 1 || next[0].pending.is_some() || next[0].history_index as usize <= index {
            return Err("Gourmand actual operation lacks unique later completion".into());
        }
        if used.contains(&previous.stack_item_id) {
            return Err("Gourmand paid selection executed twice".into());
        }
        used.push(previous.stack_item_id);
    }
    for receipt in receipts.iter().filter(|r| r.pending.is_none()) {
        let previous = receipt
            .previous_history_index
            .ok_or("Gourmand completion has no prior selection")?;
        if state.engine.event_history.iter().filter(|e| matches!(e,
            CommittedEvent::VampireGourmandCostOperationV2 { operation }
                if operation.boundary_history_index == previous && operation.history_index < receipt.history_index)).count() != 1
            || state.stack.iter().any(|i| i.v4.stack_item_id == receipt.stack_item_id) {
            return Err("Gourmand closed boundary lost actual operation or replayed live row".into());
        }
    }
    Ok(())
}

fn validate_restrictions(state: &GameState) -> Result<Vec<u32>, String> {
    let mut active = Vec::new();
    let mut used_expiries = Vec::new();
    let mut used_batches = Vec::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::VampireGourmandCostOperationV2 { operation } = event else {
            continue;
        };
        let Some(marker) = operation.result.installed.as_ref() else {
            continue;
        };
        let present = state
            .engine
            .until_end_of_turn
            .iter()
            .filter(|e| *e == marker)
            .count();
        let expired: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, e)| {
                if let CommittedEvent::VampireGourmandExpiredV2 {
                    operation_history_index,
                    cleanup_history_index,
                } = e
                {
                    (*operation_history_index as usize == at)
                        .then_some((index, *cleanup_history_index))
                } else {
                    None
                }
            })
            .collect();
        match (present, expired.as_slice()) {
            (1, []) => active.push(at.try_into().map_err(|_| "Gourmand history exceeds u32")?),
            (0, [(expired_at, cleanup)]) => {
                let Some(CommittedEvent::VampireGourmandCleanupV2 {
                    step: crate::state::Step::Cleanup,
                    operation_history_indices,
                    ..
                }) = state.engine.event_history.get(*cleanup as usize)
                else {
                    return Err("Gourmand expiry lacks genuine Cleanup batch".into());
                };
                let positions: Vec<_> = operation_history_indices
                    .iter()
                    .enumerate()
                    .filter_map(|(position, id)| (*id as usize == at).then_some(position))
                    .collect();
                if positions.len() != 1
                    || *cleanup as usize <= at
                    || *expired_at != *cleanup as usize + 1 + positions[0]
                {
                    return Err("Gourmand expiry changed batch occurrence or order".into());
                }
                used_expiries.push(*expired_at);
                used_batches.push(*cleanup as usize);
            }
            _ => {
                return Err(
                    "Gourmand restriction erased, duplicated, expired early or reintroduced".into(),
                )
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::VampireGourmandExpiredV2 { .. } if !used_expiries.contains(&at) =>
                return Err("orphan Gourmand expiry".into()),
            CommittedEvent::VampireGourmandCleanupV2 { operation_history_indices, .. } => {
                if operation_history_indices.is_empty() || !used_batches.contains(&at)
                    || operation_history_indices.iter().enumerate().any(|(ordinal,id)| operation_history_indices[..ordinal].contains(id)
                        || !matches!(state.engine.event_history.get(at + 1 + ordinal), Some(CommittedEvent::VampireGourmandExpiredV2 {
                            operation_history_index,cleanup_history_index }) if operation_history_index == id && *cleanup_history_index as usize == at)) {
                    return Err("Gourmand Cleanup batch lost exact ordered expiry members".into());
                }
            },
            _ => {},
        }
    }
    Ok(active)
}
pub(crate) fn expire(state: &mut GameState) -> Result<(), String> {
    let operation_history_indices = validate_restrictions(state)?;
    if operation_history_indices.is_empty() {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Gourmand expiry requires actual Cleanup".into());
    }
    let cleanup_history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Gourmand history exceeds u32")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::VampireGourmandCleanupV2 {
            turn: state.turn,
            ending_player: state.active_player,
            step: state.step,
            operation_history_indices: operation_history_indices.clone(),
        });
    for operation_history_index in operation_history_indices {
        state
            .engine
            .event_history
            .push(CommittedEvent::VampireGourmandExpiredV2 {
                operation_history_index,
                cleanup_history_index,
            });
    }
    Ok(())
}

pub(super) fn validate(state: &GameState) -> Result<(), String> {
    validate_with_owner(state, None)
}
fn validate_with_owner(
    state: &GameState,
    owner: Option<&VampireGourmandResolutionOwnerV1>,
) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::VampireGourmandResolutionBoundaryV1 { .. }
                | CommittedEvent::VampireGourmandCostOperationV2 { .. }
                | CommittedEvent::VampireGourmandCleanupV2 { .. }
                | CommittedEvent::VampireGourmandExpiredV2 { .. }
        )
    }) {
        return Err("optional sacrifice receipt cannot enter the public trigger event log".into());
    }
    let receipts = history(state)?;
    validate_cost_operations(state, &receipts)?;
    validate_restrictions(state)?;
    // A started live root must retain its independent stack-owned marker.
    // Erasing pending_effect and every progress receipt cannot turn it back
    // into a genuinely unstarted stack item.
    for item in &state.stack {
        let Some(started) = item.v4.vampire_gourmand_started_v1.0 else {
            continue;
        };
        if started.stack_item_id != item.v4.stack_item_id
            || item.kind != crate::state::StackItemKind::TriggeredAbility
            || item.v4.ability_source_contract != Some(started.source)
            || item
                .inline_effect
                .as_ref()
                .is_none_or(|root| !source_owns_root(started.source, root))
            || receipts
                .iter()
                .find(|r| r.stack_item_id == started.stack_item_id)
                .is_none_or(|r| {
                    r.history_index != started.first_receipt_history_index
                        || r.source != started.source
                })
        {
            return Err("started optional sacrifice root lost its independent authority".into());
        }
    }
    for receipt in &receipts {
        if let Some(pending) = receipt.pending.as_ref() {
            let first = receipts
                .iter()
                .find(|r| r.stack_item_id == receipt.stack_item_id)
                .ok_or("optional sacrifice first receipt absent")?;
            let expected = VampireGourmandStartedBindingV1 {
                stack_item_id: receipt.stack_item_id,
                source: receipt.source,
                first_receipt_history_index: first.history_index,
            };
            if pending.resolving_item.v4.vampire_gourmand_started_v1.0 != Some(expected) {
                return Err("optional sacrifice receipt lost its started stack binding".into());
            }
        }
    }
    let mut latest = Vec::new();
    for receipt in receipts.iter().rev() {
        if !latest
            .iter()
            .any(|r: &&VampireGourmandResolutionReceiptV1| r.stack_item_id == receipt.stack_item_id)
        {
            latest.push(*receipt);
        }
    }
    let open: Vec<_> = latest.into_iter().filter(|r| r.pending.is_some()).collect();
    if open.len() > 1 {
        return Err("multiple optional sacrifice resolutions claim ownership".into());
    }
    if let Some(receipt) = open.first() {
        if state.engine.pending_effect.as_ref() != receipt.pending.as_deref() {
            return Err(
                "optional sacrifice resolution lost or rewrote its committed boundary".into(),
            );
        }
        let pending = state.engine.pending_effect.as_ref().unwrap();
        let live = state
            .stack
            .iter()
            .find(|item| item.v4.stack_item_id == receipt.stack_item_id);
        let removed = owner.filter(|owner| {
            owner.expected == pending.resolving_item
                && owner.expected.v4.stack_item_id == receipt.stack_item_id
        });
        if match (live, removed) {
            (Some(live), None) => live != &pending.resolving_item,
            (None, Some(_)) => false,
            _ => true,
        } {
            return Err("optional sacrifice live and pending start authority differ".into());
        }
        if pending.resolving_item.v4.ability_source_contract != Some(receipt.source) {
            return Err("optional sacrifice original source contract changed".into());
        }
        crate::engine::validated_stack_item_target_spec(&pending.resolving_item, state)?;
        u32::try_from(state.engine.event_history.len())
            .map_err(|_| "resolution history exceeds u32")?;
    } else if tracked(state)? {
        return Err("optional sacrifice resolution has no open committed boundary".into());
    }
    Ok(())
}

pub(super) fn preflight_begin(
    _state: &GameState,
    op: &EffectOp,
    _ctx: &ExecCtx,
    item: &StackItem,
) -> Result<bool, String> {
    if item.kind == crate::state::StackItemKind::TriggeredAbility
        && item
            .v4
            .ability_source_contract
            .is_some_and(|source| source_owns_root(source, op))
    {
        return Err("Gourmand initial resolution requires its private pre-pop owner".into());
    }
    Ok(false)
}

pub(super) fn record_begin(state: &mut GameState) -> Result<(), String> {
    let pending = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("initial resolution is absent")?;
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "resolution history exceeds u32")?;
    let stack_item_id = pending.resolving_item.v4.stack_item_id;
    let source = pending
        .resolving_item
        .v4
        .ability_source_contract
        .ok_or("optional sacrifice source contract absent")?;
    let started = VampireGourmandStartedBindingV1 {
        stack_item_id,
        source,
        first_receipt_history_index: history_index,
    };
    if pending.resolving_item.v4.vampire_gourmand_started_v1.0 != Some(started) {
        let live = state
            .stack
            .iter_mut()
            .find(|item| item.v4.stack_item_id == stack_item_id)
            .ok_or("optional sacrifice initial publication lacks removed-item ownership")?;
        if live != &pending.resolving_item {
            return Err("optional sacrifice initial live item changed".into());
        }
        live.v4.vampire_gourmand_started_v1.0 = Some(started);
    }
    let pending = state.engine.pending_effect.as_mut().unwrap();
    pending.resolving_item.v4.vampire_gourmand_started_v1.0 = Some(started);
    let receipt = VampireGourmandResolutionReceiptV1 {
        history_index,
        previous_history_index: None,
        ordinal: 0,
        stack_item_id: pending.resolving_item.v4.stack_item_id,
        source: pending
            .resolving_item
            .v4
            .ability_source_contract
            .ok_or("optional sacrifice source contract absent")?,
        pending: Some(Box::new(pending.clone())),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::VampireGourmandResolutionBoundaryV1 { receipt });
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
    let receipt = VampireGourmandResolutionReceiptV1 {
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
        .push(CommittedEvent::VampireGourmandResolutionBoundaryV1 { receipt });
    Ok(())
}

pub(super) fn drive(state: &mut GameState) -> Result<ResumableProgress, String> {
    if tracked(state)? {
        return Err("Gourmand resolution requires its genuine private removed-item owner".into());
    }
    super::frenzied_goblin_resolution_v1::drive(state)
}

fn drive_owned(
    state: &mut GameState,
    owner: &VampireGourmandResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    validate_with_owner(state, Some(owner))?;
    let before = state.engine.pending_effect.as_ref().unwrap().clone();
    let result = if before.choice.is_some()
        || matches!(before.frames.as_slice(),
        [EffectFrame::Program { op, path }] if *op == crate::vampire_gourmand_v1::program() && path.is_empty())
    {
        super::frenzied_goblin_resolution_v1::drive(state)?
    } else {
        let [EffectFrame::ApplySelectedObjectsV1 {
            rule,
            original_candidates,
            selected,
            path,
        }] = before.frames.as_slice()
        else {
            return Err("Gourmand consequence lost its exact accepted selection".into());
        };
        if before.answered_choice_guard.is_some()
            || !path.is_empty()
            || crate::vampire_gourmand_v1::program() != (EffectOp::SelectObjectsV1 { rule: *rule })
        {
            return Err("Gourmand consequence changed its definition-owned shape".into());
        }
        super::standard_selection_v1::validate_answer(
            state,
            &before,
            *rule,
            original_candidates,
            selected,
            path,
        )?;
        let boundary = history(state)?
            .last()
            .ok_or("Gourmand cost lost prior boundary")?
            .history_index;
        let result = crate::vampire_gourmand_v1::pay_and_capture(state, &before, selected)?;
        let operation = VampireGourmandCostOperationV2 {
            history_index: u32::try_from(state.engine.event_history.len())
                .map_err(|_| "Gourmand operation history exceeds u32")?,
            boundary_history_index: boundary,
            before: Box::new(before.clone()),
            result,
        };
        state
            .engine
            .event_history
            .push(CommittedEvent::VampireGourmandCostOperationV2 {
                operation: Box::new(operation),
            });
        state.engine.pending_effect = None;
        ResumableProgress::Complete(Box::new(before.resolving_item))
    };
    record_boundary(state)?;
    Ok(result)
}
