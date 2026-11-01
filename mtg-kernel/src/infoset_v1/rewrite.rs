//! The field-by-field rewrite of a serialized state (W1 design 6.5, 6.8).
//!
//! The state is serialized with object ids tagged ([`ObjectIdTaggingV1`]),
//! so every object reference is found by value. Generations are paired with
//! their object by field name; timestamps are found by field name. Any
//! generation-like or timestamp-like field this module cannot classify is a
//! refusal (R2): the schema fails closed instead of exporting a value with
//! no map entry.

use super::{ExportErrorV1, RefusalCodeV1};
use crate::ids::{ObjectId, ObjectIdTaggingV1, OBJECT_ID_TAG_PREFIX_V1};
use serde_json::{Map, Value};

/// Where a reference sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClassV1 {
    /// Live state: contracts, bindings, attachments, pending items, zones.
    Live,
    /// Event `e` of the committed event history.
    Event(usize),
    /// A history-like record (LKI vectors, turn trackers, trigger uses, the
    /// transient event log). `r` numbers the record's top-level entries.
    Record(usize),
}

/// One object reference found in the state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefV1 {
    pub class: ClassV1,
    /// Reference slot within its event or record (traversal order).
    pub slot: u32,
    pub object: ObjectId,
    /// Every epoch of the object the record stores beside it (a cast
    /// receipt stores its origin and stack epochs), in field order.
    pub generations: Vec<u32>,
}

/// What a reference becomes: the new id and the new epochs, in the order
/// given. `None` leaves it unchanged (collect pass).
pub(crate) type ResolvedV1 = Option<(u32, Vec<u32>)>;

pub(crate) fn tagged_value<T: serde::Serialize>(value: &T) -> Value {
    let _tagging = ObjectIdTaggingV1::begin();
    serde_json::to_value(value).expect("engine state serializes")
}

pub(crate) fn parse_tag(value: &Value) -> Option<ObjectId> {
    value
        .as_str()
        .and_then(|s| s.strip_prefix(OBJECT_ID_TAG_PREFIX_V1))
        .and_then(|n| n.parse().ok())
        .map(ObjectId)
}

fn parse_tag_str(s: &str) -> Option<ObjectId> {
    s.strip_prefix(OBJECT_ID_TAG_PREFIX_V1)
        .and_then(|n| n.parse().ok())
        .map(ObjectId)
}

/// Field names that hold an object generation.
pub(crate) fn is_generation_key(key: &str) -> bool {
    key.contains("zone_change") || key == "generation" || key.ends_with("_generation")
}

/// Field names that hold an effect timestamp.
pub(crate) fn is_timestamp_key(key: &str) -> bool {
    key.contains("timestamp")
}

/// Fields whose value is a `(value, timestamp)` pair.
const TIMESTAMP_PAIRS_V1: &[&str] = &["added_subtype_v1"];

fn generation_partner(id_key: &str, map: &Map<String, Value>) -> Option<String> {
    let stem = id_key.strip_suffix("_id").unwrap_or(id_key);
    [
        format!("{id_key}_zone_change_count"),
        format!("{id_key}_zone_change_generation"),
        format!("{id_key}_generation"),
        format!("{stem}_zone_change_count"),
        format!("{stem}_zone_change_generation"),
        format!("{stem}_generation"),
    ]
    .into_iter()
    .find(|candidate| map.contains_key(candidate))
}

/// Walks `value`, calling `resolve` on every object reference; resolved
/// references are written back.
pub(crate) struct WalkerV1<'a> {
    pub resolve: &'a mut dyn FnMut(RefV1) -> Result<ResolvedV1, ExportErrorV1>,
    /// Called for every timestamp value; returns its replacement (or None).
    pub timestamp: &'a mut dyn FnMut(u64) -> Option<u64>,
    pub slot: u32,
    /// Whether generation fields must pair with a tagged object (off for
    /// passes over an already rewritten state).
    pub check_pairs: bool,
    /// Inside an arena object: generation fields in a map that names no
    /// object are epochs of this object itself (`spell_cast_origin`).
    pub self_object: Option<ObjectId>,
}

fn r2(detail: String) -> ExportErrorV1 {
    ExportErrorV1::refuse(RefusalCodeV1::R2, detail)
}

