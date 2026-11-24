//! M4 masked softmax for drafter and builder draws and loss replay.
//!
//! The caller supplies canonical order (ascending card id for picks/pointers,
//! count 0 through 40 for basics, ascending five-bit mask for land sets).
//! A true mask bit enables an entry. Masked logits are never inspected.
//! For each enabled logit, d = f64(logit) - f64(max). The pinned exp has a
//! domain floor of -120, so its input is max(d, -120). Its binary64 output e
//! is positive and at most one, and every maximum gives exactly one. S is the
//! left-to-right binary64 sum. Loss uses d - ln(S), with the original d.
//!
//! # Exact apportionment
//!
//! Let N = 2^53 and T be the exact sum of the binary64 weights. Express
//! each weight in units of 2^-226, with integer total U = T * 2^226.
//! Each quota N e_i / T equals a_i + r_i / U, with integer floor a_i
//! and 0 <= r_i < U. The quotas sum to N. Thus the number of
//! missing units is the integer L = sum(r_i / U). If L is positive it is
//! strictly smaller than the number of positive remainders. Giving one unit
//! to each of the first L descending remainders (lower index first for ties)
//! adds exactly L, so sum(q_i) = N. Integral quotas stay exact. Every other
//! count is a floor or ceiling, hence |q_i/N - e_i/T| < 2^-53. Masked
//! entries have weight and count zero.
//!
//! # Bound against the loss
//!
//! For each entry compute A = floor(N e_i/T)/N and B = ceil(N e_i/T)/N.
//! Both are exactly representable. For p = exp(returned log probability),
//! |q_i/N - p| <= max(|A - p|, |B - p|). This per-input bound includes the
//! pinned exp approximation and floor, rounding in S, the pinned logarithm
//! and the final subtraction. To evaluate it in binary64, the tests bracket
//! p around libm 0.2.16 exp (documented error below one ulp) and round the
//! maximum endpoint distance outward. The fixture tests require this bound
//! below 1e-8. A single grid step applies to e_i/T.
//! Cross-architecture byte verification is owed separately.

use crate::deterministic_math_v1::{exp_f64_v1, ln_f64_v1, EXP_DOMAIN_FLOOR_V1};
use std::fmt;

pub const SAMPLER_ID_V1: &str = "softmax-cdf-splitmix64-v2";
pub const LN_ID_V1: &str = "m4-ln-atanh23-f64-v1";
const GRID_BITS: u32 = 53;
pub const GRID_SIZE: u64 = 1 << GRID_BITS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerErrorV1 {
    MaskLengthMismatch,
    NoUnmaskedEntry,
    NonFiniteLogit { index: usize },
}

impl fmt::Display for SamplerErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MaskLengthMismatch => write!(f, "logits and mask have different lengths"),
            Self::NoUnmaskedEntry => write!(f, "the mask enables no entries"),
            Self::NonFiniteLogit { index } => write!(f, "unmasked logit {index} is nonfinite"),
        }
    }
}

impl std::error::Error for SamplerErrorV1 {}

#[derive(Debug, PartialEq)]
pub struct SampleV1 {
    pub selected_index: usize,
    pub counts: Vec<u64>,
    /// Masked entries have log probability negative infinity.
    pub log_probabilities: Vec<f64>,
    pub selected_log_probability: f64,
}

struct Weights {
    max: f64,
    values: Vec<f64>,
}

fn weights(logits: &[f32], mask: &[bool]) -> Result<Weights, SamplerErrorV1> {
    if logits.len() != mask.len() {
        return Err(SamplerErrorV1::MaskLengthMismatch);
    }
    let mut max = f64::NEG_INFINITY;
    for (index, (&logit, &enabled)) in logits.iter().zip(mask).enumerate() {
        if enabled {
            if !logit.is_finite() {
                return Err(SamplerErrorV1::NonFiniteLogit { index });
            }
            max = max.max(f64::from(logit));
        }
    }
    if max == f64::NEG_INFINITY {
        return Err(SamplerErrorV1::NoUnmaskedEntry);
    }
    let values = logits
        .iter()
        .zip(mask)
        .map(|(&logit, &enabled)| {
            if enabled {
                exp_f64_v1((f64::from(logit) - max).max(EXP_DOMAIN_FLOOR_V1))
            } else {
                0.0
            }
        })
        .collect();
    Ok(Weights { max, values })
}

// Each positive weight is normal with unbiased exponent in [-174, 0].
// In units of 2^-226 its significand shifts left by exponent + 174.
// Each weight is <= 2^226 and a slice has < 2^64 entries. Even twice the
// total fits in 291 bits. Most significant limb first for derived ordering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct Units([u64; 5]);

