//! Riddler adjustment for the printed fixed draw at a resolving root.

use super::{has_started, start_instruction, InstructionV1, SourceV1};
use crate::effect::{EffectContinuation, EffectOp, PlayerRef};
use crate::eternal_vintage_e7b_v1::dredge;
use crate::event;
use crate::state::{GameState, StackItemKind, StackSourceContractV4, StackTargetContractV4, Zone};

pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(InstructionV1, SourceV1)> {
    let item = &pending.resolving_item;
    let source = match item.kind {
        StackItemKind::Spell if !item.is_copy && item.inline_effect.is_none() => {
            let source = item.v4.source_contract?;
            state.objects.try_get(item.source)?;
            if source.zone != Zone::Stack
                || StackSourceContractV4::capture(state, item.source, source.cast_method) != source
            {
                return None;
            }
            SourceV1::Spell(source)
        }
        StackItemKind::TriggeredAbility => {
            let source = item.v4.ability_source_contract?;
            if source.source != item.source || source.controller != item.controller {
                return None;
            }
            SourceV1::Ability(source)
        }
        _ => return None,
    };
    let EffectOp::DrawCards { player, count } =
        *crate::effect::validated_definition_owned_root_effect(state, pending).ok()?
    else {
        return None;
    };
    let drawer = match player {
        PlayerRef::Controller => item.controller,
        PlayerRef::Opponent => item.controller.opponent(),
        PlayerRef::Target(index) => match item.v4.target_contracts.get(usize::from(index))? {
            StackTargetContractV4::Player(player) => *player,
            _ => return None,
        },
        PlayerRef::ObjectController(_) => return None,
    };
    Some((InstructionV1::FixedRoot { player, drawer, count }, source))
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    player: PlayerRef,
    count: u32,
    path: &[u16],
) -> Result<bool, String> {
    if !path.is_empty() || !pending.frames.is_empty() {
        return Ok(false);
    }
    let drawer = pending.ctx.resolve_player(player, state);
    let extra = event::draw_instruction_extra(state, drawer, count);
    if extra == 0 {
        return Ok(false);
    }
    let Some((instruction, source)) = selected(state, pending) else {
        return Ok(false);
    };
    if instruction != (InstructionV1::FixedRoot { player, drawer, count }) {
        return Err("Eternal fixed draw changed its definition-owned root".into());
    }
    if state.eternal_owned_draw_progress_v1.is_some()
        || has_started(state, pending.resolving_item.v4.stack_item_id)
    {
        return Err("Eternal fixed draw already started its original instruction".into());
    }
    if dredge::owned_draw_needs_choice(state, drawer, count) {
        start_instruction(state, pending, instruction, source, count, extra);
    } else {
        event::draw_instruction(state, drawer, count);
    }
    Ok(true)
}
