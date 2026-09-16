//! Paired BO1 win-rate estimator (design section 2, W2): a candidate and
//! its incumbent are played against the same opponent under the identical
//! `pair_environment_seed`, so shared randomness cancels in the paired
//! difference. This module owns only the estimator (the trial runner and
//! the bootstrap), not the search loop that calls it (Task E) and not the
//! candidate generator (also Task E).

use crate::ids::PlayerId;
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, RlSessionError,
};
use crate::state::SplitMix64;

/// The policy's only access to a live episode is its actor-relative scorer
/// projection. The underlying session and hidden game state stay private.
pub struct PairedBo1PolicyInputV1<'a> {
    session: &'a FastActorSessionV1,
    decision: FastActorDecisionV1,
}

impl<'a> PairedBo1PolicyInputV1<'a> {
    pub(crate) fn new(session: &'a FastActorSessionV1, decision: FastActorDecisionV1) -> Self {
        Self { session, decision }
    }

    pub fn decision(&self) -> FastActorDecisionV1 {
        self.decision
    }

    /// Trusted BO3 recording uses the same bound actor projection as scoring.
    /// This deliberately does not expose the session or either hidden hand.
    pub(crate) fn capture_bo3_gameplay_v1(
        &self,
        decision_index: u64,
        package_sha256: String,
        behavior: crate::phase1_agent_v1::BehaviorDistributionV1,
    ) -> Result<crate::phase1_agent_v1::Bo3DecisionRecordV1, String> {
        if self.session.current_response() != FastActorResponseV1::Decision(self.decision) {
            return Err("BO3 capture differs from the scoring decision binding".into());
        }
        crate::phase1_agent_v1::Bo3DecisionRecordV1::gameplay_from_session_v1(
            decision_index,
            package_sha256,
            behavior,
            self.session,
        )
    }

    pub(crate) fn encode_scoring_owned_v3(
        &self,
        encoder: &mut crate::flat_policy_v3::FlatDecisionEncoderV3,
        buffers: &mut crate::flat_policy_v2::FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<crate::flat_policy_v3::FlatDecisionV3, crate::flat_policy_v2::FlatDecisionErrorV2>
    {
        self.session
            .encode_current_flat_scoring_decision_owned_v3(self.decision, encoder, buffers)
    }

    /// V4 sibling of `encode_scoring_owned_v3`, for a policy whose
    /// `feature_generation_v1()` is `PlayPolicyGenerationV1::V4`.
    pub(crate) fn encode_scoring_owned_v4(
        &self,
        encoder: &mut crate::flat_policy_v4::FlatDecisionEncoderV4,
        buffers: &mut crate::flat_policy_v2::FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<crate::flat_policy_v4::FlatDecisionV4, crate::flat_policy_v2::FlatDecisionErrorV2>
    {
        self.session
            .encode_current_flat_scoring_decision_owned_v4(self.decision, encoder, buffers)
    }

    pub(crate) fn encode_scoring_owned_v2(
        &self,
        encoder: &mut crate::flat_policy_v2::FlatDecisionEncoderV2,
        buffers: &mut crate::flat_policy_v2::FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<crate::flat_policy_v2::FlatDecisionV2, crate::flat_policy_v2::FlatDecisionErrorV2>
    {
        self.session
            .encode_current_flat_scoring_decision_owned_v2(self.decision, encoder, buffers)
    }
}

/// Tri-state generation a paired-BO1 policy actually scores/pairs under. `V2`
/// is the legacy default; `V3` and `V4` are the two explicit inference
/// feature-transfer generations, each with independently recorded identities.
/// Never widen this into an open-ended allow-list: mixed-generation seat
/// pairings must be rejected by strict equality of this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayPolicyGenerationV1 {
    V2,
    V3,
    V4,
}

/// A frozen play policy with explicitly reset per-seat sampling streams.
/// Implementations reset recurrent state and both sampling streams at every
/// game boundary. Cache keys must include the episode's observation identity.
pub trait PairedBo1PolicyV1 {
    /// The default preserves every existing V2 consumer. V3 (and, since the
    /// fresh-lineage V4 sibling landed, V4 too) is explicit inference feature
    /// transfer, with independently recorded identities. Both use the same
    /// wide session/environment constructor (`play_one_side_v1` below), so
    /// this stays a boolean; `feature_generation_v1` is the tri-state that
    /// tells V3 and V4 apart for pairing/equality checks.
    fn uses_observation_successor_v3(&self) -> bool {
        false
    }

    /// True generation, derived from the boolean by default so no existing
    /// V2/V3-only implementor needs to change. Only a policy that can
    /// actually track fresh-lineage (V4) state needs to override this;
    /// mixed-generation seat pairings must compare this, never the boolean,
    /// since two different generations both report `true` for the boolean.
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        if self.uses_observation_successor_v3() {
            PlayPolicyGenerationV1::V3
        } else {
            PlayPolicyGenerationV1::V2
        }
    }

    fn reset_for_game_v1(&mut self, policy_seeds: [u64; 2]) -> Result<(), RlSessionError>;

    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError>;
}

/// Fixed domain separation from the environment seed and independent seat
/// streams. Candidate and incumbent receive the same pair of policy seeds.
pub fn paired_policy_seeds_v1(pair_environment_seed: u64) -> [u64; 2] {
    let mut rng = SplitMix64::seed(pair_environment_seed ^ 0x5041_4952_504f_4c31);
    [rng.next_u64(), rng.next_u64()]
}

#[cfg(test)]
pub(crate) mod policy_test_support {
    use super::*;

