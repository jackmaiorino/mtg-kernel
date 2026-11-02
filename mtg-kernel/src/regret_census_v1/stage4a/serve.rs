//! `s4a-serve`: the W1 search entrant's search server (collab
//! LANES/lead/W1-ENTRANT-DESIGN.md v7, sections 2 and 4).
//!
//! A pool process (`mode=s4a-serve`) keeps K persistent single-threaded
//! worker processes (`mode=s4a-serve-worker`, the same executable) and reads
//! one search request per stdin line: a root id, the budget B, the fixed
//! charge a_units and an `x_kernel_infoset_v1` payload. Tree k gets
//! floor(B/K) + (1 if k < B mod K) native transitions and seeds
//! RootSeeds(`w1-entrant-v1`, root id, k), tree 0 being the single-tree seed.
//! Each worker imports the payload and runs unmodified s4a E search; the pool
//! merges the trees' root counts in live-action order (integer sums in tree
//! order), picks with `pick-v2` and answers with a receipt. The charge is the
//! units all trees used plus a_units; it never reads a clock.

use super::arms::Limits;
use super::bank::{bank_roles, guide_ctx};
use super::canon2::KeyMode;
use super::play::decision;
use super::refresh::{Cursor, Lookup};
use super::seeds::RootSeeds;
use super::world::DeckPrior;
use crate::infoset_v1::PayloadV1;
use crate::rl_session::FastActorSessionV1;
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};
use std::time::Instant;

pub(super) const SERVE_MODE: &str = "s4a-serve";
pub(super) const WORKER_MODE: &str = "s4a-serve-worker";
/// RootSeeds' namespace for every W1 entrant search.
pub(crate) const SERVE_SEED_MODEL_V1: &str = "w1-entrant-v1";
const SERVE_STACK_BYTES_V1: usize = 256 << 20;
/// Prefix of a pool error that leaves a worker pipe out of step: the pool
/// answers it and exits instead of serving another search.
const FATAL: &str = "fatal: ";

/// Tree k's share of budget B over K trees.
pub(crate) fn partition_v1(budget: u64, k: u64, trees: u64) -> u64 {
    budget / trees + u64::from(k < budget % trees)
}

/// RootSeeds' root string of tree k; tree 0 is the single-tree seed.
pub(crate) fn tree_root_v1(root_id: &str, k: u64) -> String {
    if k == 0 {
        root_id.to_string()
    } else {
        format!("{root_id}/tree{k}")
    }
}

/// Section 2's spending rule, with a_units reserved first: the budget of the
/// next search, or None for a policy answer (allowance-limited).
#[allow(
    dead_code,
    reason = "the entrant bot calls it once the bridge port lands"
)]
pub(crate) fn spend_v1(
    budget: u64,
    a_units: u64,
    game_remaining: u64,
    group_remaining: Option<u64>,
) -> Option<u64> {
    let remaining = group_remaining.map_or(game_remaining, |g| g.min(game_remaining));
    let avail = remaining.checked_sub(a_units)?;
    (avail.saturating_mul(4) >= budget).then(|| budget.min(avail))
}

/// One edge's integer counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CountsV1 {
    pub(crate) n: u64,
    pub(crate) wins: u64,
    pub(crate) draws: u64,
}

/// One tree's result, in live-action order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct TreeResultV1 {
    pub(crate) tree: u64,
    pub(crate) units_budget: u64,
    pub(crate) menu_sha256: String,
    /// Root counts by live index; None when the tree has no root node.
    pub(crate) edges: Option<Vec<CountsV1>>,
    pub(crate) transitions: u64,
    pub(crate) incomplete: bool,
    pub(crate) cap_reached: bool,
    pub(crate) faults: Vec<String>,
    pub(crate) digest: String,
    pub(crate) wall_seconds: f64,
    pub(crate) cpu_seconds: f64,
}

