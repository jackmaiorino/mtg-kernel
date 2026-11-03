//! The map between the imported session's menu order and the bridge's
//! lockstep rows (W1 design v7, sections 4 and 7).
//!
//! The engine lists candidates in native-id order, so the imported state
//! (canonical ids) and the lockstep session (native ids) list the same menu
//! in different orders. A row's **canonical key** is a digest of its V4
//! menu semantics with every `arena_id` renamed to the object's canonical
//! arena index (identity on the imported state; through the export's `C:`
//! handles on the lockstep session) and the bookkeeping fields AC3 drops
//! (hashes, zone change counts, decision and step counters) removed. AC3
//! compares the two menus as sets of exactly these forms; the order map
//! pairs them row by row.

use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Hex digits kept from a row key's SHA-256.
const KEY_HEX: usize = 16;

/// Renames `arena_id`s through `ids` (none: keep them, the imported side),
/// drops the bookkeeping fields AC3 drops and sorts the lists the engine
/// keeps in native-id order (legal targets, object lists). An `arena_id`
/// missing from `ids` becomes `"unmapped:<n>"`, which never equals an
/// imported row.
pub(crate) fn canonical_rename_v1(v: &mut Value, ids: Option<&BTreeMap<u64, u64>>) {
    match v {
        Value::Object(m) => {
            m.retain(|k, _| {
                !k.contains("hash")
                    && !matches!(
                        k.as_str(),
                        "zone_change_count" | "physical_decision_id" | "step_index"
                    )
            });
            for (k, x) in m.iter_mut() {
                if k == "arena_id" {
                    if let (Some(n), Some(ids)) = (x.as_u64(), ids) {
                        *x = ids
                            .get(&n)
                            .map_or(serde_json::json!(format!("unmapped:{n}")), |c| {
                                serde_json::json!(c)
                            });
                    }
                    continue;
                }
                canonical_rename_v1(x, ids);
            }
            if let Some(Value::Array(xs)) = m.get_mut("legal_targets") {
                xs.sort_by_key(|x| x.to_string());
            }
        }
        Value::Array(xs) => {
            xs.iter_mut().for_each(|x| canonical_rename_v1(x, ids));
            // Lists of objects the engine keeps in native-id order (known
            // hand cards, candidates) compare as sets.
            let objects = !xs.is_empty()
                && xs.iter().all(|x| {
                    x.as_object()
                        .is_some_and(|m| m.contains_key("stable") || m.contains_key("arena_id"))
                });
            if objects {
                xs.sort_by_key(|x| x.to_string());
            }
        }
        _ => {}
    }
}

/// Native object id to canonical arena index for the export's tracked
/// objects (`C:<n>` sits at arena index n of the exported state).
pub(crate) fn canonical_ids_v1(export: &super::InfosetV1) -> BTreeMap<u64, u64> {
    export
        .tracked_natives
        .iter()
        .enumerate()
        .map(|(n, id)| (u64::from(id.0), n as u64))
        .collect()
}

/// The canonical key of every row of the current decision, in the
/// session's own row order. None at a terminal or when the V4 menu cannot
/// be read.
pub(crate) fn row_keys_v1(
    s: &FastActorSessionV1,
    ids: Option<&BTreeMap<u64, u64>>,
) -> Option<Vec<String>> {
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        return None;
    };
    let (_, menu) = s.actor_visible_decision_v4(d).ok()?;
    Some(
        menu.iter()
            .map(|row| {
                let mut v = serde_json::to_value(row).expect("semantics serialize");
                canonical_rename_v1(&mut v, ids);
                let digest = Sha256::digest(v.to_string().as_bytes());
                format!("{digest:x}")[..KEY_HEX].to_string()
            })
            .collect(),
    )
}

/// For each imported row i, the lockstep row with the same key: None
/// unless the keys pair the two menus one to one (every key distinct on
/// each side and present on both).
pub(crate) fn live_rows_v1(lockstep: &[String], imported: &[String]) -> Option<Vec<u32>> {
    if lockstep.len() != imported.len() {
        return None;
    }
    let mut by_key: BTreeMap<&str, u32> = BTreeMap::new();
    for (row, key) in lockstep.iter().enumerate() {
        if by_key.insert(key.as_str(), u32::try_from(row).ok()?).is_some() {
            return None;
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    imported
        .iter()
        .map(|key| {
            let row = *by_key.get(key.as_str())?;
            seen.insert(row).then_some(row)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn live_rows_pair_equal_keys_and_refuse_anything_else() {
        assert_eq!(
            live_rows_v1(&keys(&["a", "b", "c"]), &keys(&["c", "a", "b"])),
            Some(vec![2, 0, 1])
        );
        assert_eq!(live_rows_v1(&keys(&["a"]), &keys(&["a"])), Some(vec![0]));
        // A key on one side only, duplicates, or a length mismatch.
        assert_eq!(live_rows_v1(&keys(&["a", "b"]), &keys(&["a", "x"])), None);
        assert_eq!(live_rows_v1(&keys(&["a", "a"]), &keys(&["a", "a"])), None);
        assert_eq!(live_rows_v1(&keys(&["a", "b"]), &keys(&["a", "a"])), None);
        assert_eq!(live_rows_v1(&keys(&["a", "b"]), &keys(&["a"])), None);
    }

    #[test]
    fn rename_maps_ids_drops_bookkeeping_and_sorts_object_lists() {
        let ids: BTreeMap<u64, u64> = [(40, 1), (41, 0)].into_iter().collect();
        let mut live = serde_json::json!({"kind": "x", "zone_change_count": 3,
            "state_hash": 9, "cards": [{"arena_id": 40}, {"arena_id": 41}],
            "legal_targets": [{"arena_id": 41}, {"arena_id": 40}], "other": {"arena_id": 7}});
        canonical_rename_v1(&mut live, Some(&ids));
        assert_eq!(
            live,
            serde_json::json!({"kind": "x", "cards": [{"arena_id": 0}, {"arena_id": 1}],
                "legal_targets": [{"arena_id": 0}, {"arena_id": 1}],
                "other": {"arena_id": "unmapped:7"}})
        );
        let mut imported = serde_json::json!({"cards": [{"arena_id": 1}, {"arena_id": 0}]});
        canonical_rename_v1(&mut imported, None);
        assert_eq!(
            imported,
            serde_json::json!({"cards": [{"arena_id": 0}, {"arena_id": 1}]})
        );
    }
}
