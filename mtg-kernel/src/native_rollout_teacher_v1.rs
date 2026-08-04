//! Bounded perfect-information rollout-teacher ceiling diagnostic.
//!
//! This module asks whether full-terminal counterfactual continuations can
//! identify useful policy corrections at simple Rally decisions. A session
//! snapshot contains hidden hands and future library order, so this first
//! probe is intentionally not a training-data producer. A pass only supports
//! implementing acting-player information-set redeterminization next.

use crate::async_flat_scored_rollout_v1::FlatScoredFamilyCore;
use crate::async_flat_scored_rollout_v2::{FlatScoredFamilyV2, OwnedFlatScoringDecisionV2};
use crate::fast_sampler::{
    FastCategoricalScratch, FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    FAST_CATEGORICAL_SAMPLER_VERSION,
};
use crate::flat_policy_v2::FlatDecisionBindingV2;
use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::native_xmage_cp7_outcome_reinforce_v1::{
    load_xmage_cp7_outcome_inference_v1, NativeXmageCp7OutcomeInferenceV1,
};
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionSnapshotV1,
    FastActorSessionV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::io;
use std::path::Path;
use std::time::Instant;

pub const NATIVE_ROLLOUT_TEACHER_SCHEMA_V1: &str = "mtg-kernel-native-rollout-teacher-ceiling/v1";
pub const NATIVE_ROLLOUT_TEACHER_ENVELOPE_SCHEMA_V1: &str =
    "mtg-kernel-native-rollout-teacher-ceiling-envelope/v1";

const ROOT_COUNT_V1: usize = 32;
const RANKING_ROLLOUTS_PER_ACTION_V1: usize = 4;
const RANKING_ROLLOUTS_PER_ACTION_RANK16_V1: usize = 16;
const CONFIRMATION_ROLLOUTS_PER_ACTION_V1: usize = 32;
const MAX_BRANCH_POLICY_STEPS_V1: usize = 512;
const MAX_SOURCE_EPISODES_V1: usize = 256;
const MAX_SOURCE_POLICY_STEPS_V1: usize = 2_048;
const SESSION_MAX_PHYSICAL_DECISIONS_V1: u64 = 1_024;
const SESSION_MAX_POLICY_STEPS_V1: u64 = 2_048;
const MIN_ROOT_PHYSICAL_DECISION_ID_V1: u64 = 10;
const MIN_ROOT_ACTIONS_V1: u32 = 2;
const MAX_ROOT_ACTIONS_V1: u32 = 8;
const ROOT_EPISODE_ID_BASE_V1: u64 = 1_470_000;
const PROBE_BASE_SEED_V1: u64 = 0x726f_6c6c_6f75_7431;
const MAIN_POLICY_DOMAIN_V1: u64 = 0x6d61_696e_706f_6c31;
const RANKING_POLICY_DOMAIN_V1: u64 = 0x7261_6e6b_706f_6c31;
const CONFIRM_POLICY_DOMAIN_V1: u64 = 0x636f_6e66_706f_6c31;
const RANK16_RANKING_POLICY_DOMAIN_V1: u64 = 0x7231_3672_706f_6c31;
const RANK16_CONFIRM_POLICY_DOMAIN_V1: u64 = 0x7231_3663_706f_6c31;
const RETAINED_MANIFEST_SHA256_V1: &str =
    "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb";
const RETAINED_PAYLOAD_SHA256_V1: &str =
    "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c";
const RETAINED_NATIVE_STATE_SHA256_V1: &str =
    "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8";
const RETAINED_MODEL_PARAMETER_SHA256_V1: &str =
    "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546";
