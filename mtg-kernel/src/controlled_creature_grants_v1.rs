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

fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .any(|d| d.controlled_creature_grant.is_some())
    })
}
fn active(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|d| d.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}
fn applies(state: &GameState, source: ObjectId, recipient: ObjectId) -> bool {
    let Some(source_object) = state.objects.try_get(source) else {
        return false;
    };
    state.objects.try_get(recipient).is_some_and(|live| {
        live.zone == Zone::Battlefield
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
    if !enabled() {
        return Ok(());
    }
    if state.objects.iter().any(|(source, live)| {
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
