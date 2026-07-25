//! Promotion-gate arithmetic (Self-Play Ladder Design Contract S2, Section
//! 4, GATE UNITS SUPERSEDED by Amendment 1 / Section 8A point 1): the
//! predeclared rung promotion test. No I/O, no run-record schema, no
//! evidence chain -- a small pure function over already-computed evaluation
//! outcomes, so the pilot rung (and any later rung) can reuse it unchanged.
//!
//! Two independent sub-checks, both required to promote:
//!
//! - Win-rate test: strictly greater than 55 percent over 2,048 GAMES
//!   (1,024 seat-swapped CRN pairs = 2,048 games) against the rung's
//!   PRIMARY opponent, draws counted as losses (so `wins` excludes draws;
//!   anything not a strict win falls into `games - wins`). AMENDMENT 1
//!   (Section 8A point 1): the pilot rung surfaced that this gate's own
//!   denominator is a GAME count, not a pair count -- "VERIFIED for this
//!   evaluator: 2,048 raw games ran for the declared 1,024 pairs". The
//!   pre-pilot draft's `PROMOTION_GATE_PAIR_COUNT_V1 = 1_024` was a PAIR
//!   count; feeding raw GAME wins into that 1,024-configured denominator
//!   silently DOUBLES the apparent win rate (the contract's own worked
//!   example: 992 game wins would read 96.9 percent instead of the correct
//!   48.4 percent). That constant and its 563/564 boundary fixtures are
//!   SUPERSEDED and retired from this module; every caller now feeds RAW
//!   GAME WINS over [`PROMOTION_GATE_GAME_COUNT_V1`] games, never a
//!   pair-level win count.
//! - Panel regression test: the candidate's panel/v1 mean must not regress
//!   more than 2 sigma below the previous promoted checkpoint's panel/v1
//!   mean, where sigma is the binomial standard error AT THE PREVIOUS MEAN
//!   over the panel's pair count ("no forgetting the base game"). Untouched
//!   by Amendment 1.
//!
//! `panel_pair_count` (the panel's own sample size, `n` in the binomial
//! sigma) is a caller-supplied parameter rather than a literal pinned here:
//! Section 4 names the panel/v1 mean but does not fix its pair count as part
//! of THIS gate's frozen literals (that belongs to the frozen
//! stopping/checkpoint-selection policy, 126fd81a..., out of scope for this
//! layer). Inventing an unstated constant here would misrepresent contract
//! authority this module does not have.

/// Predeclared GAME count for the win-rate test (Amendment 1 / Section 8A
/// point 1): "the candidate plays 1,024 seat-swapped CRN pairs = 2,048
/// games... gate quantity = wins / 2,048". SUPERSEDES the pre-pilot,
/// pair-denominated `PROMOTION_GATE_PAIR_COUNT_V1 = 1_024` (removed from
/// this module; do not reintroduce a pair-count denominator here -- see the
/// module docs for why doubling the win rate was the exact failure mode
/// this rename closes).
pub(crate) const PROMOTION_GATE_GAME_COUNT_V1: u64 = 2_048;

/// Predeclared win-rate threshold (Section 4): "strictly greater than 55
/// percent". The test is `wins / games > 0.55`, not `>=`.
pub(crate) const PROMOTION_GATE_WIN_RATE_THRESHOLD_V1: f64 = 0.55;

/// Predeclared panel-regression bound (Section 4): "does not regress more
/// than 2 sigma below the previous promoted checkpoint's panel/v1 mean".
pub(crate) const PROMOTION_GATE_REGRESSION_SIGMA_MULTIPLIER_V1: f64 = 2.0;

/// Win-rate sub-check: strictly greater than 55 percent of `games` are
/// strict wins (draws already folded into `games - wins` by the caller, per
/// the contract's "draws as losses" convention). `games == 0` or
/// `wins > games` fail closed. `wins` MUST be a raw GAME win count
/// (Amendment 1 / Section 8A point 1), never a pair-level win count -- see
/// the module docs for the doubling hazard of feeding the wrong unit here.
pub(crate) fn promotion_gate_win_rate_passes_v1(wins: u64, games: u64) -> bool {
    if games == 0 || wins > games {
        return false;
    }
    (wins as f64 / games as f64) > PROMOTION_GATE_WIN_RATE_THRESHOLD_V1
}

