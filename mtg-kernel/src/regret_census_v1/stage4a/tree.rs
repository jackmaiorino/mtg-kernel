//! Candidate E: persistent information-set sequence UCT (RUNNER.md section 2).
//!
//! A node is the focal player's observation/action history from the root.
//! Its key chains SHA-256 over the parent's key, the executed edge, and the
//! canonical bytes of the focal player's observation and legal menu at the
//! node. Canonical bytes come from the acting player's V4 observation and
//! frozen-source menu (`actor_visible_decision_v4`), with:
//! - engine object handles (`arena_id`) replaced by their order of first
//!   appearance within that one observation (observation, then menu), and
//!   `zone_change_count` replaced by its offset from the smallest count shown
//!   for the same object in that observation, so neither hidden allocation
//!   IDs nor absolute incarnation counters enter a key;
//! - the step index, physical decision ID and projection hash removed (they
//!   count opponent transitions and engine bookkeeping, not observations).
//!
//! Edges are canonical semantic bytes (with an occurrence ordinal for exact
//! duplicates); an edge is matched to a fresh live candidate in every clone.
//! Each node stores its canonical bytes, parent key and edge, so a key
//! collision is detected and reported as an instrument fault.

use super::play::PlayErr;
use super::seeds::{Purpose, RootSeeds};
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use crate::state::SplitMix64;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(crate) const UCB_C2: f64 = 2.0; // (sqrt 2)^2
pub(crate) const MAX_DEPTH: u32 = 32;
pub(crate) const MIN_EXEC_BACKUPS: u64 = 8;

pub(crate) type Key = [u8; 32];

/// Canonical view of one focal decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Canon {
    /// Canonical observation bytes.
    pub(crate) obs: Vec<u8>,
    /// Canonical bytes per live candidate, in live order, with an occurrence
    /// ordinal appended to exact duplicates.
    pub(crate) menu: Vec<Vec<u8>>,
    /// The same canonical actions, sorted: a node's edges.
    pub(crate) edges: Vec<Vec<u8>>,
}

/// Engine handles and counters found in one observation (first pass).
#[derive(Default)]
struct Handles {
    /// Object handles in order of first appearance (`arena_id` fields and
    /// `attachments` lists).
    order: Vec<u64>,
    /// Smallest `zone_change_count` shown per object.
    min_zcc: HashMap<u64, u64>,
    /// Distinct continuous-effect `timestamp` values (sorted later).
    stamps: Vec<u64>,
}

impl Handles {
    fn see(&mut self, id: u64) {
        if !self.order.contains(&id) {
            self.order.push(id);
        }
    }

    fn index(&self, id: u64) -> u64 {
        self.order.iter().position(|&x| x == id).expect("collected") as u64
    }
}

fn collect_ids(v: &Value, h: &mut Handles) {
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                h.see(id);
                if let Some(z) = m.get("zone_change_count").and_then(Value::as_u64) {
                    let e = h.min_zcc.entry(id).or_insert(z);
                    *e = (*e).min(z);
                }
            }
            if let Some(Value::Array(a)) = m.get("attachments") {
                a.iter().filter_map(Value::as_u64).for_each(|id| h.see(id));
            }
            if let Some(t) = m.get("timestamp").and_then(Value::as_u64) {
                if !h.stamps.contains(&t) {
                    h.stamps.push(t);
                }
            }
            for x in m.values() {
                collect_ids(x, h);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_ids(x, h)),
        _ => {}
    }
}

/// Second pass: object handles become first-appearance indices, incarnation
/// counters become offsets from the smallest shown for that object, effect
/// timestamps become their rank within the observation, and exile-permission
/// generations (absolute counters) are dropped.
fn rewrite(v: &mut Value, h: &Handles) {
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                m.insert("arena_id".into(), Value::from(h.index(id)));
                if let Some(z) = m.get("zone_change_count").and_then(Value::as_u64) {
                    let base = h.min_zcc.get(&id).copied().unwrap_or(z);
                    m.insert("zone_change_count".into(), Value::from(z - base));
                }
            }
            if let Some(Value::Array(a)) = m.get_mut("attachments") {
                for x in a.iter_mut() {
                    if let Some(id) = x.as_u64() {
                        *x = Value::from(h.index(id));
                    }
                }
            }
            if let Some(t) = m.get("timestamp").and_then(Value::as_u64) {
                let rank = h.stamps.iter().filter(|&&s| s < t).count() as u64;
                m.insert("timestamp".into(), Value::from(rank));
            }
            m.remove("zone_change_generation");
            for (k, x) in m.iter_mut() {
                if k != "attachments" {
                    rewrite(x, h);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| rewrite(x, h)),
        _ => {}
    }
}

