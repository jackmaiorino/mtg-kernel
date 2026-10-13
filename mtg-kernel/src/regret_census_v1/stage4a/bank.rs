//! Guide banks for the terminal-only reset curriculum pilot (collab
//! LANES/spy-learning-pilot-plan-20261010, RUNNER.md section 4), mode
//! `s4a-bank`.
//!
//! For each source spec in `BANK_SPEC` (generated and frozen by
//! `tools/spy_reset/schedule.py`):
//! 1. the source game: the frozen parent (`source=`) against the fixed
//!    guide opponent, plain policy for both seats, from the spec's game seed,
//!    starting player and decks, under the resolution-boundary profile from
//!    the opening and the training episode limits;
//! 2. if it ends naturally, one anchor among the focal non-forced physical
//!    decision starts (first substep, two or more legal actions), chosen by
//!    the spec's frozen uniform draw: `floor(draw * count / 2^64)`. The
//!    choice reads only the count, never cards, rewards or outcomes;
//! 3. two guides from that identical anchor and world: P continues with the
//!    plain parent; S runs one persistent search (Stage 4a candidate E with
//!    finite urgency 1.5, unchanged tree, sampler, prior, depth and
//!    eight-backup execution rule, 512,000 raw selection transitions), then
//!    executes the frozen tree along the actual world with the plain parent
//!    as fallback. Both use the same guide policy streams; no evaluation
//!    worlds are run.
//!
//! Each row records the actions (both seats) needed to replay any cut, the
//! focal physical decision starts at or after the anchor with their replay
//! keys, and passive Spy menu witnesses for the exposure check. Nothing in
//! the witnesses feeds back into play, anchors or cuts.

use super::arms::{EStats, Limits, Roles, RootCtx};
use super::labels::{classify, spy_defs, Suffix};
use super::play::{act, acting, apply, decision, Counters, Meter, PlayErr};
use super::seeds::RootSeeds;
use super::world::DeckPrior;
use super::{replay_key, sha, write_line};
use crate::ids::PlayerId;
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub(super) const ROW_KIND: &str = "bank_source";
pub(super) const URGENCY: f64 = 1.5;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeckSpec {
    pub(super) label: String,
    pub(super) mainboard: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceSpec {
    pub(super) id: String,
    pub(super) own: usize,
    pub(super) other: usize,
    pub(super) seat: u8,
    pub(super) opp: String,
    pub(super) seed: u64,
    pub(super) starting_player: u8,
    pub(super) source_policy_seeds: [u64; 2],
    pub(super) anchor_draw: u64,
    pub(super) guide_policy_seeds: [u64; 2],
    pub(super) search_root: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BankSpec {
    pub(super) schema: String,
    pub(super) namespace: String,
    pub(super) lineage: String,
    pub(super) runtime_rules: String,
    pub(super) select_rule: String,
    pub(super) max_physical_decisions: u64,
    pub(super) max_policy_steps: u64,
    pub(super) decks: Vec<DeckSpec>,
    pub(super) sources: Vec<SourceSpec>,
}

pub(super) const SPEC_SCHEMA: &str = "spy-reset-pilot-v1/bank-spec";

impl BankSpec {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema != SPEC_SCHEMA
            || self.namespace != "spy-reset-pilot-v1"
            || self.runtime_rules != "resolution-boundary-v1"
            || self.select_rule != "fpu-1.5"
        {
            return Err("bank spec schema, runtime rules or select rule differs".into());
        }
        let mut ids = HashSet::new();
        for s in &self.sources {
            if !ids.insert(s.id.as_str())
                || s.own >= self.decks.len()
                || s.other >= self.decks.len()
                || s.seat > 1
                || s.starting_player > 1
            {
                return Err(format!("invalid source spec {}", s.id));
            }
        }
        Ok(())
    }
}

/// The game a source spec names, built as the training collector builds its
/// episodes (`episode_session_v1`): same constructor, deck labels, mainboard
/// order, limits and runtime rules.
pub(super) fn source_session(
    spec: &BankSpec,
    src: &SourceSpec,
) -> Result<FastActorSessionV1, String> {
    let (focal, other) = (&spec.decks[src.own], &spec.decks[src.other]);
    let seats = if src.seat == 0 {
        [focal, other]
    } else {
        [other, focal]
    };
    let mut s = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        1,
        src.seed,
        spec.max_physical_decisions,
        spec.max_policy_steps,
        seats.map(|d| d.label.clone()),
        seats.map(|d| d.mainboard.clone()),
        PlayerId(src.starting_player),
    )
    .map_err(|e| format!("{e:?}"))?;
    s.set_runtime_rules_v1(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)?;
    Ok(s)
}

