//! Candidate E and controls A and D: selection under the transition ceiling,
//! then independent evaluation on paired worlds (RUNNER.md sections 2-3).

use super::labels::{SpyDefs, Suffix};
use super::play::{
    act, acting, apply, decision, observe, plain_to_end, terminal, Counters, End, Meter, PlayErr,
};
use super::seeds::{Purpose, RootSeeds};
use super::tree::{canon, child_key, live_index, Found, Key, Node, Tree, MAX_DEPTH};
use super::world::{sample, DeckPrior};
use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde_json::{json, Value};
use std::collections::BTreeMap;
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
    let r = super::prof::timed(super::prof::SAMPLE, || sample(s, seed, ctx.prior));
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
            let mut w = super::prof::timed(super::prof::CLONE, || world.clone());
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
                let mut s = super::prof::timed(super::prof::CLONE, || world.clone());
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
    fn simulate(
        &mut self,
        ctx: &RootCtx,
        tree: &Tree,
        mut s: FastActorSessionV1,
        meter: &mut Meter,
        c: &mut Counters,
        suffix: &mut Suffix,
    ) -> Result<(Vec<(Key, usize)>, Option<(Key, Node)>, End), PlayErr> {
        let mut path: Vec<(Key, usize)> = Vec::new();
        let mut new: Option<(Key, Node)> = None;
        let mut parent: (Key, Vec<u8>) = ([0u8; 32], b"root".to_vec());
        let mut in_tree = true;
        let mut depth = 0u32;
        let mut last_phys: Option<u64> = None;
        loop {
            if let Some(end) = terminal(&s, ctx.focal) {
                return Ok((path, new, end));
            }
            let d = decision(&s).expect("decision");
            let a = if acting(&d) == ctx.focal {
                if d.legal_action_count >= 2 {
                    if last_phys != Some(d.physical_decision_id) {
                        depth += 1;
                        last_phys = Some(d.physical_decision_id);
                    }
                    let a = if in_tree && depth <= MAX_DEPTH {
                        let cn = super::prof::timed(super::prof::CANON, || canon(&s, d))?;
                        let key = child_key(&parent.0, &parent.1, &cn);
                        let found = super::prof::timed(super::prof::TREE, || tree.find(&key, &parent.0, &parent.1, &cn))?;
                        let (edge_index, edge) = match found {
                            Found::Hit(node) => {
                                let e = node.select();
                                (e, cn.edges[e].clone())
                            }
                            Found::Miss => {
                                let node = Node::new(
                                    &key,
                                    parent.0,
                                    parent.1.clone(),
                                    cn.clone(),
                                    depth,
                                    ctx.seeds,
                                );
                                let e = node.select();
                                new = Some((key, node));
                                in_tree = false;
                                (e, cn.edges[e].clone())
                            }
                        };
                        path.push((key, edge_index));
                        let live = live_index(&cn, &edge)?;
                        parent = (key, edge);
                        live
                    } else {
                        act(&mut self.focal, &s, &d, c)?
                    };
                    observe(Some(suffix), &s, a, ctx.focal, &ctx.defs);
                    a
                } else {
                    0
                }
            } else {
                act(&mut self.opps[ctx.opp], &s, &d, c)?
            };
            apply(&mut s, &d, a, meter, ctx.focal, Some(suffix), &ctx.defs)?;
        }
    }

    /// E selection: simulations until the transition ceiling; one new node
    /// per completed simulation; natural terminal backups only.
    pub(crate) fn select_e(&mut self, ctx: &RootCtx) -> (Selection, Tree) {
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
            let world = match sample_world(ctx, ctx.root, seed, &mut sel.sampler) {
                Ok(w) => w,
                Err(_) => {
                    consecutive += 1;
                    continue;
                }
            };
            consecutive = 0;
            self.reset_main(ctx, "E", index, "select");
            let mut suffix = Suffix::new(ctx.cast_root);
            match self.simulate(ctx, &tree, world, &mut meter, &mut c, &mut suffix) {
                Ok((path, new, End::Natural { win })) => {
                    sel.discovery
                        .add(&suffix, Some(End::Natural { win }), index);
                    super::prof::timed(super::prof::TREE, || tree.backup(&path, new, win));
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
        sel.transitions = meter.spent;
        sel.inference = c.inference;
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
        let mut s = super::prof::timed(super::prof::CLONE, || world.clone());
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
            );
            let mut out = finish(r, suffix, &meter, &c);
            out.fallback_root = !stats.root_qualified;
            out.e = stats.json();
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
    #[allow(clippy::too_many_arguments, reason = "explicit roles and accounting")]
    fn frozen(
        &mut self,
        ctx: &RootCtx,
        tree: &Tree,
        s: &mut FastActorSessionV1,
        meter: &mut Meter,
        c: &mut Counters,
        suffix: &mut Suffix,
        stats: &mut EStats,
    ) -> Result<End, PlayErr> {
        let mut parent: (Key, Vec<u8>) = ([0u8; 32], b"root".to_vec());
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
                    let a = if matching && depth <= MAX_DEPTH {
                        let cn = super::prof::timed(super::prof::CANON, || canon(s, d))?;
                        let key = child_key(&parent.0, &parent.1, &cn);
                        if !root {
                            stats.nonroot_lookups += 1;
                        }
                        let live = match tree.find(&key, &parent.0, &parent.1, &cn)? {
                            Found::Hit(node) => {
                                if !root {
                                    stats.nonroot_hits += 1;
                                }
                                stats.max_hit_depth = stats.max_hit_depth.max(depth);
                                match node.frozen_choice() {
                                    Some(e) => {
                                        if root {
                                            stats.root_qualified = true;
                                        }
                                        stats.executed.push((depth, node.n[e]));
                                        live_index(&cn, &cn.edges[e])?
                                    }
                                    None => {
                                        stats.fallbacks += 1;
                                        act(&mut self.focal, s, &d, c)?
                                    }
                                }
                            }
                            Found::Miss => {
                                matching = false;
                                act(&mut self.focal, s, &d, c)?
                            }
                        };
                        parent = (key, cn.menu[live as usize].clone());
                        live
                    } else {
                        act(&mut self.focal, s, &d, c)?
                    };
                    observe(Some(suffix), s, a, ctx.focal, &ctx.defs);
                    a
                } else {
                    0
                }
            } else {
                act(&mut self.opps[ctx.opp], s, &d, c)?
            };
            apply(s, &d, a, meter, ctx.focal, Some(suffix), &ctx.defs)?;
        }
    }
}

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
}

impl EStats {
    fn json(&self) -> Value {
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
