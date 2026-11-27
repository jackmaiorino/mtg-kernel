//! A definition-owned root that publishes a draw and its following frames.

use super::{has_started, start_instruction, InstructionV1, SourceV1};
use crate::effect::{exact_staged_draw_root, EffectContinuation, PlayerRef};
use crate::eternal_vintage_e7b_v1::dredge;
use crate::event;
use crate::state::{GameState, StackItemKind, StackSourceContractV4, Zone};

fn source(state: &GameState, pending: &EffectContinuation) -> Option<SourceV1> {
    let item = &pending.resolving_item;
    match item.kind {
        StackItemKind::Spell if !item.is_copy && item.inline_effect.is_none() => {
            let source = item.v4.source_contract?;
            state.objects.try_get(item.source)?;
            (source.zone == Zone::Stack
                && StackSourceContractV4::capture(state, item.source, source.cast_method) == source)
                .then_some(SourceV1::Spell(source))
        }
        StackItemKind::TriggeredAbility | StackItemKind::ActivatedAbility => {
            let source = item.v4.ability_source_contract?;
            (source.source == item.source && source.controller == item.controller)
                .then_some(SourceV1::Ability(source))
        }
        _ => None,
    }
}

pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(InstructionV1, SourceV1)> {
    let (player, count, _) = exact_staged_draw_root(state, pending)?;
    let drawer = pending.ctx.resolve_player(player, state);
    Some((
        InstructionV1::StagedRootV1 {
            player,
            drawer,
            count,
        },
        source(state, pending)?,
    ))
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    player: PlayerRef,
    count: u32,
    path: &[u16],
) -> Result<bool, String> {
    if !path.is_empty() || count == 0 || state.eternal_owned_draw_progress_v1.is_some() {
        return Ok(false);
    }
    let drawer = pending.ctx.resolve_player(player, state);
    let extra = event::draw_instruction_extra(state, drawer, count);
    if extra == 0
        || !(dredge::owned_draw_needs_stage(state, drawer, count)
            || dredge::may_need_draw_choice(state, drawer))
    {
        return Ok(false);
    }
    let Some((original_player, original_count, tail)) = exact_staged_draw_root(state, pending)
    else {
        return Ok(false);
    };
    if player != original_player || count != original_count || pending.frames != tail {
        return Ok(false);
    }
    let Some(source) = source(state, pending) else {
        return Ok(false);
    };
    if has_started(state, pending.resolving_item.v4.stack_item_id) {
        return Err("Eternal staged draw already started its original instruction".into());
    }
    let instruction = InstructionV1::StagedRootV1 {
        player,
        drawer,
        count,
    };
    start_instruction(state, pending, instruction, source, count, extra);
    Ok(true)
}
