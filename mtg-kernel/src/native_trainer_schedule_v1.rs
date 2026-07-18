//! Frozen seed and seat schedule for the future native trainer.
//!
//! This module intentionally does not alter the public scored-rollout schedule.
//! It reproduces the Python `terminal_reinforce_value/v3` trainer schedule for
//! cross-language trainer parity.

use crate::rl::PlayerSeatV1;
use sha2::{Digest, Sha256};

pub(crate) const NATIVE_TRAINER_SCHEDULE_VERSION_V1: &str =
    "mtg-kernel-native-trainer-schedule-sha256-v1";
pub(crate) const PYTHON_REFERENCE_SEED_VERSION_V1: &str = "kernel-python-rl-trainer-sha256-v2";
pub(crate) const NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1: &str =
    "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c";
pub(crate) const NATIVE_TRAINER_PYTHON_PARITY_CLAIM_V1: &str =
    "bit-exact Python terminal_reinforce_value/v3 seed and seat reproduction";
pub(crate) const NATIVE_TRAINER_SEAT_RULE_V1: &str =
    "absolute zero-based even episode -> P0 learner; odd episode -> P1 learner";
pub(crate) const NATIVE_TRAINER_ATOM_ALGORITHM_V1: &str =
    "sha256(u32be(tag_len)||tag||u64be(payload_len)||payload atoms; u63 payloads are 8-byte big-endian and str payloads are UTF-8)[:8]be & 0x7fff_ffff_ffff_ffff";
pub(crate) const NATIVE_TRAINER_REQUIRED_ATOM_TAGS_V1: &str =
    "version,namespace,field-name,u63,str";
pub(crate) const NATIVE_TRAINER_INTEGER_DOMAIN_V1: &str =
    "u63 values are integers in [0,2^63-1] encoded as exactly 8-byte big-endian";
pub(crate) const NATIVE_TRAINER_SUBSTEP_DOMAIN_V1: &str =
    "Rust accepts substep_index as u32; raw or wire inputs require checked u32 conversion rejecting values >=2^32, then encode in a u63 atom as 8-byte big-endian";
pub(crate) const NATIVE_TRAINER_PAIR_ENV_RULE_V1: &str =
    "pair_index=floor(episode_index/2); paired even/odd episodes share train-env(base_seed,pair_index)";
pub(crate) const NATIVE_TRAINER_ROLE_SEPARATION_RULE_V1: &str =
    "learner and opponent streams use distinct group and substep namespaces; physical seat is selected only by episode parity and is not a seed field";
pub(crate) const NATIVE_TRAINER_ACTOR_GROUP_ORDINAL_RULE_V1: &str =
    "normative future native-scheduler rule: learner and opponent ordinals each start at 0 per episode and advance once only after that actor completes a physical group";
pub(crate) const NATIVE_TRAINER_PARTIAL_GROUP_RULE_V1: &str =
    "normative future native-scheduler rule: preflight complete group before first substep; partial, halted, truncated, or failed group publishes no ordinal, update, checkpoint, or record";
pub(crate) const NATIVE_TRAINER_SCHEDULER_EXCLUSIONS_V1: &str =
    "worker,lane,session,chunk,round,batch,timing,device";
pub(crate) const NATIVE_TRAINER_VERSION_CHANGE_RULE_V1: &str =
    "any seed, seat, namespace, framing, domain, field-order, ordinal, partial-group, exclusion, or golden change requires a new schedule version announced on the CODEX-CLAUDE channel";
pub(crate) const NATIVE_TRAINER_RECORD_BINDING_RULE_V1: &str =
    "bind trainer_schedule_version,python_reference_seed_version,python_parity_claim,seat_rule,atom_algorithm,required_atom_tags,integer_domain,substep_domain,pair_env_rule,role_separation_rule,all namespace and ordered-field declarations,actor_group_ordinal_rule,partial_group_rule,scheduler_exclusions,version_change_rule,goldens_sha256";

const U63_MAX: u64 = (1_u64 << 63) - 1;
const SEED_MASK: u64 = U63_MAX;

