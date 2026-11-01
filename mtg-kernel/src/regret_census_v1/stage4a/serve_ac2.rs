//! AC2 determinism rows for the W1 entrant (design section 9), mode
//! `s4a-serve-ac2`. A game between two copies of the loaded policy is played
//! from a seeded reset; at viewer decisions with at least two candidates the
//! producer exports and, for each search seed, compares:
//! - direct search on the canonical session against search on the
//!   serialized, imported payload (every tree digest);
//! - the K-tree pool (separate processes, concurrent) against the same trees
//!   run serially in this process;
//! - a K = 1 pool against the single-tree path at the full budget.
//!
//! Environment: `W1_AC2_DECKS` (two deck ids), `W1_AC2_GAME_SEED`,
//! `W1_AC2_DECISIONS` (eligible decisions to check), `W1_AC2_BUDGET` (B),
//! `W1_AC2_K`, `W1_AC2_SEEDS` (comma list). Output: JSONL rows at `out=`.

use super::play::decision;
use super::serve::{menu_sha256, merge_v1, partition_v1, search_tree_v1, TreeResultV1};
use crate::infoset_v1::{export_v1, ExportInputV1, ExportOptionsV1, PayloadV1};
use crate::rl_session::FastActorSessionV1;
use crate::runtime_decks::runtime_deck_by_id;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub(super) const AC2_MODE: &str = "s4a-serve-ac2";

fn env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is required"))
}

fn env_u64(name: &str) -> Result<u64, String> {
    env(name)?
        .parse()
        .map_err(|_| format!("{name} must be an integer"))
}

