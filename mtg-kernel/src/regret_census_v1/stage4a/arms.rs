//! Candidate E and controls A and D: selection under the transition ceiling,
//! then independent evaluation on paired worlds (RUNNER.md sections 2-3).

use super::canon2::{KeyMemo, KeyMode, KeyStats, Keyer};
use super::diag::{EvalSrc, SelSrc, Trace};
use super::labels::{SpyDefs, Suffix};
use super::play::{
    act, acting, apply, decision, observe, plain_to_end, terminal, Counters, End, Meter, PlayErr,
};
use super::seeds::{Purpose, RootSeeds};
use super::tree::{
    canon, canon_values, child_key, live_index, raw_decision, Found, Key, Mapping, Node, Tree,
    MAX_DEPTH,
};
use super::world::{sample, DeckPrior};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

pub(crate) const SELECT_CAP: u64 = 512_000;
pub(crate) const EVAL_WORLDS: u64 = 16;
pub(crate) const EVAL_CAP: u64 = 65_536;
pub(crate) const MAX_CONSECUTIVE_REJECTIONS: u64 = 64;
pub(crate) const A_K: usize = 4;
pub(crate) const D_TOP: usize = 2;
pub(crate) const D_UNIFORM: usize = 2;
pub(crate) const D_HORIZON: usize = 3;
pub(crate) const ARMS: [&str; 3] = ["E", "A", "D"];

fn select_observed(node: &Node, urgency: Option<f64>, _trace: &mut Option<&mut Trace>) -> usize {
    #[cfg(feature = "viewer-recording-v1")]
    if let Some(v) = _trace
        .as_deref_mut()
        .and_then(|t| t.viewer.as_mut())
        .filter(|v| v.error.is_none())
    {
        return node.select_with_scores(urgency, &mut v.scores);
    }
    node.select_with(urgency)
}

/// Run settings (the formal values are the constants above; tests and the
/// cost-only engineering checks may lower them, and every row records them).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) select_cap: u64,
    pub(crate) eval_worlds: u64,
    pub(crate) eval_cap: u64,
}

impl Limits {
    pub(crate) const FORMAL: Self = Self {
        select_cap: SELECT_CAP,
        eval_worlds: EVAL_WORLDS,
        eval_cap: EVAL_CAP,
    };

    pub(crate) fn json(&self) -> Value {
        json!({"select_cap":self.select_cap,"eval_worlds":self.eval_worlds,"eval_cap":self.eval_cap,
            "max_consecutive_rejections":MAX_CONSECUTIVE_REJECTIONS,"a_k":A_K,"d_top":D_TOP,
            "d_uniform":D_UNIFORM,"d_horizon_physical":D_HORIZON,"e_max_depth":MAX_DEPTH,
            "e_min_exec_backups":super::tree::MIN_EXEC_BACKUPS,"e_ucb_c2":super::tree::UCB_C2,
            "formal":self.select_cap == SELECT_CAP && self.eval_worlds == EVAL_WORLDS && self.eval_cap == EVAL_CAP})
    }
}

/// Per-worker policy instances; each role has its own sampling stream.
pub(crate) struct Roles {
    pub(crate) focal: FrozenPlayPolicyV1,
    pub(crate) opps: Vec<FrozenPlayPolicyV1>,
    pub(crate) scorer: FrozenPlayPolicyV1,
    pub(crate) inner_focal: FrozenPlayPolicyV1,
    pub(crate) inner_opps: Vec<FrozenPlayPolicyV1>,
}

/// Everything fixed for one frozen root.
pub(crate) struct RootCtx<'a> {
    pub(crate) visible_history: &'a [super::e_supported::VisibleFrame],
    pub(crate) root: &'a FastActorSessionV1,
    pub(crate) d: FastActorDecisionV1,
    pub(crate) focal: PlayerId,
    pub(crate) opp: usize,
    pub(crate) seeds: &'a RootSeeds,
    pub(crate) prior: &'a DeckPrior,
    pub(crate) defs: SpyDefs,
    pub(crate) cast_root: bool,
    pub(crate) probs: Vec<f64>,
    pub(crate) limits: Limits,
    /// E's untried-action urgency (None: the formal untried-first rule).
    pub(crate) urgency: Option<f64>,
    /// E's node key version (`canon2.rs`); selection and frozen execution
    /// always use the same one.
    pub(crate) key_mode: KeyMode,
    /// Merge interchangeable choices into one edge (`interchange.rs`);
    /// selection and frozen execution always agree on it.
    pub(crate) interchange: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SamplerStats {
    pub(crate) ok: u64,
    pub(crate) rejected: u64,
    pub(crate) kinds: BTreeMap<String, u64>,
    pub(crate) seconds: f64,
}

impl SamplerStats {
    pub(crate) fn json(&self) -> Value {
        json!({"ok":self.ok,"rejected":self.rejected,"kinds":self.kinds})
    }
}

fn sample_world(
    ctx: &RootCtx,
    s: &FastActorSessionV1,
    seed: u64,
    stats: &mut SamplerStats,
) -> Result<FastActorSessionV1, String> {
    let t = Instant::now();
    let r = sample(s, seed, ctx.prior);
    stats.seconds += t.elapsed().as_secs_f64();
    match r {
        Ok(x) => {
            stats.ok += 1;
            Ok(x.world)
        }
        Err(e) => {
            stats.rejected += 1;
            let kind: String = e.split(':').next().unwrap_or("").chars().take(60).collect();
            *stats.kinds.entry(kind).or_default() += 1;
            Err(e)
        }
    }
}

