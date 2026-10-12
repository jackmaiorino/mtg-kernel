//! Allocator v2 (collab LANES/allocator-redesign-20261012, DESIGN.md),
//! modes `s4a-allocator-v2` and `s4a-allocator-v2-probe`.
//!
//! Every arm-game plays Spy from an ordinary opening under a per-game
//! ceiling (default 2,048,000 raw selection transitions), with the
//! `s4a-budget` cursor, support rule, keys and plain fallback unchanged
//! (`refresh::guide`). Arms:
//! - `current`: the budget executor (128,000 at the first focal nonforced
//!   decision, then 64,000 support-triggered refreshes).
//! - `q`: the dynamic random base `q` at every consultation.
//! - `S`, `H`: a per-parent actor-critic controller from a weights file;
//!   `pi = softmax(log q + actor(x))`, behavior `mu = 0.8 pi + 0.2 q`.
//!   `S` reads the shared context features, `H` the same plus the frozen
//!   player's detached state and pooled legal-action scorer representations.
//!
//! Consultation is v1's: a supported edge executes without a consultation
//! (even at zero allowance); otherwise, with allowance left, the scheduler is
//! consulted once per substep. Zero keeps the retained cursor; a positive
//! amount runs one fresh search that replaces the tree.
//!
//! Nominal amounts [0, 32000, 128000, 512000] clip to the remaining
//! allowance and equal executable amounts coalesce by summing probabilities
//! (q, pi and mu alike). With remaining fraction r, the positive probability
//! is p = 0.5 r, split 0.5/0.4/0.1 over the three positive amounts.

use super::allocator::{pick, uniform, Choice, Counts, Public};
use super::arms::softmax;
use super::bank::{bank_roles, source_session, BankShared, SourceSpec};
use super::canon2::KeyMode;
use super::refresh::{
    guide, Allocation, Consult, ConsultCtx, GuideContext, GuideRun, Lookup, ProbeSpec,
    ANCHOR_BUDGET, REFRESH_BUDGET,
};
use crate::native_policy_value_net_v1::{PooledRepresentationV1, POOLING_SCHEMA_V1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Instant;

/// Development per-game ceiling.
pub(super) const CAP: u64 = 2_048_000;
pub(super) const NA: usize = 4;
pub(super) const NOMINAL: [u64; NA] = [0, 32_000, 128_000, 512_000];
pub(super) const ACTION_LABELS: [&str; NA] = ["0", "32000", "128000", "512000"];
/// Positive-search probability at a full allowance (p = this * r).
pub(super) const POSITIVE_AT_FULL: f64 = 0.5;
/// Split of the positive probability over 32k / 128k / 512k.
pub(super) const POSITIVE_SPLIT: [f64; 3] = [0.5, 0.4, 0.1];
/// Weight of q in the behavior mixture.
pub(super) const EPSILON: f64 = 0.2;
pub(super) const HIDDEN: usize = 64;
/// Width of each frozen-player representation block.
pub(super) const REP: usize = 64;
pub(super) const WEIGHTS_SCHEMA: &str = "allocator-redesign-v1/controller";
pub(super) const FEATURE_SCHEMA: &str = "allocator-redesign-v1/features-s1";
pub(super) const GAME_SCHEMA: &str = "allocator-redesign-v1/game";
/// Fixed scaling reference for absolute budgets.
const BUDGET_SCALE: f64 = 2_048_000.0;
pub(super) const NS: usize = 51;
/// Shared context features (arm S, and the first block of H), pinned.
/// Players are ordered focal then opponent. `node_*` describe the matched
/// node at a matched-unsupported lookup, `tree_*` the retained tree; both
/// are zero with their missingness flag set otherwise.
pub(super) const S_FEATURES: [&str; NS] = [
    "cap_over_2048000",
    "remaining_over_2048000",
    "remaining_fraction",
    "used_over_2048000",
    "log_focal_physical_256",
    "log_focal_substeps_512",
    "substep_index_clip8",
    "log_natural_simulations_65536",
    "searches_clip32",
    "never_searched",
    "substeps_since_search_clip64",
    "physical_since_search_clip32",
    "lookup_no_tree",
    "lookup_history_miss",
    "lookup_matched_unsupported",
    "lookup_depth_limit",
    "log2_options_plus1_clip8",
    "policy_entropy_over_log_options",
    "policy_top1",
    "policy_top2_gap",
    "policy_value_tanh",
    "turn_clip32",
    "focal_is_active_player",
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
    "tree_missing",
    "log_tree_nodes_65536",
    "log_tree_root_visits_65536",
    "tree_root_supported_edges_clip8",
    "tree_root_best_mean",
    "log_tree_search_natural_65536",
    "node_missing",
    "node_edges_over_options",
    "node_tried_fraction",
    "log_node_visits_4096",
    "node_max_backups_over_8",
    "node_supported_edges_clip8",
    "node_best_mean",
    "node_mean_gap",
    "node_mean_gap_missing",
    "node_best_is_policy_argmax",
    "node_best_missing",
];

/// Feature names of an arm's controller input.
pub(super) fn feature_names(arm: Arm) -> Vec<String> {
    let mut v: Vec<String> = S_FEATURES.iter().map(|s| (*s).to_owned()).collect();
    if arm == Arm::H {
        for block in ["rep_state", "rep_action_mean", "rep_action_max"] {
            v.extend((0..REP).map(|i| format!("{block}_{i:02}")));
        }
    }
    v
}

/// The dynamic random base over nominal actions at `remaining` of `cap`.
pub(super) fn base_q(remaining: u64, cap: u64) -> [f64; NA] {
    let r = (remaining as f64 / cap.max(1) as f64).clamp(0.0, 1.0);
    let p = POSITIVE_AT_FULL * r;
    [
        1.0 - p,
        POSITIVE_SPLIT[0] * p,
        POSITIVE_SPLIT[1] * p,
        POSITIVE_SPLIT[2] * p,
    ]
}

/// Distinct executable amounts at `remaining` (> 0), ascending, with the
/// nominal actions each merges.
pub(super) fn choices(remaining: u64) -> Vec<Choice> {
    let ex = NOMINAL.map(|n| n.min(remaining));
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

pub(super) fn coalesce(p: &[f64; NA], choices: &[Choice]) -> Vec<f64> {
    choices
        .iter()
        .map(|c| c.members.iter().map(|&a| p[a]).sum())
        .collect()
}

/// `mu = (1 - EPSILON) pi + EPSILON q` on coalesced choices.
pub(super) fn mixture(pi_c: &[f64], q_c: &[f64]) -> Vec<f64> {
    pi_c.iter()
        .zip(q_c)
        .map(|(p, q)| (1.0 - EPSILON) * p + EPSILON * q)
        .collect()
}

/// `softmax(log q + actor)` over the nominal actions.
pub(super) fn policy(q: &[f64; NA], actor: &[f64; NA]) -> [f64; NA] {
    let z: [f64; NA] = std::array::from_fn(|a| q[a].ln() + actor[a]);
    let m = z.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let e = z.map(|v| (v - m).exp());
    let s: f64 = e.iter().sum();
    e.map(|v| v / s)
}

/// Allocator v2 arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Arm {
    Current,
    Q,
    S,
    H,
}

impl Arm {
    pub(super) fn parse(s: &str) -> Result<Self, String> {
        match s {
            "current" => Ok(Self::Current),
            "q" => Ok(Self::Q),
            "S" => Ok(Self::S),
            "H" => Ok(Self::H),
            other => Err(format!("unknown allocator v2 arm {other}")),
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Q => "q",
            Self::S => "S",
            Self::H => "H",
        }
    }

    fn learned(self) -> bool {
        matches!(self, Self::S | Self::H)
    }
}

/// The frozen parent a controller is bound to.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ParentBinding {
    pub(super) lineage: String,
    pub(super) weights_sha256: String,
    pub(super) feature_contract_digest: String,
    pub(super) feature_encoding_digest: String,
}