/// Sort key of an observation element: its JSON with engine handles and
/// incarnation counters masked.
fn masked(v: &Value) -> String {
    fn mask(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for k in ["arena_id", "zone_change_count", "attachments"] {
                    if m.contains_key(k) {
                        m.insert(k.into(), Value::Null);
                    }
                }
                m.values_mut().for_each(mask);
            }
            Value::Array(a) => a.iter_mut().for_each(mask),
            _ => {}
        }
    }
    let mut x = v.clone();
    mask(&mut x);
    x.to_string()
}

/// Observation lists whose order carries no game information in this card
/// pool (hands, battlefield, graveyards, exile, known hand cards, effects,
/// relations, permissions, historical sources, a library search's offered
/// cards) but can follow engine handles or hidden draw order (a mill reveals
/// the library in its sampled order). Every other list keeps engine order:
/// stack, targets, combat orders, pending triggers. Never applied to the menu.
const UNORDERED_LISTS: [&str; 10] = [
    "own_hand",
    "known_hand_cards",
    "battlefield",
    "graveyards",
    "exile",
    "continuous_effects",
    "object_relations",
    "exile_play_permissions",
    "historical_public_sources",
    "cards",
];

fn sort_by_content(a: &mut Vec<Value>) {
    if a.len() > 1 && a.iter().all(Value::is_object) {
        let mut keyed: Vec<(String, Value)> = a.drain(..).map(|x| (masked(&x), x)).collect();
        keyed.sort_by(|x, y| x.0.cmp(&y.0));
        a.extend(keyed.into_iter().map(|(_, x)| x));
    }
}

