//! Versioned search keys (collab LANES/search-canonical-repair-plan-20261011).
//!
//! `Legacy` is the frozen rule (`tree.rs`): each focal frame is canonicalized
//! on its own, and entries that are equal under the content mask keep engine
//! order, so which of two visibly identical copies an outside reference names
//! can change the key.
//!
//! `Repaired` keys a frame by the minimum canonical bytes over all orders of
//! such tie classes, but only for classes that are provably exchangeable
//! along the visible search prefix:
//! - a tie class is a set of entries of one declared-unordered list that are
//!   equal under the content mask; it matters only if permuting it can change
//!   the bytes (a member's handle appears outside its own entry, a member
//!   entry names another handle, or the menu is not invariant);
//! - such a class qualifies only if, in every earlier frame of this path
//!   (root included), every member is shown in the same zone with the same
//!   incarnation count, the members are tied there too, that frame's view and
//!   menu are unchanged by exchanging any two members, and the action taken
//!   there names no member. Staying in one shown zone at one incarnation is
//!   the visible continuity; a member that left the view, moved, or changed
//!   incarnation disqualifies the class;
//! - the minimum is exact: every order of the qualifying classes is tried,
//!   within `TIE_WORK_BOUND` canonicalizations.
//!
//! A frame with a mattering class that does not qualify, or over the work
//! bound, takes a separately tagged fallback key built from the complete
//! legacy history key, so it shares nothing that legacy separates. The legacy
//! history key is carried along every path for this reason.
//!
//! `Shadow` stores and backs up under legacy keys, but seeds each node's tie
//! order from the repaired key over repaired edges, mapped to legacy edges
//! through the live candidates (the B arm). Nothing here reads hidden state.

use super::play::PlayErr;
use super::seeds::{Purpose, RootSeeds};
use super::tree::{
    canon_prepared, child_key, masked, own_handle, permutation, prepare, raw_decision, Canon, Key,
    TieRanks, UNORDERED_LISTS,
};
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

/// Most canonicalizations tried to resolve one frame's qualifying ties
/// (fixed before any measurement; above it the frame falls back).
pub(crate) const TIE_WORK_BOUND: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyMode {
    Legacy,
    Shadow,
    Repaired,
}

impl KeyMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Shadow => "shadow-priority",
            Self::Repaired => "repaired-v2",
        }
    }
}

/// Why a frame took the fallback key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Fallback {
    /// A member is missing from an earlier frame of the path.
    MemberAbsent,
    /// A member changed zone or incarnation along the path.
    ContinuityBroken,
    /// The members were not tied in an earlier frame.
    UntiedBefore,
    /// An earlier view or menu distinguishes the members.
    PrefixAsymmetric,
    /// An earlier action named a member.
    ActionNamedMember,
    /// More than `TIE_WORK_BOUND` canonicalizations would be needed.
    WorkLimit,
}

impl Fallback {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::MemberAbsent => "member_absent",
            Self::ContinuityBroken => "continuity_broken",
            Self::UntiedBefore => "untied_before",
            Self::PrefixAsymmetric => "prefix_asymmetric",
            Self::ActionNamedMember => "action_named_member",
            Self::WorkLimit => "work_limit",
        }
    }
}

fn hash_key(tag: &[u8], parts: &[&[u8]]) -> Key {
    let mut h = Sha256::new();
    let mut put = |b: &[u8]| {
        h.update((b.len() as u64).to_le_bytes());
        h.update(b);
    };
    put(tag);
    for p in parts {
        put(p);
    }
    h.finalize().into()
}

/// A repaired node key: the v2 domain over the repaired parent, edge and view.
pub(crate) fn child_key_v2(parent: &Key, edge: &[u8], c: &Canon) -> Key {
    let mut parts: Vec<&[u8]> = vec![parent, edge, &c.obs];
    let n = (c.edges.len() as u64).to_le_bytes();
    parts.push(&n);
    for e in &c.edges {
        parts.push(e);
    }
    hash_key(b"stage4a-node-v2", &parts)
}

/// The fallback key: the complete legacy history key, separately tagged.
pub(crate) fn fallback_key(legacy: &Key) -> Key {
    hash_key(b"stage4a-node-v2-fallback", &[legacy])
}

/// Every handle occurrence (`arena_id` values and `attachments` items).
fn handles(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                out.push(id);
            }
            if let Some(Value::Array(a)) = m.get("attachments") {
                out.extend(a.iter().filter_map(Value::as_u64));
            }
            m.values().for_each(|x| handles(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| handles(x, out)),
        _ => {}
    }
}

