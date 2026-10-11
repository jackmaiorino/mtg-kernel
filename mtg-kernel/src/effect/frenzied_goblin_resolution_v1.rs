//! Definition-owned Frenzied attack optional-payment and restriction lifecycle.
//! Predecessor programs emit no receipts and retain their existing behavior.
use super::*;

use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, FrenziedGoblinStartedBindingV1};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrenziedGoblinResolutionReceiptV1 {
    pub history_index: u32,
    pub previous_history_index: Option<u32>,
    pub ordinal: u32,
    pub stack_item_id: StackItemId,
    pub source: AbilitySourceContractV4,
    /// Independent committed snapshot of the engine-published boundary.
    /// None closes the resolution, forbidding a restart of this stack item.
    pub pending: Option<Box<EffectContinuation>>,
}

pub(crate) fn owned_root(op: &EffectOp) -> bool {
    *op == crate::trigger::frenzied_goblin_effect()
}
fn source_owns_root(source: AbilitySourceContractV4, root: &EffectOp) -> bool {
    crate::frenzied_goblin_v1::is_owned(source.card_def) && owned_root(root)
}
/// A synchronous, nonserializable loan of an actually indexed stack item.
/// Only this module constructs it, before engine removal; external restore
/// validation cannot manufacture an absent-live resolution from pending data.
pub(crate) struct FrenziedGoblinResolutionOwnerV1 {
    original: StackItem,
    expected: StackItem,
    initial: bool,
}
impl FrenziedGoblinResolutionOwnerV1 {
    pub(crate) fn completion_matches(&self, item: &StackItem) -> bool {
        *item == self.expected
    }
}
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
    initial: bool,
) -> Result<Option<FrenziedGoblinResolutionOwnerV1>, String> {
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
    publication_for(state, item)?;
    crate::engine::validated_stack_item_target_spec(item, state)?;
    if state
        .stack
        .iter()
        .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|live| live == item)
    {
        return Err(
            "Frenzied red-payment owner did not acquire the genuine live stack item".into(),
        );
    }
    let mut expected = item.clone();
    if initial {
        if state.engine.pending_effect.is_some()
            || !item.v4.frenzied_goblin_started_v1.is_empty()
            || history(state)?
                .iter()
                .any(|r| r.stack_item_id == item.v4.stack_item_id)
        {
            return Err("Frenzied red-payment initial owner cannot restart".into());
        }
        expected.v4.frenzied_goblin_started_v1.0 = Some(FrenziedGoblinStartedBindingV1 {
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
        return Err("Frenzied red-payment resumed owner changed its published item".into());
    }
    Ok(Some(FrenziedGoblinResolutionOwnerV1 {
        original: item.clone(),
        expected,
        initial,
    }))
}

pub(crate) fn begin_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: &FrenziedGoblinResolutionOwnerV1,
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
        return Err("Frenzied red-payment initial removed-item ownership changed".into());
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
        return Err("Frenzied red-payment removed owner lost its definition-owned context".into());
    }
    state.engine.pending_effect = Some(pending);
    record_begin(state)?;
    drive_owned(state, owner)
}

pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: &FrenziedGoblinResolutionOwnerV1,
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

