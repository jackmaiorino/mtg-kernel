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

/// EX1 label qualification (design v3.3 section 7 item 7): builds a scripted
/// known-positive label job when no recorded evaluator game completes the
/// line. The recorded job is replayed to decision `EX1_LABEL_INDEX`, then the
/// focal seat plays the checker's certified oracle line (`EX1_LABEL_LINE`,
/// action semantics) and the other seat passes when it can, else plays its
/// first action (the oracle's reported reply order), to the terminal. The
/// job is written to `EX1_LABEL_OUT` and labelled here. Run by hand:
/// `cargo test --release ex1_scripted_known_positive -- --ignored`.
#[cfg(test)]
mod ex1_scripted {
    use super::*;

    #[test]
    #[ignore = "builds the EX1 label-qualification job from a recorded game"]
    fn ex1_scripted_known_positive() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"));
        let text = std::fs::read_to_string(var("EX1_LABEL_JOBS")).unwrap();
        let id = var("EX1_LABEL_ID");
        let index: usize = var("EX1_LABEL_INDEX").parse().unwrap();
        let line: Vec<String> = serde_json::from_str(&var("EX1_LABEL_LINE")).unwrap();
        let mut document: Value = text
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .find(|j| j["id"] == json!(id))
            .expect("the recorded job");
        let job: LabelJob = serde_json::from_value(document.clone()).unwrap();
        let mut s = crate::learned_bo3_v1::replay_game_session_v1(
            &job.config,
            job.uses_v3,
            job.game_index,
            job.environment_seed,
            job.mainboards.clone(),
            PlayerId(job.starting_player),
        )
        .unwrap();
        let focal = PlayerId(job.focal);
        let mut decisions = Vec::new();
        let mut remaining = line.iter();
        loop {
            let FastActorResponseV1::Decision(d) = s.current_response() else {
                break;
            };
            let selected = if decisions.len() < index {
                job.decisions[decisions.len()].selected
            } else {
                let menu = s.diagnostic_current_action_semantics().unwrap();
                if acting(&d) == focal {
                    let want = remaining.next().expect("the line ends before the terminal");
                    menu.iter()
                        .position(|a| &format!("{a:?}") == want)
                        .expect("a line action is not on the menu") as u32
                } else {
                    menu.iter()
                        .position(|a| matches!(a, crate::rl::ActionSemanticV1::Pass { .. }))
                        .unwrap_or(0) as u32
                }
            };
            decisions.push(json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "actor":acting(&d).0,"legal_action_count":d.legal_action_count,"selected":selected}));
            s.step(d.episode_id, d.step, selected).unwrap();
        }
        let FastActorResponseV1::Terminal(t) = s.current_response() else {
            unreachable!()
        };
        let winner = t.winner.map(|w| match w {
            crate::rl::PlayerSeatV1::P0 => 0u8,
            crate::rl::PlayerSeatV1::P1 => 1u8,
        });
        document["id"] = json!(format!("{id}/ex1-scripted-line-at-{index}"));
        document["decisions"] = json!(decisions);
        document["winner"] = json!(winner);
        let scripted: LabelJob = serde_json::from_value(document.clone()).unwrap();
        let row = label_job(&scripted).unwrap();
        assert_eq!(row["combo_resolved"], true, "{row}");
        assert_eq!(row["combo_won"], true, "{row}");
        std::fs::write(var("EX1_LABEL_OUT"), format!("{document}\n")).unwrap();
    }
}
