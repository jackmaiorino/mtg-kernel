//! Shared metric core for the CUDA qualification campaign.
//!
//! Implements the sealed characterization contract's metric semantics
//! (identity `mtg-kernel-native-cuda-characterization-contract-v1`,
//! digest 4c3d3249003bc34cae59166fc6dbcf1a71f0fd1d34758372564af394284aba28)
//! as pure f64 functions over exported f32 bits, so the launcher and any
//! auditor can independently recompute every channel from framed raw
//! arrays rather than trusting in-record aggregates. Contract semantics
//! honored here: each f32 is promoted to f64 exactly once and rejected if
//! nonfinite before any fold; deltas are CUDA minus CPU with first-tie
//! min/max selection under an action-index-ascending left fold; the
//! softmax is stable f64 (first-max tie shift, exp, action-index-ascending
//! sum) with no f32 round trip; row KL is sum over actions of own
//! probability times (own log-probability minus other log-probability);
//! the unscaled gradient metric is the L1 distance between the two
//! probability vectors; and cap comparison is finite-nonnegative inclusive
//! less-than-or-equal with no epsilon and no clamp.

/// Promote one exported f32 bit pattern to f64 exactly once, rejecting
/// nonfinite values before any fold.
pub(crate) fn promote_bits_v1(bits: u32, name: &str) -> f64 {
    let value = f32::from_bits(bits);
    assert!(
        value.is_finite(),
        "nonfinite raw {name} (bits {bits:#010x})"
    );
    f64::from(value)
}

/// Per-row delta statistics under the contract fold: deltas are CUDA minus
/// CPU in action-index-ascending order, min/max keep the FIRST tie, and
/// the range is max minus min.
pub(crate) struct RowDeltaStatsV1 {
    pub(crate) min_delta: f64,
    pub(crate) max_delta: f64,
    pub(crate) min_delta_index: usize,
    pub(crate) max_delta_index: usize,
    pub(crate) range: f64,
    pub(crate) max_abs: f64,
}

pub(crate) fn row_delta_stats_v1(cuda_bits: &[u32], cpu_bits: &[u32]) -> RowDeltaStatsV1 {
    assert_eq!(cuda_bits.len(), cpu_bits.len(), "row width mismatch");
    assert!(!cuda_bits.is_empty(), "empty row");
    let mut min_delta = f64::INFINITY;
    let mut max_delta = f64::NEG_INFINITY;
    let mut min_delta_index = 0_usize;
    let mut max_delta_index = 0_usize;
    let mut max_abs = 0.0_f64;
    for (index, (cuda, cpu)) in cuda_bits.iter().zip(cpu_bits).enumerate() {
        let delta = promote_bits_v1(*cuda, "cuda logit") - promote_bits_v1(*cpu, "cpu logit");
        // Strict comparisons keep the first tie per the contract.
        if delta < min_delta {
            min_delta = delta;
            min_delta_index = index;
        }
        if delta > max_delta {
            max_delta = delta;
            max_delta_index = index;
        }
        max_abs = max_abs.max(delta.abs());
    }
    RowDeltaStatsV1 {
        min_delta,
        max_delta,
        min_delta_index,
        max_delta_index,
        range: max_delta - min_delta,
        max_abs,
    }
}

/// Stable f64 log-softmax over one row of exported f32 bits: first-max tie
/// shift, exp, action-index-ascending sum, natural log; no f32 round trip
/// anywhere.
pub(crate) fn stable_log_softmax_v1(row_bits: &[u32]) -> Vec<f64> {
    assert!(!row_bits.is_empty(), "empty softmax row");
    let promoted: Vec<f64> = row_bits
        .iter()
        .map(|bits| promote_bits_v1(*bits, "softmax logit"))
        .collect();
    // First-max tie: strict > keeps the earliest maximal index.
    let mut maximum = promoted[0];
    for value in promoted.iter().skip(1) {
        if *value > maximum {
            maximum = *value;
        }
    }
    let mut sum = 0.0_f64;
    for value in &promoted {
        sum += (value - maximum).exp();
    }
    let log_sum = sum.ln();
    promoted
        .iter()
        .map(|value| (value - maximum) - log_sum)
        .collect()
}

/// Row KL from the `own` distribution to the `other`: sum over actions of
/// own probability times (own log-probability minus other log-probability),
/// action-index-ascending.
pub(crate) fn row_kl_v1(own_bits: &[u32], other_bits: &[u32]) -> f64 {
    let own_logp = stable_log_softmax_v1(own_bits);
    let other_logp = stable_log_softmax_v1(other_bits);
    assert_eq!(own_logp.len(), other_logp.len(), "row width mismatch");
    let mut sum = 0.0_f64;
    for (own, other) in own_logp.iter().zip(&other_logp) {
        sum += own.exp() * (own - other);
    }
    assert!(sum.is_finite(), "nonfinite KL intermediate");
    sum
}

