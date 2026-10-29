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
    /// Each handle's position in `order`.
    position: HashMap<u64, u64>,
    /// Distinct continuous-effect `timestamp` values, sorted before `rewrite`.
    stamps: Vec<u64>,
}

impl Handles {
    fn see(&mut self, id: u64) {
        let next = self.order.len() as u64;
        if let std::collections::hash_map::Entry::Vacant(e) = self.position.entry(id) {
            e.insert(next);
            self.order.push(id);
        }
    }

    fn index(&self, id: u64) -> u64 {
        *self.position.get(&id).expect("collected")
    }

    /// Sorts and dedups `stamps`, so a timestamp's rank is a binary search.
    fn finish(&mut self) {
        self.stamps.sort_unstable();
        self.stamps.dedup();
    }

    /// The number of distinct shown timestamps below `t`.
    fn stamp_rank(&self, t: u64) -> u64 {
        debug_assert!(
            self.stamps.windows(2).all(|w| w[0] < w[1]),
            "finish() first"
        );
        self.stamps.partition_point(|&s| s < t) as u64
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
                h.stamps.push(t);
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
                m.insert("timestamp".into(), Value::from(h.stamp_rank(t)));
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
pub(crate) fn masked(v: &Value) -> String {
    /// Serializes as `v` with every `arena_id`, `zone_change_count` and
    /// `attachments` value replaced by null, without cloning `v`: the same
    /// bytes as nulling those fields in a copy and calling `to_string`.
    struct Masked<'a>(&'a Value);
    impl serde::Serialize for Masked<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::{SerializeMap, SerializeSeq};
            match self.0 {
                Value::Object(m) => {
                    let mut map = s.serialize_map(Some(m.len()))?;
                    for (k, x) in m {
                        if matches!(k.as_str(), "arena_id" | "zone_change_count" | "attachments") {
                            map.serialize_entry(k, &Value::Null)?;
                        } else {
                            map.serialize_entry(k, &Masked(x))?;
                        }
                    }
                    map.end()
                }
                Value::Array(a) => {
                    let mut seq = s.serialize_seq(Some(a.len()))?;
                    for x in a {
                        seq.serialize_element(&Masked(x))?;
                    }
                    seq.end()
                }
                other => other.serialize(s),
            }
        }
    }
    serde_json::to_string(&Masked(v)).expect("a JSON value serializes")
}

/// Observation lists whose order carries no game information in this card
/// pool (hands, battlefield, graveyards, exile, known hand cards, effects,
/// relations, permissions, historical sources, a library search's offered
/// cards) but can follow engine handles or hidden draw order (a mill reveals
/// the library in its sampled order). Every other list keeps engine order:
/// stack, targets, combat orders, pending triggers. Never applied to the menu.
pub(crate) const UNORDERED_LISTS: [&str; 10] = [
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

/// Raw handle → tie rank, consulted only between entries equal under the
/// content mask (repaired key mode; `None` is the legacy rule: input order).
pub(crate) type TieRanks = HashMap<u64, u32>;

/// The handle that names a list entry: the shallowest `arena_id` in it.
pub(crate) fn own_handle(v: &Value) -> Option<u64> {
    let mut level = vec![v];
    while !level.is_empty() {
        let mut next = Vec::new();
        for x in level {
            match x {
                Value::Object(m) => {
                    if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                        return Some(id);
                    }
                    next.extend(m.values());
                }
                Value::Array(a) => next.extend(a.iter()),
                _ => {}
            }
        }
        level = next;
    }
    None
}

fn sort_by_content(a: &mut Vec<Value>, ranks: Option<&TieRanks>) {
    if a.len() > 1 && a.iter().all(Value::is_object) {
        let rank = |x: &Value| {
            ranks
                .and_then(|r| own_handle(x).and_then(|h| r.get(&h).copied()))
                .unwrap_or(u32::MAX)
        };
        let mut keyed: Vec<((String, u32), Value)> =
            a.drain(..).map(|x| ((masked(&x), rank(&x)), x)).collect();
        keyed.sort_by(|x, y| x.0.cmp(&y.0));
        a.extend(keyed.into_iter().map(|(_, x)| x));
    }
}

/// Calls `f` on every declared-unordered list (each seat's list of a
/// per-seat pair separately), innermost first.
pub(crate) fn for_unordered_lists(v: &mut Value, f: &mut impl FnMut(&mut Vec<Value>)) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                for_unordered_lists(x, f);
                if UNORDERED_LISTS.contains(&k.as_str()) {
                    if let Value::Array(a) = x {
                        // Per-seat pairs ([[..], [..]]) sort within each seat.
                        if a.iter().all(Value::is_array) {
                            for inner in a.iter_mut() {
                                if let Value::Array(i) = inner {
                                    f(i);
                                }
                            }
                        } else {
                            f(a);
                        }
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| for_unordered_lists(x, f)),
        _ => {}
    }
}

#[cfg(test)]
fn sort_object_lists(v: &mut Value) {
    sort_object_lists_with(v, None);
}

fn sort_object_lists_with(v: &mut Value, ranks: Option<&TieRanks>) {
    for_unordered_lists(v, &mut |a| sort_by_content(a, ranks));
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
                    let rank = all.partition_point(|&s| s < t) as u64;
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
    let (obs, sem) = raw_decision(s, d)?;
    Ok(canon_values(obs, sem)?.0)
}

/// The acting player's raw V4 observation and frozen-source menu, as JSON
/// (before any canonical rewriting).
pub(crate) fn raw_decision(
    s: &FastActorSessionV1,
    d: FastActorDecisionV1,
) -> Result<(Value, Vec<Value>), PlayErr> {
    let (obs, sem) = s
        .actor_visible_decision_v4(d)
        .map_err(|e| PlayErr::Fault(format!("actor-visible decision: {e:?}")))?;
    let obs = serde_json::to_value(&obs).map_err(|e| PlayErr::Fault(e.to_string()))?;
    let sem: Vec<Value> = sem
        .iter()
        .map(|x| serde_json::to_value(x).map_err(|e| PlayErr::Fault(e.to_string())))
        .collect::<Result<_, _>>()?;
    Ok((obs, sem))
}

/// Raw-to-canonical handle mapping of one canonicalization (diagnostic):
/// raw `arena_id` values in first-appearance order (canonical index = list
/// position) and the smallest `zone_change_count` shown per raw handle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Mapping {
    pub(crate) order: Vec<u64>,
    pub(crate) min_zcc: Vec<(u64, u64)>,
}

/// The observation and menu with step bookkeeping removed and absolute
/// counters ranked: the input to the content sort.
pub(crate) fn prepare(mut obs: Value, mut sem: Vec<Value>) -> (Value, Vec<Value>) {
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
    rank_counters(&mut obs);
    for x in &mut sem {
        rank_counters(x);
    }
    (obs, sem)
}

/// `canon` on a raw observation and menu, with its handle mapping.
pub(crate) fn canon_values(obs: Value, sem: Vec<Value>) -> Result<(Canon, Mapping), PlayErr> {
    let (obs, sem) = prepare(obs, sem);
    canon_prepared(obs, sem, None)
}

