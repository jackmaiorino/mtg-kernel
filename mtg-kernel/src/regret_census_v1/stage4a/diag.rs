//! Passive E-only decision trace for the Spy execution diagnosis (collab
//! LANES/spy-execution-diagnosis-plan-20261010, RUNNER.md section 3).
//!
//! Tracing reads values the search and frozen execution already computed
//! (canonical decisions, node statistics, chosen actions, the existing
//! suffix resolution events) and the focal player's current menu semantics.
//! It never calls a model, draws a random number, replays an action or makes
//! a tree lookup, so a traced run is identical to an untraced one. Card
//! labels here are descriptive only: nothing that chooses, samples, scores
//! or backs up reads them. Opponent decisions contribute counts and the
//! existing resolution events only, never their menus.

use super::labels::{classify, Events, SpyDefs};
use super::tree::{Canon, Key, Node, Tree};
use crate::ids::PlayerId;
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Label bits of one action: 1 cast Spy, 2 Spy targets the focal player,
/// 4 Dread Return targets Lotleth Giant, 8 Spy targets another player,
/// 16 Dread Return targets something else.
pub(crate) fn bits(
    x: &ActionSemanticV1,
    s: &FastActorSessionV1,
    focal: PlayerId,
    d: &SpyDefs,
) -> u8 {
    let (cast, own, dr) = classify(x, s.game_state(), focal, d);
    let mut b = u8::from(cast) | u8::from(own) << 1 | u8::from(dr) << 2;
    if let ActionSemanticV1::ChooseTarget { source, target, .. } = x {
        if source.card_db_id == d.spy && !own {
            let other = !matches!(target, TargetRefV1::Player { player } if *player == PlayerSeatV1::from(focal));
            b |= u8::from(other) << 3;
        }
        if source.card_db_id == d.dread_return && !dr {
            b |= 16;
        }
    }
    b
}

fn event_bits(e: Events) -> u8 {
    u8::from(e.spy_resolved)
        | u8::from(e.self_target_resolved) << 1
        | u8::from(e.dr_giant_resolved) << 2
        | u8::from(e.dr_giant_stacked) << 3
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Where a selection simulation's focal non-forced action came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelSrc {
    /// UCB choice at an existing node.
    Tree,
    /// First choice at the node this simulation expands.
    Expand,
    /// Plain policy after the expansion.
    TailAfterExpand,
    /// Plain policy below the depth limit while still in the tree.
    TailDepth,
}

impl SelSrc {
    fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Expand => "expand",
            Self::TailAfterExpand => "tail_after_expand",
            Self::TailDepth => "tail_depth",
        }
    }
}

/// Where a frozen-execution focal non-forced action came from (mutually
/// exclusive; precedence: matching already disabled, then depth, then the
/// lookup that execution actually made).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvalSrc {
    TreeEdge,
    MatchedNoQualified,
    FirstMiss,
    AfterMiss,
    DepthLimit,
}

impl EvalSrc {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::TreeEdge => "tree_edge",
            Self::MatchedNoQualified => "matched_no_qualified_plain",
            Self::FirstMiss => "first_miss_plain",
            Self::AfterMiss => "after_miss_plain",
            Self::DepthLimit => "depth_limit_plain",
        }
    }
}

struct NodeMeta {
    id: u32,
    parent: Option<(u32, usize)>,
    labels: Vec<u8>,
    edges: Vec<String>,
}

/// Opt-in capture of full canonical views at the first step of a
/// choose-all selection (every remaining option must be taken) and at the
/// chooser's first decision after it, for a few sibling simulations
/// (`S4A_MILLOBS=<n>`). Operator diagnostics only.
#[derive(Default)]
pub(crate) struct MillObs {
    pub(crate) want: usize,
    captured: usize,
    /// (parent node id, edge index) of the first captured selection node.
    parent: Option<(u64, u64)>,
    /// Source card of the selection awaiting its post-selection capture.
    awaiting: Option<u16>,
}

