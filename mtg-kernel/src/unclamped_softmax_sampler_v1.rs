//! Unclamped temperature-1 softmax collection sampler for g115 line (b)
//! (CODEX-G115-EXIT-PROPOSAL-20260927.md v0.2, "common trainable mask and
//! sampling"; contract dispositions of 2026-09-27 11:23, item 1).
//!
//! The legacy samplers in `fast_sampler` quantize logit gaps to Q8 and clamp
//! them at 16 nats, so an unclamped-softmax loss is not on-policy for them.
//! This sampler is a finite-precision binary64 inverse CDF whose selection
//! law stays within a declared envelope of the unclamped softmax. It is a new
//! identity: it never reinterprets artifacts of
//! `f32-q8-expq63-hamilton-splitmix64-v1` or its wide successor, and
//! `fast_sampler` is unchanged.
//!
//! The canonical contract is [`UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_JSON_V1`]
//! (SHA-256 pinned). In short: binary64 gaps from the first maximal logit, a
//! libm-free `exp` for gaps at or above -708 (weight `+0.0` below), sequential
//! prefix sums in legal-action order, a 53-bit uniform from ONE `next_u64` of
//! the acting seat's continuing SplitMix64 stream (no reseeding), and the
//! smallest legal index whose prefix sum exceeds `u * S`.
//!
//! Envelope: for every legal index `i` of a width-`n` menu (`1 <= n <=
//! 65,536`, finite binary32 logits), the selection probability `P(i)` over
//! the `2**53` draw grid satisfies `|P(i) - softmax_i| <= (4n + 16) * 2**-53`
//! against the real-valued softmax of the same binary32 logits. The
//! derivation in `docs/line_b_teacher_v1.md` gives `(11n/8 + 10) * 2**-53 +
//! n * e**-708`, accounting for the gap subtraction, `exp_v1` (relative error
//! at most `4 * 2**-53`), sequential summation, the threshold product, the
//! boundary comparisons and the draw grid. Gaps below -708 get zero mass; the
//! total true mass so dropped is below `n * e**-708 < 2.2e-303`.
//!
//! Every admitted call verifies the calling thread's pinned MXCSR
//! (round-to-nearest-even, FTZ=0, DAZ=0) and fails closed otherwise, because
//! the arithmetic is binary64 rather than integer.

use core::fmt;

use crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1;

pub const UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1: &str = "unclamped-softmax-f64-icdf-u53-v1";
/// Resource bound shared with the wide legacy sampler, not a claim about the
/// largest possible engine menu.
pub const UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1: usize = 65_536;
/// Gaps strictly below this floor receive weight `+0.0`.
pub const UNCLAMPED_SOFTMAX_GAP_FLOOR_V1: f64 = -708.0;

/// Canonical UTF-8 contract bytes (sorted keys). Its SHA-256 is pinned below.
pub const UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_JSON_V1: &str = r#"{"domain":"1..65536 finite IEEE-754 binary32 logits in legal-action order","draw":"x = the acting seat stream's next splitmix64-v1 uint64 output, exactly one per sampled decision, never reseeded; u = (x >> 11) * 2**-53; t = u * S in binary64","envelope":"|P(i) - softmax_i| <= (4n+16) * 2**-53 for every legal index i of a width-n menu, P = selection probability over the 2**53 draw grid, softmax over the real-valued binary32 logits; derived bound (11n/8 + 10) * 2**-53 + n * e**-708","exp":"exp_v1(d) for d in [-708, 0], hex values are binary64 bit patterns: k = (d * 0x3ff71547652b82fe + 0x4338000000000000) - 0x4338000000000000; r = (d - k * 0x3fe62e42fee00000) - k * 0x3dea39ef35793c76; e^r by Horner over c_j = RN(1/j!) for j = 13 down to 0; result = e^r * 2**k by exact exponent-field construction; no platform math library; relative error <= 4 * 2**-53","floating_point":"binary64 round-to-nearest-even without fused multiply-add; MXCSR FTZ=0 and DAZ=0 verified on every call, fail closed","gap":"d_i = binary64(z_i) - binary64(z_max), z_max = first maximal logit in legal-action order","sampler_version":"unclamped-softmax-f64-icdf-u53-v1","selection":"smallest legal index i with C_i > t; t < S for every draw, so a selection always exists (checked, fail closed); a zero-weight action is never selected","sums":"C_i = C_(i-1) + w_i sequentially in legal-action order from +0.0; S = C_(n-1)","tail":"w_i = +0.0 for d_i < -708; total true softmax mass so dropped < n * e**-708 < 2.2e-303","weights":"w_i = exp_v1(d_i) for d_i >= -708; every maximal logit has weight exactly 1"}"#;

