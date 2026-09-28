//! G115 line (b) teacher target and auxiliary divergences
//! (CODEX-G115-EXIT-PROPOSAL-20260927.md v0.2 "Target and loss"; director
//! ruling of 2026-09-27: slot 1 reverse `KL(p_theta || q)` (default), slot 2
//! forward `KL(q || p_theta)`; `c`, `T` and `K` are parameters, never tuned).
//!
//! All arithmetic is binary64 round-to-nearest-even without fused
//! multiply-add or a platform math library (`exp_v1` is the collection
//! sampler's, `ln_v1` is below), so a teacher packet computed on one host is
//! bit-identical on another. Every public entry verifies the pinned MXCSR and
//! fails closed. Callers promote binary32 logits exactly.
//!
//! - Softmax: gaps `d_i = z_i - z_max` from the first maximal logit, weights
//!   `w_i = exp_v1(d_i)` for `d_i >= -708` and `+0.0` below (the sampler's
//!   tail rule), `S` summed sequentially from `+0.0` in legal-action order,
//!   `p_i = w_i / S` and `log p_i = d_i - ln_v1(S)`.
//! - Target: `log q = log_softmax(z + (Qhat - max Qhat) / T)` over the root's
//!   legal actions, with `z` the recorded collection logits and `Qhat` each
//!   action's mean natural terminal return. This is the proposal's `log p_u +
//!   Qhat/T - LSE(log p_u + Qhat/T)`. The shift is exact for K = 16 and T =
//!   0.25; a zero shift leaves the logit untouched, so with equal returns `q`
//!   and `log q` repeat `p_u` and `log p_u` bit for bit.
//! - Reverse (slot 1): value `sum p_i (log p_i - log q_i)` over `p_i > 0`,
//!   logit gradient `p_i ((log p_i - log q_i) - KL)`.
//! - Forward (slot 2): value `sum q_i (log q_i - log p_i)` over `q_i > 0`,
//!   logit gradient `p_i - q_i`.
//!
//! Both gradients sum to zero over the menu in exact arithmetic, so the
//! scorer-bias gauge is unaffected up to rounding. The coefficient `c` and
//! the selected-root mean are the caller's.

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1;
use crate::unclamped_softmax_sampler_v1::{
    exp_v1, UNCLAMPED_SOFTMAX_GAP_FLOOR_V1, UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1,
};

pub const LINE_B_TEACHER_TARGET_VERSION_V1: &str = "line-b-teacher-target-f64-v1";
/// Declared recipe values (proposal v0.2): parameters, never tuned.
pub const LINE_B_TEACHER_TEMPERATURE_V1: f64 = 0.25;
pub const LINE_B_TEACHER_COEFFICIENT_V1: f64 = 0.1;
pub const LINE_B_TEACHER_ROLLOUTS_V1: usize = 16;
/// Declared CUDA-to-collection envelope (line (b) change 2; review change 9
/// of FABLE-REVIEW-20260928), in log-probability units, fixed and never
/// moved: the largest `|log p_device(a) - log p_collection(a)|` a selected
/// root may show in a treatment update. At K = 16 and T = 0.25 the smallest
/// non-zero return contrast (1/16) is 0.25 in target log-odds; a discrepancy
/// `d` adds `c p_i d` of spurious logit gradient per root against `c p_i`
/// times that contrast, a 0.4 percent per-coordinate scale comparison, not
/// a relative-gradient bound: centering can double the contribution and
/// the true gradient can cancel (CODEX #616). A breach fails the update.
pub const LINE_B_CUDA_ENVELOPE_V1: f64 = 1e-3;

