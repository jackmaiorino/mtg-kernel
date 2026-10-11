//! Private installation authority. No record is emitted by predecessor roots.
use crate::effect::{EffectOp, ExecCtx};
use crate::engine::{EffectDuration, UntilEndOfTurnEffect};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::rl::{CardStableRefV1, RlContractError};
use crate::state::{
    GameObject, GameState, StackItem, StackItemKind, StackTargetContractV4, Target, Zone,
};
use serde::{Deserialize, Serialize};

// The private snapshot never crosses a public or Human transport boundary.
// Its JSON bytes supply Hash without changing GameObject's predecessor Hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptureV1 {
    pub(crate) item: Box<StackItem>,
    pub(crate) recipient: CardStableRefV1,
    pub(crate) timestamp: u64,
    pub(crate) recipient_snapshot: String,
}

pub(crate) fn masked_ref(
    id: ObjectId,
    object: &GameObject,
) -> Result<CardStableRefV1, RlContractError> {
    let card_db_id = if let Some(face) = object.v4.face_down_v1 {
        crate::card_def::card_id_by_name(if face.disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        })
        .ok_or_else(|| RlContractError("face-down public sentinel missing".into()))?
    } else {
        object.card_def
    };
    Ok(CardStableRefV1 {
        arena_id: id.0,
        card_db_id,
        owner: object.owner.into(),
        controller: object.controller.into(),
        zone: object.zone,
        zone_change_count: object.zone_change_count,
    })
}

fn owns_item(item: &StackItem) -> bool {
    let Some(source) = item.v4.ability_source_contract else {
        return false;
    };
    let Some(index) = item.v4.activated_ability_index else {
        return false;
    };
    if item.kind != StackItemKind::ActivatedAbility
        || crate::card_def::CARD_DEFS
            .get(usize::from(source.card_def))
            .is_none_or(|def| def.name != "Sower of Chaos")
    {
        return false;
    }
    let Ok(selector) = crate::rules_meaning_sower_v1::live_selector_v1() else {
        return false;
    };
    crate::rules_meaning_sower_v1::meaning_v1(&selector, source.card_def, u16::from(index)).is_ok()
}

pub(crate) fn begin_resolution(state: &mut GameState) {
    let Some(item) = state.stack.last().filter(|item| owns_item(item)).cloned() else {
        return;
    };
    let Some(Target::Object(recipient)) = item.targets.first() else {
        return;
    };
    let Some(object) = state.objects.try_get(*recipient) else {
        return;
    };
    let recipient_snapshot = serde_json::to_string(object).expect("target snapshot serializes");
    state
        .engine
        .event_history
        .push(CommittedEvent::SowerRestrictionResolutionV1 {
            item: Box::new(item),
            timestamp: state.engine.next_effect_timestamp,
            recipient_snapshot,
        });
}

pub(crate) fn install(state: &mut GameState, ctx: &ExecCtx, object: ObjectId, timestamp: u64) {
    let Some(item) = ctx.stack_item_id.and_then(|id| {
        state.engine.event_history.iter().rev().find_map(|event| {
            let CommittedEvent::SowerRestrictionResolutionV1 { item, .. } = event else {
                return None;
            };
            (item.v4.stack_item_id == id).then_some(item.as_ref())
        })
    }) else {
        return;
    };
    if masked_ref(object, state.objects.get(object)).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let capture = CaptureV1 {
        item: Box::new(item.clone()),
        recipient: crate::rl::card_ref(state, object).expect("legal public battlefield target"),
        timestamp,
        recipient_snapshot: serde_json::to_string(state.objects.get(object))
            .expect("target snapshot serializes"),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::SowerRestrictionInstalledV1 {
            capture: Box::new(capture),
        });
}

pub(crate) fn expire(state: &mut GameState) {
    let captures: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::SowerRestrictionInstalledV1 { capture } => Some(capture),
            _ => None,
        })
        .filter(|capture| {
            state
                .engine
                .until_end_of_turn
                .iter()
                .any(|effect| marker_matches(effect, capture))
        })
        .cloned()
        .collect();
    for capture in captures {
        state
            .engine
            .event_history
            .push(CommittedEvent::SowerRestrictionExpiredV1 { capture });
    }
}

fn marker_matches(effect: &UntilEndOfTurnEffect, capture: &CaptureV1) -> bool {
    matches!(effect, UntilEndOfTurnEffect::ResolvedObjectCantBlock {
        object_id, object_zone_change_count, timestamp, duration: EffectDuration::EndOfTurn
    } if object_id.0 == capture.recipient.arena_id && *object_zone_change_count == capture.recipient.zone_change_count && *timestamp == capture.timestamp)
}