/// The live menu's hash at the imported root.
pub(crate) fn menu_sha256(s: &FastActorSessionV1) -> Result<String, String> {
    let sem = s
        .diagnostic_current_action_semantics()
        .ok_or("the root has no action semantics")?;
    let bytes = serde_json::to_vec(&sem).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn opponent_deck_index(payload: &PayloadV1) -> Result<usize, String> {
    let opp = &payload.deck_ids[usize::from(1 - payload.viewer)];
    RUNTIME_DECKS
        .iter()
        .position(|d| d.id == opp)
        .ok_or_else(|| format!("deck {opp} is not registered"))
}

/// Runs tree k on an imported root.
pub(crate) fn search_tree_v1(
    policy: &FrozenPlayPolicyV1,
    root: &FastActorSessionV1,
    payload: &PayloadV1,
    root_id: &str,
    k: u64,
    units: u64,
) -> Result<TreeResultV1, String> {
    let focal = crate::ids::PlayerId(payload.viewer);
    let prior = DeckPrior::new(&[opponent_deck_index(payload)?]);
    let opponent = policy.fork_for_collection_v3()?;
    let mut roles = bank_roles(policy, std::slice::from_ref(&opponent))?;
    let seeds = RootSeeds {
        model: SERVE_SEED_MODEL_V1.into(),
        root: tree_root_v1(root_id, k),
    };
    let limits = Limits {
        select_cap: units,
        ..Limits::FORMAL
    };
    let menu = menu_sha256(root)?;
    let ctx = guide_ctx(root, focal, 0, &seeds, &prior, limits)?;
    let cpu0 = super::process_cpu::seconds().unwrap_or(f64::NAN);
    let started = Instant::now();
    let (sel, tree) = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("w1-tree".into())
            .stack_size(SERVE_STACK_BYTES_V1)
            .spawn_scoped(scope, || roles.select_e(&ctx))
            .map_err(|e| format!("tree thread: {e}"))?
            .join()
            .map_err(|_| "tree thread panicked".to_string())
    })?;
    let wall_seconds = started.elapsed().as_secs_f64();
    let cpu_seconds = super::process_cpu::seconds().unwrap_or(f64::NAN) - cpu0;
    // Mapping the root after the search can fail; the tree is then excluded
    // as a fault, but its spent transitions stay charged.
    let mapped = (|| -> Result<Option<Vec<CountsV1>>, String> {
        let d = decision(root).ok_or("the root is terminal")?;
        let width = d.legal_action_count as usize;
        let mut cursor = Cursor::new(tree, 0, KeyMode::Legacy, false);
        let (lookup, hit) = cursor.lookup(root, d)?;
        Ok(match (&lookup, &hit) {
            (Lookup::Miss | Lookup::DepthLimit, _) => None,
            (_, Some(h)) => match h.key.and_then(|key| cursor.tree.nodes.get(&key)) {
                None => None,
                Some(node) => {
                    if node.n.len() != width {
                        return Err("the root node's menu differs from the live menu".into());
                    }
                    let mut counts: Vec<Option<CountsV1>> = vec![None; width];
                    for e in 0..width {
                        let live = cursor.live(h, e)? as usize;
                        let slot = counts
                            .get_mut(live)
                            .ok_or("an edge maps outside the menu")?;
                        if slot.is_some() {
                            return Err("two edges map to one live action".into());
                        }
                        *slot = Some(CountsV1 {
                            n: node.n[e],
                            wins: node.wins[e],
                            draws: node.draws[e],
                        });
                    }
                    Some(counts.into_iter().map(|c| c.expect("bijection")).collect())
                }
            },
            _ => None,
        })
    })();
    let mut faults = sel.faults.clone();
    let edges = mapped.unwrap_or_else(|e| {
        faults.push(format!("root mapping: {e}"));
        None
    });
    let mut result = TreeResultV1 {
        tree: k,
        units_budget: units,
        menu_sha256: menu,
        edges,
        transitions: sel.transitions,
        incomplete: sel.incomplete,
        cap_reached: sel.cap_reached,
        faults,
        digest: String::new(),
        wall_seconds,
        cpu_seconds,
    };
    result.digest = tree_result_digest(&result);
    Ok(result)
}

/// Digest of a tree's deterministic fields (no clocks).
pub(crate) fn tree_result_digest(r: &TreeResultV1) -> String {
    let v = json!({"tree": r.tree, "units_budget": r.units_budget, "menu": r.menu_sha256,
        "edges": r.edges, "transitions": r.transitions, "incomplete": r.incomplete,
        "cap_reached": r.cap_reached, "faults": r.faults});
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&v).expect("json"))
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PickStageV1 {
    Visits,
    Q,
    Index,
}

