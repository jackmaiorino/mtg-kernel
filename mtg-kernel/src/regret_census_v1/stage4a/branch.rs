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

use super::arms::{EvalOut, Limits, Roles, SamplerStats, SimRecord};
use super::bank::{
    bank_roles, guide_ctx, plain_logged, source_session, walk, BankShared, SourceSpec,
};
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

/// The root position, its selection label and identity witnesses.
fn root_session(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    opp: usize,
    spec: &RootSpec,
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
    let (root, label) = match &spec.named {
        None => (anchor, src.search_root.clone()),
        Some(n) => {
            let text = std::fs::read_to_string(&n.row).map_err(|e| format!("{}: {e}", n.row))?;
            let row: Value = serde_json::from_str(text.lines().next().ok_or("empty saved row")?)
                .map_err(|e| e.to_string())?;
            if row["id"] != src.id {
                return Err("saved row is for another source".into());
            }
            let r = &row["arms"]["R"];
            if r["searches"][n.search]["step"].as_u64() != Some(n.step) {
                return Err("saved search does not start at the named step".into());
            }
            let saved: Vec<u32> =
                serde_json::from_value(r["actions"].clone()).map_err(|e| e.to_string())?;
            let k = (n.step - anchor_step) as usize;
            let root = walk(anchor, &saved[..k], |_, _, _| Ok(()))?;
            (root, format!("{}/refresh{}", src.search_root, n.search))
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
    let (obs, menu) = replay_key(&root, d)?;
    let identity = json!({"anchor_step":anchor_step,"step":d.step,"physical_decision_id":d.physical_decision_id,
        "substep":d.substep_index,"k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
        "state_hash":format!("{:016x}", root.diagnostic_state_hash()),"selection_root":label,
        "source_actions_sha256":sha(&actions.iter().flat_map(|a| a.to_le_bytes()).collect::<Vec<u8>>())});
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
    let mut groups: Vec<(&(Key, Vec<u8>), &Vec<Key>)> = children.iter().collect();
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

pub(super) fn sim_report(records: &[SimRecord], retain: usize) -> Value {
    let mut outcomes: BTreeMap<u8, usize> = BTreeMap::new();
    let mut first_depth: BTreeMap<u32, usize> = BTreeMap::new();
    let natural: Vec<&SimRecord> = records
        .iter()
        .filter(|r| r.outcome == 1 || r.outcome == 2)
        .collect();
    for r in records {
        *outcomes.entry(r.outcome).or_default() += 1;
    }
    for r in &natural {
        *first_depth.entry(r.first_depth).or_default() += 1;
    }
    let f = |g: &dyn Fn(&SimRecord) -> f64| mean(&natural.iter().map(|r| g(r)).collect::<Vec<_>>());
    let new_total: usize = natural.iter().map(|r| r.new_nodes).sum();
    let expanded = natural.iter().filter(|r| r.first_depth > 0).count();
    json!({"retain":retain,"attempts":records.len(),
        "outcomes":{"win":outcomes.get(&1).copied().unwrap_or(0),"nonwin":outcomes.get(&2).copied().unwrap_or(0),
            "nonnatural":outcomes.get(&3).copied().unwrap_or(0),"truncated":outcomes.get(&4).copied().unwrap_or(0),
            "fault":outcomes.get(&5).copied().unwrap_or(0)},
        "natural":{"count":natural.len(),"mean_transitions":f(&|r| r.transitions as f64),
            "mean_transitions_before_first_expansion":f(&|r| r.before.0 as f64),
            "mean_transitions_after_first_expansion":f(&|r| (r.transitions - r.before.0) as f64),
            "mean_policy_calls_before":f(&|r| r.before.1 as f64),
            "mean_policy_calls_after":f(&|r| (r.policy_calls - r.before.1) as f64),
            "mean_existing_nodes_traversed":f(&|r| r.existing as f64),
            "first_expansion_depth_histogram":first_depth,
            "first_expansions":expanded,"retained_tail_nodes":new_total.saturating_sub(expanded),
            "revisits_to_tail_nodes":natural.iter().map(|r| r.staged_hits as u64).sum::<u64>()}})
}

/// Per-world execution metrics from E's frozen decisions.
pub(super) fn world_metrics(out: &EvalOut) -> Value {
    // Groups of focal choices in order: (physical id, [sources]).
    let mut groups: Vec<(u64, Vec<u8>)> = Vec::new();
    for &(phys, _sub, src) in &out.decisions {
        match groups.last_mut() {
            Some((p, v)) if *p == phys => v.push(src),
            _ => groups.push((phys, vec![src])),
        }
    }
    let full = |v: &Vec<u8>| v.iter().all(|&s| s == 0);
    let l = groups.iter().take_while(|(_, v)| full(v)).count();
    let first_sub = groups.iter().filter(|(_, v)| v[0] == 0).count();
    let descendants = groups.iter().skip(1).filter(|(_, v)| v[0] == 0).count();
    let first_fallback = groups.iter().position(|(_, v)| !full(v));
    let after = first_fallback.map_or(0, |i| {
        groups.iter().skip(i + 1).filter(|(_, v)| v[0] == 0).count()
    });
    let first_of = |code: u8| groups.iter().position(|(_, v)| v.contains(&code));
    json!({"end":out.end,"win":out.w,"valid":!out.unknown,"transitions":out.transitions,
        "groups":groups.len(),"L":l,"first_substep_controlled":first_sub,
        "first_substep_share":if groups.is_empty() { 0.0 } else { first_sub as f64 / groups.len() as f64 },
        "descendant_controlled":descendants,"support_after_first_fallback":after,
        "first_unsupported_group":first_of(1),"first_miss_group":first_of(2),
        "spy":out.suffix.json()})
}

/// One root, one arm: selection, passive tree summary, frozen evaluation.
pub(super) fn root_arm(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    spec: &RootSpec,
    retain: usize,
) -> Result<Value, String> {
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
    let (root, label, identity) = root_session(sh, policy, src, opp, spec)?;
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
    let ctx = guide_ctx(&root, focal, opp, &seeds, &sh.prior, limits)?;
    let t0 = Instant::now();
    let mut records = Vec::new();
    let (sel, tree) = roles.select_e_retaining(&ctx, None, retain, Some(&mut records));
    let selection_wall = t0.elapsed().as_secs_f64();
    let failed = sel.incomplete || !sel.faults.is_empty();
    let tree_hash = tree.hash();
    let t1 = Instant::now();
    let report = tree_report(&tree, src.seat as usize);
    let sims = sim_report(&records, retain);
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
    if !failed {
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
    Ok(
        json!({"kind":"branch_root","root_id":spec.root_id,"source":spec.source,"lineage":sh.spec.lineage,
        "own":src.own,"seat":src.seat,"arm":if retain == 1 { "B1" } else { "D8" },"retain":retain,
        "budget":spec.budget,"root":identity,"selection":sel.json(),"selection_failed":failed,
        "tree_hash":tree_hash,"expected_b1_tree":spec.expected_b1_tree,
        "b1_parity":(retain == 1).then(|| tree_hash == spec.expected_b1_tree),
        "tree":report,"simulations":sims,"evaluation":worlds,
        "timing":{"reconstruct":reconstruct,"selection":selection_wall,"report":report_wall,"evaluation":eval_wall,
            "total":started.elapsed().as_secs_f64()}}),
    )
}

#[cfg(test)]
mod tests {
    use super::super::world::DeckPrior;
    use super::*;
    use crate::runtime_decks::RUNTIME_DECKS;

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
                a.before,
                a.first_depth
            ),
            (
                b.outcome,
                b.transitions,
                b.policy_calls,
                b.before,
                b.first_depth
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