/// One root's trace; `lines` become the JSONL sidecar.
#[derive(Default)]
pub(crate) struct Trace {
    pub(crate) lines: Vec<String>,
    ids: HashMap<Key, u32>,
    meta: HashMap<Key, NodeMeta>,
    cur: Map<String, Value>,
    pub(crate) millobs: Option<MillObs>,
}

/// Source card of a first choose-all selection step: every option selects
/// one object for the same effect, none selected yet, and the effect must
/// take all of them.
fn choose_all_start(sem: &[ActionSemanticV1]) -> Option<u16> {
    let mut src = None;
    for x in sem {
        let ActionSemanticV1::ChooseEffectTarget { source, selected_count, min_targets, max_targets, .. } = x else {
            return None;
        };
        if *selected_count != 0 || min_targets != max_targets || usize::from(*min_targets) != sem.len() {
            return None;
        }
        if src.is_some_and(|s| s != source.card_db_id) {
            return None;
        }
        src = Some(source.card_db_id);
    }
    src.filter(|_| sem.len() >= 2)
}

fn full_view(s: &FastActorSessionV1, d: &FastActorDecisionV1) -> Value {
    let canon = super::tree::canon(s, *d).ok();
    let raw = s.actor_visible_decision_v4(*d).ok();
    json!({
        "canon_obs": canon.as_ref().and_then(|c| serde_json::from_slice::<Value>(&c.obs).ok()),
        "canon_menu": canon.as_ref().map(|c| c.menu.iter().map(|m| String::from_utf8_lossy(m).into_owned()).collect::<Vec<_>>()),
        "raw_obs": raw.as_ref().and_then(|(o, _)| serde_json::to_value(o).ok()),
        "raw_menu": raw.as_ref().and_then(|(_, m)| serde_json::to_value(m).ok()),
    })
}

fn menu_bits(s: &FastActorSessionV1, focal: PlayerId, defs: &SpyDefs) -> Vec<u8> {
    s.diagnostic_current_action_semantics()
        .map(|sem| sem.iter().map(|x| bits(x, s, focal, defs)).collect())
        .unwrap_or_default()
}

fn push(m: &mut Map<String, Value>, key: &str, v: Value) {
    if let Some(Value::Array(a)) = m.get_mut(key) {
        a.push(v);
    }
}

impl Trace {
    fn id(&mut self, key: &Key) -> u32 {
        let n = self.ids.len() as u32;
        *self.ids.entry(*key).or_insert(n)
    }

    pub(crate) fn meta_line(&mut self, v: Value) {
        self.lines.push(v.to_string());
    }

    pub(crate) fn sim_rejected(&mut self, index: u64) {
        self.lines
            .push(json!({"r":"sel","i":index,"end":"rejected"}).to_string());
    }

    pub(crate) fn sim_begin(&mut self, index: u64, t0: u64) {
        if let Some(m) = self.millobs.as_mut() {
            m.awaiting = None;
        }
        self.cur = json!({"r":"sel","i":index,"t0":t0,"path":[],"dec":[],"ev":[],"tail":null})
            .as_object()
            .cloned()
            .unwrap_or_default();
    }

