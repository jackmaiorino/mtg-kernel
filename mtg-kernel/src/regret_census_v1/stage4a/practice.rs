//! Discovery-efficiency practice track (collab
//! LANES/discovery-efficiency-plan-20261012, RUNNER.md sections 2 and 3):
//! analysis-only modes over the trainer's own reset path.
//!
//! - `s4a-practice-starts` (PRACTICE_JOBS): replays a complete recorded
//!   game through the trainer's session constructor and lists the focal
//!   seat's legal physical-decision starts (first substep, forced ones
//!   included as in the archive and guide records), with the replay keys the trainer checks at a reset cut.
//! - `s4a-practice-values` (PRACTICE_JOBS, PRACTICE_PARENT): restores each
//!   candidate cut with the trainer's `restore_reset_cut_v1` and scores the
//!   learner's ordinary visible decision with the frozen parent. Nothing is
//!   sampled; source outcomes are never read here.
//! - `s4a-train-exposure` (EXPOSURE_EPISODES): passive exposure of recorded
//!   training trajectories. Rebuilds each episode, applies its reset prefix
//!   (inherited Spy events), then its recorded suffix actions (fresh events),
//!   and checks every suffix row against the replay. Labels never choose
//!   anything and never reach training.

use super::labels::{events, spy_defs, Before, SpyDefs, Suffix};
use super::play::{acting, decision};
use crate::expanded_deck_training_v1::{
    episode_session_v1, load_expanded_inference_v1, restore_reset_cut_v1, ExpandedEpisodeV1,
    ExpandedModelSourceV1,
};
use crate::ids::PlayerId;
use crate::rl::TerminalClassificationV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartsJob {
    id: String,
    episode: ExpandedEpisodeV1,
    actions: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValueJob {
    id: String,
    episode: ExpandedEpisodeV1,
}

fn session(episode: &ExpandedEpisodeV1) -> Result<FastActorSessionV1, String> {
    episode_session_v1(episode, &episode.configurations_v1()?)
}

fn winner(s: &FastActorSessionV1) -> Result<Option<u8>, String> {
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("not a terminal".into());
    };
    if t.terminal_classification != TerminalClassificationV1::Natural {
        return Err("non-natural terminal".into());
    }
    Ok(t.winner.map(|w| match w {
        crate::rl::PlayerSeatV1::P0 => 0u8,
        crate::rl::PlayerSeatV1::P1 => 1u8,
    }))
}

fn starts_job(job: &StartsJob) -> Result<Value, String> {
    if job.episode.reset.is_some() {
        return Err("a starts job replays a whole game, not a reset".into());
    }
    let mut s = session(&job.episode)?;
    let focal = PlayerId(job.episode.learner_seat);
    let mut starts = Vec::new();
    for (i, &a) in job.actions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("game ended before action {i}"))?;
        if d.step != i as u64 {
            return Err(format!("step {} differs from action index {i}", d.step));
        }
        if acting(&d) == focal && d.substep_index == 0 {
            let (obs, menu) = super::decision_replay_key(&s, d.clone())?;
            starts.push(json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
                "state_hash":format!("{:016x}", s.diagnostic_state_hash())}));
        }
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("action {i}: {e:?}"))?;
    }
    let w = winner(&s)?;
    Ok(json!({"kind":"practice_starts","id":job.id,"focal":job.episode.learner_seat,
        "actions":job.actions.len(),"winner":w,"starts":starts}))
}

fn value_job(
    job: &ValueJob,
    parent: &mut crate::sideboard_play_policy_v1::FrozenPlayPolicyV1,
) -> Result<Value, String> {
    let reset = job.episode.reset.as_ref().ok_or("a value job needs a reset cut")?;
    let mut s = session(&job.episode)?;
    restore_reset_cut_v1(&mut s, &job.episode)?;
    let scores = parent.score_fast_session_v1(&s)?;
    let v = scores.value;
    Ok(json!({"kind":"practice_value","id":job.id,"step":reset.prefix.len(),
        "physical_decision_id":reset.cut_physical_decision_id,
        "value":if v.is_finite() { json!(v) } else { Value::Null },
        "value_bits":v.to_bits(),"finite":v.is_finite()}))
}

