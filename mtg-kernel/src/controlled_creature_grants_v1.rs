//! Immutable definition-owned continuous controlled-creature grants.
//! No serialized state, delayed expiry or printed-card name executor.
use crate::card_def::{CardType, ControlledCreatureGrantDef, Keywords, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub const MEMORIAL: ControlledCreatureGrantDef = ControlledCreatureGrantDef {
    keywords: Keywords(
        Keywords::FLYING.0
            | Keywords::FIRST_STRIKE.0
            | Keywords::VIGILANCE.0
            | Keywords::TRAMPLE.0
            | Keywords::HASTE.0,
    ),
    protection_color_mask: (1 << 2) | (1 << 3),
};

pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .any(|d| d.is_executable() && d.controlled_creature_grant.is_some())
    })
}
pub(crate) fn active(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && state.players[live.controller.index()]
                .battlefield
                .contains(&source)
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|d| d.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}
pub(crate) fn applies(state: &GameState, source: ObjectId, recipient: ObjectId) -> bool {
    let Some(source_object) = state.objects.try_get(source) else {
        return false;
    };
    state.objects.try_get(recipient).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && state.players[live.controller.index()]
                .battlefield
                .contains(&recipient)
            && active(state, source)
            && source_object.controller == live.controller
            && crate::engine::object_has_type(state, recipient, CardType::Creature)
            && crate::continuous_characteristics_v1::grant_survives(
                state,
                recipient,
                source_object.v4.layer_timestamp.unwrap_or(0),
            )
    })
}
pub(crate) fn grants_keyword(state: &GameState, recipient: ObjectId, keyword: Keywords) -> bool {
    if !enabled() {
        return false;
    }
    state.objects.iter().any(|(source, live)| {
        CARD_DEFS
            .get(usize::from(live.card_def))
            .and_then(|d| d.controlled_creature_grant)
            .is_some_and(|grant| grant.keywords.has(keyword) && applies(state, source, recipient))
    })
}
pub(crate) fn color_protection(state: &GameState, recipient: ObjectId) -> u8 {
    if !enabled() {
        return 0;
    }
    state.objects.iter().fold(0, |mask, (source, live)| {
        let grant = CARD_DEFS
            .get(usize::from(live.card_def))
            .and_then(|d| d.controlled_creature_grant);
        if let Some(grant) = grant.filter(|_| applies(state, source, recipient)) {
            mask | grant.protection_color_mask
        } else {
            mask
        }
    })
}
pub(crate) fn protects_from_colors(
    state: &GameState,
    recipient: ObjectId,
    source_colors: u8,
) -> bool {
    color_protection(state, recipient) & source_colors & 0x1f != 0
}
/// Old transports have no precise color-protection/static-source row.
pub(crate) fn require_legacy_consumer_supported(state: &GameState) -> Result<(), String> {
    validate_color_history(state)?;
    if !enabled() {
        return Ok(());
    }
    if state.engine.event_history.iter().any(|event| {
        matches!(
            event,
            crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { .. }
        )
    }) || state.objects.iter().any(|(source, live)| {
        active(state, source)
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|d| d.controlled_creature_grant.is_some())
    }) {
        return Err(
            "controlled-creature color protection requires its explicit consumer successor".into(),
        );
    }
    Ok(())
}

/// 704.5: detach protected Equipment; bestow ceases being an Aura.
pub(crate) fn detach_protected_nonauras(state: &mut GameState) -> bool {
    if !enabled() {
        return false;
    }
    let detach: Vec<_> = state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            let definition = CARD_DEFS.get(usize::from(live.card_def))?;
            let link = live.v4.attached_to?;
            (live.zone == Zone::Battlefield
                && (definition.equipment.is_some() || definition.bestow.is_some())
                && state.objects.try_get(link.object).is_some_and(|host| {
                    host.zone == Zone::Battlefield
                        && host.zone_change_count == link.zone_change_count
                        && host.attachments.contains(&id)
                })
                && protects_from_colors(
                    state,
                    link.object,
                    crate::engine::object_color_mask(state, id),
                ))
            .then_some((id, link))
        })
        .collect();
    for (id, link) in &detach {
        if state.objects.try_get(link.object).is_some() {
            state
                .objects
                .get_mut(link.object)
                .attachments
                .retain(|attachment| *attachment != *id);
        }
        let live = state.objects.get_mut(*id);
        live.v4.attached_to = None;
        let definition = &CARD_DEFS[usize::from(live.card_def)];
        if definition.bestow.is_some() {
            live.v4.effective_subtype_ids =
                definition.subtypes.iter().map(|s| s.stable_id()).collect();
            live.v4.effective_subtype_ids.sort_unstable();
            live.v4.effective_subtype_ids.dedup();
        }
    }
    !detach.is_empty()
}

