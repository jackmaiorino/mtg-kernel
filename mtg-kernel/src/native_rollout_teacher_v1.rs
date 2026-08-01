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
    if logits.len() != expected.legal_action_count as usize
        || logits.is_empty()
        || logits.iter().any(|value| !value.is_finite())
    {
        return Err(());
    }
    drop(<FlatScoredFamilyV2 as FlatScoredFamilyCore>::into_owned_packet(packet));
    Ok(ScoredCurrentDecisionV1 {
        expected,
        binding,
        logits,
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

fn aggregate_v1(
    roots: &[NativeRolloutTeacherRootV1],
    source_episodes_examined: usize,
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
            if action.outcomes.natural != RANKING_ROLLOUTS_PER_ACTION_V1 as u64 {
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
