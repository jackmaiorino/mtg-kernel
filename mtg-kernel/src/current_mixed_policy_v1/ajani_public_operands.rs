//! Convert only the exact public Ajani V42 transport's incarnation operands:
//! each activation's target and created tokens. No private state lookup or
//! generic raw-reference exception is introduced.
use super::{Error, Result};
use crate::ajani_public_v42::{FactsV42, IncarnationV42};
use serde_json::{json, Value};
fn error() -> Error {
    Error::UnsupportedOperand("ajani_public_operand".into())
}
fn relation(value: IncarnationV42) -> Value {
    json!({"object":value.arena_id,"generation":value.zone_change_count})
}
fn convert(value: &mut Value) -> Result<()> {
    let typed: FactsV42 = serde_json::from_value(value.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *value {
        return Err(error());
    }
    if typed.activations.is_empty() {
        // No incarnation operand is present; the value is left unchanged.
        return Ok(());
    }
    if typed.meanings != crate::ajani_public_v42::meanings().map_err(|_| error())? {
        return Err(error());
    }
    for (index, activation) in typed.activations.iter().enumerate() {
        if !typed
            .meanings
            .iter()
            .any(|meaning| meaning.card_id == activation.source_at_payment.card_db_id)
        {
            return Err(error());
        }
        let row = &mut value["activations"][index];
        row["target"] = activation.target.map_or(Value::Null, relation);
        row["created_tokens"] = Value::Array(
            activation
                .created_tokens
                .iter()
                .copied()
                .map(relation)
                .collect(),
        );
    }
    Ok(())
}
/// The Ajani facts carry no schema tag of their own. They are recognized only
/// as the `ajani` field beside the `families` field of the shared common
/// observation, the one typed route the V75 projection authenticates.
pub(super) fn prepare(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("families") {
                if let Some(ajani) = fields.get_mut("ajani") {
                    convert(ajani)?;
                }
            }
            for (key, child) in fields.iter_mut() {
                if key != "ajani" {
                    prepare(child)?;
                }
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
