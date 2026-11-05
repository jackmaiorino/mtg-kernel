//! The custody validation as it was before the checked-prefix memo (main at
//! 6c73be872), verbatim but for its name: the reference the memoized
//! validation is compared with.
use super::*;

/// Validate originals and one-use custody before a public engine mutation.
/// Private current-operation tails are permitted only under the actual Runner.
pub(crate) fn validate_v0(state: &GameState) -> Result<(), String> {
    // Kind-guarded scans: a history with no event of the kind gives no rows
    // either way, and otherwise the whole history (positions included) is read.
    let rows = state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::DrakeJournalV1])
        .iter()
        .enumerate()
        .filter_map(|(position, event)| match event {
            CommittedEvent::DrakeJournalV1 { record } => Some((position, &**record)),
            _ => None,
        })
        .map(|(position, row)| {
            // An identical appended row must not borrow the earlier record's
            // index and enclosing range. Authenticate its physical position
            // before normalization, relation checks, or enclosing replay.
            if u64::try_from(position).ok() != Some(row.index) {
                return Err("Drake record index differs from actual history position".to_string());
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>, String>>()?;
    if rows.len() as u64 != state.engine.drake_ledger_v1.issued {
        return Err("Drake original journal was erased/replayed".into());
    }
    if !rows.is_empty() && !world_checked_in_frame(state) {
        // Compression must prove every discarded nested original equals the
        // whole actual indexed record before any caller can normalize it.
        // Only a success on the frame's own immutable state is remembered.
        world(state)?;
        remember_world_checked_in_frame(state);
    }
    if state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::DrakeHistoryRefV1])
        .iter()
        .any(|e| matches!(e, CommittedEvent::DrakeHistoryRefV1 { .. }))
    {
        return Err("Drake nested original reference escaped into actual history".into());
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|e| journal(e).is_some() || matches!(e, CommittedEvent::DrakeHistoryRefV1 { .. }))
    {
        return Err("Drake private authority entered trigger log".into());
    }
    let local = crate::linked_aura_operation_v1::locally_owned(state);
    let verifying = VERIFYING.with(Cell::get);
    let keys = if verifying
        || !rows
            .iter()
            .any(|row| matches!(row.value, JournalV1::Enclosing { .. }))
    {
        BTreeMap::new()
    } else {
        replay_keys(&state.engine.event_history)
    };
    let mut verified_keys = Vec::new();
    // Indexed once: scanning every row again for each row made validation
    // quadratic in the rows. Rows are in history order, so each enclosing
    // operation covers one contiguous run of them.
    let mut referencing = HashMap::<(RowReferenceV1, u64), Vec<&RecordV1>>::new();
    let mut covering = vec![0_i64; rows.len() + 1];
    for (position, row) in rows.iter().enumerate() {
        if let Some(target) = reference(&row.value) {
            referencing.entry(target).or_default().push(row);
        }
        if let JournalV1::Enclosing { start, .. } = row.value {
            covering[rows.partition_point(|r| r.index < start).min(position)] += 1;
            covering[position] -= 1;
        }
    }
    let refs = |kind: RowReferenceV1, target: u64| {
        referencing
            .get(&(kind, target))
            .map_or(&[][..], Vec::as_slice)
    };
    let mut coverage = 0;
    let last_closed = rows
        .iter()
        .filter(|r| matches!(r.value, JournalV1::Enclosing { .. }))
        .map(|r| r.index)
        .max();
    for (position, row) in rows.iter().enumerate() {
        coverage += covering[position];
        if state
            .engine
            .event_history
            .get(row.index as usize)
            .and_then(journal)
            != Some(*row)
        {
            return Err("Drake record index moved".into());
        }
        if !matches!(row.value, JournalV1::Enclosing { .. })
            && coverage != 1
            && !(local && last_closed.is_none_or(|last| row.index > last))
        {
            return Err("Drake record lost its unique original enclosing operation".into());
        }
        match &row.value {
            JournalV1::CombatInput { world, proposals } => {
                let genuine = proposals.iter().any(|p| matches!(p, ProposedEvent::Damage(d) if d.is_combat && fresh_source(world, d.source)));
                if !world.engine.event_history.is_empty() || !genuine {
                    return Err("Drake pre-batch input changed".into());
                }
            }
            JournalV1::CombatFinished { input } => {
                if !matches!(at(state, *input)?.value, JournalV1::CombatInput { .. })
                    || *input >= row.index
                {
                    return Err("Drake batch finished without earlier original input".into());
                }
            }
            JournalV1::Captured {
                input,
                damage,
                marker,
                source: original,
                player,
                amount,
            } => {
                let JournalV1::CombatInput { world, .. } = &at(state, *input)?.value else {
                    return Err("Drake capture input absent".into());
                };
                if *amount <= 0
                    || original.zone != Zone::Battlefield
                    || !definition(original.card_def)
                    || world.objects.try_get(original.source).is_none()
                    || AbilitySourceContractV4::capture(world, original.source) != *original
                    || !crate::continuous_characteristics_v1::printed_abilities_active(
                        world,
                        original.source,
                    )
                    || !(*input < *damage && *damage < *marker && *marker < row.index)
                {
                    return Err("Drake original combat source/controller/quantity changed".into());
                }
                let exact_damage = matches!(state.engine.event_history.get(*damage as usize), Some(CommittedEvent::Damage { source, target: crate::state::Target::Player(hit), amount: n }) if *source == original.source && hit == player && n == amount);
                let exact_marker = matches!(state.engine.event_history.get(*marker as usize), Some(CommittedEvent::CombatDamageToPlayer { source, source_zone_change_count, player: hit, amount: n }) if *source == original.source && *source_zone_change_count == original.zone_change_count && hit == player && n == amount);
                if !exact_damage || !exact_marker {
                    return Err(
                        "Drake capture no longer matches actual final committed damage/marker"
                            .into(),
                    );
                }
                let collections = refs(RowReferenceV1::Collected, row.index);
                if collections.len() > 1 || (!local && collections.len() != 1) {
                    return Err("Drake capture collection lost/duplicated".into());
                }
            }
            JournalV1::Collected {
                capture,
                pending: saved,
            } => {
                let (original, _) = source(state, *capture)?;
                if *capture >= row.index || saved.as_ref() != &pending(original, *capture) {
                    return Err("Drake collected input changed".into());
                }
                let publications = refs(RowReferenceV1::Published, *capture);
                let held_count = held(state)
                    .filter(|p| capture_of(&p.effect) == Some(*capture))
                    .count();
                if publications.len() > 1
                    || held_count > 1
                    || (!local && publications.len() + held_count != 1)
                {
                    return Err("Drake APNAP occurrence disappeared/replayed".into());
                }
            }
            JournalV1::Published { capture, item } => {
                let (original, _) = source(state, *capture)?;
                let collections = refs(RowReferenceV1::Collected, *capture);
                if **item != trigger_item(original, *capture, item.v4.stack_item_id)
                    || item.v4.stack_item_id == StackItemId::default()
                    || collections.len() != 1
                    || collections[0].index >= row.index
                {
                    return Err("Drake publication no longer owns collected source".into());
                }
            }
            JournalV1::Debit {
                source,
                kind,
                count,
                before,
                after,
            } => {
                if *kind != SourceCounterKindV1::Incubation
                    || *count == 0
                    || *before < 0
                    || before.checked_sub(i32::from(*count)) != Some(*after)
                    || *after < 0
                    || source.zone != Zone::Battlefield
                {
                    return Err("Drake original typed debit changed".into());
                }
                if !local && refs(RowReferenceV1::Paid, row.index).len() != 1 {
                    return Err("Drake debit lost unique paid publication".into());
                }
            }
            JournalV1::Paid {
                debit,
                original,
                pending: saved,
                item,
            } => {
                let JournalV1::Debit {
                    source,
                    kind,
                    count,
                    before,
                    after,
                } = &at(state, *debit)?.value
                else {
                    return Err("Drake payment debit absent".into());
                };
                if !original.engine.event_history.is_empty()
                    || *debit >= row.index
                    || *kind != SourceCounterKindV1::Incubation
                    || *count != 3
                    || !definition(source.card_def)
                    || **item != activation_item(*source, item.v4.stack_item_id)
                    || item.v4.stack_item_id == StackItemId::default()
                    || saved.source != source.source
                    || saved.controller != source.controller
                    || saved.source_zone_change_count != source.zone_change_count
                    || saved.ability_index != 0
                    || saved.cauldron_grant.0.is_some()
                    || saved.target_spec != TargetSpec::None
                    || !saved.targets_chosen.is_empty()
                    || !saved.target_contracts.is_empty()
                    || !saved.object_cost_chosen.is_empty()
                    || !crate::continuous_characteristics_v1::printed_abilities_active(
                        original,
                        source.source,
                    )
                    || original
                        .objects
                        .try_get(source.source)
                        .is_none_or(|o| o.counters.incubation != *before)
                    || AbilitySourceContractV4::capture(original, source.source) != *source
                    || before.checked_sub(3) != Some(*after)
                {
                    return Err("Drake paid source/counter/input changed".into());
                }
            }
            JournalV1::Removed {
                publication,
                item,
                kind,
                original,
            } => {
                let published = at(state, *publication)?;
                match &published.value {
                    JournalV1::Published { item: saved, .. }
                    | JournalV1::Paid { item: saved, .. }
                        if saved == item => {}
                    _ => return Err("Drake removal lost original publication".into()),
                }
                after_remove(original, item, *kind)?;
                if *publication >= row.index || !original.engine.event_history.is_empty() {
                    return Err("Drake removal input moved/nested".into());
                }
                if !local && refs(RowReferenceV1::Completed, row.index).len() != 1 {
                    return Err("Drake removal lost unique completion".into());
                }
            }
            JournalV1::Completed { removed, output } => {
                if *removed >= row.index
                    || !matches!(at(state, *removed)?.value, JournalV1::Removed { .. })
                    || !output.engine.event_history.is_empty()
                {
                    return Err("Drake completion lost original removal/output".into());
                }
            }
            JournalV1::Enclosing {
                start,
                original,
                operation,
                answers,
                used_banner,
                output,
            } => {
                if *start > row.index
                    || !original.engine.event_history.is_empty()
                    || !output.engine.event_history.is_empty()
                {
                    return Err("Drake whole operation input/output prefix changed".into());
                }
                if state.engine.event_history.get(..*start as usize).is_none() {
                    return Err("Drake operation original prefix absent".into());
                }
                // `rows` is every journal event in history order, so this
                // counts the prefix's journal events without scanning it.
                if original.engine.drake_ledger_v1.issued
                    != rows.partition_point(|r| r.index < *start) as u64
                {
                    return Err("Drake operation original ledger changed".into());
                }
                if !verifying {
                    let key = keys.get(&row.index);
                    let hit = key.is_some_and(replay_cached);
                    #[cfg(all(
                        test,
                        feature = "limited-fdn-fixtures",
                        not(feature = "standard-magezero-fixtures")
                    ))]
                    count_replay(hit, key.is_some());
                    if !hit {
                        verification(|| {
                            let input = restore_world(
                                original,
                                &state.engine.event_history,
                                *start as usize,
                            )?;
                            crate::linked_aura_operation_v1::as_recorded(|| {
                                let replay = {
                                    #[cfg(test)]
                                    let _timing = crate::rl_session::test_timing_v1::span(
                                        crate::rl_session::test_timing_v1::Phase::DrakeActualEnclosingReplay,
                                    );
                                    crate::linked_aura_operation_v1::replay_drake_enclosing_v1(
                                        &input,
                                        operation,
                                        answers,
                                        *used_banner,
                                    )?
                                };
                                if replay.engine.event_history.get(*start as usize..)
                                    != state
                                        .engine
                                        .event_history
                                        .get(*start as usize..row.index as usize)
                                    || world(&replay)? != **output
                                {
                                    return Err("Drake complete enclosing original operation/history replay differs".into());
                                }
                                Ok(())
                            })
                        })?;
                        if let Some(key) = key {
                            verified_keys.push(*key);
                        }
                    }
                }
            }
        }
        if let JournalV1::Published { item, .. } | JournalV1::Paid { item, .. } = &row.value {
            let removals = refs(RowReferenceV1::Removed, row.index);
            let lives = state
                .stack
                .iter()
                .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
                .collect::<Vec<_>>();
            let live_once =
                matches!((lives.as_slice(), removals), ([live], []) if *live == item.as_ref());
            let disposed = crate::engine::time_stop_v1::disposed(state, item)?;
            if (disposed && (!lives.is_empty() || !removals.is_empty()))
                || removals.len() > 1
                || !(local || live_once || lives.is_empty() && removals.len() == 1 || disposed)
            {
                return Err("Drake issued stack row lost/duplicated/replayed".into());
            }
        }
    }
    for p in held(state) {
        if let Some(capture) = capture_of(&p.effect) {
            let (original, _) = source(state, capture)?;
            let mut exact = p.clone();
            exact.placement_ordered = false;
            if exact != pending(original, capture) {
                return Err("Drake held APNAP row changed".into());
            }
        }
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    // Even successful pure predicates are not remembered after a later error.
    for key in verified_keys {
        remember_replay(key);
    }
    Ok(())
}