/// Natural outcome for the focal player, or "nonnatural".
pub(super) fn outcome(s: &FastActorSessionV1, focal: PlayerId) -> Option<&'static str> {
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return None;
    };
    if t.terminal_classification != crate::rl::TerminalClassificationV1::Natural {
        return Some("nonnatural");
    }
    Some(match t.winner {
        None => "draw",
        Some(w) if w == PlayerSeatV1::from(focal) => "win",
        Some(_) => "loss",
    })
}

/// The plain policy for one decision (`play::act`) with a string error.
pub(super) fn act_plain(
    p: &mut FrozenPlayPolicyV1,
    s: &FastActorSessionV1,
    d: &crate::rl_session::FastActorDecisionV1,
    c: &mut Counters,
) -> Result<u32, String> {
    act(p, s, d, c).map_err(err)
}

pub(super) fn err(e: PlayErr) -> String {
    format!("{e:?}")
}

/// Plain play for both seats to the end, logging every applied action.
pub(super) fn plain_logged(
    s: &mut FastActorSessionV1,
    focal: PlayerId,
    f: &mut FrozenPlayPolicyV1,
    o: &mut FrozenPlayPolicyV1,
    log: &mut Vec<u32>,
) -> Result<u64, String> {
    let defs = spy_defs();
    let mut meter = Meter::new(u64::MAX);
    let mut c = Counters::default();
    while let Some(d) = decision(s) {
        let a = if acting(&d) == focal {
            act(f, s, &d, &mut c)
        } else {
            act(o, s, &d, &mut c)
        }
        .map_err(err)?;
        log.push(a);
        apply(s, &d, a, &mut meter, focal, None, &defs).map_err(err)?;
    }
    Ok(c.inference)
}

/// Replays `actions` from `s`, calling `visit` before each action.
pub(super) fn walk(
    mut s: FastActorSessionV1,
    actions: &[u32],
    mut visit: impl FnMut(
        &FastActorSessionV1,
        &crate::rl_session::FastActorDecisionV1,
        u32,
    ) -> Result<(), String>,
) -> Result<FastActorSessionV1, String> {
    for &a in actions {
        let d = decision(&s).ok_or("replay reached a terminal early")?;
        visit(&s, &d, a)?;
        s.step(d.episode_id, d.step, a)
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(s)
}

/// Focal physical decision starts (forced included) and Spy menu witnesses
/// of a guide continuation from `anchor`.
pub(super) fn guide_record(
    anchor: &FastActorSessionV1,
    focal: PlayerId,
    actions: &[u32],
) -> Result<(Vec<Value>, Vec<Value>), String> {
    let defs = spy_defs();
    let mut starts = Vec::new();
    let mut witnesses = Vec::new();
    let end = walk(anchor.clone(), actions, |s, d, a| {
        if acting(d) != focal {
            return Ok(());
        }
        if d.substep_index == 0 {
            let (obs, menu) = replay_key(s, *d)?;
            starts.push(
                json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                "k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
                "state_hash":format!("{:016x}", s.diagnostic_state_hash())}),
            );
        }
        if d.legal_action_count >= 2 {
            let sem = s
                .diagnostic_current_action_semantics()
                .ok_or("missing semantics")?;
            let (mut own_offered, mut dr_offered) = (false, false);
            for x in &sem {
                let (_, own, dr) = classify(x, s.game_state(), focal, &defs);
                own_offered |= own;
                dr_offered |= dr;
            }
            if own_offered || dr_offered {
                let (_, own_chosen, dr_chosen) =
                    classify(&sem[a as usize], s.game_state(), focal, &defs);
                witnesses.push(
                    json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
                    "substep":d.substep_index,"k":d.legal_action_count,
                    "spy_self_offered":own_offered,"dr_giant_offered":dr_offered,
                    "spy_self_chosen":own_chosen,"dr_giant_chosen":dr_chosen}),
                );
            }
        }
        Ok(())
    })?;
    if outcome(&end, focal).is_none() {
        return Err("guide actions end before the terminal".into());
    }
    Ok((starts, witnesses))
}

