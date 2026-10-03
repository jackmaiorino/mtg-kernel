//! Exact attachments and timestamp ordering for creature-Aura overrides.

use crate::card_def::{AttachmentDef, CreatureCharacteristicsOverrideDef, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::{GameState, ObjectLinkV4, Zone};

pub(crate) fn creature_override(
    state: &GameState,
    host: ObjectId,
) -> Option<(CreatureCharacteristicsOverrideDef, u64)> {
    if !cfg!(feature = "limited-fdn-fixtures") {
        return None;
    }
    let object = state.objects.try_get(host)?;
    if object.zone != Zone::Battlefield {
        return None;
    }
    let host_link = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: object.zone_change_count,
    });
    object
        .attachments
        .iter()
        .filter_map(|&aura| {
            let source = state.objects.try_get(aura)?;
            if source.zone != Zone::Battlefield || source.v4.attached_to != host_link {
                return None;
            }
            let definition = CARD_DEFS.get(source.card_def as usize)?;
            if !definition.is_executable() {
                return None;
            }
            let AttachmentDef::AuraCreatureOverride(characteristics) = definition.attachment?
            else {
                return None;
            };
            Some((characteristics, source.v4.layer_timestamp.unwrap_or(0)))
        })
        .max_by_key(|(_, timestamp)| *timestamp)
}

pub(crate) fn removal_timestamp(state: &GameState, host: ObjectId) -> Option<u64> {
    creature_override(state, host)
        .filter(|(characteristics, _)| characteristics.loses_abilities)
        .map(|(_, timestamp)| timestamp)
}

pub(crate) fn printed_abilities_active(state: &GameState, source: ObjectId) -> bool {
    removal_timestamp(state, source).is_none()
}

pub(crate) fn grant_survives(state: &GameState, host: ObjectId, timestamp: u64) -> bool {
    removal_timestamp(state, host).is_none_or(|removed_at| timestamp > removed_at)
}
