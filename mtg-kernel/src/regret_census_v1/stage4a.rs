//! Stage 4a: qualify persistent sequence search for Spy (collab
//! LANES/spy-discovery-plan-20261009, DESIGN.md and RUNNER.md).
//!
//! Diagnostic only: no training, store records or gates. Modes:
//! - `s4a-corpus`: plain development games (focal model against the
//!   `OPPONENTS` models under the nine-deck setup). One row per game; for
//!   Spy-focal games, the earliest cast-Spy and Spy-self-target-available
//!   decisions before any Spy self-target, with replay keys. Rows carry no
//!   outcome. `tools/stage4a/freeze_roots.py` freezes the roots.
//! - `s4a-run`: for each frozen root in `ROOTS`, selection by E, A and D
//!   under the transition ceiling, then 16 paired evaluation worlds per arm.
//!   One row per root; a root already present in `out` is skipped (resume).
//! - `s4a-diag`: E only for each frozen root, unchanged selection from
//!   simulation zero and the same evaluation worlds, for the Spy execution
//!   diagnosis (collab LANES/spy-execution-diagnosis-plan-20261010). With
//!   `S4A_TRACE=<dir>`, writes a passive decision trace per root there
//!   (`diag.rs`); the row's E fields are identical with or without it.
//!
//! Settings: `S4A_MODEL` (seed namespace model label, e.g. r1), `ROOTS`,
//! `OPPONENTS` (`label=source,...`, in game-setup order), and optionally
//! `S4A_LIMITS=select_cap,eval_worlds,eval_cap` for cost-only engineering
//! checks (rows record the limits and whether they are formal).

mod arms;
mod diag;
mod labels;
mod play;
mod seeds;
mod tree;
mod world;

use super::search::{game_setup, new_session, GameSetup};
use super::{load_policy_v1, CensusConfigV1};
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use arms::{Limits, Roles, RootCtx, SamplerStats, ARMS};
use labels::{root_strata, spy_defs};
use play::{act, acting, apply, decision, Counters, Meter};
use seeds::{Purpose, RootSeeds};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub(super) fn is_mode(mode: &str) -> bool {
    matches!(mode, "s4a-corpus" | "s4a-run" | "s4a-diag")
}

const SPY_DECK: &str = "Spy";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Replay key of a focal decision: canonical observation and menu hashes.
fn replay_key(
    s: &crate::rl_session::FastActorSessionV1,
    d: crate::rl_session::FastActorDecisionV1,
) -> Result<(String, String), String> {
    let c = tree::canon(s, d).map_err(|e| format!("{e:?}"))?;
    let mut menu = Vec::new();
    for m in &c.menu {
        menu.extend_from_slice(&(m.len() as u64).to_le_bytes());
        menu.extend_from_slice(m);
    }
    Ok((sha(&c.obs), sha(&menu)))
}

struct Shared {
    labels: Vec<String>,
    opponents: Vec<FrozenPlayPolicyV1>,
    roots: Vec<Value>,
    model: String,
    limits: Limits,
    prior: world::DeckPrior,
}

fn load_shared(cfg: &CensusConfigV1) -> Result<Shared, String> {
    let entries: Vec<(String, String)> = std::env::var("OPPONENTS")
        .map_err(|_| "OPPONENTS is required")?
        .split(',')
        .filter(|x| !x.trim().is_empty())
        .map(|x| {
            x.split_once('=')
                .map(|(l, p)| (l.trim().to_owned(), p.trim().to_owned()))
                .ok_or_else(|| format!("OPPONENTS entry {x} needs label=path"))
        })
        .collect::<Result<_, _>>()?;
    let mut labels = Vec::new();
    let mut opponents = Vec::new();
    for (l, p) in entries {
        opponents.push(load_policy_v1(&p)?);
        labels.push(l);
    }
    let model = std::env::var("S4A_MODEL").map_err(|_| "S4A_MODEL is required")?;
    let limits = match std::env::var("S4A_LIMITS") {
        Ok(v) => {
            let x: Vec<u64> = v
                .split(',')
                .map(|t| t.trim().parse().map_err(|_| format!("bad S4A_LIMITS {v}")))
                .collect::<Result<_, _>>()?;
            if x.len() != 3 {
                return Err("S4A_LIMITS needs select_cap,eval_worlds,eval_cap".into());
            }
            Limits {
                select_cap: x[0],
                eval_worlds: x[1],
                eval_cap: x[2],
            }
        }
        Err(_) => Limits::FORMAL,
    };
    let roots = if cfg.mode != "s4a-corpus" {
        let path = std::env::var("ROOTS").map_err(|_| "ROOTS names no roots file")?;
        std::fs::read_to_string(&path)
            .map_err(|e| format!("{path}: {e}"))?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
            .collect::<Result<Vec<Value>, _>>()?
    } else {
        Vec::new()
    };
    Ok(Shared {
        labels,
        opponents,
        roots,
        model,
        limits,
        prior: world::DeckPrior::new(&cfg.decks),
    })
}

