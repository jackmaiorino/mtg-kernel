//! Private committed installation and cleanup authority for the new opcode.
use super::*;
use crate::event::CommittedEvent;
use crate::state::{AbilitySourceContractV4, GameObject, Step};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstallationOriginV1 {
    PrintedSpell {
        stack_item_id: StackItemId,
        resolution_history_index: usize,
    },
    RegisteredPrimitive,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationV1 {
    pub source: AbilitySourceContractV4,
    pub target: StackTargetContractV4,
    pub previous_controller: PlayerId,
    pub controller: PlayerId,
    pub timestamp: u64,
    pub turn: u32,
    pub active_player: PlayerId,
    pub origin: InstallationOriginV1,
}

/// Existing direct opcode fixtures use genuine registered non-control sources
/// from hand. This cannot stand in for a resolving printed control spell, even
/// after its physical source departs or its independent resolution row is lost.
fn registered_primitive_source(state: &GameState, source: AbilitySourceContractV4) -> bool {
    source.zone == Zone::Hand
        && source.attached_to.is_none()
        && crate::card_def::CARD_DEFS
            .get(usize::from(source.card_def))
            .is_some_and(|def| {
                def.is_executable()
                    && def.name != "Involuntary Employment"
                    && !(def.spell_effect)().as_ref().is_some_and(contains_control)
            })
        && state
            .objects
            .try_get(source.source)
            .is_some_and(|physical| {
                physical.owner == source.owner
                    && physical.zone_change_count >= source.zone_change_count
                    && (physical.card_def == source.card_def
                        || crate::standard_cards_v1::had_card_def(
                            state,
                            source.source,
                            source.zone_change_count,
                            source.card_def,
                        ))
            })
}

impl InstallationV1 {
    fn record(&self) -> Result<UntilEndOfTurnEffect, String> {
        let StackTargetContractV4::Object {
            object,
            controller,
            zone: Zone::Battlefield,
            zone_change_count,
            ..
        } = self.target
        else {
            return Err("temporary control installation lost its battlefield recipient".into());
        };
        if controller != self.previous_controller {
            return Err(
                "temporary control installation changed its original recipient controller".into(),
            );
        }
        Ok(UntilEndOfTurnEffect::ResolvedObjectControlEffect {
            object_id: object,
            object_zone_change_count: zone_change_count,
            previous_controller: self.previous_controller,
            controller: self.controller,
            timestamp: self.timestamp,
        })
    }
}

fn targeted_before(state: &GameState, item: &StackItem, before: usize) -> Result<(), String> {
    let [Target::Object(object)] = item.targets.as_slice() else {
        return Err("temporary control resolution requires its one captured target".into());
    };
    let [StackTargetContractV4::Object {
        object: bound,
        zone_change_count,
        ..
    }] = item.v4.target_contracts.as_slice()
    else {
        return Err("temporary control resolution lost its target contract".into());
    };
    let events: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            matches!(event, CommittedEvent::Targeted { targeting_stack_item, .. }
            if *targeting_stack_item == item.v4.stack_item_id)
        })
        .collect();
    if object != bound
        || item.v4.stack_item_id.0 == 0
        || !matches!(events.as_slice(),
        [(index, CommittedEvent::Targeted { target, target_zone_change_count, targeting_controller, .. })]
        if *index < before && target == object && target_zone_change_count == zone_change_count
            && *targeting_controller == item.controller)
    {
        return Err("temporary control resolution lost its committed targeting occurrence".into());
    }
    Ok(())
}

/// Called before the ordinary resolver removes the genuine validated item.
/// No resolution receipt is published for a fully illegal target or old root.
pub(crate) fn begin_resolution(state: &mut GameState) -> Result<(), String> {
    let Some(item) = state
        .stack
        .last()
        .filter(|item| owns_spell(state, item))
        .cloned()
    else {
        return Ok(());
    };
    crate::engine::validated_stack_item_target_spec(&item, state)?;
    targeted_before(state, &item, state.engine.event_history.len())?;
    let Target::Object(target) = item.targets[0] else {
        return Err("temporary control resolution lost its target".into());
    };
    let snapshot = serde_json::to_string(state.objects.get(target)).expect("recipient serializes");
    state
        .engine
        .event_history
        .push(CommittedEvent::TemporaryControlResolutionV1 {
            item: Box::new(item),
            recipient_snapshot: snapshot,
            timestamp: state.engine.next_effect_timestamp,
            turn: state.turn,
            active_player: state.active_player,
        });
    Ok(())
}

