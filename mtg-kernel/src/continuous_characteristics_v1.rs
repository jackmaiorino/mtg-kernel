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
        .chain(land_override_timestamp(state, host))
        .chain(crate::standard_creatures_v1::removal_timestamp(state, host))
        .chain(if cfg!(feature = "eternal-fixtures") {
            // Oko, Thief of Crowns's Elks.
            crate::eternal_e5a_v1::statics::removal_timestamp(state, host)
        } else {
            None
        })
        .chain(if cfg!(feature = "eternal-fixtures") {
            // Dress Down: "Creatures lose all abilities."
            crate::eternal_legacy_e7a_v1::removal_timestamp(state, host)
        } else {
            None
        })
        .chain(if cfg!(feature = "eternal-fixtures") {
            // Harbinger of the Seas: nonbasic lands are Islands (305.7).
            crate::eternal_legacy_e8a_v1::statics::removal_timestamp(state, host)
        } else {
            None
        })
        .max()
}

/// Moon uses the same exact two-way incarnation link as other Auras. Do not
/// ask the host's effective type here: that query itself depends on this layer.
pub(crate) fn land_override_timestamp(state: &GameState, host: ObjectId) -> Option<u64> {
    if !cfg!(feature = "limited-fdn-fixtures") {
        return None;
    }
    let object = state.objects.try_get(host)?;
    if object.zone != Zone::Battlefield {
        return None;
    }
    let link = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: object.zone_change_count,
    });
    object
        .attachments
        .iter()
        .copied()
        .filter_map(|aura| {
            let source = state.objects.try_get(aura)?;
            if aura == host
                || source.zone != Zone::Battlefield
                || source.v4.attached_to != link
                || source.v4.face_down_v1.is_some()
                || object.attachments.iter().filter(|&&id| id == aura).count() != 1
                || creature_override(state, aura).is_some_and(|(c, _)| c.loses_abilities)
                || crate::standard_creatures_v1::removal_timestamp(state, aura).is_some()
            {
                return None;
            }
            let definition = CARD_DEFS.get(source.card_def as usize)?;
            (definition.is_executable()
                && definition.attachment == Some(AttachmentDef::AuraLandOverride))
            .then_some(source.v4.layer_timestamp.unwrap_or(0))
        })
        .max()
}

/// Witness sets only Creature in layer 4. Later Moon replaces that with only
/// Land; a later animation can subsequently add Creature/Artifact again.
pub(crate) fn land_type_timestamp(state: &GameState, host: ObjectId) -> Option<u64> {
    let timestamp = land_override_timestamp(state, host)?;
    creature_override(state, host)
        .is_none_or(|(_, other)| timestamp > other)
        .then_some(timestamp)
}

pub(crate) fn land_colorless(state: &GameState, host: ObjectId) -> bool {
    land_override_timestamp(state, host).is_some_and(|timestamp| {
        creature_override(state, host).is_none_or(|(_, other)| timestamp > other)
            && state
                .objects
                .get(host)
                .v4
                .creature_upgrade
                .as_ref()
                .and_then(|upgrade| upgrade.color)
                .is_none_or(|(_, other)| timestamp > other)
            && animation(state, host)
                .is_none_or(|(a, other)| a.colors.is_empty() || timestamp > other)
    })
}

pub(crate) fn later_land_animation(state: &GameState, host: ObjectId, timestamp: u64) -> bool {
    state
        .objects
        .get(host)
        .v4
        .creature_upgrade
        .as_ref()
        .and_then(|upgrade| upgrade.temporary_creature)
        .is_some_and(|(_, _, other)| other > timestamp)
}

/// The granted ability survives only the later layer-6 ability removals. It
/// is independent of the original card's printed mana abilities and amounts.
pub(crate) fn land_grants_colorless_mana(state: &GameState, host: ObjectId) -> bool {
    land_override_timestamp(state, host)
        .is_some_and(|timestamp| removal_timestamp(state, host) == Some(timestamp))
}

pub(crate) fn is_land_subtype(id: u16) -> bool {
    use crate::card_def::Subtype as S;
    matches!(
        S::from_stable_id(id),
        Some(
            S::Forest
                | S::Island
                | S::Mountain
                | S::Plains
                | S::Swamp
                | S::Gate
                | S::Urzas
                | S::Tower
                | S::PowerPlant
                | S::Mine
                | S::Desert
                | S::Sphere
                | S::Town
        )
    )
}

/// Existing observations do not describe the new type/ability/mana override.
/// Select only a face the viewer actually knows. Unknown opposing hands,
/// libraries and neutral face-down sources never select this refusal.
pub(crate) fn refuse_land_override_consumer(
    state: &GameState,
    viewer: crate::ids::PlayerId,
) -> Result<(), String> {
    let selected = state.objects.iter().any(|(id, object)| {
        object.v4.face_down_v1.is_none()
            && (matches!(
                object.zone,
                Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Stack | Zone::Command
            ) || (object.zone == Zone::Hand
                && (object.owner == viewer
                    || state
                        .known_hand_cards(viewer, object.owner)
                        .iter()
                        .any(|entry| {
                            entry.object == id
                                && entry.zone_change_count == object.zone_change_count
                        }))))
            && CARD_DEFS
                .get(usize::from(object.card_def))
                .is_some_and(|def| def.attachment == Some(AttachmentDef::AuraLandOverride))
    });
    if selected {
        Err(
            "land-only Aura characteristics and granted mana require a complete consumer successor"
                .into(),
        )
    } else {
        Ok(())
    }
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
/// timestamp of the ability that applied it. An Aura
/// creature override replaces it entirely while one applies.
pub(crate) fn animation(
    state: &GameState,
    id: ObjectId,
) -> Option<(crate::card_def::AnimationDef, u64)> {
    if !cfg!(any(
        feature = "standard-magezero-fixtures",
        feature = "limited-fdn-fixtures"
    )) {
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
    // Aura animation applies its base stats in layer 7b as well as making
    // its artifact host a creature in layer 4.
    if let Some((animation, timestamp)) = animation(state, id) {
        settings.push((animation.power, animation.toughness, timestamp));
    }
    if cfg!(feature = "eternal-fixtures") {
        // Oko, Thief of Crowns's Elks are 3/3.
        settings.extend(crate::eternal_e5a_v1::statics::base_power_toughness(
            state, id,
        ));
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