impl ParentBinding {
    pub(super) fn of(policy: &FrozenPlayPolicyV1, lineage: &str) -> Self {
        let id = policy.actual_model_identity_v1();
        Self {
            lineage: lineage.to_owned(),
            weights_sha256: id.weights_sha256,
            feature_contract_digest: id.feature_contract_digest,
            feature_encoding_digest: id.feature_encoding_digest,
        }
    }

    fn json(&self) -> Value {
        json!({"lineage":self.lineage,"weights_sha256":self.weights_sha256,
            "feature_contract_digest":self.feature_contract_digest,
            "feature_encoding_digest":self.feature_encoding_digest})
    }
}

/// Input -> 64 tanh hidden -> actor logits (4) and a scalar critic.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Controller {
    pub(super) arm: Arm,
    pub(super) nf: usize,
    pub(super) mean: Vec<f64>,
    pub(super) scale: Vec<f64>,
    /// Row-major HIDDEN x nf.
    pub(super) w1: Vec<f64>,
    pub(super) b1: Vec<f64>,
    /// Row-major NA x HIDDEN.
    pub(super) wa: Vec<f64>,
    pub(super) ba: [f64; NA],
    pub(super) wv: Vec<f64>,
    pub(super) bv: f64,
    pub(super) parent: ParentBinding,
}

impl Controller {
    /// Actor outputs and critic value at raw feature row `x`.
    pub(super) fn forward(&self, x: &[f64]) -> ([f64; NA], f64) {
        debug_assert_eq!(x.len(), self.nf);
        let xn: Vec<f64> = x
            .iter()
            .zip(&self.mean)
            .zip(&self.scale)
            .map(|((v, m), s)| (v - m) / s)
            .collect();
        let h: Vec<f64> = (0..HIDDEN)
            .map(|j| {
                let row = &self.w1[j * self.nf..(j + 1) * self.nf];
                let z: f64 = row.iter().zip(&xn).map(|(w, v)| w * v).sum();
                (self.b1[j] + z).tanh()
            })
            .collect();
        let actor = std::array::from_fn(|a| {
            let row = &self.wa[a * HIDDEN..(a + 1) * HIDDEN];
            self.ba[a] + row.iter().zip(&h).map(|(w, v)| w * v).sum::<f64>()
        });
        let value = self.bv + self.wv.iter().zip(&h).map(|(w, v)| w * v).sum::<f64>();
        (actor, value)
    }