/// `canon_values` on prepared input, with optional tie ranks.
pub(crate) fn canon_prepared(
    mut obs: Value,
    mut sem: Vec<Value>,
    ranks: Option<&TieRanks>,
) -> Result<(Canon, Mapping), PlayErr> {
    sort_object_lists_with(&mut obs, ranks);
    let mut handles = Handles::default();
    collect_ids(&obs, &mut handles);
    // Handles named only by the menu are indexed in content order, not in
    // live candidate order (which can follow handles).
    let mut by_content: Vec<&Value> = sem.iter().collect();
    by_content.sort_by_cached_key(|x| masked(x));
    for x in by_content {
        collect_ids(x, &mut handles);
    }
    handles.finish();
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
    let mut min_zcc: Vec<(u64, u64)> = handles.min_zcc.into_iter().collect();
    min_zcc.sort_unstable();
    let mapping = Mapping {
        order: handles.order,
        min_zcc,
    };
    Ok((Canon { obs, menu, edges }, mapping))
}

/// `canon_prepared` for one prepared frame under any number of tie-rank
/// choices, without copying or rewriting the view. What does not depend on
/// tie ranks is worked out once: which lists the content sort orders, and
/// each entry's content key. (An entry's key does not depend on tie ranks:
/// ranks only order entries whose keys are equal, so an enclosing entry's
/// masked bytes are the same under every choice.) Each `canon` then orders
/// those lists for its ranks, collects handles in that order, and writes the
/// relabeled bytes directly, exactly as `canon_prepared` would produce them
/// from a copy (`S4A_CANON_CHECK=1` compares the two on every call).
pub(crate) struct CanonPlan<'a> {
    obs: &'a Value,
    menu: &'a [Value],
    /// Each sorted list's address, mapped to its index in `lists`.
    ids: HashMap<usize, usize>,
    /// Sorted lists, innermost first (the order `for_unordered_lists` sorts
    /// them in).
    lists: Vec<ListPlan<'a>>,
    /// Menu entries in content order (the order their handles are indexed).
    menu_order: Vec<usize>,
}

/// One list `sort_by_content` orders (two or more entries, all objects).
struct ListPlan<'a> {
    list: &'a [Value],
    /// Entry indices in content-key order, equal keys in input order.
    base: Vec<usize>,
    /// Ranges of `base` with two or more equal keys, which tie ranks order.
    runs: Vec<std::ops::Range<usize>>,
}

/// Entry order of every sorted list under one tie-rank choice (by list).
type ListOrders<'p> = Vec<std::borrow::Cow<'p, [usize]>>;

impl<'a> CanonPlan<'a> {
    pub(crate) fn new(obs: &'a Value, menu: &'a [Value]) -> Self {
        let mut plan = Self {
            obs,
            menu,
            ids: HashMap::new(),
            lists: Vec::new(),
            menu_order: Vec::new(),
        };
        plan.plan_lists(obs);
        let keys: Vec<String> = menu.iter().map(masked).collect();
        let mut order: Vec<usize> = (0..menu.len()).collect();
        order.sort_by(|&x, &y| keys[x].cmp(&keys[y]));
        plan.menu_order = order;
        plan
    }

