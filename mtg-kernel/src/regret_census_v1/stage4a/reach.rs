//! RS0, forward reach measurement (mode `s4a-reach`; collab
//! LANES/reverse-search-20261029/RS0-PLAN.md sections 4 to 6). Measurement
//! only: nothing that chooses, samples, trains or selects reads these rows.
//!
//! Each root's label job is rebuilt exactly as `s4a-win-check` rebuilds it
//! and checked against its record up to the root. At the root, unmodified E
//! search runs as EX1 enters it (`guide_ctx`, SELF roles from `bank_roles`,
//! the unconditioned registered-deck prior, default search options), with a
//! passive trace and per-simulation records, once per budget and seed.
//!
//! Environment: `REACH_ROOTS` (jsonl from `tools/reach/plan.py`),
//! `REACH_BUDGETS` (comma list of selection transitions), `REACH_SEEDS`
//! (comma list), optional `REACH_ROLES` (comma list of `stratum:role`; only
//! roots carrying one of them are searched), `REACH_SHARD` (`i/n`: roots
//! whose position mod n is i), `REACH_LIMIT` (at most this many roots after
//! the filters), `REACH_IDENTITY=1` (instead of measuring, run each root
//! plain and traced at the first budget and seed and compare). The planner
//! writes selected roots only; replacements come from re-planning with the
//! refusals (`tools/reach/plan.py --refused`).

use super::arms::{Limits, SimRecord};
use super::bank::{bank_roles, guide_ctx};
use super::canon2::KeyMode;
use super::diag::Trace;
use super::labels::spy_defs;
use super::play::{acting, apply, decision, Meter};
use super::refresh::{Cursor, Lookup};
use super::replay_labels::LabelJob;
use super::seeds::RootSeeds;
use super::tree::{Key, Tree};
use super::world::DeckPrior;
use crate::ex1_v1::{ex1_qhat_v1, ex1_target_v1, Ex1EdgeCountsV1, EX1_SUPPORT_MIN_V1};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