fn write_line(sink: &Mutex<std::fs::File>, row: &Value) -> Result<(), String> {
    let mut f = sink.lock().map_err(|_| "sink poisoned")?;
    writeln!(f, "{row}").map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())
}

/// Plays a game with the plain policy (`act`), calling `visit` at every
/// decision with the action about to be applied; `visit` returning true
/// stops before applying it. Corpus and replay share this loop.
fn drive(
    setup: &GameSetup,
    focal: &mut FrozenPlayPolicyV1,
    opp: &mut FrozenPlayPolicyV1,
    mut visit: impl FnMut(
        &crate::rl_session::FastActorSessionV1,
        &crate::rl_session::FastActorDecisionV1,
        u32,
    ) -> Result<bool, String>,
) -> Result<(crate::rl_session::FastActorSessionV1, bool), String> {
    let mut s = new_session(setup)?;
    let seeds = paired_policy_seeds_v1(setup.seed);
    focal.reset_sampling_v1(seeds);
    opp.reset_sampling_v1(seeds);
    let focal_id = crate::ids::PlayerId(setup.focal as u8);
    let defs = spy_defs();
    let mut c = Counters::default();
    let mut meter = Meter::new(u64::MAX);
    loop {
        let Some(d) = decision(&s) else {
            return Ok((s, true));
        };
        let a = if acting(&d) == focal_id {
            act(focal, &s, &d, &mut c)
        } else {
            act(opp, &s, &d, &mut c)
        }
        .map_err(|e| format!("{e:?}"))?;
        if visit(&s, &d, a)? {
            return Ok((s, false));
        }
        apply(&mut s, &d, a, &mut meter, focal_id, None, &defs).map_err(|e| format!("{e:?}"))?;
    }
}

fn root_record(
    s: &crate::rl_session::FastActorSessionV1,
    d: &crate::rl_session::FastActorDecisionV1,
    a: u32,
) -> Result<Value, String> {
    let (obs, menu) = replay_key(s, *d)?;
    let st = s.game_state();
    Ok(
        json!({"step":d.step,"turn":st.turn,"phase":format!("{:?}",st.step),
        "k":d.legal_action_count,"physical_decision_id":d.physical_decision_id,
        "substep":[d.substep_index,d.substep_count],"obs_hash":obs,"menu_hash":menu,"plain_action":a}),
    )
}