/// Exchanges handles `x` and `y` everywhere.
pub(crate) fn swap_handles(v: &mut Value, x: u64, y: u64) {
    let sw = |id: u64| {
        if id == x {
            y
        } else if id == y {
            x
        } else {
            id
        }
    };
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                m.insert("arena_id".into(), Value::from(sw(id)));
            }
            if let Some(Value::Array(a)) = m.get_mut("attachments") {
                for e in a.iter_mut() {
                    if let Some(id) = e.as_u64() {
                        *e = Value::from(sw(id));
                    }
                }
            }
            for (k, e) in m.iter_mut() {
                if k != "attachments" {
                    swap_handles(e, x, y);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|e| swap_handles(e, x, y)),
        _ => {}
    }
}

/// Unordered lists sorted by full JSON (multiset comparison).
fn as_multisets(v: &Value) -> Value {
    let mut x = v.clone();
    super::tree::for_unordered_lists(&mut x, &mut |a| {
        a.sort_by_cached_key(|e| e.to_string());
    });
    x
}

fn menu_multiset(menu: &[Value]) -> Vec<String> {
    let mut m: Vec<String> = menu.iter().map(Value::to_string).collect();
    m.sort_unstable();
    m
}

/// Read-only walk over every declared-unordered list.
fn visit_unordered<'a>(v: &'a Value, f: &mut impl FnMut(&'a [Value])) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                visit_unordered(x, f);
                if UNORDERED_LISTS.contains(&k.as_str()) {
                    if let Value::Array(a) = x {
                        if a.iter().all(Value::is_array) {
                            for inner in a {
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
        Value::Array(a) => a.iter().for_each(|x| visit_unordered(x, f)),
        _ => {}
    }
}

/// One tie class: raw handles of entries equal under the mask, in engine
/// (input) order, with whether any member entry names another handle.
#[derive(Clone, Debug)]
pub(crate) struct Class {
    pub(crate) members: Vec<u64>,
    foreign: bool,
    inside: Vec<usize>,
}

/// Tie classes of a prepared observation (entries without a handle ignored).
pub(crate) fn tie_classes(obs: &Value) -> Vec<Class> {
    let mut out = Vec::new();
    visit_unordered(obs, &mut |list| {
        if list.len() < 2 || !list.iter().all(Value::is_object) {
            return;
        }
        let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for e in list {
            groups.entry(masked(e)).or_default().push(e);
        }
        for (_, g) in groups {
            if g.len() < 2 {
                continue;
            }
            let ids: Vec<Option<u64>> = g.iter().map(|e| own_handle(e)).collect();
            if ids.iter().any(Option::is_none) {
                continue;
            }
            let members: Vec<u64> = ids.into_iter().flatten().collect();
            let mut foreign = false;
            let mut inside = Vec::new();
            for (e, &id) in g.iter().zip(&members) {
                let mut hs = Vec::new();
                handles(e, &mut hs);
                foreign |= hs.iter().any(|&h| h != id);
                inside.push(hs.iter().filter(|&&h| h == id).count());
            }
            out.push(Class {
                members,
                foreign,
                inside,
            });
        }
    });
    out
}

/// Whether permuting the class could change the frame's canonical bytes.
fn matters(c: &Class, total: &HashMap<u64, usize>, menu: &[Value]) -> bool {
    if c.foreign {
        return true;
    }
    for (id, inside) in c.members.iter().zip(&c.inside) {
        if total.get(id).copied().unwrap_or(0) > *inside {
            return true;
        }
    }
    let base = menu_multiset(menu);
    c.members[1..].iter().any(|&y| {
        let swapped: Vec<Value> = menu
            .iter()
            .map(|m| {
                let mut m = m.clone();
                swap_handles(&mut m, c.members[0], y);
                m
            })
            .collect();
        menu_multiset(&swapped) != base
    })
}

/// A frame already on the path: prepared view and menu, and the action
/// taken there.
#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub(crate) obs: Value,
    pub(crate) menu: Vec<Value>,
    pub(crate) taken: Value,
}

/// (masked entry, zone, incarnation) of the entry a handle names, if shown in
/// a declared-unordered list.
fn shown(obs: &Value, id: u64) -> Option<(String, Value, Value)> {
    let mut found = None;
    visit_unordered(obs, &mut |list| {
        if found.is_some() {
            return;
        }
        for e in list {
            if own_handle(e) == Some(id) {
                let mut at = None;
                let mut level = vec![e];
                while at.is_none() && !level.is_empty() {
                    let mut next = Vec::new();
                    for x in level {
                        match x {
                            Value::Object(m) => {
                                if m.get("arena_id").and_then(Value::as_u64) == Some(id) {
                                    at = Some(m);
                                    break;
                                }
                                next.extend(m.values());
                            }
                            Value::Array(a) => next.extend(a.iter()),
                            _ => {}
                        }
                    }
                    level = next;
                }
                let zone = at
                    .and_then(|m| m.get("zone"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let zcc = at
                    .and_then(|m| m.get("zone_change_count"))
                    .cloned()
                    .unwrap_or(Value::Null);
                found = Some((masked(e), zone, zcc));
                return;
            }
        }
    });
    found
}

/// Whether a mattering class is exchangeable along the whole prefix.
fn qualifies(c: &Class, now: &Value, prefix: &[Frame]) -> Result<(), Fallback> {
    let current: Vec<(Value, Value)> = c
        .members
        .iter()
        .map(|&id| {
            shown(now, id)
                .map(|(_, z, n)| (z, n))
                .unwrap_or((Value::Null, Value::Null))
        })
        .collect();
    for f in prefix {
        let mut masks = Vec::new();
        for (id, cur) in c.members.iter().zip(&current) {
            let (m, z, n) = shown(&f.obs, *id).ok_or(Fallback::MemberAbsent)?;
            if (z, n) != *cur {
                return Err(Fallback::ContinuityBroken);
            }
            masks.push(m);
        }
        if masks.iter().any(|m| *m != masks[0]) {
            return Err(Fallback::UntiedBefore);
        }
        let mut hs = Vec::new();
        handles(&f.taken, &mut hs);
        if hs.iter().any(|h| c.members.contains(h)) {
            return Err(Fallback::ActionNamedMember);
        }
        let base_obs = as_multisets(&f.obs);
        let base_menu = menu_multiset(&f.menu);
        for &y in &c.members[1..] {
            let mut o = f.obs.clone();
            swap_handles(&mut o, c.members[0], y);
            let menu: Vec<Value> = f
                .menu
                .iter()
                .map(|m| {
                    let mut m = m.clone();
                    swap_handles(&mut m, c.members[0], y);
                    m
                })
                .collect();
            if as_multisets(&o) != base_obs || menu_multiset(&menu) != base_menu {
                return Err(Fallback::PrefixAsymmetric);
            }
        }
    }
    Ok(())
}

fn factorial(n: usize) -> usize {
    (1..=n).fold(1usize, |a, b| a.saturating_mul(b))
}

/// All orders of `items` (identity first).
fn permutations(items: &[u64]) -> Vec<Vec<u64>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut p in permutations(&rest) {
            p.insert(0, head);
            out.push(p);
        }
    }
    out
}

/// The repaired canonical view of one frame, or why it falls back; with the
/// number of canonicalizations used.
pub(crate) fn repaired_view(
    obs: &Value,
    menu: &[Value],
    legacy: &Canon,
    prefix: &[Frame],
    bound: usize,
) -> Result<(Result<Canon, Fallback>, usize, usize), PlayErr> {
    let classes = tie_classes(obs);
    if classes.is_empty() {
        return Ok((Ok(legacy.clone()), 0, 0));
    }
    let mut total: HashMap<u64, usize> = HashMap::new();
    let mut all = Vec::new();
    handles(obs, &mut all);
    for h in all {
        *total.entry(h).or_default() += 1;
    }
    let mattering: Vec<&Class> = classes
        .iter()
        .filter(|c| matters(c, &total, menu))
        .collect();
    if mattering.is_empty() {
        return Ok((Ok(legacy.clone()), 0, 0));
    }
    for c in &mattering {
        if let Err(f) = qualifies(c, obs, prefix) {
            return Ok((Err(f), 0, mattering.len()));
        }
    }
    let work = mattering
        .iter()
        .fold(1usize, |a, c| a.saturating_mul(factorial(c.members.len())));
    if work > bound {
        return Ok((Err(Fallback::WorkLimit), 0, mattering.len()));
    }
    // Exact minimum over the product of class orders (identity first).
    let per: Vec<Vec<Vec<u64>>> = mattering.iter().map(|c| permutations(&c.members)).collect();
    let mut idx = vec![0usize; per.len()];
    let mut best: Option<Canon> = None;
    let mut tried = 0;
    loop {
        let mut ranks: TieRanks = HashMap::new();
        for (k, p) in idx.iter().zip(&per) {
            for (r, &h) in p[*k].iter().enumerate() {
                ranks.insert(h, r as u32);
            }
        }
        let (c, _) = canon_prepared(obs.clone(), menu.to_vec(), Some(&ranks))?;
        tried += 1;
        if best
            .as_ref()
            .is_none_or(|b| (&c.obs, &c.edges) < (&b.obs, &b.edges))
        {
            best = Some(c);
        }
        // Next combination (odometer).
        let mut i = 0;
        loop {
            if i == idx.len() {
                return Ok((Ok(best.expect("one order")), tried, mattering.len()));
            }
            idx[i] += 1;
            if idx[i] < per[i].len() {
                break;
            }
            idx[i] = 0;
            i += 1;
        }
    }
}

/// Keying cost and outcomes of one search.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyStats {
    pub(crate) frames: u64,
    pub(crate) frames_with_mattering_ties: u64,
    pub(crate) exact_resolved: u64,
    pub(crate) canonicalizations_for_ties: u64,
    pub(crate) fallbacks: BTreeMap<&'static str, u64>,
    pub(crate) seconds: f64,
}

impl KeyStats {
    pub(crate) fn json(&self) -> Value {
        serde_json::json!({"frames":self.frames,"frames_with_mattering_ties":self.frames_with_mattering_ties,
            "exact_resolved":self.exact_resolved,"canonicalizations_for_ties":self.canonicalizations_for_ties,
            "fallbacks":self.fallbacks,"fallback_total":self.fallbacks.values().sum::<u64>(),
            "keying_seconds":self.seconds,"work_bound":TIE_WORK_BOUND})
    }
}

/// One keyed focal frame.
pub(crate) struct Step {
    pub(crate) legacy_key: Key,
    pub(crate) legacy: Canon,
    pub(crate) repaired_key: Key,
    pub(crate) repaired: Canon,
    obs: Value,
    menu: Vec<Value>,
}

/// Keys along one path from the root; selection and frozen execution use it
/// identically.
pub(crate) struct Keyer<'s> {
    pub(crate) mode: KeyMode,
    legacy: (Key, Vec<u8>),
    repaired: (Key, Vec<u8>),
    frames: Vec<Frame>,
    pub(crate) stats: &'s mut KeyStats,
}

