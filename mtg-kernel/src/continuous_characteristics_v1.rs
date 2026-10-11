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
        .into_iter()
        .chain(crate::standard_creatures_v1::removal_timestamp(state, host))
        .max()
}

pub(crate) fn printed_abilities_active(state: &GameState, source: ObjectId) -> bool {
    state
        .objects
        .try_get(source)
        .is_none_or(|object| object.v4.face_down_v1.is_none())
        && removal_timestamp(state, source).is_none()
}

pub(crate) fn has_printed_cant_block(name: &str) -> bool {
    cfg!(feature = "limited-fdn-fixtures") && name == "Vampire Soulcaller"
}

pub(crate) fn printed_cant_block(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|object| {
        object.zone == Zone::Battlefield
            && CARD_DEFS
                .get(usize::from(object.card_def))
                .is_some_and(|def| has_printed_cant_block(def.name))
            && printed_abilities_active(state, source)
    })
}

pub(crate) fn grant_survives(state: &GameState, host: ObjectId, timestamp: u64) -> bool {
    removal_timestamp(state, host).is_none_or(|removed_at| timestamp > removed_at)
}

/// The active `CardDef::animation` of a battlefield incarnation and the
/// timestamp of the ability that applied it (Mishra's Foundry). An Aura
/// creature override replaces it entirely while one applies.
pub(crate) fn animation(
    state: &GameState,
    id: ObjectId,
) -> Option<(crate::card_def::AnimationDef, u64)> {
    if !cfg!(feature = "standard-magezero-fixtures") {
        return None;
    }
    let object = state.objects.try_get(id)?;
    if object.zone != Zone::Battlefield {
        return None;
    }
    let definition = CARD_DEFS.get(object.card_def as usize)?;
    if !definition.is_executable() || creature_override(state, id).is_some() {
        return None;
    }
    let own = object
        .v4
        .animation_timestamp
        .and_then(|timestamp| definition.animation.map(|animation| (animation, timestamp)));
    let host = Some(ObjectLinkV4 {
        object: id,
        zone_change_count: object.zone_change_count,
    });
    let attached = object.attachments.iter().filter_map(|aura| {
        let source = state.objects.try_get(*aura)?;
        if source.zone != Zone::Battlefield || source.v4.attached_to != host {
            return None;
        }
        let def = CARD_DEFS.get(source.card_def as usize)?;
        if !def.is_executable() || !printed_abilities_active(state, *aura) {
            return None;
        }
        let AttachmentDef::AuraArtifactAnimation(animation) = def.attachment? else {
            return None;
        };
        Some((animation, source.v4.layer_timestamp.unwrap_or(0)))
    });
    own.into_iter()
        .chain(attached)
        .max_by_key(|(_, timestamp)| *timestamp)
}

/// Layer 7b settings use timestamps. Characteristic-defining abilities (7a)
/// are consulted only when no setting applies.
pub(crate) fn base_power_toughness(state: &GameState, id: ObjectId) -> Option<(i16, i16)> {
    let object = state.objects.try_get(id)?;
    let mut settings = Vec::new();
    if crate::standard_cards_v1::kaito_is_creature(state, id) {
        settings.push((3, 4, object.v4.layer_timestamp.unwrap_or(0)));
    }
    if let Some((override_, timestamp)) = creature_override(state, id) {
        settings.push((override_.power, override_.toughness, timestamp));
    }
    if object.zone == Zone::Battlefield {
        if let Some(timestamp) = object.v4.animation_timestamp {
            if let Some(animation) = CARD_DEFS[object.card_def as usize].animation {
                settings.push((animation.power, animation.toughness, timestamp));
            }
        }
    }
    if object.zone == Zone::Battlefield {
        settings.extend(object.v4.temporary_base_pt_v1);
        settings.extend(
            object
                .v4
                .creature_upgrade
                .as_ref()
                .into_iter()
                .flat_map(|upgrade| {
                    upgrade
                        .base_stats
                        .into_iter()
                        .chain(upgrade.temporary_creature)
                }),
        );
    }
    settings
        .into_iter()
        .max_by_key(|setting| setting.2)
        .map(|(power, toughness, _)| (power, toughness))
}