/// SHA-256 over `UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_JSON_V1.as_bytes()`.
pub const UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_SHA256_V1: &str =
    "61e91239ba9e68bee0183a4a33208794f396ac0cd4084569383eaa0b65862b00";

const TWO_POW_NEG_53: f64 = 1.0 / 9_007_199_254_740_992.0;
/// `RN(1/ln 2)`, identical to `core::f64::consts::LOG2_E`.
const INV_LN2_V1: f64 = f64::from_bits(0x3ff7_1547_652b_82fe);
/// fdlibm's two-part `ln 2`: the high part has 21 trailing zero significand
/// bits, so `k * LN2_HI_V1` is exact for every `|k| <= 1022` used here.
const LN2_HI_V1: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN2_LO_V1: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
/// `1.5 * 2**52`: adding and subtracting it rounds to the nearest integer
/// (ties to even) using only IEEE addition.
const ROUND_SHIFTER_V1: f64 = f64::from_bits(0x4338_0000_0000_0000);

/// `RN(1/j!)` for `j = 0..=13`. Each `j!` is exact in binary64, so each entry
/// is one correctly rounded division evaluated at compile time.
const EXP_TAYLOR_V1: [f64; 14] = [
    1.0,
    1.0,
    1.0 / 2.0,
    1.0 / 6.0,
    1.0 / 24.0,
    1.0 / 120.0,
    1.0 / 720.0,
    1.0 / 5_040.0,
    1.0 / 40_320.0,
    1.0 / 362_880.0,
    1.0 / 3_628_800.0,
    1.0 / 39_916_800.0,
    1.0 / 479_001_600.0,
    1.0 / 6_227_020_800.0,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnclampedSoftmaxSamplerErrorV1 {
    Empty,
    WidthExceeded { width: usize, maximum: usize },
    NonFinite { index: usize, bits: u32 },
    FloatingPointEnvironment { mxcsr: u32 },
    InternalInvariant { code: &'static str },
}

impl fmt::Display for UnclampedSoftmaxSamplerErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(
                formatter,
                "unclamped softmax sampling requires at least one action"
            ),
            Self::WidthExceeded { width, maximum } => write!(
                formatter,
                "unclamped softmax action width {width} exceeds fail-closed maximum {maximum}"
            ),
            Self::NonFinite { index, bits } => write!(
                formatter,
                "unclamped softmax logit at legal index {index} is non-finite (bits=0x{bits:08x})"
            ),
            Self::FloatingPointEnvironment { mxcsr } => write!(
                formatter,
                "unclamped softmax sampling requires pinned MXCSR (found 0x{mxcsr:08x})"
            ),
            Self::InternalInvariant { code } => {
                write!(
                    formatter,
                    "unclamped softmax internal invariant failed: {code}"
                )
            }
        }
    }
}

impl std::error::Error for UnclampedSoftmaxSamplerErrorV1 {}

/// Libm-free `e^d` for `d` in `[UNCLAMPED_SOFTMAX_GAP_FLOOR_V1, 0]`, with the
/// operation order the contract names. Callers guarantee the domain.
/// Shared with the line (b) teacher target.
pub(crate) fn exp_v1(d: f64) -> f64 {
    let k_float = (d * INV_LN2_V1 + ROUND_SHIFTER_V1) - ROUND_SHIFTER_V1;
    let r = (d - k_float * LN2_HI_V1) - k_float * LN2_LO_V1;
    let mut value = EXP_TAYLOR_V1[13];
    for coefficient in EXP_TAYLOR_V1[..13].iter().rev() {
        value = value * r + coefficient;
    }
    // k is an integer in -1021..=0 on the admitted domain, so the exponent
    // field below is normal and the product is an exact power-of-two scaling.
    let k = k_float as i64;
    value * f64::from_bits(((k + 1023) as u64) << 52)
}

/// The contract's absolute envelope for a menu of `width` legal actions.
pub fn unclamped_softmax_envelope_v1(width: usize) -> f64 {
    (4.0 * width as f64 + 16.0) * TWO_POW_NEG_53
}

