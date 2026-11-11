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
//! - (b) casts and activations whose every offered target was a permanent
//!   the focal seat controls or the focal player, and those whose resolution
//!   removed or damaged an own target;
//! - (c) chump blocks (blocker died, attacker lived) with the no-block
//!   counterfactual's life;
//! - (d) losses to combat damage after a block declaration, with an
//!   exhaustive walk over the legal declarations for a surviving one.
//!
//! At every case position the candidate is scored on the live session and
//! the case row carries the softmax of its logits. Rows hold no wall-clock
//! fields: the same jobs give the same bytes at any shard count.
//!
//! Environment: `LABEL_JOBS` (jsonl), `CENSUS_SOURCE` (JSON model source of
//! the candidate, as the plan's requests carry it), `CENSUS_CASES` (case rows
//! path; the per-game rows go to `out`), `CENSUS_WIN` (`1` runs (a), default
//! `0`), `CENSUS_WIN_FAMILIES` (default 2), `WIN_NODES` (default 5000),
//! `WIN_DEPTH` (default 400), `CENSUS_TIMING` (optional path for per-job
//! seconds), `CENSUS_SCORE_CHECK` (`1`: a second copy of the candidate,
//! reset at game start, scores every focal decision in order, and each case
//! position's logits must equal it bit for bit; mismatches are counted).

use super::play::{acting, decision};
use super::replay_labels::{LabelJob, RecordedDecision};
use crate::ids::{ObjectId, PlayerId};
use crate::learned_bo3_v1::public_evaluation::{load, ModelSource, Play};
use crate::paired_bo1_harness_v1::{paired_policy_seeds_v1, PairedBo1PolicyV1};
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::state::{Step, Zone};
use crate::win_search_v1::{oracle_win_this_turn_v1, WinSearchBoundsV1, WinVerdictV1};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

type Session = FastActorSessionV1;
type Counts = BTreeMap<&'static str, u64>;

const STACK_BYTES: usize = 256 << 20;
/// Bound on steps along one passive combat continuation (per leaf in (d)).
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

fn bump(n: &mut Counts, k: &'static str) {
    add(n, k, 1);
}

fn add(n: &mut Counts, k: &'static str, v: u64) {
    *n.entry(k).or_default() += v;
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

/// The live decision equals record `r` (fail closed on any difference).
fn matches_record(d: &FastActorDecisionV1, r: &RecordedDecision, i: usize) -> Result<(), String> {
    if d.step != r.step
        || d.physical_decision_id != r.physical_decision_id
        || acting(d).0 != r.actor
        || d.legal_action_count != r.legal_action_count
    {
        return Err(format!("replay differs from the record at decision {i}"));
    }
    Ok(())
}

/// The replay's end against the record: `ended` is None when the session is
/// not terminal after the last recorded action, else its winner.
fn end_check(ended: Option<Option<u8>>, recorded: Option<u8>) -> Result<Option<u8>, String> {
    match ended {
        None => Err("recorded actions end before the terminal".into()),
        Some(winner) if winner != recorded => Err("replayed winner differs from the record".into()),
        Some(winner) => Ok(winner),
    }
}

/// The focal seat lost.
fn focal_lost(s: &Session, focal: PlayerId) -> Option<bool> {
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return None;
    };
    Some(t.winner.is_some_and(|w| w != seat(focal)))
}

fn softmax(logits: &[f32]) -> Vec<f64> {
    let max = logits
        .iter()
        .fold(f64::NEG_INFINITY, |m, &x| m.max(f64::from(x)));
    let e: Vec<f64> = logits.iter().map(|&x| (f64::from(x) - max).exp()).collect();
    let z: f64 = e.iter().sum();
    e.into_iter().map(|x| x / z).collect()
}

/// The candidate's softmax at case positions. A position it cannot score
/// yields no probabilities (counted), never a lost game. With a shadow, the
/// shadow has scored every focal decision in order and the case position's
/// logits must equal its bits.
struct Scorer {
    play: Play,
    shadow: Option<Play>,
    shadow_bits: Option<Vec<u32>>,
    failures: u64,
    checks: u64,
    mismatches: u64,
}

fn logits(play: &mut Play, s: &Session) -> Result<Option<Vec<f32>>, String> {
    let p = play
        .legacy_policy_mut()
        .ok_or("the candidate must be a Legacy source")?;
    Ok(p.score_fast_session_v1(s).ok().map(|x| x.logits))
}

impl Scorer {
    fn reset(&mut self, seed: u64) -> Result<(), String> {
        let seeds = paired_policy_seeds_v1(seed);
        self.play
            .reset_for_game_v1(seeds)
            .map_err(|e| e.to_string())?;
        if let Some(shadow) = self.shadow.as_mut() {
            shadow.reset_for_game_v1(seeds).map_err(|e| e.to_string())?;
        }
        (self.failures, self.checks, self.mismatches) = (0, 0, 0);
        Ok(())
    }

