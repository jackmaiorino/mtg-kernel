//! Bounded, allocation-free categorical sampling from finite binary32 logits.
//!
//! This is a new sampler identity. It does not reinterpret artifacts produced
//! by `decimal-softmax-hamilton-splitmix64-v1`.

use core::fmt;

pub const FAST_CATEGORICAL_SAMPLER_VERSION: &str = "f32-q8-expq63-hamilton-splitmix64-v1";
pub const FAST_CATEGORICAL_MAX_ACTIONS: usize = 64;
pub const FAST_CATEGORICAL_DELTA_Q_BITS: u32 = 8;
pub const FAST_CATEGORICAL_DELTA_CLAMP: i32 = -16;
pub const FAST_CATEGORICAL_EXP_TABLE_LEN: usize = 4_097;
pub const FAST_CATEGORICAL_MASS_TOTAL: u128 = 1_u128 << 64;

/// `round_ties_even(exp(-1/256) * 2**63)`, evaluated with 100 decimal digits.
///
/// The exact contracted integer is used by the recurrence; no runtime floating
/// point or platform math-library operation participates in table generation.
pub const FAST_CATEGORICAL_EXP_BASE_Q63: u64 = 9_187_413_517_043_429_148;

/// SHA-256 over every Q63 table entry encoded as little-endian `u64` bytes.
/// Verified by tests and emitted by the diagnostic.
pub const FAST_CATEGORICAL_EXP_TABLE_SHA256: &str =
    "2cdd19abdec245d7a9f892e8757c299a282ae097361baecc46cfd6a57c476e2a";

/// Canonical UTF-8 contract bytes. Its SHA-256 is separately pinned below.
pub const FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON: &str = r#"{"action_rng":"first splitmix64-v1 uint64 output from the supplied uint64 seed","algorithm":"inverse CDF over Hamilton-apportioned 2**64 mass in legal-action order","apportionment":"floor(weight*2**64/sum), then residual units by descending exact integer remainder and ascending legal-action index","delta_quantization":"exact finite IEEE-754 binary32 difference from the maximum; multiply by 256; round to nearest integer ties-to-even; clamp to [0,4096]","exp_base_q63":9187413517043429148,"exp_recurrence":"w[0]=2**63; w[k]=round_ties_even(w[k-1]*base/2**63), k=1..4096","exp_table_len":4097,"input":"1..64 finite IEEE-754 binary32 logits in legal-action order","mass_total":"2**64","sampler_version":"f32-q8-expq63-hamilton-splitmix64-v1","scratch":"caller-owned fixed-capacity arrays; no allocation for admitted widths"}"#;

/// SHA-256 over `FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON.as_bytes()`.
pub const FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256: &str =
    "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6";

const Q63_SCALE: u128 = 1_u128 << 63;
const Q63_HALF: u128 = 1_u128 << 62;
const F32_SIGN_MASK: u32 = 1_u32 << 31;
const F32_EXP_MASK: u32 = 0xff_u32 << 23;
const F32_FRACTION_MASK: u32 = (1_u32 << 23) - 1;
const UNIVERSAL_SCALE_TO_Q8_SHIFT: usize = 141;
const Q8_CLAMP: u16 = 4_096;

const fn round_q63_product_ties_even(left: u64, right: u64) -> u64 {
    let product = (left as u128) * (right as u128);
    let quotient = product >> 63;
    let remainder = product & (Q63_SCALE - 1);
    let increment = remainder > Q63_HALF || (remainder == Q63_HALF && quotient & 1 == 1);
    (quotient + increment as u128) as u64
}

const fn build_exp_table_q63() -> [u64; FAST_CATEGORICAL_EXP_TABLE_LEN] {
    let mut result = [0_u64; FAST_CATEGORICAL_EXP_TABLE_LEN];
    result[0] = 1_u64 << 63;
    let mut index = 1;
    while index < FAST_CATEGORICAL_EXP_TABLE_LEN {
        result[index] =
            round_q63_product_ties_even(result[index - 1], FAST_CATEGORICAL_EXP_BASE_Q63);
        index += 1;
    }
    result
}

