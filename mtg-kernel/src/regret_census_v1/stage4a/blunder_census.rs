//! Blunder census for recorded evaluator games (mode `s4a-blunder-census`;
//! collab LANES/census-20261110/DEFINITIONS.md). Measurement only: nothing
//! that plays, trains, samples or selects reads these rows.
//!
//! Each label job is rebuilt exactly as `s4a-label-replay` rebuilds it (the
//! evaluator's constructor, opening and runtime rules) and checked against
//! its record at every decision. Along the replay it counts, for the focal
//! seat:
//! - (a) certified wins this turn that were not taken (oracle
//!   `win_search_v1` at own-turn decisions with two or more actions; only
//!   when `CENSUS_WIN` is `1`, only on jobs whose seed family is below
//!   `CENSUS_WIN_FAMILIES`);
//! - (b) casts and activations whose every offered target was the focal
//!   seat's own permanent or the focal player, and those whose resolution
//!   removed or damaged an own target;
//! - (c) chump blocks (blocker died, attacker lived) with the no-block
//!   counterfactual's life;
//! - (d) losses to combat damage after a block declaration, with an
//!   exhaustive walk over the legal declarations for a surviving one.
//!
//! At every case position the candidate is scored on the live session and
//! the case row carries the softmax of its logits.
//!
//! Environment: `LABEL_JOBS` (jsonl), `CENSUS_SOURCE` (JSON model source of
//! the candidate, as the plan carries it), `CENSUS_WIN` (`1` runs (a),
//! default `0`), `CENSUS_WIN_FAMILIES` (default 2), `WIN_NODES` (default
//! 5000), `WIN_DEPTH` (default 400), `CENSUS_CASES` (case rows path; the
//! per-game rows go to `out`).

use super::play::{acting, decision};
use super::replay_labels::LabelJob;
use crate::ids::{ObjectId, PlayerId};
use crate::learned_bo3_v1::public_evaluation::{load, ModelSource, Play};
use crate::paired_bo1_harness_v1::{paired_policy_seeds_v1, PairedBo1PolicyV1};
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::state::{Step, Target, Zone};
use crate::win_search_v1::{oracle_win_this_turn_v1, WinSearchBoundsV1, WinVerdictV1};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

type Session = FastActorSessionV1;

const STACK_BYTES: usize = 256 << 20;
/// Bound on steps in one passive combat continuation.
const SIM_STEPS: usize = 4000;
/// Bound on leaves in the (d) walk.
const MAX_LEAVES: u64 = 4096;
/// Menu rows kept in a case row (highest probability first).
const TOP_ROWS: usize = 8;

fn env_u64(name: &str, default: u64) -> Result<u64, String> {
    match std::env::var(name) {
        Ok(text) => text
            .parse()
            .map_err(|_| format!("{name} must be an integer")),
        Err(_) => Ok(default),
    }
}

fn seat(p: PlayerId) -> PlayerSeatV1 {
    PlayerSeatV1::from(p)
}

fn step(s: &mut Session, d: &FastActorDecisionV1, a: u32) -> Result<(), String> {
    s.step(d.episode_id, d.step, a)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

fn menu(s: &Session) -> Vec<ActionSemanticV1> {
    s.diagnostic_current_action_semantics().unwrap_or_default()
}

fn pass_row(m: &[ActionSemanticV1]) -> Option<u32> {
    m.iter()
        .position(|x| matches!(x, ActionSemanticV1::Pass { .. }))
        .map(|i| i as u32)
}

/// The focal seat lost (a natural or other terminal with the other winner).
fn focal_lost(s: &Session, focal: PlayerId) -> Option<bool> {
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return None;
    };
    Some(t.winner.is_some_and(|w| w != seat(focal)))
}

/// Softmax of the candidate's logits on the live session.
/// A position the scorer cannot score yields no probabilities (counted), never
/// a lost game.
struct Scorer {
    play: Play,
    failures: u64,
}

impl Scorer {
    fn probs(&mut self, s: &Session) -> Result<Vec<f64>, String> {
        let p = self
            .play
            .legacy_policy_mut()
            .ok_or("the candidate must be a Legacy source")?;
        let Ok(scores) = p.score_fast_session_v1(s) else {
            self.failures += 1;
            return Ok(Vec::new());
        };
        let max = scores
            .logits
            .iter()
            .fold(f64::NEG_INFINITY, |m, &x| m.max(f64::from(x)));
        let e: Vec<f64> = scores
            .logits
            .iter()
            .map(|&x| (f64::from(x) - max).exp())
            .collect();
        let z: f64 = e.iter().sum();
        Ok(e.into_iter().map(|x| x / z).collect())
    }

