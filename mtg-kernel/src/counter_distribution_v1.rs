//! Announce targets and allocation before a divided-counter trigger enters
//! the stack. Repeated picks add to one allocation, never add another target.
use super::*;

pub(super) fn waiting(pending: &PendingTrigger) -> bool {
    matches!(
        pending.effect,
        EffectOp::DistributePlusOneCounters {
            finalized: false,
            ..
        }
    )
}

pub(super) fn validate(
    pending: &PendingTrigger,
    state: &GameState,
    complete: bool,
) -> Result<(), String> {
    let EffectOp::DistributePlusOneCounters {
        total,
        allocations,
        finalized,
    } = &pending.effect
    else {
        return Ok(());
    };
    let source = pending
        .source_contract
        .ok_or("counter distribution has no source contract")?;
    if card_def::CARD_DEFS[source.card_def as usize].name != "Quirion Beastcaller"
        || pending.target_spec != TargetSpec::CounterDistribution
        || source.zone != Zone::Battlefield
    {
        return Err("counter distribution has an invalid producer".into());
    }
    let printed_total = state
        .counter_lki_for(source.source, source.zone_change_count)
        .map_or(0, |counters| counters.plus1_plus1.max(0) as u32);
    let sum: u64 = allocations.iter().copied().map(u64::from).sum();
    if *total != printed_total
        || allocations.len() != pending.targets.len()
        || allocations.contains(&0)
        || sum > u64::from(*total)
        || (*finalized && !allocations.is_empty() && sum != u64::from(*total))
        || (complete && !*finalized)
    {
        return Err("counter distribution allocation changed or is incomplete".into());
    }
    Ok(())
}

pub(super) fn decision(state: &GameState, pending: &PendingTrigger) -> Option<Decision> {
    let EffectOp::DistributePlusOneCounters {
        total,
        allocations,
        finalized: false,
    } = &pending.effect
    else {
        return None;
    };
    let assigned: u32 = allocations.iter().copied().sum();
    Some(Decision::ChooseTargets {
        player: pending.controller,
        spell: pending.source,
        remaining: u8::try_from(total.saturating_sub(assigned)).unwrap_or(u8::MAX),
        legal_targets: legal_targets_for_controller_from_source(
            TargetSpec::ControlledCreature,
            &[],
            pending.controller,
            pending_trigger_targeting_source(pending),
            state,
        ),
        can_finish: assigned == 0,
    })
}

pub(super) fn answer(state: &mut GameState, action: &Action) -> Result<(), String> {
    let pending = state
        .engine
        .pending_triggers
        .first()
        .ok_or("no counter distribution")?
        .clone();
    validate_pending_trigger(state, &pending)?;
    let Some(Decision::ChooseTargets {
        legal_targets,
        can_finish,
        ..
    }) = decision(state, &pending)
    else {
        return Err("counter distribution already complete".into());
    };
    match action {
        Action::ChooseTarget(target) if legal_targets.contains(target) => {
            let contract = StackTargetContractV4::capture(state, *target);
            let live = &mut state.engine.pending_triggers[0];
            let EffectOp::DistributePlusOneCounters {
                total,
                allocations,
                finalized,
            } = &mut live.effect
            else {
                unreachable!()
            };
            if let Some(index) = live.targets.iter().position(|chosen| chosen == target) {
                allocations[index] += 1;
            } else {
                live.targets.push(*target);
                live.target_contracts.push(contract);
                allocations.push(1);
            }
            *finalized = allocations.iter().copied().sum::<u32>() == *total;
            Ok(())
        }
        Action::FinishEffectSelection if can_finish => {
            let EffectOp::DistributePlusOneCounters { finalized, .. } =
                &mut state.engine.pending_triggers[0].effect
            else {
                unreachable!()
            };
            *finalized = true;
            Ok(())
        }
        _ => Err(
            "counter distribution requires a legal target allocation or choosing no targets".into(),
        ),
    }
}