fn history(state: &GameState) -> Result<Vec<&FrenziedGoblinResolutionReceiptV1>, String> {
    let mut receipts: Vec<&FrenziedGoblinResolutionReceiptV1> = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { receipt } = event else {
            continue;
        };
        if usize::try_from(receipt.history_index).ok() != Some(index) {
            return Err(
                "Frenzied red-payment resolution receipt moved in committed history".into(),
            );
        }
        let previous = receipts
            .iter()
            .rev()
            .find(|r| r.stack_item_id == receipt.stack_item_id);
        match previous {
            None => {
                if receipt.ordinal != 0 || receipt.previous_history_index.is_some() {
                    return Err("Frenzied red-payment resolution lost its initial receipt".into());
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
                        "initial Frenzied red-payment receipt is not the generated root".into(),
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
                        "Frenzied red-payment resolution receipt chain changed or replayed".into(),
                    );
                }
            }
        }
        if let Some(pending) = receipt.pending.as_ref() {
            publication_for(state, &pending.resolving_item)?;
            crate::frenzied_goblin_v1::validate_continuation(state, pending)?;
            if pending.resolving_item.v4.stack_item_id != receipt.stack_item_id
                || pending.ctx.stack_item_id != Some(receipt.stack_item_id)
                || pending.ctx.source != receipt.source.source
                || pending.ctx.controller != receipt.source.controller
                || pending.resolving_item.source != receipt.source.source
                || pending.resolving_item.controller != receipt.source.controller
            {
                return Err("Frenzied red-payment receipt source or stack identity changed".into());
            }
        }
        receipts.push(receipt);
    }
    Ok(receipts)
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    validate_with_owner(state, None)?;
    validate_publications(state)?;
    active(state)?;
    Ok(())
}
fn validate_with_owner(
    state: &GameState,
    owner: Option<&FrenziedGoblinResolutionOwnerV1>,
) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { .. }
        )
    }) {
        return Err(
            "Frenzied red-payment receipt cannot enter the public trigger event log".into(),
        );
    }
    validate_publications_with_owner(state, owner)?;
    let receipts = history(state)?;
    // A started live root must retain its independent stack-owned marker.
    // Erasing pending_effect and every progress receipt cannot turn it back
    // into a genuinely unstarted stack item.
    for item in &state.stack {
        let Some(started) = item.v4.frenzied_goblin_started_v1.0 else {
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
            return Err("started Frenzied red-payment root lost its independent authority".into());
        }
    }
    for receipt in &receipts {
        if let Some(pending) = receipt.pending.as_ref() {
            let first = receipts
                .iter()
                .find(|r| r.stack_item_id == receipt.stack_item_id)
                .ok_or("Frenzied red-payment first receipt absent")?;
            let expected = FrenziedGoblinStartedBindingV1 {
                stack_item_id: receipt.stack_item_id,
                source: receipt.source,
                first_receipt_history_index: first.history_index,
            };
            if pending.resolving_item.v4.frenzied_goblin_started_v1.0 != Some(expected) {
                return Err("Frenzied red-payment receipt lost its started stack binding".into());
            }
        }
    }
    let mut latest = Vec::new();
    for receipt in receipts.iter().rev() {
        if !latest
            .iter()
            .any(|r: &&FrenziedGoblinResolutionReceiptV1| r.stack_item_id == receipt.stack_item_id)
        {
            latest.push(*receipt);
        }
    }
    let open: Vec<_> = latest.into_iter().filter(|r| r.pending.is_some()).collect();
    if open.len() > 1 {
        return Err("multiple Frenzied red-payment resolutions claim ownership".into());
    }
    if let Some(receipt) = open.first() {
        if state.engine.pending_effect.as_ref() != receipt.pending.as_deref() {
            return Err(
                "Frenzied red-payment resolution lost or rewrote its committed boundary".into(),
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
            return Err("Frenzied red-payment live and pending start authority differ".into());
        }
        if pending.resolving_item.v4.ability_source_contract != Some(receipt.source) {
            return Err("Frenzied red-payment original source contract changed".into());
        }
        crate::engine::validated_stack_item_target_spec(&pending.resolving_item, state)?;
        u32::try_from(state.engine.event_history.len())
            .map_err(|_| "resolution history exceeds u32")?;
    } else if tracked(state)? {
        return Err("Frenzied red-payment resolution has no open committed boundary".into());
    }
    Ok(())
}