    /// The chosen row's probability, the top rows with their semantics.
    fn menu_json(&mut self, s: &Session, chosen: u32) -> Result<Value, String> {
        let p = self.probs(s)?;
        let m = menu(s);
        let mut order: Vec<usize> = (0..p.len()).collect();
        order.sort_by(|&a, &b| p[b].total_cmp(&p[a]));
        let top: Vec<Value> = order
            .iter()
            .take(TOP_ROWS)
            .map(|&i| json!({"row": i, "p": p[i], "action": m.get(i).map(|x| format!("{x:?}"))}))
            .collect();
        let pass = pass_row(&m);
        Ok(
            json!({"legal": p.len(), "chosen": chosen, "p_chosen": p.get(chosen as usize),
            "chosen_action": m.get(chosen as usize).map(|x| format!("{x:?}")),
            "p_pass": pass.and_then(|r| p.get(r as usize)), "top": top}),
        )
    }
}

// ---------------------------------------------------------------- combat helpers

/// One focal blocker-inclusion decision: (attacker, blocker) and its rows.
struct BlockerScan {
    include_row: Option<u32>,
    exclude_row: Option<u32>,
}

fn blocker_scan(s: &Session, d: &FastActorDecisionV1, focal: PlayerId) -> Option<BlockerScan> {
    if acting(d) != focal {
        return None;
    }
    let m = s.diagnostic_current_action_semantics()?;
    let mut scan: Option<(u32, u32, BlockerScan)> = None;
    for (row, action) in m.iter().enumerate() {
        let ActionSemanticV1::ChooseBlockerInclusion {
            attacker,
            blocker,
            include,
            ..
        } = action
        else {
            return None;
        };
        let entry = scan.get_or_insert((
            attacker.arena_id,
            blocker.arena_id,
            BlockerScan {
                include_row: None,
                exclude_row: None,
            },
        ));
        if (entry.0, entry.1) != (attacker.arena_id, blocker.arena_id) {
            return None;
        }
        if *include {
            entry.2.include_row = Some(row as u32);
        } else {
            entry.2.exclude_row = Some(row as u32);
        }
    }
    scan.map(|x| x.2)
}

fn presence(s: &Session, id: ObjectId) -> Option<u32> {
    s.game_state()
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield)
        .map(|o| o.zone_change_count)
}

/// `w4_combat`'s rule: combat damage, its state-based actions and triggers
/// are over at the current decision.
fn past_combat(s: &Session, turn: (u32, PlayerId)) -> bool {
    let state = s.game_state();
    if (state.turn, state.active_player) != turn || (state.step as u8) > (Step::CombatDamage as u8)
    {
        return true;
    }
    state.step == Step::CombatDamage
        && state.stack.is_empty()
        && state.engine.pending_triggers.is_empty()
        && s.diagnostic_current_action_semantics()
            .is_some_and(|m| m.iter().any(|x| matches!(x, ActionSemanticV1::Pass { .. })))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimEnd {
    CombatOver,
    Terminal,
    StepBound,
    StepError,
}

impl SimEnd {
    fn as_str(self) -> &'static str {
        match self {
            SimEnd::CombatOver => "combat_over",
            SimEnd::Terminal => "terminal",
            SimEnd::StepBound => "step_bound",
            SimEnd::StepError => "step_error",
        }
    }
}

/// Passive continuation: focal blocker decisions take `choose`'s row (None:
/// the scan cannot follow the request), every other decision passes when it
/// can, otherwise its first row; until combat is over or the game ends.
fn passive(
    mut s: Session,
    focal: PlayerId,
    turn: (u32, PlayerId),
    choose: &mut dyn FnMut(&BlockerScan) -> Option<u32>,
) -> (Session, SimEnd, bool) {
    let mut followed = true;
    for _ in 0..SIM_STEPS {
        let Some(d) = decision(&s) else {
            return (s, SimEnd::Terminal, followed);
        };
        let a = if let Some(scan) = blocker_scan(&s, &d, focal) {
            match choose(&scan) {
                Some(row) => row,
                None => {
                    followed = false;
                    scan.include_row.or(scan.exclude_row).unwrap_or(0)
                }
            }
        } else if past_combat(&s, turn) {
            return (s, SimEnd::CombatOver, followed);
        } else {
            pass_row(&menu(&s)).unwrap_or(0)
        };
        if step(&mut s, &d, a).is_err() {
            return (s, SimEnd::StepError, followed);
        }
    }
    (s, SimEnd::StepBound, followed)
}

