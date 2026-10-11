//! Branching attribution and depth retention (collab
//! LANES/search-budget-plan-20261011, HANDOFF.md and PILOT.json), mode
//! `s4a-branch`.
//!
//! For each root in `BRANCH_ROOTS` (a stopped-bank source's original anchor,
//! or a named root reached by replaying a saved R guide's actions) and the arm
//! in `BRANCH_ARM`:
//! - B1: the formal search (one new node per natural simulation);
//! - D8: the same search retaining up to eight new focal nonforced choice
//!   nodes per natural simulation along the sampled rollout (`arms.rs`).
//!
//! Selection uses the root's original seed context and allowance. The whole
//! stored tree is then summarized passively (action dispersion, conditional
//! observation fanout, repeated current views, expansion and rollout
//! accounting); the tree is frozen and executed on 16 fresh lawful worlds
//! (`<selection root>/branch-depth-v2/eval`, `EvalWorld`, arm "", path
//! `world`), dispatched through arm label "E", with no refresh and no tree
//! update. Nothing here feeds back into selection, backups or execution.

use super::arms::{Capture, EvalOut, Limits, Roles, SamplerStats, SearchDiag, SimRecord};
use super::bank::{
    bank_roles, decision_identity, guide_ctx, plain_logged, source_identity, source_session, walk,
    BankShared, SourceSpec,
};
use super::canon2::KeyMode;
use super::play::{acting, decision};
use super::seeds::{Purpose, RootSeeds};
use super::tree::{Key, Tree};
use super::world::sample;
use super::{replay_key, sha};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

pub(super) const EVAL_SUFFIX: &str = "/branch-depth-v2/eval";
pub(super) const EVAL_WORLDS: u64 = 16;
pub(super) const EVAL_CAP: u64 = 65_536;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedRoot {
    /// Saved row holding the R guide (`arms.R.actions`, `arms.R.searches`).
    pub(super) row: String,
    /// Pins authenticate the saved producer, independently of the current build.
    pub(super) row_sha256: String,
    pub(super) bank_binding_sha256: String,
    pub(super) search: usize,
    pub(super) step: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootSpec {
    pub(super) root_id: String,
    pub(super) source: String,
    pub(super) budget: u64,
    #[serde(default)]
    pub(super) named: Option<NamedRoot>,
    /// The saved tree hash this root's B1 search must reproduce.
    pub(super) expected_b1_tree: String,
}

/// A (parent key, parent action) link group.
type Group = (Key, Vec<u8>);

/// Authenticate one saved guide before any replay or selection. Older witness-free
/// pilot rows are preserved but cannot qualify this prospective strict consumer.
fn saved_guide(bytes: &[u8], n: &NamedRoot, identity: &Value) -> Result<Value, String> {
    if n.row_sha256.len() != 64 || sha(bytes) != n.row_sha256 {
        return Err("saved row SHA-256 differs".into());
    }
    let row: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if n.bank_binding_sha256.len() != 64 || row["bank_binding_sha256"] != n.bank_binding_sha256 {
        return Err("saved producer binding differs".into());
    }
    for key in [
        "id",
        "lineage",
        "own",
        "other",
        "seat",
        "opp",
        "seed",
        "starting_player",
        "runtime_rules",
        "select_rule",
        "limits",
        "source",
        "anchor",
    ] {
        if identity.get(key).is_none() || row.get(key) != identity.get(key) {
            return Err(format!("saved source/anchor witness differs: {key}"));
        }
    }
    let search = row["arms"]["R"]["searches"]
        .get(n.search)
        .ok_or("missing saved search")?;
    if search["search"].as_u64() != Some(n.search as u64)
        || search["step"].as_u64() != Some(n.step)
        || !search["root"].is_object()
        || search["incomplete"] != false
        || search["faults"].as_array().is_none_or(|v| !v.is_empty())
    {
        return Err("saved search is invalid or lacks root witnesses".into());
    }
    Ok(row)
}

fn replay_prefix(actions: &[u32], step: u64, anchor: u64) -> Result<&[u32], String> {
    let count = step
        .checked_sub(anchor)
        .ok_or("named step precedes anchor")?;
    let count = usize::try_from(count).map_err(|_| "named replay length overflows")?;
    actions
        .get(..count)
        .ok_or_else(|| "named replay exceeds saved actions".into())
}

/// The root position, its selection label and identity witnesses.
fn root_session(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    opp: usize,
    spec: &RootSpec,
    binding: &str,
) -> Result<(FastActorSessionV1, String, Value), String> {
    let focal = PlayerId(src.seat);
    let mut f = policy.fork_for_collection_v3()?;
    let mut o = sh.opponents[opp].fork_for_collection_v3()?;
    let mut s = source_session(&sh.spec, src)?;
    f.reset_sampling_v1(src.source_policy_seeds);
    o.reset_sampling_v1(src.source_policy_seeds);
    let mut actions = Vec::new();
    plain_logged(&mut s, focal, &mut f, &mut o, &mut actions)?;
    let mut eligible = Vec::new();
    walk(source_session(&sh.spec, src)?, &actions, |_, d, _| {
        if acting(d) == focal && d.substep_index == 0 && d.legal_action_count >= 2 {
            eligible.push(d.step);
        }
        Ok(())
    })?;
    if eligible.is_empty() {
        return Err("source has no eligible anchor".into());
    }
    let index = ((u128::from(src.anchor_draw) * eligible.len() as u128) >> 64) as usize;
    let anchor_step = eligible[index];
    let anchor = walk(
        source_session(&sh.spec, src)?,
        &actions[..anchor_step as usize],
        |_, _, _| Ok(()),
    )?;
    let source = source_identity(
        sh,
        src,
        &s,
        &actions,
        &anchor,
        index,
        eligible.len(),
        binding,
    )?;
    let mut saved_root = None;
    let (root, label) = match &spec.named {
        None => (anchor, src.search_root.clone()),
        Some(n) => {
            let bytes = std::fs::read(&n.row).map_err(|e| format!("{}: {e}", n.row))?;
            let row = saved_guide(&bytes, n, &source)?;
            let r = &row["arms"]["R"];
            if r["searches"][n.search]["budget"].as_u64() != Some(spec.budget) {
                return Err("named selection allowance differs from saved search".into());
            }
            saved_root = Some(r["searches"][n.search]["root"].clone());
            let saved: Vec<u32> =
                serde_json::from_value(r["actions"].clone()).map_err(|e| e.to_string())?;
            let root = walk(
                anchor,
                replay_prefix(&saved, n.step, anchor_step)?,
                |_, _, _| Ok(()),
            )?;
            let label = if n.search == 0 {
                src.search_root.clone()
            } else {
                format!("{}/refresh{}", src.search_root, n.search)
            };
            (root, label)
        }
    };
    let d = decision(&root).ok_or("root is terminal")?;
    if acting(&d) != focal || d.legal_action_count < 2 {
        return Err("root is not a focal choice".into());
    }
    if let Some(n) = &spec.named {
        if d.step != n.step {
            return Err("reconstructed root step differs".into());
        }
    }
    if let Some(expected) = saved_root {
        if decision_identity(&root)? != expected {
            return Err("named root public decision witnesses differ".into());
        }
    }
    let (obs, menu) = replay_key(&root, d)?;
    let identity = json!({"anchor_step":anchor_step,"step":d.step,"physical_decision_id":d.physical_decision_id,
        "substep":d.substep_index,"k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
        "state_hash":format!("{:016x}", root.diagnostic_state_hash()),"selection_root":label,
        "source_actions_sha256":sha(&actions.iter().flat_map(|a| a.to_le_bytes()).collect::<Vec<u8>>())});
    let mut identity = identity;
    identity["source_identity"] = source;
    Ok((root, label, identity))
}

fn hex(bytes: &[u8]) -> String {
    sha(bytes)
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// Which visible fields differ between two sibling observations (tags can
/// overlap; residual paths are listed for inspection).
fn diff_tags(a: &Value, b: &Value, actor: usize) -> (Vec<&'static str>, Vec<String>) {
    let mut paths = Vec::new();
    collect_diff(a, b, "", &mut paths, 40);
    let mut tags = HashSet::new();
    for p in &paths {
        let parts: Vec<&str> = p.split('/').collect();
        let tag = match parts.get(1).copied() {
            Some("own_hand") => "own_private",
            Some("known_library_cards") | Some("known_hand_cards") => {
                if parts.get(2).and_then(|x| x.parse::<usize>().ok()) == Some(actor) {
                    "own_private"
                } else {
                    "opponent_known"
                }
            }
            Some("projection") => {
                const PER_SEAT: [&str; 10] = [
                    "life_totals",
                    "mana_pools",
                    "hand_counts",
                    "library_counts",
                    "player_status",
                    "battlefield",
                    "graveyards",
                    "poison_counters",
                    "ninja_emblems",
                    "restricted_mana",
                ];
                match (
                    parts.get(2),
                    parts.get(3).and_then(|x| x.parse::<usize>().ok()),
                ) {
                    (Some(f), Some(i)) if PER_SEAT.contains(f) => {
                        if i == actor {
                            "own_public"
                        } else {
                            "opponent_public"
                        }
                    }
                    _ => "public_other",
                }
            }
            _ => "residual",
        };
        tags.insert(tag);
    }
    let mut tags: Vec<&'static str> = tags.into_iter().collect();
    tags.sort_unstable();
    (tags, paths.into_iter().take(8).collect())
}

fn collect_diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>, limit: usize) {
    if out.len() >= limit || a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                collect_diff(
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                    &format!("{path}/{k}"),
                    out,
                    limit,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                collect_diff(va, vb, &format!("{path}/{i}"), out, limit);
            }
        }
        _ => out.push(path.to_owned()),
    }
}

