//! Draw continuation for the actual return batch of Paradoxical Outcome.

use super::{paradoxical_outcome, vintage, VintageOpV1};
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, PlayerRef};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{PlayerId, StackItemId};
use crate::state::{
    GameState, ObjectLinkV4, StackItemKind, StackSourceContractV4, StackTargetContractV4, Target,
    Zone,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DrawProgressV1 {
    item: StackItemId,
    source: StackSourceContractV4,
    controller: PlayerId,
    returned: ReturnBatch,
    completed: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReturnBatch {
    start: usize,
    end: usize,
    pub(super) arrivals: Vec<ObjectLinkV4>,
}

fn owned_arrivals(
    state: &GameState,
    ctx: &ExecCtx,
    events: &[CommittedEvent],
) -> Vec<ObjectLinkV4> {
    events.iter().filter_map(|event| {
        let CommittedEvent::ZoneChange {
            object, from: Zone::Battlefield, to: Zone::Hand, controller_before,
        } = event else {
            return None;
        };
        if *controller_before != ctx.controller {
            return None;
        }
        let original_generation = ctx.target_contracts.iter().find_map(|target| match target {
            StackTargetContractV4::Object {
                object: bound, owner, zone: Zone::Battlefield, zone_change_count, ..
            } if bound == object && *owner == ctx.controller => Some(*zone_change_count),
            _ => None,
        })?;
        let generation = original_generation.checked_add(1)?;
        let card = state.objects.try_get(*object)?;
        (card.owner == ctx.controller && card.zone == Zone::Hand && card.zone_change_count == generation)
            .then_some(ObjectLinkV4 { object: *object, zone_change_count: generation })
    }).collect()
}

pub(super) fn return_targets(state: &mut GameState, ctx: &ExecCtx) -> ReturnBatch {
    let start = state.engine.event_history.len();
    if let Some(spec) = state.objects.try_get(ctx.source)
        .and_then(|source| CARD_DEFS.get(usize::from(source.card_def)))
        .map(|def| def.target_spec)
    {
        let events = ctx.targets.iter().enumerate().filter_map(|(index, target)| {
            let Target::Object(object) = *target else { return None };
            (ctx.target_incarnation_matches(index, state)
                && crate::engine::effect_target_is_legal(
                    state, ctx.source, ctx.controller, spec, &ctx.targets, index,
                ))
                .then(|| ProposedEvent::zone_change(object, Zone::Hand))
        }).collect::<Vec<_>>();
        if !events.is_empty() {
            event::propose_and_commit_batch(state, events);
        }
    }
    let end = state.engine.event_history.len();
    let arrivals = owned_arrivals(state, ctx, &state.engine.event_history[start..end]);
    ReturnBatch { start, end, arrivals }
}

fn spell_source(state: &GameState, pending: &EffectContinuation) -> Option<StackSourceContractV4> {
    let item = &pending.resolving_item;
    if item.kind != StackItemKind::Spell || item.inline_effect.is_some() {
        return None;
    }
    let source = item.v4.source_contract?;
    state.objects.try_get(item.source)?;
    let def = CARD_DEFS.get(usize::from(source.card_def))?;
    (source.zone == Zone::Stack
        && StackSourceContractV4::capture(state, item.source, source.cast_method) == source
        && (def.spell_effect)().as_ref() == Some(&paradoxical_outcome()))
        .then_some(source)
}

fn completion(ordinal: u16) -> EffectFrame {
    EffectFrame::Program {
        op: vintage(VintageOpV1::ParadoxicalOutcomeDrawCompletedV1 { ordinal }),
        path: vec![ordinal, 0],
    }
}

fn stage(pending: &mut EffectContinuation, ordinal: u16) {
    pending.frames.push(completion(ordinal));
    pending.frames.push(EffectFrame::Program {
        op: EffectOp::DrawCards { player: PlayerRef::Controller, count: 1 },
        path: vec![ordinal],
    });
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    path: &[u16],
) -> Result<(), String> {
    let source = spell_source(state, pending).ok_or("Paradoxical Outcome lost its original spell")?;
    if !path.is_empty() || !pending.frames.is_empty() || state.paradoxical_outcome_draw_progress_v1.is_some() {
        return Err("Paradoxical Outcome lost its original return stage".into());
    }
    let returned = return_targets(state, &pending.ctx);
    if state.engine.halted.is_some() {
        return Err("Paradoxical Outcome return batch halted".into());
    }
    if !returned.arrivals.is_empty() {
        state.paradoxical_outcome_draw_progress_v1 = Some(DrawProgressV1 {
            item: pending.resolving_item.v4.stack_item_id,
            source,
            controller: pending.resolving_item.controller,
            returned,
            completed: 0,
        });
        stage(pending, 0);
    }
    Ok(())
}

fn progress<'a>(state: &'a GameState, pending: &EffectContinuation) -> Option<&'a DrawProgressV1> {
    let progress = state.paradoxical_outcome_draw_progress_v1.as_ref()?;
    (progress.item == pending.resolving_item.v4.stack_item_id
        && Some(progress.source) == spell_source(state, pending)
        && progress.controller == pending.resolving_item.controller
        && usize::from(progress.completed) < progress.returned.arrivals.len())
        .then_some(progress)
}