    /// `for_unordered_lists` without mutation: plans every list it would
    /// sort, innermost first.
    fn plan_lists(&mut self, v: &'a Value) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    self.plan_lists(x);
                    if UNORDERED_LISTS.contains(&k.as_str()) {
                        if let Value::Array(a) = x {
                            if a.iter().all(Value::is_array) {
                                for inner in a {
                                    if let Value::Array(i) = inner {
                                        self.plan_list(i);
                                    }
                                }
                            } else {
                                self.plan_list(a);
                            }
                        }
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| self.plan_lists(x)),
            _ => {}
        }
    }

    /// `sort_by_content`'s condition and keys, for one list.
    fn plan_list(&mut self, a: &'a [Value]) {
        if a.len() > 1 && a.iter().all(Value::is_object) {
            let keys: Vec<String> = a.iter().map(|x| self.masked(x)).collect();
            let mut base: Vec<usize> = (0..a.len()).collect();
            // Stable: equal keys keep input order, as in `sort_by_content`.
            base.sort_by(|&x, &y| keys[x].cmp(&keys[y]));
            let mut runs = Vec::new();
            let mut start = 0;
            for i in 1..=base.len() {
                if i == base.len() || keys[base[i]] != keys[base[start]] {
                    if i - start > 1 {
                        runs.push(start..i);
                    }
                    start = i;
                }
            }
            self.ids.insert(a.as_ptr() as usize, self.lists.len());
            self.lists.push(ListPlan {
                list: a,
                base,
                runs,
            });
        }
    }

    /// `masked(x)` as `canon_prepared` computes it, after the lists inside
    /// `x` are sorted: planned lists are written in key order.
    fn masked(&self, x: &Value) -> String {
        struct Masked<'p, 'a>(&'p CanonPlan<'a>, &'p Value);
        impl serde::Serialize for Masked<'_, '_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::{SerializeMap, SerializeSeq};
                match self.1 {
                    Value::Object(m) => {
                        let mut map = s.serialize_map(Some(m.len()))?;
                        for (k, x) in m {
                            if matches!(
                                k.as_str(),
                                "arena_id" | "zone_change_count" | "attachments"
                            ) {
                                map.serialize_entry(k, &Value::Null)?;
                            } else {
                                map.serialize_entry(k, &Masked(self.0, x))?;
                            }
                        }
                        map.end()
                    }
                    Value::Array(a) => {
                        let mut seq = s.serialize_seq(Some(a.len()))?;
                        match self.0.list(a) {
                            Some(l) => {
                                for &i in &self.0.lists[l].base {
                                    seq.serialize_element(&Masked(self.0, &a[i]))?;
                                }
                            }
                            None => {
                                for x in a {
                                    seq.serialize_element(&Masked(self.0, x))?;
                                }
                            }
                        }
                        seq.end()
                    }
                    other => other.serialize(s),
                }
            }
        }
        serde_json::to_string(&Masked(self, x)).expect("a JSON value serializes")
    }

    /// The plan index of a sorted list.
    fn list(&self, a: &[Value]) -> Option<usize> {
        if a.len() < 2 {
            return None;
        }
        self.ids.get(&(a.as_ptr() as usize)).copied()
    }

    /// Each sorted list's order under `ranks`: within a run of equal keys,
    /// by the rank of each entry's own handle (unranked last), stably.
    fn orders(&self, ranks: Option<&TieRanks>) -> ListOrders<'_> {
        let mut orders: ListOrders<'_> = Vec::with_capacity(self.lists.len());
        for l in &self.lists {
            let Some(ranks) = ranks.filter(|_| !l.runs.is_empty()) else {
                orders.push(std::borrow::Cow::Borrowed(&l.base));
                continue;
            };
            let mut order = l.base.clone();
            for run in &l.runs {
                // Inner lists come earlier in `lists`, so their orders are
                // known: `own_handle` reads the entry as sorted.
                let mut keyed: Vec<(u32, usize)> = order[run.clone()]
                    .iter()
                    .map(|&i| {
                        let r = self
                            .own_handle(&l.list[i], &orders)
                            .and_then(|h| ranks.get(&h).copied())
                            .unwrap_or(u32::MAX);
                        (r, i)
                    })
                    .collect();
                keyed.sort_by_key(|&(r, _)| r);
                for (slot, (_, i)) in order[run.clone()].iter_mut().zip(keyed) {
                    *slot = i;
                }
            }
            orders.push(std::borrow::Cow::Owned(order));
        }
        orders
    }

    /// `own_handle` on an entry whose inner lists are in `orders`.
    fn own_handle<'v>(&self, v: &'v Value, orders: &ListOrders<'_>) -> Option<u64> {
        let mut level = vec![v];
        while !level.is_empty() {
            let mut next: Vec<&'v Value> = Vec::new();
            for x in level {
                match x {
                    Value::Object(m) => {
                        if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                            return Some(id);
                        }
                        next.extend(m.values());
                    }
                    Value::Array(a) => match self.list(a) {
                        Some(l) => next.extend(orders[l].iter().map(|&i| &a[i])),
                        None => next.extend(a.iter()),
                    },
                    _ => {}
                }
            }
            level = next;
        }
        None
    }

    /// `collect_ids` on `v` with its sorted lists in `orders`.
    fn collect_ids(&self, v: &Value, h: &mut Handles, orders: &ListOrders<'_>) {
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
                    h.stamps.push(t);
                }
                for x in m.values() {
                    self.collect_ids(x, h, orders);
                }
            }
            Value::Array(a) => match self.list(a) {
                Some(l) => orders[l]
                    .iter()
                    .for_each(|&i| self.collect_ids(&a[i], h, orders)),
                None => a.iter().for_each(|x| self.collect_ids(x, h, orders)),
            },
            _ => {}
        }
    }

    /// The bytes `serde_json::to_vec` writes for `v` after `rewrite`
    /// (`relabel`) and with sorted lists in `orders`. `rewrite` does not
    /// descend into `attachments`, so their contents are written as they
    /// are (`relabel` false), lists still sorted.
    fn write(
        &self,
        v: &Value,
        relabel: bool,
        h: &Handles,
        orders: &ListOrders<'_>,
        out: &mut Vec<u8>,
    ) {
        let leaf = |x: &Value, out: &mut Vec<u8>| {
            serde_json::to_writer(&mut *out, x).expect("a JSON value serializes");
        };
        let number = |n: u64, out: &mut Vec<u8>| leaf(&Value::from(n), out);
        match v {
            Value::Object(m) => {
                out.push(b'{');
                let id = if relabel {
                    m.get("arena_id").and_then(Value::as_u64)
                } else {
                    None
                };
                let mut first = true;
                for (k, x) in m {
                    if relabel && k == "zone_change_generation" {
                        continue;
                    }
                    if !first {
                        out.push(b',');
                    }
                    first = false;
                    serde_json::to_writer(&mut *out, k).expect("a JSON key serializes");
                    out.push(b':');
                    match (relabel, k.as_str()) {
                        (true, "arena_id") if id.is_some() => {
                            number(h.index(id.expect("checked")), out);
                        }
                        (true, "zone_change_count") if id.is_some() && x.as_u64().is_some() => {
                            let z = x.as_u64().expect("checked");
                            let base = h.min_zcc.get(&id.expect("checked")).copied().unwrap_or(z);
                            number(z - base, out);
                        }
                        (true, "attachments") => match x {
                            Value::Array(a) => {
                                out.push(b'[');
                                for (i, e) in a.iter().enumerate() {
                                    if i > 0 {
                                        out.push(b',');
                                    }
                                    match e.as_u64() {
                                        Some(id) => number(h.index(id), out),
                                        None => self.write(e, false, h, orders, out),
                                    }
                                }
                                out.push(b']');
                            }
                            other => self.write(other, false, h, orders, out),
                        },
                        (true, "timestamp") if x.as_u64().is_some() => {
                            number(h.stamp_rank(x.as_u64().expect("checked")), out);
                        }
                        _ => self.write(x, relabel, h, orders, out),
                    }
                }
                out.push(b'}');
            }
            Value::Array(a) => {
                out.push(b'[');
                let item = |n: usize, x: &Value, out: &mut Vec<u8>| {
                    if n > 0 {
                        out.push(b',');
                    }
                    self.write(x, relabel, h, orders, out);
                };
                match self.list(a) {
                    Some(l) => {
                        for (n, &i) in orders[l].iter().enumerate() {
                            item(n, &a[i], out);
                        }
                    }
                    None => {
                        for (n, x) in a.iter().enumerate() {
                            item(n, x, out);
                        }
                    }
                }
                out.push(b']');
            }
            other => leaf(other, out),
        }
    }

    /// `canon_prepared(obs.clone(), menu.to_vec(), ranks).0`.
    pub(crate) fn canon(&self, ranks: Option<&TieRanks>) -> Result<Canon, PlayErr> {
        let orders = self.orders(ranks);
        let mut handles = Handles::default();
        self.collect_ids(self.obs, &mut handles, &orders);
        // Menu lists are never sorted (no menu list is planned).
        for &i in &self.menu_order {
            self.collect_ids(&self.menu[i], &mut handles, &orders);
        }
        handles.finish();
        let mut obs = Vec::new();
        self.write(self.obs, true, &handles, &orders, &mut obs);
        let mut raw: Vec<Vec<u8>> = Vec::with_capacity(self.menu.len());
        let mut menu: Vec<Vec<u8>> = Vec::with_capacity(self.menu.len());
        for x in self.menu {
            let mut b = Vec::new();
            self.write(x, true, &handles, &orders, &mut b);
            let dup = raw.iter().filter(|r| **r == b).count() as u64;
            raw.push(b.clone());
            let mut m = b;
            if dup > 0 {
                m.push(b'#');
                m.extend_from_slice(&dup.to_le_bytes());
            }
            menu.push(m);
        }
        let mut edges = menu.clone();
        edges.sort();
        let c = Canon { obs, menu, edges };
        if canon_check() {
            let (reference, _) = canon_prepared(self.obs.clone(), self.menu.to_vec(), ranks)?;
            assert_eq!(c, reference, "CanonPlan differs from canon_prepared");
        }
        Ok(c)
    }
}

/// Whether `S4A_CANON_CHECK=1` asks `CanonPlan::canon` to compare every
/// result with `canon_prepared`.
fn canon_check() -> bool {
    static CHECK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CHECK.get_or_init(|| std::env::var("S4A_CANON_CHECK").is_ok_and(|v| v == "1"))
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
    /// Natural draws among `n` (EX1's Q-hat; counted as non-wins in `wins`
    /// and never read by selection).
    pub(crate) draws: Vec<u64>,
    pub(crate) depth: u32,
    /// Edges merged into an interchangeable sibling (`interchange.rs`), never
    /// selected or executed; empty when merging is off.
    pub(crate) skip: Vec<bool>,
    /// Best-line values (`BestLine`); None under the formal mean backup.
    pub(crate) bl: Option<Box<BestLine>>,
}