/// Passive Stage A summary of a whole stored tree.
pub(super) fn tree_report(tree: &Tree, actor: usize) -> Value {
    let root_key = [0u8; 32];
    // Children by (parent, edge); topology depth by walking parents.
    let mut sorted: Vec<(&Key, &super::tree::Node)> = tree.nodes.iter().collect();
    sorted.sort_by_key(|(k, _)| **k);
    let mut children: HashMap<(Key, Vec<u8>), Vec<Key>> = HashMap::new();
    for &(k, n) in &sorted {
        if n.parent != root_key {
            children
                .entry((n.parent, n.edge.clone()))
                .or_default()
                .push(*k);
        }
    }
    let mut topo: HashMap<Key, u32> = HashMap::new();
    fn topo_depth(tree: &Tree, k: &Key, memo: &mut HashMap<Key, u32>) -> u32 {
        if let Some(&d) = memo.get(k) {
            return d;
        }
        let n = &tree.nodes[k];
        let d = if n.parent == [0u8; 32] || !tree.nodes.contains_key(&n.parent) {
            1
        } else {
            topo_depth(tree, &n.parent, memo) + 1
        };
        memo.insert(*k, d);
        d
    }
    let keys: Vec<Key> = tree.nodes.keys().copied().collect();
    for k in &keys {
        topo_depth(tree, k, &mut topo);
    }
    // Per parent node (any node with visits).
    let mut by_depth: BTreeMap<u32, Vec<[f64; 7]>> = BTreeMap::new();
    let (mut unexpanded_backed, mut unexpanded_backups) = (0u64, 0u64);
    let mut parents_with_children = HashSet::new();
    for &(k, n) in &sorted {
        let total: u64 = n.n.iter().sum();
        let legal = n.n.len();
        let tried = n.n.iter().filter(|&&x| x > 0).count();
        let mut child_bearing = 0;
        for (e, edge) in n.canon.edges.iter().enumerate() {
            let has = children.contains_key(&(*k, edge.clone()));
            if has {
                child_bearing += 1;
                parents_with_children.insert(*k);
            } else if n.n[e] > 0 {
                unexpanded_backed += 1;
                unexpanded_backups += n.n[e];
            }
        }
        let eligible =
            n.n.iter()
                .filter(|&&x| x >= super::tree::MIN_EXEC_BACKUPS)
                .count();
        let (top, eff) = if total > 0 {
            let t = total as f64;
            let top = *n.n.iter().max().unwrap_or(&0) as f64 / t;
            let s2: f64 = n.n.iter().map(|&x| (x as f64 / t).powi(2)).sum();
            (top, 1.0 / s2)
        } else {
            (0.0, 0.0)
        };
        by_depth.entry(n.depth).or_default().push([
            legal as f64,
            tried as f64,
            child_bearing as f64,
            eligible as f64,
            top,
            eff,
            n.visits as f64,
        ]);
    }
    let depth_rows: Vec<Value> = by_depth
        .iter()
        .map(|(d, rows)| {
            let col = |i: usize| mean(&rows.iter().map(|r| r[i]).collect::<Vec<_>>());
            json!({"physical_depth":d,"nodes":rows.len(),"mean_legal":col(0),"mean_tried":col(1),
                "mean_child_bearing":col(2),"mean_eligible":col(3),"mean_top_share":col(4),
                "mean_effective_actions":col(5),"visits":rows.iter().map(|r| r[6]).sum::<f64>(),
                "nodes_with_eligible":rows.iter().filter(|r| r[3] > 0.0).count()})
        })
        .collect();
    // Per (parent, action) group: observation and menu fanout.
    let (mut c_links, mut g_groups) = (0usize, 0usize);
    let (mut multi, mut obs_split, mut menu_only_split) = (0usize, 0usize, 0usize);
    let mut fanout_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut tag_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut examples = Vec::new();
    let mut groups: Vec<(&Group, &Vec<Key>)> = children.iter().collect();
    groups.sort_by_key(|(k, v)| (std::cmp::Reverse(v.len()), k.0, k.1.clone()));
    for (_, kids) in &groups {
        g_groups += 1;
        c_links += kids.len();
        *fanout_hist.entry(kids.len().min(20)).or_default() += 1;
        if kids.len() > 1 {
            multi += 1;
            let obs: HashSet<String> = kids.iter().map(|k| hex(&tree.nodes[k].canon.obs)).collect();
            let menus: HashSet<String> = kids
                .iter()
                .map(|k| hex(&tree.nodes[k].canon.edges.concat()))
                .collect();
            if obs.len() > 1 {
                obs_split += 1;
            } else if menus.len() > 1 {
                menu_only_split += 1;
            }
            // Tags from the two most visited children.
            let mut v: Vec<&Key> = kids.iter().collect();
            v.sort_by_key(|k| (std::cmp::Reverse(tree.nodes[*k].visits), **k));
            let (a, b) = (&tree.nodes[v[0]], &tree.nodes[v[1]]);
            let (tags, paths) = diff_tags(
                &serde_json::from_slice(&a.canon.obs).unwrap_or(Value::Null),
                &serde_json::from_slice(&b.canon.obs).unwrap_or(Value::Null),
                actor,
            );
            for t in &tags {
                *tag_counts.entry(t).or_default() += 1;
            }
            if a.canon.edges != b.canon.edges {
                *tag_counts.entry("menu").or_default() += 1;
            }
            if examples.len() < 4 {
                examples.push(json!({"children":kids.len(),"physical_depth":a.depth,
                    "visits":v.iter().take(6).map(|k| tree.nodes[*k].visits).collect::<Vec<_>>(),
                    "tags":tags,"paths":paths,"menu_differs":a.canon.edges != b.canon.edges}));
            }
        }
    }
    let p_parents = parents_with_children.len();
    // Repeated current views across histories (diagnostic only).
    let mut views: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for &(_, n) in &sorted {
        let mut bytes = n.canon.obs.clone();
        for e in &n.canon.edges {
            bytes.extend_from_slice(&(e.len() as u64).to_le_bytes());
            bytes.extend_from_slice(e);
        }
        views.entry(hex(&bytes)).or_default().push(n.visits);
    }
    let repeated: Vec<&Vec<u64>> = views.values().filter(|v| v.len() > 1).collect();
    let max_topo = topo.values().copied().max().unwrap_or(0);
    let max_phys = tree.nodes.values().map(|n| n.depth).max().unwrap_or(0);
    json!({"nodes":tree.nodes.len(),"hash":tree.hash(),"max_physical_depth":max_phys,"max_topology_depth":max_topo,
        "by_physical_depth":depth_rows,
        "links":{"C_child_links":c_links,"P_parents_with_children":p_parents,"G_action_groups_with_children":g_groups,
            "identity_holds":c_links == p_parents + (g_groups - p_parents) + (c_links - g_groups),
            "extra_action_groups":g_groups - p_parents,"extra_observations_within_groups":c_links - g_groups},
        "action_groups":{"with_several_children":multi,"observation_split":obs_split,"menu_only_split":menu_only_split,
            "children_per_group_histogram":fanout_hist,"sibling_difference_tags":tag_counts,"examples":examples},
        "unexpanded_backed_edges":{"edges":unexpanded_backed,"backups":unexpanded_backups},
        "repeated_views":{"distinct_views":views.len(),"views_reached_by_several_histories":repeated.len(),
            "nodes_in_repeated_views":repeated.iter().map(|v| v.len()).sum::<usize>(),
            "max_histories_per_view":repeated.iter().map(|v| v.len()).max().unwrap_or(0),
            "visits_in_repeated_views":repeated.iter().map(|v| v.iter().sum::<u64>()).sum::<u64>()}})
}