    /// A node created by expansion (the parent is the path's last entry).
    #[allow(
        clippy::too_many_arguments,
        reason = "values already computed by the search"
    )]
    pub(crate) fn node_created(
        &mut self,
        key: &Key,
        parent: Option<&(Key, usize)>,
        cn: &Canon,
        s: &FastActorSessionV1,
        focal: PlayerId,
        defs: &SpyDefs,
    ) {
        let id = self.id(key);
        let parent = parent.map(|(k, e)| (self.id(k), *e));
        if self.meta.contains_key(key) {
            return;
        }
        let live = menu_bits(s, focal, defs);
        let mut labels = vec![0u8; cn.edges.len()];
        for (i, m) in cn.menu.iter().enumerate() {
            if let (Some(p), Some(b)) = (cn.edges.iter().position(|x| x == m), live.get(i)) {
                labels[p] = *b;
            }
        }
        let edges = cn
            .edges
            .iter()
            .map(|e| String::from_utf8_lossy(e).into_owned())
            .collect();
        self.meta.insert(
            *key,
            NodeMeta {
                id,
                parent,
                labels,
                edges,
            },
        );
    }

    /// A focal non-forced selection decision; `at` is the tree node and edge
    /// index when the choice came from the tree.
    #[allow(
        clippy::too_many_arguments,
        reason = "values already computed by the search"
    )]
    pub(crate) fn sel_decision(
        &mut self,
        s: &FastActorSessionV1,
        d: &FastActorDecisionV1,
        a: u32,
        depth: u32,
        src: SelSrc,
        at: Option<&(Key, usize)>,
        t: u64,
        focal: PlayerId,
        defs: &SpyDefs,
    ) {
        let menu = menu_bits(s, focal, defs);
        let off = menu.iter().fold(0u8, |x, y| x | y);
        let ch = menu.get(a as usize).copied().unwrap_or(0);
        let pd = d.physical_decision_id;
        let ss = json!([d.substep_index, d.substep_count]);
        let mut p = Value::Null;
        if let Some((k, e)) = at {
            let id = self.id(k);
            if let Some(Value::Array(path)) = self.cur.get_mut("path") {
                p = json!(path.len());
                path.push(json!([id, e, t, pd, ss]));
            }
        }
        self.mill_capture(s, d, &p, t, depth);
        if matches!(src, SelSrc::TailAfterExpand | SelSrc::TailDepth)
            && self.cur.get("tail").is_some_and(Value::is_null)
        {
            self.cur
                .insert("tail".into(), json!({"t":t,"why":src.name(),"dep":depth}));
        }
        if off != 0 {
            push(
                &mut self.cur,
                "dec",
                json!({"t":t,"pd":pd,"ss":ss,"dep":depth,"src":src.name(),"off":off,"ch":ch,"p":p,
                    "menu":menu}),
            );
        }
    }

    fn mill_capture(&mut self, s: &FastActorSessionV1, d: &FastActorDecisionV1, p: &Value, t: u64, depth: u32) {
        let Some(m) = self.millobs.as_mut() else {
            return;
        };
        let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
        let start = choose_all_start(&sem);
        let i = self.cur.get("i").cloned().unwrap_or(Value::Null);
        if let Some(src) = m.awaiting {
            let same = sem.iter().all(|x| matches!(x, ActionSemanticV1::ChooseEffectTarget { source, .. } if source.card_db_id == src));
            if !same {
                m.awaiting = None;
                let view = full_view(s, d);
                self.lines.push(json!({"r":"millobs","phase":"after_selection","i":i,"t":t,"dep":depth,"view":view}).to_string());
            }
            return;
        }
        let (Some(src), Some(pi)) = (start, p.as_u64()) else {
            return;
        };
        if m.captured >= m.want || pi == 0 {
            return;
        }
        let parent = self.cur.get("path").and_then(|x| x.get(pi as usize - 1)).map(|e| (e[0].as_u64().unwrap_or(0), e[1].as_u64().unwrap_or(0)));
        let Some(parent) = parent else {
            return;
        };
        if m.parent.is_some_and(|x| x != parent) {
            return;
        }
        m.parent = Some(parent);
        m.captured += 1;
        m.awaiting = Some(src);
        let view = full_view(s, d);
        self.lines.push(json!({"r":"millobs","phase":"selection_start","i":i,"t":t,"dep":depth,"source":src,
            "parent":[parent.0, parent.1],"view":view}).to_string());
    }

    /// Existing resolution events of the transition that ended at `t`.
    pub(crate) fn event(&mut self, t: u64, focal_acted: bool, e: Events) {
        let b = event_bits(e);
        if b != 0 {
            push(&mut self.cur, "ev", json!([t, u8::from(!focal_acted), b]));
        }
    }

    pub(crate) fn sim_end(&mut self, end: &str, t1: u64, new: Option<&Key>) {
        let new = new.map(|k| self.id(k));
        let mut m = std::mem::take(&mut self.cur);
        m.insert("end".into(), json!(end));
        m.insert("t1".into(), json!(t1));
        m.insert("new".into(), json!(new));
        self.lines.push(Value::Object(m).to_string());
    }

    /// The final selection tree: one dictionary line per node.
    pub(crate) fn finish_tree(&mut self, tree: &Tree) {
        let mut rows: Vec<(u32, String)> = Vec::new();
        for (k, n) in &tree.nodes {
            let Some(m) = self.meta.get(k) else {
                rows.push((
                    u32::MAX,
                    json!({"r":"node","key":hex(k),"error":"no trace metadata"}).to_string(),
                ));
                continue;
            };
            rows.push((
                m.id,
                json!({"r":"node","id":m.id,"key":hex(k),"parent":m.parent,"dep":n.depth,
                    "visits":n.visits,"n":n.n,"w":n.wins,"perm":n.perm,"lab":m.labels,"edges":m.edges})
                .to_string(),
            ));
        }
        rows.sort();
        self.lines.extend(rows.into_iter().map(|(_, l)| l));
    }

    pub(crate) fn world_begin(&mut self, e: u64) {
        self.cur = json!({"r":"eval","world":e,"dec":[],"ev":[],"forced":0,"opp":0})
            .as_object()
            .cloned()
            .unwrap_or_default();
    }

    pub(crate) fn count(&mut self, key: &str) {
        let n = self.cur.get(key).and_then(Value::as_u64).unwrap_or(0);
        self.cur.insert(key.into(), json!(n + 1));
    }

    /// A focal non-forced frozen-execution decision. `hit` is the matched
    /// node with its frozen choice; `key` the lookup key when execution
    /// computed one; `edge` the chosen action's edge index when execution
    /// computed the canonical decision.
    #[allow(
        clippy::too_many_arguments,
        reason = "values already computed by execution"
    )]
    pub(crate) fn eval_decision(
        &mut self,
        s: &FastActorSessionV1,
        d: &FastActorDecisionV1,
        a: u32,
        depth: u32,
        matching: (bool, bool),
        src: EvalSrc,
        root: bool,
        key: Option<&Key>,
        hit: Option<(&Node, Option<usize>)>,
        edge: Option<usize>,
        t: u64,
        focal: PlayerId,
        defs: &SpyDefs,
    ) {
        let menu = menu_bits(s, focal, defs);
        let off = menu.iter().fold(0u8, |x, y| x | y);
        let ch = menu.get(a as usize).copied().unwrap_or(0);
        let node = match (hit, key) {
            (Some(_), Some(k)) => self
                .ids
                .get(k)
                .map(|&i| json!(i))
                .unwrap_or(json!("untraced")),
            _ => Value::Null,
        };
        let mut v = json!({"t":t,"pd":d.physical_decision_id,"ss":[d.substep_index,d.substep_count],
            "dep":depth,"mb":matching.0,"ma":matching.1,"src":src.name(),"root":root,
            "k":d.legal_action_count,"live":a,"edge":edge,"node":node,"off":off,"ch":ch});
        if let Some((_, fc)) = hit {
            v["fc"] = json!(fc);
        }
        if src == EvalSrc::FirstMiss {
            v["miss_key"] = json!(key.map(|k| hex(k)));
        }
        if off != 0 {
            v["menu"] = json!(menu);
            if let Some(sem) = s.diagnostic_current_action_semantics() {
                v["sem"] = serde_json::to_value(sem.get(a as usize)).unwrap_or(Value::Null);
            }
        }
        push(&mut self.cur, "dec", v);
    }

    pub(crate) fn world_end(&mut self, out: Value) {
        let mut m = std::mem::take(&mut self.cur);
        m.insert("out".into(), out);
        self.lines.push(Value::Object(m).to_string());
    }
}