    /// Reads a weights file (`allocator2.py` writes it), checking every
    /// pinned contract field and the parent binding.
    pub(super) fn from_json(bytes: &[u8], parent: &ParentBinding) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Norm {
            mean: Vec<f64>,
            scale: Vec<f64>,
        }
        #[derive(Deserialize)]
        struct Params {
            w1: Vec<Vec<f64>>,
            b1: Vec<f64>,
            wa: Vec<Vec<f64>>,
            ba: Vec<f64>,
            wv: Vec<f64>,
            bv: f64,
        }
        #[derive(Deserialize)]
        struct File {
            schema: String,
            arm: String,
            feature_schema: String,
            features: Vec<String>,
            pooling_schema: Option<String>,
            forward_activation_mode: String,
            parent: ParentBinding,
            actions: Vec<String>,
            positive_at_full: f64,
            positive_split: Vec<f64>,
            epsilon: f64,
            hidden: usize,
            normalization: Norm,
            params: Params,
        }
        let f: File = serde_json::from_slice(bytes).map_err(|e| format!("weights: {e}"))?;
        let arm = Arm::parse(&f.arm)?;
        if !arm.learned() {
            return Err("weights arm must be S or H".into());
        }
        if f.schema != WEIGHTS_SCHEMA || f.feature_schema != FEATURE_SCHEMA {
            return Err(format!(
                "weights schema {} / {}",
                f.schema, f.feature_schema
            ));
        }
        let names = feature_names(arm);
        let pooling = (arm == Arm::H).then(|| POOLING_SCHEMA_V1.to_owned());
        if f.features != names
            || f.actions != ACTION_LABELS
            || f.positive_at_full != POSITIVE_AT_FULL
            || f.positive_split != POSITIVE_SPLIT
            || f.epsilon != EPSILON
            || f.hidden != HIDDEN
            || f.pooling_schema != pooling
        {
            return Err(
                "weights features, actions, base, epsilon, width or pooling differ from the pinned schema"
                    .into(),
            );
        }
        if f.forward_activation_mode != "libm_tanh" {
            return Err("weights must bind the ordinary libm_tanh forward".into());
        }
        if f.parent != *parent {
            return Err(format!(
                "weights are bound to parent {:?}, not {:?}",
                f.parent, parent
            ));
        }
        let nf = names.len();
        let p = f.params;
        let n = &f.normalization;
        if n.mean.len() != nf
            || n.scale.len() != nf
            || p.w1.len() != HIDDEN
            || p.w1.iter().any(|r| r.len() != nf)
            || p.b1.len() != HIDDEN
            || p.wa.len() != NA
            || p.wa.iter().any(|r| r.len() != HIDDEN)
            || p.ba.len() != NA
            || p.wv.len() != HIDDEN
        {
            return Err("weights shapes differ from the schema".into());
        }
        let all = n
            .mean
            .iter()
            .chain(&n.scale)
            .chain(p.w1.iter().flatten())
            .chain(&p.b1)
            .chain(p.wa.iter().flatten())
            .chain(&p.ba)
            .chain(&p.wv)
            .chain(std::iter::once(&p.bv));
        if all.clone().any(|v| !v.is_finite()) || n.scale.iter().any(|&s| s <= 0.0) {
            return Err("weights must be finite with positive scales".into());
        }
        Ok(Self {
            arm,
            nf,
            mean: n.mean.clone(),
            scale: n.scale.clone(),
            w1: p.w1.concat(),
            b1: p.b1,
            wa: p.wa.concat(),
            ba: [p.ba[0], p.ba[1], p.ba[2], p.ba[3]],
            wv: p.wv,
            bv: p.bv,
            parent: f.parent,
        })
    }
}

/// Loaded weights: the controller and the SHA-256 of the file bytes.
pub(super) struct Weights {
    pub(super) controller: Controller,
    pub(super) sha256: String,
    pub(super) path: String,
}

impl Weights {
    pub(super) fn load(path: &str, parent: &ParentBinding) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(Self {
            controller: Controller::from_json(&bytes, parent)?,
            sha256: super::sha(&bytes),
            path: path.to_owned(),
        })
    }
}

fn lg(v: f64, hi: f64) -> f64 {
    (v.ln_1p() / hi.ln_1p()).min(1.0)
}

fn clip(v: f64, hi: f64) -> f64 {
    v.min(hi) / hi
}

/// Everything the shared features read at one consultation.
#[derive(Clone, Debug)]
pub(super) struct Inputs {
    pub(super) counts: Counts,
    pub(super) context: GuideContext,
    pub(super) lookup: Lookup,
    pub(super) probs: Vec<f64>,
    pub(super) argmax: usize,
    pub(super) value: f64,
    pub(super) public: Public,
    pub(super) turn: u32,
    pub(super) active: bool,
}

/// The pinned shared feature row (`S_FEATURES`), fixed scalings.
pub(super) fn features_s(i: &Inputs) -> [f64; NS] {
    let c = &i.counts;
    let g = &i.context;
    let k = i.probs.len() as f64;
    let entropy: f64 = -i
        .probs
        .iter()
        .filter(|&&p| p > 0.0)
        .map(|p| p * p.ln())
        .sum::<f64>();
    let mut sorted = i.probs.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let top = sorted.first().copied().unwrap_or(0.0);
    let gap = top - sorted.get(1).copied().unwrap_or(0.0);
    let cap = c.cap.max(1) as f64;
    let mut x = [0.0; NS];
    x[0] = c.cap as f64 / BUDGET_SCALE;
    x[1] = c.remaining as f64 / BUDGET_SCALE;
    x[2] = (c.remaining as f64 / cap).min(1.0);
    x[3] = g.transitions_used as f64 / BUDGET_SCALE;
    x[4] = lg(g.physical as f64, 256.0);
    x[5] = lg(c.nonforced as f64, 512.0);
    x[6] = clip(f64::from(g.substep_index), 8.0);
    x[7] = lg(g.natural as f64, 65_536.0);
    x[8] = clip(c.searches as f64, 32.0);
    x[9] = f64::from(u8::from(c.searches == 0));
    x[10] = clip(c.since_search.unwrap_or(64) as f64, 64.0);
    x[11] = clip(g.physical_since_search.unwrap_or(32) as f64, 32.0);
    let (no_tree, miss, unsupported, depth) = match i.lookup {
        Lookup::Miss if g.no_tree => (1.0, 0.0, 0.0, 0.0),
        Lookup::Miss => (0.0, 1.0, 0.0, 0.0),
        Lookup::Unsupported => (0.0, 0.0, 1.0, 0.0),
        Lookup::DepthLimit => (0.0, 0.0, 0.0, 1.0),
        Lookup::Supported(_) => (0.0, 0.0, 0.0, 0.0),
    };
    x[12] = no_tree;
    x[13] = miss;
    x[14] = unsupported;
    x[15] = depth;
    x[16] = (k + 1.0).log2().min(8.0) / 8.0;
    x[17] = if k > 1.0 { entropy / k.ln() } else { 0.0 };
    x[18] = top;
    x[19] = gap;
    x[20] = i.value.tanh();
    x[21] = clip(f64::from(i.turn), 32.0);
    x[22] = f64::from(u8::from(i.active));
    let p = &i.public;
    for (pi, base) in [(0usize, 23usize), (1, 28)] {
        x[base] = f64::from(p.life[pi].clamp(-20, 80)) / 80.0;
        x[base + 1] = clip(p.battlefield[pi] as f64, 64.0);
        x[base + 2] = clip(p.graveyard[pi] as f64, 64.0);
        x[base + 3] = clip(p.hand[pi] as f64, 32.0);
        x[base + 4] = clip(p.library[pi] as f64, 128.0);
    }
    x[33] = clip(p.stack as f64, 64.0);
    match &g.tree {
        Some(t) => {
            x[35] = lg(t.nodes as f64, 65_536.0);
            x[36] = lg(t.root.visits as f64, 65_536.0);
            x[37] = clip(t.root.supported as f64, 8.0);
            x[38] = t.root.best_mean.unwrap_or(0.0);
            x[39] = lg(t.completed_natural as f64, 65_536.0);
        }
        None => x[34] = 1.0,
    }
    match &g.node {
        Some(n) => {
            x[41] = n.edges as f64 / k.max(1.0);
            x[42] = n.tried as f64 / (n.edges.max(1) as f64);
            x[43] = lg(n.visits as f64, 4_096.0);
            x[44] = clip(n.max_backups as f64, 8.0);
            x[45] = clip(n.supported as f64, 8.0);
            x[46] = n.best_mean.unwrap_or(0.0);
            match n.mean_gap {
                Some(v) => x[47] = v,
                None => x[48] = 1.0,
            }
            match g.node_best_live {
                Some(live) => x[49] = f64::from(u8::from(live as usize == i.argmax)),
                None => x[50] = 1.0,
            }
        }
        None => {
            x[40] = 1.0;
            x[48] = 1.0;
            x[50] = 1.0;
        }
    }
    x
}