/// The compile-time Q63 approximation to `exp(-k/256)`, for `k=0..4096`.
pub static FAST_CATEGORICAL_EXP_TABLE_Q63: [u64; FAST_CATEGORICAL_EXP_TABLE_LEN] =
    build_exp_table_q63();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FastCategoricalError {
    Empty,
    WidthExceeded { width: usize, maximum: usize },
    NonFinite { index: usize, bits: u32 },
    InternalInvariant { code: &'static str },
}

impl fmt::Display for FastCategoricalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(
                formatter,
                "fast categorical sampling requires at least one action"
            ),
            Self::WidthExceeded { width, maximum } => write!(
                formatter,
                "fast categorical action width {width} exceeds fail-closed maximum {maximum}"
            ),
            Self::NonFinite { index, bits } => write!(
                formatter,
                "fast categorical logit at legal index {index} is non-finite (bits=0x{bits:08x})"
            ),
            Self::InternalInvariant { code } => {
                write!(
                    formatter,
                    "fast categorical internal invariant failed: {code}"
                )
            }
        }
    }
}

impl std::error::Error for FastCategoricalError {}

/// Reusable fixed-capacity scratch for the complete sampling path.
///
/// Construction and every admitted call are allocation-free. Keep one scratch
/// value per worker; it is deliberately neither shared nor synchronized.
#[derive(Clone)]
pub struct FastCategoricalScratch {
    weights: [u64; FAST_CATEGORICAL_MAX_ACTIONS],
    remainders: [u128; FAST_CATEGORICAL_MAX_ACTIONS],
    masses: [u128; FAST_CATEGORICAL_MAX_ACTIONS],
    order: [u8; FAST_CATEGORICAL_MAX_ACTIONS],
}

impl Default for FastCategoricalScratch {
    fn default() -> Self {
        Self {
            weights: [0; FAST_CATEGORICAL_MAX_ACTIONS],
            remainders: [0; FAST_CATEGORICAL_MAX_ACTIONS],
            masses: [0; FAST_CATEGORICAL_MAX_ACTIONS],
            order: [0; FAST_CATEGORICAL_MAX_ACTIONS],
        }
    }
}

impl FastCategoricalScratch {
    /// Quantize, look up weights, and Hamilton-apportion exactly `2**64` mass.
    pub fn apportion(&mut self, logits: &[f32]) -> Result<&[u128], FastCategoricalError> {
        let width = validate_width(logits)?;
        let mut maximum_bits = logits[0].to_bits();
        let mut maximum_key = finite_order_key(maximum_bits);

        for (index, logit) in logits.iter().copied().enumerate() {
            let bits = logit.to_bits();
            if bits & F32_EXP_MASK == F32_EXP_MASK {
                return Err(FastCategoricalError::NonFinite { index, bits });
            }
            let key = finite_order_key(bits);
            if key > maximum_key {
                maximum_bits = bits;
                maximum_key = key;
            }
        }

        let mut weight_total = 0_u128;
        for (index, logit) in logits.iter().copied().enumerate() {
            let delta_index = quantized_gap_q8(maximum_bits, logit.to_bits())? as usize;
            let weight = FAST_CATEGORICAL_EXP_TABLE_Q63[delta_index];
            self.weights[index] = weight;
            weight_total += u128::from(weight);
        }

        let mut apportioned_total = 0_u128;
        for index in 0..width {
            let numerator = u128::from(self.weights[index]) * FAST_CATEGORICAL_MASS_TOTAL;
            let quotient = numerator / weight_total;
            self.masses[index] = quotient;
            self.remainders[index] = numerator % weight_total;
            apportioned_total += quotient;
            self.order[index] = index as u8;
        }

        // Stable insertion sort: exact remainder descending, then legal index
        // ascending. Width is explicitly bounded to 64.
        for position in 1..width {
            let candidate = self.order[position];
            let candidate_index = usize::from(candidate);
            let mut insertion = position;
            while insertion > 0 {
                let previous = self.order[insertion - 1];
                let previous_index = usize::from(previous);
                let candidate_is_better = self.remainders[candidate_index]
                    > self.remainders[previous_index]
                    || (self.remainders[candidate_index] == self.remainders[previous_index]
                        && candidate_index < previous_index);
                if !candidate_is_better {
                    break;
                }
                self.order[insertion] = previous;
                insertion -= 1;
            }
            self.order[insertion] = candidate;
        }

        let Some(residual_mass) = FAST_CATEGORICAL_MASS_TOTAL.checked_sub(apportioned_total) else {
            return Err(FastCategoricalError::InternalInvariant {
                code: "apportioned-total-overflow",
            });
        };
        let residual = usize::try_from(residual_mass).map_err(|_| {
            FastCategoricalError::InternalInvariant {
                code: "hamilton-residual-not-usize",
            }
        })?;
        if residual >= width {
            return Err(FastCategoricalError::InternalInvariant {
                code: "hamilton-residual-out-of-range",
            });
        }
        for rank in 0..residual {
            self.masses[usize::from(self.order[rank])] += 1;
        }
        if self.masses[..width].iter().copied().sum::<u128>() != FAST_CATEGORICAL_MASS_TOTAL {
            return Err(FastCategoricalError::InternalInvariant {
                code: "hamilton-mass-total",
            });
        }
        Ok(&self.masses[..width])
    }