/// Opt-in best-line backup (collab LANES/bestline-step0-20261013; proposal
/// "score our own choices by their best line"): an edge's value averages what
/// followed it in the world, our own later choices count by their best edge.
///
/// - An edge's value is the try-weighted mean of the values of the stored
///   nodes it led to and of the results no rollout-policy choice of ours
///   touched after the tree path. Results the rollout policy played after the
///   tree path count only while the edge has fewer than `m` such outcomes.
/// - A node's value is the greatest value among its edges with at least `m`
///   tries (merged edges excluded), or its mean result when none has `m`.
///
/// Selection and frozen execution use an edge's value where the formal rule
/// uses its mean; the UCB bonus, untried rule and eight-backup execution
/// floor are unchanged. Counts, keys and the tree hash are the formal ones.
#[derive(Clone, Debug)]
pub(crate) struct BestLine {
    pub(crate) m: u64,
    /// Value per edge (meaningful once the edge has a try).
    pub(crate) q: Vec<f64>,
    /// The node's value.
    pub(crate) v: f64,
    /// Per edge: the sum of visits times value over the stored nodes it led
    /// to (kept incrementally: an attempt changes only its path's nodes), and
    /// the sum of their visits.
    kid_sum: Vec<f64>,
    kid_visits: Vec<u64>,
    /// Per edge: (tries, wins) ending with no rollout-policy choice of ours
    /// after the tree path, and ending after one.
    clean: Vec<(u64, u64)>,
    rollout: Vec<(u64, u64)>,
}

impl BestLine {
    pub(crate) fn new(k: usize, m: u64) -> Self {
        Self {
            m,
            q: vec![0.0; k],
            v: 0.0,
            kid_sum: vec![0.0; k],
            kid_visits: vec![0; k],
            clean: vec![(0, 0); k],
            rollout: vec![(0, 0); k],
        }
    }

    /// Edge `a`'s value from its children's sums and its direct results.
    fn edge_value(&self, a: usize, kid_sum: f64, kid_visits: u64) -> f64 {
        let (ct, cw) = self.clean[a];
        let (sum, cnt) = (kid_sum + cw as f64, kid_visits + ct);
        if cnt >= self.m {
            sum / cnt as f64
        } else {
            let (rt, rw) = self.rollout[a];
            (sum + rw as f64) / (cnt + rt) as f64
        }
    }
}

/// An edge's direct results: (tries, wins) clean and after a rollout choice.
#[cfg(test)]
pub(crate) type DirectResults = ((u64, u64), (u64, u64));

