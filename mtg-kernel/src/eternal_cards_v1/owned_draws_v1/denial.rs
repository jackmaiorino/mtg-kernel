//! The original paid Arcane Denial owns both next-turn delayed instructions.
use super::{
    has_started, record, start_instruction, InstructionV1, JournalV1 as DrawJournal, SourceV1,
    StartedV1,
};
use crate::card_def::CARD_DEFS;
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, EffectOptionChoicePurpose, PendingEffectChoice,
    PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_e9b_v1::{self as e9b, E9bOpV1};
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackSourceContractV4,
    StackStateV4, StackTargetContractV4, Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Paid {
        item: StackItemId,
        source: StackSourceContractV4,
        target: StackTargetContractV4,
        stack_sha256: [u8; 32],
    },
    Target {
        paid_at: usize,
        item: StackItemId,
        source: StackSourceContractV4,
        turn: TurnV1,
    },
    Scheduled {
        paid_at: usize,
        target_at: usize,
        source: StackSourceContractV4,
        drawer: PlayerId,
        turn: TurnV1,
    },
    Queued {
        scheduled_at: usize,
        upkeep_at: usize,
        turn: TurnV1,
        optional: bool,
    },
    Published {
        queued_at: usize,
        item: StackItemId,
        stack_sha256: [u8; 32],
    },
    Answered {
        queued_at: usize,
        item: StackItemId,
        count: u8,
    },
    DrawStarted {
        queued_at: usize,
        item: StackItemId,
        count: u8,
    },
    Rebound {
        paid_at: usize,
        previous_at: usize,
        item: StackItemId,
        source: StackSourceContractV4,
        target: StackTargetContractV4,
        stack_sha256: [u8; 32],
    },
}
fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else {
        return None;
    };
    let DrawJournal::DenialV1(row) = **journal else {
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
        .filter_map(|(at, _)| journal(state, at).map(|row| (at, row)))
}
fn note(state: &mut GameState, row: JournalV1) -> usize {
    let at = state.engine.event_history.len();
    record(state, DrawJournal::DenialV1(row));
    at
}
fn digest(item: &StackItem) -> Option<[u8; 32]> {
    Some(Sha256::digest(serde_json::to_vec(item).ok()?).into())
}
fn leaf(queued_at: usize, count: u8) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9bV1(
        E9bOpV1::DenialDrawBoundV1 { queued_at, count },
    )))
}
fn leaf_binding(effect: &EffectOp) -> Option<(usize, u8)> {
    if let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9bV1(
        E9bOpV1::DenialDrawBoundV1 { queued_at, count },
    ))) = effect
    {
        Some((*queued_at, *count))
    } else {
        None
    }
}
pub(crate) fn bound(effect: &EffectOp) -> Option<usize> {
    if let Some((at, 1)) = leaf_binding(effect) {
        return Some(at);
    }
    if let EffectOp::Choice { options, .. } = effect {
        let (at, 0) = leaf_binding(options.first()?)? else {
            return None;
        };
        if *options == (0..=2).map(|count| leaf(at, count)).collect::<Vec<_>>() {
            return Some(at);
        }
    }
    None
}
fn cast_source(item: &StackItem) -> Option<StackSourceContractV4> {
    let source = item.v4.source_contract?;
    (cfg!(feature = "eternal-fixtures")
        && item.kind == StackItemKind::Spell
        && item.inline_effect.is_none()
        && source.source == item.source
        && source.controller == item.controller
        && source.zone == Zone::Stack
        && item.v4.cast_method == Some(source.cast_method)
        && item.targets.len() == 1
        && item.v4.target_contracts.len() == 1
        && CARD_DEFS.get(usize::from(source.card_def))?.object_name == "Arcane Denial"
        && CARD_DEFS[usize::from(source.card_def)].printed_mode_effect(item.mode_chosen)
            == Some(e9b::arcane_denial()))
    .then_some(source)
}
fn original_context(pending: &EffectContinuation) -> bool {
    let row = &pending.resolving_item;
    let ctx = &pending.ctx;
    ctx.source == row.source
        && ctx.controller == row.controller
        && ctx.stack_item_id == Some(row.v4.stack_item_id)
        && ctx.targets == row.targets
        && ctx.target_contracts == row.v4.target_contracts
        && ctx.discarded == row.discarded
        && ctx.paid_cost_refs == row.v4.paid_cost_refs
        && ctx.ability_source_contract == row.v4.ability_source_contract
        && ctx.hidden_ability_source == row.v4.hidden_ability_source
        && ctx.cauldron_grant == row.v4.cauldron_grant
        && ctx.kicked == row.kicked
        && ctx.optional_additional_cost_paid == row.v4.optional_additional_cost_paid
        && ctx.x_value == row.v4.x_value
}
pub(crate) fn record_cast(state: &mut GameState, item: StackItemId) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let row = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == item)
        .ok_or("Denial cast lost its row")?;
    let Some(source) = cast_source(row).filter(|_| !row.is_copy) else {
        return Ok(());
    };
    if source.spell_copy_origin.is_some()
        || source
            .spell_cast_origin
            .is_none_or(|origin| origin.finalized_method != Some(source.cast_method))
        || StackSourceContractV4::capture(state, row.source, source.cast_method) != source
    {
        return Err("Denial cast lost its finalized origin".into());
    }
    let target = row.v4.target_contracts[0];
    let stack_sha256 = digest(row).ok_or("Denial could not bind its actual paid row")?;
    note(
        state,
        JournalV1::Paid {
            item,
            source,
            target,
            stack_sha256,
        },
    );
    Ok(())
}
fn binding(state: &GameState, row: &StackItem) -> Option<(usize, usize)> {
    let source = cast_source(row)?;
    if row.is_copy {
        return None;
    }
    let mut receipts = rows(state).filter_map(|(at, entry)| match entry {
        JournalV1::Paid {
            item,
            source: original,
            target,
            stack_sha256,
        } if item == row.v4.stack_item_id => Some((at, original, target, stack_sha256)),
        _ => None,
    });
    let (paid_at, mut original, mut target, mut hash) = receipts.next()?;
    if receipts.next().is_some() {
        return None;
    }
    let mut current_at = paid_at;
    for (at, entry) in rows(state) {
        if let JournalV1::Rebound {
            paid_at: paid,
            previous_at,
            item,
            source: next,
            target: next_target,
            stack_sha256,
        } = entry
        {
            if paid != paid_at {
                continue;
            }
            if previous_at != current_at || current_at >= at || item != row.v4.stack_item_id {
                return None;
            }
            let mut normalized = next;
            normalized.controller = original.controller;
            if normalized != original {
                return None;
            }
            original = next;
            target = next_target;
            hash = stack_sha256;
            current_at = at;
        }
    }
    (original == source && row.v4.target_contracts == [target] && digest(row) == Some(hash))
        .then_some((paid_at, current_at))
}
fn paid(state: &GameState, pending: &EffectContinuation) -> Option<(usize, StackSourceContractV4)> {
    let row = &pending.resolving_item;
    let source = cast_source(row)?;
    if !original_context(pending)
        || StackSourceContractV4::capture(state, row.source, source.cast_method) != source
    {
        return None;
    }
    let (_, current_at) = binding(state, row)?;
    Some((current_at, source))
}
/// Only the native control and retarget mutation endpoints call this, with
/// their pre-mutation row. A caller-authored continuation cannot renew payment.
pub(crate) fn record_rebind(state: &mut GameState, before: &StackItem) -> Result<(), String> {
    if before.is_copy || cast_source(before).is_none() {
        return Ok(());
    }
    let (paid_at, previous_at) =
        binding(state, before).ok_or("Denial mutation lost its original payment")?;
    let row = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == before.v4.stack_item_id)
        .ok_or("Denial mutation lost its row")?;
    let source = cast_source(row).ok_or("Denial mutation changed its recipe")?;
    let mut normalized = row.clone();
    normalized.controller = before.controller;
    normalized.v4.source_contract.as_mut().unwrap().controller = before.controller;
    normalized.targets.clone_from(&before.targets);
    normalized
        .v4
        .target_contracts
        .clone_from(&before.v4.target_contracts);
    if normalized != *before
        || StackSourceContractV4::capture(state, row.source, source.cast_method) != source
    {
        return Err("Denial mutation changed its original physical source".into());
    }
    let item = row.v4.stack_item_id;
    let target = row.v4.target_contracts[0];
    let stack_sha256 = digest(row).ok_or("Denial mutation could not bind its row")?;
    note(
        state,
        JournalV1::Rebound {
            paid_at,
            previous_at,
            item,
            source,
            target,
            stack_sha256,
        },
    );
    Ok(())
}
fn target_matches(
    target: StackTargetContractV4,
    item: StackItemId,
    source: StackSourceContractV4,
) -> bool {
    match target {
        StackTargetContractV4::Object {
            object,
            card_def,
            owner,
            zone: Zone::Stack,
            zone_change_count,
            spell_copy_origin,
            ..
        } => {
            object == source.source
                && card_def == source.card_def
                && owner == source.owner
                && zone_change_count == source.zone_change_count
                && spell_copy_origin == source.spell_copy_origin
        }
        StackTargetContractV4::StackItem {
            stack_item_id,
            source: object,
            card_def,
            owner,
            source_zone: Zone::Stack,
            source_zone_change_count,
            kind: StackItemKind::Spell,
            ..
        } => {
            stack_item_id == item
                && object == source.source
                && card_def == source.card_def
                && owner == source.owner
                && source_zone_change_count == source.zone_change_count
        }
        _ => false,
    }
}
pub(crate) fn resolve(
    state: &mut GameState,
    pending: &EffectContinuation,
    path: &[u16],
) -> Result<(), String> {
    if !path.is_empty()
        || !pending.frames.is_empty()
        || state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == pending.resolving_item.v4.stack_item_id)
    {
        return Err("Denial changed its original resolution root".into());
    }
    if pending.resolving_item.is_copy {
        let source =
            cast_source(&pending.resolving_item).ok_or("Denial copy lost its printed recipe")?;
        if !original_context(pending)
            || source.spell_copy_origin.is_none()
            || source.spell_cast_origin.is_some()
            || StackSourceContractV4::capture(state, source.source, source.cast_method) != source
        {
            return Err("Denial copy lacks its native source".into());
        }
        e9b::resolve_denial_legacy(&pending.ctx, state);
        return Ok(());
    }
    let (paid_at, source) = paid(state, pending).ok_or("Denial lacks its actual paid spell")?;
    if rows(state)
        .any(|(_, row)| matches!(row, JournalV1::Target { paid_at: old, .. } if old == paid_at))
    {
        return Err("Denial cannot replay its counter and schedule".into());
    }
    if !crate::engine::effect_target_is_legal(
        state,
        source.source,
        source.controller,
        CARD_DEFS[usize::from(source.card_def)].target_spec,
        &pending.ctx.targets,
        0,
    ) {
        return Ok(());
    }
    let target = pending.ctx.target_contracts[0];
    let mut candidates = state.stack.iter().filter_map(|row| {
        let source = row.v4.source_contract?;
        (row.kind == StackItemKind::Spell
            && target_matches(target, row.v4.stack_item_id, source)
            && source.controller == row.controller)
            .then_some((row.v4.stack_item_id, source))
    });
    let Some((target_item, target_source)) = candidates.next() else {
        return Ok(());
    };
    if candidates.next().is_some() {
        return Err("Denial target row is ambiguous".into());
    }
    let turn = TurnV1::now(state);
    let target_at = note(
        state,
        JournalV1::Target {
            paid_at,
            item: target_item,
            source: target_source,
            turn,
        },
    );
    crate::engine::counter_stack_item_by_id(state, target_item)?
        .ok_or("Denial target disappeared before counter")?;
    note(
        state,
        JournalV1::Scheduled {
            paid_at,
            target_at,
            source,
            drawer: target_source.controller,
            turn,
        },
    );
    Ok(())
}
fn scheduled(state: &GameState, at: usize) -> Option<(StackSourceContractV4, PlayerId, TurnV1)> {
    let JournalV1::Scheduled {
        paid_at,
        target_at,
        source,
        drawer,
        turn,
    } = journal(state, at)?
    else {
        return None;
    };
    let (original, target) = scheduled_binding(state, paid_at)?;
    let JournalV1::Target {
        paid_at: target_paid,
        item,
        source: target_source,
        turn: target_turn,
    } = journal(state, target_at)?
    else {
        return None;
    };
    (paid_at < target_at && target_at < at && target_turn == turn && target_paid == paid_at && original == source && drawer == target_source.controller
        && target_matches(target, item, target_source)
        && rows(state).filter(|(_, row)| matches!(row, JournalV1::Target { paid_at: old, .. } if *old == paid_at)).count() == 1
        && rows(state).filter(|(_, row)| matches!(row, JournalV1::Scheduled { paid_at: old, .. } if *old == paid_at)).count() == 1)
        .then_some((source, drawer, turn))
}
fn scheduled_binding(
    state: &GameState,
    at: usize,
) -> Option<(StackSourceContractV4, StackTargetContractV4)> {
    let (paid_at, item, source, target) = match journal(state, at)? {
        JournalV1::Paid {
            item,
            source,
            target,
            ..
        } => (at, item, source, target),
        JournalV1::Rebound {
            paid_at,
            item,
            source,
            target,
            ..
        } => (paid_at, item, source, target),
        _ => return None,
    };
    let JournalV1::Paid {
        item: original_item,
        source: mut original,
        ..
    } = journal(state, paid_at)?
    else {
        return None;
    };
    if item != original_item
        || rows(state)
            .filter(
                |(_, entry)| matches!(entry, JournalV1::Paid { item: other, .. } if *other == item),
            )
            .count()
            != 1
    {
        return None;
    }
    let mut tip = paid_at;
    for (index, entry) in rows(state) {
        if let JournalV1::Rebound {
            paid_at: paid,
            previous_at,
            item: other,
            source: next,
            ..
        } = entry
        {
            if paid != paid_at {
                continue;
            }
            let mut normalized = next;
            normalized.controller = original.controller;
            if previous_at != tip || index <= tip || other != item || normalized != original {
                return None;
            }
            original = next;
            tip = index;
        }
    }
    (tip == at && original == source).then_some((source, target))
}
fn root(queued_at: usize, caster: PlayerId, drawer: PlayerId, optional: bool) -> EffectOp {
    if optional {
        EffectOp::Choice {
            controller: if drawer == caster {
                PlayerRef::Controller
            } else {
                PlayerRef::Opponent
            },
            options: (0..=2).map(|count| leaf(queued_at, count)).collect(),
        }
    } else {
        leaf(queued_at, 1)
    }
}
pub(crate) fn queue_due(state: &mut GameState) -> Result<(), ObjectId> {
    let now = TurnV1::now(state);
    let due: Vec<_> = rows(state).filter_map(|(at, row)| match row {
        JournalV1::Scheduled { turn, source, .. } if turn != now && !rows(state).any(|(_, row)| matches!(row, JournalV1::Queued { scheduled_at, .. } if scheduled_at == at)) => Some((at, source.source)), _ => None,
    }).collect();
    for (scheduled_at, object) in due {
        let (source, drawer, _) = scheduled(state, scheduled_at).ok_or(object)?;
        let upkeep_at = state.engine.event_history.iter().rposition(|event| matches!(event, CommittedEvent::UpkeepBegan { player } if *player == now.active)).ok_or(object)?;
        if upkeep_at <= scheduled_at {
            return Err(object);
        }
        for optional in [true, false] {
            let queued_at = note(
                state,
                JournalV1::Queued {
                    scheduled_at,
                    upkeep_at,
                    turn: now,
                    optional,
                },
            );
            let transport = AbilitySourceContractV4 {
                source: source.source,
                card_def: source.card_def,
                owner: source.owner,
                controller: source.controller,
                zone: source.zone,
                zone_change_count: source.zone_change_count,
                attached_to: None,
            };
            e9b::queue_trigger(
                state,
                transport,
                source.controller,
                root(queued_at, source.controller, drawer, optional),
                false,
            );
        }
    }
    Ok(())
}
fn queued(state: &GameState, at: usize) -> Option<(StackSourceContractV4, PlayerId, bool)> {
    let JournalV1::Queued {
        scheduled_at,
        upkeep_at,
        turn,
        optional,
    } = journal(state, at)?
    else {
        return None;
    };
    let (source, target_drawer, original_turn) = scheduled(state, scheduled_at)?;
    if original_turn == turn
        || turn != TurnV1::now(state)
        || scheduled_at >= upkeep_at
        || upkeep_at >= at
        || !matches!(state.engine.event_history.get(upkeep_at), Some(CommittedEvent::UpkeepBegan { player }) if *player == turn.active)
    {
        return None;
    }
    for kind in [true, false] {
        let mut matches = rows(state).filter_map(|(_, row)| match row {
            JournalV1::Queued {
                scheduled_at: old,
                upkeep_at,
                turn,
                optional: old_kind,
            } if old == scheduled_at && old_kind == kind => Some((upkeep_at, turn)),
            _ => None,
        });
        if matches.next() != Some((upkeep_at, turn)) || matches.next().is_some() {
            return None;
        }
    }
    Some((
        source,
        if optional {
            target_drawer
        } else {
            source.controller
        },
        optional,
    ))
}
fn trigger_matches(state: &GameState, item: &StackItem, at: usize) -> bool {
    queued(state, at).is_some_and(|(source, drawer, optional)| {
        *item
            == StackItem {
                kind: StackItemKind::TriggeredAbility,
                source: source.source,
                controller: source.controller,
                targets: Vec::new(),
                is_copy: false,
                inline_effect: Some(root(at, source.controller, drawer, optional)),
                discarded: Vec::new(),
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: false,
                v4: StackStateV4 {
                    stack_item_id: item.v4.stack_item_id,
                    source_contract: Some(source),
                    ..StackStateV4::default()
                },
            }
    })
}
pub(crate) fn pending_source(
    state: &GameState,
    effect: &EffectOp,
) -> Result<StackSourceContractV4, String> {
    bound(effect)
        .and_then(|at| queued(state, at).map(|entry| entry.0))
        .ok_or_else(|| "Denial pending trigger lost its original paid source".into())
}
pub(crate) fn validate_trigger(
    state: &GameState,
    item: &StackItem,
    publication: bool,
) -> Result<(), String> {
    let at = item
        .inline_effect
        .as_ref()
        .and_then(bound)
        .ok_or("Denial lost its bound trigger root")?;
    if !trigger_matches(state, item, at) || (publication && !owns(state, item, at)) {
        return Err("Denial changed its source or native publication".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let item = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == id)
        .ok_or("Denial publication lost its row")?;
    let Some(queued_at) = item.inline_effect.as_ref().and_then(bound) else {
        return Ok(());
    };
    if !trigger_matches(state, item, queued_at) || rows(state).any(|(_, row)| matches!(row, JournalV1::Published { queued_at: old, .. } if old == queued_at)) { return Err("Denial changed or republished its trigger".into()); }
    let stack_sha256 = digest(item).ok_or("Denial publication could not bind its row")?;
    note(
        state,
        JournalV1::Published {
            queued_at,
            item: id,
            stack_sha256,
        },
    );
    Ok(())
}
fn owns(state: &GameState, item: &StackItem, at: usize) -> bool {
    let mut matches = rows(state).filter_map(|(_, row)| match row {
        JournalV1::Published {
            queued_at,
            item,
            stack_sha256,
        } if queued_at == at => Some((item, stack_sha256)),
        _ => None,
    });
    let Some((id, hash)) = matches.next() else {
        return false;
    };
    matches.next().is_none()
        && id == item.v4.stack_item_id
        && digest(item) == Some(hash)
        && trigger_matches(state, item, at)
}
pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(usize, StackSourceContractV4, PlayerId, bool)> {
    let at = pending
        .resolving_item
        .inline_effect
        .as_ref()
        .and_then(bound)?;
    let (source, drawer, optional) = queued(state, at)?;
    (original_context(pending)
        && !state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == pending.resolving_item.v4.stack_item_id)
        && owns(state, &pending.resolving_item, at))
    .then_some((at, source, drawer, optional))
}
fn answer(state: &GameState, at: usize, item: StackItemId) -> Option<u8> {
    let mut answers = rows(state).filter_map(|(_, row)| match row {
        JournalV1::Answered {
            queued_at,
            item: id,
            count,
        } if queued_at == at && id == item => Some(count),
        _ => None,
    });
    let count = answers.next()?;
    (count <= 2 && answers.next().is_none()).then_some(count)
}
fn started(state: &GameState, at: usize) -> bool {
    rows(state)
        .any(|(_, row)| matches!(row, JournalV1::DrawStarted { queued_at, .. } if queued_at == at))
}
pub(crate) fn record_answer(
    state: &mut GameState,
    pending: &EffectContinuation,
    count: u16,
) -> Result<(), String> {
    if pending
        .resolving_item
        .inline_effect
        .as_ref()
        .and_then(bound)
        .is_none()
    {
        return Ok(());
    }
    let (at, _, _, optional) =
        selected(state, pending).ok_or("Denial answer lost its published trigger")?;
    let item = pending.resolving_item.v4.stack_item_id;
    if !optional
        || count > 2
        || answer(state, at, item).is_some()
        || started(state, at)
        || !pending.frames.is_empty()
    {
        return Err("Denial repeated or changed its optional draw answer".into());
    }
    note(
        state,
        JournalV1::Answered {
            queued_at: at,
            item,
            count: count as u8,
        },
    );
    Ok(())
}
pub(super) fn validate_idle(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<bool, String> {
    if pending
        .resolving_item
        .inline_effect
        .as_ref()
        .and_then(bound)
        .is_none()
    {
        return Ok(false);
    }
    let (at, source, drawer, optional) =
        selected(state, pending).ok_or("Denial lost its original delayed trigger")?;
    let item = pending.resolving_item.v4.stack_item_id;
    if started(state, at) || has_started(state, item) || pending.answered_choice_guard.is_some() {
        return Err("Denial cannot repeat a started draw".into());
    }
    let actual_root = root(at, source.controller, drawer, optional);
    let valid = if let Some(count) = answer(state, at, item) {
        optional
            && matches!((&pending.choice, pending.frames.as_slice()), (None, [EffectFrame::Program { op, path }]) if *op == leaf(at, count) && *path == [u16::from(count)])
    } else {
        matches!((&pending.choice, pending.frames.as_slice()), (None, [EffectFrame::Program { op, path }]) if *op == actual_root && path.is_empty())
            || matches!((&pending.choice, pending.frames.as_slice(), &actual_root), (Some(PendingEffectChoice::ChooseOption { player, path, options, purpose: EffectOptionChoicePurpose::Generic }), [], EffectOp::Choice { options: original, .. }) if *player == drawer && path.is_empty() && options == original)
    };
    if !valid {
        return Err("Denial changed its native count choice or phase".into());
    }
    Ok(true)
}
pub(crate) fn begin_draw(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    at: usize,
    count: u8,
    path: &[u16],
) -> Result<(), String> {
    let (actual, source, drawer, optional) =
        selected(state, pending).ok_or("Denial draw lost its original source")?;
    let item = pending.resolving_item.v4.stack_item_id;
    if at != actual
        || !pending.frames.is_empty()
        || state.eternal_owned_draw_progress_v1.is_some()
        || has_started(state, item)
        || started(state, at)
        || state.stack.iter().any(|row| row.v4.stack_item_id == item)
        || if optional {
            count > 2 || answer(state, at, item) != Some(count) || path != [u16::from(count)]
        } else {
            count != 1 || !path.is_empty()
        }
    {
        return Err("Denial draw changed its native answer or phase".into());
    }
    let begun_at = note(
        state,
        JournalV1::DrawStarted {
            queued_at: at,
            item,
            count,
        },
    );
    if count > 0 {
        let extra = crate::event::draw_instruction_extra(state, drawer, u32::from(count));
        start_instruction(
            state,
            pending,
            InstructionV1::Denial {
                begun_at,
                drawer,
                count,
            },
            SourceV1::Spell(source),
            u32::from(count),
            extra,
        );
    }
    Ok(())
}
pub(super) fn matches_start(
    state: &GameState,
    pending: &EffectContinuation,
    start: StartedV1,
    started_at: usize,
) -> bool {
    let InstructionV1::Denial {
        begun_at,
        drawer,
        count,
    } = start.instruction
    else {
        return false;
    };
    let Some((at, source, actual_drawer, optional)) = selected(state, pending) else {
        return false;
    };
    started_at == begun_at + 1 && drawer == actual_drawer && start.source == SourceV1::Spell(source) && start.total == u32::from(count)
        && journal(state, begun_at) == Some(JournalV1::DrawStarted { queued_at: at, item: start.item, count })
        && rows(state).filter(|(_, row)| matches!(row, JournalV1::DrawStarted { queued_at, .. } if *queued_at == at)).count() == 1
        && if optional { count > 0 && answer(state, at, start.item) == Some(count) } else { count == 1 }
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.eternal_owned_draw_progress_v1.is_some_and(|progress| matches!(state.engine.event_history.get(progress.started_at), Some(CommittedEvent::EternalOwnedDrawV1 { journal }) if journal.start().is_some_and(|(start, _)| matches!(start.instruction, InstructionV1::Denial { .. })))) { return Err("Denial owned draw choices require an observation successor".into()); }
    Ok(())
}