impl<'s> Keyer<'s> {
    pub(crate) fn new(mode: KeyMode, stats: &'s mut KeyStats) -> Self {
        Self {
            mode,
            legacy: ([0u8; 32], b"root".to_vec()),
            repaired: ([0u8; 32], b"root".to_vec()),
            frames: Vec::new(),
            stats,
        }
    }

    pub(crate) fn step(
        &mut self,
        s: &FastActorSessionV1,
        d: FastActorDecisionV1,
    ) -> Result<Step, PlayErr> {
        let (raw_obs, raw_menu) = raw_decision(s, d)?;
        self.key_raw(raw_obs, raw_menu)
    }

    /// `step` on a raw actor-visible observation and menu.
    pub(crate) fn key_raw(&mut self, raw_obs: Value, raw_menu: Vec<Value>) -> Result<Step, PlayErr> {
        let t0 = Instant::now();
        let (obs, menu) = prepare(raw_obs, raw_menu);
        let (legacy, _) = canon_prepared(obs.clone(), menu.clone(), None)?;
        let legacy_key = child_key(&self.legacy.0, &self.legacy.1, &legacy);
        let step = if self.mode == KeyMode::Legacy {
            Step {
                legacy_key,
                repaired_key: legacy_key,
                repaired: legacy.clone(),
                legacy,
                obs,
                menu,
            }
        } else {
            let (view, tried, mattering) =
                repaired_view(&obs, &menu, &legacy, &self.frames, TIE_WORK_BOUND)?;
            self.stats.frames_with_mattering_ties += u64::from(mattering > 0);
            self.stats.canonicalizations_for_ties += tried as u64;
            match view {
                Ok(c) => {
                    self.stats.exact_resolved += u64::from(mattering > 0);
                    Step {
                        legacy_key,
                        repaired_key: child_key_v2(&self.repaired.0, &self.repaired.1, &c),
                        repaired: c,
                        legacy,
                        obs,
                        menu,
                    }
                }
                Err(f) => {
                    *self.stats.fallbacks.entry(f.label()).or_default() += 1;
                    Step {
                        legacy_key,
                        repaired_key: fallback_key(&legacy_key),
                        repaired: legacy.clone(),
                        legacy,
                        obs,
                        menu,
                    }
                }
            }
        };
        self.stats.frames += 1;
        self.stats.seconds += t0.elapsed().as_secs_f64();
        Ok(step)
    }

