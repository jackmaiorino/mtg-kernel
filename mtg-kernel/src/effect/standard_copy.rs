//! Optional retargeting of a spell snapshot, independently for every target.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CopyTargetChoice {
    pub spell: Box<StackItem>,
    pub targets: Vec<Target>,
    pub contracts: Vec<StackTargetContractV4>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}
pub(super) fn validate(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &CopyTargetChoice,
) -> Result<(), String> {
    crate::engine::validate_copy_snapshot(state, &choice.spell)?;
    let root = validated_definition_owned_root_effect(state, pending)?;
    if effect_op_at_path(&root, &choice.path)
        != Some(&EffectOp::CopySpellSnapshot {
            spell: choice.spell.clone(),
        })
        || choice.remaining != pending.frames
        || choice.targets.len() != choice.contracts.len()
        || choice.targets.len() > choice.spell.targets.len()
    {
        return Err("spell copy continuation changed".into());
    }
    if !crate::engine::copy_snapshot_prefix_can_complete(
        state,
        &choice.spell,
        pending.ctx.controller,
        &choice.targets,
        &choice.contracts,
    )? {
        return Err("copy retarget prefix has no legal final assignment".into());
    }

    Ok(())
}
pub(super) fn candidates(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &CopyTargetChoice,
) -> Result<Vec<EffectTargetCandidate>, String> {
    Ok(crate::engine::copy_snapshot_replacement_targets(
        state,
        &choice.spell,
        pending.ctx.controller,
        &choice.targets,
    )?
    .into_iter()
    .map(|target| {
        let mut targets = choice.targets.clone();
        let mut contracts = choice.contracts.clone();
        targets.push(target);
        contracts.push(StackTargetContractV4::capture(state, target));
        crate::engine::copy_snapshot_prefix_can_complete(
            state,
            &choice.spell,
            pending.ctx.controller,
            &targets,
            &contracts,
        )
        .map(|possible| possible.then_some(target))
    })
    .collect::<Result<Vec<_>, String>>()?
    .into_iter()
    .flatten()
    .map(|target| EffectTargetCandidate {
        expected_object: match target {
            Target::Object(object) => Some(EffectObjectBinding {
                object,
                expected_zone: state.objects.get(object).zone,
                expected_zone_change_count: state.objects.get(object).zone_change_count,
            }),
            _ => None,
        },
        target,
    })
    .collect())
}
pub(super) fn stage(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    choice: CopyTargetChoice,
) -> Result<bool, String> {
    validate(state, pending, &choice)?;
    if choice.targets.len() == choice.spell.targets.len() {
        if thousand_year_storm_v1::selected(state, &pending.resolving_item) {
            // New progress validation and child publication commit together.
            let mut staged = state.clone();
            let before = staged.stack.len();
            crate::engine::materialize_copy_snapshot(
                &mut staged,
                &choice.spell,
                pending.ctx.controller,
                choice.targets,
                choice.contracts,
            )?;
            thousand_year_storm_v1::note_copy(&mut staged, pending, &choice.path, before)?;
            *state = staged;
        } else {
            crate::engine::materialize_copy_snapshot(
                state,
                &choice.spell,
                pending.ctx.controller,
                choice.targets,
                choice.contracts,
            )?;
        }
        return Ok(false);
    }
    let legal = candidates(state, pending, &choice)?;
    let min_targets = u16::from(!can_keep(state, pending, &choice)?);
    if legal.is_empty() && min_targets != 0 {
        return Err("copy target slot has no valid continuation".into());
    }
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player: pending.ctx.controller,
        path: choice.path.clone(),
        selected: Vec::new(),
        legal,
        min_targets,
        max_targets: 1,
        ordered: false,
        purpose: EffectTargetSelectionPurpose::CopyTarget { choice },
    });
    Ok(true)
}
pub(super) fn finish(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    mut choice: CopyTargetChoice,
    selected: Option<EffectTargetCandidate>,
) -> Result<bool, String> {
    validate(state, pending, &choice)?;
    let index = choice.targets.len();
    if index >= choice.spell.targets.len() {
        return Err("copy target index out of bounds".into());
    }
    let (target, contract) = if let Some(selected) = selected {
        if !candidates(state, pending, &choice)?.contains(&selected) {
            return Err("copy target selection is stale".into());
        }
        (
            selected.target,
            StackTargetContractV4::capture(state, selected.target),
        )
    } else {
        if !can_keep(state, pending, &choice)? {
            return Err("keeping this target would prevent a legal final copy".into());
        }
        (
            choice.spell.targets[index],
            choice.spell.v4.target_contracts[index],
        )
    };
    choice.targets.push(target);
    choice.contracts.push(contract);
    stage(state, pending, choice)
}

pub(super) fn can_keep(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &CopyTargetChoice,
) -> Result<bool, String> {
    let i = choice.targets.len();
    if i >= choice.spell.targets.len() {
        return Ok(false);
    }
    let mut targets = choice.targets.clone();
    let mut contracts = choice.contracts.clone();
    targets.push(choice.spell.targets[i]);
    contracts.push(choice.spell.v4.target_contracts[i]);
    crate::engine::copy_snapshot_prefix_can_complete(
        state,
        &choice.spell,
        pending.ctx.controller,
        &targets,
        &contracts,
    )
}