/// `s4a-corpus`: one game.
fn corpus_game(
    cfg: &CensusConfigV1,
    shared: &Shared,
    focal: &mut FrozenPlayPolicyV1,
    opps: &mut [FrozenPlayPolicyV1],
    game: u64,
    sink: &Mutex<std::fs::File>,
) -> Result<(), String> {
    let setup = game_setup(cfg, shared.labels.len(), game);
    let deck = RUNTIME_DECKS[setup.decks[setup.focal]].id;
    let spy = deck == SPY_DECK;
    let focal_id = crate::ids::PlayerId(setup.focal as u8);
    let defs = spy_defs();
    let mut self_targeted = false;
    let (mut cast_root, mut target_root) = (Value::Null, Value::Null);
    let mut decisions = 0u64;
    let (_, finished) = drive(&setup, focal, &mut opps[setup.model], |s, d, a| {
        decisions += 1;
        if spy && acting(d) == focal_id && d.legal_action_count >= 2 && !self_targeted {
            let sem = s
                .diagnostic_current_action_semantics()
                .ok_or("missing semantics")?;
            let (cast, target) = root_strata(&sem, s.game_state(), focal_id, &defs);
            if cast && cast_root.is_null() {
                cast_root = root_record(s, d, a)?;
            }
            if target && target_root.is_null() {
                target_root = root_record(s, d, a)?;
            }
            if labels::classify(&sem[a as usize], s.game_state(), focal_id, &defs).1 {
                self_targeted = true;
            }
        }
        Ok(false)
    })?;
    let row = json!({"kind":"s4a_game","game":game,"seed":setup.seed,"deck":deck,
        "opp_deck":RUNTIME_DECKS[setup.decks[1-setup.focal]].id,"opp_model":shared.labels[setup.model],
        "opp_model_index":setup.model,"focal_seat":setup.focal,"starting_player":setup.starting,
        "decisions":decisions,"finished":finished,"spy_focal":spy,
        "cast_root":cast_root,"target_root":target_root});
    write_line(sink, &row)
}

/// Replays a frozen root's game to its step and checks every replay key.
fn replay(
    cfg: &CensusConfigV1,
    shared: &Shared,
    roles: &mut Roles,
    root: &Value,
) -> Result<(GameSetup, crate::rl_session::FastActorSessionV1), String> {
    let game = root["game"].as_u64().ok_or("root has no game")?;
    let step = root["step"].as_u64().ok_or("root has no step")?;
    let setup = game_setup(cfg, shared.labels.len(), game);
    for (name, got, want) in [
        ("seed", json!(setup.seed), root["seed"].clone()),
        ("focal_seat", json!(setup.focal), root["focal_seat"].clone()),
        (
            "opp_model",
            json!(shared.labels[setup.model]),
            root["opp_model"].clone(),
        ),
        (
            "deck",
            json!(RUNTIME_DECKS[setup.decks[setup.focal]].id),
            root["deck"].clone(),
        ),
    ] {
        if got != want {
            return Err(format!("replay mismatch: {name} {got} != {want}"));
        }
    }
    let mut found = None;
    let (s, finished) = drive(
        &setup,
        &mut roles.focal,
        &mut roles.opps[setup.model],
        |s, d, a| {
            if d.step > step {
                return Err(format!("replay passed step {step}"));
            }
            if d.step == step {
                found = Some(root_record(s, d, a)?);
                return Ok(true);
            }
            Ok(false)
        },
    )?;
    if finished {
        return Err("replay reached the end before the root".into());
    }
    let got = found.ok_or("replay stopped without a root")?;
    for key in [
        "turn",
        "k",
        "physical_decision_id",
        "obs_hash",
        "menu_hash",
        "plain_action",
    ] {
        if got[key] != root[key] {
            return Err(format!(
                "replay mismatch: {key} {} != {}",
                got[key], root[key]
            ));
        }
    }
    Ok((setup, s))
}

fn primary_hash(row: &Value) -> String {
    let mut r = row.clone();
    if let Value::Object(m) = &mut r {
        m.remove("timing");
        m.remove("primary_sha256");
    }
    sha(r.to_string().as_bytes())
}