/// Accounting schema of `sim_report`. v1 rows used a zero sentinel for "no
/// expansion" (crediting a no-expansion simulation's work to "after
/// expansion") and sampled counters only at loop entry; see the erratum in
/// collab LANES/search-identity-audit-20261011.
pub(super) const SIM_SCHEMA: &str = "branch-sim-accounting-v2";

/// Selection accounting by outcome, in three mutually exclusive work
/// buckets (transitions and inference calls, not CPU): the tree prefix, work
/// after the first expansion (from its applied action on, including any
/// retained tail), and the depth-limit tail of a simulation that never
/// expanded. Pooled sums within one root; `reconciles` checks them against
/// the selection totals.
pub(super) fn sim_report(
    records: &[SimRecord],
    retain: usize,
    selection_transitions: u64,
    selection_calls: u64,
    attempted: u64,
    sampler_rejected: u64,
) -> Value {
    const NAMES: [&str; 6] = [
        "win",
        "nonwin",
        "nonnatural",
        "truncated",
        "fault",
        "sampler_rejection",
    ];
    let bucket_sums = |rs: &[&SimRecord]| {
        let mut s = [[0u64; 2]; 3];
        for r in rs {
            for (b, (t, c)) in r.buckets().iter().enumerate() {
                s[b][0] += t;
                s[b][1] += c;
            }
        }
        s
    };
    let pair = |s: &[[u64; 2]; 3], i: usize| {
        json!({"tree_prefix":s[0][i],"after_expansion":s[1][i],"depth_tail_without_expansion":s[2][i],
            "total":s[0][i] + s[1][i] + s[2][i]})
    };
    let mut by = serde_json::Map::new();
    let mut bucket_ok = true;
    for (i, name) in NAMES.iter().enumerate() {
        let rs: Vec<&SimRecord> = records
            .iter()
            .filter(|r| usize::from(r.outcome) == i + 1)
            .collect();
        let s = bucket_sums(&rs);
        let t: u64 = rs.iter().map(|r| r.transitions).sum();
        let c: u64 = rs.iter().map(|r| r.policy_calls).sum();
        bucket_ok &= s[0][0] + s[1][0] + s[2][0] == t && s[0][1] + s[1][1] + s[2][1] == c;
        let expanded = rs.iter().filter(|r| r.expansion.is_some()).count();
        let depth_only = rs
            .iter()
            .filter(|r| r.expansion.is_none() && r.depth_limit.is_some())
            .count();
        by.insert(
            (*name).into(),
            json!({"count":rs.len(),"expanded":expanded,"not_expanded":rs.len() - expanded,
                "depth_limited_without_expansion":depth_only,"transitions":pair(&s, 0),"calls":pair(&s, 1)}),
        );
    }
    let accepted = records
        .iter()
        .filter(|r| r.outcome != SimRecord::REJECTED)
        .count();
    let rejected = records.len() - accepted;
    let total_t: u64 = records.iter().map(|r| r.transitions).sum();
    let total_c: u64 = records.iter().map(|r| r.policy_calls).sum();
    let natural: Vec<&SimRecord> = records
        .iter()
        .filter(|r| r.outcome == 1 || r.outcome == 2)
        .collect();
    let expanded: Vec<&SimRecord> = natural
        .iter()
        .copied()
        .filter(|r| r.expansion.is_some())
        .collect();
    let mut first_depth: BTreeMap<String, usize> = BTreeMap::new();
    for r in &natural {
        let k = r
            .expansion
            .map_or_else(|| "none".to_owned(), |e| e.depth.to_string());
        *first_depth.entry(k).or_default() += 1;
    }
    let ns = bucket_sums(&natural);
    let share = |b: usize, i: usize| {
        let tot = ns[0][i] + ns[1][i] + ns[2][i];
        (tot > 0).then(|| ns[b][i] as f64 / tot as f64)
    };
    let es = bucket_sums(&expanded);
    let cond = |b: usize, i: usize| {
        (!expanded.is_empty()).then(|| es[b][i] as f64 / expanded.len() as f64)
    };
    let new_total: usize = natural.iter().map(|r| r.new_nodes).sum();
    json!({"schema":SIM_SCHEMA,"retain":retain,
        "attempts":attempted,"accepted":accepted,"sampler_rejections":rejected,
        "by_outcome":by,
        "reconciles":{"attempts_equal_accepted_plus_rejections":attempted == records.len() as u64,
            "transitions_equal_selection_total":total_t == selection_transitions,
            "calls_equal_selection_total":total_c == selection_calls,
            "buckets_sum_to_totals":bucket_ok,
            "snapshots_within_totals":records.iter().all(SimRecord::snapshots_within_totals),
            "rejections_equal_sampler_count":rejected as u64 == sampler_rejected,
            "natural_expansion_iff_committed":natural.iter().all(|r| r.expansion.is_some() == (r.new_nodes > 0))},
        "natural":{"count":natural.len(),"expanded":expanded.len(),"not_expanded":natural.len() - expanded.len(),
            "pooled_share_of_natural_work":{
                "transitions":{"tree_prefix":share(0, 0),"after_expansion":share(1, 0),"depth_tail_without_expansion":share(2, 0)},
                "calls":{"tree_prefix":share(0, 1),"after_expansion":share(1, 1),"depth_tail_without_expansion":share(2, 1)}},
            "mean_among_expanded":{"transitions_tree_prefix":cond(0, 0),"transitions_after_expansion":cond(1, 0),
                "calls_tree_prefix":cond(0, 1),"calls_after_expansion":cond(1, 1)},
            "mean_existing_nodes_traversed":mean(&natural.iter().map(|r| f64::from(r.existing)).collect::<Vec<_>>()),
            "first_expansion_depth_histogram":first_depth,
            "retained_tail_nodes":new_total.saturating_sub(expanded.len()),
            "revisits_to_tail_nodes":natural.iter().map(|r| u64::from(r.staged_hits)).sum::<u64>()}})
}