pub(super) struct BankShared {
    pub(super) spec: BankSpec,
    pub(super) labels: Vec<String>,
    pub(super) opponents: Vec<FrozenPlayPolicyV1>,
    pub(super) prior: DeckPrior,
    pub(super) limits: Limits,
}

/// The generic root context of a guide search: any focal decision, no
/// stratum, Spy labels diagnostic only, finite urgency 1.5.
pub(super) fn guide_ctx<'a>(
    anchor: &'a FastActorSessionV1,
    focal: PlayerId,
    opp: usize,
    seeds: &'a RootSeeds,
    prior: &'a DeckPrior,
    limits: Limits,
) -> Result<RootCtx<'a>, String> {
    let d = decision(anchor).ok_or("anchor is terminal")?;
    Ok(RootCtx {
        visible_history: &[],
        root: anchor,
        d,
        focal,
        opp,
        seeds,
        prior,
        defs: spy_defs(),
        cast_root: false,
        probs: Vec::new(),
        limits,
        urgency: Some(URGENCY),
        key_mode: super::canon2::KeyMode::Legacy,
        interchange: false,
    })
}

pub(super) fn bank_roles(
    policy: &FrozenPlayPolicyV1,
    opponents: &[FrozenPlayPolicyV1],
) -> Result<Roles, String> {
    let fork = |p: &FrozenPlayPolicyV1| p.fork_for_collection_v3();
    Ok(Roles {
        focal: fork(policy)?,
        opps: opponents.iter().map(fork).collect::<Result<_, _>>()?,
        scorer: fork(policy)?,
        inner_focal: fork(policy)?,
        inner_opps: opponents.iter().map(fork).collect::<Result<_, _>>()?,
    })
}

/// The S guide's search at `anchor` and its frozen execution along the
/// actual world. Returns the selection record and the logged actions.
pub(super) fn search_guide(
    roles: &mut Roles,
    sh: &BankShared,
    anchor: &FastActorSessionV1,
    focal: PlayerId,
    opp: usize,
    seeds: &RootSeeds,
    guide_seeds: [u64; 2],
) -> Result<(Value, Vec<u32>, FastActorSessionV1), String> {
    let ctx = guide_ctx(anchor, focal, opp, seeds, &sh.prior, sh.limits)?;
    let (sel, tree) = roles.select_e(&ctx);
    let selection = sel.json();
    if sel.incomplete || !sel.faults.is_empty() {
        return Err(format!("search incomplete or faulted: {selection}"));
    }
    let mut s = anchor.clone();
    roles.focal.reset_sampling_v1(guide_seeds);
    roles.opps[opp].reset_sampling_v1(guide_seeds);
    let mut meter = Meter::new(u64::MAX);
    let mut c = Counters::default();
    let mut suffix = Suffix::new(false);
    let mut stats = EStats::default();
    let mut log = Vec::new();
    let end = roles
        .frozen(
            &ctx,
            &tree,
            &mut s,
            &mut meter,
            &mut c,
            &mut suffix,
            &mut stats,
            None,
            Some(&mut log),
        )
        .map_err(err)?;
    let _ = end;
    Ok((
        json!({"selection":selection,"execution":stats.json(),"execution_inference_calls":c.inference}),
        log,
        s,
    ))
}