/// A combat end survives: no focal loss and focal life above 0.
fn survives(s: &Session, end: SimEnd, focal: PlayerId) -> bool {
    match end {
        SimEnd::CombatOver => s.game_state().players[focal.index()].life > 0,
        SimEnd::Terminal => focal_lost(s, focal) == Some(false),
        _ => false,
    }
}

struct Walk {
    leaves: u64,
    bounded: bool,
}

/// Depth-first over every offered row of every focal blocker decision; the
/// include/exclude path of the first surviving leaf.
fn walk(
    mut s: Session,
    focal: PlayerId,
    turn: (u32, PlayerId),
    path: &mut Vec<bool>,
    w: &mut Walk,
) -> Option<Vec<bool>> {
    for _ in 0..SIM_STEPS {
        let Some(d) = decision(&s) else {
            w.leaves += 1;
            return survives(&s, SimEnd::Terminal, focal).then(|| path.clone());
        };
        if let Some(scan) = blocker_scan(&s, &d, focal) {
            for (include, row) in [(true, scan.include_row), (false, scan.exclude_row)] {
                let Some(row) = row else { continue };
                if w.leaves >= MAX_LEAVES {
                    w.bounded = true;
                    return None;
                }
                let mut next = s.clone();
                if step(&mut next, &d, row).is_err() {
                    continue;
                }
                path.push(include);
                let found = walk(next, focal, turn, path, w);
                path.pop();
                if found.is_some() {
                    return found;
                }
            }
            return None;
        }
        if past_combat(&s, turn) {
            w.leaves += 1;
            return survives(&s, SimEnd::CombatOver, focal).then(|| path.clone());
        }
        let a = pass_row(&menu(&s)).unwrap_or(0);
        if step(&mut s, &d, a).is_err() {
            w.leaves += 1;
            return None;
        }
    }
    w.leaves += 1;
    w.bounded = true;
    None
}

/// A combat the focal seat defends, from its first blocker decision.
struct Combat {
    turn: (u32, PlayerId),
    start: Session,
    start_index: usize,
    /// (decision index, include chosen, include p, exclude p, rows offered)
    scans: Vec<Value>,
    /// The recorded rows of the focal blocker decisions, in order.
    recorded: Vec<u32>,
    forced_include: bool,
    /// (attacker, blocker, attacker zcc, blocker zcc) once declared.
    declared: Option<Vec<(ObjectId, ObjectId, Option<u32>, Option<u32>)>>,
}

// ---------------------------------------------------------------- casts

/// A focal cast or activation that may lead to target steps.
struct Cast {
    index: usize,
    source: u32,
    card_db_id: u16,
    kind: &'static str,
    cast_menu: Value,
    stack_len: usize,
    steps: Vec<Value>,
    own_only: bool,
    decisions_left: u32,
    turn: (u32, PlayerId),
}

/// A focal stack item from an own-only cast, until it leaves the stack.
struct Watch {
    case: Value,
    position: usize,
    source: ObjectId,
    /// (object, zcc, damage) of each own targeted permanent at creation.
    targets: Vec<(ObjectId, u32, u32)>,
    library: usize,
    opp_life: i32,
    opp_permanents: usize,
}

fn target_steps_source(m: &[ActionSemanticV1]) -> Option<(u32, u16)> {
    let mut src = None;
    let mut any_target = false;
    for x in m {
        let s = match x {
            ActionSemanticV1::ChooseTarget { source, .. } => {
                any_target = true;
                source
            }
            ActionSemanticV1::FinishTargetSelection { source, .. } => source,
            _ => return None,
        };
        match src {
            None => src = Some((s.arena_id, s.card_db_id)),
            Some((a, _)) if a != s.arena_id => return None,
            _ => {}
        }
    }
    if any_target {
        src
    } else {
        None
    }
}

