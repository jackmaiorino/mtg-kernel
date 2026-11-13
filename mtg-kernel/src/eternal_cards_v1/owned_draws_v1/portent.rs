//! Paid Portent resolution, next-upkeep publication and its owned draw.
use super::{record, JournalV1 as DrawJournalV1};
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::eternal_premodern_p1a_v1::{self as p1a, P1aOpV1};
use crate::event::CommittedEvent;
use crate::ids::{PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackSourceContractV4,
    StackTargetContractV4, Target, Zone,
};
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
    Look {
        paid_at: usize,
        target: PlayerId,
        top: [Option<EffectObjectBinding>; 3],
    },
    LookCompleted {
        look_at: usize,
    },
    ShuffleCompleted {
        look_at: usize,
        shuffled: bool,
    },
    Scheduled {
        paid_at: usize,
        look_at: usize,
        source: StackSourceContractV4,
        turn: TurnV1,
    },
    Queued {
        scheduled_at: usize,
        upkeep_at: usize,
        turn: TurnV1,
    },
    Published {
        queued_at: usize,
        item: StackItemId,
        stack_sha256: [u8; 32],
    },
    /// A native retarget (Misdirection) changed the paid row's target while
    /// it was on the stack; the row now binds this digest.
    Retargeted {
        paid_at: usize,
        stack_sha256: [u8; 32],
    },
}

fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else {
        return None;
    };
    let DrawJournalV1::PortentV1(row) = **journal else {
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
    record(state, DrawJournalV1::PortentV1(row));
    at
}
fn digest(item: &StackItem) -> Option<[u8; 32]> {
    Some(Sha256::digest(serde_json::to_vec(item).ok()?).into())
}
fn op(leaf: P1aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(leaf)))
}
pub(crate) fn bound(effect: &EffectOp) -> Option<usize> {
    match effect {
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(
            P1aOpV1::PortentUpkeepBoundV1 { queued_at },
        ))) => Some(*queued_at),
        _ => None,
    }
}
fn cast_source(item: &StackItem) -> Option<StackSourceContractV4> {
    let source = item.v4.source_contract?;
    (cfg!(feature = "eternal-fixtures") && item.kind == StackItemKind::Spell
        && item.inline_effect.is_none() && source.source == item.source
        && source.controller == item.controller && source.zone == Zone::Stack
        && item.v4.cast_method == Some(source.cast_method)
        && CARD_DEFS.get(usize::from(source.card_def))?.object_name == "Portent"
        && CARD_DEFS[usize::from(source.card_def)].printed_mode_effect(item.mode_chosen) == Some(p1a::portent())
        && matches!(item.targets.as_slice(), [Target::Player(_)])
        && matches!(item.v4.target_contracts.as_slice(), [StackTargetContractV4::Player(player)] if item.targets == [Target::Player(*player)]))
        .then_some(source)
}
pub(crate) fn physical(pending: &EffectContinuation) -> bool {
    !pending.resolving_item.is_copy && cast_source(&pending.resolving_item).is_some()
}
fn original_context(pending: &EffectContinuation) -> bool {
    let row = &pending.resolving_item;
    pending.ctx.source == row.source
        && pending.ctx.controller == row.controller
        && pending.ctx.stack_item_id == Some(row.v4.stack_item_id)
        && pending.ctx.targets == row.targets
        && pending.ctx.target_contracts == row.v4.target_contracts
        && pending.ctx.paid_cost_refs == row.v4.paid_cost_refs
        && pending.ctx.ability_source_contract == row.v4.ability_source_contract
}
pub(crate) fn legacy_copy(state: &GameState, pending: &EffectContinuation) -> bool {
    let row = &pending.resolving_item;
    cast_source(row).is_some_and(|source| {
        row.is_copy
            && source.spell_copy_origin.is_some()
            && source.spell_cast_origin.is_none()
            && original_context(pending)
            && StackSourceContractV4::capture(state, row.source, source.cast_method) == source
    })
}
/// Only finalize_cast calls this after its successful payment and publication.
pub(crate) fn record_cast(state: &mut GameState, item: StackItemId) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let Some(row) = state.stack.iter().find(|row| row.v4.stack_item_id == item) else {
        return Err("Portent cast lost its native row".into());
    };
    let Some(source) = cast_source(row).filter(|_| !row.is_copy) else {
        return Ok(());
    };
    if source.spell_copy_origin.is_some()
        || !source
            .spell_cast_origin
            .is_some_and(|origin| origin.finalized_method == Some(source.cast_method))
        || StackSourceContractV4::capture(state, row.source, source.cast_method) != source
    {
        return Err("Portent cast lacks its finalized physical origin".into());
    }
    let stack_sha256 = digest(row).ok_or("Portent could not bind its paid row")?;
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
fn paid(state: &GameState, pending: &EffectContinuation) -> Option<(usize, StackSourceContractV4)> {
    let row = &pending.resolving_item;
    let source = cast_source(row)?;
    if row.is_copy
        || !original_context(pending)
        || StackSourceContractV4::capture(state, row.source, source.cast_method) != source
    {
        return None;
    }
    let mut matches = rows(state).filter_map(|(at, entry)| match entry {
        JournalV1::Paid {
            item,
            source: original,
            stack_sha256,
        } if item == row.v4.stack_item_id => Some((at, original, stack_sha256)),
        _ => None,
    });
    let (at, original, stack_sha256) = matches.next()?;
    (matches.next().is_none()
        && original == source
        && digest(row) == Some(bound_digest(state, at, stack_sha256)))
    .then_some((at, source))
}
/// The digest the paid row at `paid_at` binds: its latest native retarget's,
/// or else the cast's own.
fn bound_digest(state: &GameState, paid_at: usize, paid: [u8; 32]) -> [u8; 32] {
    rows(state)
        .filter_map(|(_, row)| match row {
            JournalV1::Retargeted {
                paid_at: original,
                stack_sha256,
            } if original == paid_at => Some(stack_sha256),
            _ => None,
        })
        .last()
        .unwrap_or(paid)
}
/// The paid cast a live stack row still binds before a native retarget
/// changes it. `None` for every other row, including a paid row that no
/// longer binds, which then fails closed as it resolves.
pub(crate) fn retarget_binding(state: &GameState, item: StackItemId) -> Option<usize> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let row = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == item)?;
    let source = cast_source(row).filter(|_| !row.is_copy)?;
    let mut matches = rows(state).filter_map(|(at, entry)| match entry {
        JournalV1::Paid {
            item: paid,
            source: original,
            stack_sha256,
        } if paid == item => Some((at, original, stack_sha256)),
        _ => None,
    });
    let (at, original, stack_sha256) = matches.next()?;
    (matches.next().is_none()
        && original == source
        && StackSourceContractV4::capture(state, row.source, source.cast_method) == source
        && look(state, at).is_none()
        && digest(row) == Some(bound_digest(state, at, stack_sha256)))
    .then_some(at)
}
/// Rebinds the paid row `retarget_binding` found to its retargeted digest.
/// Only the engine's native retarget routines call this, after their
/// targeting events.
pub(crate) fn record_retarget(
    state: &mut GameState,
    item: StackItemId,
    paid_at: Option<usize>,
) -> Result<(), String> {
    let Some(paid_at) = paid_at else {
        return Ok(());
    };
    let Some(row) = state.stack.iter().find(|row| row.v4.stack_item_id == item) else {
        return Err("retargeted Portent left the stack".into());
    };
    if row.is_copy || cast_source(row).is_none() {
        return Err("retargeted Portent lost its paid cast".into());
    }
    let stack_sha256 = digest(row).ok_or("Portent could not bind its retargeted row")?;
    note(
        state,
        JournalV1::Retargeted {
            paid_at,
            stack_sha256,
        },
    );
    Ok(())
}
fn remainder(index: usize) -> Vec<EffectFrame> {
    let EffectOp::Sequence(parts) = p1a::portent() else {
        unreachable!();
    };
    parts
        .into_iter()
        .enumerate()
        .skip(index + 1)
        .rev()
        .map(|(i, op)| EffectFrame::Program {
            op,
            path: vec![i as u16],
        })
        .collect()
}
fn look(
    state: &GameState,
    paid_at: usize,
) -> Option<(usize, PlayerId, [Option<EffectObjectBinding>; 3])> {
    let mut matches = rows(state).filter_map(|(at, row)| match row {
        JournalV1::Look {
            paid_at: original,
            target,
            top,
        } if original == paid_at => Some((at, target, top)),
        _ => None,
    });
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}
fn look_completed(state: &GameState, look_at: usize) -> Option<usize> {
    let mut matches = rows(state)
        .filter_map(|(at, row)| (row == JournalV1::LookCompleted { look_at }).then_some(at));
    let at = matches.next()?;
    (at > look_at && matches.next().is_none()).then_some(at)
}
pub(crate) fn begin_look(
    state: &mut GameState,
    pending: &EffectContinuation,
    path: &[u16],
) -> Result<(), String> {
    let (paid_at, _) = paid(state, pending).ok_or("Portent look lacks its actual paid cast")?;
    if path != [0]
        || pending.frames != remainder(0)
        || look(state, paid_at).is_some()
        || state
            .stack
            .iter()
            .any(|live| live.v4.stack_item_id == pending.resolving_item.v4.stack_item_id)
    {
        return Err("Portent look changed or replayed its original root".into());
    }
    let [Target::Player(target)] = pending.resolving_item.targets.as_slice() else {
        return Err("Portent lost its original target".into());
    };
    let mut top = [None; 3];
    for (slot, id) in top
        .iter_mut()
        .zip(state.players[target.index()].library.iter())
    {
        *slot = Some(EffectObjectBinding {
            object: *id,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(*id).zone_change_count,
        });
    }
    note(
        state,
        JournalV1::Look {
            paid_at,
            target: *target,
            top,
        },
    );
    Ok(())
}
/// Restored public/private menus must still be the actual paid spell's
/// original look or optional shuffle, including its untouched scheduling tail.
pub(super) fn validate_parent(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    use crate::effect::{
        EffectOptionChoicePurpose, EffectTargetSelectionPurpose, PendingEffectChoice,
    };
    use crate::eternal_cards_v1::e3::E3ChoiceKindV1;
    if !physical(pending) || pending.choice.is_none() {
        return Ok(());
    }
    let (paid_at, source) = paid(state, pending).ok_or("Portent prompt lost its paid source")?;
    let (look_at, target, top) =
        look(state, paid_at).ok_or("Portent prompt lost its original look")?;
    if rows(state).any(|(_, row)| matches!(row, JournalV1::Scheduled { paid_at: original, .. } if original == paid_at)) {
        return Err("Portent prompt replayed its completed source".into());
    }
    let valid = match pending.choice.as_ref() {
        Some(PendingEffectChoice::SelectTargets { purpose: EffectTargetSelectionPurpose::EternalV1 { choice }, .. }) => {
            let expected: Vec<_> = top.into_iter().flatten().collect();
            choice.kind == E3ChoiceKindV1::P1aV1(p1a::P1aChoiceV1::PortentOrderV1 { player: target })
                && choice.player == source.controller && choice.path == [0]
                && choice.bindings == expected
                && choice.candidates == expected.iter().map(|binding| Target::Object(binding.object)).collect::<Vec<_>>()
                && choice.min as usize == expected.len() && choice.max == choice.min && choice.ordered
                && choice.remaining == remainder(0) && pending.frames == remainder(0)
                && look_completed(state, look_at).is_none()
        }
        Some(PendingEffectChoice::ChooseOption { player, path, options, purpose: EffectOptionChoicePurpose::Generic }) => {
            *player == source.controller && path == &[1]
                && *options == [op(P1aOpV1::ShuffleTargetV1), op(P1aOpV1::NoEffectV1)]
                && pending.frames == remainder(1) && look_completed(state, look_at).is_some()
                && !rows(state).any(|(_, row)| matches!(row, JournalV1::ShuffleCompleted { look_at: original, .. } if original == look_at))
        }
        _ => false,
    };
    if !valid {
        return Err("Portent prompt changed its original menu or remaining effects".into());
    }
    Ok(())
}
pub(crate) fn finish_look(
    state: &mut GameState,
    pending: &EffectContinuation,
    chosen: &[EffectObjectBinding],
    path: &[u16],
) -> Result<(), String> {
    let (paid_at, _) = paid(state, pending).ok_or("Portent order lost its paid cast")?;
    let (look_at, target, top) =
        look(state, paid_at).ok_or("Portent order lost its native look")?;
    let expected: Vec<_> = top.into_iter().flatten().collect();
    if path != [0]
        || pending.frames != remainder(0)
        || look_completed(state, look_at).is_some()
        || chosen.len() != expected.len()
        || expected
            .iter()
            .any(|binding| chosen.iter().filter(|card| *card == binding).count() != 1)
        || state.players[target.index()]
            .library
            .iter()
            .take(expected.len())
            .any(|id| {
                !expected.iter().any(|binding| {
                    binding.object == *id
                        && binding.expected_zone_change_count
                            == state.objects.get(*id).zone_change_count
                })
            })
    {
        return Err("Portent order changed its original cards or progress".into());
    }
    note(state, JournalV1::LookCompleted { look_at });
    Ok(())
}
pub(crate) fn finish_shuffle(
    state: &mut GameState,
    pending: &EffectContinuation,
    leaf: &P1aOpV1,
    path: &[u16],
) -> Result<(), String> {
    let (paid_at, _) = paid(state, pending).ok_or("Portent shuffle lost its paid cast")?;
    let (look_at, target, _) =
        look(state, paid_at).ok_or("Portent shuffle lost its native look")?;
    let shuffled = *leaf == P1aOpV1::ShuffleTargetV1;
    if path != [1, u16::from(!shuffled)] || pending.frames != remainder(1)
        || look_completed(state, look_at).is_none()
        || rows(state).any(|(_, row)| matches!(row, JournalV1::ShuffleCompleted { look_at: original, .. } if original == look_at))
    { return Err("Portent shuffle changed or replayed its original choice".into()); }
    if shuffled {
        state
            .shuffle_library(target)
            .map_err(|error| error.to_string())?;
    }
    note(state, JournalV1::ShuffleCompleted { look_at, shuffled });
    Ok(())
}
fn completed_parent(state: &GameState, paid_at: usize, look_at: usize, until: usize) -> bool {
    let Some((at, _, _)) = look(state, paid_at) else {
        return false;
    };
    let Some(completed) = look_completed(state, look_at) else {
        return false;
    };
    let mut shuffled = rows(state).filter_map(|(at, row)| matches!(row, JournalV1::ShuffleCompleted { look_at: original, .. } if original == look_at).then_some(at));
    let Some(shuffle) = shuffled.next() else {
        return false;
    };
    at == look_at
        && paid_at < look_at
        && completed < shuffle
        && shuffle < until
        && shuffled.next().is_none()
}
pub(crate) fn schedule(
    state: &mut GameState,
    pending: &EffectContinuation,
    path: &[u16],
) -> Result<(), String> {
    let (paid_at, source) =
        paid(state, pending).ok_or("Portent delay lacks its actual paid cast")?;
    let (look_at, _, _) = look(state, paid_at).ok_or("Portent delay lacks its original look")?;
    if path != [2] || !pending.frames.is_empty()
        || !completed_parent(state, paid_at, look_at, state.engine.event_history.len())
        || rows(state).any(|(_, row)| matches!(row, JournalV1::Scheduled { paid_at: original, .. } if original == paid_at))
    { return Err("Portent delay cannot replay or skip its original resolution".into()); }
    let turn = TurnV1::now(state);
    note(
        state,
        JournalV1::Scheduled {
            paid_at,
            look_at,
            source,
            turn,
        },
    );
    Ok(())
}
fn scheduled(state: &GameState, at: usize) -> Option<(StackSourceContractV4, TurnV1)> {
    let JournalV1::Scheduled {
        paid_at,
        look_at,
        source,
        turn,
    } = journal(state, at)?
    else {
        return None;
    };
    let JournalV1::Paid {
        source: original, ..
    } = journal(state, paid_at)?
    else {
        return None;
    };
    (completed_parent(state, paid_at, look_at, at) && source == original).then_some((source, turn))
}
pub(crate) fn queue_due(
    state: &mut GameState,
    active: PlayerId,
) -> Result<(), crate::ids::ObjectId> {
    let now = TurnV1::now(state);
    let due: Vec<_> = rows(state).filter_map(|(at, row)| match row {
        JournalV1::Scheduled { turn, source, .. } if turn != now && !rows(state).any(|(_, row)| matches!(row, JournalV1::Queued { scheduled_at, .. } if scheduled_at == at)) => Some((at, source.source)),
        _ => None,
    }).collect();
    for (scheduled_at, source_id) in due {
        let upkeep_at = state.engine.event_history.iter().rposition(|event| matches!(event, CommittedEvent::UpkeepBegan { player } if *player == active)).ok_or(source_id)?;
        let (source, _) = scheduled(state, scheduled_at).ok_or(source_id)?;
        if upkeep_at <= scheduled_at || active != state.active_player {
            return Err(source_id);
        }
        let queued_at = note(
            state,
            JournalV1::Queued {
                scheduled_at,
                upkeep_at,
                turn: now,
            },
        );
        // P1a's historical queue stores an ability-shaped transport. The bound
        // recipe is converted to a spell-sourced pending trigger on collection;
        // this transport is never published as an ability source contract.
        let transport = AbilitySourceContractV4 {
            source: source.source,
            card_def: source.card_def,
            owner: source.owner,
            controller: source.controller,
            zone: source.zone,
            zone_change_count: source.zone_change_count,
            attached_to: None,
        };
        p1a::queue_trigger(
            state,
            transport,
            source.controller,
            op(P1aOpV1::PortentUpkeepBoundV1 { queued_at }),
            None,
        );
    }
    Ok(())
}