pub(crate) fn complete(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    ordinal: u16,
    path: &[u16],
) -> Result<(), String> {
    let progress = progress(state, pending).ok_or("Paradoxical Outcome lost its draw owner")?;
    if ordinal != progress.completed || path != [ordinal, 0] || !pending.frames.is_empty() {
        return Err("Paradoxical Outcome completed the wrong draw".into());
    }
    let completed = ordinal.checked_add(1).ok_or("Paradoxical Outcome draw count overflow")?;
    if usize::from(completed) == progress.returned.arrivals.len() {
        state.paradoxical_outcome_draw_progress_v1 = None;
    } else {
        state.paradoxical_outcome_draw_progress_v1.as_mut().unwrap().completed = completed;
        stage(pending, completed);
    }
    Ok(())
}

pub(crate) fn abandon(state: &mut GameState, ctx: &ExecCtx) {
    if state.paradoxical_outcome_draw_progress_v1.as_ref().is_some_and(|progress| {
        Some(progress.item) == ctx.stack_item_id
            && progress.source.source == ctx.source
            && progress.controller == ctx.controller
    }) {
        state.paradoxical_outcome_draw_progress_v1 = None;
    }
}

pub(crate) fn validate(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    if spell_source(state, pending).is_none() {
        return if state.paradoxical_outcome_draw_progress_v1.is_none() {
            Ok(())
        } else {
            Err("Paradoxical Outcome draw progress lost its original spell".into())
        };
    }
    if state.paradoxical_outcome_draw_progress_v1.is_none()
        && matches!((&pending.choice, pending.frames.as_slice()),
            (None, [EffectFrame::Program { op, path }]) if *op == paradoxical_outcome() && path.is_empty())
    {
        return Ok(());
    }
    let progress = progress(state, pending).ok_or("Paradoxical Outcome lost its draw progress")?;
    let returned = &progress.returned;
    let batch = state.engine.event_history.get(returned.start..returned.end)
        .ok_or("Paradoxical Outcome lost its return batch")?;
    if returned.arrivals != owned_arrivals(state, &pending.ctx, batch) {
        return Err("Paradoxical Outcome changed its actual hand arrivals".into());
    }
    let (path, remaining) = crate::eternal_vintage_e7b_v1::dredge::draw_stage(pending)
        .ok_or("Paradoxical Outcome lost its original draw stage")?;
    if !draw_stage_matches(state, pending, path, remaining) {
        return Err("Paradoxical Outcome draw changed its progress or remaining effects".into());
    }
    crate::eternal_vintage_e7b_v1::dredge::validate_draw_choice(
        state, pending, progress.controller, path, remaining, "Paradoxical Outcome draw",
    )
}

pub(crate) fn draw_stage_matches(
    state: &GameState,
    pending: &EffectContinuation,
    path: &[u16],
    remaining: &[EffectFrame],
) -> bool {
    progress(state, pending).is_some_and(|progress| {
        path == [progress.completed] && remaining == [completion(progress.completed)]
    })
}
