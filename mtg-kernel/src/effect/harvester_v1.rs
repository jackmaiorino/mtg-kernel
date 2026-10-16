//! Native Harvester prerequisites. Registration is deliberately separate.
//! A copy exception belongs to layer 1, not a timestamped subtype grant.
use super::*;
use crate::state::AbilitySourceContractV4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightmareV1 {
    pub source: AbilitySourceContractV4,
    pub creation_history_index: u64,
}

/// The source is the actual copy-layer form after the native layer change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightmareLayerV1 {
    pub source: AbilitySourceContractV4,
    pub nightmare: bool,
}

pub(crate) fn record_entry(state: &mut GameState, creation: usize) {
    let Some(event::CommittedEvent::CreateToken { object, .. }) =
        state.engine.event_history.get(creation)
    else {
        return;
    };
    let object = *object;
    if !state.objects.get(object).v4.copied_nightmare_v1 {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    state
        .engine
        .event_history
        .push(event::CommittedEvent::NightmareCopyEnteredV1 {
            entry: NightmareV1 {
                source,
                creation_history_index: creation as u64,
            },
        });
}

pub(crate) fn record_layer(state: &mut GameState, object: ObjectId, prior: bool) {
    let live = state.objects.get(object);
    if live.v4.copied_nightmare_v1 == prior {
        return;
    }
    let layer = NightmareLayerV1 {
        source: AbilitySourceContractV4::capture(state, object),
        nightmare: live.v4.copied_nightmare_v1,
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::NightmareCopyLayerV1 { layer });
}

pub(crate) fn historical_nightmare(
    state: &GameState,
    source: AbilitySourceContractV4,
    before: usize,
) -> Result<bool, String> {
    let mut current = false;
    let mut entry_seen = false;
    for (at, event) in state.engine.event_history.iter().enumerate().take(before) {
        match event {
            event::CommittedEvent::NightmareCopyEnteredV1 { entry }
                if entry.source.source == source.source
                    && entry.source.zone_change_count == source.zone_change_count =>
            {
                if entry_seen
                    || entry.source.owner != source.owner
                    || entry.source.zone != Zone::Battlefield
                    || entry.source.zone_change_count != 0
                    || entry.creation_history_index >= at as u64
                    || !matches!(state.engine.event_history.get(entry.creation_history_index as usize),
                        Some(event::CommittedEvent::CreateToken { object, token_def, controller, .. })
                        if *object == source.source && *token_def == entry.source.card_def && *controller == entry.source.controller)
                {
                    return Err("Nightmare copy lost its original native creation".into());
                }
                crate::engine::validate_historical_ability_source_contract(state, entry.source)?;
                entry_seen = true;
                current = true;
            }
            event::CommittedEvent::NightmareCopyLayerV1 { layer }
                if layer.source.source == source.source
                    && layer.source.zone_change_count == source.zone_change_count =>
            {
                if !entry_seen
                    || layer.source.owner != source.owner
                    || layer.source.zone != Zone::Battlefield
                    || layer.nightmare == current
                {
                    return Err("Nightmare copy-layer sequence changed".into());
                }
                crate::engine::validate_historical_ability_source_contract(state, layer.source)?;
                current = layer.nightmare;
            }
            _ => {}
        }
    }
    Ok(current)
}

pub(crate) fn copiable_nightmare(state: &GameState, object: ObjectId) -> Result<bool, String> {
    let live = state
        .objects
        .try_get(object)
        .ok_or("Nightmare copy source absent")?;
    let expected = historical_nightmare(
        state,
        AbilitySourceContractV4::capture(state, object),
        state.engine.event_history.len(),
    )?;
    if expected != live.v4.copied_nightmare_v1
        || expected && (live.zone != Zone::Battlefield || !live.v4.is_token)
    {
        return Err("Nightmare copy component changed from its native entry/layers".into());
    }
    Ok(expected && live.v4.face_down_v1.is_none())
}

pub(crate) fn require_legacy_absence(state: &GameState) -> Result<(), String> {
    if state
        .objects
        .iter()
        .any(|(_, o)| o.zone == Zone::Battlefield && o.v4.copied_nightmare_v1)
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                event::CommittedEvent::NightmareCopyEnteredV1 { .. }
                    | event::CommittedEvent::NightmareCopyLayerV1 { .. }
            )
        })
    {
        return Err("copiable Nightmare exception requires a distinct complete consumer".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GraveyardOriginV1 {
    ZoneChange {
        history_index: u64,
    },
    /// A reversed cast retains its original graveyard eligibility. It is not
    /// a new card put into the graveyard during this turn.
    AbortedCast {
        original_receipt: u64,
        source: crate::state::StackSourceContractV4,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraveyardEntryV1 {
    pub card: EffectObjectBinding,
    pub card_def: u16,
    pub owner: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub origin: GraveyardOriginV1,
}

pub(crate) fn catalog_enabled() -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && card_def::card_id_by_name("Abyssal Harvester").is_some()
}

pub(crate) fn record_graveyard_entry(state: &mut GameState, history_index: usize) {
    if !catalog_enabled() {
        return;
    }
    let Some(event::CommittedEvent::ZoneChange {
        object,
        to: Zone::Graveyard,
        ..
    }) = state.engine.event_history.get(history_index)
    else {
        return;
    };
    let object = *object;
    let live = state.objects.get(object);
    if live.v4.is_token || live.spell_copy_origin.is_some() {
        return;
    }
    let entry = GraveyardEntryV1 {
        card: EffectObjectBinding {
            object,
            expected_zone: Zone::Graveyard,
            expected_zone_change_count: live.zone_change_count,
        },
        card_def: live.card_def,
        owner: live.owner,
        turn: state.turn_key_v1(),
        active_player: state.active_player,
        origin: GraveyardOriginV1::ZoneChange {
            history_index: history_index as u64,
        },
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry });
}

fn graveyard_entry(state: &GameState, at: usize) -> Result<GraveyardEntryV1, String> {
    let Some(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }) =
        state.engine.event_history.get(at)
    else {
        return Err("Harvester graveyard occurrence absent".into());
    };
    let original = *entry;
    let mut cursor = at;
    let mut current = original;
    loop {
        let live = state
            .objects
            .try_get(current.card.object)
            .ok_or("Harvester graveyard card absent")?;
        if current.card.expected_zone != Zone::Graveyard
            || current.owner != live.owner
            || (current.card.expected_zone_change_count == live.zone_change_count
                && current.card_def != live.card_def)
            || current.card.expected_zone_change_count > live.zone_change_count
        {
            return Err("Harvester graveyard entry changed its physical card/incarnation".into());
        }
        match current.origin {
            GraveyardOriginV1::ZoneChange { history_index } => {
                if history_index >= cursor as u64
                    || !matches!(state.engine.event_history.get(history_index as usize),
                    Some(event::CommittedEvent::ZoneChange { object, to: Zone::Graveyard, .. }) if *object == current.card.object)
                {
                    return Err("Harvester entry lost its actual graveyard move".into());
                }
                break;
            }
            GraveyardOriginV1::AbortedCast {
                original_receipt,
                source,
            } => {
                if original_receipt >= cursor as u64
                    || source.source != current.card.object
                    || source.owner != current.owner
                    || source.card_def != current.card_def
                    || source.zone != Zone::Stack
                    || source.zone_change_count.checked_add(1)
                        != Some(current.card.expected_zone_change_count)
                {
                    return Err("Harvester rollback lost original cast source".into());
                }
                let next = usize::try_from(original_receipt)
                    .map_err(|_| "Harvester rollback index width")?;
                let Some(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }) =
                    state.engine.event_history.get(next)
                else {
                    return Err("Harvester rollback input absent".into());
                };
                if entry.card.object != current.card.object
                    || entry.owner != current.owner
                    || entry.card_def != current.card_def
                    || entry.turn != current.turn
                    || entry.active_player != current.active_player
                    || !source.spell_cast_origin.is_some_and(|origin| {
                        origin.origin_zone == Zone::Graveyard
                            && origin.origin_zone_change_count
                                == entry.card.expected_zone_change_count
                            && origin.origin_zone_change_count.checked_add(1)
                                == Some(source.zone_change_count)
                    })
                {
                    return Err("Harvester rollback invented new graveyard eligibility".into());
                }
                cursor = next;
                current = *entry;
            }
        }
    }
    Ok(original)
}

