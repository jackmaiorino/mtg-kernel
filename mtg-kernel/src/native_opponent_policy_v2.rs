//! Frozen ladder-opponent identities layered on the frozen uniform sampler.
//!
//! Self-Play Ladder Design Contract (S2), Section 2. The V1 uniform opponent
//! identities in `native_opponent_sampler_v1` (owned there, validated in
//! `native_training_store_run_v2`) are FROZEN FOREVER and untouched by this
//! module: their values are never edited and their fields are never
//! repurposed. A ladder run is distinguished from a uniform run purely by
//! carrying these NEW identity strings in its opponent contracts, plus a new
//! ladder pool section (`OpponentLadderPoolContractV1`, owned by
//! `native_training_store_run_v2`) that is present if and only if the ladder
//! policy identity is present.
//!
//! Per contract Section 3, the ladder opponent pool is a fixed K = 4 window:
//! PRIMARY, PREDECESSOR-A, PREDECESSOR-B (checkpoint references), and the
//! UNIFORM FLOOR (the frozen uniform sampler, reused verbatim via the
//! preserved superseded-identity semantics for that one slot). Per episode,
//! the pool member is drawn with fixed weights 40/20/20/20
//! (primary/predecessor-a/predecessor-b/uniform-floor); weights are contract
//! constants and changing them mints a new contract version.

pub const FROZEN_CHECKPOINT_OPPONENT_POLICY_IDENTITY_V2: &str =
    "mtg-kernel-trainer-frozen-checkpoint-policy-v2";
pub const FROZEN_CHECKPOINT_OPPONENT_POLICY_MODEL_RULE_V2: &str =
    "frozen-checkpoint-softmax-t1-one-seed-per-decision";

/// The selection rule for the three policy-driven pool slots: a schedule-
/// derived seed drives one categorical sample per opponent decision from the
/// frozen checkpoint's softmax(temperature=1.0) policy distribution over
/// legal actions. No argmax; the RNG consumes exactly one seed per opponent
/// decision, matching the width-one rule of the uniform sampler contract
/// this ladder pool supersedes for the non-floor slots.
pub const FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2: &str =
    "seeded-categorical-sample-from-softmax-temperature-1.0-checkpoint-policy-one-seed-per-decision";

pub const OPPONENT_LADDER_POOL_IDENTITY_V2: &str = "mtg-kernel-opponent-ladder-pool-v1";

/// K, the fixed pool size (contract Section 3).
pub const OPPONENT_LADDER_POOL_SIZE_V2: u64 = 4;

pub const OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2: u64 = 40;
pub const OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2: u64 = 20;
pub const OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2: u64 = 20;
pub const OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2: u64 = 20;
pub const OPPONENT_LADDER_POOL_WEIGHT_TOTAL_V2: u64 = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sum_to_total_and_match_contract_ratios() {
        assert_eq!(
            OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2
                + OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2
                + OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2
                + OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2,
            OPPONENT_LADDER_POOL_WEIGHT_TOTAL_V2
        );
        assert_eq!(OPPONENT_LADDER_POOL_WEIGHT_TOTAL_V2, 100);
        // 80 percent policy opponent, 20 percent uniform floor (Section 3).
        assert_eq!(
            OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2
                + OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2
                + OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2,
            80
        );
        assert_eq!(OPPONENT_LADDER_POOL_WEIGHT_UNIFORM_FLOOR_V2, 20);
        assert_eq!(OPPONENT_LADDER_POOL_SIZE_V2, 4);
    }

    #[test]
    fn identities_are_printable_ascii_and_distinct_from_uniform_v1() {
        for value in [
            FROZEN_CHECKPOINT_OPPONENT_POLICY_IDENTITY_V2,
            FROZEN_CHECKPOINT_OPPONENT_POLICY_MODEL_RULE_V2,
            FROZEN_CHECKPOINT_OPPONENT_POLICY_SAMPLING_RULE_V2,
            OPPONENT_LADDER_POOL_IDENTITY_V2,
        ] {
            assert!(!value.is_empty());
            assert!(value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)));
        }
        assert_ne!(
            FROZEN_CHECKPOINT_OPPONENT_POLICY_IDENTITY_V2,
            crate::native_opponent_sampler_v1::NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_IDENTITY_V1
        );
        assert_ne!(
            FROZEN_CHECKPOINT_OPPONENT_POLICY_MODEL_RULE_V2,
            crate::native_opponent_sampler_v1::NATIVE_TRAINER_UNIFORM_OPPONENT_POLICY_MODEL_RULE_V1
        );
    }
}