/// Per-world execution metrics from E's frozen decisions. A group is one
/// physical decision of the focal player with at least one nonforced choice;
/// substeps keep their engine indices.
/// - `L`: leading groups whose every nonforced substep executed a tree edge
///   (a count of physical groups, not substeps).
/// - `first_recorded_choice_*` (v1 `first_substep_*`): groups whose first
///   recorded nonforced choice executed a tree edge; that choice need not be
///   substep zero.
/// - `substep0_*`: among groups with a nonforced choice at substep index 0,
///   those where it executed a tree edge; the share is null with no such group.
/// - `support_after_first_fallback`: structurally zero under exact-history
///   lookup with a permanent miss fallback (expected, not a finding).
pub(super) fn world_metrics(out: &EvalOut) -> Value {
    let mut m = decision_metrics(&out.decisions);
    m["end"] = json!(out.end);
    m["win"] = json!(out.w);
    m["valid"] = json!(!out.unknown);
    m["transitions"] = json!(out.transitions);
    m["spy"] = out.suffix.json();
    m
}

/// The decision-derived part of `world_metrics`.
fn decision_metrics(decisions: &[(u64, u32, u8)]) -> Value {
    let groups = choice_groups(decisions);
    let full = |v: &Vec<(u32, u8)>| v.iter().all(|&(_, s)| s == 0);
    let l = groups.iter().take_while(|(_, v)| full(v)).count();
    let first_rec = groups.iter().filter(|(_, v)| v[0].1 == 0).count();
    let descendants = groups.iter().skip(1).filter(|(_, v)| v[0].1 == 0).count();
    let first_fallback = groups.iter().position(|(_, v)| !full(v));
    let after = first_fallback.map_or(0, |i| {
        groups
            .iter()
            .skip(i + 1)
            .filter(|(_, v)| v[0].1 == 0)
            .count()
    });
    let (zero_groups, zero_controlled) = substep0(&groups);
    let first_of = |code: u8| {
        groups
            .iter()
            .position(|(_, v)| v.iter().any(|&(_, s)| s == code))
    };
    let choices: Vec<[u64; 3]> = groups
        .iter()
        .enumerate()
        .flat_map(|(g, (_, v))| {
            v.iter()
                .map(move |&(sub, s)| [g as u64, u64::from(sub), u64::from(s)])
        })
        .collect();
    json!({"groups":groups.len(),"L":l,
        "first_recorded_choice_controlled":first_rec,
        "first_recorded_choice_share":if groups.is_empty() { 0.0 } else { first_rec as f64 / groups.len() as f64 },
        "substep0_groups":zero_groups,"substep0_controlled":zero_controlled,
        "substep0_share":(zero_groups > 0).then(|| zero_controlled as f64 / zero_groups as f64),
        "descendant_first_recorded_controlled":descendants,"support_after_first_fallback":after,
        "first_unsupported_group":first_of(1),"first_miss_group":first_of(2),
        "choices":choices})
}

/// Focal nonforced choices grouped by physical decision, in order:
/// (physical id, [(substep index, source)]).
fn choice_groups(decisions: &[(u64, u32, u8)]) -> Vec<(u64, Vec<(u32, u8)>)> {
    let mut groups: Vec<(u64, Vec<(u32, u8)>)> = Vec::new();
    for &(phys, sub, src) in decisions {
        match groups.last_mut() {
            Some((p, v)) if *p == phys => v.push((sub, src)),
            _ => groups.push((phys, vec![(sub, src)])),
        }
    }
    groups
}

/// (groups with a nonforced choice at substep 0, of which tree-executed).
fn substep0(groups: &[(u64, Vec<(u32, u8)>)]) -> (usize, usize) {
    let zero: Vec<u8> = groups
        .iter()
        .filter_map(|(_, v)| v.iter().find(|&&(sub, _)| sub == 0).map(|&(_, s)| s))
        .collect();
    (zero.len(), zero.iter().filter(|&&s| s == 0).count())
}

