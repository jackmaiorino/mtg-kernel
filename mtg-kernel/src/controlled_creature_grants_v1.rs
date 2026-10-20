//! Immutable definition-owned continuous controlled-creature grants.
//! No serialized state, delayed expiry or printed-card name executor.
use crate::card_def::{CardType, ControlledCreatureGrantDef, Keywords, CARD_DEFS};
use crate::event_history_v1::EventKindV1;
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
            || crate::source_type_hexproof_v1::enabled()
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
fn grant_of(
    live: &crate::state::GameObject,
) -> Option<crate::card_def::ControlledCreatureGrantDef> {
    CARD_DEFS
        .get(usize::from(live.card_def))
        .and_then(|d| d.controlled_creature_grant)
}
/// Every object whose definition carries a grant, in arena order. Objects
/// without one never grant or protect, so the scans below may skip them.
fn grant_sources(state: &GameState) -> Option<std::rc::Rc<[ObjectId]>> {
    crate::continuous_source_frame_v1::sources(
        state,
        crate::continuous_source_frame_v1::SourceKindV1::ControlledCreatureGrant,
        || {
            state
                .objects
                .iter()
                .filter(|(_, live)| grant_of(live).is_some())
                .map(|(source, _)| source)
                .collect()
        },
    )
}
pub(crate) fn grants_keyword(state: &GameState, recipient: ObjectId, keyword: Keywords) -> bool {
    if !enabled() {
        return false;
    }
    let grants = |source: ObjectId, live: &crate::state::GameObject| {
        grant_of(live)
            .is_some_and(|grant| grant.keywords.has(keyword) && applies(state, source, recipient))
    };
    if let Some(sources) = grant_sources(state) {
        return sources
            .iter()
            .any(|&source| grants(source, state.objects.get(source)));
    }
    state
        .objects
        .iter()
        .any(|(source, live)| grants(source, live))
}
/// Every keyword `grants_keyword` reports for `recipient`, as one set: a
/// keyword is granted exactly when some applying source's grant holds it.
pub(crate) fn granted_keywords(state: &GameState, recipient: ObjectId) -> Keywords {
    if !enabled() {
        return Keywords::NONE;
    }
    let granted = |source: ObjectId, live: &crate::state::GameObject| {
        grant_of(live)
            .filter(|grant| grant.keywords != Keywords::NONE && applies(state, source, recipient))
            .map_or(Keywords::NONE, |grant| grant.keywords)
    };
    if let Some(sources) = grant_sources(state) {
        return sources.iter().fold(Keywords::NONE, |all, &source| {
            all | granted(source, state.objects.get(source))
        });
    }
    state
        .objects
        .iter()
        .fold(Keywords::NONE, |all, (source, live)| {
            all | granted(source, live)
        })
}
pub(crate) fn color_protection(state: &GameState, recipient: ObjectId) -> u8 {
    if !enabled() {
        return 0;
    }
    let protect = |mask: u8, source: ObjectId, live: &crate::state::GameObject| {
        if let Some(grant) = grant_of(live).filter(|_| applies(state, source, recipient)) {
            mask | grant.protection_color_mask
        } else {
            mask
        }
    };
    if let Some(sources) = grant_sources(state) {
        return sources.iter().fold(0, |mask, &source| {
            protect(mask, source, state.objects.get(source))
        });
    }
    state
        .objects
        .iter()
        .fold(0, |mask, (source, live)| protect(mask, source, live))
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
    let mut retained_grant_source = false;
    for event in state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::ProtectionSourceColorDepartureV1])
    {
        let crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = event
        else {
            continue;
        };
        if CARD_DEFS
            .get(usize::from(snapshot.source.card_def))
            .is_some_and(|definition| definition.controlled_creature_grant.is_some())
        {
            let captured: crate::state::GameObject =
                serde_json::from_str(&snapshot.private_snapshot_json)
                    .map_err(|_| "invalid original color visibility snapshot")?;
            retained_grant_source |=
                captured.v4.face_down_v1.is_none() && captured.v4.face_index == 0;
        }
    }
    // Ordinary color LKI remains mandatory for future protection, but an
    // unrelated departure does not introduce a controlled-creature grant.
    // A masked source cannot select a refusal through its hidden definition.
    if retained_grant_source
        || state.objects.iter().any(|(source, live)| {
            active(state, source)
                && CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|d| d.controlled_creature_grant.is_some())
        })
    {
        return Err(
            "controlled-creature color protection requires its explicit consumer successor".into(),
        );
    }
    Ok(())
}

