//! Passive per-attempt search log for offline rescoring (collab
//! LANES/bestline-step0-20261013), enabled by `S4A_PATH_LOG=<directory>`.
//!
//! Every E selection writes one `<id>.search.json`: each stored node's
//! parent link, depth, menu size, final edge counts and labelled edges (Spy
//! cast and targets, Dread Return targets, mana abilities with a colour
//! choice), and each natural simulation's tree path, result, transitions and
//! the number of focal nonforced choices the rollout policy made after the
//! tree path. A guide also writes `<id>.actual.json`: the tree node keys its
//! actual history matched in each search's tree.
//!
//! Nothing here is read by selection, backups, sampling or execution; with
//! the variable unset no log exists and no file is written.

use super::arms::{RootCtx, SimRecord};
use super::labels::spy_defs;
use super::play::decision;
use super::tree::{live_index, Canon, Key, Node, Tree};
use crate::card_def::CARD_DEFS;
use crate::ids::PlayerId;
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
use crate::rl_session::FastActorSessionV1;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) const SEARCH_SCHEMA: &str = "s4a-path-log/search-v1";
pub(crate) const ACTUAL_SCHEMA: &str = "s4a-path-log/actual-v1";

/// The log directory from `S4A_PATH_LOG` (unset or empty: no log).
pub(crate) fn dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| match std::env::var("S4A_PATH_LOG") {
        Ok(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => None,
    })
    .clone()
}

pub(crate) fn key_hex(k: &Key) -> String {
    k.iter().map(|b| format!("{b:02x}")).collect()
}

/// Labelled edges of a node's menu, by storage edge index; null when none.
fn labels(s: &FastActorSessionV1, cn: &Canon, focal: PlayerId) -> Value {
    let Some(sem) = s.diagnostic_current_action_semantics() else {
        return Value::Null;
    };
    let defs = spy_defs();
    let me = PlayerSeatV1::from(focal);
    let (mut cast, mut own, mut other, mut giant, mut dr) =
        (vec![], vec![], vec![], vec![], vec![]);
    let mut mana = Vec::new();
    for (e, edge) in cn.edges.iter().enumerate() {
        let Some(x) = live_index(cn, edge).ok().and_then(|l| sem.get(l as usize)) else {
            continue;
        };
        match x {
            ActionSemanticV1::CastSpell { source, .. } if source.card_db_id == defs.spy => {
                cast.push(e);
            }
            ActionSemanticV1::ChooseTarget { source, target, .. }
                if source.card_db_id == defs.spy =>
            {
                if matches!(target, TargetRefV1::Player { player } if *player == me) {
                    own.push(e);
                } else {
                    other.push(e);
                }
            }
            ActionSemanticV1::ChooseTarget { source, target, .. }
                if source.card_db_id == defs.dread_return =>
            {
                if matches!(target, TargetRefV1::Object { object } if object.card_db_id == defs.giant)
                {
                    giant.push(e);
                } else {
                    dr.push(e);
                }
            }
            ActionSemanticV1::ActivateManaAbility {
                source,
                mana_choice: Some(colour),
                cost_target,
                ..
            } => mana.push(json!([
                e,
                CARD_DEFS.get(source.card_db_id as usize).map(|c| c.name),
                format!("{colour:?}"),
                source.arena_id,
                cost_target.as_ref().map(|t| t.arena_id)
            ])),
            _ => {}
        }
    }
    let mut v = serde_json::Map::new();
    for (name, list) in [
        ("spy_cast", cast),
        ("spy_self", own),
        ("spy_other", other),
        ("dr_giant", giant),
        ("dr_other", dr),
    ] {
        if !list.is_empty() {
            v.insert(name.into(), json!(list));
        }
    }
    if !mana.is_empty() {
        v.insert("mana".into(), Value::Array(mana));
    }
    if v.is_empty() {
        Value::Null
    } else {
        Value::Object(v)
    }
}

/// One selection's log.
#[derive(Debug, Default)]
pub(crate) struct PathLog {
    dir: PathBuf,
    ids: HashMap<Key, u32>,
    nodes: Vec<(Key, Value)>,
    sims: Vec<Value>,
    /// Labels of nodes created by the simulation in progress.
    pending: HashMap<Key, Value>,
}