impl WalkerV1<'_> {
    pub(crate) fn walk(
        &mut self,
        value: &mut Value,
        class: ClassV1,
        parent: &str,
    ) -> Result<(), ExportErrorV1> {
        match value {
            Value::Object(map) => self.walk_map(map, class, parent),
            Value::Array(items) => {
                let tagged = items.iter().filter(|x| parse_tag(x).is_some()).count();
                let numbers = items.iter().filter(|x| x.is_u64() || x.is_i64()).count();
                if tagged > 0 && numbers > 0 {
                    return Err(r2(format!(
                        "an unregistered tuple holds an object and numbers under `{parent}`"
                    )));
                }
                if TIMESTAMP_PAIRS_V1.contains(&parent) && items.len() == 2 {
                    if let Some(t) = items[1].as_u64() {
                        if let Some(new) = (self.timestamp)(t) {
                            items[1] = Value::from(new);
                        }
                    }
                    return self.walk(&mut items[0], class, parent);
                }
                for item in items.iter_mut() {
                    if let Some(object) = parse_tag(item) {
                        let r = RefV1 {
                            class,
                            slot: self.next_slot(),
                            object,
                            generations: Vec::new(),
                        };
                        if let Some((id, _)) = (self.resolve)(r)? {
                            *item = Value::from(id);
                        }
                    } else {
                        self.walk(item, class, parent)?;
                    }
                }
                Ok(())
            }
            Value::String(s) if s.starts_with(OBJECT_ID_TAG_PREFIX_V1) => {
                // A bare id at the top of a walk (callers walk fields).
                let object = parse_tag(value).ok_or_else(|| r2("malformed tag".into()))?;
                let r = RefV1 {
                    class,
                    slot: self.next_slot(),
                    object,
                    generations: Vec::new(),
                };
                if let Some((id, _)) = (self.resolve)(r)? {
                    *value = Value::from(id);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn next_slot(&mut self) -> u32 {
        let s = self.slot;
        self.slot += 1;
        s
    }

    fn walk_map(
        &mut self,
        map: &mut Map<String, Value>,
        class: ClassV1,
        parent: &str,
    ) -> Result<(), ExportErrorV1> {
        // Object-valued fields and their paired generations.
        let id_keys: Vec<String> = map
            .iter()
            .filter(|(_, v)| parse_tag(v).is_some())
            .map(|(k, _)| k.clone())
            .collect();
        let gen_keys: Vec<String> = map
            .iter()
            .filter(|(k, v)| {
                is_generation_key(k) && !v.is_null() && !v.is_object() && !v.is_array()
            })
            .map(|(k, _)| k.clone())
            .collect();
        // (id key or None for the containing object, the object, its
        // generation keys).
        let mut pairs: Vec<(Option<String>, ObjectId, Vec<String>)> = Vec::new();
        let mut claimed: Vec<String> = Vec::new();
        for k in &id_keys {
            let g = generation_partner(k, map).filter(|g| gen_keys.contains(g));
            if let Some(g) = &g {
                claimed.push(g.clone());
            }
            let object = parse_tag(&map[k]).expect("filtered");
            pairs.push((Some(k.clone()), object, g.into_iter().collect()));
        }
        let unclaimed: Vec<String> = gen_keys
            .iter()
            .filter(|g| !claimed.contains(g))
            .cloned()
            .collect();
        if !unclaimed.is_empty() && self.check_pairs {
            let unpaired: Vec<usize> = (0..pairs.len())
                .filter(|&i| pairs[i].2.is_empty())
                .collect();
            if pairs.len() == 1 {
                // One object: every epoch field in the record is its own.
                pairs[0].2.extend(unclaimed);
            } else if unclaimed.len() == 1 && unpaired.len() == 1 {
                pairs[unpaired[0]].2 = unclaimed;
            } else if let (true, Some(own)) = (pairs.is_empty(), self.self_object) {
                pairs.push((None, own, unclaimed));
            } else {
                return Err(r2(format!(
                    "generation field `{}` under `{parent}` pairs with no object",
                    unclaimed.join(",")
                )));
            }
        }
        for (k, object, gens) in pairs {
            let generations = gens
                .iter()
                .map(|g| {
                    map[g]
                        .as_u64()
                        .and_then(|x| u32::try_from(x).ok())
                        .ok_or_else(|| r2(format!("generation `{g}` is not a u32")))
                })
                .collect::<Result<Vec<u32>, _>>()?;
            let r = RefV1 {
                class,
                slot: self.next_slot(),
                object,
                generations,
            };
            if let Some((id, new_gens)) = (self.resolve)(r)? {
                if let Some(k) = k {
                    map.insert(k, Value::from(id));
                }
                for (g, new_gen) in gens.into_iter().zip(new_gens) {
                    map.insert(g, Value::from(new_gen));
                }
            }
        }
        // Object-keyed maps.
        let tagged_keys: Vec<String> = map
            .keys()
            .filter(|k| parse_tag_str(k).is_some())
            .cloned()
            .collect();
        for k in tagged_keys {
            let object = parse_tag_str(&k).expect("filtered");
            let r = RefV1 {
                class,
                slot: self.next_slot(),
                object,
                generations: Vec::new(),
            };
            let mut inner = map.remove(&k).expect("present");
            self.walk(&mut inner, class, parent)?;
            match (self.resolve)(r)? {
                Some((id, _)) => {
                    map.insert(id.to_string(), inner);
                }
                None => {
                    map.insert(k, inner);
                }
            }
        }
        // Timestamps and nested values.
        let keys: Vec<String> = map.keys().cloned().collect();
        for k in keys {
            let v = map.get_mut(&k).expect("present");
            if parse_tag(v).is_some() || (v.is_number() && is_generation_key(&k)) {
                continue;
            }
            if is_timestamp_key(&k) && !TIMESTAMP_PAIRS_V1.contains(&k.as_str()) {
                match v {
                    Value::Null => {}
                    Value::Number(n) => {
                        let t = n
                            .as_u64()
                            .ok_or_else(|| r2(format!("timestamp `{k}` is not a u64")))?;
                        if let Some(new) = (self.timestamp)(t) {
                            *v = Value::from(new);
                        }
                    }
                    _ => {
                        return Err(r2(format!("timestamp field `{k}` has an unexpected shape")));
                    }
                }
                continue;
            }
            self.walk(v, class, &k)?;
        }
        Ok(())
    }
}

/// Static assertion: no tagged id survives the rewrite.
pub(crate) fn has_tag(value: &Value) -> bool {
    match value {
        Value::String(s) => s.starts_with(OBJECT_ID_TAG_PREFIX_V1),
        Value::Array(xs) => xs.iter().any(has_tag),
        Value::Object(m) => m
            .iter()
            .any(|(k, v)| k.starts_with(OBJECT_ID_TAG_PREFIX_V1) || has_tag(v)),
        _ => false,
    }
}