/// `pick-v2`: most merged visits; then higher merged Q = (wins + draws/2)/n,
/// compared exactly; then the lowest live index.
pub(crate) fn pick_v2(merged: &[CountsV1]) -> Option<(usize, PickStageV1)> {
    let best_n = merged.iter().map(|c| c.n).max()?;
    let top: Vec<usize> = (0..merged.len())
        .filter(|&i| merged[i].n == best_n)
        .collect();
    if top.len() == 1 {
        return Some((top[0], PickStageV1::Visits));
    }
    if best_n == 0 {
        return Some((top[0], PickStageV1::Index));
    }
    // Same n: compare 2 wins + draws directly.
    let score = |i: usize| 2 * u128::from(merged[i].wins) + u128::from(merged[i].draws);
    let best = top.iter().map(|&i| score(i)).max()?;
    let q_top: Vec<usize> = top.into_iter().filter(|&i| score(i) == best).collect();
    if q_top.len() == 1 {
        Some((q_top[0], PickStageV1::Q))
    } else {
        Some((q_top[0], PickStageV1::Index))
    }
}

/// The merged search over K trees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct MergedV1 {
    pub(crate) root_id: String,
    pub(crate) budget: u64,
    pub(crate) a_units: u64,
    pub(crate) payload_sha256: String,
    pub(crate) menu_sha256: String,
    pub(crate) merged: Vec<CountsV1>,
    pub(crate) pick: Option<usize>,
    pub(crate) stage: Option<PickStageV1>,
    /// Units used by all trees, failed attempts included.
    pub(crate) units: u64,
    /// units + a_units.
    pub(crate) charge: u64,
    pub(crate) excluded: Vec<u64>,
    pub(crate) incomplete: bool,
    pub(crate) tree_digests: Vec<String>,
}