fn source_row(sh: &BankShared, roles: &mut Roles, src: &SourceSpec) -> Result<Value, String> {
    let started = Instant::now();
    let spec = &sh.spec;
    let focal = PlayerId(src.seat);
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or_else(|| format!("unknown guide opponent {}", src.opp))?;
    let mut row = json!({"kind":ROW_KIND,"id":src.id,"lineage":spec.lineage,"own":src.own,
        "other":src.other,"seat":src.seat,"opp":src.opp,"seed":src.seed,
        "starting_player":src.starting_player,"runtime_rules":spec.runtime_rules,
        "select_rule":spec.select_rule,"limits":sh.limits.json()});
    // 1. Source game.
    let mut s = source_session(spec, src)?;
    roles.focal.reset_sampling_v1(src.source_policy_seeds);
    roles.opps[opp].reset_sampling_v1(src.source_policy_seeds);
    let mut source_actions = Vec::new();
    let (f, o) = (&mut roles.focal, &mut roles.opps[opp]);
    plain_logged(&mut s, focal, f, o, &mut source_actions)?;
    let source_end = outcome(&s, focal).ok_or("source did not end")?;
    // Eligible anchors: focal non-forced physical decision starts.
    let mut eligible: Vec<(u64, u64)> = Vec::new();
    walk(source_session(spec, src)?, &source_actions, |_, d, _| {
        if acting(d) == focal && d.substep_index == 0 && d.legal_action_count >= 2 {
            eligible.push((d.step, d.physical_decision_id));
        }
        Ok(())
    })?;
    row["source"] = json!({"end":source_end,"steps":source_actions.len(),
        "eligible_anchors":eligible.len(),"actions_sha256":sha(&actions_bytes(&source_actions)),
        "wall":started.elapsed().as_secs_f64()});
    if source_end == "nonnatural" || eligible.is_empty() {
        row["usable"] = json!(false);
        row["unusable_reason"] = json!(if source_end == "nonnatural" {
            "source non-natural"
        } else {
            "no focal non-forced decision"
        });
        row["anchor"] = Value::Null;
        row["guides"] = Value::Null;
        row["wall"] = json!(started.elapsed().as_secs_f64());
        return Ok(row);
    }
    let index = ((u128::from(src.anchor_draw) * eligible.len() as u128) >> 64) as usize;
    let (anchor_step, anchor_physical) = eligible[index];
    // 2. Restore the anchor from the source game's own actions.
    let anchor = walk(
        source_session(spec, src)?,
        &source_actions[..anchor_step as usize],
        |_, _, _| Ok(()),
    )?;
    let d = decision(&anchor).ok_or("anchor is terminal")?;
    if d.step != anchor_step || d.physical_decision_id != anchor_physical || acting(&d) != focal {
        return Err("anchor restore differs from the source game".into());
    }
    let (obs, menu) = replay_key(&anchor, d)?;
    row["anchor"] = json!({"index":index,"step":anchor_step,"physical_decision_id":anchor_physical,
        "k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
        "state_hash":format!("{:016x}", anchor.diagnostic_state_hash())});
    row["source_prefix"] = json!(source_actions[..anchor_step as usize]);
    // 3. Guides.
    let mut guides = serde_json::Map::new();
    // P: the plain parent from the anchor.
    let p_started = Instant::now();
    let mut p = anchor.clone();
    roles.focal.reset_sampling_v1(src.guide_policy_seeds);
    roles.opps[opp].reset_sampling_v1(src.guide_policy_seeds);
    let mut p_actions = Vec::new();
    let (f, o) = (&mut roles.focal, &mut roles.opps[opp]);
    plain_logged(&mut p, focal, f, o, &mut p_actions)?;
    let p_end = outcome(&p, focal).ok_or("P guide did not end")?;
    let (starts, witnesses) = guide_record(&anchor, focal, &p_actions)?;
    guides.insert(
        "P".into(),
        json!({"end":p_end,"actions":p_actions,"starts":starts,"witnesses":witnesses,
            "wall":p_started.elapsed().as_secs_f64()}),
    );
    // S: one search at the anchor, then its frozen tree on the actual world.
    let s_started = Instant::now();
    let seeds = RootSeeds {
        model: spec.lineage.clone(),
        root: src.search_root.clone(),
    };
    let (search, s_actions, s_final) = search_guide(
        roles,
        sh,
        &anchor,
        focal,
        opp,
        &seeds,
        src.guide_policy_seeds,
    )?;
    let s_end = outcome(&s_final, focal).ok_or("S guide did not end")?;
    let (starts, witnesses) = guide_record(&anchor, focal, &s_actions)?;
    guides.insert(
        "S".into(),
        json!({"end":s_end,"actions":s_actions,"starts":starts,"witnesses":witnesses,
            "search":search,"wall":s_started.elapsed().as_secs_f64()}),
    );
    let usable = p_end != "nonnatural" && s_end != "nonnatural";
    row["usable"] = json!(usable);
    row["unusable_reason"] = if usable {
        Value::Null
    } else {
        json!("guide non-natural")
    };
    row["guides"] = Value::Object(guides);
    row["wall"] = json!(started.elapsed().as_secs_f64());
    Ok(row)
}

