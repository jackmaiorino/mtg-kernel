//! Bounded, deterministic decision-time search wrapper for exactly one
//! seat's play policy in the Phase 1 yardstick evaluator (design ruling
//! 2026-09-01: a fixed model wrapped in a bounded test-time search is
//! admissible and is the intended product form for this line of work).
//!
//! Design choice, checked directly against this evaluator's own runtime
//! shape rather than assumed: this module does NOT reuse
//! `model_guided_search_core_v1` / `kernel_native_search_opponent_v1` (the
//! existing model-guided search stack, built for an earlier line). Three
//! independent reasons:
//!
//! 1. Hidden-zone redetermination for search
//!    (`FastActorSessionV1::kernel_search_redeterminized_clone_v1`) refuses
//!    any session whose `flat_action_contract_mode` is not `V2`
//!    (`rl_session.rs`'s `kernel_search_redeterminized_clone_core_v1`).
//!    Every session this evaluator builds for a V3 or V4 policy goes
//!    through `into_flat_action_v3()`
//!    (`rl_session/flat_action_v3.rs`), which sets the mode to `V3`. So
//!    that reuse path would return `UnsupportedFlatActionContract` on
//!    every call here, regardless of anything else below.
//! 2. `ModelGuidedSearchAuthorityV1` requires its `action_seed` to be one of
//!    four pre-registered values (a formal-panel discipline, by design),
//!    structurally incompatible with a distinct, deterministic seed per
//!    yardstick case.
//! 3. The only real (non-mock) leaf evaluator in that stack, and all of the
//!    V4 tensorization types it would need, are `pub(crate)` to the
//!    library and were never wired to a deterministic V4 feature-transfer
//!    forward pass (only a V1/V2-contract deterministic forward exists
//!    there today).
//!
//! So this is instead a new, small, single-seat rollout search, built
//! entirely from `FrozenPlayPolicyV1`'s already-`pub` session-scoring API
//! (`score_fast_session_v1`) and `FastActorSessionV1`'s already-`pub`
//! `Clone` and `step`. At a decision with more than one legal action: take
//! the real per-decision priors and value (computed once, exactly as the
//! unwrapped policy already computes them for its own sampled action), rank
//! a seeded, weighted-without-replacement sample of up to `budget`
//! candidate actions from their softmax prior, step a plain clone of the
//! session one ply under each candidate, and score the resulting decision's
//! value head (or read the true terminal reward, if that one step ends the
//! match) as that candidate's leaf value, negated to the root seat's
//! perspective whenever the next decision belongs to the other seat
//! (matching this codebase's existing negamax / "root-perspective value"
//! convention; see `model_guided_search_core_v1.rs` and
//! `model_guided_search_value_quantization_v1.rs`). The candidate with the
//! best backed-up value is selected.
//!
//! Known, documented simplification: the one-ply clone above is a plain
//! `FastActorSessionV1::clone()`, not redeterminized (point 1 above), so it
//! retains the session's true hidden zones during the wrapper's own
//! internal lookahead. The real per-decision input features stay exactly as
//! actor-visible as the unwrapped policy's, on the identical
//! `score_fast_session_v1` encoding path either way; only the wrapper's own
//! internal candidate comparison can see slightly more than a real decision
//! maker would, one ply ahead. This is a bounded, stated simplification
//! appropriate for a development read (never a strength claim), not a
//! general search-quality design decision; extending
//! `kernel_search_redeterminized_clone_v1` to the `V3` contract would
//! remove it, and is out of scope for this bounded change.
//!
//! Determinism: no wall-clock read anywhere in this module. Every simulated
//! branch is single-threaded and seeded from `SplitMix64` mixed
//! deterministically from the match's own case seed and the decision's own
//! `episode_id` and `step`; the same case seed always reproduces the same
//! search trace. RNG safety: simulated leaf scoring calls only
//! `FrozenPlayPolicyV1::score_fast_session_v1`, never
//! `select_fast_session_v1` or `select_paired_with_scores_v1`, so it never
//! reads or advances the policy's seat-sampling RNG (`seat_rng`, touched
//! only by `sample_scores`); the wrapped policy's real decision stream is
//! therefore exactly the raw policy's stream, whether or not search ran or
//! how big its budget was. Falling back: any error stepping a simulated
//! clone, a budget of zero, or a decision with at most one legal action,
//! all return the already-computed raw policy action unchanged.
//!
//! Rollout leaf (follow-up to the r block-24 development read: the
//! committed value-head leaf above measured 87.8 points behind the V3
//! candidate on Rally mirrors, worse than the raw policy's own 23.3 behind,
//! and sign-flipping it is worse still; conclusion of record: the value
//! head is unusable as a one-ply leaf). A second, independent leaf
//! evaluator, selected by a nonzero `rollouts` count: a new field, sibling
//! to `budget`, on `EndSeatSearchRequestV1` and on this wrapper's own
//! constructor. When `rollouts > 0`, a sampled candidate's leaf value is
//! never the value head: the position one ply under the candidate is
//! played forward to a natural terminal `rollouts` independent times, each
//! time with this wrapper's own raw policy sampling the action for BOTH
//! seats (a single-model self-play proxy; the wrapper has no access to the
//! real opponent's policy), and the candidate is scored by the mean of the
//! root seat's unit terminal outcome across those rollouts (win 1, loss 0,
//! draw 0.5). A candidate whose one-ply step is itself terminal skips the
//! rollouts entirely and uses that exact, non-stochastic outcome, matching
//! the value-head leaf's own one-ply-terminal shortcut above. The candidate
//! with the best mean wins; an exact tie (routine here, since a mean of
//! `rollouts` unit outcomes is one of only `rollouts + 1` possible values)
//! is broken by the candidate's own root prior (its sampled logit), never
//! by sampling order.
//!
//! Rollout determinism and RNG safety: every rollout's action stream is its
//! own `SplitMix64`, seeded (`mix_rollout_seed_v1`) from exactly the case
//! seed, the root decision's own `episode_id` and `step`, and the rollout
//! index `0..rollouts`: deliberately never the candidate action, so every
//! candidate at one decision replays the identical `rollouts` sequences of
//! "luck" from its own resulting position onward (common random numbers
//! across candidates, reducing comparison noise, not an oversight). That
//! per-rollout stream is advanced once per multi-option decision
//! encountered along the way; a forced, single-option decision takes it
//! directly, with no draw and no forward pass. Each draw feeds
//! `FrozenPlayPolicyV1`'s own production categorical sampler through a new
//! deterministic-seed entry point (`sample_deterministic_v1`, sibling to
//! the seat-RNG-driven `sample_scores`), so a rollout samples with the
//! exact distribution real play would, without ever touching `seat_rng`:
//! the wrapped policy's real decision stream is therefore unperturbed by
//! rollout mode, for the same reason `Normal` mode does not perturb it (see
//! the RNG safety paragraph above).
//!
//! The rollout leaf never reads the value head. `score_fast_session_v1`
//! still returns one fused `{logits, value}` pair per forward pass (there
//! is no cheaper logits-only path; `NativePolicyValueNetV1` computes both
//! in the same pass), so a rollout's per-step forward pass computes a
//! `value` like any other call, but nothing in the rollout leaf's candidate
//! scoring ever reads it; only `.logits` feeds the sampler. This module's
//! own tests hold the wrapper to that with a real counter
//! (`value_head_reads`), incremented at the one place `Normal` and
//! `SignFlipped` mode read `.value`, and never incremented on the rollout
//! path.
//!
//! Identity: `SearchUsageReceiptV1` (already the sole record that a read
//! used this wrapper at all, since it is absent from every unwrapped read)
//! now also carries `rollouts` alongside `budget`, so a rollout read's own
//! receipts are self-labeled and cannot be confused with either a raw
//! (unwrapped) read or a `budget`-only one-ply read: `rollouts == 0` is
//! exactly every read before this change, byte-identical; `rollouts > 0` is
//! new and only ever produced by the rollout path. The compiled source hash
//! the CLI already embeds in every run's `run-start.json`
//! (`compiled_sources()["decision_time_search"]`, over this file) changes
//! with this edit regardless, so even a config that never sets `rollouts`
//! is still traceable to a build that has the rollout leaf compiled in.
//!
//! Cost and cloning: each rollout clones the one-ply branch session again
//! (`FastActorSessionV1::clone()`, the same real, independent, owned-data
//! clone the one-ply leaf already relies on; see the design-choice note
//! above) and plays it forward with `step`, so it can never mutate the
//! evaluator's own live game, the root session, or any other rollout's or
//! candidate's clone. A budget-`B`, rollouts-`R` decision therefore costs
//! up to `B * R` full-game self-play rollouts on top of the `B` one-ply
//! clones and steps `Normal` mode alone would have cost: substantially more
//! expensive per decision than any prior leaf mode, and measured, not
//! assumed, by the read this change ships with.