    pub(crate) struct SeededRandomBo1PolicyV1 {
        rng: [SplitMix64; 2],
        pub(crate) resets: Vec<[u64; 2]>,
        pub(crate) traces: Vec<Vec<(crate::rl::PlayerSeatV1, u32)>>,
    }

    impl Default for SeededRandomBo1PolicyV1 {
        fn default() -> Self {
            Self {
                rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
                resets: Vec::new(),
                traces: Vec::new(),
            }
        }
    }

    impl PairedBo1PolicyV1 for SeededRandomBo1PolicyV1 {
        fn reset_for_game_v1(&mut self, policy_seeds: [u64; 2]) -> Result<(), RlSessionError> {
            self.rng = policy_seeds.map(SplitMix64::seed);
            self.resets.push(policy_seeds);
            self.traces.push(Vec::new());
            Ok(())
        }

        fn select_action_v1(
            &mut self,
            input: PairedBo1PolicyInputV1<'_>,
        ) -> Result<u32, RlSessionError> {
            let decision = input.decision();
            let seat = match decision.acting_player {
                crate::rl::PlayerSeatV1::P0 => 0,
                crate::rl::PlayerSeatV1::P1 => 1,
            };
            let selected = (self.rng[seat].next_u64() as u32) % decision.legal_action_count;
            self.traces
                .last_mut()
                .unwrap()
                .push((decision.acting_player, selected));
            Ok(selected)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairedTrialOutcomeV1 {
    pub delta: i8,
}

pub fn run_paired_bo1_trial_v1(
    candidate_mainboard: &[u16],
    incumbent_mainboard: &[u16],
    opponent_mainboard: &[u16],
    candidate_and_incumbent_seat: PlayerId,
    pair_environment_seed: u64,
    starting_player: PlayerId,
    max_physical_decisions: u64,
    policy: &mut dyn PairedBo1PolicyV1,
) -> Result<PairedTrialOutcomeV1, RlSessionError> {
    let candidate_win = play_one_side_v1(
        candidate_mainboard,
        opponent_mainboard,
        candidate_and_incumbent_seat,
        pair_environment_seed,
        starting_player,
        max_physical_decisions,
        policy,
    )?;
    let incumbent_win = play_one_side_v1(
        incumbent_mainboard,
        opponent_mainboard,
        candidate_and_incumbent_seat,
        pair_environment_seed,
        starting_player,
        max_physical_decisions,
        policy,
    )?;
    let delta = i8::from(candidate_win) - i8::from(incumbent_win);
    Ok(PairedTrialOutcomeV1 { delta })
}

fn play_one_side_v1(
    self_mainboard: &[u16],
    opponent_mainboard: &[u16],
    self_seat: PlayerId,
    pair_environment_seed: u64,
    starting_player: PlayerId,
    max_physical_decisions: u64,
    policy: &mut dyn PairedBo1PolicyV1,
) -> Result<bool, RlSessionError> {
    let mainboards = match self_seat {
        PlayerId::P0 => [self_mainboard.to_vec(), opponent_mainboard.to_vec()],
        PlayerId::P1 => [opponent_mainboard.to_vec(), self_mainboard.to_vec()],
        other => panic!("unsupported seat {}", other.0),
    };
    let deck_ids = ["candidate_or_incumbent".to_owned(), "opponent".to_owned()];
    let constructor = if policy.uses_observation_successor_v3() {
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1
    } else {
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1
    };
    let mut session = constructor(
        1,
        pair_environment_seed,
        max_physical_decisions,
        max_physical_decisions.saturating_mul(128).max(1),
        deck_ids,
        mainboards,
        starting_player,
    )?;
    policy.reset_for_game_v1(paired_policy_seeds_v1(pair_environment_seed))?;
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                if terminal.terminal_classification != crate::rl::TerminalClassificationV1::Natural
                {
                    return Err(RlSessionError {
                        code: crate::rl_session::RlSessionErrorCode::NonNaturalTerminal,
                        message: format!(
                            "paired BO1 game has non-natural terminal {:?}/{:?}",
                            terminal.terminal_classification, terminal.terminal_outcome,
                        ),
                    });
                }
                return Ok(terminal.winner == Some(self_seat.into()));
            }
            FastActorResponseV1::Decision(decision) => {
                let selected_index =
                    policy.select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))?;
                session.step(decision.episode_id, decision.step, selected_index)?;
            }
        }
    }
}