/// RootSeeds' namespace for every RS0 search.
pub(super) const REACH_SEED_MODEL_V1: &str = "rs0-reach-v1";
/// Each search's thread stack (EX1's).
const REACH_STACK_BYTES_V1: usize = 256 << 20;
/// Trace label bit of the Spy self-target choice (`diag::bits`).
const SELF_TARGET_BIT: u8 = 2;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleSpec {
    stratum: String,
    role: String,
    rank: usize,
    selected: bool,
    opportunity_turn: Option<u32>,
    certified_index: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReachRoot {
    root_id: String,
    game_id: String,
    index: usize,
    turn: u32,
    roles: Vec<RoleSpec>,
    job: LabelJob,
}

fn env_list(name: &str) -> Result<Vec<u64>, String> {
    std::env::var(name)
        .map_err(|_| format!("{name} is required"))?
        .split(',')
        .filter(|x| !x.trim().is_empty())
        .map(|x| {
            x.trim()
                .parse()
                .map_err(|_| format!("{name} must list integers"))
        })
        .collect()
}

/// The root session rebuilt from its job, and what the replay saw.
struct Rebuilt {
    root: FastActorSessionV1,
    focal: PlayerId,
    /// Recorded decisions from the root up to (excluding) the certified
    /// decision: all, focal nonforced, opponent nonforced.
    prefix: Option<(u64, u64, u64)>,
    /// Product of the policy's probabilities of the recorded focal nonforced
    /// actions from the root up to (excluding) the certified decision.
    witness: Option<f64>,
}

/// The student's probabilities from its deployed logits, on the same pinned
/// softmax `ex1-target-v1` uses.
fn softmax(logits: &[f32]) -> Result<Vec<f64>, String> {
    let z: Vec<f64> = logits.iter().map(|&x| f64::from(x)).collect();
    crate::line_b_teacher_target_v1::line_b_softmax_v1(&z)
        .map(|s| s.probabilities)
        .map_err(|e| format!("rs0 softmax: {e}"))
}

/// Rebuilds the job to `root.index`, checking every decision against its
/// record (step, physical decision, actor, menu size), as `s4a-win-check`.
fn rebuild(r: &ReachRoot, scorer: &mut FrozenPlayPolicyV1) -> Result<Rebuilt, String> {
    let job = &r.job;
    let mut s = crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )?;
    let focal = PlayerId(job.focal);
    let defs = spy_defs();
    let mut meter = Meter::new(u64::MAX);
    let certified = r.roles.iter().find_map(|x| x.certified_index);
    let wants_witness = r.roles.iter().any(|x| x.role == "k0");
    let end = certified.unwrap_or(r.index).max(r.index);
    let mut root = None;
    let (mut all, mut own, mut opp) = (0u64, 0u64, 0u64);
    let mut witness = 1.0f64;
    for (i, rec) in job.decisions.iter().enumerate().take(end + 1) {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        if d.step != rec.step
            || d.physical_decision_id != rec.physical_decision_id
            || acting(&d).0 != rec.actor
            || d.legal_action_count != rec.legal_action_count
        {
            return Err(format!("replay differs from the record at decision {i}"));
        }
        if i == r.index {
            if acting(&d) != focal
                || d.legal_action_count < 2
                || s.game_state().active_player != focal
            {
                return Err(
                    "the root is not a focal decision with two or more actions in the hero's turn"
                        .into(),
                );
            }
            if s.game_state().turn != r.turn {
                return Err("the root's turn differs from the planner's".into());
            }
            root = Some(s.clone());
        }
        if i == end {
            break;
        }
        if i >= r.index {
            all += 1;
            if d.legal_action_count >= 2 {
                if acting(&d) == focal {
                    own += 1;
                    if wants_witness {
                        let scores = scorer.score_fast_session_v1(&s)?;
                        let p = softmax(&scores.logits)?;
                        witness *= p.get(rec.selected as usize).copied().unwrap_or(0.0);
                    }
                } else {
                    opp += 1;
                }
            }
        }
        apply(&mut s, &d, rec.selected, &mut meter, focal, None, &defs)
            .map_err(|e| format!("{e:?}"))?;
    }
    let root = root.ok_or("the job ends before the root")?;
    let has_prefix = certified.is_some_and(|c| c >= r.index);
    Ok(Rebuilt {
        root,
        focal,
        prefix: has_prefix.then_some((all, own, opp)),
        witness: (has_prefix && wants_witness).then_some(witness),
    })
}

/// One search's result (the Selection, the tree and the passive records).
struct Searched {
    sel: super::arms::Selection,
    tree: Tree,
    trace: Option<Trace>,
    records: Vec<SimRecord>,
    cpu: f64,
    wall: f64,
}

fn search(
    policy: &FrozenPlayPolicyV1,
    rb: &Rebuilt,
    root_id: &str,
    budget: u64,
    seed: u64,
    traced: bool,
) -> Result<Searched, String> {
    let opponent = policy.fork_for_collection_v3()?;
    let mut roles = bank_roles(policy, std::slice::from_ref(&opponent))?;
    let prior = DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>());
    let seeds = RootSeeds {
        model: REACH_SEED_MODEL_V1.into(),
        root: format!("rs0/{root_id}/s{seed}"),
    };
    let limits = Limits {
        select_cap: budget,
        ..Limits::FORMAL
    };
    let ctx = guide_ctx(&rb.root, rb.focal, 0, &seeds, &prior, limits)?;
    let cpu0 = super::process_cpu::seconds().unwrap_or(f64::NAN);
    let started = Instant::now();
    let mut trace = traced.then(Trace::default);
    let mut records = Vec::new();
    let (sel, tree) = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("rs0-search".into())
            .stack_size(REACH_STACK_BYTES_V1)
            .spawn_scoped(scope, || {
                if traced {
                    roles.select_e_retaining(&ctx, trace.as_mut(), 1, Some(&mut records))
                } else {
                    roles.select_e(&ctx)
                }
            })
            .map_err(|e| format!("rs0 search thread: {e}"))?
            .join()
            .map_err(|_| "rs0 search thread panicked".to_string())
    })?;
    let wall = started.elapsed().as_secs_f64();
    let cpu = super::process_cpu::seconds().unwrap_or(f64::NAN) - cpu0;
    Ok(Searched {
        sel,
        tree,
        trace,
        records,
        cpu,
        wall,
    })
}

