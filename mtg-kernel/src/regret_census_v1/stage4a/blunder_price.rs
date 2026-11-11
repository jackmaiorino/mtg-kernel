//! Blunder pricing for census cases (mode `s4a-blunder-price`; collab
//! LANES/pricing-20261111/DEFINITIONS.md). Measurement only: nothing that
//! plays, trains, samples or selects reads these rows.
//!
//! Each case restores a recorded census decision by replaying its label job
//! (record-checked as `s4a-blunder-census` does). `PRICE_MODE`:
//! - `price`: from the case's key decision, plays each of two forced arms
//!   (the recorded play and the definition's alternative) to the end of the
//!   game `PRICE_N` times in worlds resampled for the focal seat
//!   (`census_redeterminized_clone_v1`) and `PRICE_N_TRUE` times from the
//!   true state. Rollout k uses the same world and policy seeds in both arms.
//!   After the forced actions both seats play their census policies (raw,
//!   sampled), reset per rollout with `paired_policy_seeds_v1`.
//! - `score`: at each case's divergence decision, every scorer in
//!   `PRICE_SCORERS` scores the same state: the bad row's logit margin over
//!   the best other row, and the probability on the good row.
//!
//! Environment: `LABEL_JOBS`, `PRICE_CASES` (jsonl), `PRICE_MODE`,
//! `PRICE_SOURCES` (price: `{"candidate", "opponent"}` model sources),
//! `PRICE_SCORERS` (score: `[{"name", "source"}]`), `PRICE_N` (default 64),
//! `PRICE_N_TRUE` (default 32), `WIN_NODES` (default 5000), `WIN_DEPTH`
//! (default 400), `PRICE_TIMING` (optional per-case seconds file).

use super::blunder_census::{
    blocker_scan_menu, env_u64, matches_record, menu, pass_row, softmax, step, Session,
};
use super::play::{acting, decision};
use super::replay_labels::LabelJob;
use crate::ids::PlayerId;
use crate::learned_bo3_v1::public_evaluation::{load, ModelSource, Play};
use crate::paired_bo1_harness_v1::{
    paired_policy_seeds_v1, PairedBo1PolicyInputV1, PairedBo1PolicyV1,
};
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::FastActorResponseV1;
use crate::win_search_v1::{oracle_win_this_turn_v1, WinSearchBoundsV1, WinVerdictV1};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;

const STACK_BYTES: usize = 256 << 20;
/// Bound on decisions in one rollout (the evaluator's own limits end games
/// first; this only guards a runaway).
const MAX_STEPS: usize = 50_000;

/// A forced arm, applied from the key decision on.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Plan {
    /// These rows at the next focal decisions, in order.
    Rows { rows: Vec<u32> },
    /// The Pass row at the next focal decision.
    Pass,
    /// The exclude row at every focal blocker decision until the first focal
    /// decision that is not one.
    ExcludeScans,
    /// Include (true) or exclude (false) at each focal blocker decision, in
    /// order, until the first focal decision that is not one.
    ScanPath { path: Vec<bool> },
    /// At every focal decision of the starting turn, the oracle's proving
    /// line's first action when Proven, else the policy.
    OracleTurn,
}

