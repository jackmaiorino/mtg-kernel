//! Actual resolution-time integer sampling under the existing private damage owner.
//! Original sampling rows are trusted committed engine occurrences, never public DTOs.
use crate::card_def::{CardType, DynamicValueDef};
use crate::effect::{EffectOp, ExecCtx, TargetRef};
use crate::event::{CommittedEvent as E, DamageProposed};
use crate::ids::ObjectId;
use crate::state::{GameObject, GameState, StackItem, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperandV38 {
    pub id: ObjectId,
    pub original: GameObject,
    pub first: bool,
    pub second: bool,
    pub mana_value: Option<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleV38 {
    pub index: u32,
    pub previous: u32,
    pub item: Box<StackItem>,
    pub ctx: ExecCtx,
    pub op: EffectOp,
    pub path: Vec<u16>,
    pub value: DynamicValueDef,
    pub multiplier: i32,
    pub operands: Vec<OperandV38>,
    pub amount: i32,
}
pub(crate) fn selected(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::DealDamageDynamic { .. } | EffectOp::DealDamageByControlledCreatureCount { .. }
    )
}
fn recipe(op: &EffectOp) -> Result<(TargetRef, DynamicValueDef, i32), String> {
    match op {
        EffectOp::DealDamageDynamic { target, amount } => Ok((*target, *amount, 1)),
        EffectOp::DealDamageByControlledCreatureCount { target, multiplier } => Ok((
            *target,
            DynamicValueDef::ControlledPermanentsWithType(CardType::Creature),
            *multiplier,
        )),
        _ => Err("dynamic damage sample belongs to a different instruction".into()),
    }
}
fn sampled_amount(sample: &SampleV38) -> Result<i32, String> {
    use DynamicValueDef as V;
    let count = match sample.value {
        V::Fixed(n) => {
            return n
                .checked_mul(sample.multiplier)
                .ok_or_else(|| "dynamic damage exceeds i32 representation".into())
        }
        V::AmountIfControllerControlsEach {
            amount_when_met,
            amount_otherwise,
            ..
        } => {
            return i32::from(
                if sample.operands.iter().any(|o| o.first)
                    && sample.operands.iter().any(|o| o.second)
                {
                    amount_when_met
                } else {
                    amount_otherwise
                },
            )
            .checked_mul(sample.multiplier)
            .ok_or_else(|| "dynamic damage exceeds i32 representation".into())
        }
        V::DistinctManaValuesAmongControlledNonlandPermanents => sample
            .operands
            .iter()
            .filter(|o| o.first)
            .filter_map(|o| o.mana_value)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        _ => sample.operands.iter().filter(|o| o.first).count(),
    };
    i32::try_from(count)
        .map_err(|_| "dynamic damage membership exceeds i32")?
        .checked_mul(sample.multiplier)
        .ok_or_else(|| "dynamic damage exceeds i32 representation".into())
}
fn operands(state: &GameState, ctx: &ExecCtx, value: DynamicValueDef) -> Vec<OperandV38> {
    use DynamicValueDef as V;
    if matches!(value, V::Fixed(_)) {
        return Vec::new();
    }
    let candidates: Vec<_> = if matches!(value, V::ControllerGraveyardCardsWithType(_)) {
        state.players[ctx.controller.index()].graveyard.clone()
    } else {
        state
            .objects
            .iter()
            .filter_map(|(id, o)| (o.zone == Zone::Battlefield).then_some(id))
            .collect()
    };
    candidates
        .into_iter()
        .map(|id| {
            let object = state.objects.get(id);
            let controlled = object.controller == ctx.controller;
            let (first, second, mana_value) = match value {
                V::Fixed(_) => unreachable!(),
                V::BattlefieldPermanentsWithSubtype(subtype) => (
                    crate::engine::has_effective_subtype(state, id, subtype),
                    false,
                    None,
                ),
                V::ControlledPermanentsWithSubtype(subtype) => (
                    controlled && crate::engine::has_effective_subtype(state, id, subtype),
                    false,
                    None,
                ),
                V::ControllerGraveyardCardsWithType(kind) => (
                    object.owner == ctx.controller
                        && object.zone == Zone::Graveyard
                        && !object.v4.is_token
                        && crate::card_def::CARD_DEFS[usize::from(object.card_def)].has_type(kind),
                    false,
                    None,
                ),
                V::ControlledPermanentsWithType(kind) => (
                    controlled && crate::engine::object_has_type(state, id, kind),
                    false,
                    None,
                ),
                V::DistinctManaValuesAmongControlledNonlandPermanents => {
                    let yes =
                        controlled && !crate::engine::object_has_type(state, id, CardType::Land);
                    (
                        yes,
                        false,
                        yes.then(|| crate::engine::object_mana_value(state, id)),
                    )
                }
                V::AmountIfControllerControlsEach { required, .. } => {
                    let matches = |slot: usize| {
                        controlled
                            && crate::engine::has_effective_subtype(state, id, required[slot].first)
                            && crate::engine::has_effective_subtype(
                                state,
                                id,
                                required[slot].second,
                            )
                    };
                    (matches(0), matches(1), None)
                }
            };
            OperandV38 {
                id,
                original: object.clone(),
                first,
                second,
                mana_value,
            }
        })
        .collect()
}
pub(crate) fn capture(
    state: &mut GameState,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<(), String> {
    let (_, value, multiplier) = recipe(op)?;
    if lookup(state, ctx, op, path)?.is_some() {
        return Err("dynamic damage cannot resample a consumed instruction".into());
    }
    let (previous, item) = owner
        .packet_before(state, ctx, op, path)
        .ok_or("dynamic damage sample lacks genuine removed owner/root/path")?;
    let mut sample = SampleV38 {
        index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "dynamic sample index exceeds u32")?,
        previous,
        item: Box::new(item),
        ctx: ctx.clone(),
        op: op.clone(),
        path: path.to_vec(),
        value,
        multiplier,
        operands: operands(state, ctx, value),
        amount: 0,
    };
    sample.amount = sampled_amount(&sample)?;
    let expected = crate::engine::evaluate_dynamic_value(state, value, ctx.controller)
        .checked_mul(multiplier)
        .ok_or("dynamic damage exceeds i32 representation")?;
    if sample.amount != expected {
        return Err("dynamic sample differs from actual engine query".into());
    }
    state
        .engine
        .event_history
        .push(E::TwinflameDynamicDamageSampleV38 {
            sample: Box::new(sample),
        });
    Ok(())
}
fn lookup<'a>(
    state: &'a GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<Option<&'a SampleV38>, String> {
    let mut rows = state.engine.event_history.iter().filter_map(|e| match e {
        E::TwinflameDynamicDamageSampleV38 { sample }
            if sample.ctx == *ctx && sample.op == *op && sample.path == path =>
        {
            Some(sample.as_ref())
        }
        _ => None,
    });
    let found = rows.next();
    if rows.next().is_some() {
        return Err("duplicate original dynamic damage sample".into());
    }
    Ok(found)
}
pub(crate) fn damage(
    state: &GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<DamageProposed, String> {
    let (target, _, _) = recipe(op)?;
    let sample = lookup(state, ctx, op, path)?
        .ok_or("dynamic damage lost original resolution-time sample")?;
    Ok(DamageProposed {
        source: ctx.source,
        target: ctx.resolve_target(target),
        amount: sample.amount,
        touched_by: Vec::new(),
        is_combat: false,
        historical_source: None,
    })
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, E::TwinflameDynamicDamageSampleV38 { .. }))
    {
        return Err("private dynamic damage sampling appeared in public event log".into());
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let E::TwinflameDynamicDamageSampleV38 { sample } = event else {
            continue;
        };
        let (_, value, multiplier) = recipe(&sample.op)?;
        let Some(E::CrystalPreventionBoundaryV1 { boundary }) =
            state.engine.event_history.get(sample.previous as usize)
        else {
            return Err("dynamic sample lost original owner boundary".into());
        };
        if sample.index as usize != at
            || sample.previous as usize >= at
            || sample.item != boundary.item
            || boundary.pending.as_deref().map(|p| &p.ctx) != Some(&sample.ctx)
            || sample.ctx.source != sample.item.source
            || sample.ctx.controller != sample.item.controller
            || sample.ctx.stack_item_id != Some(sample.item.v4.stack_item_id)
            || sample.ctx.ability_source_contract != sample.item.v4.ability_source_contract
            || sample.value != value
            || sample.multiplier != multiplier
            || sample.amount != sampled_amount(sample)?
            || crate::effect::effect_op_at_structural_path(&boundary.root, &sample.path)
                != Some(&sample.op)
            || sample
                .operands
                .iter()
                .map(|o| o.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != sample.operands.len()
            || sample.operands.iter().any(|o| match value {
                DynamicValueDef::ControllerGraveyardCardsWithType(_) => {
                    o.original.zone != Zone::Graveyard
                }
                _ => o.original.zone != Zone::Battlefield,
            })
        {
            return Err("dynamic damage original sample/root/member/amount changed".into());
        }
        let owners: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                E::CrystalExecutionV1 { execution }
                    if execution.previous == sample.previous && execution.item == sample.item =>
                {
                    Some(execution.as_ref())
                }
                _ => None,
            })
            .flat_map(|e| {
                e.steps.iter().filter_map(move |step| {
                    matches!(&step.frame,
            crate::effect::EffectFrame::Program{op,path} if *op==sample.op && *path==sample.path)
                    .then_some((e, step))
                })
            })
            .collect();
        if owners.len() != 1
            || owners[0].0.history_index <= sample.index
            || owners[0].0.before.ctx != sample.ctx
        {
            return Err("dynamic damage sample lost unique actual executing instruction".into());
        }
    }
    Ok(())
}