impl PathLog {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            ..Self::default()
        }
    }

    /// A new simulation: labels from an unfinished one are dropped.
    pub(crate) fn begin_sim(&mut self) {
        self.pending.clear();
    }

    /// A node created at `s` (before its first action is applied).
    pub(crate) fn created(
        &mut self,
        key: Key,
        s: &FastActorSessionV1,
        cn: &Canon,
        focal: PlayerId,
    ) {
        self.pending.insert(key, labels(s, cn, focal));
    }

    /// A natural simulation about to be backed up: assigns ids to its new
    /// nodes in path order and records the attempt.
    pub(crate) fn natural(
        &mut self,
        ordinal: u64,
        path: &[(Key, usize)],
        new: &[(Key, Node)],
        win: bool,
        rec: &SimRecord,
    ) {
        let mut flat: Vec<i64> = Vec::with_capacity(path.len() * 2);
        for (i, (k, a)) in path.iter().enumerate() {
            let id = match self.ids.get(k) {
                Some(&id) => id,
                None => {
                    // Every path node is stored or new; -1 marks a broken log.
                    let Some((_, node)) = new.iter().find(|(nk, _)| nk == k) else {
                        flat.extend([-1, *a as i64]);
                        continue;
                    };
                    let id = self.nodes.len() as u32;
                    let (parent, edge) = match i.checked_sub(1).map(|j| path[j]) {
                        Some((pk, pa)) => (self.ids.get(&pk).map(|&p| p as i64), Some(pa)),
                        None => (Some(-1), None),
                    };
                    let row = json!({"key":key_hex(k),"parent":parent,"edge":edge,
                        "depth":node.depth,"k":node.canon.edges.len(),"perm":node.perm,
                        "labels":self.pending.remove(k).unwrap_or(Value::Null)});
                    self.ids.insert(*k, id);
                    self.nodes.push((*k, row));
                    id
                }
            };
            flat.extend([i64::from(id), *a as i64]);
        }
        self.sims.push(json!([
            ordinal,
            u8::from(win),
            rec.transitions,
            rec.rollout_choices,
            flat
        ]));
        self.pending.clear();
    }

    /// Writes the log with the final tree's counts; returns the file path.
    pub(crate) fn write(
        mut self,
        ctx: &RootCtx,
        tree: &Tree,
        completed: u64,
        attempted: u64,
        transitions: u64,
    ) -> Result<PathBuf, String> {
        let tree_hash = tree.hash();
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for (k, mut row) in std::mem::take(&mut self.nodes) {
            if let Some(n) = tree.nodes.get(&k) {
                row["visits"] = json!(n.visits);
                row["n"] = json!(n.n);
                row["wins"] = json!(n.wins);
                if n.skip.iter().any(|&x| x) {
                    row["skip"] = json!(n.skip);
                }
            }
            nodes.push(row);
        }
        let root_step = decision(ctx.root).map(|d| d.step);
        let v = json!({"schema":SEARCH_SCHEMA,"model":ctx.seeds.model,"root":ctx.seeds.root,
            "focal":ctx.focal.0,"root_step":root_step,"select_cap":ctx.limits.select_cap,
            "urgency":ctx.urgency,"key_mode":ctx.key_mode.label(),"interchange":ctx.interchange,
            "tree_hash":tree_hash,"tree_nodes":tree.nodes.len(),"completed":completed,
            "attempted":attempted,"transitions":transitions,
            "sim_fields":["ordinal","win","transitions","rollout_choices","path"],
            "nodes":nodes,"sims":self.sims});
        let id = super::sha(
            format!("{}\0{}\0{}", ctx.seeds.model, ctx.seeds.root, tree_hash).as_bytes(),
        );
        write_new(&self.dir, &id[..16], "search", &v)
    }
}

/// Writes `value` to `<dir>/<stem>.<kind>.json`, never overwriting.
fn write_new(dir: &Path, stem: &str, kind: &str, value: &Value) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    for n in 0..1000 {
        let name = if n == 0 {
            format!("{stem}.{kind}.json")
        } else {
            format!("{stem}-{n}.{kind}.json")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(&bytes).map_err(|e| e.to_string())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("too many path logs with one name".into())
}

/// A guide's actual history in its searches' trees.
#[derive(Debug, Default)]
pub(crate) struct ActualLog {
    hits: Vec<Value>,
}

impl ActualLog {
    /// Decision `step` of the actual game matched `key` in search `search`.
    pub(crate) fn hit(&mut self, search: usize, step: u64, key: &Key) {
        self.hits.push(json!([search, step, key_hex(key)]));
    }

    /// Writes the guide's log next to its search logs.
    pub(crate) fn write(self, dir: &Path, header: Value) -> Result<PathBuf, String> {
        let mut v = header;
        let id = super::sha(
            format!(
                "{}\0{}\0{}",
                v["lineage"].as_str().unwrap_or(""),
                v["search_root"].as_str().unwrap_or(""),
                v["id"].as_str().unwrap_or("")
            )
            .as_bytes(),
        );
        v["schema"] = json!(ACTUAL_SCHEMA);
        v["hit_fields"] = json!(["search", "step", "key"]);
        v["hits"] = Value::Array(self.hits);
        write_new(dir, &id[..16], "actual", &v)
    }
}