pub(crate) fn softmax(logits: &[f32]) -> Vec<f64> {
    let m = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max) as f64;
    let e: Vec<f64> = logits.iter().map(|&x| ((x as f64) - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.into_iter().map(|x| x / s).collect()
}

/// Indices by descending probability, ties by index.
pub(crate) fn prob_order(probs: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..probs.len()).collect();
    order.sort_by(|&a, &b| {
        probs[b]
            .partial_cmp(&probs[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    order
}

fn uniform_draw(pool: &[usize], take: usize, rng: &mut SplitMix64) -> Vec<usize> {
    let mut pool = pool.to_vec();
    let take = take.min(pool.len());
    for i in 0..take {
        let bound = (pool.len() - i) as u64;
        let threshold = bound.wrapping_neg() % bound;
        let j = i + loop {
            let v = rng.next_u64();
            if v >= threshold {
                break (v % bound) as usize;
            }
        };
        pool.swap(i, j);
    }
    pool.truncate(take);
    pool
}

/// Best mean; ties to the higher policy probability, then the lower index.
fn best_by_mean(cands: &[usize], means: &[f64], probs: &[f64]) -> usize {
    let mut best = 0;
    for i in 1..cands.len() {
        if means[i] > means[best]
            || (means[i] == means[best] && probs[cands[i]] > probs[cands[best]])
        {
            best = i;
        }
    }
    best
}

/// A: top four root candidates by policy probability.
pub(crate) fn a_candidates(probs: &[f64]) -> Vec<usize> {
    prob_order(probs).into_iter().take(A_K).collect()
}

/// D: top two plus two uniform draws from the rest (all if the menu has at
/// most four actions), fixed per root.
pub(crate) fn d_candidates(probs: &[f64], seeds: &RootSeeds) -> Vec<usize> {
    let order = prob_order(probs);
    if order.len() <= D_TOP + D_UNIFORM {
        return order;
    }
    let mut c: Vec<usize> = order[..D_TOP].to_vec();
    let mut rng = SplitMix64::seed(seeds.get(Purpose::Ties, "D", 0, b"root-candidates"));
    c.extend(uniform_draw(&order[D_TOP..], D_UNIFORM, &mut rng));
    c
}

/// Where a D improved continuation runs (selection simulation or evaluation
/// world), for its seeds.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    Select(u64),
    Eval(u64),
}

impl Phase {
    fn tag(self) -> (&'static str, u64) {
        match self {
            Self::Select(i) => ("select", i),
            Self::Eval(e) => ("eval", e),
        }
    }
}

/// D improved-continuation counters.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DStats {
    pub(crate) improved: u64,
    pub(crate) deviations: u64,
    pub(crate) inner_rejections: u64,
    pub(crate) inner_nonnatural: u64,
}

impl DStats {
    fn add(&mut self, o: &Self) {
        self.improved += o.improved;
        self.deviations += o.deviations;
        self.inner_rejections += o.inner_rejections;
        self.inner_nonnatural += o.inner_nonnatural;
    }

    fn json(&self) -> Value {
        json!({"improved_decisions":self.improved,"deviations":self.deviations,
            "inner_rejections":self.inner_rejections,"inner_nonnatural":self.inner_nonnatural})
    }
}

/// Discovery labels over a set of trajectories.
#[derive(Clone, Debug, Default)]
pub(crate) struct Discovery {
    pub(crate) trajectories: u64,
    pub(crate) natural: u64,
    pub(crate) completions_natural: u64,
    pub(crate) completion_wins: u64,
    pub(crate) completion_seeds: Vec<u64>,
    pub(crate) cast_chosen: u64,
    pub(crate) self_offered: u64,
    pub(crate) self_chosen: u64,
    pub(crate) dr_giant_offered: u64,
    pub(crate) dr_giant_chosen: u64,
    pub(crate) self_target_resolved: u64,
    pub(crate) dr_giant_resolved: u64,
    pub(crate) dr_giant_stacked: u64,
}

impl Discovery {
    fn add(&mut self, s: &Suffix, end: Option<End>, seed_index: u64) {
        self.trajectories += 1;
        self.cast_chosen += u64::from(s.cast_chosen);
        self.self_offered += u64::from(s.self_offered);
        self.self_chosen += u64::from(s.self_chosen);
        self.dr_giant_offered += u64::from(s.dr_giant_offered);
        self.dr_giant_chosen += u64::from(s.dr_giant_chosen);
        self.self_target_resolved += u64::from(s.self_target_resolved);
        self.dr_giant_resolved += u64::from(s.dr_giant_resolved);
        self.dr_giant_stacked += u64::from(s.dr_giant_stacked);
        if let Some(End::Natural { win }) = end {
            self.natural += 1;
            if s.complete() {
                self.completions_natural += 1;
                self.completion_wins += u64::from(win);
                if !self.completion_seeds.contains(&seed_index) {
                    self.completion_seeds.push(seed_index);
                }
            }
        }
    }

    fn json(&self) -> Value {
        json!({"discovered":self.completions_natural > 0,"trajectories":self.trajectories,
            "natural":self.natural,"completions_natural":self.completions_natural,
            "completion_wins":self.completion_wins,"completion_distinct_seeds":self.completion_seeds.len(),
            "cast_chosen":self.cast_chosen,"self_offered":self.self_offered,"self_chosen":self.self_chosen,
            "dr_giant_offered":self.dr_giant_offered,"dr_giant_chosen":self.dr_giant_chosen,
            "self_target_resolved":self.self_target_resolved,"dr_giant_resolved":self.dr_giant_resolved,"dr_giant_stacked":self.dr_giant_stacked})
    }
}

/// One arm's selection result.
#[derive(Clone, Debug, Default)]
pub(crate) struct Selection {
    pub(crate) transitions: u64,
    pub(crate) inference: u64,
    pub(crate) wall: f64,
    pub(crate) attempted: u64,
    pub(crate) completed: u64,
    pub(crate) nonnatural: u64,
    pub(crate) cap_reached: bool,
    pub(crate) incomplete: bool,
    pub(crate) faults: Vec<String>,
    pub(crate) sampler: SamplerStats,
    pub(crate) discovery: Discovery,
    pub(crate) d: DStats,
    /// A/D: root candidate edges (canonical bytes) and the chosen one.
    pub(crate) choice: Option<Vec<u8>>,
    pub(crate) extra: Value,
}

impl Selection {
    pub(crate) fn json(&self) -> Value {
        json!({"transitions":self.transitions,"inference_calls":self.inference,
            "simulations_attempted":self.attempted,"completed_natural":self.completed,
            "nonnatural":self.nonnatural,"cap_reached":self.cap_reached,"incomplete":self.incomplete,
            "faults":self.faults,"sampler":self.sampler.json(),"discovery":self.discovery.json(),
            "d_continuation":self.d.json(),"no_completed_evidence":self.completed == 0,"fallback":self.choice.is_none() && self.extra.get("tree").is_none(),
            "extra":self.extra})
    }
}

/// One evaluation rollout's record.
#[derive(Clone, Debug)]
pub(crate) struct EvalOut {
    pub(crate) end: &'static str,
    pub(crate) w: bool,
    pub(crate) j: bool,
    pub(crate) unknown: bool,
    pub(crate) transitions: u64,
    pub(crate) inference: u64,
    pub(crate) suffix: Suffix,
    pub(crate) fallback_root: bool,
    pub(crate) d: DStats,
    pub(crate) e: Value,
    pub(crate) fault: Option<String>,
    /// E's frozen-execution choices (see `EStats::decisions`); not serialized.
    pub(crate) decisions: Vec<(u64, u32, u8)>,
}

impl EvalOut {
    pub(crate) fn json(&self) -> Value {
        let mut v = json!({"end":self.end,"w":self.w,"j":self.j,"unknown":self.unknown,
            "transitions":self.transitions,"inference_calls":self.inference,
            "suffix":self.suffix.json(),"root_fallback":self.fallback_root});
        if self.d.improved > 0 || self.d.inner_rejections > 0 {
            v["d_continuation"] = self.d.json();
        }
        if !self.e.is_null() {
            v["e"] = self.e.clone();
        }
        if let Some(f) = &self.fault {
            v["fault"] = json!(f);
        }
        v
    }
}

fn finish(end: Result<End, PlayErr>, suffix: Suffix, meter: &Meter, c: &Counters) -> EvalOut {
    let (end_label, w, unknown, fault) = match &end {
        Ok(End::Natural { win: true }) => ("win", true, false, None),
        Ok(End::Natural { win: false }) => ("nonwin", false, false, None),
        Ok(End::NonNatural) => ("nonnatural", false, true, None),
        Err(PlayErr::Truncated) => ("truncated", false, true, None),
        Err(PlayErr::Fault(e)) => ("fault", false, true, Some(e.clone())),
    };
    EvalOut {
        end: end_label,
        w,
        j: w && suffix.complete(),
        unknown,
        transitions: meter.spent,
        inference: c.inference,
        suffix,
        fallback_root: false,
        d: DStats::default(),
        e: Value::Null,
        fault,
        decisions: Vec::new(),
    }
}

impl Roles {
    /// D's improved choice at the current focal decision: top two, one
    /// uniform draw from the rest and the model-sampled action, one inner
    /// plain playout each on one shared lawful sample of the focal player's
    /// information set (never the hidden world). The sampled action is kept
    /// unless another candidate is strictly better. A rejected inner sample
    /// or a non-natural inner ending keeps the sampled action.
    #[allow(clippy::too_many_arguments, reason = "explicit roles and accounting")]
    fn improve(
        &mut self,
        ctx: &RootCtx,
        s: &FastActorSessionV1,
        sampled: u32,
        phase: Phase,
        ordinal: u64,
        meter: &mut Meter,
        c: &mut Counters,
        stats: &mut DStats,
        sampler: &mut SamplerStats,
    ) -> Result<u32, PlayErr> {
        stats.improved += 1;
        c.inference += 1;
        let probs = softmax(
            &self
                .scorer
                .score_fast_session_v1(s)
                .map_err(PlayErr::Fault)?
                .logits,
        );
        let (tag, index) = phase.tag();
        let order = prob_order(&probs);
        let mut cands: Vec<usize> = order.iter().copied().take(2).collect();
        if order.len() > 2 {
            let mut rng = SplitMix64::seed(ctx.seeds.get(
                Purpose::Ties,
                "D",
                index,
                format!("{tag}/{ordinal}").as_bytes(),
            ));
            cands.extend(uniform_draw(&order[2..], 1, &mut rng));
        }
        if !cands.contains(&(sampled as usize)) {
            cands.push(sampled as usize);
        }
        let inner_seed = match phase {
            Phase::Select(i) => ctx.seeds.get(
                Purpose::Select,
                "D",
                i,
                format!("inner/{ordinal}").as_bytes(),
            ),
            Phase::Eval(e) => {
                ctx.seeds
                    .get(Purpose::EvalInner, "D", e, format!("{ordinal}").as_bytes())
            }
        };
        let Ok(world) = sample_world(ctx, s, inner_seed, sampler) else {
            stats.inner_rejections += 1;
            return Ok(sampled);
        };
        let d = decision(&world).ok_or_else(|| PlayErr::Fault("inner world is terminal".into()))?;
        let fs = ctx
            .seeds
            .policy("D", index, &format!("{tag}-inner/{ordinal}/focal"));
        let os = ctx
            .seeds
            .policy("D", index, &format!("{tag}-inner/{ordinal}/opp"));
        let mut means = Vec::with_capacity(cands.len());
        for &cand in &cands {
            let mut w = world.clone();
            self.inner_focal.reset_sampling_v1(fs);
            self.inner_opps[ctx.opp].reset_sampling_v1(os);
            apply(&mut w, &d, cand as u32, meter, ctx.focal, None, &ctx.defs)?;
            match plain_to_end(
                &mut w,
                ctx.focal,
                &mut self.inner_focal,
                &mut self.inner_opps[ctx.opp],
                meter,
                c,
                None,
                &ctx.defs,
            )? {
                End::Natural { win } => means.push(f64::from(u8::from(win))),
                End::NonNatural => {
                    stats.inner_nonnatural += 1;
                    return Ok(sampled);
                }
            }
        }
        let own = cands
            .iter()
            .position(|&x| x == sampled as usize)
            .expect("sampled is a candidate");
        let best = best_by_mean(&cands, &means, &probs);
        let chosen = if means[best] > means[own] {
            cands[best] as u32
        } else {
            sampled
        };
        stats.deviations += u64::from(chosen != sampled);
        Ok(chosen)
    }

    /// From a world at the root decision: play `action`, then plain or D's
    /// improved continuation to the end.
    #[allow(clippy::too_many_arguments, reason = "explicit roles and accounting")]
    fn play_from_root(
        &mut self,
        ctx: &RootCtx,
        s: &mut FastActorSessionV1,
        action: u32,
        improved: Option<Phase>,
        meter: &mut Meter,
        c: &mut Counters,
        mut labels: Option<&mut Suffix>,
        dstats: &mut DStats,
        sampler: &mut SamplerStats,
    ) -> Result<End, PlayErr> {
        let d = decision(s).ok_or_else(|| PlayErr::Fault("root is terminal".into()))?;
        observe(labels.as_deref_mut(), s, action, ctx.focal, &ctx.defs);
        apply(
            s,
            &d,
            action,
            meter,
            ctx.focal,
            labels.as_deref_mut(),
            &ctx.defs,
        )?;
        let Some(phase) = improved else {
            return plain_to_end(
                s,
                ctx.focal,
                &mut self.focal,
                &mut self.opps[ctx.opp],
                meter,
                c,
                labels,
                &ctx.defs,
            );
        };
        let root_phys = ctx.d.physical_decision_id;
        let mut phys: Vec<u64> = Vec::new();
        let mut ordinal = 0u64;
        loop {
            if let Some(end) = terminal(s, ctx.focal) {
                return Ok(end);
            }
            let d = decision(s).expect("decision");
            let a = if acting(&d) == ctx.focal {
                let sampled = act(&mut self.focal, s, &d, c)?;
                let a = if d.legal_action_count >= 2 {
                    let pid = d.physical_decision_id;
                    let window = pid == root_phys
                        || phys.contains(&pid)
                        || (phys.len() < D_HORIZON && {
                            phys.push(pid);
                            true
                        });
                    let a = if window {
                        ordinal += 1;
                        self.improve(ctx, s, sampled, phase, ordinal, meter, c, dstats, sampler)?
                    } else {
                        sampled
                    };
                    observe(labels.as_deref_mut(), s, a, ctx.focal, &ctx.defs);
                    a
                } else {
                    sampled
                };
                a
            } else {
                act(&mut self.opps[ctx.opp], s, &d, c)?
            };
            apply(s, &d, a, meter, ctx.focal, labels.as_deref_mut(), &ctx.defs)?;
        }
    }

    fn reset_main(&mut self, ctx: &RootCtx, arm: &str, index: u64, phase: &str) {
        let _ = arm;
        self.focal
            .reset_sampling_v1(ctx.seeds.policy("", index, &format!("{phase}/focal")));
        self.opps[ctx.opp].reset_sampling_v1(ctx.seeds.policy("", index, &format!("{phase}/opp")));
    }

    /// A or D selection: round-robin over the root candidates on one shared
    /// lawful world per round; cumulative means over completed rounds.
    pub(crate) fn select_rounds(&mut self, ctx: &RootCtx, arm: &str, cands: &[usize]) -> Selection {
        let started = Instant::now();
        let improved = arm == "D";
        let root_canon = canon(ctx.root, ctx.d);
        let mut sel = Selection::default();
        let mut meter = Meter::new(ctx.limits.select_cap);
        let mut c = Counters::default();
        let n = cands.len();
        let mut sums = vec![0u64; n];
        let mut rounds = 0u64;
        let mut consecutive = 0u64;
        let mut i = 0u64;
        'rounds: loop {
            if consecutive >= MAX_CONSECUTIVE_REJECTIONS {
                sel.incomplete = true;
                break;
            }
            let seed = ctx.seeds.get(Purpose::Select, "", i, b"world");
            let index = i;
            i += 1;
            sel.attempted += 1;
            let world = match sample_world(ctx, ctx.root, seed, &mut sel.sampler) {
                Ok(w) => w,
                Err(_) => {
                    consecutive += 1;
                    continue;
                }
            };
            consecutive = 0;
            let mut scores = Vec::with_capacity(n);
            let mut ok = true;
            for &cand in cands {
                let mut s = world.clone();
                self.reset_main(ctx, arm, index, "select");
                let mut suffix = Suffix::new(ctx.cast_root);
                let mut dstats = DStats::default();
                let r = self.play_from_root(
                    ctx,
                    &mut s,
                    cand as u32,
                    improved.then_some(Phase::Select(index)),
                    &mut meter,
                    &mut c,
                    Some(&mut suffix),
                    &mut dstats,
                    &mut sel.sampler,
                );
                sel.d.add(&dstats);
                match r {
                    Ok(End::Natural { win }) => {
                        sel.discovery
                            .add(&suffix, Some(End::Natural { win }), index);
                        scores.push(u64::from(win));
                    }
                    Ok(End::NonNatural) => {
                        sel.discovery.add(&suffix, Some(End::NonNatural), index);
                        sel.nonnatural += 1;
                        ok = false;
                        break;
                    }
                    Err(PlayErr::Truncated) => {
                        sel.cap_reached = true;
                        break 'rounds;
                    }
                    Err(PlayErr::Fault(e)) => {
                        // An instrument fault invalidates the root: stop
                        // (a fault before any transition would otherwise
                        // repeat without charging the ceiling).
                        sel.faults.push(e.chars().take(300).collect());
                        sel.incomplete = true;
                        break 'rounds;
                    }
                }
            }
            if ok {
                for (s, x) in sums.iter_mut().zip(&scores) {
                    *s += x;
                }
                rounds += 1;
            }
        }
        sel.completed = rounds;
        sel.transitions = meter.spent;
        sel.inference = c.inference;
        let means: Vec<f64> = sums
            .iter()
            .map(|&s| {
                if rounds > 0 {
                    s as f64 / rounds as f64
                } else {
                    f64::NAN
                }
            })
            .collect();
        let chosen = (rounds > 0).then(|| cands[best_by_mean(cands, &means, &ctx.probs)]);
        match (&root_canon, chosen) {
            (Ok(rc), Some(a)) => sel.choice = Some(rc.menu[a].clone()),
            (Err(PlayErr::Fault(e)), _) => sel.faults.push(e.clone()),
            _ => {}
        }
        sel.extra = json!({"candidates":cands,"candidate_probs":cands.iter().map(|&x| ctx.probs[x]).collect::<Vec<_>>(),
            "rounds_completed":rounds,"wins":sums,"means":if rounds > 0 { json!(means) } else { Value::Null },
            "chosen":chosen,"chosen_prob":chosen.map(|a| ctx.probs[a])});
        sel.wall = started.elapsed().as_secs_f64();
        sel
    }

    /// One E selection simulation on `world`. Returns the path, the new node
    /// (if any) and the ending; the tree is not modified.
    #[allow(clippy::type_complexity, reason = "one simulation's whole result")]
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit roles, accounting and trace"
    )]
    fn simulate(
        &mut self,
        ctx: &RootCtx,
        tree: &Tree,
        mut s: FastActorSessionV1,
        meter: &mut Meter,
        c: &mut Counters,
        suffix: &mut Suffix,
        mut trace: Option<&mut Trace>,
        retain: usize,
        staged: &std::collections::HashSet<Key>,
        rec: &mut SimRecord,
        mut capture: Option<&mut Capture>,
        diag: &mut SearchDiag,
        mut memo: Option<&mut KeyMemo>,
    ) -> Result<(Vec<(Key, usize)>, Vec<(Key, Node)>, End), PlayErr> {
        let capturing = capture
            .as_deref()
            .is_some_and(|c| !c.keys.is_empty() || c.all_up_to > 0 || !c.snapshot_sims.is_empty());
        if ctx.key_mode != KeyMode::Legacy && (retain > 1 || capturing) {
            return Err(PlayErr::Fault(
                "depth retention and capture use legacy keys only".into(),
            ));
        }
        if ctx.interchange && (retain > 1 || capturing || !ctx.visible_history.is_empty()) {
            return Err(PlayErr::Fault(
                "depth retention, capture and E history do not merge interchangeable choices"
                    .into(),
            ));
        }
        let SearchDiag {
            keys,
            legacy_path,
            path_log,
            ..
        } = diag;
        legacy_path.clear();
        let mut keyer = Keyer::with_history(ctx.key_mode, keys, ctx.visible_history)?
            .with_interchange(ctx.interchange);
        if memo.is_some() {
            keyer.start_memo();
        }
        let mut sup = Sup::new();
        let mut path: Vec<(Key, usize)> = Vec::new();
        let mut new: Vec<(Key, Node)> = Vec::new();
        let mut parent: (Key, Vec<u8>) = ([0u8; 32], b"root".to_vec());
        let (spent0, calls0) = (meter.spent, c.inference);
        let snap = |depth: u32, meter: &Meter, c: &Counters| Snap {
            depth,
            transitions: meter.spent - spent0,
            calls: c.inference - calls0,
        };
        let mut in_tree = true;
        let mut depth = 0u32;
        let mut last_phys: Option<u64> = None;
        loop {
            let span = crate::search_profile_v1::span(crate::search_profile_v1::Bucket::Response);
            if let Some(end) = terminal(&s, ctx.focal) {
                drop(span);
                sup.finish(rec);
                return Ok((path, new, end));
            }
            let d = decision(&s).expect("decision");
            drop(span);
            let a = if acting(&d) == ctx.focal {
                if d.legal_action_count >= 2 {
                    if last_phys != Some(d.physical_decision_id) {
                        depth += 1;
                        last_phys = Some(d.physical_decision_id);
                    }
                    // Diagnostic: whether this choice executes a tree edge
                    // with eight prior natural terminal backups.
                    let mut supported = false;
                    // Accounting only: the tree prefix ends at the first
                    // choice past the depth limit when nothing expanded.
                    if in_tree && depth > MAX_DEPTH && rec.depth_limit.is_none() {
                        rec.depth_limit = Some(snap(depth, meter, c));
                    }
                    // Trace only: where this choice comes from.
                    let mut src = if in_tree {
                        SelSrc::TailDepth
                    } else {
                        SelSrc::TailAfterExpand
                    };
                    let a = if in_tree && depth <= MAX_DEPTH {
                        let span = crate::search_profile_v1::span(
                            crate::search_profile_v1::Bucket::KeyStep,
                        );
                        let st = keyer.step_memo(&s, d, memo.as_deref_mut())?;
                        let (key, pk, pe, cn) = keyer.storage(&st);
                        drop(span);
                        let span = crate::search_profile_v1::span(
                            crate::search_profile_v1::Bucket::TreeFind,
                        );
                        let found = tree.find(&key, &pk, &pe, cn)?;
                        drop(span);
                        let (edge_index, edge) = match found {
                            Found::Hit(node) => {
                                let _span = crate::search_profile_v1::span(
                                    crate::search_profile_v1::Bucket::NodeSelect,
                                );
                                src = SelSrc::Tree;
                                rec.existing += 1;
                                if staged.contains(&key) {
                                    rec.staged_hits += 1;
                                }
                                let e = select_observed(node, ctx.urgency, &mut trace);
                                supported = node.n[e] >= super::tree::MIN_EXEC_BACKUPS;
                                (e, cn.edges[e].clone())
                            }
                            Found::Miss => {
                                src = SelSrc::Expand;
                                if let Some(cap) = capture.as_deref_mut() {
                                    cap.see(key, &s, d, depth, cn, &path)?;
                                }
                                if let Some(t) = trace.as_deref_mut() {
                                    t.node_created(&key, path.last(), cn, &s, ctx.focal, &ctx.defs);
                                }
                                let span = crate::search_profile_v1::span(
                                    crate::search_profile_v1::Bucket::Expand,
                                );
                                let perm = keyer.perm(&st, ctx.seeds)?;
                                let mut node = Node::with_perm(pk, pe, cn.clone(), depth, perm);
                                node.skip = keyer.skipped(&st);
                                if let Some(log) = path_log.as_mut() {
                                    log.created(key, &s, cn, ctx.focal);
                                }
                                let e = select_observed(&node, ctx.urgency, &mut trace);
                                drop(span);
                                new.push((key, node));
                                in_tree = false;
                                // Boundary: before the expanded choice's
                                // action is applied.
                                rec.expansion = Some(snap(depth, meter, c));
                                (e, cn.edges[e].clone())
                            }
                        };
                        path.push((key, edge_index));
                        legacy_path.push(st.legacy_key);
                        let live = live_index(cn, &edge)?;
                        parent = (key, edge);
                        let _span = crate::search_profile_v1::span(
                            crate::search_profile_v1::Bucket::KeyAdvance,
                        );
                        keyer.advance_memo(st, live, memo.as_deref_mut());
                        live
                    } else if !in_tree && new.len() < retain && depth <= MAX_DEPTH {
                        // Depth retention (D8): stage this choice node and
                        // the rollout's own action; the rollout is unchanged.
                        let cn = canon(&s, d)?;
                        let key = child_key(&parent.0, &parent.1, &cn);
                        let a = act(&mut self.focal, &s, &d, c)?;
                        rec.rollout_choices += 1;
                        if let Some(log) = path_log.as_mut() {
                            log.created(key, &s, &cn, ctx.focal);
                        }
                        let edge = cn.menu[a as usize].clone();
                        let e = cn
                            .edges
                            .iter()
                            .position(|x| *x == edge)
                            .ok_or_else(|| PlayErr::Fault("staged edge not in menu".into()))?;
                        if let Some(cap) = capture.as_deref_mut() {
                            cap.see(key, &s, d, depth, &cn, &path)?;
                        }
                        let node =
                            Node::new(&key, parent.0, parent.1.clone(), cn, depth, ctx.seeds);
                        path.push((key, e));
                        new.push((key, node));
                        parent = (key, edge);
                        a
                    } else {
                        rec.rollout_choices += 1;
                        act(&mut self.focal, &s, &d, c)?
                    };
                    if let Some(cap) = capture.as_deref_mut() {
                        cap.opp_since.clear();
                    }
                    sup.see(d.physical_decision_id, depth, supported);
                    if let Some(t) = trace.as_deref_mut() {
                        let at = matches!(src, SelSrc::Tree | SelSrc::Expand)
                            .then(|| path.last())
                            .flatten();
                        t.sel_decision(
                            &s,
                            &d,
                            a,
                            depth,
                            src,
                            at,
                            meter.spent,
                            ctx.focal,
                            &ctx.defs,
                        );
                    }
                    observe(Some(suffix), &s, a, ctx.focal, &ctx.defs);
                    a
                } else {
                    0
                }
            } else {
                let a = act(&mut self.opps[ctx.opp], &s, &d, c)?;
                if let Some(cap) = capture.as_deref_mut() {
                    if cap.snapshot_sims.contains(&cap.sim) {
                        cap.opp_since.push((s.clone(), d.step, a));
                    }
                }
                a
            };
            #[cfg(feature = "viewer-recording-v1")]
            if let Some(v) = trace.as_deref_mut().and_then(|t| t.viewer.as_mut()) {
                v.before(&s, d, a);
            }
            apply(&mut s, &d, a, meter, ctx.focal, Some(suffix), &ctx.defs)?;
            #[cfg(feature = "viewer-recording-v1")]
            if let Some(v) = trace.as_deref_mut().and_then(|t| t.viewer.as_mut()) {
                v.after(&s, suffix.last);
            }
            if let Some(t) = trace.as_deref_mut() {
                t.event(meter.spent, acting(&d) == ctx.focal, suffix.last);
            }
        }
    }

    /// E selection: simulations until the transition ceiling; one new node
    /// per completed simulation; natural terminal backups only.
    pub(crate) fn select_e(&mut self, ctx: &RootCtx) -> (Selection, Tree) {
        self.select_e_traced(ctx, None)
    }

    /// `select_e` with an optional passive trace.
    pub(crate) fn select_e_traced(
        &mut self,
        ctx: &RootCtx,
        trace: Option<&mut Trace>,
    ) -> (Selection, Tree) {
        self.select_e_retaining(ctx, trace, 1, None)
    }

    /// `select_e_capturing` without a capture.
    pub(crate) fn select_e_retaining(
        &mut self,
        ctx: &RootCtx,
        trace: Option<&mut Trace>,
        retain: usize,
        records: Option<&mut Vec<SimRecord>>,
    ) -> (Selection, Tree) {
        self.select_e_capturing(ctx, trace, retain, records, None)
    }

    /// `select_e_traced` with up to `retain` new choice nodes per natural
    /// simulation (1 is B1, the formal rule) and optional per-simulation
    /// records. Selection, rollouts and sampling are otherwise unchanged.
    pub(crate) fn select_e_capturing(
        &mut self,
        ctx: &RootCtx,
        mut trace: Option<&mut Trace>,
        retain: usize,
        mut records: Option<&mut Vec<SimRecord>>,
        mut capture: Option<&mut Capture>,
    ) -> (Selection, Tree) {
        self.select_e_diagnosed(
            ctx,
            trace.take(),
            retain,
            records.take(),
            capture.take(),
            None,
        )
    }

    /// `select_e_capturing` with optional search diagnostics (keying cost,
    /// per-edge first wins, legacy identities per stored node).
    pub(crate) fn select_e_diagnosed(
        &mut self,
        ctx: &RootCtx,
        mut trace: Option<&mut Trace>,
        retain: usize,
        mut records: Option<&mut Vec<SimRecord>>,
        mut capture: Option<&mut Capture>,
        diag: Option<&mut SearchDiag>,
    ) -> (Selection, Tree) {
        let mut own = SearchDiag::default();
        let diag = diag.unwrap_or(&mut own);
        if diag.path_log.is_none() {
            diag.path_log = super::pathlog::dir().map(super::pathlog::PathLog::new);
        }
        let retain = retain.max(1);
        // Exact and per search: this context's keys only.
        let mut memo = key_memo_enabled().then(KeyMemo::new);
        let mut staged: std::collections::HashSet<Key> = std::collections::HashSet::new();
        let started = Instant::now();
        let mut sel = Selection::default();
        let mut tree = Tree::default();
        let mut meter = Meter::new(ctx.limits.select_cap);
        let mut c = Counters::default();
        let mut consecutive = 0u64;
        let mut i = 0u64;
        loop {
            if consecutive >= MAX_CONSECUTIVE_REJECTIONS {
                sel.incomplete = true;
                break;
            }
            let seed = ctx.seeds.get(Purpose::Select, "", i, b"world");
            let index = i;
            i += 1;
            sel.attempted += 1;
            let span = crate::search_profile_v1::span(crate::search_profile_v1::Bucket::Sample);
            let world = sample_world(ctx, ctx.root, seed, &mut sel.sampler);
            drop(span);
            let world = match world {
                Ok(w) => w,
                Err(_) => {
                    consecutive += 1;
                    if let Some(t) = trace.as_deref_mut() {
                        t.sim_rejected(index);
                    }
                    if let Some(v) = records.as_deref_mut() {
                        v.push(SimRecord {
                            outcome: SimRecord::REJECTED,
                            ..SimRecord::default()
                        });
                    }
                    continue;
                }
            };
            consecutive = 0;
            let span = crate::search_profile_v1::span(crate::search_profile_v1::Bucket::Reseed);
            self.reset_main(ctx, "E", index, "select");
            drop(span);
            let mut suffix = Suffix::new(ctx.cast_root);
            if let Some(t) = trace.as_deref_mut() {
                t.sim_begin(index, meter.spent);
            }
            let mut rec = SimRecord::default();
            if let Some(log) = diag.path_log.as_mut() {
                log.begin_sim();
            }
            let (spent0, calls0) = (meter.spent, c.inference);
            if let Some(cap) = capture.as_deref_mut() {
                cap.begin(index);
            }
            let r = self.simulate(
                ctx,
                &tree,
                world,
                &mut meter,
                &mut c,
                &mut suffix,
                trace.as_deref_mut(),
                retain,
                &staged,
                &mut rec,
                capture.as_deref_mut(),
                diag,
                memo.as_mut(),
            );
            if let Some(cap) = capture.as_deref_mut() {
                cap.end(matches!(&r, Ok((_, _, End::Natural { .. }))));
            }
            // Every ending, including truncation or a fault after inference.
            rec.transitions = meter.spent - spent0;
            rec.policy_calls = c.inference - calls0;
            rec.outcome = match &r {
                Ok((_, _, End::Natural { win: true })) => 1,
                Ok((_, _, End::Natural { win: false })) => 2,
                Ok((_, _, End::NonNatural)) => 3,
                Err(PlayErr::Truncated) => 4,
                Err(PlayErr::Fault(_)) => 5,
            };
            if let Ok((_, new, End::Natural { .. })) = &r {
                rec.new_nodes = new.len();
                for (k, _) in new.iter().skip(1) {
                    staged.insert(*k);
                }
            }
            if let Some(v) = records.as_deref_mut() {
                v.push(rec);
            }
            if let Some(t) = trace.as_deref_mut() {
                let (end, new) = match &r {
                    Ok((_, new, End::Natural { win: true })) => ("win", new.first().map(|x| &x.0)),
                    Ok((_, new, End::Natural { win: false })) => {
                        ("loss", new.first().map(|x| &x.0))
                    }
                    Ok((_, _, End::NonNatural)) => ("nonnatural", None),
                    Err(PlayErr::Truncated) => ("truncated", None),
                    Err(PlayErr::Fault(_)) => ("fault", None),
                };
                t.sim_end(end, meter.spent, new);
            }
            match r {
                Ok((path, new, End::Natural { win })) => {
                    sel.discovery
                        .add(&suffix, Some(End::Natural { win }), index);
                    let _span =
                        crate::search_profile_v1::span(crate::search_profile_v1::Bucket::Backup);
                    diag.before_backup(&tree, &path, &new, win, sel.completed);
                    if let Some(log) = diag.path_log.as_mut() {
                        log.natural(index, &path, &new, win, &rec);
                    }
                    tree.backup(&path, new, win);
                    sel.completed += 1;
                }
                Ok((_, _, End::NonNatural)) => {
                    sel.discovery.add(&suffix, Some(End::NonNatural), index);
                    sel.nonnatural += 1;
                }
                Err(PlayErr::Truncated) => {
                    sel.cap_reached = true;
                    break;
                }
                Err(PlayErr::Fault(e)) => {
                    sel.faults.push(e.chars().take(300).collect());
                    sel.incomplete = true;
                    break;
                }
            }
        }
        if let Some(t) = trace {
            t.finish_tree(&tree);
        }
        if let Some(m) = &memo {
            diag.key_memo = m.json();
        }
        sel.transitions = meter.spent;
        sel.inference = c.inference;
        if let Some(log) = diag.path_log.take() {
            if let Err(e) = log.write(ctx, &tree, sel.completed, sel.attempted, sel.transitions) {
                eprintln!("S4A_PATH_LOG: search log not written: {e}");
            }
        }
        sel.extra = json!({"tree":tree_stats(&tree)});
        sel.wall = started.elapsed().as_secs_f64();
        (sel, tree)
    }

    /// Evaluation of one arm on one world (a clone of the paired world).
    #[allow(clippy::too_many_arguments, reason = "explicit roles and accounting")]
    pub(crate) fn evaluate(
        &mut self,
        ctx: &RootCtx,
        arm: &str,
        world: &FastActorSessionV1,
        e: u64,
        choice: Option<&Vec<u8>>,
        tree: Option<&Tree>,
        sampler: &mut SamplerStats,
    ) -> EvalOut {
        self.evaluate_traced(ctx, arm, world, e, choice, tree, sampler, None)
    }

    /// `evaluate` with an optional passive trace of E's frozen execution.
    #[allow(clippy::too_many_arguments, reason = "explicit roles and accounting")]
    pub(crate) fn evaluate_traced(
        &mut self,
        ctx: &RootCtx,
        arm: &str,
        world: &FastActorSessionV1,
        e: u64,
        choice: Option<&Vec<u8>>,
        tree: Option<&Tree>,
        sampler: &mut SamplerStats,
        trace: Option<&mut Trace>,
    ) -> EvalOut {
        let mut s = world.clone();
        self.reset_main(ctx, arm, e, "eval");
        let mut meter = Meter::new(ctx.limits.eval_cap);
        let mut c = Counters::default();
        let mut suffix = Suffix::new(ctx.cast_root);
        suffix.history = Some([0u8; 32]);
        if arm == "E" {
            let mut stats = EStats::default();
            let r = self.frozen(
                ctx,
                tree.expect("E has a tree"),
                &mut s,
                &mut meter,
                &mut c,
                &mut suffix,
                &mut stats,
                trace,
                None,
            );
            let mut out = finish(r, suffix, &meter, &c);
            out.fallback_root = !stats.root_qualified;
            out.e = stats.json();
            out.decisions = std::mem::take(&mut stats.decisions);
            return out;
        }
        let mut dstats = DStats::default();
        let mut fallback = false;
        let r = (|| {
            let d = decision(&s).ok_or_else(|| PlayErr::Fault("root is terminal".into()))?;
            let action = match choice {
                Some(edge) => live_index(&canon(&s, d)?, edge)?,
                None => {
                    fallback = true;
                    act(&mut self.focal, &s, &d, &mut c)?
                }
            };
            let improved = (arm == "D").then_some(Phase::Eval(e));
            self.play_from_root(
                ctx,
                &mut s,
                action,
                improved,
                &mut meter,
                &mut c,
                Some(&mut suffix),
                &mut dstats,
                sampler,
            )
        })();
        let mut out = finish(r, suffix, &meter, &c);
        out.fallback_root = fallback;
        out.d = dstats;
        out
    }

    /// E's frozen tree executed on an evaluation world: at matching
    /// histories up to the same depth, the greatest-mean edge with at least
    /// eight backups; otherwise, and at every unseen history, plain policy.
    /// `log`, when given, receives every applied action (both seats), so a
    /// guide executed on its actual world can be replayed exactly.
    #[allow(clippy::too_many_arguments, reason = "explicit roles and accounting")]
    pub(crate) fn frozen(
        &mut self,
        ctx: &RootCtx,
        tree: &Tree,
        s: &mut FastActorSessionV1,
        meter: &mut Meter,
        c: &mut Counters,
        suffix: &mut Suffix,
        stats: &mut EStats,
        mut trace: Option<&mut Trace>,
        mut log: Option<&mut Vec<u32>>,
    ) -> Result<End, PlayErr> {
        let mut kstats = KeyStats::default();
        let mut keyer = Keyer::with_history(ctx.key_mode, &mut kstats, ctx.visible_history)?
            .with_interchange(ctx.interchange);
        let mut matching = true;
        let mut depth = 0u32;
        let mut last_phys: Option<u64> = None;
        let mut first = true;
        loop {
            if let Some(end) = terminal(s, ctx.focal) {
                return Ok(end);
            }
            let d = decision(s).expect("decision");
            let a = if acting(&d) == ctx.focal {
                if d.legal_action_count >= 2 {
                    if last_phys != Some(d.physical_decision_id) {
                        depth += 1;
                        last_phys = Some(d.physical_decision_id);
                    }
                    let root = std::mem::replace(&mut first, false);
                    // Trace only: values execution computes anyway.
                    let matching_before = matching;
                    let mut src = if matching {
                        EvalSrc::DepthLimit
                    } else {
                        EvalSrc::AfterMiss
                    };
                    let mut seen: Option<Seen> = None;
                    let a = if matching && depth <= MAX_DEPTH {
                        let st = keyer.step(s, d)?;
                        let (key, pk, pe, cn) = keyer.storage(&st);
                        if !root {
                            stats.nonroot_lookups += 1;
                        }
                        let mut hit = None;
                        let live = match tree.find(&key, &pk, &pe, cn)? {
                            Found::Hit(node) => {
                                if !root {
                                    stats.nonroot_hits += 1;
                                }
                                stats.max_hit_depth = stats.max_hit_depth.max(depth);
                                let fc = node.frozen_choice();
                                hit = Some((node, fc));
                                match fc {
                                    Some(e) => {
                                        src = EvalSrc::TreeEdge;
                                        if root {
                                            stats.root_qualified = true;
                                        }
                                        stats.executed.push((depth, node.n[e]));
                                        live_index(cn, &cn.edges[e])?
                                    }
                                    None => {
                                        src = EvalSrc::MatchedNoQualified;
                                        stats.fallbacks += 1;
                                        // An interchangeable copy plays as
                                        // its representative, so the history
                                        // stays on the stored edge.
                                        let a = act(&mut self.focal, s, &d, c)?;
                                        keyer.representative(&st, a)
                                    }
                                }
                            }
                            Found::Miss => {
                                src = EvalSrc::FirstMiss;
                                matching = false;
                                act(&mut self.focal, s, &d, c)?
                            }
                        };
                        if trace.is_some() {
                            let m = &cn.menu[live as usize];
                            let edge = cn.edges.iter().position(|x| x == m).unwrap_or(usize::MAX);
                            seen = Some((key, hit, edge));
                        }
                        keyer.advance(st, live);
                        live
                    } else {
                        act(&mut self.focal, s, &d, c)?
                    };
                    stats.decisions.push((
                        d.physical_decision_id,
                        d.substep_index,
                        match src {
                            EvalSrc::TreeEdge => 0,
                            EvalSrc::MatchedNoQualified => 1,
                            EvalSrc::FirstMiss => 2,
                            EvalSrc::AfterMiss => 3,
                            EvalSrc::DepthLimit => 4,
                        },
                    ));
                    if let Some(t) = trace.as_deref_mut() {
                        let key = seen.as_ref().map(|x| &x.0);
                        let hit = seen.as_ref().and_then(|x| x.1);
                        let edge = seen.as_ref().map(|x| x.2);
                        t.eval_decision(
                            s,
                            &d,
                            a,
                            depth,
                            (matching_before, matching),
                            src,
                            root,
                            key,
                            hit,
                            edge,
                            meter.spent,
                            ctx.focal,
                            &ctx.defs,
                        );
                    }
                    observe(Some(suffix), s, a, ctx.focal, &ctx.defs);
                    a
                } else {
                    if let Some(t) = trace.as_deref_mut() {
                        t.count("forced");
                    }
                    0
                }
            } else {
                if let Some(t) = trace.as_deref_mut() {
                    t.count("opp");
                }
                act(&mut self.opps[ctx.opp], s, &d, c)?
            };
            if let Some(l) = log.as_deref_mut() {
                l.push(a);
            }
            apply(s, &d, a, meter, ctx.focal, Some(suffix), &ctx.defs)?;
            if let Some(t) = trace.as_deref_mut() {
                t.event(meter.spent, acting(&d) == ctx.focal, suffix.last);
            }
        }
    }
}

