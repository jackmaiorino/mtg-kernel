//! Learned search scheduler (collab LANES/discovery-efficiency-plan-20261012,
//! RUNNER.md sections 5-7, track A), mode `s4a-allocator`.
//!
//! Every arm-game plays from an ordinary opening under a per-game ceiling of
//! 512,000 raw selection transitions, with the `s4a-budget` cursor, support
//! rule, keys and plain fallback unchanged (`refresh::guide`). Arms:
//! - `current`: exactly the budget executor (128,000 at the first focal
//!   nonforced decision, then 64,000 support-triggered refreshes).
//! - `random`: at each scheduler decision, sample the nominal actions with
//!   the base probabilities `Q`.
//! - `learned`: a linear-softmax controller `pi` from a weights file; the
//!   behavior is `mu = (1 - EPSILON) * pi + EPSILON * Q`.
//!
//! A scheduler decision happens only at a focal choice with two or more
//! legal actions whose retained tree has no supported action, while
//! allowance remains. Supported continuations execute without a decision.
//! A positive choice runs exactly one search of that amount, then the
//! support/fallback rule applies at that substep; the controller is never
//! consulted twice at one substep. Zero runs no search (retained knowledge
//! stays usable at later decisions).
//!
//! Positive nominal amounts are clipped to the remaining allowance and equal
//! executable amounts are coalesced by summing their probabilities; the
//! behavior gradient is that of the log of the coalesced `mu`.

use super::arms::softmax;
use super::bank::{bank_roles, source_session, BankShared, SourceSpec};
use super::canon2::KeyMode;
use super::refresh::{guide, Allocation, GuideRun, Lookup, ANCHOR_BUDGET, REFRESH_BUDGET};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Per-game selection-transition ceiling of every arm.
pub(super) const CAP: u64 = 512_000;
/// Nominal actions; `u64::MAX` stands for "remaining".
pub(super) const NOMINAL: [u64; NA] = [0, 16_000, 64_000, 256_000, u64::MAX];
pub(super) const ACTION_LABELS: [&str; NA] = ["0", "16000", "64000", "256000", "remaining"];
/// Base (random-control) probabilities over the nominal actions.
pub(super) const Q: [f64; NA] = [0.95, 0.0125, 0.0125, 0.0125, 0.0125];
/// Weight of `Q` in the training and held-out behavior of the learned arm.
pub(super) const EPSILON: f64 = 0.2;
pub(super) const NA: usize = 5;
pub(super) const NF: usize = 21;
pub(super) const WEIGHTS_SCHEMA: &str = "discovery-efficiency-v1/allocator-weights";
pub(super) const FEATURE_SCHEMA: &str = "discovery-efficiency-v1/allocator-features-v1";
/// Feature order, pinned. Players are ordered focal (self) then opponent.
pub(super) const FEATURES: [&str; NF] = [
    "intercept",
    "remaining_over_cap",
    "focal_nonforced_substeps_clip256",
    "substeps_since_search_clip64",
    "searches_clip32",
    "lookup_miss",
    "lookup_unsupported",
    "log2_options_plus1_clip8",
    "policy_entropy_over_log_options",
    "policy_top2_gap",
    "self_life_clamp_m20_80",
    "self_battlefield_clip64",
    "self_graveyard_clip64",
    "self_hand_clip32",
    "self_library_clip128",
    "opp_life_clamp_m20_80",
    "opp_battlefield_clip64",
    "opp_graveyard_clip64",
    "opp_hand_clip32",
    "opp_library_clip128",
    "stack_clip64",
];

/// Executable amounts of the nominal actions at `remaining` (> 0).
pub(super) fn executable(remaining: u64) -> [u64; NA] {
    NOMINAL.map(|n| n.min(remaining))
}

/// One executable choice: an amount and the nominal actions it merges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Choice {
    pub(super) amount: u64,
    pub(super) members: Vec<usize>,
}