/// The H block: state, action mean and action max, each `REP` wide.
pub(super) fn features_h(rep: &PooledRepresentationV1) -> Result<Vec<f64>, String> {
    if rep.state.len() != REP || rep.action_mean.len() != REP || rep.action_max.len() != REP {
        return Err(format!(
            "representation width {} differs from the pinned {REP}",
            rep.state.len()
        ));
    }
    Ok(rep
        .state
        .iter()
        .chain(&rep.action_mean)
        .chain(&rep.action_max)
        .map(|&v| f64::from(v))
        .collect())
}

/// The live scheduler of one q, S or H arm-game.
pub(super) struct Scheduler {
    arm: Arm,
    controller: Option<Controller>,
    rng: SplitMix64,
    scorer: FrozenPlayPolicyV1,
    cap: u64,
    probe: Option<ProbeSpec>,
    pub(super) seconds: f64,
    pub(super) decisions: u64,
    pub(super) requested: u64,
}

impl Scheduler {
    pub(super) fn new(
        arm: Arm,
        controller: Option<Controller>,
        controller_seed: u64,
        policy: &FrozenPlayPolicyV1,
        cap: u64,
        probe: Option<ProbeSpec>,
    ) -> Result<Self, String> {
        if arm.learned() != controller.is_some()
            || controller.as_ref().is_some_and(|c| c.arm != arm)
        {
            return Err(format!("arm {} and its controller differ", arm.label()));
        }
        Ok(Self {
            arm,
            controller,
            rng: SplitMix64::seed(controller_seed),
            scorer: policy.fork_for_collection_v3()?,
            cap,
            probe,
            seconds: 0.0,
            decisions: 0,
            requested: 0,
        })
    }
}

impl Consult for Scheduler {
    fn cap(&self) -> u64 {
        self.cap
    }

    fn probe(&self) -> Option<ProbeSpec> {
        self.probe
    }

    fn consult(&mut self, c: &ConsultCtx<'_>) -> Result<(u64, Value), String> {
        let started = Instant::now();
        debug_assert!(c.counts.remaining > 0);
        let (scores, rep) = self.scorer.score_fast_session_with_representation_v1(c.s)?;
        let probs = softmax(&scores.logits);
        // First maximum (ties to the lower index).
        let argmax = (0..scores.logits.len()).fold(0, |b, i| {
            if scores.logits[i] > scores.logits[b] {
                i
            } else {
                b
            }
        });
        let st = c.s.game_state();
        let inputs = Inputs {
            counts: c.counts,
            context: c.context,
            lookup: c.lookup,
            probs,
            argmax,
            value: f64::from(scores.value),
            public: Public::of(c.s, c.focal),
            turn: st.turn,
            active: st.active_player == c.focal,
        };
        let s = features_s(&inputs);
        let h = features_h(&rep)?;
        let q = base_q(c.counts.remaining, self.cap);
        let cs = choices(c.counts.remaining);
        let q_c = coalesce(&q, &cs);
        let (pi_c, mu_c, critic) = match (&self.controller, self.arm) {
            (Some(ctl), Arm::S | Arm::H) => {
                let x: Vec<f64> = if self.arm == Arm::H {
                    s.iter().chain(&h).copied().collect()
                } else {
                    s.to_vec()
                };
                let (actor, value) = ctl.forward(&x);
                let pi_c = coalesce(&policy(&q, &actor), &cs);
                let mu_c = mixture(&pi_c, &q_c);
                (pi_c, mu_c, Some(value))
            }
            (None, Arm::Q) => (q_c.clone(), q_c.clone(), None),
            _ => return Err("the current arm has no scheduler".into()),
        };
        // With one executable choice there is nothing to decide and no row
        // for the policy gradient (unreachable while remaining > 0).
        let (i, u) = if cs.len() < 2 {
            (0, None)
        } else {
            let u = uniform(&mut self.rng);
            (pick(&mu_c, u), Some(u))
        };
        let amount = cs[i].amount;
        self.decisions += 1;
        self.requested += amount;
        self.seconds += started.elapsed().as_secs_f64();
        let lookup = match c.lookup {
            Lookup::Unsupported => "matched_unsupported",
            Lookup::DepthLimit => "depth_limit",
            _ if c.context.no_tree => "no_tree",
            _ => "history_miss",
        };
        Ok((
            amount,
            json!({"index":self.decisions - 1,"s":s.to_vec(),"rep":h,"rep_actions":rep.actions,
                "lookup":lookup,"remaining":c.counts.remaining,"cap":self.cap,
                "nonforced":c.counts.nonforced,"physical":c.context.physical,
                "searches":c.counts.searches,"k":inputs.probs.len(),"policy_argmax":argmax,
                "amounts":cs.iter().map(|c| c.amount).collect::<Vec<_>>(),
                "members":cs.iter().map(|c| c.members.clone()).collect::<Vec<_>>(),
                "q":q_c,"pi":pi_c,"mu":mu_c,"critic":critic,"u":u,"chosen":i,"amount":amount,
                "nontrivial":cs.len() >= 2}),
        ))
    }
}