/// Merges trees in tree order. A tree with no root or a differing menu is
/// excluded and counted; the search is incomplete only if every tree failed.
pub(crate) fn merge_v1(
    root_id: &str,
    budget: u64,
    a_units: u64,
    payload_sha256: &str,
    menu: &str,
    width: usize,
    trees: &[TreeResultV1],
) -> MergedV1 {
    let mut merged = vec![CountsV1::default(); width];
    let mut excluded = Vec::new();
    let mut units = 0u64;
    for t in trees {
        units += t.transitions;
        match &t.edges {
            Some(edges) if t.menu_sha256 == menu && edges.len() == width => {
                for (m, e) in merged.iter_mut().zip(edges) {
                    m.n += e.n;
                    m.wins += e.wins;
                    m.draws += e.draws;
                }
            }
            _ => excluded.push(t.tree),
        }
    }
    let incomplete = excluded.len() == trees.len();
    let picked = if incomplete { None } else { pick_v2(&merged) };
    MergedV1 {
        root_id: root_id.to_string(),
        budget,
        a_units,
        payload_sha256: payload_sha256.to_string(),
        menu_sha256: menu.to_string(),
        merged,
        pick: picked.map(|p| p.0),
        stage: picked.map(|p| p.1),
        units,
        charge: units + a_units,
        excluded,
        incomplete,
        tree_digests: trees.iter().map(|t| t.digest.clone()).collect(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeRequestV1 {
    root_id: String,
    tree: u64,
    units: u64,
    payload: PayloadV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchRequestV1 {
    root_id: String,
    budget: u64,
    a_units: u64,
    payload: PayloadV1,
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

/// Worker: one tree per request line, one result per answer line.
pub(super) fn run_worker(policy: &FrozenPlayPolicyV1) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        // Echo the request's identity so the pool can match every reply.
        let id = serde_json::from_str::<Value>(&line).ok();
        let (root_id, tree) = match &id {
            Some(v) => (v["root_id"].clone(), v["tree"].clone()),
            None => (Value::Null, Value::Null),
        };
        let answer = (|| -> Result<Value, String> {
            let req: TreeRequestV1 =
                serde_json::from_str(&line).map_err(|e| format!("request: {e}"))?;
            let root = req.payload.import()?;
            let r = search_tree_v1(
                policy,
                &root,
                &req.payload,
                &req.root_id,
                req.tree,
                req.units,
            )?;
            Ok(json!({"ok": true, "root_id": req.root_id, "tree": req.tree, "result": r}))
        })()
        .unwrap_or_else(|e| json!({"ok": false, "root_id": root_id, "tree": tree, "error": e}));
        writeln!(out, "{answer}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A tree whose worker reported an error (a panic, a lost result): excluded,
/// recorded with its fault and debited its whole partition, since the work
/// it spent before failing cannot be recovered (section 2 charges failed
/// attempts).
fn failed_tree(tree: u64, units: u64, error: String) -> TreeResultV1 {
    let mut r = TreeResultV1 {
        tree,
        units_budget: units,
        menu_sha256: String::new(),
        edges: None,
        transitions: units,
        incomplete: true,
        cap_reached: false,
        faults: vec![error],
        digest: String::new(),
        wall_seconds: 0.0,
        cpu_seconds: 0.0,
    };
    r.digest = tree_result_digest(&r);
    r
}

struct WorkerV1 {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

fn spawn_worker(source: &str) -> Result<WorkerV1, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(exe)
        .args([
            format!("source={source}"),
            format!("mode={WORKER_MODE}"),
            "out=-".into(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn worker: {e}"))?;
    let stdin = child.stdin.take().ok_or("worker stdin")?;
    let stdout = std::io::BufReader::new(child.stdout.take().ok_or("worker stdout")?);
    Ok(WorkerV1 {
        child,
        stdin,
        stdout,
    })
}

/// Pool: K persistent workers (`W1_SERVE_K`), one merged search per request.
pub(super) fn run_pool(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    let k: u64 = std::env::var("W1_SERVE_K")
        .map_err(|_| "W1_SERVE_K is required")?
        .parse()
        .map_err(|_| "W1_SERVE_K must be an integer")?;
    if k == 0 {
        return Err("W1_SERVE_K must be positive".into());
    }
    let mut workers: Vec<WorkerV1> = (0..k)
        .map(|_| spawn_worker(&cfg.source))
        .collect::<Result<_, _>>()?;
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let started = Instant::now();
        let answer = (|| -> Result<Value, String> {
            let req: SearchRequestV1 =
                serde_json::from_str(&line).map_err(|e| format!("request: {e}"))?;
            // The receipt names the payload by the digest import recomputes.
            let root = req.payload.import()?;
            let menu = menu_sha256(&root)?;
            let width = decision(&root)
                .ok_or("the root is terminal")?
                .legal_action_count as usize;
            for (t, w) in workers.iter_mut().enumerate() {
                let tree_req = json!({"root_id": req.root_id, "tree": t,
                    "units": partition_v1(req.budget, t as u64, k), "payload": req.payload});
                writeln!(w.stdin, "{tree_req}").map_err(|e| format!("{FATAL}worker {t}: {e}"))?;
                w.stdin
                    .flush()
                    .map_err(|e| format!("{FATAL}worker {t}: {e}"))?;
            }
            // Every dispatched request is answered before the next search:
            // a tree error never leaves a stale reply in a worker's pipe.
            let mut trees = Vec::with_capacity(workers.len());
            for (t, w) in workers.iter_mut().enumerate() {
                let units = partition_v1(req.budget, t as u64, k);
                let mut reply = String::new();
                w.stdout
                    .read_line(&mut reply)
                    .map_err(|e| format!("{FATAL}worker {t}: {e}"))?;
                let v: Value = serde_json::from_str(&reply)
                    .map_err(|e| format!("{FATAL}worker {t} reply: {e}"))?;
                if v["root_id"] != json!(req.root_id) || v["tree"] != json!(t) {
                    return Err(format!("{FATAL}worker {t} answered another request"));
                }
                if v["ok"] != json!(true) {
                    trees.push(failed_tree(t as u64, units, v["error"].to_string()));
                    continue;
                }
                match serde_json::from_value::<TreeResultV1>(v["result"].clone()) {
                    Ok(r) if r.tree == t as u64 && r.units_budget == units => trees.push(r),
                    Ok(_) => return Err(format!("{FATAL}worker {t} answered another tree")),
                    Err(e) => trees.push(failed_tree(t as u64, units, e.to_string())),
                }
            }
            let merged = merge_v1(
                &req.root_id,
                req.budget,
                req.a_units,
                &req.payload.sha256,
                &menu,
                width,
                &trees,
            );
            Ok(
                json!({"ok": true, "search": merged, "host": host_name(), "k": k,
                "trees": trees, "wall_seconds": started.elapsed().as_secs_f64()}),
            )
        })()
        .unwrap_or_else(|e| json!({"ok": false, "error": e}));
        writeln!(out, "{answer}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        if let Some(e) = answer["error"].as_str().filter(|e| e.starts_with(FATAL)) {
            return Err(e.to_string());
        }
    }
    for mut w in workers {
        drop(w.stdin);
        let _ = w.child.wait();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: u64, wins: u64, draws: u64) -> CountsV1 {
        CountsV1 { n, wins, draws }
    }

    #[test]
    fn partition_gives_the_remainder_to_the_first_trees() {
        let shares: Vec<u64> = (0..4).map(|k| partition_v1(16_386, k, 4)).collect();
        assert_eq!(shares, vec![4097, 4097, 4096, 4096]);
        assert_eq!(shares.iter().sum::<u64>(), 16_386);
        assert_eq!(partition_v1(4096, 0, 1), 4096);
        assert_eq!(tree_root_v1("g/1/7/0", 0), "g/1/7/0");
        assert_eq!(tree_root_v1("g/1/7/0", 2), "g/1/7/0/tree2");
    }

    #[test]
    fn pick_v2_stages() {
        assert_eq!(
            pick_v2(&[c(3, 1, 0), c(5, 0, 0)]),
            Some((1, PickStageV1::Visits))
        );
        // Equal visits: Q = (wins + draws / 2) / n, exact.
        assert_eq!(
            pick_v2(&[c(4, 1, 1), c(4, 1, 2)]),
            Some((1, PickStageV1::Q))
        );
        assert_eq!(
            pick_v2(&[c(4, 2, 0), c(4, 1, 2)]),
            Some((0, PickStageV1::Index))
        );
        assert_eq!(
            pick_v2(&[c(0, 0, 0), c(0, 0, 0)]),
            Some((0, PickStageV1::Index))
        );
        assert_eq!(pick_v2(&[]), None);
    }

    #[test]
    fn spend_reserves_the_fixed_charge_before_the_threshold() {
        // B = 4000, a_units = 300.
        assert_eq!(spend_v1(4000, 300, 10_000, None), Some(4000));
        assert_eq!(spend_v1(4000, 300, 2_300, None), Some(2000));
        assert_eq!(spend_v1(4000, 300, 1_300, None), Some(1000), "exactly B/4");
        assert_eq!(spend_v1(4000, 300, 1_299, None), None);
        assert_eq!(spend_v1(4000, 300, 200, None), None);
        // The group remainder binds when smaller.
        assert_eq!(spend_v1(4000, 300, 100_000, Some(2_300)), Some(2000));
    }

    fn tree(k: u64, edges: Option<Vec<CountsV1>>, menu: &str, t: u64) -> TreeResultV1 {
        let mut r = TreeResultV1 {
            tree: k,
            units_budget: 10,
            menu_sha256: menu.into(),
            edges,
            transitions: t,
            incomplete: false,
            cap_reached: false,
            faults: vec![],
            digest: String::new(),
            wall_seconds: 0.0,
            cpu_seconds: 0.0,
        };
        r.digest = tree_result_digest(&r);
        r
    }

    #[test]
    fn merge_sums_in_tree_order_and_excludes_faulted_trees() {
        let trees = vec![
            tree(0, Some(vec![c(2, 1, 0), c(1, 0, 1)]), "m", 7),
            tree(1, None, "m", 3),
            tree(2, Some(vec![c(1, 1, 0), c(2, 2, 0)]), "x", 5),
            tree(3, Some(vec![c(0, 0, 0), c(2, 1, 0)]), "m", 4),
        ];
        let m = merge_v1("r", 40, 9, "p", "m", 2, &trees);
        assert_eq!(m.merged, vec![c(2, 1, 0), c(3, 1, 1)]);
        assert_eq!(m.excluded, vec![1, 2]);
        assert_eq!(m.units, 19, "failed attempts are charged");
        assert_eq!(m.charge, 28);
        assert_eq!(m.pick, Some(1));
        assert!(!m.incomplete);
        let none = merge_v1("r", 40, 9, "p", "m", 2, &trees[1..2]);
        assert!(none.incomplete && none.pick.is_none());
    }
}