/// One transition for two trackers of the same focal seat.
fn step2(
    s: &mut FastActorSessionV1,
    a: u32,
    focal: PlayerId,
    trackers: &mut [&mut Suffix],
    defs: &SpyDefs,
) -> Result<(), String> {
    let d = decision(s).ok_or("no decision to step")?;
    let befores: Vec<Before> = trackers
        .iter()
        .map(|l| {
            let mut b = Before::take(s.game_state(), focal, defs);
            b.self_chosen_now = l.self_pending;
            if l.cast_pending {
                b.watch_spy_cast(s.game_state(), focal, defs);
            }
            b
        })
        .collect();
    s.step(d.episode_id, d.step, a)
        .map_err(|e| format!("step {} action {a}: {e:?}", d.step))?;
    for (l, b) in trackers.iter_mut().zip(befores) {
        l.apply(events(&b, s.game_state(), focal, defs));
        l.self_pending = false;
        l.cast_pending = false;
    }
    Ok(())
}

fn observe(l: &mut Suffix, s: &FastActorSessionV1, a: u32, focal: PlayerId, defs: &SpyDefs) {
    if let Some(sem) = s.diagnostic_current_action_semantics() {
        l.observe_menu(s, &sem, a as usize, focal, defs);
    }
}

fn exposure(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let episode: ExpandedEpisodeV1 =
        serde_json::from_value(doc["episode"].clone()).map_err(|e| format!("{path}: {e}"))?;
    let focal = PlayerId(episode.learner_seat);
    let defs = spy_defs();
    let mut s = session(&episode)?;
    // Whole-history tracker: inherited events through the prefix, then the
    // suffix. Fresh tracker: starts at the cut and must see the whole line.
    let mut whole = Suffix::new(true);
    let mut fresh = Suffix::new(true);
    let prefix: &[u32] = episode.reset.as_ref().map_or(&[], |r| &r.prefix);
    for (i, &a) in prefix.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("prefix ends before action {i}"))?;
        if acting(&d) == focal && d.legal_action_count >= 2 {
            observe(&mut whole, &s, a, focal, &defs);
        }
        step2(&mut s, a, focal, &mut [&mut whole], &defs)?;
    }
    if episode.reset.is_some() {
        // The trainer's own cut check, on a fresh session (the tracker
        // session above already applied the same prefix).
        let mut check = session(&episode)?;
        restore_reset_cut_v1(&mut check, &episode)?;
        if check.diagnostic_state_hash() != s.diagnostic_state_hash() {
            return Err("tracker prefix differs from the trainer's restore".into());
        }
    }
    let inherited = whole.json();
    let decisions = doc["decisions"].as_array().ok_or("decisions missing")?;
    let mut first_step = None;
    let (mut learner_rows, mut learner_choices) = (0u64, 0u64);
    let mut learner_physical = std::collections::BTreeSet::new();
    let mut first_value = None;
    let mut fresh_first: serde_json::Map<String, Value> = serde_json::Map::new();
    for (i, r) in decisions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("replay ends before suffix row {i}"))?;
        let step = r["step"].as_u64().ok_or("row step")?;
        let actor = r["actor"].as_u64().ok_or("row actor")? as u8;
        let selected = r["selected"].as_u64().ok_or("row selected")? as u32;
        if d.step != step
            || d.physical_decision_id != r["physical_decision_id"].as_u64().ok_or("row pd")?
            || acting(&d).0 != actor
        {
            return Err(format!(
                "suffix row {i} differs from the replay: replay step {} physical {} actor {} k {}, \
                 record step {step} physical {} actor {actor}",
                d.step,
                d.physical_decision_id,
                acting(&d).0,
                d.legal_action_count,
                r["physical_decision_id"]
            ));
        }
        first_step.get_or_insert(step);
        // Labels before this row's menu and transition: offered/chosen flags
        // are set by the menu observation, resolutions by the transition.
        let before = fresh.json();
        if actor == focal.0 {
            learner_rows += 1;
            learner_physical.insert(d.physical_decision_id);
            if first_value.is_none() {
                let bits = r["value"].as_u64().ok_or("row value")? as u32;
                first_value = Some(f32::from_bits(bits));
            }
            if d.legal_action_count >= 2 {
                learner_choices += 1;
                observe(&mut whole, &s, selected, focal, &defs);
                observe(&mut fresh, &s, selected, focal, &defs);
            }
        }
        step2(&mut s, selected, focal, &mut [&mut whole, &mut fresh], &defs)?;
        let after = fresh.json();
        for k in [
            "cast_offered", "cast_chosen", "self_offered", "self_chosen", "dr_giant_offered",
            "dr_giant_chosen", "spy_resolved", "self_target_resolved", "dr_giant_resolved",
        ] {
            if after[k] == json!(true) && before[k] != json!(true) && !fresh_first.contains_key(k) {
                fresh_first.insert(k.into(), json!({"step":step,
                    "physical_decision_id":d.physical_decision_id,"substep":d.substep_index}));
            }
        }
    }
    let w = winner(&s)?;
    let recorded = doc["terminal"]["winner"].as_str();
    let recorded = match recorded {
        Some("p0") => Some(0u8),
        Some("p1") => Some(1u8),
        _ => None,
    };
    if recorded != w {
        return Err("replayed winner differs from the recorded terminal".into());
    }
    let reward = match w {
        Some(x) if x == focal.0 => 1,
        Some(_) => -1,
        None => 0,
    };
    let inherited_complete = inherited["complete"] == json!(true);
    Ok(json!({"kind":"train_exposure","file":path,"id":episode.id,
        "learner_seat":focal.0,"learner_deck":episode.selected[focal.0 as usize].label,
        "reset":episode.reset.as_ref().map(|r| json!({"guide":r.guide,"cut_step":r.prefix.len(),
            "cut_physical_decision_id":r.cut_physical_decision_id})),
        "first_suffix_step":first_step,"suffix_rows":decisions.len(),
        "learner_rows":learner_rows,"learner_choices":learner_choices,
        "learner_physical_decisions":learner_physical.len(),
        "reward":reward,"first_learner_value":first_value.map(|v| v as f64),
        "inherited":inherited,"fresh":fresh.json(),"fresh_first":fresh_first,
        "whole":whole.json(),
        "fresh_full_sequence":fresh.complete(),
        "remaining_sequence_completed":!inherited_complete && whole.complete(),
        "inherited_complete":inherited_complete}))
}