/// A probe case: the probe decision, its refresh size (0 executes the
/// current tree) and the paired continuation seeds.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProbeCase {
    pub(super) source: String,
    pub(super) at: u64,
    pub(super) refresh: u64,
    pub(super) continuation_seeds: [u64; 2],
}

/// One allocator v2 case line (`ALLOC2_CASES`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AllocCase {
    pub(super) id: String,
    pub(super) arm: String,
    pub(super) controller_seed: u64,
    pub(super) case: SourceSpec,
    #[serde(default)]
    pub(super) probe: Option<ProbeCase>,
}

/// One arm-game from the case's ordinary opening.
#[allow(clippy::too_many_arguments, reason = "explicit roles and identities")]
pub(super) fn allocator_case(
    sh: &BankShared,
    policy: &FrozenPlayPolicyV1,
    parent: &ParentBinding,
    case: &AllocCase,
    weights: Option<&Weights>,
    total: u64,
    mode: KeyMode,
    binding: &str,
) -> Result<Value, String> {
    let started = Instant::now();
    let src = &case.case;
    let arm = Arm::parse(&case.arm)?;
    let deck = sh.spec.decks.get(src.own).map(|d| d.label.as_str());
    if deck.and_then(|l| l.split('/').next()) != Some(super::SPY_DECK) {
        return Err(format!("{}: allocator v2 plays Spy only", case.id));
    }
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
        extended: true,
    };
    let probe = case.probe.as_ref().map(|p| ProbeSpec {
        at: p.at,
        refresh: p.refresh,
        continuation_seeds: p.continuation_seeds,
    });
    let (allocation, mut scheduler, weights_used) = match arm {
        Arm::Current => {
            if probe.is_some() {
                return Err("a probe replays a scheduler arm".into());
            }
            (
                Allocation {
                    anchor: ANCHOR_BUDGET,
                    refresh: REFRESH_BUDGET,
                    total,
                },
                None,
                None,
            )
        }
        Arm::Q | Arm::S | Arm::H => {
            let (controller, used) = match (arm, weights) {
                (Arm::S | Arm::H, Some(w)) => (Some(w.controller.clone()), Some(w)),
                (Arm::S | Arm::H, None) => return Err("a learned arm needs ALLOC2_WEIGHTS".into()),
                _ => (None, None),
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
                    probe,
                )?),
                used,
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
        scheduler.as_mut().map(|s| s as &mut dyn Consult),
    )?;
    let sched = scheduler.as_ref().map(
        |s| json!({"decisions":s.decisions,"requested":s.requested,"controller_seconds":s.seconds}),
    );
    // The frozen player after the game: allocator updates never touch it.
    let player = ParentBinding::of(policy, &sh.spec.lineage);
    Ok(
        json!({"kind":"allocator2_game","schema":GAME_SCHEMA,"id":case.id,
        "arm":arm.label(),"lineage":sh.spec.lineage,"own":src.own,"other":src.other,"seat":src.seat,
        "opp":src.opp,"seed":src.seed,"starting_player":src.starting_player,
        "live_policy_seeds":src.guide_policy_seeds,"search_root":src.search_root,
        "controller_seed":case.controller_seed,"bank_binding_sha256":binding,
        "cap":total,"allocation":{"initial":allocation.anchor,"refresh":allocation.refresh,"total":allocation.total},
        "controller":{"feature_schema":FEATURE_SCHEMA,"features":S_FEATURES.to_vec(),"actions":ACTION_LABELS,
            "pooling_schema":POOLING_SCHEMA_V1,"positive_at_full":POSITIVE_AT_FULL,
            "positive_split":POSITIVE_SPLIT,"epsilon":if arm.learned() { json!(EPSILON) } else { Value::Null },
            "weights_sha256":weights_used.map(|w| w.sha256.clone()),"weights_path":weights_used.map(|w| w.path.clone()),
            "parent":parent.json(),"forward_activation_mode":policy.forward_activation_mode_label_v1()},
        "player_after":player.json(),
        "probe":case.probe.as_ref().map(|p| json!({"source":p.source,"at":p.at,"refresh":p.refresh,
            "continuation_seeds":p.continuation_seeds})),
        "scheduler":sched,"game":rec,"wall":started.elapsed().as_secs_f64()}),
    )
}

#[cfg(test)]
mod tests {
    use super::super::allocator::Counts;
    use super::super::refresh::{NodeSummary, TreeSummary};
    use super::*;
    use crate::rl_session::FastActorSessionV1;