/// One captured node creation (diagnostic only): the raw actor-visible
/// decision, its canonical mapping, where it was created, and (operator-only)
/// private snapshots of the opponent decisions since the focal player's
/// previous choice.
pub(crate) struct Captured {
    pub(crate) sim: u64,
    pub(crate) step: u64,
    pub(crate) physical_decision_id: u64,
    pub(crate) substep: u32,
    pub(crate) depth: u32,
    pub(crate) raw_obs: Value,
    pub(crate) raw_menu: Vec<Value>,
    pub(crate) mapping: Mapping,
    /// Canonical menu in live order (indices match `raw_menu`).
    pub(crate) canon_menu: Vec<Vec<u8>>,
    /// (key, edge index) of every tree node on the way to this one.
    pub(crate) path: Vec<(Key, usize)>,
    /// (state before the opponent acted, step, action), operator-only.
    pub(crate) opp_since: Vec<(FastActorSessionV1, u64, u32)>,
}

/// Passive capture during selection: the creating simulation of every
/// committed node, and full raw views for fixed keys. Nothing here is read by
/// selection, backup, sampling or execution.
#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) keys: HashSet<Key>,
    /// Also capture every node created at this physical depth or shallower
    /// (0: none), for the visible history above captured nodes.
    pub(crate) all_up_to: u32,
    /// Simulations whose opponent decisions keep private state snapshots.
    pub(crate) snapshot_sims: HashSet<u64>,
    pub(crate) sim: u64,
    pending_created: Vec<Key>,
    pending: Vec<(Key, Captured)>,
    pub(crate) opp_since: Vec<(FastActorSessionV1, u64, u32)>,
    /// Committed node -> creating simulation index.
    pub(crate) created: HashMap<Key, u64>,
    pub(crate) committed: BTreeMap<Key, Captured>,
}