impl Units {
    fn from_weight(weight: f64) -> Self {
        let bits = weight.to_bits();
        let mut words = [0; 5];
        if bits != 0 {
            let significand = (1 << 52) | (bits & ((1 << 52) - 1));
            let shift = (bits >> 52) as usize - (1023 - 174);
            let low = 4 - shift / 64;
            let offset = shift % 64;
            words[low] = significand << offset;
            if offset != 0 {
                words[low - 1] = significand >> (64 - offset);
            }
        }
        Self(words)
    }

    fn plus(mut self, rhs: Self) -> Self {
        let mut carry = 0;
        for i in (0..5).rev() {
            let sum = u128::from(self.0[i]) + u128::from(rhs.0[i]) + carry;
            self.0[i] = sum as u64;
            carry = sum >> 64;
        }
        debug_assert_eq!(carry, 0);
        self
    }

    fn minus(mut self, rhs: Self) -> Self {
        let mut borrow = false;
        for i in (0..5).rev() {
            let (difference, first) = self.0[i].overflowing_sub(rhs.0[i]);
            let (difference, second) = difference.overflowing_sub(u64::from(borrow));
            self.0[i] = difference;
            borrow = first || second;
        }
        debug_assert!(!borrow);
        self
    }

    // Binary long division of weight/total. After b iterations the pair
    // represents weight * 2^b = quotient * total + remainder.
    fn quota(self, total: Self, bits: u32) -> (u64, Self) {
        if self == total {
            return (1 << bits, Self::default());
        }
        let mut remainder = self;
        let mut quotient = 0;
        for _ in 0..bits {
            remainder = remainder.plus(remainder);
            quotient <<= 1;
            if remainder >= total {
                remainder = remainder.minus(total);
                quotient += 1;
            }
        }
        (quotient, remainder)
    }
}

fn apportion<const BITS: u32>(weights: &[f64]) -> Vec<u64> {
    let total = weights
        .iter()
        .map(|&w| Units::from_weight(w))
        .fold(Units::default(), Units::plus);
    let mut counts = vec![0; weights.len()];
    let mut remainders = Vec::with_capacity(weights.len());
    let mut missing = 1_u64 << BITS;
    for (i, &weight) in weights.iter().enumerate() {
        let units = Units::from_weight(weight);
        if units != Units::default() {
            let (floor, remainder) = units.quota(total, BITS);
            counts[i] = floor;
            missing -= floor;
            if remainder != Units::default() {
                remainders.push((remainder, i));
            }
        }
    }
    remainders.sort_unstable_by(|(a, i), (b, j)| b.cmp(a).then_with(|| i.cmp(j)));
    for &(_, i) in &remainders[..missing as usize] {
        counts[i] += 1;
    }
    counts
}

fn select(counts: &[u64], k: u64) -> usize {
    let mut end = 0;
    counts
        .iter()
        .position(|&q| {
            end += q;
            k < end
        })
        .expect("k is below the apportioned total")
}

/// Exact counts in caller order, including zeros for masked entries.
pub fn counts_v1(logits: &[f32], mask: &[bool]) -> Result<Vec<u64>, SamplerErrorV1> {
    Ok(apportion::<GRID_BITS>(&weights(logits, mask)?.values))
}

/// Use the high 53 bits of the caller's SplitMix64 stream word to draw from
/// the integer CDF. The returned log probabilities are the M4 loss inputs.
pub fn sample_v1(
    logits: &[f32],
    mask: &[bool],
    stream_word: u64,
) -> Result<SampleV1, SamplerErrorV1> {
    let weights = weights(logits, mask)?;
    let counts = apportion::<GRID_BITS>(&weights.values);
    let mut sum = 0.0;
    for &weight in &weights.values {
        sum += weight;
    }
    let log_sum = ln_f64_v1(sum);
    let log_probabilities: Vec<_> = logits
        .iter()
        .zip(mask)
        .map(|(&logit, &enabled)| {
            if enabled {
                (f64::from(logit) - weights.max) - log_sum
            } else {
                f64::NEG_INFINITY
            }
        })
        .collect();
    let selected_index = select(&counts, stream_word >> 11);
    Ok(SampleV1 {
        selected_index,
        selected_log_probability: log_probabilities[selected_index],
        counts,
        log_probabilities,
    })
}