/// fdlibm's two-part `ln 2` (the sampler's constants).
const LN2_HI_V1: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN2_LO_V1: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
/// `RN(sqrt 2)`.
const SQRT2_V1: f64 = f64::from_bits(0x3ff6_a09e_667f_3bcd);
/// `RN(1/(2k+1))` for `k = 0..=11`: `atanh t = t + t^3/3 + t^5/5 + ...`.
const ATANH_ODD_V1: [f64; 12] = [
    1.0,
    1.0 / 3.0,
    1.0 / 5.0,
    1.0 / 7.0,
    1.0 / 9.0,
    1.0 / 11.0,
    1.0 / 13.0,
    1.0 / 15.0,
    1.0 / 17.0,
    1.0 / 19.0,
    1.0 / 21.0,
    1.0 / 23.0,
];

/// The auxiliary divergence of a line (b) slot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum LineBDivergenceV1 {
    /// `KL(p_theta || q)`, slot 1.
    #[default]
    #[serde(rename = "reverse-kl")]
    Reverse,
    /// `KL(q || p_theta)`, slot 2.
    #[serde(rename = "forward-kl")]
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineBTeacherErrorV1 {
    Empty,
    WidthExceeded {
        width: usize,
    },
    LengthMismatch {
        expected: usize,
        found: usize,
    },
    NonFiniteLogit {
        index: usize,
    },
    /// A mean return outside `[-1, 1]` (or non-finite).
    MeanReturn {
        index: usize,
    },
    /// A rollout return other than a natural terminal value in `{-1, 0, 1}`.
    NaturalReturn {
        index: usize,
        value: i8,
    },
    /// `T` not finite and positive, or a shifted logit overflowed.
    Temperature,
    FloatingPointEnvironment {
        mxcsr: u32,
    },
}

impl fmt::Display for LineBTeacherErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "line (b) teacher menus must be nonempty"),
            Self::WidthExceeded { width } => write!(
                formatter,
                "line (b) teacher menu width {width} exceeds {UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1}"
            ),
            Self::LengthMismatch { expected, found } => write!(
                formatter,
                "line (b) teacher input has {found} entries where the menu has {expected}"
            ),
            Self::NonFiniteLogit { index } => {
                write!(formatter, "line (b) teacher logit {index} is non-finite")
            }
            Self::MeanReturn { index } => write!(
                formatter,
                "line (b) teacher mean return {index} is outside [-1, 1]"
            ),
            Self::NaturalReturn { index, value } => write!(
                formatter,
                "line (b) rollout return {index} is {value}, not a natural terminal value"
            ),
            Self::Temperature => write!(
                formatter,
                "line (b) teacher temperature must be finite and positive with finite shifts"
            ),
            Self::FloatingPointEnvironment { mxcsr } => write!(
                formatter,
                "line (b) teacher arithmetic requires pinned MXCSR (found 0x{mxcsr:08x})"
            ),
        }
    }
}

impl std::error::Error for LineBTeacherErrorV1 {}

/// A binary64 softmax over one legal menu, in log and linear form.
#[derive(Clone, Debug, PartialEq)]
pub struct LineBSoftmaxV1 {
    pub log_probabilities: Vec<f64>,
    pub probabilities: Vec<f64>,
    /// `S`: the weights summed from `+0.0` in legal-action order.
    pub weight_sum: f64,
}

/// One root's divergence and its gradient with respect to the student logits
/// (before `c` and the selected-root mean).
#[derive(Clone, Debug, PartialEq)]
pub struct LineBDivergenceValueV1 {
    pub divergence: f64,
    pub logit_gradient: Vec<f64>,
}

/// Libm-free natural logarithm for finite `x >= 1` (sums of softmax weights):
/// `x = m * 2^e` with `m` in `[sqrt(1/2), sqrt 2]` and `ln m = 2 atanh((m -
/// 1) / (m + 1))`. `m - 1` is exact; the series is truncated after `t^23/23`.
pub(crate) fn ln_v1(x: f64) -> f64 {
    debug_assert!(x.is_finite() && x >= 1.0);
    let bits = x.to_bits();
    let mut exponent = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let mut mantissa = f64::from_bits((bits & ((1_u64 << 52) - 1)) | (1023_u64 << 52));
    if mantissa > SQRT2_V1 {
        mantissa *= 0.5;
        exponent += 1;
    }
    let t = (mantissa - 1.0) / (mantissa + 1.0);
    let s = t * t;
    let mut series = ATANH_ODD_V1[11];
    for coefficient in ATANH_ODD_V1[..11].iter().rev() {
        series = series * s + coefficient;
    }
    let e = exponent as f64;
    e * LN2_HI_V1 + (e * LN2_LO_V1 + 2.0 * (t * series))
}