/// `s4a-run`: one frozen root, all arms.
fn run_root(
    cfg: &CensusConfigV1,
    shared: &Shared,
    roles: &mut Roles,
    root: &Value,
) -> Result<Value, String> {
    let started = Instant::now();
    let root_id = root["root_id"]
        .as_str()
        .ok_or("root has no root_id")?
        .to_owned();
    let (setup, session) = replay(cfg, shared, roles, root)?;
    let replay_secs = started.elapsed().as_secs_f64();
    let d = decision(&session).ok_or("root is terminal")?;
    let seeds = RootSeeds {
        model: shared.model.clone(),
        root: root_id.clone(),
    };
    let probs = arms::softmax(&roles.scorer.score_fast_session_v1(&session)?.logits);
    let cast_root = match root["stratum"].as_str() {
        Some("cast") => true,
        Some("target") => false,
        other => return Err(format!("root stratum {other:?}")),
    };
    let ctx = RootCtx {
        root: &session,
        d,
        focal: crate::ids::PlayerId(setup.focal as u8),
        opp: setup.model,
        seeds: &seeds,
        prior: &shared.prior,
        defs: spy_defs(),
        cast_root,
        probs: probs.clone(),
        limits: shared.limits,
    };
    let (e_sel, e_tree) = roles.select_e(&ctx);
    let a_cands = arms::a_candidates(&probs);
    let a_sel = roles.select_rounds(&ctx, "A", &a_cands);
    let d_cands = arms::d_candidates(&probs, &seeds);
    let d_sel = roles.select_rounds(&ctx, "D", &d_cands);
    let eval_started = Instant::now();
    let mut eval_sampler = SamplerStats::default();
    let mut worlds = Vec::new();
    let mut rejected_worlds = Vec::new();
    let mut per_arm: Vec<Vec<Value>> = vec![Vec::new(); ARMS.len()];
    let mut summary = vec![(0u64, 0u64, 0u64, 0u64, 0u64); ARMS.len()];
    for e in 0..shared.limits.eval_worlds {
        let seed = seeds.get(Purpose::EvalWorld, "", e, b"world");
        let world = match world::sample(&session, seed, &shared.prior) {
            Ok(w) => w,
            Err(err) => {
                rejected_worlds
                    .push(json!({"world":e,"error":err.chars().take(300).collect::<String>()}));
                continue;
            }
        };
        worlds.push(json!({"world":e,"prior_deck":shared.prior.ids()[world.prior_deck]}));
        for (i, arm) in ARMS.iter().enumerate() {
            let out = match *arm {
                "E" => roles.evaluate(
                    &ctx,
                    arm,
                    &world.world,
                    e,
                    None,
                    Some(&e_tree),
                    &mut eval_sampler,
                ),
                "A" => roles.evaluate(
                    &ctx,
                    arm,
                    &world.world,
                    e,
                    a_sel.choice.as_ref(),
                    None,
                    &mut eval_sampler,
                ),
                _ => roles.evaluate(
                    &ctx,
                    arm,
                    &world.world,
                    e,
                    d_sel.choice.as_ref(),
                    None,
                    &mut eval_sampler,
                ),
            };
            let s = &mut summary[i];
            s.0 += u64::from(out.w);
            s.1 += u64::from(out.j);
            s.2 += u64::from(out.unknown);
            s.3 += u64::from(out.fault.is_some());
            s.4 += out.transitions;
            let mut v = out.json();
            v["world"] = json!(e);
            per_arm[i].push(v);
        }
    }
    let eval_wall = eval_started.elapsed().as_secs_f64();
    let sel = [&e_sel, &a_sel, &d_sel];
    let mut arms_json = serde_json::Map::new();
    for (i, arm) in ARMS.iter().enumerate() {
        let j_any = per_arm[i].iter().any(|v| v["j"] == json!(true));
        arms_json.insert(
            (*arm).into(),
            json!({"selection":sel[i].json(),"eval":per_arm[i],
                "summary":{"w":summary[i].0,"j":summary[i].1,"unknown":summary[i].2,"faults":summary[i].3,
                    "eval_transitions":summary[i].4,"j_any":j_any}}),
        );
    }
    let invalid = !rejected_worlds.is_empty()
        || summary.iter().any(|s| s.3 > 0)
        || sel.iter().any(|s| !s.faults.is_empty())
        || sel.iter().any(|s| s.incomplete);
    let mut row = json!({"kind":"s4a_root","root_id":root_id,"model":shared.model,
        "cell":root["cell"],"stratum":root["stratum"],"game":root["game"],"seed":root["seed"],
        "step":root["step"],"opp_model":root["opp_model"],"focal_seat":setup.focal,
        "starting_player":setup.starting,"opp_deck":RUNTIME_DECKS[setup.decks[1-setup.focal]].id,
        "probs":probs,"k":d.legal_action_count,
        "config":{"limits":shared.limits.json(),"sampler":world::SAMPLER_VERSION,
            "prior_decks":shared.prior.ids(),"opponents":shared.labels,"seed_namespace":seeds::NAMESPACE},
        "arms":arms_json,"eval_worlds":worlds,"rejected_eval_worlds":rejected_worlds,
        "eval_sampler":eval_sampler.json(),"invalid":invalid,
        "cost":{"selection_transitions":sel.iter().map(|s| s.transitions).sum::<u64>(),
            "selection_inference_calls":sel.iter().map(|s| s.inference).sum::<u64>(),
            "eval_transitions":summary.iter().map(|s| s.4).sum::<u64>()},
        "timing":{"replay":replay_secs,"selection_wall":{"E":e_sel.wall,"A":a_sel.wall,"D":d_sel.wall},
            "selection_sampler_seconds":{"E":e_sel.sampler.seconds,"A":a_sel.sampler.seconds,"D":d_sel.sampler.seconds},
            "eval_wall":eval_wall,"eval_sampler_seconds":eval_sampler.seconds,
            "root_wall":started.elapsed().as_secs_f64()}});
    row["primary_sha256"] = json!(primary_hash(&row));
    Ok(row)
}