/// Repeat-support readout of one finished search (collab
/// LANES/search-canonical-repair-plan-20261011). Edge counts are natural
/// terminal backups; child visits are not used as edge backups.
pub(super) fn support_report(tree: &Tree, records: &[SimRecord], diag: &SearchDiag) -> Value {
    let min = super::tree::MIN_EXEC_BACKUPS;
    let natural_total = records
        .iter()
        .filter(|r| r.outcome == 1 || r.outcome == 2)
        .count() as u64;
    // Per-edge first wins and what followed.
    let mut cat: BTreeMap<&str, u64> = BTreeMap::new();
    let (mut repeat_losses, mut repeat_wins, mut after_opportunities, mut node_visits_after) =
        (0u64, 0u64, 0u64, 0u64);
    let mut first_win_prior: BTreeMap<u64, u64> = BTreeMap::new();
    let mut by_depth: BTreeMap<u32, [u64; 6]> = BTreeMap::new();
    let mut keys: Vec<&Key> = tree.nodes.keys().collect();
    keys.sort_unstable();
    for k in keys {
        let n = &tree.nodes[k];
        let d = by_depth.entry(n.depth).or_default();
        d[0] += 1;
        d[1] += u64::from(n.n.iter().any(|&x| x >= min));
        for (a, (&b, &w)) in n.n.iter().zip(&n.wins).enumerate() {
            if b == 0 {
                continue;
            }
            d[2] += 1;
            d[3] += u64::from(b >= min);
            d[4] += b;
            d[5] += w;
            let e = diag.edges.get(&(*k, a)).copied().unwrap_or_default();
            let c = match e.first_win {
                None => "never_won",
                Some((ord, prior, visits)) => {
                    *first_win_prior.entry(prior.min(min)).or_default() += 1;
                    after_opportunities += natural_total.saturating_sub(ord + 1);
                    node_visits_after += n.visits.saturating_sub(visits + 1);
                    repeat_losses += e.backups_after - e.wins_after;
                    repeat_wins += e.wins_after;
                    if e.backups_after == 0 {
                        "won_without_later_backup"
                    } else {
                        "won_then_backed_up_again"
                    }
                }
            };
            *cat.entry(c).or_default() += 1;
        }
    }
    let depth: Vec<Value> = by_depth
        .iter()
        .map(|(d, x)| {
            json!({"physical_depth":d,"nodes":x[0],"nodes_with_eligible_edge":x[1],"backed_edges":x[2],
                "eligible_edges":x[3],"backups":x[4],"winning_backups":x[5]})
        })
        .collect();
    // Legacy identities folded into stored nodes.
    let (mut multi, mut multi_ids, mut consolidated, mut newly_eligible) = (0u64, 0u64, 0u64, 0u64);
    let mut legacy_keys: Vec<&Key> = diag.legacy.keys().collect();
    legacy_keys.sort_unstable();
    for k in legacy_keys {
        let ids = &diag.legacy[k];
        if ids.len() < 2 {
            continue;
        }
        multi += 1;
        multi_ids += ids.len() as u64;
        let width = ids.values().map(Vec::len).max().unwrap_or(0);
        for a in 0..width {
            let parts: Vec<u64> = ids
                .values()
                .map(|v| v.get(a).copied().unwrap_or(0))
                .collect();
            let total: u64 = parts.iter().sum();
            let contributors = parts.iter().filter(|&&x| x > 0).count();
            if total >= min && contributors >= 2 {
                if parts.iter().all(|&x| x < min) {
                    newly_eligible += 1;
                } else {
                    consolidated += 1;
                }
            }
        }
    }
    // Contiguous supported prefixes of selection trajectories (natural).
    let nat: Vec<&SimRecord> = records
        .iter()
        .filter(|r| r.outcome == 1 || r.outcome == 2)
        .collect();
    let hist = |f: &dyn Fn(&SimRecord) -> u32| {
        let mut h: BTreeMap<u32, u64> = BTreeMap::new();
        for r in &nat {
            *h.entry(f(r)).or_default() += 1;
        }
        h
    };
    let mean_of = |f: &dyn Fn(&SimRecord) -> u32| {
        if nat.is_empty() {
            0.0
        } else {
            nat.iter().map(|r| f64::from(f(r))).sum::<f64>() / nat.len() as f64
        }
    };
    // Per attempt (index = selection-world index): [outcome, groups, substeps].
    let per_attempt: Vec<[u32; 3]> = records
        .iter()
        .map(|r| {
            [
                u32::from(r.outcome),
                r.supported_groups,
                r.supported_substeps,
            ]
        })
        .collect();
    json!({"eligibility_threshold":min,
        "edges":{"by_category":cat,"repeat_losses_after_first_win":repeat_losses,
            "repeat_wins_after_first_win":repeat_wins,
            "later_natural_simulations_after_first_win":after_opportunities,
            "node_visits_after_first_win":node_visits_after,
            "prior_backups_at_first_win":first_win_prior},
        "by_physical_depth":depth,
        "legacy_identities":{"stored_nodes_with_several":multi,"identities_in_them":multi_ids,
            "edges_consolidated_already_eligible":consolidated,"edges_newly_eligible_only_when_pooled":newly_eligible},
        "selection_trajectories":{"note":"selection diagnostics, not held-out L",
            "natural":nat.len(),
            "mean_supported_groups":mean_of(&|r| r.supported_groups),
            "mean_supported_substeps":mean_of(&|r| r.supported_substeps),
            "supported_groups_histogram":hist(&|r| r.supported_groups),
            "supported_substeps_histogram":hist(&|r| r.supported_substeps),
            "first_unsupported_depth_histogram":hist(&|r| r.first_unsupported_depth),
            "per_attempt":per_attempt}})
}

/// Row schema of `root_arm` (v1 rows: no `schema` field).
pub(super) const ROW_SCHEMA: &str = "branch-root-v2";

/// Diagnostic options of one root run (collab
/// LANES/search-identity-audit-plan-20261011). None changes selection.
#[derive(Clone, Debug, Default)]
pub(super) struct RunOpts {
    /// Run the frozen 16-world evaluation (false: selection only).
    pub(super) evaluate: bool,
    /// Dump the `groups` largest (parent, edge) child groups, every child's
    /// canonical view included.
    pub(super) groups: usize,
    /// Capture raw views for these node keys.
    pub(super) capture_keys: Vec<Key>,
    /// Capture every node at this physical depth or shallower (0: none).
    pub(super) capture_up_to: u32,
    /// Simulations whose opponent decisions keep private snapshots.
    pub(super) snapshot_sims: Vec<u64>,
    /// Captured sibling pairs to probe (`identity.rs`).
    pub(super) probe_pairs: Vec<(Key, Key)>,
    /// E's node key version (legacy by default).
    pub(super) key_mode: Option<KeyMode>,
}

/// Shared limits of all paired engine probes in one run (collab
/// LANES/search-identity-audit-plan-20261011 TARGETS.json).
pub(super) const PROBE_PAIRS: u32 = 32;
pub(super) const PROBE_TRANSITIONS: u32 = 512;

fn key_hex(k: &Key) -> String {
    k.iter().map(|b| format!("{b:02x}")).collect()
}

fn edge_text(e: &[u8]) -> String {
    String::from_utf8_lossy(e).into_owned()
}

/// The `n` largest (parent, edge) child groups with each child's canonical
/// view, visits and creating simulation.
fn group_dump(tree: &Tree, created: &HashMap<Key, u64>, n: usize) -> Vec<Value> {
    let mut children: BTreeMap<(Key, Vec<u8>), Vec<Key>> = BTreeMap::new();
    for (k, node) in &tree.nodes {
        if node.parent != [0u8; 32] {
            children
                .entry((node.parent, node.edge.clone()))
                .or_default()
                .push(*k);
        }
    }
    let mut groups: Vec<(&Group, &Vec<Key>)> = children.iter().collect();
    groups.sort_by_key(|(k, v)| (std::cmp::Reverse(v.len()), k.0, k.1.clone()));
    groups
        .into_iter()
        .take(n)
        .enumerate()
        .map(|(rank, ((parent, edge), kids))| {
            let p = &tree.nodes[parent];
            let mut kids = kids.clone();
            kids.sort_by_key(|k| (std::cmp::Reverse(tree.nodes[k].visits), *k));
            let rows: Vec<Value> = kids
                .iter()
                .map(|k| {
                    let c = &tree.nodes[k];
                    let menu: Vec<u8> = c.canon.edges.concat();
                    json!({"key":key_hex(k),"visits":c.visits,"n":c.n,"wins":c.wins,"physical_depth":c.depth,
                        "obs_sha256":hex(&c.canon.obs),"menu_sha256":hex(&menu),
                        "created_sim":created.get(k),
                        "obs":serde_json::from_slice::<Value>(&c.canon.obs).unwrap_or(Value::Null),
                        "edges":c.canon.edges.iter().map(|e| edge_text(e)).collect::<Vec<_>>()})
                })
                .collect();
            json!({"kind":"group","rank":rank,"parent":key_hex(parent),"edge":edge_text(edge),
                "edge_sha256":hex(edge),"parent_physical_depth":p.depth,"parent_visits":p.visits,
                "parent_edge_backups":p.canon.edges.iter().position(|e| e == edge).map(|i| p.n[i]),
                "parent_obs":serde_json::from_slice::<Value>(&p.canon.obs).unwrap_or(Value::Null),
                "parent_edges":p.canon.edges.iter().map(|e| edge_text(e)).collect::<Vec<_>>(),
                "children":rows})
        })
        .collect()
}

