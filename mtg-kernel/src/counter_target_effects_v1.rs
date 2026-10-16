//! Native placement on zero to two exact, currently controlled targets.
//! This module grants no model, transport or owning-consumer admission.
use crate::{
    card_def::TargetSpec,
    effect::ExecCtx,
    engine,
    standard_cards_v1::StandardTargetV1,
    standard_creatures_v1::{CounterExtrasV1, CounterTransferV1},
    state::{GameState, Target},
};

#[derive(Clone, Copy)]
pub(crate) enum CounterTargetEffectV1 {
    PutPlusOne,
    DoubleEachKind,
}

pub(crate) fn execute(mode: CounterTargetEffectV1, ctx: &ExecCtx, state: &mut GameState) {
    if apply(mode, ctx, state).is_err() {
        state.engine.halted = Some((
            engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

fn apply(mode: CounterTargetEffectV1, ctx: &ExecCtx, state: &mut GameState) -> Result<(), ()> {
    let source = ctx.ability_source_contract.ok_or(())?;
    if source.source != ctx.source
        || source.controller != ctx.controller
        || ctx.targets.len() > 2
        || ctx.target_contracts.len() != ctx.targets.len()
        || ctx.targets.iter().enumerate().any(|(i, target)| {
            !matches!(target, Target::Object(_)) || ctx.targets[..i].contains(target)
        })
    {
        return Err(());
    }
    let filter = match mode {
        CounterTargetEffectV1::PutPlusOne => StandardTargetV1::UpToTwoControlledCreatures,
        CounterTargetEffectV1::DoubleEachKind => {
            StandardTargetV1::UpToTwoControlledArtifactsOrCreatures
        }
    };
    // Check the whole declared roster against the same resolution scene.
    // Changed control, type and incarnation make only that target illegal.
    let targets: Vec<_> = ctx
        .targets
        .iter()
        .enumerate()
        .filter_map(|(index, target)| {
            let Target::Object(id) = target else {
                return None;
            };
            (ctx.target_incarnation_matches(index, state)
                && engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    TargetSpec::StandardV1(filter),
                    &ctx.targets,
                    index,
                ))
            .then_some(*id)
        })
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    let mut staged = state.clone();
    for target in targets {
        match mode {
            CounterTargetEffectV1::PutPlusOne => {
                crate::standard_cards_v1::scale_counters_checked_v48(&staged, ctx.controller, 1)
                    .map_err(|_| ())?;
                crate::event::add_plus_one_counters(&mut staged, target, ctx.controller, 1)
                    .map_err(|_| ())?;
            }
            CounterTargetEffectV1::DoubleEachKind => {
                let live = staged.objects.get(target);
                // These are CURRENT resolution counts, not announcement or
                // source-departure LKI. Every native kind keeps its own width.
                let current = CounterTransferV1 {
                    counters: live.counters,
                    extras: CounterExtrasV1 {
                        loyalty: crate::planeswalker_v1::loyalty(&staged, target).unwrap_or(0),
                        lifelink: live.v4.lifelink_keyword_counters,
                        time: live.v4.time_counters_v1,
                        finality: live.v4.creature_upgrade.as_ref().map_or(0, |v| v.finality),
                    },
                };
                crate::standard_creatures_v1::place_counters_checked_v1(
                    &mut staged,
                    target,
                    ctx.controller,
                    current,
                )
                .ok_or(())?;
            }
        }
    }
    *state = staged;
    Ok(())
}