fn authenticate(capture: &CaptureV1) -> Result<(), RlContractError> {
    let invalid = || RlContractError("malformed Sower restriction capture".into());
    let item = &capture.item;
    let source = item.v4.ability_source_contract.ok_or_else(invalid)?;
    let index = item.v4.activated_ability_index.ok_or_else(invalid)?;
    let selector = crate::rules_meaning_sower_v1::live_selector_v1().map_err(|_| invalid())?;
    crate::rules_meaning_sower_v1::meaning_v1(&selector, source.card_def, u16::from(index))
        .map_err(|_| invalid())?;
    if item.kind != StackItemKind::ActivatedAbility
        || item.source != source.source
        || item.controller != source.controller
        || source.zone != Zone::Battlefield
        || item.is_copy
        || item.v4.stack_item_id.0 == 0
        || item.inline_effect
            != Some(EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 })
        || item.targets != [Target::Object(ObjectId(capture.recipient.arena_id))]
        || item.v4.target_contracts.len() != 1
    {
        return Err(invalid());
    }
    let object: GameObject =
        serde_json::from_str(&capture.recipient_snapshot).map_err(|_| invalid())?;
    let StackTargetContractV4::Object {
        object: id,
        card_def,
        owner,
        zone,
        zone_change_count,
        ..
    } = item.v4.target_contracts[0]
    else {
        return Err(invalid());
    };
    if id.0 != capture.recipient.arena_id
        || object.card_def != card_def
        || object.owner != owner
        || object.zone != Zone::Battlefield
        || zone != Zone::Battlefield
        || object.zone_change_count != zone_change_count
        || masked_ref(id, &object)? != capture.recipient
    {
        return Err(invalid());
    }
    Ok(())
}

/// Active records are complete, including stale recipients. Expiry is tied to
/// the exact committed installation, so deletion or resurrection refuses.
pub(crate) fn active(state: &GameState) -> Result<Vec<&CaptureV1>, RlContractError> {
    let invalid = || RlContractError("incomplete Sower restriction lifecycle".into());
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::SowerRestrictionInstalledV1 { .. }
                | CommittedEvent::SowerRestrictionExpiredV1 { .. }
                | CommittedEvent::SowerRestrictionResolutionV1 { .. }
        )
    }) {
        return Err(invalid());
    }
    let mut installs = Vec::new();
    let mut expiries = Vec::new();
    let mut resolutions = Vec::new();
    for (ordinal, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::SowerRestrictionResolutionV1 {
                item,
                timestamp,
                recipient_snapshot,
            } => {
                if !owns_item(item)
                    || resolutions
                        .iter()
                        .any(|(_, old, _, _): &(usize, &StackItem, u64, &str)| {
                            old.v4.stack_item_id == item.v4.stack_item_id
                        })
                {
                    return Err(invalid());
                }
                resolutions.push((
                    ordinal,
                    item.as_ref(),
                    *timestamp,
                    recipient_snapshot.as_str(),
                ));
            }
            CommittedEvent::SowerRestrictionInstalledV1 { capture } => {
                authenticate(capture)?;
                if !resolutions
                    .iter()
                    .any(|(before, item, timestamp, snapshot)| {
                        *before < ordinal
                            && *item == capture.item.as_ref()
                            && *timestamp == capture.timestamp
                            && *snapshot == capture.recipient_snapshot
                    })
                {
                    return Err(invalid());
                }
                if capture.timestamp >= state.engine.next_effect_timestamp {
                    return Err(invalid());
                }
                if installs.iter().any(|(_, old): &(usize, &CaptureV1)| {
                    old.timestamp == capture.timestamp
                        || old.item.v4.stack_item_id == capture.item.v4.stack_item_id
                }) {
                    return Err(invalid());
                }
                installs.push((ordinal, capture.as_ref()));
            }
            CommittedEvent::SowerRestrictionExpiredV1 { capture } => {
                if !installs
                    .iter()
                    .any(|(before, installed)| *before < ordinal && *installed == capture.as_ref())
                    || expiries.contains(&capture.as_ref())
                {
                    return Err(invalid());
                }
                expiries.push(capture.as_ref());
            }
            _ => {}
        }
    }
    for (_, item, _, _) in resolutions {
        if installs
            .iter()
            .filter(|(_, capture)| capture.item.as_ref() == item)
            .count()
            != 1
        {
            return Err(invalid());
        }
    }
    let mut active = Vec::new();
    for (_, capture) in installs {
        let count = state
            .engine
            .until_end_of_turn
            .iter()
            .filter(|effect| marker_matches(effect, capture))
            .count();
        if expiries.contains(&capture) {
            if count != 0 {
                return Err(invalid());
            }
        } else {
            if count != 1 {
                return Err(invalid());
            }
            crate::engine::validate_historical_ability_source_contract(
                state,
                capture
                    .item
                    .v4
                    .ability_source_contract
                    .ok_or_else(invalid)?,
            )
            .map_err(RlContractError)?;
            active.push(capture);
        }
    }
    for effect in &state.engine.until_end_of_turn {
        if matches!(effect, UntilEndOfTurnEffect::ResolvedObjectCantBlock { .. })
            && active
                .iter()
                .filter(|capture| marker_matches(effect, capture))
                .count()
                != 1
        {
            return Err(invalid());
        }
    }
    Ok(active)
}

#[cfg(all(test, feature = "standard-magezero-fixtures"))]
mod mask_tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};
    #[test]
    fn snapshot_mask_matches_existing_viewer_safe_authority_without_underlying_definition() {
        for owner in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let mut state = ready_state();
            let id = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(id).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            let masked = masked_ref(id, state.objects.get(id)).unwrap();
            assert_eq!(masked, crate::rl::card_ref(&state, id).unwrap());
            assert_eq!(
                masked.card_db_id,
                crate::card_def::card_id_by_name("Face-down card").unwrap()
            );
            let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
            state.objects.get_mut(id).card_def = bolt;
            assert_eq!(masked_ref(id, state.objects.get(id)).unwrap(), masked);
            assert_eq!(crate::rl::card_ref(&state, id).unwrap(), masked);
        }
    }
}