/// Raw captured views, with each node's visible parent/action chain.
fn capture_dump(tree: &Tree, cap: &Capture) -> Vec<Value> {
    cap.committed
        .iter()
        .map(|(k, c)| {
            let chain: Vec<Value> = c
                .path
                .iter()
                .map(|(pk, e)| {
                    let n = &tree.nodes[pk];
                    json!({"key":key_hex(pk),"physical_depth":n.depth,"obs_sha256":hex(&n.canon.obs),
                        "edge_index":e,"edge":edge_text(&n.canon.edges[*e])})
                })
                .collect();
            json!({"kind":"capture","key":key_hex(k),"in_tree":tree.nodes.contains_key(k),"sim":c.sim,
                "step":c.step,"physical_decision_id":c.physical_decision_id,"substep":c.substep,
                "physical_depth":c.depth,"raw_obs":c.raw_obs,"raw_menu":c.raw_menu,
                "canon_menu":c.canon_menu.iter().map(|e| edge_text(e)).collect::<Vec<_>>(),
                "mapping":{"order":c.mapping.order,"min_zcc":c.mapping.min_zcc},
                "chain":chain,"private_snapshots_kept":c.opp_since.len()})
        })
        .collect()
}

/// One root, one arm: selection, passive tree summary, optional frozen
/// evaluation, and optional group and capture dumps (second output).
pub(super) fn root_arm(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    spec: &RootSpec,
    retain: usize,
    binding: &str,
    opts: &RunOpts,
    budget: &mut super::identity::Budget,
) -> Result<(Value, Vec<Value>), String> {
    let started = Instant::now();
    let src = sh
        .spec
        .sources
        .iter()
        .find(|s| s.id == spec.source)
        .ok_or("unknown source")?;
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    let focal = PlayerId(src.seat);
    if !matches!(retain, 1 | 8) || spec.budget == 0 {
        return Err("invalid branch arm or zero selection budget".into());
    }
    let (root, label, identity) = root_session(sh, policy, src, opp, spec, binding)?;
    let reconstruct = started.elapsed().as_secs_f64();
    let mut roles: Roles = bank_roles(policy, &sh.opponents)?;
    let seeds = RootSeeds {
        model: sh.spec.lineage.clone(),
        root: label.clone(),
    };
    let limits = Limits {
        select_cap: spec.budget,
        eval_worlds: EVAL_WORLDS,
        eval_cap: EVAL_CAP,
    };
    let mut ctx = guide_ctx(&root, focal, opp, &seeds, &sh.prior, limits)?;
    let key_mode = opts.key_mode.unwrap_or(KeyMode::Legacy);
    ctx.key_mode = key_mode;
    if key_mode != KeyMode::Legacy
        && (opts.evaluate || retain > 1 || opts.groups > 0 || !opts.capture_keys.is_empty())
    {
        return Err("non-legacy keys run B1 selection only, without dumps".into());
    }
    let t0 = Instant::now();
    let mut records = Vec::new();
    let mut diag = SearchDiag::default();
    let mut cap = Capture::new(opts.capture_keys.iter().copied().collect());
    cap.snapshot_sims = opts.snapshot_sims.iter().copied().collect();
    cap.all_up_to = opts.capture_up_to;
    let (sel, tree) = roles.select_e_diagnosed(
        &ctx,
        None,
        retain,
        Some(&mut records),
        Some(&mut cap),
        Some(&mut diag),
    );
    let selection_wall = t0.elapsed().as_secs_f64();
    let failed = sel.incomplete || !sel.faults.is_empty();
    let tree_hash = tree.hash();
    let t1 = Instant::now();
    let report = tree_report(&tree, src.seat as usize);
    let sims = sim_report(
        &records,
        retain,
        sel.transitions,
        sel.inference,
        sel.attempted,
        sel.sampler.rejected,
    );
    let support = support_report(&tree, &records, &diag);
    let mut dump = group_dump(&tree, &cap.created, opts.groups);
    let captured = cap.committed.len();
    dump.extend(capture_dump(&tree, &cap));
    let share = PROBE_PAIRS / (opts.probe_pairs.len().max(1) as u32);
    for (a, b) in &opts.probe_pairs {
        // An equal share of the shared limits per pair.
        let mut own = super::identity::Budget {
            pairs: share.min(budget.pairs),
            transitions: (2 * share).min(budget.transitions),
        };
        let given = (own.pairs, own.transitions);
        let r = super::identity::probe_pair(&roles, opp, focal, &cap, a, b, &mut own);
        budget.pairs -= given.0 - own.pairs;
        budget.transitions -= given.1 - own.transitions;
        dump.push(json!({"kind":"probe","a":key_hex(a),"b":key_hex(b),
            "result":r.unwrap_or_else(|e| json!({"error":e}))}));
    }
    // Private snapshots never leave the process.
    drop(cap);
    let report_wall = t1.elapsed().as_secs_f64();
    // Frozen evaluation on 16 fresh lawful worlds.
    let eval_seeds = RootSeeds {
        model: sh.spec.lineage.clone(),
        root: format!("{label}{EVAL_SUFFIX}"),
    };
    let ectx = guide_ctx(&root, focal, opp, &eval_seeds, &sh.prior, limits)?;
    let t2 = Instant::now();
    let mut worlds = Vec::new();
    let mut sampler = SamplerStats::default();
    if !failed && opts.evaluate {
        for e in 0..EVAL_WORLDS {
            let seed = eval_seeds.get(Purpose::EvalWorld, "", e, b"world");
            match sample(&root, seed, &sh.prior) {
                Err(err) => worlds
                    .push(json!({"world":e,"rejected":err.chars().take(200).collect::<String>()})),
                Ok(w) => {
                    let state = format!("{:016x}", w.world.diagnostic_state_hash());
                    let out =
                        roles.evaluate(&ectx, "E", &w.world, e, None, Some(&tree), &mut sampler);
                    let mut m = world_metrics(&out);
                    m["world"] = json!(e);
                    m["world_state_hash"] = json!(state);
                    worlds.push(m);
                }
            }
        }
    }
    let eval_wall = t2.elapsed().as_secs_f64();
    let row = json!({"kind":"branch_root","schema":ROW_SCHEMA,"root_id":spec.root_id,"source":spec.source,
        "lineage":sh.spec.lineage,"own":src.own,"seat":src.seat,"arm":if retain == 1 { "B1" } else { "D8" },
        "retain":retain,"budget":spec.budget,"bank_binding_sha256":binding,"root":identity,"selection":sel.json(),
        "selection_failed":failed,
        "named_input":spec.named.as_ref().map(|n| json!({"row_sha256":n.row_sha256,
            "bank_binding_sha256":n.bank_binding_sha256,"search":n.search,"step":n.step})),
        "tree_hash":tree_hash,"expected_b1_tree":spec.expected_b1_tree,
        "b1_parity":(retain == 1 && key_mode == KeyMode::Legacy).then(|| tree_hash == spec.expected_b1_tree),
        "key_mode":key_mode.label(),"keys":diag.keys.json(),"support":support,
        "tree":report,"simulations":sims,"evaluated":opts.evaluate,"evaluation":worlds,
        "dump":{"groups":opts.groups,"capture_keys":opts.capture_keys.len(),"captured":captured,
            "probe_pairs":opts.probe_pairs.len(),"probe_pairs_left":budget.pairs,"probe_transitions_left":budget.transitions},
        "timing":{"note":"elapsed wall seconds, not CPU; process CPU is in the launcher ledger",
            "reconstruct_wall_seconds":reconstruct,"selection_wall_seconds":selection_wall,
            "report_wall_seconds":report_wall,"evaluation_wall_seconds":eval_wall,
            "total_wall_seconds":started.elapsed().as_secs_f64()}});
    Ok((row, dump))
}

