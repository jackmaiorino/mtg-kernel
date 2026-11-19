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
//!
//! The runner's contract is [`RUNNER_CONTRACT_V1`], which every
//! `campaign.json` records: the design's frozen choices as built, and what
//! the research lead's rulings on issue 657 require of the runner.
use crate::unclamped_softmax_sampler_v1::{
    exp_v1, unclamped_softmax_select_v1, unclamped_softmax_unit_v1, UNCLAMPED_SOFTMAX_GAP_FLOOR_V1,
};
use sha2::{Digest, Sha256};

pub mod campaign;
pub mod checkpoint;
pub mod collect;
pub mod diagnostics;
pub mod host;
pub mod inventory;
pub mod schedule;

/// The runner's contract, one clause per line.
pub const RUNNER_CONTRACT_V1: [&str; 11] = [
    "Session: -session full-pool, profile fdn-full-pool-consumer/v1, hand knowledge \
     public-reveal-minima/v3. Every deal is limited_match_v1::deal_pair_v1 over the 40 train \
     decks in pool.tsv order; eval decks are never parsed, resolved or dealt in training, \
     league or dev games.",
    "Inventory: sample_sha256_in_pool_order is SHA-256 over each deck's file name and bytes in \
     pool.tsv order, then the name pool.tsv and its bytes in CRLF form, exactly as \
     python/tools/limited_training_inventory_v1.py::canonical_pool_bytes hashes it, and must \
     equal the design's pin; both card-file digests and the four oversize registrations are \
     pinned; each train deck resolves to exactly its pool.tsv count, which lies in \
     40..=MAX_CUSTOM_DECK_CARDS_V1.",
    "Retention: every decision of a learner seat with more than one legal action is retained. \
     A forced single-action step takes ordinal 0 without a model call or a seat-stream draw and \
     is dropped before a trajectory is built, so GAE indexes retained decisions only.",
    "Terminal classes: a natural game returns +1, 0 or -1 per seat. A game that reaches \
     max_turns 100, max_decisions 4000 or a ResetV1 ceiling (16000 physical decisions, 2048000 \
     policy steps) is truncated: its receipt is kept and counted, and it is left out of the \
     policy loss and the value loss (and out of N). A halted game or any error writes \
     failed-iteration-UUUU.json and exits with code 2.",
    "Truncation gate: more than 2 truncated games in one batch on two consecutive updates stops \
     the campaign with stop_reason stopped_excess_truncation, before the second such update is \
     applied.",
    "Determinism: each game is a function of its deal, its seat policies and its seat streams \
     derive_seed(mtg-kernel/limited-campaign-actor-rng/v1, [schedule_seed, pair, leg, seat]). \
     Games are gathered in (pair, leg) order and the update reduces in (pair, leg, seat, \
     decision) order, so every published file is identical for any worker count and with or \
     without --stage-diagnostics.",
    "Arithmetic: the v2 learner and the sampler use libm-free functions only, so the \
     actor-critic arms replay across host classes. The control arm pair-centered-reinforce/v1 \
     runs on the unchanged v1 learner, whose arithmetic uses the platform math library: it is \
     replayable within one host class only, its runs and their replay stay on one class, its \
     launch records name the class, and it is a diagnostic that carries no strength claim.",
    "Host class: the tuple (CPU model, OS family, target triple, trainer binary SHA-256), \
     written in full, with the short name win11-13700k, win11-5900xt, linux-9950x or \
     runpod-epyc-<vcpus>. A qualification receipt is valid for its class only; other hosts \
     run the workload as engineering verification only.",
    "Gate 2 pairs: pair indices 0 to 7 at seed 657_001. If the set lacks FDN_top_09446_URpG \
     or lacks every Drake Hatcher train deck, index 7 (then 6) is replaced by the lowest index \
     of 8 or more whose pair contains the missing deck. The rule and the indices are recorded \
     in campaign.json and the qualification manifest.",
    "Guarded launcher: a campaign of more than one update requires a qualification manifest \
     whose binary_source_sha256, feature_sha256, card_db_hash, learner contract, host class, \
     U_max (100 to 200) and W* (at least 2) match the run.",
    "Publication: each update writes its receipt, then checkpoints/step_UUUU.json by staging, \
     sync and rename with an immediate readback restore, then the dev panel at a milestone and \
     complete.json, then renames the iteration directory into place. Resume verifies the whole \
     chain from step_0000 and continues with the bytes an uninterrupted run writes.",
];

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