impl Capture {
    pub(crate) fn new(keys: HashSet<Key>) -> Self {
        Self {
            keys,
            ..Self::default()
        }
    }

    fn begin(&mut self, sim: u64) {
        self.sim = sim;
        self.pending_created.clear();
        self.pending.clear();
        self.opp_since.clear();
    }

    fn see(
        &mut self,
        key: Key,
        s: &FastActorSessionV1,
        d: FastActorDecisionV1,
        depth: u32,
        cn: &super::tree::Canon,
        path: &[(Key, usize)],
    ) -> Result<(), PlayErr> {
        self.pending_created.push(key);
        if !self.keys.contains(&key) && depth > self.all_up_to {
            return Ok(());
        }
        let (raw_obs, raw_menu) = raw_decision(s, d)?;
        let (again, mapping) = canon_values(raw_obs.clone(), raw_menu.clone())?;
        if again != *cn {
            return Err(PlayErr::Fault("capture: canonical view differs".into()));
        }
        self.pending.push((
            key,
            Captured {
                sim: self.sim,
                step: d.step,
                physical_decision_id: d.physical_decision_id,
                substep: d.substep_index,
                depth,
                raw_obs,
                raw_menu,
                mapping,
                canon_menu: cn.menu.clone(),
                path: path.to_vec(),
                opp_since: std::mem::take(&mut self.opp_since),
            },
        ));
        Ok(())
    }