/// A deterministic digest of every node's key and counts.
fn tree_digest(tree: &Tree) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    let sorted: BTreeMap<&Key, _> = tree.nodes.iter().collect();
    for (k, n) in sorted {
        h.update(k);
        h.update(n.visits.to_le_bytes());
        for v in [&n.n, &n.wins, &n.draws] {
            for x in v.iter() {
                h.update(x.to_le_bytes());
            }
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_hex_key(text: &str) -> Option<Key> {
    let mut k = [0u8; 32];
    if text.len() != 64 {
        return None;
    }
    for (i, slot) in k.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(k)
}

/// Per-edge trace labels by node key, from the trace's node lines.
fn node_labels(trace: &Trace) -> HashMap<Key, Vec<u8>> {
    let mut out = HashMap::new();
    for line in &trace.lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["r"] != "node" {
            continue;
        }
        let (Some(k), Some(lab)) = (
            v["key"].as_str().and_then(parse_hex_key),
            v["lab"].as_array(),
        ) else {
            continue;
        };
        out.insert(
            k,
            lab.iter().map(|x| x.as_u64().unwrap_or(0) as u8).collect(),
        );
    }
    out
}

/// O3, the supported self-target Q-lead (RS0-PLAN.md section 5): nodes with
/// a self-target edge and at least 8 visits, how many of them have two or
/// more supported edges, and how many of those have a supported self-target
/// edge whose Q-hat is strictly above every other supported edge's.
fn o3(tree: &Tree, labels: &HashMap<Key, Vec<u8>>) -> Value {
    let (mut offered, mut supported_nodes, mut lead) = (0u64, 0u64, 0u64);
    let mut best_depth: Option<u32> = None;
    for (k, n) in &tree.nodes {
        let Some(lab) = labels.get(k) else {
            continue;
        };
        let selfs: Vec<usize> = (0..n.n.len())
            .filter(|&e| lab.get(e).is_some_and(|b| b & SELF_TARGET_BIT != 0))
            .collect();
        if selfs.is_empty() || n.visits < EX1_SUPPORT_MIN_V1 {
            continue;
        }
        offered += 1;
        let support: Vec<usize> = (0..n.n.len())
            .filter(|&e| n.n[e] >= EX1_SUPPORT_MIN_V1)
            .collect();
        if support.len() < 2 {
            continue;
        }
        supported_nodes += 1;
        let q = |e: usize| {
            ex1_qhat_v1(&Ex1EdgeCountsV1 {
                n: n.n[e],
                wins: n.wins[e],
                draws: n.draws[e],
            })
        };
        let leads = selfs
            .iter()
            .any(|&s| support.contains(&s) && support.iter().all(|&o| o == s || q(s) > q(o)));
        if leads {
            lead += 1;
            best_depth = Some(best_depth.map_or(n.depth, |d| d.min(n.depth)));
        }
    }
    json!({"self_target_nodes_8_visits": offered, "with_two_supported_edges": supported_nodes,
        "self_target_q_lead": lead, "found": lead > 0, "shallowest_lead_depth": best_depth})
}

/// One simulation's trace summary.
struct SimEnd {
    end: String,
    /// Turn of the simulation's latest focal nonforced decision (tree or tail).
    last_turn: Option<u32>,
    /// (trace node id, edge index) of the simulation's first tree choice.
    first: Option<(u64, usize)>,
}

/// Per-simulation trace summaries in simulation order, and the root node's
/// trace id (the depth-1 node without a parent).
fn sim_ends(trace: &Trace) -> (Vec<SimEnd>, Option<u64>) {
    let mut ends = Vec::new();
    let mut root = None;
    for v in trace
        .lines
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        if v["r"] == "node" && v["parent"].is_null() && v["dep"] == 1 {
            root = v["id"].as_u64();
        }
        if v["r"] != "sel" {
            continue;
        }
        let first = v["path"]
            .get(0)
            .and_then(|p| Some((p.get(0)?.as_u64()?, p.get(1)?.as_u64()? as usize)));
        ends.push(SimEnd {
            end: v["end"].as_str().unwrap_or("").to_owned(),
            last_turn: v["lt"].as_u64().map(|t| t as u32),
            first,
        });
    }
    (ends, root)
}

