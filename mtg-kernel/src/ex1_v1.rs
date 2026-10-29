//! EX1, search in the training loop (collab
//! LANES/spy-primary-direction-20261028/EX1-DESIGN-DRAFT.md v3.3, adopted at
//! blob 8baee54f): the pure parts every EX1 caller shares. The menu stratum
//! (section 4) is the one rule the win checker and the collection hook both
//! call; `ex1-target-v1` (section 3) is the complete target vector; the
//! placement and row-cap draws (sections 3 and 4) are label-blind hashes.
//!
//! The target is binary64 round-to-nearest-even without fused multiply-add,
//! on `line_b_softmax_v1`'s pinned log-sum-exp softmax, so the Python mirror
//! (`tools/ex1/target_ref.py`) reproduces it bit for bit.

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::line_b_teacher_target_v1::{
    line_b_softmax_v1, ln_normal_v1, LineBDivergenceV1, LineBSoftmaxV1,
};
use crate::rl::ActionSemanticV1;

pub const EX1_TARGET_VERSION_V1: &str = "ex1-target-v1";
/// An edge is supported when it has at least this many terminal backups.
pub const EX1_SUPPORT_MIN_V1: u64 = 8;
pub const EX1_TEMPERATURE_V1: f64 = 0.25;
/// `1 / 0.25`, exact: `x / 0.25 == 4 x` for every finite binary64.
const EX1_INVERSE_TEMPERATURE_V1: f64 = 4.0;
/// Below this student mass on the supported set a root gives no target.
pub const EX1_MASS_FLOOR_V1: f64 = 1e-6;
/// The loss is forward KL, `KL(q || p_theta)`; the engine default is reverse.
pub const EX1_DIVERGENCE_V1: LineBDivergenceV1 = LineBDivergenceV1::Forward;
/// At most this many taught rows enter one update.
pub const EX1_ROW_CAP_V1: usize = 128;
/// Each row's gradient weight is `c / 128` whatever the row count.
pub const EX1_DENOMINATOR_V1: usize = 128;

/// The section 4 menu stratum of a decision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Ex1StratumV1 {
    Target,
    CastOrActivate,
    Other,
}

impl Ex1StratumV1 {
    pub const ALL: [Self; 3] = [Self::Target, Self::CastOrActivate, Self::Other];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::CastOrActivate => "cast_or_activate",
            Self::Other => "other",
        }
    }

    /// p(S) = (E / 3) / n(S) at E = 3, from step 1's strata table (section 4).
    pub fn placement_rate_v1(self) -> f64 {
        match self {
            Self::Target => 0.155,
            Self::CastOrActivate => 0.0311,
            Self::Other => 0.0261,
        }
    }

    /// n(S): decisions with two or more legal actions per learner episode.
    pub fn expected_decisions_v1(self) -> f64 {
        match self {
            Self::Target => 6.46,
            Self::CastOrActivate => 32.17,
            Self::Other => 38.30,
        }
    }
}

/// Any ChooseTarget or ChooseEffectTarget action; otherwise any CastSpell or
/// ActivateAbility action; otherwise other (ActivateManaAbility and PlotSpell
/// included). The precedence of the checker's `stratum()` at 389130d7.
pub fn ex1_stratum_v1(menu: &[ActionSemanticV1]) -> Ex1StratumV1 {
    use ActionSemanticV1 as A;
    if menu
        .iter()
        .any(|a| matches!(a, A::ChooseTarget { .. } | A::ChooseEffectTarget { .. }))
    {
        Ex1StratumV1::Target
    } else if menu
        .iter()
        .any(|a| matches!(a, A::CastSpell { .. } | A::ActivateAbility { .. }))
    {
        Ex1StratumV1::CastOrActivate
    } else {
        Ex1StratumV1::Other
    }
}

/// `SHA256(json([tag, run, update, seed, physical_decision_id]))` with
/// compact JSON (Python's `separators=(",", ":")`), its first 8 bytes read
/// little-endian.
pub fn ex1_hash_u64_v1(tag: &str, run: &str, update: u64, seed: u64, decision: u64) -> u64 {
    let text = json!([tag, run, update, seed, decision]).to_string();
    let digest = Sha256::digest(text.as_bytes());
    let mut first = [0_u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first)
}

/// The hash as a uniform number in `[0, 1)`: its top 53 bits over `2^53`
/// (exact in binary64).
pub fn ex1_uniform_v1(hash: u64) -> f64 {
    (hash >> 11) as f64 * (1.0 / (1_u64 << 53) as f64)
}

/// Section 4's label-blind trigger.
pub fn ex1_placement_v1(
    run: &str,
    update: u64,
    seed: u64,
    decision: u64,
    stratum: Ex1StratumV1,
) -> bool {
    ex1_uniform_v1(ex1_hash_u64_v1("ex1/placement", run, update, seed, decision))
        < stratum.placement_rate_v1()
}