    /// Storage (key, parent key, parent edge, canonical view) of a step.
    pub(crate) fn storage<'a>(&self, st: &'a Step) -> (Key, Key, Vec<u8>, &'a Canon) {
        match self.mode {
            KeyMode::Legacy | KeyMode::Shadow => (
                st.legacy_key,
                self.legacy.0,
                self.legacy.1.clone(),
                &st.legacy,
            ),
            KeyMode::Repaired => (
                st.repaired_key,
                self.repaired.0,
                self.repaired.1.clone(),
                &st.repaired,
            ),
        }
    }

    /// The tie order for a new node over the storage view's edges.
    pub(crate) fn perm(&self, st: &Step, seeds: &RootSeeds) -> Result<Vec<usize>, PlayErr> {
        match self.mode {
            KeyMode::Legacy => Ok(permutation(
                st.legacy.edges.len(),
                seeds.get(Purpose::Ties, "E", 0, &st.legacy_key),
            )),
            KeyMode::Repaired => Ok(permutation(
                st.repaired.edges.len(),
                seeds.get(Purpose::Ties, "E", 0, &st.repaired_key),
            )),
            KeyMode::Shadow => {
                // Repaired priority over repaired edges, mapped through the
                // live candidates to legacy edges (multiplicity preserved).
                let pr = permutation(
                    st.repaired.edges.len(),
                    seeds.get(Purpose::Ties, "E", 0, &st.repaired_key),
                );
                map_priority(&pr, &st.repaired, &st.legacy)
            }
        }
    }

    /// Records the live action taken at this frame.
    pub(crate) fn advance(&mut self, st: Step, live: u32) {
        let i = live as usize;
        self.legacy = (st.legacy_key, st.legacy.menu[i].clone());
        self.repaired = (st.repaired_key, st.repaired.menu[i].clone());
        if self.mode != KeyMode::Legacy {
            let taken = st.menu[i].clone();
            self.frames.push(Frame {
                obs: st.obs,
                menu: st.menu,
                taken,
            });
        }
    }
}