/// The 53-bit uniform `u = (draw >> 11) * 2**-53` for one seat-stream
/// output `draw`. Exact in binary64.
pub fn unclamped_softmax_unit_v1(draw: u64) -> f64 {
    (draw >> 11) as f64 * TWO_POW_NEG_53
}

/// Selects the smallest legal index whose prefix sum exceeds `u * S`.
/// `prefix` must come from [`UnclampedSoftmaxScratchV1::prefix_sums_v1`] and
/// `u` must lie on the draw grid in `[0, 1)`.
pub fn unclamped_softmax_select_v1(
    prefix: &[f64],
    u: f64,
) -> Result<usize, UnclampedSoftmaxSamplerErrorV1> {
    let total = *prefix
        .last()
        .ok_or(UnclampedSoftmaxSamplerErrorV1::InternalInvariant {
            code: "unclamped-softmax-empty-prefix",
        })?;
    let threshold = u * total;
    let index = prefix.partition_point(|&cumulative| cumulative <= threshold);
    if index < prefix.len() {
        Ok(index)
    } else {
        Err(UnclampedSoftmaxSamplerErrorV1::InternalInvariant {
            code: "unclamped-softmax-threshold-not-below-total",
        })
    }
}

/// Reusable heap scratch; keep one per worker.
#[derive(Clone, Debug, Default)]
pub struct UnclampedSoftmaxScratchV1 {
    prefix: Vec<f64>,
}

impl UnclampedSoftmaxScratchV1 {
    /// Validate, weight and sum `logits`; returns the prefix sums `C_i`.
    pub fn prefix_sums_v1(
        &mut self,
        logits: &[f32],
    ) -> Result<&[f64], UnclampedSoftmaxSamplerErrorV1> {
        verify_pinned_mxcsr_state_v1().map_err(|error| {
            UnclampedSoftmaxSamplerErrorV1::FloatingPointEnvironment {
                mxcsr: error.observed_v1(),
            }
        })?;
        let width = logits.len();
        if width == 0 {
            return Err(UnclampedSoftmaxSamplerErrorV1::Empty);
        }
        if width > UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 {
            return Err(UnclampedSoftmaxSamplerErrorV1::WidthExceeded {
                width,
                maximum: UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1,
            });
        }
        let mut maximum = logits[0];
        for (index, &logit) in logits.iter().enumerate() {
            if !logit.is_finite() {
                return Err(UnclampedSoftmaxSamplerErrorV1::NonFinite {
                    index,
                    bits: logit.to_bits(),
                });
            }
            if logit > maximum {
                maximum = logit;
            }
        }
        let maximum = f64::from(maximum);
        self.prefix.clear();
        if self.prefix.capacity() < width {
            self.prefix.try_reserve_exact(width).map_err(|_| {
                UnclampedSoftmaxSamplerErrorV1::InternalInvariant {
                    code: "unclamped-softmax-scratch-allocation",
                }
            })?;
        }
        let mut cumulative = 0.0_f64;
        for &logit in logits {
            let gap = f64::from(logit) - maximum;
            let weight = if gap >= UNCLAMPED_SOFTMAX_GAP_FLOOR_V1 {
                exp_v1(gap)
            } else {
                0.0
            };
            cumulative += weight;
            self.prefix.push(cumulative);
        }
        Ok(&self.prefix)
    }

