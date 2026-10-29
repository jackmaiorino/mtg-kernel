//! Certified win-available-this-turn labels for recorded evaluator games
//! (mode `s4a-win-check`). Measurement only: nothing that chooses, samples,
//! trains or selects reads these rows.
//!
//! Each label job is rebuilt exactly as `s4a-label-replay` rebuilds it (the
//! evaluator's constructor, opening and runtime rules) and checked against
//! its record at every decision. At each focal decision (the focal seat with
//! two or more legal actions) a clone of the live session is searched to the
//! end of the turn (`win_search_v1`): the oracle estimand on the true state,
//! and the robust estimand over the true world plus `WIN_WORLDS` resampled
//! worlds. The game row carries the Stage 4a Spy labels for agreement checks.
//!
//! Environment: `LABEL_JOBS` (jsonl), `WIN_NODES` (node budget per search,
//! default 20000), `WIN_DEPTH` (decisions below the root, default 400),
//! `WIN_WORLDS` (resampled worlds, default 4; 0 skips the robust search;
//! the robust search gets `WIN_NODES` per world),
//! `WIN_INDEX` (optional comma list of decision indices to search; others
//! are replayed without searching).

use super::labels::{spy_defs, Suffix};
use super::play::{acting, apply, decision, observe, Meter};
use super::replay_labels::LabelJob;
use crate::ids::PlayerId;
use crate::rl_session::FastActorResponseV1;
use crate::win_search_v1::{oracle_win_this_turn_v1, robust_win_this_turn_v1, WinSearchBoundsV1, WinSearchResultV1, WinVerdictV1};
use serde_json::{json, Value};

fn env_u64(name: &str, default: u64) -> Result<u64, String> {
    match std::env::var(name) {
        Ok(text) => text.parse().map_err(|_| format!("{name} must be an integer")),
        Err(_) => Ok(default),
    }
}

fn result_json(result: &WinSearchResultV1) -> Value {
    json!({"verdict": result.verdict.as_str(), "unknown_reason": result.unknown_reason.map(|r| r.as_str()),
        "nodes": result.nodes, "depth_cuts": result.depth_cuts, "refused_actions": result.refused_actions,
        "forced_order_collapses": result.forced_order_collapses, "expanded": result.expanded, "line": result.line})
}

/// EX1 menu stratum: a target choice, else a cast or activation, else other.
fn stratum(s: &crate::rl_session::FastActorSessionV1) -> &'static str {
    use crate::rl::ActionSemanticV1 as A;
    let menu = s.diagnostic_current_action_semantics().unwrap_or_default();
    if menu.iter().any(|a| matches!(a, A::ChooseTarget { .. } | A::ChooseEffectTarget { .. })) {
        "target"
    } else if menu.iter().any(|a| matches!(a, A::CastSpell { .. } | A::ActivateAbility { .. })) {
        "cast_or_activate"
    } else {
        "other"
    }
}

