use super::*;

fn assert_close(actual: f32, expected: f32, epsilon: f32) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "actual {actual} vs expected {expected} (epsilon {epsilon})"
    );
}

/// One-decision episode. Every dyadic (power-of-two-denominator) input below
/// is exactly representable in `f32`, so the whole recursion is exact and
/// every assertion here is `assert_eq!`, not an approximation.
#[test]
fn gae_one_decision_episode_hand_computed_golden() {
    let outputs = gae_episode_advantages_v1(&[(0.25, true)], 1.0, 0.5, 0.5).unwrap();
    // delta_0 = 1.0 + 0.5*0 - 0.25 = 0.75; A_0 = 0.75 + 0.5*0.5*0 = 0.75
    // value_target_0 = 0.75 + 0.25 = 1.0
    assert_eq!(outputs, vec![(0.75, 1.0)]);
}

/// Two-decision episode, hand-computed golden (see module doc for the
/// arithmetic; every value is dyadic, so this is exact).
#[test]
fn gae_two_decision_episode_hand_computed_golden() {
    let outputs =
        gae_episode_advantages_v1(&[(0.25, false), (-0.5, true)], 1.0, 0.5, 0.5).unwrap();
    // g=1 (final): delta_1 = 1.0 + 0.5*0 - (-0.5) = 1.5; A_1 = 1.5; vt_1 = 1.0
    // g=0: delta_0 = 0 + 0.5*(-0.5) - 0.25 = -0.5; A_0 = -0.5 + 0.5*0.5*1.5 = -0.125
    //      vt_0 = -0.125 + 0.25 = 0.125
    assert_eq!(outputs, vec![(-0.125, 0.125), (1.5, 1.0)]);
}

/// Three-decision episode, hand-computed golden (see module doc; dyadic
/// inputs throughout, exact).
#[test]
fn gae_three_decision_episode_hand_computed_golden() {
    let outputs = gae_episode_advantages_v1(
        &[(0.25, false), (-0.5, false), (0.125, true)],
        1.0,
        0.5,
        0.5,
    )
    .unwrap();
    // g=2 (final): delta_2 = 1.0 - 0.125 = 0.875; A_2 = 0.875; vt_2 = 1.0
    // g=1: delta_1 = 0.5*0.125 - (-0.5) = 0.5625; A_1 = 0.5625 + 0.25*0.875 = 0.78125
    //      vt_1 = 0.78125 - 0.5 = 0.28125
    // g=0: delta_0 = 0.5*(-0.5) - 0.25 = -0.5; A_0 = -0.5 + 0.25*0.78125 = -0.3046875
    //      vt_0 = -0.3046875 + 0.25 = -0.0546875
    assert_eq!(
        outputs,
        vec![(-0.3046875, -0.0546875), (0.78125, 0.28125), (0.875, 1.0)]
    );
}

/// The regression test tying the two loss identities together (design
/// section 2): at `gamma = 1.0, lambda = 1.0` the recursion telescopes to
/// exactly today's v3 formula, `advantage = terminal_return - value` and
/// `value_target = terminal_return`, for every decision regardless of its
/// position. Dyadic inputs keep this an exact (`assert_eq!`) check, not an
/// approximation, across several episode lengths.
#[test]
fn gae_reduces_to_terminal_reinforce_value_v3_at_gamma_lambda_one() {
    for values in [
        vec![0.125],
        vec![0.125, -0.375],
        vec![0.125, -0.375, 0.25, 0.0625],
        vec![0.5, 0.5, -0.5, 0.5, -0.25, 0.0],
    ] {
        let terminal_return = 0.5f32;
        let n = values.len();
        let decisions: Vec<(f32, bool)> = values
            .iter()
            .enumerate()
            .map(|(index, value)| (*value, index == n - 1))
            .collect();
        let outputs =
            gae_episode_advantages_v1(&decisions, terminal_return, 1.0, 1.0).unwrap();
        for (index, (advantage, value_target)) in outputs.iter().enumerate() {
            assert_eq!(
                *advantage,
                terminal_return - values[index],
                "advantage at {index} must equal terminal_return - value at gamma=lambda=1"
            );
            assert_eq!(
                *value_target, terminal_return,
                "value_target at {index} must equal terminal_return at gamma=lambda=1"
            );
        }
    }
}

