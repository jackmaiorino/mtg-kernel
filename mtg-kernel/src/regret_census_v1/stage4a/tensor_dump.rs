//! Big-network capacity pilot dumps (collab LANES/bignet-pilot-20261031).
//! Measurement only: nothing that plays, trains or selects reads these rows.
//!
//! - `s4a-tensor-dump`: plain games (as `s4a-corpus`), one row per game. At
//!   every decision with two or more legal actions, the acting seat's policy
//!   selects through `select_with_training_tensor_v4`, the collector's own
//!   score-then-sample call, so the game is the plain-policy game and the
//!   recorded tensor is the exact V4 input that policy scored. Forced
//!   decisions take action 0 without a policy call, as `play::act`.
//! - `s4a-tensor-replay`: recorded evaluator games (`LABEL_JOBS`, as
//!   `s4a-label-replay`). The recorded actions are applied for both seats;
//!   at every focal decision the loaded policy scores the state (forced
//!   decisions included, as the W4 score pass) and the tensor is recorded.
//!   Each decision's step, physical decision, actor and legal count, and the
//!   winner, must equal the record.
//!
//! Tensors go to `TD_BIN` (appended; each game's block offset and length are
//! in its row). One decision record, all little-endian:
//! `u32 x 8` header (objects, edges, actions, refs, state length, selected,
//! legal count, actor), then `state` f32, `object_features` f32,
//! `object_card_ids` i32, `object_groups` i32, `object_node_ids` i32,
//! `edge_features` f32, `edge_source_indices` i32, `edge_target_indices` i32,
//! `action_features` f32, `action_ref_features` f32, `action_ref_card_ids`
//! i32, `action_ref_action_indices` i32, `action_ref_node_indices` i32,
//! `logits` f32 (one per action) and `value` f32.

use super::play::{acting, apply, decision, Meter};
use super::replay_labels::LabelJob;
use super::{game_setup, new_session, spy_defs, write_line, CensusConfigV1, Shared};
use crate::ids::PlayerId;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_policy_value_net_v1::{
    ACTION_FEATURE_DIM_V1, ACTION_REF_FEATURE_DIM_V1, EDGE_FEATURE_DIM_V1, OBJECT_FEATURE_DIM_V1,
};
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl_session::FastActorResponseV1;
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde_json::{json, Value};
use std::io::{Seek, SeekFrom, Write};
use std::sync::{Mutex, OnceLock};

static BIN: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

fn bin() -> Result<&'static Mutex<std::fs::File>, String> {
    if let Some(b) = BIN.get() {
        return Ok(b);
    }
    let path = std::env::var("TD_BIN").map_err(|_| "TD_BIN is required")?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("{path}: {e}"))?;
    Ok(BIN.get_or_init(|| Mutex::new(file)))
}

/// Appends one game's block and returns its offset.
fn put_block(block: &[u8]) -> Result<u64, String> {
    let mut f = bin()?.lock().map_err(|_| "bin poisoned")?;
    let offset = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    f.write_all(block).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())?;
    Ok(offset)
}

fn f32s(out: &mut Vec<u8>, xs: &[f32]) {
    xs.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
}

fn i32s(out: &mut Vec<u8>, xs: &[i64]) -> Result<(), String> {
    for &x in xs {
        let v = i32::try_from(x).map_err(|_| format!("index {x} outside i32"))?;
        out.extend_from_slice(&v.to_le_bytes());
    }
    Ok(())
}

fn u32_of(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| format!("count {n} outside u32"))
}

/// Appends one decision record; checks every array against its count.
fn encode(
    out: &mut Vec<u8>,
    t: &NativeFlatDecisionTensorV2,
    logits: &[f32],
    value: f32,
    selected: u32,
    legal: u32,
    actor: u8,
) -> Result<(), String> {
    let objects = t.object_card_ids.len();
    let edges = t.edge_source_indices.len();
    let actions = logits.len();
    let refs = t.action_ref_action_indices.len();
    let shapes = [
        (t.object_features.len(), objects * OBJECT_FEATURE_DIM_V1),
        (t.object_groups.len(), objects),
        (t.object_node_ids.len(), objects),
        (t.edge_features.len(), edges * EDGE_FEATURE_DIM_V1),
        (t.edge_target_indices.len(), edges),
        (t.action_features.len(), actions * ACTION_FEATURE_DIM_V1),
        (t.action_ref_features.len(), refs * ACTION_REF_FEATURE_DIM_V1),
        (t.action_ref_card_ids.len(), refs),
        (t.action_ref_node_indices.len(), refs),
    ];
    if shapes.iter().any(|(got, want)| got != want) || legal as usize != actions {
        return Err(format!("tensor shape mismatch {shapes:?}, legal {legal}, actions {actions}"));
    }
    for n in [objects, edges, actions, refs, t.state.len()] {
        out.extend_from_slice(&u32_of(n)?.to_le_bytes());
    }
    for n in [selected, legal, u32::from(actor)] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    f32s(out, &t.state);
    f32s(out, &t.object_features);
    i32s(out, &t.object_card_ids)?;
    i32s(out, &t.object_groups)?;
    i32s(out, &t.object_node_ids)?;
    f32s(out, &t.edge_features);
    i32s(out, &t.edge_source_indices)?;
    i32s(out, &t.edge_target_indices)?;
    f32s(out, &t.action_features);
    f32s(out, &t.action_ref_features);
    i32s(out, &t.action_ref_card_ids)?;
    i32s(out, &t.action_ref_action_indices)?;
    i32s(out, &t.action_ref_node_indices)?;
    f32s(out, logits);
    f32s(out, &[value]);
    Ok(())
}

