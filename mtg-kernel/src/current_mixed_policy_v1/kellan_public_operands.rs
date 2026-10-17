//! Normalize only authenticated public Kellan upgrade operands and ordinary
//! public exile permissions. No GameState/private owner enters this projection.
use super::{Error, Result};
use crate::{rl::CardStableRefV1, state::AbilitySourceContractV4};
use serde::Serialize;
use serde_json::{json, Value};
fn error() -> Error {
    Error::UnsupportedOperand("kellan_public_operand".into())
}
fn exact<T: serde::de::DeserializeOwned + Serialize>(value: &Value) -> Result<T> {
    let typed: T = serde_json::from_value(value.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *value {
        return Err(error());
    }
    Ok(typed)
}
fn grant(source: AbilitySourceContractV4) -> Value {
    let stable = CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    };
    json!({"source":stable,"attached_to":source.attached_to.map(|link|json!({"object":link.object.0,"generation":link.zone_change_count}))})
}
fn upgrade(value: &Value) -> Result<Value> {
    use crate::standard_creatures_v1::CreatureUpgradeV1;
    let CreatureUpgradeV1 {
        finality,
        temporary_creature,
        suppressed_by,
        graveyard_adventure,
        haste_blockers_only,
        creature_types,
        base_stats,
        color,
        keyword_grants,
        keyword_losses,
        combat_impulse,
        once_activated,
        wurmlet_resolved_turn,
        combat_impulse_source,
        combat_impulse_donor,
    } = exact(value)?;
    let pt = |value: Option<(i16, i16, u64)>| {
        value.map(|(power,toughness,timestamp)|json!({"power":power,"toughness":toughness,"timestamp":timestamp}))
    };
    let keywords = |values: Vec<(crate::card_def::Keywords, u64)>| {
        values
            .into_iter()
            .map(|(keywords, timestamp)| json!({"keywords":keywords.0,"timestamp":timestamp}))
            .collect::<Vec<_>>()
    };
    Ok(json!({
        "finality":finality,"temporary_creature":pt(temporary_creature),
        "suppressed_by":suppressed_by.into_iter().map(|entry|json!({"source":{"object":entry.source.object.0,"generation":entry.source.zone_change_count},"timestamp":entry.timestamp})).collect::<Vec<_>>(),
        "graveyard_adventure":graveyard_adventure,"haste_blockers_only":haste_blockers_only,
        "creature_types":creature_types.map(|(types,timestamp)|json!({"types":types,"timestamp":timestamp})),
        "base_stats":pt(base_stats),"color":color.map(|(mask,timestamp)|json!({"mask":mask,"timestamp":timestamp})),
        "keyword_grants":keywords(keyword_grants),"keyword_losses":keywords(keyword_losses),
        "combat_impulse":combat_impulse.map(|timestamp|json!({"timestamp":timestamp})),
        "combat_impulse_source":combat_impulse_source.map(grant),"combat_impulse_donor":combat_impulse_donor.map(grant),
        "once_activated":once_activated,"wurmlet_resolved_turn":wurmlet_resolved_turn.map(|(turn,active_player)|json!({"turn":turn,"active_player":active_player})),
    }))
}
fn permissions(value: &mut Value) -> Result<()> {
    let Value::Array(rows) = value else {
        return Err(error());
    };
    for row in rows {
        let permission: crate::rl::ExilePlayPermissionPublicV2 = exact(row)?;
        if permission.zone_change_generation != permission.object.zone_change_count {
            return Err(error());
        }
        let fields = row.as_object_mut().ok_or_else(error)?;
        fields.remove("zone_change_generation");
        fields.insert("permission_incarnation".into(),json!({"object":permission.object.arena_id,"generation":permission.zone_change_generation}));
    }
    Ok(())
}
pub(super) fn prepare(value: &mut Value) -> Result<()> {
    match value {
        Value::Array(rows) => {
            for row in rows {
                prepare(row)?;
            }
        }
        Value::Object(fields) => {
            if let (Some(stable), Some(state)) =
                (fields.get("stable"), fields.get("creature_upgrade"))
            {
                let public: CardStableRefV1 = exact(stable)?;
                let is_kellan = |id: u16| {
                    crate::card_def::CARD_DEFS
                        .get(usize::from(id))
                        .is_some_and(|d| d.name == "Kellan, Planar Trailblazer")
                };
                // Current public identity or an already public historical grant
                // selects this conversion. No hidden world lookup is performed.
                let historical = if state.is_null() {
                    false
                } else {
                    let typed: crate::standard_creatures_v1::CreatureUpgradeV1 = exact(state)?;
                    typed
                        .combat_impulse_source
                        .into_iter()
                        .chain(typed.combat_impulse_donor)
                        .any(|source| is_kellan(source.card_def))
                };
                if !state.is_null() && (is_kellan(public.card_db_id) || historical) {
                    let normalized = upgrade(state)?;
                    fields.insert("creature_upgrade".into(), normalized);
                    // Do not visit that converted value a second time.
                    for (key, child) in fields.iter_mut() {
                        if key != "creature_upgrade" {
                            prepare(child)?;
                        }
                    }
                    return Ok(());
                }
            }
            for (key, child) in fields {
                if key == "exile_play_permissions" {
                    permissions(child)?;
                } else {
                    prepare(child)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