fn lines(var: &str) -> Result<(String, String), String> {
    let path = std::env::var(var).map_err(|_| format!("{var} is required"))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    Ok((path, text))
}

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    let mut out = std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?;
    let mut write = |row: Value| writeln!(out, "{row}").map_err(|e| e.to_string());
    match cfg.mode.as_str() {
        "s4a-practice-starts" => {
            let (path, text) = lines("PRACTICE_JOBS")?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let job: StartsJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
                write(starts_job(&job).unwrap_or_else(|e| json!({"kind":"error","id":job.id,"error":e})))?;
            }
        }
        "s4a-practice-values" => {
            let (path, text) = lines("PRACTICE_JOBS")?;
            let parent_path =
                std::env::var("PRACTICE_PARENT").map_err(|_| "PRACTICE_PARENT is required")?;
            let source: ExpandedModelSourceV1 = serde_json::from_slice(
                &std::fs::read(&parent_path).map_err(|e| format!("{parent_path}: {e}"))?,
            )
            .map_err(|e| format!("{parent_path}: {e}"))?;
            let (mut parent, identity) = load_expanded_inference_v1(&source)?;
            write(json!({"kind":"practice_parent","checkpoint_sha256":identity.checkpoint_sha256,
                "adam_step":identity.adam_step,"state_sha256":identity.state_sha256}))?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let job: ValueJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
                write(value_job(&job, &mut parent)
                    .unwrap_or_else(|e| json!({"kind":"error","id":job.id,"error":e})))?;
            }
        }
        "s4a-train-exposure" => {
            let (_, text) = lines("EXPOSURE_EPISODES")?;
            for file in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                write(exposure(file).unwrap_or_else(|e| json!({"kind":"error","file":file,"error":e})))?;
            }
        }
        _ => return Err(format!("unknown practice mode {}", cfg.mode)),
    }
    Ok(())
}