fn check_menu_v1(values: &[f64]) -> Result<(), LineBTeacherErrorV1> {
    verify_pinned_mxcsr_state_v1().map_err(|error| {
        LineBTeacherErrorV1::FloatingPointEnvironment {
            mxcsr: error.observed_v1(),
        }
    })?;
    if values.is_empty() {
        return Err(LineBTeacherErrorV1::Empty);
    }
    if values.len() > UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 {
        return Err(LineBTeacherErrorV1::WidthExceeded {
            width: values.len(),
        });
    }
    match values.iter().position(|value| !value.is_finite()) {
        Some(index) => Err(LineBTeacherErrorV1::NonFiniteLogit { index }),
        None => Ok(()),
    }
}

fn check_length_v1(expected: usize, found: usize) -> Result<(), LineBTeacherErrorV1> {
    if expected == found {
        Ok(())
    } else {
        Err(LineBTeacherErrorV1::LengthMismatch { expected, found })
    }
}

/// Callers guarantee a checked menu of finite values.
fn softmax_unchecked_v1(values: &[f64]) -> LineBSoftmaxV1 {
    let mut maximum = values[0];
    for &value in values {
        if value > maximum {
            maximum = value;
        }
    }
    let mut gaps = Vec::with_capacity(values.len());
    let mut weights = Vec::with_capacity(values.len());
    let mut total = 0.0_f64;
    for &value in values {
        let gap = value - maximum;
        let weight = if gap >= UNCLAMPED_SOFTMAX_GAP_FLOOR_V1 {
            exp_v1(gap)
        } else {
            0.0
        };
        total += weight;
        gaps.push(gap);
        weights.push(weight);
    }
    // The first maximum has weight exactly 1, so total >= 1.
    let log_total = ln_v1(total);
    LineBSoftmaxV1 {
        log_probabilities: gaps.iter().map(|gap| gap - log_total).collect(),
        probabilities: weights.iter().map(|weight| weight / total).collect(),
        weight_sum: total,
    }
}

/// The binary64 softmax of one legal menu (the collection policy `p_u` when
/// given the recorded collection logits).
pub fn line_b_softmax_v1(logits: &[f64]) -> Result<LineBSoftmaxV1, LineBTeacherErrorV1> {
    check_menu_v1(logits)?;
    Ok(softmax_unchecked_v1(logits))
}

/// `Qhat`: the mean of an action's natural terminal returns in `{-1, 0, 1}`,
/// exact for K = 16.
pub fn line_b_mean_return_v1(returns: &[i8]) -> Result<f64, LineBTeacherErrorV1> {
    if returns.is_empty() {
        return Err(LineBTeacherErrorV1::Empty);
    }
    let mut total = 0_i64;
    for (index, &value) in returns.iter().enumerate() {
        if !(-1..=1).contains(&value) {
            return Err(LineBTeacherErrorV1::NaturalReturn { index, value });
        }
        total += i64::from(value);
    }
    Ok(total as f64 / returns.len() as f64)
}

