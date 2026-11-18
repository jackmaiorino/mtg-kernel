//! Convert only the exact public Krenko transport's created token incarnations.
//! No private state lookup or generic raw-reference exception is introduced.
use super::{Error, Result};
use crate::krenko_public_v1::{FactsV1, TokenIncarnationV1, SCHEMA};
use crate::rules_meaning_krenko_v1::{self as meaning, RootV1};
use serde_json::{json, Value};
fn error() -> Error {
    Error::UnsupportedOperand("krenko_public_operand".into())
}
fn relation(token: TokenIncarnationV1) -> Value {
    json!({"object":token.arena_id,"generation":token.zone_change_count})
}
fn convert(value: &mut Value) -> Result<()> {
    let typed: FactsV1 = serde_json::from_value(value.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *value
        || typed.schema != SCHEMA
        || typed.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || typed.producer_sha256 != crate::krenko_public_v1::producer_sha256_v1()
    {
        return Err(error());
    }
    // The selector exists only in the Limited catalog, so it is read only when
    // a printed source or activation row actually needs its root.
    let root = |card: u16, digest: &str| -> Result<()> {
        let selector = meaning::live_selector().map_err(|_| error())?;
        let printed = meaning::meaning(
            &selector,
            card,
            RootV1::PrintedTapActivation { ability_index: 0 },
        )
        .map_err(|_| error())?;
        if printed.root_sha256 != digest {
            return Err(error());
        }
        Ok(())
    };
    for source in &typed.sources {
        root(source.source.card_db_id, &source.printed_root_sha256)?;
    }
    for (index, activation) in typed.activations.iter().enumerate() {
        root(
            activation.source_at_payment.card_db_id,
            &activation.printed_root_sha256,
        )?;
        // Each created token is the exact incarnation the resolved activation
        // produced. Existing normalization binds it to the same object and
        // incarnation ordinals as the token's stable battlefield reference.
        value["activations"][index]["created_tokens"] = Value::Array(
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
pub(super) fn prepare(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(fields) if fields.get("schema").and_then(Value::as_str) == Some(SCHEMA) => {
            // Conversion consumes this complete, authenticated public DTO once.
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