fn queued(state: &GameState, queued_at: usize) -> Option<StackSourceContractV4> {
    let JournalV1::Queued {
        scheduled_at,
        upkeep_at,
        turn,
    } = journal(state, queued_at)?
    else {
        return None;
    };
    let (source, original_turn) = scheduled(state, scheduled_at)?;
    let count = rows(state).filter(|(_, row)| matches!(row, JournalV1::Queued { scheduled_at: original, .. } if *original == scheduled_at)).count();
    (original_turn != turn && turn == TurnV1::now(state) && scheduled_at < upkeep_at && upkeep_at < queued_at
        && count == 1 && matches!(state.engine.event_history.get(upkeep_at), Some(CommittedEvent::UpkeepBegan { player }) if *player == turn.active))
        .then_some(source)
}
fn trigger_matches(state: &GameState, item: &StackItem, queued_at: usize) -> bool {
    queued(state, queued_at).is_some_and(|source| {
        item.kind == StackItemKind::TriggeredAbility
            && !item.is_copy
            && item.source == source.source
            && item.controller == source.controller
            && item.v4.source_contract == Some(source)
            && item.v4.ability_source_contract.is_none()
            && item.targets.is_empty()
            && item.v4.target_contracts.is_empty()
            && item.inline_effect.as_ref().and_then(bound) == Some(queued_at)
    })
}
pub(crate) fn pending_source(
    state: &GameState,
    effect: &EffectOp,
) -> Result<StackSourceContractV4, String> {
    bound(effect)
        .and_then(|at| queued(state, at))
        .ok_or_else(|| "Portent pending trigger lost its original paid source".into())
}
pub(crate) fn validate_trigger(
    state: &GameState,
    item: &StackItem,
    require_publication: bool,
) -> Result<(), String> {
    let at = item
        .inline_effect
        .as_ref()
        .and_then(bound)
        .ok_or("Portent trigger lost its bound recipe")?;
    if !trigger_matches(state, item, at) || (require_publication && !owns(state, item, at)) {
        return Err("Portent trigger changed its original paid source or publication".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let Some(item) = state.stack.iter().find(|item| item.v4.stack_item_id == id) else {
        return Err("Portent publication lost its actual stack row".into());
    };
    let Some(queued_at) = item.inline_effect.as_ref().and_then(bound) else {
        return Ok(());
    };
    if !trigger_matches(state, item, queued_at)
        || rows(state).any(|(_, row)| matches!(row, JournalV1::Published { queued_at: original, .. } if original == queued_at))
    { return Err("Portent changed or republished its native delayed trigger".into()); }
    let stack_sha256 = digest(item).ok_or("Portent could not bind its native delayed row")?;
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
pub(super) fn owns(state: &GameState, item: &StackItem, queued_at: usize) -> bool {
    if !trigger_matches(state, item, queued_at) {
        return false;
    }
    let mut matches = rows(state).filter_map(|(_, row)| match row {
        JournalV1::Published {
            queued_at: original,
            item: original_item,
            stack_sha256,
        } if original == queued_at => Some((original_item, stack_sha256)),
        _ => None,
    });
    let Some((original_item, stack_sha256)) = matches.next() else {
        return false;
    };
    matches.next().is_none()
        && original_item == item.v4.stack_item_id
        && digest(item) == Some(stack_sha256)
}
pub(super) fn selected_draw(
    state: &GameState,
    pending: &EffectContinuation,
    queued_at: usize,
) -> bool {
    original_context(pending) && owns(state, &pending.resolving_item, queued_at)
}

/// Ordinary pending and published triggers retain their existing public meaning.
/// Only a suspended owned replacement instruction needs an observation successor.
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state.eternal_owned_draw_progress_v1.is_some_and(|progress| {
        matches!(state.engine.event_history.get(progress.started_at),
            Some(CommittedEvent::EternalOwnedDrawV1 { journal })
                if journal.start().is_some_and(|(started, _)| matches!(started.instruction, super::InstructionV1::Portent { .. })))
    }) {
        return Err("Portent owned draw choices require an observation successor".into());
    }
    Ok(())
}