    fn parent() -> ParentBinding {
        ParentBinding {
            lineage: "r1".into(),
            weights_sha256: "aa".into(),
            feature_contract_digest: "bb".into(),
            feature_encoding_digest: "cc".into(),
        }
    }

    fn controller(arm: Arm, seed: u64, actor_zero: bool) -> Controller {
        let nf = feature_names(arm).len();
        let mut r = SplitMix64::seed(seed);
        let mut u = || uniform(&mut r) - 0.5;
        Controller {
            arm,
            nf,
            mean: (0..nf).map(|_| 0.1 * u()).collect(),
            scale: (0..nf).map(|_| 1.0 + u().abs()).collect(),
            w1: (0..HIDDEN * nf).map(|_| 0.3 * u()).collect(),
            b1: (0..HIDDEN).map(|_| 0.1 * u()).collect(),
            wa: (0..NA * HIDDEN)
                .map(|_| if actor_zero { 0.0 } else { u() })
                .collect(),
            ba: if actor_zero {
                [0.0; NA]
            } else {
                [u(), u(), u(), u()]
            },
            wv: (0..HIDDEN).map(|_| u()).collect(),
            bv: 0.0,
            parent: parent(),
        }
    }

    fn file(c: &Controller) -> Value {
        json!({"schema":WEIGHTS_SCHEMA,"arm":c.arm.label(),"feature_schema":FEATURE_SCHEMA,
            "features":feature_names(c.arm),
            "pooling_schema":if c.arm == Arm::H { json!(POOLING_SCHEMA_V1) } else { Value::Null },
            "forward_activation_mode":"libm_tanh","parent":parent().json(),"actions":ACTION_LABELS,
            "positive_at_full":POSITIVE_AT_FULL,"positive_split":POSITIVE_SPLIT,"epsilon":EPSILON,
            "hidden":HIDDEN,"update":0,
            "normalization":{"mean":c.mean,"scale":c.scale,"frozen":false},
            "params":{"w1":c.w1.chunks(c.nf).map(<[f64]>::to_vec).collect::<Vec<_>>(),"b1":c.b1,
                "wa":c.wa.chunks(HIDDEN).map(<[f64]>::to_vec).collect::<Vec<_>>(),"ba":c.ba,"wv":c.wv,"bv":c.bv}})
    }

    #[test]
    fn base_q_tapers_with_the_remaining_fraction() {
        assert_eq!(base_q(CAP, CAP), [0.5, 0.25, 0.2, 0.05]);
        let q = base_q(CAP / 2, CAP);
        assert_eq!(q, [0.75, 0.125, 0.1, 0.025]);
        for r in [1, 1000, 500_000, CAP] {
            assert!((base_q(r, CAP).iter().sum::<f64>() - 1.0).abs() < 1e-15);
        }
    }

    #[test]
    fn amounts_clip_and_coalesce() {
        let amounts = |r| choices(r).iter().map(|c| c.amount).collect::<Vec<_>>();
        assert_eq!(amounts(CAP), [0, 32_000, 128_000, 512_000]);
        assert_eq!(amounts(300_000), [0, 32_000, 128_000, 300_000]);
        assert_eq!(amounts(100_000), [0, 32_000, 100_000]);
        assert_eq!(choices(100_000)[2].members, [2, 3]);
        assert_eq!(amounts(5), [0, 5]);
        assert_eq!(choices(5)[1].members, [1, 2, 3]);
        let q = base_q(100_000, CAP);
        let qc = coalesce(&q, &choices(100_000));
        assert!((qc[2] - (q[2] + q[3])).abs() < 1e-18);
        assert!((qc.iter().sum::<f64>() - 1.0).abs() < 1e-15);
    }

    #[test]
    fn zero_actor_gives_pi_equal_q_and_mu_equal_q() {
        for arm in [Arm::S, Arm::H] {
            let c = controller(arm, 3, true);
            let mut r = SplitMix64::seed(9);
            for remaining in [CAP, 700_000, 100_000, 7] {
                let x: Vec<f64> = (0..c.nf).map(|_| uniform(&mut r)).collect();
                let (actor, value) = c.forward(&x);
                assert_eq!(actor, [0.0; NA]);
                assert!(
                    value.is_finite() && value != 0.0,
                    "critic is not zero-initialized"
                );
                let q = base_q(remaining, CAP);
                let cs = choices(remaining);
                let qc = coalesce(&q, &cs);
                let pc = coalesce(&policy(&q, &actor), &cs);
                let mc = mixture(&pc, &qc);
                for ((p, m), q) in pc.iter().zip(&mc).zip(&qc) {
                    assert!((p - q).abs() < 1e-15 && (m - q).abs() < 1e-15);
                }
            }
        }
    }