/// Distinct executable amounts at `remaining` (> 0), ascending.
pub(super) fn choices(remaining: u64) -> Vec<Choice> {
    let ex = executable(remaining);
    let mut amounts: Vec<u64> = ex.to_vec();
    amounts.sort_unstable();
    amounts.dedup();
    amounts
        .into_iter()
        .map(|amount| Choice {
            amount,
            members: (0..NA).filter(|&a| ex[a] == amount).collect(),
        })
        .collect()
}

/// Coalesced probabilities of `choices` under nominal probabilities `p`.
pub(super) fn coalesce(p: &[f64; NA], choices: &[Choice]) -> Vec<f64> {
    choices
        .iter()
        .map(|c| c.members.iter().map(|&a| p[a]).sum())
        .collect()
}

/// The behavior mixture over nominal actions.
pub(super) fn mixture(pi: &[f64; NA]) -> [f64; NA] {
    std::array::from_fn(|a| (1.0 - EPSILON) * pi[a] + EPSILON * Q[a])
}

/// Linear-softmax controller: `logits = W x`, feature 0 the intercept.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Controller {
    pub(super) w: [[f64; NF]; NA],
}

impl Controller {
    /// Zero weights with intercept log(Q), so that pi = Q.
    pub(super) fn init() -> Self {
        let mut w = [[0.0; NF]; NA];
        for (row, q) in w.iter_mut().zip(Q) {
            row[0] = q.ln();
        }
        Self { w }
    }

    pub(super) fn pi(&self, x: &[f64; NF]) -> [f64; NA] {
        let z: [f64; NA] =
            std::array::from_fn(|a| self.w[a].iter().zip(x).map(|(w, v)| w * v).sum());
        let m = z.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let e = z.map(|v| (v - m).exp());
        let s: f64 = e.iter().sum();
        e.map(|v| v / s)
    }

    /// log mu of the coalesced choice `amount` at `remaining`.
    pub(super) fn log_mu(&self, x: &[f64; NF], remaining: u64, amount: u64) -> f64 {
        let cs = choices(remaining);
        let mu = coalesce(&mixture(&self.pi(x)), &cs);
        let i = cs
            .iter()
            .position(|c| c.amount == amount)
            .expect("an executable amount");
        mu[i].ln()
    }

    /// Gradient of `log_mu` with respect to the weights. With `E` the merged
    /// nominal actions, d log mu(E) / d z_j = (1 - eps) pi_j ([j in E] - pi(E)) / mu(E).
    pub(super) fn grad_log_mu(
        &self,
        x: &[f64; NF],
        remaining: u64,
        amount: u64,
    ) -> [[f64; NF]; NA] {
        let pi = self.pi(x);
        let cs = choices(remaining);
        let c = cs
            .iter()
            .find(|c| c.amount == amount)
            .expect("an executable amount");
        let pi_e: f64 = c.members.iter().map(|&a| pi[a]).sum();
        let mu_e: f64 = c.members.iter().map(|&a| mixture(&pi)[a]).sum();
        let mut g = [[0.0; NF]; NA];
        for (j, (row, p)) in g.iter_mut().zip(pi).enumerate() {
            let inside = if c.members.contains(&j) { 1.0 } else { 0.0 };
            let dz = (1.0 - EPSILON) * p * (inside - pi_e) / mu_e;
            for (gk, xk) in row.iter_mut().zip(x) {
                *gk = dz * xk;
            }
        }
        g
    }

    /// Reads a weights file (`allocator.py` writes it), checking the schema,
    /// feature order and action set.
    pub(super) fn from_json(bytes: &[u8]) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            schema: String,
            feature_schema: String,
            features: Vec<String>,
            actions: Vec<String>,
            q: Vec<f64>,
            epsilon: f64,
            weights: Vec<Vec<f64>>,
        }
        let f: File = serde_json::from_slice(bytes).map_err(|e| format!("weights: {e}"))?;
        if f.schema != WEIGHTS_SCHEMA || f.feature_schema != FEATURE_SCHEMA {
            return Err(format!(
                "weights schema {} / {}",
                f.schema, f.feature_schema
            ));
        }
        if f.features != FEATURES || f.actions != ACTION_LABELS || f.q != Q || f.epsilon != EPSILON
        {
            return Err(
                "weights features, actions, q or epsilon differ from the pinned schema".into(),
            );
        }
        if f.weights.len() != NA || f.weights.iter().any(|r| r.len() != NF) {
            return Err("weights must be 5 rows of 21".into());
        }
        if f.weights.iter().flatten().any(|v| !v.is_finite()) {
            return Err("weights must be finite".into());
        }
        let mut w = [[0.0; NF]; NA];
        for (dst, row) in w.iter_mut().zip(&f.weights) {
            dst.copy_from_slice(row);
        }
        Ok(Self { w })
    }
}

