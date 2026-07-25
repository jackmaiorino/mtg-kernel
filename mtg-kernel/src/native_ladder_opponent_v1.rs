//! Ladder opponent engine: the runtime bridge from a validated
//! `OpponentLadderPoolContractV1` and three loaded frozen-checkpoint
//! inference handles to per-decision opponent action selection (Self-Play
//! Ladder Design Contract S2, Sections 2, 3, 5).
//!
//! This module owns exactly two things layered on top of authorities
//! validated elsewhere:
//!
//! - Per-episode pool-member selection via the `train-opponent-pool-choice`
//!   seed and the frozen 40/20/20/20 thresholds (Section 3).
//! - Per-decision action selection: for the three policy-driven pool
//!   members (PRIMARY, PREDECESSOR-A, PREDECESSOR-B), score the decision
//!   with `NativeCheckpointInferenceV1::score_decision_v1` and sample from
//!   the softmax(temperature=1.0) distribution with exactly one seed (the
//!   `train-opponent-policy-substep` seed); for the UNIFORM FLOOR member,
//!   delegate unchanged to `select_native_trainer_opponent_action_v1`, the
//!   existing frozen uniform sampler.
//!
//! Loading checkpoint bytes into validated, digest-revalidated
//! `NativeCheckpointInferenceV1` handles is explicitly the caller's
//! responsibility (the runner integration layer, not this module): this
//! engine's constructor takes already-validated handles plus the pool
//! contract and performs no I/O.
//!
//! ## The softmax sampling surface is NEW
//!
//! The softmax computation here is a fresh, independent numeric surface
//! with its own goldens (see the `tests` module): f32 scores from the net
//! are widened to f64, shifted by the maximum score for stability, and
//! exponentiated in f64. This deliberately does NOT reuse
//! [`crate::fast_sampler::FastCategoricalScratch`], which is a distinct,
//! frozen fixed-point (Q8 quantized gap, Q63 exponential table) sampler
//! identity built for the learner's production action sampling. Reusing it
//! here would misrepresent a numeric-parity claim that does not exist for
//! this new opponent-scoring surface.
//!
//! What IS reused is the seed-consumption discipline: exactly one
//! `splitmix64_first` draw per decision (the same primitive
//! `FastCategoricalScratch::sample` uses), mapped into `[0, sum)` by
//! treating the draw as a fraction of `2**64`, then walked against the
//! cumulative distribution (inverse CDF). This mirrors the frozen uniform
//! sampler's own documented bias posture: the mapping is intentionally
//! biased (no rejection sampling) and the bias is disclosed here rather
//! than hidden.

use crate::fast_sampler::splitmix64_first;
use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
use crate::native_opponent_policy_v2::{
    FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2, OPPONENT_LADDER_POOL_IDENTITY_V2,
    OPPONENT_LADDER_POOL_SIZE_V2, OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2,
    OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2, OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2,
    OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
};
use crate::native_opponent_sampler_v1::select_native_trainer_opponent_action_v1;
use crate::native_trainer_schedule_v2::{
    derive_native_trainer_opponent_pool_choice_seed_v2, select_opponent_ladder_pool_member_v2,
    OpponentLadderPoolMemberV2,
};
use crate::native_training_store_run_v2::OpponentLadderPoolContractV1;
use core::fmt;

/// Identity for this module's independent softmax-sampling surface. Not a
/// run-record field (the pinned run-record identity for the policy sampling
/// rule is `FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2`, owned by
/// `native_opponent_policy_v2`); this is a local documentation/test anchor
/// for the concrete f64 numeric construction implemented here.
pub(crate) const LADDER_OPPONENT_POLICY_SOFTMAX_SAMPLER_IDENTITY_V1: &str =
    "ladder-opponent-softmax-temperature-1.0-f64-splitmix64-first-inverse-cdf-v1";

