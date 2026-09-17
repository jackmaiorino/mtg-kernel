//! Pure generalized-advantage-estimation (GAE) math for one episode's
//! learner physical decisions, plus the whole-update normalization and
//! summary statistics built on top of it. See
//! `sideboarding-integration-20260912/phase1/TRAINING-SIGNAL-DESIGN-001.md`
//! section 2 for the design this implements, and section 5 task 1 for this
//! module's scope. No trajectory, tensor, or training-loop type is imported
//! here: every function takes plain scalars and returns plain scalars, so it
//! is testable without any model, forward pass, or file I/O.
//!
//! Loss identity string for the new estimator (section 2, section 6 risk
//! "Naming collision"): `"gae_advantage_value/v1"`. Grepped across the whole
//! tree before landing; it does not collide with the unrelated cycle-4
//! lane's `"terminal_reinforce_value/v4-candidate"`.
use super::*;

pub(crate) const GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1: &str = "gae_advantage_value/v1";

/// One episode's GAE advantages and lambda-return value targets, computed
/// backward per the design's math block (section 2):
///
/// ```text
/// delta_g = reward_g + gamma * V_{g+1} - V_g            (V_n := 0)
/// A_g     = delta_g + gamma * lambda * A_{g+1}           (A_n := 0)
/// value_target_g = A_g + V_g
/// ```
///
/// `decisions` is one episode's learner physical decisions in order, each
/// `(value, is_final)`: `value` is the current-parameter value estimate for
/// that physical decision (today's `tapes[0].tape.value_v1()`, or the
/// bit-exact equivalent `expected_value_bits` already validated to
/// reproduce it), and `is_final` marks the episode's one learner decision
/// that receives `terminal_return` as its reward (every other decision's
/// reward is implicitly zero). Exactly one entry must be final, and it must
/// be the last entry: both are checked, not assumed.
///
/// Returns `(advantage, value_target)` pairs in the same order as
/// `decisions`. This is a strict generalization of the existing v3 formula:
/// at `gamma = 1.0, lambda = 1.0` it reduces exactly to
/// `advantage = terminal_return - value`, `value_target = terminal_return`
/// for every decision (`gae_reduces_to_terminal_reinforce_value_v3_at_gamma_lambda_one`
/// below). At `lambda = 0.0` `value_target` reduces to the one-step TD
/// target `gamma * V_{g+1}` for every non-final decision, and to
/// `terminal_return` for the final one
/// (`gae_reduces_to_one_step_td_target_at_lambda_zero` below).
pub(crate) fn gae_episode_advantages_v1(
    decisions: &[(f32, bool)],
    terminal_return: f32,
    gamma: f32,
    lambda: f32,
) -> Result<Vec<(f32, f32)>, String> {
    ensure(
        !decisions.is_empty(),
        "gae episode requires at least one physical decision",
    )?;
    ensure(
        gamma.is_finite() && (0.0..=1.0).contains(&gamma),
        "gae gamma must be finite in 0.0..=1.0",
    )?;
    ensure(
        lambda.is_finite() && (0.0..=1.0).contains(&lambda),
        "gae lambda must be finite in 0.0..=1.0",
    )?;
    ensure(
        terminal_return.is_finite(),
        "gae terminal_return must be finite",
    )?;
    let final_count = decisions.iter().filter(|(_, is_final)| *is_final).count();
    ensure(
        final_count == 1,
        "gae episode must have exactly one final decision",
    )?;
    ensure(
        decisions
            .last()
            .map(|(_, is_final)| *is_final)
            .unwrap_or(false),
        "gae episode's final decision must be its last decision",
    )?;
    let mut outputs = vec![(0.0f32, 0.0f32); decisions.len()];
    let mut next_value = 0.0f32;
    let mut next_advantage = 0.0f32;
    for (index, (value, is_final)) in decisions.iter().enumerate().rev() {
        ensure(value.is_finite(), "gae decision value must be finite")?;
        let reward = if *is_final { terminal_return } else { 0.0 };
        let delta = reward + gamma * next_value - value;
        let advantage = delta + gamma * lambda * next_advantage;
        let value_target = advantage + value;
        outputs[index] = (advantage, value_target);
        next_value = *value;
        next_advantage = advantage;
    }
    Ok(outputs)
}

/// Advantage summary statistics carried in the update receipt (section 3),
/// gated on the new loss identity so v3 receipts gain zero new keys.
/// `clip_fraction` is always `0.0`: there is no PPO clip in v1 (section 2),
/// the field is kept in the schema only so a possible future clipped
/// variant does not need another schema change.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdvantageStatisticsV1 {
    pub(crate) mean: f32,
    pub(crate) std: f32,
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) clip_fraction: f32,
}

/// Per-update advantage normalization (section 2): subtract the update's own
/// mean, divide by its own std, with a small epsilon guard. `epsilon` is the
/// pre-registered `1e-6`. Returns the normalized advantages in the same
/// order as `raw`, plus their own summary statistics (mean/std/min/max are
/// of the *normalized* series, matching section 3).
pub(crate) fn normalize_advantages_v1(
    raw: &[f32],
    epsilon: f32,
) -> Result<(Vec<f32>, AdvantageStatisticsV1), String> {
    ensure(!raw.is_empty(), "advantage normalization requires at least one group")?;
    ensure(
        epsilon.is_finite() && epsilon > 0.0,
        "advantage normalization epsilon must be finite and positive",
    )?;
    for value in raw {
        ensure(value.is_finite(), "raw advantage must be finite")?;
    }
    let count = exact_group_count_f32_v1(raw.len())?;
    let mean = raw.iter().copied().fold(0.0f32, |sum, value| sum + value) / count;
    let variance = raw
        .iter()
        .copied()
        .fold(0.0f32, |sum, value| {
            let deviation = value - mean;
            sum + deviation * deviation
        })
        / count;
    let std = variance.sqrt();
    let denominator = std + epsilon;
    let normalized: Vec<f32> = raw.iter().map(|value| (value - mean) / denominator).collect();
    for value in &normalized {
        ensure(value.is_finite(), "normalized advantage must be finite")?;
    }
    let normalized_mean =
        normalized.iter().copied().fold(0.0f32, |sum, value| sum + value) / count;
    let normalized_variance = normalized
        .iter()
        .copied()
        .fold(0.0f32, |sum, value| {
            let deviation = value - normalized_mean;
            sum + deviation * deviation
        })
        / count;
    let normalized_std = normalized_variance.sqrt();
    let min = normalized
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let max = normalized
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    Ok((
        normalized,
        AdvantageStatisticsV1 {
            mean: normalized_mean,
            std: normalized_std,
            min,
            max,
            clip_fraction: 0.0,
        },
    ))
}

/// Same exactness proof `native_policy_train_step_v1::exact_group_count_f32`
/// already applies to the update's group-count divisor, reused here for the
/// advantage normalization's own group-count divisor: a group count that
/// cannot be represented exactly in `f32` fails closed rather than silently
/// rounding the divisor.
fn exact_group_count_f32_v1(group_count: usize) -> Result<f32, String> {
    let widened = u64::try_from(group_count).map_err(|_| "group count exceeds u64".to_string())?;
    let represented = widened as f32;
    ensure(
        represented.is_finite() && represented as u128 == u128::from(widened),
        "group count is not exactly representable in f32",
    )?;
    Ok(represented)
}

#[cfg(test)]
mod tests;