/// The good row at a divergence decision.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Good {
    Row { row: u32 },
    Pass,
    Exclude,
    Include,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Divergence {
    index: usize,
    bad_row: u32,
    good: Good,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    class: String,
    index: usize,
    chosen: Plan,
    alt: Plan,
    divergence: Divergence,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sources {
    candidate: ModelSource,
    opponent: ModelSource,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scorer {
    name: String,
    source: ModelSource,
}

/// What a plan does at one focal decision.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Forced {
    /// Play this row.
    Row(u32),
    /// The plan is over: the policy plays from here.
    Done,
    /// The plan's row is not offered: the arm is invalid.
    Invalid,
    /// Ask the oracle (`OracleTurn`).
    Oracle,
}

/// A plan's progress.
pub(super) struct Cursor {
    plan: Plan,
    next: usize,
    done: bool,
}

impl Cursor {
    pub(super) fn new(plan: Plan) -> Self {
        Self {
            plan,
            next: 0,
            done: false,
        }
    }

    /// The forced action at a focal decision with menu `m` (`same_turn`:
    /// still the key decision's turn).
    pub(super) fn at(&mut self, m: &[ActionSemanticV1], same_turn: bool) -> Forced {
        if self.done {
            return Forced::Done;
        }
        let legal = m.len() as u32;
        let out = match &self.plan {
            Plan::Rows { rows } => match rows.get(self.next) {
                Some(&r) if r < legal => {
                    self.next += 1;
                    if self.next == rows.len() {
                        self.done = true;
                    }
                    return Forced::Row(r);
                }
                Some(_) => Forced::Invalid,
                None => Forced::Done,
            },
            Plan::Pass => {
                self.done = true;
                return pass_row(m).map_or(Forced::Invalid, Forced::Row);
            }
            Plan::ExcludeScans => match blocker_scan_menu(m) {
                Some(scan) => scan.exclude_row.map_or(Forced::Invalid, Forced::Row),
                None => Forced::Done,
            },
            Plan::ScanPath { path } => match (blocker_scan_menu(m), path.get(self.next)) {
                (Some(scan), Some(&include)) => {
                    self.next += 1;
                    let row = if include {
                        scan.include_row
                    } else {
                        scan.exclude_row
                    };
                    row.map_or(Forced::Invalid, Forced::Row)
                }
                (Some(_), None) => Forced::Invalid,
                (None, _) => Forced::Done,
            },
            Plan::OracleTurn => {
                if same_turn {
                    Forced::Oracle
                } else {
                    Forced::Done
                }
            }
        };
        if matches!(out, Forced::Done | Forced::Invalid) {
            self.done = true;
        }
        out
    }
}

/// The candidate (focal seat) and opponent policies of one panel.
struct Panel {
    candidate: Play,
    opponent: Play,
}

impl Panel {
    fn reset(&mut self, seed: u64) -> Result<(), String> {
        let seeds = paired_policy_seeds_v1(seed);
        self.candidate
            .reset_for_game_v1(seeds)
            .map_err(|e| e.to_string())?;
        self.opponent
            .reset_for_game_v1(seeds)
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rollout {
    score: f64,
    natural: bool,
    invalid: bool,
}

/// SplitMix64-style mix of a case key and a counter.
fn mix(key: &str, index: usize, k: u64, salt: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ salt;
    for byte in key.bytes() {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    let mut z = h ^ ((index as u64) << 20) ^ k.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn rollout(
    root: &Session,
    focal: PlayerId,
    plan: &Plan,
    panel: &mut Panel,
    seed: u64,
    bounds: WinSearchBoundsV1,
) -> Result<Rollout, String> {
    panel.reset(seed)?;
    let turn = (root.game_state().turn, root.game_state().active_player);
    let mut cursor = Cursor::new(plan.clone());
    let mut s = root.clone();
    for _ in 0..MAX_STEPS {
        let d = match s.current_response() {
            FastActorResponseV1::Decision(d) => d,
            FastActorResponseV1::Terminal(t) => {
                let score = match t.winner {
                    Some(w) if w == PlayerSeatV1::from(focal) => 1.0,
                    Some(_) => 0.0,
                    None => 0.5,
                };
                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                return Ok(Rollout {
                    score: if natural { score } else { 0.5 },
                    natural,
                    invalid: false,
                });
            }
        };
        let is_focal = acting(&d) == focal;
        let mut chosen = None;
        if is_focal {
            let state = s.game_state();
            let same_turn = (state.turn, state.active_player) == turn;
            match cursor.at(&menu(&s), same_turn) {
                Forced::Row(r) => chosen = Some(r),
                Forced::Invalid => {
                    return Ok(Rollout {
                        score: 0.5,
                        natural: false,
                        invalid: true,
                    })
                }
                Forced::Oracle if d.legal_action_count >= 2 => {
                    if let Some(result) = oracle_win_this_turn_v1(&s, bounds) {
                        if result.verdict == WinVerdictV1::Proven {
                            let m = menu(&s);
                            chosen = result.line.first().and_then(|l| {
                                m.iter()
                                    .position(|x| format!("{x:?}") == *l)
                                    .map(|i| i as u32)
                            });
                        }
                    }
                }
                Forced::Oracle | Forced::Done => {}
            }
        }
        let a = match chosen {
            Some(r) => r,
            None => {
                let play = if is_focal {
                    &mut panel.candidate
                } else {
                    &mut panel.opponent
                };
                play.select_action_v1(PairedBo1PolicyInputV1::new(&s, d))
                    .map_err(|e| e.to_string())?
            }
        };
        step(&mut s, &d, a)?;
    }
    Ok(Rollout {
        score: 0.5,
        natural: false,
        invalid: false,
    })
}

/// The session at decision `index` of the job, record-checked.
fn restore(job: &LabelJob, index: usize) -> Result<Session, String> {
    let mut s = crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )?;
    for i in 0..index {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        matches_record(&d, &job.decisions[i], i)?;
        step(&mut s, &d, job.decisions[i].selected)?;
    }
    let d = decision(&s).ok_or("the case decision is terminal")?;
    matches_record(&d, &job.decisions[index], index)?;
    Ok(s)
}

fn arms_json(rows: &[(Rollout, Rollout)]) -> Value {
    let scores = |f: fn(&(Rollout, Rollout)) -> Rollout| -> Vec<f64> {
        rows.iter().map(|r| f(r).score).collect()
    };
    let count = |f: fn(&(Rollout, Rollout)) -> Rollout, g: fn(&Rollout) -> bool| {
        rows.iter().filter(|r| g(&f(r))).count()
    };
    json!({"chosen": scores(|r| r.0), "alt": scores(|r| r.1),
        "invalid": [count(|r| r.0, |x| x.invalid), count(|r| r.1, |x| x.invalid)],
        "unnatural": [count(|r| r.0, |x| !x.natural && !x.invalid), count(|r| r.1, |x| !x.natural && !x.invalid)]})
}

fn price_case(
    job: &LabelJob,
    case: &Case,
    panel: &mut Panel,
    n: u64,
    n_true: u64,
    bounds: WinSearchBoundsV1,
) -> Result<Value, String> {
    let focal = PlayerId(job.focal);
    let s = restore(job, case.index)?;
    let d = decision(&s).ok_or("no decision")?;
    if acting(&d) != focal {
        return Err("the key decision is not the focal seat's".into());
    }
    let mut worlds = Vec::new();
    let mut world_errors = 0u64;
    for k in 0..n {
        let world =
            match s.census_redeterminized_clone_v1(mix(&case.id, case.index, k, 0x5752_4c44)) {
                Ok(w) => w,
                Err(_) => {
                    world_errors += 1;
                    continue;
                }
            };
        let seed = mix(&case.id, case.index, k, 0x504f_4c49);
        let chosen = rollout(&world, focal, &case.chosen, panel, seed, bounds)?;
        let alt = rollout(&world, focal, &case.alt, panel, seed, bounds)?;
        worlds.push((chosen, alt));
    }
    let mut truth = Vec::new();
    for k in 0..n_true {
        let seed = mix(&case.id, case.index, k, 0x5452_5545);
        let chosen = rollout(&s, focal, &case.chosen, panel, seed, bounds)?;
        let alt = rollout(&s, focal, &case.alt, panel, seed, bounds)?;
        truth.push((chosen, alt));
    }
    Ok(
        json!({"kind": "price", "id": case.id, "class": case.class, "index": case.index,
        "focal": job.focal, "worlds": arms_json(&worlds), "world_errors": world_errors,
        "true": arms_json(&truth)}),
    )
}

fn score_case(
    job: &LabelJob,
    case: &Case,
    scorers: &mut [(String, Play)],
) -> Result<Value, String> {
    let div = &case.divergence;
    let s = restore(job, div.index)?;
    let m = menu(&s);
    let good = match div.good {
        Good::Row { row } => Some(row),
        Good::Pass => pass_row(&m),
        Good::Exclude => blocker_scan_menu(&m).and_then(|x| x.exclude_row),
        Good::Include => blocker_scan_menu(&m).and_then(|x| x.include_row),
    }
    .filter(|&g| (g as usize) < m.len() && g != div.bad_row)
    .ok_or("the good row is not offered at the divergence decision")?;
    if div.bad_row as usize >= m.len() {
        return Err("the bad row is not offered".into());
    }
    let mut out = serde_json::Map::new();
    for (name, play) in scorers.iter_mut() {
        let p = play
            .legacy_policy_mut()
            .ok_or("scorers must be Legacy sources")?;
        let logits = p.score_fast_session_v1(&s)?.logits;
        let bad = logits[div.bad_row as usize];
        let best_other = logits
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != div.bad_row as usize)
            .map(|(_, &x)| x)
            .fold(f32::NEG_INFINITY, f32::max);
        let probs = softmax(&logits);
        out.insert(
            name.clone(),
            json!({"margin": f64::from(bad) - f64::from(best_other),
                "p_bad": probs[div.bad_row as usize], "p_good": probs[good as usize],
                "logit_bad": bad, "logit_good": logits[good as usize]}),
        );
    }
    Ok(
        json!({"kind": "score", "id": case.id, "class": case.class, "index": case.index,
        "divergence": div.index, "bad_row": div.bad_row, "good_row": good, "legal": m.len(),
        "scores": out}),
    )
}

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    let out = cfg.out.clone();
    std::thread::Builder::new()
        .name("s4a-blunder-price".into())
        .stack_size(STACK_BYTES)
        .spawn(move || run_on_worker(&out))
        .map_err(|e| format!("spawn pricing worker: {e}"))?
        .join()
        .map_err(|_| "pricing worker panicked".to_string())?
}

fn read_json<T: for<'de> Deserialize<'de>>(var: &str) -> Result<T, String> {
    let path = std::env::var(var).map_err(|_| format!("{var} is required"))?;
    serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|e| format!("{path}: {e}"))
}

fn read_lines<T: for<'de> Deserialize<'de>>(var: &str) -> Result<Vec<T>, String> {
    let path = std::env::var(var).map_err(|_| format!("{var} is required"))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{path}: {e}")))
        .collect()
}

fn run_on_worker(out_path: &str) -> Result<(), String> {
    let mode = std::env::var("PRICE_MODE").map_err(|_| "PRICE_MODE is required")?;
    let jobs: Vec<LabelJob> = read_lines("LABEL_JOBS")?;
    let cases: Vec<Case> = read_lines("PRICE_CASES")?;
    let by_id: BTreeMap<&str, &LabelJob> = jobs.iter().map(|j| (j.id.as_str(), j)).collect();
    let bounds = WinSearchBoundsV1 {
        max_nodes: env_u64("WIN_NODES", 5000)?,
        max_depth: env_u64("WIN_DEPTH", 400)? as u32,
    };
    let create = |p: &str| {
        std::fs::File::create(p)
            .map(std::io::BufWriter::new)
            .map_err(|e| format!("{p}: {e}"))
    };
    let mut out = create(out_path)?;
    let mut timing = match std::env::var("PRICE_TIMING") {
        Ok(p) => Some(create(&p)?),
        Err(_) => None,
    };
    let mut panel = None;
    let mut scorers = Vec::new();
    match mode.as_str() {
        "price" => {
            let sources: Sources = read_json("PRICE_SOURCES")?;
            panel = Some(Panel {
                candidate: load(&sources.candidate)?.0,
                opponent: load(&sources.opponent)?.0,
            });
        }
        "score" => {
            let list: Vec<Scorer> = read_json("PRICE_SCORERS")?;
            for x in list {
                scorers.push((x.name, load(&x.source)?.0));
            }
        }
        other => return Err(format!("unknown PRICE_MODE {other}")),
    }
    let n = env_u64("PRICE_N", 64)?;
    let n_true = env_u64("PRICE_N_TRUE", 32)?;
    for case in &cases {
        let started = std::time::Instant::now();
        let row = match by_id.get(case.id.as_str()) {
            None => Err("no label job for the case".to_string()),
            Some(job) => match panel.as_mut() {
                Some(p) => price_case(job, case, p, n, n_true, bounds),
                None => score_case(job, case, &mut scorers),
            },
        }
        .unwrap_or_else(
            |e| json!({"kind": "error", "id": case.id, "index": case.index, "error": e}),
        );
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        if let Some(t) = timing.as_mut() {
            writeln!(t, "{}", json!({"id": case.id, "index": case.index, "seconds": started.elapsed().as_secs_f64()}))
                .map_err(|e| e.to_string())?;
            t.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::CardStableRefV1;
    use crate::state::Zone;

    const ME: PlayerSeatV1 = PlayerSeatV1::P0;

    fn card(arena_id: u32) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id: 11,
            owner: ME,
            controller: ME,
            zone: Zone::Battlefield,
            zone_change_count: 1,
        }
    }

    fn block(attacker: u32, blocker: u32, include: bool) -> ActionSemanticV1 {
        ActionSemanticV1::ChooseBlockerInclusion {
            actor: ME,
            attacker: card(attacker),
            blocker: card(blocker),
            include,
        }
    }

    fn pass() -> ActionSemanticV1 {
        ActionSemanticV1::Pass { actor: ME }
    }

    fn scan() -> Vec<ActionSemanticV1> {
        vec![block(1, 2, false), block(1, 2, true)]
    }

    #[test]
    fn rows_are_forced_in_order_then_the_policy_plays() {
        let mut c = Cursor::new(Plan::Rows { rows: vec![1, 0] });
        assert_eq!(c.at(&scan(), true), Forced::Row(1));
        assert_eq!(c.at(&scan(), true), Forced::Row(0));
        assert_eq!(c.at(&scan(), true), Forced::Done);
        let mut c = Cursor::new(Plan::Rows { rows: vec![5] });
        assert_eq!(c.at(&scan(), true), Forced::Invalid);
        assert_eq!(c.at(&scan(), true), Forced::Done);
    }

    #[test]
    fn pass_needs_a_pass_row() {
        let mut c = Cursor::new(Plan::Pass);
        assert_eq!(c.at(&[block(1, 2, true), pass()], true), Forced::Row(1));
        assert_eq!(c.at(&[pass()], true), Forced::Done);
        let mut c = Cursor::new(Plan::Pass);
        assert_eq!(c.at(&scan(), true), Forced::Invalid);
    }

    #[test]
    fn exclude_scans_runs_through_the_declaration() {
        let mut c = Cursor::new(Plan::ExcludeScans);
        assert_eq!(c.at(&scan(), true), Forced::Row(0));
        assert_eq!(c.at(&scan(), true), Forced::Row(0));
        assert_eq!(c.at(&[pass()], true), Forced::Done);
        assert_eq!(c.at(&scan(), true), Forced::Done);
        let mut c = Cursor::new(Plan::ExcludeScans);
        assert_eq!(c.at(&[block(1, 2, true)], true), Forced::Invalid);
    }

    #[test]
    fn scan_path_follows_its_flags() {
        let mut c = Cursor::new(Plan::ScanPath {
            path: vec![true, false],
        });
        assert_eq!(c.at(&scan(), true), Forced::Row(1));
        assert_eq!(c.at(&scan(), true), Forced::Row(0));
        assert_eq!(c.at(&scan(), true), Forced::Invalid);
        let mut c = Cursor::new(Plan::ScanPath { path: vec![true] });
        assert_eq!(c.at(&scan(), true), Forced::Row(1));
        assert_eq!(c.at(&[pass()], true), Forced::Done);
    }

    #[test]
    fn oracle_turn_ends_with_the_turn() {
        let mut c = Cursor::new(Plan::OracleTurn);
        assert_eq!(c.at(&[pass()], true), Forced::Oracle);
        assert_eq!(c.at(&[pass()], true), Forced::Oracle);
        assert_eq!(c.at(&[pass()], false), Forced::Done);
        assert_eq!(c.at(&[pass()], true), Forced::Done);
    }

    #[test]
    fn mix_separates_cases_rollouts_and_salts() {
        let a = mix("p/o0-t0-k0-s0", 10, 0, 1);
        assert_ne!(a, mix("p/o0-t0-k0-s0", 10, 1, 1));
        assert_ne!(a, mix("p/o0-t0-k0-s0", 11, 0, 1));
        assert_ne!(a, mix("p/o0-t0-k0-s1", 10, 0, 1));
        assert_ne!(a, mix("p/o0-t0-k0-s0", 10, 0, 2));
        assert_eq!(a, mix("p/o0-t0-k0-s0", 10, 0, 1));
    }

    #[test]
    fn cases_parse() {
        let c: Case = serde_json::from_str(
            r#"{"id":"p/x","class":"c","index":3,
            "chosen":{"kind":"rows","rows":[1,0]},"alt":{"kind":"exclude_scans"},
            "divergence":{"index":3,"bad_row":1,"good":{"kind":"exclude"}}}"#,
        )
        .unwrap();
        assert_eq!(c.alt, Plan::ExcludeScans);
        let p: Plan = serde_json::from_str(r#"{"kind":"scan_path","path":[true]}"#).unwrap();
        assert_eq!(p, Plan::ScanPath { path: vec![true] });
        assert_eq!(
            serde_json::from_str::<Plan>(r#"{"kind":"oracle_turn"}"#).unwrap(),
            Plan::OracleTurn
        );
    }
}
