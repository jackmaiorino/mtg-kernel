//! A/B/C comparison with independent fresh-world frozen-tree evaluation.
//! Optional fixed-action/plain evaluation is a secondary conditional-policy diagnostic.

use super::arms::{Limits, SamplerStats, SearchDiag};
use super::bank::{
    bank_roles, guide_ctx, plain_logged, source_session, walk, BankShared, SourceSpec,
};
use super::canon2::{KeyMode, KeyStats, Keyer};
use super::play::{acting, decision};
use super::root_allocation::{RootAllocation, SearchOptions};
use super::seeds::{Purpose, RootSeeds};
use super::tree::{canon, live_index, Found, Tree, MIN_EXEC_BACKUPS};
use crate::ids::PlayerId;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Instant;

struct Config {
    arm: String,
    seed: String,
    options: SearchOptions,
    eval_worlds: u64,
    evaluate: bool,
    frozen_continuation: bool,
    eval_cap: u64,
    interchange: bool,
    eval_only: Option<String>,
}

fn count(name: &str, default: u64) -> Result<u64, String> {
    match std::env::var(name) {
        Ok(v) => v
            .parse::<u64>()
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| format!("{name} must be a positive count")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(e) => Err(e.to_string()),
    }
}

impl Config {
    fn load() -> Result<Self, String> {
        let arm = std::env::var("PROFILE_ARM").map_err(|_| "PROFILE_ARM is required")?;
        let allocation = match arm.as_str() {
            "A" | "B" => RootAllocation::Uct,
            "C" => RootAllocation::Halving,
            _ => return Err("PROFILE_ARM must be A, B or C".into()),
        };
        let seed = std::env::var("PROFILE_SEED").map_err(|_| "PROFILE_SEED is required")?;
        if seed.is_empty() {
            return Err("PROFILE_SEED must not be empty".into());
        }
        let cpu_seconds = std::env::var("PROFILE_CPU_SECONDS")
            .ok()
            .map(|v| {
                v.parse::<f64>()
                    .ok()
                    .filter(|s| s.is_finite() && *s > 0.0)
                    .ok_or("PROFILE_CPU_SECONDS must be positive and finite".to_owned())
            })
            .transpose()?;
        let work_limit = std::env::var("PROFILE_WORK_LIMIT")
            .ok()
            .map(|v| {
                v.parse::<u64>()
                    .map_err(|_| "PROFILE_WORK_LIMIT must be a nonnegative count".to_owned())
            })
            .transpose()?;
        if cpu_seconds.is_some() && work_limit.is_some() {
            return Err("CPU calibration and deterministic work replay are separate modes".into());
        }
        let interchange = match std::env::var("PROFILE_INTERCHANGE").as_deref() {
            Ok("1") => true,
            Ok("0") | Err(_) => false,
            _ => return Err("PROFILE_INTERCHANGE must be 0 or 1".into()),
        };
        let evaluate = match std::env::var("PROFILE_EVALUATE").as_deref() {
            Ok("0") => false,
            Ok("1") | Err(_) => true,
            _ => return Err("PROFILE_EVALUATE must be 0 or 1".into()),
        };
        if !evaluate && std::env::var("PROFILE_EVAL_ONLY").is_ok() {
            return Err("evaluation-only jobs need PROFILE_EVALUATE=1".into());
        }
        let frozen_continuation = match std::env::var("PROFILE_EVAL_CONTINUATION").as_deref() {
            Ok("plain") => false,
            Ok("frozen-tree") | Err(_) => true,
            _ => return Err("PROFILE_EVAL_CONTINUATION must be frozen-tree or plain".into()),
        };
        if frozen_continuation && std::env::var("PROFILE_EVAL_ONLY").is_ok() {
            return Err("frozen-tree primary evaluation must run inline; evaluation-only is secondary and needs PROFILE_EVAL_CONTINUATION=plain".into());
        }
        Ok(Self {
            frozen_continuation,
            evaluate,
            options: SearchOptions {
                allocation,
                sharing: arm == "B",
                cpu_seconds,
                work_limit,
            },
            arm,
            seed,
            eval_worlds: count("PROFILE_EVAL_WORLDS", 128)?,
            eval_cap: count("PROFILE_EVAL_CAP", 65_536)?,
            interchange,
            eval_only: std::env::var("PROFILE_EVAL_ONLY").ok(),
        })
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "compatible profile harness inputs"
)]
pub(super) fn profile_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    src: &SourceSpec,
    positions: &[(String, f64)],
    budget: u64,
    mode: KeyMode,
    binding: &str,
) -> Result<Vec<Value>, String> {
    let config = Config::load()?;
    if positions.len() != 1 {
        return Err(
            "experiment jobs need exactly one PROFILE_POSITIONS root for full cost accounting"
                .into(),
        );
    }
    let cpu0 = super::process_cpu::seconds()?;
    let wall0 = Instant::now();
    let focal = PlayerId(src.seat);
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown opponent")?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    live_focal.reset_sampling_v1(src.guide_policy_seeds);
    live_opp.reset_sampling_v1(src.guide_policy_seeds);
    let mut session = source_session(&sh.spec, src)?;
    let mut actions = Vec::new();
    plain_logged(
        &mut session,
        focal,
        &mut live_focal,
        &mut live_opp,
        &mut actions,
    )?;
    let mut eligible = Vec::new();
    walk(source_session(&sh.spec, src)?, &actions, |_, d, _| {
        if acting(d) == focal && d.legal_action_count >= 2 {
            eligible.push((d.step, d.legal_action_count));
        }
        Ok(())
    })?;
    if eligible.is_empty() {
        return Err("source has no eligible focal decision".into());
    }
    let (label, fraction) = &positions[0];
    let index = ((*fraction * eligible.len() as f64) as usize).min(eligible.len() - 1);
    let (step, width) = eligible[index];
    let root = walk(
        source_session(&sh.spec, src)?,
        &actions[..step as usize],
        |_, _, _| Ok(()),
    )?;
    let (obs_hash, menu_hash) = super::replay_key(&root, decision(&root).ok_or("terminal root")?)?;
    let namespace = format!(
        "{}/sharing-allocation/{}/{label}",
        src.search_root, config.seed
    );
    let seeds = RootSeeds {
        model: sh.spec.lineage.clone(),
        root: format!("{namespace}/selection"),
    };
    let limits = Limits {
        select_cap: budget,
        eval_worlds: config.eval_worlds,
        eval_cap: config.eval_cap,
    };
    let mut ctx = guide_ctx(&root, focal, opp, &seeds, &sh.prior, limits)?;
    ctx.key_mode = mode;
    ctx.interchange = config.interchange;
    let mut roles = bank_roles(policy, &sh.opponents)?;
    let setup_cpu = super::process_cpu::seconds()? - cpu0;
    let (selection, teacher, primary_hash, work, search_tree, search_tree_hash) =
        if let Some(path) = &config.eval_only {
            let rows: Vec<Value> = std::fs::read_to_string(path)
                .map_err(|e| format!("{path}: {e}"))?
                .lines()
                .filter(|s| !s.trim().is_empty())
                .map(|s| serde_json::from_str(s).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()?;
            let candidates: Vec<_> = rows
                .iter()
                .filter(|r| {
                    r["id"] == src.id
                        && r["position"] == *label
                        && r["seed"] == config.seed
                        && r["arm"] == config.arm
                })
                .collect();
            if candidates.len() != 1 {
                return Err("evaluation source must contain exactly one matching selection".into());
            }
            let row = candidates[0];
            if row["kind"] != "search_allocation_experiment"
                || row["root_obs_sha256"] != obs_hash
                || row["root_menu_sha256"] != menu_hash
                || row["bank_binding_sha256"] != binding
                || row["key_mode"] != mode.label()
                || row["interchange"] != config.interchange
                || row["root_step"] != step
            {
                return Err(
                    "evaluation source selection binding does not match reconstructed root".into(),
                );
            }
            (
                row["selection"].clone(),
                row["teacher"].clone(),
                row["primary_sha256"].clone(),
                row["work_attempts"].clone(),
                None,
                row["search_tree_sha256"].clone(),
            )
        } else {
            let mut options = config.options.clone();
            if let Some(cpu) = options.cpu_seconds {
                options.cpu_seconds = Some((cpu - setup_cpu).max(0.0));
            }
            let mut diag = SearchDiag::default();
            let (sel, tree) =
                roles.select_e_options(&ctx, None, 1, None, None, Some(&mut diag), &options);
            let teacher = root_choice(
                &root,
                mode,
                config.interchange,
                &tree,
                sel.choice.as_deref(),
            )?;
            let mut selection = sel.json();
            // CPU cutoff and recorded work cutoff differ only in stop provenance.
            if let Some(extra) = selection["extra"].as_object_mut() {
                extra.remove("stop_reason");
            }
            if let Some(sharing) = selection["extra"]["sharing"].as_object_mut() {
                sharing.remove("seconds");
            }
            let mut keys = diag.keys.json();
            keys["keying_seconds"] = Value::Null;
            let tree_sha = tree_digest(&tree);
            let primary = json!({"selection":selection,"teacher":teacher,"keys":keys,
            "tree_sha256":tree_sha});
            (
                sel.json(),
                teacher,
                json!(super::sha(primary.to_string().as_bytes())),
                json!(sel.attempted),
                Some(tree),
                json!(tree_sha),
            )
        };
    let search_cpu = super::process_cpu::seconds()? - cpu0;
    let search_wall = wall0.elapsed().as_secs_f64();
    let eval_seeds = RootSeeds {
        model: sh.spec.lineage.clone(),
        root: format!("{namespace}/fresh-action-evaluation"),
    };
    ctx.seeds = &eval_seeds;
    let execution_prep_cpu0 = super::process_cpu::seconds()?;
    let mut execution_tree = None;
    let mut forced_root_mask = Value::Null;
    if config.evaluate && config.frozen_continuation && config.arm == "C" {
        if let Some(edge) = teacher["selected_edge"].as_u64() {
            let mut execution = search_tree
                .as_ref()
                .ok_or("missing inline search tree")?
                .clone();
            forced_root_mask = restrict_root(
                &root,
                mode,
                config.interchange,
                &mut execution,
                edge as usize,
            )?;
            execution_tree = Some(execution);
        }
    }
    let execution_prep_cpu = super::process_cpu::seconds()? - execution_prep_cpu0;
    let evaluation = if config.evaluate && config.frozen_continuation {
        let tree = execution_tree
            .as_ref()
            .or(search_tree.as_ref())
            .ok_or("missing inline search tree")?;
        evaluate_frozen(
            &mut roles,
            &ctx,
            &teacher,
            tree,
            config.eval_worlds,
            &config.arm,
            &forced_root_mask,
        )?
    } else if config.evaluate {
        evaluate_action(&mut roles, &ctx, &teacher, config.eval_worlds)?
    } else {
        json!({"performed":false,"scheduled":0,"samples":[]})
    };
    let evaluation_hash = super::sha(evaluation.to_string().as_bytes());
    let total_cpu = super::process_cpu::seconds()? - cpu0;
    let mut row = json!({"kind":"search_allocation_experiment","schema":"search-sharing-allocation-v1/root",
        "id":src.id,"lineage":sh.spec.lineage,"own":src.own,"other":src.other,"seat":src.seat,
        "opp":src.opp,"position":label,"fraction":fraction,"eligible_index":index,"eligible_count":eligible.len(),
        "root_step":step,"root_width":width,"root_obs_sha256":obs_hash,"root_menu_sha256":menu_hash,
        "game_actions":actions.len(),"budget":budget,"key_mode":mode.label(),"interchange":config.interchange,
        "urgency":ctx.urgency,"seed":config.seed,"seed_namespace":namespace,"arm":config.arm,
        "bank_binding_sha256":binding,"selection":selection,"teacher":teacher,"primary_sha256":primary_hash,
        "search_tree_sha256":search_tree_hash,"search_tree_estimated_bytes":search_tree.as_ref().map(tree_bytes),
        "work_attempts":work,"cpu_budget_seconds":config.options.cpu_seconds,"work_limit":config.options.work_limit,
        "evaluation_source":config.eval_only,"setup_cpu_seconds":setup_cpu,"search_cpu_seconds":search_cpu,
        "search_wall_seconds":search_wall,"evaluation":evaluation,"evaluation_sha256":evaluation_hash,
        "execution_prep_cpu_seconds":execution_prep_cpu,
        "execution_tree_clone_estimated_bytes":execution_tree.as_ref().map(tree_bytes),
        "evaluation_cpu_seconds":total_cpu-search_cpu,"total_cpu_seconds":total_cpu,
        "total_wall_seconds":wall0.elapsed().as_secs_f64()});
    let output_cpu = super::process_cpu::seconds()?;
    let _serialized = serde_json::to_vec(&row).map_err(|e| e.to_string())?;
    row["output_serialize_cpu_seconds"] = json!(super::process_cpu::seconds()? - output_cpu);
    Ok(vec![row])
}

fn root_choice(
    root: &crate::rl_session::FastActorSessionV1,
    mode: KeyMode,
    interchange: bool,
    tree: &Tree,
    chosen: Option<&[u8]>,
) -> Result<Value, String> {
    let mut stats = KeyStats::default();
    let mut keyer = Keyer::new(mode, &mut stats).with_interchange(interchange);
    let d = decision(root).ok_or("terminal root")?;
    let st = keyer.step(root, d).map_err(super::bank::err)?;
    let (key, pk, pe, cn) = keyer.storage(&st);
    match tree.find(&key, &pk, &pe, cn).map_err(super::bank::err)? {
        Found::Hit(node) => {
            let edge = match chosen {
                Some(edge) => node.canon.edges.iter().position(|e| e == edge),
                None => node.frozen_choice(),
            };
            let live = edge
                .map(|e| live_index(cn, &cn.edges[e]))
                .transpose()
                .map_err(super::bank::err)?;
            Ok(
                json!({"root_found":true,"supported":edge.is_some(),"teacher_live":live,
                "selected_backups":edge.map(|e| node.n[e]),"root_visits":node.visits,
                "supported_edges":node.n.iter().filter(|&&n| n >= MIN_EXEC_BACKUPS).count(),
                "edge_backups":node.n,"edge_wins":node.wins,"seeded_order":node.perm,
                "skipped":node.skip,"selected_edge":edge}),
            )
        }
        Found::Miss => Ok(json!({"root_found":false,"supported":false,"teacher_live":null})),
    }
}

/// Change execution eligibility on a private view, leaving all search
/// evidence and descendant support untouched.
fn restrict_root(
    root: &crate::rl_session::FastActorSessionV1,
    mode: KeyMode,
    interchange: bool,
    tree: &mut Tree,
    edge: usize,
) -> Result<Value, String> {
    let mut stats = KeyStats::default();
    let mut keyer = Keyer::new(mode, &mut stats).with_interchange(interchange);
    let st = keyer
        .step(root, decision(root).ok_or("terminal root")?)
        .map_err(super::bank::err)?;
    let (key, _, _, _) = keyer.storage(&st);
    let node = tree
        .nodes
        .get_mut(&key)
        .ok_or("selected root missing from execution tree")?;
    if node.n.get(edge).copied().unwrap_or(0) < MIN_EXEC_BACKUPS
        || node.skip.get(edge).copied().unwrap_or(false)
    {
        return Err("forced execution root lacks actual local terminal support".into());
    }
    node.skip = (0..node.n.len()).map(|i| i != edge).collect();
    Ok(json!({"forced_edge":edge,"skip_mask":node.skip}))
}

fn tree_digest(tree: &Tree) -> String {
    let mut nodes: Vec<_> = tree.nodes.iter().collect();
    nodes.sort_by_key(|(key, _)| **key);
    let mut hash = Sha256::new();
    for (key, n) in nodes {
        let bytes = serde_json::to_vec(&json!({"key":key,"parent":n.parent,"edge":n.edge,
            "obs":n.canon.obs,"menu":n.canon.menu,"edges":n.canon.edges,"perm":n.perm,
            "visits":n.visits,"n":n.n,"wins":n.wins,"depth":n.depth,"skip":n.skip}))
        .expect("tree JSON");
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn tree_bytes(tree: &Tree) -> usize {
    tree.nodes
        .values()
        .map(|n| {
            std::mem::size_of_val(n)
                + n.edge.len()
                + n.canon.obs.len()
                + n.canon
                    .menu
                    .iter()
                    .chain(&n.canon.edges)
                    .map(|e| 24 + e.len())
                    .sum::<usize>()
                + 8 * (n.perm.len() + n.n.len() + n.wins.len())
                + n.skip.len()
        })
        .sum()
}

#[allow(
    clippy::too_many_arguments,
    reason = "fixed primary evaluation contract and provenance"
)]
fn evaluate_frozen(
    roles: &mut super::arms::Roles,
    ctx: &super::arms::RootCtx,
    teacher: &Value,
    tree: &Tree,
    worlds: u64,
    arm: &str,
    forced_root: &Value,
) -> Result<Value, String> {
    let mut stats = SamplerStats::default();
    let mut samples = Vec::new();
    let mut natural = 0;
    let mut wins = 0;
    let mut root_fallbacks = 0;
    for i in 0..worlds {
        let seed = ctx.seeds.get(Purpose::EvalWorld, "", i, b"world");
        let sample = match super::world::sample(ctx.root, seed, ctx.prior) {
            Ok(world) => {
                let out = roles.evaluate(ctx, "E", &world.world, i, None, Some(tree), &mut stats);
                natural += u64::from(!out.unknown);
                wins += u64::from(out.w);
                root_fallbacks += u64::from(out.fallback_root);
                json!({"index":i,"world_seed":seed,"outcome":out.json()})
            }
            Err(e) => json!({"index":i,"world_seed":seed,"sample_rejected":e}),
        };
        samples.push(sample);
    }
    let tree_sha = tree_digest(tree);
    let eval_id = super::sha(
        json!({"namespace":ctx.seeds.root,"model":ctx.seeds.model,
        "arm":arm,"execution_tree_sha256":tree_sha,"forced_root":forced_root,
        "worlds":worlds,"cap":ctx.limits.eval_cap,"continuation":"frozen-search-tree"})
        .to_string()
        .as_bytes(),
    );
    Ok(
        json!({"supported":teacher["supported"],"scheduled":worlds,"independent_natural":natural,
        "wins":wins,"unknown":worlds-natural,"root_fallbacks":root_fallbacks,"evaluation_id":eval_id,
        "seed_namespace":ctx.seeds.root,"continuation":"frozen-search-tree",
        "execution_tree_sha256":tree_sha,"forced_root":forced_root,"samples":samples}),
    )
}

fn evaluate_action(
    roles: &mut super::arms::Roles,
    ctx: &super::arms::RootCtx,
    teacher: &Value,
    worlds: u64,
) -> Result<Value, String> {
    let Some(live) = teacher["teacher_live"].as_u64() else {
        return Ok(
            json!({"supported":false,"scheduled":worlds,"samples":[],"independent_natural":0}),
        );
    };
    if teacher["supported"] != true
        || teacher["selected_backups"].as_u64().unwrap_or(0) < MIN_EXEC_BACKUPS
    {
        return Err("selected action lacks eight actual natural terminal backups".into());
    }
    let cn = canon(ctx.root, ctx.d).map_err(super::bank::err)?;
    let edge = cn
        .menu
        .get(live as usize)
        .ok_or("selected live action out of range")?;
    let mut stats = SamplerStats::default();
    let mut samples = Vec::new();
    let mut natural = 0;
    let mut wins = 0;
    for i in 0..worlds {
        let seed = ctx.seeds.get(Purpose::EvalWorld, "", i, b"world");
        let sample = match super::world::sample(ctx.root, seed, ctx.prior) {
            Ok(world) => {
                let out = roles.evaluate(ctx, "A", &world.world, i, Some(edge), None, &mut stats);
                natural += u64::from(!out.unknown);
                wins += u64::from(out.w);
                json!({"index":i,"world_seed":seed,"outcome":out.json()})
            }
            Err(e) => json!({"index":i,"world_seed":seed,"sample_rejected":e}),
        };
        samples.push(sample);
    }
    let action_sha = super::sha(edge);
    let eval_id = super::sha(
        json!({"namespace":ctx.seeds.root,"model":ctx.seeds.model,
        "action_sha256":action_sha,"worlds":worlds,"cap":ctx.limits.eval_cap})
        .to_string()
        .as_bytes(),
    );
    Ok(
        json!({"supported":true,"scheduled":worlds,"independent_natural":natural,"wins":wins,
        "unknown":worlds-natural,"action_sha256":action_sha,"evaluation_id":eval_id,
        "seed_namespace":ctx.seeds.root,"continuation":"plain-frozen-policy",
        "samples":samples}),
    )
}