    /// Commits this simulation's creations if its nodes were committed.
    fn end(&mut self, natural: bool) {
        if natural {
            for k in self.pending_created.drain(..) {
                self.created.insert(k, self.sim);
            }
            for (k, c) in self.pending.drain(..) {
                self.committed.insert(k, c);
            }
        }
        self.pending_created.clear();
        self.pending.clear();
        self.opp_since.clear();
    }
}

/// First natural winning backup of one stored edge, and what followed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EdgeWin {
    /// (natural-simulation ordinal, prior backups of the edge, prior visits
    /// of its node) at the first winning backup.
    pub(crate) first_win: Option<(u64, u64, u64)>,
    pub(crate) backups_after: u64,
    pub(crate) wins_after: u64,
}

/// Whether E searches memoize key steps (`S4A_KEY_MEMO=1`; default off).
/// Exact: selection, trees and key statistics are unchanged; only time is.
pub(crate) fn key_memo_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| match std::env::var("S4A_KEY_MEMO").as_deref() {
        Ok("1") => true,
        Ok("0") | Err(_) => false,
        Ok(v) => panic!("S4A_KEY_MEMO must be 0 or 1, got {v:?}"),
    })
}

/// Search diagnostics (never read by selection, backup or execution).
#[derive(Debug, Default)]
pub(crate) struct SearchDiag {
    pub(crate) keys: KeyStats,
    /// Key memo counters when `S4A_KEY_MEMO=1` (null otherwise).
    pub(crate) key_memo: Value,
    pub(crate) edges: HashMap<(Key, usize), EdgeWin>,
    /// Stored node -> legacy history identity -> natural backups per edge.
    pub(crate) legacy: HashMap<Key, BTreeMap<Key, Vec<u64>>>,
    legacy_path: Vec<Key>,
    /// Per-attempt path log (`pathlog.rs`, `S4A_PATH_LOG`); a log set by the
    /// caller is used as is.
    pub(crate) path_log: Option<super::pathlog::PathLog>,
}