use crate::paired_bo1_harness_v1::{
    PairedBo1PolicyInputV1, PairedBo1PolicyV1, PlayPolicyGenerationV1,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1, RlSessionError};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};

/// Config-driven, per-decision search budget for exactly one physical seat.
/// `budget` bounds how many candidate actions a decision with more than one
/// legal action may expand (a node count: one clone, step, and score each).
/// `budget == 0` makes the wrapper fully inert; see the module doc and this
/// module's `budget_zero_reproduces_raw_policy_decision_exactly` test.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EndSeatSearchRequestV1 {
    pub seat: u8,
    pub budget: u32,
    /// Rollout leaf count, sibling to `budget`: `0` (the default, and the
    /// only value every config before this field existed could express)
    /// keeps the committed one-ply leaf (`Normal`/diagnostic modes)
    /// byte-identical; a nonzero count switches that decision's candidate
    /// scoring to the rollout leaf described in this module's doc comment.
    /// `#[serde(default)]` so a config written before this field existed
    /// still decodes unchanged.
    #[serde(default)]
    pub rollouts: u32,
}

/// Per-match counters, recorded into the match receipt and the chunk
/// summary so the analysis can label a read: how many decisions this
/// wrapped seat faced, how many it actually searched (budget > 0 and more
/// than one legal action, with at least one candidate scored), and how
/// many it started to search but fell back on (every candidate's clone or
/// step failed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndSeatSearchStatsV1 {
    pub decisions_seen: u64,
    pub decisions_searched: u64,
    pub decisions_fallback: u64,
}

/// Match-receipt and chunk-summary shape for requirement 2: which seat
/// carried the search wrapper, its configured budget, and the resulting
/// counters. A new, additive, `skip_serializing_if`-guarded field wherever
/// it is embedded; it is never present unless a config explicitly requests
/// `end_seat_search`, so every existing output keeps its exact prior shape.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct SearchUsageReceiptV1 {
    pub seat: u8,
    pub budget: u32,
    /// Copied from `EndSeatSearchRequestV1::rollouts` (requirement: both
    /// the candidate count and the rollout count must appear in the
    /// receipt). `0` means the one-ply leaf ran, exactly as every receipt
    /// before this field existed; `> 0` means the rollout leaf ran, so a
    /// rollout read's own receipts are self-labeled and cannot be confused
    /// with a one-ply read or a raw (unwrapped) checkpoint's read (which
    /// carries no `search_usage` at all).
    pub rollouts: u32,
    pub decisions_seen: u64,
    pub decisions_searched: u64,
    pub decisions_fallback: u64,
}