    /// Called at every focal decision before any case scoring there.
    fn shadow_step(&mut self, s: &Session) -> Result<(), String> {
        if let Some(shadow) = self.shadow.as_mut() {
            self.shadow_bits = logits(shadow, s)?.map(|v| v.iter().map(|x| x.to_bits()).collect());
        }
        Ok(())
    }

    fn probs(&mut self, s: &Session) -> Result<Vec<f64>, String> {
        let Some(l) = logits(&mut self.play, s)? else {
            self.failures += 1;
            return Ok(Vec::new());
        };
        if self.shadow.is_some() {
            self.checks += 1;
            let bits: Vec<u32> = l.iter().map(|x| x.to_bits()).collect();
            if self.shadow_bits.as_ref() != Some(&bits) {
                self.mismatches += 1;
            }
        }
        Ok(softmax(&l))
    }

    /// The chosen row's probability and the top rows with their semantics.
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
            json!({"legal": m.len(), "chosen": chosen, "p_chosen": p.get(chosen as usize),
            "chosen_action": m.get(chosen as usize).map(|x| format!("{x:?}")),
            "p_pass": pass.and_then(|r| p.get(r as usize)), "top": top}),
        )
    }
}

// ---------------------------------------------------------------- combat helpers

/// One blocker-inclusion decision: its include and exclude rows.
#[derive(Debug, PartialEq, Eq)]
struct BlockerScan {
    include_row: Option<u32>,
    exclude_row: Option<u32>,
}

/// A menu whose every row is a `ChooseBlockerInclusion` of one pair.
fn blocker_scan_menu(m: &[ActionSemanticV1]) -> Option<BlockerScan> {
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

fn blocker_scan(s: &Session, d: &FastActorDecisionV1, focal: PlayerId) -> Option<BlockerScan> {
    if acting(d) != focal {
        return None;
    }
    blocker_scan_menu(&s.diagnostic_current_action_semantics()?)
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

    fn complete(self) -> bool {
        matches!(self, SimEnd::CombatOver | SimEnd::Terminal)
    }
}

/// Passive continuation: focal blocker decisions take `choose`'s row (None:
/// the request cannot be followed), every other decision passes when it can,
/// otherwise its first row; until combat is over or the game ends.
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

#[derive(Default)]
struct Walk {
    leaves: u64,
    bounded: bool,
    errors: u64,
}

/// Depth-first over every offered row of every focal blocker decision, with
/// `budget` steps left on this path; the include/exclude path of the first
/// surviving leaf.
fn walk(
    mut s: Session,
    focal: PlayerId,
    turn: (u32, PlayerId),
    mut budget: usize,
    path: &mut Vec<bool>,
    w: &mut Walk,
) -> Option<Vec<bool>> {
    loop {
        let Some(d) = decision(&s) else {
            w.leaves += 1;
            return survives(&s, SimEnd::Terminal, focal).then(|| path.clone());
        };
        if budget == 0 {
            w.leaves += 1;
            w.bounded = true;
            return None;
        }
        budget -= 1;
        if let Some(scan) = blocker_scan(&s, &d, focal) {
            for (include, row) in [(true, scan.include_row), (false, scan.exclude_row)] {
                let Some(row) = row else { continue };
                if w.leaves >= MAX_LEAVES {
                    w.bounded = true;
                    return None;
                }
                let mut next = s.clone();
                if step(&mut next, &d, row).is_err() {
                    w.leaves += 1;
                    w.errors += 1;
                    continue;
                }
                path.push(include);
                let found = walk(next, focal, turn, budget, path, w);
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
            w.errors += 1;
            return None;
        }
    }
}

/// (d) class of a combat loss, first match: the recorded declaration could
/// not be replayed passively; it survives passively (the loss came later);
/// some declaration survives; the walk was bounded; a step failed; none
/// survives.
fn lethal_class(recorded_ok: bool, recorded_survives: bool, found: bool, w: &Walk) -> &'static str {
    if !recorded_ok {
        "recorded_continuation_failed"
    } else if recorded_survives {
        "post_declaration_loss"
    } else if found {
        "blunder"
    } else if w.bounded {
        "bounded_out"
    } else if w.errors > 0 {
        "walk_error"
    } else {
        "no_survivor"
    }
}

/// (attacker, blocker, attacker zcc, blocker zcc) as declared.
type DeclaredPair = (ObjectId, ObjectId, Option<u32>, Option<u32>);

/// A combat the focal seat defends, from its first blocker decision.
struct Combat {
    turn: (u32, PlayerId),
    start: Session,
    start_index: usize,
    scans: Vec<Value>,
    /// The recorded rows of the focal blocker decisions, in order.
    recorded: Vec<u32>,
    forced_include: bool,
    /// (attacker, blocker, attacker zcc, blocker zcc) once declared.
    declared: Option<Vec<DeclaredPair>>,
}

// ---------------------------------------------------------------- casts

/// The source of a target step: a menu whose every row is a `ChooseTarget`
/// (at least one) or a `FinishTargetSelection` of one source object.
fn target_steps_source(m: &[ActionSemanticV1]) -> Option<u32> {
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
            None => src = Some(s.arena_id),
            Some(a) if a != s.arena_id => return None,
            _ => {}
        }
    }
    src.filter(|_| any_target)
}

