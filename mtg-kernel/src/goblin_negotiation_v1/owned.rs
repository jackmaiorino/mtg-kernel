//! Original excess operands and completion under the existing private Crystal owner.
use super::*;
use crate::effect::{
    crystal_prevention_v1::{self, ExecutionStepV1, ExecutionV1, OwnerV1},
    EffectContinuation, EffectFrame,
};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{GameObject, StackTargetContractV4, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureV1 {
    pub previous: u32,
    pub history_start: u32,
    pub ctx: ExecCtx,
    pub op: EffectOp,
    pub path: Vec<u16>,
    pub source: GameObject,
    pub target: GameObject,
    pub toughness: i32,
    pub deathtouch: bool,
    pub lethal: i64,
    pub replacement: crate::doubling_season_v48::ReplacementInputV48,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intersection: Option<super::IntersectionV1>,
}
impl std::hash::Hash for CaptureV1 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.previous.hash(h);
        self.history_start.hash(h);
        self.ctx.hash(h);
        self.op.hash(h);
        self.path.hash(h);
        self.source.hash(h);
        self.target.hash(h);
        self.toughness.hash(h);
        self.deathtouch.hash(h);
        self.lethal.hash(h);
        self.replacement.hash(h);
        if let Some(input) = &self.intersection {
            "goblin-excess-intersection/v1".hash(h);
            input.hash(h);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FinishV1 {
    pub capture_previous: u32,
    pub damage: Option<u32>,
    pub excess: u32,
    pub token_start: u32,
    pub created: Vec<ObjectId>,
}
pub(crate) fn selected(op: &EffectOp) -> bool {
    matches!(op, EffectOp::DealXDamageThenCreateTokensForExcess { .. })
}
fn target(c: &CaptureV1) -> Result<ObjectId, String> {
    let EffectOp::DealXDamageThenCreateTokensForExcess { target_index, .. } = &c.op else {
        return Err("excess capture changed root".into());
    };
    match c.ctx.targets.get(usize::from(*target_index)) {
        Some(Target::Object(id)) => Ok(*id),
        _ => Err("excess target absent".into()),
    }
}
fn token(c: &CaptureV1) -> Result<u16, String> {
    match &c.op {
        EffectOp::DealXDamageThenCreateTokensForExcess { token_def, .. } => Ok(*token_def),
        _ => Err("excess token root absent".into()),
    }
}
pub(crate) fn lookup(
    state: &GameState,
    ctx: &ExecCtx,
    path: &[u16],
) -> Result<Option<CaptureV1>, String> {
    let mut rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::CrystalExecutionV1 { execution } => Some(execution.steps.iter()),
            _ => None,
        })
        .flatten()
        .filter_map(|step| step.excess_capture.as_ref())
        .filter(|c| c.ctx == *ctx && c.path == path);
    let row = rows.next().cloned();
    if rows.next().is_some() {
        return Err("excess original input repeated".into());
    }
    Ok(row)
}
pub(crate) fn capture(
    state: &GameState,
    owner: &OwnerV1,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<CaptureV1, String> {
    if lookup(state, ctx, path)?.is_some() {
        return Err("excess stage already captured".into());
    }
    let (previous, _) = owner
        .packet_before(state, ctx, op, path)
        .ok_or("excess capture lost actual removed owner")?;
    let EffectOp::DealXDamageThenCreateTokensForExcess { target_index, .. } = op else {
        return Err("excess capture foreign root".into());
    };
    let Some(Target::Object(target)) = ctx.targets.get(usize::from(*target_index)) else {
        return Err("excess capture target absent".into());
    };
    let source = state.objects.get(ctx.source).clone();
    let original_target = state.objects.get(*target).clone();
    Ok(CaptureV1 {
        previous,
        history_start: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "excess input index overflow")?,
        ctx: ctx.clone(),
        op: op.clone(),
        path: path.to_vec(),
        source,
        target: original_target,
        toughness: crate::engine::effective_toughness(state, *target),
        deathtouch: crate::engine::has_effective_keyword(state, ctx.source, Keywords::DEATHTOUCH),
        lethal: super::lethal_threshold(state, ctx.source, *target)?,
        replacement: crate::doubling_season_v48::ReplacementInputV48::capture(
            state,
            ctx.controller,
        ),
        intersection: super::intersection(
            state,
            *target,
            super::creature_lethal_threshold(state, ctx.source, *target),
        )?,
    })
}
fn actual_damage(
    state: &GameState,
    c: &CaptureV1,
    until: usize,
) -> Result<(Option<u32>, i32), String> {
    let target = target(c)?;
    let start = usize::try_from(c.history_start).map_err(|_| "excess history index")?;
    let rows = state
        .engine
        .event_history
        .get(start..until)
        .ok_or("excess actual damage interval absent")?;
    let mut found = None;
    for (offset, row) in rows.iter().enumerate() {
        if let CommittedEvent::Damage {
            source,
            target: Target::Object(to),
            amount,
        } = row
        {
            if *source == c.ctx.source && *to == target {
                if found.is_some() || *amount <= 0 {
                    return Err("excess actual packet repeated or nonpositive".into());
                }
                found = Some((
                    u32::try_from(start + offset).map_err(|_| "excess damage index")?,
                    *amount,
                ));
            }
        }
    }
    Ok(found.map_or((None, 0), |(at, n)| (Some(at), n)))
}
/// Only constructed by the genuine finishing interpreter; no serialized field
/// or caller-supplied event list grants permission to bypass transient validation.
pub(crate) struct TokenBatchPermitV1<'a> {
    owner: &'a OwnerV1,
    capture: &'a CaptureV1,
    excess: usize,
}
impl TokenBatchPermitV1<'_> {
    pub(crate) fn events(&self) -> Result<Vec<ProposedEvent>, String> {
        let definition = token(self.capture)?;
        Ok((0..self.excess)
            .map(|_| ProposedEvent::create_token(definition, self.capture.ctx.controller))
            .collect())
    }
    pub(crate) fn allows(&self, state: &GameState, events: &[ProposedEvent]) -> bool {
        let c = self.capture;
        self.owner.packet_before(state, &c.ctx, &c.op, &c.path).is_some_and(|(previous, _)|
            matches!(state.engine.event_history.get(previous as usize), Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary })
                if boundary.pending.as_deref().is_some_and(|pending| pending.ctx == c.ctx
                    && pending.choice.is_none()
                    && pending.frames.last() == Some(&EffectFrame::Program { op:c.op.clone(), path:c.path.clone() }))))
            && state.objects.try_get(c.ctx.source) == Some(&c.source)
            && actual_damage(state, c, state.engine.event_history.len()).ok().is_some_and(|(_, amount)|
                usize::try_from((i64::from(amount) - c.lethal).max(0)).ok() == Some(self.excess))
            && events.len() == self.excess
            && token(c).ok().is_some_and(|definition| events.iter().all(|event| matches!(event,
                ProposedEvent::CreateToken(t) if t.token_def == definition && t.controller == c.ctx.controller
                    && t.initialization_v48.is_none() && !t.entry_tapped && t.entry_face_index.is_none() && t.touched_by.is_empty())))
    }
}
pub(crate) fn finish(
    state: &mut GameState,
    owner: &OwnerV1,
    c: &CaptureV1,
) -> Result<FinishV1, String> {
    owner
        .packet_before(state, &c.ctx, &c.op, &c.path)
        .ok_or("excess finish lost actual removed owner")?;
    if state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::CrystalExecutionV1 { execution } if execution.before.ctx == c.ctx
            && execution.steps.iter().any(|step| step.excess_finish.as_ref().is_some_and(|f| f.capture_previous == c.previous)))) {
        return Err("excess completion already committed".into());
    }
    let (damage, amount) = actual_damage(state, c, state.engine.event_history.len())?;
    let mut expected_target = c.target.clone();
    expected_target.damage = expected_target
        .damage
        .checked_add(amount as u32)
        .ok_or("excess marked damage overflow")?;
    expected_target.v4.deathtouch_damage |= amount > 0 && c.deathtouch;
    let target_id = target(c)?;
    if crate::engine::object_has_type(state, target_id, CardType::Planeswalker)
        != c.intersection.is_some()
        || c.intersection.as_ref().is_some_and(|input| {
            crate::planeswalker_v1::loyalty(state, target_id)
                != Some(input.predamage_loyalty.saturating_sub(amount as u32))
        })
    {
        return Err("excess finishing original loyalty result changed".into());
    }
    if state.objects.try_get(c.ctx.source) != Some(&c.source)
        || state.objects.try_get(target(c)?) != Some(&expected_target)
    {
        return Err("excess finishing source or target incarnation changed".into());
    }
    let excess = u32::try_from((i64::from(amount) - c.lethal).max(0))
        .map_err(|_| "excess result overflow")?;
    super::preflight_tokens(state, c.ctx.controller, token(c)?, excess as usize)?;
    let token_start = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "excess completion index overflow")?;
    if excess > 0 {
        let permit = TokenBatchPermitV1 {
            owner,
            capture: c,
            excess: excess as usize,
        };
        crate::event::commit_goblin_excess_tokens_owned(state, permit)?;
    }
    let created = state.engine.event_history[token_start as usize..]
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::CreateToken { object, .. } => Some(*object),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected = c.replacement.replaced_token_count(excess as usize)?;
    if created.len() != expected {
        return Err("excess actual token roster changed frozen replacement input".into());
    }
    Ok(FinishV1 {
        capture_previous: c.previous,
        damage,
        excess,
        token_start,
        created,
    })
}
fn validate_capture(c: &CaptureV1, item: &StackItem, ctx: &ExecCtx) -> Result<(), String> {
    let id = target(c)?;
    let EffectOp::DealXDamageThenCreateTokensForExcess { target_index, .. } = &c.op else {
        unreachable!()
    };
    // Match the announcement's incarnation fields using the same semantics as
    // engine::target_contract_matches_live. A Creature target remains legal
    // after a control change; its resolution-time controller is captured here
    // for the affected-player menu, independently of the spell's controller.
    let same_incarnation = matches!(c.ctx.target_contracts.get(usize::from(*target_index)),
        Some(StackTargetContractV4::Object { object, card_def, owner, zone, zone_change_count, spell_copy_origin, .. })
            if *object == id && *card_def == c.target.card_def && *owner == c.target.owner
                && *zone == c.target.zone && *zone_change_count == c.target.zone_change_count
                && *spell_copy_origin == c.target.spell_copy_origin);
    let remaining = (i64::from(c.toughness) - i64::from(c.target.damage)).max(0);
    let creature = if c.deathtouch {
        remaining.min(1)
    } else {
        remaining
    };
    let lethal = if let Some(input) = &c.intersection {
        if input.schema != "mtg-kernel/goblin-excess-intersection/v1"
            || input.creature_lethal_damage != creature
            || input.effective_lethal_damage != creature.min(i64::from(input.predamage_loyalty))
        {
            return Err("excess original intersection operands changed".into());
        }
        input.effective_lethal_damage
    } else {
        creature
    };
    if c.ctx != *ctx
        || !c
            .ctx
            .same_except_niv_damage_origin_v1(&crystal_prevention_v1::context(item))
        || c.source.zone != Zone::Stack
        || c.source.controller != ctx.controller
        || !same_incarnation
        || c.target.zone != Zone::Battlefield
        || c.lethal != lethal
        || c.replacement.controller != ctx.controller
    {
        return Err("excess original source/target/context/lethal snapshot changed".into());
    }
    let source = item
        .v4
        .source_contract
        .ok_or("excess original spell contract absent")?;
    if source.source != ctx.source
        || source.card_def != c.source.card_def
        || source.owner != c.source.owner
        || source.zone_change_count != c.source.zone_change_count
    {
        return Err("excess original source incarnation changed".into());
    }
    Ok(())
}
pub(crate) fn validate_step(
    state: &GameState,
    execution: &ExecutionV1,
    step: &ExecutionStepV1,
    ctx: &ExecCtx,
) -> Result<(), String> {
    let EffectFrame::Program { op, path } = &step.frame else {
        return Err("excess step foreign frame".into());
    };
    if !selected(op) {
        if step.excess_capture.is_some() || step.excess_finish.is_some() {
            return Err("foreign leaf carries excess operands".into());
        }
        return Ok(());
    }
    let c = step
        .excess_capture
        .clone()
        .or(lookup(state, ctx, path)?)
        .ok_or("excess stage lost original operands")?;
    validate_capture(&c, &execution.item, ctx)?;
    if c.op != *op
        || c.path != *path
        || c.history_start <= c.previous
        || c.history_start > execution.history_index
        || step
            .excess_capture
            .as_ref()
            .is_some_and(|capture| capture.previous != execution.previous)
    {
        return Err("excess original occurrence changed".into());
    }
    if let Some(finish) = step.excess_finish.as_ref() {
        let (damage, amount) = actual_damage(state, &c, finish.token_start as usize)?;
        let excess = u32::try_from((i64::from(amount) - c.lethal).max(0))
            .map_err(|_| "excess result overflow")?;
        if finish.token_start < c.history_start
            || finish.token_start > execution.history_index
            || actual_damage(state, &c, execution.history_index as usize)? != (damage, amount)
        {
            return Err("excess completion reordered its actual damage and tokens".into());
        }
        let mut actual = Vec::new();
        for event in state
            .engine
            .event_history
            .get(finish.token_start as usize..execution.history_index as usize)
            .ok_or("excess actual token interval absent")?
        {
            if let CommittedEvent::CreateToken {
                object,
                token_def,
                controller,
                ..
            } = event
            {
                if *token_def != token(&c)? || *controller != ctx.controller {
                    return Err("excess actual token definition or actor changed".into());
                }
                actual.push(*object);
            }
        }
        if finish.capture_previous != c.previous
            || finish.damage != damage
            || finish.excess != excess
            || finish.created != actual
            || actual.len() != c.replacement.replaced_token_count(excess as usize)?
        {
            return Err("excess completion changed actual damage or token batch".into());
        }
        for object in actual {
            let live = state
                .objects
                .try_get(object)
                .ok_or("excess actual token disappeared")?;
            if live.card_def != token(&c)? || live.owner != ctx.controller {
                return Err("excess token identity or owner changed".into());
            }
        }
    } else if step.offered_menu.is_none() {
        return Err("excess instruction omitted completion".into());
    }
    Ok(())
}
pub(crate) fn validate_live_choice(
    state: &GameState,
    pending: &EffectContinuation,
    op: &EffectOp,
    path: &[u16],
) -> Result<(), String> {
    if !selected(op) {
        return Ok(());
    }
    let c = lookup(state, &pending.ctx, path)?.ok_or("excess pending menu lost original input")?;
    validate_capture(&c, &pending.resolving_item, &pending.ctx)?;
    if state.objects.try_get(c.ctx.source) != Some(&c.source)
        || state.objects.try_get(target(&c)?) != Some(&c.target)
        || super::intersection(
            state,
            target(&c)?,
            super::creature_lethal_threshold(state, c.ctx.source, target(&c)?),
        )? != c.intersection
    {
        return Err("excess pending source or target incarnation/world changed".into());
    }
    Ok(())
}

