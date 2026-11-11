//! The paid physical Bonder activation owns both APNAP draw instructions.
use super::{
    completed_attempts, has_started, record, start_instruction, InstructionV1, JournalV1, SourceV1,
    StartedV1,
};
use crate::card_def::{CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectOp};
use crate::event::{self, CommittedEvent};
use crate::ids::{PlayerId, StackItemId};
use crate::mana::Cost;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidV1 {
    item: StackItemId,
    source: AbilitySourceContractV4,
    stack_sha256: [u8; 32],
    original: OriginalSourceV1,
    tap_at: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum OriginalSourceV1 {
    Printed,
    Metamorph {
        physical_card_def: u16,
        copy_sha256: [u8; 32],
    },
}

/// Capture the original printing or the existing native, same-generation
/// Metamorph copy record while the actual source pays. Never infer this from
/// the arena object's later incarnation during a restored draw.
fn original_source(state: &GameState, source: AbilitySourceContractV4) -> Option<OriginalSourceV1> {
    let live = state.objects.try_get(source.source)?;
    if live.card_def != source.card_def
        || live.owner != source.owner
        || live.zone != source.zone
        || live.zone_change_count != source.zone_change_count
        || live.v4.is_token
        || live.spell_copy_origin.is_some()
    {
        return None;
    }
    let mut copies = state
        .eternal_v1
        .iter()
        .flat_map(|e| &e.vintage.copies)
        .filter(|copy| {
            copy.host.object == source.source
                && copy.host.zone_change_count == source.zone_change_count
        });
    let Some(copy) = copies.next() else {
        return Some(OriginalSourceV1::Printed);
    };
    if copies.next().is_some()
        || CARD_DEFS.get(usize::from(copy.card_def))?.object_name != "Phyrexian Metamorph"
        || source.zone_change_count == 0
    {
        return None;
    }
    Some(OriginalSourceV1::Metamorph {
        physical_card_def: copy.card_def,
        copy_sha256: Sha256::digest(serde_json::to_vec(copy).ok()?).into(),
    })
}

fn root(card_def: u16) -> EffectOp {
    EffectOp::EachPlayerControllingDefinitionDrawsCard { card_def }
}

fn recipe(item: &StackItem) -> Option<AbilitySourceContractV4> {
    let source = item.v4.ability_source_contract?;
    let def = CARD_DEFS.get(usize::from(source.card_def))?;
    let ability = def.activated_abilities.first()?;
    (cfg!(feature = "eternal-fixtures")
        && item.kind == StackItemKind::ActivatedAbility
        && !item.is_copy
        && item.v4.activated_ability_index == Some(0)
        && item.v4.granted_by.is_none()
        && item.v4.cauldron_grant.is_empty()
        && item.targets.is_empty()
        && item.v4.target_contracts.is_empty()
        && item.v4.target_spec == Some(TargetSpec::None)
        && source.source == item.source
        && source.controller == item.controller
        && source.zone == Zone::Battlefield
        && def.object_name == "Bonder's Ornament"
        && ability.activation_zone == Zone::Battlefield
        && ability.target_spec == TargetSpec::None
        && ability.cost
            == [
                CostComponent::Mana(Cost {
                    generic: 4,
                    pips: &[],
                    x_count: 0,
                }),
                CostComponent::Tap,
            ]
        && (ability.effect)() == root(source.card_def)
        && item.inline_effect == Some(root(source.card_def)))
    .then_some(source)
}

fn fingerprint(item: &StackItem) -> Option<[u8; 32]> {
    Some(Sha256::digest(serde_json::to_vec(item).ok()?).into())
}

/// Called only after the actual activation has paid and published its stack row.
/// Capture all row fields, including source incarnation, costs and entry index.
pub(crate) fn record_paid(
    state: &mut GameState,
    item: StackItemId,
    payment_start: usize,
) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let row = state
        .stack
        .last()
        .filter(|row| row.v4.stack_item_id == item)
        .ok_or("Bonder payment lost its published stack row")?;
    if !matches!(
        row.inline_effect,
        Some(EffectOp::EachPlayerControllingDefinitionDrawsCard { .. })
    ) {
        return Ok(());
    }
    let source = recipe(row).ok_or("Bonder draw requires its original printed activation")?;
    let original = original_source(state, source)
        .ok_or("Bonder payment lost its physical or native Metamorph source")?;
    if AbilitySourceContractV4::capture(state, source.source) != source {
        return Err("Bonder payment changed its battlefield incarnation".into());
    }
    let stack_sha256 = fingerprint(row).ok_or("Bonder payment could not bind its stack row")?;
    let tap_at = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(payment_start)
        .find_map(|(at, event)| {
            matches!(event, CommittedEvent::Tap { object } if *object == source.source)
                .then_some(at)
        })
        .ok_or("Bonder payment lost its actual tap")?;
    record(
        state,
        JournalV1::BonderPaidV1(PaidV1 {
            item,
            source,
            stack_sha256,
            original,
            tap_at,
        }),
    );
    Ok(())
}

fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    let CommittedEvent::EternalOwnedDrawV1 { journal } = state.engine.event_history.get(at)? else {
        return None;
    };
    Some(**journal)
}

pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(usize, AbilitySourceContractV4)> {
    let row = &pending.resolving_item;
    let source = recipe(row)?;
    crate::engine::validate_historical_ability_source_contract(state, source).ok()?;
    if pending.ctx.source != source.source
        || pending.ctx.controller != source.controller
        || pending.ctx.stack_item_id != Some(row.v4.stack_item_id)
        || pending.ctx.ability_source_contract != Some(source)
        || pending.ctx.targets != row.targets
        || pending.ctx.target_contracts != row.v4.target_contracts
        || pending.ctx.paid_cost_refs != row.v4.paid_cost_refs
    {
        return None;
    }
    let digest = fingerprint(row)?;
    let mut matches = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| {
            let CommittedEvent::EternalOwnedDrawV1 { journal } = event else {
                return None;
            };
            let JournalV1::BonderPaidV1(paid) = **journal else {
                return None;
            };
            (paid.item == row.v4.stack_item_id).then_some((at, paid))
        });
    let (at, paid) = matches.next()?;
    (matches.next().is_none()
        && paid.source == source
        && paid.stack_sha256 == digest
        && state.objects.try_get(source.source).is_some_and(|live| {
            live.zone_change_count > source.zone_change_count
                || original_source(state, source) == Some(paid.original)
        })
        && paid.tap_at < at
        && matches!(state.engine.event_history.get(paid.tap_at), Some(CommittedEvent::Tap { object }) if *object == source.source))
        .then_some((at, source))
}

fn eligible(state: &GameState, card_def: u16) -> [bool; 2] {
    [PlayerId::P0, PlayerId::P1].map(|player| {
        state
            .players
            .iter()
            .flat_map(|seat| &seat.battlefield)
            .any(|id| {
                let live = state.objects.get(*id);
                live.zone == Zone::Battlefield
                    && live.controller == player
                    && live.card_def == card_def
            })
    })
}

