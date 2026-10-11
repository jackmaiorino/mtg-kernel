//! Committed ownership of the bounded look-four optional-reveal random-bottom lifecycle.
//! Predecessor programs emit no receipts and retain their existing behavior.
use super::*;
use crate::card_def::CARD_DEFS;
use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, SquadRallierStartedBindingV1};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SquadRallierResolutionReceiptV1 {
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
    super::private_keep_one_v1::is_recipe(op)
        || matches!(op, EffectOp::LookTopSelectV1 { player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(4), rule, pick_x: false }
        if *rule == LibraryPickRule {
            pick: 1, choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::CreaturePowerAtMost(2),
                optional: true, destination: Zone::Hand, reveal_selected: true,
            }),
        })
}
fn source_owns_root(source: AbilitySourceContractV4, root: &EffectOp) -> bool {
    CARD_DEFS
        .get(usize::from(source.card_def))
        .is_some_and(|definition| {
            (definition.name == "Squad Rallier"
                && definition
                    .activated_abilities
                    .iter()
                    .any(|ability| (ability.effect)() == *root)
                && !super::private_keep_one_v1::is_recipe(root)
                && owned_root(root))
                || (definition.name == "Gutless Plunderer"
                    && super::private_keep_one_v1::is_recipe(root)
                    && crate::trigger::triggers_for(source.card_def)
                        .iter()
                        .any(|ability| (ability.effect)() == *root))
        })
}
fn kind_owns(kind: crate::state::StackItemKind, root: Option<&EffectOp>) -> bool {
    match kind {
        crate::state::StackItemKind::ActivatedAbility => {
            root.is_some_and(|root| !super::private_keep_one_v1::is_recipe(root))
        }
        crate::state::StackItemKind::TriggeredAbility => {
            root.is_some_and(super::private_keep_one_v1::is_recipe)
        }
        _ => false,
    }
}
/// A synchronous, nonserializable loan of an actually indexed stack item.
/// Only this module constructs it, before engine removal; external restore
/// validation cannot manufacture an absent-live resolution from pending data.
pub(crate) struct SquadRallierResolutionOwnerV1 {
    original: StackItem,
    expected: StackItem,
    initial: bool,
}
impl SquadRallierResolutionOwnerV1 {
    pub(crate) fn completion_matches(&self, item: &StackItem) -> bool {
        *item == self.expected
    }
}
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
    initial: bool,
) -> Result<Option<SquadRallierResolutionOwnerV1>, String> {
    let owns = kind_owns(item.kind, item.inline_effect.as_ref())
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
        return Err("paid look owner did not acquire the genuine live stack item".into());
    }
    let mut expected = item.clone();
    if initial {
        if state.engine.pending_effect.is_some()
            || !item.v4.squad_rallier_started_v1.is_empty()
            || history(state)?
                .iter()
                .any(|r| r.stack_item_id == item.v4.stack_item_id)
        {
            return Err("paid look initial owner cannot restart".into());
        }
        expected.v4.squad_rallier_started_v1.0 = Some(SquadRallierStartedBindingV1 {
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
        return Err("paid look resumed owner changed its published item".into());
    }
    Ok(Some(SquadRallierResolutionOwnerV1 {
        original: item.clone(),
        expected,
        initial,
    }))
}

pub(crate) fn begin_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: &SquadRallierResolutionOwnerV1,
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
        return Err("paid look initial removed-item ownership changed".into());
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
        return Err("paid look removed owner lost its definition-owned context".into());
    }
    state.engine.pending_effect = Some(pending);
    record_begin(state)?;
    drive_owned(state, owner)
}

pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: &SquadRallierResolutionOwnerV1,
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
    Ok(kind_owns(
        pending.resolving_item.kind,
        pending.resolving_item.inline_effect.as_ref(),
    ) && pending
        .resolving_item
        .v4
        .ability_source_contract
        .is_some_and(|source| {
            pending
                .resolving_item
                .inline_effect
                .as_ref()
                .is_some_and(|root| source_owns_root(source, root))
        }))
}