#[cfg(test)]
mod tests {
    use super::super::world::DeckPrior;
    use super::*;
    use crate::runtime_decks::RUNTIME_DECKS;

    #[test]
    fn saved_guide_authentication_and_replay_bounds_fail_closed() {
        let identity = json!({"id":"test","lineage":"r1","own":4,"other":5,"seat":0,
            "opp":"other","seed":12,"starting_player":0,"runtime_rules":"rules",
            "select_rule":"rule","limits":{"select_cap":128000},
            "source":{"end":"win","actions_sha256":"source"},"anchor":{"step":50}});
        let mut row = identity.clone();
        row["bank_binding_sha256"] = json!("a".repeat(64));
        row["arms"] = json!({"R":{"actions":[1,2],"searches":[{"search":0,"step":51,
            "root":{"step":51},"incomplete":false,"faults":[]}]}});
        let mut n = NamedRoot {
            row: "unused".into(),
            row_sha256: String::new(),
            bank_binding_sha256: "a".repeat(64),
            search: 0,
            step: 51,
        };
        let bytes = serde_json::to_vec(&row).unwrap();
        n.row_sha256 = sha(&bytes);
        assert!(saved_guide(&bytes, &n, &identity).is_ok());
        let mut bad_bytes = bytes.clone();
        bad_bytes.push(b' ');
        assert!(saved_guide(&bad_bytes, &n, &identity).is_err());
        for key in ["source", "anchor", "bank_binding_sha256"] {
            let mut bad = row.clone();
            bad[key] = Value::Null;
            let bytes = serde_json::to_vec(&bad).unwrap();
            n.row_sha256 = sha(&bytes);
            assert!(saved_guide(&bytes, &n, &identity).is_err(), "{key}");
        }
        let mut bad = row.clone();
        bad["arms"]["R"]["searches"][0]["root"] = Value::Null;
        let bytes = serde_json::to_vec(&bad).unwrap();
        n.row_sha256 = sha(&bytes);
        assert!(saved_guide(&bytes, &n, &identity).is_err());
        let twice = [
            serde_json::to_vec(&row).unwrap(),
            serde_json::to_vec(&row).unwrap(),
        ]
        .concat();
        n.row_sha256 = sha(&twice);
        assert!(saved_guide(&twice, &n, &identity).is_err());
        assert!(replay_prefix(&[1, 2], 49, 50).is_err());
        assert!(replay_prefix(&[1, 2], 53, 50).is_err());
        assert_eq!(replay_prefix(&[1, 2], 51, 50).unwrap(), &[1]);
    }

    fn root() -> FastActorSessionV1 {
        let decks = [&RUNTIME_DECKS[4], &RUNTIME_DECKS[5]];
        let mut s = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1, 21, 100_000, 200_000, decks.map(|d| d.id.to_owned()), decks.map(|d| d.card_ids.to_vec()), PlayerId(0),
        )
        .unwrap();
        s.set_runtime_rules_v1(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
            .unwrap();
        loop {
            let d = decision(&s).unwrap();
            if d.legal_action_count >= 2 && acting(&d) == PlayerId(0) && d.step > 20 {
                return s;
            }
            s.step(d.episode_id, d.step, 0).unwrap();
        }
    }

