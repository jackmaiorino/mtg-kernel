//! The Foundations Limited training campaign of issue 657
//! (`docs/design/limited_campaign_v1.md`).
//!
//! This module holds the campaign's library code so that the library tests
//! run it; the binary `kernel_limited_campaign_v1` only parses its command
//! line. [`checkpoint`] is the checkpoint schema
//! `limited-campaign-checkpoint/v1` (design Section 3.4), which the
//! evaluation policies read as well.
//!
//! Action sampling ([`sample_logits_v1`], recorded as `f64-exp-icdf-u53/v1`)
//! uses only the libm-free `exp_v1` of `unclamped_softmax_sampler_v1`, so a
//! seat's choices have the same bits on every platform for the same logits.
use crate::unclamped_softmax_sampler_v1::{
    exp_v1, unclamped_softmax_select_v1, unclamped_softmax_unit_v1, UNCLAMPED_SOFTMAX_GAP_FLOOR_V1,
};
use sha2::{Digest, Sha256};

pub mod checkpoint;

/// The sampler name every campaign artifact records.
pub const SAMPLER_V1: &str = "f64-exp-icdf-u53/v1";
/// How the sampler forms its weights, recorded next to [`SAMPLER_V1`].
pub const SAMPLER_EXP_V1: &str =
    "exp_v1 of unclamped_softmax_sampler_v1 (libm-free) on binary64 gaps from the first maximal logit";

/// Lowercase hexadecimal SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// True for 64 lowercase hexadecimal characters.
pub(crate) fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The first eight bytes (little-endian) of SHA-256 over `domain` and each
/// part's little-endian bytes. This is the rule of `limited_match_v1`'s
/// `derive_seed`, which the design's two campaign tags use (Section 2.2).
pub fn derive_seed_v1(domain: &[u8], parts: &[u64]) -> u64 {
    let mut hash = Sha256::new();
    hash.update(domain);
    for part in parts {
        hash.update(part.to_le_bytes());
    }
    u64::from_le_bytes(hash.finalize()[..8].try_into().expect("eight digest bytes"))
}

/// Samples one index of `logits` for one seat-stream output `draw`: binary64
/// gaps `z_i - z_max` from the first maximal logit, weights `exp_v1(gap)` for
/// gaps at or above -708 and `+0.0` below, prefix sums in index order, then
/// the smallest index whose prefix sum exceeds `u * S` with
/// `u = (draw >> 11) * 2^-53`. The caller takes exactly one `next_u64` of the
/// acting seat's stream per sampled decision.
pub fn sample_logits_v1(logits: &[f64], draw: u64) -> Result<usize, String> {
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|error| {
        format!(
            "sampling requires the pinned floating-point state (found 0x{:08x})",
            error.observed_v1()
        )
    })?;
    let Some(&first) = logits.first() else {
        return Err("sampling needs at least one logit".into());
    };
    let mut maximum = first;
    for (index, &logit) in logits.iter().enumerate() {
        if !logit.is_finite() {
            return Err(format!("logit {index} is not finite"));
        }
        if logit > maximum {
            maximum = logit;
        }
    }
    let mut cumulative = 0.0_f64;
    let prefix = logits
        .iter()
        .map(|&logit| {
            let gap = logit - maximum;
            cumulative += if gap >= UNCLAMPED_SOFTMAX_GAP_FLOOR_V1 {
                exp_v1(gap)
            } else {
                0.0
            };
            cumulative
        })
        .collect::<Vec<_>>();
    unclamped_softmax_select_v1(&prefix, unclamped_softmax_unit_v1(draw))
        .map_err(|error| error.to_string())
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
mod tests {
    use super::*;
    use crate::state::SplitMix64;

    #[test]
    fn derive_seed_reproduces_the_match_driver_deal_stream() {
        // `deal_pair_v1` seeds its generator with the same rule and tag.
        let mut rng = SplitMix64::seed(derive_seed_v1(
            b"mtg-kernel/limited-match-pair/v1\0",
            &[657_000, 3],
        ));
        let first = (rng.next_u64() % 40) as usize;
        let (decks, _, _) = crate::limited_match_v1::deal_pair_v1(657_000, 3, 40).unwrap();
        // The rejection threshold for 40 is below 2^6, so the first draw is
        // accepted except with negligible probability; the test pins it.
        assert_eq!(decks[0], first);
    }

    #[test]
    fn sampler_is_deterministic_and_follows_the_prefix_rule() {
        assert!(sample_logits_v1(&[], 0).is_err());
        assert!(sample_logits_v1(&[0.0, f64::NAN], 0).is_err());
        assert!(sample_logits_v1(&[0.0, f64::INFINITY], 0).is_err());
        // One action: always index 0.
        for draw in [0, 1, u64::MAX] {
            assert_eq!(sample_logits_v1(&[3.5], draw).unwrap(), 0);
        }
        // Equal logits: the unit interval splits in halves.
        assert_eq!(sample_logits_v1(&[1.0, 1.0], 0).unwrap(), 0);
        assert_eq!(sample_logits_v1(&[1.0, 1.0], (1 << 63) - 1).unwrap(), 0);
        assert_eq!(sample_logits_v1(&[1.0, 1.0], 1 << 63).unwrap(), 1);
        assert_eq!(sample_logits_v1(&[1.0, 1.0], u64::MAX).unwrap(), 1);
        // A gap below -708 has zero weight and is never chosen.
        for draw in [0, 1 << 62, u64::MAX] {
            assert_eq!(sample_logits_v1(&[0.0, -800.0], draw).unwrap(), 0);
            assert_eq!(sample_logits_v1(&[-800.0, 0.0], draw).unwrap(), 1);
        }
        // Frequencies follow the softmax within sampling error.
        let logits = [0.0, 1.0, -1.0, 0.5];
        let mut rng = SplitMix64::seed(657);
        let mut counts = [0usize; 4];
        let draws = 40_000;
        for _ in 0..draws {
            counts[sample_logits_v1(&logits, rng.next_u64()).unwrap()] += 1;
        }
        let weights = logits.map(f64::exp);
        let total: f64 = weights.iter().sum();
        for (count, weight) in counts.iter().zip(weights) {
            let expected = weight / total;
            let observed = *count as f64 / draws as f64;
            assert!((observed - expected).abs() < 0.01, "{observed} {expected}");
        }
        // The same draws give the same choices.
        let mut first = SplitMix64::seed(9);
        let mut second = SplitMix64::seed(9);
        for _ in 0..100 {
            assert_eq!(
                sample_logits_v1(&logits, first.next_u64()),
                sample_logits_v1(&logits, second.next_u64())
            );
        }
    }
}
