//! Ladder opponent seed schedule additions layered on the V1 native trainer
//! schedule (Self-Play Ladder Design Contract S2, Section 2).
//!
//! Adds exactly two new namespaces, using the identical sha256 atom
//! construction `native_trainer_schedule_v1::derive_seed` uses (read there,
//! replicated here byte-for-byte rather than imported, so this module's
//! namespace/field-set declarations stay a single auditable unit):
//!
//! - `train-opponent-pool-choice` (fields: `base_seed`, `episode_index`):
//!   selects which of the K = 4 ladder pool members (Section 3) faces the
//!   learner for one episode.
//! - `train-opponent-policy-substep` (fields: `base_seed`, `episode_index`,
//!   `opponent_physical_decision_index`, `substep_index`): drives the
//!   per-decision seeded categorical sample from a policy-driven pool
//!   member's softmax(temperature=1.0) distribution.
//!
//! Both namespaces are flat single-stage derivations (unlike V1's
//! group-then-substep opponent-action pair): each takes its full field set
//! directly in one `derive_seed` call, exactly as specified by the S2
//! contract's field-set declarations.
//!
//! This module mints its own seed-derivation version atom
//! (`NATIVE_LADDER_SCHEDULE_SEED_VERSION_V2`), distinct from V1's
//! `PYTHON_REFERENCE_SEED_VERSION_V1`: there is no Python reference
//! implementation for these two namespaces, so reusing the Python-parity
//! version string would misrepresent a cross-language parity claim that does
//! not exist. The atom framing mechanism itself is identical to V1's.

use crate::native_opponent_policy_v2::{
    OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2, OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2,
    OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2, OPPONENT_LADDER_POOL_WEIGHT_TOTAL_V2,
};
use crate::native_trainer_schedule_v1::NativeTrainerScheduleContractV1;
use sha2::{Digest, Sha256};

pub(crate) const NATIVE_TRAINER_SCHEDULE_VERSION_V2: &str =
    "mtg-kernel-native-trainer-schedule-sha256-v2";
pub(crate) const NATIVE_LADDER_SCHEDULE_SEED_VERSION_V2: &str =
    "kernel-native-ladder-trainer-sha256-v1";
pub(crate) const NATIVE_TRAINER_SCHEDULE_V2_VERSION_CHANGE_RULE_V2: &str =
    "any seed, namespace, framing, domain, field-order, weight-threshold, or golden change requires a new schedule version announced on the CODEX-CLAUDE channel";

const TRAIN_OPPONENT_POOL_CHOICE_NAMESPACE_V2: &str = "train-opponent-pool-choice";
const TRAIN_OPPONENT_POOL_CHOICE_FIELDS_V2: &str = "base_seed,episode_index";
const TRAIN_OPPONENT_POLICY_SUBSTEP_NAMESPACE_V2: &str = "train-opponent-policy-substep";
const TRAIN_OPPONENT_POLICY_SUBSTEP_FIELDS_V2: &str =
    "base_seed,episode_index,opponent_physical_decision_index,substep_index";

/// The modulo-100 draw pool-choice maps to the 40/20/20/20 weight table via
/// explicit integer thresholds (contract Section 3).
pub(crate) const OPPONENT_LADDER_POOL_CHOICE_MODULO_V2: u64 = OPPONENT_LADDER_POOL_WEIGHT_TOTAL_V2;
pub(crate) const OPPONENT_LADDER_POOL_CHOICE_THRESHOLD_RULE_V2: &str = "draw=pool_choice_seed%100;[0,40)->primary;[40,60)->predecessor_a;[60,80)->predecessor_b;[80,100)->uniform_floor";
/// Same bias posture as the V1 uniform sampler's stated rule
/// (`UNIFORM_INDEX_MODULO_U64_BIAS_RULE_V1`): intentional, undisclosed-away,
/// no rejection sampling.
pub(crate) const OPPONENT_LADDER_POOL_CHOICE_BIAS_RULE_V2: &str =
    "intentional-modulo-bias-no-rejection-sampling;when-100-does-not-divide-the-seed-domain-low-residues-have-one-extra-preimage;consistent-with-the-v1-uniform-sampler-bias-rule;changing-this-rule-requires-a-new-schedule-version";