/// A permanent the focal seat controls, or the focal player.
fn own_target(t: &TargetRefV1, focal: PlayerSeatV1) -> bool {
    match t {
        TargetRefV1::Player { player } => *player == focal,
        TargetRefV1::Object { object } => {
            object.controller == focal && object.zone == Zone::Battlefield
        }
        TargetRefV1::StackItem { .. } => false,
    }
}

/// The focal choice that opens a cast: (source arena id, card, kind).
fn cast_choice(chosen: Option<&ActionSemanticV1>) -> Option<(u32, u16, &'static str)> {
    match chosen {
        Some(ActionSemanticV1::CastSpell { source, .. }) => {
            Some((source.arena_id, source.card_db_id, "cast"))
        }
        Some(ActionSemanticV1::ActivateAbility { source, .. }) => {
            Some((source.arena_id, source.card_db_id, "activate"))
        }
        _ => None,
    }
}

/// What a decision does to the open cast window.
#[derive(Debug, PartialEq, Eq)]
enum Window {
    /// A target step of the open cast.
    Link,
    /// A target step with no open cast of its source (a trigger, or a cast
    /// whose window already ended): never counted.
    Unlinked,
    /// The open cast had target steps and this decision is not one: judge it.
    Close,
    /// The open cast has no target step yet and this is an opponent decision
    /// or a focal priority decision (a Pass row): it is not a targeted cast.
    Drop,
    Keep,
    None,
}

fn window(open: Option<(u32, bool)>, is_focal: bool, m: &[ActionSemanticV1]) -> Window {
    let source = if is_focal {
        target_steps_source(m)
    } else {
        None
    };
    match (open, source) {
        (Some((cast, _)), Some(src)) if cast == src => Window::Link,
        (Some((_, true)), _) => Window::Close,
        (Some((_, false)), _) if !is_focal || pass_row(m).is_some() => Window::Drop,
        (_, Some(_)) => Window::Unlinked,
        (Some(_), None) => Window::Keep,
        (None, None) => Window::None,
    }
}

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
    /// Own battlefield objects chosen at the target steps: (object, zcc, damage).
    chosen: Vec<(ObjectId, u32, u32)>,
    /// Window baselines at the cast.
    library: usize,
    opp_life: i32,
    opp_permanents: Vec<(ObjectId, u32)>,
}

/// An own-only cast's stack item, until it leaves the stack.
struct Watch {
    case: Value,
    position: usize,
    source: ObjectId,
    targets: Vec<(ObjectId, u32, u32)>,
    library: usize,
    opp_life: i32,
    opp_permanents: Vec<(ObjectId, u32)>,
}

fn permanents(s: &Session, p: PlayerId) -> Vec<(ObjectId, u32)> {
    s.game_state().players[p.index()]
        .battlefield
        .iter()
        .filter_map(|&id| presence(s, id).map(|z| (id, z)))
        .collect()
}

/// How a targeted own permanent stands now: on the battlefield with the
/// same incarnation (its damage), back under the focal seat's control as a
/// new incarnation (flicker), or gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Now {
    Same(u32),
    Flickered,
    Gone,
}

/// (harm, exclusion) for the targets of an own-only cast (DEFINITIONS (b)).
fn judge(
    targets: &[(ObjectId, u32, u32)],
    now: impl Fn(ObjectId, u32) -> Now,
    library_shrank: bool,
    opponent_lost: bool,
) -> (bool, Option<&'static str>, u32, u32) {
    let (mut removed, mut damaged, mut flicker) = (0u32, 0u32, false);
    for &(id, zcc, damage) in targets {
        match now(id, zcc) {
            Now::Same(d) => damaged += u32::from(d > damage),
            Now::Flickered => {
                removed += 1;
                flicker = true;
            }
            Now::Gone => removed += 1,
        }
    }
    let harm = removed + damaged > 0;
    let exclusion = if !harm {
        None
    } else if flicker {
        Some("flicker")
    } else if library_shrank {
        Some("library_shrank")
    } else if opponent_lost {
        Some("opponent_lost")
    } else {
        None
    };
    (harm, exclusion, removed, damaged)
}