#[cfg(test)]
mod golden_cases;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_counts_draws_and_ln_bits() {
        // Generated by the m4_sampler_goldens_v1 example (pinned routine output).
        let expected = [
            ("two_logits", "c2089a8a9bea842b6392fafb75d0892da4ae8ce1e2bf284819a98d03b0a26008 [0, 0, 0, 1, 1, 1, 1, 1] 3fe1336e2549223c"),
            ("seven_equal", "2acaf4d1664c3c07580176d34bd7fcfe632566f6e33b35f125128bf2869ad1f4 [0, 0, 0, 3, 3, 5, 6, 6] 3fff2272ae325a57"),
            ("forty_five_equal", "82fd37c1b0ff06a18b0851750643f48aff567f454860d07a9a188038e30980f9 [0, 1, 6, 22, 22, 33, 44, 44] 400e740b76a3c9a4"),
            ("near_ties", "d89ab5a917fcbe7fb339cde001c61f4e599dfa39cf126b89e3909e9869c07420 [0, 0, 0, 1, 1, 2, 3, 3] 3ff62e42e2fa39f9"),
            ("one_unmasked_of_45", "509e72e0b9aeb2078fd0cc2e5663e717ed5b77e7b5cf1d5b4e42115660db6f87 [23, 23, 23, 23, 23, 23, 23, 23] 0000000000000000"),
            ("large_and_small", "e3259289fbb15df2cd6467d3f6469c8767e47a3998a933c9b38b6112a197f0fa [3, 3, 3, 3, 3, 3, 3, 3] 0000000000000000"),
            ("thirty_two_small_gaps", "b5f52570e80ad8bb33165514fbb9dc6bbd1508ef35500a1e727c315c73bea4e0 [0, 0, 4, 15, 15, 23, 31, 31] 400bb9d3beb8c856"),
            ("one_and_five_quarters", "4ce4386d79b8ee58583ecc7de4100871a4695a47d2a856cd5a4836fb3123a465 [0, 0, 0, 1, 1, 3, 5, 5] 3fe9f323eb9cbc3a"),
        ];
        let cases = golden_cases::cases();
        assert_eq!(cases.len(), expected.len());
        for (case, (name, record)) in cases.into_iter().zip(expected) {
            assert_eq!(case.name, name);
            assert_eq!(case.record(), record, "{name}");
        }
    }

    // Independent cross multiplication for the exact quota inequality.
    // In these menus (<= 45 entries) even N * total fits in five limbs.
    fn times(mut value: Units, factor: u64) -> Units {
        let mut carry = 0;
        for limb in value.0.iter_mut().rev() {
            let product = u128::from(*limb) * u128::from(factor) + carry;
            *limb = product as u64;
            carry = product >> 64;
        }
        assert_eq!(carry, 0);
        value
    }

    fn check_menu(logits: &[f32], mask: &[bool]) {
        let sample = sample_v1(logits, mask, 0).unwrap();
        let weights = weights(logits, mask).unwrap();
        assert_eq!(sample.counts.iter().sum::<u64>(), GRID_SIZE);
        assert_eq!(counts_v1(logits, mask).unwrap(), sample.counts);
        let total = weights
            .values
            .iter()
            .map(|&w| Units::from_weight(w))
            .fold(Units::default(), Units::plus);
        let log_sum = ln_f64_v1(weights.values.iter().copied().sum());
        let mut end = 0;
        for (i, (&q, &w)) in sample.counts.iter().zip(&weights.values).enumerate() {
            if !mask[i] {
                assert_eq!(q, 0);
                assert_eq!(sample.log_probabilities[i], f64::NEG_INFINITY);
            } else {
                assert!(w > 0.0 && w <= 1.0);
                if f64::from(logits[i]) == weights.max {
                    assert_eq!(w, 1.0);
                }
                assert_eq!(
                    sample.log_probabilities[i],
                    (f64::from(logits[i]) - weights.max) - log_sum
                );
            }
            let units = Units::from_weight(w);
            let numerator = times(units, GRID_SIZE);
            let assigned = times(total, q);
            assert!(assigned.max(numerator).minus(assigned.min(numerator)) < total);

            let (floor, remainder) = units.quota(total, GRID_BITS);
            let ceil = floor + u64::from(remainder != Units::default());
            let lo = floor as f64 / GRID_SIZE as f64;
            let hi = ceil as f64 / GRID_SIZE as f64;
            let p = libm::exp(sample.log_probabilities[i]);
            // Two neighbors also cover a change in spacing at a binade edge.
            let p_lo = p.next_down().next_down().max(0.0);
            let p_hi = p.next_up().next_up().min(1.0);
            let bound = (hi - p_lo).abs().max((lo - p_hi).abs()).next_up();
            let mass = q as f64 / GRID_SIZE as f64;
            assert!((mass - p_lo).abs().max((mass - p_hi).abs()) <= bound);
            assert!(bound < 1e-8, "fixture {i} has loss bound {bound:e}");

            if q != 0 {
                // Both ends of every occupied interval, including the next
                // entry after any run of zero counts. Exercise low bits too.
                for word in [end << 11, ((end + q - 1) << 11) | 0x7ff] {
                    let draw = sample_v1(logits, mask, word).unwrap();
                    assert_eq!(draw.selected_index, i);
                    assert_eq!(
                        draw.selected_log_probability.to_bits(),
                        sample.log_probabilities[i].to_bits()
                    );
                }
            }
            end += q;
        }
    }

    #[test]
    fn exact_quotas_loss_bounds_and_every_cdf_boundary() {
        for case in golden_cases::cases() {
            check_menu(&case.logits, &case.mask);
        }
        let logits = [f32::NAN, -120.0, 1.0, -119.0, f32::INFINITY, 0.0, -120.0];
        let mask = [false, true, true, true, false, true, true];
        check_menu(&logits, &mask);
        let counts = counts_v1(&logits, &mask).unwrap();
        for i in [0, 1, 3, 4, 6] {
            assert_eq!(counts[i], 0);
        }
        // A polynomial-exp discrepancy large enough to distinguish the two bounds.
        let sample = sample_v1(&[-0.34, 0.0], &[true; 2], 0).unwrap();
        assert!(
            (sample.counts[0] as f64 / GRID_SIZE as f64 - libm::exp(sample.log_probabilities[0]))
                .abs()
                > 1.0 / GRID_SIZE as f64
        );
    }

    #[test]
    fn agrees_with_direct_u128_division_and_canonical_ties() {
        // Exact dyadic inputs in [1/8, 1], where units of 2^-55 permit
        // direct u128 multiplication, division and modulo as a second oracle.
        let mut quarters = vec![0.25000000000000033; 6];
        quarters[0] = 1.0;
        for values in [
            vec![1.0; 7],
            vec![1.0; 45],
            quarters,
            vec![1.0, 0.125_f64.next_up(), 0.5_f64.next_down(), 0.25, 0.0],
        ] {
            let units: Vec<_> = values
                .iter()
                .map(|&v| (v * (1_u64 << 55) as f64) as u128)
                .collect();
            let total = units.iter().sum::<u128>();
            let numerators: Vec<_> = units.iter().map(|&n| n * u128::from(GRID_SIZE)).collect();
            let mut expected: Vec<_> = numerators.iter().map(|&n| (n / total) as u64).collect();
            let missing = GRID_SIZE - expected.iter().sum::<u64>();
            let mut order: Vec<_> = (0..values.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(numerators[i] % total));
            for &i in order.iter().take(missing as usize) {
                expected[i] += 1;
            }
            assert_eq!(apportion::<GRID_BITS>(&values), expected);
        }
    }

    #[test]
    fn all_tiny_grid_draws_recover_counts_for_each_mask() {
        let logits = [-0.34, -120.0, 0.0, 0.0, -5.0];
        for bits in 1_u32..(1 << logits.len()) {
            let mask: Vec<_> = (0..logits.len()).map(|i| bits & (1 << i) != 0).collect();
            let weights = weights(&logits, &mask).unwrap();
            let counts = apportion::<8>(&weights.values);
            let mut histogram = vec![0; logits.len()];
            for k in 0..256 {
                let i = select(&counts, k);
                assert!(mask[i] && counts[i] > 0);
                histogram[i] += 1;
            }
            assert_eq!(histogram, counts);
        }
    }

    #[test]
    fn count_bytes_repeat_across_serial_and_concurrent_calls() {
        let bytes = |case: &golden_cases::Case| -> Vec<u8> {
            counts_v1(&case.logits, &case.mask)
                .unwrap()
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect()
        };
        let cases = golden_cases::cases();
        let expected: Vec<_> = cases.iter().map(bytes).collect();
        assert_eq!(cases.iter().map(bytes).collect::<Vec<_>>(), expected);
        std::thread::scope(|s| {
            let workers: Vec<_> = cases
                .chunks(2)
                .map(|chunk| s.spawn(move || chunk.iter().map(bytes).collect::<Vec<_>>()))
                .collect();
            let parallel: Vec<_> = workers
                .into_iter()
                .flat_map(|w| w.join().unwrap())
                .collect();
            assert_eq!(parallel, expected);
        });
    }

    #[test]
    fn refuses_invalid_inputs() {
        assert_eq!(
            counts_v1(&[0.0], &[]),
            Err(SamplerErrorV1::MaskLengthMismatch)
        );
        assert_eq!(counts_v1(&[], &[]), Err(SamplerErrorV1::NoUnmaskedEntry));
        assert_eq!(
            sample_v1(&[f32::NAN], &[false], 0),
            Err(SamplerErrorV1::NoUnmaskedEntry)
        );
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                counts_v1(&[0.0, bad], &[true; 2]),
                Err(SamplerErrorV1::NonFiniteLogit { index: 1 })
            );
        }
    }
}