    /// Sample one legal index. `draw` must be the acting seat stream's next
    /// `u64`, taken exactly once for this decision; nothing else is consumed.
    pub fn sample(
        &mut self,
        logits: &[f32],
        draw: u64,
    ) -> Result<usize, UnclampedSoftmaxSamplerErrorV1> {
        let u = unclamped_softmax_unit_v1(draw);
        let prefix = self.prefix_sums_v1(logits)?;
        unclamped_softmax_select_v1(prefix, u)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fast_sampler::{FastCategoricalScratch, FAST_CATEGORICAL_MASS_TOTAL};
    use crate::state::SplitMix64;
    use sha2::{Digest, Sha256};

    const GRID: u64 = 1 << 53;

    /// Selection counts over the 2**53 draw grid. Selection is monotone in the
    /// grid index, so each action's draws form one interval whose end is found
    /// by binary search.
    fn grid_counts(logits: &[f32]) -> Vec<u64> {
        let mut scratch = UnclampedSoftmaxScratchV1::default();
        let prefix = scratch.prefix_sums_v1(logits).unwrap().to_vec();
        let select =
            |g: u64| unclamped_softmax_select_v1(&prefix, g as f64 * TWO_POW_NEG_53).unwrap();
        let mut counts = Vec::with_capacity(logits.len());
        let mut start = 0_u64;
        for action in 0..logits.len() {
            let (mut low, mut high) = (start, GRID);
            while low < high {
                let middle = low + (high - low) / 2;
                if select(middle) > action {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            counts.push(low - start);
            start = low;
        }
        assert_eq!(start, GRID, "every grid point selects exactly one action");
        counts
    }

    /// Reference softmax: platform `exp` plus Neumaier summation in binary64.
    /// Its absolute error is below 8 * 2**-53, charged separately below.
    fn reference_softmax(logits: &[f32]) -> Vec<f64> {
        let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let weights: Vec<f64> = logits
            .iter()
            .map(|&z| (f64::from(z) - maximum).exp())
            .collect();
        let (mut sum, mut compensation) = (0.0_f64, 0.0_f64);
        for &w in &weights {
            let next = sum + w;
            compensation += if sum.abs() >= w.abs() {
                (sum - next) + w
            } else {
                (w - next) + sum
            };
            sum = next;
        }
        let total = sum + compensation;
        weights.iter().map(|w| w / total).collect()
    }

    const REFERENCE_ERROR: f64 = 8.0 * TWO_POW_NEG_53;

    /// Largest envelope margin (non-positive means inside) and the largest
    /// absolute discrepancy.
    fn envelope_report(logits: &[f32]) -> (f64, f64) {
        let counts = grid_counts(logits);
        let reference = reference_softmax(logits);
        let bound = unclamped_softmax_envelope_v1(logits.len()) + REFERENCE_ERROR;
        let mut worst_margin = f64::NEG_INFINITY;
        let mut worst_error = 0.0_f64;
        for (count, p) in counts.iter().zip(&reference) {
            let error = (*count as f64 * TWO_POW_NEG_53 - p).abs();
            worst_error = worst_error.max(error);
            worst_margin = worst_margin.max(error - bound);
        }
        (worst_margin, worst_error)
    }

    fn envelope_vectors() -> Vec<Vec<f32>> {
        let mut vectors: Vec<Vec<f32>> = vec![
            vec![0.0, -3.0],
            vec![0.0, -16.5],
            vec![0.0, -19.37, -3.0, -25.0],
            vec![0.0, -36.0, -40.0, -1.0, 0.0],
            vec![5.0, 5.0 - 700.0, 5.0 - 710.0],
            vec![0.0, -1.0, -2.0, -4.0, -8.0, -16.0, -17.0, -19.37],
            vec![0.0, -0.0],
            vec![3.0e38, -3.0e38],
            vec![1.0e-30, -1.0e-30, 0.0],
            vec![-50.0, -50.0, -50.0],
            vec![12.5],
        ];
        vectors.push((0..64).map(|i| -0.5 * i as f32).collect());
        vectors.push((0..65).map(|i| -0.4 * i as f32).collect());
        vectors.push((0..5_040).map(|i| -0.37 * (i % 97) as f32).collect());
        vectors.push(
            (0..UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1)
                .map(|i| -0.011 * (i % 3_331) as f32)
                .collect(),
        );
        vectors
    }

    #[test]
    fn contract_digest_and_constants_are_exact() {
        let digest = format!(
            "{:x}",
            Sha256::digest(UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_JSON_V1.as_bytes())
        );
        assert_eq!(digest, UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_SHA256_V1);
        let contract: serde_json::Value =
            serde_json::from_str(UNCLAMPED_SOFTMAX_SAMPLER_CONTRACT_JSON_V1).unwrap();
        assert_eq!(
            contract["sampler_version"],
            UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1
        );
        assert_eq!(INV_LN2_V1, core::f64::consts::LOG2_E);
        assert_eq!(LN2_HI_V1 + LN2_LO_V1, core::f64::consts::LN_2);
        assert_eq!(LN2_HI_V1.to_bits() & ((1 << 21) - 1), 0);
        assert_eq!(ROUND_SHIFTER_V1, 1.5 * 4_503_599_627_370_496.0);
        for (j, coefficient) in EXP_TAYLOR_V1.iter().enumerate() {
            let factorial: f64 = (1..=j).map(|v| v as f64).product();
            assert_eq!(*coefficient, 1.0 / factorial, "coefficient {j}");
        }
    }

    #[test]
    fn exp_v1_matches_platform_exp_within_three_ulp_on_the_admitted_domain() {
        let mut worst = 0_u64;
        let mut worst_input = 0.0_f64;
        let mut check = |d: f64| {
            let actual = exp_v1(d);
            let expected = d.exp();
            assert!(actual.is_finite() && actual > 0.0 && actual <= 1.0, "d={d}");
            let distance = actual.to_bits().abs_diff(expected.to_bits());
            if distance > worst {
                worst = distance;
                worst_input = d;
            }
        };
        for step in 0..=2_000_000_u64 {
            check(UNCLAMPED_SOFTMAX_GAP_FLOOR_V1 * step as f64 / 2_000_000.0);
        }
        for bits in [
            0_u64,
            1,
            0x3cb0_0000_0000_0000,
            0x3fd6_2e42_fefa_39ef,
            0x3fe6_2e42_fefa_39ef,
        ] {
            check(-f64::from_bits(bits));
        }
        check(UNCLAMPED_SOFTMAX_GAP_FLOOR_V1);
        assert_eq!(exp_v1(0.0).to_bits(), 1.0_f64.to_bits());
        assert_eq!(exp_v1(-0.0).to_bits(), 1.0_f64.to_bits());
        eprintln!("exp_v1 vs platform exp: max {worst} ulp at {worst_input}");
        assert!(
            worst <= 3,
            "exp_v1 differs from platform exp by {worst} ulp at {worst_input}"
        );
    }

    #[test]
    fn selection_probabilities_meet_the_declared_envelope_including_gaps_beyond_16() {
        for logits in envelope_vectors() {
            let (margin, error) = envelope_report(&logits);
            eprintln!(
                "unclamped-softmax envelope width {}: max |P-p| {error:.3e}, bound {:.3e}",
                logits.len(),
                unclamped_softmax_envelope_v1(logits.len())
            );
            assert!(
                margin <= 0.0,
                "width {} violates the envelope: error {error:e}, margin {margin:e}",
                logits.len()
            );
        }
        // Tail actions the legacy clamp inflates are reproduced, not floored.
        let counts = grid_counts(&[0.0, -19.37]);
        let tail = counts[1] as f64 * TWO_POW_NEG_53;
        let truth = (-19.37_f32 as f64).exp() / (1.0 + (-19.37_f32 as f64).exp());
        assert!(
            (tail / truth - 1.0).abs() < 1e-6,
            "tail {tail:e} truth {truth:e}"
        );
    }

    #[test]
    fn the_envelope_check_rejects_the_clamped_legacy_sampler() {
        // Power check: the same comparison fails for the Q8/16-nat sampler.
        let logits = [0.0_f32, -19.37];
        let mut legacy = FastCategoricalScratch::default();
        let masses = legacy.apportion(&logits).unwrap().to_vec();
        let legacy_tail = masses[1] as f64 / FAST_CATEGORICAL_MASS_TOTAL as f64;
        let truth = reference_softmax(&logits)[1];
        assert!(
            legacy_tail / truth > 20.0,
            "legacy tail {legacy_tail:e} truth {truth:e}"
        );
        assert!((legacy_tail - truth).abs() > unclamped_softmax_envelope_v1(2) + REFERENCE_ERROR);
    }

    #[test]
    fn underflow_ties_and_zero_weights_follow_the_contract() {
        let logits = [5.0, 5.0 - 710.0, 5.0, -3.0e38];
        let mut scratch = UnclampedSoftmaxScratchV1::default();
        let prefix = scratch.prefix_sums_v1(&logits).unwrap().to_vec();
        assert_eq!(prefix[0], 1.0);
        assert_eq!(prefix[1], 1.0, "a gap below -708 has weight +0.0");
        assert_eq!(prefix[2], 2.0, "every maximal logit has weight exactly 1");
        assert_eq!(prefix[3], 2.0);
        assert_eq!(grid_counts(&logits), vec![GRID / 2, 0, GRID / 2, 0]);
        // Equal logits split the grid exactly for power-of-two widths.
        assert_eq!(grid_counts(&[-2.0; 4]), vec![GRID / 4; 4]);
        assert_eq!(grid_counts(&[7.0]), vec![GRID]);
    }

    #[test]
    fn invalid_inputs_fail_closed() {
        let mut scratch = UnclampedSoftmaxScratchV1::default();
        assert_eq!(
            scratch.sample(&[], 1),
            Err(UnclampedSoftmaxSamplerErrorV1::Empty)
        );
        assert_eq!(
            scratch.sample(&[0.0, f32::NAN], 1),
            Err(UnclampedSoftmaxSamplerErrorV1::NonFinite {
                index: 1,
                bits: f32::NAN.to_bits()
            })
        );
        assert_eq!(
            scratch.sample(&[f32::INFINITY, 0.0], 1),
            Err(UnclampedSoftmaxSamplerErrorV1::NonFinite {
                index: 0,
                bits: f32::INFINITY.to_bits()
            })
        );
        let wide = vec![0.0_f32; UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 + 1];
        assert_eq!(
            scratch.sample(&wide, 1),
            Err(UnclampedSoftmaxSamplerErrorV1::WidthExceeded {
                width: UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 + 1,
                maximum: UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1,
            })
        );
    }

    #[test]
    fn draws_are_on_the_grid_and_below_the_total() {
        for draw in [0_u64, 1, 2047, 2048, u64::MAX, 0x9e37_79b9_7f4a_7c15] {
            let u = unclamped_softmax_unit_v1(draw);
            assert!((0.0..1.0).contains(&u));
            assert_eq!((u / TWO_POW_NEG_53).fract(), 0.0);
        }
        // The largest grid point still selects inside the menu for totals of
        // every significand shape.
        let top = unclamped_softmax_unit_v1(u64::MAX);
        assert_eq!(top, (GRID - 1) as f64 * TWO_POW_NEG_53);
        for total_bits in [
            0x3ff0_0000_0000_0000_u64,
            0x3fff_ffff_ffff_ffff,
            0x4000_0000_0000_0001,
            0x40c3_8800_0000_0000,
        ] {
            let total = f64::from_bits(total_bits);
            assert!(top * total < total);
            assert_eq!(unclamped_softmax_select_v1(&[total], top), Ok(0));
        }
    }

    /// Deterministic battery: widths 1..12 plus 64, 65 and 300, logits with
    /// gaps up to 40 nats and repeated values from one SplitMix64 stream, and
    /// one draw per menu from a separate continuing seat stream.
    fn golden_battery() -> Vec<(Vec<f32>, u64)> {
        let mut values = SplitMix64::seed(0x6c69_6e65_2d62_2d76);
        let mut seat = SplitMix64::seed(0x7365_6174_2d73_7472);
        let mut cases = Vec::new();
        for case in 0..512 {
            let width = match case % 16 {
                13 => 64,
                14 => 65,
                15 => 300,
                other => 1 + other as usize % 12,
            };
            let logits = (0..width)
                .map(|_| {
                    let value = values.next_u64();
                    if value % 7 == 0 {
                        -1.25
                    } else {
                        -(((value >> 8) % 4_001) as f32) / 100.0
                    }
                })
                .collect();
            cases.push((logits, seat.next_u64()));
        }
        cases
    }

    /// Also reproduced by python/tools/unclamped_softmax_sampler_v1_replica.py,
    /// an independent binary64 replica of this module.
    const GOLDEN_BATTERY_SHA256_V1: &str =
        "9dee7479b9c15daeb153d4b839cd0b1e701672feb39cb5c2e41965a18948ba8d";

    #[test]
    fn golden_battery_selection_stream_is_pinned() {
        let mut scratch = UnclampedSoftmaxScratchV1::default();
        let mut digest = Sha256::new();
        for (logits, draw) in golden_battery() {
            let selected = scratch.sample(&logits, draw).unwrap();
            digest.update((logits.len() as u32).to_le_bytes());
            for logit in &logits {
                digest.update(logit.to_bits().to_le_bytes());
            }
            digest.update(draw.to_le_bytes());
            digest.update((selected as u32).to_le_bytes());
        }
        assert_eq!(format!("{:x}", digest.finalize()), GOLDEN_BATTERY_SHA256_V1);
    }

    #[test]
    fn repeated_and_reused_scratch_calls_are_identical() {
        let battery = golden_battery();
        let mut reused = UnclampedSoftmaxScratchV1::default();
        for (logits, draw) in &battery {
            let mut fresh = UnclampedSoftmaxScratchV1::default();
            assert_eq!(reused.sample(logits, *draw), fresh.sample(logits, *draw));
        }
    }
}