fn actions_bytes(actions: &[u32]) -> Vec<u8> {
    actions.iter().flat_map(|a| a.to_le_bytes()).collect()
}

/// Bind every future row to the actual installed policies and all inputs.
/// Historical rows stay untouched and cannot be resumed under this binding.
pub(super) fn bank_binding(policy: &FrozenPlayPolicyV1, sh: &BankShared) -> Result<String, String> {
    let identity = |p: &FrozenPlayPolicyV1| {
        json!({"origin":p.identity_v1(),"actual":p.actual_model_identity_v1(),
            "sampler":p.runtime_sampler_identity_v1(),
            "max_actions":p.runtime_sampler_max_actions_v1()})
    };
    if sh.labels.len() != sh.opponents.len()
        || sh.labels.iter().collect::<HashSet<_>>().len() != sh.labels.len()
        || sh.spec.sources.iter().any(|s| !sh.labels.contains(&s.opp))
    {
        return Err("bank opponent labels must be unique and cover every source".into());
    }
    let binding = json!({"schema":"spy-reset-bank-binding-v1","spec":sh.spec,
        "focal":identity(policy),"opponents":sh.labels.iter().zip(&sh.opponents)
            .map(|(label, p)| json!({"label":label,"policy":identity(p)})).collect::<Vec<_>>(),
        "limits":sh.limits.json(),"prior":sh.prior,"urgency":URGENCY,
        "sampler":super::world::SAMPLER_VERSION,"seed_namespace":super::seeds::NAMESPACE,
        "git_head":env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256":env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "carddb_hash":format!("{:016x}",crate::card_def::KERNEL_CARDDB_HASH)});
    Ok(sha(
        &serde_json::to_vec(&binding).map_err(|e| e.to_string())?
    ))
}

/// Public decision witnesses for authenticated replay consumers.
pub(super) fn decision_identity(s: &FastActorSessionV1) -> Result<Value, String> {
    let d = decision(s).ok_or("decision is terminal")?;
    let (obs, menu) = replay_key(s, d)?;
    Ok(
        json!({"step":d.step,"physical_decision_id":d.physical_decision_id,
        "substep":d.substep_index,"k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
        "state_hash":format!("{:016x}",s.diagnostic_state_hash())}),
    )
}

/// Witnesses for new paired consumers. Historical bank rows remain untouched.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit paired source witnesses"
)]
pub(super) fn source_identity(
    sh: &BankShared,
    src: &SourceSpec,
    source: &FastActorSessionV1,
    actions: &[u32],
    anchor: &FastActorSessionV1,
    index: usize,
    eligible: usize,
    binding: &str,
) -> Result<Value, String> {
    let focal = PlayerId(src.seat);
    let end = outcome(source, focal).ok_or("source did not end")?;
    if end == "nonnatural" || eligible == 0 || index >= eligible {
        return Err("paired source must be natural and have an eligible anchor".into());
    }
    let d = decision(anchor).ok_or("anchor is terminal")?;
    if acting(&d) != focal || d.substep_index != 0 || d.legal_action_count < 2 {
        return Err("paired anchor is not an eligible focal decision".into());
    }
    let (obs, menu) = replay_key(anchor, d)?;
    Ok(json!({"id":src.id,"lineage":sh.spec.lineage,"own":src.own,
        "other":src.other,"seat":src.seat,"opp":src.opp,"seed":src.seed,
        "starting_player":src.starting_player,"runtime_rules":sh.spec.runtime_rules,
        "select_rule":sh.spec.select_rule,"limits":sh.limits.json(),
        "bank_binding_sha256":binding,
        "source":{"end":end,"steps":actions.len(),"eligible_anchors":eligible,
            "actions_sha256":sha(&actions_bytes(actions))},
        "anchor":{"index":index,"step":d.step,"physical_decision_id":d.physical_decision_id,
            "k":d.legal_action_count,"obs_hash":obs,"menu_hash":menu,
            "state_hash":format!("{:016x}",anchor.diagnostic_state_hash())}}))
}