/// The binomial standard error at probability `mean` over `pair_count`
/// trials: `sqrt(mean * (1 - mean) / pair_count)`.
pub(crate) fn binomial_sigma_v1(mean: f64, pair_count: u64) -> Option<f64> {
    if pair_count == 0 || !(0.0..=1.0).contains(&mean) || !mean.is_finite() {
        return None;
    }
    Some((mean * (1.0 - mean) / (pair_count as f64)).sqrt())
}

/// Panel-regression sub-check: the candidate's panel mean must not fall more
/// than 2 sigma (computed at the PREVIOUS mean) below the previous mean.
/// Exactly 2 sigma below passes (non-strict `>=`); anything beyond fails.
/// `None` on invalid inputs (fails closed at the call site: an invalid
/// panel-mean input can never authorize a promotion).
pub(crate) fn promotion_gate_panel_regression_passes_v1(
    previous_panel_mean: f64,
    candidate_panel_mean: f64,
    panel_pair_count: u64,
) -> Option<bool> {
    if !(0.0..=1.0).contains(&candidate_panel_mean) || !candidate_panel_mean.is_finite() {
        return None;
    }
    let sigma = binomial_sigma_v1(previous_panel_mean, panel_pair_count)?;
    let floor = previous_panel_mean - PROMOTION_GATE_REGRESSION_SIGMA_MULTIPLIER_V1 * sigma;
    Some(candidate_panel_mean >= floor)
}

