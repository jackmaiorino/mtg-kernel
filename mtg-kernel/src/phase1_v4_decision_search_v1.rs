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

use crate::paired_bo1_harness_v1::{
    PairedBo1PolicyInputV1, PairedBo1PolicyV1, PlayPolicyGenerationV1,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, RlSessionError};
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
    pub decisions_seen: u64,
    pub decisions_searched: u64,
    pub decisions_fallback: u64,
}

impl SearchUsageReceiptV1 {
    pub fn new_v1(request: EndSeatSearchRequestV1, stats: EndSeatSearchStatsV1) -> Self {
        Self {
            seat: request.seat,
            budget: request.budget,
            decisions_seen: stats.decisions_seen,
            decisions_searched: stats.decisions_searched,
            decisions_fallback: stats.decisions_fallback,
        }
    }

    /// Field-wise sum, for the chunk summary's aggregate across every match
    /// in that chunk-orientation. `seat`/`budget` come from `self` (every
    /// match in one command shares the same config); `other`'s must agree,
    /// which is guaranteed by construction (one `end_seat_search` per
    /// command) rather than checked here.
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
    case_seed: u64,
    stats: EndSeatSearchStatsV1,
    diagnostic_mode: DiagnosticModeV1,
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
    pub fn new_v1(inner: &'a mut FrozenPlayPolicyV1, budget: u32, case_seed: u64) -> Self {
        Self {
            inner,
            budget,
            case_seed,
            stats: EndSeatSearchStatsV1::default(),
            diagnostic_mode: DiagnosticModeV1::from_env_v1(),
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

        let mut best: Option<(u32, f32)> = None;
        for action in candidates {
            // PriorOnly (diagnostic-only, never the default): no clone or
            // step at all, just the root policy's own logit for this
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
                    if next_decision.acting_player == root_seat {
                        leaf_scores.value
                    } else {
                        -leaf_scores.value
                    }
                }
            };
            // SignFlipped (diagnostic-only, never the default): negate the
            // exact same leaf value Normal mode would have used, to test
            // whether the leaf's perspective is inverted.
            if self.diagnostic_mode == DiagnosticModeV1::SignFlipped {
                value = -value;
            }
            if best.is_none_or(|(_, best_value)| value > best_value) {
                best = Some((action, value));
            }
        }

        match best {
            Some((action, _)) => {
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
        let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut wrapped_policy, 0, 777);
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
            let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut policy, 4, 999_888_777);
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
            let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut searched_policy, 4, 555);
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

    /// Diagnostic requested after the r block-24 development read came back
    /// far worse than the raw policy: a real position where attacking with
    /// an untapped, unblocked creature for its exact power is immediately
    /// lethal, alongside a legal "do not attack" alternative that is not.
    /// If the wrapper picks the losing action here, the leaf sign or the
    /// seat perspective is inverted.
    #[test]
    fn wrapper_selects_a_lethal_attack_over_not_attacking() {
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
        // Twenty basic lands on each side rules that out for this test's
        // one-turn horizon.
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
        let mut wrapped = SearchWrappedPlayPolicyV1::new_v1(&mut policy, 2, 424_242);
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
}