pub(super) fn preflight_begin(
    state: &GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    item: &StackItem,
) -> Result<bool, String> {
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || !item
            .v4
            .ability_source_contract
            .is_some_and(|source| source_owns_root(source, op))
    {
        return Ok(false);
    }
    validate(state)?;
    if !item.v4.frenzied_goblin_started_v1.is_empty() {
        return Err("started Frenzied red-payment stack resolution cannot restart".into());
    }
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
        return Err(
            "Frenzied red-payment begin lost its finalized definition-owned stack item".into(),
        );
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    let receipts = history(state)?;
    if receipts
        .iter()
        .any(|r| r.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Frenzied red-payment stack resolution cannot restart".into());
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
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "resolution history exceeds u32")?;
    let stack_item_id = pending.resolving_item.v4.stack_item_id;
    let source = pending
        .resolving_item
        .v4
        .ability_source_contract
        .ok_or("Frenzied red-payment source contract absent")?;
    let started = FrenziedGoblinStartedBindingV1 {
        stack_item_id,
        source,
        first_receipt_history_index: history_index,
    };
    if pending.resolving_item.v4.frenzied_goblin_started_v1.0 != Some(started) {
        let live = state
            .stack
            .iter_mut()
            .find(|item| item.v4.stack_item_id == stack_item_id)
            .ok_or("Frenzied red-payment initial publication lacks removed-item ownership")?;
        if live != &pending.resolving_item {
            return Err("Frenzied red-payment initial live item changed".into());
        }
        live.v4.frenzied_goblin_started_v1.0 = Some(started);
    }
    let pending = state.engine.pending_effect.as_mut().unwrap();
    pending.resolving_item.v4.frenzied_goblin_started_v1.0 = Some(started);
    let receipt = FrenziedGoblinResolutionReceiptV1 {
        history_index,
        previous_history_index: None,
        ordinal: 0,
        stack_item_id: pending.resolving_item.v4.stack_item_id,
        source: pending
            .resolving_item
            .v4
            .ability_source_contract
            .ok_or("Frenzied red-payment source contract absent")?,
        pending: Some(Box::new(pending.clone())),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { receipt });
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
    let previous = (*previous).clone();
    if state.engine.pending_effect.is_none() {
        record_completion(state, &previous)?;
    }
    let receipt = FrenziedGoblinResolutionReceiptV1 {
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
        .push(CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { receipt });
    Ok(())
}

pub(super) fn drive(state: &mut GameState) -> Result<ResumableProgress, String> {
    // The ordinary wrapper is unchanged for all predecessor programs.
    let owned = tracked(state)?;
    if owned {
        validate(state)?;
    }
    let result = super::squad_rallier_resolution_v1::drive(state);
    if owned && result.is_ok() {
        record_boundary(state)?;
    }
    result
}

fn drive_owned(
    state: &mut GameState,
    owner: &FrenziedGoblinResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    validate_with_owner(state, Some(owner))?;
    let result = super::drive_resumable_inner(state);
    if result.is_ok() {
        record_boundary(state)?;
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrenziedGoblinPublicationRecordV1 {
    pub history_index: u32,
    pub declaration_history_index: u32,
    pub targeting_history_index: u32,
    pub item: Box<StackItem>,
    pub target_at_placement: crate::rl::CardStableRefV1,
    pub target_snapshot: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrenziedGoblinOutcomeV1 {
    Paid,
    Declined,
    Unpayable,
    NoLegalTarget,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrenziedGoblinRestrictionV1 {
    pub publication_history_index: u32,
    pub recipient: crate::rl::CardStableRefV1,
    pub recipient_snapshot: String,
    pub timestamp: u64,
}

fn owns_item(item: &StackItem) -> bool {
    item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.v4.ability_source_contract.is_some_and(|source| {
            item.inline_effect
                .as_ref()
                .is_some_and(|root| source_owns_root(source, root))
        })
}
fn published_at(
    state: &GameState,
    index: u32,
) -> Result<&FrenziedGoblinPublicationRecordV1, String> {
    let Some(CommittedEvent::FrenziedGoblinPublishedV1 { publication }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Frenzied publication absent".into());
    };
    let item = &publication.item;
    let target_snapshot: crate::state::GameObject =
        serde_json::from_str(&publication.target_snapshot)
            .map_err(|_| "Frenzied placement snapshot corrupt")?;
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Frenzied publication source absent")?;
    let (
        [Target::Object(target)],
        [StackTargetContractV4::Object {
            object,
            zone: Zone::Battlefield,
            zone_change_count,
            ..
        }],
    ) = (item.targets.as_slice(), item.v4.target_contracts.as_slice())
    else {
        return Err("Frenzied publication target shape changed".into());
    };
    if publication.history_index != index
        || !owns_item(item)
        || item.v4.frenzied_goblin_publication_v1.0 != Some(index)
        || !item.v4.frenzied_goblin_started_v1.is_empty()
        || item.source != source.source
        || item.controller != source.controller
        || source.zone != Zone::Battlefield
        || item.v4.target_spec != Some(crate::card_def::TargetSpec::Creature)
        || item.v4.stack_item_id == StackItemId::default()
        || item.is_copy
        || item.is_flashback
        || item.madness_offer
        || item.kicked
        || target != object
        || publication.target_at_placement.arena_id != target.0
        || target_snapshot.zone != Zone::Battlefield
        || target_snapshot.zone_change_count != *zone_change_count
        || crate::sower_restriction_capture_v1::masked_ref(*target, &target_snapshot)
            .map_err(|error| error.to_string())?
            != publication.target_at_placement
        || !matches!(item.v4.target_contracts.as_slice(), [StackTargetContractV4::Object { owner, card_def, spell_copy_origin, .. }] if *owner == target_snapshot.owner && *card_def == target_snapshot.card_def && *spell_copy_origin == target_snapshot.spell_copy_origin)
        || publication.declaration_history_index >= index
        || publication.targeting_history_index >= index
        || !matches!(state.engine.event_history.get(publication.declaration_history_index as usize),
            Some(CommittedEvent::DeclaredAttacker { source: attacker, source_zone_change_count, controller })
            if *attacker == source.source && *source_zone_change_count == source.zone_change_count
                && *controller == source.controller)
        || !matches!(state.engine.event_history.get(publication.targeting_history_index as usize),
            Some(CommittedEvent::Targeted { target: actual, target_zone_change_count: generation,
                targeting_stack_item, targeting_controller, .. })
            if actual == target && generation == zone_change_count
                && *targeting_stack_item == item.v4.stack_item_id && *targeting_controller == source.controller)
        || state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event, CommittedEvent::Targeted { targeting_stack_item, .. }
            if *targeting_stack_item == item.v4.stack_item_id)
            })
            .count()
            != 1
    {
        return Err(
            "Frenzied publication changed its printed attack/controller/target authority".into(),
        );
    }
    Ok(publication)
}
fn publication_for<'a>(
    state: &'a GameState,
    item: &StackItem,
) -> Result<&'a FrenziedGoblinPublicationRecordV1, String> {
    let index = item
        .v4
        .frenzied_goblin_publication_v1
        .0
        .ok_or("Frenzied live publication marker absent")?;
    let publication = published_at(state, index)?;
    let mut original = item.clone();
    original.v4.frenzied_goblin_started_v1 = Default::default();
    if original != *publication.item {
        return Err("Frenzied published stack row changed".into());
    }
    Ok(publication)
}
pub(crate) fn publish(state: &mut GameState) -> Result<(), String> {
    let Some(item) = state.stack.last().filter(|item| owns_item(item)).cloned() else {
        return Ok(());
    };
    let source = item.v4.ability_source_contract.unwrap();
    crate::frenzied_goblin_v1::validate_source(state, source.card_def, Some(source))?;
    let declaration = state.engine.event_history.iter().rposition(|event|
        matches!(event, CommittedEvent::DeclaredAttacker { source: attacker, source_zone_change_count, controller }
            if *attacker == source.source && *source_zone_change_count == source.zone_change_count
                && *controller == source.controller)).ok_or("Frenzied actual attack absent")?;
    let targeting = state
        .engine
        .event_history
        .iter()
        .rposition(|event| {
            matches!(event, CommittedEvent::Targeted { targeting_stack_item, .. }
            if *targeting_stack_item == item.v4.stack_item_id)
        })
        .ok_or("Frenzied target occurrence absent")?;
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Frenzied history exceeds u32")?;
    let [Target::Object(target)] = item.targets.as_slice() else {
        return Err("Frenzied placement target absent".into());
    };
    let target_at_placement =
        crate::sower_restriction_capture_v1::masked_ref(*target, state.objects.get(*target))
            .map_err(|error| error.to_string())?;
    let target_snapshot =
        serde_json::to_string(state.objects.get(*target)).map_err(|error| error.to_string())?;
    let live = state.stack.last_mut().unwrap();
    if !live.v4.frenzied_goblin_publication_v1.is_empty() {
        return Err("Frenzied trigger republished".into());
    }
    live.v4.frenzied_goblin_publication_v1.0 = Some(index);
    let publication = FrenziedGoblinPublicationRecordV1 {
        history_index: index,
        declaration_history_index: u32::try_from(declaration)
            .map_err(|_| "declaration exceeds u32")?,
        targeting_history_index: u32::try_from(targeting).map_err(|_| "targeting exceeds u32")?,
        item: Box::new(live.clone()),
        target_at_placement,
        target_snapshot,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinPublishedV1 {
            publication: Box::new(publication),
        });
    published_at(state, index)?;
    Ok(())
}
pub(crate) fn complete_unstarted(
    state: &mut GameState,
    item: &StackItem,
    outcome: FrenziedGoblinOutcomeV1,
) -> Result<(), String> {
    if !owns_item(item) {
        return Ok(());
    }
    let publication = publication_for(state, item)?.history_index;
    if !matches!(
        outcome,
        FrenziedGoblinOutcomeV1::NoLegalTarget | FrenziedGoblinOutcomeV1::Countered
    ) || !item.v4.frenzied_goblin_started_v1.is_empty()
        || state
            .stack
            .iter()
            .any(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|event| {
            matches!(event,
            CommittedEvent::FrenziedGoblinCompletedV1 { publication_history_index,.. }
                if *publication_history_index == publication)
        })
    {
        return Err("Frenzied unstarted closure changed its actual removal".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinRemovalV1 {
            publication_history_index: publication,
            item: Box::new(item.clone()),
            outcome,
        });
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinCompletedV1 {
            publication_history_index: publication,
            outcome,
        });
    Ok(())
}
pub(crate) fn record_payment(state: &mut GameState, ctx: &ExecCtx) -> Result<(), String> {
    if !ctx
        .ability_source_contract
        .is_some_and(|source| crate::frenzied_goblin_v1::is_owned(source.card_def))
    {
        return Ok(());
    }
    let receipts = history(state)?;
    let receipt = receipts
        .iter()
        .rev()
        .find(|receipt| Some(receipt.stack_item_id) == ctx.stack_item_id)
        .ok_or("Frenzied payment has no receipt")?;
    let pending = receipt
        .pending
        .as_ref()
        .ok_or("Frenzied payment receipt closed")?;
    crate::frenzied_goblin_v1::validate_continuation(state, pending)?;
    if pending.ctx != *ctx
        || !matches!(pending.frames.as_slice(), [EffectFrame::PayManaThen { .. }])
        || pending.answered_choice_guard.is_none()
    {
        return Err("Frenzied payment has no authenticated accepted answer".into());
    }
    let publication = publication_for(state, &pending.resolving_item)?.history_index;
    if state.engine.event_history.iter().any(|event| {
        matches!(event,
        CommittedEvent::FrenziedGoblinPaymentV1 { publication_history_index }
            if *publication_history_index == publication)
    }) {
        return Err("Frenzied paid twice".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinPaymentV1 {
            publication_history_index: publication,
        });
    Ok(())
}
pub(crate) fn install(
    state: &mut GameState,
    ctx: &ExecCtx,
    object: ObjectId,
    timestamp: u64,
) -> Result<(), String> {
    if !ctx
        .ability_source_contract
        .is_some_and(|source| crate::frenzied_goblin_v1::is_owned(source.card_def))
    {
        return Ok(());
    }
    let receipts = history(state)?;
    let receipt = receipts
        .iter()
        .rev()
        .find(|receipt| Some(receipt.stack_item_id) == ctx.stack_item_id)
        .ok_or("Frenzied install lacks original payment receipt")?;
    let pending = receipt
        .pending
        .as_ref()
        .ok_or("Frenzied install receipt closed")?;
    let publication = publication_for(state, &pending.resolving_item)?.history_index;
    if pending.ctx != *ctx
        || pending.ctx.targets != [Target::Object(object)]
        || state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::FrenziedGoblinPaymentV1 { publication_history_index }
                if *publication_history_index == publication)
            })
            .count()
            != 1
    {
        return Err("Frenzied installation lacks actual red payment".into());
    }
    let capture = FrenziedGoblinRestrictionV1 {
        publication_history_index: publication,
        recipient: crate::sower_restriction_capture_v1::masked_ref(
            object,
            state.objects.get(object),
        )
        .map_err(|error| error.to_string())?,
        recipient_snapshot: serde_json::to_string(state.objects.get(object))
            .map_err(|error| error.to_string())?,
        timestamp,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinInstalledV1 {
            capture: Box::new(capture),
        });
    Ok(())
}
fn marker_matches(
    effect: &crate::engine::UntilEndOfTurnEffect,
    capture: &FrenziedGoblinRestrictionV1,
) -> bool {
    matches!(effect,crate::engine::UntilEndOfTurnEffect::ResolvedObjectCantBlock {
        object_id,object_zone_change_count,timestamp,duration:crate::engine::EffectDuration::EndOfTurn }
        if object_id.0 == capture.recipient.arena_id && *object_zone_change_count == capture.recipient.zone_change_count
            && *timestamp == capture.timestamp)
}
pub(crate) fn expire(state: &mut GameState) -> Result<(), String> {
    let captures: Vec<_> = active(state)?.into_iter().cloned().collect();
    if captures.is_empty() {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Frenzied expiry requires actual Cleanup entry".into());
    }
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Frenzied history exceeds u32")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinCleanupV1 {
            history_index: index,
            turn: state.turn,
            ending_player: state.active_player,
            step: state.step,
            captures: captures.clone(),
        });
    for capture in captures {
        state
            .engine
            .event_history
            .push(CommittedEvent::FrenziedGoblinExpiredV1 {
                capture: Box::new(capture),
                cleanup_history_index: index,
            });
    }
    Ok(())
}
pub(crate) fn active(state: &GameState) -> Result<Vec<&FrenziedGoblinRestrictionV1>, String> {
    let mut installs: Vec<&FrenziedGoblinRestrictionV1> = Vec::new();
    let mut expired = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::FrenziedGoblinInstalledV1 { capture } => {
                let publication = published_at(state, capture.publication_history_index)?;
                let object: crate::state::GameObject =
                    serde_json::from_str(&capture.recipient_snapshot)
                        .map_err(|_| "Frenzied recipient snapshot corrupt")?;
                let [StackTargetContractV4::Object {
                    object: id,
                    owner,
                    zone: Zone::Battlefield,
                    zone_change_count,
                    ..
                }] = publication.item.v4.target_contracts.as_slice()
                else {
                    return Err("Frenzied install target contract changed".into());
                };
                let payments:Vec<_>=state.engine.event_history.iter().enumerate().filter_map(|(ordinal,event)|
                    matches!(event,CommittedEvent::FrenziedGoblinPaymentV1 {publication_history_index}
                        if *publication_history_index == capture.publication_history_index).then_some(ordinal)).collect();
                if payments.len() != 1
                    || payments[0] >= index
                    || publication.history_index as usize >= index
                    || capture.recipient.arena_id != id.0
                    || object.owner != *owner
                    || object.zone != Zone::Battlefield
                    || object.zone_change_count != *zone_change_count
                    || crate::sower_restriction_capture_v1::masked_ref(*id, &object)
                        .map_err(|error| error.to_string())?
                        != capture.recipient
                    || capture.timestamp >= state.engine.next_effect_timestamp
                    || installs.iter().any(|old| {
                        old.timestamp == capture.timestamp
                            || old.publication_history_index == capture.publication_history_index
                    })
                {
                    return Err("Frenzied paid installation binding changed".into());
                }
                installs.push(capture);
            }
            CommittedEvent::FrenziedGoblinCleanupV1 {
                history_index,
                step,
                captures,
                ..
            } => {
                let remaining: Vec<_> = installs
                    .iter()
                    .copied()
                    .filter(|capture| !expired.contains(capture))
                    .collect();
                if *history_index as usize != index
                    || *step != crate::state::Step::Cleanup
                    || captures.is_empty()
                    || captures.iter().collect::<Vec<_>>() != remaining
                {
                    return Err(
                        "Frenzied Cleanup batch lost its complete active installations".into(),
                    );
                }
                for (offset, capture) in captures.iter().enumerate() {
                    if !matches!(state.engine.event_history.get(index+1+offset),
                        Some(CommittedEvent::FrenziedGoblinExpiredV1 {capture:expired,cleanup_history_index})
                        if expired.as_ref()==capture && *cleanup_history_index==*history_index)
                    {
                        return Err("Frenzied Cleanup batch lost an ordered expiry".into());
                    }
                }
            }
            CommittedEvent::FrenziedGoblinExpiredV1 {
                capture,
                cleanup_history_index,
            } => {
                let Some(CommittedEvent::FrenziedGoblinCleanupV1 {
                    history_index,
                    step,
                    captures,
                    ..
                }) = state
                    .engine
                    .event_history
                    .get(*cleanup_history_index as usize)
                else {
                    return Err("Frenzied expiry lacks independent Cleanup authority".into());
                };
                if *history_index != *cleanup_history_index
                    || *step != crate::state::Step::Cleanup
                    || captures
                        .iter()
                        .position(|row| row == capture.as_ref())
                        .is_none_or(|offset| index != *cleanup_history_index as usize + 1 + offset)
                {
                    return Err("Frenzied expiry changed its genuine Cleanup occurrence".into());
                }
                if !installs.contains(&capture.as_ref()) || expired.contains(&capture.as_ref()) {
                    return Err("Frenzied expiry lost exact installation".into());
                }
                expired.push(capture.as_ref());
            }
            _ => {}
        }
    }
    let mut active = Vec::new();
    for capture in installs {
        let count = state
            .engine
            .until_end_of_turn
            .iter()
            .filter(|effect| marker_matches(effect, capture))
            .count();
        if count != usize::from(!expired.contains(&capture)) {
            return Err("Frenzied marker erased or resurrected".into());
        }
        if count == 1 {
            active.push(capture);
        }
    }
    Ok(active)
}
fn validate_publications(state: &GameState) -> Result<(), String> {
    validate_publications_with_owner(state, None)
}
fn validate_publications_with_owner(
    state: &GameState,
    owner: Option<&FrenziedGoblinResolutionOwnerV1>,
) -> Result<(), String> {
    let receipts = history(state)?;
    for item in &state.stack {
        if owns_item(item)
            || !item.v4.frenzied_goblin_publication_v1.is_empty()
            || !item.v4.frenzied_goblin_started_v1.is_empty()
        {
            publication_for(state, item)?;
        }
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::FrenziedGoblinPublishedV1 { .. }
                | CommittedEvent::FrenziedGoblinPaymentV1 { .. }
                | CommittedEvent::FrenziedGoblinCompletedV1 { .. }
                | CommittedEvent::FrenziedGoblinInstalledV1 { .. }
                | CommittedEvent::FrenziedGoblinExpiredV1 { .. }
                | CommittedEvent::FrenziedGoblinRemovalV1 { .. }
                | CommittedEvent::FrenziedGoblinCleanupV1 { .. }
        )
    }) {
        return Err("Frenzied journal replayed into dispatch log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::FrenziedGoblinPublishedV1 { publication } = event else {
            continue;
        };
        if publication.history_index as usize != index {
            return Err("Frenzied publication occurrence moved".into());
        }
        published_at(state, publication.history_index)?;
        let id = publication.item.v4.stack_item_id;
        if state.engine.event_history[..index].iter().any(|event|matches!(event,
            CommittedEvent::FrenziedGoblinPublishedV1 {publication:previous} if previous.item.v4.stack_item_id==id)) {
            return Err("Frenzied duplicate publication".into());
        }
        let completed: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(ordinal, event)| match event {
                CommittedEvent::FrenziedGoblinCompletedV1 {
                    publication_history_index,
                    outcome,
                } if *publication_history_index == publication.history_index => {
                    Some((ordinal, *outcome))
                }
                _ => None,
            })
            .collect();
        let payments: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(ordinal, event)| {
                matches!(event,CommittedEvent::FrenziedGoblinPaymentV1 {publication_history_index}
                if *publication_history_index==publication.history_index)
                .then_some(ordinal)
            })
            .collect();
        let removals: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(ordinal, event)| match event {
                CommittedEvent::FrenziedGoblinRemovalV1 {
                    publication_history_index,
                    item,
                    outcome,
                } if *publication_history_index == publication.history_index => {
                    Some((ordinal, item.as_ref(), *outcome))
                }
                _ => None,
            })
            .collect();
        if removals.len() > 1 {
            return Err("Frenzied actual removal repeated".into());
        }
        if let Some((ordinal, item, outcome)) = removals.first() {
            if *ordinal <= index
                || *item != publication.item.as_ref()
                || !matches!(
                    outcome,
                    FrenziedGoblinOutcomeV1::NoLegalTarget | FrenziedGoblinOutcomeV1::Countered
                )
                || completed
                    .first()
                    .is_none_or(|(end, closed)| *end != ordinal + 1 || closed != outcome)
            {
                return Err("Frenzied closure lost its independent actual removal".into());
            }
        }
        let installations: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(ordinal, event)| {
                matches!(event,CommittedEvent::FrenziedGoblinInstalledV1 {capture}
                if capture.publication_history_index==publication.history_index)
                .then_some(ordinal)
            })
            .collect();
        let last = receipts
            .iter()
            .rev()
            .find(|receipt| receipt.stack_item_id == id);
        let live: Vec<_> = state
            .stack
            .iter()
            .filter(|item| item.v4.stack_item_id == id)
            .collect();
        if completed.len() > 1
            || payments.len() > 1
            || completed
                .first()
                .is_some_and(|(ordinal, _)| *ordinal <= index)
            || payments.first().is_some_and(|ordinal| *ordinal <= index)
        {
            return Err("Frenzied operation journal repeats or precedes publication".into());
        }
        if let Some((end, outcome)) = completed.first() {
            if !live.is_empty()
                || owner.is_some_and(|owner| owner.expected.v4.stack_item_id == id)
                || last.is_some_and(|receipt| receipt.pending.is_some())
                || (*outcome == FrenziedGoblinOutcomeV1::Paid) != (payments.len() == 1)
                || payments.first().is_some_and(|ordinal| ordinal >= end)
                || matches!(
                    outcome,
                    FrenziedGoblinOutcomeV1::NoLegalTarget | FrenziedGoblinOutcomeV1::Countered
                ) != last.is_none()
                || matches!(
                    outcome,
                    FrenziedGoblinOutcomeV1::NoLegalTarget | FrenziedGoblinOutcomeV1::Countered
                ) != (removals.len() == 1)
                || last.is_some_and(|receipt| receipt.history_index as usize != end + 1)
                || (*outcome == FrenziedGoblinOutcomeV1::Paid) != (installations.len() == 1)
                || installations.iter().any(|ordinal| {
                    *ordinal >= *end || payments.first().is_none_or(|paid| *ordinal <= *paid)
                })
            {
                return Err("Frenzied closed resolution revived or changed operation".into());
            }
        } else {
            for item in &live {
                publication_for(state, item)?;
            }
            if let Some(removed) = owner.filter(|owner| owner.expected.v4.stack_item_id == id) {
                publication_for(state, &removed.expected)?;
            }
            let removed = owner.is_some_and(|owner| owner.expected.v4.stack_item_id == id);
            if live.len() + usize::from(removed) != 1
                || last.is_some_and(|receipt| receipt.pending.is_none())
            {
                return Err("Frenzied live/publication/continuation ownership erased".into());
            }
        }
    }
    for event in &state.engine.event_history {
        let index = match event {
            CommittedEvent::FrenziedGoblinPaymentV1 {
                publication_history_index,
            }
            | CommittedEvent::FrenziedGoblinCompletedV1 {
                publication_history_index,
                ..
            }
            | CommittedEvent::FrenziedGoblinRemovalV1 {
                publication_history_index,
                ..
            } => Some(*publication_history_index),
            _ => None,
        };
        if let Some(index) = index {
            published_at(state, index)?;
        }
    }
    Ok(())
}
fn record_completion(
    state: &mut GameState,
    previous: &FrenziedGoblinResolutionReceiptV1,
) -> Result<(), String> {
    let pending = previous
        .pending
        .as_ref()
        .ok_or("Frenzied completion already closed")?;
    let publication = publication_for(state, &pending.resolving_item)?.history_index;
    let paid=state.engine.event_history.iter().any(|event|matches!(event,
        CommittedEvent::FrenziedGoblinPaymentV1 {publication_history_index} if *publication_history_index==publication));
    let outcome = if paid {
        FrenziedGoblinOutcomeV1::Paid
    } else if pending.frames.is_empty() {
        FrenziedGoblinOutcomeV1::Declined
    } else {
        FrenziedGoblinOutcomeV1::Unpayable
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::FrenziedGoblinCompletedV1 {
            publication_history_index: publication,
            outcome,
        });
    Ok(())
}