pub(crate) fn permutation(len: usize, seed: u64) -> Vec<usize> {
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
        let perm = permutation(canon.edges.len(), seeds.get(Purpose::Ties, "E", 0, key));
        Self::with_perm(parent, edge, canon, depth, perm)
    }

    /// A node with an explicit seeded tie order over its edges.
    pub(crate) fn with_perm(
        parent: Key,
        edge: Vec<u8>,
        canon: Canon,
        depth: u32,
        perm: Vec<usize>,
    ) -> Self {
        let k = canon.edges.len();
        debug_assert_eq!(perm.len(), k);
        Self {
            parent,
            edge,
            perm,
            canon,
            visits: 0,
            n: vec![0; k],
            wins: vec![0; k],
            draws: vec![0; k],
            depth,
            skip: Vec::new(),
            bl: None,
        }
    }

    /// Edge `a`'s direct results (tries, wins) with no rollout-policy choice
    /// of ours after the tree path, and after one; None for formal nodes.
    #[cfg(test)]
    pub(crate) fn best_line_direct(&self, a: usize) -> Option<DirectResults> {
        self.bl.as_deref().map(|b| (b.clean[a], b.rollout[a]))
    }

    /// An edge's value: its best-line value when that backup is on, else its
    /// mean (exactly the formal arithmetic).
    fn value(&self, a: usize) -> f64 {
        match &self.bl {
            Some(b) => b.q[a],
            None => self.wins[a] as f64 / self.n[a] as f64,
        }
    }

    fn skipped(&self, a: usize) -> bool {
        self.skip.get(a).copied().unwrap_or(false)
    }

    /// Selection rule: the first untried action in the seeded order; once
    /// all are tried, maximum UCB, ties to the seeded order.
    #[cfg(test)]
    pub(crate) fn select(&self) -> usize {
        self.select_with(None)
    }

    /// `select`, or with `Some(u)` the finite-urgency candidate (collab
    /// LANES/spy-history-support-plan-20261010/FPU-CANDIDATE.md): an action
    /// with no natural terminal backup scores `u`, every other action its
    /// UCB score, and the maximum wins, ties to the seeded order.
    pub(crate) fn select_with(&self, urgency: Option<f64>) -> usize {
        self.select_receipt(urgency, None)
    }

    #[cfg(feature = "viewer-recording-v1")]
    pub(crate) fn select_with_scores(
        &self,
        urgency: Option<f64>,
        scores: &mut Vec<Option<f64>>,
    ) -> usize {
        scores.clear();
        scores.resize(self.n.len(), None);
        self.select_receipt(urgency, Some(scores))
    }

    fn select_receipt(&self, urgency: Option<f64>, scores: Option<&mut Vec<Option<f64>>>) -> usize {
        self.select_evidence_receipt(urgency, self.visits, &self.n, &self.wins, true, scores)
    }

    /// UCT guidance from separately accounted natural evidence. This only
    /// borrows the incoming node's legal edges, tie order and skip mask; it
    /// cannot change its stored counts or its execution support.
    pub(crate) fn select_evidence(
        &self,
        urgency: Option<f64>,
        visits: u64,
        n: &[u64],
        wins: &[u64],
    ) -> usize {
        self.select_evidence_receipt(urgency, visits, n, wins, false, None)
    }

    fn select_evidence_receipt(
        &self,
        urgency: Option<f64>,
        visits: u64,
        counts: &[u64],
        wins: &[u64],
        own: bool,
        mut scores: Option<&mut Vec<Option<f64>>>,
    ) -> usize {
        debug_assert_eq!(counts.len(), self.n.len());
        debug_assert_eq!(wins.len(), self.wins.len());
        // An edge's mean, or its best-line value when that backup is on and
        // the counts are the node's own (borrowed evidence has no values).
        let best_line = self.bl.as_deref().filter(|_| own);
        let mean = |a: usize, n: f64| match best_line {
            Some(b) => b.q[a],
            None => wins[a] as f64 / n,
        };
        if let Some(u) = urgency {
            let ln = (visits.max(2) as f64).ln();
            let mut best = self.perm[0];
            let mut best_v = f64::NEG_INFINITY;
            for &a in self.perm.iter().filter(|&&a| !self.skipped(a)) {
                let v = if counts[a] == 0 {
                    u
                } else {
                    let n = counts[a] as f64;
                    mean(a, n) + (UCB_C2 * ln / n).sqrt()
                };
                if let Some(out) = scores.as_deref_mut() {
                    out[a] = Some(v);
                }
                if v > best_v {
                    best_v = v;
                    best = a;
                }
            }
            return best;
        }
        if let Some(&a) = self
            .perm
            .iter()
            .find(|&&a| counts[a] == 0 && !self.skipped(a))
        {
            return a;
        }
        let ln = (visits.max(2) as f64).ln();
        let mut best = self.perm[0];
        let mut best_v = f64::NEG_INFINITY;
        for &a in self.perm.iter().filter(|&&a| !self.skipped(a)) {
            let n = counts[a] as f64;
            let v = mean(a, n) + (UCB_C2 * ln / n).sqrt();
            if let Some(out) = scores.as_deref_mut() {
                out[a] = Some(v);
            }
            if v > best_v {
                best_v = v;
                best = a;
            }
        }
        best
    }

    /// Frozen execution: greatest mean (best-line value when that backup is
    /// on) among edges with at least eight natural terminal backups, ties to
    /// the seeded order.
    pub(crate) fn frozen_choice(&self) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for &a in &self.perm {
            if self.n[a] < MIN_EXEC_BACKUPS || self.skipped(a) {
                continue;
            }
            let m = self.value(a);
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
    /// path, inserting its new nodes (one under B1; with depth retention, the
    /// first expansion and its retained tail).
    /// `world_only` says no rollout-policy choice of ours followed the tree
    /// path (read only by best-line nodes).
    pub(crate) fn backup_observed(
        &mut self,
        path: &[(Key, usize)],
        new: Vec<(Key, Node)>,
        win: bool,
        draw: bool,
        world_only: bool,
    ) {
        for (k, n) in new {
            self.nodes.insert(k, n);
        }
        for (k, a) in path {
            let n = self.nodes.get_mut(k).expect("path node exists");
            n.visits += 1;
            n.n[*a] += 1;
            n.wins[*a] += u64::from(win);
            n.draws[*a] += u64::from(draw);
        }
        self.best_line_backup(path, win, world_only);
    }

    /// Best-line values along a backed-up path, bottom up: only the path's
    /// nodes change, and each passes its change of visits times value to its
    /// parent's edge. Nothing for formal nodes.
    fn best_line_backup(&mut self, path: &[(Key, usize)], win: bool, world_only: bool) {
        // A search's nodes are all best-line or all formal: one lookup keeps
        // the formal backup's cost.
        let on = path
            .first()
            .and_then(|(k, _)| self.nodes.get(k))
            .is_some_and(|n| n.bl.is_some());
        if !on {
            return;
        }
        // The child's contribution to its parent's edge before and after.
        let mut carry: Option<(f64, f64)> = None;
        for (j, (k, a)) in path.iter().enumerate().rev() {
            let Some(Node {
                visits,
                n: tries,
                wins,
                skip,
                bl: Some(b),
                ..
            }) = self.nodes.get_mut(k)
            else {
                carry = None;
                continue;
            };
            if j + 1 == path.len() {
                let x = if world_only {
                    &mut b.clean[*a]
                } else {
                    &mut b.rollout[*a]
                };
                x.0 += 1;
                x.1 += u64::from(win);
            } else if let Some((before, after)) = carry {
                b.kid_sum[*a] += after - before;
                b.kid_visits[*a] += 1;
            }
            b.q[*a] = b.edge_value(*a, b.kid_sum[*a], b.kid_visits[*a]);
            let old = b.v;
            let best = (0..tries.len())
                .filter(|&e| tries[e] >= b.m && !skip.get(e).copied().unwrap_or(false))
                .map(|e| b.q[e])
                .fold(None, |x: Option<f64>, q| Some(x.map_or(q, |x| x.max(q))));
            b.v = best.unwrap_or_else(|| wins.iter().sum::<u64>() as f64 / *visits as f64);
            carry = Some(((*visits - 1) as f64 * old, *visits as f64 * b.v));
        }
    }

    /// Test check of the incremental best-line values: every best-line node's
    /// values recomputed from scratch, bottom up over parent links, equal the
    /// stored ones.
    #[cfg(test)]
    pub(crate) fn best_line_mismatches(&self) -> Vec<String> {
        let mut kids: HashMap<(Key, usize), Vec<Key>> = HashMap::new();
        for (k, n) in &self.nodes {
            if let Some(p) = self.nodes.get(&n.parent) {
                let e = p.canon.edges.iter().position(|x| *x == n.edge);
                kids.entry((n.parent, e.expect("edge in parent")))
                    .or_default()
                    .push(*k);
            }
        }
        fn value(
            t: &Tree,
            kids: &HashMap<(Key, usize), Vec<Key>>,
            k: &Key,
            memo: &mut HashMap<Key, (Vec<f64>, f64)>,
        ) -> f64 {
            if let Some((_, v)) = memo.get(k) {
                return *v;
            }
            let n = &t.nodes[k];
            let b = n.bl.as_deref().expect("best-line node");
            let mut q = vec![0.0; n.n.len()];
            for (a, x) in q.iter_mut().enumerate() {
                if n.n[a] == 0 {
                    continue;
                }
                let (mut sum, mut visits) = (0.0, 0u64);
                for c in kids.get(&(*k, a)).into_iter().flatten() {
                    sum += t.nodes[c].visits as f64 * value(t, kids, c, memo);
                    visits += t.nodes[c].visits;
                }
                *x = b.edge_value(a, sum, visits);
            }
            let best = (0..q.len())
                .filter(|&e| n.n[e] >= b.m && !n.skipped(e))
                .map(|e| q[e])
                .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
            let v = best.unwrap_or(n.wins.iter().sum::<u64>() as f64 / n.visits as f64);
            memo.insert(*k, (q, v));
            v
        }
        let mut memo = HashMap::new();
        let mut out = Vec::new();
        for (k, n) in &self.nodes {
            let Some(b) = n.bl.as_deref() else {
                out.push(format!("{k:?}: no best-line values"));
                continue;
            };
            if n.visits == 0 {
                continue;
            }
            // Every try of an edge reached a stored child or ended there.
            for a in 0..n.n.len() {
                let direct = b.clean[a].0 + b.rollout[a].0;
                if b.kid_visits[a] + direct != n.n[a] {
                    out.push(format!(
                        "{k:?} edge {a}: {} child visits + {direct} direct != {} tries",
                        b.kid_visits[a], n.n[a]
                    ));
                }
            }
            value(self, &kids, k, &mut memo);
            let (q, v) = &memo[k];
            let tried = |a: &usize| n.n[*a] > 0;
            if (v - b.v).abs() > 1e-9
                || (0..q.len())
                    .filter(tried)
                    .any(|a| (q[a] - b.q[a]).abs() > 1e-9)
            {
                out.push(format!("{k:?}: stored {:?}/{} vs {q:?}/{v}", b.q, b.v));
            }
        }
        out
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
    use serde_json::json;

    /// A random frame for `canon_plan_matches_canon_prepared`: few handles,
    /// card ids and damage values, so lists hold many entries equal under the
    /// mask; nested and per-seat unordered lists; attachments holding numbers,
    /// objects or a non-list; handles and counters that are not integers.
    fn random_frame(rng: &mut SplitMix64) -> (Value, Vec<Value>) {
        fn pick(rng: &mut SplitMix64, n: u64) -> u64 {
            rng.next_u64() % n
        }
        fn leaf(rng: &mut SplitMix64) -> Value {
            match pick(rng, 7) {
                0 => json!(null),
                1 => json!(pick(rng, 3) == 0),
                2 => json!(pick(rng, 4)),
                3 => json!(["a", "b\"\\\u{1}", "\u{e9}"][pick(rng, 3) as usize]),
                4 => json!(-(pick(rng, 3) as i64)),
                5 => json!(0.5),
                _ => json!({}),
            }
        }
        fn handle(rng: &mut SplitMix64) -> Value {
            match pick(rng, 12) {
                0 => json!(null),
                1 => json!("h"),
                _ => json!(1 + pick(rng, 7)),
            }
        }
        fn entry(rng: &mut SplitMix64, depth: u32) -> Value {
            let mut m = serde_json::Map::new();
            let mut stable = serde_json::Map::new();
            stable.insert("arena_id".into(), handle(rng));
            stable.insert("card_db_id".into(), json!(pick(rng, 2)));
            if pick(rng, 4) > 0 {
                stable.insert("zone_change_count".into(), json!(pick(rng, 3)));
            }
            if pick(rng, 6) == 0 {
                stable.insert("zone_change_generation".into(), json!(pick(rng, 9)));
            }
            m.insert("stable".into(), Value::Object(stable));
            m.insert("damage".into(), json!(pick(rng, 2)));
            match pick(rng, 5) {
                0 => {}
                1 => {
                    m.insert("attachments".into(), json!(pick(rng, 3)));
                }
                2 if depth > 0 => {
                    m.insert(
                        "attachments".into(),
                        json!([1 + pick(rng, 7), value(rng, depth - 1)]),
                    );
                }
                _ => {
                    let a: Vec<Value> =
                        (0..pick(rng, 3)).map(|_| json!(1 + pick(rng, 7))).collect();
                    m.insert("attachments".into(), Value::Array(a));
                }
            }
            if pick(rng, 3) == 0 {
                m.insert("timestamp".into(), json!(100 + pick(rng, 4)));
            }
            if depth > 0 && pick(rng, 3) == 0 {
                m.insert("cards".into(), list(rng, depth - 1));
            }
            if pick(rng, 5) == 0 {
                m.insert("x".into(), leaf(rng));
            }
            Value::Object(m)
        }
        fn list(rng: &mut SplitMix64, depth: u32) -> Value {
            let n = pick(rng, 5);
            match pick(rng, 6) {
                0 => Value::Array((0..n).map(|_| leaf(rng)).collect()),
                1 => json!([
                    Value::Array((0..n).map(|_| entry(rng, depth)).collect()),
                    Value::Array((0..pick(rng, 4)).map(|_| entry(rng, depth)).collect())
                ]),
                _ => Value::Array((0..n).map(|_| entry(rng, depth)).collect()),
            }
        }
        fn value(rng: &mut SplitMix64, depth: u32) -> Value {
            let mut m = serde_json::Map::new();
            for k in ["battlefield", "exile", "stack", "object_relations", "y"] {
                if pick(rng, 2) == 0 {
                    m.insert(k.into(), list(rng, depth));
                }
            }
            if pick(rng, 2) == 0 {
                m.insert("arena_id".into(), handle(rng));
                m.insert("zone_change_count".into(), json!(pick(rng, 3)));
            }
            if pick(rng, 3) == 0 {
                m.insert(
                    "timestamp".into(),
                    [json!("t"), json!(7), json!(101)][pick(rng, 3) as usize].clone(),
                );
            }
            if pick(rng, 4) == 0 {
                m.insert("zone_change_generation".into(), json!(1));
            }
            Value::Object(m)
        }
        let obs = value(rng, 2);
        let menu = (0..pick(rng, 5))
            .map(|_| match pick(rng, 4) {
                0 => json!({"action_kind":"pass"}),
                1 => json!({"action_kind":"cast","cards":list(rng, 0)}),
                _ => json!({"action_kind":"attack",
                    "source":{"arena_id":handle(rng),"zone_change_count":pick(rng, 3)}}),
            })
            .collect();
        (obs, menu)
    }

    /// `CanonPlan` gives exactly `canon_prepared`'s canonical view, for the
    /// legacy rule and for arbitrary tie ranks (including ranks for handles
    /// that are not tied and repeated rank values).
    #[test]
    fn canon_plan_matches_canon_prepared() {
        let mut rng = SplitMix64::seed(20261019);
        let (mut tied, mut reordered) = (0, 0);
        for _ in 0..4000 {
            let (obs, menu) = random_frame(&mut rng);
            let plan = CanonPlan::new(&obs, &menu);
            tied += usize::from(plan.lists.iter().any(|l| !l.runs.is_empty()));
            let reference = |ranks: Option<&TieRanks>| {
                canon_prepared(obs.clone(), menu.clone(), ranks).unwrap().0
            };
            let legacy = plan.canon(None).unwrap();
            assert_eq!(legacy, reference(None), "{obs} {menu:?}");
            for _ in 0..3 {
                let mut ranks = TieRanks::new();
                for h in 1..=8u64 {
                    if !rng.next_u64().is_multiple_of(3) {
                        ranks.insert(h, (rng.next_u64() % 4) as u32);
                    }
                }
                let ranked = plan.canon(Some(&ranks)).unwrap();
                reordered += usize::from(ranked != legacy);
                assert_eq!(ranked, reference(Some(&ranks)), "{obs} {menu:?} {ranks:?}");
            }
        }
        // The frames exercise tie runs often, and ranks often change the view.
        assert!(tied > 500 && reordered > 200, "{tied} {reordered}");
    }

    /// The clone-and-mask `masked` this module used before the streaming one.
    fn masked_by_clone(v: &Value) -> String {
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

    #[test]
    fn streaming_mask_matches_clone_and_mask_bytes() {
        let cases = [
            serde_json::json!(null),
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!({"arena_id":7,"zone_change_count":2,"attachments":[3,4],"name":"Bolt"}),
            serde_json::json!({"a":[{"arena_id":1,"x":{"arena_id":2,"attachments":[{"arena_id":9}]}}],
                "f":1.5e-7,"g":-0.0,"h":18446744073709551615u64,"i":-9007199254740993i64,
                "s":"quote \" back \\ tab \t uni \u{e9} ctl \u{1}","t":true,"n":null,
                "zone_change_count":{"arena_id":3},"zz":[[{"zone_change_count":1}],[]]}),
        ];
        for v in &cases {
            assert_eq!(masked(v), masked_by_clone(v), "{v}");
            let mut w = v.clone();
            sort_object_lists(&mut w);
            assert_eq!(masked(&w), masked_by_clone(&w));
        }
    }

    #[test]
    fn stamp_ranks_count_distinct_smaller_timestamps() {
        let v = serde_json::json!({"e":[{"timestamp":700},{"timestamp":40},{"timestamp":700},
            {"timestamp":41},{"x":{"timestamp":5}}]});
        let mut h = Handles::default();
        collect_ids(&v, &mut h);
        h.finish();
        let distinct = [5u64, 40, 41, 700];
        for t in [0u64, 5, 6, 40, 41, 699, 700, 701] {
            let expected = distinct.iter().filter(|&&s| s < t).count() as u64;
            assert_eq!(h.stamp_rank(t), expected, "{t}");
        }
        let mut r = v.clone();
        rank_counters(&mut r);
        assert_eq!(r["e"][0]["timestamp"], 3);
        assert_eq!(r["e"][1]["timestamp"], 1);
        assert_eq!(r["e"][3]["timestamp"], 2);
        assert_eq!(r["e"][4]["x"]["timestamp"], 0);
    }

    #[test]
    fn handle_positions_are_first_appearance() {
        let mut h = Handles::default();
        for id in [9u64, 3, 9, 7, 3, 1] {
            h.see(id);
        }
        assert_eq!(h.order, vec![9, 3, 7, 1]);
        for (i, id) in h.order.iter().enumerate() {
            assert_eq!(h.index(*id), i as u64);
        }
    }

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

    /// A tree of three nodes: root edge 0 leads to our choice node C, where
    /// edge 0 always wins with no rollout choice after it and edge 1 always
    /// loses through the rollout policy; root edge 1 leads to node D, which
    /// wins 6 of 10 by rollout. `bl` turns the best-line backup on.
    fn three_node_tree(bl: Option<u64>, attempts: usize) -> (Tree, [Key; 3]) {
        let keys = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let make = |parent: Key, edge: Vec<u8>, k: usize| {
            let menu: Vec<Vec<u8>> = (0..k).map(|i| vec![i as u8]).collect();
            let canon = Canon {
                obs: vec![],
                edges: menu.clone(),
                menu,
            };
            let mut n = Node::with_perm(parent, edge, canon, 1, (0..k).collect());
            n.bl = bl.map(|m| Box::new(BestLine::new(k, m)));
            n
        };
        let mut nodes = [
            Some((keys[0], make([0; 32], b"root".to_vec(), 2))),
            Some((keys[1], make(keys[0], vec![0], 2))),
            Some((keys[2], make(keys[0], vec![1], 1))),
        ];
        let mut t = Tree::default();
        let mut sims = Vec::new();
        for i in 0..10 {
            sims.push((vec![(keys[0], 0), (keys[1], 0)], true, true));
            sims.push((vec![(keys[0], 0), (keys[1], 1)], false, false));
            sims.push((vec![(keys[0], 1), (keys[2], 0)], i < 6, false));
        }
        for (path, win, world_only) in sims.into_iter().take(attempts) {
            let new: Vec<(Key, Node)> = path
                .iter()
                .filter_map(|(k, _)| {
                    let i = keys.iter().position(|x| x == k)?;
                    nodes[i].take()
                })
                .collect();
            t.backup_observed(&path, new, win, false, world_only);
        }
        (t, keys)
    }

    /// The best-line backup scores our own choices by their best edge and
    /// keeps the world's results: losses from our exploratory edge do not
    /// count against the root edge that leads to a proven win.
    #[test]
    fn best_line_backup_scores_our_choices_by_their_best_edge() {
        let (formal, keys) = three_node_tree(None, 30);
        let (bl, _) = three_node_tree(Some(3), 30);
        assert_eq!(
            formal.hash(),
            bl.hash(),
            "counts and hash are the formal ones"
        );
        assert_eq!(
            formal.nodes[&keys[0]].frozen_choice(),
            Some(1),
            "means 0.5 vs 0.6"
        );
        let root = &bl.nodes[&keys[0]];
        let b = root.bl.as_deref().unwrap();
        assert!(
            (b.q[0] - 1.0).abs() < 1e-12 && (b.q[1] - 0.6).abs() < 1e-12,
            "{:?}",
            b.q
        );
        assert!((b.v - 1.0).abs() < 1e-12);
        assert_eq!(root.frozen_choice(), Some(0));
        assert!(bl.best_line_mismatches().is_empty());
        let c = bl.nodes[&keys[1]].bl.as_deref().unwrap();
        assert!((c.v - 1.0).abs() < 1e-12 && c.q[1].abs() < 1e-12);
        // Selection uses the value plus the unchanged bonus: the proven edge.
        assert_eq!(root.select_with(Some(1.5)), 0);
        assert_eq!(formal.nodes[&keys[0]].select_with(Some(1.5)), 1);
    }

    /// Rollout results count while an edge has fewer than m clean outcomes,
    /// and a node with no edge of m tries is worth its mean result.
    #[test]
    fn best_line_falls_back_to_rollouts_and_means_below_the_threshold() {
        let (t, keys) = three_node_tree(Some(3), 3);
        let c = t.nodes[&keys[1]].bl.as_deref().unwrap();
        assert!((c.q[0] - 1.0).abs() < 1e-12 && c.q[1].abs() < 1e-12);
        assert!((c.v - 0.5).abs() < 1e-12, "{}", c.v);
        let r = t.nodes[&keys[0]].bl.as_deref().unwrap();
        // Root edge 0 has two tries reaching C (value 0.5): mean 0.5; root has
        // no edge with three tries, so its value is its mean result 2/3.
        assert!((r.q[0] - 0.5).abs() < 1e-12 && (r.q[1] - 1.0).abs() < 1e-12);
        assert!((r.v - 2.0 / 3.0).abs() < 1e-12, "{}", r.v);
    }

    /// With the backup off, a node's value is exactly the formal mean, so
    /// selection and execution are the formal ones.
    #[test]
    fn formal_nodes_use_the_formal_mean() {
        let mut n = node(4);
        for (a, (t, w)) in [(3u64, 1u64), (5, 4), (2, 0), (7, 7)]
            .into_iter()
            .enumerate()
        {
            n.n[a] = t;
            n.wins[a] = w;
            n.visits += t;
            assert_eq!(n.value(a).to_bits(), (w as f64 / t as f64).to_bits());
        }
        assert!(n.bl.is_none());
    }

    /// Merged edges (`interchange.rs`) are never tried, selected or executed,
    /// under either selection rule; with no merges nothing changes.
    #[test]
    fn skipped_edges_are_never_selected_or_executed() {
        for u in [None, Some(1.5)] {
            let mut n = node(4);
            n.skip = vec![false, true, false, true];
            for _ in 0..200 {
                let a = n.select_with(u);
                assert!(!n.skip[a], "{a}");
                visit(&mut n, a, a == 2);
            }
            assert_eq!(n.n[1] + n.n[3], 0);
            assert_eq!(n.frozen_choice(), Some(2));
            n.n[1] = 50;
            n.wins[1] = 50;
            assert_eq!(n.frozen_choice(), Some(2), "a skipped edge never executes");
        }
        let mut plain = node(3);
        let order = plain.perm.clone();
        for &a in &order {
            assert_eq!(plain.select(), a);
            visit(&mut plain, a, false);
        }
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
        h.finish();
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
        h.finish();
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

    /// A small actor view: two seats' battlefields, a stack spell with one
    /// object target, an aura relation, and a menu naming objects.
    fn view(bf: Vec<Value>, target: (u64, u64), source: (u64, u64)) -> (Value, Vec<Value>) {
        let obs = serde_json::json!({"projection":{
            "battlefield":[[{"stable":{"arena_id":90,"card_db_id":5,"zone_change_count":1},"tapped":false,"damage":0,"attachments":[]}], bf],
            "stack":[{"source":{"arena_id":source.0,"card_db_id":41,"zone":"Stack","zone_change_count":source.1},
                "targets":[{"object":{"arena_id":target.0,"card_db_id":19,"zone":"Battlefield","zone_change_count":target.1},"target_kind":"object"}]}],
            "object_relations":[]},"step_index":7});
        let menu = vec![
            serde_json::json!({"action_kind":"pass"}),
            serde_json::json!({"action_kind":"activate","source":{"arena_id":90,"zone_change_count":1}}),
        ];
        (obs, menu)
    }

    fn creature(id: u64, zcc: u64, damage: u64, tapped: bool, attachments: Vec<u64>) -> Value {
        serde_json::json!({"stable":{"arena_id":id,"card_db_id":19,"zone_change_count":zcc},
            "tapped":tapped,"damage":damage,"attachments":attachments})
    }

    fn rename(v: &mut Value, f: &impl Fn(u64) -> u64) {
        match v {
            Value::Object(m) => {
                if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                    m.insert("arena_id".into(), Value::from(f(id)));
                }
                if let Some(Value::Array(a)) = m.get_mut("attachments") {
                    for x in a.iter_mut() {
                        *x = Value::from(f(x.as_u64().unwrap()));
                    }
                }
                for (k, x) in m.iter_mut() {
                    if k != "attachments" {
                        rename(x, f);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| rename(x, f)),
            _ => {}
        }
    }

    fn c((obs, menu): (Value, Vec<Value>)) -> (Canon, Mapping) {
        canon_values(obs, menu).unwrap()
    }

    /// Positive: a consistent renaming of every handle leaves the canonical
    /// view and sorted edges unchanged, and the mapping is the renaming.
    #[test]
    fn canonical_view_is_invariant_under_consistent_renaming() {
        let bf = vec![
            creature(11, 3, 0, false, vec![]),
            creature(12, 3, 2, true, vec![]),
        ];
        let base = view(bf, (12, 3), (40, 0));
        let f = |id: u64| 1000 - 7 * id;
        let mut renamed = base.clone();
        rename(&mut renamed.0, &f);
        renamed.1.iter_mut().for_each(|m| rename(m, &f));
        let (a, ma) = c(base);
        let (b, mb) = c(renamed);
        assert_eq!(a, b);
        assert_eq!(mb.order, ma.order.iter().map(|&x| f(x)).collect::<Vec<_>>());
        // Live candidates bind one to one.
        for (i, m) in a.menu.iter().enumerate() {
            assert_eq!(live_index(&b, m).unwrap(), i as u32);
        }
    }

    /// Positive: permuting a declared-unordered list of distinct entries
    /// leaves the canonical view unchanged.
    #[test]
    fn distinct_entries_are_order_free() {
        let x = creature(11, 3, 0, false, vec![]);
        let y = creature(12, 3, 2, true, vec![]);
        let a = c(view(vec![x.clone(), y.clone()], (12, 3), (40, 0))).0;
        let b = c(view(vec![y, x], (12, 3), (40, 0))).0;
        assert_eq!(a, b);
    }

    /// Failing invariance witness (collab LANES/search-identity-audit-20261011):
    /// two visibly identical copies tie in the content sort, so their
    /// declared-unordered list order (engine order) decides which canonical
    /// handle each receives, and a target naming "the first copy" and one
    /// naming "the second" get different canonical views although the two
    /// views are the same labeled graph up to exchanging the copies. This
    /// asserts the current behavior; a repair must flip it deliberately.
    #[test]
    fn equal_copy_order_changes_target_identity() {
        let x = creature(11, 3, 0, false, vec![]);
        let y = creature(12, 3, 0, false, vec![]);
        let first = c(view(vec![x.clone(), y.clone()], (11, 3), (40, 0))).0;
        let permuted = c(view(vec![y.clone(), x.clone()], (11, 3), (40, 0))).0;
        assert_ne!(first.obs, permuted.obs, "list permutation changed the view");
        // Equivalently, targeting either copy: an exchange of the two handles.
        let other = c(view(vec![x, y], (12, 3), (40, 0))).0;
        assert_ne!(first.obs, other.obs);
        assert_eq!(permuted.obs, other.obs);
        assert_eq!(first.edges, other.edges);
    }

    /// Second failing invariance witness: the sort mask also nulls
    /// `attachments`, so two entries that differ only in what is attached
    /// to them tie, and engine order again decides their canonical handles
    /// (the attachment itself stays visible, so targets remain distinct).
    #[test]
    fn attachment_only_difference_ties() {
        let x = creature(11, 3, 0, false, vec![]);
        let y = creature(12, 3, 0, false, vec![90]);
        let a = c(view(vec![x.clone(), y.clone()], (12, 3), (40, 0))).0;
        let b = c(view(vec![y, x], (12, 3), (40, 0))).0;
        assert_ne!(a.obs, b.obs, "list permutation changed the view");
    }

    /// Negative controls: targets that differ in damage, tapped state, an
    /// attachment, or incarnation stay distinct, whatever the list order.
    #[test]
    fn real_target_distinctions_survive() {
        let cases = [
            (
                creature(11, 3, 0, false, vec![]),
                creature(12, 3, 2, false, vec![]),
            ),
            (
                creature(11, 3, 0, false, vec![]),
                creature(12, 3, 0, true, vec![]),
            ),
            (
                creature(11, 3, 0, false, vec![]),
                creature(12, 3, 0, false, vec![90]),
            ),
        ];
        for (i, (x, y)) in cases.into_iter().enumerate() {
            let tx = c(view(vec![x.clone(), y.clone()], (11, 3), (40, 0))).0;
            let ty = c(view(vec![x.clone(), y.clone()], (12, 3), (40, 0))).0;
            let ty_perm = c(view(vec![y, x], (12, 3), (40, 0))).0;
            assert_ne!(tx.obs, ty.obs);
            // Entries differing only in attachments tie in the masked sort
            // (see `attachment_only_difference_ties`).
            if i < 2 {
                assert_eq!(ty, ty_perm);
            }
        }
        // A stale target reference (an earlier incarnation of the same
        // object) differs from a current one.
        let bf = vec![
            creature(11, 4, 0, false, vec![]),
            creature(12, 3, 0, false, vec![]),
        ];
        let current = c(view(bf.clone(), (11, 4), (40, 0))).0;
        let stale = c(view(bf.clone(), (11, 3), (40, 0))).0;
        assert_ne!(current.obs, stale.obs);
        // A frozen trigger source from an earlier incarnation differs from
        // the object's current incarnation as source.
        let frozen = c(view(bf.clone(), (12, 3), (11, 3))).0;
        let live = c(view(bf, (12, 3), (11, 4))).0;
        assert_ne!(frozen.obs, live.obs);
    }

    /// The stack, its targets and combat orders keep engine order.
    #[test]
    fn ordered_structures_are_not_sorted() {
        let mut v = serde_json::json!({"stack":[{"n":2},{"n":1}],"targets":[{"n":2},{"n":1}],
            "combat":{"attack_order":[{"n":2},{"n":1}]}});
        sort_object_lists(&mut v);
        assert_eq!(v["stack"][0]["n"], 2);
        assert_eq!(v["targets"][0]["n"], 2);
        assert_eq!(v["combat"]["attack_order"][0]["n"], 2);
    }
}