pub(crate) fn public_ordering_stage(
    state: &GameState,
) -> Result<Option<super::PublicOrderingStageV1>, crate::rl::RlContractError> {
    use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    crystal_prevention_v1::validate(state).map_err(crate::rl::RlContractError)?;
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = pending.choice.as_ref()
    else {
        return Ok(None);
    };
    let Some(c) = lookup(state, &pending.ctx, &choice.path).map_err(crate::rl::RlContractError)?
    else {
        return Ok(None);
    };
    validate_live_choice(state, pending, &c.op, &c.path).map_err(crate::rl::RlContractError)?;
    Ok(Some(super::PublicOrderingStageV1 {
        source: crate::sower_restriction_capture_v1::masked_ref(c.ctx.source, &c.source)?,
        target: crate::sower_restriction_capture_v1::masked_ref(
            target(&c).map_err(crate::rl::RlContractError)?,
            &c.target,
        )?,
        controller: c.ctx.controller.into(),
        chooser: choice.chooser.into(),
        original_x: c.ctx.x_value,
        lethal_damage: c.lethal,
        remaining_damage: choice.remaining_amount,
        intersection: c.intersection,
    }))
}
pub(crate) fn public_completed(
    state: &GameState,
) -> Result<Vec<super::PublicCompletedV1>, crate::rl::RlContractError> {
    crystal_prevention_v1::validate(state).map_err(crate::rl::RlContractError)?;
    let mut result = Vec::new();
    for event in &state.engine.event_history {
        let CommittedEvent::CrystalExecutionV1 { execution } = event else {
            continue;
        };
        for step in &execution.steps {
            let Some(finish) = step.excess_finish.as_ref() else {
                continue;
            };
            let EffectFrame::Program { path, .. } = &step.frame else {
                continue;
            };
            let c = step
                .excess_capture
                .clone()
                .or(lookup(state, &execution.before.ctx, path)
                    .map_err(crate::rl::RlContractError)?)
                .ok_or_else(|| {
                    crate::rl::RlContractError(
                        "excess public completion lost original input".into(),
                    )
                })?;
            let (_, amount) = actual_damage(state, &c, finish.token_start as usize)
                .map_err(crate::rl::RlContractError)?;
            result.push(super::PublicCompletedV1 {
                source: crate::sower_restriction_capture_v1::masked_ref(c.ctx.source, &c.source)?,
                target: crate::sower_restriction_capture_v1::masked_ref(
                    target(&c).map_err(crate::rl::RlContractError)?,
                    &c.target,
                )?,
                controller: c.ctx.controller.into(),
                original_x: c.ctx.x_value,
                lethal_damage: c.lethal,
                actual_damage: amount,
                excess: finish.excess,
                tokens_created: u64::try_from(finish.created.len()).map_err(|_| {
                    crate::rl::RlContractError("excess public token count overflow".into())
                })?,
                intersection: c.intersection,
            });
        }
    }
    Ok(result)
}