impl SearchUsageReceiptV1 {
    pub fn new_v1(request: EndSeatSearchRequestV1, stats: EndSeatSearchStatsV1) -> Self {
        Self {
            seat: request.seat,
            budget: request.budget,
            rollouts: request.rollouts,
            decisions_seen: stats.decisions_seen,
            decisions_searched: stats.decisions_searched,
            decisions_fallback: stats.decisions_fallback,
        }
    }

    /// Field-wise sum, for the chunk summary's aggregate across every match
    /// in that chunk-orientation. `seat`/`budget`/`rollouts` come from
    /// `self` (every match in one command shares the same config);
    /// `other`'s must agree, which is guaranteed by construction (one
    /// `end_seat_search` per command) rather than checked here.
    pub fn accumulate_v1(&mut self, other: &Self) {
        self.decisions_seen += other.decisions_seen;
        self.decisions_searched += other.decisions_searched;
        self.decisions_fallback += other.decisions_fallback;
    }
}

/// Wraps exactly one seat's `FrozenPlayPolicyV1` with the bounded search
/// described in this module's doc comment. Implements `PairedBo1PolicyV1`
/// so it plugs directly into `SeatRoutedBo3PlayPolicyV1` in place of a raw
/// policy reference. The other seat is left as a sibling wrapper with
/// `budget: 0`, which this module's own tests prove is behaviorally
/// identical to no wrapper at all, so "leaving the start/opponent seat's
/// scoring unchanged" holds by construction rather than by a separate code
/// path.
pub struct SearchWrappedPlayPolicyV1<'a> {
    inner: &'a mut FrozenPlayPolicyV1,
    budget: u32,
    /// Rollout count; `0` is the committed one-ply leaf (`Normal` by
    /// default, or a diagnostic mode via `PHASE1_V4_SEARCH_DIAGNOSTIC_MODE`).
    /// `> 0` switches every searched decision to the rollout leaf,
    /// unconditionally ahead of `diagnostic_mode`: the two are orthogonal
    /// scaffolds (rollout is config-driven and receipted; the diagnostic
    /// modes are env-var-only and were never wired into any config or
    /// receipt), and this module never needs to compose them.
    rollouts: u32,
    case_seed: u64,
    stats: EndSeatSearchStatsV1,
    diagnostic_mode: DiagnosticModeV1,
    /// Test-only witness (always maintained; cheap). Counts exactly the
    /// calls that read `.value` off a leaf score, so a test can assert the
    /// rollout leaf never does, without guessing from behavior alone. See
    /// this module's doc comment ("The rollout leaf never reads the value
    /// head").
    value_head_reads: u64,
}

/// Investigation-only leaf-evaluation variants, selected once via the
/// `PHASE1_V4_SEARCH_DIAGNOSTIC_MODE` environment variable, never the
/// default: `Normal` (unset, or any other value) is byte-identical to the
/// committed behavior above. `SignFlipped` negates the leaf value used in
/// `Normal` mode, to test whether the leaf's perspective is inverted.
/// `PriorOnly` skips the session clone/step entirely and uses the root
/// policy's own logit for each sampled candidate as its score instead (so
/// selection reduces to an argmax over the sampled candidates, isolating
/// "argmax over samples" from "value head as leaf"). Not wired into any
/// config field; a diagnostic scaffold read only from the process
/// environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticModeV1 {
    Normal,
    SignFlipped,
    PriorOnly,
}

impl DiagnosticModeV1 {
    fn from_env_v1() -> Self {
        match std::env::var("PHASE1_V4_SEARCH_DIAGNOSTIC_MODE").ok().as_deref() {
            Some("sign_flipped") => Self::SignFlipped,
            Some("prior_only") => Self::PriorOnly,
            _ => Self::Normal,
        }
    }
}

impl<'a> SearchWrappedPlayPolicyV1<'a> {
    pub fn new_v1(
        inner: &'a mut FrozenPlayPolicyV1,
        budget: u32,
        rollouts: u32,
        case_seed: u64,
    ) -> Self {
        Self {
            inner,
            budget,
            rollouts,
            case_seed,
            stats: EndSeatSearchStatsV1::default(),
            diagnostic_mode: DiagnosticModeV1::from_env_v1(),
            value_head_reads: 0,
        }
    }

    pub fn stats_v1(&self) -> EndSeatSearchStatsV1 {
        self.stats
    }
}

