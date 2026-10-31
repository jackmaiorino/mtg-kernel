//! Search on recorded human-game decisions (mode `s4a-sbroot`). Measurement
//! only: nothing that chooses, samples, trains or selects reads these rows.
//!
//! A Spellbench pauper-v2 game is rebuilt on this engine from the native
//! steps its adapter sent the bridge (`tools/reach/sbcapture.py` writes the
//! job): the flat-action session the bridge kept in lockstep with its raw
//! session (`reset_with_decks_and_limits_flat_action_v3` with the bridge's
//! episode id and game seed), stepped with each native step's flat row. Every
//! recorded decision must match this engine's actor and row kinds. A
//! single-candidate recorded choice that this engine resolves by itself
//! (the pauper-v2 line's neutral choices) is skipped and logged. Each
//! step's menu and the game facts are written so the job tool can compare
//! names (from the card table it writes first) and the acting seat's view
//! against the record. At each target decision, unmodified E search runs
//! exactly as RS0 runs it (`reach`), once per budget and seed.
//!
//! Environment: `SBROOT_JOB` (json), `SBROOT_BUDGETS` and `SBROOT_SEEDS`
//! (comma lists).

use super::labels::spy_defs;
use super::play::{acting, apply, decision, Meter};
use super::reach::{o2, search, Rebuilt};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    /// Index in the wire record's decision list (both seats).
    i: usize,
    seat: u8,
    /// Row kinds in flat-row order (`action_kind` names).
    kinds: Vec<String>,
    /// The recorded selection's flat row.
    row: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    game_id: String,
    episode_id: u64,
    game_seed: u64,
    deck_ids: [String; 2],
    max_decisions: u64,
    max_steps: u64,
    steps: Vec<Step>,
    targets: Vec<usize>,
}

fn env_list(name: &str) -> Result<Vec<u64>, String> {
    let text = std::env::var(name).map_err(|_| format!("{name} is required"))?;
    text.split(',')
        .filter(|x| !x.trim().is_empty())
        .map(|x| {
            x.trim()
                .parse()
                .map_err(|_| format!("{name}: bad entry {x}"))
        })
        .collect()
}

/// This engine's menu: each row's kind and the card ids it names.
fn menu(s: &FastActorSessionV1) -> Result<Vec<Value>, String> {
    let sem = s
        .diagnostic_current_action_semantics()
        .ok_or("missing semantics")?;
    sem.iter()
        .map(|x| {
            let v = serde_json::to_value(x).map_err(|e| e.to_string())?;
            fn walk(v: &Value, out: &mut Vec<u64>) {
                match v {
                    Value::Object(m) => {
                        if let Some(c) = m.get("card_db_id").and_then(Value::as_u64) {
                            out.push(c);
                        }
                        m.values().for_each(|x| walk(x, out));
                    }
                    Value::Array(xs) => xs.iter().for_each(|x| walk(x, out)),
                    _ => {}
                }
            }
            let mut cards = Vec::new();
            walk(&v, &mut cards);
            Ok(json!({"kind": v["action_kind"], "cards": cards}))
        })
        .collect()
}

pub(super) fn run(
    cfg: &super::super::CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
) -> Result<(), String> {
    let path = std::env::var("SBROOT_JOB").map_err(|_| "SBROOT_JOB is required")?;
    let budgets = env_list("SBROOT_BUDGETS")?;
    let seeds = env_list("SBROOT_SEEDS")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let job: Job = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let mut out =
        std::io::BufWriter::new(std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?);
    let mut line = |v: Value| -> Result<(), String> {
        writeln!(out, "{v}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())
    };
    let names: Vec<&str> = crate::card_def::CARD_DEFS.iter().map(|c| c.name).collect();
    line(json!({"kind": "card_names", "names": names}))?;
    let mut s = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        job.episode_id,
        job.game_seed,
        job.max_decisions,
        job.max_steps,
        job.deck_ids.clone(),
    )
    .map_err(|e| e.message)?;
    let defs = spy_defs();
    let mut meter = Meter::new(u64::MAX);
    let mut scorer = policy.fork_for_collection_v3()?;
    for step in &job.steps {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {}", step.i))?;
        let rows = menu(&s)?;
        let kinds: Vec<&str> = rows
            .iter()
            .map(|r| r["kind"].as_str().unwrap_or(""))
            .collect();
        let actor = acting(&d).0;
        let facts = super::wm_dump::facts(s.game_state());
        let here = json!({"actor": actor, "legal": d.legal_action_count, "rows": rows,
            "turn": s.game_state().turn, "facts": facts});
        let same = actor == step.seat && kinds == step.kinds;
        if !same && step.kinds.len() == 1 {
            line(json!({"kind": "step", "i": step.i, "status": "skipped", "main": here}))?;
            if job.targets.contains(&step.i) {
                line(json!({"kind": "target", "i": step.i, "status": "resolved_by_this_engine"}))?;
            }
            continue;
        }
        if !same {
            line(
                json!({"kind": "mismatch", "i": step.i, "record_seat": step.seat,
                "record_kinds": step.kinds, "main": here}),
            )?;
            return Ok(());
        }
        line(json!({"kind": "step", "i": step.i, "status": "applied", "main": here}))?;
        if job.targets.contains(&step.i) {
            let rb = Rebuilt {
                root: s.clone(),
                focal: PlayerId(step.seat),
                prefix: None,
                witness: None,
            };
            let root_id = format!("{}#{}", job.game_id, step.i);
            for &budget in &budgets {
                for &seed in &seeds {
                    let row = (|| -> Result<Value, String> {
                        let r = search(policy, &rb, &root_id, budget, seed, false)?;
                        let o2 = o2(r.tree, &rb, &mut scorer, None)?;
                        Ok(json!({"kind": "search", "i": step.i, "root_id": root_id,
                            "budget": budget, "seed": seed, "selection": r.sel.json(),
                            "o2": o2, "cpu_seconds": r.cpu, "wall_seconds": r.wall}))
                    })()
                    .unwrap_or_else(|e| {
                        json!({"kind": "error", "i": step.i, "budget": budget, "seed": seed, "error": e})
                    });
                    line(row)?;
                }
            }
        }
        apply(
            &mut s,
            &d,
            step.row,
            &mut meter,
            PlayerId(step.seat),
            None,
            &defs,
        )
        .map_err(|e| format!("{e:?}"))?;
    }
    line(json!({"kind": "done", "game_id": job.game_id}))
}