fn history(state: &GameState) -> Result<Vec<&SquadRallierResolutionReceiptV1>, String> {
    let mut receipts: Vec<&SquadRallierResolutionReceiptV1> = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt } = event else {
            continue;
        };
        if usize::try_from(receipt.history_index).ok() != Some(index) {
            return Err("paid look resolution receipt moved in committed history".into());
        }
        let previous = receipts
            .iter()
            .rev()
            .find(|r| r.stack_item_id == receipt.stack_item_id);
        match previous {
            None => {
                if receipt.ordinal != 0 || receipt.previous_history_index.is_some() {
                    return Err("paid look resolution lost its initial receipt".into());
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
                    return Err("initial paid look receipt is not the generated root".into());
                }
            }
            Some(previous) => {
                if previous.pending.is_none()
                    || previous.source != receipt.source
                    || previous.ordinal.checked_add(1) != Some(receipt.ordinal)
                    || receipt.previous_history_index != Some(previous.history_index)
                {
                    return Err("paid look resolution receipt chain changed or replayed".into());
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
                return Err("paid look receipt source or stack identity changed".into());
            }
        }
        receipts.push(receipt);
    }
    Ok(receipts)
}

pub(super) fn validate(state: &GameState) -> Result<(), String> {
    validate_with_owner(state, None)
}
fn validate_with_owner(
    state: &GameState,
    owner: Option<&SquadRallierResolutionOwnerV1>,
) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
        )
    }) {
        return Err("paid look receipt cannot enter the public trigger event log".into());
    }
    let receipts = history(state)?;
    // A started live root must retain its independent stack-owned marker.
    // Erasing pending_effect and every progress receipt cannot turn it back
    // into a genuinely unstarted stack item.
    for item in &state.stack {
        let Some(started) = item.v4.squad_rallier_started_v1.0 else {
            continue;
        };
        if started.stack_item_id != item.v4.stack_item_id
            || !kind_owns(item.kind, item.inline_effect.as_ref())
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
            return Err("started paid look root lost its independent authority".into());
        }
    }
    for receipt in &receipts {
        if let Some(pending) = receipt.pending.as_ref() {
            let first = receipts
                .iter()
                .find(|r| r.stack_item_id == receipt.stack_item_id)
                .ok_or("paid look first receipt absent")?;
            let expected = SquadRallierStartedBindingV1 {
                stack_item_id: receipt.stack_item_id,
                source: receipt.source,
                first_receipt_history_index: first.history_index,
            };
            if pending.resolving_item.v4.squad_rallier_started_v1.0 != Some(expected) {
                return Err("paid look receipt lost its started stack binding".into());
            }
        }
    }
    let mut latest = Vec::new();
    for receipt in receipts.iter().rev() {
        if !latest
            .iter()
            .any(|r: &&SquadRallierResolutionReceiptV1| r.stack_item_id == receipt.stack_item_id)
        {
            latest.push(*receipt);
        }
    }
    let open: Vec<_> = latest.into_iter().filter(|r| r.pending.is_some()).collect();
    if open.len() > 1 {
        return Err("multiple paid look resolutions claim ownership".into());
    }
    if let Some(receipt) = open.first() {
        if state.engine.pending_effect.as_ref() != receipt.pending.as_deref() {
            return Err("paid look resolution lost or rewrote its committed boundary".into());
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
            return Err("paid look live and pending start authority differ".into());
        }
        if pending.resolving_item.v4.ability_source_contract != Some(receipt.source) {
            return Err("paid look original source contract changed".into());
        }
        crate::engine::validated_stack_item_target_spec(&pending.resolving_item, state)?;
        u32::try_from(state.engine.event_history.len())
            .map_err(|_| "resolution history exceeds u32")?;
    } else if tracked(state)? {
        return Err("paid look resolution has no open committed boundary".into());
    }
    Ok(())
}