fn resolve_watch(w: Watch, s: &Session, focal: PlayerId, n: &mut Counts, out: &mut Vec<Value>) {
    let st = s.game_state();
    let opp = focal.opponent();
    let now = |id: ObjectId, zcc: u32| match st
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield)
    {
        Some(o) if o.zone_change_count == zcc => Now::Same(o.damage),
        Some(o) if o.controller == focal => Now::Flickered,
        _ => Now::Gone,
    };
    let library_shrank = st.players[focal.index()].library.len() < w.library;
    let opponent_lost = st.players[opp.index()].life < w.opp_life
        || w.opp_permanents
            .iter()
            .any(|&(id, zcc)| presence(s, id) != Some(zcc));
    let (harm, exclusion, removed, damaged) = judge(&w.targets, now, library_shrank, opponent_lost);
    let harmful = harm && exclusion.is_none();
    if harm {
        bump(n, "b_harm_before_exclusions");
    }
    if harmful {
        bump(n, "b_harmful");
    }
    match exclusion {
        Some("flicker") => bump(n, "b_excluded_flicker"),
        Some("library_shrank") => bump(n, "b_excluded_library"),
        Some(_) => bump(n, "b_excluded_opponent"),
        None => {}
    }
    let mut row = w.case;
    row["harmful"] = json!(harmful);
    row["targets_removed"] = json!(removed);
    row["targets_damaged"] = json!(damaged);
    row["exclusion"] = json!(exclusion);
    out.push(row);
}

/// Ends a cast window. A targeted own-only cast is watched while its item is
/// on the stack, else judged now (it left the stack before this decision).
/// Targets are always those chosen at its steps: the stack item's own list
/// can still be empty while costs are being paid.
fn close_cast(
    job: &LabelJob,
    c: Cast,
    s: &Session,
    focal: PlayerId,
    n: &mut Counts,
    out: &mut Vec<Value>,
) -> Option<Watch> {
    if c.steps.is_empty() {
        return None;
    }
    bump(n, "b_targeted_casts");
    if !c.own_only {
        return None;
    }
    bump(n, "b_own_only");
    let position = s
        .game_state()
        .stack
        .iter()
        .enumerate()
        .skip(c.stack_len)
        .find(|(_, x)| x.source == ObjectId(c.source) && x.controller == focal)
        .map(|(position, _)| position);
    let row = case(
        job,
        "b_own_only",
        c.index,
        s,
        json!({"source": c.source, "card_db_id": c.card_db_id,
        "cast_kind": c.kind, "cast_menu": c.cast_menu, "target_steps": c.steps,
        "on_stack_after_steps": position.is_some()}),
    );
    let w = Watch {
        case: row,
        position: position.unwrap_or(usize::MAX),
        source: ObjectId(c.source),
        targets: c.chosen,
        library: c.library,
        opp_life: c.opp_life,
        opp_permanents: c.opp_permanents,
    };
    if position.is_some() {
        return Some(w);
    }
    resolve_watch(w, s, focal, n, out);
    None
}

// ---------------------------------------------------------------- the job

struct Options {
    win: bool,
    win_families: u64,
    bounds: WinSearchBoundsV1,
}

