//! Passive Spy combo labels for recorded evaluator games (mode
//! `s4a-label-replay`, spy-reset-pilot-v1 RUNNER.md section 7).
//!
//! The public evaluator keeps only winners and, with `capture_decisions`,
//! each decision's selected index. This mode rebuilds a recorded physical
//! game with the evaluator's own constructor, opening and runtime rules
//! (`learned_bo3_v1::replay_game_session_v1`), applies the recorded actions
//! for both seats and runs the Stage 4a resolution labels (`labels.rs`,
//! starting before the Spy cast) for one focal seat. Each decision's step,
//! physical decision, actor and legal-menu size and the game's winner must
//! equal the record. Labels never choose anything.

use super::labels::{spy_defs, Suffix};
use super::play::{acting, apply, decision, observe, Meter};
use crate::ids::PlayerId;
use crate::learned_bo3_v1::LearnedBo3RunConfigV1;
use crate::rl_session::FastActorResponseV1;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordedDecision {
    pub(super) step: u64,
    pub(super) physical_decision_id: u64,
    pub(super) actor: u8,
    pub(super) legal_action_count: u32,
    pub(super) selected: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LabelJob {
    pub(super) id: String,
    pub(super) config: LearnedBo3RunConfigV1,
    pub(super) uses_v3: bool,
    pub(super) game_index: u8,
    pub(super) environment_seed: u64,
    pub(super) starting_player: u8,
    pub(super) mainboards: [Vec<u16>; 2],
    pub(super) focal: u8,
    pub(super) decisions: Vec<RecordedDecision>,
    /// The recorded winner seat, or None for a draw.
    pub(super) winner: Option<u8>,
}

pub(super) fn label_job(job: &LabelJob) -> Result<Value, String> {
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
    let mut suffix = Suffix::new(true);
    let mut meter = Meter::new(u64::MAX);
    for (i, r) in job.decisions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        if d.step != r.step
            || d.physical_decision_id != r.physical_decision_id
            || acting(&d).0 != r.actor
            || d.legal_action_count != r.legal_action_count
        {
            return Err(format!("replay differs from the record at decision {i}"));
        }
        if acting(&d) == focal && d.legal_action_count >= 2 {
            observe(Some(&mut suffix), &s, r.selected, focal, &defs);
        }
        apply(
            &mut s,
            &d,
            r.selected,
            &mut meter,
            focal,
            Some(&mut suffix),
            &defs,
        )
        .map_err(|e| format!("{e:?}"))?;
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
    Ok(
        json!({"kind":"label_replay","id":job.id,"focal":job.focal,"winner":winner,
        "decisions":job.decisions.len(),"win":win,"labels":suffix.json(),
        "combo_resolved":suffix.complete(),"combo_won":suffix.complete() && win}),
    )
}

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    use std::io::Write;
    let path = std::env::var("LABEL_JOBS").map_err(|_| "LABEL_JOBS is required")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut out = std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let job: LabelJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        let row = label_job(&job).unwrap_or_else(|e| json!({"kind":"error","id":job.id,"error":e}));
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    Ok(())
}