/// `s4a-diag`: one frozen root, E only. Selection, worlds and execution are
/// those of `run_root`; the E fields of the row equal the archived E
/// projection. With `S4A_TRACE=<dir>` a passive trace is written to
/// `<dir>/<root_id>.trace.jsonl`; its hash goes under `diag`, outside the
/// E fields.
fn run_root_diag(
    cfg: &CensusConfigV1,
    shared: &Shared,
    roles: &mut Roles,
    root: &Value,
) -> Result<Value, String> {
    let started = Instant::now();
    let root_id = root["root_id"]
        .as_str()
        .ok_or("root has no root_id")?
        .to_owned();
    let (setup, mut session) = replay(cfg, shared, roles, root)?;
    // `S4A_RUNTIME=resolution-boundary-v1`: continue from the replayed root
    // under the opt-in rules profile (a new runtime identity).
    let runtime = match std::env::var("S4A_RUNTIME").ok().as_deref() {
        None | Some("") | Some("historical") => None,
        Some("resolution-boundary-v1") => Some(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1),
        Some(other) => return Err(format!("unknown S4A_RUNTIME {other}")),
    };
    if let Some(r) = runtime {
        session.set_runtime_rules_v1(r);
    }
    let replay_secs = started.elapsed().as_secs_f64();
    let d = decision(&session).ok_or("root is terminal")?;
    let seeds = RootSeeds {
        model: shared.model.clone(),
        root: root_id.clone(),
    };
    let probs = arms::softmax(&roles.scorer.score_fast_session_v1(&session)?.logits);
    let cast_root = match root["stratum"].as_str() {
        Some("cast") => true,
        Some("target") => false,
        other => return Err(format!("root stratum {other:?}")),
    };
    let ctx = RootCtx {
        root: &session,
        d,
        focal: crate::ids::PlayerId(setup.focal as u8),
        opp: setup.model,
        seeds: &seeds,
        prior: &shared.prior,
        defs: spy_defs(),
        cast_root,
        probs: probs.clone(),
        limits: shared.limits,
    };
    let trace_dir = std::env::var("S4A_TRACE").ok();
    let mut trace = trace_dir.as_ref().map(|_| diag::Trace::default());
    if let (Some(t), Ok(n)) = (trace.as_mut(), std::env::var("S4A_MILLOBS")) {
        t.millobs = Some(diag::MillObs {
            want: n.parse().map_err(|_| format!("bad S4A_MILLOBS {n}"))?,
            ..Default::default()
        });
    }
    if let Some(t) = trace.as_mut() {
        t.meta_line(json!({"r":"meta","schema":"s4a-diag-trace/v1","root_id":root_id,
            "model":shared.model,"stratum":root["stratum"],"cast_root":cast_root,"focal_seat":setup.focal,
            "opp_model":root["opp_model"],"limits":shared.limits.json(),"k":d.legal_action_count,
            "event_bits":"1 spy resolved, 2 self-target resolved, 4 DR->Giant resolved, 8 DR->Giant on stack",
            "action_bits":"1 cast Spy, 2 Spy targets focal, 4 DR targets Giant, 8 Spy targets other, 16 DR targets other"}));
    }
    let (e_sel, e_tree) = roles.select_e_traced(&ctx, trace.as_mut());
    let eval_started = Instant::now();
    let mut eval_sampler = SamplerStats::default();
    let mut worlds = Vec::new();
    let mut rejected_worlds = Vec::new();
    let mut per_world: Vec<Value> = Vec::new();
    let mut summary = (0u64, 0u64, 0u64, 0u64, 0u64);
    for e in 0..shared.limits.eval_worlds {
        let seed = seeds.get(Purpose::EvalWorld, "", e, b"world");
        let world = match world::sample(&session, seed, &shared.prior) {
            Ok(w) => w,
            Err(err) => {
                rejected_worlds
                    .push(json!({"world":e,"error":err.chars().take(300).collect::<String>()}));
                continue;
            }
        };
        worlds.push(json!({"world":e,"prior_deck":shared.prior.ids()[world.prior_deck]}));
        if let Some(t) = trace.as_mut() {
            t.world_begin(e);
        }
        let out = roles.evaluate_traced(
            &ctx,
            "E",
            &world.world,
            e,
            None,
            Some(&e_tree),
            &mut eval_sampler,
            trace.as_mut(),
        );
        summary.0 += u64::from(out.w);
        summary.1 += u64::from(out.j);
        summary.2 += u64::from(out.unknown);
        summary.3 += u64::from(out.fault.is_some());
        summary.4 += out.transitions;
        let mut v = out.json();
        v["world"] = json!(e);
        if let Some(t) = trace.as_mut() {
            t.world_end(v.clone());
        }
        per_world.push(v);
    }
    let eval_wall = eval_started.elapsed().as_secs_f64();
    let j_any = per_world.iter().any(|v| v["j"] == json!(true));
    let mut arms_json = serde_json::Map::new();
    arms_json.insert(
        "E".into(),
        json!({"selection":e_sel.json(),"eval":per_world,
            "summary":{"w":summary.0,"j":summary.1,"unknown":summary.2,"faults":summary.3,
                "eval_transitions":summary.4,"j_any":j_any}}),
    );
    let invalid = !rejected_worlds.is_empty()
        || summary.3 > 0
        || !e_sel.faults.is_empty()
        || e_sel.incomplete;
    let mut row = json!({"kind":"s4a_diag_root","root_id":root_id,"model":shared.model,
        "cell":root["cell"],"stratum":root["stratum"],"game":root["game"],"seed":root["seed"],
        "step":root["step"],"opp_model":root["opp_model"],"focal_seat":setup.focal,
        "starting_player":setup.starting,"opp_deck":RUNTIME_DECKS[setup.decks[1-setup.focal]].id,
        "probs":probs,"k":d.legal_action_count,
        "config":{"limits":shared.limits.json(),"sampler":world::SAMPLER_VERSION,
            "prior_decks":shared.prior.ids(),"opponents":shared.labels,"seed_namespace":seeds::NAMESPACE},
        "arms":arms_json,"eval_worlds":worlds,"rejected_eval_worlds":rejected_worlds,
        "eval_sampler":eval_sampler.json(),"invalid":invalid,"runtime_rules":runtime.map(|_| "resolution-boundary-v1"),
        "cost":{"selection_transitions":e_sel.transitions,"selection_inference_calls":e_sel.inference,
            "eval_transitions":summary.4},
        "timing":{"replay":replay_secs,"selection_wall":{"E":e_sel.wall},
            "selection_sampler_seconds":{"E":e_sel.sampler.seconds},
            "eval_wall":eval_wall,"root_wall":started.elapsed().as_secs_f64()}});
    let mut diag_info = json!({"trace":null});
    if let (Some(dir), Some(t)) = (trace_dir, trace) {
        let mut bytes = t.lines.join("\n").into_bytes();
        bytes.push(b'\n');
        let path = std::path::Path::new(&dir).join(format!("{root_id}.trace.jsonl"));
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        diag_info = json!({"trace":{"path":path.display().to_string(),"sha256":sha(&bytes),
            "lines":t.lines.len(),"bytes":bytes.len()}});
    }
    diag_info["root_wall_with_trace_io"] = json!(started.elapsed().as_secs_f64());
    row["diag"] = diag_info;
    Ok(row)
}

