//! Exact once ownership of the genuine attack occurrence. Original journals are
//! trusted engine provenance, not cryptographic protection of an entire save.
use crate::effect::{EffectObjectBinding, EffectOp};
use crate::engine::UntilEndOfTurnEffect;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, Target, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV2 {
    Attack {
        declaration: u64,
        source: AbilitySourceContractV4,
        turn: u32,
        active: PlayerId,
    },
    Capture {
        attack: u64,
    },
    Collected {
        capture: u64,
    },
    Published {
        capture: u64,
        item: Box<StackItem>,
    },
    PlacementRemoved {
        capture: u64,
        pending: Box<PendingTrigger>,
    },
    Removed {
        capture: u64,
        item: Box<StackItem>,
        countered: bool,
        legal: bool,
        power: i32,
    },
    Installed {
        capture: u64,
        effects: Vec<UntilEndOfTurnEffect>,
    },
    Operation {
        capture: u64,
        removal: u64,
        installation: Option<u64>,
    },
    Completed {
        capture: u64,
        operation: u64,
    },
    Expired {
        installation: u64,
        cleanup: u64,
    },
    Cleanup {
        installations: Vec<u64>,
        turn: u32,
        ending_player: PlayerId,
    },
}
fn journal(state: &GameState, at: u64) -> Result<&JournalV2, String> {
    match usize::try_from(at)
        .ok()
        .and_then(|at| state.engine.event_history.get(at))
    {
        Some(CommittedEvent::AshrootDispatchV2 { journal }) => Ok(journal),
        _ => Err("Ashroot lost its original occurrence journal".into()),
    }
}
fn append(state: &mut GameState, journal: JournalV2) -> u64 {
    let at = u64::try_from(state.engine.event_history.len()).expect("bounded event history");
    state
        .engine
        .event_history
        .push(CommittedEvent::AshrootDispatchV2 { journal });
    at
}
pub(crate) fn capture_of(op: &EffectOp) -> Option<u64> {
    match op {
        EffectOp::AshrootBoostFromCaptureV2 {
            capture_history_index,
        } => Some(*capture_history_index),
        _ => None,
    }
}
fn root(capture: u64) -> EffectOp {
    EffectOp::AshrootBoostFromCaptureV2 {
        capture_history_index: capture,
    }
}
pub(crate) fn source_at(
    state: &GameState,
    capture: u64,
) -> Result<AbilitySourceContractV4, String> {
    let JournalV2::Capture { attack } = journal(state, capture)? else {
        return Err("Ashroot capture changed kind".into());
    };
    let JournalV2::Attack {
        declaration,
        source,
        ..
    } = journal(state, *attack)?
    else {
        return Err("Ashroot original attacker missing".into());
    };
    if declaration.checked_add(1) != Some(*attack)
        || attack.checked_add(1) != Some(capture)
        || !crate::ashroot_animist_v1::owns_definition(source.card_def)
        || source.zone != Zone::Battlefield
        || !matches!(state.engine.event_history.get(*declaration as usize),
            Some(CommittedEvent::DeclaredAttacker { source: id, source_zone_change_count, controller })
                if *id == source.source && *source_zone_change_count == source.zone_change_count && *controller == source.controller)
    {
        return Err("Ashroot original declaration/controller changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, *source)?;
    Ok(*source)
}
/// Called immediately after the actual declaration is committed, before SBA.
pub(crate) fn record_attack(state: &mut GameState, source: ObjectId) {
    let live = state.objects.get(source);
    if !crate::ashroot_animist_v1::owns_definition(live.card_def)
        || live.v4.face_index != 0
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, source);
    let declaration = (state.engine.event_history.len() - 1) as u64;
    let attack = append(
        state,
        JournalV2::Attack {
            declaration,
            source,
            turn: state.turn,
            active: state.active_player,
        },
    );
    let capture = CommittedEvent::AshrootDispatchV2 {
        journal: JournalV2::Capture { attack },
    };
    state.engine.event_history.push(capture.clone());
    state.engine.event_log.push(capture);
}
fn indices(state: &GameState) -> Vec<u64> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            matches!(
                e,
                CommittedEvent::AshrootDispatchV2 {
                    journal: JournalV2::Capture { .. }
                }
            )
            .then_some(at as u64)
        })
        .collect()
}
fn rows(state: &GameState, capture: u64) -> Vec<(u64, &JournalV2)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::AshrootDispatchV2 { journal: j }
                if match j {
                    JournalV2::Collected { capture: c }
                    | JournalV2::Published { capture: c, .. }
                    | JournalV2::PlacementRemoved { capture: c, .. }
                    | JournalV2::Removed { capture: c, .. }
                    | JournalV2::Installed { capture: c, .. }
                    | JournalV2::Operation { capture: c, .. }
                    | JournalV2::Completed { capture: c, .. } => *c == capture,
                    _ => false,
                } =>
            {
                Some((at as u64, j))
            }
            _ => None,
        })
        .collect()
}
pub(crate) fn validate_trigger(
    state: &GameState,
    op: &EffectOp,
    id: ObjectId,
    controller: PlayerId,
    contract: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let Some(capture) = capture_of(op) else {
        if contract.is_some_and(|s| crate::ashroot_animist_v1::owns_definition(s.card_def)) {
            return Err("Ashroot erased its exact attack binding".into());
        }
        return Ok(());
    };
    let source = source_at(state, capture)?;
    if contract != Some(source) || id != source.source || controller != source.controller {
        return Err("Ashroot original attack beneficiary changed".into());
    }
    Ok(())
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    let mut result = Vec::new();
    let mut used = Vec::new();
    for capture in indices(state) {
        let source = source_at(state, capture).map_err(|_| ObjectId(0))?;
        let collected = rows(state, capture)
            .iter()
            .filter(|(_, j)| matches!(j, JournalV2::Collected { .. }))
            .count();
        let marker = &state.engine.event_history[capture as usize];
        let logged = events.iter().filter(|e| *e == marker).count();
        if collected > 1 || (collected == 0 && logged != 1) || (collected == 1 && logged != 0) {
            return Err(source.source);
        }
        if collected == 0 {
            used.push(capture);
            result.push(PendingTrigger {
                controller: source.controller,
                source: source.source,
                effect: root(capture),
                source_contract: Some(source),
                target_spec: crate::card_def::TargetSpec::AnotherControlledCreature,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                is_madness_offer: false,
                kicked: false,
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            });
        }
    }
    for event in events {
        if matches!(
            event,
            CommittedEvent::AshrootDispatchV2 {
                journal: JournalV2::Capture { .. }
            }
        ) && !used
            .iter()
            .any(|at| state.engine.event_history[*at as usize] == *event)
        {
            return Err(ObjectId(0));
        }
    }
    for capture in used {
        append(state, JournalV2::Collected { capture });
    }
    Ok(result)
}
fn validate_item(state: &GameState, capture: u64, item: &StackItem) -> Result<(), String> {
    let source = source_at(state, capture)?;
    if item.source != source.source
        || item.controller != source.controller
        || item.kind != StackItemKind::TriggeredAbility
        || item.inline_effect != Some(root(capture))
        || item.v4.ability_source_contract != Some(source)
        || item.v4.stack_item_id == Default::default()
        || item.is_copy
        || item.madness_offer
        || item.is_flashback
        || item.kicked
        || item.v4.granted_by.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.activated_ability_index.is_some()
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || item.v4.target_spec != Some(crate::card_def::TargetSpec::AnotherControlledCreature)
        || item.targets.len() != 1
        || item.v4.target_contracts.len() != 1
    {
        return Err("Ashroot changed its exact published attack trigger".into());
    }
    let crate::state::StackTargetContractV4::Object {
        object,
        zone: Zone::Battlefield,
        zone_change_count,
        ..
    } = item.v4.target_contracts[0]
    else {
        return Err("Ashroot target contract changed".into());
    };
    if item.targets[0] != Target::Object(object) || !state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::Targeted { target, target_zone_change_count, targeting_stack_item, targeting_controller }
        if *target == object && *target_zone_change_count == zone_change_count && *targeting_stack_item == item.v4.stack_item_id && *targeting_controller == item.controller)) {
        return Err("Ashroot publication lost actual target occurrence".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = item.inline_effect.as_ref().and_then(capture_of) else {
        return Ok(());
    };
    validate_item(state, capture, item)?;
    if rows(state, capture)
        .iter()
        .any(|(_, j)| matches!(j, JournalV2::Published { .. }))
    {
        return Err("Ashroot publication repeated".into());
    }
    append(
        state,
        JournalV2::Published {
            capture,
            item: Box::new(item.clone()),
        },
    );
    Ok(())
}
pub(crate) fn unavailable(state: &mut GameState, pending: &PendingTrigger) -> Result<(), String> {
    let Some(capture) = capture_of(&pending.effect) else {
        return Ok(());
    };
    validate_trigger(
        state,
        &pending.effect,
        pending.source,
        pending.controller,
        pending.source_contract,
    )?;
    if crate::engine::pending_trigger_targets_can_complete(pending, state) {
        return Err("Ashroot legal placement silently dropped".into());
    }
    let removal = append(
        state,
        JournalV2::PlacementRemoved {
            capture,
            pending: Box::new(pending.clone()),
        },
    );
    let operation = append(
        state,
        JournalV2::Operation {
            capture,
            removal,
            installation: None,
        },
    );
    append(state, JournalV2::Completed { capture, operation });
    Ok(())
}
pub(crate) struct RemovedOwnerV2 {
    item: StackItem,
    capture: u64,
    power: Option<i32>,
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovedOwnerV2>, String> {
    let Some(capture) = item.inline_effect.as_ref().and_then(capture_of) else {
        return Ok(None);
    };
    validate_state(state)?;
    validate_item(state, capture, item)?;
    if state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>()
        != [item]
    {
        return Err("Ashroot removal lost unique original live item".into());
    }
    Ok(Some(RemovedOwnerV2 {
        item: item.clone(),
        capture,
        power: None,
    }))
}
pub(crate) fn acquire_resolution(
    state: &GameState,
    item: &StackItem,
    legal: bool,
) -> Result<Option<RemovedOwnerV2>, String> {
    let mut owner = acquire(state, item)?;
    if let Some(owner) = &mut owner {
        owner.power = Some(if legal {
            crate::ashroot_animist_v1::sample_power(state, source_at(state, owner.capture)?)?
        } else {
            0
        });
    }
    Ok(owner)
}
fn complete(
    state: &mut GameState,
    owner: &RemovedOwnerV2,
    countered: bool,
    legal: bool,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Ashroot original loan was not removed".into());
    }
    let power = if !countered && legal {
        owner
            .power
            .ok_or("Ashroot lost genuine pre-removal power input")?
    } else {
        0
    };
    let removal = append(
        state,
        JournalV2::Removed {
            capture: owner.capture,
            item: Box::new(owner.item.clone()),
            countered,
            legal,
            power,
        },
    );
    let installation = if !countered && legal {
        let Target::Object(target) = owner.item.targets[0] else {
            return Err("Ashroot target missing".into());
        };
        let start = state.engine.until_end_of_turn.len();
        crate::effect::install_temporary_boost(
            state,
            EffectObjectBinding {
                object: target,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: state.objects.get(target).zone_change_count,
            },
            power,
            power,
            crate::card_def::Keywords::TRAMPLE,
        );
        let effects = state.engine.until_end_of_turn[start..].to_vec();
        Some(append(
            state,
            JournalV2::Installed {
                capture: owner.capture,
                effects,
            },
        ))
    } else {
        None
    };
    let operation = append(
        state,
        JournalV2::Operation {
            capture: owner.capture,
            removal,
            installation,
        },
    );
    append(
        state,
        JournalV2::Completed {
            capture: owner.capture,
            operation,
        },
    );
    Ok(())
}
pub(crate) fn resolve(
    state: &mut GameState,
    owner: &RemovedOwnerV2,
    legal: bool,
) -> Result<(), String> {
    complete(state, owner, false, legal)
}
pub(crate) fn counter(state: &mut GameState, owner: Option<&RemovedOwnerV2>) -> Result<(), String> {
    if let Some(owner) = owner {
        complete(state, owner, true, false)?;
    }
    Ok(())
}
pub(crate) fn expire(state: &mut GameState) -> Result<(), String> {
    validate_state(state)?;
    let active: Vec<_> = state.engine.event_history.iter().enumerate().filter_map(|(at,e)| match e {
        CommittedEvent::AshrootDispatchV2 {journal:JournalV2::Installed {..}} if !state.engine.event_history.iter().any(|e| matches!(e,CommittedEvent::AshrootDispatchV2 {journal:JournalV2::Expired {installation,..}} if *installation == at as u64)) => Some(at as u64), _=>None
    }).collect();
    if active.is_empty() {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Ashroot expiry requires actual Cleanup entry".into());
    }
    let cleanup = append(
        state,
        JournalV2::Cleanup {
            installations: active.clone(),
            turn: state.turn,
            ending_player: state.active_player,
        },
    );
    for installation in active {
        append(
            state,
            JournalV2::Expired {
                installation,
                cleanup,
            },
        );
    }
    Ok(())
}
pub(crate) fn has_active(state: &GameState) -> bool {
    state.engine.event_history.iter().enumerate().any(|(at,e)| matches!(e,CommittedEvent::AshrootDispatchV2 {journal:JournalV2::Installed {..}}) && !state.engine.event_history.iter().any(|e| matches!(e,CommittedEvent::AshrootDispatchV2 {journal:JournalV2::Expired {installation,..}} if *installation == at as u64)))
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let captures = indices(state);
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::AshrootDispatchV2 { journal: j } = event else {
            continue;
        };
        match j {
            JournalV2::Attack {..} => if captures.iter().filter(|c| matches!(journal(state,**c),Ok(JournalV2::Capture {attack}) if *attack == at as u64)).count()!=1 { return Err("Ashroot declaration lost mandatory capture".into()); },
            JournalV2::Capture {..} => { source_at(state,at as u64)?; }
            JournalV2::Expired {installation,cleanup} => {
                if *installation>=at as u64 || *cleanup>=at as u64 || *cleanup<=*installation || !matches!(journal(state,*installation)?,JournalV2::Installed {..}) || !matches!(journal(state,*cleanup)?,JournalV2::Cleanup {installations,..} if installations.iter().filter(|i|*i==installation).count()==1) {return Err("Ashroot orphan expiry".into());}
            }
            JournalV2::Cleanup {installations,..} => {
                if installations.is_empty() {return Err("Ashroot empty cleanup journal".into());}
                for installation in installations {if *installation>=at as u64 || !matches!(journal(state,*installation)?,JournalV2::Installed {..}) || state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::AshrootDispatchV2 {journal:JournalV2::Expired {installation:i,cleanup}} if i==installation && *cleanup==at as u64)).count()!=1 {return Err("Ashroot cleanup lost exact mandatory expiry".into());}}
            }
            JournalV2::Collected {capture} | JournalV2::Published {capture,..} | JournalV2::PlacementRemoved {capture,..} | JournalV2::Removed {capture,..} | JournalV2::Installed {capture,..} | JournalV2::Operation {capture,..} | JournalV2::Completed {capture,..} => if !captures.contains(capture) || *capture >= at as u64 { return Err("Ashroot orphan lifecycle journal".into()); }
        }
    }
    for event in &state.engine.event_log {
        if let CommittedEvent::AshrootDispatchV2 { journal: j } = event {
            if !matches!(j, JournalV2::Capture { .. })
                || !captures
                    .iter()
                    .any(|at| state.engine.event_history[*at as usize] == *event)
            {
                return Err("Ashroot private or forged dispatch log".into());
            }
        }
    }
    let waiting: Vec<_> = state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
        .collect();
    for pending in &waiting {
        validate_trigger(
            state,
            &pending.effect,
            pending.source,
            pending.controller,
            pending
                .source_contract
                .filter(|_| pending.granted_by.is_none()),
        )?;
    }
    for item in &state.stack {
        if item.kind == StackItemKind::TriggeredAbility
            && item.v4.granted_by.is_none()
            && item.v4.cauldron_grant.0.is_none()
        {
            if let Some(op) = &item.inline_effect {
                validate_trigger(
                    state,
                    op,
                    item.source,
                    item.controller,
                    item.v4.ability_source_contract,
                )?;
            }
        }
    }
    for capture in captures {
        let source = source_at(state, capture)?;
        let all = rows(state, capture);
        let selected = |predicate: fn(&JournalV2) -> bool| {
            all.iter()
                .copied()
                .filter(|(_, j)| predicate(j))
                .collect::<Vec<_>>()
        };
        let collected = selected(|j| matches!(j, JournalV2::Collected { .. }));
        let published = selected(|j| matches!(j, JournalV2::Published { .. }));
        let removed = selected(|j| {
            matches!(
                j,
                JournalV2::Removed { .. } | JournalV2::PlacementRemoved { .. }
            )
        });
        let operations = selected(|j| matches!(j, JournalV2::Operation { .. }));
        let completed = selected(|j| matches!(j, JournalV2::Completed { .. }));
        let installed = selected(|j| matches!(j, JournalV2::Installed { .. }));
        let logged = state
            .engine
            .event_log
            .iter()
            .filter(|e| *e == &state.engine.event_history[capture as usize])
            .count();
        let held = waiting
            .iter()
            .filter(|p| capture_of(&p.effect) == Some(capture))
            .copied()
            .collect::<Vec<_>>();
        for pending in &held {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            if pending.source_contract != Some(source)
                || pending.target_spec != crate::card_def::TargetSpec::AnotherControlledCreature
                || pending.granted_by.is_some()
            {
                return Err("Ashroot waiting owner changed".into());
            }
        }
        let original = match published.as_slice() {
            [] => None,
            [(at, JournalV2::Published { item, .. })] => {
                validate_item(state, capture, item)?;
                Some((*at, item.as_ref()))
            }
            _ => return Err("Ashroot duplicate publication".into()),
        };
        let live = state
            .stack
            .iter()
            .filter(|i| {
                i.inline_effect.as_ref().and_then(capture_of) == Some(capture)
                    || original.is_some_and(|(_, p)| p.v4.stack_item_id == i.v4.stack_item_id)
            })
            .collect::<Vec<_>>();
        if completed.is_empty() {
            let JournalV2::Capture { attack } = journal(state, capture)? else {
                unreachable!()
            };
            let JournalV2::Attack { turn, active, .. } = journal(state, *attack)? else {
                unreachable!()
            };
            if state.turn != *turn || state.active_player != *active {
                return Err("Ashroot open occurrence crossed turn".into());
            }
            if !removed.is_empty() || !operations.is_empty() || !installed.is_empty() {
                return Err("Ashroot incomplete outcome".into());
            }
            match (collected.as_slice(), original) {
                ([], None) if logged == 1 && held.is_empty() && live.is_empty() => {}
                ([(at, _)], None)
                    if *at > capture && logged == 0 && held.len() == 1 && live.is_empty() => {}
                ([(at, _)], Some((pub_at, item)))
                    if *at > capture
                        && pub_at > *at
                        && logged == 0
                        && held.is_empty()
                        && live.as_slice() == [item] => {}
                _ => return Err("Ashroot mandatory owner erased or duplicated".into()),
            }
            continue;
        }
        if collected.len() != 1
            || completed.len() != 1
            || removed.len() != 1
            || operations.len() != 1
            || !held.is_empty()
            || !live.is_empty()
            || logged != 0
        {
            return Err("Ashroot closed occurrence replayed or erased".into());
        }
        let (close_at, JournalV2::Completed { operation, .. }) = completed[0] else {
            unreachable!()
        };
        let (
            op_at,
            JournalV2::Operation {
                removal,
                installation,
                ..
            },
        ) = operations[0]
        else {
            unreachable!()
        };
        let (remove_at, row) = removed[0];
        if *operation != op_at
            || *removal != remove_at
            || !(capture < collected[0].0
                && collected[0].0 < remove_at
                && remove_at < op_at
                && op_at < close_at)
        {
            return Err("Ashroot closed outcome lost actual ordered operation".into());
        }
        let expected = match (row, original) {
            (JournalV2::PlacementRemoved { pending, .. }, None)
                if pending.source_contract == Some(source)
                    && pending.effect == root(capture)
                    && pending.controller == source.controller
                    && pending.target_spec
                        == crate::card_def::TargetSpec::AnotherControlledCreature =>
            {
                false
            }
            (
                JournalV2::Removed {
                    item,
                    countered,
                    legal,
                    power,
                    ..
                },
                Some((pub_at, original)),
            ) if item.as_ref() == original
                && pub_at < remove_at
                && *power >= 0
                && (!*countered || !*legal)
                && (*legal || *power == 0) =>
            {
                !*countered && *legal
            }
            _ => return Err("Ashroot closure changed original removal".into()),
        };
        if expected != installation.is_some() || installed.len() != usize::from(expected) {
            return Err("Ashroot actual boost skipped or fabricated".into());
        }
        if let Some(installation) = installation {
            if !matches!(installed.as_slice(),[(at,_) ] if at==installation)
                || !(remove_at < *installation && *installation < op_at)
            {
                return Err("Ashroot installation reference changed".into());
            }
            let JournalV2::Installed { effects, .. } = journal(state, *installation)? else {
                unreachable!()
            };
            let JournalV2::Removed { item, power, .. } = row else {
                unreachable!()
            };
            let Target::Object(target) = item.targets[0] else {
                unreachable!()
            };
            let crate::state::StackTargetContractV4::Object {
                zone_change_count: generation,
                ..
            } = item.v4.target_contracts[0]
            else {
                unreachable!()
            };
            let mut timestamp = None;
            let mut pt = 0;
            let mut keyword = 0;
            for effect in effects {
                let stamp = match effect {
                    UntilEndOfTurnEffect::ResolvedObjectEffect {
                        object_id,
                        object_zone_change_count,
                        layer,
                        timestamp,
                        duration,
                        power: p,
                        toughness,
                        grant_haste: false,
                    } if *object_id == target
                        && *object_zone_change_count == generation
                        && *layer == crate::engine::Layers::POWER_TOUGHNESS
                        && *duration == crate::engine::EffectDuration::EndOfTurn
                        && *p == *power
                        && *toughness == *power
                        && *power > 0 =>
                    {
                        pt += 1;
                        *timestamp
                    }
                    UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id,
                        object_zone_change_count,
                        layer,
                        timestamp,
                        duration,
                        keywords,
                    } if *object_id == target
                        && *object_zone_change_count == generation
                        && *layer == crate::engine::Layers::ABILITY_ADDING
                        && *duration == crate::engine::EffectDuration::EndOfTurn
                        && *keywords == crate::card_def::Keywords::TRAMPLE =>
                    {
                        keyword += 1;
                        *timestamp
                    }
                    _ => return Err("Ashroot installed outcome changed".into()),
                };
                if timestamp.is_some_and(|old| old != stamp) {
                    return Err("Ashroot install timestamp changed".into());
                }
                if stamp >= state.engine.next_effect_timestamp {
                    return Err("Ashroot installation timestamp is not committed".into());
                }
                timestamp = Some(stamp);
            }
            if keyword != 1 || pt != usize::from(*power > 0) {
                return Err("Ashroot incomplete boost/trample".into());
            }
            let expiries=state.engine.event_history.iter().enumerate().filter_map(|(at,e)|matches!(e,CommittedEvent::AshrootDispatchV2 {journal:JournalV2::Expired {installation:i,..}} if i==installation).then_some(at as u64)).collect::<Vec<_>>();
            if expiries.len() > 1 || expiries.first().is_some_and(|at| *at <= close_at) {
                return Err("Ashroot expiry order/replay changed".into());
            }
            let JournalV2::Capture { attack } = journal(state, capture)? else {
                unreachable!()
            };
            let JournalV2::Attack { turn, active, .. } = journal(state, *attack)? else {
                unreachable!()
            };
            if expiries.is_empty() {
                if state.turn != *turn || state.active_player != *active {
                    return Err("Ashroot active duration crossed original turn".into());
                }
            } else {
                let JournalV2::Expired { cleanup, .. } = journal(state, expiries[0])? else {
                    unreachable!()
                };
                if !matches!(journal(state,*cleanup)?,JournalV2::Cleanup {turn:t,ending_player,..} if t==turn && ending_player==active)
                {
                    return Err("Ashroot expiry changed original ending turn".into());
                }
            }
            for effect in effects {
                let count = state
                    .engine
                    .until_end_of_turn
                    .iter()
                    .filter(|live| *live == effect)
                    .count();
                if count != usize::from(expiries.is_empty()) {
                    return Err(
                        "Ashroot active outcome erased or expired outcome resurrected".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    use crate::engine::{self, Action, Decision};

    #[test]
    fn registered_unrelated_declaration_hooks_preserve_bytes_and_hash() {
        let mut state = crate::policy_observation_v6::tests::ready_state();
        let elf = crate::policy_observation_v6::tests::put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.diagnostic_state_hash();
        record_attack(&mut state, elf);
        assert!(collect(&mut state, &[]).unwrap().is_empty());
        validate_state(&state).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.diagnostic_state_hash(), hash);
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn genuine_exact_id_counter_and_manufactured_closure_refuse_both_seats() {
        // Physical registered-permanent setup plus actual declaration/targeting;
        // the counter is the accepted exact-ID primitive, not a printed Counterspell.
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            state.step = crate::state::Step::DeclareAttackers;
            let source = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Ashroot Animist",
                Zone::Battlefield,
            );
            let target = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(source).summoning_sick = false;
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::DeclareAttackers {eligible,..} if eligible.contains(&source))
            );
            engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets {legal_targets,..} if legal_targets.contains(&Target::Object(target)))
            );
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let original = state.stack.last().unwrap().clone();
            let capture = original
                .inline_effect
                .as_ref()
                .and_then(capture_of)
                .unwrap();
            for mode in 0..2 {
                let mut bad = state.clone();
                bad.stack.pop();
                let removal = if mode == 0 {
                    0
                } else {
                    // New mutable outcome/close markers cannot substitute for a
                    // genuine private pre-removal operation journal.
                    bad.engine.event_history.len() as u64
                };
                let operation = append(
                    &mut bad,
                    JournalV2::Operation {
                        capture,
                        removal,
                        installation: None,
                    },
                );
                append(&mut bad, JournalV2::Completed { capture, operation });
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            }
            engine::counter_stack_item_by_id(&mut state, original.v4.stack_item_id).unwrap();
            validate_state(&state).unwrap();
            assert_eq!(engine::effective_power(&state, target), 1);
            assert!(!engine::has_effective_keyword(
                &state,
                target,
                crate::card_def::Keywords::TRAMPLE
            ));
            let bytes = serde_json::to_vec(&state).unwrap();
            for mode in 0..3 {
                let mut bad: GameState = serde_json::from_slice(&bytes).unwrap();
                match mode {
                    0 => bad.stack.push(original.clone()),
                    1 => bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            CommittedEvent::AshrootDispatchV2 {
                                journal: JournalV2::Removed { .. }
                            }
                        )
                    }),
                    _ => bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            CommittedEvent::AshrootDispatchV2 {
                                journal: JournalV2::Operation { .. }
                            }
                        )
                    }),
                }
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
                let mut again: GameState = serde_json::from_slice(&before).unwrap();
                assert!(engine::step(&mut again, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&again).unwrap(), before);
            }
        }
    }
}