/// `2**64`, exactly representable in `f64` (a pure power of two).
const TWO_POW_64_V1: f64 = 18_446_744_073_709_551_616.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LadderOpponentErrorV1 {
    /// The supplied `OpponentLadderPoolContractV1` does not match the
    /// frozen ladder identity/size/weights/sampling-rule literals.
    PoolContractMismatch,
    /// `select_policy_action_v1` was called with the uniform-floor member.
    NotAPolicyMember,
    /// A V2 schedule seed derivation rejected its inputs (outside the u63
    /// domain).
    ScheduleDerivation,
    /// `NativeCheckpointInferenceV1::score_decision_v1` rejected the
    /// decision.
    Inference,
    /// The frozen uniform sampler rejected its inputs.
    UniformSampler,
    /// The score vector was empty.
    EmptyScores,
    /// A score was non-finite at the given legal-action index.
    NonFiniteScore { index: usize },
    /// The softmax normalizer was not a finite positive number.
    InvalidDistribution,
}

impl fmt::Display for LadderOpponentErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PoolContractMismatch => {
                formatter.write_str("ladder pool contract does not match the frozen literals")
            }
            Self::NotAPolicyMember => {
                formatter.write_str("select_policy_action_v1 requires a policy-driven pool member")
            }
            Self::ScheduleDerivation => {
                formatter.write_str("ladder schedule seed derivation failed")
            }
            Self::Inference => formatter.write_str("checkpoint inference rejected the decision"),
            Self::UniformSampler => {
                formatter.write_str("frozen uniform sampler rejected its inputs")
            }
            Self::EmptyScores => {
                formatter.write_str("softmax sampling requires at least one score")
            }
            Self::NonFiniteScore { index } => {
                write!(formatter, "non-finite score at legal index {index}")
            }
            Self::InvalidDistribution => {
                formatter.write_str("softmax normalizer was not finite and positive")
            }
        }
    }
}

impl std::error::Error for LadderOpponentErrorV1 {}

/// Resolves the ladder pool member for one episode via the
/// `train-opponent-pool-choice` seed and the frozen 40/20/20/20 thresholds
/// (Self-Play Ladder Design Contract S2, Section 3). Pure function of
/// `(base_seed, episode_index)`; touches no loaded checkpoint.
pub(crate) fn ladder_pool_member_for_episode_v1(
    base_seed: u64,
    episode_index: u64,
) -> Result<OpponentLadderPoolMemberV2, LadderOpponentErrorV1> {
    let seed = derive_native_trainer_opponent_pool_choice_seed_v2(base_seed, episode_index)
        .map_err(|_| LadderOpponentErrorV1::ScheduleDerivation)?;
    Ok(select_opponent_ladder_pool_member_v2(seed))
}

/// Selects the uniform-floor member's action via the EXISTING frozen
/// uniform sampler, unchanged: bit-identical to a plain uniform run for the
/// same `(base_seed, episode_index, opponent_physical_decision_index,
/// substep_index, legal_action_count)` tuple.
pub(crate) fn select_ladder_floor_action_v1(
    base_seed: u64,
    episode_index: u64,
    opponent_physical_decision_index: u64,
    substep_index: u32,
    legal_action_count: u32,
) -> Result<u32, LadderOpponentErrorV1> {
    select_native_trainer_opponent_action_v1(
        base_seed,
        episode_index,
        opponent_physical_decision_index,
        substep_index,
        legal_action_count,
    )
    .map(|selection| selection.selected_index)
    .map_err(|_| LadderOpponentErrorV1::UniformSampler)
}

