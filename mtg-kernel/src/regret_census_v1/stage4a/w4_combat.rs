//! W4 Step A combat diagnostics (mode `s4a-w4`; collab
//! LANES/w4-combat-20261030/DESIGN.md section 4). Measurement only: nothing
//! that plays, trains or selects reads these rows.
//!
//! Every mode rebuilds label jobs exactly as `s4a-label-replay` does (the
//! evaluator's constructor, opening and runtime rules) and plays with the
//! capture evaluator's own policies: `public_evaluation::load` on each
//! panel's sources, both reset with `paired_policy_seeds_v1` of the game's
//! environment seed, one seat each, as the evaluator's router does.
//!
//! `W4_MODE`:
//! - `accept`: the driver acceptance check. Each job is played from game
//!   start by the policies alone, and every decision (both seats) must equal
//!   the record. At each focal decision a policy reset just before scoring
//!   must score exactly as one reset at game start, since continuations
//!   start from reset policies.
//! - `score`: replays the recorded actions. At every focal decision (the
//!   focal seat acts, forced decisions included) it records the candidate's
//!   logits, value and scorer hidden and the public material features. At
//!   each focal blocker-pair decision it adds the rules facts, the state
//!   hidden, a random-init net's hidden and the raw action and object
//!   features. Hidden vectors go to the `W4_HIDDEN` f32 file by offset.
//! - `a2`: the forced-action continuation and playout driver at the
//!   positions in `W4_POSITIONS`.
//!
//! Environment: `LABEL_JOBS` (jsonl), `W4_SOURCES` (JSON: panel -> candidate
//! and opponent model sources), `W4_HIDDEN`, `W4_RANDOM_SEED` (default
//! 20261030), `W4_POSITIONS` (jsonl of `{"id", "index", "worlds"}`),
//! `W4_ACCEPT` (a2 only: the passed acceptance output; A2 refuses to run
//! without it), `W4_RECORDS` (a2 only: `1` adds a continuation record to
//! every value and playout row; unset or `0` leaves the rows unchanged).

use super::play::{acting, decision};
use super::replay_labels::LabelJob;
use crate::card_def::CardType;
use crate::engine::{effective_power, effective_toughness, object_has_type};
use crate::ids::{ObjectId, PlayerId};
use crate::learned_bo3_v1::public_evaluation::{load, ModelSource, Play};
use crate::native_policy_value_net_v1::{NativeNamedParameterV1, NativePolicyValueNetV1};
use crate::paired_bo1_harness_v1::{
    paired_policy_seeds_v1, PairedBo1PolicyInputV1, PairedBo1PolicyV1,
};
use crate::rl::{ActionSemanticV1, TerminalClassificationV1};
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::state::{GameState, SplitMix64, Step, Zone};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;

type Session = FastActorSessionV1;

/// Value continuation seeds (estimator) and terminal playout seeds
/// (reference); disjoint, paired within each set (DESIGN.md section 4, A2).
const VALUE_SEEDS: std::ops::Range<u64> = 0..16;
const PLAYOUT_SEEDS: std::ops::Range<u64> = 1000..1256;
const WORLD_PLAYOUT_SEEDS: std::ops::Range<u64> = 2000..2064;
const WORLDS: u64 = 4;
/// Bound on forced-pass steps in a rules-fact simulation.
const FACT_STEPS: usize = 4000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelSources {
    candidate: ModelSource,
    opponent: ModelSource,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    id: String,
    index: usize,
    worlds: bool,
}

/// The candidate and opponent of one panel, each loaded once.
struct Panel {
    candidate: Play,
    opponent: Play,
}

impl Panel {
    fn load(sources: &PanelSources) -> Result<Self, String> {
        let (candidate, _) = load(&sources.candidate)?;
        let (opponent, _) = load(&sources.opponent)?;
        Ok(Self {
            candidate,
            opponent,
        })
    }

    fn reset(&mut self, seeds: [u64; 2]) -> Result<(), String> {
        self.candidate
            .reset_for_game_v1(seeds)
            .map_err(|e| e.to_string())?;
        self.opponent
            .reset_for_game_v1(seeds)
            .map_err(|e| e.to_string())
    }

    /// One policy step by the acting seat's policy.
    fn select(
        &mut self,
        s: &Session,
        d: FastActorDecisionV1,
        focal: PlayerId,
    ) -> Result<u32, String> {
        let play = if acting(&d) == focal {
            &mut self.candidate
        } else {
            &mut self.opponent
        };
        play.select_action_v1(PairedBo1PolicyInputV1::new(s, d))
            .map_err(|e| e.to_string())
    }

