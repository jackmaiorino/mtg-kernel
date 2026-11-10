//! One replacement choice per draw in four definition-owned Eternal effects.

use super::EternalOpV1;
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, PlayerRef};
use crate::eternal_lurrus_v1::LurrusOpV1;
use crate::eternal_pauper_e8b_v1::{self, E8bOpV1};
use crate::eternal_vintage_e7b_v1::dredge;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, StackItemKind, StackSourceContractV4, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum InstructionV1 {
    Frog,
    Plunder { normal: u8, from_graveyard: u8 },
    Tamiyo,
    TwinnedVision,
}

impl InstructionV1 {
    fn from_op(op: &EternalOpV1) -> Option<Self> {
        match op {
            EternalOpV1::LurrusV1(LurrusOpV1::FrogDrawV1) => Some(Self::Frog),
            EternalOpV1::DrawCastFromGraveyardV1 { normal, from_graveyard } => {
                Some(Self::Plunder { normal: *normal, from_graveyard: *from_graveyard })
            }
            EternalOpV1::DrawHalfLibraryThenNoMaximumHandSizeV1 => Some(Self::Tamiyo),
            EternalOpV1::E8bV1(E8bOpV1::TwinnedVisionV1) => Some(Self::TwinnedVision),
            _ => None,
        }
    }

    fn from_program(op: &EffectOp) -> Option<Self> {
        let EffectOp::StandardV1(StandardOpV1::EternalV1(op)) = op else {
            return None;
        };
        Self::from_op(op)
    }

    fn count(self, state: &GameState, ctx: &ExecCtx) -> u32 {
        match self {
            Self::Frog => 1,
            Self::Plunder { normal, from_graveyard } => {
                graveyard_cast_count(state, ctx.source, normal, from_graveyard)
            }
            Self::Tamiyo => half_library_count(state.players[ctx.controller.index()].library.len()),
            Self::TwinnedVision => eternal_pauper_e8b_v1::twinned_vision_draw_count(state, ctx.source),
        }
    }
}

pub(crate) fn recognizes(op: &EternalOpV1) -> bool {
    InstructionV1::from_op(op).is_some()
}

pub(crate) fn graveyard_cast_count(
    state: &GameState,
    source: ObjectId,
    normal: u8,
    from_graveyard: u8,
) -> u32 {
    let from_graveyard_cast = state.objects.try_get(source).is_some_and(|source| {
        source.zone == Zone::Stack
            && source.v4.spell_cast_origin.is_some_and(|origin| origin.origin_zone == Zone::Graveyard)
    });
    u32::from(if from_graveyard_cast { from_graveyard } else { normal })
}

pub(crate) fn half_library_count(library_len: usize) -> u32 {
    u32::try_from(library_len.div_ceil(2)).unwrap_or(u32::MAX)
}

