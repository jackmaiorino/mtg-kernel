//! The genuine opaque Fiery root retains damage, equipment and shield stages.
use crate::effect::{EffectFrame, EffectOp, ExecCtx};
use crate::event::{CommittedEvent as E, DamageProposed, ReplacementEffectKind};
use crate::state::{AbilitySourceContractV4, GameObject, GameState, StackItem, Target, Zone};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptureV38 {
    pub index: u32,
    pub previous: u32,
    pub item: Box<StackItem>,
    pub ctx: ExecCtx,
    pub path: Vec<u16>,
    pub source: AbilitySourceContractV4,
    pub original_source: GameObject,
    pub targets: [Option<GameObject>; 2],
    pub legal: [bool; 2],
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FinishV38 {
    pub index: u32,
    pub capture: u32,
    pub previous: u32,
    pub item: Box<StackItem>,
    pub equipment_move: Option<u32>,
    pub installation: Option<u32>,
}
pub(crate) fn lookup<'a>(
    state: &'a GameState,
    ctx: &ExecCtx,
    path: &[u16],
) -> Result<Option<&'a CaptureV38>, String> {
    let mut rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            E::TwinflameFieryCaptureV38 { capture }
                if capture.ctx == *ctx && capture.path == path =>
            {
                Some(capture.as_ref())
            }
            _ => None,
        });
    let found = rows.next();
    if rows.next().is_some() {
        return Err("Fiery repeated original damage stage".into());
    }
    Ok(found)
}
pub(crate) fn capture(
    state: &mut GameState,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
    ctx: &ExecCtx,
    path: &[u16],
) -> Result<(), String> {
    let op = EffectOp::FieryAnnihilation;
    if crate::linked_aura_operation_v1::needs_event_owner(state)
        && !crate::linked_aura_operation_v1::locally_owned(state)
    {
        return Err("Fiery stages require the actual surrounding linked-operation owner".into());
    }
    if lookup(state, ctx, path)?.is_some() {
        return Err("Fiery cannot recapture a started stage".into());
    }
    let (previous, item) = owner
        .packet_before(state, ctx, &op, path)
        .ok_or("Fiery original stage lacks genuine removed owner")?;
    let source = state
        .objects
        .try_get(ctx.source)
        .ok_or("Fiery original physical source absent")?;
    let selector = crate::rules_meaning_fiery_v1::live_selector()
        .map_err(|_| "Fiery needs actual selected printing")?;
    crate::rules_meaning_fiery_v1::meaning(&selector, source.card_def, 0)
        .map_err(|_| "Fiery root differs from actual printing")?;
    let legal = crate::fiery_annihilation_v1::target_legalities(
        state,
        ctx.source,
        ctx.controller,
        &ctx.targets,
        &ctx.target_contracts,
    );
    if legal[0] && state.engine.next_replacement_id.checked_add(1).is_none() {
        return Err("Fiery replacement identity exhausted".into());
    }
    let mut contract = AbilitySourceContractV4::capture(state, ctx.source);
    contract.controller = ctx.controller;
    let targets = std::array::from_fn(|slot| {
        ctx.targets.get(slot).and_then(|t| match t {
            Target::Object(id) => state.objects.try_get(*id).cloned(),
            _ => None,
        })
    });
    let row = CaptureV38 {
        index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Fiery capture index exceeds u32")?,
        previous,
        item: Box::new(item),
        ctx: ctx.clone(),
        path: path.to_vec(),
        source: contract,
        original_source: source.clone(),
        targets,
        legal,
    };
    state
        .engine
        .event_history
        .push(E::TwinflameFieryCaptureV38 {
            capture: Box::new(row),
        });
    Ok(())
}
pub(crate) fn damage(
    state: &GameState,
    ctx: &ExecCtx,
    path: &[u16],
) -> Result<DamageProposed, String> {
    let capture =
        lookup(state, ctx, path)?.ok_or("Fiery lost original target/legalities capture")?;
    Ok(DamageProposed {
        source: ctx.source,
        target: *ctx
            .targets
            .first()
            .ok_or("Fiery lost original creature target")?,
        amount: if capture.legal[0] { 5 } else { 0 },
        touched_by: Vec::new(),
        is_combat: false,
        historical_source: None,
    })
}
pub(crate) fn finishing(
    state: &GameState,
    ctx: &ExecCtx,
    path: &[u16],
    previous: u32,
) -> Result<bool, String> {
    Ok(lookup(state, ctx, path)?.is_some_and(|c| c.previous != previous))
}
pub(crate) fn finish(
    state: &mut GameState,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
    ctx: &ExecCtx,
    path: &[u16],
) -> Result<(), String> {
    let original = lookup(state, ctx, path)?
        .ok_or("Fiery completion lost original stage")?
        .clone();
    let (previous, item) = owner
        .packet_before(state, ctx, &EffectOp::FieryAnnihilation, path)
        .ok_or("Fiery completion lacks genuine removed owner")?;
    if state.engine.event_history.iter().any(|e|matches!(e,E::TwinflameFieryFinishedV38{operation} if operation.capture==original.index)){
        return Err("Fiery completion repeated".into());
    }
    let mut equipment_move = None;
    if original.legal[1] {
        let Target::Object(equipment) = ctx.targets[1] else {
            return Err("Fiery equipment relation changed".into());
        };
        let snapshot = original.targets[1]
            .as_ref()
            .ok_or("Fiery equipment original facts absent")?;
        if state.objects.try_get(equipment) != Some(snapshot) {
            return Err("Fiery waiting equipment incarnation changed".into());
        }
        let start = state.engine.event_history.len();
        crate::event::propose_and_commit_twinflame_fiery_equipment_owned(state, &original, owner)?;
        let moves:Vec<_>=state.engine.event_history[start..].iter().enumerate().filter_map(|(offset,e)|
            matches!(e,E::ZoneChange{object,from:Zone::Battlefield,to:Zone::Exile,..} if *object==equipment).then_some(start+offset)).collect();
        let [at] = moves.as_slice() else {
            return Err("Fiery completion lost unique actual equipment exile".into());
        };
        equipment_move =
            Some(u32::try_from(*at).map_err(|_| "Fiery equipment move index exceeds u32")?);
    }
    let installation = if original.legal[0] {
        let at = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Fiery installation index exceeds u32")?;
        crate::event::install_exile_if_dies(state, original.source, ctx.target_contracts[0]);
        if !matches!(
            state.engine.event_history.get(at as usize),
            Some(E::ExileIfDiesReplacementInstalledV1 { .. })
        ) {
            return Err("Fiery completion lost actual shield installation".into());
        }
        Some(at)
    } else {
        None
    };
    let row = FinishV38 {
        index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Fiery completion index exceeds u32")?,
        capture: original.index,
        previous,
        item: Box::new(item),
        equipment_move,
        installation,
    };
    state
        .engine
        .event_history
        .push(E::TwinflameFieryFinishedV38 {
            operation: Box::new(row),
        });
    Ok(())
}
pub(crate) fn validate_finish(
    state: &GameState,
    capture: &CaptureV38,
    previous: u32,
    until: u32,
) -> Result<(), String> {
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            E::TwinflameFieryFinishedV38 { operation } if operation.capture == capture.index => {
                Some(operation.as_ref())
            }
            _ => None,
        })
        .collect();
    let [finish] = rows.as_slice() else {
        return Err("Fiery completion lost unique actual operation".into());
    };
    if finish.previous != previous
        || finish.item != capture.item
        || finish.index <= capture.index
        || finish.index >= until
        || finish.equipment_move.is_some() != capture.legal[1]
        || finish.installation.is_some() != capture.legal[0]
    {
        return Err("Fiery completion changed original item/legalities/order".into());
    }
    let barrier = if !capture.legal[0] {
        capture.index
    } else if finish.previous == capture.previous {
        let operations: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                E::CrystalDamageOperationV3 { operation }
                    if operation.ctx == capture.ctx
                        && operation.op == EffectOp::FieryAnnihilation
                        && operation.path == capture.path
                        && operation.history_index > capture.index
                        && operation.history_index < finish.index =>
                {
                    Some(operation.history_index)
                }
                _ => None,
            })
            .collect();
        let [index] = operations.as_slice() else {
            return Err("Fiery later instructions lost actual prior damage operation".into());
        };
        *index
    } else {
        let Some(E::CrystalPreventionBoundaryV1 { boundary }) =
            state.engine.event_history.get(finish.previous as usize)
        else {
            return Err("Fiery final stage lost actual terminal answer".into());
        };
        if boundary.item != capture.item
            || boundary
                .pending
                .as_deref()
                .is_none_or(|p| p.choice.is_some())
            || boundary
                .answer
                .as_ref()
                .is_none_or(|a| a.before.path != capture.path || a.before.original_amount != 5)
        {
            return Err("Fiery later instructions preceded completed replacement answer".into());
        }
        finish.previous
    };
    if let Some(index) = finish.equipment_move {
        let Some(Target::Object(equipment)) = capture.ctx.targets.get(1) else {
            return Err("Fiery original equipment target absent".into());
        };
        if index <= barrier
            || index >= finish.index
            || !matches!(state.engine.event_history.get(index as usize),Some(E::ZoneChange{object,from:Zone::Battlefield,to:Zone::Exile,..}) if object==equipment)
        {
            return Err("Fiery equipment completion lost exact original move".into());
        }
    }
    if let Some(index) = finish.installation {
        if index <= barrier
            || index >= finish.index
            || finish.equipment_move.is_some_and(|m| m >= index)
            || !matches!(state.engine.event_history.get(index as usize),Some(E::ExileIfDiesReplacementInstalledV1{replacement}) if matches!(&replacement.kind,
                ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1{target,source,..} if *target==capture.ctx.target_contracts[0] && *source==capture.source))
        {
            return Err("Fiery completion changed actual shield source/target/order".into());
        }
    }
    Ok(())
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            E::TwinflameFieryCaptureV38 { .. } | E::TwinflameFieryFinishedV38 { .. }
        )
    }) {
        return Err("private Fiery operation entered public event log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let E::TwinflameFieryCaptureV38 { capture } = event else {
            continue;
        };
        let Some(E::CrystalPreventionBoundaryV1 { boundary }) =
            state.engine.event_history.get(capture.previous as usize)
        else {
            return Err("Fiery capture lost original publication".into());
        };
        if capture.index as usize != index
            || capture.previous >= capture.index
            || capture.item != boundary.item
            || boundary.pending.as_deref().map(|p| &p.ctx) != Some(&capture.ctx)
            || crate::effect::effect_op_at_structural_path(&boundary.root, &capture.path)
                != Some(&EffectOp::FieryAnnihilation)
            || capture.source.source != capture.ctx.source
            || capture.source.controller != capture.ctx.controller
            || capture.source.card_def != capture.original_source.card_def
            || capture.source.owner != capture.original_source.owner
            || capture.source.zone_change_count != capture.original_source.zone_change_count
            || capture.source.zone != capture.original_source.zone
        {
            return Err("Fiery original capture changed owner/context/root/source".into());
        }
        let executions:Vec<_>=state.engine.event_history.iter().filter_map(|e|match e{
            E::CrystalExecutionV1{execution} if execution.item==capture.item=>Some(execution.as_ref()),_=>None,
        }).flat_map(|e|e.steps.iter().filter_map(move|s|matches!(&s.frame,EffectFrame::Program{op:EffectOp::FieryAnnihilation,path} if *path==capture.path).then_some((e,s)))).collect();
        let initial: Vec<_> = executions
            .iter()
            .filter(|(e, _)| e.previous == capture.previous)
            .collect();
        if initial.len() != 1 || initial[0].0.history_index <= capture.index {
            return Err("Fiery capture lost unique initial execution".into());
        }
        let finishes: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                E::TwinflameFieryFinishedV38 { operation }
                    if operation.capture == capture.index =>
                {
                    Some(operation.as_ref())
                }
                _ => None,
            })
            .collect();
        if initial[0].1.offered_menu.is_some() {
            if finishes.is_empty() {
                if executions.len() != 1
                    || state.engine.pending_effect.as_ref().is_none_or(|p| {
                        p.resolving_item != *capture.item
                            || p.frames.last()
                                != Some(&EffectFrame::Program {
                                    op: EffectOp::FieryAnnihilation,
                                    path: capture.path.clone(),
                                })
                    })
                {
                    return Err(
                        "Fiery suspended damage lost mandatory waiting owner/completion".into(),
                    );
                }
            } else if executions.len() != 2 || finishes.len() != 1 {
                return Err("Fiery resumed completion skipped/repeated its actual stage".into());
            }
        } else if executions.len() != 1 || finishes.len() != 1 {
            return Err("Fiery atomic completion skipped/repeated".into());
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let E::TwinflameFieryFinishedV38 { operation } = event else {
            continue;
        };
        if operation.index as usize != at
            || !matches!(state.engine.event_history.get(operation.capture as usize),Some(E::TwinflameFieryCaptureV38{capture}) if capture.index==operation.capture)
        {
            return Err("Fiery finished operation orphaned/moved".into());
        }
    }
    Ok(())
}
