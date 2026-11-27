//! Occult Epiphany's fixed draw attempts and authenticated discard tail.

use super::{
    draw_won_game, has_started, start_instruction, validate_native_progress, InstructionV1,
    JournalV1, ProgressV1, SourceV1,
};
use crate::effect::{
    occult_draw_root, EffectAnsweredChoiceGuard, EffectContinuation, EffectFrame,
    EffectTargetSelectionPurpose, PendingEffectChoice, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cube_c1a_v1::C1aChoiceV1;
use crate::eternal_vintage_e7b_v1::dredge;
use crate::event::{self, CommittedEvent};
use crate::event_history_v1::EventKindV1;
use crate::state::{GameState, StackSourceContractV4, Target, Zone};

fn source(state: &GameState, pending: &EffectContinuation) -> Option<SourceV1> {
    let item = &pending.resolving_item;
    let source = item.v4.source_contract?;
    state.objects.try_get(item.source)?;
    (source.zone == Zone::Stack
        && StackSourceContractV4::capture(state, item.source, source.cast_method) == source)
        .then_some(SourceV1::Spell(source))
}

pub(super) fn selected(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(InstructionV1, SourceV1)> {
    let (player, count, _) = occult_draw_root(state, pending)?;
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
    if !path.is_empty() || state.eternal_owned_draw_progress_v1.is_some() {
        return Ok(false);
    }
    let Some((original_player, original_count, tail)) = occult_draw_root(state, pending) else {
        return Ok(false);
    };
    let Some(source) = source(state, pending) else {
        return Ok(false);
    };
    // A resumed raw draw must never fall through simply because Riddler
    // no longer applies after the original attempts have filled the hand.
    if has_started(state, pending.resolving_item.v4.stack_item_id) {
        return Err("Occult Epiphany already started its original draws".into());
    }
    if count == 0 || player != original_player || count != original_count || pending.frames != tail
    {
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
    let instruction = InstructionV1::StagedRootV1 {
        player,
        drawer,
        count,
    };
    start_instruction(state, pending, instruction, source, count, extra);
    Ok(true)
}

/// An idle Occult continuation with a native start is legal only after all
/// of that instruction's attempts, and only at its original discard.
pub(super) fn validate_idle(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<bool, String> {
    let Some((instruction, source)) = selected(state, pending) else {
        return Ok(false);
    };
    let item = pending.resolving_item.v4.stack_item_id;
    let mut starts = state
        .engine
        .event_history
        .of_kinds_v2(&[EventKindV1::EternalOwnedDrawV1])
        .filter_map(|(at, entry)| match entry {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::Started(started) | JournalV1::StartedWithRiddlerV1 { started, .. }
                    if started.item == item
                        && matches!(started.instruction, InstructionV1::StagedRootV1 { .. }) =>
                {
                    Some((at, **journal))
                }
                _ => None,
            },
            _ => None,
        });
    let Some((started_at, journal)) = starts.next() else {
        return Ok(false);
    };
    let (started, attempts) = journal
        .start()
        .ok_or("Occult Epiphany has an invalid draw start")?;
    if starts.next().is_some()
        || started.instruction != instruction
        || started.source != source
        || started.controller != pending.ctx.controller
        || draw_won_game(state, pending.ctx.controller)
        || state.engine.pending_discard.is_some()
    {
        return Err("Occult Epiphany lost its original draw instruction".into());
    }
    validate_native_progress(
        state,
        &pending.ctx,
        ProgressV1 {
            started_at,
            completed: attempts,
        },
        started,
        attempts,
    )?;
    let (_, _, tail) =
        occult_draw_root(state, pending).ok_or("Occult Epiphany lost its discard")?;
    match (
        &pending.choice,
        pending.frames.as_slice(),
        &pending.answered_choice_guard,
    ) {
        (None, frames, None) if frames == tail => Ok(true),
        (
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
                min_targets,
                ..
            }),
            [],
            None,
        ) if *min_targets == choice.min && discard_choice_matches(state, pending, choice) => {
            Ok(true)
        }
        (
            None,
            [frame @ EffectFrame::EternalSelectionV1 { choice, selected }],
            Some(EffectAnsweredChoiceGuard::StandardSelection { frame: answered }),
        ) if frame == answered.as_ref()
            && discard_choice_matches(state, pending, choice)
            && selected.len() == usize::from(choice.min)
            && selected.iter().enumerate().all(|(index, target)| {
                choice.candidates.contains(target) && !selected[..index].contains(target)
            }) =>
        {
            Ok(true)
        }
        _ => Err("Occult Epiphany retained something other than its discard".into()),
    }
}

fn discard_choice_matches(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &E3ChoiceV1,
) -> bool {
    let controller = pending.ctx.controller;
    let count = pending.ctx.x_value;
    let hand = crate::standard_cards_v1::hand_candidates(state, controller);
    count > 0
        && usize::from(count) < hand.len()
        && choice.kind == E3ChoiceKindV1::C1aV1(C1aChoiceV1::OccultDiscardV1)
        && choice.player == controller
        && choice.bindings == hand
        && choice
            .candidates
            .iter()
            .copied()
            .eq(hand.iter().map(|card| Target::Object(card.object)))
        && choice.min == count
        && choice.max == count
        && !choice.ordered
        && choice.path.is_empty()
        && choice.remaining.is_empty()
}