/// Maps a priority order over `from`'s edges to `to`'s edges through the live
/// candidates (both canonical menus are in live order).
pub(crate) fn map_priority(
    perm: &[usize],
    from: &Canon,
    to: &Canon,
) -> Result<Vec<usize>, PlayErr> {
    if from.menu.len() != to.menu.len() {
        return Err(PlayErr::Fault("priority map: menu sizes differ".into()));
    }
    let mut out = Vec::with_capacity(perm.len());
    for &e in perm {
        let live = from
            .menu
            .iter()
            .position(|m| *m == from.edges[e])
            .ok_or_else(|| PlayErr::Fault("priority map: edge without live candidate".into()))?;
        let target = to
            .edges
            .iter()
            .position(|m| *m == to.menu[live])
            .ok_or_else(|| PlayErr::Fault("priority map: live candidate without edge".into()))?;
        out.push(target);
    }
    let mut seen = out.clone();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != out.len() {
        return Err(PlayErr::Fault("priority map is not a bijection".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::tree::{canon_values, live_index};
    use super::*;
    use serde_json::json;

    /// A creature entry: handle, card, incarnation, damage, tapped, attachments.
    fn cr(id: u64, card: u64, zcc: u64, damage: u64, tapped: bool, att: Vec<u64>) -> Value {
        json!({"stable":{"arena_id":id,"card_db_id":card,"zone":"Battlefield","zone_change_count":zcc},
            "damage":damage,"tapped":tapped,"attachments":att})
    }

    fn obj(id: u64, card: u64, zcc: u64) -> Value {
        json!({"arena_id":id,"card_db_id":card,"zone":"Battlefield","zone_change_count":zcc})
    }

    /// A view: focal battlefield, a stack spell with the given targets, and
    /// optional relations.
    fn view(bf: Vec<Value>, targets: Vec<Value>, relations: Vec<Value>) -> Value {
        let stack = if targets.is_empty() {
            json!([])
        } else {
            json!([{"source":{"arena_id":40,"card_db_id":41,"zone":"Stack","zone_change_count":0},
                "targets":targets.into_iter().map(|t| json!({"object":t,"target_kind":"object"})).collect::<Vec<_>>()}])
        };
        json!({"projection":{"battlefield":[[cr(90,5,1,0,false,vec![])], bf],"stack":stack,
            "object_relations":relations},"step_index":7})
    }

    fn menu(ids: &[u64]) -> Vec<Value> {
        let mut m = vec![json!({"action_kind":"pass"})];
        for &id in ids {
            m.push(json!({"action_kind":"attack","source":{"arena_id":id,"zone_change_count":3}}));
        }
        m
    }

    fn keyer(mode: KeyMode, stats: &mut KeyStats) -> Keyer<'_> {
        Keyer::new(mode, stats)
    }

    /// Repaired key of a single (root) frame.
    fn root_key(o: Value, m: Vec<Value>) -> (Key, Canon, u64) {
        let mut s = KeyStats::default();
        let st = keyer(KeyMode::Repaired, &mut s).key_raw(o, m).unwrap();
        let fb = s.fallbacks.values().sum();
        (st.repaired_key, st.repaired, fb)
    }

    fn x() -> Value {
        cr(11, 19, 3, 0, false, vec![])
    }
    fn y() -> Value {
        cr(12, 19, 3, 0, false, vec![])
    }

    /// The two audit witnesses become order-invariant in the repaired mode
    /// and stay order-dependent in the legacy mode.
    #[test]
    fn witnesses_flip_to_invariance() {
        let a = view(vec![x(), y()], vec![obj(11, 19, 3)], vec![]);
        let b = view(vec![y(), x()], vec![obj(11, 19, 3)], vec![]);
        let other = view(vec![x(), y()], vec![obj(12, 19, 3)], vec![]);
        let (ka, ca, f) = root_key(a.clone(), menu(&[11, 12]));
        assert_eq!(f, 0);
        assert_eq!(ka, root_key(b.clone(), menu(&[11, 12])).0);
        assert_eq!(ka, root_key(other, menu(&[11, 12])).0);
        assert_ne!(
            canon_values(a, menu(&[11, 12])).unwrap().0.obs,
            canon_values(b, menu(&[11, 12])).unwrap().0.obs
        );
        // Attachment-only difference.
        let p = cr(11, 19, 3, 0, false, vec![]);
        let q = cr(12, 19, 3, 0, false, vec![90]);
        let a2 = view(vec![p.clone(), q.clone()], vec![obj(12, 19, 3)], vec![]);
        let b2 = view(vec![q, p], vec![obj(12, 19, 3)], vec![]);
        assert_eq!(root_key(a2, menu(&[])).0, root_key(b2, menu(&[])).0);
        assert!(!ca.obs.is_empty());
    }

    /// Consistent renaming plus unordered-list and menu permutations give
    /// the same key, and every live candidate binds to exactly one edge,
    /// duplicate counts preserved.
    #[test]
    fn renaming_and_permutations_keep_key_and_binding() {
        let mut m = menu(&[11, 12]);
        m.push(json!({"action_kind":"pass"}));
        let a = view(vec![x(), y()], vec![obj(12, 19, 3)], vec![]);
        let (ka, ca, _) = root_key(a, m.clone());
        // Rename 11->70, 12->60, 90->80, 40->30, permute lists and menu.
        let ren = |v: &Value| {
            let mut v = v.clone();
            swap_handles(&mut v, 11, 70);
            swap_handles(&mut v, 12, 60);
            swap_handles(&mut v, 90, 80);
            swap_handles(&mut v, 40, 30);
            v
        };
        let b = ren(&view(vec![y(), x()], vec![obj(12, 19, 3)], vec![]));
        let mut mb: Vec<Value> = m.iter().map(ren).collect();
        mb.reverse();
        let (kb, cb, _) = root_key(b, mb.clone());
        assert_eq!(ka, kb);
        assert_eq!(ca.edges, cb.edges);
        let mut seen = Vec::new();
        for (i, e) in cb.menu.iter().enumerate() {
            assert_eq!(live_index(&cb, e).unwrap(), i as u32);
            seen.push(e.clone());
        }
        seen.sort();
        assert_eq!(seen, cb.edges, "one edge per live candidate, duplicates kept");
        assert_eq!(cb.edges.len(), 4);
    }

    /// Real distinctions survive the repaired mode.
    #[test]
    fn distinctions_survive() {
        let k = |o: Value| root_key(o, menu(&[])).0;
        // [X, X] versus [X, Y] targets.
        let xx = view(vec![x(), y()], vec![obj(11, 19, 3), obj(11, 19, 3)], vec![]);
        let xy = view(vec![x(), y()], vec![obj(11, 19, 3), obj(12, 19, 3)], vec![]);
        assert_ne!(k(xx), k(xy));
        // Damage, tapped state, attachment topology.
        for other in [
            cr(12, 19, 3, 2, false, vec![]),
            cr(12, 19, 3, 0, true, vec![]),
            cr(12, 19, 3, 0, false, vec![90]),
        ] {
            let tx = view(vec![x(), other.clone()], vec![obj(11, 19, 3)], vec![]);
            let ty = view(vec![x(), other], vec![obj(12, 19, 3)], vec![]);
            assert_ne!(k(tx), k(ty));
        }
        // Stale versus current incarnation, and a frozen source.
        let bf = vec![cr(11, 19, 4, 0, false, vec![]), y()];
        let cur = view(bf.clone(), vec![obj(11, 19, 4)], vec![]);
        let stale = view(bf.clone(), vec![obj(11, 19, 3)], vec![]);
        assert_ne!(k(cur), k(stale));
        // Ordered stack and target slots.
        let a = view(vec![x(), y(), cr(13, 7, 0, 0, false, vec![])], vec![obj(11, 19, 3), obj(13, 7, 0)], vec![]);
        let b = view(vec![x(), y(), cr(13, 7, 0, 0, false, vec![])], vec![obj(13, 7, 0), obj(11, 19, 3)], vec![]);
        assert_ne!(k(a), k(b));
    }

    /// Two frames: an earlier action naming one copy keeps later targets of
    /// the two copies apart; with no such reference they merge.
    #[test]
    fn earlier_reference_keeps_copies_apart() {
        let run = |first_action: usize, target: u64| {
            let mut s = KeyStats::default();
            let mut kr = keyer(KeyMode::Repaired, &mut s);
            let f1 = kr.key_raw(view(vec![x(), y()], vec![], vec![]), menu(&[11, 12])).unwrap();
            // Live index of the chosen action in the frame-1 menu.
            kr.advance(f1, first_action as u32);
            let f2 = kr
                .key_raw(view(vec![x(), y()], vec![obj(target, 19, 3)], vec![]), menu(&[]))
                .unwrap();
            let fb = s.fallbacks.clone();
            (f2.repaired_key, fb)
        };
        // Passed first: the later targets merge.
        let (p11, f) = run(0, 11);
        let (p12, _) = run(0, 12);
        assert!(f.is_empty());
        assert_eq!(p11, p12);
        // Attacked with copy 11 first (frame-1 menu index 1): no merge.
        let (a11, f) = run(1, 11);
        let (a12, _) = run(1, 12);
        assert_eq!(f.get("action_named_member"), Some(&1));
        assert_ne!(a11, a12);
    }

    /// A member missing from an earlier frame (concealed, then shown again
    /// with the same engine handle) or with a changed incarnation does not
    /// certify continuity: the frame falls back.
    #[test]
    fn invisible_or_broken_continuity_falls_back() {
        let run = |frame1: Value| {
            let mut s = KeyStats::default();
            let mut kr = keyer(KeyMode::Repaired, &mut s);
            let f1 = kr.key_raw(frame1, menu(&[])).unwrap();
            kr.advance(f1, 0);
            let _ = kr
                .key_raw(view(vec![x(), y()], vec![obj(11, 19, 3)], vec![]), menu(&[]))
                .unwrap();
            s.fallbacks.clone()
        };
        let absent = run(view(vec![x()], vec![], vec![]));
        assert_eq!(absent.get("member_absent"), Some(&1));
        let moved = run(view(vec![x(), cr(12, 19, 2, 0, false, vec![])], vec![], vec![]));
        assert_eq!(moved.get("continuity_broken"), Some(&1));
    }

    fn rel(a: u64, b: u64) -> Value {
        json!({"kind":"linked","source":{"arena_id":a},"target":{"arena_id":b}})
    }

    /// Six identical objects related as two triangles or as one hexagon
    /// cannot be told apart by per-object reference counts. Over the work
    /// bound both fall back (no merge); with an exact search they differ.
    #[test]
    fn regular_relations_are_not_merged() {
        let bf: Vec<Value> = (1..=6).map(|i| cr(10 + i, 19, 3, 0, false, vec![])).collect();
        let tri = vec![rel(11, 12), rel(12, 13), rel(13, 11), rel(14, 15), rel(15, 16), rel(16, 14)];
        let hex = vec![rel(11, 12), rel(12, 13), rel(13, 14), rel(14, 15), rel(15, 16), rel(16, 11)];
        let (kt, _, ft) = root_key(view(bf.clone(), vec![], tri.clone()), menu(&[]));
        let (kh, _, fh) = root_key(view(bf.clone(), vec![], hex.clone()), menu(&[]));
        assert_eq!((ft, fh), (1, 1), "720 orders exceed the bound");
        assert_ne!(kt, kh);
        let exact = |r: Vec<Value>| {
            let (o, m) = prepare(view(bf.clone(), vec![], r), menu(&[]));
            let (l, _) = canon_prepared(o.clone(), m.clone(), None).unwrap();
            repaired_view(&o, &m, &l, &[], 1000).unwrap().0.unwrap()
        };
        assert_ne!(exact(tri), exact(hex));
    }

    /// A repaired merge followed by an unresolved frame: the fallback keys
    /// keep the legacy separation of the two histories.
    #[test]
    fn fallback_after_merge_keeps_legacy_separation() {
        let run = |target: u64| {
            let mut s = KeyStats::default();
            let mut kr = keyer(KeyMode::Repaired, &mut s);
            let f1 = kr
                .key_raw(view(vec![x(), y()], vec![obj(target, 19, 3)], vec![]), menu(&[]))
                .unwrap();
            let r1 = f1.repaired_key;
            kr.advance(f1, 0);
            // Frame 2 still shows the target on the stack: the copies are
            // asymmetric in frame 1, so a tie that matters cannot resolve.
            let f2 = kr
                .key_raw(view(vec![x(), y()], vec![obj(target, 19, 3)], vec![]), menu(&[]))
                .unwrap();
            (r1, f2.repaired_key, f2.legacy_key, s.fallbacks.clone())
        };
        let (a1, a2, al, fa) = run(11);
        let (b1, b2, bl, _) = run(12);
        assert_eq!(a1, b1, "frame 1 merges");
        assert_eq!(fa.get("prefix_asymmetric"), Some(&1));
        assert_ne!(al, bl);
        assert_ne!(a2, b2, "fallback keeps the legacy separation");
        assert_eq!(a2, fallback_key(&al));
    }

    /// The shadow arm's tie order maps, action by action, to the repaired
    /// arm's order over the same live candidates.
    #[test]
    fn shadow_and_repaired_priorities_correspond() {
        let seeds = RootSeeds {
            model: "t".into(),
            root: "r".into(),
        };
        let mut m = menu(&[11, 12]);
        m.push(json!({"action_kind":"pass"}));
        let o = view(vec![y(), x()], vec![obj(11, 19, 3)], vec![]);
        let (mut s1, mut s2) = (KeyStats::default(), KeyStats::default());
        let mut sh = keyer(KeyMode::Shadow, &mut s1);
        let mut re = keyer(KeyMode::Repaired, &mut s2);
        let a = sh.key_raw(o.clone(), m.clone()).unwrap();
        let b = re.key_raw(o, m).unwrap();
        let ps = sh.perm(&a, &seeds).unwrap();
        let pr = re.perm(&b, &seeds).unwrap();
        assert_eq!(ps.len(), pr.len());
        for (es, er) in ps.iter().zip(&pr) {
            let ls = a.legacy.menu.iter().position(|x| *x == a.legacy.edges[*es]).unwrap();
            let lr = b.repaired.menu.iter().position(|x| *x == b.repaired.edges[*er]).unwrap();
            // Duplicate canonical actions bind to either duplicate; compare
            // the semantic action.
            assert_eq!(a.legacy.menu[ls], a.legacy.menu[lr], "same live action class");
        }
        // Legacy mode keys equal the legacy chain.
        let mut s3 = KeyStats::default();
        let st = keyer(KeyMode::Legacy, &mut s3)
            .key_raw(view(vec![x(), y()], vec![], vec![]), menu(&[11]))
            .unwrap();
        let (c, _) = canon_values(view(vec![x(), y()], vec![], vec![]), menu(&[11])).unwrap();
        assert_eq!(st.legacy_key, child_key(&[0u8; 32], b"root", &c));
        assert_eq!(st.repaired_key, st.legacy_key);
    }
}