/// At `lambda = 0.0`, `value_target_g` reduces to the one-step TD target
/// `gamma * V_{g+1}` for every non-final decision, and to `terminal_return`
/// for the final one (design section 2). Dyadic inputs keep this exact.
#[test]
fn gae_reduces_to_one_step_td_target_at_lambda_zero() {
    let values = [0.25f32, -0.125, 0.0625];
    let gamma = 0.5f32;
    let terminal_return = 1.0f32;
    let decisions: Vec<(f32, bool)> = values
        .iter()
        .enumerate()
        .map(|(index, value)| (*value, index == values.len() - 1))
        .collect();
    let outputs = gae_episode_advantages_v1(&decisions, terminal_return, gamma, 0.0).unwrap();
    for index in 0..values.len() - 1 {
        assert_eq!(
            outputs[index].1,
            gamma * values[index + 1],
            "non-final value_target at {index} must equal gamma * V_(g+1) at lambda=0"
        );
    }
    assert_eq!(outputs[values.len() - 1].1, terminal_return);
}

#[test]
fn gae_rejects_invalid_inputs() {
    assert!(gae_episode_advantages_v1(&[], 1.0, 0.5, 0.5).is_err());
    assert!(gae_episode_advantages_v1(&[(0.1, true)], 1.0, 1.5, 0.5).is_err());
    assert!(gae_episode_advantages_v1(&[(0.1, true)], 1.0, -0.1, 0.5).is_err());
    assert!(gae_episode_advantages_v1(&[(0.1, true)], 1.0, 0.5, 1.5).is_err());
    assert!(gae_episode_advantages_v1(&[(0.1, true)], 1.0, 0.5, -0.1).is_err());
    assert!(gae_episode_advantages_v1(&[(0.1, true)], f32::NAN, 0.5, 0.5).is_err());
    assert!(gae_episode_advantages_v1(&[(f32::NAN, true)], 1.0, 0.5, 0.5).is_err());
    // Zero final decisions.
    assert!(gae_episode_advantages_v1(&[(0.1, false), (0.2, false)], 1.0, 0.5, 0.5).is_err());
    // Two final decisions.
    assert!(gae_episode_advantages_v1(&[(0.1, true), (0.2, true)], 1.0, 0.5, 0.5).is_err());
    // Final decision not last.
    assert!(gae_episode_advantages_v1(&[(0.1, true), (0.2, false)], 1.0, 0.5, 0.5).is_err());
}

/// Task 4's synthetic batch, hand-computed mean/std/min/max. `raw =
/// [1.0, 2.0, 3.0, 4.0]`: mean 2.5, variance 1.25 (population, divide by
/// count), std sqrt(1.25) ~= 1.1180339887; epsilon is negligible next to
/// that std, so the normalized series is very close to the textbook
/// z-score `(x - 2.5) / 1.1180339887` = `[-1.3416407, -0.4472136,
/// 0.4472136, 1.3416407]`, and its own mean/std land at ~0/~1 by
/// construction (not exactly, since the denominator is `std + epsilon`,
/// not `std`).
#[test]
fn normalize_advantages_hand_computed_golden() {
    let (normalized, statistics) = normalize_advantages_v1(&[1.0, 2.0, 3.0, 4.0], 1.0e-6).unwrap();
    let expected = [-1.3416407, -0.4472136, 0.4472136, 1.3416407];
    for (actual, expected) in normalized.iter().zip(expected) {
        assert_close(*actual, expected, 1.0e-5);
    }
    assert_close(statistics.mean, 0.0, 1.0e-5);
    assert_close(statistics.std, 1.0, 1.0e-4);
    assert_close(statistics.min, -1.3416407, 1.0e-5);
    assert_close(statistics.max, 1.3416407, 1.0e-5);
    assert_eq!(statistics.clip_fraction, 0.0);
}

#[test]
fn normalize_advantages_rejects_invalid_inputs() {
    assert!(normalize_advantages_v1(&[], 1.0e-6).is_err());
    assert!(normalize_advantages_v1(&[1.0], 0.0).is_err());
    assert!(normalize_advantages_v1(&[1.0], -1.0e-6).is_err());
    assert!(normalize_advantages_v1(&[f32::NAN], 1.0e-6).is_err());
}

/// A constant series normalizes to all zeros (std is exactly `0.0`, so the
/// epsilon guard is the whole denominator): proves the guard actually
/// prevents a division by zero rather than merely being present in the
/// formula.
#[test]
fn normalize_advantages_constant_series_uses_epsilon_guard() {
    let (normalized, statistics) = normalize_advantages_v1(&[2.0, 2.0, 2.0], 1.0e-6).unwrap();
    assert_eq!(normalized, vec![0.0, 0.0, 0.0]);
    assert_eq!(statistics.mean, 0.0);
    assert_eq!(statistics.std, 0.0);
    assert_eq!(statistics.min, 0.0);
    assert_eq!(statistics.max, 0.0);
}
