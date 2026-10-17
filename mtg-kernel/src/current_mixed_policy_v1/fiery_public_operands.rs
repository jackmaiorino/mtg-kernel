//! Convert only the exact public Fiery transport's target incarnation operands.
//! No private state lookup or generic raw-reference exception is introduced.
use super::{Error, Result};
use crate::fiery_public_v1::{FactsV1, TargetIncarnationV1, SCHEMA};
use serde_json::{json, Value};
fn error() -> Error {
    Error::UnsupportedOperand("fiery_public_operand".into())
}
fn relation(target: TargetIncarnationV1) -> Value {
    json!({"object":target.arena_id,"generation":target.zone_change_count})
}
fn convert(value: &mut Value) -> Result<()> {
    let typed: FactsV1 = serde_json::from_value(value.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *value
        || typed.schema != SCHEMA
        || typed.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || typed.producer_sha256 != crate::fiery_public_v1::producer_sha256_v1()
    {
        return Err(error());
    }
    let root = |source: &crate::rl::CardStableRefV1, digest: &str| -> Result<()> {
        let selector = crate::rules_meaning_fiery_v1::live_selector().map_err(|_| error())?;
        let meaning = crate::rules_meaning_fiery_v1::meaning(&selector, source.card_db_id, 0)
            .map_err(|_| error())?;
        if meaning.root_sha256 != digest {
            return Err(error());
        }
        Ok(())
    };
    if let Some(pending) = &typed.pending_cast_targets {
        root(&pending.announced_source, &pending.printed_root_sha256)?;
        if pending.caster != pending.announced_source.controller
            || pending.mandatory_creature_count != 1
            || pending.equipment_minimum != 0
            || pending.equipment_maximum != 1
            || pending.selected_targets.len() > 2
        {
            return Err(error());
        }
        value["pending_cast_targets"]["selected_targets"] = Value::Array(
            pending
                .selected_targets
                .iter()
                .copied()
                .map(relation)
                .collect(),
        );
    }
    for (index, replacement) in typed.death_replacements.iter().enumerate() {
        root(
            &replacement.source_at_resolution,
            &replacement.printed_root_sha256,
        )?;
        value["death_replacements"][index]["original_target"] =
            relation(replacement.original_target);
    }
    Ok(())
}
pub(super) fn prepare(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(fields) if fields.get("schema").and_then(Value::as_str) == Some(SCHEMA) => {
            // Conversion consumes this complete, authenticated public DTO once.
            // Existing normalization then binds both halves to its ordinal tables.
            convert(value)?;
        }
        Value::Object(fields) => {
            for child in fields.values_mut() {
                prepare(child)?;
            }
        }
        Value::Array(rows) => {
            for child in rows {
                prepare(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}