/// The rule at one focal nonforced decision of a scheduler arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SchedStep {
    Execute(usize),
    Consult,
    Plain,
}

/// Supported actions execute; after a search at this substep (or a zero
/// choice) the fallback plays; with no allowance nothing is sampled.
pub(super) fn sched_rule(lookup: Lookup, remaining: u64, decided_here: bool) -> SchedStep {
    match lookup {
        Lookup::Supported(e) => SchedStep::Execute(e),
        _ if decided_here || remaining == 0 => SchedStep::Plain,
        _ => SchedStep::Consult,
    }
}

/// Public counts of the ordinary observation, focal player first.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Public {
    pub(super) life: [i32; 2],
    pub(super) battlefield: [usize; 2],
    pub(super) graveyard: [usize; 2],
    pub(super) hand: [usize; 2],
    pub(super) library: [usize; 2],
    pub(super) stack: usize,
}

impl Public {
    pub(super) fn of(s: &FastActorSessionV1, focal: PlayerId) -> Self {
        let st = s.game_state();
        let order = [focal.0 as usize, 1 - focal.0 as usize];
        let p = order.map(|i| &st.players[i]);
        Self {
            life: p.map(|x| x.life),
            battlefield: p.map(|x| x.battlefield.len()),
            graveyard: p.map(|x| x.graveyard.len()),
            hand: p.map(|x| x.hand.len()),
            library: p.map(|x| x.library.len()),
            stack: st.stack.len(),
        }
    }
}

/// Game-level counters at a scheduler decision.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Counts {
    pub(super) remaining: u64,
    pub(super) cap: u64,
    /// Focal nonforced substeps before this one.
    pub(super) nonforced: u64,
    /// Focal nonforced substeps since the last search's substep; None before any.
    pub(super) since_search: Option<u64>,
    pub(super) searches: u64,
}

fn clip(v: f64, hi: f64) -> f64 {
    v.min(hi) / hi
}

/// The pinned feature row (`FEATURES`).
pub(super) fn features(c: Counts, lookup: Lookup, probs: &[f64], public: Public) -> [f64; NF] {
    let k = probs.len() as f64;
    let entropy: f64 = -probs
        .iter()
        .filter(|&&p| p > 0.0)
        .map(|p| p * p.ln())
        .sum::<f64>();
    let mut sorted = probs.to_vec();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let gap = sorted[0] - sorted.get(1).copied().unwrap_or(0.0);
    let (miss, unsupported) = match lookup {
        Lookup::Unsupported => (0.0, 1.0),
        // A depth limit is a history beyond the retained tree: a miss.
        Lookup::Miss | Lookup::DepthLimit => (1.0, 0.0),
        Lookup::Supported(_) => (0.0, 0.0),
    };
    let mut x = [0.0; NF];
    x[0] = 1.0;
    x[1] = c.remaining as f64 / c.cap as f64;
    x[2] = clip(c.nonforced as f64, 256.0);
    x[3] = clip(c.since_search.unwrap_or(64) as f64, 64.0);
    x[4] = clip(c.searches as f64, 32.0);
    x[5] = miss;
    x[6] = unsupported;
    x[7] = (k + 1.0).log2().min(8.0) / 8.0;
    x[8] = entropy / k.ln();
    x[9] = gap;
    for (i, base) in [(0usize, 10usize), (1, 15)] {
        x[base] = f64::from(public.life[i].clamp(-20, 80)) / 80.0;
        x[base + 1] = clip(public.battlefield[i] as f64, 64.0);
        x[base + 2] = clip(public.graveyard[i] as f64, 64.0);
        x[base + 3] = clip(public.hand[i] as f64, 32.0);
        x[base + 4] = clip(public.library[i] as f64, 128.0);
    }
    x[20] = clip(public.stack as f64, 64.0);
    x
}