/// O2: `ex1-target-v1` at the root, with live-order counts from the refresh
/// cursor (as EX1 maps them) and the policy's logits at the root.
fn o2(
    tree: Tree,
    rb: &Rebuilt,
    scorer: &mut FrozenPlayPolicyV1,
    found_edge: Option<usize>,
) -> Result<Value, String> {
    let d = decision(&rb.root).ok_or("the root is terminal")?;
    let mut cursor = Cursor::new(tree, 0, KeyMode::Legacy, false);
    let (lookup, hit) = cursor.lookup(&rb.root, d)?;
    let node = match (&lookup, &hit) {
        (Lookup::Miss | Lookup::DepthLimit, _) => None,
        (_, Some(h)) => h.key.and_then(|k| cursor.tree.nodes.get(&k)),
        _ => None,
    };
    let Some(node) = node else {
        return Ok(json!({"valid": false, "reason": "no_root_node", "lookup": lookup.reason()}));
    };
    let h = hit.as_ref().ok_or("a matched node has a hit")?;
    let width = d.legal_action_count as usize;
    if node.n.len() != width {
        return Err("rs0: the root node's menu differs from the live menu".into());
    }
    let mut counts = vec![None; width];
    for e in 0..width {
        let live = cursor.live(h, e)? as usize;
        let slot = counts
            .get_mut(live)
            .ok_or("rs0: a node edge maps outside the live menu")?;
        if slot.is_some() {
            return Err("rs0: two node edges map to one live action".into());
        }
        *slot = Some(Ex1EdgeCountsV1 {
            n: node.n[e],
            wins: node.wins[e],
            draws: node.draws[e],
        });
    }
    let counts: Vec<Ex1EdgeCountsV1> = counts.into_iter().map(|c| c.expect("bijection")).collect();
    // The first winning simulation's root edge, in live order.
    let found = match found_edge {
        Some(e) if e < width => Some(cursor.live(h, e)? as usize),
        _ => None,
    };
    let scores = scorer.score_fast_session_v1(&rb.root)?;
    let bits: Vec<u32> = scores.logits.iter().map(|z| z.to_bits()).collect();
    // Most visited: highest n, then the lowest live index.
    let top = (0..width)
        .max_by_key(|&a| (counts[a].n, std::cmp::Reverse(a)))
        .unwrap_or(0);
    let q = |a: usize| (counts[a].n > 0).then(|| ex1_qhat_v1(&counts[a]));
    let strict_lead = |e: usize| {
        counts[e].n >= EX1_SUPPORT_MIN_V1
            && (0..width).all(|a| a == e || counts[a].n < EX1_SUPPORT_MIN_V1 || q(e) > q(a))
    };
    let edges: Vec<Value> = counts
        .iter()
        .map(|c| json!([c.n, c.wins, c.draws]))
        .collect();
    let mut row = match ex1_target_v1(&bits, &counts)? {
        Ok(t) => {
            let at = |e: Option<usize>| {
                e.map(|e| {
                    json!({"edge": e, "q": t.target.probabilities[e], "p": t.student.probabilities[e],
                        "lift": t.target.probabilities[e] - t.student.probabilities[e]})
                })
            };
            json!({"valid": true, "mass": t.mass, "tv": t.total_variation_v1(), "support": t.support,
                "found": at(found), "top": at(Some(top))})
        }
        Err(reason) => json!({"valid": false, "reason": reason.as_str()}),
    };
    row["edges"] = json!(edges);
    row["found_edge"] = json!(found);
    row["found_strict_q_lead"] = json!(found.map(strict_lead));
    row["top_edge"] = json!(top);
    row["top_strict_q_lead"] = json!(strict_lead(top));
    Ok(row)
}