    fn select(retain: usize, cap: u64) -> (Tree, Vec<SimRecord>, u64) {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut roles = bank_roles(&policy, std::slice::from_ref(&policy)).unwrap();
        let s = root();
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let seeds = RootSeeds {
            model: "test".into(),
            root: "branch".into(),
        };
        let limits = Limits {
            select_cap: cap,
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(&s, PlayerId(0), 0, &seeds, &prior, limits).unwrap();
        let mut records = Vec::new();
        let (sel, tree) = roles.select_e_retaining(&ctx, None, retain, Some(&mut records));
        (tree, records, sel.completed)
    }

    /// The first simulation from an empty tree is the same trajectory, with
    /// the same draws and policy calls, under B1 and D8.
    #[test]
    #[ignore = "native qualification: fixture game and small searches"]
    fn first_simulation_is_unchanged_by_retention() {
        let (_, b1, _) = select(1, 20_000);
        let (_, d8, _) = select(8, 20_000);
        let (a, b) = (b1[0], d8[0]);
        assert_eq!(
            (
                a.outcome,
                a.transitions,
                a.policy_calls,
                a.expansion,
                a.depth_limit
            ),
            (
                b.outcome,
                b.transitions,
                b.policy_calls,
                b.expansion,
                b.depth_limit
            )
        );
    }

    /// Committed nodes are exactly those of natural simulations (at most
    /// eight each under D8, one under B1), every node's visits equal its edge
    /// backups, children never exceed their parent edge's backups, staged
    /// edges bind to their parent's menu, and depth stays within 32.
    #[test]
    #[ignore = "native qualification: fixture game and small searches"]
    fn retention_commits_one_backup_per_traversed_edge() {
        for retain in [1usize, 8] {
            let (tree, records, completed) = select(retain, 30_000);
            let natural: Vec<&SimRecord> = records
                .iter()
                .filter(|r| r.outcome == 1 || r.outcome == 2)
                .collect();
            assert_eq!(natural.len() as u64, completed);
            assert!(natural.iter().all(|r| r.new_nodes <= retain));
            assert!(records
                .iter()
                .filter(|r| r.outcome > 2)
                .all(|r| r.new_nodes == 0));
            assert_eq!(
                tree.nodes.len(),
                natural.iter().map(|r| r.new_nodes).sum::<usize>()
            );
            let root = tree.nodes.values().find(|n| n.parent == [0u8; 32]).unwrap();
            assert_eq!(root.visits, completed);
            for n in tree.nodes.values() {
                assert_eq!(n.visits, n.n.iter().sum::<u64>());
                assert!(n.depth <= super::super::tree::MAX_DEPTH);
                if n.parent != [0u8; 32] {
                    let p = &tree.nodes[&n.parent];
                    let e = p
                        .canon
                        .edges
                        .iter()
                        .position(|x| *x == n.edge)
                        .expect("edge in parent menu");
                    assert!(p.n[e] >= n.visits);
                }
            }
            if retain == 8 {
                assert!(
                    natural.iter().any(|r| r.new_nodes > 1),
                    "D8 retained no tail"
                );
            }
        }
    }

    use super::super::arms::Snap;

    fn rec(
        outcome: u8,
        t: u64,
        c: u64,
        exp: Option<(u64, u64)>,
        lim: Option<(u64, u64)>,
    ) -> SimRecord {
        let snap = |(transitions, calls): (u64, u64)| Snap {
            depth: 3,
            transitions,
            calls,
        };
        SimRecord {
            outcome,
            transitions: t,
            policy_calls: c,
            expansion: exp.map(snap),
            depth_limit: lim.map(snap),
            new_nodes: usize::from(exp.is_some() && outcome <= 2),
            ..SimRecord::default()
        }
    }

    /// Each kind of ending lands in exactly one set of buckets, and the
    /// buckets always sum to the simulation's measured totals.
    #[test]
    fn accounting_buckets_cover_every_ending() {
        // No expansion, ended inside the tree: all prefix.
        let a = rec(1, 40, 5, None, None);
        assert_eq!(a.buckets(), [(40, 5), (0, 0), (0, 0)]);
        // Expansion and natural completion.
        let b = rec(2, 100, 30, Some((12, 4)), None);
        assert_eq!(b.buckets(), [(12, 4), (88, 26), (0, 0)]);
        // Depth-only rollout without expansion.
        let c = rec(2, 90, 20, None, Some((50, 9)));
        assert_eq!(c.buckets(), [(50, 9), (0, 0), (40, 11)]);
        // Expansion followed by truncation: the final work counts after it.
        let d = rec(4, 17, 6, Some((10, 2)), None);
        assert_eq!(d.buckets(), [(10, 2), (7, 4), (0, 0)]);
        // A fault after inference, before any expansion.
        let e = rec(5, 3, 2, None, None);
        assert_eq!(e.buckets(), [(3, 2), (0, 0), (0, 0)]);
        // A sampler rejection did no work.
        let f = rec(SimRecord::REJECTED, 0, 0, None, None);
        assert_eq!(f.buckets(), [(0, 0); 3]);
        let all = [a, b, c, d, e, f];
        let r = sim_report(&all, 1, 250, 63, 6, 1);
        assert_eq!(
            r["reconciles"]["attempts_equal_accepted_plus_rejections"],
            true
        );
        assert_eq!(r["reconciles"]["transitions_equal_selection_total"], true);
        assert_eq!(r["reconciles"]["calls_equal_selection_total"], true);
        assert_eq!(r["reconciles"]["buckets_sum_to_totals"], true);
        assert_eq!(r["accepted"], 5);
        assert_eq!(r["sampler_rejections"], 1);
        assert_eq!(
            r["by_outcome"]["nonwin"]["depth_limited_without_expansion"],
            1
        );
        assert_eq!(
            r["by_outcome"]["truncated"]["transitions"]["after_expansion"],
            7
        );
        assert_eq!(r["natural"]["expanded"], 1);
        assert_eq!(r["natural"]["first_expansion_depth_histogram"]["none"], 2);
        // Natural pooled shares: 230 transitions, of which 102 prefix.
        let share = r["natural"]["pooled_share_of_natural_work"]["transitions"]["tree_prefix"]
            .as_f64()
            .unwrap();
        assert!((share - 102.0 / 230.0).abs() < 1e-12);
        // Conditional means use the expanded natural simulations only.
        assert_eq!(
            r["natural"]["mean_among_expanded"]["transitions_after_expansion"],
            88.0
        );
        // A total that does not match the selection is reported, not hidden.
        let bad = sim_report(&all, 1, 251, 63, 7, 0);
        assert_eq!(
            bad["reconciles"]["transitions_equal_selection_total"],
            false
        );
        assert_eq!(
            bad["reconciles"]["attempts_equal_accepted_plus_rejections"],
            false
        );
    }

    /// L counts physical groups; the legacy first-recorded-choice metric and
    /// the actual substep-zero metric differ when substep 0 was forced.
    #[test]
    fn group_and_substep_semantics() {
        // Group 7: substep 0 forced (absent), substep 1 tree-executed.
        // Group 8: substeps 0 and 1 tree-executed. Group 9: substep 0 miss.
        let d = [(7, 1, 0), (8, 0, 0), (8, 1, 0), (9, 0, 2), (9, 1, 3)];
        let m = decision_metrics(&d);
        assert_eq!(m["groups"], 3);
        assert_eq!(m["L"], 2);
        assert_eq!(m["first_recorded_choice_controlled"], 2);
        assert_eq!(m["substep0_groups"], 2);
        assert_eq!(m["substep0_controlled"], 1);
        assert_eq!(m["substep0_share"], 0.5);
        assert_eq!(m["first_miss_group"], 2);
        assert_eq!(m["choices"][0], json!([0, 1, 0]));
        // Only a forced substep zero: no qualifying group, share undefined.
        let f = decision_metrics(&[(4, 2, 0)]);
        assert_eq!(f["substep0_groups"], 0);
        assert_eq!(f["substep0_share"], Value::Null);
        assert_eq!(f["first_recorded_choice_controlled"], 1);
    }

    /// Real selection: every ending is reconciled, and an expansion is
    /// recorded exactly when the simulation added a node or was cut after one.
    #[test]
    #[ignore = "native qualification: fixture game and small searches"]
    fn selection_accounting_reconciles() {
        let policy = FrozenPlayPolicyV1::training_fixture_v3();
        let mut roles = bank_roles(&policy, std::slice::from_ref(&policy)).unwrap();
        let s = root();
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let seeds = RootSeeds {
            model: "test".into(),
            root: "branch".into(),
        };
        let limits = Limits {
            select_cap: 20_000,
            ..Limits::FORMAL
        };
        let ctx = guide_ctx(&s, PlayerId(0), 0, &seeds, &prior, limits).unwrap();
        for retain in [1usize, 8] {
            let mut records = Vec::new();
            let (sel, tree) = roles.select_e_retaining(&ctx, None, retain, Some(&mut records));
            let r = sim_report(
                &records,
                retain,
                sel.transitions,
                sel.inference,
                sel.attempted,
                sel.sampler.rejected,
            );
            for k in [
                "attempts_equal_accepted_plus_rejections",
                "transitions_equal_selection_total",
                "calls_equal_selection_total",
                "buckets_sum_to_totals",
                "snapshots_within_totals",
                "rejections_equal_sampler_count",
                "natural_expansion_iff_committed",
            ] {
                assert_eq!(r["reconciles"][k], true, "{k} retain {retain}");
            }
            for x in records.iter().filter(|x| x.outcome <= 2) {
                assert_eq!(x.expansion.is_some(), x.new_nodes > 0);
            }
            assert_eq!(
                records.iter().map(|x| x.new_nodes).sum::<usize>(),
                tree.nodes.len()
            );
        }
    }

    /// Both arms' fresh evaluation worlds come from the same derivation: a
    /// regenerated world has the identical private starting state.
    #[test]
    #[ignore = "native qualification: fixture game"]
    fn evaluation_worlds_regenerate_identically() {
        let s = root();
        let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
        let seeds = RootSeeds {
            model: "test".into(),
            root: format!("branch{EVAL_SUFFIX}"),
        };
        let mut seen = 0;
        for e in 0..EVAL_WORLDS {
            let seed = seeds.get(Purpose::EvalWorld, "", e, b"world");
            if let (Ok(a), Ok(b)) = (sample(&s, seed, &prior), sample(&s, seed, &prior)) {
                assert_eq!(
                    a.world.diagnostic_state_hash(),
                    b.world.diagnostic_state_hash()
                );
                seen += 1;
            }
        }
        assert!(seen > 0);
    }
}