pub(super) fn install(state: &mut GameState, ctx: &ExecCtx, object: ObjectId, timestamp: u64) {
    let resolution_history_index = ctx.stack_item_id.and_then(|id| {
        state
            .engine
            .event_history
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, event)| {
                matches!(event, CommittedEvent::TemporaryControlResolutionV1 { item, .. }
                if item.v4.stack_item_id == id)
                .then_some(index)
            })
    });
    // Direct registered primitive execution has no stack incarnation. A real
    // stack execution must always have the independent pre-removal owner.
    if ctx.stack_item_id.is_some() && resolution_history_index.is_none() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let mut source = AbilitySourceContractV4::capture(state, ctx.source);
    source.controller = ctx.controller;
    let origin = match (ctx.stack_item_id, resolution_history_index) {
        (Some(stack_item_id), Some(resolution_history_index)) => {
            InstallationOriginV1::PrintedSpell {
                stack_item_id,
                resolution_history_index,
            }
        }
        (None, None) if registered_primitive_source(state, source) => {
            InstallationOriginV1::RegisteredPrimitive
        }
        _ => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            return;
        }
    };
    let capture = InstallationV1 {
        source,
        target: StackTargetContractV4::capture(state, Target::Object(object)),
        previous_controller: state.objects.get(object).controller,
        controller: ctx.controller,
        timestamp,
        turn: state.turn,
        active_player: state.active_player,
        origin,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::TemporaryControlInstalledV1 {
            installation: Box::new(capture),
        });
}