/// Deterministic world seeds per job and decision.
fn world_seeds(id: &str, index: usize, count: u64) -> Vec<u64> {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for byte in id.bytes() {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    (0..count)
        .map(|k| {
            let mut z = h ^ ((index as u64) << 20) ^ k.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        })
        .collect()
}

fn check_job(job: &LabelJob, bounds: WinSearchBoundsV1, worlds: u64, only: Option<&[usize]>, out: &mut impl std::io::Write) -> Result<Value, String> {
    let mut s = crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )?;
    let focal = PlayerId(job.focal);
    let runtime = serde_json::to_value(job.config.runtime_rules).unwrap_or(Value::Null);
    let defs = spy_defs();
    let mut suffix = Suffix::new(true);
    let mut meter = Meter::new(u64::MAX);
    let mut focal_decisions = 0u64;
    let mut last_turn = 0u32;
    // Decisions with two or more legal actions by seat role and menu stratum.
    let mut strata: std::collections::BTreeMap<String, u64> = Default::default();
    for (i, r) in job.decisions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        if d.step != r.step
            || d.physical_decision_id != r.physical_decision_id
            || acting(&d).0 != r.actor
            || d.legal_action_count != r.legal_action_count
        {
            return Err(format!("replay differs from the record at decision {i}"));
        }
        last_turn = s.game_state().turn;
        let is_focal = acting(&d) == focal && d.legal_action_count >= 2;
        focal_decisions += u64::from(is_focal);
        if d.legal_action_count >= 2 {
            let role = if acting(&d) == focal { "focal" } else { "opponent" };
            *strata.entry(format!("{role}:{}", stratum(&s))).or_default() += 1;
        }
        if is_focal && only.is_none_or(|list| list.contains(&i)) {
            let started = std::time::Instant::now();
            let oracle = oracle_win_this_turn_v1(&s, bounds).ok_or("focal decision has no pending decision")?;
            // The true world is in every sample: an oracle refutation certifies
            // robust none, and only an oracle win leaves a robust line to find.
            let robust = if worlds == 0 {
                Value::Null
            } else if oracle.verdict != WinVerdictV1::Proven {
                json!({"verdict": oracle.verdict.as_str(), "unknown_reason": oracle.unknown_reason.map(|r| r.as_str()), "via": "oracle"})
            } else {
                // One node budget per world of the sample.
                let robust_bounds = WinSearchBoundsV1 { max_nodes: bounds.max_nodes * (worlds + 1), ..bounds };
                match robust_win_this_turn_v1(&s, &world_seeds(&job.id, i, worlds), robust_bounds) {
                    Some(Ok(result)) => result_json(&result),
                    Some(Err(error)) => json!({"verdict": "unknown", "unknown_reason": "world_sampling_failed", "error": error}),
                    None => Value::Null,
                }
            };
            let chosen = s.diagnostic_current_action_semantics().and_then(|v| v.get(r.selected as usize).cloned());
            let row = json!({"kind": "decision", "id": job.id, "focal": job.focal, "runtime": runtime, "index": i,
                "step": r.step, "turn": last_turn, "active": s.game_state().active_player.0, "legal": r.legal_action_count,
                "selected": r.selected, "selected_semantic": chosen.map(|c| format!("{c:?}")),
                "spy_stage_before": suffix.json()["stage"].clone(),
                "max_nodes": bounds.max_nodes, "max_depth": bounds.max_depth, "worlds": worlds,
                "oracle": result_json(&oracle), "robust": robust, "seconds": started.elapsed().as_secs_f64()});
            writeln!(out, "{row}").map_err(|e| e.to_string())?;
        }
        if is_focal {
            observe(Some(&mut suffix), &s, r.selected, focal, &defs);
        }
        apply(&mut s, &d, r.selected, &mut meter, focal, Some(&mut suffix), &defs).map_err(|e| format!("{e:?}"))?;
    }
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("recorded actions end before the terminal".into());
    };
    let winner = t.winner.map(|w| match w {
        crate::rl::PlayerSeatV1::P0 => 0u8,
        crate::rl::PlayerSeatV1::P1 => 1u8,
    });
    if winner != job.winner {
        return Err("replayed winner differs from the record".into());
    }
    let win = winner == Some(job.focal);
    Ok(json!({"kind": "game", "id": job.id, "focal": job.focal, "runtime": runtime, "deck_ids": job.config.deck_ids,
        "winner": winner, "win": win, "final_turn": s.game_state().turn, "last_decision_turn": last_turn,
        "decisions": job.decisions.len(), "focal_decisions": focal_decisions, "strata": strata, "labels": suffix.json(),
        "combo_resolved": suffix.complete(), "combo_won": suffix.complete() && win}))
}

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    use std::io::Write;
    let path = std::env::var("LABEL_JOBS").map_err(|_| "LABEL_JOBS is required")?;
    let bounds = WinSearchBoundsV1 { max_nodes: env_u64("WIN_NODES", 20_000)?, max_depth: env_u64("WIN_DEPTH", 400)? as u32 };
    let worlds = env_u64("WIN_WORLDS", 4)?;
    let only: Option<Vec<usize>> = match std::env::var("WIN_INDEX") {
        Ok(text) => Some(text.split(',').map(|x| x.trim().parse().map_err(|_| "WIN_INDEX must list integers".to_string())).collect::<Result<_, _>>()?),
        Err(_) => None,
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let job: LabelJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        let row = check_job(&job, bounds, worlds, only.as_deref(), &mut out).unwrap_or_else(|e| json!({"kind": "error", "id": job.id, "error": e}));
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
