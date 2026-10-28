//! Whole-object determinization. This is a sampling convention, not a posterior.
use super::*;
use crate::event_history_v1::EventKindV1;
use crate::state::{AbilitySourceContractV4, SplitMix64, StackItem};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SamplingScopeV68 {
    Legacy,
    EndTurn,
    Call,
    GenesisWave,
    Muldrotha,
    Tinybones,
    Liliana,
    Chandra,
    Kaito,
}
fn diagnose_refusal<T>(stage: &'static str, value: T) -> T {
    #[cfg(test)]
    eprintln!("hidden-slot sampling refused at {stage}");
    #[cfg(not(test))]
    let _ = stage;
    value
}
fn conflicts(
    state: &GameState,
    pool: &[ObjectId],
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
) -> bool {
    conflicts_in_scope(state, pool, plan, SamplingScopeV68::Legacy)
}
fn conflicts_in_scope(
    state: &GameState,
    pool: &[ObjectId],
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
    scope: SamplingScopeV68,
) -> bool {
    let op_conflicts = |op: &crate::effect::EffectOp| match scope {
        SamplingScopeV68::Call => super::effect_refs::op_conflicts_call_v62(state, pool, op),
        SamplingScopeV68::GenesisWave => {
            super::effect_refs::op_conflicts_genesis_wave_v69(state, pool, op)
        }
        SamplingScopeV68::Muldrotha => {
            super::effect_refs::op_conflicts_muldrotha_v71(state, pool, op)
        }
        SamplingScopeV68::Tinybones => {
            super::effect_refs::op_conflicts_tinybones_v72(state, pool, op)
        }
        SamplingScopeV68::Liliana => super::effect_refs::op_conflicts_liliana_v73(state, pool, op),
        SamplingScopeV68::Chandra => super::effect_refs::op_conflicts_chandra_v74(state, pool, op),
        SamplingScopeV68::Kaito => super::effect_refs::op_conflicts_kaito_v75(state, pool, op),
        SamplingScopeV68::EndTurn => super::effect_refs::op_conflicts_end_turn_v68(state, pool, op),
        SamplingScopeV68::Legacy => super::effect_refs::op_conflicts(state, pool, op),
    };
    if crate::bloodthirsty_conqueror_v49::has_history(state)
        || crate::soul_counters_v1::has_history(state)
    {
        return diagnose_refusal("references.soul_or_conqueror", true);
    }
    if crate::ajani_full_v42::lifecycle::requires_successor(state) {
        return diagnose_refusal("references.ajani", true);
    }
    if crate::fishing_pole_v39::lifecycle::requires_successor(state) {
        return diagnose_refusal("references.fishing", true);
    }
    if crate::krenko_v1::requires_successor(state) {
        return diagnose_refusal("references.krenko", true);
    }
    // Frozen V4 search retains its historical refusal after either entry
    // observer journal, including a passive entry with no actual observer.
    // New owning scopes validate those passive records before the reference
    // walk below, while still refusing actual unsupported observer roots.
    let entry_conflict = if scope == SamplingScopeV68::Legacy {
        crate::arahbo_v1::requires_successor(state)
            || state
                .engine
                .event_history
                .iter()
                .any(crate::arahbo_v1::is_history)
            || state
                .engine
                .event_history
                .iter()
                .any(crate::garruks_uprising_v1::is_history)
    } else {
        crate::arahbo_v1::validate_state(state).is_err()
            || crate::garruks_uprising_v1::validate_state(state).is_err()
            || crate::arahbo_v1::requires_successor(state)
            || crate::garruks_uprising_v1::requires_successor(state)
    };
    if entry_conflict {
        return diagnose_refusal("references.entry_observers", true);
    }
    // Retained linked duration and attack ownership need their qualified sampler.
    if crate::banishing_light_v1::requires_successor(state)
        || (state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                crate::event::CommittedEvent::AshrootDispatchV2 { .. }
            )
        }) && !pool.is_empty())
    {
        return diagnose_refusal("references.banishing_or_ashroot_dispatch", true);
    }
    // The source's old definition and power remain authoritative after it returns
    // to a hidden zone. This lane does not admit a historical resampling scheme.
    if state.engine.event_history.iter().any(|event| {
        matches!(event,
        crate::event::CommittedEvent::AshrootPowerBeforeDepartureV1 { source, .. }
            if pool.contains(&source.object))
    }) {
        return diagnose_refusal("references.ashroot_departure", true);
    }
    // Receipt-owned references remain binding through legend/deferred stages
    // and after the physical source or recipient returns to a hidden zone.
    match crate::wildwood_capture_v1::active(state) {
        Ok(rows) if rows.iter().any(|(_, _, capture)| {
            capture.observers.iter().any(|source| pool.contains(&source.source))
                || matches!(capture.recipient, crate::state::StackTargetContractV4::Object { object, .. } if pool.contains(&object))
        }) => return true,
        Err(_) => return true,
        _ => {},
    }
    if state
        .wildwood_waiting_v1
        .as_ref()
        .is_some_and(|waiting| waiting.iter().any(|row| op_conflicts(&row.effect)))
    {
        return true;
    }
    if match scope {
        SamplingScopeV68::Legacy => super::effect_refs::conflicts(state, pool, plan),
        SamplingScopeV68::EndTurn => super::effect_refs::conflicts_end_turn_v68(state, pool, plan),
        SamplingScopeV68::Call => super::effect_refs::conflicts_call_v62(state, pool, plan),
        SamplingScopeV68::GenesisWave => {
            super::effect_refs::conflicts_genesis_wave_v69(state, pool, plan)
        }
        SamplingScopeV68::Muldrotha => {
            super::effect_refs::conflicts_muldrotha_v71(state, pool, plan)
        }
        SamplingScopeV68::Tinybones => {
            super::effect_refs::conflicts_tinybones_v72(state, pool, plan)
        }
        SamplingScopeV68::Liliana => super::effect_refs::conflicts_liliana_v73(state, pool, plan),
        SamplingScopeV68::Chandra => super::effect_refs::conflicts_chandra_v74(state, pool, plan),
        SamplingScopeV68::Kaito => super::effect_refs::conflicts_kaito_v75(state, pool, plan),
    } {
        return diagnose_refusal("references.effect_walk", true);
    }
    let same = |id: ObjectId, generation: u32| {
        pool.contains(&id) && state.objects.get(id).zone_change_count == generation
    };
    let ability = |c: &AbilitySourceContractV4| {
        same(c.source, c.zone_change_count)
            || c.attached_to
                .is_some_and(|x| same(x.object, x.zone_change_count))
    };
    let historical_death_source =
        |source: &AbilitySourceContractV4, effect: &crate::effect::EffectOp| {
            pool.contains(&source.source)
                && crate::creature_death_capture_v1::owns_program(source, effect)
        };
    let active_call_source = |source: ObjectId, op: &crate::effect::EffectOp| {
        scope != SamplingScopeV68::Legacy
            && pool.contains(&source)
            && matches!(
                op,
                crate::effect::EffectOp::BindAleshaRaidReturn
                    | crate::effect::EffectOp::AleshaRaidReturn { .. }
                    | crate::effect::EffectOp::BindNineLivesFamiliarReturnV1
                    | crate::effect::EffectOp::ScheduleNineLivesFamiliarReturnV1 { .. }
                    | crate::effect::EffectOp::ReturnNineLivesFamiliarV1 { .. }
                    | crate::effect::EffectOp::BindValkyriesCallDeathV1
                    | crate::effect::EffectOp::ReturnValkyriesCallCreatureV1 { .. }
            )
    };
    let stack = |s: &StackItem| {
        s.inline_effect
            .as_ref()
            .is_some_and(|op| op_conflicts(op) || active_call_source(s.source, op))
            || s.v4.ability_source_contract.as_ref().is_some_and(&ability)
            || s.v4
                .ability_source_contract
                .as_ref()
                .zip(s.inline_effect.as_ref())
                .is_some_and(|(source, effect)| historical_death_source(source, effect))
            || s.v4.granted_by.as_ref().is_some_and(&ability)
            || s.v4
                .cauldron_grant
                .0
                .as_ref()
                .is_some_and(|grant| ability(&grant.host) || ability(&grant.donor))
            || s.v4
                .hidden_ability_source
                .is_some_and(|x| same(x.object, x.zone_change_count))
            || s.v4
                .madness_source_contract
                .is_some_and(|x| same(x.source, x.zone_change_count))
            || s.v4
                .source_contract
                .as_ref()
                .is_some_and(|x| same(x.source, x.zone_change_count))
    };
    let e = &state.engine;
    if state.objects.iter().any(|(_, object)| {
        object.v4.creature_upgrade.as_ref().is_some_and(|upgrade| {
            upgrade.combat_impulse_source.as_ref().is_some_and(&ability)
                || upgrade.combat_impulse_donor.as_ref().is_some_and(&ability)
        })
    }) || e.pending_triggers.iter().any(|t| {
        op_conflicts(&t.effect)
            || t.source_contract.as_ref().is_some_and(&ability)
            || t.source_contract
                .as_ref()
                .is_some_and(|source| active_call_source(source.source, &t.effect))
            || t.source_contract
                .as_ref()
                .is_some_and(|source| historical_death_source(source, &t.effect))
            || t.granted_by.as_ref().is_some_and(&ability)
    }) || state.stack.iter().any(&stack)
        || e.pending_effect
            .as_ref()
            .is_some_and(|p| stack(&p.resolving_item))
        || e.initiative_source.as_ref().is_some_and(&ability)
        || e.monarch_source.as_ref().is_some_and(&ability)
        || e.pending_activation.as_ref().is_some_and(|p| {
            same(p.source, p.source_zone_change_count)
                || p.cauldron_grant
                    .0
                    .as_ref()
                    .is_some_and(|grant| ability(&grant.host) || ability(&grant.donor))
        })
        || e.pending_land_play
            .as_ref()
            .is_some_and(|p| same(p.source, p.source_zone_change_count))
        || e.pending_cast.as_ref().is_some_and(|p| {
            same(
                p.source_contract.source,
                p.source_contract.zone_change_count,
            )
        })
        || e.pending_spell_copy
            .as_ref()
            .is_some_and(|p| same(p.resolving_source, p.resolving_source_zone_change_count))
        || e.linked_exile_records
            .iter()
            .any(|r| ability(&r.source) || same(r.exiled, r.exiled_zone_change_count))
        || e.until_next_turn_keywords
            .iter()
            .any(|r| same(r.object_id, r.object_zone_change_count))
        || e.exile_play_permissions
            .iter()
            .any(|r| same(r.object, r.zone_change_generation))
        || e.pending_kicked_source.is_some_and(|id| pool.contains(&id))
    {
        return diagnose_refusal("references.current_native_binding", true);
    }
    // New committed authority must not be relabeled by hidden pool resampling,
    // including after Cleanup removed the active shield. Predecessor histories
    // contain no such event and retain their original sampling behavior.
    if e.event_history
        .scan_for_v1(&[EventKindV1::ExileIfDiesReplacementInstalledV1])
        .iter()
        .any(|event| match event {
            crate::event::CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement } => {
                pool.contains(&replacement.source)
                    || match replacement.kind {
                        crate::event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
                            target: crate::state::StackTargetContractV4::Object { object, .. },
                            ..
                        } => pool.contains(&object),
                        _ => false,
                    }
            }
            _ => false,
        })
    {
        return true;
    }
    // Conservative exclusions for unversioned references. Do not pin or redraw.
    if e.active_replacements.iter().any(|r| {
        pool.contains(&r.source)
            || match r.kind {
                crate::event::ReplacementEffectKind::PreventNextDamage {
                    target: crate::state::Target::Object(id),
                    ..
                } => pool.contains(&id),
                crate::event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
                    target: crate::state::StackTargetContractV4::Object { object, .. },
                    ..
                } => pool.contains(&object),
                _ => false,
            }
    }) {
        return true;
    }
    e.until_end_of_turn.iter().any(|r| {
        use crate::engine::UntilEndOfTurnEffect::*;
        match r {
            SyntheticMarker(id) => pool.contains(id),
            ResolvedSetEffect { object_ids, .. } => object_ids.iter().any(|id| pool.contains(id)),
            ResolvedObjectEffect {
                object_id,
                object_zone_change_count,
                ..
            }
            | ResolvedObjectKeywordEffect {
                object_id,
                object_zone_change_count,
                ..
            }
            | ResolvedObjectCantBlock {
                object_id,
                object_zone_change_count,
                ..
            }
            | ResolvedObjectControlEffect {
                object_id,
                object_zone_change_count,
                ..
            }
            | ResolvedObjectCantBeBlockedV1 {
                object_id,
                object_zone_change_count,
                ..
            } => same(*object_id, *object_zone_change_count),
            DamageCannotBePrevented { .. } => false,
        }
    })
}