fn measure(
    policy: &FrozenPlayPolicyV1,
    scorer: &mut FrozenPlayPolicyV1,
    r: &ReachRoot,
    rb: &Rebuilt,
    budget: u64,
    seed: u64,
) -> Result<Value, String> {
    let s = search(policy, rb, &r.root_id, budget, seed, true)?;
    let trace = s.trace.as_ref().ok_or("traced search has no trace")?;
    let labels = node_labels(trace);
    let o3 = o3(&s.tree, &labels);
    let (ends, root_trace_id) = sim_ends(trace);
    let opportunity_turn = r.roles.iter().find_map(|x| x.opportunity_turn);
    let wins = ends.iter().filter(|x| x.end == "win").count();
    let wins_in_t = opportunity_turn.map(|t| {
        ends.iter()
            .filter(|x| x.end == "win" && x.last_turn == Some(t))
            .count()
    });
    let mut win_turns: BTreeMap<String, u64> = BTreeMap::new();
    for x in ends.iter().filter(|x| x.end == "win") {
        let rel = x.last_turn.map_or("none".to_owned(), |t| {
            (i64::from(t) - i64::from(r.turn)).to_string()
        });
        *win_turns.entry(rel).or_default() += 1;
    }
    // The first winning simulation (simulation order) whose first tree
    // choice was at the root: its root edge (node edge order).
    let found_edge = ends
        .iter()
        .find(|x| x.end == "win" && x.first.is_some_and(|(id, _)| Some(id) == root_trace_id))
        .and_then(|x| x.first.map(|(_, e)| e));
    let failed = s.sel.incomplete || !s.sel.faults.is_empty();
    let depth_limited = s.records.iter().filter(|x| x.depth_limit.is_some()).count();
    let digest = tree_digest(&s.tree);
    let disc = &s.sel.discovery;
    // A failed search gives EX1 no tree (ex1_search.rs), so no target.
    let o2 = if failed {
        json!({"valid": false, "reason": "failed_search"})
    } else {
        o2(s.tree, rb, scorer, found_edge)?
    };
    // What this row measured, so a re-plan never reuses it under other roles.
    let roles: Vec<Value> = r
        .roles
        .iter()
        .map(|x| json!({"stratum": x.stratum, "role": x.role}))
        .collect();
    let certified_index = r.roles.iter().find_map(|x| x.certified_index);
    Ok(
        json!({"kind": "search", "root_id": r.root_id, "budget": budget, "seed": seed, "failed": failed,
        "roles": roles, "opportunity_turn": opportunity_turn, "certified_index": certified_index,
        "selection": s.sel.json(), "cpu_seconds": s.cpu, "wall": s.wall, "tree_digest": digest,
        "simulations_recorded": s.records.len(), "depth_limited_simulations": depth_limited,
        "o1_spy": disc.completion_wins > 0, "spy_completion_wins": disc.completion_wins,
        "spy_completions_natural": disc.completions_natural, "self_chosen_simulations": disc.self_chosen,
        "hero_wins": wins, "o1_burn": wins_in_t.map(|w| w > 0), "hero_wins_latest_nonforced_in_t": wins_in_t,
        "hero_wins_by_latest_nonforced_turn_minus_root": win_turns, "o2": o2, "o3": o3}),
    )
}