pub(super) fn preflight_begin(
    state: &GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    item: &StackItem,
) -> Result<bool, String> {
    if !kind_owns(item.kind, item.inline_effect.as_ref())
        || !item
            .v4
            .ability_source_contract
            .is_some_and(|source| source_owns_root(source, op))
    {
        return Ok(false);
    }
    validate(state)?;
    if !item.v4.squad_rallier_started_v1.is_empty() {
        return Err("started paid look stack resolution cannot restart".into());
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
        return Err("paid look begin lost its finalized definition-owned stack item".into());
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    let receipts = history(state)?;
    if receipts
        .iter()
        .any(|r| r.stack_item_id == item.v4.stack_item_id)
    {
        return Err("paid look stack resolution cannot restart".into());
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
        .ok_or("paid look source contract absent")?;
    let started = SquadRallierStartedBindingV1 {
        stack_item_id,
        source,
        first_receipt_history_index: history_index,
    };
    if pending.resolving_item.v4.squad_rallier_started_v1.0 != Some(started) {
        let live = state
            .stack
            .iter_mut()
            .find(|item| item.v4.stack_item_id == stack_item_id)
            .ok_or("paid look initial publication lacks removed-item ownership")?;
        if live != &pending.resolving_item {
            return Err("paid look initial live item changed".into());
        }
        live.v4.squad_rallier_started_v1.0 = Some(started);
    }
    let pending = state.engine.pending_effect.as_mut().unwrap();
    pending.resolving_item.v4.squad_rallier_started_v1.0 = Some(started);
    let receipt = SquadRallierResolutionReceiptV1 {
        history_index,
        previous_history_index: None,
        ordinal: 0,
        stack_item_id: pending.resolving_item.v4.stack_item_id,
        source: pending
            .resolving_item
            .v4
            .ability_source_contract
            .ok_or("paid look source contract absent")?,
        pending: Some(Box::new(pending.clone())),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt });
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
    let receipt = SquadRallierResolutionReceiptV1 {
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
        .push(CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt });
    Ok(())
}

pub(super) fn drive(state: &mut GameState) -> Result<ResumableProgress, String> {
    // The ordinary wrapper is unchanged for all predecessor programs.
    let owned = tracked(state)?;
    if owned {
        validate(state)?;
    }
    let result = super::drive_resumable_inner(state);
    if owned && result.is_ok() {
        record_boundary(state)?;
    }
    result
}

fn drive_owned(
    state: &mut GameState,
    owner: &SquadRallierResolutionOwnerV1,
) -> Result<ResumableProgress, String> {
    validate_with_owner(state, Some(owner))?;
    let result = super::drive_resumable_inner(state);
    if result.is_ok() {
        record_boundary(state)?;
    }
    result
}

#[cfg(test)]
#[path = "squad_rallier_legacy_tests.rs"]
mod legacy_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::ids::PlayerId;

    fn library(actor: PlayerId, names: &[&str]) -> GameState {
        let cards: Vec<_> = names
            .iter()
            .map(|name| card_id_by_name(name).unwrap())
            .collect();
        GameState::new_from_libraries_with_starting_player_v1(
            if actor == PlayerId::P0 { &cards } else { &[] },
            if actor == PlayerId::P1 { &cards } else { &[] },
            |id| CARD_DEFS[id as usize].object_name.into(),
            110_330,
            actor,
        )
    }
    fn filter() -> LibraryPartitionFilter {
        LibraryPartitionFilter::Pick(LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::CreaturePowerAtMost(2),
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        })
    }
    #[test]
    fn registered_library_power_filter_uses_power_not_value_and_keeps_physical_prefix() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = library(
                actor,
                &[
                    "Burning-Tree Emissary",
                    "Llanowar Elves",
                    "Tolarian Terror",
                    "Forest",
                    "Counterspell",
                ],
            );
            let prefix = bind_library_top(&state, actor, 4);
            validate_library_partition_live_metadata(&state, actor, 4, 5, filter(), &prefix)
                .unwrap();
            let before = serde_json::to_vec(&state).unwrap();
            assert_eq!(
                library_partition_matching_prefix(&state, filter(), &prefix).unwrap(),
                prefix[..2]
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert!(state.engine.event_history.iter().all(|event| !matches!(
                event,
                CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
            )));
            let mut restored: GameState = serde_json::from_slice(&before).unwrap();
            restored.players[actor.index()].library.swap(0, 1);
            assert!(validate_library_partition_live_metadata(
                &restored,
                actor,
                4,
                5,
                filter(),
                &prefix
            )
            .is_err());
            let mut restored: GameState = serde_json::from_slice(&before).unwrap();
            restored.objects.get_mut(prefix[0].object).zone_change_count += 1;
            assert!(validate_library_partition_live_metadata(
                &restored,
                actor,
                4,
                5,
                filter(),
                &prefix
            )
            .is_err());
            let mut restored: GameState = serde_json::from_slice(&before).unwrap();
            restored.objects.get_mut(prefix[0].object).v4.face_index = 1;
            assert!(library_partition_matching_prefix(&restored, filter(), &prefix).is_err());
        }
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    #[test]
    fn accepted_standard_library_characteristic_defining_power_is_rechecked() {
        use crate::event::{self, ProposedEvent};
        use crate::policy_observation_v6::tests::put;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = library(actor, &["Haughty Djinn", "Forest", "Forest", "Forest"]);
            let prefix = bind_library_top(&state, actor, 4);
            assert_eq!(
                library_partition_matching_prefix(&state, filter(), &prefix).unwrap(),
                prefix[..1]
            );
            for _ in 0..3 {
                let spell = put(&mut state, actor, "Counterspell", Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(spell, Zone::Graveyard),
                );
            }
            assert_eq!(crate::engine::effective_power(&state, prefix[0].object), 3);
            assert!(library_partition_matching_prefix(&state, filter(), &prefix)
                .unwrap()
                .is_empty());
        }
    }
}