fn sort_object_lists(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                sort_object_lists(x);
                if UNORDERED_LISTS.contains(&k.as_str()) {
                    if let Value::Array(a) = x {
                        // Per-seat pairs ([[..], [..]]) sort within each seat.
                        if a.iter().all(Value::is_array) {
                            for inner in a.iter_mut() {
                                if let Value::Array(i) = inner {
                                    sort_by_content(i);
                                }
                            }
                        } else {
                            sort_by_content(a);
                        }
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(sort_object_lists),
        _ => {}
    }
}

/// Timestamps become their rank within the observation; exile-permission
/// generations are dropped (both are absolute engine counters).
fn rank_counters(v: &mut Value) {
    fn stamps(v: &Value, out: &mut Vec<u64>) {
        match v {
            Value::Object(m) => {
                if let Some(t) = m.get("timestamp").and_then(Value::as_u64) {
                    out.push(t);
                }
                m.values().for_each(|x| stamps(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| stamps(x, out)),
            _ => {}
        }
    }
    fn apply(v: &mut Value, all: &[u64]) {
        match v {
            Value::Object(m) => {
                if let Some(t) = m.get("timestamp").and_then(Value::as_u64) {
                    let rank = all.iter().filter(|&&s| s < t).count() as u64;
                    m.insert("timestamp".into(), Value::from(rank));
                }
                m.remove("zone_change_generation");
                m.values_mut().for_each(|x| apply(x, all));
            }
            Value::Array(a) => a.iter_mut().for_each(|x| apply(x, all)),
            _ => {}
        }
    }
    let mut all = Vec::new();
    stamps(v, &mut all);
    all.sort_unstable();
    all.dedup();
    apply(v, &all);
}

/// Canonical bytes of the current focal decision.
pub(crate) fn canon(s: &FastActorSessionV1, d: FastActorDecisionV1) -> Result<Canon, PlayErr> {
    let (obs, sem) = s
        .actor_visible_decision_v4(d)
        .map_err(|e| PlayErr::Fault(format!("actor-visible decision: {e:?}")))?;
    let mut obs = serde_json::to_value(&obs).map_err(|e| PlayErr::Fault(e.to_string()))?;
    if let Value::Object(m) = &mut obs {
        for k in [
            "step_index",
            "physical_decision_id",
            "visible_projection_hash",
            "kernel_version",
        ] {
            m.remove(k);
        }
    }
    let mut sem: Vec<Value> = sem
        .iter()
        .map(|x| serde_json::to_value(x).map_err(|e| PlayErr::Fault(e.to_string())))
        .collect::<Result<_, _>>()?;
    rank_counters(&mut obs);
    for x in &mut sem {
        rank_counters(x);
    }
    sort_object_lists(&mut obs);
    let mut handles = Handles::default();
    collect_ids(&obs, &mut handles);
    // Handles named only by the menu are indexed in content order, not in
    // live candidate order (which can follow handles).
    let mut by_content: Vec<&Value> = sem.iter().collect();
    by_content.sort_by_cached_key(|x| masked(x));
    for x in by_content {
        collect_ids(x, &mut handles);
    }
    rewrite(&mut obs, &handles);
    for x in &mut sem {
        rewrite(x, &handles);
    }
    let obs = serde_json::to_vec(&obs).map_err(|e| PlayErr::Fault(e.to_string()))?;
    let mut raw: Vec<Vec<u8>> = Vec::with_capacity(sem.len());
    let mut menu: Vec<Vec<u8>> = Vec::with_capacity(sem.len());
    for x in &sem {
        let b = serde_json::to_vec(x).map_err(|e| PlayErr::Fault(e.to_string()))?;
        // Exact duplicates get an occurrence ordinal (JSON never ends in '#').
        let dup = raw.iter().filter(|r| **r == b).count() as u64;
        raw.push(b.clone());
        let mut m = b;
        if dup > 0 {
            m.push(b'#');
            m.extend_from_slice(&dup.to_le_bytes());
        }
        menu.push(m);
    }
    // Live candidate order can follow engine handles (identical cards), so
    // the node's edge list is the sorted set of canonical actions.
    let mut edges = menu.clone();
    edges.sort();
    Ok(Canon { obs, menu, edges })
}

pub(crate) fn child_key(parent: &Key, edge: &[u8], c: &Canon) -> Key {
    let mut h = Sha256::new();
    let mut put = |b: &[u8]| {
        h.update((b.len() as u64).to_le_bytes());
        h.update(b);
    };
    put(b"stage4a-node-v1");
    put(parent);
    put(edge);
    put(&c.obs);
    put(&(c.edges.len() as u64).to_le_bytes());
    for m in &c.edges {
        put(m);
    }
    h.finalize().into()
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) parent: Key,
    pub(crate) edge: Vec<u8>,
    pub(crate) canon: Canon,
    /// Seeded order of the menu (untried choice and tie-breaks).
    pub(crate) perm: Vec<usize>,
    /// Completed simulations through this node (every action is legal at
    /// every visit, since the key includes the menu).
    pub(crate) visits: u64,
    pub(crate) n: Vec<u64>,
    pub(crate) wins: Vec<u64>,
    pub(crate) depth: u32,
}

fn permutation(len: usize, seed: u64) -> Vec<usize> {
    let mut p: Vec<usize> = (0..len).collect();
    let mut rng = SplitMix64::seed(seed);
    for i in (1..len).rev() {
        let bound = (i + 1) as u64;
        let threshold = bound.wrapping_neg() % bound;
        let j = loop {
            let v = rng.next_u64();
            if v >= threshold {
                break (v % bound) as usize;
            }
        };
        p.swap(i, j);
    }
    p
}

impl Node {
    pub(crate) fn new(
        key: &Key,
        parent: Key,
        edge: Vec<u8>,
        canon: Canon,
        depth: u32,
        seeds: &RootSeeds,
    ) -> Self {
        let k = canon.edges.len();
        Self {
            parent,
            edge,
            perm: permutation(k, seeds.get(Purpose::Ties, "E", 0, key)),
            canon,
            visits: 0,
            n: vec![0; k],
            wins: vec![0; k],
            depth,
        }
    }

    /// Selection rule: the first untried action in the seeded order; once
    /// all are tried, maximum UCB, ties to the seeded order.
    pub(crate) fn select(&self) -> usize {
        self.select_with(None)
    }

    /// `select`, or with `Some(u)` the finite-urgency candidate (collab
    /// LANES/spy-history-support-plan-20261010/FPU-CANDIDATE.md): an action
    /// with no natural terminal backup scores `u`, every other action its
    /// UCB score, and the maximum wins, ties to the seeded order.
    pub(crate) fn select_with(&self, urgency: Option<f64>) -> usize {
        if let Some(u) = urgency {
            let ln = (self.visits.max(2) as f64).ln();
            let mut best = self.perm[0];
            let mut best_v = f64::NEG_INFINITY;
            for &a in &self.perm {
                let v = if self.n[a] == 0 {
                    u
                } else {
                    let n = self.n[a] as f64;
                    self.wins[a] as f64 / n + (UCB_C2 * ln / n).sqrt()
                };
                if v > best_v {
                    best_v = v;
                    best = a;
                }
            }
            return best;
        }
        if let Some(&a) = self.perm.iter().find(|&&a| self.n[a] == 0) {
            return a;
        }
        let ln = (self.visits.max(2) as f64).ln();
        let mut best = self.perm[0];
        let mut best_v = f64::NEG_INFINITY;
        for &a in &self.perm {
            let n = self.n[a] as f64;
            let v = self.wins[a] as f64 / n + (UCB_C2 * ln / n).sqrt();
            if v > best_v {
                best_v = v;
                best = a;
            }
        }
        best
    }

    /// Frozen execution: greatest mean among edges with at least eight
    /// natural terminal backups, ties to the seeded order.
    pub(crate) fn frozen_choice(&self) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for &a in &self.perm {
            if self.n[a] < MIN_EXEC_BACKUPS {
                continue;
            }
            let m = self.wins[a] as f64 / self.n[a] as f64;
            if best.is_none_or(|(_, b)| m > b) {
                best = Some((a, m));
            }
        }
        best.map(|(a, _)| a)
    }

    fn same(&self, parent: &Key, edge: &[u8], canon: &Canon) -> bool {
        self.parent == *parent
            && self.edge == edge
            && self.canon.obs == canon.obs
            && self.canon.edges == canon.edges
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Tree {
    pub(crate) nodes: HashMap<Key, Node>,
}

/// Lookup result.
pub(crate) enum Found<'a> {
    Hit(&'a Node),
    Miss,
}

impl Tree {
    pub(crate) fn find(
        &self,
        key: &Key,
        parent: &Key,
        edge: &[u8],
        canon: &Canon,
    ) -> Result<Found<'_>, PlayErr> {
        match self.nodes.get(key) {
            Some(n) if n.same(parent, edge, canon) => Ok(Found::Hit(n)),
            Some(_) => Err(PlayErr::Fault("node key collision".into())),
            None => Ok(Found::Miss),
        }
    }

    /// Backs up one natural terminal result over a completed simulation's
    /// path (inserting its new node, if any).
    pub(crate) fn backup(&mut self, path: &[(Key, usize)], new: Option<(Key, Node)>, win: bool) {
        if let Some((k, n)) = new {
            self.nodes.insert(k, n);
        }
        for (k, a) in path {
            let n = self.nodes.get_mut(k).expect("path node exists");
            n.visits += 1;
            n.n[*a] += 1;
            n.wins[*a] += u64::from(win);
        }
    }

    /// Order-independent hash of the frozen tree.
    pub(crate) fn hash(&self) -> String {
        let mut keys: Vec<&Key> = self.nodes.keys().collect();
        keys.sort_unstable();
        let mut h = Sha256::new();
        for k in keys {
            let n = &self.nodes[k];
            h.update(k);
            h.update(n.visits.to_le_bytes());
            for (a, b) in n.n.iter().zip(&n.wins) {
                h.update(a.to_le_bytes());
                h.update(b.to_le_bytes());
            }
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Live candidate index for an edge's canonical bytes.
pub(crate) fn live_index(c: &Canon, edge: &[u8]) -> Result<u32, PlayErr> {
    let mut hits = c
        .menu
        .iter()
        .enumerate()
        .filter(|(_, m)| m.as_slice() == edge);
    match (hits.next(), hits.next()) {
        (Some((i, _)), None) => Ok(i as u32),
        _ => Err(PlayErr::Fault(
            "edge does not bind to exactly one live candidate".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(k: usize) -> Node {
        let seeds = RootSeeds {
            model: "r1".into(),
            root: "t".into(),
        };
        let menu: Vec<Vec<u8>> = (0..k).map(|i| vec![i as u8]).collect();
        let canon = Canon {
            obs: vec![],
            edges: menu.clone(),
            menu,
        };
        Node::new(&[1; 32], [0; 32], vec![], canon, 1, &seeds)
    }

    #[test]
    fn untried_first_then_ucb_with_repeat_exploration() {
        let mut n = node(3);
        // Untried actions in the seeded order.
        let order = n.perm.clone();
        for &a in &order {
            assert_eq!(n.select(), a);
            n.visits += 1;
            n.n[a] += 1;
        }
        // Action that lost early keeps being revisited (persistence).
        let first = order[0];
        n.wins[order[1]] = 1;
        n.wins[order[2]] = 1;
        let mut revisits = 0;
        for _ in 0..200 {
            let a = n.select();
            n.visits += 1;
            n.n[a] += 1;
            if a == first {
                revisits += 1;
                n.wins[a] += 1; // it starts winning
            } else {
                n.wins[a] += u64::from(n.n[a].is_multiple_of(2));
            }
        }
        assert!(revisits > 50, "{revisits}");
        assert_eq!(n.frozen_choice(), Some(first));
    }

    fn visit(n: &mut Node, a: usize, win: bool) {
        n.visits += 1;
        n.n[a] += 1;
        n.wins[a] += u64::from(win);
    }

    #[test]
    fn finite_urgency_repeats_a_win_explores_after_losses_and_breaks_ties_by_seed() {
        let u = Some(1.5);
        // A first win is repeated on the next visit.
        let mut n = node(5);
        let first = n.select_with(u);
        assert_eq!(first, n.perm[0], "untried actions tie at 1.5; seeded order");
        visit(&mut n, first, true);
        assert_eq!(n.select_with(u), first);
        // A lucky win followed by a loss gives way to untried actions.
        visit(&mut n, first, false);
        assert_ne!(n.select_with(u), first);
        // A loss alone gives way to untried actions.
        let mut m = node(4);
        let a = m.select_with(u);
        visit(&mut m, a, false);
        assert_ne!(m.select_with(u), a);
        // Two 1/1 actions tie; the seeded order decides.
        let mut t = node(3);
        let (x, y) = (t.perm[0], t.perm[1]);
        visit(&mut t, x, true);
        visit(&mut t, y, true);
        assert_eq!(t.select_with(u), x);
        // An always-winning action still lets every action be tried.
        let mut e = node(6);
        let best = e.perm[0];
        for _ in 0..400 {
            let a = e.select_with(u);
            visit(&mut e, a, a == best);
        }
        assert!(e.n.iter().all(|&c| c > 0), "{:?}", e.n);
        // Frozen eligibility is unchanged by the selection rule.
        assert_eq!(e.frozen_choice(), Some(best));
        // The default rule is untouched.
        let mut d = node(3);
        let a = d.select();
        visit(&mut d, a, true);
        assert_ne!(d.select(), a, "baseline tries every action first");
    }

    #[test]
    fn frozen_choice_needs_eight_backups() {
        let mut n = node(2);
        n.n = vec![7, 8];
        n.wins = vec![7, 0];
        assert_eq!(n.frozen_choice(), Some(1));
        n.n = vec![7, 7];
        assert_eq!(n.frozen_choice(), None);
    }

    #[test]
    fn canonical_ids_are_first_appearance_and_zcc_relative() {
        let mut v = serde_json::json!({"a":{"arena_id":90,"zone_change_count":7},
            "b":[{"arena_id":12,"zone_change_count":3},{"arena_id":90,"zone_change_count":9}]});
        let mut h = Handles::default();
        collect_ids(&v, &mut h);
        rewrite(&mut v, &h);
        assert_eq!(v["a"]["arena_id"], 0);
        assert_eq!(v["a"]["zone_change_count"], 0);
        assert_eq!(v["b"][0]["arena_id"], 1);
        assert_eq!(v["b"][0]["zone_change_count"], 0);
        assert_eq!(v["b"][1]["arena_id"], 0);
        assert_eq!(v["b"][1]["zone_change_count"], 2);
        let mut w = serde_json::json!({"c":{"arena_id":5,"attachments":[9,5]},
            "e":[{"timestamp":700},{"timestamp":40},{"timestamp":700}],
            "p":{"object":{"arena_id":9},"zone_change_generation":31}});
        let mut h = Handles::default();
        collect_ids(&w, &mut h);
        rewrite(&mut w, &h);
        assert_eq!(w["c"]["attachments"], serde_json::json!([1, 0]));
        assert_eq!(
            w["e"],
            serde_json::json!([{"timestamp":1},{"timestamp":0},{"timestamp":1}])
        );
        assert_eq!(w["p"], serde_json::json!({"object":{"arena_id":1}}));
    }

    #[test]
    fn only_unordered_lists_are_sorted() {
        let mut v = serde_json::json!({
            "player_status":[{"has_lost":false,"x":2},{"has_lost":false,"x":1}],
            "graveyards":[[{"n":"b"},{"n":"a"}],[]],
            "battlefield":[[{"n":"b"},{"n":"a"}],[{"n":"d"},{"n":"c"}]],
            "own_hand":[{"n":"z"},{"n":"y"}],
            "stack":[{"n":"q"},{"n":"p"}]});
        sort_object_lists(&mut v);
        assert_eq!(v["player_status"][0]["x"], 2);
        assert_eq!(v["graveyards"][0][0]["n"], "a");
        assert_eq!(v["battlefield"][0][0]["n"], "a");
        assert_eq!(v["battlefield"][1][0]["n"], "c");
        assert_eq!(v["own_hand"][0]["n"], "y");
        assert_eq!(v["stack"][0]["n"], "q");
    }
}