pub(crate) fn preflight_unstarted_removal(
    state: &GameState,
    item: &StackItem,
) -> Result<(), String> {
    if !owns_item(item) {
        return Ok(());
    }
    validate(state)?;
    publication_for(state, item)?;
    if !item.v4.frenzied_goblin_started_v1.is_empty() {
        return Err(
            "a suspended Frenzied resolution cannot be countered or fizzled as an unstarted root"
                .into(),
        );
    }
    u32::try_from(state.engine.event_history.len() + 2)
        .map_err(|_| "Frenzied history exceeds u32")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::ids::PlayerId;
    #[test]
    fn absent_registered_roots_preserve_state_bytes_hash_and_empty_wrapper_hash_input() {
        use std::hash::{Hash, Hasher};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let elf = card_id_by_name("Llanowar Elves").unwrap();
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[elf; 8],
                &[elf; 8],
                |_| "elf".into(),
                115_150,
                actor,
            );
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.state_hash();
            validate(&state).unwrap();
            publish(&mut state).unwrap();
            expire(&mut state).unwrap();
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert_eq!(state.state_hash(), hash);
            let mut expected = std::collections::hash_map::DefaultHasher::new();
            let mut actual = std::collections::hash_map::DefaultHasher::new();
            119_u32.hash(&mut expected);
            119_u32.hash(&mut actual);
            crate::state::FrenziedGoblinStartedV1::default().hash(&mut actual);
            crate::state::FrenziedGoblinPublicationV1::default().hash(&mut actual);
            assert_eq!(actual.finish(), expected.finish());
            let json = serde_json::to_value(crate::state::StackStateV4::default()).unwrap();
            assert!(json.get("frenzied_goblin_started_v1").is_none());
            assert!(json.get("frenzied_goblin_publication_v1").is_none());
        }
    }
}