fn own_target(t: &TargetRefV1, focal: PlayerSeatV1) -> bool {
    match t {
        TargetRefV1::Player { player } => *player == focal,
        TargetRefV1::Object { object } => object.controller == focal,
        TargetRefV1::StackItem { .. } => false,
    }
}

fn bump(n: &mut BTreeMap<&'static str, u64>, k: &'static str) {
    *n.entry(k).or_default() += 1;
}

fn permanents(s: &Session, p: PlayerId) -> usize {
    s.game_state().players[p.index()].battlefield.len()
}

// ---------------------------------------------------------------- the job

struct Options {
    win: bool,
    win_families: u64,
    bounds: WinSearchBoundsV1,
}

fn family(id: &str) -> Option<u64> {
    let tail = id.rsplit('/').next()?;
    let k = tail.split('-').find(|p| p.starts_with('k'))?;
    k[1..].parse().ok()
}

fn case(job: &LabelJob, kind: &str, index: usize, s: &Session, rest: Value) -> Value {
    let state = s.game_state();
    let mut row = json!({"kind": kind, "id": job.id, "focal": job.focal, "index": index,
        "turn": state.turn, "active": state.active_player.0, "step": format!("{:?}", state.step),
        "life": [state.players[0].life, state.players[1].life]});
    if let (Value::Object(a), Value::Object(b)) = (&mut row, rest) {
        a.extend(b);
    }
    row
}