/// The seed family `k` of a job id `<panel>/o<own>-t<other>-k<k>-s<seat>`.
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
    scorer.reset(job.environment_seed)?;
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
    let mut n = Counts::new();

    let search = opt.win && family(&job.id).is_some_and(|k| k < opt.win_families);
    let mut proven_turns = BTreeSet::<(u32, u8)>::new();
    let mut opportunity: Vec<((u32, u8), Value)> = Vec::new();

    let mut cast: Option<Cast> = None;
    let mut watches: Vec<Watch> = Vec::new();
    let mut combat: Option<Combat> = None;

    for (i, r) in job.decisions.iter().enumerate() {
        let d = decision(&s).ok_or_else(|| format!("replay ended before decision {i}"))?;
        matches_record(&d, r, i)?;
        let state = s.game_state();
        let turn = (state.turn, state.active_player);
        let is_focal = acting(&d) == focal;
        if is_focal {
            scorer.shadow_step(&s)?;
        }

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
                        "menu": scorer.menu_json(&s, r.selected)?}),
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
                let p = scorer.probs(&s)?;
                let c = combat.as_mut().unwrap();
                c.recorded.push(r.selected);
                c.forced_include |= scan.exclude_row.is_none();
                c.scans.push(json!({"index": i, "action": menu(&s).get(r.selected as usize).map(|x| format!("{x:?}")),
                    "include_chosen": scan.include_row == Some(r.selected),
                    "p_include": scan.include_row.and_then(|k| p.get(k as usize)),
                    "p_exclude": scan.exclude_row.and_then(|k| p.get(k as usize))}));
            }
        }

        // (b): the cast window.
        let m = if is_focal { menu(&s) } else { Vec::new() };
        // An activation links target steps only while the engine's pending
        // activation is its own (an activation has no stack entry until its
        // targets and costs are done), so a self-trigger right after it
        // cannot link.
        let entry = cast.as_ref().is_some_and(|c| {
            c.kind != "activate"
                || s.game_state()
                    .engine
                    .pending_activation
                    .as_ref()
                    .is_some_and(|p| p.source == ObjectId(c.source) && p.controller == focal)
        });
        let mut cast_window = window(
            cast.as_ref().map(|c| (c.source, !c.steps.is_empty())),
            is_focal,
            &m,
        );
        if cast_window == Window::Link && !entry {
            bump(&mut n, "b_unlinked_without_pending_activation");
            cast_window = Window::Unlinked;
        }
        match cast_window {
            Window::Link => {
                let own = m.iter().all(|x| match x {
                    ActionSemanticV1::ChooseTarget { target, .. } => own_target(target, fseat),
                    _ => true,
                });
                let step_menu = scorer.menu_json(&s, r.selected)?;
                let c = cast.as_mut().unwrap();
                c.own_only &= own;
                if let Some(ActionSemanticV1::ChooseTarget {
                    target: TargetRefV1::Object { object },
                    ..
                }) = m.get(r.selected as usize)
                {
                    let id = ObjectId(object.arena_id);
                    if let Some(o) = s
                        .game_state()
                        .objects
                        .try_get(id)
                        .filter(|o| o.zone == Zone::Battlefield && o.controller == focal)
                    {
                        c.chosen.push((id, o.zone_change_count, o.damage));
                    }
                }
                c.steps
                    .push(json!({"index": i, "own_only": own, "menu": step_menu}));
            }
            Window::Close => {
                let c = cast.take().unwrap();
                if let Some(w) = close_cast(job, c, &s, focal, &mut n, &mut out) {
                    watches.push(w);
                }
            }
            Window::Drop => cast = None,
            Window::Unlinked => bump(&mut n, "b_unlinked_target_steps"),
            Window::Keep | Window::None => {}
        }
        if let Some((source, card_db_id, kind)) = cast_choice(m.get(r.selected as usize)) {
            if let Some(old) = cast.take() {
                if let Some(w) = close_cast(job, old, &s, focal, &mut n, &mut out) {
                    watches.push(w);
                }
            }
            let st = s.game_state();
            cast = Some(Cast {
                index: i,
                source,
                card_db_id,
                kind,
                cast_menu: scorer.menu_json(&s, r.selected)?,
                stack_len: st.stack.len(),
                steps: Vec::new(),
                own_only: true,
                chosen: Vec::new(),
                library: st.players[focal.index()].library.len(),
                opp_life: st.players[focal.opponent().index()].life,
                opp_permanents: permanents(&s, focal.opponent()),
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

        // Watched stack items that left the stack.
        let mut k = 0;
        while k < watches.len() {
            let w = &watches[k];
            let gone = s
                .game_state()
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

    let ended = match s.current_response() {
        FastActorResponseV1::Terminal(t) => Some(t.winner.map(|w| match w {
            PlayerSeatV1::P0 => 0u8,
            PlayerSeatV1::P1 => 1u8,
        })),
        FastActorResponseV1::Decision(_) => None,
    };
    let winner = end_check(ended, job.winner)?;
    if let Some(c) = cast.take() {
        if let Some(w) = close_cast(job, c, &s, focal, &mut n, &mut out) {
            watches.push(w);
        }
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
    if combat_loss {
        bump(&mut n, "d_combat_losses");
        if !declared_here {
            bump(&mut n, "d_combat_loss_no_block_decision");
        }
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
        "scoring_failures": scorer.failures, "score_checks": scorer.checks,
        "score_mismatches": scorer.mismatches, "counts": n}),
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_combat(
    job: &LabelJob,
    c: Combat,
    s: &Session,
    focal: PlayerId,
    start_life: i32,
    lethal: Option<bool>,
    n: &mut Counts,
    out: &mut Vec<Value>,
) {
    let declared = c.declared.clone().unwrap_or_default();
    bump(n, "c_combats_with_blocker_decision");
    if c.forced_include {
        bump(n, "c_forced_include_combats");
    }
    if !declared.is_empty() {
        bump(n, "c_combats_with_block");
        add(n, "c_blocks", declared.len() as u64);
    }
    let chumps: Vec<(ObjectId, ObjectId)> = declared
        .iter()
        .filter(|&&(a, b, za, zb)| {
            zb.is_some() && presence(s, b) != zb && za.is_some() && presence(s, a) == za
        })
        .map(|&(a, b, _, _)| (a, b))
        .collect();
    if !chumps.is_empty() {
        let k = chumps.len() as u64;
        add(n, "c_chumps", k);
        let (after, end, followed) =
            passive(c.start.clone(), focal, c.turn, &mut |scan| scan.exclude_row);
        let life = match end {
            SimEnd::Terminal if focal_lost(&after, focal) == Some(true) => None,
            SimEnd::CombatOver | SimEnd::Terminal => {
                Some(after.game_state().players[focal.index()].life)
            }
            _ => None,
        };
        let valid = !c.forced_include && followed && end.complete();
        let half = valid && life.is_some_and(|l| 2 * l >= start_life);
        let quarter = valid && life.is_some_and(|l| 4 * l >= start_life);
        if c.forced_include {
            add(n, "c_chumps_in_forced_combats", k);
        } else if !(followed && end.complete()) {
            add(n, "c_chumps_counterfactual_failed", k);
        }
        if half {
            add(n, "c_pointless_half", k);
        }
        if quarter {
            add(n, "c_pointless_quarter", k);
        }
        out.push(case(job, "c_chump", c.start_index, &c.start, json!({
            "chumps": chumps.iter().map(|(a, b)| json!({"attacker": a.0, "blocker": b.0})).collect::<Vec<_>>(),
            "blocks": declared.len(), "scans": c.scans, "forced_include": c.forced_include,
            "no_block_life": life, "no_block_end": end.as_str(), "no_block_followed": followed,
            "start_life": start_life, "life_after_combat": s.game_state().players[focal.index()].life,
            "pointless_half": half, "pointless_quarter": quarter})));
    }
    if lethal == Some(true) {
        bump(n, "d_combat_losses_with_block_decision");
        let mut rows = c.recorded.clone().into_iter();
        let (after, end, followed) = passive(c.start.clone(), focal, c.turn, &mut |_| rows.next());
        let recorded_ok = followed && rows.next().is_none() && end.complete();
        let recorded_survives = recorded_ok && survives(&after, end, focal);
        let mut w = Walk::default();
        let found = walk(
            c.start.clone(),
            focal,
            c.turn,
            SIM_STEPS,
            &mut Vec::new(),
            &mut w,
        );
        let class = lethal_class(recorded_ok, recorded_survives, found.is_some(), &w);
        bump(
            n,
            match class {
                "recorded_continuation_failed" => "d_recorded_continuation_failed",
                "post_declaration_loss" => "d_post_declaration_loss",
                "blunder" => "d_blunder",
                "bounded_out" => "d_bounded_out",
                "walk_error" => "d_walk_error",
                _ => "d_no_survivor",
            },
        );
        out.push(case(
            job,
            "d_combat_loss",
            c.start_index,
            &c.start,
            json!({
            "class": class, "scans": c.scans, "recorded_rows": c.recorded,
            "recorded_passive_end": end.as_str(), "recorded_followed": followed,
            "surviving_path_include": found, "leaves": w.leaves, "bounded": w.bounded,
            "walk_errors": w.errors}),
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
    let shadow = if env_u64("CENSUS_SCORE_CHECK", 0)? == 1 {
        Some(load(&source)?.0)
    } else {
        None
    };
    let mut scorer = Scorer {
        play,
        shadow,
        shadow_bits: None,
        failures: 0,
        checks: 0,
        mismatches: 0,
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let create = |p: &str| {
        std::fs::File::create(p)
            .map(std::io::BufWriter::new)
            .map_err(|e| format!("{p}: {e}"))
    };
    let mut out = create(out_path)?;
    let mut cases = create(&cases_path)?;
    let mut timing = match std::env::var("CENSUS_TIMING") {
        Ok(p) => Some(create(&p)?),
        Err(_) => None,
    };
    writeln!(out, "{}", json!({"kind": "header", "model": receipt, "win": opt.win,
        "win_families": opt.win_families, "max_nodes": opt.bounds.max_nodes, "max_depth": opt.bounds.max_depth}))
    .map_err(|e| e.to_string())?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let job: LabelJob = serde_json::from_str(line).map_err(|e| format!("{path}: {e}"))?;
        let started = std::time::Instant::now();
        let mut buffer = Vec::new();
        let row = census_job(&job, &mut scorer, &opt, &mut buffer)
            .unwrap_or_else(|e| json!({"kind": "error", "id": job.id, "error": e}));
        if row["kind"] == "game" {
            cases.write_all(&buffer).map_err(|e| e.to_string())?;
        }
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        cases.flush().map_err(|e| e.to_string())?;
        if let Some(t) = timing.as_mut() {
            writeln!(
                t,
                "{}",
                json!({"id": job.id, "seconds": started.elapsed().as_secs_f64()})
            )
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
    use crate::rl_session::FastActorDecisionKindV1;

    const ME: PlayerSeatV1 = PlayerSeatV1::P0;
    const YOU: PlayerSeatV1 = PlayerSeatV1::P1;

    fn card(arena_id: u32, controller: PlayerSeatV1, zone: Zone) -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id,
            card_db_id: 11,
            owner: controller,
            controller,
            zone,
            zone_change_count: 1,
        }
    }

    fn target(source: u32, object: CardStableRefV1) -> ActionSemanticV1 {
        ActionSemanticV1::ChooseTarget {
            actor: ME,
            source: card(source, ME, Zone::Stack),
            remaining: 1,
            target: TargetRefV1::Object { object },
        }
    }

    fn pass() -> ActionSemanticV1 {
        ActionSemanticV1::Pass { actor: ME }
    }

    fn mana(source: u32) -> ActionSemanticV1 {
        ActionSemanticV1::ActivateManaAbility {
            actor: ME,
            source: card(source, ME, Zone::Battlefield),
            mana_choice: None,
            cost_target: None,
        }
    }

    fn block(attacker: u32, blocker: u32, include: bool) -> ActionSemanticV1 {
        ActionSemanticV1::ChooseBlockerInclusion {
            actor: ME,
            attacker: card(attacker, YOU, Zone::Battlefield),
            blocker: card(blocker, ME, Zone::Battlefield),
            include,
        }
    }

    #[test]
    fn target_steps_need_one_source_and_a_target_row() {
        let own = card(5, ME, Zone::Battlefield);
        assert_eq!(target_steps_source(&[target(7, own.clone())]), Some(7));
        let finish = ActionSemanticV1::FinishTargetSelection {
            actor: ME,
            source: card(7, ME, Zone::Stack),
            selected_count: 1,
        };
        assert_eq!(target_steps_source(std::slice::from_ref(&finish)), None);
        assert_eq!(
            target_steps_source(&[target(7, own.clone()), finish]),
            Some(7)
        );
        assert_eq!(
            target_steps_source(&[target(7, own.clone()), target(8, own.clone())]),
            None
        );
        assert_eq!(target_steps_source(&[target(7, own), pass()]), None);
        assert_eq!(target_steps_source(&[]), None);
    }

    #[test]
    fn own_targets_are_own_permanents_or_self() {
        let t = |object| TargetRefV1::Object { object };
        assert!(own_target(&t(card(5, ME, Zone::Battlefield)), ME));
        assert!(!own_target(&t(card(5, YOU, Zone::Battlefield)), ME));
        assert!(!own_target(&t(card(5, ME, Zone::Graveyard)), ME));
        assert!(own_target(&TargetRefV1::Player { player: ME }, ME));
        assert!(!own_target(&TargetRefV1::Player { player: YOU }, ME));
    }

    #[test]
    fn seed_family_from_job_id() {
        assert_eq!(family("b512-vs-b512/o4-t7-k0-s1"), Some(0));
        assert_eq!(family("c1-u0065536-vs-b512/o0-t8-k11-s0"), Some(11));
        assert_eq!(family("x/o0-t0-s0"), None);
    }

    #[test]
    fn blocker_scan_reads_one_pair() {
        let scan = blocker_scan_menu(&[block(1, 2, false), block(1, 2, true)]).unwrap();
        assert_eq!(
            scan,
            BlockerScan {
                include_row: Some(1),
                exclude_row: Some(0)
            }
        );
        let forced = blocker_scan_menu(&[block(1, 2, true)]).unwrap();
        assert_eq!(forced.exclude_row, None);
        assert!(blocker_scan_menu(&[block(1, 2, true), block(1, 3, false)]).is_none());
        assert!(blocker_scan_menu(&[block(1, 2, true), pass()]).is_none());
        assert!(blocker_scan_menu(&[]).is_none());
    }

    #[test]
    fn the_cast_window_never_links_a_trigger() {
        let own = card(5, ME, Zone::Battlefield);
        // Cast of source 7 (no target step yet): mana payment keeps it open.
        assert_eq!(
            window(Some((7, false)), true, &[mana(3), mana(4)]),
            Window::Keep
        );
        // Its own target step links.
        assert_eq!(
            window(Some((7, false)), true, &[target(7, own.clone())]),
            Window::Link
        );
        // A priority decision (Pass row) or any opponent decision ends a
        // step-less cast; the creature's later enters trigger of the same
        // arena id is then unlinked, never counted.
        assert_eq!(
            window(Some((7, false)), true, &[pass(), mana(3)]),
            Window::Drop
        );
        assert_eq!(window(Some((7, false)), false, &[]), Window::Drop);
        assert_eq!(
            window(None, true, &[target(7, own.clone())]),
            Window::Unlinked
        );
        // Another source's target step during an open step-less cast is
        // unlinked too (a trigger).
        assert_eq!(
            window(Some((7, false)), true, &[target(9, own.clone())]),
            Window::Unlinked
        );
        // A cast with steps closes at the first decision that is not one.
        assert_eq!(window(Some((7, true)), true, &[mana(3)]), Window::Close);
        assert_eq!(window(Some((7, true)), false, &[]), Window::Close);
        assert_eq!(
            window(Some((7, true)), true, &[target(7, own)]),
            Window::Link
        );
        assert_eq!(window(None, false, &[]), Window::None);
    }

    #[test]
    fn harm_and_its_exclusions() {
        let t = [(ObjectId(5), 1, 0)];
        let gone = |_, _| Now::Gone;
        assert_eq!(judge(&t, gone, false, false), (true, None, 1, 0));
        assert_eq!(
            judge(&t, |_, _| Now::Same(0), false, false),
            (false, None, 0, 0)
        );
        assert_eq!(
            judge(&t, |_, _| Now::Same(2), false, false),
            (true, None, 0, 1)
        );
        assert_eq!(
            judge(&t, |_, _| Now::Flickered, false, false),
            (true, Some("flicker"), 1, 0)
        );
        assert_eq!(
            judge(&t, gone, true, false),
            (true, Some("library_shrank"), 1, 0)
        );
        assert_eq!(
            judge(&t, gone, false, true),
            (true, Some("opponent_lost"), 1, 0)
        );
        // No harm means no exclusion, whatever else happened.
        assert_eq!(judge(&[], gone, true, true), (false, None, 0, 0));
    }

    #[test]
    fn lethal_classes_in_order() {
        let w = Walk::default();
        let bounded = Walk {
            bounded: true,
            ..Walk::default()
        };
        let errored = Walk {
            errors: 1,
            ..Walk::default()
        };
        assert_eq!(
            lethal_class(false, false, true, &w),
            "recorded_continuation_failed"
        );
        assert_eq!(lethal_class(true, true, true, &w), "post_declaration_loss");
        assert_eq!(lethal_class(true, false, true, &bounded), "blunder");
        assert_eq!(lethal_class(true, false, false, &bounded), "bounded_out");
        assert_eq!(lethal_class(true, false, false, &errored), "walk_error");
        assert_eq!(lethal_class(true, false, false, &w), "no_survivor");
    }

    #[test]
    fn a_divergent_record_fails_closed() {
        let d = FastActorDecisionV1 {
            episode_id: 1,
            step: 10,
            environment_revision: 0,
            physical_decision_id: 4,
            substep_index: 0,
            substep_count: 1,
            acting_player: ME,
            decision_kind: FastActorDecisionKindV1::Surface,
            legal_action_count: 3,
        };
        let r = |step, physical_decision_id, actor, legal_action_count| RecordedDecision {
            step,
            physical_decision_id,
            actor,
            legal_action_count,
            selected: 0,
        };
        assert!(matches_record(&d, &r(10, 4, 0, 3), 0).is_ok());
        for bad in [
            r(11, 4, 0, 3),
            r(10, 5, 0, 3),
            r(10, 4, 1, 3),
            r(10, 4, 0, 2),
        ] {
            assert_eq!(
                matches_record(&d, &bad, 7),
                Err("replay differs from the record at decision 7".to_string())
            );
        }
    }

    #[test]
    fn a_replay_that_ends_early_or_with_another_winner_fails_closed() {
        assert_eq!(end_check(Some(Some(1)), Some(1)), Ok(Some(1)));
        assert_eq!(end_check(Some(None), None), Ok(None));
        assert_eq!(
            end_check(None, Some(1)),
            Err("recorded actions end before the terminal".to_string())
        );
        for (ended, recorded) in [(Some(0), Some(1)), (None, Some(0)), (Some(1), None)] {
            assert_eq!(
                end_check(Some(ended), recorded),
                Err("replayed winner differs from the record".to_string())
            );
        }
    }

    #[test]
    fn softmax_is_a_distribution() {
        let p = softmax(&[0.0, 1.0, -1.0]);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(p[1] > p[0] && p[0] > p[2]);
    }
}