impl SearchDiag {
    fn before_backup(
        &mut self,
        tree: &Tree,
        path: &[(Key, usize)],
        new: &[(Key, Node)],
        win: bool,
        ordinal: u64,
    ) {
        for (i, (k, a)) in path.iter().enumerate() {
            let (prior, visits, edges) = match tree.nodes.get(k) {
                Some(n) => (n.n[*a], n.visits, n.n.len()),
                None => {
                    let len = new
                        .iter()
                        .find(|(nk, _)| nk == k)
                        .map_or(0, |(_, n)| n.n.len());
                    (0, 0, len)
                }
            };
            let e = self.edges.entry((*k, *a)).or_default();
            if e.first_win.is_some() {
                e.backups_after += 1;
                e.wins_after += u64::from(win);
            } else if win {
                e.first_win = Some((ordinal, prior, visits));
            }
            if let Some(l) = self.legacy_path.get(i) {
                let v = self
                    .legacy
                    .entry(*k)
                    .or_default()
                    .entry(*l)
                    .or_insert_with(|| vec![0; edges]);
                if let Some(x) = v.get_mut(*a) {
                    *x += 1;
                }
            }
        }
    }
}

/// Contiguous supported prefix of one selection trajectory (diagnostic): a
/// choice is supported when it executes a tree edge with at least eight prior
/// natural terminal backups; a physical group when all its visited nonforced
/// focal substeps are.
struct Sup {
    open_sub: bool,
    open_group: bool,
    cur: Option<u64>,
    cur_ok: bool,
    groups: u32,
    subs: u32,
    first_unsupported_depth: u32,
}

