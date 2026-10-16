//! Exact Fishing family transport conversion, including an equipped host that
//! has since departed. Only authenticated public incarnation pairs are mapped.
use super::{Error, Result};
use crate::{
    akroma_family_public_v40::FamilyFactsV40,
    fishing_pole_public_v39::IncarnationV39,
    rules_meaning_fishing_pole_v39::{self as meaning, RootV39},
};
use serde_json::{json, Value};

fn error() -> Error {
    Error::UnsupportedOperand("fishing_public_operand".into())
}
fn relation(value: IncarnationV39) -> Value {
    json!({"object":value.arena_id,"generation":value.zone_change_count})
}
fn convert(value: &mut Value) -> Result<()> {
    let typed: FamilyFactsV40 = serde_json::from_value(value.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *value {
        return Err(error());
    }
    let FamilyFactsV40::Fishing(facts) = typed else {
        return Err(error());
    };
    let selector = meaning::live_selector().map_err(|_| error())?;
    for root in &facts.roots {
        let canonical =
            meaning::meaning(&selector, root.card_id, root.root).map_err(|_| error())?;
        if canonical.root_sha256 != root.root_sha256 {
            return Err(error());
        }
    }
    for (index, pole) in facts.poles.iter().enumerate() {
        meaning::meaning(
            &selector,
            pole.source.card_db_id,
            RootV39::PrintedArtifactSpell,
        )
        .map_err(|_| error())?;
        if let Some(host) = pole.attached_host {
            value["Fishing"]["poles"][index]["attached_host"] = relation(host);
        }
    }
    for (index, operation) in facts.operations.iter().enumerate() {
        let root = if operation.trigger {
            RootV39::EquippedCreatureUntaps
        } else {
            RootV39::GrantedBaitActivation
        };
        let canonical = meaning::meaning(&selector, operation.original_pole.card_db_id, root)
            .map_err(|_| error())?;
        if canonical.root_sha256 != operation.root_sha256 {
            return Err(error());
        }
        value["Fishing"]["operations"][index]["original_host"] = relation(operation.original_host);
        if let Some(fish) = operation.created_fish {
            value["Fishing"]["operations"][index]["created_fish"] = relation(fish);
        }
    }
    Ok(())
}
pub(super) fn prepare(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(fields) if fields.contains_key("Fishing") => convert(value)?,
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