/// The complete predeclared promotion gate (Section 4, win-rate units per
/// Amendment 1 / Section 8A point 1): both sub-checks must pass. `wins` and
/// `games` MUST be raw game-level counts. `None` propagates an invalid
/// panel-regression input (fails closed: the caller must treat `None` as "do
/// not promote", never as "gate inapplicable").
pub(crate) fn promotion_gate_passes_v1(
    wins: u64,
    games: u64,
    previous_panel_mean: f64,
    candidate_panel_mean: f64,
    panel_pair_count: u64,
) -> Option<bool> {
    let win_rate_ok = promotion_gate_win_rate_passes_v1(wins, games);
    let regression_ok = promotion_gate_panel_regression_passes_v1(
        previous_panel_mean,
        candidate_panel_mean,
        panel_pair_count,
    )?;
    Some(win_rate_ok && regression_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Win-rate boundary fixtures (Amendment 1 / Section 8A point 1,
    // re-pinned from the pre-pilot 563/1024-564/1024 pair-count boundary):
    // 1,126/2,048 fails, 1,127/2,048 passes.
    // ------------------------------------------------------------------

    #[test]
    fn win_rate_boundary_1126_fails_1127_passes() {
        assert!(!promotion_gate_win_rate_passes_v1(
            1_126,
            PROMOTION_GATE_GAME_COUNT_V1
        ));
        assert!(promotion_gate_win_rate_passes_v1(
            1_127,
            PROMOTION_GATE_GAME_COUNT_V1
        ));
        // Sanity: 0.55 * 2048 = 1126.4, so 1126 is exactly the highest
        // integer win count still at or below the threshold.
        assert!(
            1_126.0 / (PROMOTION_GATE_GAME_COUNT_V1 as f64) < PROMOTION_GATE_WIN_RATE_THRESHOLD_V1
        );
        assert!(
            1_127.0 / (PROMOTION_GATE_GAME_COUNT_V1 as f64) > PROMOTION_GATE_WIN_RATE_THRESHOLD_V1
        );
    }

    #[test]
    fn win_rate_exactly_55_percent_fails_the_strict_test() {
        // 550/1000 is exactly 55.0 percent: the contract requires STRICTLY
        // greater than 55 percent, so an exact tie fails.
        assert!(!promotion_gate_win_rate_passes_v1(550, 1_000));
        assert!(promotion_gate_win_rate_passes_v1(551, 1_000));
    }

    #[test]
    fn win_rate_degenerate_inputs_fail_closed() {
        assert!(!promotion_gate_win_rate_passes_v1(0, 0));
        assert!(!promotion_gate_win_rate_passes_v1(2_049, 2_048));
        assert!(!promotion_gate_win_rate_passes_v1(
            0,
            PROMOTION_GATE_GAME_COUNT_V1
        ));
        assert!(promotion_gate_win_rate_passes_v1(
            PROMOTION_GATE_GAME_COUNT_V1,
            PROMOTION_GATE_GAME_COUNT_V1
        ));
    }

    // ------------------------------------------------------------------
    // Panel-regression boundary fixtures (contract Section 7): exactly at 2
    // sigma passes, beyond fails.
    // ------------------------------------------------------------------

    #[test]
    fn panel_regression_exactly_two_sigma_passes_beyond_fails() {
        let previous_mean = 0.60_f64;
        let panel_pairs = 256_u64;
        let sigma = binomial_sigma_v1(previous_mean, panel_pairs).unwrap();
        let exactly_two_sigma_below = previous_mean - 2.0 * sigma;
        let just_beyond_two_sigma = previous_mean - 2.0 * sigma - 1e-9;

        assert_eq!(
            promotion_gate_panel_regression_passes_v1(
                previous_mean,
                exactly_two_sigma_below,
                panel_pairs
            ),
            Some(true)
        );
        assert_eq!(
            promotion_gate_panel_regression_passes_v1(
                previous_mean,
                just_beyond_two_sigma,
                panel_pairs
            ),
            Some(false)
        );
        // A candidate that matches or beats the previous mean always passes.
        assert_eq!(
            promotion_gate_panel_regression_passes_v1(previous_mean, previous_mean, panel_pairs),
            Some(true)
        );
        assert_eq!(
            promotion_gate_panel_regression_passes_v1(previous_mean, 1.0, panel_pairs),
            Some(true)
        );
    }

    #[test]
    fn panel_regression_invalid_inputs_fail_closed_via_none() {
        assert_eq!(promotion_gate_panel_regression_passes_v1(0.6, 0.5, 0), None);
        assert_eq!(
            promotion_gate_panel_regression_passes_v1(1.5, 0.5, 256),
            None
        );
        assert_eq!(
            promotion_gate_panel_regression_passes_v1(0.6, f64::NAN, 256),
            None
        );
        assert_eq!(
            promotion_gate_panel_regression_passes_v1(f64::NAN, 0.5, 256),
            None
        );
    }

    #[test]
    fn binomial_sigma_matches_hand_computation_and_rejects_out_of_range() {
        // p = 0.5, n = 100 -> sigma = sqrt(0.25 / 100) = 0.05 exactly.
        assert_eq!(binomial_sigma_v1(0.5, 100), Some(0.05));
        // p = 0 or p = 1 -> sigma = 0 (no variance).
        assert_eq!(binomial_sigma_v1(0.0, 100), Some(0.0));
        assert_eq!(binomial_sigma_v1(1.0, 100), Some(0.0));
        assert_eq!(binomial_sigma_v1(-0.1, 100), None);
        assert_eq!(binomial_sigma_v1(1.1, 100), None);
        assert_eq!(binomial_sigma_v1(0.5, 0), None);
    }

    // ------------------------------------------------------------------
    // Combined gate: both sub-checks must pass; either failing fails the
    // whole gate; an invalid regression input fails closed via None.
    // ------------------------------------------------------------------

    #[test]
    fn combined_gate_requires_both_subchecks() {
        let panel_pairs = 256_u64;
        let previous_mean = 0.60_f64;
        let sigma = binomial_sigma_v1(previous_mean, panel_pairs).unwrap();
        let passing_panel_mean = previous_mean - 2.0 * sigma;
        let failing_panel_mean = previous_mean - 2.0 * sigma - 1e-9;

        // Win rate passes, regression passes -> promote.
        assert_eq!(
            promotion_gate_passes_v1(
                1_127,
                PROMOTION_GATE_GAME_COUNT_V1,
                previous_mean,
                passing_panel_mean,
                panel_pairs
            ),
            Some(true)
        );
        // Win rate fails, regression passes -> do not promote.
        assert_eq!(
            promotion_gate_passes_v1(
                1_126,
                PROMOTION_GATE_GAME_COUNT_V1,
                previous_mean,
                passing_panel_mean,
                panel_pairs
            ),
            Some(false)
        );
        // Win rate passes, regression fails -> do not promote.
        assert_eq!(
            promotion_gate_passes_v1(
                1_127,
                PROMOTION_GATE_GAME_COUNT_V1,
                previous_mean,
                failing_panel_mean,
                panel_pairs
            ),
            Some(false)
        );
        // Invalid regression input fails closed via None, regardless of the
        // win-rate outcome.
        assert_eq!(
            promotion_gate_passes_v1(1_127, PROMOTION_GATE_GAME_COUNT_V1, previous_mean, 0.5, 0),
            None
        );
    }
}