#[derive(Debug)]
struct BankResume {
    done: HashSet<String>,
    keep: usize,
    needs_newline: bool,
}

/// Parse and authenticate all retained rows before any output mutation.
/// A complete unterminated record is retained; only an interrupted final
/// JSON record may be discarded after preceding rows have been validated.
fn bank_resume(
    bytes: &[u8],
    lineage: &str,
    binding: &str,
    source_ids: &HashSet<&str>,
) -> Result<BankResume, String> {
    let boundary = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let text = std::str::from_utf8(&bytes[..boundary]).map_err(|e| e.to_string())?;
    let mut done = HashSet::new();
    let mut admit = |row: Value| -> Result<(), String> {
        if row["lineage"] != lineage || row["bank_binding_sha256"] != binding {
            return Err("resume refused: bank input binding differs or is absent; preserve the output and use a new path".into());
        }
        let id = row["id"].as_str().ok_or("resume row has no source id")?;
        if !source_ids.contains(id) {
            return Err(format!("resume row names unknown source {id}"));
        }
        if row["kind"] == ROW_KIND && row["select_rule"] == "fpu-1.5" {
            if !done.insert(id.to_owned()) {
                return Err(format!("duplicate completed source {id}"));
            }
        } else if row["kind"] == "error" {
            return Err("resume refused: retained error row makes the bank inadmissible; preserve the output and use a new path".into());
        } else {
            return Err("resume row kind or select rule differs".into());
        }
        Ok(())
    };
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        admit(serde_json::from_str(line).map_err(|e| e.to_string())?)?;
    }
    let mut keep = boundary;
    let mut needs_newline = false;
    if boundary < bytes.len() {
        match serde_json::from_slice::<Value>(&bytes[boundary..]) {
            Ok(row) => {
                admit(row)?;
                keep = bytes.len();
                needs_newline = true;
            }
            Err(e) if e.is_eof() => {}
            Err(e) => return Err(format!("resume final row is malformed: {e}")),
        }
    }
    Ok(BankResume {
        done,
        keep,
        needs_newline,
    })
}