/// Samples one action index from `scores` (raw per-action model output, one
/// per legal action in order) via softmax at temperature 1.0, consuming
/// exactly one seed. See the module docs for the numeric discipline and
/// bias posture.
pub(crate) fn softmax_sample_temperature_one_v1(
    scores: &[f32],
    seed: u64,
) -> Result<u32, LadderOpponentErrorV1> {
    if scores.is_empty() {
        return Err(LadderOpponentErrorV1::EmptyScores);
    }
    let mut max_score = f64::NEG_INFINITY;
    for (index, &score) in scores.iter().enumerate() {
        let value = f64::from(score);
        if !value.is_finite() {
            return Err(LadderOpponentErrorV1::NonFiniteScore { index });
        }
        if value > max_score {
            max_score = value;
        }
    }
    let mut exp_values = Vec::with_capacity(scores.len());
    let mut sum = 0.0_f64;
    for &score in scores {
        let exp_value = (f64::from(score) - max_score).exp();
        sum += exp_value;
        exp_values.push(exp_value);
    }
    if !(sum.is_finite() && sum > 0.0) {
        return Err(LadderOpponentErrorV1::InvalidDistribution);
    }
    // Uniform-u64-to-interval construction: the single splitmix64 draw is
    // treated as a fraction of 2**64, then scaled to [0, sum). This mirrors
    // the "one seed -> one uniform draw -> inverse CDF" discipline
    // `FastCategoricalScratch::sample` uses (see module docs), replicated
    // here at f64 softmax precision instead of the fixed-point Q63 table.
    let draw = splitmix64_first(seed);
    let fraction = (draw as f64) / TWO_POW_64_V1;
    let target = fraction * sum;
    let mut cumulative = 0.0_f64;
    for (index, &exp_value) in exp_values.iter().enumerate() {
        cumulative += exp_value;
        if target < cumulative {
            return Ok(index as u32);
        }
    }
    // Bias posture (documented, intentional): mathematically `fraction` is
    // always strictly below 1.0 (draw <= u64::MAX < 2**64), so `target` is
    // always strictly below `sum` and the loop above always returns.
    // Floating-point rounding of the division/multiplication is the only
    // way this fallback is reached; it folds that vanishingly rare rounding
    // slack onto the last action rather than erroring, exactly the same
    // "intentional bias, no rejection sampling" posture the frozen uniform
    // sampler and the pool-choice threshold rule both document explicitly.
    Ok((scores.len() - 1) as u32)
}

/// Pure literal check: does `pool` match the frozen ladder
/// identity/size/weights/sampling-rule constants? Factored out of
/// `LadderOpponentEngineV1::new_v1` so it is directly testable without
/// constructing any checkpoint handle.
fn ladder_pool_matches_frozen_literals_v1(pool: &OpponentLadderPoolContractV1) -> bool {
    pool.identity == OPPONENT_LADDER_POOL_IDENTITY_V2
        && pool.size == OPPONENT_LADDER_POOL_SIZE_V2
        && pool.policy_member_sampling_rule == FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2
        && pool.weight_primary == OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2
        && pool.weight_predecessor_a == OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2
        && pool.weight_predecessor_b == OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2
        && pool.weight_uniform_floor == OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2
}

/// Runtime bridge from one validated ladder pool contract plus three loaded
/// frozen-checkpoint inference handles to per-decision opponent action
/// selection. Constructed once per process/run.
pub(crate) struct LadderOpponentEngineV1 {
    pool: OpponentLadderPoolContractV1,
    primary: NativeCheckpointInferenceV1,
    predecessor_a: NativeCheckpointInferenceV1,
    predecessor_b: NativeCheckpointInferenceV1,
}

impl fmt::Debug for LadderOpponentEngineV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LadderOpponentEngineV1")
            .field("pool_identity", &self.pool.identity)
            .field("primary", &self.primary)
            .field("predecessor_a", &self.predecessor_a)
            .field("predecessor_b", &self.predecessor_b)
            .finish()
    }
}

impl LadderOpponentEngineV1 {
    /// Constructs the engine from a validated pool contract and three
    /// already-loaded, digest-revalidated checkpoint handles. Fails closed
    /// if the pool contract's literal fields do not match the frozen
    /// ladder identity/size/weights/sampling-rule constants (defense in
    /// depth: the run-record validator already enforces this for any
    /// validated record, but this constructor does not assume its caller
    /// routed the contract through that validator).
    ///
    /// Checkpoint-to-pool-slot correspondence (which handle is PRIMARY vs
    /// PREDECESSOR-A vs PREDECESSOR-B) and digest cross-validation between
    /// each handle and its `OpponentLadderCheckpointRefV1` are the caller's
    /// responsibility; this constructor does not attempt to infer or
    /// re-verify that mapping.
    pub(crate) fn new_v1(
        pool: OpponentLadderPoolContractV1,
        primary: NativeCheckpointInferenceV1,
        predecessor_a: NativeCheckpointInferenceV1,
        predecessor_b: NativeCheckpointInferenceV1,
    ) -> Result<Self, LadderOpponentErrorV1> {
        if !ladder_pool_matches_frozen_literals_v1(&pool) {
            return Err(LadderOpponentErrorV1::PoolContractMismatch);
        }
        Ok(Self {
            pool,
            primary,
            predecessor_a,
            predecessor_b,
        })
    }

    pub(crate) fn pool(&self) -> &OpponentLadderPoolContractV1 {
        &self.pool
    }