pub(crate) fn rollback_input(
    state: &GameState,
    pending: &crate::engine::PendingCast,
) -> Option<(u64, GraveyardEntryV1)> {
    if !catalog_enabled() || pending.origin_zone != Zone::Graveyard {
        return None;
    }
    let generation = pending
        .source_contract
        .spell_cast_origin?
        .origin_zone_change_count;
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, event)| match event {
            event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }
                if entry.card.object == pending.spell
                    && entry.card.expected_zone_change_count == generation =>
            {
                Some((at as u64, *entry))
            }
            _ => None,
        })
}

pub(crate) fn record_rollback(
    state: &mut GameState,
    pending: &crate::engine::PendingCast,
    input: Option<(u64, GraveyardEntryV1)>,
) {
    let Some((original_receipt, mut entry)) = input else {
        return;
    };
    let live = state.objects.get(pending.spell);
    if live.zone != Zone::Graveyard {
        return;
    }
    entry.card.expected_zone_change_count = live.zone_change_count;
    entry.origin = GraveyardOriginV1::AbortedCast {
        original_receipt,
        source: pending.source_contract,
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry });
}

pub(crate) fn eligible(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Graveyard
        || live.v4.is_token
        || live.spell_copy_origin.is_some()
        || !card_def::CARD_DEFS[live.card_def as usize].has_type(CardType::Creature)
    {
        return false;
    }
    let mut matches = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }
                if entry.card.object == object
                    && entry.card.expected_zone_change_count == live.zone_change_count =>
            {
                Some(at)
            }
            _ => None,
        });
    let Some(at) = matches.next() else {
        return false;
    };
    matches.next().is_none()
        && graveyard_entry(state, at).is_ok_and(|entry| {
            entry.turn == state.turn_key_v1() && entry.active_player == state.active_player
        })
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let mut copied = std::collections::BTreeSet::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            event::CommittedEvent::NightmareCopyEnteredV1 { entry } => {
                copied.insert(entry.source.source);
            }
            event::CommittedEvent::NightmareCopyLayerV1 { layer } => {
                copied.insert(layer.source.source);
            }
            _ => {}
        }
        if matches!(
            event,
            event::CommittedEvent::HarvesterGraveyardEntryV1 { .. }
        ) {
            graveyard_entry(state, at)?;
        }
        if matches!(
            event,
            event::CommittedEvent::NightmareCopyEnteredV1 { .. }
                | event::CommittedEvent::NightmareCopyLayerV1 { .. }
        ) && state.engine.event_log.contains(event)
        {
            return Err("private Nightmare copy journal exposed as event".into());
        }
    }
    for (id, live) in state.objects.iter() {
        if live.zone == Zone::Battlefield && (live.v4.copied_nightmare_v1 || copied.contains(&id)) {
            copiable_nightmare(state, id)?;
            if live.v4.copied_nightmare_v1
                && !live
                    .v4
                    .effective_subtype_ids
                    .contains(&card_def::Subtype::Nightmare.stable_id())
            {
                return Err("copiable Nightmare lost its base subtype".into());
            }
        } else if live.v4.copied_nightmare_v1 {
            return Err("copiable Nightmare survived a zone change".into());
        }
    }
    Ok(())
}
