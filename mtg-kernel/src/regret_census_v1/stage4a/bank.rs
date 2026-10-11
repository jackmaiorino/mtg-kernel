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
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub(super) const ROW_KIND: &str = "bank_source";
pub(super) const URGENCY: f64 = 1.5;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeckSpec {
    pub(super) label: String,
    pub(super) mainboard: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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
fn outcome(s: &FastActorSessionV1, focal: PlayerId) -> Option<&'static str> {
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

fn err(e: PlayErr) -> String {
    format!("{e:?}")
}

/// Plain play for both seats to the end, logging every applied action.
fn plain_logged(
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
fn walk(
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
fn guide_record(
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

/// Completed source ids already in `out` (resume); a partial last line is
/// cut by the caller.
fn done_ids(bytes: &[u8], lineage: &str) -> Result<HashSet<String>, String> {
    let keep = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let text = std::str::from_utf8(&bytes[..keep]).map_err(|e| e.to_string())?;
    let mut done = HashSet::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if row["kind"] == ROW_KIND {
            if row["lineage"] != lineage || row["select_rule"] != "fpu-1.5" {
                return Err("resume refused: lineage or select rule differs".into());
            }
            done.insert(row["id"].as_str().unwrap_or("").to_owned());
        }
    }
    Ok(done)
}

pub(super) fn run_bank(
    cfg: &super::super::CensusConfigV1,
    policy: &FrozenPlayPolicyV1,
    sh: BankShared,
) -> Result<(), String> {
    sh.spec.validate()?;
    if let Ok(bytes) = std::fs::read(&cfg.out) {
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            let keep = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            std::fs::write(&cfg.out, &bytes[..keep]).map_err(|e| e.to_string())?;
        }
    }
    let done = match std::fs::read(&cfg.out) {
        Ok(bytes) => done_ids(&bytes, &sh.spec.lineage)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashSet::new(),
        Err(e) => return Err(e.to_string()),
    };
    let sink = Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&cfg.out)
            .map_err(|e| e.to_string())?,
    );
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
            let (sh, sink, next, done) = (&sh, &sink, &next, &done);
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
                    let row = source_row(sh, &mut roles, src).unwrap_or_else(
                        |e| json!({"kind":"error","id":src.id,"lineage":sh.spec.lineage,"error":e}),
                    );
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