    #[test]
    fn behavior_is_the_coalesced_mixture() {
        let c = controller(Arm::S, 5, false);
        let x = vec![0.3; c.nf];
        let (actor, _) = c.forward(&x);
        for remaining in [CAP, 100_000, 31_000] {
            let q = base_q(remaining, CAP);
            let pi = policy(&q, &actor);
            let cs = choices(remaining);
            let (pc, qc) = (coalesce(&pi, &cs), coalesce(&q, &cs));
            let mc = mixture(&pc, &qc);
            for (ch, m) in cs.iter().zip(&mc) {
                let want: f64 = ch.members.iter().map(|&a| 0.8 * pi[a] + 0.2 * q[a]).sum();
                assert!((m - want).abs() < 1e-15);
            }
            assert!((mc.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        }
    }

    /// The forward equals an explicit transcription (normalize, tanh layer,
    /// actor and critic heads).
    #[test]
    fn forward_matches_explicit_formula() {
        let c = controller(Arm::H, 11, false);
        let x: Vec<f64> = (0..c.nf).map(|i| (i as f64 * 0.37).sin()).collect();
        let (actor, value) = c.forward(&x);
        let mut h = vec![0.0; HIDDEN];
        for (j, hj) in h.iter_mut().enumerate() {
            let mut z = c.b1[j];
            for (k, xk) in x.iter().enumerate() {
                z += c.w1[j * c.nf + k] * (xk - c.mean[k]) / c.scale[k];
            }
            *hj = z.tanh();
        }
        for (a, act) in actor.iter().enumerate() {
            let want = c.ba[a]
                + (0..HIDDEN)
                    .map(|j| c.wa[a * HIDDEN + j] * h[j])
                    .sum::<f64>();
            assert!((act - want).abs() < 1e-12);
        }
        let want = c.bv + (0..HIDDEN).map(|j| c.wv[j] * h[j]).sum::<f64>();
        assert!((value - want).abs() < 1e-12);
    }

    #[test]
    fn weights_file_round_trips_and_binds_parent_and_schema() {
        for arm in [Arm::S, Arm::H] {
            let c = controller(arm, 21, false);
            let f = file(&c);
            let back = Controller::from_json(f.to_string().as_bytes(), &parent()).unwrap();
            assert_eq!(back, c, "decimal weights parse exactly");
            let other = ParentBinding {
                weights_sha256: "zz".into(),
                ..parent()
            };
            assert!(Controller::from_json(f.to_string().as_bytes(), &other).is_err());
            for (k, v) in [
                ("epsilon", json!(0.1)),
                ("hidden", json!(32)),
                ("forward_activation_mode", json!("fast_search_tanh")),
                ("arm", json!(if arm == Arm::S { "H" } else { "S" })),
            ] {
                let mut bad = f.clone();
                bad[k] = v;
                assert!(
                    Controller::from_json(bad.to_string().as_bytes(), &parent()).is_err(),
                    "{k}"
                );
            }
            let mut bad = f.clone();
            bad["normalization"]["scale"][0] = json!(0.0);
            assert!(Controller::from_json(bad.to_string().as_bytes(), &parent()).is_err());
        }
    }

    fn inputs() -> Inputs {
        Inputs {
            counts: Counts {
                remaining: 1_024_000,
                cap: CAP,
                nonforced: 10,
                since_search: Some(3),
                searches: 2,
            },
            context: GuideContext {
                no_tree: false,
                physical: 7,
                physical_since_search: Some(2),
                natural: 1000,
                transitions_used: 1_024_000,
                substep_index: 1,
                tree: Some(TreeSummary {
                    nodes: 500,
                    root: NodeSummary {
                        edges: 3,
                        tried: 3,
                        visits: 100,
                        max_backups: 60,
                        supported: 2,
                        best_edge: Some(1),
                        best_mean: Some(0.6),
                        mean_gap: Some(0.2),
                    },
                    completed_natural: 120,
                }),
                node: Some(NodeSummary {
                    edges: 4,
                    tried: 2,
                    visits: 9,
                    max_backups: 5,
                    supported: 0,
                    best_edge: Some(2),
                    best_mean: Some(0.75),
                    mean_gap: None,
                }),
                node_best_live: Some(1),
            },
            lookup: Lookup::Unsupported,
            probs: vec![0.1, 0.6, 0.2, 0.1],
            argmax: 1,
            value: 0.5,
            public: Public {
                life: [20, 3],
                battlefield: [2, 5],
                graveyard: [10, 0],
                hand: [5, 7],
                library: [40, 50],
                stack: 1,
            },
            turn: 4,
            active: true,
        }
    }

    #[test]
    fn shared_features_follow_the_pinned_scaling() {
        let i = inputs();
        let x = features_s(&i);
        assert_eq!(S_FEATURES.len(), NS);
        assert_eq!(x[0], 1.0);
        assert_eq!((x[1], x[2], x[3]), (0.5, 0.5, 0.5));
        assert!((x[4] - 8f64.ln() / 257f64.ln()).abs() < 1e-15);
        assert_eq!(x[6], 1.0 / 8.0);
        assert_eq!(
            (x[8], x[9], x[10], x[11]),
            (2.0 / 32.0, 0.0, 3.0 / 64.0, 2.0 / 32.0)
        );
        assert_eq!(&x[12..16], &[0.0, 0.0, 1.0, 0.0]);
        assert!((x[18] - 0.6).abs() < 1e-15 && (x[19] - 0.4).abs() < 1e-15);
        assert_eq!((x[21], x[22]), (4.0 / 32.0, 1.0));
        assert_eq!(x[34], 0.0, "tree present");
        assert_eq!((x[37], x[38]), (2.0 / 8.0, 0.6));
        assert_eq!(x[40], 0.0, "node present");
        assert_eq!((x[41], x[42], x[44]), (1.0, 0.5, 5.0 / 8.0));
        assert_eq!(
            (x[46], x[47], x[48]),
            (0.75, 0.0, 1.0),
            "one tried edge: gap missing"
        );
        assert_eq!((x[49], x[50]), (1.0, 0.0), "tree best is the policy argmax");
        // No tree yet: exactly the no-tree lookup and both missing flags.
        let mut j = inputs();
        j.lookup = Lookup::Miss;
        j.context.no_tree = true;
        j.context.tree = None;
        j.context.node = None;
        j.context.node_best_live = None;
        let y = features_s(&j);
        assert_eq!(&y[12..16], &[1.0, 0.0, 0.0, 0.0]);
        assert_eq!((y[34], y[40], y[48], y[50]), (1.0, 1.0, 1.0, 1.0));
        assert!(y[35..40].iter().chain(&y[41..48]).all(|&v| v == 0.0));
        j.context.no_tree = false;
        assert_eq!(&features_s(&j)[12..16], &[0.0, 1.0, 0.0, 0.0]);
        j.lookup = Lookup::DepthLimit;
        assert_eq!(&features_s(&j)[12..16], &[0.0, 0.0, 0.0, 1.0]);
        assert!(x.iter().chain(&y).all(|v| v.is_finite()));
    }

    #[test]
    fn h_names_append_three_representation_blocks() {
        let n = feature_names(Arm::H);
        assert_eq!(n.len(), NS + 3 * REP);
        assert_eq!(n[NS], "rep_state_00");
        assert_eq!(n[NS + REP], "rep_action_mean_00");
        assert_eq!(n[NS + 3 * REP - 1], "rep_action_max_63");
        assert_eq!(feature_names(Arm::S).len(), NS);
    }

    fn fixture_game(seed: u64) -> FastActorSessionV1 {
        use crate::runtime_decks::RUNTIME_DECKS;
        let decks = [&RUNTIME_DECKS[4], &RUNTIME_DECKS[5]];
        let mut s = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1, seed, 100_000, 200_000, decks.map(|d| d.id.to_owned()), decks.map(|d| d.card_ids.to_vec()),
            crate::ids::PlayerId(0),
        )
        .unwrap();
        s.set_runtime_rules_v1(crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1)
            .unwrap();
        s
    }

    /// Extracting the pooled representation leaves the player's logits,
    /// value and sampling bit-identical on real decisions, and repeated
    /// extraction at one decision is identical (nothing retained).
    #[test]
    fn representation_extraction_leaves_play_bit_identical() {
        let base = FrozenPlayPolicyV1::training_fixture_v3();
        let mut plain = base.fork_for_collection_v3().unwrap();
        let mut extracting = base.fork_for_collection_v3().unwrap();
        let mut decisions = 0u64;
        for seed in [3u64, 4] {
            plain.reset_sampling_v1([seed, seed + 10]);
            extracting.reset_sampling_v1([seed, seed + 10]);
            let mut s = fixture_game(seed);
            while let Some(d) = super::super::play::decision(&s) {
                if decisions >= 600 {
                    break;
                }
                let a = plain.score_fast_session_v1(&s).unwrap();
                let (b, rep) = extracting
                    .score_fast_session_with_representation_v1(&s)
                    .unwrap();
                let (_, again) = extracting
                    .score_fast_session_with_representation_v1(&s)
                    .unwrap();
                assert_eq!(rep, again);
                assert_eq!(a.logits.len(), b.logits.len());
                for (x, y) in a.logits.iter().zip(&b.logits) {
                    assert_eq!(x.to_bits(), y.to_bits());
                }
                assert_eq!(a.value.to_bits(), b.value.to_bits());
                assert_eq!(rep.actions, a.logits.len());
                let h = features_h(&rep).unwrap();
                assert!(h.iter().all(|v| v.abs() <= 1.0), "tanh rows");
                for j in 0..REP {
                    assert!(rep.action_max[j] >= rep.action_mean[j] - 1e-6);
                }
                let x = plain.select_fast_session_v1(&s).unwrap();
                let y = extracting.select_fast_session_v1(&s).unwrap();
                assert_eq!(x, y, "sampling differs after extraction");
                s.step(d.episode_id, d.step, x).unwrap();
                decisions += 1;
            }
        }
        assert!(decisions > 100, "{decisions} decisions");
    }

    /// A weights file written by `allocator2.py` loads in Rust and the Rust
    /// forward, policy and behavior equal Python's values (the fixture's).
    #[test]
    fn python_weights_and_forward_agree() {
        let fixture: Value = serde_json::from_str(include_str!("allocator2_parity.json")).unwrap();
        let bound = ParentBinding {
            lineage: "r1".into(),
            weights_sha256: "parity".into(),
            feature_contract_digest: "c".into(),
            feature_encoding_digest: "e".into(),
        };
        let c = Controller::from_json(fixture["weights"].to_string().as_bytes(), &bound).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let x: Vec<f64> = serde_json::from_value(case["x"].clone()).unwrap();
            let remaining = case["remaining"].as_u64().unwrap();
            let (actor, value) = c.forward(&x);
            let close = |a: f64, b: &Value| (a - b.as_f64().unwrap()).abs() < 1e-12;
            for (a, want) in actor.iter().zip(case["actor"].as_array().unwrap()) {
                assert!(close(*a, want), "actor {a} vs {want}");
            }
            assert!(close(value, &case["value"]));
            let q = base_q(remaining, CAP);
            let cs = choices(remaining);
            let pi = coalesce(&policy(&q, &actor), &cs);
            let mu = mixture(&pi, &coalesce(&q, &cs));
            for (got, want) in pi.iter().zip(case["pi"].as_array().unwrap()) {
                assert!(close(*got, want));
            }
            for (got, want) in mu.iter().zip(case["mu"].as_array().unwrap()) {
                assert!(close(*got, want));
            }
        }
    }

    #[test]
    fn case_lines_parse_with_and_without_a_probe() {
        let src = json!({"id":"x","own":4,"other":1,"seat":0,"opp":"T1","seed":5,"starting_player":1,
            "source_policy_seeds":[0,0],"anchor_draw":0,"guide_policy_seeds":[1,2],"search_root":"r"});
        let a: AllocCase =
            serde_json::from_value(json!({"id":"a","arm":"S","controller_seed":3,"case":src}))
                .unwrap();
        assert!(a.probe.is_none());
        let b: AllocCase = serde_json::from_value(json!({"id":"b","arm":"q","controller_seed":3,
            "case":src,"probe":{"source":"a","at":4,"refresh":128000,"continuation_seeds":[7,8]}}))
        .unwrap();
        assert_eq!(b.probe.unwrap().at, 4);
        assert!(serde_json::from_value::<AllocCase>(
            json!({"id":"a","arm":"S","controller_seed":3,"case":src,"extra":1})
        )
        .is_err());
    }
}