const U63_MAX: u64 = (1_u64 << 63) - 1;
const SEED_MASK: u64 = U63_MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeTrainerScheduleV2ErrorV1 {
    IntegerOutsideU63 { field: &'static str, value: u64 },
    AtomLengthOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTrainerScheduleContractV2 {
    pub(crate) v1: NativeTrainerScheduleContractV1,
    pub(crate) schedule_version: &'static str,
    pub(crate) seed_version: &'static str,
    pub(crate) opponent_pool_choice_namespace: &'static str,
    pub(crate) opponent_pool_choice_fields: &'static str,
    pub(crate) opponent_policy_substep_namespace: &'static str,
    pub(crate) opponent_policy_substep_fields: &'static str,
    pub(crate) pool_choice_modulo: u64,
    pub(crate) pool_choice_threshold_rule: &'static str,
    pub(crate) pool_choice_bias_rule: &'static str,
    pub(crate) version_change_rule: &'static str,
}

pub(crate) const NATIVE_TRAINER_SCHEDULE_CONTRACT_V2: NativeTrainerScheduleContractV2 =
    NativeTrainerScheduleContractV2 {
        v1: crate::native_trainer_schedule_v1::NATIVE_TRAINER_SCHEDULE_CONTRACT_V1,
        schedule_version: NATIVE_TRAINER_SCHEDULE_VERSION_V2,
        seed_version: NATIVE_LADDER_SCHEDULE_SEED_VERSION_V2,
        opponent_pool_choice_namespace: TRAIN_OPPONENT_POOL_CHOICE_NAMESPACE_V2,
        opponent_pool_choice_fields: TRAIN_OPPONENT_POOL_CHOICE_FIELDS_V2,
        opponent_policy_substep_namespace: TRAIN_OPPONENT_POLICY_SUBSTEP_NAMESPACE_V2,
        opponent_policy_substep_fields: TRAIN_OPPONENT_POLICY_SUBSTEP_FIELDS_V2,
        pool_choice_modulo: OPPONENT_LADDER_POOL_CHOICE_MODULO_V2,
        pool_choice_threshold_rule: OPPONENT_LADDER_POOL_CHOICE_THRESHOLD_RULE_V2,
        pool_choice_bias_rule: OPPONENT_LADDER_POOL_CHOICE_BIAS_RULE_V2,
        version_change_rule: NATIVE_TRAINER_SCHEDULE_V2_VERSION_CHANGE_RULE_V2,
    };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedFieldValueV2 {
    U63(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SeedFieldV2 {
    name: &'static str,
    value: SeedFieldValueV2,
}

const fn u63_field(name: &'static str, value: u64) -> SeedFieldV2 {
    SeedFieldV2 {
        name,
        value: SeedFieldValueV2::U63(value),
    }
}

fn checked_u63(field: &'static str, value: u64) -> Result<u64, NativeTrainerScheduleV2ErrorV1> {
    if value > U63_MAX {
        return Err(NativeTrainerScheduleV2ErrorV1::IntegerOutsideU63 { field, value });
    }
    Ok(value)
}

fn append_atom(
    hasher: &mut Sha256,
    tag: &str,
    payload: &[u8],
) -> Result<(), NativeTrainerScheduleV2ErrorV1> {
    let tag_len =
        u32::try_from(tag.len()).map_err(|_| NativeTrainerScheduleV2ErrorV1::AtomLengthOverflow)?;
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| NativeTrainerScheduleV2ErrorV1::AtomLengthOverflow)?;
    hasher.update(tag_len.to_be_bytes());
    hasher.update(tag.as_bytes());
    hasher.update(payload_len.to_be_bytes());
    hasher.update(payload);
    Ok(())
}

fn derive_seed(
    namespace: &'static str,
    fields: &[SeedFieldV2],
) -> Result<u64, NativeTrainerScheduleV2ErrorV1> {
    let mut hasher = Sha256::new();
    append_atom(
        &mut hasher,
        "version",
        NATIVE_LADDER_SCHEDULE_SEED_VERSION_V2.as_bytes(),
    )?;
    append_atom(&mut hasher, "namespace", namespace.as_bytes())?;
    for field in fields {
        append_atom(&mut hasher, "field-name", field.name.as_bytes())?;
        match field.value {
            SeedFieldValueV2::U63(value) => {
                let value = checked_u63(field.name, value)?;
                append_atom(&mut hasher, "u63", &value.to_be_bytes())?;
            }
        }
    }
    let digest = hasher.finalize();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    Ok(u64::from_be_bytes(prefix) & SEED_MASK)
}

/// Per-episode ladder pool member selection seed (contract Section 3).
pub(crate) fn derive_native_trainer_opponent_pool_choice_seed_v2(
    base_seed: u64,
    episode_index: u64,
) -> Result<u64, NativeTrainerScheduleV2ErrorV1> {
    derive_seed(
        TRAIN_OPPONENT_POOL_CHOICE_NAMESPACE_V2,
        &[
            u63_field("base_seed", base_seed),
            u63_field("episode_index", episode_index),
        ],
    )
}

/// Per-decision softmax sampling seed for a policy-driven pool member.
pub(crate) fn derive_native_trainer_opponent_policy_substep_seed_v2(
    base_seed: u64,
    episode_index: u64,
    opponent_physical_decision_index: u64,
    substep_index: u32,
) -> Result<u64, NativeTrainerScheduleV2ErrorV1> {
    derive_seed(
        TRAIN_OPPONENT_POLICY_SUBSTEP_NAMESPACE_V2,
        &[
            u63_field("base_seed", base_seed),
            u63_field("episode_index", episode_index),
            u63_field(
                "opponent_physical_decision_index",
                opponent_physical_decision_index,
            ),
            u63_field("substep_index", u64::from(substep_index)),
        ],
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpponentLadderPoolMemberV2 {
    Primary,
    PredecessorA,
    PredecessorB,
    UniformFloor,
}

const PRIMARY_THRESHOLD_V2: u64 = OPPONENT_LADDER_POOL_WEIGHT_PRIMARY_V2;
const PREDECESSOR_A_THRESHOLD_V2: u64 =
    PRIMARY_THRESHOLD_V2 + OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_A_V2;
const PREDECESSOR_B_THRESHOLD_V2: u64 =
    PREDECESSOR_A_THRESHOLD_V2 + OPPONENT_LADDER_POOL_WEIGHT_PREDECESSOR_B_V2;

/// Maps a pool-choice seed to a pool member via explicit integer thresholds
/// over a modulo-100 draw: `[0,40)` primary, `[40,60)` predecessor-a,
/// `[60,80)` predecessor-b, `[80,100)` uniform floor. Modulo bias is
/// intentional (`OPPONENT_LADDER_POOL_CHOICE_BIAS_RULE_V2`); no rejection
/// sampling.
pub(crate) fn select_opponent_ladder_pool_member_v2(
    pool_choice_seed: u64,
) -> OpponentLadderPoolMemberV2 {
    let draw = pool_choice_seed % OPPONENT_LADDER_POOL_CHOICE_MODULO_V2;
    if draw < PRIMARY_THRESHOLD_V2 {
        OpponentLadderPoolMemberV2::Primary
    } else if draw < PREDECESSOR_A_THRESHOLD_V2 {
        OpponentLadderPoolMemberV2::PredecessorA
    } else if draw < PREDECESSOR_B_THRESHOLD_V2 {
        OpponentLadderPoolMemberV2::PredecessorB
    } else {
        OpponentLadderPoolMemberV2::UniformFloor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_v2_contract_bundles_v1_and_is_scheduler_free() {
        let contract = NATIVE_TRAINER_SCHEDULE_CONTRACT_V2;
        assert_eq!(
            contract.v1,
            crate::native_trainer_schedule_v1::NATIVE_TRAINER_SCHEDULE_CONTRACT_V1
        );
        assert_eq!(
            contract.schedule_version,
            NATIVE_TRAINER_SCHEDULE_VERSION_V2
        );
        assert_eq!(
            contract.opponent_pool_choice_namespace,
            TRAIN_OPPONENT_POOL_CHOICE_NAMESPACE_V2
        );
        assert_eq!(
            contract.opponent_policy_substep_namespace,
            TRAIN_OPPONENT_POLICY_SUBSTEP_NAMESPACE_V2
        );
        assert_eq!(contract.pool_choice_modulo, 100);
        for forbidden in [
            "worker", "lane", "session", "chunk", "round", "batch", "timing", "device",
        ] {
            assert!(!TRAIN_OPPONENT_POOL_CHOICE_NAMESPACE_V2.contains(forbidden));
            assert!(!TRAIN_OPPONENT_POLICY_SUBSTEP_NAMESPACE_V2.contains(forbidden));
        }
        assert_ne!(
            NATIVE_TRAINER_SCHEDULE_VERSION_V2,
            NATIVE_LADDER_SCHEDULE_SEED_VERSION_V2
        );
        assert_ne!(
            NATIVE_LADDER_SCHEDULE_SEED_VERSION_V2,
            crate::native_trainer_schedule_v1::PYTHON_REFERENCE_SEED_VERSION_V1
        );
    }

    #[test]
    fn pool_choice_thresholds_partition_0_to_99_by_weight() {
        let mut counts = [0_u32; 4];
        for draw in 0..100_u64 {
            let member = select_opponent_ladder_pool_member_v2(draw);
            counts[match member {
                OpponentLadderPoolMemberV2::Primary => 0,
                OpponentLadderPoolMemberV2::PredecessorA => 1,
                OpponentLadderPoolMemberV2::PredecessorB => 2,
                OpponentLadderPoolMemberV2::UniformFloor => 3,
            }] += 1;
        }
        assert_eq!(counts, [40, 20, 20, 20]);
        // Boundary draws land on the correct side of each threshold.
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
        // The modulo wraps: draws >=100 fold back onto the same partition.
        assert_eq!(
            select_opponent_ladder_pool_member_v2(139),
            OpponentLadderPoolMemberV2::Primary
        );
    }

    #[test]
    fn u63_domain_fails_closed() {
        let invalid = 1_u64 << 63;
        assert_eq!(
            derive_native_trainer_opponent_pool_choice_seed_v2(invalid, 0),
            Err(NativeTrainerScheduleV2ErrorV1::IntegerOutsideU63 {
                field: "base_seed",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_pool_choice_seed_v2(0, invalid),
            Err(NativeTrainerScheduleV2ErrorV1::IntegerOutsideU63 {
                field: "episode_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_policy_substep_seed_v2(invalid, 0, 0, 0),
            Err(NativeTrainerScheduleV2ErrorV1::IntegerOutsideU63 {
                field: "base_seed",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_policy_substep_seed_v2(0, invalid, 0, 0),
            Err(NativeTrainerScheduleV2ErrorV1::IntegerOutsideU63 {
                field: "episode_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_policy_substep_seed_v2(0, 0, invalid, 0),
            Err(NativeTrainerScheduleV2ErrorV1::IntegerOutsideU63 {
                field: "opponent_physical_decision_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_policy_substep_seed_v2(0, 0, 0, u32::MAX),
            Ok(derive_native_trainer_opponent_policy_substep_seed_v2(0, 0, 0, u32::MAX).unwrap())
        );
    }

    #[test]
    fn pool_choice_and_policy_substep_use_distinct_namespaces() {
        // Same (base_seed, episode_index) input must not collide across the
        // two namespaces (distinct namespace atoms feed the hash).
        for (base_seed, episode_index) in [(71_501_u64, 0_u64), (0, 0), (U63_MAX, U63_MAX)] {
            let pool_choice =
                derive_native_trainer_opponent_pool_choice_seed_v2(base_seed, episode_index)
                    .unwrap();
            let substep = derive_native_trainer_opponent_policy_substep_seed_v2(
                base_seed,
                episode_index,
                0,
                0,
            )
            .unwrap();
            assert_ne!(pool_choice, substep);
        }
    }

    // ------------------------------------------------------------------
    // Golden vectors (generate once, then pin). Independently reimplemented
    // stdlib-only in python/probes/ladder_schedule_v2/verify_ladder_schedule_v2.py.
    // ------------------------------------------------------------------

    const POOL_CHOICE_GOLDENS_V2: &[(u64, u64, u64)] =
        &include!("native_trainer_schedule_v2_pool_choice_goldens.in");
    const POLICY_SUBSTEP_GOLDENS_V2: &[(u64, u64, u64, u32, u64)] =
        &include!("native_trainer_schedule_v2_policy_substep_goldens.in");

    #[test]
    fn pool_choice_seed_matches_pinned_goldens() {
        assert!(POOL_CHOICE_GOLDENS_V2.len() >= 24);
        let episode_indices: std::collections::BTreeSet<u64> = POOL_CHOICE_GOLDENS_V2
            .iter()
            .map(|(_, episode_index, _)| *episode_index)
            .collect();
        for expected in [0, 1, 2, 63, 64, 65, 4095] {
            assert!(
                episode_indices.contains(&expected),
                "missing episode {expected}"
            );
        }
        for &(base_seed, episode_index, expected_seed) in POOL_CHOICE_GOLDENS_V2 {
            assert_eq!(
                derive_native_trainer_opponent_pool_choice_seed_v2(base_seed, episode_index)
                    .unwrap(),
                expected_seed,
                "base_seed={base_seed} episode_index={episode_index}"
            );
        }
    }

    #[test]
    fn policy_substep_seed_matches_pinned_goldens() {
        assert!(POLICY_SUBSTEP_GOLDENS_V2.len() >= 24);
        let episode_indices: std::collections::BTreeSet<u64> = POLICY_SUBSTEP_GOLDENS_V2
            .iter()
            .map(|(_, episode_index, _, _, _)| *episode_index)
            .collect();
        for expected in [0, 1, 2, 63, 64, 65, 4095] {
            assert!(
                episode_indices.contains(&expected),
                "missing episode {expected}"
            );
        }
        let decision_indices: std::collections::BTreeSet<u64> = POLICY_SUBSTEP_GOLDENS_V2
            .iter()
            .map(|(_, _, decision_index, _, _)| *decision_index)
            .collect();
        for expected in [0, 1, 7, 1023] {
            assert!(
                decision_indices.contains(&expected),
                "missing decision {expected}"
            );
        }
        let substeps: std::collections::BTreeSet<u32> = POLICY_SUBSTEP_GOLDENS_V2
            .iter()
            .map(|(_, _, _, substep_index, _)| *substep_index)
            .collect();
        for expected in [0, 1] {
            assert!(substeps.contains(&expected), "missing substep {expected}");
        }
        for &(base_seed, episode_index, decision_index, substep_index, expected_seed) in
            POLICY_SUBSTEP_GOLDENS_V2
        {
            assert_eq!(
                derive_native_trainer_opponent_policy_substep_seed_v2(
                    base_seed,
                    episode_index,
                    decision_index,
                    substep_index
                )
                .unwrap(),
                expected_seed,
                "base_seed={base_seed} episode_index={episode_index} decision_index={decision_index} substep_index={substep_index}"
            );
        }
    }
}