#[allow(clippy::too_many_lines)]
fn census_job(
    job: &LabelJob,
    scorer: &mut Scorer,
    opt: &Options,
    cases: &mut impl Write,
) -> Result<Value, String> {
    let focal = PlayerId(job.focal);
    let fseat = seat(focal);
    scorer
        .play
        .reset_for_game_v1(paired_policy_seeds_v1(job.environment_seed))
        .map_err(|e| e.to_string())?;
    let mut s = crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )?;
    let start_life = s.game_state().players[focal.index()].life;
    let mut out = Vec::<Value>::new();
    let mut n = BTreeMap::<&'static str, u64>::new();

    let search = opt.win && family(&job.id).is_some_and(|k| k < opt.win_families);
    let mut proven_turns = BTreeSet::<(u32, u8)>::new();
    let mut opportunity: Vec<((u32, u8), Value)> = Vec::new();

    let mut cast: Option<Cast> = None;
    let mut watches: Vec<Watch> = Vec::new();
    let mut combat: Option<Combat> = None;

    for (i, r) in job.decisions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        if d.step != r.step
            || d.physical_decision_id != r.physical_decision_id
            || acting(&d).0 != r.actor
            || d.legal_action_count != r.legal_action_count
        {
            return Err(format!("replay differs from the record at decision {i}"));
        }
        let state = s.game_state();
        let turn = (state.turn, state.active_player);
        let is_focal = acting(&d) == focal;

        // (c), (d): a combat ends before this decision.
        if combat.as_ref().is_some_and(|c| past_combat(&s, c.turn)) {
            let c = combat.take().unwrap();
            finish_combat(job, c, &s, focal, start_life, None, &mut n, &mut out);
        }

        // (a): own-turn focal decisions with two or more actions.
        if search
            && is_focal
            && turn.1 == focal
            && d.legal_action_count >= 2
            && !proven_turns.contains(&(turn.0, turn.1 .0))
        {
            let started = std::time::Instant::now();
            let oracle = oracle_win_this_turn_v1(&s, opt.bounds)
                .ok_or("focal decision has no pending decision")?;
            bump(&mut n, "a_searched");
            match oracle.verdict {
                WinVerdictV1::Proven => {
                    bump(&mut n, "a_proven");
                    proven_turns.insert((turn.0, turn.1 .0));
                    let m = menu(&s);
                    let line_row = oracle
                        .line
                        .first()
                        .and_then(|l| m.iter().position(|x| format!("{x:?}") == *l));
                    let p = scorer.probs(&s)?;
                    let row = case(
                        job,
                        "a_opportunity",
                        i,
                        &s,
                        json!({
                        "selected": r.selected, "p_chosen": p.get(r.selected as usize),
                        "line_row": line_row, "p_line_first": line_row.and_then(|k| p.get(k)),
                        "chose_line_first": line_row == Some(r.selected as usize),
                        "line": oracle.line, "nodes": oracle.nodes,
                        "menu": scorer.menu_json(&s, r.selected)?,
                        "seconds": started.elapsed().as_secs_f64()}),
                    );
                    opportunity.push(((turn.0, turn.1 .0), row));
                }
                WinVerdictV1::Refuted => bump(&mut n, "a_refuted"),
                WinVerdictV1::Unknown => bump(&mut n, "a_unknown"),
            }
        }

        // (c), (d): focal blocker decisions on the opponent's turn.
        if is_focal && turn.1 != focal {
            if let Some(scan) = blocker_scan(&s, &d, focal) {
                if combat.as_ref().is_none_or(|c| c.turn != turn) {
                    combat = Some(Combat {
                        turn,
                        start: s.clone(),
                        start_index: i,
                        scans: Vec::new(),
                        recorded: Vec::new(),
                        forced_include: false,
                        declared: None,
                    });
                }
                let c = combat.as_mut().unwrap();
                c.recorded.push(r.selected);
                if scan.exclude_row.is_none() {
                    c.forced_include = true;
                }
                let p = scorer.probs(&s)?;
                c.scans.push(json!({"index": i, "action": menu(&s).get(r.selected as usize).map(|x| format!("{x:?}")),
                    "include_chosen": scan.include_row == Some(r.selected),
                    "p_include": scan.include_row.and_then(|k| p.get(k as usize)),
                    "p_exclude": scan.exclude_row.and_then(|k| p.get(k as usize))}));
            }
        }

        // (b): casts, target steps.
        let m = if is_focal { menu(&s) } else { Vec::new() };
        let step_source = if is_focal {
            target_steps_source(&m)
        } else {
            None
        };
        // A cast with target steps ends at the first decision that is not
        // one of its target steps: its stack item carries the chosen targets.
        let continues =
            |c: &Cast| step_source.is_some_and(|(src, db)| c.source == src || c.card_db_id == db);
        if cast
            .as_ref()
            .is_some_and(|c| !c.steps.is_empty() && !continues(c))
        {
            let c = cast.take().unwrap();
            let st = s.game_state();
            let item = st
                .stack
                .iter()
                .enumerate()
                .skip(c.stack_len)
                .find(|(_, x)| x.source == ObjectId(c.source) && x.controller == focal)
                .map(|(position, item)| {
                    let targets = item
                        .targets
                        .iter()
                        .filter_map(|t| match t {
                            Target::Object(id) => st
                                .objects
                                .try_get(*id)
                                .filter(|o| o.zone == Zone::Battlefield && o.controller == focal)
                                .map(|o| (*id, o.zone_change_count, o.damage)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    (position, targets)
                });
            if let Some(w) = close_cast(job, c, &s, focal, item, &mut n, &mut out) {
                watches.push(w);
            }
        }
        if is_focal {
            if let Some((src, db)) = step_source {
                if let Some(c) = cast
                    .as_mut()
                    .filter(|c| c.source == src || c.card_db_id == db)
                {
                    let own = m.iter().all(|x| match x {
                        ActionSemanticV1::ChooseTarget { target, .. } => own_target(target, fseat),
                        _ => true,
                    });
                    c.own_only &= own;
                    c.steps.push(json!({"index": i, "own_only": own,
                        "menu": scorer.menu_json(&s, r.selected)?}));
                } else {
                    bump(&mut n, "b_unlinked_target_steps");
                }
            }
        }
        let chosen = m.get(r.selected as usize).cloned();
        let new_cast = match &chosen {
            Some(ActionSemanticV1::CastSpell { source, .. }) => Some((source, "cast")),
            Some(ActionSemanticV1::ActivateAbility { source, .. }) => Some((source, "activate")),
            _ => None,
        };
        if let Some((source, kind)) = new_cast {
            if let Some(old) = cast.take() {
                close_cast(job, old, &s, focal, None, &mut n, &mut out);
            }
            cast = Some(Cast {
                index: i,
                source: source.arena_id,
                card_db_id: source.card_db_id,
                kind,
                cast_menu: scorer.menu_json(&s, r.selected)?,
                stack_len: s.game_state().stack.len(),
                steps: Vec::new(),
                own_only: true,
                decisions_left: 64,
                turn,
            });
        }

        step(&mut s, &d, r.selected)?;

        // Declared blockers, once.
        if let Some(c) = combat.as_mut() {
            let st = s.game_state();
            if c.declared.is_none()
                && (st.turn, st.active_player) == c.turn
                && st.engine.combat.blockers_declared
            {
                let pairs = st
                    .engine
                    .combat
                    .blocked_by
                    .iter()
                    .flat_map(|(a, bs)| bs.iter().map(move |b| (*a, *b)))
                    .map(|(a, b)| (a, b, presence(&s, a), presence(&s, b)))
                    .collect();
                c.declared = Some(pairs);
            }
        }

        // A cast that never reaches a target step is dropped.
        if let Some(c) = cast.as_mut() {
            c.decisions_left = c.decisions_left.saturating_sub(1);
            let st = s.game_state();
            if c.steps.is_empty()
                && (c.decisions_left == 0 || (st.turn, st.active_player) != c.turn)
            {
                cast = None;
            }
        }

        // Watched stack items that left the stack.
        let mut k = 0;
        while k < watches.len() {
            let st = s.game_state();
            let w = &watches[k];
            let gone = st
                .stack
                .get(w.position)
                .is_none_or(|x| x.source != w.source);
            if gone {
                let w = watches.swap_remove(k);
                resolve_watch(w, &s, focal, &mut n, &mut out);
            } else {
                k += 1;
            }
        }
    }

    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("recorded actions end before the terminal".into());
    };
    let winner = t.winner.map(|w| match w {
        PlayerSeatV1::P0 => 0u8,
        PlayerSeatV1::P1 => 1u8,
    });
    if winner != job.winner {
        return Err("replayed winner differs from the record".into());
    }
    if let Some(c) = cast.take() {
        close_cast(job, c, &s, focal, None, &mut n, &mut out);
    }
    for w in std::mem::take(&mut watches) {
        resolve_watch(w, &s, focal, &mut n, &mut out);
    }
    let st = s.game_state();
    let end_turn = (st.turn, st.active_player);
    let combat_loss = winner == Some(1 - job.focal)
        && end_turn.1 != focal
        && st.step == Step::CombatDamage
        && st.players[focal.index()].life <= 0;
    let mut declared_here = false;
    if let Some(c) = combat.take() {
        declared_here = c.turn == end_turn;
        let lethal = combat_loss && declared_here;
        finish_combat(
            job,
            c,
            &s,
            focal,
            start_life,
            Some(lethal),
            &mut n,
            &mut out,
        );
    }
    if combat_loss && !declared_here {
        bump(&mut n, "d_combat_loss_no_block_decision");
    }
    if combat_loss {
        bump(&mut n, "d_combat_losses");
    }
    for (key, mut row) in opportunity {
        let converted = winner == Some(job.focal) && (end_turn.0, end_turn.1 .0) == key;
        bump(&mut n, "a_opportunities");
        if converted {
            bump(&mut n, "a_converted");
        }
        row["converted"] = json!(converted);
        out.push(row);
    }
    for row in &out {
        writeln!(cases, "{row}").map_err(|e| e.to_string())?;
    }
    Ok(
        json!({"kind": "game", "id": job.id, "focal": job.focal, "deck_ids": job.config.deck_ids,
        "winner": winner, "win": winner == Some(job.focal), "searched_a": search,
        "start_life": start_life, "final_turn": st.turn, "final_step": format!("{:?}", st.step),
        "final_life": [st.players[0].life, st.players[1].life], "decisions": job.decisions.len(),
        "counts": n}),
    )
}