/// The frozen teacher target `q` for one root from its recorded collection
/// logits and per-action mean returns.
pub fn line_b_teacher_target_v1(
    collection_logits: &[f64],
    mean_returns: &[f64],
    temperature: f64,
) -> Result<LineBSoftmaxV1, LineBTeacherErrorV1> {
    check_menu_v1(collection_logits)?;
    check_length_v1(collection_logits.len(), mean_returns.len())?;
    if !(temperature.is_finite() && temperature > 0.0) {
        return Err(LineBTeacherErrorV1::Temperature);
    }
    let mut maximum = mean_returns[0];
    for (index, &value) in mean_returns.iter().enumerate() {
        if !(-1.0..=1.0).contains(&value) {
            return Err(LineBTeacherErrorV1::MeanReturn { index });
        }
        if value > maximum {
            maximum = value;
        }
    }
    let mut shifted = Vec::with_capacity(collection_logits.len());
    for (&logit, &value) in collection_logits.iter().zip(mean_returns) {
        let shift = (value - maximum) / temperature;
        // Adding +0.0 would turn a -0.0 logit into +0.0; skipping zero shifts
        // keeps equal-return targets bitwise equal to the collection softmax.
        let logit = if shift == 0.0 { logit } else { logit + shift };
        if !logit.is_finite() {
            return Err(LineBTeacherErrorV1::Temperature);
        }
        shifted.push(logit);
    }
    Ok(softmax_unchecked_v1(&shifted))
}

/// One root's divergence between the student softmax of `student_logits` and
/// the frozen `target`, with its student-logit gradient.
pub fn line_b_divergence_v1(
    direction: LineBDivergenceV1,
    student_logits: &[f64],
    target: &LineBSoftmaxV1,
) -> Result<LineBDivergenceValueV1, LineBTeacherErrorV1> {
    check_menu_v1(student_logits)?;
    check_length_v1(student_logits.len(), target.log_probabilities.len())?;
    check_length_v1(student_logits.len(), target.probabilities.len())?;
    let student = softmax_unchecked_v1(student_logits);
    Ok(divergence_unchecked_v1(direction, &student, target))
}

