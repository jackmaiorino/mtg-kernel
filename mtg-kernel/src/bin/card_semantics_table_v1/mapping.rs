//! The mapping layer: a reviewed, data-only map from card-named effect
//! leaves (and card-named trigger conditions or cost parts) to the shared
//! tokens a generic definition of the same rules would yield. It lives in
//! `data/card_semantics_v1/leaf_mapping_v1.json` and is read only here; no
//! rules code reads it.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Emit {
    Count(String, i64),
    Value(String, i64),
}

#[derive(Debug, Clone)]
pub struct MapEntry {
    pub emit: Vec<Emit>,
    pub why: String,
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub entries: BTreeMap<String, MapEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileV1 {
    schema: String,
    #[allow(dead_code)]
    rules: Vec<String>,
    entries: Vec<EntryV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryV1 {
    source: String,
    emit: Vec<EmitV1>,
    why: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmitV1 {
    token: String,
    #[serde(default)]
    count: Option<i64>,
    #[serde(default)]
    value: Option<i64>,
}

pub const MAPPING_SCHEMA: &str = "card_semantics_leaf_mapping/v1";

/// Prefixes a mapping source may carry: the walker consults the map at
/// exactly these points.
const SOURCE_PREFIXES: [&str; 4] = ["leaf:", "trigger:", "cond:", "costpart:"];

pub fn parse(text: &str) -> Result<Mapping, String> {
    let file: FileV1 = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.schema != MAPPING_SCHEMA {
        return Err(format!(
            "mapping schema {} is not {MAPPING_SCHEMA}",
            file.schema
        ));
    }
    let mut entries = BTreeMap::new();
    for entry in file.entries {
        if !SOURCE_PREFIXES.iter().any(|p| entry.source.starts_with(p)) {
            return Err(format!(
                "mapping source {} has no known prefix",
                entry.source
            ));
        }
        if entry.why.trim().is_empty() {
            return Err(format!(
                "mapping source {} has no justification",
                entry.source
            ));
        }
        let mut emit = Vec::new();
        for item in entry.emit {
            emit.push(match (item.count, item.value) {
                (Some(n), None) => Emit::Count(item.token, n),
                (None, Some(v)) => Emit::Value(item.token, v),
                (None, None) => Emit::Count(item.token, 1),
                (Some(_), Some(_)) => {
                    return Err(format!(
                        "mapping source {}: token {} has both count and value",
                        entry.source, item.token
                    ))
                }
            });
        }
        if emit.is_empty() {
            return Err(format!("mapping source {} emits nothing", entry.source));
        }
        if entries
            .insert(
                entry.source.clone(),
                MapEntry {
                    emit,
                    why: entry.why,
                },
            )
            .is_some()
        {
            return Err(format!("mapping source {} listed twice", entry.source));
        }
    }
    Ok(Mapping { entries })
}