/// Color LKI is an opt-in core dependency of the new protection descriptor.
/// The actual departure follows this independent snapshot immediately.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DepartureColorV1 {
    pub source: crate::state::AbilitySourceContractV4,
    pub color_mask: u8,
    pub private_snapshot_json: String,
}
pub(crate) fn capture_departure_color(
    state: &GameState,
    event: &crate::event::ProposedEvent,
) -> Option<DepartureColorV1> {
    if !enabled() {
        return None;
    }
    let crate::event::ProposedEvent::ZoneChange(change) = event else {
        return None;
    };
    let live = state.objects.try_get(change.object)?;
    if live.zone != Zone::Battlefield || change.to_zone == Zone::Battlefield {
        return None;
    }
    Some(DepartureColorV1 {
        source: crate::state::AbilitySourceContractV4::capture(state, change.object),
        color_mask: crate::engine::object_color_mask(state, change.object),
        private_snapshot_json: serde_json::to_string(live).expect("GameObject serialization"),
    })
}
pub(crate) fn log_departure_color(state: &mut GameState, snapshot: DepartureColorV1) {
    let event = crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 {
        snapshot,
        departure_history_index: u64::try_from(state.engine.event_history.len())
            .expect("history index")
            + 1,
    };
    state.engine.event_log.push(event.clone());
    state.engine.event_history.push(event);
}
pub(crate) fn validate_color_history(state: &GameState) -> Result<(), String> {
    use crate::event::CommittedEvent;
    if !enabled() {
        if state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, CommittedEvent::ProtectionSourceColorDepartureV1 { .. }))
        {
            return Err("color departure history requires its selected source catalog".into());
        }
        return Ok(());
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ProtectionSourceColorDepartureV1 {
            snapshot,
            departure_history_index,
        } = event
        {
            let index =
                usize::try_from(*departure_history_index).map_err(|_| "color departure index")?;
            if index != at + 1
                || snapshot.source.zone != Zone::Battlefield
                || snapshot.color_mask & !0x1f != 0
                || !matches!(state.engine.event_history.get(index),
                    Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before })
                    if *object == snapshot.source.source && *to != Zone::Battlefield
                        && *controller_before == snapshot.source.controller)
            {
                return Err("color snapshot lacks exact committed battlefield departure".into());
            }
            let captured: crate::state::GameObject =
                serde_json::from_str(&snapshot.private_snapshot_json)
                    .map_err(|_| "invalid original color visibility snapshot")?;
            if serde_json::to_string(&captured).map_err(|_| "color visibility serialization")?
                != snapshot.private_snapshot_json
                || captured.card_def != snapshot.source.card_def
                || captured.owner != snapshot.source.owner
                || captured.controller != snapshot.source.controller
                || captured.zone != snapshot.source.zone
                || captured.zone_change_count != snapshot.source.zone_change_count
                || (captured.v4.face_down_v1.is_some() && snapshot.color_mask != 0)
            {
                return Err("color visibility snapshot changed original source".into());
            }
            let live = state
                .objects
                .try_get(snapshot.source.source)
                .ok_or("color source disappeared")?;
            if snapshot.source.owner != live.owner
                || live.zone_change_count <= snapshot.source.zone_change_count
                || (live.card_def != snapshot.source.card_def
                    && !crate::standard_cards_v1::had_card_def(
                        state,
                        snapshot.source.source,
                        snapshot.source.zone_change_count,
                        snapshot.source.card_def,
                    ))
            {
                return Err("color departure changed original definition/owner/incarnation".into());
            }
        }
        let missing_snapshot = match event {
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to,
                ..
            } if *to != Zone::Battlefield => {
                !matches!(at.checked_sub(1).and_then(|p| state.engine.event_history.get(p)),
                        Some(CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, departure_history_index })
                            if snapshot.source.source == *object
                                && usize::try_from(*departure_history_index).ok() == Some(at))
            }
            _ => false,
        };
        if missing_snapshot {
            return Err("battlefield departure lost its independent source color snapshot".into());
        }
    }
    Ok(())
}
/// Current characteristics win for the original live incarnation. A departed
/// incarnation uses its exact committed prebatch snapshot, never the new object.
pub(crate) fn targeting_source_colors(
    state: &GameState,
    object: ObjectId,
    card_def: u16,
    zone: Zone,
    generation: u32,
) -> Result<u8, String> {
    if state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == generation)
    {
        return Ok(crate::engine::object_color_mask(state, object));
    }
    if !enabled() || zone != Zone::Battlefield {
        return Ok(crate::card_def::mana_colors_mask(
            CARD_DEFS[usize::from(card_def)].colors,
        ));
    }
    validate_color_history(state)?;
    let mut snapshots = state.engine.event_history.iter().filter_map(|event| {
        let crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = event
        else {
            return None;
        };
        (snapshot.source.source == object
            && snapshot.source.zone_change_count == generation
            && (snapshot.source.card_def == card_def
                || crate::standard_cards_v1::had_card_def(state, object, generation, card_def)))
        .then_some(snapshot.color_mask)
    });
    let colors = snapshots
        .next()
        .ok_or("departed targeting source lost exact color LKI")?;
    if snapshots.next().is_some() {
        return Err("duplicate departed source color LKI".into());
    }
    Ok(colors)
}

#[cfg(test)]
mod tests;