    /// Execute quantization, weights, Hamilton apportionment, one SplitMix64
    /// draw, and inverse-CDF selection.
    pub fn sample(&mut self, logits: &[f32], seed: u64) -> Result<usize, FastCategoricalError> {
        let draw = u128::from(splitmix64_first(seed));
        let masses = self.apportion(logits)?;
        let mut cumulative = 0_u128;
        for (index, mass) in masses.iter().copied().enumerate() {
            cumulative += mass;
            if draw < cumulative {
                return Ok(index);
            }
        }
        Err(FastCategoricalError::InternalInvariant {
            code: "inverse-cdf-not-total",
        })
    }
}

/// Return the first SplitMix64-v1 output from the supplied seed.
#[inline]
pub fn splitmix64_first(seed: u64) -> u64 {
    let mut mixed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

fn validate_width(logits: &[f32]) -> Result<usize, FastCategoricalError> {
    let width = logits.len();
    if width == 0 {
        return Err(FastCategoricalError::Empty);
    }
    if width > FAST_CATEGORICAL_MAX_ACTIONS {
        return Err(FastCategoricalError::WidthExceeded {
            width,
            maximum: FAST_CATEGORICAL_MAX_ACTIONS,
        });
    }
    Ok(width)
}

#[inline]
fn finite_order_key(bits: u32) -> u32 {
    if bits & F32_SIGN_MASK == 0 {
        bits ^ F32_SIGN_MASK
    } else {
        !bits
    }
}

type Magnitude = [u64; 5];

fn f32_magnitude_in_subnormal_units(bits: u32) -> Magnitude {
    let exponent = ((bits & F32_EXP_MASK) >> 23) as usize;
    let fraction = u64::from(bits & F32_FRACTION_MASK);
    let (significand, shift) = if exponent == 0 {
        (fraction, 0)
    } else {
        (fraction | (1_u64 << 23), exponent - 1)
    };
    let mut result = [0_u64; 5];
    let word = shift / 64;
    let offset = shift % 64;
    result[word] = significand << offset;
    if offset != 0 {
        result[word + 1] = significand >> (64 - offset);
    }
    result
}

fn add_magnitude(mut left: Magnitude, right: Magnitude) -> Result<Magnitude, FastCategoricalError> {
    let mut carry = 0_u128;
    for index in 0..left.len() {
        let sum = u128::from(left[index]) + u128::from(right[index]) + carry;
        left[index] = sum as u64;
        carry = sum >> 64;
    }
    if carry != 0 {
        return Err(FastCategoricalError::InternalInvariant {
            code: "magnitude-add-overflow",
        });
    }
    Ok(left)
}

fn subtract_magnitude(
    mut left: Magnitude,
    right: Magnitude,
) -> Result<Magnitude, FastCategoricalError> {
    let mut borrow = 0_u128;
    for index in 0..left.len() {
        let subtrahend = u128::from(right[index]) + borrow;
        let minuend = u128::from(left[index]);
        left[index] = minuend.wrapping_sub(subtrahend) as u64;
        borrow = u128::from(minuend < subtrahend);
    }
    if borrow != 0 {
        return Err(FastCategoricalError::InternalInvariant {
            code: "magnitude-subtract-underflow",
        });
    }
    Ok(left)
}

fn bit_is_set(value: &Magnitude, bit: usize) -> bool {
    value[bit / 64] & (1_u64 << (bit % 64)) != 0
}

fn any_bits_below(value: &Magnitude, exclusive_bit: usize) -> bool {
    let whole_words = exclusive_bit / 64;
    if value[..whole_words].iter().any(|word| *word != 0) {
        return true;
    }
    let partial = exclusive_bit % 64;
    partial != 0 && value[whole_words] & ((1_u64 << partial) - 1) != 0
}

fn any_bits_at_or_above(value: &Magnitude, inclusive_bit: usize) -> bool {
    let word = inclusive_bit / 64;
    let offset = inclusive_bit % 64;
    (value[word] >> offset != 0) || value[word + 1..].iter().any(|part| *part != 0)
}

fn extract_u16(value: &Magnitude, start_bit: usize, width: usize) -> u16 {
    let word = start_bit / 64;
    let offset = start_bit % 64;
    let mut result = value[word] >> offset;
    if offset + width > 64 {
        result |= value[word + 1] << (64 - offset);
    }
    (result & ((1_u64 << width) - 1)) as u16
}

fn quantized_gap_q8(maximum_bits: u32, value_bits: u32) -> Result<u16, FastCategoricalError> {
    let maximum_magnitude = f32_magnitude_in_subnormal_units(maximum_bits);
    let value_magnitude = f32_magnitude_in_subnormal_units(value_bits);
    let maximum_negative = maximum_bits & F32_SIGN_MASK != 0;
    let value_negative = value_bits & F32_SIGN_MASK != 0;

    let gap = match (maximum_negative, value_negative) {
        (false, false) => subtract_magnitude(maximum_magnitude, value_magnitude)?,
        (false, true) => add_magnitude(maximum_magnitude, value_magnitude)?,
        (true, true) => subtract_magnitude(value_magnitude, maximum_magnitude)?,
        (true, false) => {
            return Err(FastCategoricalError::InternalInvariant {
                code: "finite-maximum-sign-order",
            });
        }
    };

    // `gap` is in units of 2**-149. Multiplying by 2**8 and rounding
    // therefore means dividing by 2**141. Any bit at 141+12 or above is
    // already at the clamp boundary.
    if any_bits_at_or_above(&gap, UNIVERSAL_SCALE_TO_Q8_SHIFT + 12) {
        return Ok(Q8_CLAMP);
    }
    let mut quotient = extract_u16(&gap, UNIVERSAL_SCALE_TO_Q8_SHIFT, 12);
    let half_bit = UNIVERSAL_SCALE_TO_Q8_SHIFT - 1;
    let round_up =
        bit_is_set(&gap, half_bit) && (any_bits_below(&gap, half_bit) || quotient & 1 == 1);
    if round_up {
        quotient += 1;
    }
    Ok(quotient.min(Q8_CLAMP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q63_recurrence_endpoints_are_pinned() {
        assert_eq!(FAST_CATEGORICAL_EXP_TABLE_Q63[0], 1_u64 << 63);
        assert_eq!(
            FAST_CATEGORICAL_EXP_TABLE_Q63[1],
            FAST_CATEGORICAL_EXP_BASE_Q63
        );
        assert_eq!(FAST_CATEGORICAL_EXP_TABLE_Q63[4_096], 1_037_953_783_666);
    }

    #[test]
    fn exact_gap_quantization_rounds_ties_to_even_and_clamps() {
        assert_eq!(
            quantized_gap_q8(0.0_f32.to_bits(), 0.0_f32.to_bits()),
            Ok(0)
        );
        assert_eq!(
            quantized_gap_q8(0.0_f32.to_bits(), (-1.0 / 512.0_f32).to_bits()),
            Ok(0)
        );
        assert_eq!(
            quantized_gap_q8(0.0_f32.to_bits(), (-3.0 / 512.0_f32).to_bits()),
            Ok(2)
        );
        assert_eq!(
            quantized_gap_q8(0.0_f32.to_bits(), (-16.0_f32).to_bits()),
            Ok(4_096)
        );
        assert_eq!(
            quantized_gap_q8(f32::MAX.to_bits(), (-f32::MAX).to_bits()),
            Ok(4_096)
        );
    }

    #[test]
    fn exact_gap_quantization_matches_binary64_reference_for_finite_pairs() {
        let mut state = 0x243f_6a88_85a3_08d3_u64;
        for _ in 0..250_000 {
            let mut values = [0.0_f32; 2];
            for value in &mut values {
                loop {
                    state = state
                        .wrapping_add(0x9e37_79b9_7f4a_7c15)
                        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
                    let bits = (state ^ (state >> 32)) as u32;
                    if bits & F32_EXP_MASK != F32_EXP_MASK {
                        *value = f32::from_bits(bits);
                        break;
                    }
                }
            }
            let (maximum, value) =
                if finite_order_key(values[0].to_bits()) >= finite_order_key(values[1].to_bits()) {
                    (values[0], values[1])
                } else {
                    (values[1], values[0])
                };
            let gap = f64::from(maximum) - f64::from(value);
            let expected = if gap >= 16.0 {
                Q8_CLAMP
            } else {
                ((gap * 256.0).round_ties_even() as u16).min(Q8_CLAMP)
            };
            assert_eq!(
                quantized_gap_q8(maximum.to_bits(), value.to_bits()),
                Ok(expected),
                "maximum=0x{:08x} value=0x{:08x}",
                maximum.to_bits(),
                value.to_bits()
            );
        }
    }
}