/// Unscaled gradient metric for one row: the L1 distance between the two
/// probability vectors, action-index-ascending.
pub(crate) fn row_probability_l1_v1(candidate_bits: &[u32], reference_bits: &[u32]) -> f64 {
    let candidate = stable_log_softmax_v1(candidate_bits);
    let reference = stable_log_softmax_v1(reference_bits);
    assert_eq!(candidate.len(), reference.len(), "row width mismatch");
    let mut sum = 0.0_f64;
    for (c, r) in candidate.iter().zip(&reference) {
        sum += (c.exp() - r.exp()).abs();
    }
    sum
}

/// Selected log-probability delta for one row: candidate selected
/// log-probability minus reference selected log-probability.
pub(crate) fn selected_logp_delta_v1(
    candidate_bits: &[u32],
    reference_bits: &[u32],
    selected_index: usize,
) -> f64 {
    let candidate = stable_log_softmax_v1(candidate_bits);
    let reference = stable_log_softmax_v1(reference_bits);
    assert!(selected_index < candidate.len(), "selected index in range");
    candidate[selected_index] - reference[selected_index]
}

/// Decode a directed f64 cap from its exact bit string.
pub(crate) fn cap_from_bits_v1(bits_hex: &str) -> f64 {
    let bits = u64::from_str_radix(bits_hex, 16).expect("cap bit string");
    let cap = f64::from_bits(bits);
    assert!(cap.is_finite() && cap > 0.0, "cap must be finite positive");
    cap
}

/// Contract cap comparison: the observed value must be finite and
/// nonnegative, and within the cap inclusively; no epsilon, no clamp.
pub(crate) fn within_cap_v1(observed: f64, cap: f64) -> bool {
    observed.is_finite() && observed >= 0.0 && observed <= cap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_fold_keeps_first_tie_and_exact_range() {
        // Two actions share the identical maximal delta; the first index
        // must win. Values chosen exactly representable so the recompute is
        // bit-deterministic.
        let cpu = [1.0_f32, 2.0, 3.0, 4.0].map(f32::to_bits);
        let cuda = [1.5_f32, 2.5, 2.75, 4.0].map(f32::to_bits);
        let stats = row_delta_stats_v1(&cuda, &cpu);
        assert_eq!(stats.max_delta, 0.5);
        assert_eq!(stats.max_delta_index, 0, "first tie must win");
        assert_eq!(stats.min_delta, -0.25);
        assert_eq!(stats.min_delta_index, 2);
        assert_eq!(stats.range, 0.75);
        assert_eq!(stats.max_abs, 0.5);
    }

    #[test]
    fn softmax_is_shift_invariant_and_kl_of_identical_rows_is_zero() {
        let row = [0.5_f32, -1.25, 2.0].map(f32::to_bits);
        let shifted = [100.5_f32, 99.75 - 1.0, 102.0].map(f32::to_bits);
        let a = stable_log_softmax_v1(&row);
        let b = stable_log_softmax_v1(&shifted);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-12, "{x} vs {y}");
        }
        assert_eq!(row_kl_v1(&row, &row), 0.0);
        assert_eq!(row_probability_l1_v1(&row, &row), 0.0);
        assert_eq!(selected_logp_delta_v1(&row, &row, 1), 0.0);
    }

    #[test]
    fn kl_and_l1_are_positive_for_distinct_rows_and_directional() {
        let p = [1.0_f32, 0.0].map(f32::to_bits);
        let q = [0.0_f32, 1.0].map(f32::to_bits);
        let forward = row_kl_v1(&p, &q);
        let backward = row_kl_v1(&q, &p);
        assert!(forward > 0.0 && backward > 0.0);
        assert!(row_probability_l1_v1(&p, &q) > 0.0);
    }

    #[test]
    fn caps_decode_to_directed_values_and_compare_inclusively() {
        // The row cap decodes to exactly the sealed bit pattern; sanity-band
        // it rather than comparing against a decimal literal, since 0.0018
        // itself is not exactly representable.
        let row_cap = cap_from_bits_v1("3f5d7dbf487fcb92");
        assert_eq!(row_cap.to_bits(), 0x3f5d7dbf487fcb92);
        assert!(row_cap > 0.00179 && row_cap < 0.00181);
        assert!(within_cap_v1(row_cap, row_cap), "inclusive at the cap");
        let above = f64::from_bits(row_cap.to_bits() + 1);
        assert!(!within_cap_v1(above, row_cap), "one ULP beyond fails");
        assert!(!within_cap_v1(f64::NAN, row_cap));
        assert!(!within_cap_v1(-0.5, row_cap));
        // Path cap 3f8460d6ccca3676 is the greatest f64 below ln(101/100).
        let path_cap = cap_from_bits_v1("3f8460d6ccca3676");
        assert!(path_cap < 0.009950330853168083);
    }

    #[test]
    #[should_panic(expected = "nonfinite raw")]
    fn nonfinite_bits_reject_before_any_fold() {
        let poisoned = [f32::NAN.to_bits(), 0];
        let clean = [0_u32, 0];
        let _ = row_delta_stats_v1(&poisoned, &clean);
    }
}