/// 704.5: detach Equipment from noncreatures or protected hosts; bestow
/// ceases being an Aura when its creature attachment becomes illegal.
pub(crate) fn detach_illegal_nonauras(state: &mut GameState) -> bool {
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
                && (!crate::engine::object_has_type(
                    state,
                    link.object,
                    crate::card_def::CardType::Creature,
                ) || protects_from_colors(
                    state,
                    link.object,
                    crate::engine::object_color_mask(state, id),
                )))
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
    #[serde(
        default,
        skip_serializing_if = "crate::source_type_hexproof_v1::SourceTypeProjectionV1::is_empty"
    )]
    pub source_types_v1: crate::source_type_hexproof_v1::SourceTypeProjectionV1,
    #[serde(
        default,
        skip_serializing_if = "crate::source_type_hexproof_v1::SourceTypeInputProjectionV1::is_empty"
    )]
    pub source_type_input_v1: crate::source_type_hexproof_v1::SourceTypeInputProjectionV1,
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
    let typed_stack = crate::source_type_hexproof_v1::enabled()
        && live.zone == Zone::Stack
        && change.to_zone != Zone::Stack;
    if !typed_stack && (live.zone != Zone::Battlefield || change.to_zone == Zone::Battlefield) {
        return None;
    }
    Some(DepartureColorV1 {
        source: crate::state::AbilitySourceContractV4::capture(state, change.object),
        color_mask: crate::engine::object_color_mask(state, change.object),
        private_snapshot_json: serde_json::to_string(live).expect("GameObject serialization"),
        source_types_v1: crate::source_type_hexproof_v1::SourceTypeProjectionV1(
            crate::source_type_hexproof_v1::enabled().then(|| {
                crate::source_type_hexproof_v1::current_types(state, change.object)
                    .expect("actual departure retains selected source types")
            }),
        ),
        source_type_input_v1: crate::source_type_hexproof_v1::SourceTypeInputProjectionV1(
            crate::source_type_hexproof_v1::enabled().then(|| {
                Box::new(
                    crate::source_type_hexproof_v1::capture_type_input(state, change.object)
                        .expect("actual prebatch type input"),
                )
            }),
        ),
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
            .scan_for_v1(&[EventKindV1::ProtectionSourceColorDepartureV1])
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
                || !(snapshot.source.zone == Zone::Battlefield
                    || (crate::source_type_hexproof_v1::enabled()
                        && snapshot.source.zone == Zone::Stack))
                || snapshot.color_mask & !0x1f != 0
                || !matches!(state.engine.event_history.get(index),
                    Some(CommittedEvent::ZoneChange { object, from, to, controller_before })
                    if *object == snapshot.source.source && *from == snapshot.source.zone && to != from
                        && *controller_before == snapshot.source.controller)
            {
                return Err("color snapshot lacks exact committed battlefield departure".into());
            }
            if crate::source_type_hexproof_v1::enabled() != snapshot.source_types_v1.0.is_some()
                || crate::source_type_hexproof_v1::enabled()
                    != snapshot.source_type_input_v1.0.is_some()
                || snapshot
                    .source_types_v1
                    .0
                    .is_some_and(|types| !types.valid())
            {
                return Err("source-type departure projection missing or foreign".into());
            }
            if crate::source_type_hexproof_v1::enabled() {
                crate::source_type_hexproof_v1::validate_snapshot_types(
                    snapshot,
                    state.objects.len(),
                )?;
            }
            validate_snapshot_object(snapshot)?;
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
                object, from, to, ..
            } if (*from == Zone::Battlefield
                || (crate::source_type_hexproof_v1::enabled() && *from == Zone::Stack))
                && to != from =>
            {
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
/// History validation runs this for every retained departure on every
/// decision. It depends only on the snapshot, so a snapshot that passed once
/// passes again; every other case, including every failure, takes the full
/// check below.
fn validate_snapshot_object(snapshot: &DepartureColorV1) -> Result<(), &'static str> {
    thread_local! {
        static PASSED: std::cell::RefCell<std::collections::HashSet<DepartureColorV1>> =
            std::cell::RefCell::default();
    }
    // try_with: during thread teardown the set may be gone; then this is
    // simply the full check.
    if PASSED
        .try_with(|passed| passed.borrow().contains(snapshot))
        .unwrap_or(false)
    {
        return Ok(());
    }
    validate_snapshot_object_full(snapshot)?;
    let _ = PASSED.try_with(|passed| {
        let mut passed = passed.borrow_mut();
        // A game retains few departures; the small cap bounds memory.
        if passed.len() >= 256 {
            passed.clear();
        }
        passed.insert(snapshot.clone());
    });
    Ok(())
}
fn validate_snapshot_object_full(snapshot: &DepartureColorV1) -> Result<(), &'static str> {
    let captured: crate::state::GameObject = serde_json::from_str(&snapshot.private_snapshot_json)
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
        return Err("color visibility snapshot changed original source");
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
    let mut snapshots = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::ProtectionSourceColorDepartureV1])
        .iter()
        .filter_map(|event| {
            let crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } =
                event
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