/// Records a finished cast; returns a watch for an own-only cast whose stack
/// item was found.
fn close_cast(
    job: &LabelJob,
    c: Cast,
    s: &Session,
    focal: PlayerId,
    item: Option<(usize, Vec<(ObjectId, u32, u32)>)>,
    n: &mut BTreeMap<&'static str, u64>,
    out: &mut Vec<Value>,
) -> Option<Watch> {
    if c.steps.is_empty() {
        return None;
    }
    *n.entry("b_targeted_casts").or_default() += 1;
    if !c.own_only {
        return None;
    }
    *n.entry("b_own_only").or_default() += 1;
    let row = case(
        job,
        "b_own_only",
        c.index,
        s,
        json!({"source": c.source, "card_db_id": c.card_db_id,
        "cast_kind": c.kind, "cast_menu": c.cast_menu, "target_steps": c.steps,
        "stack_item_found": item.is_some()}),
    );
    match item {
        Some((position, targets)) => {
            let st = s.game_state();
            let opp = focal.opponent();
            Some(Watch {
                case: row,
                position,
                source: ObjectId(c.source),
                targets,
                library: st.players[focal.index()].library.len(),
                opp_life: st.players[opp.index()].life,
                opp_permanents: permanents(s, opp),
            })
        }
        None => {
            let mut row = row;
            row["harmful"] = json!(false);
            out.push(row);
            None
        }
    }
}