pub(crate) fn remove_maximum_hand_size(state: &mut GameState, player: PlayerId) {
    state.eternal_v1.get_or_insert_with(Default::default).no_maximum_hand_size[player.index()] = true;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum SourceV1 {
    Spell(StackSourceContractV4),
    Ability(AbilitySourceContractV4),
}

impl SourceV1 {
    fn object(self) -> ObjectId {
        match self {
            Self::Spell(source) => source.source,
            Self::Ability(source) => source.source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgressV1 {
    item: StackItemId,
    source: SourceV1,
    controller: PlayerId,
    instruction: InstructionV1,
    total: u32,
    completed: u32,
    library_history_start: Option<usize>,
}

fn selected(state: &GameState, pending: &EffectContinuation) -> Option<(InstructionV1, SourceV1)> {
    let item = &pending.resolving_item;
    if let Some(inline) = &item.inline_effect {
        let instruction = InstructionV1::from_program(inline)?;
        let source = item.v4.ability_source_contract?;
        if source.source != item.source || source.controller != item.controller || source.zone != Zone::Battlefield {
            return None;
        }
        let name = CARD_DEFS.get(usize::from(source.card_def))?.object_name;
        return match (instruction, item.kind, name) {
            (InstructionV1::Frog, StackItemKind::TriggeredAbility, "Psychic Frog") => {
                Some((instruction, SourceV1::Ability(source)))
            }
            (InstructionV1::Tamiyo, StackItemKind::ActivatedAbility, "Tamiyo, Inquisitive Student")
                if item.v4.activated_ability_index == Some(2) =>
            {
                Some((instruction, SourceV1::Ability(source)))
            }
            _ => None,
        };
    }
    if item.kind != StackItemKind::Spell {
        return None;
    }
    let source = item.v4.source_contract?;
    let def = CARD_DEFS.get(usize::from(source.card_def))?;
    if !matches!(def.object_name, "Plunder the Trollshaws" | "Twinned Vision") {
        return None;
    }
    state.objects.try_get(item.source)?;
    if source.zone != Zone::Stack
        || StackSourceContractV4::capture(state, item.source, source.cast_method) != source
    {
        return None;
    }
    let instruction = InstructionV1::from_program(&def.printed_mode_effect(item.mode_chosen)?)?;
    match (instruction, def.object_name) {
        (InstructionV1::Plunder { .. }, "Plunder the Trollshaws")
        | (InstructionV1::TwinnedVision, "Twinned Vision") => Some((instruction, SourceV1::Spell(source))),
        _ => None,
    }
}

fn completion(ordinal: u32) -> EffectFrame {
    EffectFrame::Program {
        op: super::eternal(EternalOpV1::OwnedDrawCompletedV1 { ordinal }),
        path: vec![1],
    }
}

fn stage(pending: &mut EffectContinuation, ordinal: u32) {
    pending.frames.push(completion(ordinal));
    pending.frames.push(EffectFrame::Program {
        op: EffectOp::DrawCards { player: PlayerRef::Controller, count: 1 },
        path: vec![0],
    });
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    op: &EternalOpV1,
    path: &[u16],
) -> Result<(), String> {
    let (instruction, source) = selected(state, pending).ok_or("Eternal draw lost its original source")?;
    if InstructionV1::from_op(op) != Some(instruction)
        || !path.is_empty()
        || !pending.frames.is_empty()
        || state.eternal_owned_draw_progress_v1.is_some()
    {
        return Err("Eternal draw lost its original instruction".into());
    }
    let total = instruction.count(state, &pending.ctx);
    if total == 0 {
        if instruction == InstructionV1::Tamiyo {
            remove_maximum_hand_size(state, pending.ctx.controller);
        }
        return Ok(());
    }
    state.eternal_owned_draw_progress_v1 = Some(ProgressV1 {
        item: pending.resolving_item.v4.stack_item_id,
        source,
        controller: pending.ctx.controller,
        instruction,
        total,
        completed: 0,
        library_history_start: (instruction == InstructionV1::Tamiyo).then_some(state.engine.event_history.len()),
    });
    stage(pending, 0);
    Ok(())
}

fn progress(state: &GameState, pending: &EffectContinuation) -> Option<ProgressV1> {
    let progress = state.eternal_owned_draw_progress_v1?;
    (Some((progress.instruction, progress.source)) == selected(state, pending)
        && progress.item == pending.resolving_item.v4.stack_item_id
        && progress.controller == pending.ctx.controller
        && progress.completed < progress.total).then_some(progress)
}

pub(crate) fn complete(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    ordinal: u32,
    path: &[u16],
) -> Result<(), String> {
    let progress = progress(state, pending).ok_or("Eternal draw lost its progress owner")?;
    if ordinal != progress.completed || path != [1] || !pending.frames.is_empty() {
        return Err("Eternal draw completed the wrong attempt".into());
    }
    let completed = ordinal + 1;
    if completed == progress.total {
        state.eternal_owned_draw_progress_v1 = None;
        if progress.instruction == InstructionV1::Tamiyo {
            remove_maximum_hand_size(state, progress.controller);
        }
    } else {
        state.eternal_owned_draw_progress_v1.as_mut().unwrap().completed = completed;
        stage(pending, completed);
    }
    Ok(())
}

pub(crate) fn abandon(state: &mut GameState, ctx: &ExecCtx) {
    if state.eternal_owned_draw_progress_v1.is_some_and(|progress| {
        Some(progress.item) == ctx.stack_item_id
            && progress.source.object() == ctx.source
            && progress.controller == ctx.controller
    }) {
        state.eternal_owned_draw_progress_v1 = None;
    }
}

fn initial_library_len(state: &GameState, player: PlayerId, start: usize) -> Option<usize> {
    let current = state.players[player.index()].library.len();
    state.engine.event_history.get(start..)?.iter().rev().try_fold(current, |len, event| {
        match event {
            CommittedEvent::Draw { player: drawer, object: Some(_) } if *drawer == player => {
                len.checked_add(1)
            }
            CommittedEvent::ZoneChange { object, from, to, .. }
                if from != to
                    && (*from == Zone::Library || *to == Zone::Library)
                    && state.objects.try_get(*object).is_some_and(|card| card.owner == player) =>
            {
                if *from == Zone::Library { len.checked_add(1) } else { len.checked_sub(1) }
            }
            _ => Some(len),
        }
    })
}

fn expected_total(state: &GameState, ctx: &ExecCtx, progress: ProgressV1) -> Option<u32> {
    match (progress.instruction, progress.library_history_start) {
        (InstructionV1::Tamiyo, Some(start)) => {
            initial_library_len(state, progress.controller, start).map(half_library_count)
        }
        (InstructionV1::Tamiyo, None) | (_, Some(_)) => None,
        (instruction, None) => Some(instruction.count(state, ctx)),
    }
}

pub(crate) fn draw_stage_matches(
    state: &GameState,
    pending: &EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
    remaining: &[EffectFrame],
) -> bool {
    progress(state, pending).is_some_and(|progress| {
        drawer == progress.controller
            && path == [0]
            && remaining == [completion(progress.completed)]
    })
}

pub(crate) fn validate(state: &GameState, pending: &EffectContinuation) -> Result<(), String> {
    let Some((instruction, _)) = selected(state, pending) else {
        return if state.eternal_owned_draw_progress_v1.is_none() {
            Ok(())
        } else {
            Err("Eternal draw progress lost its original source".into())
        };
    };
    if state.eternal_owned_draw_progress_v1.is_none()
        && matches!((&pending.choice, pending.frames.as_slice()),
            (None, [EffectFrame::Program { op, path }]) if InstructionV1::from_program(op) == Some(instruction) && path.is_empty())
    {
        return Ok(());
    }
    let progress = progress(state, pending).ok_or("Eternal draw lost its progress owner")?;
    if expected_total(state, &pending.ctx, progress) != Some(progress.total) {
        return Err("Eternal draw changed its original count".into());
    }
    let (path, remaining) = dredge::draw_stage(pending).ok_or("Eternal draw lost its original stage")?;
    if !draw_stage_matches(state, pending, progress.controller, path, remaining) {
        return Err("Eternal draw changed its progress or remaining effects".into());
    }
    dredge::validate_draw_choice(state, pending, progress.controller, path, remaining, "Eternal draw")
}