    /// See [`ladder_pool_member_for_episode_v1`]; exposed as a method for a
    /// single cohesive engine API surface.
    pub(crate) fn pool_member_for_episode_v1(
        &self,
        base_seed: u64,
        episode_index: u64,
    ) -> Result<OpponentLadderPoolMemberV2, LadderOpponentErrorV1> {
        ladder_pool_member_for_episode_v1(base_seed, episode_index)
    }

    fn handle_for_v1(
        &self,
        member: OpponentLadderPoolMemberV2,
    ) -> Result<&NativeCheckpointInferenceV1, LadderOpponentErrorV1> {
        match member {
            OpponentLadderPoolMemberV2::Primary => Ok(&self.primary),
            OpponentLadderPoolMemberV2::PredecessorA => Ok(&self.predecessor_a),
            OpponentLadderPoolMemberV2::PredecessorB => Ok(&self.predecessor_b),
            OpponentLadderPoolMemberV2::UniformFloor => {
                Err(LadderOpponentErrorV1::NotAPolicyMember)
            }
        }
    }

    /// Selects the action for a policy-driven pool member (PRIMARY,
    /// PREDECESSOR-A, or PREDECESSOR-B): scores `decision` with the
    /// member's checkpoint via `score_decision_v1`, then samples from the
    /// softmax(temperature=1.0) distribution with `policy_substep_seed`
    /// (the caller-derived `train-opponent-policy-substep` seed). Returns
    /// [`LadderOpponentErrorV1::NotAPolicyMember`] for the uniform-floor
    /// member; use [`select_ladder_floor_action_v1`] (or
    /// [`Self::select_floor_action_v1`]) for that case instead.
    pub(crate) fn select_policy_action_v1(
        &self,
        member: OpponentLadderPoolMemberV2,
        decision: FlatScoringDecisionViewV2<'_>,
        policy_substep_seed: u64,
    ) -> Result<u32, LadderOpponentErrorV1> {
        let handle = self.handle_for_v1(member)?;
        let output = handle
            .score_decision_v1(decision)
            .map_err(|_| LadderOpponentErrorV1::Inference)?;
        softmax_sample_temperature_one_v1(output.action_logits(), policy_substep_seed)
    }

