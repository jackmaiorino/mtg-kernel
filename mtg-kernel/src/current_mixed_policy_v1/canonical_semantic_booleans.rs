//! Preserve exact canonical programs while distinguishing printed semantic
//! booleans from executable raw identity operands. Only a whole revalidated
//! meaning may use these private numeric-tree spellings; other keys still refuse.
use super::{Error, Result};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
fn error() -> Error {
    Error::UnsupportedOperand("canonical_semantic_boolean".into())
}
fn exact<T: DeserializeOwned + Serialize>(value: &Value) -> Result<T> {
    let typed: T = serde_json::from_value(value.clone()).map_err(|_| error())?;
    if serde_json::to_value(&typed).map_err(|_| error())? != *value {
        return Err(error());
    }
    Ok(typed)
}
fn rename(value: &mut Value, old: &str, new: &str) -> Result<()> {
    let fields = value.as_object_mut().ok_or_else(error)?;
    if fields.contains_key(new) {
        return Err(error());
    }
    let boolean = fields.remove(old).ok_or_else(error)?;
    if !boolean.is_boolean() {
        return Err(error());
    }
    fields.insert(new.into(), boolean);
    Ok(())
}
pub(super) fn prepare(value: &mut Value) -> Result<()> {
    if let Some(fields) = value.as_object() {
        let selected = fields
            .get("selector")
            .and_then(|s| s.get("schema"))
            .and_then(Value::as_str);
        match selected {
            Some(crate::rules_meaning_ashroot_v1::SCHEMA) => {
                let typed: crate::rules_meaning_ashroot_v1::MeaningV1 = exact(value)?;
                crate::rules_meaning_ashroot_v1::validate(&typed).map_err(|_| error())?;
                rename(
                    &mut value["rule"],
                    "original_source_owner_battlefield_generation",
                    "original_source_owner_battlefield_incarnation",
                )?;
                rename(
                    &mut value["rule"],
                    "exact_recipient_generation_power_and_toughness_boost",
                    "exact_recipient_incarnation_power_and_toughness_boost",
                )?;
                return Ok(());
            }
            Some(crate::rules_meaning_fiendish_panda_v1::SCHEMA) => {
                let typed: crate::rules_meaning_fiendish_panda_v1::MeaningV1 = exact(value)?;
                crate::rules_meaning_fiendish_panda_v1::validate(&typed).map_err(|_| error())?;
                rename(
                    value,
                    "target_excludes_physical_source_id",
                    "target_excludes_same_physical_source",
                )?;
                return Ok(());
            }
            Some(crate::rules_meaning_strongbox_v1::SCHEMA) => {
                let typed: crate::rules_meaning_strongbox_v1::MeaningV1 = exact(value)?;
                crate::rules_meaning_strongbox_v1::validate(&typed.selector, &typed)
                    .map_err(|_| error())?;
                rename(
                    value,
                    "exact_new_exile_generation",
                    "exact_new_exile_incarnation",
                )?;
                return Ok(());
            }
            _ => {}
        }
        if fields.get("schema").and_then(Value::as_str)
            == Some(crate::rules_meaning_next_five_v59::SCHEMA)
        {
            use crate::rules_meaning_next_five_v59::{
                FamilyV59, InstructionV59, MeaningV59, RuleV59,
            };
            let typed: MeaningV59 = exact(value)?;
            crate::rules_meaning_next_five_v59::validate(&typed).map_err(|_| error())?;
            if typed.family == FamilyV59::Infernal {
                for (rule_index, rule) in typed.ordered_rules.iter().enumerate() {
                    if let RuleV59::Trigger { ordered, .. } = rule {
                        for (op_index, op) in ordered.iter().enumerate() {
                            if matches!(
                                op,
                                InstructionV59::ReturnOriginalGraveyardIncarnation { .. }
                            ) {
                                rename(
                                    &mut value["ordered_rules"][rule_index]["Trigger"]["ordered"]
                                        [op_index]["ReturnOriginalGraveyardIncarnation"],
                                    "requires_exact_successor_generation",
                                    "requires_exact_successor_incarnation",
                                )?;
                            }
                        }
                    }
                }
                return Ok(());
            }
        }
        if fields.get("schema").and_then(Value::as_str)
            == Some(crate::rules_meaning_ravenous_amulet_v51::SCHEMA)
        {
            let typed: crate::rules_meaning_ravenous_amulet_v51::MeaningV51 = exact(value)?;
            crate::rules_meaning_ravenous_amulet_v51::validate(&typed).map_err(|_| error())?;
            rename(
                value,
                "loss_uses_original_generation_lki",
                "loss_uses_original_incarnation_lki",
            )?;
            return Ok(());
        }
    }
    match value {
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
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "tests/canonical_semantic_booleans.rs"]
mod tests;