    fn scorer(
        &mut self,
    ) -> Result<&mut crate::sideboard_play_policy_v1::FrozenPlayPolicyV1, String> {
        self.candidate
            .legacy_policy_mut()
            .ok_or_else(|| "the candidate must be a Legacy source".to_string())
    }
}

fn panel_of(id: &str) -> &str {
    id.split('/').next().unwrap_or(id)
}

fn env_path(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is required"))
}

fn read_jobs() -> Result<Vec<LabelJob>, String> {
    let path = env_path("LABEL_JOBS")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{path}: {e}")))
        .collect()
}

fn read_sources() -> Result<BTreeMap<String, PanelSources>, String> {
    let path = env_path("W4_SOURCES")?;
    serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|e| format!("{path}: {e}"))
}

/// Loads each panel the jobs use, once.
fn panels(
    jobs: &[&LabelJob],
    sources: &BTreeMap<String, PanelSources>,
) -> Result<BTreeMap<String, Panel>, String> {
    let mut out = BTreeMap::new();
    for job in jobs {
        let name = panel_of(&job.id);
        if !out.contains_key(name) {
            let source = sources
                .get(name)
                .ok_or_else(|| format!("no sources for panel {name}"))?;
            out.insert(name.to_string(), Panel::load(source)?);
        }
    }
    Ok(out)
}

fn session(job: &LabelJob) -> Result<Session, String> {
    crate::learned_bo3_v1::replay_game_session_v1(
        &job.config,
        job.uses_v3,
        job.game_index,
        job.environment_seed,
        job.mainboards.clone(),
        PlayerId(job.starting_player),
    )
}

/// The live decision, checked against record `i`.
fn checked(s: &Session, job: &LabelJob, i: usize) -> Result<FastActorDecisionV1, String> {
    let r = &job.decisions[i];
    let d = decision(s).ok_or_else(|| format!("replay ended before decision {i}"))?;
    if d.step != r.step
        || d.physical_decision_id != r.physical_decision_id
        || acting(&d).0 != r.actor
        || d.legal_action_count != r.legal_action_count
    {
        return Err(format!("replay differs from the record at decision {i}"));
    }
    Ok(d)
}

fn step(s: &mut Session, d: &FastActorDecisionV1, a: u32) -> Result<(), String> {
    s.step(d.episode_id, d.step, a)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

fn seat_u8(seat: crate::rl::PlayerSeatV1) -> u8 {
    match seat {
        crate::rl::PlayerSeatV1::P0 => 0,
        crate::rl::PlayerSeatV1::P1 => 1,
    }
}

/// Win 1, draw 0.5, loss 0 for `focal`, and the terminal's classification.
fn terminal_score(s: &Session, focal: PlayerId) -> Option<(f64, TerminalClassificationV1)> {
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return None;
    };
    let score = match t.winner {
        Some(w) if seat_u8(w) == focal.0 => 1.0,
        Some(_) => 0.0,
        None => 0.5,
    };
    Some((score, t.terminal_classification))
}

/// A focal decision: the focal seat acts (forced decisions included).
fn is_focal(d: &FastActorDecisionV1, focal: PlayerId) -> bool {
    acting(d) == focal
}

/// One focal blocker-inclusion decision: (attacker, blocker, include row, if
/// offered; exclude row, if offered). A forced scan step offers one row.
struct BlockerScan {
    pair: (ObjectId, ObjectId),
    include_row: Option<u32>,
    exclude_row: Option<u32>,
}

fn blocker_scan(s: &Session, d: &FastActorDecisionV1, focal: PlayerId) -> Option<BlockerScan> {
    if !is_focal(d, focal) {
        return None;
    }
    let menu = s.diagnostic_current_action_semantics()?;
    let mut scan: Option<BlockerScan> = None;
    for (row, action) in menu.iter().enumerate() {
        let ActionSemanticV1::ChooseBlockerInclusion {
            attacker,
            blocker,
            include,
            ..
        } = action
        else {
            return None;
        };
        let pair = (ObjectId(attacker.arena_id), ObjectId(blocker.arena_id));
        let entry = scan.get_or_insert(BlockerScan {
            pair,
            include_row: None,
            exclude_row: None,
        });
        if entry.pair != pair {
            return None;
        }
        if *include {
            entry.include_row = Some(row as u32);
        } else {
            entry.exclude_row = Some(row as u32);
        }
    }
    scan
}

/// A focal blocker-pair decision (both rows offered): (attacker, blocker,
/// include row).
fn blocker_pair(
    s: &Session,
    d: &FastActorDecisionV1,
    focal: PlayerId,
) -> Option<(ObjectId, ObjectId, u32)> {
    let scan = blocker_scan(s, d, focal)?;
    scan.exclude_row?;
    Some((scan.pair.0, scan.pair.1, scan.include_row?))
}