    /// See [`select_ladder_floor_action_v1`]; exposed as a method for a
    /// single cohesive engine API surface (does not touch `self`: the
    /// floor member delegates entirely to the pre-existing frozen uniform
    /// sampler).
    pub(crate) fn select_floor_action_v1(
        base_seed: u64,
        episode_index: u64,
        opponent_physical_decision_index: u64,
        substep_index: u32,
        legal_action_count: u32,
    ) -> Result<u32, LadderOpponentErrorV1> {
        select_ladder_floor_action_v1(
            base_seed,
            episode_index,
            opponent_physical_decision_index,
            substep_index,
            legal_action_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_identity_is_printable_ascii_and_distinct_from_run_record_identity() {
        assert!(!LADDER_OPPONENT_POLICY_SOFTMAX_SAMPLER_IDENTITY_V1.is_empty());
        assert!(LADDER_OPPONENT_POLICY_SOFTMAX_SAMPLER_IDENTITY_V1
            .bytes()
            .all(|byte| (0x20..=0x7e).contains(&byte)));
        assert_ne!(
            LADDER_OPPONENT_POLICY_SOFTMAX_SAMPLER_IDENTITY_V1,
            FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2
        );
    }

    // ------------------------------------------------------------------
    // Pool-choice threshold boundaries (raw seed values), through this
    // module's own entry point over `select_opponent_ladder_pool_member_v2`.
    // ------------------------------------------------------------------

    #[test]
    fn pool_member_thresholds_hold_at_39_40_59_60_79_80_99_boundaries() {
        assert_eq!(
            select_opponent_ladder_pool_member_v2(39),
            OpponentLadderPoolMemberV2::Primary
        );
        assert_eq!(
            select_opponent_ladder_pool_member_v2(40),
            OpponentLadderPoolMemberV2::PredecessorA
        );
        assert_eq!(
            select_opponent_ladder_pool_member_v2(59),
            OpponentLadderPoolMemberV2::PredecessorA
        );
        assert_eq!(
            select_opponent_ladder_pool_member_v2(60),
            OpponentLadderPoolMemberV2::PredecessorB
        );
        assert_eq!(
            select_opponent_ladder_pool_member_v2(79),
            OpponentLadderPoolMemberV2::PredecessorB
        );
        assert_eq!(
            select_opponent_ladder_pool_member_v2(80),
            OpponentLadderPoolMemberV2::UniformFloor
        );
        assert_eq!(
            select_opponent_ladder_pool_member_v2(99),
            OpponentLadderPoolMemberV2::UniformFloor
        );
    }

    /// Wiring proof: this module's own episode-level entry point reaches
    /// the frozen threshold rule end to end (seed derivation included), not
    /// just the already-tested raw-seed threshold function.
    #[test]
    fn ladder_pool_member_for_episode_matches_derived_seed_threshold() {
        let base_seed = 71_501_u64;
        for episode_index in [0_u64, 1, 2, 63, 64, 65, 4_095] {
            let seed = derive_native_trainer_opponent_pool_choice_seed_v2(base_seed, episode_index)
                .unwrap();
            let expected = select_opponent_ladder_pool_member_v2(seed);
            assert_eq!(
                ladder_pool_member_for_episode_v1(base_seed, episode_index).unwrap(),
                expected
            );
        }
    }

    // ------------------------------------------------------------------
    // Floor-member bit-identity: same seeds through the engine-free-fn
    // path vs the raw uniform sampler give identical indices.
    // ------------------------------------------------------------------

    #[test]
    fn floor_action_is_bit_identical_to_raw_uniform_sampler() {
        let base_seed = 71_501_u64;
        for (episode_index, ordinal, substep, legal_count) in [
            (0_u64, 0_u64, 0_u32, 1_u32),
            (0, 0, 1, 2),
            (1, 3, 0, 64),
            (4_095, 7, 1, 17),
        ] {
            let via_engine = select_ladder_floor_action_v1(
                base_seed,
                episode_index,
                ordinal,
                substep,
                legal_count,
            )
            .unwrap();
            let via_engine_method = LadderOpponentEngineV1::select_floor_action_v1(
                base_seed,
                episode_index,
                ordinal,
                substep,
                legal_count,
            )
            .unwrap();
            let raw = select_native_trainer_opponent_action_v1(
                base_seed,
                episode_index,
                ordinal,
                substep,
                legal_count,
            )
            .unwrap();
            assert_eq!(via_engine, raw.selected_index);
            assert_eq!(via_engine_method, raw.selected_index);
        }
    }

    // ------------------------------------------------------------------
    // Softmax sampling goldens: synthetic score vectors, pinned indices.
    // ------------------------------------------------------------------

    #[test]
    fn softmax_sample_rejects_empty_and_non_finite() {
        assert_eq!(
            softmax_sample_temperature_one_v1(&[], 0),
            Err(LadderOpponentErrorV1::EmptyScores)
        );
        assert_eq!(
            softmax_sample_temperature_one_v1(&[0.0, f32::NAN], 0),
            Err(LadderOpponentErrorV1::NonFiniteScore { index: 1 })
        );
        assert_eq!(
            softmax_sample_temperature_one_v1(&[f32::INFINITY, 0.0], 0),
            Err(LadderOpponentErrorV1::NonFiniteScore { index: 0 })
        );
    }

    #[test]
    fn softmax_sample_uniform_scores_partition_seed_space_by_width() {
        // Equal scores -> uniform categorical distribution: the sampled
        // index is purely a function of which half of the (post-splitmix)
        // u64 domain the seed's draw lands in. Pinned via the actual
        // `splitmix64_first` output for each seed, not an assumption about
        // the raw seed value's own magnitude.
        let scores = [0.0_f32, 0.0];
        assert_eq!(
            softmax_sample_temperature_one_v1(&scores, 12_345).unwrap(),
            0
        );
        assert_eq!(softmax_sample_temperature_one_v1(&scores, 0).unwrap(), 1);
    }

    #[test]
    fn softmax_sample_favors_the_higher_score() {
        // A large positive gap must collapse virtually all mass onto index
        // 1 regardless of seed.
        let scores = [-50.0_f32, 50.0];
        for seed in [0_u64, 1, 12_345, u64::MAX / 2, u64::MAX] {
            assert_eq!(softmax_sample_temperature_one_v1(&scores, seed).unwrap(), 1);
        }
    }

    /// Pinned golden vectors: fixed synthetic score vector, fixed seed set,
    /// fixed expected indices. Regenerating these requires recomputing the
    /// f64 softmax construction documented in the module docs; a change in
    /// the result for any of these (seed, scores) pairs is a numeric
    /// regression in this new sampling surface.
    #[test]
    fn softmax_sample_matches_pinned_goldens() {
        let scores = [0.1_f32, -0.3, 2.0, 0.0, -1.5];
        let goldens: &[(u64, u32)] = &[
            (0, 3),
            (1, 2),
            (12_345, 1),
            (0xDEAD_BEEF_0000_0000, 2),
            (u64::MAX / 2, 1),
            (u64::MAX, 3),
        ];
        for &(seed, expected) in goldens {
            assert_eq!(
                softmax_sample_temperature_one_v1(&scores, seed).unwrap(),
                expected,
                "seed={seed}"
            );
        }
    }

    // ------------------------------------------------------------------
    // Engine construction fail-closed matrix.
    // ------------------------------------------------------------------

    fn hex64(fill: char) -> String {
        std::iter::repeat(fill).take(64).collect()
    }

    fn checkpoint_ref(
        fill: char,
    ) -> crate::native_training_store_run_v2::OpponentLadderCheckpointRefV1 {
        crate::native_training_store_run_v2::OpponentLadderCheckpointRefV1 {
            source_run_sha256: hex64(fill),
            generation: 256,
            checkpoint_sha256: hex64(fill),
            sidecar_sha256: hex64(fill),
            state_sha256: hex64(fill),
        }
    }

    fn valid_pool_fixture() -> OpponentLadderPoolContractV1 {
        OpponentLadderPoolContractV1 {
            identity: OPPONENT_LADDER_POOL_IDENTITY_V2.to_owned(),
            size: OPPONENT_LADDER_POOL_SIZE_V2,
            policy_member_sampling_rule: FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2
                .to_owned(),
            weight_primary: OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2,
            weight_predecessor_a: OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2,
            weight_predecessor_b: OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2,
            weight_uniform_floor: OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
            primary: checkpoint_ref('a'),
            predecessor_a: checkpoint_ref('b'),
            predecessor_b: checkpoint_ref('c'),
            uniform_floor: crate::native_training_store_run_v2::OpponentLadderUniformFloorV1 {
                identity:
                    crate::native_opponent_sampler_v1::NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1
                        .to_owned(),
                model_rule:
                    crate::native_opponent_sampler_v1::NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1
                        .to_owned(),
                sampler_identity: crate::native_opponent_sampler_v1::UNIFORM_INDEX_MODULO_U64_IDENTITY_V1
                    .to_owned(),
                sampler_algorithm:
                    crate::native_opponent_sampler_v1::UNIFORM_INDEX_MODULO_U64_ALGORITHM_V1.to_owned(),
            },
        }
    }

    #[test]
    fn pool_contract_literal_matcher_accepts_valid_and_rejects_corruption_matrix() {
        assert!(ladder_pool_matches_frozen_literals_v1(&valid_pool_fixture()));

        let mut corrupted = valid_pool_fixture();
        corrupted.weight_primary = 41;
        assert!(!ladder_pool_matches_frozen_literals_v1(&corrupted));

        let mut corrupted = valid_pool_fixture();
        corrupted.size = 5;
        assert!(!ladder_pool_matches_frozen_literals_v1(&corrupted));

        let mut corrupted = valid_pool_fixture();
        corrupted.identity = "wrong".to_owned();
        assert!(!ladder_pool_matches_frozen_literals_v1(&corrupted));

        let mut corrupted = valid_pool_fixture();
        corrupted.policy_member_sampling_rule = "wrong".to_owned();
        assert!(!ladder_pool_matches_frozen_literals_v1(&corrupted));

        let mut corrupted = valid_pool_fixture();
        corrupted.weight_uniform_floor = 21;
        assert!(!ladder_pool_matches_frozen_literals_v1(&corrupted));
    }

    // `LadderOpponentEngineV1::new_v1` itself just calls
    // `ladder_pool_matches_frozen_literals_v1` before touching any
    // checkpoint handle (see its body above), so the matcher test above
    // fully covers its fail-closed branch. A full engine-construction-
    // succeeds-against-a-real-checkpoint test belongs to the pilot runner
    // integration layer (`native_ladder_pool_resolution_v1`'s fixture (a)),
    // which resolves real checkpoint bytes to validated handles; the
    // cross-process determinism fixture below builds its own lightweight
    // in-memory genesis checkpoint instead (no real Store, no D:\
    // dependency), since determinism of the seed/sampling MATH is what is
    // under test here, not any particular checkpoint's content.

    // ------------------------------------------------------------------
    // Fixture (b): cross-process determinism (contract Section 7). Same
    // checkpoint bytes, same pool contract, same seeds, run the engine's
    // per-episode member selection plus per-decision sampling for a fixed
    // synthetic schedule TWICE via `std::process::Command` re-invoking this
    // test binary (the pattern already used by
    // `strict_source_tree_attestation_v1`'s
    // `inherited_git_routing_is_stripped_without_mutating_global_environment`:
    // the SAME `#[test]` fn serves as both parent and child, gated by an
    // env var the parent sets only on the spawned children) and assert
    // bit-identical action sequences.
    const DETERMINISM_CHILD_MODE_V1: &str = "MTG_KERNEL_LADDER_DETERMINISM_CHILD_V1";
    const DETERMINISM_DIGEST_PREFIX_V1: &str = "LADDER_DETERMINISM_DIGEST=";

    fn determinism_execution_config_v1(
        run: &crate::native_training_store_run_v2::ValidatedTrainRunV2,
    ) -> crate::native_training_executor_v1::NativeTrainingExecutionConfigV1 {
        crate::native_training_executor_v1::NativeTrainingExecutionConfigV1 {
            run_base_seed: run.record().schedule.base_seed,
            batch_episodes: run.batch_episodes(),
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            max_physical_decisions: run.record().limits.max_physical_decisions,
            max_policy_steps: run.record().limits.max_policy_steps,
            worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
            sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker)
                .unwrap(),
            broker_batch_target: usize::try_from(run.record().topology.broker_batch_target)
                .unwrap(),
            scheduler_timeout: std::time::Duration::from_secs(30),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5_f32.to_bits(),
            learning_rate_bits: 0.001_f32.to_bits(),
            numerical_backend:
                crate::native_training_executor_v1::NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    /// Builds one fresh in-memory genesis checkpoint (no filesystem Store,
    /// no D:\ dependency) and returns three independently loaded
    /// `NativeCheckpointInferenceV1` handles built from the identical bytes,
    /// mirroring `native_checkpoint_inference_v1::tests::fixture_v1` /
    /// `authorities_v1`'s pattern (that machinery is private to its own
    /// module; this reproduces the same public-constructor sequence here).
    fn determinism_engine_v1() -> LadderOpponentEngineV1 {
        use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
        use crate::native_training_executor_v1::NativeTrainingExecutorV1;
        use crate::native_training_store_checkpoint_v3::{
            build_genesis_checkpoint_manifest_v3, decode_genesis_checkpoint_manifest_v3,
        };
        use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};

        let run_bytes = test_fixture_bytes_v2();
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let (snapshot_manifest, snapshot_payload) =
            crate::common_model_snapshot_v1::common_model_snapshot_paths_v1();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            determinism_execution_config_v1(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let checkpoint_candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = checkpoint_candidate.payload().to_vec();
        let manifest = build_genesis_checkpoint_manifest_v3(&run, &payload)
            .unwrap()
            .canonical_bytes()
            .to_vec();

        let mut handles = Vec::with_capacity(3);
        for _ in 0..3 {
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let checkpoint =
                decode_genesis_checkpoint_manifest_v3(&manifest, &payload, &run).unwrap();
            handles.push(load_native_checkpoint_inference_v1(&run, &checkpoint, &payload).unwrap());
        }
        let mut handles = handles.into_iter();
        let primary = handles.next().unwrap();
        let predecessor_a = handles.next().unwrap();
        let predecessor_b = handles.next().unwrap();
        LadderOpponentEngineV1::new_v1(valid_pool_fixture(), primary, predecessor_a, predecessor_b)
            .unwrap()
    }

    fn determinism_decision_view_parts_v1() -> (
        crate::flat_policy_v2::FlatGlobalsV2,
        [crate::flat_policy_v2::FlatScorerActionCoreV2; 2],
    ) {
        use crate::flat_policy_v2::{FlatGlobalsV2, FlatRelativePlayerV2, FlatScorerActionCoreV2};
        (
            FlatGlobalsV2 {
                acting_player: FlatRelativePlayerV2::SelfPlayer,
                ..FlatGlobalsV2::default()
            },
            [
                FlatScorerActionCoreV2::default(),
                FlatScorerActionCoreV2::default(),
            ],
        )
    }

    /// The fixed synthetic schedule this fixture drives: every episode in
    /// `0..EPISODE_COUNT` (enough for the 40/20/20/20 thresholds to cover
    /// all four pool members at `DETERMINISM_BASE_SEED`), one physical
    /// decision per episode, one substep, over the two-action decision view
    /// above. Returns a joined string of
    /// `episode:member:action` triples -- the "action sequence" fixture (b)
    /// asks for, in a stable, cross-process-comparable text form.
    fn run_fixed_synthetic_schedule_v1(engine: &LadderOpponentEngineV1) -> String {
        use crate::flat_policy_v2::FlatScoringDecisionViewV2;

        const DETERMINISM_BASE_SEED: u64 = 555_001;
        const EPISODE_COUNT: u64 = 40;

        let (globals, actions) = determinism_decision_view_parts_v1();
        let mut lines = Vec::with_capacity(EPISODE_COUNT as usize);
        for episode_index in 0..EPISODE_COUNT {
            let member = engine
                .pool_member_for_episode_v1(DETERMINISM_BASE_SEED, episode_index)
                .unwrap();
            let action = match member {
                OpponentLadderPoolMemberV2::UniformFloor => {
                    LadderOpponentEngineV1::select_floor_action_v1(
                        DETERMINISM_BASE_SEED,
                        episode_index,
                        0,
                        0,
                        actions.len() as u32,
                    )
                    .unwrap()
                }
                policy_member => {
                    let seed = crate::native_trainer_schedule_v2::derive_native_trainer_opponent_policy_substep_seed_v2(
                        DETERMINISM_BASE_SEED,
                        episode_index,
                        0,
                        0,
                    )
                    .unwrap();
                    let view = FlatScoringDecisionViewV2::new(
                        &globals,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &actions,
                        &[],
                    );
                    engine
                        .select_policy_action_v1(policy_member, view, seed)
                        .unwrap()
                }
            };
            lines.push(format!("{episode_index}:{member:?}:{action}"));
        }
        lines.join("|")
    }

    fn determinism_spawn_child_v1() -> String {
        let self_exe = std::env::current_exe().expect("current test binary path");
        let output = std::process::Command::new(self_exe)
            .arg("native_ladder_opponent_v1::tests::cross_process_determinism_bit_identical_action_sequence")
            .arg("--exact")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(DETERMINISM_CHILD_MODE_V1, "1")
            .output()
            .expect("spawn determinism child process");
        assert!(
            output.status.success(),
            "determinism child process failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // The test harness's own "test <name> ... " progress prefix shares a
        // line with `--nocapture` output under `--test-threads=1`, so the
        // digest marker is not necessarily at the START of its line; search
        // the whole capture for the marker instead of anchoring per-line.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let after_marker = stdout
            .split(DETERMINISM_DIGEST_PREFIX_V1)
            .nth(1)
            .expect("child must print its digest line");
        after_marker
            .split_whitespace()
            .next()
            .expect("digest line must carry a value")
            .to_owned()
    }

    #[test]
    fn cross_process_determinism_bit_identical_action_sequence() {
        if std::env::var_os(DETERMINISM_CHILD_MODE_V1).is_some() {
            let engine = determinism_engine_v1();
            let sequence = run_fixed_synthetic_schedule_v1(&engine);
            println!("{DETERMINISM_DIGEST_PREFIX_V1}{sequence}");
            return;
        }

        let first = determinism_spawn_child_v1();
        let second = determinism_spawn_child_v1();
        assert!(!first.is_empty());
        assert_eq!(
            first, second,
            "cross-process action sequences must be bit-identical"
        );
        // Sanity: the fixed schedule must actually exercise more than one
        // pool member at this base seed (otherwise the fixture would prove
        // nothing about the policy-sampling branch).
        assert!(first.contains("Primary") || first.contains("PredecessorA"));
        assert!(first.contains("UniformFloor") || first.contains("PredecessorB"));
    }
}