fn identity(
    policy: &FrozenPlayPolicyV1,
    r: &ReachRoot,
    rb: &Rebuilt,
    budget: u64,
    seed: u64,
) -> Result<Value, String> {
    let plain = search(policy, rb, &r.root_id, budget, seed, false)?;
    let traced = search(policy, rb, &r.root_id, budget, seed, true)?;
    let (a, b) = (plain.sel.json(), traced.sel.json());
    let (da, db) = (tree_digest(&plain.tree), tree_digest(&traced.tree));
    let same = a == b && da == db;
    Ok(
        json!({"kind": "identity", "root_id": r.root_id, "budget": budget, "seed": seed,
        "identical": same, "selection_equal": a == b, "tree_equal": da == db,
        "plain_digest": da, "traced_digest": db, "plain_cpu": plain.cpu, "traced_cpu": traced.cpu}),
    )
}

pub(super) fn run(
    cfg: &super::super::CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
) -> Result<(), String> {
    use std::io::Write;
    let path = std::env::var("REACH_ROOTS").map_err(|_| "REACH_ROOTS is required")?;
    let budgets = env_list("REACH_BUDGETS")?;
    let seeds = env_list("REACH_SEEDS")?;
    if budgets.is_empty() || seeds.is_empty() {
        return Err("REACH_BUDGETS and REACH_SEEDS must be non-empty".into());
    }
    let roles: Option<Vec<String>> = std::env::var("REACH_ROLES").ok().map(|t| {
        t.split(',')
            .map(|x| x.trim().to_owned())
            .filter(|x| !x.is_empty())
            .collect()
    });
    let limit: Option<usize> = match std::env::var("REACH_LIMIT") {
        Ok(t) => Some(t.parse().map_err(|_| "REACH_LIMIT must be an integer")?),
        Err(_) => None,
    };
    let mut taken = 0usize;
    let identity_mode = std::env::var("REACH_IDENTITY").is_ok_and(|v| v == "1");
    let (shard, shards) = match std::env::var("REACH_SHARD") {
        Ok(t) => {
            let (i, n) = t.split_once('/').ok_or("REACH_SHARD must be i/n")?;
            let (i, n): (usize, usize) = (
                i.parse().map_err(|_| "REACH_SHARD must be i/n")?,
                n.parse().map_err(|_| "REACH_SHARD must be i/n")?,
            );
            if n == 0 || i >= n {
                return Err("REACH_SHARD must be i/n with i < n".into());
            }
            (i, n)
        }
        Err(_) => (0, 1),
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut out =
        std::io::BufWriter::new(std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?);
    let mut scorer = policy.fork_for_collection_v3()?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    for (pos, line) in lines.iter().enumerate() {
        if pos % shards != shard {
            continue;
        }
        let r: ReachRoot = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        if r.game_id != r.job.id {
            return Err(format!("{}: root game id differs from its job", r.root_id));
        }
        if !r.roles.iter().any(|x| x.selected) {
            continue;
        }
        if let Some(want) = &roles {
            if !r
                .roles
                .iter()
                .any(|x| want.contains(&format!("{}:{}", x.stratum, x.role)))
            {
                continue;
            }
        }
        if limit.is_some_and(|n| taken >= n) {
            break;
        }
        taken += 1;
        let roles_json: Vec<Value> = r
            .roles
            .iter()
            .map(|x| json!({"stratum": x.stratum, "role": x.role, "rank": x.rank, "selected": x.selected}))
            .collect();
        let rb = match rebuild(&r, &mut scorer) {
            Ok(rb) => rb,
            Err(e) => {
                let row = json!({"kind": "error", "root_id": r.root_id, "game_id": r.game_id, "roles": roles_json, "error": e});
                writeln!(out, "{row}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
                continue;
            }
        };
        let state_hash = format!("{:016x}", rb.root.diagnostic_state_hash());
        let row = json!({"kind": "root", "root_id": r.root_id, "game_id": r.game_id, "index": r.index,
            "turn": r.turn, "roles": roles_json, "opportunity_turn": r.roles.iter().find_map(|x| x.opportunity_turn),
            "certified_index": r.roles.iter().find_map(|x| x.certified_index), "state_hash": state_hash,
            "prefix_decisions": rb.prefix.map(|p| p.0), "prefix_focal_nonforced": rb.prefix.map(|p| p.1),
            "prefix_opponent_nonforced": rb.prefix.map(|p| p.2), "prefix_witness_probability": rb.witness});
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
        if identity_mode {
            let row = identity(policy, &r, &rb, budgets[0], seeds[0])
                .unwrap_or_else(|e| json!({"kind": "error", "root_id": r.root_id, "error": e}));
            writeln!(out, "{row}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
            continue;
        }
        for &budget in &budgets {
            for &seed in &seeds {
                let row = measure(policy, &mut scorer, &r, &rb, budget, seed).unwrap_or_else(
                    |e| json!({"kind": "error", "root_id": r.root_id, "budget": budget, "seed": seed, "error": e}),
                );
                writeln!(out, "{row}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_keys_round_trip() {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i * 7) as u8;
        }
        let text: String = k.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(parse_hex_key(&text), Some(k));
        assert_eq!(parse_hex_key("00"), None);
        assert_eq!(parse_hex_key(&"zz".repeat(32)), None);
    }

    #[test]
    fn softmax_sums_to_one() {
        let p = softmax(&[1.0, 2.0, 3.0]).unwrap();
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(p[2] > p[1] && p[1] > p[0]);
    }

    fn node(n: Vec<u64>, wins: Vec<u64>, visits: u64) -> super::super::tree::Node {
        let k = n.len();
        let canon = super::super::tree::Canon {
            obs: Vec::new(),
            menu: (0..k).map(|i| vec![i as u8]).collect(),
            edges: (0..k).map(|i| vec![i as u8]).collect(),
        };
        let mut node =
            super::super::tree::Node::with_perm([0u8; 32], Vec::new(), canon, 1, (0..k).collect());
        node.n = n;
        node.wins = wins;
        node.draws = vec![0; k];
        node.visits = visits;
        node
    }

    #[test]
    fn o3_requires_a_supported_strict_self_target_lead() {
        let key = [1u8; 32];
        let mut labels = HashMap::new();
        labels.insert(key, vec![0, SELF_TARGET_BIT, 0]);
        let mut tree = Tree::default();
        // Self-target edge 1 supported and strictly best.
        tree.nodes
            .insert(key, node(vec![8, 10, 3], vec![2, 9, 3], 21));
        assert_eq!(o3(&tree, &labels)["self_target_q_lead"], 1);
        // Tied with another supported edge: no strict lead.
        tree.nodes
            .insert(key, node(vec![10, 10, 0], vec![9, 9, 0], 20));
        assert_eq!(o3(&tree, &labels)["self_target_q_lead"], 0);
        // Only the self-target edge supported: not two supported edges.
        tree.nodes
            .insert(key, node(vec![1, 10, 1], vec![0, 9, 0], 12));
        let v = o3(&tree, &labels);
        assert_eq!(v["with_two_supported_edges"], 0);
        assert_eq!(v["self_target_q_lead"], 0);
        // Fewer than 8 visits: not counted.
        tree.nodes
            .insert(key, node(vec![3, 4, 0], vec![0, 4, 0], 7));
        assert_eq!(o3(&tree, &labels)["self_target_nodes_8_visits"], 0);
    }
}