fn seat_u8(seat: crate::rl::PlayerSeatV1) -> u8 {
    u8::from(seat == crate::rl::PlayerSeatV1::P1)
}

/// `s4a-tensor-dump`: one plain game.
pub(super) fn dump_game(
    cfg: &CensusConfigV1,
    shared: &Shared,
    focal: &mut FrozenPlayPolicyV1,
    opps: &mut [FrozenPlayPolicyV1],
    game: u64,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let setup = game_setup(cfg, shared.labels.len(), game);
    let opp = &mut opps[setup.model];
    let mut s = new_session(&setup)?;
    let seeds = paired_policy_seeds_v1(setup.seed);
    focal.reset_sampling_v1(seeds);
    opp.reset_sampling_v1(seeds);
    let focal_id = PlayerId(setup.focal as u8);
    let defs = spy_defs();
    let mut meter = Meter::new(u64::MAX);
    let mut block = Vec::new();
    let mut rows = Vec::new();
    let mut forced = 0u64;
    while let Some(d) = decision(&s) {
        let actor = acting(&d);
        let a = if d.legal_action_count < 2 {
            forced += 1;
            0
        } else {
            let p = if actor == focal_id { &mut *focal } else { &mut *opp };
            let (a, scores, tensor) = p.select_with_training_tensor_v4(&s)?;
            let start = block.len();
            encode(
                &mut block,
                &tensor.common,
                &scores.logits,
                scores.value,
                a,
                d.legal_action_count,
                actor.0,
            )?;
            rows.push(json!([d.step, actor.0, d.legal_action_count, a,
                s.game_state().turn, start, block.len() - start]));
            a
        };
        apply(&mut s, &d, a, &mut meter, focal_id, None, &defs).map_err(|e| format!("{e:?}"))?;
    }
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("game ended without a terminal".into());
    };
    let offset = put_block(&block)?;
    let row = json!({"kind":"td_game","game":game,"seed":setup.seed,
        "decks":[RUNTIME_DECKS[setup.decks[0]].id,RUNTIME_DECKS[setup.decks[1]].id],
        "focal_seat":setup.focal,"opp_model":shared.labels[setup.model],
        "starting_player":setup.starting,
        "classification":format!("{:?}", t.terminal_classification),
        "winner":t.winner.map(seat_u8),"forced":forced,
        "bin_offset":offset,"bin_bytes":block.len(),"decisions":rows});
    write_line(sink, &row)
}

/// `s4a-tensor-replay`: every job in `LABEL_JOBS`, scored by `policy`.
pub(super) fn replay(cfg: &CensusConfigV1, policy: &FrozenPlayPolicyV1) -> Result<(), String> {
    let path = std::env::var("LABEL_JOBS").map_err(|_| "LABEL_JOBS is required")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let sink = Mutex::new(std::fs::File::create(&cfg.out).map_err(|e| e.to_string())?);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let job: LabelJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        let row = replay_job(&job, policy)
            .unwrap_or_else(|e| json!({"kind":"error","id":job.id,"error":e}));
        write_line(&sink, &row)?;
    }
    Ok(())
}

fn replay_job(job: &LabelJob, policy: &FrozenPlayPolicyV1) -> Result<Value, String> {
    let focal = PlayerId(job.focal);
    let mut p = policy.fork_for_collection_v3()?;
    p.reset_sampling_v1(paired_policy_seeds_v1(job.environment_seed));
    let mut s = crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )?;
    let mut block = Vec::new();
    let mut rows = Vec::new();
    for (i, r) in job.decisions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        if d.step != r.step
            || d.physical_decision_id != r.physical_decision_id
            || acting(&d).0 != r.actor
            || d.legal_action_count != r.legal_action_count
        {
            return Err(format!("replay differs from the record at decision {i}"));
        }
        if acting(&d) == focal {
            let scores = p.score_fast_session_v1(&s)?;
            let tensor = p.last_scored_training_tensor_v4()?;
            let start = block.len();
            encode(
                &mut block,
                &tensor.common,
                &scores.logits,
                scores.value,
                r.selected,
                d.legal_action_count,
                job.focal,
            )?;
            rows.push(json!([i, d.legal_action_count, r.selected,
                s.game_state().turn, start, block.len() - start]));
        }
        s.step(d.episode_id, d.step, r.selected)
            .map_err(|e| format!("{e:?}"))?;
    }
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("the record ends before the terminal".into());
    };
    let winner = t.winner.map(seat_u8);
    if winner != job.winner {
        return Err("winner differs from the record".into());
    }
    let offset = put_block(&block)?;
    Ok(json!({"kind":"td_replay","id":job.id,"focal":job.focal,
        "deck_ids":job.config.deck_ids,"winner":winner,
        "bin_offset":offset,"bin_bytes":block.len(),"decisions":rows}))
}