fn creatures(state: &GameState, seat: PlayerId) -> impl Iterator<Item = ObjectId> + '_ {
    state.players[seat.index()]
        .battlefield
        .iter()
        .copied()
        .filter(move |&id| object_has_type(state, id, CardType::Creature))
}

/// Public material features, focal side first (DESIGN.md section 4, A3).
fn material(state: &GameState, focal: PlayerId) -> Value {
    let side = |seat: PlayerId| {
        let mut count = 0i64;
        let (mut power, mut toughness, mut untapped) = (0i64, 0i64, 0i64);
        for id in creatures(state, seat) {
            let p = i64::from(effective_power(state, id));
            count += 1;
            power += p;
            toughness += i64::from(effective_toughness(state, id));
            if !state.objects.try_get(id).is_some_and(|o| o.tapped) {
                untapped += p;
            }
        }
        json!({"life": state.players[seat.index()].life, "creatures": count, "power": power,
            "toughness": toughness, "untapped_power": untapped,
            "hand": state.players[seat.index()].hand.len()})
    };
    json!({"focal": side(focal), "opponent": side(focal.opponent()), "turn": state.turn})
}

/// Where an object stands, to tell later whether it left the battlefield.
fn presence(state: &GameState, id: ObjectId) -> Option<u32> {
    state
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield)
        .map(|o| o.zone_change_count)
}

fn step_order(step: Step) -> u8 {
    step as u8
}

/// Whether combat damage, its state-based actions and triggers are all over
/// at the current decision: a later turn (the engine's round holds both
/// players' turns), a later step than combat damage, or a priority decision
/// in the combat damage step with an empty stack and no pending trigger.
/// Combat damage, both waves, is dealt as the step begins.
fn past_combat(s: &Session, turn: (u32, PlayerId)) -> bool {
    let state = s.game_state();
    if (state.turn, state.active_player) != turn
        || step_order(state.step) > step_order(Step::CombatDamage)
    {
        return true;
    }
    state.step == Step::CombatDamage
        && state.stack.is_empty()
        && state.engine.pending_triggers.is_empty()
        && s.diagnostic_current_action_semantics()
            .is_some_and(|m| m.iter().any(|x| matches!(x, ActionSemanticV1::Pass { .. })))
}

/// Engine-computed rules facts for one blocker pair, assuming only that pair
/// blocks: from the start of the declaration every other pair is excluded,
/// then both seats pass whenever they can (otherwise their first action)
/// until combat is over. The life facts are the frozen formula on the
/// pair's own state.
fn rules_facts(
    start: &Session,
    at: &Session,
    focal: PlayerId,
    pair: (ObjectId, ObjectId),
) -> Value {
    let state = at.game_state();
    let unblocked: i64 = state
        .engine
        .combat
        .attackers
        .iter()
        .filter(|&&a| a != pair.0)
        .map(|&a| i64::from(effective_power(state, a)))
        .sum();
    let life_after = i64::from(state.players[focal.index()].life) - unblocked;
    let before = (presence(state, pair.0), presence(state, pair.1));
    let turn = (state.turn, state.active_player);
    let mut s = start.clone();
    let mut reached = false;
    let mut impossible = false;
    let mut steps = 0usize;
    let outcome = loop {
        let Some(d) = decision(&s) else {
            break match terminal_score(&s, focal).map(|(_, c)| c) {
                Some(TerminalClassificationV1::Natural) => "terminal",
                _ => "terminal_not_natural",
            };
        };
        if steps >= FACT_STEPS {
            break "step_bound";
        }
        steps += 1;
        let a = if let Some(scan) = blocker_scan(&s, &d, focal) {
            let hit = scan.pair == pair;
            reached |= hit;
            let want = if hit {
                scan.include_row
            } else {
                scan.exclude_row
            };
            match want {
                Some(row) => row,
                None => {
                    // A forced scan step that contradicts "only this pair".
                    impossible = true;
                    break "single_pair_impossible";
                }
            }
        } else if past_combat(&s, turn) {
            break "combat_over";
        } else {
            s.diagnostic_current_action_semantics()
                .and_then(|m| {
                    m.iter()
                        .position(|x| matches!(x, ActionSemanticV1::Pass { .. }))
                })
                .unwrap_or(0) as u32
        };
        if step(&mut s, &d, a).is_err() {
            break "step_error";
        }
    };
    // Facts only from a simulation that reached the pair and finished combat
    // (or ended the game naturally); every other end leaves them null.
    let complete = reached && !impossible && matches!(outcome, "combat_over" | "terminal");
    let after = s.game_state();
    let dies = |id: ObjectId, was: Option<u32>| was.is_some() && presence(after, id) != was;
    json!({"blocker_dies": complete.then(|| dies(pair.1, before.1)),
        "attacker_dies": complete.then(|| dies(pair.0, before.0)),
        "focal_life_minus_unblocked_power": life_after, "lethal_on_board": life_after <= 0,
        "simulation": {"reached_pair": reached, "complete": complete, "end": outcome, "steps": steps}})
}