fn resolve_watch(
    w: Watch,
    s: &Session,
    focal: PlayerId,
    n: &mut BTreeMap<&'static str, u64>,
    out: &mut Vec<Value>,
) {
    let st = s.game_state();
    let mut removed = 0u32;
    let mut damaged = 0u32;
    let mut flicker = false;
    for &(id, zcc, damage) in &w.targets {
        let obj = st.objects.try_get(id);
        match obj.filter(|o| o.zone == Zone::Battlefield) {
            Some(o) if o.zone_change_count == zcc => damaged += u32::from(o.damage > damage),
            Some(o) => {
                removed += 1;
                flicker |= o.controller == focal;
            }
            None => removed += 1,
        }
    }
    let opp = focal.opponent();
    let harm = removed + damaged > 0;
    let exclusion = if !harm {
        None
    } else if flicker {
        Some("flicker")
    } else if st.players[focal.index()].library.len() < w.library {
        Some("library_shrank")
    } else if st.players[opp.index()].life < w.opp_life || permanents(s, opp) < w.opp_permanents {
        Some("opponent_lost")
    } else {
        None
    };
    let harmful = harm && exclusion.is_none();
    if harm {
        *n.entry("b_harm_before_exclusions").or_default() += 1;
    }
    if harmful {
        *n.entry("b_harmful").or_default() += 1;
    }
    if let Some(e) = exclusion {
        *n.entry(match e {
            "flicker" => "b_excluded_flicker",
            "library_shrank" => "b_excluded_library",
            _ => "b_excluded_opponent",
        })
        .or_default() += 1;
    }
    let mut row = w.case;
    row["harmful"] = json!(harmful);
    row["targets_removed"] = json!(removed);
    row["targets_damaged"] = json!(damaged);
    row["exclusion"] = json!(exclusion);
    out.push(row);
}