fn draw(rng: &mut SplitMix64, bound: u64) -> usize {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

/// Called only on a disposable cloned state. Failure never permits retrying a seed.
#[cfg(test)]
pub(super) fn redeterminize(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    redeterminize_with_library_plan(state, actor, seed, None)
}
pub(super) fn redeterminize_with_library_plan(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
) -> Result<(), Error> {
    redeterminize_with_library_plan_inner(state, actor, seed, plan, false, SamplingScopeV68::Legacy)
}
pub(super) fn redeterminize_with_library_plan_classified(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
) -> Result<(), Error> {
    redeterminize_with_library_plan_inner(state, actor, seed, plan, true, SamplingScopeV68::Legacy)
}
pub(super) fn redeterminize_end_turn_v68(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    // Validate all new factual journals before either owner's first permutation.
    crate::end_turn_observation_v68::validate_state(state)
        .map_err(|_| Error::HiddenStateContract)?;
    if crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true) {
        return Err(Error::HiddenReferenceConflict);
    }
    redeterminize_with_library_plan_inner(state, actor, seed, None, true, SamplingScopeV68::EndTurn)
}
pub(super) fn redeterminize_call_v62(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::call_observation_v62::validate_state(state).map_err(|_| Error::HiddenStateContract)?;
    redeterminize_with_library_plan_inner(state, actor, seed, None, true, SamplingScopeV68::Call)
}
pub(super) fn redeterminize_genesis_wave_v69(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::genesis_wave_observation_v69::validate_state(state)
        .map_err(|_| Error::HiddenStateContract)?;
    // Exact enclosing entry checkpoints retain a whole original operation.
    // They need their own current-slot continuation, never a pinned old library.
    if crate::linked_aura_operation_v1::pending_original_operation(state).is_some() {
        return crate::linked_aura_operation_v1::sample_genesis_checkpoint_v69(state, actor, seed)
            .map_err(|_| Error::HiddenReferenceConflict);
    }
    if crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true) {
        return Err(Error::HiddenReferenceConflict);
    }
    redeterminize_with_library_plan_inner(
        state,
        actor,
        seed,
        None,
        true,
        SamplingScopeV68::GenesisWave,
    )
}
pub(super) fn redeterminize_muldrotha_v71(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::muldrotha_observation_v71::validate_state(state)
        .map_err(|_| Error::HiddenStateContract)?;
    // Entry checkpoints retain whole original worlds. V69's replay derivation
    // cannot authenticate a V71 world, so this scope preserves that refusal.
    if crate::linked_aura_operation_v1::pending_original_operation(state).is_some() {
        return Err(diagnose_refusal(
            "v71.entry_checkpoint",
            Error::HiddenReferenceConflict,
        ));
    }
    if crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true) {
        return Err(diagnose_refusal(
            "v71.time_stop",
            Error::HiddenReferenceConflict,
        ));
    }
    if crate::engine::muldrotha_v1::sampler_conflicts_v71(state, &[]).unwrap_or(true) {
        return Err(diagnose_refusal(
            "v71.permission_history",
            Error::HiddenReferenceConflict,
        ));
    }
    redeterminize_with_library_plan_inner(
        state,
        actor,
        seed,
        None,
        true,
        SamplingScopeV68::Muldrotha,
    )
    .map_err(|error| diagnose_refusal("v71.current_slots", error))
}
pub(super) fn redeterminize_tinybones_v72(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::tinybones_observation_v72::validate_state(state)
        .map_err(|_| Error::HiddenStateContract)?;
    // Entry checkpoints retain whole original worlds. V69's replay derivation
    // cannot authenticate a V72 world, so this scope preserves that refusal.
    if crate::linked_aura_operation_v1::pending_original_operation(state).is_some()
        || crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true)
        || crate::engine::muldrotha_v1::sampler_conflicts_v71(state, &[]).unwrap_or(true)
        || crate::engine::tinybones_v72::sampler_conflicts_v72(state, &[]).unwrap_or(true)
    {
        return Err(Error::HiddenReferenceConflict);
    }
    redeterminize_with_library_plan_inner(
        state,
        actor,
        seed,
        None,
        true,
        SamplingScopeV68::Tinybones,
    )
}
pub(super) fn redeterminize_liliana_v73(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::liliana_observation_v73::validate_state(state)
        .map_err(|_| Error::HiddenStateContract)?;
    // Entry checkpoints retain whole original worlds. V69's replay derivation
    // cannot authenticate a V73 world, so this scope preserves that refusal.
    if crate::linked_aura_operation_v1::pending_original_operation(state).is_some()
        || crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true)
        || crate::engine::muldrotha_v1::sampler_conflicts_v71(state, &[]).unwrap_or(true)
        || crate::engine::tinybones_v72::sampler_conflicts_v72(state, &[]).unwrap_or(true)
        || crate::effect::liliana_dreadhorde_v1::sampler_conflicts_v73(state, &[]).unwrap_or(true)
    {
        return Err(Error::HiddenReferenceConflict);
    }
    redeterminize_with_library_plan_inner(state, actor, seed, None, true, SamplingScopeV68::Liliana)
}
pub(super) fn redeterminize_chandra_v74(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::chandra_observation_v74::validate_state(state)
        .map_err(|_| Error::HiddenStateContract)?;
    // Entry checkpoints retain whole original worlds. V69's replay derivation
    // cannot authenticate a V74 world, so this scope preserves that refusal.
    if crate::linked_aura_operation_v1::pending_original_operation(state).is_some()
        || crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true)
        || crate::engine::muldrotha_v1::sampler_conflicts_v71(state, &[]).unwrap_or(true)
        || crate::engine::tinybones_v72::sampler_conflicts_v72(state, &[]).unwrap_or(true)
        || crate::effect::liliana_dreadhorde_v1::sampler_conflicts_v73(state, &[]).unwrap_or(true)
        || crate::effect::chandra_flameshaper_v1::sampler_conflicts_v74(state, &[]).unwrap_or(true)
    {
        return Err(Error::HiddenReferenceConflict);
    }
    redeterminize_with_library_plan_inner(state, actor, seed, None, true, SamplingScopeV68::Chandra)
}
pub(super) fn redeterminize_kaito_v75(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    crate::kaito_observation_v75::validate_state(state).map_err(|_| Error::HiddenStateContract)?;
    // Entry checkpoints retain whole original worlds. V69's replay derivation
    // cannot authenticate a V75 world, so this scope preserves that refusal.
    if crate::linked_aura_operation_v1::pending_original_operation(state).is_some()
        || crate::engine::time_stop_v1::sampler_conflicts_v68(state, &[]).unwrap_or(true)
        || crate::engine::muldrotha_v1::sampler_conflicts_v71(state, &[]).unwrap_or(true)
        || crate::engine::tinybones_v72::sampler_conflicts_v72(state, &[]).unwrap_or(true)
        || crate::effect::liliana_dreadhorde_v1::sampler_conflicts_v73(state, &[]).unwrap_or(true)
        || crate::effect::chandra_flameshaper_v1::sampler_conflicts_v74(state, &[]).unwrap_or(true)
        || crate::effect::kaito_cunning_v75::sampler_conflicts_v75(state, &[]).unwrap_or(true)
    {
        return Err(Error::HiddenReferenceConflict);
    }
    redeterminize_with_library_plan_inner(state, actor, seed, None, true, SamplingScopeV68::Kaito)
}
fn current_unknown_slots(
    state: &GameState,
    actor: PlayerId,
    owner: PlayerId,
) -> Vec<(Zone, usize, ObjectId)> {
    let mut slots = Vec::new();
    if owner != actor {
        for (i, &id) in state.players[owner.index()].hand.iter().enumerate() {
            let obj = state.objects.get(id);
            if !state
                .known_hand_cards(actor, owner)
                .iter()
                .any(|k| k.object == id && k.zone_change_count == obj.zone_change_count)
            {
                slots.push((Zone::Hand, i, id));
            }
        }
    }
    for (i, &id) in state.players[owner.index()].library.iter().enumerate() {
        let obj = state.objects.get(id);
        if !state.known_library_cards(actor, owner).iter().any(|k| {
            k.position as usize == i
                && k.object == id
                && k.zone_change_count == obj.zone_change_count
        }) {
            slots.push((Zone::Library, i, id));
        }
    }
    slots
}
pub(super) fn same_current_unknown_pool_v69(
    before: &GameState,
    current: &GameState,
    actor: PlayerId,
) -> bool {
    [PlayerId::P0, PlayerId::P1].into_iter().all(|owner| {
        let a = current_unknown_slots(before, actor, owner);
        let b = current_unknown_slots(current, actor, owner);
        let shape =
            |v: &[(Zone, usize, ObjectId)]| v.iter().map(|(z, i, _)| (*z, *i)).collect::<Vec<_>>();
        let pool = |s: &GameState, v: &[(Zone, usize, ObjectId)]| {
            let mut ids = v
                .iter()
                .map(|(_, _, id)| (*id, s.objects.get(*id).zone_change_count))
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids
        };
        shape(&a) == shape(&b) && pool(before, &a) == pool(current, &b)
    })
}
fn redeterminize_with_library_plan_inner(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
    classified: bool,
    scope: SamplingScopeV68,
) -> Result<(), Error> {
    // Refuse before even the first hidden-owner permutation, including empty pools.
    if crate::bloodthirsty_conqueror_v49::has_history(state)
        || crate::soul_counters_v1::has_history(state)
    {
        return Err(if classified {
            Error::HiddenReferenceConflict
        } else {
            Error::HiddenStateContract
        });
    }
    if state.extra_turn_v1.is_some()
        || state
            .engine
            .event_history
            .scan_for_v1(&[
                EventKindV1::ExtraTurnBeganV1,
                EventKindV1::ExtraTurnOwnerPublishedV1,
                EventKindV1::ExtraTurnOwnerRemovedV1,
                EventKindV1::ExtraTurnScheduledV1,
                EventKindV1::ExtraTurnStartedV1,
                EventKindV1::ExtraTurnVirtualCeasedV1,
            ])
            .iter()
            .any(|event| {
                matches!(
                    event,
                    crate::event::CommittedEvent::ExtraTurnOwnerPublishedV1 { .. }
                        | crate::event::CommittedEvent::ExtraTurnOwnerRemovedV1 { .. }
                        | crate::event::CommittedEvent::ExtraTurnVirtualCeasedV1 { .. }
                        | crate::event::CommittedEvent::ExtraTurnScheduledV1 { .. }
                        | crate::event::CommittedEvent::ExtraTurnStartedV1 { .. }
                        | crate::event::CommittedEvent::ExtraTurnBeganV1 { .. }
                )
            })
    {
        return Err(Error::UnsupportedActionContract);
    }
    let observer = actor.opponent();
    if !state.hand_knowledge[observer.index()][observer.index()].is_empty() {
        return Err(Error::HiddenStateContract);
    }
    // Validate both owners against the original world before changing either
    // pool. A historical reference in the second pool must refuse read-only.
    let mut pools = Vec::with_capacity(2);
    for owner in [PlayerId::P0, PlayerId::P1] {
        // Slots are canonical: owner order, then hand indices, then library indices.
        let slots = current_unknown_slots(state, actor, owner);
        let mut pool: Vec<_> = slots.iter().map(|x| x.2).collect();
        pool.sort_unstable();
        if pool.windows(2).any(|x| x[0] == x[1]) {
            return Err(Error::HiddenStateContract);
        }
        if match scope {
            SamplingScopeV68::Legacy => conflicts(state, &pool, plan),
            SamplingScopeV68::EndTurn
            | SamplingScopeV68::Call
            | SamplingScopeV68::GenesisWave
            | SamplingScopeV68::Muldrotha
            | SamplingScopeV68::Tinybones
            | SamplingScopeV68::Liliana
            | SamplingScopeV68::Chandra
            | SamplingScopeV68::Kaito => conflicts_in_scope(state, &pool, plan, scope),
        } {
            return Err(if classified {
                Error::HiddenReferenceConflict
            } else {
                Error::HiddenStateContract
            });
        }
        for &(zone, _, id) in &slots {
            let obj = state.objects.get(id);
            if obj.owner != owner
                || obj.zone != zone
                || obj.spell_copy_origin.is_some()
                || !obj.attachments.is_empty()
                || obj.v4.attached_to.is_some()
                || obj.controller != owner
                || obj.tapped
                || obj.summoning_sick
                || obj.damage != 0
                || obj.counters != crate::state::Counters::default()
                || obj.plotted_turn.is_some()
                || obj.v4.is_token
                || obj.v4 != crate::state::ObjectStateV4::from_card_def(obj.card_def)
            {
                return Err(Error::HiddenStateContract);
            }
        }
        pools.push((owner, slots, pool));
    }
    let mut rng = SplitMix64::seed(seed);
    for (owner, slots, mut pool) in pools {
        for i in (1..pool.len()).rev() {
            let j = draw(&mut rng, (i + 1) as u64);
            pool.swap(i, j);
        }
        // Only library knowledge needs rebinding: own-hand knowledge is implicit,
        // and the other observer's knowledge of actor's hand is never pooled.
        for ((zone, i, old), new) in slots.iter().copied().zip(pool.iter().copied()) {
            let old_generation = state.objects.get(old).zone_change_count;
            let new_generation = state.objects.get(new).zone_change_count;
            match zone {
                Zone::Hand => {
                    state.players[owner.index()].hand[i] = new;
                }
                Zone::Library => {
                    for k in &mut state.library_knowledge[observer.index()][owner.index()] {
                        if k.position as usize == i
                            && k.object == old
                            && k.zone_change_count == old_generation
                        {
                            k.object = new;
                            k.zone_change_count = new_generation;
                        }
                    }
                    state.players[owner.index()].library[i] = new;
                }
                _ => return Err(Error::HiddenStateContract),
            }
            state.objects.get_mut(new).zone = zone;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn paid_observerless_entries_allow_scoped_sampling_but_keep_frozen_v4_and_actual_observers_bound(
    ) {
        use crate::engine::time_stop_v1::tests::{cast, finish, ready, restore};
        use crate::event::{propose_and_commit, CommittedEvent, ProposedEvent};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let elf = cast(&mut state, actor, "Llanowar Elves", &[], 1);
            finish(&mut state);
            assert!(!crate::arahbo_v1::requires_successor(&state));
            assert!(!crate::garruks_uprising_v1::requires_successor(&state));
            let history = state.engine.event_history.clone();
            assert!(history.iter().any(|event| matches!(
                event,
                CommittedEvent::ProtectionSourceColorDepartureV1 { .. }
            )));
            assert!(!super::super::effect_refs::conflicts(&state, &[], None));
            assert!(super::super::effect_refs::conflicts(&state, &[elf], None));
            let mut legacy = restore(&state);
            assert_eq!(
                redeterminize_with_library_plan_classified(&mut legacy, actor, 8177, None),
                Err(Error::HiddenReferenceConflict)
            );
            assert_eq!(legacy, state);
            let mut malformed = restore(&state);
            let snapshot = malformed
                .engine
                .event_history
                .iter_mut()
                .find_map(|event| {
                    if let CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = event
                    {
                        Some(snapshot)
                    } else {
                        None
                    }
                })
                .unwrap();
            snapshot.private_snapshot_json = "{}".into();
            let unchanged = malformed.clone();
            assert_eq!(
                redeterminize_with_library_plan_inner(
                    &mut malformed,
                    actor,
                    8177,
                    None,
                    true,
                    SamplingScopeV68::EndTurn
                ),
                Err(Error::HiddenReferenceConflict)
            );
            assert_eq!(malformed, unchanged);
            let mut sampled = restore(&state);
            let mut repeat = restore(&state);
            redeterminize_with_library_plan_inner(
                &mut sampled,
                actor,
                8177,
                None,
                true,
                SamplingScopeV68::EndTurn,
            )
            .unwrap();
            redeterminize_with_library_plan_inner(
                &mut repeat,
                actor,
                8177,
                None,
                true,
                SamplingScopeV68::EndTurn,
            )
            .unwrap();
            assert_eq!(sampled, repeat);
            assert_eq!(sampled.engine.event_history, history);

            let mut departed = restore(&state);
            propose_and_commit(
                &mut departed,
                ProposedEvent::zone_change(elf, Zone::Library),
            );
            assert!(conflicts(&departed, &[elf], None));

            let mut malformed = restore(&state);
            malformed
                .engine
                .event_history
                .retain(|event| !matches!(event, CommittedEvent::GarrukEntryObservationV1 { .. }));
            let before = malformed.clone();
            assert_eq!(
                redeterminize_with_library_plan_inner(
                    &mut malformed,
                    actor,
                    8177,
                    None,
                    true,
                    SamplingScopeV68::EndTurn
                ),
                Err(Error::HiddenReferenceConflict)
            );
            assert_eq!(malformed, before);

            cast(&mut state, actor, "Garruk's Uprising", &[], 3);
            finish(&mut state);
            let before = state.clone();
            assert_eq!(
                redeterminize_with_library_plan_inner(
                    &mut state,
                    actor,
                    8177,
                    None,
                    true,
                    SamplingScopeV68::EndTurn
                ),
                Err(Error::HiddenReferenceConflict)
            );
            assert_eq!(state, before);
        }
    }
    #[test]
    fn retained_soul_world_refuses_before_any_hidden_owner_permutation_both_seats() {
        use crate::{
            effect::ExecCtx,
            event::{add_soul_counter_to_ability_source_v1, propose_and_commit, ProposedEvent},
            policy_observation_v6::tests::put,
        };
        for actor in [PlayerId::P0, PlayerId::P1] {
            for departed in [false, true] {
                let mut state = super::super::tests::state(actor);
                let source = put(&mut state, actor, "Forest", Zone::Battlefield);
                let mut ctx = ExecCtx::no_targets(source, actor);
                ctx.ability_source_contract =
                    Some(AbilitySourceContractV4::capture(&state, source));
                add_soul_counter_to_ability_source_v1(&mut state, &ctx).unwrap();
                if departed {
                    propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
                    assert_eq!(state.objects.get(source).counters.soul, 0);
                }
                state.engine.event_log.clear();
                crate::soul_counters_v1::validate(&state).unwrap();
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for mut branch in [state, restored] {
                    let before = branch.clone();
                    assert_eq!(
                        redeterminize_with_library_plan_classified(&mut branch, actor, 8172, None),
                        Err(Error::HiddenReferenceConflict)
                    );
                    assert_eq!(branch, before);
                    assert_eq!(
                        redeterminize(&mut branch, actor, 8172),
                        Err(Error::HiddenStateContract)
                    );
                    assert_eq!(branch, before);
                }
            }
        }
    }

    #[test]
    fn v4_search_sampler_identity_repeatability_and_slot_knowledge() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = super::super::tests::state(actor);
            let other = actor.opponent();
            assert!(state.hand_knowledge[other.index()][other.index()].is_empty());
            state.reveal_library_top(other, other, 2);
            state.reveal_library_top(other, actor, 2);
            let original = state.clone();
            let mut repeat = state.clone();
            redeterminize(&mut state, actor, 8172).unwrap();
            redeterminize(&mut repeat, actor, 8172).unwrap();
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&repeat).unwrap()
            );
            for owner in [PlayerId::P0, PlayerId::P1] {
                for id in original.players[owner.index()]
                    .hand
                    .iter()
                    .chain(&original.players[owner.index()].library)
                {
                    let mut before = original.objects.get(*id).clone();
                    let after = state.objects.get(*id);
                    before.zone = after.zone;
                    assert_eq!(
                        serde_json::to_vec(&before).unwrap(),
                        serde_json::to_vec(after).unwrap()
                    );
                }
            }
            assert!(state.hand_knowledge[other.index()][other.index()].is_empty());
            for owner in [actor, other] {
                assert_eq!(
                    state.library_knowledge[other.index()][owner.index()].len(),
                    2
                );
                for k in &state.library_knowledge[other.index()][owner.index()] {
                    assert_eq!(
                        k.object,
                        state.players[owner.index()].library[k.position as usize]
                    );
                    assert_eq!(
                        k.zone_change_count,
                        state.objects.get(k.object).zone_change_count
                    );
                }
            }
            // Canonical pool makes a fixed object set independent of its unknown slot arrangement.
            let mut reordered = original.clone();
            reordered.players[actor.index()].library.reverse();
            redeterminize(&mut reordered, actor, 8172).unwrap();
            assert_eq!(
                state.players[actor.index()].library,
                reordered.players[actor.index()].library
            );
        }
    }
    #[test]
    fn v4_search_sampler_rejects_same_incarnation_without_pinning() {
        let mut state = super::super::tests::state(PlayerId::P0);
        let id = state.players[0].library[0];
        state.engine.initiative_source = Some(AbilitySourceContractV4::capture(&state, id));
        let original = serde_json::to_vec(&state).unwrap();
        assert_eq!(
            redeterminize(&mut state, PlayerId::P0, 1),
            Err(Error::HiddenStateContract)
        );
        assert_eq!(original, serde_json::to_vec(&state).unwrap());
    }
    #[test]
    fn v4_search_sampler_rejects_hidden_residue_without_normalizing() {
        let mut state = super::super::tests::state(PlayerId::P0);
        let id = state.players[0].library[0];
        state.objects.get_mut(id).v4.entered_battlefield_turn = Some(state.turn);
        let before = state.objects.get(id).clone();
        assert_eq!(
            redeterminize(&mut state, PlayerId::P0, 1),
            Err(Error::HiddenStateContract)
        );
        assert_eq!(&before, state.objects.get(id));
        let obj = state.objects.get_mut(id);
        obj.v4
            .reset_for_zone_change(obj.card_def, Zone::Library, state.turn);
        assert!(redeterminize(&mut state, PlayerId::P0, 1).is_ok());
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn bloodwitch_hidden_return_refuses_resampling_of_historical_pending_and_stack_source() {
        use crate::engine::{self, Action, Decision};
        use crate::event::{self, ProposedEvent};
        use crate::policy_observation_v6::tests::put;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let controller = actor.opponent();
            let mut state = super::super::tests::state(actor);
            let source = put(
                &mut state,
                controller,
                "Vengeful Bloodwitch",
                Zone::Battlefield,
            );
            let victim = put(&mut state, controller, "Llanowar Elves", Zone::Battlefield);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(victim, Zone::Graveyard),
            );
            let pending = crate::trigger::collect_and_process(&mut state);
            assert_eq!(pending.len(), 1);
            state.engine.pending_triggers.extend(pending);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Library),
            );
            assert!(crate::trigger::collect_and_process(&mut state).is_empty());
            assert!(state.engine.event_log.is_empty());
            assert_eq!(state.objects.get(source).zone_change_count, 1);
            assert_eq!(
                state.engine.pending_triggers[0]
                    .source_contract
                    .unwrap()
                    .zone_change_count,
                0
            );
            assert!(conflicts(&state, &[source], None));
            for _ in 0..2 {
                let original = serde_json::to_vec(&state).unwrap();
                let restored: GameState = serde_json::from_slice(&original).unwrap();
                assert!(conflicts(&restored, &[source], None));
                let mut disposable = restored;
                assert_eq!(
                    redeterminize_with_library_plan_classified(&mut disposable, actor, 991, None),
                    Err(Error::HiddenReferenceConflict)
                );
                assert_eq!(original, serde_json::to_vec(&state).unwrap());
                if state.stack.is_empty() {
                    let decision = engine::advance_until_decision(&mut state);
                    assert!(
                        matches!(decision, Decision::ChooseTargets { player, .. } if player == controller)
                    );
                    engine::step(
                        &mut state,
                        Action::ChooseTarget(crate::state::Target::Player(actor)),
                    )
                    .unwrap();
                    engine::advance_until_decision(&mut state);
                    assert_eq!(state.stack.len(), 1);
                    assert_eq!(
                        state.stack[0]
                            .v4
                            .ability_source_contract
                            .unwrap()
                            .zone_change_count,
                        0
                    );
                    assert!(crate::trigger::collect_and_process(&mut state).is_empty());
                    assert!(state.engine.event_log.is_empty());
                    assert!(conflicts(&state, &[source], None));
                }
            }
            // Removing the pending/stack root does not erase its independently
            // retained departure-color authority. The unmigrated sampler must
            // still refuse changing that historical physical source.
            state.stack.clear();
            state.engine.pending_triggers.clear();
            assert!(state.engine.event_history.iter().any(|event|
                matches!(event, crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 {snapshot,..}
                    if snapshot.source.source==source)));
            assert!(conflicts(&state, &[source], None));
            let before = state.clone();
            assert_eq!(
                redeterminize_with_library_plan_classified(&mut state, actor, 991, None),
                Err(Error::HiddenReferenceConflict)
            );
            assert_eq!(state, before);
        }
    }
    #[test]
    fn fiery_historical_installation_receipt_refuses_hidden_resampling_readonly_after_cleanup() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = super::super::tests::state(player);
            let source = state.players[player.index()].library[0];
            let target = state.players[player.opponent().index()].library[0];
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            let source_contract = AbilitySourceContractV4::capture(&state, source);
            let target_contract = crate::state::StackTargetContractV4::capture(
                &state,
                crate::state::Target::Object(target),
            );
            crate::event::install_exile_if_dies(&mut state, source_contract, target_contract);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(target, Zone::Library),
            );
            // The historical receipt survives Cleanup. No active or event-log
            // reference is needed to forbid rewriting its physical identities.
            state.engine.active_replacements.clear();
            state.engine.event_log.clear();
            assert!(conflicts(&state, &[source], None));
            assert!(conflicts(&state, &[target], None));
            let before = serde_json::to_vec(&state).unwrap();
            assert_eq!(
                redeterminize(&mut state, player, 17),
                Err(Error::HiddenStateContract)
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }

    #[test]
    fn retained_conqueror_amount_only_history_refuses_before_sampling_after_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let Some(original) =
                crate::bloodthirsty_conqueror_v49::tests::amount_only_sampler_state(actor)
            else {
                continue;
            };
            for restored in [false, true] {
                let mut state = if restored {
                    serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap()
                } else {
                    original.clone()
                };
                assert!(state.engine.event_log.is_empty());
                assert!(conflicts(&state, &[], None));
                assert!(super::super::effect_refs::conflicts(&state, &[], None));
                let bytes = serde_json::to_vec(&state).unwrap();
                assert_eq!(
                    redeterminize(&mut state, actor, 49),
                    Err(Error::HiddenStateContract)
                );
                assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
                assert_eq!(
                    redeterminize_with_library_plan_classified(&mut state, actor, 49, None),
                    Err(Error::HiddenReferenceConflict)
                );
                assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            }
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_resolved_and_countered_conqueror_history_remains_unsampleable_without_live_or_transient_root(
    ) {
        use crate::bloodthirsty_conqueror_v49::{self as conqueror, tests::future_printed};
        for actor in [PlayerId::P0, PlayerId::P1] {
            for counter in [false, true] {
                let (mut state, source) = future_printed::paid(actor);
                crate::event::propose_and_commit(
                    &mut state,
                    crate::event::ProposedEvent::life_loss(actor.opponent(), 3),
                );
                crate::engine::advance_until_decision(&mut state);
                let id = state
                    .stack
                    .iter()
                    .find(|item| {
                        item.source == source
                            && item
                                .inline_effect
                                .as_ref()
                                .is_some_and(|op| conqueror::custody::binding(op).is_some())
                    })
                    .unwrap()
                    .v4
                    .stack_item_id;
                if counter {
                    crate::engine::counter_stack_item_by_id(&mut state, id).unwrap();
                }
                future_printed::finish(&mut state);
                assert!(!state.stack.iter().any(|item| item
                    .inline_effect
                    .as_ref()
                    .is_some_and(|op| conqueror::custody::binding(op).is_some())));
                assert!(!state.engine.event_log.iter().any(|event| matches!(
                    event,
                    crate::event::CommittedEvent::ConquerorCapturedV49 { .. }
                )));
                assert_eq!(
                    state.players[actor.index()].life,
                    if counter { 20 } else { 23 }
                );
                conqueror::validate(&state).unwrap();
                let mut restored =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let bytes = serde_json::to_vec(&restored).unwrap();
                assert!(conflicts(&restored, &[], None));
                assert_eq!(
                    redeterminize(&mut restored, actor, 50),
                    Err(Error::HiddenStateContract)
                );
                assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
            }
        }
    }
}
