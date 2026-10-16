//! Passive, opt-in evidence for executed reused tree decisions.
//!
//! Hashes have fixed size; record count follows the existing episode limits.
//! These receipts do not admit historical rows to any utility consumer.

use super::tree::{Canon, Found, Key, Tree};
use serde_json::{json, Value};

pub(super) const SCHEMA: &str = "s4a-reused-decision-witness/v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Recording {
    #[default]
    Disabled,
    Enabled,
}

impl Recording {
    /// Strict runner-boundary parsing; unit tests need no environment changes.
    pub(super) fn parse(setting: Result<String, std::env::VarError>) -> Result<Self, String> {
        match setting.as_deref() {
            Ok("1") => Ok(Self::Enabled),
            Ok("0") | Err(std::env::VarError::NotPresent) => Ok(Self::Disabled),
            Ok(value) => Err(format!("S4A_REUSE_WITNESS must be 0 or 1, not {value}")),
            Err(error) => Err(format!("invalid S4A_REUSE_WITNESS: {error}")),
        }
    }

    pub(super) fn annotate(self, game: &mut Value) {
        if self == Self::Enabled {
            game["reuse_witnesses"] = json!({"schema":SCHEMA,"enabled":true,
                "scope":"every executed reused tree decision",
                "evidence_convention":"natural terminal backups and focal wins; natural draws are nonwins",
                "mean_convention":"focal wins divided by natural terminal backups; not ordinary game score",
                "selection_convention":"greatest supported mean, or best-line value when that backup is enabled"});
        }
    }

    pub(super) fn capture(
        self,
        identity: impl FnOnce() -> Result<Value, String>,
        search: usize,
        search_record: &Value,
        selected: Selected<'_>,
    ) -> Result<Option<Value>, String> {
        if self == Self::Disabled {
            return Ok(None);
        }
        let bad = || "reused-decision witness binding differs or is missing".to_owned();
        let tree_hash = search_record["tree"]["hash"].as_str().ok_or_else(bad)?;
        if search_record["search"].as_u64() != Some(search as u64) || !hex(tree_hash, 64) {
            return Err(bad());
        }
        let node = match selected
            .tree
            .find(
                &selected.key,
                &selected.parent,
                &selected.parent_edge,
                selected.canon,
            )
            .map_err(super::bank::err)?
        {
            Found::Hit(node) => node,
            Found::Miss => return Err(bad()),
        };
        let edge = selected.canon.edges.get(selected.edge).ok_or_else(bad)?;
        if node.frozen_choice() != Some(selected.edge)
            || selected.canon.menu.get(selected.applied as usize) != Some(edge)
        {
            return Err(bad());
        }
        let backups = *node.n.get(selected.edge).ok_or_else(bad)?;
        let wins = *node.wins.get(selected.edge).ok_or_else(bad)?;
        if selected.backups != Some(backups) || wins > backups {
            return Err(bad());
        }
        let mean = wins as f64 / backups as f64;
        let (rule, value) = match &node.bl {
            Some(bestline) => (
                super::refresh::bestline_rule(bestline.m),
                bestline.q[selected.edge],
            ),
            None => ("mean".to_owned(), mean),
        };
        if !value.is_finite() {
            return Err(bad());
        }
        let decision = identity()?;
        if decision["k"].as_u64() != Some(selected.canon.menu.len() as u64)
            || !["step", "physical_decision_id", "substep"]
                .iter()
                .all(|key| decision[key].as_u64().is_some())
            || !["obs_hash", "menu_hash"]
                .iter()
                .all(|key| decision[key].as_str().is_some_and(|x| hex(x, 64)))
            || !decision["state_hash"].as_str().is_some_and(|x| hex(x, 16))
            || !search_record["step"]
                .as_u64()
                .zip(decision["step"].as_u64())
                .is_some_and(|(root, live)| root <= live)
        {
            return Err(bad());
        }
        // Tree hashes already belong to the immutable search record. Do not
        // rehash the whole frozen tree at every actual reused decision.
        Ok(Some(
            json!({"schema":SCHEMA,"decision":decision,"search":search,
            "search_tree_hash":tree_hash,"node_key":super::pathlog::key_hex(&selected.key),
            "storage_edge_index":selected.edge,"storage_edge_sha256":super::sha(edge),
            "applied_live_index":selected.applied,
            "natural_terminal_backups":backups,"focal_wins":wins,
            "mean_focal_win_rate":mean,"selection_backup":rule,"selection_value":value}),
        ))
    }
}

/// The storage mapping already obtained by the cursor, after final live
/// representative mapping and before the keyed step is consumed by advance.
pub(super) struct Selected<'a> {
    pub(super) tree: &'a Tree,
    pub(super) key: Key,
    pub(super) parent: Key,
    pub(super) parent_edge: Vec<u8>,
    pub(super) canon: &'a Canon,
    pub(super) edge: usize,
    pub(super) backups: Option<u64>,
    pub(super) applied: u32,
}

fn hex(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x))
}

#[cfg(test)]
mod tests {
    use super::super::tree::{BestLine, Node};
    use super::*;

    fn fixture() -> (Tree, Canon, Key, Value) {
        let canon = Canon {
            obs: vec![7],
            menu: vec![vec![2], vec![1]],
            edges: vec![vec![1], vec![2]],
        };
        let key = [3; 32];
        let mut node = Node::with_perm([0; 32], vec![], canon.clone(), 2, vec![0, 1]);
        node.n = vec![12, 9];
        node.wins = vec![9, 2];
        node.visits = 21;
        let mut tree = Tree::default();
        tree.nodes.insert(key, node);
        let record = json!({"search":2,"step":1,"tree":{"hash":tree.hash()}});
        (tree, canon, key, record)
    }