const RETAINED_ADAM_STEP_V1: u64 = 1;

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherSourceV1 {
    pub outcome_manifest_sha256: String,
    pub outcome_payload_sha256: String,
    pub native_state_sha256: String,
    pub model_parameter_sha256: String,
    pub corpus_sha256: String,
    pub adam_step: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherConfigV1 {
    pub base_seed_u64_hex: String,
    pub root_count: usize,
    pub max_source_episodes: usize,
    pub roots_per_episode: usize,
    pub root_eligibility: &'static str,
    pub ranking_rollouts_per_action: usize,
    pub confirmation_rollouts_per_action: usize,
    pub max_branch_policy_steps_including_forced_root: usize,
    pub continuation_policy: &'static str,
    pub continuation_sampler_identity: &'static str,
    pub continuation_sampler_contract_sha256: &'static str,
    pub branch_randomness: &'static str,
    pub information_scope: &'static str,
    pub training_admissibility: &'static str,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NativeRolloutOutcomeCountsV1 {
    pub attempted: u64,
    pub natural: u64,
    pub non_natural_terminal: u64,
    pub horizon_exhausted: u64,
    pub failures: u64,
    pub natural_reward_sum: i64,
}

impl NativeRolloutOutcomeCountsV1 {
    fn observe_v1(&mut self, outcome: ContinuationOutcomeV1) {
        self.attempted += 1;
        match outcome {
            ContinuationOutcomeV1::Natural(reward) => {
                self.natural += 1;
                self.natural_reward_sum += i64::from(reward);
            }
            ContinuationOutcomeV1::NonNaturalTerminal => self.non_natural_terminal += 1,
            ContinuationOutcomeV1::HorizonExhausted => self.horizon_exhausted += 1,
            ContinuationOutcomeV1::Failure => self.failures += 1,
        }
    }

    fn add_v1(&mut self, other: &Self) {
        self.attempted += other.attempted;
        self.natural += other.natural;
        self.non_natural_terminal += other.non_natural_terminal;
        self.horizon_exhausted += other.horizon_exhausted;
        self.failures += other.failures;
        self.natural_reward_sum += other.natural_reward_sum;
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherActionRankingV1 {
    pub action_index: u32,
    pub parent_logit_f32_bits: u32,
    pub outcomes: NativeRolloutOutcomeCountsV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherConfirmationV1 {
    pub teacher_outcomes: NativeRolloutOutcomeCountsV1,
    pub parent_argmax_outcomes: NativeRolloutOutcomeCountsV1,
    pub paired_teacher_better: u32,
    pub paired_parent_better: u32,
    pub paired_equal: u32,
    pub paired_incomplete: u32,
    pub paired_complete: u32,
    pub same_action_pair_mismatches: u32,
    pub teacher_minus_parent_reward_sum: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherRootV1 {
    pub root_ordinal: usize,
    pub source_episode_ordinal: usize,
    pub episode_id: u64,
    pub environment_seed_u64_hex: String,
    pub step: u64,
    pub physical_decision_id: u64,
    pub acting_player: PlayerSeatV1,
    pub legal_action_count: u32,
    pub privileged_state_hash_u64_hex: String,
    pub action_semantics: Vec<ActionSemanticV1>,
    pub parent_logits_f32_bits: Vec<u32>,
    pub parent_argmax_index: u32,
    pub teacher_index: u32,
    pub teacher_differs_from_parent: bool,
    pub ranking: Vec<NativeRolloutTeacherActionRankingV1>,
    pub confirmation: NativeRolloutTeacherConfirmationV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherAggregateV1 {
    pub source_episodes_examined: usize,
    pub roots_collected: usize,
    pub all_outcomes: NativeRolloutOutcomeCountsV1,
    pub natural_completion_basis_points: u64,
    pub changed_roots: usize,
    pub positive_changed_roots: usize,
    pub negative_changed_roots: usize,
    pub zero_delta_changed_roots: usize,
    pub same_action_pair_mismatches: usize,
    pub incomplete_ranking_actions: usize,
    pub incomplete_confirmation_pairs: usize,
    pub confirmed_teacher_minus_parent_reward_sum: i64,
    pub confirmed_mean_root_reward_delta_basis_points: i64,
    pub confirmed_mean_root_reward_delta_numerator: i64,
    pub confirmed_mean_root_reward_delta_denominator: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherGatesV1 {
    pub collected_all_32_roots: bool,
    pub zero_branch_failures: bool,
    pub common_random_numbers_same_action_exact: bool,
    pub all_ranking_rollouts_natural: bool,
    pub all_confirmation_pairs_complete: bool,
    pub at_least_99_percent_natural: bool,
    pub teacher_changed_at_least_6_roots: bool,
    pub positive_changed_roots_outnumber_negative: bool,
    pub confirmed_mean_delta_at_least_0p05: bool,
    pub intrinsic_signal_pass: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherReportV1 {
    pub schema: &'static str,
    pub publication_encoding: &'static str,
    pub source: NativeRolloutTeacherSourceV1,
    pub config: NativeRolloutTeacherConfigV1,
    pub roots: Vec<NativeRolloutTeacherRootV1>,
    pub aggregate: NativeRolloutTeacherAggregateV1,
    pub gates: NativeRolloutTeacherGatesV1,
    pub interpretation: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherEnvelopeV1 {
    pub schema: &'static str,
    pub deterministic_report_sha256: String,
    pub elapsed_milliseconds: u64,
    pub runtime_under_ten_minutes: bool,
    pub disposition: &'static str,
    pub report: NativeRolloutTeacherReportV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContinuationOutcomeV1 {
    Natural(i32),
    NonNaturalTerminal,
    HorizonExhausted,
    Failure,
}

struct ScoredCurrentDecisionV1 {
    expected: FastActorDecisionV1,
    binding: FlatDecisionBindingV2,
    logits: Vec<f32>,
    value: f32,
}

fn invalid_data_v1(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn splitmix64_first_v1(seed: u64) -> u64 {
    let mut value = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn environment_seed_v1(episode_ordinal: usize) -> u64 {
    splitmix64_first_v1(
        PROBE_BASE_SEED_V1 ^ (episode_ordinal as u64).wrapping_mul(0xd6e8_feb8_6659_fd93),
    )
}

fn main_policy_seed_v1(episode_ordinal: usize, policy_step: u64) -> u64 {
    splitmix64_first_v1(
        PROBE_BASE_SEED_V1
            ^ MAIN_POLICY_DOMAIN_V1
            ^ (episode_ordinal as u64).wrapping_mul(0xa076_1d64_78bd_642f)
            ^ policy_step.wrapping_mul(0xe703_7ed1_a0b4_28db),
    )
}

fn selective_search_environment_seed_v1(episode_ordinal: usize) -> u64 {
    splitmix64_first_v1(
        SELECTIVE_SEARCH_PROBE_BASE_SEED_V1
            ^ (episode_ordinal as u64).wrapping_mul(0xd6e8_feb8_6659_fd93),
    )
}

fn selective_search_main_policy_seed_v1(episode_ordinal: usize, policy_step: u64) -> u64 {
    splitmix64_first_v1(
        SELECTIVE_SEARCH_PROBE_BASE_SEED_V1
            ^ SELECTIVE_SEARCH_MAIN_POLICY_DOMAIN_V1
            ^ (episode_ordinal as u64).wrapping_mul(0xa076_1d64_78bd_642f)
            ^ policy_step.wrapping_mul(0xe703_7ed1_a0b4_28db),
    )
}

fn continuation_policy_seed_v1(
    domain: u64,
    root_ordinal: usize,
    rollout_ordinal: usize,
    actor: PlayerSeatV1,
    actor_policy_ordinal: u64,
) -> u64 {
    let actor_tag = match actor {
        PlayerSeatV1::P0 => 0x5030_5030_5030_5030,
        PlayerSeatV1::P1 => 0x5031_5031_5031_5031,
    };
    splitmix64_first_v1(
        PROBE_BASE_SEED_V1
            ^ domain
            ^ (root_ordinal as u64).wrapping_mul(0x8ebc_6af0_9c88_c6e3)
            ^ (rollout_ordinal as u64).wrapping_mul(0x5899_65cc_7537_4cc3)
            ^ actor_tag
            ^ actor_policy_ordinal.wrapping_mul(0x1d8e_4e27_c47d_124f),
    )
}

fn player_index_v1(player: PlayerSeatV1) -> usize {
    match player {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn score_current_decision_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
) -> Result<ScoredCurrentDecisionV1, ()> {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        return Err(());
    };
    let packet = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::encode_packet(
        session,
        expected,
        &mut Default::default(),
        OwnedFlatScoringDecisionV2::default(),
    )?;
    if !<FlatScoredFamilyV2 as FlatScoredFamilyCore>::expected_matches_binding(
        expected,
        <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_decision(&packet),
    ) {
        return Err(());
    }
    let binding = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_binding(&packet);
    let output = inference
        .score_decision_v1(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_view(
            &packet,
        ))
        .map_err(|_| ())?;
    let logits = output.logits_v1().to_vec();
    let value = output.value_v1();
    if logits.len() != expected.legal_action_count as usize
        || logits.is_empty()
        || logits.iter().any(|value| !value.is_finite())
        || !value.is_finite()
    {
        return Err(());
    }
    drop(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::into_owned_packet(packet));
    Ok(ScoredCurrentDecisionV1 {
        expected,
        binding,
        logits,
        value,
    })
}

fn consume_scored_v1(
    session: &mut FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    selected_index: u32,
) -> Result<FastActorResponseV1, ()> {
    if selected_index >= scored.expected.legal_action_count {
        return Err(());
    }
    <FlatScoredFamilyV2 as FlatScoredFamilyCore>::consume(session, scored.binding, selected_index)
}

fn eligible_root_v1(decision: FastActorDecisionV1) -> bool {
    decision.decision_kind == FastActorDecisionKindV1::Surface
        && decision.substep_index == 0
        && decision.substep_count == 1
        && decision.physical_decision_id >= MIN_ROOT_PHYSICAL_DECISION_ID_V1
        && (MIN_ROOT_ACTIONS_V1..=MAX_ROOT_ACTIONS_V1).contains(&decision.legal_action_count)
}

fn eligible_selective_search_root_v1(decision: FastActorDecisionV1) -> bool {
    decision.decision_kind == FastActorDecisionKindV1::Surface
        && decision.substep_index == 0
        && decision.substep_count == 1
        && decision.physical_decision_id >= SELECTIVE_SEARCH_MIN_PHYSICAL_DECISION_ID_V1
        && (MIN_ROOT_ACTIONS_V1..=MAX_ROOT_ACTIONS_V1).contains(&decision.legal_action_count)
}

fn argmax_v1(logits: &[f32]) -> u32 {
    let mut best = 0usize;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[best]).is_gt() {
            best = index;
        }
    }
    best as u32
}

fn sample_policy_v1(logits: &[f32], seed: u64) -> Result<u32, ()> {
    FastCategoricalScratch::default()
        .sample(logits, seed)
        .map_err(|_| ())
        .and_then(|index| u32::try_from(index).map_err(|_| ()))
}

fn terminal_outcome_v1(
    response: FastActorResponseV1,
    root_actor: PlayerSeatV1,
) -> Option<ContinuationOutcomeV1> {
    let FastActorResponseV1::Terminal(terminal) = response else {
        return None;
    };
    if terminal.terminal_classification != TerminalClassificationV1::Natural {
        return Some(ContinuationOutcomeV1::NonNaturalTerminal);
    }
    Some(ContinuationOutcomeV1::Natural(
        terminal.terminal_reward[player_index_v1(root_actor)],
    ))
}

fn run_continuation_v1(
    root_session: &FastActorSessionV1,
    root_snapshot: &FastActorSessionSnapshotV1,
    expected_root_hash: u64,
    inference: &NativeXmageCp7OutcomeInferenceV1,
    root_actor: PlayerSeatV1,
    forced_root_index: u32,
    domain: u64,
    root_ordinal: usize,
    rollout_ordinal: usize,
) -> ContinuationOutcomeV1 {
    let mut session = root_session.clone();
    session.restore_v1(root_snapshot);
    if session.privileged_core_environment_hash() != expected_root_hash {
        return ContinuationOutcomeV1::Failure;
    }
    let root_scored = match score_current_decision_v1(inference, &session) {
        Ok(scored) => scored,
        Err(()) => return ContinuationOutcomeV1::Failure,
    };
    let response = match consume_scored_v1(&mut session, root_scored, forced_root_index) {
        Ok(response) => response,
        Err(()) => return ContinuationOutcomeV1::Failure,
    };
    if let Some(outcome) = terminal_outcome_v1(response, root_actor) {
        return outcome;
    }

    let mut actor_policy_ordinals = [0_u64; 2];
    for _ in 1..MAX_BRANCH_POLICY_STEPS_V1 {
        let scored = match score_current_decision_v1(inference, &session) {
            Ok(scored) => scored,
            Err(()) => return ContinuationOutcomeV1::Failure,
        };
        let actor_index = player_index_v1(scored.expected.acting_player);
        let seed = continuation_policy_seed_v1(
            domain,
            root_ordinal,
            rollout_ordinal,
            scored.expected.acting_player,
            actor_policy_ordinals[actor_index],
        );
        actor_policy_ordinals[actor_index] += 1;
        let selected = match sample_policy_v1(&scored.logits, seed) {
            Ok(selected) => selected,
            Err(_) => return ContinuationOutcomeV1::Failure,
        };
        let response = match consume_scored_v1(&mut session, scored, selected) {
            Ok(response) => response,
            Err(()) => return ContinuationOutcomeV1::Failure,
        };
        if let Some(outcome) = terminal_outcome_v1(response, root_actor) {
            return outcome;
        }
    }
    ContinuationOutcomeV1::HorizonExhausted
}

fn teacher_index_v1(ranking: &[NativeRolloutTeacherActionRankingV1]) -> u32 {
    let mut best = 0usize;
    for index in 1..ranking.len() {
        let candidate = &ranking[index];
        let incumbent = &ranking[best];
        let better = candidate.outcomes.natural_reward_sum > incumbent.outcomes.natural_reward_sum
            || (candidate.outcomes.natural_reward_sum == incumbent.outcomes.natural_reward_sum
                && (f32::from_bits(candidate.parent_logit_f32_bits)
                    .total_cmp(&f32::from_bits(incumbent.parent_logit_f32_bits))
                    .is_gt()
                    || (candidate.parent_logit_f32_bits == incumbent.parent_logit_f32_bits
                        && candidate.action_index < incumbent.action_index)));
        if better {
            best = index;
        }
    }
    ranking[best].action_index
}

fn conservative_teacher_index_v1(
    ranking: &[NativeRolloutTeacherActionRankingV1],
    parent_index: u32,
    minimum_reward_margin: i64,
) -> u32 {
    let best_index = teacher_index_v1(ranking);
    if best_index == parent_index {
        return parent_index;
    }
    let best_reward = ranking
        .iter()
        .find(|row| row.action_index == best_index)
        .map(|row| row.outcomes.natural_reward_sum);
    let parent_reward = ranking
        .iter()
        .find(|row| row.action_index == parent_index)
        .map(|row| row.outcomes.natural_reward_sum);
    match (best_reward, parent_reward) {
        (Some(best), Some(parent)) if best - parent >= minimum_reward_margin => best_index,
        _ => parent_index,
    }
}

fn evaluate_root_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    root_ordinal: usize,
    source_episode_ordinal: usize,
    environment_seed: u64,
) -> Result<NativeRolloutTeacherRootV1, io::Error> {
    let expected = scored.expected;
    let root_actor = expected.acting_player;
    let root_hash = session.privileged_core_environment_hash();
    let snapshot = session.snapshot_v1();
    let semantics = session
        .diagnostic_current_action_semantics()
        .ok_or_else(|| invalid_data_v1("root action semantics missing"))?;
    if semantics.len() != scored.logits.len() {
        return Err(invalid_data_v1("root action semantic width mismatch"));
    }
    let parent_argmax = argmax_v1(&scored.logits);
    let parent_logits_f32_bits: Vec<u32> =
        scored.logits.iter().map(|value| value.to_bits()).collect();

    let mut ranking = Vec::with_capacity(scored.logits.len());
    for action_index in 0..scored.logits.len() {
        let mut outcomes = NativeRolloutOutcomeCountsV1::default();
        for rollout_ordinal in 0..RANKING_ROLLOUTS_PER_ACTION_V1 {
            outcomes.observe_v1(run_continuation_v1(
                session,
                &snapshot,
                root_hash,
                inference,
                root_actor,
                action_index as u32,
                RANKING_POLICY_DOMAIN_V1,
                root_ordinal,
                rollout_ordinal,
            ));
        }
        ranking.push(NativeRolloutTeacherActionRankingV1 {
            action_index: action_index as u32,
            parent_logit_f32_bits: parent_logits_f32_bits[action_index],
            outcomes,
        });
    }
    let teacher_index = teacher_index_v1(&ranking);

    let mut teacher_outcomes = NativeRolloutOutcomeCountsV1::default();
    let mut parent_outcomes = NativeRolloutOutcomeCountsV1::default();
    let mut paired_teacher_better = 0_u32;
    let mut paired_parent_better = 0_u32;
    let mut paired_equal = 0_u32;
    let mut paired_incomplete = 0_u32;
    let mut paired_complete = 0_u32;
    let mut same_action_pair_mismatches = 0_u32;
    let mut teacher_minus_parent_reward_sum = 0_i64;
    for rollout_ordinal in 0..CONFIRMATION_ROLLOUTS_PER_ACTION_V1 {
        let teacher = run_continuation_v1(
            session,
            &snapshot,
            root_hash,
            inference,
            root_actor,
            teacher_index,
            CONFIRM_POLICY_DOMAIN_V1,
            root_ordinal,
            rollout_ordinal,
        );
        let parent = run_continuation_v1(
            session,
            &snapshot,
            root_hash,
            inference,
            root_actor,
            parent_argmax,
            CONFIRM_POLICY_DOMAIN_V1,
            root_ordinal,
            rollout_ordinal,
        );
        teacher_outcomes.observe_v1(teacher);
        parent_outcomes.observe_v1(parent);
        if teacher_index == parent_argmax && teacher != parent {
            same_action_pair_mismatches += 1;
        }
        match (teacher, parent) {
            (ContinuationOutcomeV1::Natural(a), ContinuationOutcomeV1::Natural(b)) => {
                paired_complete += 1;
                teacher_minus_parent_reward_sum += i64::from(a - b);
                if a > b {
                    paired_teacher_better += 1;
                } else if a < b {
                    paired_parent_better += 1;
                } else {
                    paired_equal += 1;
                }
            }
            _ => paired_incomplete += 1,
        }
    }
    Ok(NativeRolloutTeacherRootV1 {
        root_ordinal,
        source_episode_ordinal,
        episode_id: expected.episode_id,
        environment_seed_u64_hex: format!("{environment_seed:016x}"),
        step: expected.step,
        physical_decision_id: expected.physical_decision_id,
        acting_player: expected.acting_player,
        legal_action_count: expected.legal_action_count,
        privileged_state_hash_u64_hex: format!("{root_hash:016x}"),
        action_semantics: semantics,
        parent_logits_f32_bits,
        parent_argmax_index: parent_argmax,
        teacher_index,
        teacher_differs_from_parent: teacher_index != parent_argmax,
        ranking,
        confirmation: NativeRolloutTeacherConfirmationV1 {
            teacher_outcomes,
            parent_argmax_outcomes: parent_outcomes,
            paired_teacher_better,
            paired_parent_better,
            paired_equal,
            paired_incomplete,
            paired_complete,
            same_action_pair_mismatches,
            teacher_minus_parent_reward_sum,
        },
    })
}

fn aggregate_with_ranking_samples_v1(
    roots: &[NativeRolloutTeacherRootV1],
    source_episodes_examined: usize,
    ranking_rollouts_per_action: usize,
) -> (NativeRolloutTeacherAggregateV1, NativeRolloutTeacherGatesV1) {
    let mut all_outcomes = NativeRolloutOutcomeCountsV1::default();
    let mut changed_roots = 0usize;
    let mut positive_changed_roots = 0usize;
    let mut negative_changed_roots = 0usize;
    let mut zero_delta_changed_roots = 0usize;
    let mut same_action_pair_mismatches = 0usize;
    let mut incomplete_ranking_actions = 0usize;
    let mut incomplete_confirmation_pairs = 0usize;
    let mut confirmed_delta_sum = 0i64;
    let mut completed_confirmation_pairs = 0u64;
    for root in roots {
        for action in &root.ranking {
            all_outcomes.add_v1(&action.outcomes);
            if action.outcomes.natural != ranking_rollouts_per_action as u64 {
                incomplete_ranking_actions += 1;
            }
        }
        all_outcomes.add_v1(&root.confirmation.teacher_outcomes);
        all_outcomes.add_v1(&root.confirmation.parent_argmax_outcomes);
        incomplete_confirmation_pairs += root.confirmation.paired_incomplete as usize;
        completed_confirmation_pairs += u64::from(root.confirmation.paired_complete);
        same_action_pair_mismatches += root.confirmation.same_action_pair_mismatches as usize;
        let delta = root.confirmation.teacher_minus_parent_reward_sum;
        confirmed_delta_sum += delta;
        if root.teacher_differs_from_parent {
            changed_roots += 1;
            if delta > 0 {
                positive_changed_roots += 1;
            } else if delta < 0 {
                negative_changed_roots += 1;
            } else {
                zero_delta_changed_roots += 1;
            }
        }
    }
    let natural_completion_basis_points = if all_outcomes.attempted == 0 {
        0
    } else {
        all_outcomes.natural.saturating_mul(10_000) / all_outcomes.attempted
    };
    let delta_denominator = completed_confirmation_pairs;
    let mean_delta_basis_points = if delta_denominator == 0 {
        0
    } else {
        confirmed_delta_sum.saturating_mul(10_000) / delta_denominator as i64
    };
    let collected_all_32_roots = roots.len() == ROOT_COUNT_V1;
    let zero_branch_failures = all_outcomes.failures == 0;
    let common_random_numbers_same_action_exact = same_action_pair_mismatches == 0;
    let all_ranking_rollouts_natural = incomplete_ranking_actions == 0;
    let all_confirmation_pairs_complete = incomplete_confirmation_pairs == 0
        && completed_confirmation_pairs
            == (roots.len() * CONFIRMATION_ROLLOUTS_PER_ACTION_V1) as u64;
    let at_least_99_percent_natural = natural_completion_basis_points >= 9_900;
    let teacher_changed_at_least_6_roots = changed_roots >= 6;
    let positive_changed_roots_outnumber_negative = positive_changed_roots > negative_changed_roots;
    let confirmed_mean_delta_at_least_0p05 =
        delta_denominator > 0 && confirmed_delta_sum.saturating_mul(20) >= delta_denominator as i64;
    let intrinsic_signal_pass = collected_all_32_roots
        && zero_branch_failures
        && common_random_numbers_same_action_exact
        && all_ranking_rollouts_natural
        && all_confirmation_pairs_complete
        && at_least_99_percent_natural
        && teacher_changed_at_least_6_roots
        && positive_changed_roots_outnumber_negative
        && confirmed_mean_delta_at_least_0p05;
    (
        NativeRolloutTeacherAggregateV1 {
            source_episodes_examined,
            roots_collected: roots.len(),
            all_outcomes,
            natural_completion_basis_points,
            changed_roots,
            positive_changed_roots,
            negative_changed_roots,
            zero_delta_changed_roots,
            same_action_pair_mismatches,
            incomplete_ranking_actions,
            incomplete_confirmation_pairs,
            confirmed_teacher_minus_parent_reward_sum: confirmed_delta_sum,
            confirmed_mean_root_reward_delta_basis_points: mean_delta_basis_points,
            confirmed_mean_root_reward_delta_numerator: confirmed_delta_sum,
            confirmed_mean_root_reward_delta_denominator: delta_denominator,
        },
        NativeRolloutTeacherGatesV1 {
            collected_all_32_roots,
            zero_branch_failures,
            common_random_numbers_same_action_exact,
            all_ranking_rollouts_natural,
            all_confirmation_pairs_complete,
            at_least_99_percent_natural,
            teacher_changed_at_least_6_roots,
            positive_changed_roots_outnumber_negative,
            confirmed_mean_delta_at_least_0p05,
            intrinsic_signal_pass,
        },
    )
}

fn aggregate_v1(
    roots: &[NativeRolloutTeacherRootV1],
    source_episodes_examined: usize,
) -> (NativeRolloutTeacherAggregateV1, NativeRolloutTeacherGatesV1) {
    aggregate_with_ranking_samples_v1(
        roots,
        source_episodes_examined,
        RANKING_ROLLOUTS_PER_ACTION_V1,
    )
}

/// Exact deterministic bytes written by the CLI and covered by its reported
/// SHA-256. Runtime timing is deliberately absent.
pub fn native_rollout_teacher_report_bytes_v1(
    report: &NativeRolloutTeacherReportV1,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn report_sha256_v1(report: &NativeRolloutTeacherReportV1) -> Result<String, serde_json::Error> {
    let bytes = native_rollout_teacher_report_bytes_v1(report)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Runs the fixed, sequential ceiling probe from one verified retained outcome
/// derivative. Timing is kept outside the hashed deterministic report.
pub fn run_native_rollout_teacher_v1(
    source_outcome_root: impl AsRef<Path>,
) -> Result<NativeRolloutTeacherEnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let inference = load_xmage_cp7_outcome_inference_v1(source_outcome_root.as_ref())?;
    if lower_hex_raw32_v1(inference.manifest_sha256_v1()) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_raw32_v1(inference.payload_sha256_v1()) != RETAINED_PAYLOAD_SHA256_V1
        || lower_hex_raw32_v1(inference.native_state_sha256_v1()) != RETAINED_NATIVE_STATE_SHA256_V1
        || lower_hex_raw32_v1(inference.model_parameter_sha256_v1())
            != RETAINED_MODEL_PARAMETER_SHA256_V1
        || inference.adam_step_v1() != RETAINED_ADAM_STEP_V1
    {
        return Err(invalid_data_v1("source is not the exact retained 706b checkpoint").into());
    }
    let source = NativeRolloutTeacherSourceV1 {
        outcome_manifest_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
        outcome_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
        native_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
        corpus_sha256: lower_hex_raw32_v1(inference.corpus_sha256_v1()),
        adam_step: inference.adam_step_v1(),
    };
    let config = NativeRolloutTeacherConfigV1 {
        base_seed_u64_hex: format!("{PROBE_BASE_SEED_V1:016x}"),
        root_count: ROOT_COUNT_V1,
        max_source_episodes: MAX_SOURCE_EPISODES_V1,
        roots_per_episode: 1,
        root_eligibility:
            "surface, substep_count=1, physical_decision_id>=10, legal_action_count=2..8",
        ranking_rollouts_per_action: RANKING_ROLLOUTS_PER_ACTION_V1,
        confirmation_rollouts_per_action: CONFIRMATION_ROLLOUTS_PER_ACTION_V1,
        max_branch_policy_steps_including_forced_root: MAX_BRANCH_POLICY_STEPS_V1,
        continuation_policy: "retained-checkpoint-temperature-1-self-play",
        continuation_sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        continuation_sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        branch_randomness: "paired-common-random-number-policy-sequences/v1",
        information_scope: "perfect-information-fixed-hidden-state-ceiling",
        training_admissibility: "not-admissible-without-information-set-redeterminization",
    };

    let mut roots = Vec::with_capacity(ROOT_COUNT_V1);
    let mut source_episodes_examined = 0usize;
    for episode_ordinal in 0..MAX_SOURCE_EPISODES_V1 {
        if roots.len() == ROOT_COUNT_V1 {
            break;
        }
        source_episodes_examined += 1;
        let episode_id = ROOT_EPISODE_ID_BASE_V1 + episode_ordinal as u64;
        let environment_seed = environment_seed_v1(episode_ordinal);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            SESSION_MAX_PHYSICAL_DECISIONS_V1,
            SESSION_MAX_POLICY_STEPS_V1,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .map_err(|_| invalid_data_v1("Rally source session reset failed"))?;

        for _ in 0..MAX_SOURCE_POLICY_STEPS_V1 {
            let FastActorResponseV1::Decision(_) = session.current_response() else {
                break;
            };
            let scored = score_current_decision_v1(&inference, &session)
                .map_err(|_| invalid_data_v1("source policy scoring failed"))?;
            if eligible_root_v1(scored.expected) {
                let root = evaluate_root_v1(
                    &inference,
                    &session,
                    scored,
                    roots.len(),
                    episode_ordinal,
                    environment_seed,
                )?;
                roots.push(root);
                break;
            }
            let selected = sample_policy_v1(
                &scored.logits,
                main_policy_seed_v1(episode_ordinal, scored.expected.step),
            )
            .map_err(|_| invalid_data_v1("source policy sampling failed"))?;
            consume_scored_v1(&mut session, scored, selected)
                .map_err(|_| invalid_data_v1("source policy consume failed"))?;
        }
    }

    let (aggregate, gates) = aggregate_v1(&roots, source_episodes_examined);
    let report = NativeRolloutTeacherReportV1 {
        schema: NATIVE_ROLLOUT_TEACHER_SCHEMA_V1,
        publication_encoding: "serde-json-pretty-utf8-trailing-lf/v1",
        source,
        config,
        roots,
        aggregate,
        gates,
        interpretation: "Perfect-information ceiling only. Never train from these labels. A reproducible pass under the runtime gate supports implementing information-set redeterminization.",
    };
    let deterministic_report_sha256 = report_sha256_v1(&report)?;
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let runtime_under_ten_minutes = elapsed_milliseconds < 600_000;
    let disposition = if report.gates.intrinsic_signal_pass && runtime_under_ten_minutes {
        "provisional-pass-requires-identical-rerun-before-redeterminization"
    } else {
        "reject-full-terminal-rollout-teacher-ceiling"
    };
    Ok(NativeRolloutTeacherEnvelopeV1 {
        schema: NATIVE_ROLLOUT_TEACHER_ENVELOPE_SCHEMA_V1,
        deterministic_report_sha256,
        elapsed_milliseconds,
        runtime_under_ten_minutes,
        disposition,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranking_v1(index: u32, reward: i64, logit: f32) -> NativeRolloutTeacherActionRankingV1 {
        NativeRolloutTeacherActionRankingV1 {
            action_index: index,
            parent_logit_f32_bits: logit.to_bits(),
            outcomes: NativeRolloutOutcomeCountsV1 {
                attempted: 4,
                natural: 4,
                natural_reward_sum: reward,
                ..Default::default()
            },
        }
    }

    #[test]
    fn teacher_prefers_reward_then_parent_logit_then_lower_index_v1() {
        assert_eq!(
            teacher_index_v1(&[
                ranking_v1(0, 1, 10.0),
                ranking_v1(1, 2, -10.0),
                ranking_v1(2, 2, 5.0),
            ]),
            2
        );
        assert_eq!(
            teacher_index_v1(&[ranking_v1(0, 2, 5.0), ranking_v1(1, 2, 5.0),]),
            0
        );
    }

    #[test]
    fn conservative_teacher_requires_declared_reward_margin_v1() {
        let below_margin = [ranking_v1(0, 0, 10.0), ranking_v1(1, 5, -10.0)];
        assert_eq!(conservative_teacher_index_v1(&below_margin, 0, 6), 0);

        let at_margin = [ranking_v1(0, 0, 10.0), ranking_v1(1, 6, -10.0)];
        assert_eq!(conservative_teacher_index_v1(&at_margin, 0, 6), 1);

        let parent_is_best = [ranking_v1(0, 7, 10.0), ranking_v1(1, 6, -10.0)];
        assert_eq!(conservative_teacher_index_v1(&parent_is_best, 0, 6), 0);
    }

    #[test]
    fn continuation_seed_is_action_independent_and_domain_separated_v1() {
        let a = continuation_policy_seed_v1(RANKING_POLICY_DOMAIN_V1, 3, 7, PlayerSeatV1::P1, 11);
        let b = continuation_policy_seed_v1(RANKING_POLICY_DOMAIN_V1, 3, 7, PlayerSeatV1::P1, 11);
        let c = continuation_policy_seed_v1(CONFIRM_POLICY_DOMAIN_V1, 3, 7, PlayerSeatV1::P1, 11);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    fn aggregate_root_v1(
        root_ordinal: usize,
        changed: bool,
        delta: i64,
        incomplete_pairs: u32,
    ) -> NativeRolloutTeacherRootV1 {
        let ranking = vec![ranking_v1(0, 0, 1.0), ranking_v1(1, 1, 0.0)];
        let complete = CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u32 - incomplete_pairs;
        let teacher_natural =
            CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64 - u64::from(incomplete_pairs);
        NativeRolloutTeacherRootV1 {
            root_ordinal,
            source_episode_ordinal: root_ordinal,
            episode_id: ROOT_EPISODE_ID_BASE_V1 + root_ordinal as u64,
            environment_seed_u64_hex: "0000000000000000".to_owned(),
            step: 10,
            physical_decision_id: 10,
            acting_player: PlayerSeatV1::P0,
            legal_action_count: 2,
            privileged_state_hash_u64_hex: "0000000000000000".to_owned(),
            action_semantics: Vec::new(),
            parent_logits_f32_bits: vec![1.0_f32.to_bits(), 0.0_f32.to_bits()],
            parent_argmax_index: 0,
            teacher_index: u32::from(changed),
            teacher_differs_from_parent: changed,
            ranking,
            confirmation: NativeRolloutTeacherConfirmationV1 {
                teacher_outcomes: NativeRolloutOutcomeCountsV1 {
                    attempted: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                    natural: teacher_natural,
                    non_natural_terminal: u64::from(incomplete_pairs),
                    natural_reward_sum: delta,
                    ..Default::default()
                },
                parent_argmax_outcomes: NativeRolloutOutcomeCountsV1 {
                    attempted: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                    natural: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                    ..Default::default()
                },
                paired_teacher_better: changed.then_some(5).unwrap_or(0),
                paired_parent_better: 0,
                paired_equal: complete - changed.then_some(5).unwrap_or(0),
                paired_incomplete: incomplete_pairs,
                paired_complete: complete,
                same_action_pair_mismatches: 0,
                teacher_minus_parent_reward_sum: delta,
            },
        }
    }

    #[test]
    fn one_censored_confirmation_pair_blocks_signal_even_above_99_percent_v1() {
        let mut roots: Vec<_> = (0..ROOT_COUNT_V1)
            .map(|index| aggregate_root_v1(index, index < 6, (index < 6) as i64 * 10, 0))
            .collect();
        roots[0] = aggregate_root_v1(0, true, 10, 1);
        let (aggregate, gates) = aggregate_v1(&roots, ROOT_COUNT_V1);
        assert!(gates.at_least_99_percent_natural);
        assert!(gates.teacher_changed_at_least_6_roots);
        assert!(gates.confirmed_mean_delta_at_least_0p05);
        assert!(!gates.all_confirmation_pairs_complete);
        assert_eq!(aggregate.incomplete_confirmation_pairs, 1);
        assert!(!gates.intrinsic_signal_pass);
    }
}

pub const NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_SCHEMA_V1: &str =
    "mtg-kernel-native-rollout-teacher-information-set/v1";
pub const NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_ENVELOPE_SCHEMA_V1: &str =
    "mtg-kernel-native-rollout-teacher-information-set-envelope/v1";
pub const NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_RANK16_SCHEMA_V1: &str =
    "mtg-kernel-native-rollout-teacher-information-set-rank16/v1";
pub const NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_RANK16_ENVELOPE_SCHEMA_V1: &str =
    "mtg-kernel-native-rollout-teacher-information-set-rank16-envelope/v1";

const RANKING_REDETERMINIZATION_DOMAIN_V1: u64 = 0x7261_6e6b_7265_6431;
const CONFIRM_REDETERMINIZATION_DOMAIN_V1: u64 = 0x636f_6e66_7265_6431;
const RANK16_RANKING_REDETERMINIZATION_DOMAIN_V1: u64 = 0x7231_3672_7265_6431;
const RANK16_CONFIRM_REDETERMINIZATION_DOMAIN_V1: u64 = 0x7231_3663_7265_6431;
const RANK16_SEED_DOMAIN_SET_V1: &str =
    "rank16-held-out-domains:r16rred1,r16rpol1,r16cred1,r16cpol1;sample-first-shared-information-set-and-paired-policy-crns/v1";
const SELECTIVE_SEARCH_SCHEMA_V1: &str =
    "mtg-kernel-native-conservative-selective-search-signal/v1";
const SELECTIVE_SEARCH_ENVELOPE_SCHEMA_V1: &str =
    "mtg-kernel-native-conservative-selective-search-signal-envelope/v1";
const SELECTIVE_SEARCH_PROBE_BASE_SEED_V1: u64 = 0x7365_6c73_7263_6831;
const SELECTIVE_SEARCH_MAIN_POLICY_DOMAIN_V1: u64 = 0x7365_6c6d_706f_6c31;
const SELECTIVE_SEARCH_RANKING_REDETERMINIZATION_DOMAIN_V1: u64 = 0x7365_6c72_7265_6431;
const SELECTIVE_SEARCH_RANKING_POLICY_DOMAIN_V1: u64 = 0x7365_6c72_706f_6c31;
const SELECTIVE_SEARCH_CONFIRM_REDETERMINIZATION_DOMAIN_V1: u64 = 0x7365_6c63_7265_6431;
const SELECTIVE_SEARCH_CONFIRM_POLICY_DOMAIN_V1: u64 = 0x7365_6c63_706f_6c31;
const SELECTIVE_SEARCH_ROOT_EPISODE_ID_BASE_V1: u64 = 1_570_000;
const SELECTIVE_SEARCH_MIN_PHYSICAL_DECISION_ID_V1: u64 = 20;
const SELECTIVE_SEARCH_MIN_RANKING_REWARD_MARGIN_V1: i64 = 6;
const SELECTIVE_SEARCH_SEED_DOMAIN_SET_V1: &str =
    "fresh-selective-search-signal-v1:selsrch1,selmpol1,selrred1,selrpol1,selcred1,selcpol1";

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetSampleV1 {
    pub rollout_ordinal: usize,
    pub redeterminization_seed_u64_hex: String,
    pub sampled_privileged_state_hash_u64_hex: String,
    pub checked_branch_start_hashes_u64_hex: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetSamplingV1 {
    pub method: &'static str,
    pub distribution_claim: &'static str,
    pub ranking_samples: Vec<NativeRolloutTeacherInformationSetSampleV1>,
    pub confirmation_samples: Vec<NativeRolloutTeacherInformationSetSampleV1>,
    pub ranking_unique_sampled_state_hashes: usize,
    pub confirmation_unique_sampled_state_hashes: usize,
    pub combined_unique_sampled_state_hashes: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetRootV1 {
    pub rollout: NativeRolloutTeacherRootV1,
    pub information_set_sampling: NativeRolloutTeacherInformationSetSamplingV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetAggregateV1 {
    pub rollout: NativeRolloutTeacherAggregateV1,
    pub redeterminization_samples_required_for_collected_roots: u64,
    pub redeterminization_samples_recorded_successfully: u64,
    pub ranking_branch_starts_checked: u64,
    pub confirmation_branch_starts_checked: u64,
    pub shared_sample_branch_start_mismatches: u64,
    pub roots_with_multiple_distinct_samples: usize,
    pub minimum_unique_samples_at_any_root: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetGatesV1 {
    pub rollout: NativeRolloutTeacherGatesV1,
    pub zero_redeterminization_failures: bool,
    pub every_ranking_sample_shared_by_all_actions: bool,
    pub every_confirmation_sample_shared_by_both_branches: bool,
    pub zero_shared_sample_branch_start_mismatches: bool,
    pub every_root_has_multiple_distinct_information_set_samples: bool,
    pub information_set_signal_pass: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetReportV1 {
    pub schema: &'static str,
    pub publication_encoding: &'static str,
    pub source: NativeRolloutTeacherSourceV1,
    pub config: NativeRolloutTeacherConfigV1,
    pub roots: Vec<NativeRolloutTeacherInformationSetRootV1>,
    pub aggregate: NativeRolloutTeacherInformationSetAggregateV1,
    pub gates: NativeRolloutTeacherInformationSetGatesV1,
    pub interpretation: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetEnvelopeV1 {
    pub schema: &'static str,
    pub deterministic_report_sha256: String,
    pub elapsed_milliseconds: u64,
    pub runtime_under_ten_minutes: bool,
    pub disposition: &'static str,
    pub report: NativeRolloutTeacherInformationSetReportV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetRank16ReportV1 {
    pub schema: &'static str,
    pub publication_encoding: &'static str,
    pub source: NativeRolloutTeacherSourceV1,
    pub config: NativeRolloutTeacherConfigV1,
    pub seed_domain_set: &'static str,
    pub roots: Vec<NativeRolloutTeacherInformationSetRootV1>,
    pub aggregate: NativeRolloutTeacherInformationSetAggregateV1,
    pub gates: NativeRolloutTeacherInformationSetGatesV1,
    pub interpretation: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRolloutTeacherInformationSetRank16EnvelopeV1 {
    pub schema: &'static str,
    pub deterministic_report_sha256: String,
    pub elapsed_milliseconds: u64,
    pub runtime_under_ten_minutes: bool,
    pub disposition: &'static str,
    pub report: NativeRolloutTeacherInformationSetRank16ReportV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeSelectiveSearchConfigV1 {
    pub rollout: NativeRolloutTeacherConfigV1,
    pub minimum_ranking_reward_sum_margin: i64,
    pub fallback: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeSelectiveSearchReportV1 {
    pub schema: &'static str,
    pub publication_encoding: &'static str,
    pub source: NativeRolloutTeacherSourceV1,
    pub config: NativeSelectiveSearchConfigV1,
    pub seed_domain_set: &'static str,
    pub roots: Vec<NativeRolloutTeacherInformationSetRootV1>,
    pub aggregate: NativeRolloutTeacherInformationSetAggregateV1,
    pub gates: NativeRolloutTeacherInformationSetGatesV1,
    pub interpretation: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeSelectiveSearchEnvelopeV1 {
    pub schema: &'static str,
    pub deterministic_report_sha256: String,
    pub elapsed_milliseconds: u64,
    pub runtime_under_ten_minutes: bool,
    pub disposition: &'static str,
    pub report: NativeSelectiveSearchReportV1,
}

fn redeterminization_seed_v1(domain: u64, root_ordinal: usize, rollout_ordinal: usize) -> u64 {
    splitmix64_first_v1(
        PROBE_BASE_SEED_V1
            ^ domain
            ^ (root_ordinal as u64).wrapping_mul(0x9e37_79b1_85eb_ca87)
            ^ (rollout_ordinal as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f),
    )
}

fn prepare_information_set_sample_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    expected_root: FastActorDecisionV1,
    expected_parent_logits_f32_bits: &[u32],
    seed: u64,
) -> Result<(FastActorSessionSnapshotV1, u64), io::Error> {
    let live_hash_before = session.privileged_core_environment_hash();
    let snapshot = session
        .snapshot_current_actor_information_set_v1(seed)
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "acting-player information-set redeterminization failed: episode={} step={} physical_decision={} actor={:?} seed={seed:016x} error={error:?}",
                    expected_root.episode_id,
                    expected_root.step,
                    expected_root.physical_decision_id,
                    expected_root.acting_player,
                ),
            )
        })?;
    if session.privileged_core_environment_hash() != live_hash_before {
        return Err(invalid_data_v1(
            "information-set redeterminization mutated the live session",
        ));
    }

    let mut restored = session.clone();
    restored.restore_v1(&snapshot);
    let sampled_hash = restored.privileged_core_environment_hash();
    let restored_scored = score_current_decision_v1(inference, &restored)
        .map_err(|_| invalid_data_v1("redetermined snapshot binding validation failed"))?;
    let restored_logits_f32_bits: Vec<u32> = restored_scored
        .logits
        .iter()
        .map(|value| value.to_bits())
        .collect();
    if restored_scored.expected != expected_root
        || restored_logits_f32_bits != expected_parent_logits_f32_bits
    {
        return Err(invalid_data_v1(
            "redetermined snapshot changed the actor decision or observation",
        ));
    }
    Ok((snapshot, sampled_hash))
}

fn verified_branch_start_hash_v1(
    observed_hash: u64,
    expected_hash: u64,
    outcome: ContinuationOutcomeV1,
) -> Option<u64> {
    (observed_hash == expected_hash && outcome != ContinuationOutcomeV1::Failure)
        .then_some(observed_hash)
}

fn run_information_set_continuation_v1(
    root_session: &FastActorSessionV1,
    root_snapshot: &FastActorSessionSnapshotV1,
    expected_root_hash: u64,
    inference: &NativeXmageCp7OutcomeInferenceV1,
    root_actor: PlayerSeatV1,
    forced_root_index: u32,
    domain: u64,
    root_ordinal: usize,
    rollout_ordinal: usize,
) -> (ContinuationOutcomeV1, Option<u64>) {
    let mut restored = root_session.clone();
    restored.restore_v1(root_snapshot);
    let observed_hash = restored.privileged_core_environment_hash();
    if observed_hash != expected_root_hash {
        return (ContinuationOutcomeV1::Failure, None);
    }
    let outcome = run_continuation_v1(
        root_session,
        root_snapshot,
        expected_root_hash,
        inference,
        root_actor,
        forced_root_index,
        domain,
        root_ordinal,
        rollout_ordinal,
    );
    (
        outcome,
        verified_branch_start_hash_v1(observed_hash, expected_root_hash, outcome),
    )
}

fn unique_sample_hash_count_v1(samples: impl Iterator<Item = u64>) -> usize {
    samples.collect::<std::collections::BTreeSet<_>>().len()
}

fn evaluate_information_set_root_with_ranking_samples_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    root_ordinal: usize,
    source_episode_ordinal: usize,
    environment_seed: u64,
    ranking_rollouts_per_action: usize,
    ranking_redeterminization_domain: u64,
    ranking_policy_domain: u64,
    confirmation_redeterminization_domain: u64,
    confirmation_policy_domain: u64,
    minimum_ranking_reward_margin: i64,
) -> Result<NativeRolloutTeacherInformationSetRootV1, io::Error> {
    let expected = scored.expected;
    let root_actor = expected.acting_player;
    let original_root_hash = session.privileged_core_environment_hash();
    let semantics = session
        .diagnostic_current_action_semantics()
        .ok_or_else(|| invalid_data_v1("root action semantics missing"))?;
    if semantics.len() != scored.logits.len() {
        return Err(invalid_data_v1("root action semantic width mismatch"));
    }
    let parent_argmax = argmax_v1(&scored.logits);
    let parent_logits_f32_bits: Vec<u32> =
        scored.logits.iter().map(|value| value.to_bits()).collect();

    let mut ranking_outcomes = vec![NativeRolloutOutcomeCountsV1::default(); scored.logits.len()];
    let mut ranking_samples = Vec::with_capacity(ranking_rollouts_per_action);
    let mut ranking_sample_hashes = Vec::with_capacity(ranking_rollouts_per_action);
    for rollout_ordinal in 0..ranking_rollouts_per_action {
        let redeterminization_seed = redeterminization_seed_v1(
            ranking_redeterminization_domain,
            root_ordinal,
            rollout_ordinal,
        );
        let (snapshot, sampled_hash) = prepare_information_set_sample_v1(
            inference,
            session,
            expected,
            &parent_logits_f32_bits,
            redeterminization_seed,
        )?;
        ranking_sample_hashes.push(sampled_hash);
        let mut checked_branch_start_hashes = Vec::with_capacity(scored.logits.len());
        for (action_index, outcomes) in ranking_outcomes.iter_mut().enumerate() {
            let (outcome, verified_start_hash) = run_information_set_continuation_v1(
                session,
                &snapshot,
                sampled_hash,
                inference,
                root_actor,
                action_index as u32,
                ranking_policy_domain,
                root_ordinal,
                rollout_ordinal,
            );
            outcomes.observe_v1(outcome);
            if let Some(observed_hash) = verified_start_hash {
                checked_branch_start_hashes.push(format!("{observed_hash:016x}"));
            }
        }
        ranking_samples.push(NativeRolloutTeacherInformationSetSampleV1 {
            rollout_ordinal,
            redeterminization_seed_u64_hex: format!("{redeterminization_seed:016x}"),
            sampled_privileged_state_hash_u64_hex: format!("{sampled_hash:016x}"),
            checked_branch_start_hashes_u64_hex: checked_branch_start_hashes,
        });
    }
    let ranking: Vec<_> = ranking_outcomes
        .into_iter()
        .enumerate()
        .map(
            |(action_index, outcomes)| NativeRolloutTeacherActionRankingV1 {
                action_index: action_index as u32,
                parent_logit_f32_bits: parent_logits_f32_bits[action_index],
                outcomes,
            },
        )
        .collect();
    let teacher_index =
        conservative_teacher_index_v1(&ranking, parent_argmax, minimum_ranking_reward_margin);

    let mut teacher_outcomes = NativeRolloutOutcomeCountsV1::default();
    let mut parent_outcomes = NativeRolloutOutcomeCountsV1::default();
    let mut paired_teacher_better = 0_u32;
    let mut paired_parent_better = 0_u32;
    let mut paired_equal = 0_u32;
    let mut paired_incomplete = 0_u32;
    let mut paired_complete = 0_u32;
    let mut same_action_pair_mismatches = 0_u32;
    let mut teacher_minus_parent_reward_sum = 0_i64;
    let mut confirmation_samples = Vec::with_capacity(CONFIRMATION_ROLLOUTS_PER_ACTION_V1);
    let mut confirmation_sample_hashes = Vec::with_capacity(CONFIRMATION_ROLLOUTS_PER_ACTION_V1);
    for rollout_ordinal in 0..CONFIRMATION_ROLLOUTS_PER_ACTION_V1 {
        let redeterminization_seed = redeterminization_seed_v1(
            confirmation_redeterminization_domain,
            root_ordinal,
            rollout_ordinal,
        );
        let (snapshot, sampled_hash) = prepare_information_set_sample_v1(
            inference,
            session,
            expected,
            &parent_logits_f32_bits,
            redeterminization_seed,
        )?;
        confirmation_sample_hashes.push(sampled_hash);
        let (teacher, teacher_start_hash) = run_information_set_continuation_v1(
            session,
            &snapshot,
            sampled_hash,
            inference,
            root_actor,
            teacher_index,
            confirmation_policy_domain,
            root_ordinal,
            rollout_ordinal,
        );
        let (parent, parent_start_hash) = run_information_set_continuation_v1(
            session,
            &snapshot,
            sampled_hash,
            inference,
            root_actor,
            parent_argmax,
            confirmation_policy_domain,
            root_ordinal,
            rollout_ordinal,
        );
        let checked_branch_start_hashes_u64_hex = [teacher_start_hash, parent_start_hash]
            .into_iter()
            .flatten()
            .map(|observed_hash| format!("{observed_hash:016x}"))
            .collect();
        confirmation_samples.push(NativeRolloutTeacherInformationSetSampleV1 {
            rollout_ordinal,
            redeterminization_seed_u64_hex: format!("{redeterminization_seed:016x}"),
            sampled_privileged_state_hash_u64_hex: format!("{sampled_hash:016x}"),
            checked_branch_start_hashes_u64_hex,
        });
        teacher_outcomes.observe_v1(teacher);
        parent_outcomes.observe_v1(parent);
        if teacher_index == parent_argmax && teacher != parent {
            same_action_pair_mismatches += 1;
        }
        match (teacher, parent) {
            (ContinuationOutcomeV1::Natural(a), ContinuationOutcomeV1::Natural(b)) => {
                paired_complete += 1;
                teacher_minus_parent_reward_sum += i64::from(a - b);
                if a > b {
                    paired_teacher_better += 1;
                } else if a < b {
                    paired_parent_better += 1;
                } else {
                    paired_equal += 1;
                }
            }
            _ => paired_incomplete += 1,
        }
    }

    let ranking_unique_sampled_state_hashes =
        unique_sample_hash_count_v1(ranking_sample_hashes.iter().copied());
    let confirmation_unique_sampled_state_hashes =
        unique_sample_hash_count_v1(confirmation_sample_hashes.iter().copied());
    let combined_unique_sampled_state_hashes = unique_sample_hash_count_v1(
        ranking_sample_hashes
            .iter()
            .chain(&confirmation_sample_hashes)
            .copied(),
    );
    Ok(NativeRolloutTeacherInformationSetRootV1 {
        rollout: NativeRolloutTeacherRootV1 {
            root_ordinal,
            source_episode_ordinal,
            episode_id: expected.episode_id,
            environment_seed_u64_hex: format!("{environment_seed:016x}"),
            step: expected.step,
            physical_decision_id: expected.physical_decision_id,
            acting_player: expected.acting_player,
            legal_action_count: expected.legal_action_count,
            privileged_state_hash_u64_hex: format!("{original_root_hash:016x}"),
            action_semantics: semantics,
            parent_logits_f32_bits,
            parent_argmax_index: parent_argmax,
            teacher_index,
            teacher_differs_from_parent: teacher_index != parent_argmax,
            ranking,
            confirmation: NativeRolloutTeacherConfirmationV1 {
                teacher_outcomes,
                parent_argmax_outcomes: parent_outcomes,
                paired_teacher_better,
                paired_parent_better,
                paired_equal,
                paired_incomplete,
                paired_complete,
                same_action_pair_mismatches,
                teacher_minus_parent_reward_sum,
            },
        },
        information_set_sampling: NativeRolloutTeacherInformationSetSamplingV1 {
            method:
                "acting-player-information-set-deterministic-modulo-fisher-yates-redeterminization/v1",
            distribution_claim:
                "deterministic modulo-Fisher-Yates over information-set-consistent hidden-card assignments with negligible modulo bias, not a Bayesian posterior",
            ranking_samples,
            confirmation_samples,
            ranking_unique_sampled_state_hashes,
            confirmation_unique_sampled_state_hashes,
            combined_unique_sampled_state_hashes,
        },
    })
}

fn evaluate_information_set_root_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    root_ordinal: usize,
    source_episode_ordinal: usize,
    environment_seed: u64,
) -> Result<NativeRolloutTeacherInformationSetRootV1, io::Error> {
    evaluate_information_set_root_with_ranking_samples_v1(
        inference,
        session,
        scored,
        root_ordinal,
        source_episode_ordinal,
        environment_seed,
        RANKING_ROLLOUTS_PER_ACTION_V1,
        RANKING_REDETERMINIZATION_DOMAIN_V1,
        RANKING_POLICY_DOMAIN_V1,
        CONFIRM_REDETERMINIZATION_DOMAIN_V1,
        CONFIRM_POLICY_DOMAIN_V1,
        0,
    )
}

fn evaluate_information_set_rank16_root_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    root_ordinal: usize,
    source_episode_ordinal: usize,
    environment_seed: u64,
) -> Result<NativeRolloutTeacherInformationSetRootV1, io::Error> {
    evaluate_information_set_root_with_ranking_samples_v1(
        inference,
        session,
        scored,
        root_ordinal,
        source_episode_ordinal,
        environment_seed,
        RANKING_ROLLOUTS_PER_ACTION_RANK16_V1,
        RANK16_RANKING_REDETERMINIZATION_DOMAIN_V1,
        RANK16_RANKING_POLICY_DOMAIN_V1,
        RANK16_CONFIRM_REDETERMINIZATION_DOMAIN_V1,
        RANK16_CONFIRM_POLICY_DOMAIN_V1,
        0,
    )
}

fn evaluate_selective_search_root_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    root_ordinal: usize,
    source_episode_ordinal: usize,
    environment_seed: u64,
) -> Result<NativeRolloutTeacherInformationSetRootV1, io::Error> {
    evaluate_information_set_root_with_ranking_samples_v1(
        inference,
        session,
        scored,
        root_ordinal,
        source_episode_ordinal,
        environment_seed,
        RANKING_ROLLOUTS_PER_ACTION_RANK16_V1,
        SELECTIVE_SEARCH_RANKING_REDETERMINIZATION_DOMAIN_V1,
        SELECTIVE_SEARCH_RANKING_POLICY_DOMAIN_V1,
        SELECTIVE_SEARCH_CONFIRM_REDETERMINIZATION_DOMAIN_V1,
        SELECTIVE_SEARCH_CONFIRM_POLICY_DOMAIN_V1,
        SELECTIVE_SEARCH_MIN_RANKING_REWARD_MARGIN_V1,
    )
}

fn aggregate_information_set_with_ranking_samples_v1(
    roots: &[NativeRolloutTeacherInformationSetRootV1],
    source_episodes_examined: usize,
    ranking_rollouts_per_action: usize,
) -> (
    NativeRolloutTeacherInformationSetAggregateV1,
    NativeRolloutTeacherInformationSetGatesV1,
) {
    let rollout_roots: Vec<_> = roots.iter().map(|root| root.rollout.clone()).collect();
    let (rollout_aggregate, rollout_gates) = aggregate_with_ranking_samples_v1(
        &rollout_roots,
        source_episodes_examined,
        ranking_rollouts_per_action,
    );
    let mut samples_attempted = 0_u64;
    let mut ranking_branch_starts_checked = 0_u64;
    let mut confirmation_branch_starts_checked = 0_u64;
    let mut shared_sample_branch_start_mismatches = 0_u64;
    let mut every_ranking_sample_shared_by_all_actions = true;
    let mut every_confirmation_sample_shared_by_both_branches = true;
    let mut every_root_has_exact_sample_counts = true;
    let mut roots_with_multiple_distinct_samples = 0usize;
    let mut minimum_unique_samples_at_any_root = usize::MAX;

    for root in roots {
        if root.information_set_sampling.ranking_samples.len() != ranking_rollouts_per_action
            || root.information_set_sampling.confirmation_samples.len()
                != CONFIRMATION_ROLLOUTS_PER_ACTION_V1
        {
            every_root_has_exact_sample_counts = false;
        }
        samples_attempted += (root.information_set_sampling.ranking_samples.len()
            + root.information_set_sampling.confirmation_samples.len())
            as u64;
        minimum_unique_samples_at_any_root = minimum_unique_samples_at_any_root.min(
            root.information_set_sampling
                .combined_unique_sampled_state_hashes,
        );
        if root
            .information_set_sampling
            .combined_unique_sampled_state_hashes
            >= 2
        {
            roots_with_multiple_distinct_samples += 1;
        }
        for sample in &root.information_set_sampling.ranking_samples {
            ranking_branch_starts_checked +=
                sample.checked_branch_start_hashes_u64_hex.len() as u64;
            if sample.checked_branch_start_hashes_u64_hex.len()
                != root.rollout.legal_action_count as usize
            {
                every_ranking_sample_shared_by_all_actions = false;
            }
            shared_sample_branch_start_mismatches += (root.rollout.legal_action_count as usize)
                .saturating_sub(sample.checked_branch_start_hashes_u64_hex.len())
                as u64;
            shared_sample_branch_start_mismatches += sample
                .checked_branch_start_hashes_u64_hex
                .iter()
                .filter(|hash| **hash != sample.sampled_privileged_state_hash_u64_hex)
                .count() as u64;
        }
        for sample in &root.information_set_sampling.confirmation_samples {
            confirmation_branch_starts_checked +=
                sample.checked_branch_start_hashes_u64_hex.len() as u64;
            if sample.checked_branch_start_hashes_u64_hex.len() != 2 {
                every_confirmation_sample_shared_by_both_branches = false;
            }
            shared_sample_branch_start_mismatches +=
                2usize.saturating_sub(sample.checked_branch_start_hashes_u64_hex.len()) as u64;
            shared_sample_branch_start_mismatches += sample
                .checked_branch_start_hashes_u64_hex
                .iter()
                .filter(|hash| **hash != sample.sampled_privileged_state_hash_u64_hex)
                .count() as u64;
        }
    }
    if roots.is_empty() {
        minimum_unique_samples_at_any_root = 0;
    }
    let samples_expected =
        (roots.len() * (ranking_rollouts_per_action + CONFIRMATION_ROLLOUTS_PER_ACTION_V1)) as u64;
    let zero_redeterminization_failures =
        every_root_has_exact_sample_counts && samples_attempted == samples_expected;
    let zero_shared_sample_branch_start_mismatches = shared_sample_branch_start_mismatches == 0;
    let every_root_has_multiple_distinct_information_set_samples =
        !roots.is_empty() && roots_with_multiple_distinct_samples == roots.len();
    let information_set_signal_pass = rollout_gates.intrinsic_signal_pass
        && zero_redeterminization_failures
        && every_ranking_sample_shared_by_all_actions
        && every_confirmation_sample_shared_by_both_branches
        && zero_shared_sample_branch_start_mismatches
        && every_root_has_multiple_distinct_information_set_samples;

    (
        NativeRolloutTeacherInformationSetAggregateV1 {
            rollout: rollout_aggregate,
            redeterminization_samples_required_for_collected_roots: samples_expected,
            redeterminization_samples_recorded_successfully: samples_attempted,
            ranking_branch_starts_checked,
            confirmation_branch_starts_checked,
            shared_sample_branch_start_mismatches,
            roots_with_multiple_distinct_samples,
            minimum_unique_samples_at_any_root,
        },
        NativeRolloutTeacherInformationSetGatesV1 {
            rollout: rollout_gates,
            zero_redeterminization_failures,
            every_ranking_sample_shared_by_all_actions,
            every_confirmation_sample_shared_by_both_branches,
            zero_shared_sample_branch_start_mismatches,
            every_root_has_multiple_distinct_information_set_samples,
            information_set_signal_pass,
        },
    )
}

fn aggregate_information_set_v1(
    roots: &[NativeRolloutTeacherInformationSetRootV1],
    source_episodes_examined: usize,
) -> (
    NativeRolloutTeacherInformationSetAggregateV1,
    NativeRolloutTeacherInformationSetGatesV1,
) {
    aggregate_information_set_with_ranking_samples_v1(
        roots,
        source_episodes_examined,
        RANKING_ROLLOUTS_PER_ACTION_V1,
    )
}

fn aggregate_information_set_rank16_v1(
    roots: &[NativeRolloutTeacherInformationSetRootV1],
    source_episodes_examined: usize,
) -> (
    NativeRolloutTeacherInformationSetAggregateV1,
    NativeRolloutTeacherInformationSetGatesV1,
) {
    aggregate_information_set_with_ranking_samples_v1(
        roots,
        source_episodes_examined,
        RANKING_ROLLOUTS_PER_ACTION_RANK16_V1,
    )
}

/// Exact deterministic information-set report bytes written by the sibling
/// CLI and covered by its reported SHA-256. Runtime timing is absent.
pub fn native_rollout_teacher_information_set_report_bytes_v1(
    report: &NativeRolloutTeacherInformationSetReportV1,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn information_set_report_sha256_v1(
    report: &NativeRolloutTeacherInformationSetReportV1,
) -> Result<String, serde_json::Error> {
    let bytes = native_rollout_teacher_information_set_report_bytes_v1(report)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Runs the acting-player information-set sibling of the fixed rollout
/// diagnostic. Each rollout ordinal samples one hidden-card assignment and
/// shares that exact snapshot across every action being compared.
pub fn run_native_rollout_teacher_information_set_v1(
    source_outcome_root: impl AsRef<Path>,
) -> Result<NativeRolloutTeacherInformationSetEnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let inference = load_xmage_cp7_outcome_inference_v1(source_outcome_root.as_ref())?;
    if lower_hex_raw32_v1(inference.manifest_sha256_v1()) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_raw32_v1(inference.payload_sha256_v1()) != RETAINED_PAYLOAD_SHA256_V1
        || lower_hex_raw32_v1(inference.native_state_sha256_v1()) != RETAINED_NATIVE_STATE_SHA256_V1
        || lower_hex_raw32_v1(inference.model_parameter_sha256_v1())
            != RETAINED_MODEL_PARAMETER_SHA256_V1
        || inference.adam_step_v1() != RETAINED_ADAM_STEP_V1
    {
        return Err(invalid_data_v1("source is not the exact retained 706b checkpoint").into());
    }
    let source = NativeRolloutTeacherSourceV1 {
        outcome_manifest_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
        outcome_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
        native_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
        corpus_sha256: lower_hex_raw32_v1(inference.corpus_sha256_v1()),
        adam_step: inference.adam_step_v1(),
    };
    let config = NativeRolloutTeacherConfigV1 {
        base_seed_u64_hex: format!("{PROBE_BASE_SEED_V1:016x}"),
        root_count: ROOT_COUNT_V1,
        max_source_episodes: MAX_SOURCE_EPISODES_V1,
        roots_per_episode: 1,
        root_eligibility:
            "surface, substep_count=1, physical_decision_id>=10, legal_action_count=2..8",
        ranking_rollouts_per_action: RANKING_ROLLOUTS_PER_ACTION_V1,
        confirmation_rollouts_per_action: CONFIRMATION_ROLLOUTS_PER_ACTION_V1,
        max_branch_policy_steps_including_forced_root: MAX_BRANCH_POLICY_STEPS_V1,
        continuation_policy: "retained-checkpoint-temperature-1-self-play",
        continuation_sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        continuation_sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        branch_randomness:
            "paired-common-random-number-policy-sequences-and-shared-information-set-samples/v1",
        information_scope:
            "acting-player-information-set-deterministic-modulo-fisher-yates-redeterminization/v1",
        training_admissibility:
            "diagnostic-corpus-gate-only-until-byte-identical-reproducible-rerun",
    };

    let mut roots = Vec::with_capacity(ROOT_COUNT_V1);
    let mut source_episodes_examined = 0usize;
    for episode_ordinal in 0..MAX_SOURCE_EPISODES_V1 {
        if roots.len() == ROOT_COUNT_V1 {
            break;
        }
        source_episodes_examined += 1;
        let episode_id = ROOT_EPISODE_ID_BASE_V1 + episode_ordinal as u64;
        let environment_seed = environment_seed_v1(episode_ordinal);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            SESSION_MAX_PHYSICAL_DECISIONS_V1,
            SESSION_MAX_POLICY_STEPS_V1,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .map_err(|_| invalid_data_v1("Rally source session reset failed"))?;

        for _ in 0..MAX_SOURCE_POLICY_STEPS_V1 {
            let FastActorResponseV1::Decision(_) = session.current_response() else {
                break;
            };
            let scored = score_current_decision_v1(&inference, &session)
                .map_err(|_| invalid_data_v1("source policy scoring failed"))?;
            if eligible_root_v1(scored.expected) {
                let root = evaluate_information_set_root_v1(
                    &inference,
                    &session,
                    scored,
                    roots.len(),
                    episode_ordinal,
                    environment_seed,
                )?;
                roots.push(root);
                break;
            }
            let selected = sample_policy_v1(
                &scored.logits,
                main_policy_seed_v1(episode_ordinal, scored.expected.step),
            )
            .map_err(|_| invalid_data_v1("source policy sampling failed"))?;
            consume_scored_v1(&mut session, scored, selected)
                .map_err(|_| invalid_data_v1("source policy consume failed"))?;
        }
    }

    let (aggregate, gates) = aggregate_information_set_v1(&roots, source_episodes_examined);
    let report = NativeRolloutTeacherInformationSetReportV1 {
        schema: NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_SCHEMA_V1,
        publication_encoding: "serde-json-pretty-utf8-trailing-lf/v1",
        source,
        config,
        roots,
        aggregate,
        gates,
        interpretation: "Acting-player information-set diagnostic. Hidden cards use the frozen deterministic modulo-Fisher-Yates sampler over assignments consistent with represented knowledge; its modulo bias is negligible but it is not an exact-uniform or Bayesian posterior sampler. Labels remain diagnostic-corpus-gate-only until a byte-identical rerun; this report does not itself authorize training.",
    };
    let deterministic_report_sha256 = information_set_report_sha256_v1(&report)?;
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let runtime_under_ten_minutes = elapsed_milliseconds < 600_000;
    let disposition = if report.gates.information_set_signal_pass && runtime_under_ten_minutes {
        "provisional-pass-requires-identical-rerun-before-diagnostic-corpus"
    } else {
        "reject-information-set-rollout-teacher"
    };
    Ok(NativeRolloutTeacherInformationSetEnvelopeV1 {
        schema: NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_ENVELOPE_SCHEMA_V1,
        deterministic_report_sha256,
        elapsed_milliseconds,
        runtime_under_ten_minutes,
        disposition,
        report,
    })
}

/// Exact deterministic rank-16 information-set report bytes written by its
/// CLI and covered by its reported SHA-256. Runtime timing is absent.
pub fn native_rollout_teacher_information_set_rank16_report_bytes_v1(
    report: &NativeRolloutTeacherInformationSetRank16ReportV1,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn information_set_rank16_report_sha256_v1(
    report: &NativeRolloutTeacherInformationSetRank16ReportV1,
) -> Result<String, serde_json::Error> {
    let bytes = native_rollout_teacher_information_set_rank16_report_bytes_v1(report)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Runs the bounded rank-16 acting-player information-set sibling. Ranking
/// uses sixteen shared hidden-card samples per legal action. Confirmation
/// remains thirty-two fresh shared samples for teacher versus parent.
pub fn run_native_rollout_teacher_information_set_rank16_v1(
    source_outcome_root: impl AsRef<Path>,
) -> Result<NativeRolloutTeacherInformationSetRank16EnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let inference = load_xmage_cp7_outcome_inference_v1(source_outcome_root.as_ref())?;
    if lower_hex_raw32_v1(inference.manifest_sha256_v1()) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_raw32_v1(inference.payload_sha256_v1()) != RETAINED_PAYLOAD_SHA256_V1
        || lower_hex_raw32_v1(inference.native_state_sha256_v1()) != RETAINED_NATIVE_STATE_SHA256_V1
        || lower_hex_raw32_v1(inference.model_parameter_sha256_v1())
            != RETAINED_MODEL_PARAMETER_SHA256_V1
        || inference.adam_step_v1() != RETAINED_ADAM_STEP_V1
    {
        return Err(invalid_data_v1("source is not the exact retained 706b checkpoint").into());
    }
    let source = NativeRolloutTeacherSourceV1 {
        outcome_manifest_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
        outcome_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
        native_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
        corpus_sha256: lower_hex_raw32_v1(inference.corpus_sha256_v1()),
        adam_step: inference.adam_step_v1(),
    };
    let config = NativeRolloutTeacherConfigV1 {
        base_seed_u64_hex: format!("{PROBE_BASE_SEED_V1:016x}"),
        root_count: ROOT_COUNT_V1,
        max_source_episodes: MAX_SOURCE_EPISODES_V1,
        roots_per_episode: 1,
        root_eligibility:
            "surface, substep_count=1, physical_decision_id>=10, legal_action_count=2..8",
        ranking_rollouts_per_action: RANKING_ROLLOUTS_PER_ACTION_RANK16_V1,
        confirmation_rollouts_per_action: CONFIRMATION_ROLLOUTS_PER_ACTION_V1,
        max_branch_policy_steps_including_forced_root: MAX_BRANCH_POLICY_STEPS_V1,
        continuation_policy: "retained-checkpoint-temperature-1-self-play",
        continuation_sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        continuation_sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        branch_randomness: RANK16_SEED_DOMAIN_SET_V1,
        information_scope:
            "acting-player-information-set-deterministic-modulo-fisher-yates-redeterminization/v1",
        training_admissibility:
            "rank16-diagnostic-only-no-training-authorization-even-after-reproducible-rerun",
    };

    let mut roots = Vec::with_capacity(ROOT_COUNT_V1);
    let mut source_episodes_examined = 0usize;
    for episode_ordinal in 0..MAX_SOURCE_EPISODES_V1 {
        if roots.len() == ROOT_COUNT_V1 {
            break;
        }
        source_episodes_examined += 1;
        let episode_id = ROOT_EPISODE_ID_BASE_V1 + episode_ordinal as u64;
        let environment_seed = environment_seed_v1(episode_ordinal);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            SESSION_MAX_PHYSICAL_DECISIONS_V1,
            SESSION_MAX_POLICY_STEPS_V1,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .map_err(|_| invalid_data_v1("Rally source session reset failed"))?;

        for _ in 0..MAX_SOURCE_POLICY_STEPS_V1 {
            let FastActorResponseV1::Decision(_) = session.current_response() else {
                break;
            };
            let scored = score_current_decision_v1(&inference, &session)
                .map_err(|_| invalid_data_v1("source policy scoring failed"))?;
            if eligible_root_v1(scored.expected) {
                let root = evaluate_information_set_rank16_root_v1(
                    &inference,
                    &session,
                    scored,
                    roots.len(),
                    episode_ordinal,
                    environment_seed,
                )?;
                roots.push(root);
                break;
            }
            let selected = sample_policy_v1(
                &scored.logits,
                main_policy_seed_v1(episode_ordinal, scored.expected.step),
            )
            .map_err(|_| invalid_data_v1("source policy sampling failed"))?;
            consume_scored_v1(&mut session, scored, selected)
                .map_err(|_| invalid_data_v1("source policy consume failed"))?;
        }
    }

    let (aggregate, gates) = aggregate_information_set_rank16_v1(&roots, source_episodes_examined);
    let report = NativeRolloutTeacherInformationSetRank16ReportV1 {
        schema: NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_RANK16_SCHEMA_V1,
        publication_encoding: "serde-json-pretty-utf8-trailing-lf/v1",
        source,
        config,
        seed_domain_set: RANK16_SEED_DOMAIN_SET_V1,
        roots,
        aggregate,
        gates,
        interpretation: "Acting-player information-set rank-16 diagnostic. Ranking uses exactly sixteen sample-first hidden-card assignments shared across all legal actions; confirmation uses exactly thirty-two fresh assignments shared by teacher and parent. All four rank-16 redeterminization and continuation-policy seed domains are held out from the rank-4 diagnostic. The frozen sampler is not a Bayesian posterior. This diagnostic does not authorize training.",
    };
    let deterministic_report_sha256 = information_set_rank16_report_sha256_v1(&report)?;
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let runtime_under_ten_minutes = elapsed_milliseconds < 600_000;
    let disposition = if report.gates.information_set_signal_pass && runtime_under_ten_minutes {
        "provisional-rank16-pass-requires-identical-rerun-diagnostic-only-no-training"
    } else {
        "reject-rank16-information-set-signal-diagnostic-only-no-training"
    };
    Ok(NativeRolloutTeacherInformationSetRank16EnvelopeV1 {
        schema: NATIVE_ROLLOUT_TEACHER_INFORMATION_SET_RANK16_ENVELOPE_SCHEMA_V1,
        deterministic_report_sha256,
        elapsed_milliseconds,
        runtime_under_ten_minutes,
        disposition,
        report,
    })
}

/// Exact deterministic conservative selective-search report bytes written by
/// its CLI and covered by its reported SHA-256. Runtime timing is absent.
pub fn native_selective_search_report_bytes_v1(
    report: &NativeSelectiveSearchReportV1,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn selective_search_report_sha256_v1(
    report: &NativeSelectiveSearchReportV1,
) -> Result<String, serde_json::Error> {
    let bytes = native_selective_search_report_bytes_v1(report)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Runs the fresh conservative selective-search signal screen. Search keeps
/// the retained-policy argmax unless rank-16 terminal reward exceeds it by
/// the fixed margin, then confirms the selected action on fresh paired draws.
pub fn run_native_selective_search_signal_v1(
    source_outcome_root: impl AsRef<Path>,
) -> Result<NativeSelectiveSearchEnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let inference = load_xmage_cp7_outcome_inference_v1(source_outcome_root.as_ref())?;
    if lower_hex_raw32_v1(inference.manifest_sha256_v1()) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_raw32_v1(inference.payload_sha256_v1()) != RETAINED_PAYLOAD_SHA256_V1
        || lower_hex_raw32_v1(inference.native_state_sha256_v1()) != RETAINED_NATIVE_STATE_SHA256_V1
        || lower_hex_raw32_v1(inference.model_parameter_sha256_v1())
            != RETAINED_MODEL_PARAMETER_SHA256_V1
        || inference.adam_step_v1() != RETAINED_ADAM_STEP_V1
    {
        return Err(invalid_data_v1("source is not the exact retained 706b checkpoint").into());
    }
    let source = NativeRolloutTeacherSourceV1 {
        outcome_manifest_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
        outcome_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
        native_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
        corpus_sha256: lower_hex_raw32_v1(inference.corpus_sha256_v1()),
        adam_step: inference.adam_step_v1(),
    };
    let rollout = NativeRolloutTeacherConfigV1 {
        base_seed_u64_hex: format!("{SELECTIVE_SEARCH_PROBE_BASE_SEED_V1:016x}"),
        root_count: ROOT_COUNT_V1,
        max_source_episodes: MAX_SOURCE_EPISODES_V1,
        roots_per_episode: 1,
        root_eligibility:
            "surface, substep_count=1, physical_decision_id>=20, legal_action_count=2..8",
        ranking_rollouts_per_action: RANKING_ROLLOUTS_PER_ACTION_RANK16_V1,
        confirmation_rollouts_per_action: CONFIRMATION_ROLLOUTS_PER_ACTION_V1,
        max_branch_policy_steps_including_forced_root: MAX_BRANCH_POLICY_STEPS_V1,
        continuation_policy: "retained-checkpoint-temperature-1-self-play",
        continuation_sampler_identity: FAST_CATEGORICAL_SAMPLER_VERSION,
        continuation_sampler_contract_sha256: FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        branch_randomness: SELECTIVE_SEARCH_SEED_DOMAIN_SET_V1,
        information_scope:
            "acting-player-information-set-deterministic-modulo-fisher-yates-redeterminization/v1",
        training_admissibility:
            "selective-search-signal-only-no-training-or-live-evaluation-authorization",
    };
    let config = NativeSelectiveSearchConfigV1 {
        rollout,
        minimum_ranking_reward_sum_margin: SELECTIVE_SEARCH_MIN_RANKING_REWARD_MARGIN_V1,
        fallback: "retained-policy-argmax",
    };

    let mut roots = Vec::with_capacity(ROOT_COUNT_V1);
    let mut source_episodes_examined = 0usize;
    for episode_ordinal in 0..MAX_SOURCE_EPISODES_V1 {
        if roots.len() == ROOT_COUNT_V1 {
            break;
        }
        source_episodes_examined += 1;
        let episode_id = SELECTIVE_SEARCH_ROOT_EPISODE_ID_BASE_V1 + episode_ordinal as u64;
        let environment_seed = selective_search_environment_seed_v1(episode_ordinal);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            SESSION_MAX_PHYSICAL_DECISIONS_V1,
            SESSION_MAX_POLICY_STEPS_V1,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .map_err(|_| invalid_data_v1("Rally source session reset failed"))?;

        for _ in 0..MAX_SOURCE_POLICY_STEPS_V1 {
            let FastActorResponseV1::Decision(_) = session.current_response() else {
                break;
            };
            let scored = score_current_decision_v1(&inference, &session)
                .map_err(|_| invalid_data_v1("source policy scoring failed"))?;
            if eligible_selective_search_root_v1(scored.expected) {
                let root = evaluate_selective_search_root_v1(
                    &inference,
                    &session,
                    scored,
                    roots.len(),
                    episode_ordinal,
                    environment_seed,
                )?;
                roots.push(root);
                break;
            }
            let selected = sample_policy_v1(
                &scored.logits,
                selective_search_main_policy_seed_v1(episode_ordinal, scored.expected.step),
            )
            .map_err(|_| invalid_data_v1("source policy sampling failed"))?;
            consume_scored_v1(&mut session, scored, selected)
                .map_err(|_| invalid_data_v1("source policy consume failed"))?;
        }
    }

    let (aggregate, gates) = aggregate_information_set_rank16_v1(&roots, source_episodes_examined);
    let report = NativeSelectiveSearchReportV1 {
        schema: SELECTIVE_SEARCH_SCHEMA_V1,
        publication_encoding: "serde-json-pretty-utf8-trailing-lf/v1",
        source,
        config,
        seed_domain_set: SELECTIVE_SEARCH_SEED_DOMAIN_SET_V1,
        roots,
        aggregate,
        gates,
        interpretation: "Fresh later-game acting-player information-set selective-search screen. Ranking uses exactly sixteen sample-first hidden-card assignments shared across legal actions. The retained-policy argmax is preserved unless the best alternative exceeds its terminal reward sum by at least six. Confirmation uses exactly thirty-two fresh assignments shared by search and parent. All source, ranking, confirmation, and continuation domains are held out from prior diagnostics. The sampler is not a Bayesian posterior. This report does not authorize training or live evaluation.",
    };
    let deterministic_report_sha256 = selective_search_report_sha256_v1(&report)?;
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let runtime_under_ten_minutes = elapsed_milliseconds < 600_000;
    let disposition = if report.gates.information_set_signal_pass && runtime_under_ten_minutes {
        "pass-selective-search-signal-proceed-to-exact-g384-live-screen"
    } else {
        "reject-full-terminal-retained-policy-rollout-search"
    };
    Ok(NativeSelectiveSearchEnvelopeV1 {
        schema: SELECTIVE_SEARCH_ENVELOPE_SCHEMA_V1,
        deterministic_report_sha256,
        elapsed_milliseconds,
        runtime_under_ten_minutes,
        disposition,
        report,
    })
}

pub const NATIVE_ACTION_CONDITIONED_COUNTERFACTUAL_CORPUS_SCHEMA_V1: &str =
    "mtg-kernel-native-action-conditioned-counterfactual-corpus/v1";

const COUNTERFACTUAL_ROOT_COUNT_V1: usize = 256;
const COUNTERFACTUAL_ROOTS_PER_SEAT_V1: usize = 128;
const COUNTERFACTUAL_SAMPLES_PER_ACTION_V1: usize = 4;
const COUNTERFACTUAL_MAX_SOURCE_EPISODES_V1: usize = 2_048;
const COUNTERFACTUAL_ROOT_EPISODE_ID_BASE_V1: u64 = 1_604_000;
const COUNTERFACTUAL_BASE_SEED_V1: u64 = 0x6371_5f72_6f6f_7431;
const COUNTERFACTUAL_SOURCE_POLICY_DOMAIN_V1: u64 = 0x6371_5f73_7263_7031;
const COUNTERFACTUAL_REDETERMINIZATION_DOMAIN_V1: u64 = 0x6371_5f72_6564_6574;
const COUNTERFACTUAL_CONTINUATION_POLICY_DOMAIN_V1: u64 = 0x6371_5f63_6f6e_7431;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeCounterfactualFlatTensorV1 {
    pub state_f32_bits: Vec<u32>,
    pub object_features_f32_bits: Vec<u32>,
    pub object_card_ids: Vec<i64>,
    pub object_groups: Vec<i64>,
    pub object_node_ids: Vec<i64>,
    pub edge_features_f32_bits: Vec<u32>,
    pub edge_source_indices: Vec<i64>,
    pub edge_target_indices: Vec<i64>,
    pub action_features_f32_bits: Vec<u32>,
    pub action_ref_features_f32_bits: Vec<u32>,
    pub action_ref_card_ids: Vec<i64>,
    pub action_ref_action_indices: Vec<i64>,
    pub action_ref_node_indices: Vec<i64>,
}

impl From<NativeFlatDecisionTensorV2> for NativeCounterfactualFlatTensorV1 {
    fn from(value: NativeFlatDecisionTensorV2) -> Self {
        Self {
            state_f32_bits: value.state.into_iter().map(f32::to_bits).collect(),
            object_features_f32_bits: value
                .object_features
                .into_iter()
                .map(f32::to_bits)
                .collect(),
            object_card_ids: value.object_card_ids,
            object_groups: value.object_groups,
            object_node_ids: value.object_node_ids,
            edge_features_f32_bits: value.edge_features.into_iter().map(f32::to_bits).collect(),
            edge_source_indices: value.edge_source_indices,
            edge_target_indices: value.edge_target_indices,
            action_features_f32_bits: value
                .action_features
                .into_iter()
                .map(f32::to_bits)
                .collect(),
            action_ref_features_f32_bits: value
                .action_ref_features
                .into_iter()
                .map(f32::to_bits)
                .collect(),
            action_ref_card_ids: value.action_ref_card_ids,
            action_ref_action_indices: value.action_ref_action_indices,
            action_ref_node_indices: value.action_ref_node_indices,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualHistoryDecisionV1 {
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: PlayerSeatV1,
    pub selected_index: u32,
    pub selected_semantic: ActionSemanticV1,
    pub parent_logits_f32_bits: Vec<u32>,
    pub parent_value_f32_bits: u32,
    pub tensor: NativeCounterfactualFlatTensorV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualSampleV1 {
    pub sample_ordinal: usize,
    pub redeterminization_seed_u64_hex: String,
    pub sampled_privileged_state_hash_u64_hex: String,
    pub checked_branch_start_hashes_u64_hex: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualRootV1 {
    pub root_ordinal: usize,
    pub balance_pair_index: usize,
    pub within_seat_ordinal: usize,
    pub source_episode_ordinal: usize,
    pub episode_id: u64,
    pub environment_seed_u64_hex: String,
    pub step: u64,
    pub physical_decision_id: u64,
    pub acting_player: PlayerSeatV1,
    pub legal_action_count: u32,
    pub public_root_tensor: NativeCounterfactualFlatTensorV1,
    pub action_semantics: Vec<ActionSemanticV1>,
    pub parent_logits_f32_bits: Vec<u32>,
    pub parent_value_f32_bits: u32,
    pub parent_argmax_index: u32,
    pub public_history: Vec<NativeCounterfactualHistoryDecisionV1>,
    pub samples: Vec<NativeCounterfactualSampleV1>,
    pub action_terminal_rewards: Vec<Vec<i32>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualConfigV1 {
    pub base_seed_u64_hex: String,
    pub root_count: usize,
    pub roots_per_acting_seat: usize,
    pub samples_per_action: usize,
    pub maximum_source_episodes: usize,
    pub root_eligibility: &'static str,
    pub split: &'static str,
    pub continuation_policy: &'static str,
    pub information_scope: &'static str,
    pub target: &'static str,
    pub input_exclusions: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualAggregateV1 {
    pub source_episodes_examined: usize,
    pub roots_collected: usize,
    pub p0_roots: usize,
    pub p1_roots: usize,
    pub balance_pairs: usize,
    pub train_roots: usize,
    pub selection_roots: usize,
    pub heldout_roots: usize,
    pub legal_actions: usize,
    pub natural_terminal_continuations: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualGatesV1 {
    pub collected_exactly_256_roots: bool,
    pub exactly_128_roots_per_acting_seat: bool,
    pub exactly_one_root_per_source_episode: bool,
    pub every_root_has_four_distinct_information_set_samples: bool,
    pub every_sample_shared_by_every_legal_action: bool,
    pub exact_public_root_identity_under_every_sample: bool,
    pub every_branch_reached_natural_terminal: bool,
    pub split_is_exactly_128_64_64_roots: bool,
    pub corpus_qualified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeCounterfactualReportV1 {
    pub schema: &'static str,
    pub publication_encoding: &'static str,
    pub source: NativeRolloutTeacherSourceV1,
    pub config: NativeCounterfactualConfigV1,
    pub roots: Vec<NativeCounterfactualRootV1>,
    pub aggregate: NativeCounterfactualAggregateV1,
    pub gates: NativeCounterfactualGatesV1,
    pub interpretation: &'static str,
}

pub struct NativeCounterfactualEnvelopeV1 {
    pub deterministic_report_sha256: String,
    pub elapsed_milliseconds: u64,
    pub disposition: &'static str,
    pub report: NativeCounterfactualReportV1,
}

fn counterfactual_environment_seed_v1(episode_ordinal: usize) -> u64 {
    splitmix64_first_v1(
        COUNTERFACTUAL_BASE_SEED_V1 ^ (episode_ordinal as u64).wrapping_mul(0xd6e8_feb8_6659_fd93),
    )
}

fn counterfactual_source_policy_seed_v1(episode_ordinal: usize, policy_step: u64) -> u64 {
    splitmix64_first_v1(
        COUNTERFACTUAL_BASE_SEED_V1
            ^ COUNTERFACTUAL_SOURCE_POLICY_DOMAIN_V1
            ^ (episode_ordinal as u64).wrapping_mul(0xa076_1d64_78bd_642f)
            ^ policy_step.wrapping_mul(0xe703_7ed1_a0b4_28db),
    )
}

fn tensorize_current_decision_v1(
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
) -> Result<NativeFlatDecisionTensorV2, io::Error> {
    let packet = <FlatScoredFamilyV2 as FlatScoredFamilyCore>::encode_packet(
        session,
        expected,
        &mut Default::default(),
        OwnedFlatScoringDecisionV2::default(),
    )
    .map_err(|_| invalid_data_v1("counterfactual tensor packet encode failed"))?;
    if !<FlatScoredFamilyV2 as FlatScoredFamilyCore>::expected_matches_binding(
        expected,
        <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_decision(&packet),
    ) {
        return Err(invalid_data_v1(
            "counterfactual tensor packet binding mismatch",
        ));
    }
    let mut tensorizer = NativeFlatTensorizerV2::new();
    let mut tensor = NativeFlatDecisionTensorV2::default();
    tensorizer
        .fill(
            <FlatScoredFamilyV2 as FlatScoredFamilyCore>::packet_view(&packet),
            &mut tensor,
        )
        .map_err(|_| invalid_data_v1("counterfactual native tensorization failed"))?;
    drop(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::into_owned_packet(packet));
    Ok(tensor)
}

fn evaluate_counterfactual_root_v1(
    inference: &NativeXmageCp7OutcomeInferenceV1,
    session: &FastActorSessionV1,
    scored: ScoredCurrentDecisionV1,
    root_ordinal: usize,
    balance_pair_index: usize,
    within_seat_ordinal: usize,
    source_episode_ordinal: usize,
    environment_seed: u64,
    history: &[NativeCounterfactualHistoryDecisionV1],
) -> Result<NativeCounterfactualRootV1, io::Error> {
    let expected = scored.expected;
    let root_actor = expected.acting_player;
    let semantics = session
        .diagnostic_current_action_semantics()
        .ok_or_else(|| invalid_data_v1("counterfactual root action semantics missing"))?;
    if semantics.len() != scored.logits.len() {
        return Err(invalid_data_v1(
            "counterfactual root action semantic width mismatch",
        ));
    }
    let parent_logits_f32_bits: Vec<u32> =
        scored.logits.iter().map(|value| value.to_bits()).collect();
    let parent_argmax_index = argmax_v1(&scored.logits);
    let root_tensor = tensorize_current_decision_v1(session, expected)?;
    let mut action_terminal_rewards =
        vec![Vec::with_capacity(COUNTERFACTUAL_SAMPLES_PER_ACTION_V1); scored.logits.len()];
    let mut samples = Vec::with_capacity(COUNTERFACTUAL_SAMPLES_PER_ACTION_V1);

    for sample_ordinal in 0..COUNTERFACTUAL_SAMPLES_PER_ACTION_V1 {
        let seed = redeterminization_seed_v1(
            COUNTERFACTUAL_REDETERMINIZATION_DOMAIN_V1,
            root_ordinal,
            sample_ordinal,
        );
        let (snapshot, sampled_hash) = prepare_information_set_sample_v1(
            inference,
            session,
            expected,
            &parent_logits_f32_bits,
            seed,
        )?;
        let mut restored = session.clone();
        restored.restore_v1(&snapshot);
        let restored_tensor = tensorize_current_decision_v1(&restored, expected)?;
        let restored_semantics = restored
            .diagnostic_current_action_semantics()
            .ok_or_else(|| invalid_data_v1("counterfactual sampled semantics missing"))?;
        if restored_tensor != root_tensor || restored_semantics != semantics {
            return Err(invalid_data_v1(
                "counterfactual sampled public root identity changed",
            ));
        }

        let mut checked_branch_start_hashes = Vec::with_capacity(scored.logits.len());
        for (action_index, rewards) in action_terminal_rewards.iter_mut().enumerate() {
            let (outcome, checked_hash) = run_information_set_continuation_v1(
                session,
                &snapshot,
                sampled_hash,
                inference,
                root_actor,
                action_index as u32,
                COUNTERFACTUAL_CONTINUATION_POLICY_DOMAIN_V1,
                root_ordinal,
                sample_ordinal,
            );
            let ContinuationOutcomeV1::Natural(reward) = outcome else {
                return Err(invalid_data_v1(
                    "counterfactual branch did not reach a natural terminal",
                ));
            };
            if !(-1..=1).contains(&reward) || checked_hash != Some(sampled_hash) {
                return Err(invalid_data_v1(
                    "counterfactual reward or shared branch-start check failed",
                ));
            }
            rewards.push(reward);
            checked_branch_start_hashes.push(format!("{sampled_hash:016x}"));
        }
        samples.push(NativeCounterfactualSampleV1 {
            sample_ordinal,
            redeterminization_seed_u64_hex: format!("{seed:016x}"),
            sampled_privileged_state_hash_u64_hex: format!("{sampled_hash:016x}"),
            checked_branch_start_hashes_u64_hex: checked_branch_start_hashes,
        });
    }

    if unique_sample_hash_count_v1(samples.iter().map(|sample| {
        u64::from_str_radix(&sample.sampled_privileged_state_hash_u64_hex, 16)
            .expect("collector emitted a valid fixed-width hash")
    })) != COUNTERFACTUAL_SAMPLES_PER_ACTION_V1
    {
        return Err(invalid_data_v1(
            "counterfactual root did not produce four distinct hidden samples",
        ));
    }
    if action_terminal_rewards
        .iter()
        .any(|rewards| rewards.len() != COUNTERFACTUAL_SAMPLES_PER_ACTION_V1)
    {
        return Err(invalid_data_v1(
            "counterfactual action reward matrix is incomplete",
        ));
    }

    let minimum_history_physical = expected.physical_decision_id.saturating_sub(16);
    let public_history = history
        .iter()
        .filter(|row| row.physical_decision_id >= minimum_history_physical)
        .cloned()
        .collect();
    Ok(NativeCounterfactualRootV1 {
        root_ordinal,
        balance_pair_index,
        within_seat_ordinal,
        source_episode_ordinal,
        episode_id: expected.episode_id,
        environment_seed_u64_hex: format!("{environment_seed:016x}"),
        step: expected.step,
        physical_decision_id: expected.physical_decision_id,
        acting_player: root_actor,
        legal_action_count: expected.legal_action_count,
        public_root_tensor: root_tensor.into(),
        action_semantics: semantics,
        parent_logits_f32_bits,
        parent_value_f32_bits: scored.value.to_bits(),
        parent_argmax_index,
        public_history,
        samples,
        action_terminal_rewards,
    })
}

fn counterfactual_report_bytes_v1(
    report: &NativeCounterfactualReportV1,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn native_action_conditioned_counterfactual_report_bytes_v1(
    report: &NativeCounterfactualReportV1,
) -> Result<Vec<u8>, serde_json::Error> {
    counterfactual_report_bytes_v1(report)
}

pub fn run_native_action_conditioned_counterfactual_v1(
    source_outcome_root: impl AsRef<Path>,
) -> Result<NativeCounterfactualEnvelopeV1, Box<dyn Error>> {
    let started = Instant::now();
    let inference = load_xmage_cp7_outcome_inference_v1(source_outcome_root.as_ref())?;
    if lower_hex_raw32_v1(inference.manifest_sha256_v1()) != RETAINED_MANIFEST_SHA256_V1
        || lower_hex_raw32_v1(inference.payload_sha256_v1()) != RETAINED_PAYLOAD_SHA256_V1
        || lower_hex_raw32_v1(inference.native_state_sha256_v1()) != RETAINED_NATIVE_STATE_SHA256_V1
        || lower_hex_raw32_v1(inference.model_parameter_sha256_v1())
            != RETAINED_MODEL_PARAMETER_SHA256_V1
        || inference.adam_step_v1() != RETAINED_ADAM_STEP_V1
    {
        return Err(invalid_data_v1("source is not the exact retained 706b checkpoint").into());
    }
    let source = NativeRolloutTeacherSourceV1 {
        outcome_manifest_sha256: lower_hex_raw32_v1(inference.manifest_sha256_v1()),
        outcome_payload_sha256: lower_hex_raw32_v1(inference.payload_sha256_v1()),
        native_state_sha256: lower_hex_raw32_v1(inference.native_state_sha256_v1()),
        model_parameter_sha256: lower_hex_raw32_v1(inference.model_parameter_sha256_v1()),
        corpus_sha256: lower_hex_raw32_v1(inference.corpus_sha256_v1()),
        adam_step: inference.adam_step_v1(),
    };
    let config = NativeCounterfactualConfigV1 {
        base_seed_u64_hex: format!("{COUNTERFACTUAL_BASE_SEED_V1:016x}"),
        root_count: COUNTERFACTUAL_ROOT_COUNT_V1,
        roots_per_acting_seat: COUNTERFACTUAL_ROOTS_PER_SEAT_V1,
        samples_per_action: COUNTERFACTUAL_SAMPLES_PER_ACTION_V1,
        maximum_source_episodes: COUNTERFACTUAL_MAX_SOURCE_EPISODES_V1,
        root_eligibility:
            "surface, substep_count=1, physical_decision_id>=10, legal_action_count=2..8",
        split:
            "balance_pair_index modulo 4: 1,2 train; 3 selection; 0 heldout; pair is seat-balance-only",
        continuation_policy: "exact-retained-706b-temperature-1-self-play-to-natural-terminal",
        information_scope:
            "acting-player-information-set-deterministic-modulo-fisher-yates-redeterminization/v1",
        target: "actor-relative-natural-terminal-reward-only/v1",
        input_exclusions:
            "no-opponent-private-hand,no-library-order,no-privileged-state,no-rollout-outcome-inputs",
    };

    let mut roots = Vec::with_capacity(COUNTERFACTUAL_ROOT_COUNT_V1);
    let mut roots_by_seat = [0usize; 2];
    let mut source_episodes_examined = 0usize;
    for episode_ordinal in 0..COUNTERFACTUAL_MAX_SOURCE_EPISODES_V1 {
        if roots_by_seat == [COUNTERFACTUAL_ROOTS_PER_SEAT_V1; 2] {
            break;
        }
        source_episodes_examined += 1;
        let episode_id = COUNTERFACTUAL_ROOT_EPISODE_ID_BASE_V1 + episode_ordinal as u64;
        let environment_seed = counterfactual_environment_seed_v1(episode_ordinal);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            SESSION_MAX_PHYSICAL_DECISIONS_V1,
            SESSION_MAX_POLICY_STEPS_V1,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .map_err(|_| invalid_data_v1("counterfactual Rally source session reset failed"))?;
        let mut history = Vec::new();

        for _ in 0..MAX_SOURCE_POLICY_STEPS_V1 {
            let FastActorResponseV1::Decision(_) = session.current_response() else {
                break;
            };
            let scored = score_current_decision_v1(&inference, &session)
                .map_err(|_| invalid_data_v1("counterfactual source policy scoring failed"))?;
            let actor_index = player_index_v1(scored.expected.acting_player);
            if eligible_root_v1(scored.expected)
                && roots_by_seat[actor_index] < COUNTERFACTUAL_ROOTS_PER_SEAT_V1
            {
                let within_seat_ordinal = roots_by_seat[actor_index];
                let root = evaluate_counterfactual_root_v1(
                    &inference,
                    &session,
                    scored,
                    roots.len(),
                    within_seat_ordinal,
                    within_seat_ordinal,
                    episode_ordinal,
                    environment_seed,
                    &history,
                )?;
                roots_by_seat[actor_index] += 1;
                roots.push(root);
                break;
            }

            let selected = sample_policy_v1(
                &scored.logits,
                counterfactual_source_policy_seed_v1(episode_ordinal, scored.expected.step),
            )
            .map_err(|_| invalid_data_v1("counterfactual source policy sampling failed"))?;
            let semantics = session
                .diagnostic_current_action_semantics()
                .ok_or_else(|| invalid_data_v1("counterfactual source semantics missing"))?;
            let selected_semantic = semantics
                .get(selected as usize)
                .cloned()
                .ok_or_else(|| invalid_data_v1("counterfactual selected semantic missing"))?;
            let tensor = tensorize_current_decision_v1(&session, scored.expected)?;
            history.push(NativeCounterfactualHistoryDecisionV1 {
                step: scored.expected.step,
                physical_decision_id: scored.expected.physical_decision_id,
                substep_index: scored.expected.substep_index,
                substep_count: scored.expected.substep_count,
                acting_player: scored.expected.acting_player,
                selected_index: selected,
                selected_semantic,
                parent_logits_f32_bits: scored.logits.iter().map(|value| value.to_bits()).collect(),
                parent_value_f32_bits: scored.value.to_bits(),
                tensor: tensor.into(),
            });
            consume_scored_v1(&mut session, scored, selected)
                .map_err(|_| invalid_data_v1("counterfactual source policy consume failed"))?;
        }
    }

    let legal_actions = roots
        .iter()
        .map(|root| root.legal_action_count as usize)
        .sum::<usize>();
    let natural_terminal_continuations = legal_actions * COUNTERFACTUAL_SAMPLES_PER_ACTION_V1;
    let train_roots = roots
        .iter()
        .filter(|root| matches!(root.balance_pair_index % 4, 1 | 2))
        .count();
    let selection_roots = roots
        .iter()
        .filter(|root| root.balance_pair_index % 4 == 3)
        .count();
    let heldout_roots = roots
        .iter()
        .filter(|root| root.balance_pair_index % 4 == 0)
        .count();
    let source_episode_unique = roots
        .iter()
        .map(|root| root.source_episode_ordinal)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == roots.len();
    let four_distinct = roots.iter().all(|root| {
        root.samples
            .iter()
            .map(|sample| &sample.sampled_privileged_state_hash_u64_hex)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == COUNTERFACTUAL_SAMPLES_PER_ACTION_V1
    });
    let shared = roots.iter().all(|root| {
        root.samples.iter().all(|sample| {
            sample.checked_branch_start_hashes_u64_hex.len() == root.legal_action_count as usize
                && sample
                    .checked_branch_start_hashes_u64_hex
                    .iter()
                    .all(|hash| hash == &sample.sampled_privileged_state_hash_u64_hex)
        })
    });
    let natural = roots.iter().all(|root| {
        root.action_terminal_rewards.len() == root.legal_action_count as usize
            && root.action_terminal_rewards.iter().all(|rewards| {
                rewards.len() == COUNTERFACTUAL_SAMPLES_PER_ACTION_V1
                    && rewards.iter().all(|reward| (-1..=1).contains(reward))
            })
    });
    let collected = roots.len() == COUNTERFACTUAL_ROOT_COUNT_V1;
    let balanced = roots_by_seat == [COUNTERFACTUAL_ROOTS_PER_SEAT_V1; 2];
    let split_exact = train_roots == 128 && selection_roots == 64 && heldout_roots == 64;
    // Every accepted root was compared byte-for-byte at all four sampled
    // observations. Any mismatch returns before a report can be constructed.
    let exact_public_identity = collected;
    let corpus_qualified = collected
        && balanced
        && source_episode_unique
        && four_distinct
        && shared
        && exact_public_identity
        && natural
        && split_exact;
    let aggregate = NativeCounterfactualAggregateV1 {
        source_episodes_examined,
        roots_collected: roots.len(),
        p0_roots: roots_by_seat[0],
        p1_roots: roots_by_seat[1],
        balance_pairs: roots_by_seat[0].min(roots_by_seat[1]),
        train_roots,
        selection_roots,
        heldout_roots,
        legal_actions,
        natural_terminal_continuations,
    };
    let gates = NativeCounterfactualGatesV1 {
        collected_exactly_256_roots: collected,
        exactly_128_roots_per_acting_seat: balanced,
        exactly_one_root_per_source_episode: source_episode_unique,
        every_root_has_four_distinct_information_set_samples: four_distinct,
        every_sample_shared_by_every_legal_action: shared,
        exact_public_root_identity_under_every_sample: exact_public_identity,
        every_branch_reached_natural_terminal: natural,
        split_is_exactly_128_64_64_roots: split_exact,
        corpus_qualified,
    };
    let report = NativeCounterfactualReportV1 {
        schema: NATIVE_ACTION_CONDITIONED_COUNTERFACTUAL_CORPUS_SCHEMA_V1,
        publication_encoding: "serde-json-pretty-utf8-trailing-lf/v1",
        source,
        config,
        roots,
        aggregate,
        gates,
        interpretation: "Mechanism-screen corpus only. Targets are actor-relative natural terminal rewards from shared acting-player information-set redeterminizations and retained-policy continuations. Balance pairs are split groups, not matched trajectories. A corpus pass authorizes only the frozen linear held-out screen.",
    };
    let bytes = counterfactual_report_bytes_v1(&report)?;
    let deterministic_report_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let disposition = if report.gates.corpus_qualified {
        "pass-corpus-proceed-to-frozen-linear-heldout-screen"
    } else {
        "reject-counterfactual-corpus"
    };
    Ok(NativeCounterfactualEnvelopeV1 {
        deterministic_report_sha256,
        elapsed_milliseconds,
        disposition,
        report,
    })
}

#[cfg(test)]
mod information_set_tests {
    use super::*;

    fn sample_v1(hash: &str, branches: usize) -> NativeRolloutTeacherInformationSetSampleV1 {
        NativeRolloutTeacherInformationSetSampleV1 {
            rollout_ordinal: 0,
            redeterminization_seed_u64_hex: "0000000000000001".to_owned(),
            sampled_privileged_state_hash_u64_hex: hash.to_owned(),
            checked_branch_start_hashes_u64_hex: vec![hash.to_owned(); branches],
        }
    }

    #[test]
    fn redeterminization_seed_is_action_independent_and_phase_separated_v1() {
        let ranking = redeterminization_seed_v1(RANKING_REDETERMINIZATION_DOMAIN_V1, 3, 7);
        let same_ranking = redeterminization_seed_v1(RANKING_REDETERMINIZATION_DOMAIN_V1, 3, 7);
        let confirmation = redeterminization_seed_v1(CONFIRM_REDETERMINIZATION_DOMAIN_V1, 3, 7);
        assert_eq!(ranking, same_ranking);
        assert_ne!(ranking, confirmation);
        assert_ne!(
            ranking,
            redeterminization_seed_v1(RANKING_REDETERMINIZATION_DOMAIN_V1, 3, 8)
        );
    }

    #[test]
    fn counterfactual_split_is_pair_grouped_balanced_128_64_64_v1() {
        let roots: Vec<_> = (0..COUNTERFACTUAL_ROOTS_PER_SEAT_V1)
            .flat_map(|pair| [(pair, PlayerSeatV1::P0), (pair, PlayerSeatV1::P1)])
            .collect();
        let train: Vec<_> = roots
            .iter()
            .filter(|(pair, _)| matches!(pair % 4, 1 | 2))
            .collect();
        let selection: Vec<_> = roots.iter().filter(|(pair, _)| pair % 4 == 3).collect();
        let heldout: Vec<_> = roots.iter().filter(|(pair, _)| pair % 4 == 0).collect();
        assert_eq!([train.len(), selection.len(), heldout.len()], [128, 64, 64]);
        for split in [&train, &selection, &heldout] {
            assert_eq!(
                split
                    .iter()
                    .filter(|(_, seat)| *seat == PlayerSeatV1::P0)
                    .count(),
                split.len() / 2
            );
        }
    }

    #[test]
    fn counterfactual_seed_domains_are_distinct_from_prior_screens_v1() {
        let counterfactual =
            redeterminization_seed_v1(COUNTERFACTUAL_REDETERMINIZATION_DOMAIN_V1, 7, 3);
        assert_ne!(
            counterfactual,
            redeterminization_seed_v1(RANKING_REDETERMINIZATION_DOMAIN_V1, 7, 3)
        );
        assert_ne!(
            counterfactual,
            redeterminization_seed_v1(RANK16_RANKING_REDETERMINIZATION_DOMAIN_V1, 7, 3)
        );
        assert_ne!(
            continuation_policy_seed_v1(
                COUNTERFACTUAL_CONTINUATION_POLICY_DOMAIN_V1,
                7,
                3,
                PlayerSeatV1::P1,
                2,
            ),
            continuation_policy_seed_v1(RANK16_RANKING_POLICY_DOMAIN_V1, 7, 3, PlayerSeatV1::P1, 2,)
        );
    }

    #[test]
    fn sample_audit_detects_shared_branch_mismatch_v1() {
        let mut sample = sample_v1("0000000000000001", 3);
        assert!(sample
            .checked_branch_start_hashes_u64_hex
            .iter()
            .all(|hash| hash == &sample.sampled_privileged_state_hash_u64_hex));
        sample.checked_branch_start_hashes_u64_hex[2] = "0000000000000002".to_owned();
        assert!(sample
            .checked_branch_start_hashes_u64_hex
            .iter()
            .any(|hash| hash != &sample.sampled_privileged_state_hash_u64_hex));
    }

    #[test]
    fn distinct_sample_hash_audit_requires_actual_diversity_v1() {
        assert_eq!(unique_sample_hash_count_v1([7_u64, 7, 7].into_iter()), 1);
        assert_eq!(unique_sample_hash_count_v1([7_u64, 8, 7].into_iter()), 2);
    }

    #[test]
    fn verified_branch_start_rejects_tamper_and_failed_continuation_v1() {
        assert_eq!(
            verified_branch_start_hash_v1(7, 7, ContinuationOutcomeV1::Natural(1)),
            Some(7)
        );
        assert_eq!(
            verified_branch_start_hash_v1(8, 7, ContinuationOutcomeV1::Natural(1)),
            None
        );
        assert_eq!(
            verified_branch_start_hash_v1(7, 7, ContinuationOutcomeV1::Failure),
            None
        );
    }

    fn rank16_root_v1(root_ordinal: usize) -> NativeRolloutTeacherInformationSetRootV1 {
        let ranking = (0..2)
            .map(|action_index| NativeRolloutTeacherActionRankingV1 {
                action_index,
                parent_logit_f32_bits: (action_index as f32).to_bits(),
                outcomes: NativeRolloutOutcomeCountsV1 {
                    attempted: RANKING_ROLLOUTS_PER_ACTION_RANK16_V1 as u64,
                    natural: RANKING_ROLLOUTS_PER_ACTION_RANK16_V1 as u64,
                    natural_reward_sum: 0,
                    ..Default::default()
                },
            })
            .collect();
        let ranking_samples: Vec<_> = (0..RANKING_ROLLOUTS_PER_ACTION_RANK16_V1)
            .map(|rollout_ordinal| {
                let hash = format!("{:016x}", 1 + rollout_ordinal % 2);
                NativeRolloutTeacherInformationSetSampleV1 {
                    rollout_ordinal,
                    redeterminization_seed_u64_hex: format!(
                        "{:016x}",
                        redeterminization_seed_v1(
                            RANK16_RANKING_REDETERMINIZATION_DOMAIN_V1,
                            root_ordinal,
                            rollout_ordinal,
                        )
                    ),
                    sampled_privileged_state_hash_u64_hex: hash.clone(),
                    checked_branch_start_hashes_u64_hex: vec![hash; 2],
                }
            })
            .collect();
        let confirmation_samples: Vec<_> = (0..CONFIRMATION_ROLLOUTS_PER_ACTION_V1)
            .map(|rollout_ordinal| {
                let hash = format!("{:016x}", 3 + rollout_ordinal % 2);
                NativeRolloutTeacherInformationSetSampleV1 {
                    rollout_ordinal,
                    redeterminization_seed_u64_hex: format!(
                        "{:016x}",
                        redeterminization_seed_v1(
                            RANK16_CONFIRM_REDETERMINIZATION_DOMAIN_V1,
                            root_ordinal,
                            rollout_ordinal,
                        )
                    ),
                    sampled_privileged_state_hash_u64_hex: hash.clone(),
                    checked_branch_start_hashes_u64_hex: vec![hash; 2],
                }
            })
            .collect();
        NativeRolloutTeacherInformationSetRootV1 {
            rollout: NativeRolloutTeacherRootV1 {
                root_ordinal,
                source_episode_ordinal: root_ordinal,
                episode_id: ROOT_EPISODE_ID_BASE_V1 + root_ordinal as u64,
                environment_seed_u64_hex: "0000000000000000".to_owned(),
                step: 10,
                physical_decision_id: 10,
                acting_player: PlayerSeatV1::P0,
                legal_action_count: 2,
                privileged_state_hash_u64_hex: "0000000000000000".to_owned(),
                action_semantics: Vec::new(),
                parent_logits_f32_bits: vec![0.0_f32.to_bits(), 1.0_f32.to_bits()],
                parent_argmax_index: 1,
                teacher_index: 1,
                teacher_differs_from_parent: false,
                ranking,
                confirmation: NativeRolloutTeacherConfirmationV1 {
                    teacher_outcomes: NativeRolloutOutcomeCountsV1 {
                        attempted: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                        natural: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                        ..Default::default()
                    },
                    parent_argmax_outcomes: NativeRolloutOutcomeCountsV1 {
                        attempted: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                        natural: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u64,
                        ..Default::default()
                    },
                    paired_teacher_better: 0,
                    paired_parent_better: 0,
                    paired_equal: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u32,
                    paired_incomplete: 0,
                    paired_complete: CONFIRMATION_ROLLOUTS_PER_ACTION_V1 as u32,
                    same_action_pair_mismatches: 0,
                    teacher_minus_parent_reward_sum: 0,
                },
            },
            information_set_sampling: NativeRolloutTeacherInformationSetSamplingV1 {
                method: "test-rank16",
                distribution_claim: "test-only",
                ranking_samples,
                confirmation_samples,
                ranking_unique_sampled_state_hashes: 2,
                confirmation_unique_sampled_state_hashes: 2,
                combined_unique_sampled_state_hashes: 4,
            },
        }
    }

    #[test]
    fn rank16_aggregate_uses_exact_ranking_and_confirmation_counts_v1() {
        let mut roots: Vec<_> = (0..ROOT_COUNT_V1).map(rank16_root_v1).collect();
        let (aggregate, gates) = aggregate_information_set_rank16_v1(&roots, ROOT_COUNT_V1);
        assert_eq!(
            aggregate.redeterminization_samples_required_for_collected_roots,
            (ROOT_COUNT_V1
                * (RANKING_ROLLOUTS_PER_ACTION_RANK16_V1 + CONFIRMATION_ROLLOUTS_PER_ACTION_V1))
                as u64
        );
        assert_eq!(
            aggregate.redeterminization_samples_recorded_successfully,
            1_536
        );
        assert_eq!(aggregate.ranking_branch_starts_checked, 1_024);
        assert_eq!(aggregate.confirmation_branch_starts_checked, 2_048);
        assert_eq!(aggregate.rollout.all_outcomes.attempted, 3_072);
        assert_eq!(aggregate.rollout.all_outcomes.natural, 3_072);
        assert_eq!(aggregate.rollout.incomplete_ranking_actions, 0);
        assert!(gates.rollout.all_ranking_rollouts_natural);
        assert!(gates.rollout.all_confirmation_pairs_complete);
        assert!(gates.zero_redeterminization_failures);
        assert!(gates.every_ranking_sample_shared_by_all_actions);
        assert!(gates.every_confirmation_sample_shared_by_both_branches);
        assert!(gates.zero_shared_sample_branch_start_mismatches);

        let moved = roots[0]
            .information_set_sampling
            .ranking_samples
            .pop()
            .unwrap();
        roots[1]
            .information_set_sampling
            .ranking_samples
            .push(moved);
        let (_, malformed_gates) = aggregate_information_set_rank16_v1(&roots, ROOT_COUNT_V1);
        assert!(!malformed_gates.zero_redeterminization_failures);
    }

    #[test]
    fn rank16_seed_schedules_are_action_independent_v1() {
        let schedule = || {
            (0..RANKING_ROLLOUTS_PER_ACTION_RANK16_V1)
                .map(|rollout_ordinal| {
                    (
                        redeterminization_seed_v1(
                            RANK16_RANKING_REDETERMINIZATION_DOMAIN_V1,
                            7,
                            rollout_ordinal,
                        ),
                        continuation_policy_seed_v1(
                            RANK16_RANKING_POLICY_DOMAIN_V1,
                            7,
                            rollout_ordinal,
                            PlayerSeatV1::P0,
                            3,
                        ),
                    )
                })
                .collect::<Vec<_>>()
        };
        let schedules_for_all_legal_actions: Vec<_> = (0..MAX_ROOT_ACTIONS_V1)
            .map(|_action_index| schedule())
            .collect();
        assert_eq!(schedules_for_all_legal_actions.len(), 8);
        assert_eq!(schedules_for_all_legal_actions[0].len(), 16);
        assert!(schedules_for_all_legal_actions
            .windows(2)
            .all(|pair| pair[0] == pair[1]));
        for rollout_ordinal in 0..RANKING_ROLLOUTS_PER_ACTION_RANK16_V1 {
            assert_ne!(
                redeterminization_seed_v1(
                    RANK16_RANKING_REDETERMINIZATION_DOMAIN_V1,
                    7,
                    rollout_ordinal,
                ),
                redeterminization_seed_v1(
                    RANK16_CONFIRM_REDETERMINIZATION_DOMAIN_V1,
                    7,
                    rollout_ordinal,
                )
            );
        }
        for rollout_ordinal in 0..RANKING_ROLLOUTS_PER_ACTION_V1 {
            assert_ne!(
                schedules_for_all_legal_actions[0][rollout_ordinal].0,
                redeterminization_seed_v1(RANKING_REDETERMINIZATION_DOMAIN_V1, 7, rollout_ordinal,)
            );
            assert_ne!(
                schedules_for_all_legal_actions[0][rollout_ordinal].1,
                continuation_policy_seed_v1(
                    RANKING_POLICY_DOMAIN_V1,
                    7,
                    rollout_ordinal,
                    PlayerSeatV1::P0,
                    3,
                )
            );
        }
        for rollout_ordinal in 0..CONFIRMATION_ROLLOUTS_PER_ACTION_V1 {
            assert_ne!(
                redeterminization_seed_v1(
                    RANK16_CONFIRM_REDETERMINIZATION_DOMAIN_V1,
                    7,
                    rollout_ordinal,
                ),
                redeterminization_seed_v1(CONFIRM_REDETERMINIZATION_DOMAIN_V1, 7, rollout_ordinal,)
            );
            assert_ne!(
                continuation_policy_seed_v1(
                    RANK16_CONFIRM_POLICY_DOMAIN_V1,
                    7,
                    rollout_ordinal,
                    PlayerSeatV1::P1,
                    5,
                ),
                continuation_policy_seed_v1(
                    CONFIRM_POLICY_DOMAIN_V1,
                    7,
                    rollout_ordinal,
                    PlayerSeatV1::P1,
                    5,
                )
            );
        }
    }

    #[test]
    #[ignore = "manual external retained-checkpoint root diagnostic"]
    fn diagnose_formal_episode_1470023_information_set_snapshot_v1() {
        let source = std::env::var("MTG_KERNEL_RETAINED_OUTCOME_ROOT")
            .expect("set MTG_KERNEL_RETAINED_OUTCOME_ROOT");
        let inference = load_xmage_cp7_outcome_inference_v1(Path::new(&source)).unwrap();
        let episode_ordinal = 23usize;
        let episode_id = ROOT_EPISODE_ID_BASE_V1 + episode_ordinal as u64;
        let environment_seed = environment_seed_v1(episode_ordinal);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            SESSION_MAX_PHYSICAL_DECISIONS_V1,
            SESSION_MAX_POLICY_STEPS_V1,
            ["Rally".to_owned(), "Rally".to_owned()],
        )
        .unwrap();
        for _ in 0..MAX_SOURCE_POLICY_STEPS_V1 {
            let scored = score_current_decision_v1(&inference, &session).unwrap();
            if eligible_root_v1(scored.expected) {
                assert_eq!(scored.expected.step, 11);
                assert_eq!(scored.expected.physical_decision_id, 10);
                session
                    .snapshot_current_actor_information_set_v1(0xfcf6_ec8e_05a5_a2ee)
                    .unwrap();
                return;
            }
            let selected = sample_policy_v1(
                &scored.logits,
                main_policy_seed_v1(episode_ordinal, scored.expected.step),
            )
            .unwrap();
            consume_scored_v1(&mut session, scored, selected).unwrap();
        }
        panic!("formal episode did not reach the expected eligible root");
    }
}