impl Sup {
    fn new() -> Self {
        Self {
            open_sub: true,
            open_group: true,
            cur: None,
            cur_ok: true,
            groups: 0,
            subs: 0,
            first_unsupported_depth: 0,
        }
    }

    fn close(&mut self) {
        if self.cur.is_some() && self.open_group {
            if self.cur_ok {
                self.groups += 1;
            } else {
                self.open_group = false;
            }
        }
    }

    fn see(&mut self, phys: u64, depth: u32, ok: bool) {
        if self.cur != Some(phys) {
            self.close();
            self.cur = Some(phys);
            self.cur_ok = true;
        }
        self.cur_ok &= ok;
        if self.open_sub {
            if ok {
                self.subs += 1;
            } else {
                self.open_sub = false;
                self.first_unsupported_depth = depth;
            }
        }
    }

    fn finish(mut self, rec: &mut SimRecord) {
        self.close();
        rec.supported_groups = self.groups;
        rec.supported_substeps = self.subs;
        rec.first_unsupported_depth = self.first_unsupported_depth;
    }
}

/// Selection work at one point of a simulation, relative to its start:
/// physical focal depth, transitions and inference calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snap {
    pub(crate) depth: u32,
    pub(crate) transitions: u64,
    pub(crate) calls: u64,
}