/// Section 3's row-cap order key (lowest kept first), independent of the
/// placement hash.
pub fn ex1_rowcap_key_v1(run: &str, update: u64, seed: u64, decision: u64) -> u64 {
    ex1_hash_u64_v1("ex1/rowcap", run, update, seed, decision)
}

/// Terminal backups of one edge, in live menu order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ex1EdgeCountsV1 {
    pub n: u64,
    pub wins: u64,
    pub draws: u64,
}

/// Why a searched root gives no target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ex1NoTargetV1 {
    /// Fewer than two supported edges.
    Support,
    /// Student mass on the supported set below 1e-6.
    Mass,
}

impl Ex1NoTargetV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Support => "support",
            Self::Mass => "mass",
        }
    }
}

/// One row's target and the quantities section 5 records beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ex1TargetV1 {
    pub target: LineBSoftmaxV1,
    /// The student's softmax at the live row.
    pub student: LineBSoftmaxV1,
    /// Supported edges, ascending.
    pub support: Vec<usize>,
    /// m_S, summed in ascending live order.
    pub mass: f64,
    /// Q-hat per edge; `None` for unsupported edges.
    pub qhat: Vec<Option<f64>>,
}

impl Ex1TargetV1 {
    /// TV(q, p_student), summed in live order.
    pub fn total_variation_v1(&self) -> f64 {
        let mut total = 0.0_f64;
        for (q, p) in self
            .target
            .probabilities
            .iter()
            .zip(&self.student.probabilities)
        {
            total += (q - p).abs();
        }
        0.5 * total
    }

    /// The argmax of q over S, ties to the lowest menu index.
    pub fn target_argmax_v1(&self) -> usize {
        let mut best = self.support[0];
        for &a in &self.support {
            if self.target.probabilities[a] > self.target.probabilities[best] {
                best = a;
            }
        }
        best
    }
}

/// Q-hat(a) = (wins + 0.5 draws) / n on the [0, 1] scale.
pub fn ex1_qhat_v1(counts: &Ex1EdgeCountsV1) -> f64 {
    (counts.wins as f64 + 0.5 * counts.draws as f64) / counts.n as f64
}

/// `ex1-target-v1` at one live row: the student's deployed logit bits and
/// the searched node's per-edge counts, both in live menu order. `Ok(Err)`
/// is a counted no-target root; `Err` is a malformed input.
pub fn ex1_target_v1(
    logit_bits: &[u32],
    counts: &[Ex1EdgeCountsV1],
) -> Result<Result<Ex1TargetV1, Ex1NoTargetV1>, String> {
    if logit_bits.len() != counts.len() {
        return Err("ex1 target: logits and edge counts differ in width".into());
    }
    if counts
        .iter()
        .any(|c| c.wins.checked_add(c.draws).is_none_or(|t| t > c.n))
    {
        return Err("ex1 target: an edge has more wins and draws than backups".into());
    }
    let z: Vec<f64> = logit_bits
        .iter()
        .map(|&b| f64::from(f32::from_bits(b)))
        .collect();
    let student = line_b_softmax_v1(&z).map_err(|e| format!("ex1 target: {e}"))?;
    let support: Vec<usize> = (0..counts.len())
        .filter(|&a| counts[a].n >= EX1_SUPPORT_MIN_V1)
        .collect();
    if support.len() < 2 {
        return Ok(Err(Ex1NoTargetV1::Support));
    }
    let mut mass = 0.0_f64;
    for &a in &support {
        mass += student.probabilities[a];
    }
    if mass < EX1_MASS_FLOOR_V1 {
        return Ok(Err(Ex1NoTargetV1::Mass));
    }
    let mut qhat = vec![None; counts.len()];
    let mut scaled = Vec::with_capacity(support.len());
    for &a in &support {
        let value = ex1_qhat_v1(&counts[a]);
        qhat[a] = Some(value);
        scaled.push(EX1_INVERSE_TEMPERATURE_V1 * value);
    }
    let sigma = line_b_softmax_v1(&scaled).map_err(|e| format!("ex1 target: {e}"))?;
    let log_mass = ln_normal_v1(mass);
    let mut probabilities = student.probabilities.clone();
    let mut log_probabilities = student.log_probabilities.clone();
    for (i, &a) in support.iter().enumerate() {
        probabilities[a] = mass * sigma.probabilities[i];
        log_probabilities[a] = log_mass + sigma.log_probabilities[i];
    }
    Ok(Ok(Ex1TargetV1 {
        target: LineBSoftmaxV1 {
            probabilities,
            log_probabilities,
            weight_sum: sigma.weight_sum,
        },
        student,
        support,
        mass,
        qhat,
    }))
}

#[cfg(test)]
mod tests;