fn mix_seed_v1(case_seed: u64, episode_id: u64, step: u64) -> u64 {
    let mixed = case_seed
        .wrapping_add(episode_id.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(step.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(0xD6E8_FEB8_6659_FD93);
    SplitMix64::seed(mixed).next_u64()
}

/// Standard 53-bit-mantissa construction of a uniform `[0, 1)` double from a
/// `u64` draw. No wall clock; purely a function of the RNG state.
fn uniform_unit_v1(rng: &mut SplitMix64) -> f64 {
    ((rng.next_u64() >> 11) as f64) * (1.0 / (1u64 << 53) as f64)
}

/// Weighted sample-without-replacement of up to `budget` distinct action
/// indices from `0..logits.len()`, weighted by `softmax(logits)`. Purely a
/// function of `logits` and `seed`.
fn sample_candidates_v1(logits: &[f32], budget: u32, seed: u64) -> Vec<u32> {
    let budget = (budget as usize).min(logits.len());
    if budget == 0 {
        return Vec::new();
    }
    let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut weights: Vec<f64> = logits
        .iter()
        .map(|&x| f64::from(x - max_logit).exp())
        .collect();
    let mut rng = SplitMix64::seed(seed);
    let mut chosen = Vec::with_capacity(budget);
    for _ in 0..budget {
        let total: f64 = weights.iter().sum();
        if !(total > 0.0) {
            break;
        }
        let mut r = uniform_unit_v1(&mut rng) * total;
        let mut pick = weights.len() - 1;
        for (index, &w) in weights.iter().enumerate() {
            if r < w {
                pick = index;
                break;
            }
            r -= w;
        }
        chosen.push(pick as u32);
        weights[pick] = 0.0;
    }
    chosen
}

fn seat_index_v1(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

/// The root seat's unit terminal outcome: win 1, loss 0, draw (or any other
/// winnerless terminal, such as a truncation) 0.5.
fn unit_outcome_v1(winner: Option<PlayerSeatV1>, root_seat: PlayerSeatV1) -> f32 {
    match winner {
        Some(seat) if seat == root_seat => 1.0,
        Some(_) => 0.0,
        None => 0.5,
    }
}

/// Seeds one full rollout trajectory. Distinct from `mix_seed_v1` (root
/// candidate sampling) by construction: different mixing constants, and an
/// extra `rollout_index` term. Deliberately independent of the candidate
/// action; see this module's doc comment ("common random numbers across
/// candidates").
fn mix_rollout_seed_v1(case_seed: u64, episode_id: u64, step: u64, rollout_index: u32) -> u64 {
    let mixed = case_seed
        .wrapping_add(episode_id.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add(step.wrapping_mul(0x1656_67B1_9E37_79F9))
        .wrapping_add(u64::from(rollout_index).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0xFF51_AFD7_ED55_8CCD);
    SplitMix64::seed(mixed).next_u64()
}

/// Plays one rollout to a natural terminal from `session` (already at a
/// live decision one ply under a candidate), sampling every decision along
/// the way with `policy`'s own logits for BOTH seats: this wrapper has only
/// one model, so it stands in for the opponent too (a self-play proxy).
/// `rng` is a fresh, caller-seeded stream (see `mix_rollout_seed_v1`),
/// advanced once per multi-option decision via `sample_deterministic_v1`
/// (the policy's real production sampler, given an explicit seed instead of
/// its live `seat_rng`); a decision with only one legal action takes it
/// directly, with no draw and no forward pass. Never reads `.value`: only
/// `.logits` feeds the sampler. Returns the root seat's unit terminal
/// outcome, or an error if a step or a forward pass fails partway through
/// (the caller drops the whole candidate on any such error, so a partial
/// rollout never contributes to a mean).
fn simulate_one_rollout_v1(
    policy: &mut FrozenPlayPolicyV1,
    mut session: FastActorSessionV1,
    root_seat: PlayerSeatV1,
    mut rng: SplitMix64,
) -> Result<f32, String> {
    let mut response = session.current_response();
    loop {
        let decision = match response {
            FastActorResponseV1::Terminal(terminal) => {
                return Ok(unit_outcome_v1(terminal.winner, root_seat));
            }
            FastActorResponseV1::Decision(decision) => decision,
        };
        let action = if decision.legal_action_count <= 1 {
            0
        } else {
            let scores = policy.score_fast_session_v1(&session)?;
            let seed = rng.next_u64();
            policy.sample_deterministic_v1(&scores.logits, decision.legal_action_count, seed)?
        };
        response = session
            .step(decision.episode_id, decision.step, action)
            .map_err(|error| error.to_string())?;
    }
}

impl PairedBo1PolicyV1 for SearchWrappedPlayPolicyV1<'_> {
    fn uses_observation_successor_v3(&self) -> bool {
        self.inner.uses_observation_successor_v3()
    }

    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        self.inner.feature_generation_v1()
    }

    fn reset_for_game_v1(&mut self, policy_seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.inner.reset_for_game_v1(policy_seeds)
    }

    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        // Exactly the call the unwrapped policy makes for this decision: same
        // encoding, same sampled action, same RNG consumption, whether or not
        // search runs below. This is what makes budget 0 byte-identical to no
        // wrapper at all.
        let decision = input.decision();
        let (raw_action, scores) = self.inner.select_paired_with_scores_v1(&input)?;
        self.stats.decisions_seen += 1;
        if self.budget == 0 || decision.legal_action_count <= 1 {
            return Ok(raw_action);
        }

        let root_session = input.live_session_for_bounded_search_v1();
        let root_seat = decision.acting_player;
        let seed = mix_seed_v1(self.case_seed, decision.episode_id, decision.step);
        let candidates = sample_candidates_v1(&scores.logits, self.budget, seed);
        if candidates.is_empty() {
            return Ok(raw_action);
        }

        let best_action: Option<u32> = if self.rollouts > 0 {
            // Rollout leaf: see this module's doc comment. `best` tracks
            // (action, mean unit outcome, root prior) so an exact tie in
            // the mean (routine at a small rollout count) is broken by the
            // prior, never by sampling order.
            let mut best: Option<(u32, f32, f32)> = None;
            for action in candidates {
                let prior = scores.logits[action as usize];
                let mut branch = root_session.clone();
                let Ok(response) = branch.step(decision.episode_id, decision.step, action) else {
                    continue;
                };
                let mean_outcome = match response {
                    FastActorResponseV1::Terminal(terminal) => {
                        unit_outcome_v1(terminal.winner, root_seat)
                    }
                    FastActorResponseV1::Decision(_) => {
                        let mut total = 0.0f64;
                        let mut all_ok = true;
                        for rollout_index in 0..self.rollouts {
                            let rollout_seed = mix_rollout_seed_v1(
                                self.case_seed,
                                decision.episode_id,
                                decision.step,
                                rollout_index,
                            );
                            match simulate_one_rollout_v1(
                                self.inner,
                                branch.clone(),
                                root_seat,
                                SplitMix64::seed(rollout_seed),
                            ) {
                                Ok(outcome) => total += f64::from(outcome),
                                Err(_) => {
                                    all_ok = false;
                                    break;
                                }
                            }
                        }
                        // Any failed rollout drops the whole candidate,
                        // rather than averaging over fewer than `rollouts`
                        // samples: every surviving candidate's mean is then
                        // directly comparable, each over the same count.
                        if !all_ok {
                            continue;
                        }
                        (total / f64::from(self.rollouts)) as f32
                    }
                };
                let better = match best {
                    None => true,
                    Some((_, best_mean, best_prior)) => {
                        mean_outcome > best_mean
                            || (mean_outcome == best_mean && prior > best_prior)
                    }
                };
                if better {
                    best = Some((action, mean_outcome, prior));
                }
            }
            best.map(|(action, _, _)| action)
        } else {
            let mut best: Option<(u32, f32)> = None;
            for action in candidates {
                // PriorOnly (diagnostic-only, never the default): no clone
                // or step at all, just the root policy's own logit for this
                // candidate, isolating "argmax over the sampled candidates"
                // from "value head as leaf".
                if self.diagnostic_mode == DiagnosticModeV1::PriorOnly {
                    let value = scores.logits[action as usize];
                    if best.is_none_or(|(_, best_value)| value > best_value) {
                        best = Some((action, value));
                    }
                    continue;
                }
                let mut branch = root_session.clone();
                let Ok(response) = branch.step(decision.episode_id, decision.step, action) else {
                    continue;
                };
                let mut value = match response {
                    FastActorResponseV1::Terminal(terminal) => {
                        terminal.terminal_reward[seat_index_v1(root_seat)] as f32
                    }
                    FastActorResponseV1::Decision(next_decision) => {
                        let Ok(leaf_scores) = self.inner.score_fast_session_v1(&branch) else {
                            continue;
                        };
                        self.value_head_reads += 1;
                        if next_decision.acting_player == root_seat {
                            leaf_scores.value
                        } else {
                            -leaf_scores.value
                        }
                    }
                };
                // SignFlipped (diagnostic-only, never the default): negate
                // the exact same leaf value Normal mode would have used, to
                // test whether the leaf's perspective is inverted.
                if self.diagnostic_mode == DiagnosticModeV1::SignFlipped {
                    value = -value;
                }
                if best.is_none_or(|(_, best_value)| value > best_value) {
                    best = Some((action, value));
                }
            }
            best.map(|(action, _)| action)
        };

        match best_action {
            Some(action) => {
                self.stats.decisions_searched += 1;
                Ok(action)
            }
            None => {
                self.stats.decisions_fallback += 1;
                Ok(raw_action)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::rl_session::{
        avenging_hunter_undercity_arena_choose_targets_state_v1,
        shuffle_trigger_source_into_library_v1, FastActorSessionV1,
    };

    /// Same fixture `sideboard_play_policy_v1.rs`'s own V3/V4 scoring tests
    /// use (`score_fast_session_v1_takes_the_v4_arm_and_v3_output_is_unchanged`):
    /// a live decision with more than one legal action, reachable without any
    /// checkpoint file.
    fn fixture_session_v1() -> FastActorSessionV1 {
        let (mut state, hunter, _goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);
        FastActorSessionV1::from_v3_fixture_state(state)
    }

    fn fixture_decision_v1() -> crate::rl_session::FastActorDecisionV1 {
        match fixture_session_v1().current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            FastActorResponseV1::Terminal(_) => panic!("fixture must start at a live decision"),
        }
    }

    /// Sibling of `fixture_session_v1` with both libraries stocked (twenty
    /// basic lands each, the same technique `lethal_attack_fixture_v1`
    /// below uses), so a one-ply step from this decision can reach a live
    /// `Decision` instead of racing to a natural terminal by decking.
    /// `fixture_session_v1` alone does that deterministically for either
    /// legal target here: both candidates reach the very same forced
    /// multi-turn advance into a zero-card draw, so a test that needs a
    /// `Decision` leaf (not a `Terminal` one) needs this instead.
    fn fixture_session_with_stocked_libraries_v1() -> FastActorSessionV1 {
        use crate::policy_observation_v6::tests::put;
        use crate::state::Zone;

        let (mut state, hunter, _goaded, _ordinary) =
            avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        shuffle_trigger_source_into_library_v1(&mut state, hunter, PlayerId::P0);
        for _ in 0..20 {
            put(&mut state, PlayerId::P0, "Forest", Zone::Library);
            put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        }
        FastActorSessionV1::from_v3_fixture_state(state)
    }

    fn fixture_decision_with_stocked_libraries_v1() -> crate::rl_session::FastActorDecisionV1 {
        match fixture_session_with_stocked_libraries_v1().current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            FastActorResponseV1::Terminal(_) => panic!("fixture must start at a live decision"),
        }
    }

    #[test]
    fn budget_zero_reproduces_raw_policy_decision_exactly() {
        let decision = fixture_decision_v1();
        assert!(
            decision.legal_action_count > 1,
            "fixture must offer more than one legal action"
        );

        let session_wrapped = fixture_session_v1();
        let mut wrapped_policy = FrozenPlayPolicyV1::training_fixture_v4();
        wrapped_policy.reset_sampling_v1([111, 222]);
        let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut wrapped_policy, 0, 0, 777);
        let wrapped_action = wrapped
            .select_action_v1(PairedBo1PolicyInputV1::new(&session_wrapped, decision))
            .unwrap();
        assert_eq!(
            wrapped.stats_v1(),
            EndSeatSearchStatsV1 {
                decisions_seen: 1,
                decisions_searched: 0,
                decisions_fallback: 0,
            }
        );

        let session_raw = fixture_session_v1();
        let mut raw_policy = FrozenPlayPolicyV1::training_fixture_v4();
        raw_policy.reset_sampling_v1([111, 222]);
        let raw_action = raw_policy
            .select_action_v1(PairedBo1PolicyInputV1::new(&session_raw, decision))
            .unwrap();

        assert_eq!(wrapped_action, raw_action);
    }

    #[test]
    fn deterministic_for_a_fixed_seed_and_actually_searches() {
        let decision = fixture_decision_v1();
        assert!(decision.legal_action_count > 1);

        let run = || {
            let session = fixture_session_v1();
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            policy.reset_sampling_v1([333, 444]);
            let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut policy, 4, 0, 999_888_777);
            let action = wrapped
                .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
                .unwrap();
            (action, wrapped.stats_v1())
        };

        let (action_first, stats_first) = run();
        let (action_second, stats_second) = run();
        assert_eq!(action_first, action_second);
        assert_eq!(stats_first, stats_second);
        assert_eq!(
            stats_first.decisions_searched, 1,
            "a decision with more than one legal action and budget > 0 must actually search"
        );
        assert_eq!(stats_first.decisions_fallback, 0);
    }

    /// The invariant the module doc claims: simulated leaf scoring never
    /// touches `seat_rng`, so a searched decision leaves the policy's own
    /// sampling stream exactly where a raw (unsearched) decision would.
    /// Proven by driving a second, independent decision on both a searched
    /// and an unsearched policy afterward and requiring the same result.
    #[test]
    fn searching_a_decision_does_not_perturb_the_policy_sampling_stream() {
        let decision = fixture_decision_v1();
        let next_session = fixture_session_v1();

        let session_searched = fixture_session_v1();
        let mut searched_policy = FrozenPlayPolicyV1::training_fixture_v4();
        searched_policy.reset_sampling_v1([9, 10]);
        {
            let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut searched_policy, 4, 0, 555);
            wrapped
                .select_action_v1(PairedBo1PolicyInputV1::new(&session_searched, decision))
                .unwrap();
        }
        let after_search = searched_policy.select_fast_session_v1(&next_session).unwrap();

        let session_raw = fixture_session_v1();
        let mut raw_policy = FrozenPlayPolicyV1::training_fixture_v4();
        raw_policy.reset_sampling_v1([9, 10]);
        raw_policy
            .select_action_v1(PairedBo1PolicyInputV1::new(&session_raw, decision))
            .unwrap();
        let after_raw = raw_policy.select_fast_session_v1(&next_session).unwrap();

        assert_eq!(after_search, after_raw);
    }

    /// Fixture shared by every lethal-attack test below (the committed
    /// one-ply leaf's own regression test and the rollout leaf's): a
    /// position where attacking with an untapped, unblocked creature for
    /// its exact power is immediately lethal, alongside a legal "do not
    /// attack" alternative that is not. Returns the live session, its
    /// current decision, and the two action indices (attack, no-attack).
    fn lethal_attack_fixture_v1() -> (
        FastActorSessionV1,
        crate::rl_session::FastActorDecisionV1,
        u32,
        u32,
    ) {
        use crate::card_def::{card_id_by_name, CARD_DEFS};
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::rl::ActionSemanticV1;
        use crate::state::{Step, Zone};

        let attacker_card = "Voldaren Epicure";
        let mut state = ready_state();
        put(&mut state, PlayerId::P0, attacker_card, Zone::Battlefield);
        let card_def_id = card_id_by_name(attacker_card)
            .unwrap_or_else(|| panic!("fixture card not found: {attacker_card}"));
        let power = CARD_DEFS[card_def_id as usize]
            .power
            .unwrap_or_else(|| panic!("{attacker_card} has no power"));
        state.players[PlayerId::P1.index()].life = i32::from(power);
        // `ready_state()` gives both players an empty library. Declining to
        // attack has no real decision left this turn, so the engine's
        // "advance until a real decision or terminal" loop races all the
        // way into P1's own draw step within this same one-step call; an
        // empty library there makes P1 lose to decking regardless of the
        // attack choice, which would make this fixture prove nothing.
        // Twenty basic lands on each side rules that out for the one-ply
        // leaf's one-turn horizon. (The rollout leaf's own lethal-attack
        // test below needs a different library shape than this one, for a
        // reason specific to playing all the way to a natural terminal; see
        // `lethal_attack_fixture_for_rollout_v1`.)
        for _ in 0..20 {
            put(&mut state, PlayerId::P0, "Forest", Zone::Library);
            put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        }
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.engine.combat.attackers_declared = false;

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let decision = match session.current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            FastActorResponseV1::Terminal(_) => panic!("fixture must start at a live decision"),
        };
        assert!(decision.legal_action_count >= 2);
        let semantics = session.diagnostic_current_action_semantics().unwrap();
        let attack_index = semantics
            .iter()
            .position(|semantic| {
                matches!(
                    semantic,
                    ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                )
            })
            .expect("no attack candidate present") as u32;
        let no_attack_index = semantics
            .iter()
            .position(|semantic| {
                matches!(
                    semantic,
                    ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
                )
            })
            .expect("no decline-to-attack candidate present") as u32;
        assert_ne!(attack_index, no_attack_index);

        (session, decision, attack_index, no_attack_index)
    }

    /// Rollout-leaf sibling of `lethal_attack_fixture_v1`: the same lethal
    /// attack setup, but the acting seat's (P0's) own library is left
    /// empty, exactly as `ready_state()` already gives it, while only the
    /// opponent's is stocked, just enough to survive its own next draw.
    ///
    /// A one-ply evaluator never looks far enough ahead for this to
    /// matter, which is why `lethal_attack_fixture_v1` stocks both sides
    /// symmetrically. But the rollout leaf plays every candidate all the
    /// way to a natural terminal, self-playing both seats with the same
    /// raw policy: with both libraries stocked, "decline to attack this
    /// turn, attack next turn instead" is just as winning as attacking
    /// now, since P1 has no way to block or interact either way, so both
    /// candidates' rollouts converge on the same 1.0 mean and the
    /// comparison falls to the prior, which for this model and these
    /// seeds does not happen to favor attacking. Leaving P0's own library
    /// empty makes delay costly instead: declining costs a full extra turn
    /// cycle (the rest of this turn, then all of P1's turn), and P0 decks
    /// out on its own very next draw step before it can attack again, so
    /// "decline" reliably loses a full rollout while "attack" (won this
    /// turn, before any further draw) still reliably wins it.
    fn lethal_attack_fixture_for_rollout_v1() -> (
        FastActorSessionV1,
        crate::rl_session::FastActorDecisionV1,
        u32,
        u32,
    ) {
        use crate::card_def::{card_id_by_name, CARD_DEFS};
        use crate::policy_observation_v6::tests::{put, ready_state};
        use crate::rl::ActionSemanticV1;
        use crate::state::{Step, Zone};

        let attacker_card = "Voldaren Epicure";
        let mut state = ready_state();
        put(&mut state, PlayerId::P0, attacker_card, Zone::Battlefield);
        let card_def_id = card_id_by_name(attacker_card)
            .unwrap_or_else(|| panic!("fixture card not found: {attacker_card}"));
        let power = CARD_DEFS[card_def_id as usize]
            .power
            .unwrap_or_else(|| panic!("{attacker_card} has no power"));
        state.players[PlayerId::P1.index()].life = i32::from(power);
        for _ in 0..5 {
            put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        }
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.engine.combat.attackers_declared = false;

        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let decision = match session.current_response() {
            FastActorResponseV1::Decision(decision) => decision,
            FastActorResponseV1::Terminal(_) => panic!("fixture must start at a live decision"),
        };
        assert!(decision.legal_action_count >= 2);
        let semantics = session.diagnostic_current_action_semantics().unwrap();
        let attack_index = semantics
            .iter()
            .position(|semantic| {
                matches!(
                    semantic,
                    ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                )
            })
            .expect("no attack candidate present") as u32;
        let no_attack_index = semantics
            .iter()
            .position(|semantic| {
                matches!(
                    semantic,
                    ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
                )
            })
            .expect("no decline-to-attack candidate present") as u32;
        assert_ne!(attack_index, no_attack_index);

        (session, decision, attack_index, no_attack_index)
    }

    /// Diagnostic requested after the r block-24 development read came back
    /// far worse than the raw policy: a real position where attacking with
    /// an untapped, unblocked creature for its exact power is immediately
    /// lethal, alongside a legal "do not attack" alternative that is not.
    /// If the wrapper picks the losing action here, the leaf sign or the
    /// seat perspective is inverted.
    #[test]
    fn wrapper_selects_a_lethal_attack_over_not_attacking() {
        let (session, decision, attack_index, no_attack_index) = lethal_attack_fixture_v1();

        // Ground truth: attacking (through any forced, single-option steps
        // combat resolution needs) reaches Terminal with P0 as the winner.
        // Diagnostic instrumentation: record the very first step's response
        // kind, since the wrapper only looks one ply ahead.
        let mut ground_truth = session.clone();
        let first_response = ground_truth
            .step(decision.episode_id, decision.step, attack_index)
            .unwrap();
        let first_step_is_terminal = matches!(first_response, FastActorResponseV1::Terminal(_));
        let mut response = first_response;
        let mut forced_step_count = 0u32;
        loop {
            match response {
                FastActorResponseV1::Terminal(terminal) => {
                    assert_eq!(
                        terminal.winner,
                        Some(PlayerSeatV1::P0),
                        "attacking for exact lethal must win the game"
                    );
                    break;
                }
                FastActorResponseV1::Decision(forced) => {
                    assert_eq!(
                        forced.legal_action_count, 1,
                        "only a forced single-option step is expected en route to lethal"
                    );
                    forced_step_count += 1;
                    response = ground_truth
                        .step(forced.episode_id, forced.step, 0)
                        .unwrap();
                }
            }
        }

        // One-ply diagnostic: exactly what the wrapper itself computes for
        // each candidate, so a failure below is legible without a debugger.
        let mut diag_policy = FrozenPlayPolicyV1::training_fixture_v4();
        diag_policy.reset_sampling_v1([1, 2]);
        let attack_branch_report = {
            let mut branch = session.clone();
            match branch
                .step(decision.episode_id, decision.step, attack_index)
                .unwrap()
            {
                FastActorResponseV1::Terminal(terminal) => format!(
                    "terminal reward[root_seat]={}",
                    terminal.terminal_reward[seat_index_v1(decision.acting_player)]
                ),
                FastActorResponseV1::Decision(next) => {
                    let scores = diag_policy.score_fast_session_v1(&branch).unwrap();
                    format!(
                        "decision next_actor={:?} root_seat={:?} raw_value={} oriented_value={}",
                        next.acting_player,
                        decision.acting_player,
                        scores.value,
                        if next.acting_player == decision.acting_player {
                            scores.value
                        } else {
                            -scores.value
                        }
                    )
                }
            }
        };
        let no_attack_branch_report = {
            let mut branch = session.clone();
            match branch
                .step(decision.episode_id, decision.step, no_attack_index)
                .unwrap()
            {
                FastActorResponseV1::Terminal(terminal) => format!(
                    "terminal reward[root_seat]={}",
                    terminal.terminal_reward[seat_index_v1(decision.acting_player)]
                ),
                FastActorResponseV1::Decision(next) => {
                    let scores = diag_policy.score_fast_session_v1(&branch).unwrap();
                    format!(
                        "decision next_actor={:?} root_seat={:?} raw_value={} oriented_value={}",
                        next.acting_player,
                        decision.acting_player,
                        scores.value,
                        if next.acting_player == decision.acting_player {
                            scores.value
                        } else {
                            -scores.value
                        }
                    )
                }
            }
        };

        // The actual check: with a budget covering both candidates, the
        // wrapper must select the winning action, not the losing one.
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        policy.reset_sampling_v1([1, 2]);
        let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut policy, 2, 0, 424_242);
        let selected = wrapped
            .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
            .unwrap();
        assert_eq!(
            selected, attack_index,
            "wrapper picked the losing action; leaf sign or seat perspective is inverted.\n\
             root_seat={:?} attack_index={attack_index} no_attack_index={no_attack_index}\n\
             first_step_is_terminal={first_step_is_terminal} forced_step_count_to_terminal={forced_step_count}\n\
             attack branch: {attack_branch_report}\n\
             no_attack branch: {no_attack_branch_report}",
            decision.acting_player,
        );
    }

    /// Rollout-leaf sibling of `wrapper_selects_a_lethal_attack_over_not_attacking`
    /// (task requirement: extend the lethal-position test to the rollout
    /// leaf). The rollout leaf never looks at a value head at all, so this
    /// exercises a genuinely different code path reaching the same correct
    /// answer: the lethal-attack candidate's one-ply step is itself
    /// terminal (a win, unit outcome 1.0), and no mean outcome can exceed
    /// 1.0, so attacking must win the comparison (or, in the degenerate
    /// case every no-attack rollout errors out, be the only surviving
    /// candidate).
    #[test]
    fn rollout_leaf_selects_a_lethal_attack_over_not_attacking() {
        let (session, decision, attack_index, no_attack_index) =
            lethal_attack_fixture_for_rollout_v1();

        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        policy.reset_sampling_v1([1, 2]);
        let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut policy, 2, 3, 424_242);
        let selected = wrapped
            .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
            .unwrap();
        assert_eq!(
            selected, attack_index,
            "rollout leaf picked the losing action; root_seat={:?} attack_index={attack_index} \
             no_attack_index={no_attack_index}",
            decision.acting_player,
        );
        assert_eq!(
            wrapped.stats_v1().decisions_searched,
            1,
            "a decision with more than one legal action and budget > 0 must actually search"
        );
    }

    /// Task requirement: a determinism test for the rollout leaf, two runs,
    /// identical choices AND identical receipts (the actual
    /// `SearchUsageReceiptV1` a real read would write out, not just the raw
    /// stats).
    #[test]
    fn rollout_leaf_is_deterministic_for_a_fixed_seed() {
        let decision = fixture_decision_v1();
        assert!(decision.legal_action_count > 1);
        let request = EndSeatSearchRequestV1 {
            seat: 0,
            budget: 4,
            rollouts: 2,
        };

        let run = || {
            let session = fixture_session_v1();
            let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
            policy.reset_sampling_v1([333, 444]);
            let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(
                &mut policy,
                request.budget,
                request.rollouts,
                999_888_777,
            );
            let action = wrapped
                .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
                .unwrap();
            let receipt = SearchUsageReceiptV1::new_v1(request, wrapped.stats_v1());
            (action, receipt)
        };

        let (action_first, receipt_first) = run();
        let (action_second, receipt_second) = run();
        assert_eq!(action_first, action_second);
        assert_eq!(receipt_first, receipt_second);
        assert_eq!(
            receipt_first.decisions_searched, 1,
            "a decision with more than one legal action and budget > 0 must actually search"
        );
        assert_eq!(receipt_first.decisions_fallback, 0);
        assert_eq!(receipt_first.rollouts, 2);
    }

    /// Task requirement: the rollout leaf never calls the value head.
    /// Proven with a real counter (`value_head_reads`; see this module's
    /// doc comment), not inferred from behavior alone: zero for the
    /// rollout leaf on a fixture where the committed one-ply leaf
    /// (`rollouts: 0`) reads it at least once on the exact same decision
    /// and seeds, so the counter is a meaningful witness, not a vacuous
    /// zero.
    #[test]
    fn rollout_leaf_never_reads_the_value_head() {
        let decision = fixture_decision_with_stocked_libraries_v1();
        assert!(decision.legal_action_count > 1);

        let session_rollout = fixture_session_with_stocked_libraries_v1();
        let mut rollout_policy = FrozenPlayPolicyV1::training_fixture_v4();
        rollout_policy.reset_sampling_v1([5, 6]);
        let mut rollout_wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut rollout_policy, 4, 2, 13);
        rollout_wrapped
            .select_action_v1(PairedBo1PolicyInputV1::new(&session_rollout, decision))
            .unwrap();
        assert_eq!(rollout_wrapped.value_head_reads, 0);

        let session_normal = fixture_session_with_stocked_libraries_v1();
        let mut normal_policy = FrozenPlayPolicyV1::training_fixture_v4();
        normal_policy.reset_sampling_v1([5, 6]);
        let mut normal_wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut normal_policy, 4, 0, 13);
        normal_wrapped
            .select_action_v1(PairedBo1PolicyInputV1::new(&session_normal, decision))
            .unwrap();
        assert!(
            normal_wrapped.value_head_reads > 0,
            "fixture should exercise the one-ply leaf's value head at least once"
        );
    }
}