#[allow(clippy::too_many_arguments)]
fn finish_combat(
    job: &LabelJob,
    c: Combat,
    s: &Session,
    focal: PlayerId,
    start_life: i32,
    lethal: Option<bool>,
    n: &mut BTreeMap<&'static str, u64>,
    out: &mut Vec<Value>,
) {
    let mut add = |k: &'static str, v: u64| *n.entry(k).or_default() += v;
    let declared = c.declared.clone().unwrap_or_default();
    add("c_combats_with_blocker_decision", 1);
    if !declared.is_empty() {
        add("c_combats_with_block", 1);
        add("c_blocks", declared.len() as u64);
    }
    let chumps: Vec<(ObjectId, ObjectId)> = declared
        .iter()
        .filter(|&&(a, b, za, zb)| {
            zb.is_some() && presence(s, b) != zb && za.is_some() && presence(s, a) == za
        })
        .map(|&(a, b, _, _)| (a, b))
        .collect();
    if !chumps.is_empty() {
        add("c_chumps", chumps.len() as u64);
        let (after, end, followed) =
            passive(c.start.clone(), focal, c.turn, &mut |scan| scan.exclude_row);
        let life = if end == SimEnd::Terminal && focal_lost(&after, focal) == Some(true) {
            None
        } else if end == SimEnd::CombatOver || end == SimEnd::Terminal {
            Some(after.game_state().players[focal.index()].life)
        } else {
            None
        };
        let valid =
            followed && !c.forced_include && matches!(end, SimEnd::CombatOver | SimEnd::Terminal);
        let half = valid && life.is_some_and(|l| 2 * l >= start_life);
        let quarter = valid && life.is_some_and(|l| 4 * l >= start_life);
        if !valid {
            add("c_chumps_counterfactual_unavailable", chumps.len() as u64);
        }
        if half {
            add("c_pointless_half", chumps.len() as u64);
        }
        if quarter {
            add("c_pointless_quarter", chumps.len() as u64);
        }
        out.push(case(job, "c_chump", c.start_index, &c.start, json!({
            "chumps": chumps.iter().map(|(a, b)| json!({"attacker": a.0, "blocker": b.0})).collect::<Vec<_>>(),
            "blocks": declared.len(), "scans": c.scans, "forced_include": c.forced_include,
            "no_block_life": life, "no_block_end": end.as_str(), "start_life": start_life,
            "life_after_combat": s.game_state().players[focal.index()].life,
            "pointless_half": half, "pointless_quarter": quarter})));
    }
    if lethal == Some(true) {
        add("d_combat_losses_with_block_decision", 1);
        let mut rows = c.recorded.clone().into_iter();
        let (after, end, followed) = passive(c.start.clone(), focal, c.turn, &mut |_| rows.next());
        let recorded_survives = survives(&after, end, focal);
        let mut w = Walk {
            leaves: 0,
            bounded: false,
        };
        let found = walk(c.start.clone(), focal, c.turn, &mut Vec::new(), &mut w);
        let class = if recorded_survives {
            "post_declaration_loss"
        } else if found.is_some() {
            "blunder"
        } else if w.bounded {
            "bounded_out"
        } else {
            "no_survivor"
        };
        add(
            match class {
                "post_declaration_loss" => "d_post_declaration_loss",
                "blunder" => "d_blunder",
                "bounded_out" => "d_bounded_out",
                _ => "d_no_survivor",
            },
            1,
        );
        out.push(case(
            job,
            "d_combat_loss",
            c.start_index,
            &c.start,
            json!({
            "class": class, "scans": c.scans, "recorded_rows": c.recorded,
            "recorded_passive_end": end.as_str(), "recorded_followed": followed,
            "surviving_path_include": found, "leaves": w.leaves, "bounded": w.bounded}),
        ));
    }
}

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    let out = cfg.out.clone();
    std::thread::Builder::new()
        .name("s4a-blunder-census".into())
        .stack_size(STACK_BYTES)
        .spawn(move || run_on_worker(&out))
        .map_err(|e| format!("spawn census worker: {e}"))?
        .join()
        .map_err(|_| "census worker panicked".to_string())?
}

fn run_on_worker(out_path: &str) -> Result<(), String> {
    let path = std::env::var("LABEL_JOBS").map_err(|_| "LABEL_JOBS is required")?;
    let source_path = std::env::var("CENSUS_SOURCE").map_err(|_| "CENSUS_SOURCE is required")?;
    let cases_path = std::env::var("CENSUS_CASES").map_err(|_| "CENSUS_CASES is required")?;
    let source: ModelSource = serde_json::from_slice(
        &std::fs::read(&source_path).map_err(|e| format!("{source_path}: {e}"))?,
    )
    .map_err(|e| format!("{source_path}: {e}"))?;
    let opt = Options {
        win: env_u64("CENSUS_WIN", 0)? == 1,
        win_families: env_u64("CENSUS_WIN_FAMILIES", 2)?,
        bounds: WinSearchBoundsV1 {
            max_nodes: env_u64("WIN_NODES", 5000)?,
            max_depth: env_u64("WIN_DEPTH", 400)? as u32,
        },
    };
    let (play, receipt) = load(&source)?;
    let mut scorer = Scorer { play, failures: 0 };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut out =
        std::io::BufWriter::new(std::fs::File::create(out_path).map_err(|e| e.to_string())?);
    let mut cases =
        std::io::BufWriter::new(std::fs::File::create(&cases_path).map_err(|e| e.to_string())?);
    writeln!(out, "{}", json!({"kind": "header", "model": receipt, "win": opt.win,
        "win_families": opt.win_families, "max_nodes": opt.bounds.max_nodes, "max_depth": opt.bounds.max_depth}))
    .map_err(|e| e.to_string())?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let job: LabelJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        let started = std::time::Instant::now();
        let mut buffer = Vec::new();
        let mut row = census_job(&job, &mut scorer, &opt, &mut buffer)
            .unwrap_or_else(|e| json!({"kind": "error", "id": job.id, "error": e}));
        if row["kind"] == "game" {
            cases.write_all(&buffer).map_err(|e| e.to_string())?;
        }
        row["seconds"] = json!(started.elapsed().as_secs_f64());
        row["scoring_failures_so_far"] = json!(scorer.failures);
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        cases.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