pub(super) fn validate_idle(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<bool, String> {
    if !matches!(
        pending.resolving_item.inline_effect,
        Some(EffectOp::EachPlayerControllingDefinitionDrawsCard { .. })
    ) {
        return Ok(false);
    }
    let (_, source) = selected(state, pending).ok_or("Bonder lost its paid physical activation")?;
    if has_started(state, pending.resolving_item.v4.stack_item_id)
        || pending.choice.is_some()
        || pending.answered_choice_guard.is_some()
        || !matches!(pending.frames.as_slice(), [EffectFrame::Program { op, path }] if path.is_empty() && *op == root(source.card_def))
    {
        return Err("Bonder lost its original unresolved root".into());
    }
    Ok(true)
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    card_def: u16,
    path: &[u16],
) -> Result<(), String> {
    let (paid_at, source) =
        selected(state, pending).ok_or("Bonder lost its paid physical activation")?;
    let item = pending.resolving_item.v4.stack_item_id;
    if source.card_def != card_def
        || !path.is_empty()
        || !pending.frames.is_empty()
        || state.eternal_owned_draw_progress_v1.is_some()
        || has_started(state, item)
        || state.stack.iter().any(|row| row.v4.stack_item_id == item)
    {
        return Err("Bonder cannot restart or change its original draw instruction".into());
    }
    let eligible = eligible(state, card_def);
    let active = state.active_player;
    let Some(first) = [active, active.opponent()]
        .into_iter()
        .find(|p| eligible[p.index()])
    else {
        return Ok(());
    };
    let first_extra = event::draw_instruction_extra(state, first, 1);
    let resolution_at = state.engine.event_history.len();
    record(
        state,
        JournalV1::BonderResolutionV1 {
            item,
            paid_at,
            first,
            eligible,
            first_extra,
        },
    );
    start_instruction(
        state,
        pending,
        InstructionV1::Bonder {
            resolution_at,
            drawer: first,
        },
        SourceV1::Ability(source),
        1,
        first_extra,
    );
    Ok(())
}

pub(super) fn finish_leg(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    resolution_at: usize,
    drawer: PlayerId,
    source: SourceV1,
) -> Result<(), String> {
    let Some(JournalV1::BonderResolutionV1 {
        first, eligible, ..
    }) = journal(state, resolution_at)
    else {
        return Err("Bonder lost its resolution receipt".into());
    };
    let next = drawer.opponent();
    if drawer == first && eligible[next.index()] {
        let extra = event::draw_instruction_extra(state, next, 1);
        start_instruction(
            state,
            pending,
            InstructionV1::Bonder {
                resolution_at,
                drawer: next,
            },
            source,
            1,
            extra,
        );
    }
    Ok(())
}

pub(super) fn matches_start(
    state: &GameState,
    pending: &EffectContinuation,
    started: StartedV1,
    started_at: usize,
) -> bool {
    let InstructionV1::Bonder {
        resolution_at,
        drawer,
    } = started.instruction
    else {
        return false;
    };
    let Some(JournalV1::BonderResolutionV1 {
        item,
        paid_at,
        first,
        eligible: captured,
        first_extra,
    }) = journal(state, resolution_at)
    else {
        return false;
    };
    let Some((original_paid, source)) = selected(state, pending) else {
        return false;
    };
    let active = state.active_player;
    if item != started.item
        || original_paid != paid_at
        || paid_at >= resolution_at
        || started.source != SourceV1::Ability(source)
        || captured != eligible(state, source.card_def)
        || !captured[drawer.index()]
        || [active, active.opponent()]
            .into_iter()
            .find(|p| captured[p.index()])
            != Some(first)
    {
        return false;
    }
    let first_at = resolution_at + 1;
    let Some((first_start, first_count)) = journal(state, first_at).and_then(JournalV1::start)
    else {
        return false;
    };
    if first_start.instruction
        != (InstructionV1::Bonder {
            resolution_at,
            drawer: first,
        })
        || first_start.item != started.item
        || first_start.source != started.source
        || first_start.controller != started.controller
        || first_start.total != 1
        || first_start.initial_library_len.is_some()
        || 1u32.checked_add(first_extra) != Some(first_count)
    {
        return false;
    }
    if drawer == first {
        return started_at == first_at;
    }
    drawer == first.opponent()
        && started_at > first_at
        && !super::draw_won_game(state, first)
        && completed_attempts(
            &state.engine.event_history[first_at + 1..started_at],
            first_at,
        ) == Some(first_count)
        && journal(state, started_at - 1)
            == Some(JournalV1::Completed {
                started_at: first_at,
                ordinal: first_count - 1,
            })
}

/// Only a live owned choice needs a new public representation. Historical
/// payment receipts remain inert after an ordinary uninterrupted activation.
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if state
        .eternal_owned_draw_progress_v1
        .is_some_and(|progress| {
            journal(state, progress.started_at)
                .and_then(JournalV1::start)
                .is_some_and(|(started, _)| {
                    matches!(started.instruction, InstructionV1::Bonder { .. })
                })
        })
    {
        return Err("Bonder owned draw choices require an observation successor".into());
    }
    Ok(())
}