struct PoolV1 {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl PoolV1 {
    fn spawn(source: &str, k: u64) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut child = std::process::Command::new(exe)
            .args([
                format!("source={source}"),
                format!("mode={}", super::serve::SERVE_MODE),
                "out=-".into(),
            ])
            .env("W1_SERVE_K", k.to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn pool: {e}"))?;
        let stdin = child.stdin.take().ok_or("pool stdin")?;
        let stdout = std::io::BufReader::new(child.stdout.take().ok_or("pool stdout")?);
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    fn search(&mut self, root_id: &str, budget: u64, payload: &PayloadV1) -> Result<Value, String> {
        let req = json!({"root_id": root_id, "budget": budget, "a_units": 0, "payload": payload});
        writeln!(self.stdin, "{req}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("pool reply: {e}"))?;
        if v["ok"] != json!(true) {
            return Err(format!("pool failed: {}", v["error"]));
        }
        Ok(v)
    }
}

impl Drop for PoolV1 {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn run(
    cfg: &super::super::CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
) -> Result<(), String> {
    let decks: Vec<String> = env("W1_AC2_DECKS")?
        .split(',')
        .map(|x| x.trim().to_string())
        .collect();
    let [d0, d1] = decks.as_slice() else {
        return Err("W1_AC2_DECKS needs two deck ids".into());
    };
    let game_seed = env_u64("W1_AC2_GAME_SEED")?;
    let wanted = env_u64("W1_AC2_DECISIONS")?;
    let budget = env_u64("W1_AC2_BUDGET")?;
    let k = env_u64("W1_AC2_K")?;
    let seeds: Vec<u64> = env("W1_AC2_SEEDS")?
        .split(',')
        .filter(|x| !x.trim().is_empty())
        .map(|x| x.trim().parse().map_err(|_| "W1_AC2_SEEDS"))
        .collect::<Result<_, _>>()?;
    let lists = [d0, d1].map(|id| {
        runtime_deck_by_id(id)
            .map(|d| d.card_ids.to_vec())
            .ok_or(format!("deck {id} is not registered"))
    });
    let lists = [lists[0].clone()?, lists[1].clone()?];
    let mut out =
        std::io::BufWriter::new(std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?);
    let mut pool_k = PoolV1::spawn(&cfg.source, k)?;
    let mut pool_1 = PoolV1::spawn(&cfg.source, 1)?;
    let mut s = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        game_seed,
        game_seed,
        2_000,
        20_000,
        [d0.clone(), d1.clone()],
    )
    .map_err(|e| e.message)?;
    let mut player = policy.fork_for_collection_v3()?;
    let mut own_index = [0u64; 2];
    let (mut checked, mut failures) = (0u64, 0u64);
    while checked < wanted {
        let Some(d) = decision(&s) else { break };
        let seat = super::play::acting(&d);
        if d.legal_action_count >= 2 {
            own_index[seat.index()] += 1;
            let state = s.game_state();
            let export = export_v1(
                &ExportInputV1 {
                    state,
                    journal: state.observation_journal.notes(),
                    viewer: seat,
                    decklists: [&lists[0], &lists[1]],
                },
                ExportOptionsV1::default(),
            );
            match export {
                Err(e) => {
                    writeln!(
                        out,
                        "{}",
                        json!({"kind": "refusal", "seat": seat.0,
                        "code": e.code_name()})
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(x) => {
                    checked += 1;
                    let payload = PayloadV1::new(&x, &s, seat, own_index[seat.index()]);
                    let bytes = payload.to_bytes();
                    let imported = PayloadV1::from_bytes(&bytes)?.import()?;
                    let direct = FastActorSessionV1::import_infoset_v1(
                        &s,
                        x.state.clone(),
                        own_index[seat.index()],
                    );
                    for &seed in &seeds {
                        let root_id = format!(
                            "ac2/{game_seed}/{}/{}/s{seed}",
                            seat.0,
                            own_index[seat.index()]
                        );
                        let row = check_root(
                            policy,
                            &direct,
                            &imported,
                            &payload,
                            &root_id,
                            budget,
                            k,
                            &mut pool_k,
                            &mut pool_1,
                        )?;
                        if row["equal"] != json!(true) {
                            failures += 1;
                        }
                        writeln!(out, "{row}").map_err(|e| e.to_string())?;
                    }
                    out.flush().map_err(|e| e.to_string())?;
                }
            }
        }
        let a = if d.legal_action_count < 2 {
            0
        } else {
            player.select_fast_session_v1(&s)?
        };
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("game step: {e:?}"))?;
    }
    writeln!(
        out,
        "{}",
        json!({"kind": "done", "checked": checked, "failures": failures})
    )
    .map_err(|e| e.to_string())?;
    if failures > 0 {
        return Err(format!("{failures} AC2 rows differ"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "the compared paths, explicitly")]
fn check_root(
    policy: &FrozenPlayPolicyV1,
    direct: &FastActorSessionV1,
    imported: &FastActorSessionV1,
    payload: &PayloadV1,
    root_id: &str,
    budget: u64,
    k: u64,
    pool_k: &mut PoolV1,
    pool_1: &mut PoolV1,
) -> Result<Value, String> {
    let menu = menu_sha256(direct)?;
    let width = decision(direct).ok_or("terminal root")?.legal_action_count as usize;
    let serial = |root: &FastActorSessionV1| -> Result<Vec<TreeResultV1>, String> {
        (0..k)
            .map(|t| {
                search_tree_v1(
                    policy,
                    root,
                    payload,
                    root_id,
                    t,
                    partition_v1(budget, t, k),
                )
            })
            .collect()
    };
    let direct_trees = serial(direct)?;
    let imported_trees = serial(imported)?;
    let digests = |ts: &[TreeResultV1]| ts.iter().map(|t| t.digest.clone()).collect::<Vec<_>>();
    let serial_merge = merge_v1(
        root_id,
        budget,
        0,
        &payload.sha256,
        &menu,
        width,
        &imported_trees,
    );
    let pooled = pool_k.search(root_id, budget, payload)?;
    let pooled_digests: Vec<String> = pooled["search"]["tree_digests"]
        .as_array()
        .ok_or("pool digests")?
        .iter()
        .map(|x| x.as_str().unwrap_or("").to_string())
        .collect();
    let single = search_tree_v1(policy, imported, payload, root_id, 0, budget)?;
    let pooled_1 = pool_1.search(root_id, budget, payload)?;
    let single_pool = pooled_1["search"]["tree_digests"][0]
        .as_str()
        .unwrap_or("")
        .to_string();
    let direct_vs_import = digests(&direct_trees) == digests(&imported_trees);
    let pool_vs_serial = pooled_digests == digests(&imported_trees)
        && pooled["search"]["pick"] == json!(serial_merge.pick);
    let k1_vs_single = single_pool == single.digest;
    Ok(
        json!({"kind": "root", "root_id": root_id, "payload_sha256": payload.sha256,
        "direct_vs_import": direct_vs_import, "pool_vs_serial": pool_vs_serial,
        "k1_pool_vs_single": k1_vs_single,
        "equal": direct_vs_import && pool_vs_serial && k1_vs_single,
        "pick": serial_merge.pick, "stage": serial_merge.stage, "units": serial_merge.units,
        "tree_digests": digests(&imported_trees)}),
    )
}