pub(super) fn run_bank(
    cfg: &super::super::CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: BankShared,
) -> Result<(), String> {
    sh.spec.validate()?;
    let binding = bank_binding(policy, &sh)?;
    let source_ids = sh.spec.sources.iter().map(|s| s.id.as_str()).collect();
    let bytes = match std::fs::read(&cfg.out) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.to_string()),
    };
    let resume = bank_resume(&bytes, &sh.spec.lineage, &binding, &source_ids)?;
    let mut output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.out)
        .map_err(|e| e.to_string())?;
    if resume.keep != bytes.len() {
        output
            .set_len(resume.keep as u64)
            .map_err(|e| e.to_string())?;
    }
    if resume.needs_newline {
        output.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    let done = resume.done;
    let sink = Mutex::new(output);
    let first = cfg.first_game as usize;
    let end = if cfg.games == 0 {
        sh.spec.sources.len()
    } else {
        (first + cfg.games as usize).min(sh.spec.sources.len())
    };
    eprintln!(
        "s4a-bank {}: sources {first}..{end}, {} already done, limits {}",
        sh.spec.lineage,
        done.len(),
        sh.limits.json()
    );
    let next = AtomicUsize::new(first);
    let started = Instant::now();
    std::thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::new();
        for _ in 0..cfg.workers {
            let mut roles = bank_roles(policy, &sh.opponents)?;
            let (sh, sink, next, done, binding) = (&sh, &sink, &next, &done, &binding);
            handles.push(scope.spawn(move || -> Result<(), String> {
                loop {
                    let item = next.fetch_add(1, Ordering::SeqCst);
                    if item >= end {
                        return Ok(());
                    }
                    let src = &sh.spec.sources[item];
                    if done.contains(&src.id) {
                        continue;
                    }
                    let mut row = source_row(sh, &mut roles, src).unwrap_or_else(
                        |e| json!({"kind":"error","id":src.id,"lineage":sh.spec.lineage,"error":e}),
                    );
                    row["bank_binding_sha256"] = json!(binding);
                    write_line(sink, &row)?;
                    eprintln!(
                        "source {} done at {:.1}s",
                        src.id,
                        started.elapsed().as_secs_f64()
                    );
                }
            }));
        }
        for h in handles {
            h.join().map_err(|_| "worker panicked")??;
        }
        Ok(())
    })
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    fn row() -> Value {
        json!({"kind":ROW_KIND,"id":"source-1","lineage":"r1",
            "select_rule":"fpu-1.5","bank_binding_sha256":"binding-1"})
    }

    fn parse(bytes: &[u8]) -> Result<BankResume, String> {
        bank_resume(bytes, "r1", "binding-1", &HashSet::from(["source-1"]))
    }

    #[test]
    fn bank_resume_preserves_complete_unterminated_record() {
        let bytes = serde_json::to_vec(&row()).unwrap();
        let resume = parse(&bytes).unwrap();
        assert_eq!(resume.done, HashSet::from(["source-1".to_owned()]));
        assert_eq!(resume.keep, bytes.len());
        assert!(resume.needs_newline);
    }

    #[test]
    fn bank_resume_refuses_changed_or_unbound_inputs_before_tail_recovery() {
        for binding in [Value::Null, json!("changed-spec-or-model")] {
            let mut r = row();
            r["bank_binding_sha256"] = binding;
            let bytes = format!("{r}\n{{\"interrupted\":");
            assert!(parse(bytes.as_bytes()).unwrap_err().contains("binding"));
        }
    }

    #[test]
    fn bank_resume_rejects_duplicate_and_unknown_sources() {
        let r = row();
        assert!(parse(format!("{r}\n{r}\n").as_bytes()).is_err());
        let mut unknown = r;
        unknown["id"] = json!("foreign-source");
        assert!(parse(format!("{unknown}\n").as_bytes()).is_err());
    }

    #[test]
    fn bank_resume_rejects_malformed_complete_records() {
        assert!(parse(b"{invalid}\n{\"interrupted\":").is_err());
        assert!(parse(b"{invalid}").is_err());
    }

    #[test]
    fn bank_resume_recovers_only_interrupted_final_record() {
        let complete = format!("{}\n", row());
        for tail in [b"{\"id\":".as_slice(), b"{\"id\":\"\xe2\x82".as_slice()] {
            let bytes = [complete.as_bytes(), tail].concat();
            let resume = parse(&bytes).unwrap();
            assert_eq!(resume.keep, complete.len());
            assert!(!resume.needs_newline);
            assert_eq!(resume.done.len(), 1);
        }
    }

    #[test]
    fn bank_resume_refuses_retained_error_rows_without_relabeling_evidence() {
        let mut r = row();
        r["kind"] = json!("error");
        assert!(parse(format!("{r}\n").as_bytes())
            .unwrap_err()
            .contains("retained error row"));
        r["bank_binding_sha256"] = Value::Null;
        assert!(parse(format!("{r}\n").as_bytes()).is_err());
    }
}