const MODEL_INIT_NAMESPACE: &str = "model-init";
const TRAIN_ENV_NAMESPACE: &str = "train-env";
const LEARNER_GROUP_NAMESPACE: &str = "train-learner-action-group";
const LEARNER_SUBSTEP_NAMESPACE: &str = "train-learner-action-substep";
const OPPONENT_GROUP_NAMESPACE: &str = "train-opponent-action-group";
const OPPONENT_SUBSTEP_NAMESPACE: &str = "train-opponent-action-substep";
const MODEL_INIT_FIELDS: &str = "base_seed";
const TRAIN_ENV_FIELDS: &str = "base_seed,pair_index";
const LEARNER_GROUP_FIELDS: &str = "base_seed,episode_index,learner_physical_decision_index";
const LEARNER_SUBSTEP_FIELDS: &str = "group_seed,substep_index";
const OPPONENT_GROUP_FIELDS: &str = "base_seed,episode_index,opponent_physical_decision_index";
const OPPONENT_SUBSTEP_FIELDS: &str = "group_seed,substep_index";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeTrainerScheduleErrorV1 {
    IntegerOutsideU63 { field: &'static str, value: u64 },
    IntegerOutsideU32 { field: &'static str, value: u64 },
    AtomLengthOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTrainerEpisodeScheduleV1 {
    pub(crate) episode_index: u64,
    pub(crate) pair_index: u64,
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) environment_seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTrainerScheduleContractV1 {
    pub(crate) trainer_schedule_version: &'static str,
    pub(crate) python_reference_seed_version: &'static str,
    pub(crate) python_parity_claim: &'static str,
    pub(crate) seat_rule: &'static str,
    pub(crate) atom_algorithm: &'static str,
    pub(crate) required_atom_tags: &'static str,
    pub(crate) integer_domain: &'static str,
    pub(crate) substep_domain: &'static str,
    pub(crate) pair_env_rule: &'static str,
    pub(crate) role_separation_rule: &'static str,
    pub(crate) model_init_namespace: &'static str,
    pub(crate) model_init_fields: &'static str,
    pub(crate) environment_namespace: &'static str,
    pub(crate) environment_fields: &'static str,
    pub(crate) learner_group_namespace: &'static str,
    pub(crate) learner_group_fields: &'static str,
    pub(crate) learner_substep_namespace: &'static str,
    pub(crate) learner_substep_fields: &'static str,
    pub(crate) opponent_group_namespace: &'static str,
    pub(crate) opponent_group_fields: &'static str,
    pub(crate) opponent_substep_namespace: &'static str,
    pub(crate) opponent_substep_fields: &'static str,
    pub(crate) actor_group_ordinal_rule: &'static str,
    pub(crate) partial_group_rule: &'static str,
    pub(crate) scheduler_exclusions: &'static str,
    pub(crate) version_change_rule: &'static str,
    pub(crate) record_binding_rule: &'static str,
    pub(crate) goldens_sha256: &'static str,
}

pub(crate) const NATIVE_TRAINER_SCHEDULE_CONTRACT_V1: NativeTrainerScheduleContractV1 =
    NativeTrainerScheduleContractV1 {
        trainer_schedule_version: NATIVE_TRAINER_SCHEDULE_VERSION_V1,
        python_reference_seed_version: PYTHON_REFERENCE_SEED_VERSION_V1,
        python_parity_claim: NATIVE_TRAINER_PYTHON_PARITY_CLAIM_V1,
        seat_rule: NATIVE_TRAINER_SEAT_RULE_V1,
        atom_algorithm: NATIVE_TRAINER_ATOM_ALGORITHM_V1,
        required_atom_tags: NATIVE_TRAINER_REQUIRED_ATOM_TAGS_V1,
        integer_domain: NATIVE_TRAINER_INTEGER_DOMAIN_V1,
        substep_domain: NATIVE_TRAINER_SUBSTEP_DOMAIN_V1,
        pair_env_rule: NATIVE_TRAINER_PAIR_ENV_RULE_V1,
        role_separation_rule: NATIVE_TRAINER_ROLE_SEPARATION_RULE_V1,
        model_init_namespace: MODEL_INIT_NAMESPACE,
        model_init_fields: MODEL_INIT_FIELDS,
        environment_namespace: TRAIN_ENV_NAMESPACE,
        environment_fields: TRAIN_ENV_FIELDS,
        learner_group_namespace: LEARNER_GROUP_NAMESPACE,
        learner_group_fields: LEARNER_GROUP_FIELDS,
        learner_substep_namespace: LEARNER_SUBSTEP_NAMESPACE,
        learner_substep_fields: LEARNER_SUBSTEP_FIELDS,
        opponent_group_namespace: OPPONENT_GROUP_NAMESPACE,
        opponent_group_fields: OPPONENT_GROUP_FIELDS,
        opponent_substep_namespace: OPPONENT_SUBSTEP_NAMESPACE,
        opponent_substep_fields: OPPONENT_SUBSTEP_FIELDS,
        actor_group_ordinal_rule: NATIVE_TRAINER_ACTOR_GROUP_ORDINAL_RULE_V1,
        partial_group_rule: NATIVE_TRAINER_PARTIAL_GROUP_RULE_V1,
        scheduler_exclusions: NATIVE_TRAINER_SCHEDULER_EXCLUSIONS_V1,
        version_change_rule: NATIVE_TRAINER_VERSION_CHANGE_RULE_V1,
        record_binding_rule: NATIVE_TRAINER_RECORD_BINDING_RULE_V1,
        goldens_sha256: NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1,
    };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTrainerSeedFieldValueV1<'a> {
    U63(u64),
    Str(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeTrainerSeedFieldV1<'a> {
    name: &'static str,
    value: NativeTrainerSeedFieldValueV1<'a>,
}

const fn u63_field(name: &'static str, value: u64) -> NativeTrainerSeedFieldV1<'static> {
    NativeTrainerSeedFieldV1 {
        name,
        value: NativeTrainerSeedFieldValueV1::U63(value),
    }
}

const fn str_field<'a>(name: &'static str, value: &'a str) -> NativeTrainerSeedFieldV1<'a> {
    NativeTrainerSeedFieldV1 {
        name,
        value: NativeTrainerSeedFieldValueV1::Str(value),
    }
}

fn checked_u63(field: &'static str, value: u64) -> Result<u64, NativeTrainerScheduleErrorV1> {
    if value > U63_MAX {
        return Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 { field, value });
    }
    Ok(value)
}

pub(crate) fn checked_native_trainer_substep_index_v1(
    value: u64,
) -> Result<u32, NativeTrainerScheduleErrorV1> {
    u32::try_from(value).map_err(|_| NativeTrainerScheduleErrorV1::IntegerOutsideU32 {
        field: "substep_index",
        value,
    })
}

fn append_atom(
    hasher: &mut Sha256,
    tag: &str,
    payload: &[u8],
) -> Result<(), NativeTrainerScheduleErrorV1> {
    let tag_len =
        u32::try_from(tag.len()).map_err(|_| NativeTrainerScheduleErrorV1::AtomLengthOverflow)?;
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| NativeTrainerScheduleErrorV1::AtomLengthOverflow)?;
    hasher.update(tag_len.to_be_bytes());
    hasher.update(tag.as_bytes());
    hasher.update(payload_len.to_be_bytes());
    hasher.update(payload);
    Ok(())
}

fn derive_seed(
    namespace: &'static str,
    fields: &[NativeTrainerSeedFieldV1<'_>],
) -> Result<u64, NativeTrainerScheduleErrorV1> {
    let mut hasher = Sha256::new();
    append_atom(
        &mut hasher,
        "version",
        PYTHON_REFERENCE_SEED_VERSION_V1.as_bytes(),
    )?;
    append_atom(&mut hasher, "namespace", namespace.as_bytes())?;
    for field in fields {
        append_atom(&mut hasher, "field-name", field.name.as_bytes())?;
        match field.value {
            NativeTrainerSeedFieldValueV1::U63(value) => {
                let value = checked_u63(field.name, value)?;
                append_atom(&mut hasher, "u63", &value.to_be_bytes())?;
            }
            NativeTrainerSeedFieldValueV1::Str(value) => {
                append_atom(&mut hasher, "str", value.as_bytes())?;
            }
        }
    }
    let digest = hasher.finalize();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    Ok(u64::from_be_bytes(prefix) & SEED_MASK)
}

pub(crate) fn native_trainer_episode_schedule_v1(
    base_seed: u64,
    episode_index: u64,
) -> Result<NativeTrainerEpisodeScheduleV1, NativeTrainerScheduleErrorV1> {
    checked_u63("base_seed", base_seed)?;
    let episode_index = checked_u63("episode_index", episode_index)?;
    let pair_index = episode_index / 2;
    let learner_seat = if episode_index % 2 == 0 {
        PlayerSeatV1::P0
    } else {
        PlayerSeatV1::P1
    };
    let environment_seed = derive_seed(
        TRAIN_ENV_NAMESPACE,
        &[
            u63_field("base_seed", base_seed),
            u63_field("pair_index", pair_index),
        ],
    )?;
    Ok(NativeTrainerEpisodeScheduleV1 {
        episode_index,
        pair_index,
        learner_seat,
        environment_seed,
    })
}

pub(crate) fn derive_native_trainer_model_init_seed_v1(
    base_seed: u64,
) -> Result<u64, NativeTrainerScheduleErrorV1> {
    derive_seed(MODEL_INIT_NAMESPACE, &[u63_field("base_seed", base_seed)])
}

pub(crate) fn derive_native_trainer_learner_group_seed_v1(
    base_seed: u64,
    episode_index: u64,
    learner_physical_decision_index: u64,
) -> Result<u64, NativeTrainerScheduleErrorV1> {
    derive_seed(
        LEARNER_GROUP_NAMESPACE,
        &[
            u63_field("base_seed", base_seed),
            u63_field("episode_index", episode_index),
            u63_field(
                "learner_physical_decision_index",
                learner_physical_decision_index,
            ),
        ],
    )
}

pub(crate) fn derive_native_trainer_learner_action_seed_v1(
    base_seed: u64,
    episode_index: u64,
    learner_physical_decision_index: u64,
    substep_index: u32,
) -> Result<u64, NativeTrainerScheduleErrorV1> {
    let group_seed = derive_native_trainer_learner_group_seed_v1(
        base_seed,
        episode_index,
        learner_physical_decision_index,
    )?;
    derive_seed(
        LEARNER_SUBSTEP_NAMESPACE,
        &[
            u63_field("group_seed", group_seed),
            u63_field("substep_index", u64::from(substep_index)),
        ],
    )
}

pub(crate) fn derive_native_trainer_opponent_group_seed_v1(
    base_seed: u64,
    episode_index: u64,
    opponent_physical_decision_index: u64,
) -> Result<u64, NativeTrainerScheduleErrorV1> {
    derive_seed(
        OPPONENT_GROUP_NAMESPACE,
        &[
            u63_field("base_seed", base_seed),
            u63_field("episode_index", episode_index),
            u63_field(
                "opponent_physical_decision_index",
                opponent_physical_decision_index,
            ),
        ],
    )
}

pub(crate) fn derive_native_trainer_opponent_action_seed_v1(
    base_seed: u64,
    episode_index: u64,
    opponent_physical_decision_index: u64,
    substep_index: u32,
) -> Result<u64, NativeTrainerScheduleErrorV1> {
    let group_seed = derive_native_trainer_opponent_group_seed_v1(
        base_seed,
        episode_index,
        opponent_physical_decision_index,
    )?;
    derive_seed(
        OPPONENT_SUBSTEP_NAMESPACE,
        &[
            u63_field("group_seed", group_seed),
            u63_field("substep_index", u64::from(substep_index)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    const GOLDENS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/native_trainer_schedule_v1_goldens.json"
    ));

    #[derive(Deserialize)]
    struct GoldenFile {
        schema: String,
        schedule_version: String,
        python_reference_seed_version: String,
        str_atom_probe: StrAtomProbe,
        vectors: Vec<GoldenVector>,
    }

    #[derive(Deserialize)]
    struct StrAtomProbe {
        namespace: String,
        field_name: String,
        field_value: String,
        seed: u64,
    }

    #[derive(Deserialize)]
    struct GoldenVector {
        base_seed: u64,
        episode_index: u64,
        learner_seat: String,
        pair_index: u64,
        model_init_seed: u64,
        environment_seed: u64,
        learner_physical_decision_index: u64,
        opponent_physical_decision_index: u64,
        substep_index: u32,
        learner_group_seed: u64,
        learner_action_seed: u64,
        opponent_group_seed: u64,
        opponent_action_seed: u64,
    }

    #[test]
    fn native_schedule_matches_python_cross_language_goldens() {
        let actual_sha256 = Sha256::digest(GOLDENS.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual_sha256, NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1);
        let golden: GoldenFile = serde_json::from_str(GOLDENS).unwrap();
        assert_eq!(
            golden.schema,
            "mtg_kernel_native_trainer_schedule_goldens/v1"
        );
        assert_eq!(golden.schedule_version, NATIVE_TRAINER_SCHEDULE_VERSION_V1);
        assert_eq!(
            golden.python_reference_seed_version,
            PYTHON_REFERENCE_SEED_VERSION_V1
        );
        assert_eq!(
            golden.str_atom_probe.namespace,
            "native-trainer-str-atom-probe"
        );
        assert_eq!(golden.str_atom_probe.field_name, "role");
        assert_eq!(golden.str_atom_probe.field_value, "learner");
        assert_eq!(
            derive_seed(
                "native-trainer-str-atom-probe",
                &[str_field("role", "learner")],
            )
            .unwrap(),
            golden.str_atom_probe.seed
        );
        assert!(golden
            .vectors
            .iter()
            .any(|vector| vector.episode_index == (1_u64 << 62)));
        let promised_base_seeds = golden
            .vectors
            .iter()
            .map(|vector| vector.base_seed)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(promised_base_seeds
            .is_superset(&std::collections::BTreeSet::from([0, 1, 71_501, U63_MAX,])));
        let promised_episodes = golden
            .vectors
            .iter()
            .map(|vector| vector.episode_index)
            .collect::<std::collections::BTreeSet<_>>();
        assert!((0..=3).all(|episode| promised_episodes.contains(&episode)));
        assert!(golden.vectors.iter().any(|vector| {
            vector.learner_physical_decision_index == 0
                && vector.opponent_physical_decision_index == 0
        }));
        assert!(golden.vectors.iter().any(|vector| {
            vector.learner_physical_decision_index == 1
                && vector.opponent_physical_decision_index != 1
        }));
        assert!(golden
            .vectors
            .iter()
            .any(|vector| vector.substep_index == 0));
        assert!(golden
            .vectors
            .iter()
            .any(|vector| vector.substep_index == 1));
        assert!(golden.vectors.iter().any(|vector| {
            vector.base_seed == U63_MAX
                && vector.episode_index == U63_MAX
                && vector.learner_physical_decision_index == U63_MAX
                && vector.opponent_physical_decision_index == U63_MAX
                && vector.substep_index == u32::MAX
        }));
        for vector in golden.vectors {
            let episode =
                native_trainer_episode_schedule_v1(vector.base_seed, vector.episode_index).unwrap();
            assert_eq!(episode.episode_index, vector.episode_index);
            assert_eq!(episode.pair_index, vector.pair_index);
            assert_eq!(
                episode.learner_seat,
                match vector.learner_seat.as_str() {
                    "p0" => PlayerSeatV1::P0,
                    "p1" => PlayerSeatV1::P1,
                    invalid => panic!("unknown learner seat in golden vector: {invalid}"),
                }
            );
            assert_eq!(episode.environment_seed, vector.environment_seed);
            assert_eq!(
                derive_native_trainer_model_init_seed_v1(vector.base_seed).unwrap(),
                vector.model_init_seed
            );
            assert_eq!(
                derive_native_trainer_learner_group_seed_v1(
                    vector.base_seed,
                    vector.episode_index,
                    vector.learner_physical_decision_index,
                )
                .unwrap(),
                vector.learner_group_seed
            );
            assert_eq!(
                derive_native_trainer_learner_action_seed_v1(
                    vector.base_seed,
                    vector.episode_index,
                    vector.learner_physical_decision_index,
                    vector.substep_index,
                )
                .unwrap(),
                vector.learner_action_seed
            );
            assert_eq!(
                derive_native_trainer_opponent_group_seed_v1(
                    vector.base_seed,
                    vector.episode_index,
                    vector.opponent_physical_decision_index,
                )
                .unwrap(),
                vector.opponent_group_seed
            );
            assert_eq!(
                derive_native_trainer_opponent_action_seed_v1(
                    vector.base_seed,
                    vector.episode_index,
                    vector.opponent_physical_decision_index,
                    vector.substep_index,
                )
                .unwrap(),
                vector.opponent_action_seed
            );
            if vector.learner_physical_decision_index == vector.opponent_physical_decision_index {
                assert_ne!(vector.learner_group_seed, vector.opponent_group_seed);
                assert_ne!(vector.learner_action_seed, vector.opponent_action_seed);
            }
        }
    }

    #[test]
    fn schedule_contract_is_complete_and_scheduler_free() {
        let contract = NATIVE_TRAINER_SCHEDULE_CONTRACT_V1;
        assert_eq!(
            contract.trainer_schedule_version,
            NATIVE_TRAINER_SCHEDULE_VERSION_V1
        );
        assert_eq!(
            contract.python_reference_seed_version,
            PYTHON_REFERENCE_SEED_VERSION_V1
        );
        assert_eq!(
            contract.python_parity_claim,
            NATIVE_TRAINER_PYTHON_PARITY_CLAIM_V1
        );
        assert_eq!(contract.seat_rule, NATIVE_TRAINER_SEAT_RULE_V1);
        assert_eq!(contract.atom_algorithm, NATIVE_TRAINER_ATOM_ALGORITHM_V1);
        assert_eq!(
            contract.required_atom_tags,
            NATIVE_TRAINER_REQUIRED_ATOM_TAGS_V1
        );
        assert_eq!(contract.integer_domain, NATIVE_TRAINER_INTEGER_DOMAIN_V1);
        assert_eq!(contract.substep_domain, NATIVE_TRAINER_SUBSTEP_DOMAIN_V1);
        assert_eq!(contract.pair_env_rule, NATIVE_TRAINER_PAIR_ENV_RULE_V1);
        assert_eq!(
            contract.role_separation_rule,
            NATIVE_TRAINER_ROLE_SEPARATION_RULE_V1
        );
        assert_eq!(contract.model_init_namespace, MODEL_INIT_NAMESPACE);
        assert_eq!(contract.model_init_fields, MODEL_INIT_FIELDS);
        assert_eq!(contract.environment_namespace, TRAIN_ENV_NAMESPACE);
        assert_eq!(contract.environment_fields, TRAIN_ENV_FIELDS);
        assert_eq!(contract.learner_group_namespace, LEARNER_GROUP_NAMESPACE);
        assert_eq!(contract.learner_group_fields, LEARNER_GROUP_FIELDS);
        assert_eq!(
            contract.learner_substep_namespace,
            LEARNER_SUBSTEP_NAMESPACE
        );
        assert_eq!(contract.learner_substep_fields, LEARNER_SUBSTEP_FIELDS);
        assert_eq!(contract.opponent_group_namespace, OPPONENT_GROUP_NAMESPACE);
        assert_eq!(contract.opponent_group_fields, OPPONENT_GROUP_FIELDS);
        assert_eq!(
            contract.opponent_substep_namespace,
            OPPONENT_SUBSTEP_NAMESPACE
        );
        assert_eq!(contract.opponent_substep_fields, OPPONENT_SUBSTEP_FIELDS);
        assert_eq!(
            contract.actor_group_ordinal_rule,
            NATIVE_TRAINER_ACTOR_GROUP_ORDINAL_RULE_V1
        );
        assert_eq!(
            contract.partial_group_rule,
            NATIVE_TRAINER_PARTIAL_GROUP_RULE_V1
        );
        assert_eq!(
            contract.scheduler_exclusions,
            NATIVE_TRAINER_SCHEDULER_EXCLUSIONS_V1
        );
        assert_eq!(
            contract.version_change_rule,
            NATIVE_TRAINER_VERSION_CHANGE_RULE_V1
        );
        assert_eq!(
            contract.record_binding_rule,
            NATIVE_TRAINER_RECORD_BINDING_RULE_V1
        );
        assert_eq!(
            contract.goldens_sha256,
            NATIVE_TRAINER_SCHEDULE_GOLDENS_SHA256_V1
        );
        for forbidden in [
            "worker", "lane", "session", "chunk", "round", "batch", "timing", "device",
        ] {
            assert!(!MODEL_INIT_NAMESPACE.contains(forbidden));
            assert!(!TRAIN_ENV_NAMESPACE.contains(forbidden));
            assert!(!LEARNER_GROUP_NAMESPACE.contains(forbidden));
            assert!(!LEARNER_SUBSTEP_NAMESPACE.contains(forbidden));
            assert!(!OPPONENT_GROUP_NAMESPACE.contains(forbidden));
            assert!(!OPPONENT_SUBSTEP_NAMESPACE.contains(forbidden));
        }
    }

    #[test]
    fn paired_seats_share_environment_seed_and_width_does_not_shift_later_group() {
        for pair in 0..8_u64 {
            let even = native_trainer_episode_schedule_v1(71_501, pair * 2).unwrap();
            let odd = native_trainer_episode_schedule_v1(71_501, pair * 2 + 1).unwrap();
            assert_eq!(even.learner_seat, PlayerSeatV1::P0);
            assert_eq!(odd.learner_seat, PlayerSeatV1::P1);
            assert_eq!(even.pair_index, pair);
            assert_eq!(odd.pair_index, pair);
            assert_eq!(even.environment_seed, odd.environment_seed);
        }
        let later_learner_group =
            derive_native_trainer_learner_group_seed_v1(71_501, 4, 7).unwrap();
        let later_opponent_group =
            derive_native_trainer_opponent_group_seed_v1(71_501, 4, 7).unwrap();
        for prior_width in [1_u32, 2, 3, 17, u32::MAX] {
            let _prior_learner_last =
                derive_native_trainer_learner_action_seed_v1(71_501, 4, 6, prior_width - 1)
                    .unwrap();
            let _prior_opponent_last =
                derive_native_trainer_opponent_action_seed_v1(71_501, 4, 6, prior_width - 1)
                    .unwrap();
            assert_eq!(
                derive_native_trainer_learner_group_seed_v1(71_501, 4, 7).unwrap(),
                later_learner_group
            );
            assert_eq!(
                derive_native_trainer_opponent_group_seed_v1(71_501, 4, 7).unwrap(),
                later_opponent_group
            );
        }
        assert_ne!(later_learner_group, later_opponent_group);
    }

    #[test]
    fn u63_domain_fails_closed() {
        let invalid = 1_u64 << 63;
        assert_eq!(
            derive_native_trainer_model_init_seed_v1(invalid),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "base_seed",
                value: invalid,
            })
        );
        assert_eq!(
            native_trainer_episode_schedule_v1(invalid, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "base_seed",
                value: invalid,
            })
        );
        assert_eq!(
            native_trainer_episode_schedule_v1(0, invalid),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "episode_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_learner_action_seed_v1(0, 0, invalid, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "learner_physical_decision_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_learner_action_seed_v1(invalid, 0, 0, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "base_seed",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_learner_action_seed_v1(0, invalid, 0, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "episode_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_action_seed_v1(0, 0, invalid, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "opponent_physical_decision_index",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_action_seed_v1(invalid, 0, 0, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "base_seed",
                value: invalid,
            })
        );
        assert_eq!(
            derive_native_trainer_opponent_action_seed_v1(0, invalid, 0, 0),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU63 {
                field: "episode_index",
                value: invalid,
            })
        );
        assert_eq!(
            checked_native_trainer_substep_index_v1(u64::from(u32::MAX)),
            Ok(u32::MAX)
        );
        let invalid_substep = u64::from(u32::MAX) + 1;
        assert_eq!(
            checked_native_trainer_substep_index_v1(invalid_substep),
            Err(NativeTrainerScheduleErrorV1::IntegerOutsideU32 {
                field: "substep_index",
                value: invalid_substep,
            })
        );
    }
}