/// Per-simulation selection accounting (diagnostic only; never read by
/// selection, backup or execution).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SimRecord {
    /// 1 win, 2 natural nonwin, 3 nonnatural, 4 truncated, 5 fault,
    /// 6 sampler rejection (no simulation ran).
    pub(crate) outcome: u8,
    /// Selection transitions and policy calls in this simulation, measured
    /// after it returned (every ending).
    pub(crate) transitions: u64,
    pub(crate) policy_calls: u64,
    /// The first expansion, recorded before its chosen action is applied
    /// (present even if the attempt then commits nothing).
    pub(crate) expansion: Option<Snap>,
    /// The first focal choice past the depth limit while still in the tree
    /// (only without an expansion).
    pub(crate) depth_limit: Option<Snap>,
    /// Existing tree nodes traversed before the first expansion.
    pub(crate) existing: u32,
    /// Of those, nodes first created as retained tail nodes.
    pub(crate) staged_hits: u32,
    /// Nodes committed by this simulation (natural endings only).
    pub(crate) new_nodes: usize,
    /// Contiguous supported prefix (natural and nonnatural endings): leading
    /// physical groups and leading nonforced substeps, and the physical depth
    /// of the first unsupported choice (0: none).
    pub(crate) supported_groups: u32,
    pub(crate) supported_substeps: u32,
    pub(crate) first_unsupported_depth: u32,
    /// Focal nonforced choices the rollout policy made (after the tree path,
    /// past the depth limit, or on retained tail nodes).
    pub(crate) rollout_choices: u32,
}

impl SimRecord {
    pub(crate) const REJECTED: u8 = 6;

    /// Mutually exclusive (transitions, calls) buckets: the tree prefix
    /// (before the first expansion, or before the first choice past the
    /// depth limit, or the whole simulation when neither occurs), work after
    /// the first expansion, and the depth-limit tail without an expansion.
    pub(crate) fn buckets(&self) -> [(u64, u64); 3] {
        let total = (self.transitions, self.policy_calls);
        let rest = |s: Snap| {
            (
                total.0.saturating_sub(s.transitions),
                total.1.saturating_sub(s.calls),
            )
        };
        match (self.expansion, self.depth_limit) {
            (Some(e), _) => [(e.transitions, e.calls), rest(e), (0, 0)],
            (None, Some(l)) => [(l.transitions, l.calls), (0, 0), rest(l)],
            (None, None) => [total, (0, 0), (0, 0)],
        }
    }

    /// Every recorded snapshot lies within the simulation's totals.
    pub(crate) fn snapshots_within_totals(&self) -> bool {
        [self.expansion, self.depth_limit]
            .iter()
            .flatten()
            .all(|s| s.transitions <= self.transitions && s.calls <= self.policy_calls)
    }
}

/// Trace only: the lookup key, the matched node with its frozen choice, and
/// the chosen edge index of one frozen-execution decision.
type Seen<'a> = (Key, Option<(&'a Node, Option<usize>)>, usize);

/// E evaluation-time tree statistics.
#[derive(Clone, Debug, Default)]
pub(crate) struct EStats {
    pub(crate) root_qualified: bool,
    pub(crate) nonroot_lookups: u64,
    pub(crate) nonroot_hits: u64,
    pub(crate) fallbacks: u64,
    pub(crate) max_hit_depth: u32,
    /// (depth, supporting backups) of each executed tree edge.
    pub(crate) executed: Vec<(u32, u64)>,
    /// Every frozen-execution choice (two or more legal actions) of the
    /// focal player: (physical decision, substep, source), source 0 tree
    /// edge, 1 matched node without a supported action, 2 first history
    /// miss, 3 after a miss, 4 past the depth limit. Not serialized.
    pub(crate) decisions: Vec<(u64, u32, u8)>,
}

impl EStats {
    pub(crate) fn json(&self) -> Value {
        json!({"root_qualified":self.root_qualified,"nonroot_lookups":self.nonroot_lookups,
            "nonroot_hits":self.nonroot_hits,"fallbacks":self.fallbacks,"max_hit_depth":self.max_hit_depth,
            "executed":self.executed})
    }
}

pub(crate) fn tree_stats(tree: &Tree) -> Value {
    let mut by_depth: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
    let mut root = Value::Null;
    for n in tree.nodes.values() {
        let e = by_depth.entry(n.depth).or_default();
        e.0 += 1;
        e.1 += n.visits;
        if n.parent == [0u8; 32] {
            root = json!({"visits":n.visits,"n":n.n,"wins":n.wins,"menu_size":n.n.len(),
                "qualified_edges":n.n.iter().filter(|&&x| x >= super::tree::MIN_EXEC_BACKUPS).count(),
                "frozen_choice":n.frozen_choice()});
        }
    }
    json!({"nodes":tree.nodes.len(),"hash":tree.hash(),"root":root,
        "by_depth":by_depth.iter().map(|(d,(n,v))| json!({"depth":d,"nodes":n,"visits":v})).collect::<Vec<_>>()})
}