/// A random-init net of the candidate's architecture: every weight and bias
/// uniform in +-1/sqrt(fan_in) (fan_in from the preceding weight for a
/// bias), the card embedding standard normal with its padding row zero, all
/// from one SplitMix64 stream.
fn random_like(
    model: &NativePolicyValueNetV1,
    seed: u64,
) -> Result<NativePolicyValueNetV1, String> {
    let mut rng = SplitMix64::seed(seed);
    let mut unit = move || (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let mut fan_in = 1usize;
    let snapshot: Vec<NativeNamedParameterV1> = model
        .parameter_snapshot_v1()
        .into_iter()
        .map(|mut p| {
            if p.name.starts_with("card_embedding") {
                let dim = p.shape.get(1).copied().unwrap_or(1);
                for (k, v) in p.values.iter_mut().enumerate() {
                    *v = if k < dim {
                        0.0
                    } else {
                        let (u1, u2) = (unit().max(1e-300), unit());
                        ((-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()) as f32
                    };
                }
            } else {
                if p.shape.len() == 2 {
                    fan_in = p.shape[1];
                }
                let bound = 1.0 / (fan_in as f64).sqrt();
                for v in p.values.iter_mut() {
                    *v = ((2.0 * unit() - 1.0) * bound) as f32;
                }
            }
            p
        })
        .collect();
    let mut out = model.clone();
    out.replace_parameter_snapshot_v1(&snapshot)
        .map_err(|e| format!("random init: {e:?}"))?;
    Ok(out)
}

fn bits_equal(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// Appends f32 vectors to the hidden file and returns their offset (floats).
struct Sidecar {
    out: std::io::BufWriter<std::fs::File>,
    floats: u64,
}

impl Sidecar {
    fn put(&mut self, values: &[f32]) -> Result<u64, String> {
        let at = self.floats;
        for v in values {
            self.out
                .write_all(&v.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        self.floats += values.len() as u64;
        Ok(at)
    }
}

fn accept_job(
    job: &LabelJob,
    panel: &mut Panel,
    shadow: &mut Play,
    probe: &mut Play,
) -> Result<Value, String> {
    let focal = PlayerId(job.focal);
    let seeds = paired_policy_seeds_v1(job.environment_seed);
    panel.reset(seeds)?;
    shadow.reset_for_game_v1(seeds).map_err(|e| e.to_string())?;
    let mut s = session(job)?;
    let mut fresh_checks = 0u64;
    for i in 0..job.decisions.len() {
        let d = checked(&s, job, i)?;
        if is_focal(&d, focal) {
            // The shadow was reset at game start and scores every focal
            // decision in turn; the probe is reset right before scoring.
            let a = shadow
                .legacy_policy_mut()
                .ok_or("probe is not Legacy")?
                .score_fast_session_v1(&s)?;
            probe.reset_for_game_v1(seeds).map_err(|e| e.to_string())?;
            let b = probe
                .legacy_policy_mut()
                .ok_or("probe is not Legacy")?
                .score_fast_session_v1(&s)?;
            if !bits_equal(&a.logits, &b.logits) || a.value.to_bits() != b.value.to_bits() {
                return Err(format!(
                    "a freshly reset policy scores differently at decision {i}"
                ));
            }
            fresh_checks += 1;
        }
        let selected = panel.select(&s, d, focal)?;
        if selected != job.decisions[i].selected {
            return Err(format!(
                "policy selected {selected}, record {} at decision {i} (seat {})",
                job.decisions[i].selected,
                acting(&d).0
            ));
        }
        step(&mut s, &d, selected)?;
    }
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("the record ends before the terminal".into());
    };
    let winner = t.winner.map(seat_u8);
    if winner != job.winner {
        return Err("winner differs from the record".into());
    }
    Ok(
        json!({"kind": "accept", "id": job.id, "decisions": job.decisions.len(), "exact": true,
        "fresh_score_checks": fresh_checks}),
    )
}

#[allow(clippy::too_many_arguments)]
fn score_job(
    job: &LabelJob,
    panel: &mut Panel,
    random: &NativePolicyValueNetV1,
    sidecar: &mut Sidecar,
    out: &mut impl Write,
) -> Result<Value, String> {
    use crate::native_policy_value_net_v1::{ACTION_FEATURE_DIM_V1, OBJECT_FEATURE_DIM_V1};
    let focal = PlayerId(job.focal);
    panel.reset(paired_policy_seeds_v1(job.environment_seed))?;
    let mut s = session(job)?;
    let mut declaration: Option<(Session, Vec<Value>)> = None;
    let mut focal_rows = 0u64;
    for i in 0..job.decisions.len() {
        let d = checked(&s, job, i)?;
        let selected = job.decisions[i].selected;
        // A declaration is a run of focal blocker scan steps (forced ones
        // included); it starts at its first step.
        let scan = blocker_scan(&s, &d, focal);
        match &scan {
            None => declaration = None,
            Some(_) if declaration.is_none() => declaration = Some((s.clone(), Vec::new())),
            Some(_) => {}
        }
        let pair = blocker_pair(&s, &d, focal);
        if is_focal(&d, focal) {
            let policy = panel.scorer()?;
            let scores = policy.score_fast_session_v1(&s)?;
            let (reference, hidden) = policy.diagnostic_last_v4_hidden_v1(None)?;
            if !bits_equal(&scores.logits, &reference.logits)
                || scores.value.to_bits() != reference.value.to_bits()
            {
                return Err(format!(
                    "hidden capture forward differs from scoring at decision {i}"
                ));
            }
            let rows = scores.logits.len();
            let mut row = json!({"kind": "decision", "id": job.id, "focal": job.focal, "index": i,
                "step": d.step, "turn": s.game_state().turn, "active": s.game_state().active_player.0,
                "game_step": format!("{:?}", s.game_state().step), "legal": d.legal_action_count,
                "selected": selected, "value": scores.value, "logits": scores.logits,
                "scorer_hidden": sidecar.put(&hidden.scorer_hidden)?, "rows": rows,
                "material": material(s.game_state(), focal)});
            if let Some((attacker, blocker, include_row)) = pair {
                let (start, earlier) = declaration.as_ref().ok_or("declaration")?;
                let tensor = policy.last_scored_training_tensor_v4()?.common.clone();
                let (random_scores, random_hidden) =
                    policy.diagnostic_last_v4_hidden_v1(Some(random))?;
                let refs: Vec<usize> = tensor
                    .action_ref_action_indices
                    .iter()
                    .zip(&tensor.action_ref_node_indices)
                    .filter(|(a, _)| **a == i64::from(include_row))
                    .map(|(_, n)| *n as usize)
                    .collect();
                let objects: Vec<f32> = refs
                    .iter()
                    .flat_map(|&n| {
                        tensor.object_features
                            [n * OBJECT_FEATURE_DIM_V1..(n + 1) * OBJECT_FEATURE_DIM_V1]
                            .iter()
                            .copied()
                    })
                    .collect();
                let card = |id: ObjectId| s.game_state().objects.try_get(id).map(|o| o.card_def);
                row["block"] = json!({"attacker": attacker.0, "blocker": blocker.0,
                    "attacker_card": card(attacker), "blocker_card": card(blocker),
                    "attacker_controller": s.game_state().objects.try_get(attacker).map(|o| o.controller.0),
                    "include_row": include_row, "earlier_pairs": earlier,
                    "facts": rules_facts(start, &s, focal, (attacker, blocker)),
                    "state_hidden": sidecar.put(&hidden.state_hidden)?,
                    "random_state_hidden": sidecar.put(&random_hidden.state_hidden)?,
                    "random_scorer_hidden": sidecar.put(&random_hidden.scorer_hidden)?,
                    "random_value": random_scores.value,
                    "action_features": sidecar.put(&tensor.action_features[..rows * ACTION_FEATURE_DIM_V1])?,
                    "ref_nodes": refs, "ref_object_features": sidecar.put(&objects)?});
            }
            writeln!(out, "{row}").map_err(|e| e.to_string())?;
            focal_rows += 1;
        }
        if let (Some(scan), Some((_, earlier))) = (&scan, declaration.as_mut()) {
            earlier.push(json!([
                scan.pair.0 .0,
                scan.pair.1 .0,
                Some(selected) == scan.include_row
            ]));
        }
        step(&mut s, &d, selected)?;
    }
    let FastActorResponseV1::Terminal(t) = s.current_response() else {
        return Err("the record ends before the terminal".into());
    };
    let winner = t.winner.map(seat_u8);
    if winner != job.winner {
        return Err("winner differs from the record".into());
    }
    Ok(
        json!({"kind": "game", "id": job.id, "focal": job.focal, "deck_ids": job.config.deck_ids,
        "winner": winner, "decisions": job.decisions.len(), "focal_decisions": focal_rows}),
    )
}

/// How one continuation ended.
enum Ending {
    /// The candidate's raw value at the stopping decision.
    Value(f32),
    /// The game ended: win 1, draw 0.5, loss 0.
    Terminal(f64),
    /// A capture limit fired (scored 0.5).
    Cap,
    /// The engine halted (scored 0.5, counted).
    Halt,
}

impl Ending {
    fn json(&self) -> Value {
        match self {
            Self::Value(v) => json!({"value": v}),
            Self::Terminal(x) => json!({"terminal": x}),
            Self::Cap => json!({"cap": true}),
            Self::Halt => json!({"halt": true}),
        }
    }
}

/// The include choice at a blocker-inclusion decision, either seat's:
/// `Some(pair)` if `row` includes, `None` if it excludes; outside a blocker
/// scan, `Err(())`.
fn scan_choice(
    s: &Session,
    d: &FastActorDecisionV1,
    row: u32,
) -> Result<Option<(ObjectId, ObjectId)>, ()> {
    let scan = blocker_scan(s, d, acting(d)).ok_or(())?;
    Ok((scan.include_row == Some(row)).then_some(scan.pair))
}

/// The edges a blocker declaration committed, attackers in first-chosen
/// order, blockers in choice order.
fn edges_json(edges: &[(ObjectId, ObjectId)]) -> Value {
    let mut grouped: Vec<(ObjectId, Vec<u32>)> = Vec::new();
    for &(a, b) in edges {
        match grouped.iter_mut().find(|(x, _)| *x == a) {
            Some((_, bs)) => bs.push(b.0),
            None => grouped.push((a, vec![b.0])),
        }
    }
    json!(grouped
        .iter()
        .map(|(a, bs)| json!([a.0, bs]))
        .collect::<Vec<_>>())
}

/// A continuation record (`W4_RECORDS`, result memo observation 5): where it
/// stopped (round, active seat and step at the stopping decision or
/// terminal), the edges the position's blocker declaration committed and the
/// full-state diagnostic hash there. The declaration is the unbroken run of
/// blocker-inclusion decisions through the position, read from the choices
/// themselves: the engine's combat state can be pruned by deaths or reset
/// before the continuation next sees a decision.
fn record(s: &Session, blocks: Value) -> Value {
    let state = s.game_state();
    json!({"turn": state.turn, "active": state.active_player.0, "step": format!("{:?}", state.step),
        "blocks": blocks, "digest": format!("{:016x}", s.diagnostic_state_hash())})
}

/// One continuation's row: its ending, plus its record when kept.
fn row((end, record): (Ending, Option<Value>)) -> Value {
    let mut v = end.json();
    if let Some(r) = record {
        v["record"] = r;
    }
    v
}

fn ending_at_terminal(s: &Session, focal: PlayerId) -> Option<Ending> {
    terminal_score(s, focal).map(|(score, class)| match class {
        TerminalClassificationV1::Natural => Ending::Terminal(score),
        TerminalClassificationV1::Truncated => Ending::Cap,
        TerminalClassificationV1::Halted => Ending::Halt,
    })
}

/// Forces `choice` at the position, then plays both seats' policies (reset
/// with `paired_policy_seeds_v1(seed)`). With `stop_after_combat`, stops at
/// the first focal decision after combat is over and scores it with the
/// candidate's value; otherwise plays to the terminal. With `records` (the
/// declaration's edges chosen before the position), also returns the
/// continuation record.
fn continuation(
    root: &Session,
    choice: u32,
    seed: u64,
    focal: PlayerId,
    panel: &mut Panel,
    stop_after_combat: bool,
    records: Option<&[(ObjectId, ObjectId)]>,
) -> Result<(Ending, Option<Value>), String> {
    let d = decision(root).ok_or("the position has no decision")?;
    let turn = (root.game_state().turn, root.game_state().active_player);
    panel.reset(paired_policy_seeds_v1(seed))?;
    let mut edges = records.map(<[_]>::to_vec);
    if let Some(edges) = &mut edges {
        edges.extend(scan_choice(root, &d, choice).ok().flatten());
    }
    let mut scanning = true;
    let mut s = root.clone();
    step(&mut s, &d, choice)?;
    // Combat is over once seen at any decision, either seat's; a later
    // opponent spell does not move the stopping point.
    let mut combat_over = false;
    loop {
        let kept = |s: &Session, edges: &Option<Vec<_>>| {
            edges.as_deref().map(|e| record(s, edges_json(e)))
        };
        if let Some(end) = ending_at_terminal(&s, focal) {
            return Ok((end, kept(&s, &edges)));
        }
        let d = decision(&s).ok_or("no decision and no terminal")?;
        combat_over |= past_combat(&s, turn);
        if stop_after_combat && combat_over && is_focal(&d, focal) {
            let scores = panel.scorer()?.score_fast_session_v1(&s)?;
            return Ok((Ending::Value(scores.value), kept(&s, &edges)));
        }
        let a = panel.select(&s, d, focal)?;
        if let (Some(edges), true) = (&mut edges, scanning) {
            match scan_choice(&s, &d, a) {
                Ok(pair) => edges.extend(pair),
                Err(()) => scanning = false,
            }
        }
        step(&mut s, &d, a)?;
    }
}

/// Deterministic world seeds per job and decision (the `s4a-win-check` rule).
fn world_seeds(id: &str, index: usize, count: u64) -> Vec<u64> {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for byte in id.bytes() {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    (0..count)
        .map(|k| {
            let mut z =
                h ^ ((index as u64) << 20) ^ k.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        })
        .collect()
}

fn a2_position(
    job: &LabelJob,
    position: &Position,
    panel: &mut Panel,
    records: bool,
) -> Result<Value, String> {
    let focal = PlayerId(job.focal);
    let mut s = session(job)?;
    // The declaration's edges chosen before the position (records only).
    let mut prefix = Vec::new();
    for i in 0..position.index {
        let d = checked(&s, job, i)?;
        if records {
            match scan_choice(&s, &d, job.decisions[i].selected) {
                Ok(pair) => prefix.extend(pair),
                Err(()) => prefix.clear(),
            }
        }
        step(&mut s, &d, job.decisions[i].selected)?;
    }
    let records = records.then_some(prefix.as_slice());
    let d = checked(&s, job, position.index)?;
    let (attacker, blocker, include_row) =
        blocker_pair(&s, &d, focal).ok_or("the position is not a focal blocker-pair decision")?;
    let pre = panel.scorer()?.score_fast_session_v1(&s)?.value;
    let started = std::time::Instant::now();
    let mut choices = serde_json::Map::new();
    for (name, choice) in [("exclude", 1 - include_row), ("include", include_row)] {
        let mut value = Vec::new();
        for seed in VALUE_SEEDS {
            value.push(row(continuation(
                &s, choice, seed, focal, panel, true, records,
            )?));
        }
        let mut playout = Vec::new();
        for seed in PLAYOUT_SEEDS {
            playout.push(row(continuation(
                &s, choice, seed, focal, panel, false, records,
            )?));
        }
        let mut worlds = Vec::new();
        if position.worlds {
            for w in world_seeds(&job.id, position.index, WORLDS) {
                let world = s.census_redeterminized_clone_v1(w)?;
                let mut rows = Vec::new();
                for seed in WORLD_PLAYOUT_SEEDS {
                    rows.push(row(continuation(
                        &world, choice, seed, focal, panel, false, records,
                    )?));
                }
                worlds.push(json!({"world_seed": w, "playouts": rows}));
            }
        }
        choices.insert(
            name.into(),
            json!({"row": choice, "value": value, "playout": playout, "worlds": worlds}),
        );
    }
    Ok(
        json!({"kind": "position", "id": job.id, "index": position.index, "focal": job.focal,
        "attacker": attacker.0, "blocker": blocker.0, "include_row": include_row, "selected": job.decisions[position.index].selected,
        "value_pre": pre, "choices": choices, "seconds": started.elapsed().as_secs_f64()}),
    )
}

/// A2 runs only after the driver acceptance check passed: `W4_ACCEPT` names
/// its output, which must hold five exact games and no error.
fn require_acceptance() -> Result<(), String> {
    let path = env_path("W4_ACCEPT")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{path}: {e}")))
        .collect::<Result<_, _>>()?;
    let exact = rows
        .iter()
        .filter(|r| r["kind"] == "accept" && r["exact"] == true)
        .count();
    if exact < 5 || exact != rows.len() {
        return Err(format!(
            "A2 needs a passed acceptance check; {path} has {exact} exact of {} rows",
            rows.len()
        ));
    }
    Ok(())
}

/// Policy construction and play have large stack frames (the evaluator runs
/// them on a 16 MiB worker thread), so the work runs on its own thread.
const STACK_BYTES: usize = 256 << 20;

pub(super) fn run(cfg: &super::super::CensusConfigV1) -> Result<(), String> {
    let out = cfg.out.clone();
    std::thread::Builder::new()
        .name("s4a-w4".into())
        .stack_size(STACK_BYTES)
        .spawn(move || run_on_worker(&out))
        .map_err(|e| format!("spawn s4a-w4 worker: {e}"))?
        .join()
        .map_err(|_| "s4a-w4 worker panicked".to_string())?
}

fn run_on_worker(out_path: &str) -> Result<(), String> {
    let mode = env_path("W4_MODE")?;
    let jobs = read_jobs()?;
    let sources = read_sources()?;
    let mut out =
        std::io::BufWriter::new(std::fs::File::create(out_path).map_err(|e| e.to_string())?);
    let by_id: BTreeMap<&str, &LabelJob> = jobs.iter().map(|j| (j.id.as_str(), j)).collect();
    match mode.as_str() {
        "accept" => {
            let all: Vec<&LabelJob> = jobs.iter().collect();
            let mut loaded = panels(&all, &sources)?;
            // Two more copies of each candidate: a shadow reset at game start
            // and a probe reset before every score.
            let mut probes: BTreeMap<String, (Play, Play)> = BTreeMap::new();
            let mut failed = 0usize;
            for job in &jobs {
                let name = panel_of(&job.id);
                if !probes.contains_key(name) {
                    let source = &sources[name].candidate;
                    probes.insert(name.to_string(), (load(source)?.0, load(source)?.0));
                }
                let (shadow, probe) = probes.get_mut(name).ok_or("probe")?;
                let panel = loaded.get_mut(name).ok_or("panel")?;
                let row = accept_job(job, panel, shadow, probe).unwrap_or_else(|e| {
                    failed += 1;
                    json!({"kind": "error", "id": job.id, "error": e})
                });
                writeln!(out, "{row}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
            }
            if failed > 0 {
                return Err(format!(
                    "acceptance failed: {failed} of {} games differ",
                    jobs.len()
                ));
            }
        }
        "score" => {
            let all: Vec<&LabelJob> = jobs.iter().collect();
            let mut loaded = panels(&all, &sources)?;
            let seed = std::env::var("W4_RANDOM_SEED")
                .ok()
                .map(|t| {
                    t.parse::<u64>()
                        .map_err(|_| "W4_RANDOM_SEED must be an integer")
                })
                .transpose()?
                .unwrap_or(20261030);
            let hidden_path = env_path("W4_HIDDEN")?;
            let mut sidecar = Sidecar {
                out: std::io::BufWriter::new(
                    std::fs::File::create(&hidden_path).map_err(|e| e.to_string())?,
                ),
                floats: 0,
            };
            let mut random = None;
            for job in &jobs {
                let panel = loaded.get_mut(panel_of(&job.id)).ok_or("panel")?;
                if random.is_none() {
                    random = Some(random_like(panel.scorer()?.diagnostic_model_v1(), seed)?);
                }
                let random = random.as_ref().ok_or("random net")?;
                let row = score_job(job, panel, random, &mut sidecar, &mut out)
                    .unwrap_or_else(|e| json!({"kind": "error", "id": job.id, "error": e}));
                writeln!(out, "{row}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
            }
            sidecar.out.flush().map_err(|e| e.to_string())?;
        }
        "a2" => {
            require_acceptance()?;
            let records = match std::env::var("W4_RECORDS").as_deref() {
                Err(_) | Ok("0") => false,
                Ok("1") => true,
                Ok(other) => return Err(format!("W4_RECORDS must be 0 or 1, not {other}")),
            };
            let path = env_path("W4_POSITIONS")?;
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let positions: Vec<Position> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l).map_err(|e| format!("{path}: {e}")))
                .collect::<Result<_, _>>()?;
            let used: Vec<&LabelJob> = positions
                .iter()
                .map(|p| {
                    by_id
                        .get(p.id.as_str())
                        .copied()
                        .ok_or(format!("no job {}", p.id))
                })
                .collect::<Result<_, _>>()?;
            let mut loaded = panels(&used, &sources)?;
            for (position, job) in positions.iter().zip(used) {
                let panel = loaded.get_mut(panel_of(&job.id)).ok_or("panel")?;
                let row = a2_position(job, position, panel, records).unwrap_or_else(
                    |e| json!({"kind": "error", "id": job.id, "index": position.index, "error": e}),
                );
                writeln!(out, "{row}").map_err(|e| e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
            }
        }
        other => return Err(format!("unknown W4_MODE {other}")),
    }
    out.flush().map_err(|e| e.to_string())
}
