//! Stable dungeon/room kinds in the authenticated common observation projection.
//! These u16 program kinds are not physical object or incarnation references.
use super::{Error, Result};
use crate::rl::PublicObservationProjectionV5;
use serde_json::Value;

fn error() -> Error {
    Error::UnsupportedOperand("public_projection_operand".into())
}

fn convert(value: &mut Value) -> Result<()> {
    let typed: PublicObservationProjectionV5 =
        serde_json::from_value(value.clone()).map_err(|_| error())?;
    // Legacy projection structs permit unknown serde members. Exact round-trip
    // equality closes every nested struct, including flattened projection fields,
    // before any key changes. The owning backend already authenticated the full
    // current observation; this helper grants no additional dungeon permission.
    if serde_json::to_value(&typed).map_err(|_| error())? != *value {
        return Err(error());
    }
    rename_dungeon_kinds(value, typed.surface.player_status.len())
}

fn rename_dungeon_kinds(value: &mut Value, seats: usize) -> Result<()> {
    for seat in 0..seats {
        let fields = value["player_status"][seat]["dungeon"]
            .as_object_mut()
            .ok_or_else(error)?;
        for (old, new) in [("dungeon_id", "dungeon_kind"), ("room_id", "room_kind")] {
            let kind = fields.remove(old).ok_or_else(error)?;
            fields.insert(new.into(), kind);
        }
    }
    Ok(())
}

#[cfg(feature = "eternal-fixtures")]
pub(super) fn prepare_premodern(observation: &mut Value) -> Result<()> {
    let stable = observation.get_mut("stable").ok_or_else(error)?;
    if stable.is_null() {
        return Ok(());
    }
    let projection = stable.get_mut("projection").ok_or_else(error)?;
    let typed: crate::rl::PublicObservationProjectionV2 =
        serde_json::from_value(projection.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *projection {
        return Err(error());
    }
    rename_dungeon_kinds(projection, typed.player_status.len())
}

/// The input is an owned observation, before reference conversion or root
/// expansion. V50/V51 carry this projection directly; V52/V57 (and successors)
/// retain it through their explicit `common` envelopes. No arbitrary JSON scan
/// or raw-identity-key exception is used.
pub(super) fn prepare(observation: &mut Value) -> Result<()> {
    if let Some(projection) = observation.get_mut("projection") {
        convert(projection)?;
    }
    if let Some(common) = observation.get_mut("common") {
        prepare(common)?;
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "tests/public_projection_operands.rs"]
mod tests;
