//! TP1 Stage B2's centered win-frequency target (collab
//! LANES/teacher-price-20261019/STAGE-B2.md section 4 item 1, arithmetic
//! version `tp1b2-m-target-v1`) and the S arm's seeded permutation (section
//! 3). Binary64, round-to-nearest-even, no fused multiply-add, on
//! `line_b_softmax_v1`'s pinned softmax.

use super::*;
use sha2::{Digest, Sha256};

pub const M_TARGET_VERSION_V1: &str = "tp1b2-m-target-v1";
pub const M_PERMUTATION_VERSION_V1: &str = "tp1b2-shuffle-v1";
/// An action is supported when it has at least this many natural backups.
pub const M_SUPPORT_MIN_V1: u64 = 8;
/// `1 / 0.25`, the frozen temperature's inverse (exact).
const M_INVERSE_TEMPERATURE_V1: f64 = 4.0;

/// One row's node statistics mapped to live actions.
#[derive(Debug, PartialEq)]
pub(super) struct LiveCountsV1 {
    pub(super) n: Vec<u64>,
    pub(super) wins: Vec<u64>,
}

/// `node_stats` (storage order, `storage_to_live` map) to live order, with
/// every width and count checked.
pub(super) fn live_counts_v1(stats: &Value, width: usize) -> Result<LiveCountsV1, String> {
    let read = |key: &str| -> Result<Vec<u64>, String> {
        stats[key]
            .as_array()
            .ok_or_else(|| format!("node statistics lack {key}"))?
            .iter()
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| format!("node statistic {key} is not a count"))
            })
            .collect()
    };
    let (n, wins, map) = (read("n")?, read("wins")?, read("storage_to_live")?);
    ensure(
        n.len() == width && wins.len() == width && map.len() == width,
        "node statistics and logits differ in width",
    )?;
    let mut seen = vec![false; width];
    let mut live = LiveCountsV1 {
        n: vec![0; width],
        wins: vec![0; width],
    };
    for (storage, &target) in map.iter().enumerate() {
        let target = usize::try_from(target).map_err(err)?;
        ensure(
            target < width && !seen[target],
            "storage-to-live is not a bijection onto the live actions",
        )?;
        seen[target] = true;
        ensure(
            wins[storage] <= n[storage],
            "node statistics have more wins than backups",
        )?;
        live.n[target] = n[storage];
        live.wins[target] = wins[storage];
    }
    Ok(live)
}

/// Supported live actions in ascending order.
pub(super) fn support_v1(counts: &LiveCountsV1) -> Vec<usize> {
    (0..counts.n.len())
        .filter(|&a| counts.n[a] >= M_SUPPORT_MIN_V1)
        .collect()
}

/// SplitMix64, as STAGE-B.md section 3 and the lane tool define it.
struct SplitMix64V1(u64);

impl SplitMix64V1 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, n)` by rejection of `x >= 2^64 - (2^64 mod n)`.
    fn below(&mut self, n: u64) -> u64 {
        let limit = u64::MAX - (u64::MAX % n + 1) % n;
        loop {
            let x = self.next();
            if x <= limit {
                return x % n;
            }
        }
    }
}

/// Ascending Fisher-Yates over positions `0..k`, seeded by the first 8 bytes
/// (little-endian) of SHA-256 of `label`.
pub(super) fn shuffle_positions_v1(label: &str, k: usize) -> Vec<usize> {
    let digest = Sha256::digest(label.as_bytes());
    let mut seed = [0_u8; 8];
    seed.copy_from_slice(&digest[..8]);
    let mut rng = SplitMix64V1(u64::from_le_bytes(seed));
    let mut positions: Vec<usize> = (0..k).collect();
    for i in 0..k.saturating_sub(1) {
        let j = i + rng.below((k - i) as u64) as usize;
        positions.swap(i, j);
    }
    positions
}

/// The arm's required permutation of the supported live actions:
/// `perm[i] = support[pi[i]]`, identity for M, the seeded shuffle for S.
pub(super) fn expected_perm_v1(
    arm: &str,
    seed: u64,
    update: u64,
    step: u64,
    substep: u32,
    support: &[usize],
) -> Result<Vec<usize>, String> {
    match arm {
        "M" => Ok(support.to_vec()),
        "S" => Ok(shuffle_positions_v1(
            &format!("tp1b2-shuffle|{seed}|{update}|{step}|{substep}"),
            support.len(),
        )
        .into_iter()
        .map(|i| support[i])
        .collect()),
        _ => Err("m-kind targets are defined for arms M and S only".into()),
    }
}

/// The centered win-frequency target of one row (section 4 item 1, steps 1
/// to 6). `perm[i]` receives support[i]'s value.
pub(super) fn m_target_v1(
    logit_bits: &[u32],
    counts: &LiveCountsV1,
    perm: &[usize],
) -> Result<LineBSoftmaxV1, String> {
    let width = logit_bits.len();
    let support = support_v1(counts);
    ensure(
        support.len() >= 2,
        "an m-kind row needs at least two supported actions",
    )?;
    let mut sorted = perm.to_vec();
    sorted.sort_unstable();
    ensure(
        sorted == support,
        "an m-kind permutation must permute the supported actions",
    )?;
    // Step 1, then the permutation: Qhat(perm[i]) = Qhat_M(support[i]).
    let mut qhat = vec![0.0_f64; width];
    for (i, &a) in support.iter().enumerate() {
        let (n, wins) = (counts.n[a] as f64, counts.wins[a] as f64);
        qhat[perm[i]] = (2.0 * wins - n) / n;
    }
    // Step 2.
    let z: Vec<f64> = logit_bits
        .iter()
        .map(|&b| f64::from(f32::from_bits(b)))
        .collect();
    let p = line_b_softmax_v1(&z).map_err(|e| format!("{e:?}"))?;
    // Steps 3 and 4: the fill, summed in ascending live order.
    let mut mass = 0.0_f64;
    for &a in &support {
        mass += p.probabilities[a];
    }
    let fill = if mass > 0.0 {
        let mut numerator = 0.0_f64;
        for &a in &support {
            numerator += p.probabilities[a] * qhat[a];
        }
        numerator / mass
    } else {
        let mut total = 0.0_f64;
        for &a in &support {
            total += qhat[a];
        }
        total / support.len() as f64
    };
    for (a, value) in qhat.iter_mut().enumerate() {
        if counts.n[a] < M_SUPPORT_MIN_V1 {
            *value = fill;
        }
    }
    // Step 5: center both terms (first maximal values).
    let mut z_max = z[0];
    let mut q_max = qhat[0];
    for a in 0..width {
        if z[a] > z_max {
            z_max = z[a];
        }
        if qhat[a] > q_max {
            q_max = qhat[a];
        }
    }
    let y: Vec<f64> = (0..width)
        .map(|a| (z[a] - z_max) + M_INVERSE_TEMPERATURE_V1 * (qhat[a] - q_max))
        .collect();
    // Step 6.
    line_b_softmax_v1(&y).map_err(|e| format!("{e:?}"))
}

/// Total variation between the target and the student's softmax.
pub(super) fn total_variation_v1(
    logit_bits: &[u32],
    target: &LineBSoftmaxV1,
) -> Result<f64, String> {
    let z: Vec<f64> = logit_bits
        .iter()
        .map(|&b| f64::from(f32::from_bits(b)))
        .collect();
    let p = line_b_softmax_v1(&z).map_err(|e| format!("{e:?}"))?;
    let mut total = 0.0_f64;
    for (a, b) in p.probabilities.iter().zip(&target.probabilities) {
        total += (a - b).abs();
    }
    Ok(0.5 * total)
}