/// Scheduler arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Arm {
    Current,
    Random,
    Learned,
}

impl Arm {
    pub(super) fn parse(s: &str) -> Result<Self, String> {
        match s {
            "current" => Ok(Self::Current),
            "random" => Ok(Self::Random),
            "learned" => Ok(Self::Learned),
            other => Err(format!("unknown allocator arm {other}")),
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Random => "random",
            Self::Learned => "learned",
        }
    }
}

/// Uniform in [0, 1) from 53 random bits.
fn uniform(rng: &mut SplitMix64) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// The first choice whose cumulative probability exceeds `u` (the last on
/// rounding shortfall).
pub(super) fn pick(probs: &[f64], u: f64) -> usize {
    let mut acc = 0.0;
    for (i, p) in probs.iter().enumerate() {
        acc += p;
        if u < acc {
            return i;
        }
    }
    probs.len() - 1
}

/// The live scheduler of one random or learned arm-game.
pub(super) struct Scheduler {
    arm: Arm,
    controller: Controller,
    rng: SplitMix64,
    scorer: FrozenPlayPolicyV1,
    pub(super) cap: u64,
    pub(super) seconds: f64,
    pub(super) decisions: u64,
    pub(super) requested: u64,
}

impl Scheduler {
    pub(super) fn new(
        arm: Arm,
        controller: Controller,
        controller_seed: u64,
        policy: &FrozenPlayPolicyV1,
        cap: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            arm,
            controller,
            rng: SplitMix64::seed(controller_seed),
            scorer: policy.fork_for_collection_v3()?,
            cap,
            seconds: 0.0,
            decisions: 0,
            requested: 0,
        })
    }

    /// One scheduler decision: the amount to search now (0 = none) and its
    /// record (feature row, executable choices, pi and mu, draw, choice).
    pub(super) fn consult(
        &mut self,
        s: &FastActorSessionV1,
        focal: PlayerId,
        lookup: Lookup,
        c: Counts,
    ) -> Result<(u64, Value), String> {
        let started = Instant::now();
        debug_assert!(c.remaining > 0);
        let probs = softmax(&self.scorer.score_fast_session_v1(s)?.logits);
        let x = features(c, lookup, &probs, Public::of(s, focal));
        let (pi, mu) = match self.arm {
            Arm::Random => (Q, Q),
            Arm::Learned => {
                let pi = self.controller.pi(&x);
                (pi, mixture(&pi))
            }
            Arm::Current => return Err("the current arm has no scheduler".into()),
        };
        let cs = choices(c.remaining);
        let (pi_c, mu_c) = (coalesce(&pi, &cs), coalesce(&mu, &cs));
        let u = uniform(&mut self.rng);
        let i = pick(&mu_c, u);
        let amount = cs[i].amount;
        self.decisions += 1;
        self.requested += amount;
        self.seconds += started.elapsed().as_secs_f64();
        Ok((
            amount,
            json!({"index":self.decisions - 1,"x":x.to_vec(),"lookup":match lookup {
                    Lookup::Unsupported => "unsupported", Lookup::DepthLimit => "depth_limit", _ => "miss"},
                "remaining":c.remaining,"nonforced":c.nonforced,"since_search":c.since_search,
                "searches":c.searches,"k":probs.len(),
                "amounts":cs.iter().map(|c| c.amount).collect::<Vec<_>>(),
                "members":cs.iter().map(|c| c.members.clone()).collect::<Vec<_>>(),
                "pi":pi_c,"mu":mu_c,"u":u,"chosen":i,"amount":amount}),
        ))
    }
}

/// One allocator case line (`ALLOC_CASES`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AllocCase {
    pub(super) id: String,
    pub(super) arm: String,
    pub(super) controller_seed: u64,
    pub(super) case: SourceSpec,
}