pub fn paired_mean_delta_v1(outcomes: &[PairedTrialOutcomeV1]) -> f64 {
    assert!(
        !outcomes.is_empty(),
        "mean delta is undefined over zero trials"
    );
    outcomes
        .iter()
        .map(|outcome| f64::from(outcome.delta))
        .sum::<f64>()
        / outcomes.len() as f64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapSidednessV1 {
    OneSidedLower,
    TwoSided,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairedBootstrapResultV1 {
    pub mean: f64,
    pub lower: f64,
    pub upper: f64,
    pub resample_count: u32,
    pub seed: u64,
    pub sidedness: BootstrapSidednessV1,
}

/// Case resampling with replacement over the paired per-seed deltas
/// (design section 4's manifest field: "resampling method"). `resample_count`
/// and `seed` are always caller-supplied, never a library default, per this
/// plan's Global Constraints (single-shot discipline). Delegates to
/// `paired_bootstrap_ci_with_alpha_v1` at `alpha = 0.05` (the one-sided 5th
/// percentile and the two-sided 2.5/97.5 percentiles this function always
/// used before fix round 1 added the alpha parameter); behavior at this
/// fixed alpha is unchanged.
pub fn paired_bootstrap_ci_v1(
    deltas: &[i8],
    resample_count: u32,
    seed: u64,
    sidedness: BootstrapSidednessV1,
) -> PairedBootstrapResultV1 {
    paired_bootstrap_ci_with_alpha_v1(deltas, resample_count, seed, sidedness, 0.05)
}

/// Same estimator as `paired_bootstrap_ci_v1`, generalized to an explicit
/// `alpha` (fix round 1, item 3: the manifest's `bo1_one_sided_alpha` and
/// `bo3_confidence_level` fields must actually govern the accept/reject
/// boundary, not be hashed and ignored). `alpha` is the one-sided lower
/// tail probability for `BootstrapSidednessV1::OneSidedLower` (the `alpha`
/// percentile order statistic), and the two-sided total tail probability
/// for `BootstrapSidednessV1::TwoSided` (the `alpha / 2` and
/// `1 - alpha / 2` order statistics). Callers passing `manifest.bo1_one_sided_alpha`
/// or `1.0 - manifest.bo3_confidence_level` must first load the manifest
/// through `load_and_verify_manifest_v1`, which refuses any value outside
/// `(0, 1)`; this function additionally asserts the same bound, since it is
/// also callable directly (as the tests below do) without going through
/// that gate.
pub fn paired_bootstrap_ci_with_alpha_v1(
    deltas: &[i8],
    resample_count: u32,
    seed: u64,
    sidedness: BootstrapSidednessV1,
    alpha: f64,
) -> PairedBootstrapResultV1 {
    assert!(
        !deltas.is_empty(),
        "bootstrap is undefined over zero deltas"
    );
    assert!(resample_count > 0, "resample_count must be positive");
    assert!(
        alpha > 0.0 && alpha < 1.0,
        "alpha must lie strictly inside (0, 1)"
    );
    let mean = deltas.iter().map(|&delta| f64::from(delta)).sum::<f64>() / deltas.len() as f64;
    let mut rng = SplitMix64::seed(seed);
    let mut resample_means: Vec<f64> = Vec::with_capacity(resample_count as usize);
    for _ in 0..resample_count {
        let mut sum = 0.0f64;
        for _ in 0..deltas.len() {
            let index = (rng.next_u64() as usize) % deltas.len();
            sum += f64::from(deltas[index]);
        }
        resample_means.push(sum / deltas.len() as f64);
    }
    resample_means.sort_by(|a, b| a.partial_cmp(b).expect("resample means are never NaN"));
    let (lower, upper) = match sidedness {
        BootstrapSidednessV1::OneSidedLower => {
            let alpha_index = ((resample_count as f64) * alpha).floor() as usize;
            (
                resample_means[alpha_index.min(resample_means.len() - 1)],
                f64::INFINITY,
            )
        }
        BootstrapSidednessV1::TwoSided => {
            let lower_index = ((resample_count as f64) * (alpha / 2.0)).floor() as usize;
            let upper_index = ((resample_count as f64) * (1.0 - alpha / 2.0)).floor() as usize;
            (
                resample_means[lower_index.min(resample_means.len() - 1)],
                resample_means[upper_index.min(resample_means.len() - 1)],
            )
        }
    };
    PairedBootstrapResultV1 {
        mean,
        lower,
        upper,
        resample_count,
        seed,
        sidedness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;

    #[test]
    fn paired_mean_delta_matches_hand_computed_value() {
        let outcomes = [1, 1, 0, -1, 1].map(|delta| PairedTrialOutcomeV1 { delta });
        assert!((paired_mean_delta_v1(&outcomes) - 0.4).abs() < 1e-12);
    }

    #[test]
    fn paired_bootstrap_ci_one_sided_lower_bound_is_below_the_sample_mean_and_reproducible() {
        let deltas = [1i8, 1, 1, 0, -1, 1, 1, 0, 1, 1];
        let a = paired_bootstrap_ci_v1(&deltas, 2000, 42, BootstrapSidednessV1::OneSidedLower);
        let b = paired_bootstrap_ci_v1(&deltas, 2000, 42, BootstrapSidednessV1::OneSidedLower);
        assert_eq!(
            a, b,
            "same seed and resample_count must reproduce bit-identically"
        );
        assert!(
            a.lower <= a.mean,
            "the one-sided lower bound never exceeds the sample mean"
        );
        assert!(a.upper.is_infinite());
        // Seed 11, not 43: with only 10 paired deltas the 5th-percentile order
        // statistic is a coarse discrete value (21 possible sums), so most
        // seed pairs collide on it by chance (verified empirically: 465/498
        // alternate seeds reproduce seed 42's 0.2 lower bound). Seed 11 is a
        // fixed, deterministic counterexample confirming the estimator is
        // seed-sensitive, not a claim that every other seed differs.
        let c = paired_bootstrap_ci_v1(&deltas, 2000, 11, BootstrapSidednessV1::OneSidedLower);
        assert_ne!(
            a.lower, c.lower,
            "a different seed must not coincidentally reproduce the same bound"
        );
    }

    #[test]
    fn paired_bootstrap_ci_two_sided_brackets_the_mean_for_a_clearly_positive_sample() {
        let deltas = [1i8; 20];
        let result = paired_bootstrap_ci_v1(&deltas, 2000, 7, BootstrapSidednessV1::TwoSided);
        assert_eq!(result.mean, 1.0);
        assert_eq!(result.lower, 1.0);
        assert_eq!(result.upper, 1.0);
    }

    #[test]
    fn paired_bootstrap_ci_with_alpha_v1_moves_the_bound_and_the_unparameterized_function_delegates_at_0_05(
    ) {
        let deltas = [1i8, 1, 1, 0, -1, 1, 1, 0, 1, 1];
        let at_default_alpha = paired_bootstrap_ci_with_alpha_v1(
            &deltas,
            2000,
            42,
            BootstrapSidednessV1::OneSidedLower,
            0.05,
        );
        let at_wider_alpha = paired_bootstrap_ci_with_alpha_v1(
            &deltas,
            2000,
            42,
            BootstrapSidednessV1::OneSidedLower,
            0.10,
        );
        assert_ne!(
            at_default_alpha.lower, at_wider_alpha.lower,
            "a manifest changing bo1_one_sided_alpha from 0.05 to 0.10 must actually move the accept boundary"
        );
        assert_eq!(
            at_default_alpha,
            paired_bootstrap_ci_v1(&deltas, 2000, 42, BootstrapSidednessV1::OneSidedLower),
            "paired_bootstrap_ci_v1 must delegate to alpha = 0.05 unchanged (existing callers see identical behavior)"
        );

        let two_sided_default = paired_bootstrap_ci_with_alpha_v1(
            &deltas,
            2000,
            42,
            BootstrapSidednessV1::TwoSided,
            0.05,
        );
        let two_sided_wider = paired_bootstrap_ci_with_alpha_v1(
            &deltas,
            2000,
            42,
            BootstrapSidednessV1::TwoSided,
            0.10,
        );
        assert!(
            two_sided_default.lower != two_sided_wider.lower || two_sided_default.upper != two_sided_wider.upper,
            "a manifest changing bo3_confidence_level's derived alpha must actually move the two-sided CI"
        );
        assert_eq!(
            two_sided_default,
            paired_bootstrap_ci_v1(&deltas, 2000, 42, BootstrapSidednessV1::TwoSided),
            "paired_bootstrap_ci_v1 must delegate to alpha = 0.05 unchanged for the two-sided case too"
        );
    }

    #[test]
    #[should_panic(expected = "alpha must lie strictly inside (0, 1)")]
    fn paired_bootstrap_ci_with_alpha_v1_rejects_alpha_outside_zero_one() {
        paired_bootstrap_ci_with_alpha_v1(
            &[1i8, -1],
            10,
            1,
            BootstrapSidednessV1::OneSidedLower,
            0.0,
        );
    }

    #[test]
    fn identical_configurations_replay_identical_policy_streams_and_actions() {
        let candidate = runtime_deck_by_id("Burn").unwrap().card_ids.to_vec();
        let incumbent = runtime_deck_by_id("Burn").unwrap().card_ids.to_vec();
        let opponent = runtime_deck_by_id("Rally").unwrap().card_ids.to_vec();
        let mut policy = policy_test_support::SeededRandomBo1PolicyV1::default();
        let outcome = run_paired_bo1_trial_v1(
            &candidate,
            &incumbent,
            &opponent,
            PlayerId::P0,
            0x5050_5050_5050_5050,
            PlayerId::P0,
            2000,
            &mut policy,
        )
        .expect("paired trial completes");
        assert_eq!(outcome.delta, 0);
        assert_eq!(policy.resets.len(), 2);
        assert_eq!(policy.resets[0], policy.resets[1]);
        assert_ne!(policy.resets[0][0], policy.resets[0][1]);
        assert!(!policy.traces[0].is_empty());
        assert_eq!(policy.traces[0], policy.traces[1]);
    }

    #[test]
    fn invalid_policy_index_is_rejected_without_modulo_remapping() {
        struct InvalidPolicy;
        impl PairedBo1PolicyV1 for InvalidPolicy {
            fn reset_for_game_v1(&mut self, _: [u64; 2]) -> Result<(), RlSessionError> {
                Ok(())
            }
            fn select_action_v1(
                &mut self,
                _: PairedBo1PolicyInputV1<'_>,
            ) -> Result<u32, RlSessionError> {
                Ok(u32::MAX)
            }
        }
        let deck = runtime_deck_by_id("Rally").unwrap().card_ids;
        let result = run_paired_bo1_trial_v1(
            deck,
            deck,
            deck,
            PlayerId::P0,
            5151,
            PlayerId::P0,
            2000,
            &mut InvalidPolicy,
        );
        assert!(
            result.is_err(),
            "an invalid scorer output must not become a legal action"
        );
    }

    #[test]
    fn decision_cap_is_not_scored_as_a_paired_loss() {
        let deck = runtime_deck_by_id("Rally").unwrap().card_ids;
        let mut policy = policy_test_support::SeededRandomBo1PolicyV1::default();
        let error = run_paired_bo1_trial_v1(
            deck,
            deck,
            deck,
            PlayerId::P0,
            5151,
            PlayerId::P0,
            1,
            &mut policy,
        )
        .expect_err("a one-decision cap cannot yield a measured game outcome");
        assert_eq!(
            error.code,
            crate::rl_session::RlSessionErrorCode::NonNaturalTerminal
        );
    }
}

#[cfg(test)]
mod calibration {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;
    use std::time::Instant;

    /// Real per-game wall-clock calibration for the paired BO1 harness
    /// (design section 2's "one calibration run measures real per-game wall
    /// clock under the frozen checkpoint before any campaign-wide N... is
    /// set"). Drives a random policy (no trained checkpoint is wired by
    /// this plan; W6 substitutes the real inference policy_fn) so the
    /// number this produces is a harness-cost floor, not the final N input;
    /// the manifest's N still comes from the formula in Task E, not from
    /// this number alone.
    ///
    /// Run with this codebase's existing `--ignored`-test launcher
    /// convention (not a new `[[bin]]` target; see
    /// `kernel_native_search_calibration_runner_v1`'s module doc):
    /// `cargo test --locked -p mtg-kernel --lib paired_bo1_harness_v1::calibration::calibration_measures_real_per_game_wall_clock_v1 -- --ignored --nocapture`
    ///
    /// Records: per-game wall clock (`mean_seconds_per_paired_trial` and
    /// `mean_seconds_per_game`), the paired trial count (`game_count`), and
    /// the checkpoint identity, which here is explicitly "none: random
    /// policy" (this plan wires no trained checkpoint; W6 substitutes the
    /// real inference `policy_fn` and must re-run this calibration under
    /// that checkpoint before setting a campaign-wide N).
    #[test]
    #[ignore]
    fn calibration_measures_real_per_game_wall_clock_v1() {
        let candidate = runtime_deck_by_id("Burn").unwrap().card_ids.to_vec();
        let opponent = runtime_deck_by_id("Rally").unwrap().card_ids.to_vec();
        let game_count = 50usize;
        let mut rng = SplitMix64::seed(0xC001_C001_C001_C001);
        let started = Instant::now();
        for episode in 0..game_count {
            let mut policy = policy_test_support::SeededRandomBo1PolicyV1::default();
            run_paired_bo1_trial_v1(
                &candidate,
                &candidate,
                &opponent,
                PlayerId::P0,
                rng.next_u64(),
                PlayerId::P0,
                2000,
                &mut policy,
            )
            .unwrap_or_else(|error| panic!("calibration episode {episode} failed: {error}"));
        }
        let elapsed = started.elapsed();
        let mean_seconds_per_paired_trial = elapsed.as_secs_f64() / game_count as f64;
        let report = serde_json::json!({
            "schema": "paired_bo1_calibration/v1",
            "checkpoint_identity": "none: random policy (harness-cost floor only; W6 substitutes the real inference policy_fn and must re-calibrate under it)",
            "paired_trial_count": game_count,
            "total_seconds": elapsed.as_secs_f64(),
            "mean_seconds_per_paired_trial": mean_seconds_per_paired_trial,
            "mean_seconds_per_game": mean_seconds_per_paired_trial / 2.0,
        });
        // `cargo test -p mtg-kernel` runs test binaries with the crate
        // manifest directory (`mtg-kernel/`) as the working directory, not
        // the workspace root, so the artifact path is anchored off
        // `CARGO_MANIFEST_DIR` (compile-time) rather than a bare relative
        // path from an assumed repo-root cwd.
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docs/research/paired_bo1_calibration_2026-09-09.json"
        ));
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap())
            .expect("calibration report writes");
        eprintln!("wrote {}", path.display());
    }
}