    fn identity() -> Result<Value, String> {
        Ok(json!({"step":4,"physical_decision_id":3,"substep":0,"k":2,
            "obs_hash":"a".repeat(64),"menu_hash":"b".repeat(64),"state_hash":"c".repeat(16)}))
    }

    fn selected<'a>(tree: &'a Tree, canon: &'a Canon, key: Key) -> Selected<'a> {
        Selected {
            tree,
            key,
            parent: [0; 32],
            parent_edge: vec![],
            canon,
            edge: 0,
            backups: Some(12),
            applied: 1,
        }
    }

    #[test]
    fn reuse_witness_opt_in_is_strict_and_disabled_is_lazy() {
        assert_eq!(
            Recording::parse(Err(std::env::VarError::NotPresent)).unwrap(),
            Recording::Disabled
        );
        assert_eq!(
            Recording::parse(Ok("0".into())).unwrap(),
            Recording::Disabled
        );
        assert_eq!(
            Recording::parse(Ok("1".into())).unwrap(),
            Recording::Enabled
        );
        for value in ["", "true", "01", " 1", "1 ", "2"] {
            assert!(Recording::parse(Ok(value.into())).is_err());
        }
        assert!(Recording::parse(Err(std::env::VarError::NotUnicode("bad".into()))).is_err());
        let (tree, canon, key, _) = fixture();
        assert!(Recording::Disabled
            .capture(
                || panic!("disabled recording evaluated identity"),
                2,
                &Value::Null,
                selected(&tree, &canon, key)
            )
            .unwrap()
            .is_none());
        let mut game = json!({"actions":[1],"decisions":[]});
        let bytes = serde_json::to_vec(&game).unwrap();
        Recording::Disabled.annotate(&mut game);
        assert_eq!(bytes, serde_json::to_vec(&game).unwrap());
        Recording::Enabled.annotate(&mut game);
        assert_eq!(game["reuse_witnesses"]["schema"], SCHEMA);
    }

    #[test]
    fn reuse_witness_binds_storage_edge_to_final_live_mapping() {
        let (tree, canon, key, record) = fixture();
        let got = Recording::Enabled
            .capture(identity, 2, &record, selected(&tree, &canon, key))
            .unwrap()
            .unwrap();
        assert_eq!(got["storage_edge_index"], 0);
        assert_eq!(got["applied_live_index"], 1);
        assert_eq!(got["storage_edge_sha256"], super::super::sha(&[1]));
        assert_eq!(got["natural_terminal_backups"], 12);
        assert_eq!(got["focal_wins"], 9);
        assert_eq!(got["mean_focal_win_rate"], 0.75);
        assert_eq!(got["selection_value"], 0.75);
        assert_eq!(got["node_key"], super::super::pathlog::key_hex(&key));
        assert_eq!(got["search_tree_hash"], tree.hash());
        // A fresh live ordering must bind to its corresponding live index.
        let mut remapped = canon.clone();
        remapped.menu.swap(0, 1);
        let mut mapping = selected(&tree, &remapped, key);
        mapping.applied = 0;
        let mapped = Recording::Enabled
            .capture(identity, 2, &record, mapping)
            .unwrap()
            .unwrap();
        assert_eq!(mapped["applied_live_index"], 0);
        assert_eq!(mapped["storage_edge_sha256"], got["storage_edge_sha256"]);
    }

    #[test]
    fn reuse_witness_rejects_stale_missing_or_wrong_edge_evidence() {
        let (tree, canon, key, record) = fixture();
        for mutation in 0..6 {
            let mut mapping = selected(&tree, &canon, key);
            match mutation {
                0 => mapping.key = [9; 32],
                1 => mapping.backups = None,
                2 => mapping.backups = Some(11),
                3 => mapping.applied = 0,
                4 => mapping.edge = 1,
                _ => mapping.parent = [1; 32],
            }
            assert!(Recording::Enabled
                .capture(identity, 2, &record, mapping)
                .is_err());
        }
        assert!(Recording::Enabled
            .capture(identity, 3, &record, selected(&tree, &canon, key))
            .is_err());
        assert!(Recording::Enabled
            .capture(identity, 2, &Value::Null, selected(&tree, &canon, key))
            .is_err());
        assert!(Recording::Enabled
            .capture(
                || Ok(json!({"k":2})),
                2,
                &record,
                selected(&tree, &canon, key)
            )
            .is_err());
    }

    #[test]
    fn reuse_witness_keeps_raw_win_evidence_distinct_from_best_line_value() {
        let (mut tree, canon, key, _) = fixture();
        let mut bestline = BestLine::new(2, 8);
        bestline.q = vec![0.6, 0.1];
        tree.nodes.get_mut(&key).unwrap().bl = Some(Box::new(bestline));
        let record = json!({"search":2,"step":1,"tree":{"hash":tree.hash()}});
        let got = Recording::Enabled
            .capture(identity, 2, &record, selected(&tree, &canon, key))
            .unwrap()
            .unwrap();
        assert_eq!(got["selection_backup"], "bestline-m8");
        assert_eq!(got["selection_value"], 0.6);
        assert_eq!(got["mean_focal_win_rate"], 0.75);
    }
}