/// Loaded weights: the controller and the SHA-256 of the file bytes.
pub(super) struct Weights {
    pub(super) controller: Controller,
    pub(super) sha256: String,
    pub(super) path: String,
}

impl Weights {
    pub(super) fn load(path: &str) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(Self {
            controller: Controller::from_json(&bytes)?,
            sha256: Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            path: path.to_owned(),
        })
    }
}

/// One arm-game from the case's ordinary opening.
pub(super) fn allocator_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    case: &AllocCase,
    weights: Option<&Weights>,
    total: u64,
    mode: KeyMode,
    binding: &str,
) -> Result<Value, String> {
    let started = Instant::now();
    let src = &case.case;
    let arm = Arm::parse(&case.arm)?;
    let opp = sh
        .labels
        .iter()
        .position(|l| *l == src.opp)
        .ok_or("unknown guide opponent")?;
    let mut roles = bank_roles(policy, &sh.opponents)?;
    let mut live_focal = policy.fork_for_collection_v3()?;
    let mut live_opp = sh.opponents[opp].fork_for_collection_v3()?;
    let opening = source_session(&sh.spec, src)?;
    let run = GuideRun {
        opening: true,
        mode,
    };
    let (allocation, mut scheduler, weights_sha) = match arm {
        Arm::Current => (
            Allocation {
                anchor: ANCHOR_BUDGET,
                refresh: REFRESH_BUDGET,
                total,
            },
            None,
            None,
        ),
        Arm::Random | Arm::Learned => {
            let (controller, sha) = match (arm, weights) {
                (Arm::Learned, Some(w)) => (w.controller.clone(), Some(w.sha256.clone())),
                (Arm::Learned, None) => return Err("the learned arm needs ALLOC_WEIGHTS".into()),
                _ => (Controller::init(), None),
            };
            (
                Allocation {
                    anchor: 0,
                    refresh: 0,
                    total,
                },
                Some(Scheduler::new(
                    arm,
                    controller,
                    case.controller_seed,
                    policy,
                    total,
                )?),
                sha,
            )
        }
    };
    let (rec, _) = guide(
        sh,
        &mut roles,
        &mut live_focal,
        &mut live_opp,
        &opening,
        src,
        opp,
        allocation,
        None,
        run,
        scheduler.as_mut(),
    )?;
    let sched = scheduler.as_ref().map(
        |s| json!({"decisions":s.decisions,"requested":s.requested,"controller_seconds":s.seconds}),
    );
    Ok(
        json!({"kind":"allocator_game","schema":"discovery-efficiency-v1/allocator-game","id":case.id,
        "arm":arm.label(),"lineage":sh.spec.lineage,"own":src.own,"other":src.other,"seat":src.seat,
        "opp":src.opp,"seed":src.seed,"starting_player":src.starting_player,
        "live_policy_seeds":src.guide_policy_seeds,"search_root":src.search_root,
        "controller_seed":case.controller_seed,"bank_binding_sha256":binding,
        "cap":total,"formal_cap":total == CAP,
        "allocation":{"initial":allocation.anchor,"refresh":allocation.refresh,"total":allocation.total},
        "controller":{"feature_schema":FEATURE_SCHEMA,"features":FEATURES,"actions":ACTION_LABELS,
            "q":Q,"epsilon":if arm == Arm::Learned { json!(EPSILON) } else { Value::Null },
            "weights_sha256":weights_sha,"weights_path":weights.filter(|_| arm == Arm::Learned).map(|w| w.path.clone())},
        "scheduler":sched,"game":rec,"wall":started.elapsed().as_secs_f64()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(seed: u64) -> [f64; NF] {
        let mut r = SplitMix64::seed(seed);
        let mut x: [f64; NF] = std::array::from_fn(|_| uniform(&mut r));
        x[0] = 1.0;
        x
    }

    fn controller(seed: u64) -> Controller {
        let mut r = SplitMix64::seed(seed);
        let mut c = Controller::init();
        for row in &mut c.w {
            for v in row.iter_mut() {
                *v += uniform(&mut r) - 0.5;
            }
        }
        c
    }

    #[test]
    fn positive_amounts_clip_to_remaining() {
        assert_eq!(executable(512_000), [0, 16_000, 64_000, 256_000, 512_000]);
        assert_eq!(executable(100_000), [0, 16_000, 64_000, 100_000, 100_000]);
        assert_eq!(executable(10_000), [0, 10_000, 10_000, 10_000, 10_000]);
        assert_eq!(executable(64_000), [0, 16_000, 64_000, 64_000, 64_000]);
    }

    #[test]
    fn equal_amounts_coalesce_with_summed_probabilities() {
        let cs = choices(100_000);
        assert_eq!(
            cs,
            [
                Choice {
                    amount: 0,
                    members: vec![0]
                },
                Choice {
                    amount: 16_000,
                    members: vec![1]
                },
                Choice {
                    amount: 64_000,
                    members: vec![2]
                },
                Choice {
                    amount: 100_000,
                    members: vec![3, 4]
                },
            ]
        );
        let p = coalesce(&Q, &cs);
        assert_eq!(p, [0.95, 0.0125, 0.0125, 0.025]);
        let cs = choices(5);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[1].members, [1, 2, 3, 4]);
        assert!((coalesce(&Q, &cs).iter().sum::<f64>() - 1.0).abs() < 1e-15);
        // Full allowance: five distinct choices.
        assert_eq!(choices(CAP).len(), 5);
    }

    #[test]
    fn init_controller_is_q_and_behavior_is_q() {
        let c = Controller::init();
        for seed in 0..8 {
            let pi = c.pi(&row(seed));
            let mu = mixture(&pi);
            for ((p, m), q) in pi.iter().zip(mu).zip(Q) {
                assert!((p - q).abs() < 1e-15, "pi {pi:?}");
                assert!((m - q).abs() < 1e-15, "mu {mu:?}");
            }
        }
    }

    #[test]
    fn behavior_is_the_epsilon_mixture() {
        let c = controller(3);
        let x = row(4);
        let pi = c.pi(&x);
        let mu = mixture(&pi);
        for ((m, p), q) in mu.iter().zip(pi).zip(Q) {
            assert!((m - (0.8 * p + 0.2 * q)).abs() < 1e-15);
        }
        assert!((mu.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Coalescing applies to mu exactly as to pi.
        let cs = choices(30_000);
        let (pc, mc) = (coalesce(&pi, &cs), coalesce(&mu, &cs));
        for ((choice, m), p) in cs.iter().zip(mc).zip(pc) {
            let q: f64 = choice.members.iter().map(|&a| Q[a]).sum();
            assert!((m - (0.8 * p + 0.2 * q)).abs() < 1e-15);
        }
    }

    /// The analytic gradient of log mu equals central finite differences,
    /// including coalesced choices.
    #[test]
    fn gradient_matches_finite_differences_through_coalescing() {
        for (seed, remaining) in [(1u64, CAP), (2, 100_000), (3, 10_000), (4, 64_000), (5, 1)] {
            let c = controller(seed);
            let x = row(seed + 100);
            for choice in choices(remaining) {
                let g = c.grad_log_mu(&x, remaining, choice.amount);
                for (j, gj) in g.iter().enumerate() {
                    for (k, &gjk) in gj.iter().enumerate() {
                        let h = 1e-6;
                        let (mut up, mut dn) = (c.clone(), c.clone());
                        up.w[j][k] += h;
                        dn.w[j][k] -= h;
                        let fd = (up.log_mu(&x, remaining, choice.amount)
                            - dn.log_mu(&x, remaining, choice.amount))
                            / (2.0 * h);
                        assert!(
                            (fd - gjk).abs() < 1e-7,
                            "remaining {remaining} amount {} w[{j}][{k}]: fd {fd} analytic {gjk}",
                            choice.amount,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn supported_actions_never_consult() {
        for remaining in [0, 1, CAP] {
            for decided in [false, true] {
                assert_eq!(
                    sched_rule(Lookup::Supported(2), remaining, decided),
                    SchedStep::Execute(2)
                );
            }
        }
    }

    #[test]
    fn no_second_consult_at_one_substep() {
        for l in [Lookup::Miss, Lookup::Unsupported, Lookup::DepthLimit] {
            assert_eq!(sched_rule(l, CAP, false), SchedStep::Consult);
            assert_eq!(sched_rule(l, CAP, true), SchedStep::Plain);
        }
    }

    #[test]
    fn zero_remaining_does_not_sample() {
        for l in [Lookup::Miss, Lookup::Unsupported, Lookup::DepthLimit] {
            assert_eq!(sched_rule(l, 0, false), SchedStep::Plain);
        }
    }

    #[test]
    fn pick_follows_cumulative_probabilities() {
        let p = [0.5, 0.25, 0.25];
        assert_eq!(pick(&p, 0.0), 0);
        assert_eq!(pick(&p, 0.4999), 0);
        assert_eq!(pick(&p, 0.5), 1);
        assert_eq!(pick(&p, 0.9999), 2);
        assert_eq!(
            pick(&[0.3, 0.3], 0.99),
            1,
            "rounding shortfall takes the last"
        );
    }

    #[test]
    fn features_follow_the_pinned_scaling() {
        let c = Counts {
            remaining: 256_000,
            cap: CAP,
            nonforced: 300,
            since_search: None,
            searches: 3,
        };
        let public = Public {
            life: [-25, 100],
            battlefield: [70, 3],
            graveyard: [1, 0],
            hand: [40, 7],
            library: [200, 50],
            stack: 2,
        };
        let x = features(c, Lookup::Unsupported, &[0.5, 0.5], public);
        assert_eq!(x[0], 1.0);
        assert_eq!(x[1], 0.5);
        assert_eq!(x[2], 1.0);
        assert_eq!(x[3], 1.0, "64 before any search");
        assert_eq!(x[4], 3.0 / 32.0);
        assert_eq!((x[5], x[6]), (0.0, 1.0));
        assert!((x[7] - 3f64.log2() / 8.0).abs() < 1e-15);
        assert!((x[8] - 1.0).abs() < 1e-15, "uniform entropy is maximal");
        assert_eq!(x[9], 0.0);
        assert_eq!(&x[10..15], &[-0.25, 1.0, 1.0 / 64.0, 1.0, 1.0]);
        assert_eq!(
            &x[15..20],
            &[1.0, 3.0 / 64.0, 0.0, 7.0 / 32.0, 50.0 / 128.0]
        );
        assert_eq!(x[20], 2.0 / 64.0);
        let x = features(
            Counts {
                since_search: Some(5),
                ..c
            },
            Lookup::Miss,
            &[0.7, 0.2, 0.1],
            public,
        );
        assert_eq!(x[3], 5.0 / 64.0);
        assert_eq!((x[5], x[6]), (1.0, 0.0));
        assert!((x[9] - 0.5).abs() < 1e-15);
        assert_eq!(x[7], 0.25, "log2(4) / 8");
        assert_eq!(FEATURES.len(), NF);
    }

    #[test]
    fn weights_file_round_trips_and_rejects_other_schemas() {
        let c = controller(9);
        let file = json!({"schema":WEIGHTS_SCHEMA,"feature_schema":FEATURE_SCHEMA,"features":FEATURES,
            "actions":ACTION_LABELS,"q":Q,"epsilon":EPSILON,"update":0,
            "weights":c.w.iter().map(|r| r.to_vec()).collect::<Vec<_>>()});
        let back = Controller::from_json(file.to_string().as_bytes()).unwrap();
        assert_eq!(back, c, "decimal weights parse exactly");
        let mut bad = file.clone();
        bad["features"][3] = json!("other");
        assert!(Controller::from_json(bad.to_string().as_bytes()).is_err());
        let mut bad = file;
        bad["epsilon"] = json!(0.1);
        assert!(Controller::from_json(bad.to_string().as_bytes()).is_err());
    }
}