fn divergence_unchecked_v1(
    direction: LineBDivergenceV1,
    student: &LineBSoftmaxV1,
    target: &LineBSoftmaxV1,
) -> LineBDivergenceValueV1 {
    let (p, log_p) = (&student.probabilities, &student.log_probabilities);
    let (q, log_q) = (&target.probabilities, &target.log_probabilities);
    let mut divergence = 0.0_f64;
    let logit_gradient = match direction {
        LineBDivergenceV1::Reverse => {
            for index in 0..p.len() {
                if p[index] > 0.0 {
                    divergence += p[index] * (log_p[index] - log_q[index]);
                }
            }
            (0..p.len())
                .map(|index| {
                    if p[index] > 0.0 {
                        p[index] * ((log_p[index] - log_q[index]) - divergence)
                    } else {
                        0.0
                    }
                })
                .collect()
        }
        LineBDivergenceV1::Forward => {
            for index in 0..q.len() {
                if q[index] > 0.0 {
                    divergence += q[index] * (log_q[index] - log_p[index]);
                }
            }
            p.iter().zip(q).map(|(p, q)| p - q).collect()
        }
    };
    LineBDivergenceValueV1 {
        divergence,
        logit_gradient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SplitMix64;
    use crate::unclamped_softmax_sampler_v1::UnclampedSoftmaxScratchV1;
    use sha2::{Digest, Sha256};

    const U: f64 = 1.0 / 9_007_199_254_740_992.0;
    /// `python/tools/line_b_teacher_target_v1_replica.py` prints the same.
    const GOLDEN_BATTERY_SHA256: &str =
        "90d9e4639093111dc876079d37cdd2d02ea8476003cc1a3e31d038091192a38f";

    struct Case {
        logits: Vec<f64>,
        returns: Vec<Vec<i8>>,
        student: Vec<f64>,
    }

    /// Mirrors `golden_battery()` in the replica: widths 2..8 plus 64 and
    /// 300, some `-0.0` logits, a -800 logit below the tail floor every
    /// sixteenth case, equal returns every sixteenth case and the student at
    /// the collection logits every eighth case.
    fn golden_battery() -> Vec<Case> {
        let mut values = SplitMix64::seed(0x6C69_6E65_2D62_2D71);
        let mut cases = Vec::new();
        for case in 0..256_usize {
            let width = match case {
                c if c % 64 == 63 => 300,
                c if c % 32 == 31 => 64,
                c => 2 + c % 7,
            };
            let mut logits: Vec<f64> = (0..width)
                .map(|_| {
                    let v = values.next_u64();
                    if v % 13 == 0 {
                        -0.0
                    } else {
                        ((((v >> 11) % 4001) as i64 - 2000) as f64 / 250.0) as f32 as f64
                    }
                })
                .collect();
            if case % 16 == 9 {
                logits[width - 1] = -800.0;
            }
            let mut returns: Vec<Vec<i8>> = Vec::with_capacity(width);
            for _ in 0..width {
                if case % 16 == 5 && !returns.is_empty() {
                    returns.push(returns[0].clone());
                    continue;
                }
                returns.push(
                    (0..LINE_B_TEACHER_ROLLOUTS_V1)
                        .map(|_| (values.next_u64() % 3) as i8 - 1)
                        .collect(),
                );
            }
            let student = if case % 8 == 0 {
                logits.clone()
            } else {
                logits
                    .iter()
                    .map(|&z| {
                        let v = values.next_u64();
                        (z + ((v % 201) as i64 - 100) as f64 / 1000.0) as f32 as f64
                    })
                    .collect()
            };
            cases.push(Case {
                logits,
                returns,
                student,
            });
        }
        cases
    }

    fn mean_returns(case: &Case) -> Vec<f64> {
        case.returns
            .iter()
            .map(|row| line_b_mean_return_v1(row).unwrap())
            .collect()
    }

    fn target(case: &Case) -> LineBSoftmaxV1 {
        line_b_teacher_target_v1(
            &case.logits,
            &mean_returns(case),
            LINE_B_TEACHER_TEMPERATURE_V1,
        )
        .unwrap()
    }

    fn value(direction: LineBDivergenceV1, logits: &[f64], goal: &LineBSoftmaxV1) -> f64 {
        line_b_divergence_v1(direction, logits, goal)
            .unwrap()
            .divergence
    }

    /// Fourth-order central difference at `h = 1e-3`: a plain central
    /// difference at `h = 1e-6` carries about 2.5e-10 of rounding noise, above
    /// the proposal's 3e-11 formula check.
    fn fourth_order(
        direction: LineBDivergenceV1,
        logits: &[f64],
        goal: &LineBSoftmaxV1,
        index: usize,
    ) -> f64 {
        let h = 1e-3;
        let at = |delta: f64| {
            let mut moved = logits.to_vec();
            moved[index] = logits[index] + delta;
            value(direction, &moved, goal)
        };
        (-at(2.0 * h) + 8.0 * at(h) - 8.0 * at(-h) + at(-2.0 * h)) / (12.0 * h)
    }

    #[test]
    fn constants_are_the_declared_values() {
        assert_eq!(LN2_HI_V1 + LN2_LO_V1, core::f64::consts::LN_2);
        assert_eq!(SQRT2_V1, core::f64::consts::SQRT_2);
        for (k, coefficient) in ATANH_ODD_V1.iter().enumerate() {
            assert_eq!(*coefficient, 1.0 / (2 * k + 1) as f64, "coefficient {k}");
        }
        assert_eq!(LINE_B_TEACHER_TEMPERATURE_V1, 0.25);
        assert_eq!(LINE_B_TEACHER_COEFFICIENT_V1, 0.1);
        assert_eq!(LINE_B_TEACHER_ROLLOUTS_V1, 16);
        assert_eq!(LineBDivergenceV1::default(), LineBDivergenceV1::Reverse);
        assert_eq!(
            serde_json::to_value(LineBDivergenceV1::Reverse).unwrap(),
            "reverse-kl"
        );
        assert_eq!(
            serde_json::to_value(LineBDivergenceV1::Forward).unwrap(),
            "forward-kl"
        );
    }

    #[test]
    fn ln_v1_is_exact_at_one_and_within_three_ulp_of_platform_ln() {
        assert_eq!(ln_v1(1.0).to_bits(), 0.0_f64.to_bits());
        let mut points: Vec<f64> = (0..=4096).map(|i| 1.0 + i as f64 / 4096.0).collect();
        points.extend((0..=64 * 16).map(|i| 2.0_f64.powf(i as f64 / 64.0)));
        points.extend((1..65_537).step_by(97).map(|n| n as f64));
        points.extend([
            core::f64::consts::SQRT_2,
            f64::from_bits(core::f64::consts::SQRT_2.to_bits() - 1),
            f64::from_bits(core::f64::consts::SQRT_2.to_bits() + 1),
            65_536.0,
        ]);
        let mut worst = 0_u64;
        for &x in &points {
            let distance = ln_v1(x).to_bits().abs_diff(x.ln().to_bits());
            assert!(
                distance <= 3,
                "ln_v1({x}) is {distance} ulp from platform ln"
            );
            worst = worst.max(distance);
        }
        eprintln!(
            "ln_v1 vs platform ln: max {worst} ulp over {} points",
            points.len()
        );
    }

    #[test]
    fn proposal_formula_checks_hold() {
        let p = [0.2_f64, 0.3, 0.5];
        let z: Vec<f64> = p.iter().map(|v| v.ln()).collect();
        let goal = line_b_teacher_target_v1(&z, &[1.0, 0.0, -1.0], 0.25).unwrap();
        let mut total = 0.0;
        for q in &goal.probabilities {
            total += q;
        }
        assert!(
            (total - 1.0).abs() <= 4.0 * U,
            "sum q - 1 = {}",
            total - 1.0
        );
        for direction in [LineBDivergenceV1::Reverse, LineBDivergenceV1::Forward] {
            let gradient = line_b_divergence_v1(direction, &z, &goal)
                .unwrap()
                .logit_gradient;
            for index in 0..3 {
                let estimate = fourth_order(direction, &z, &goal, index);
                assert!(
                    (estimate - gradient[index]).abs() < 3e-11,
                    "{direction:?} {index}: {estimate} vs {}",
                    gradient[index]
                );
            }
        }
        let same = line_b_teacher_target_v1(&z, &[0.5, 0.5, 0.5], 0.25).unwrap();
        for (q, p) in same.probabilities.iter().zip(p) {
            assert!((q - p).abs() <= 1.2e-16, "equal returns moved {p} to {q}");
        }
    }

    #[test]
    fn equal_returns_repeat_the_collection_softmax_bitwise() {
        let mut checked = 0;
        for case in golden_battery() {
            let means = mean_returns(&case);
            if means.iter().any(|m| m.to_bits() != means[0].to_bits()) {
                continue;
            }
            checked += 1;
            let goal = target(&case);
            let collection = line_b_softmax_v1(&case.logits).unwrap();
            for (a, b) in goal
                .log_probabilities
                .iter()
                .chain(&goal.probabilities)
                .zip(
                    collection
                        .log_probabilities
                        .iter()
                        .chain(&collection.probabilities),
                )
            {
                assert_eq!(a.to_bits(), b.to_bits());
            }
            for direction in [LineBDivergenceV1::Reverse, LineBDivergenceV1::Forward] {
                let at_collection = line_b_divergence_v1(direction, &case.logits, &goal).unwrap();
                assert_eq!(at_collection.divergence, 0.0);
                assert!(at_collection.logit_gradient.iter().all(|g| *g == 0.0));
            }
        }
        assert!(checked >= 16, "the battery must contain equal-return roots");
        for logits in [[-0.0, 0.0, -3.0], [0.0, -0.0, -800.0]] {
            let goal = line_b_teacher_target_v1(&logits, &[0.25; 3], 0.25).unwrap();
            let collection = line_b_softmax_v1(&logits).unwrap();
            for (a, b) in goal
                .log_probabilities
                .iter()
                .chain(&goal.probabilities)
                .zip(
                    collection
                        .log_probabilities
                        .iter()
                        .chain(&collection.probabilities),
                )
            {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }
    }

    #[test]
    fn a1_reverse_gradient_at_the_collection_policy_is_the_advantage_identity() {
        let mut checked = 0;
        for case in golden_battery().iter().filter(|c| c.student == c.logits) {
            let means = mean_returns(case);
            let goal = target(case);
            let p = line_b_softmax_v1(&case.logits).unwrap().probabilities;
            let mut expected = 0.0;
            for (p, q) in p.iter().zip(&means) {
                expected += p * q;
            }
            let gradient = line_b_divergence_v1(LineBDivergenceV1::Reverse, &case.logits, &goal)
                .unwrap()
                .logit_gradient;
            for index in 0..p.len() {
                let identity =
                    -(p[index] / LINE_B_TEACHER_TEMPERATURE_V1) * (means[index] - expected);
                assert!(
                    (gradient[index] - identity).abs() <= 1e-15,
                    "action {index}: {} vs {identity}",
                    gradient[index]
                );
            }
            checked += 1;
        }
        assert!(checked >= 32);
    }

    #[test]
    fn a2_forward_gradient_is_p_minus_q_and_both_match_finite_differences() {
        for case in golden_battery().iter().filter(|c| c.logits.len() <= 8) {
            let goal = target(case);
            let student = line_b_softmax_v1(&case.student).unwrap();
            let forward = line_b_divergence_v1(LineBDivergenceV1::Forward, &case.student, &goal)
                .unwrap()
                .logit_gradient;
            for index in 0..forward.len() {
                let difference = student.probabilities[index] - goal.probabilities[index];
                assert_eq!(forward[index].to_bits(), difference.to_bits());
            }
            for direction in [LineBDivergenceV1::Reverse, LineBDivergenceV1::Forward] {
                let gradient = line_b_divergence_v1(direction, &case.student, &goal)
                    .unwrap()
                    .logit_gradient;
                for index in 0..gradient.len() {
                    let estimate = fourth_order(direction, &case.student, &goal, index);
                    assert!(
                        (estimate - gradient[index]).abs() < 3e-11,
                        "{direction:?} {index}: {estimate} vs {}",
                        gradient[index]
                    );
                }
            }
        }
    }

    #[test]
    fn both_gradients_sum_to_zero_up_to_rounding() {
        for case in golden_battery() {
            let goal = target(&case);
            let n = case.logits.len() as f64;
            for direction in [LineBDivergenceV1::Reverse, LineBDivergenceV1::Forward] {
                let result = line_b_divergence_v1(direction, &case.student, &goal).unwrap();
                let (mut total, mut magnitude) = (0.0_f64, 0.0_f64);
                for g in &result.logit_gradient {
                    total += g;
                    magnitude += g.abs();
                }
                // Reverse: sum = KL (1 - sum p) plus rounding of the terms.
                // Forward: sum = sum p - sum q, each within about n u of one.
                let bound = match direction {
                    LineBDivergenceV1::Reverse => {
                        4.0 * U * (magnitude + n * result.divergence.abs())
                    }
                    LineBDivergenceV1::Forward => 4.0 * n * U,
                };
                assert!(total.abs() <= bound, "{direction:?}: {total} > {bound}");
            }
        }
    }

    #[test]
    fn tail_rule_and_weight_sum_match_the_collection_sampler() {
        for case in golden_battery() {
            let logits: Vec<f32> = case.logits.iter().map(|&z| z as f32).collect();
            assert!(logits
                .iter()
                .zip(&case.logits)
                .all(|(a, b)| f64::from(*a) == *b));
            let mut scratch = UnclampedSoftmaxScratchV1::default();
            let prefix = scratch.prefix_sums_v1(&logits).unwrap().to_vec();
            let softmax = line_b_softmax_v1(&case.logits).unwrap();
            let maximum = case
                .logits
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            for (index, z) in case.logits.iter().enumerate() {
                let below_floor = z - maximum < UNCLAMPED_SOFTMAX_GAP_FLOOR_V1;
                assert_eq!(softmax.probabilities[index] == 0.0, below_floor);
                assert!(softmax.log_probabilities[index].is_finite());
            }
            // Same weights summed in the same order: S is the sampler's total.
            assert_eq!(
                softmax.weight_sum.to_bits(),
                prefix.last().unwrap().to_bits()
            );
        }
        let tail = line_b_softmax_v1(&[0.0, -708.0, -708.5]).unwrap();
        assert!(tail.probabilities[1] > 0.0);
        assert_eq!(tail.probabilities[2].to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn invalid_inputs_fail_closed() {
        use LineBTeacherErrorV1 as E;
        let goal = line_b_softmax_v1(&[0.0, 1.0]).unwrap();
        assert_eq!(line_b_softmax_v1(&[]), Err(E::Empty));
        assert_eq!(
            line_b_softmax_v1(&vec![0.0; UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 + 1]),
            Err(E::WidthExceeded {
                width: UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 + 1
            })
        );
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                line_b_softmax_v1(&[0.0, bad]),
                Err(E::NonFiniteLogit { index: 1 })
            );
            assert_eq!(
                line_b_divergence_v1(LineBDivergenceV1::Forward, &[bad, 0.0], &goal),
                Err(E::NonFiniteLogit { index: 0 })
            );
            assert_eq!(
                line_b_teacher_target_v1(&[0.0, 0.0], &[0.0, bad], 0.25),
                Err(E::MeanReturn { index: 1 })
            );
        }
        assert_eq!(
            line_b_teacher_target_v1(&[0.0, 0.0], &[0.0, 1.0625], 0.25),
            Err(E::MeanReturn { index: 1 })
        );
        assert_eq!(
            line_b_teacher_target_v1(&[0.0, 0.0], &[0.0], 0.25),
            Err(E::LengthMismatch {
                expected: 2,
                found: 1
            })
        );
        for temperature in [0.0, -0.25, f64::NAN, f64::INFINITY] {
            assert_eq!(
                line_b_teacher_target_v1(&[0.0, 0.0], &[0.0, 1.0], temperature),
                Err(E::Temperature)
            );
        }
        assert_eq!(
            line_b_teacher_target_v1(&[0.0, 0.0], &[1.0, -1.0], 1.0e-308),
            Err(E::Temperature)
        );
        assert_eq!(
            line_b_divergence_v1(LineBDivergenceV1::Reverse, &[0.0, 0.0, 0.0], &goal),
            Err(E::LengthMismatch {
                expected: 3,
                found: 2
            })
        );
        assert_eq!(line_b_mean_return_v1(&[]), Err(E::Empty));
        assert_eq!(
            line_b_mean_return_v1(&[1, 0, 2]),
            Err(E::NaturalReturn { index: 2, value: 2 })
        );
        assert_eq!(line_b_mean_return_v1(&[1; 16]), Ok(1.0));
        assert_eq!(line_b_mean_return_v1(&[1, -1, 0, 1]), Ok(0.25));
    }

    #[test]
    fn golden_battery_is_pinned_and_reproduced_by_the_replica() {
        let mut digest = Sha256::new();
        for case in golden_battery() {
            let means = mean_returns(&case);
            let goal = target(&case);
            digest.update((case.logits.len() as u32).to_le_bytes());
            for z in &case.logits {
                digest.update(z.to_le_bytes());
            }
            for row in &case.returns {
                digest.update(row.iter().map(|r| *r as u8).collect::<Vec<u8>>());
            }
            for m in &means {
                digest.update(m.to_le_bytes());
            }
            for v in goal.log_probabilities.iter().chain(&goal.probabilities) {
                digest.update(v.to_le_bytes());
            }
            for z in &case.student {
                digest.update(z.to_le_bytes());
            }
            for direction in [LineBDivergenceV1::Reverse, LineBDivergenceV1::Forward] {
                let result = line_b_divergence_v1(direction, &case.student, &goal).unwrap();
                digest.update(result.divergence.to_le_bytes());
                for g in &result.logit_gradient {
                    digest.update(g.to_le_bytes());
                }
            }
        }
        assert_eq!(format!("{:x}", digest.finalize()), GOLDEN_BATTERY_SHA256);
    }
}