/// Only actual Cleanup calls this function, before removing any active row.
/// The batch independently binds every exact installation consumed by this
/// cleanup, including departed recipients and later same-Cleanup installs.
pub(super) fn expire(state: &mut GameState) {
    let installations: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let CommittedEvent::TemporaryControlInstalledV1 { installation } = event else {
                return None;
            };
            let record = installation.record().ok()?;
            state
                .engine
                .until_end_of_turn
                .contains(&record)
                .then_some(index)
        })
        .collect();
    if installations.is_empty() {
        return;
    }
    assert_eq!(state.step, Step::Cleanup);
    let cleanup_history_index = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::TemporaryControlCleanupV1 {
            turn: state.turn,
            active_player: state.active_player,
            installations: installations.clone(),
        });
    for installation_history_index in installations {
        state
            .engine
            .event_history
            .push(CommittedEvent::TemporaryControlExpiredV1 {
                installation_history_index,
                cleanup_history_index,
            });
    }
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let invalid = || "incomplete committed temporary control lifecycle".to_string();
    let history = &state.engine.event_history;
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::TemporaryControlResolutionV1 { .. }
                | CommittedEvent::TemporaryControlInstalledV1 { .. }
                | CommittedEvent::TemporaryControlCleanupV1 { .. }
                | CommittedEvent::TemporaryControlExpiredV1 { .. }
        )
    }) {
        return Err(invalid());
    }
    let mut installs: Vec<(usize, &InstallationV1)> = Vec::new();
    let mut expired = Vec::new();
    let mut resolutions = Vec::new();
    for (index, event) in history.iter().enumerate() {
        match event {
            CommittedEvent::TemporaryControlResolutionV1 {
                item,
                recipient_snapshot,
                timestamp,
                ..
            } => {
                targeted_before(state, item, index)?;
                let snapshot: GameObject =
                    serde_json::from_str(recipient_snapshot).map_err(|_| invalid())?;
                let Some(source) = item.v4.source_contract else {
                    return Err(invalid());
                };
                let [target] = item.v4.target_contracts.as_slice() else {
                    return Err(invalid());
                };
                if item.kind != crate::state::StackItemKind::Spell
                    || item.inline_effect.is_some()
                    || item.mode_chosen != 0
                    || item.v4.face_index != 0
                    || item.v4.target_spec != Some(crate::card_def::TargetSpec::Creature)
                    || item.v4.cast_method != Some(source.cast_method)
                    || source.source != item.source
                    || source.controller != item.controller
                    || source.zone != Zone::Stack
                    || crate::card_def::CARD_DEFS
                        .get(usize::from(source.card_def))
                        .is_none_or(|def| {
                            def.name != "Involuntary Employment"
                                || def.target_spec != crate::card_def::TargetSpec::Creature
                                || printed_program()
                                    .is_none_or(|root| (def.spell_effect)() != Some(root))
                        })
                    || snapshot.zone != Zone::Battlefield
                    || resolutions
                        .iter()
                        .any(|(_, id)| *id == item.v4.stack_item_id)
                    || *timestamp >= state.engine.next_effect_timestamp
                {
                    return Err(invalid());
                }
                let StackTargetContractV4::Object {
                    object,
                    card_def,
                    owner,
                    zone,
                    zone_change_count,
                    spell_copy_origin,
                    ..
                } = target
                else {
                    return Err(invalid());
                };
                if *zone != Zone::Battlefield
                    || (snapshot.card_def != *card_def
                        && !crate::standard_cards_v1::had_card_def(
                            state,
                            *object,
                            *zone_change_count,
                            *card_def,
                        ))
                    || snapshot.owner != *owner
                    || snapshot.zone_change_count != *zone_change_count
                    || snapshot.spell_copy_origin != *spell_copy_origin
                    || item.targets != [Target::Object(*object)]
                {
                    return Err(invalid());
                }
                resolutions.push((index, item.v4.stack_item_id));
            }
            CommittedEvent::TemporaryControlInstalledV1 { installation } => {
                let capture = installation.as_ref();
                capture.record()?;
                if capture.source.controller != capture.controller
                    || capture.timestamp >= state.engine.next_effect_timestamp
                    || installs
                        .iter()
                        .any(|(_, old)| old.timestamp == capture.timestamp)
                {
                    return Err(invalid());
                }
                match capture.origin {
                    InstallationOriginV1::RegisteredPrimitive
                        if registered_primitive_source(state, capture.source) => {}
                    InstallationOriginV1::PrintedSpell {
                        stack_item_id: id,
                        resolution_history_index: before,
                    } if before < index => {
                        let Some(CommittedEvent::TemporaryControlResolutionV1 {
                            item,
                            recipient_snapshot,
                            timestamp,
                            turn,
                            active_player,
                        }) = history.get(before)
                        else {
                            return Err(invalid());
                        };
                        let snapshot: GameObject =
                            serde_json::from_str(recipient_snapshot).map_err(|_| invalid())?;
                        let Some(source) = item.v4.source_contract else {
                            return Err(invalid());
                        };
                        let StackTargetContractV4::Object {
                            object,
                            card_def,
                            owner,
                            zone,
                            zone_change_count,
                            spell_copy_origin,
                            ..
                        } = capture.target
                        else {
                            return Err(invalid());
                        };
                        if id != item.v4.stack_item_id
                            || capture.source.source != item.source
                            || capture.source.card_def != source.card_def
                            || capture.source.owner != source.owner
                            || capture.source.zone != source.zone
                            || capture.source.zone_change_count != source.zone_change_count
                            || capture.source.attached_to.is_some()
                            || capture.controller != item.controller
                            || capture.timestamp != *timestamp
                            || capture.turn != *turn
                            || capture.active_player != *active_player
                            || item.targets != [Target::Object(object)]
                            || snapshot.card_def != card_def
                            || snapshot.owner != owner
                            || snapshot.zone != zone
                            || snapshot.zone_change_count != zone_change_count
                            || snapshot.spell_copy_origin != spell_copy_origin
                            || snapshot.controller != capture.previous_controller
                        {
                            return Err(invalid());
                        }
                    }
                    _ => return Err(invalid()),
                }
                installs.push((index, capture));
            }
            CommittedEvent::TemporaryControlCleanupV1 {
                turn,
                active_player,
                installations,
            } => {
                let expected: Vec<_> = installs
                    .iter()
                    .filter(|(ordinal, capture)| {
                        !expired.contains(ordinal)
                            && capture.turn == *turn
                            && capture.active_player == *active_player
                    })
                    .map(|(ordinal, _)| *ordinal)
                    .collect();
                if installations.is_empty() || installations != &expected {
                    return Err(invalid());
                }
                for ordinal in installations {
                    if history.iter().enumerate().filter(|(after, event)| *after > index && matches!(event,
                        CommittedEvent::TemporaryControlExpiredV1 { installation_history_index, cleanup_history_index }
                        if installation_history_index == ordinal && *cleanup_history_index == index)).count() != 1
                    { return Err(invalid()); }
                }
            }
            CommittedEvent::TemporaryControlExpiredV1 {
                installation_history_index,
                cleanup_history_index,
            } => {
                let Some(CommittedEvent::TemporaryControlCleanupV1 { installations, .. }) =
                    history.get(*cleanup_history_index)
                else {
                    return Err(invalid());
                };
                if *cleanup_history_index >= index
                    || !installations.contains(installation_history_index)
                    || expired.contains(installation_history_index)
                {
                    return Err(invalid());
                }
                expired.push(*installation_history_index);
            }
            _ => {}
        }
    }
    for (index, id) in resolutions {
        if installs
            .iter()
            .filter(|(_, capture)| {
                capture.origin
                    == InstallationOriginV1::PrintedSpell {
                        resolution_history_index: index,
                        stack_item_id: id,
                    }
            })
            .count()
            != 1
        {
            return Err(invalid());
        }
    }
    for (index, capture) in &installs {
        let record = capture.record()?;
        let count = state
            .engine
            .until_end_of_turn
            .iter()
            .filter(|row| **row == record)
            .count();
        if expired.contains(index) {
            if count != 0 {
                return Err(invalid());
            }
        } else if count != 1
            || capture.turn != state.turn
            || capture.active_player != state.active_player
        {
            return Err(invalid());
        }
    }
    for record in &state.engine.until_end_of_turn {
        if matches!(
            record,
            UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
        ) && installs
            .iter()
            .filter(|(index, capture)| {
                !expired.contains(index) && capture.record().as_ref() == Ok(record)
            })
            .count()
            != 1
        {
            return Err(invalid());
        }
    }
    Ok(())
}