fn fork_roles(policy: &FrozenPlayPolicyV1, shared: &Shared) -> Result<Roles, String> {
    Ok(Roles {
        focal: policy.fork_for_collection_v3()?,
        opps: shared
            .opponents
            .iter()
            .map(FrozenPlayPolicyV1::fork_for_collection_v3)
            .collect::<Result<_, _>>()?,
        scorer: policy.fork_for_collection_v3()?,
        inner_focal: policy.fork_for_collection_v3()?,
        inner_opps: shared
            .opponents
            .iter()
            .map(FrozenPlayPolicyV1::fork_for_collection_v3)
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn run(cfg: &CensusConfigV1, policy: &FrozenPlayPolicyV1) -> Result<(), String> {
    let shared = load_shared(cfg)?;
    let row_kind = if cfg.mode == "s4a-diag" {
        "s4a_diag_root"
    } else {
        "s4a_root"
    };
    let done: HashSet<String> = if cfg.mode != "s4a-corpus" {
        std::fs::read_to_string(&cfg.out)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["kind"] == row_kind)
            .filter_map(|v| v["root_id"].as_str().map(str::to_owned))
            .collect()
    } else {
        HashSet::new()
    };
    // A kill can leave a partial last line; cut it so the next row starts
    // on its own line (its root is simply rerun).
    if let Ok(bytes) = std::fs::read(&cfg.out) {
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            let keep = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            std::fs::write(&cfg.out, &bytes[..keep]).map_err(|e| e.to_string())?;
            eprintln!(
                "cut a partial last line ({} bytes) from {}",
                bytes.len() - keep,
                cfg.out
            );
        }
    }
    let sink = Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&cfg.out)
            .map_err(|e| e.to_string())?,
    );
    let (first, end) = if cfg.mode != "s4a-corpus" {
        (0, shared.roots.len() as u64)
    } else {
        (cfg.first_game, cfg.first_game + cfg.games)
    };
    eprintln!(
        "stage4a {}: model {}, opponents {:?}, items {first}..{end}, limits {}, {} already done",
        cfg.mode,
        shared.model,
        shared.labels,
        shared.limits.json(),
        done.len()
    );
    let next = AtomicU64::new(first);
    let started = Instant::now();
    std::thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::new();
        for _ in 0..cfg.workers {
            let mut roles = fork_roles(policy, &shared)?;
            let (shared, sink, next, done) = (&shared, &sink, &next, &done);
            handles.push(scope.spawn(move || -> Result<(), String> {
                loop {
                    let item = next.fetch_add(1, Ordering::SeqCst);
                    if item >= end {
                        return Ok(());
                    }
                    let result = if cfg.mode == "s4a-corpus" {
                        corpus_game(cfg, shared, &mut roles.focal, &mut roles.opps, item, sink)
                    } else {
                        let root = &shared.roots[item as usize];
                        let id = root["root_id"].as_str().unwrap_or("").to_owned();
                        if done.contains(&id) {
                            continue;
                        }
                        let row = if cfg.mode == "s4a-diag" {
                            run_root_diag(cfg, shared, &mut roles, root)
                        } else {
                            run_root(cfg, shared, &mut roles, root)
                        };
                        row.and_then(|row| write_line(sink, &row))
                    };
                    if let Err(e) = result {
                        eprintln!("item {item} failed: {e}");
                        let id = shared
                            .roots
                            .get(item as usize)
                            .map(|r| r["root_id"].clone());
                        write_line(
                            sink,
                            &json!({"kind":"error","item":item,"root_id":id,"error":e}),
                        )?;
                    }
                    eprintln!(
                        "item {item} done at {:.1}s",
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
mod tests;
