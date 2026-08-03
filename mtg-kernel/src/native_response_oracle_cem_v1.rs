//! Development-only terminal-outcome response-oracle screen.

use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
use crate::native_checkpoint_runner_v1::{
    run_native_checkpoint_with_ladder_opponent_action_residual_eval_v1,
    run_native_checkpoint_with_ladder_opponent_state_conditional_residual_eval_v1,
    NativeActionKindResidualV1, NativeCheckpointRunResultV1, NativeCheckpointRunnerConfigV1,
    NativeStateConditionalResidualV1, NATIVE_ACTION_KIND_RESIDUAL_DIM_V1,
    NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1,
};
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
use crate::native_ladder_pool_resolution_v1::resolve_ladder_pool_v1;
use crate::native_training_store_resume_v2::{
    load_native_training_boundary_v2, LoadedNativeTrainingBoundaryV2,
};
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::{
    decode_train_run_v2, OpponentLadderPoolContractV1, ValidatedTrainRunV2,
};
use crate::rl::PlayerSeatV1;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_POOL_ROOT_V1: &str = r"D:\mtg-kernel-ladder-pilot-20260725\pool3";
const DEFAULT_EVIDENCE_ROOT_V1: &str = r"D:\mtg-kernel-response-oracle-cem-v1";
const DEFAULT_STATE_CONDITIONAL_EVIDENCE_ROOT_V1: &str =
    r"D:\mtg-kernel-state-conditional-response-oracle-v1";
const CANDIDATE_GENERATION_V1: u64 = 384;

struct OracleAuthoritiesV1 {
    run: ValidatedTrainRunV2,
    boundary: LoadedNativeTrainingBoundaryV2,
    mixture: Arc<LadderOpponentEngineV1>,
    promoted2: Arc<LadderOpponentEngineV1>,
}

#[derive(Clone, Debug, Serialize)]
struct OracleEvalSummaryV1 {
    evaluation_base_seed: u64,
    games: u64,
    wins: u64,
    losses: u64,
    draws: u64,
    terminal_reward_sum: i64,
    seat_wins: [u64; 2],
    seat_losses: [u64; 2],
    seat_draws: [u64; 2],
    trajectory_set_sha256: String,
    elapsed_seconds: f64,
}

#[derive(Debug, Serialize)]
struct ThroughputReportV1 {
    schema: &'static str,
    pool_root: String,
    candidate_generation: u64,
    parameter_count: usize,
    first: OracleEvalSummaryV1,
    repeat: OracleEvalSummaryV1,
    repeat_identical: bool,
    games_per_second: f64,
}

#[derive(Clone, Debug, Serialize)]
struct OracleCandidateReportV1 {
    candidate_index: usize,
    parameters: Vec<f32>,
    fitness: i64,
    l2_squared: f64,
    evaluation: OracleEvalSummaryV1,
}

#[derive(Clone, Debug, Serialize)]
struct OracleGenerationReportV1 {
    generation: usize,
    evaluation_base_seed: u64,
    sigma_before: f64,
    sigma_after: f64,
    candidates: Vec<OracleCandidateReportV1>,
    elite_candidate_indices: Vec<usize>,
    updated_mean: Vec<f32>,
    anchor: OracleEvalSummaryV1,
}

#[derive(Clone, Debug, Serialize)]
struct PairedComparisonV1 {
    gains: u64,
    losses: u64,
    ties: u64,
    candidate_minus_parent_seat_wins: [i64; 2],
}

#[derive(Clone, Debug, Serialize)]
struct OracleFreshPanelV1 {
    opponent: &'static str,
    parent: OracleEvalSummaryV1,
    candidate: OracleEvalSummaryV1,
    paired: PairedComparisonV1,
}

#[derive(Clone, Debug, Serialize)]
struct OracleFormalGatesV1 {
    pool3_paired_gain: bool,
    promoted2_paired_gain: bool,
    pool3_seat_floor: bool,
    promoted2_seat_floor: bool,
    pass: bool,
}

#[derive(Debug, Serialize)]
struct OracleFormalReportV1 {
    schema: &'static str,
    pool_root: String,
    candidate_generation: u64,
    parameter_count: usize,
    optimizer: &'static str,
    population: usize,
    elite_count: usize,
    generations: usize,
    games_per_candidate: u64,
    anchor_games: u64,
    fresh_panel_games: u64,
    rng_seed: u64,
    initial_sigma: f64,
    development_parent: OracleEvalSummaryV1,
    generation_reports: Vec<OracleGenerationReportV1>,
    selected_parameters: Vec<f32>,
    selected_anchor: OracleEvalSummaryV1,
    fresh_pool3: OracleFreshPanelV1,
    fresh_promoted2: OracleFreshPanelV1,
    gates: OracleFormalGatesV1,
    elapsed_seconds: f64,
}

#[derive(Debug, Serialize)]
struct OracleProgressV1 {
    schema: &'static str,
    completed_generations: usize,
    total_generations: usize,
    latest_anchor_fitness: i64,
    best_anchor_fitness: i64,
    sigma: f64,
    elapsed_seconds: f64,
}

#[derive(Debug, Serialize)]
struct StateConditionalThroughputReportV1 {
    schema: &'static str,
    pool_root: String,
    candidate_generation: u64,
    parameter_count: usize,
    first: OracleEvalSummaryV1,
    repeat: OracleEvalSummaryV1,
    probe: OracleEvalSummaryV1,
    repeat_identical: bool,
    probe_trajectory_differs: bool,
    games_per_second: f64,
}

#[derive(Clone, Debug, Serialize)]
struct StateConditionalCandidateReportV1 {
    candidate_index: usize,
    parameters: Vec<f32>,
    fitness: i64,
    l2_squared: f64,
    evaluation: OracleEvalSummaryV1,
}

#[derive(Clone, Debug, Serialize)]
struct StateConditionalGenerationReportV1 {
    generation: usize,
    evaluation_base_seed: u64,
    sigma_before: f64,
    sigma_after: f64,
    candidates: Vec<StateConditionalCandidateReportV1>,
    elite_candidate_indices: Vec<usize>,
    updated_mean: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
struct StateConditionalSelectorReportV1 {
    policy_index: usize,
    parameters: Vec<f32>,
    l2_squared: f64,
    panel_a: OracleEvalSummaryV1,
    panel_b: OracleEvalSummaryV1,
    worst_fitness: i64,
    summed_fitness: i64,
}

#[derive(Debug, Serialize)]
struct StateConditionalProgressV1 {
    schema: &'static str,
    completed_generations: usize,
    total_generations: usize,
    latest_generation_best_fitness: i64,
    sigma: f64,
    elapsed_seconds: f64,
}

#[derive(Debug, Serialize)]
struct StateConditionalFormalReportV1 {
    schema: &'static str,
    pool_root: String,
    candidate_generation: u64,
    parameter_count: usize,
    optimizer: &'static str,
    population: usize,
    elite_count: usize,
    generations: usize,
    games_per_candidate: u64,
    selector_games_per_panel: u64,
    fresh_panel_games: u64,
    rng_seed: u64,
    initial_sigma: f64,
    generation_reports: Vec<StateConditionalGenerationReportV1>,
    selector_reports: Vec<StateConditionalSelectorReportV1>,
    selected_policy_index: usize,
    selected_parameters: Vec<f32>,
    fresh_pool3: OracleFreshPanelV1,
    fresh_promoted2: OracleFreshPanelV1,
    gates: OracleFormalGatesV1,
    elapsed_seconds: f64,
}

struct SplitMix64V1 {
    state: u64,
}

impl SplitMix64V1 {
    const fn new_v1(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64_v1(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    /// Irwin-Hall normal approximation. This avoids platform libm calls while
    /// retaining deterministic, symmetric antithetic search directions.
    fn normal_approx_v1(&mut self) -> f64 {
        let mut sum = 0.0_f64;
        for _ in 0..12 {
            sum += self.next_u64_v1() as f64 / 18_446_744_073_709_551_616.0_f64;
        }
        sum - 6.0
    }
}

fn required_or_default_v1(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn lower_hex_v1(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[(byte >> 4) as usize]));
        output.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    output
}

fn load_authorities_v1(pool_root: &Path) -> OracleAuthoritiesV1 {
    let pool_bytes = fs::read(pool_root.join("pool.json")).expect("Pool3 pool.json must read");
    let pool: OpponentLadderPoolContractV1 =
        serde_json::from_slice(&pool_bytes).expect("Pool3 pool.json must decode");
    let primary_root = pool_root.join("primary");
    let predecessor_a_root = pool_root.join("pred-a");
    let predecessor_b_root = pool_root.join("pred-b");

    let run_bytes = fs::read(primary_root.join("run.json")).expect("primary run.json must read");
    let run = decode_train_run_v2(&run_bytes).expect("primary run.json must validate");
    let root = ValidatedNativeTrainingStoreRootV2::open_v2(&primary_root)
        .expect("primary Store root must open");
    let boundary = load_native_training_boundary_v2(&root, &run, CANDIDATE_GENERATION_V1)
        .expect("promoted2 generation 384 must validate");

    let (primary, predecessor_a, predecessor_b) = resolve_ladder_pool_v1(
        &pool,
        &primary_root,
        &predecessor_a_root,
        &predecessor_b_root,
    )
    .expect("Pool3 policy members must resolve");
    let mixture = Arc::new(
        LadderOpponentEngineV1::new_v1(pool, primary, predecessor_a, predecessor_b)
            .expect("Pool3 engine must construct"),
    );

    let primary =
        load_native_checkpoint_inference_v1(&run, boundary.checkpoint(), boundary.payload())
            .expect("promoted2 primary handle must load");
    let predecessor_a =
        load_native_checkpoint_inference_v1(&run, boundary.checkpoint(), boundary.payload())
            .expect("promoted2 duplicate handle A must load");
    let predecessor_b =
        load_native_checkpoint_inference_v1(&run, boundary.checkpoint(), boundary.payload())
            .expect("promoted2 duplicate handle B must load");
    let promoted2 = Arc::new(LadderOpponentEngineV1::head_to_head_eval_v1(
        primary,
        predecessor_a,
        predecessor_b,
    ));

    OracleAuthoritiesV1 {
        run,
        boundary,
        mixture,
        promoted2,
    }
}

fn summarize_v1(
    result: &NativeCheckpointRunResultV1,
    evaluation_base_seed: u64,
    elapsed_seconds: f64,
) -> OracleEvalSummaryV1 {
    let episodes = &result.rollout().episodes;
    let bindings = result.episode_bindings();
    assert_eq!(episodes.len(), bindings.len());
    let mut wins = 0_u64;
    let mut losses = 0_u64;
    let mut draws = 0_u64;
    let mut terminal_reward_sum = 0_i64;
    let mut seat_wins = [0_u64; 2];
    let mut seat_losses = [0_u64; 2];
    let mut seat_draws = [0_u64; 2];
    let mut trajectory_hasher = Sha256::new();
    trajectory_hasher.update(b"mtg-kernel-response-oracle-trajectory-set-v1\0");
    for (episode, binding) in episodes.iter().zip(bindings) {
        let seat = match binding.learner_seat() {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        };
        let reward = episode.terminal.terminal_reward[seat];
        terminal_reward_sum += i64::from(reward);
        match reward {
            1 => {
                wins += 1;
                seat_wins[seat] += 1;
            }
            -1 => {
                losses += 1;
                seat_losses[seat] += 1;
            }
            0 => {
                draws += 1;
                seat_draws[seat] += 1;
            }
            other => panic!("unexpected terminal reward {other}"),
        }
        trajectory_hasher.update(binding.episode_index().to_be_bytes());
        trajectory_hasher.update(binding.trajectory_sha256());
    }
    OracleEvalSummaryV1 {
        evaluation_base_seed,
        games: episodes.len() as u64,
        wins,
        losses,
        draws,
        terminal_reward_sum,
        seat_wins,
        seat_losses,
        seat_draws,
        trajectory_set_sha256: lower_hex_v1(trajectory_hasher.finalize().into()),
        elapsed_seconds,
    }
}

fn evaluate_v1(
    authorities: &OracleAuthoritiesV1,
    opponent: Arc<LadderOpponentEngineV1>,
    parameters: [f32; NATIVE_ACTION_KIND_RESIDUAL_DIM_V1],
    evaluation_base_seed: u64,
    games: u64,
) -> (NativeCheckpointRunResultV1, OracleEvalSummaryV1) {
    let residual = NativeActionKindResidualV1::new_v1(parameters)
        .expect("response-oracle parameters must be finite and bounded");
    let config = NativeCheckpointRunnerConfigV1 {
        evaluation_base_seed,
        first_episode_index: 0,
        episode_count: games,
        scheduler_timeout: Duration::from_secs(3_600),
        measure_broker_service_time: false,
    };
    let started = Instant::now();
    let result = run_native_checkpoint_with_ladder_opponent_action_residual_eval_v1(
        &authorities.run,
        authorities.boundary.checkpoint(),
        authorities.boundary.payload(),
        config,
        Some(opponent),
        residual,
    )
    .expect("response-oracle evaluation must complete");
    let summary = summarize_v1(
        &result,
        evaluation_base_seed,
        started.elapsed().as_secs_f64(),
    );
    (result, summary)
}

fn evaluate_state_conditional_v1(
    authorities: &OracleAuthoritiesV1,
    opponent: Arc<LadderOpponentEngineV1>,
    parameters: [f32; NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1],
    evaluation_base_seed: u64,
    games: u64,
) -> (NativeCheckpointRunResultV1, OracleEvalSummaryV1) {
    let residual = NativeStateConditionalResidualV1::new_v1(parameters)
        .expect("state-conditional response-oracle parameters must be finite and bounded");
    let config = NativeCheckpointRunnerConfigV1 {
        evaluation_base_seed,
        first_episode_index: 0,
        episode_count: games,
        scheduler_timeout: Duration::from_secs(3_600),
        measure_broker_service_time: false,
    };
    let started = Instant::now();
    let result = run_native_checkpoint_with_ladder_opponent_state_conditional_residual_eval_v1(
        &authorities.run,
        authorities.boundary.checkpoint(),
        authorities.boundary.payload(),
        config,
        Some(opponent),
        residual,
    )
    .expect("state-conditional response-oracle evaluation must complete");
    let summary = summarize_v1(
        &result,
        evaluation_base_seed,
        started.elapsed().as_secs_f64(),
    );
    (result, summary)
}

fn fitness_v1(summary: &OracleEvalSummaryV1) -> i64 {
    let seat_net = [
        summary.seat_wins[0] as i64 - summary.seat_losses[0] as i64,
        summary.seat_wins[1] as i64 - summary.seat_losses[1] as i64,
    ];
    2 * summary.terminal_reward_sum + seat_net[0].min(seat_net[1])
}

fn l2_squared_v1(parameters: &[f32; NATIVE_ACTION_KIND_RESIDUAL_DIM_V1]) -> f64 {
    parameters
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum()
}

fn slice_l2_squared_v1(parameters: &[f32]) -> f64 {
    parameters
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum()
}

fn candidate_is_better_v1(
    left_fitness: i64,
    left_l2: f64,
    right_fitness: i64,
    right_l2: f64,
) -> bool {
    left_fitness > right_fitness || (left_fitness == right_fitness && left_l2 < right_l2)
}

fn paired_comparison_v1(
    parent: &NativeCheckpointRunResultV1,
    candidate: &NativeCheckpointRunResultV1,
    parent_summary: &OracleEvalSummaryV1,
    candidate_summary: &OracleEvalSummaryV1,
) -> PairedComparisonV1 {
    assert_eq!(
        parent.rollout().episodes.len(),
        candidate.rollout().episodes.len()
    );
    assert_eq!(
        parent.episode_bindings().len(),
        candidate.episode_bindings().len()
    );
    for (parent_binding, candidate_binding) in parent
        .episode_bindings()
        .iter()
        .zip(candidate.episode_bindings())
    {
        assert_eq!(
            parent_binding.episode_index(),
            candidate_binding.episode_index()
        );
        assert_eq!(
            parent_binding.environment_seed(),
            candidate_binding.environment_seed()
        );
        assert_eq!(
            parent_binding.learner_seat(),
            candidate_binding.learner_seat()
        );
        assert_eq!(
            parent_binding.deck_hashes(),
            candidate_binding.deck_hashes()
        );
    }
    let mut gains = 0_u64;
    let mut losses = 0_u64;
    let mut ties = 0_u64;
    for ((parent_episode, candidate_episode), binding) in parent
        .rollout()
        .episodes
        .iter()
        .zip(&candidate.rollout().episodes)
        .zip(parent.episode_bindings())
    {
        let seat = match binding.learner_seat() {
            PlayerSeatV1::P0 => 0,
            PlayerSeatV1::P1 => 1,
        };
        let parent_reward = parent_episode.terminal.terminal_reward[seat];
        let candidate_reward = candidate_episode.terminal.terminal_reward[seat];
        match candidate_reward.cmp(&parent_reward) {
            std::cmp::Ordering::Greater => gains += 1,
            std::cmp::Ordering::Less => losses += 1,
            std::cmp::Ordering::Equal => ties += 1,
        }
    }
    PairedComparisonV1 {
        gains,
        losses,
        ties,
        candidate_minus_parent_seat_wins: [
            candidate_summary.seat_wins[0] as i64 - parent_summary.seat_wins[0] as i64,
            candidate_summary.seat_wins[1] as i64 - parent_summary.seat_wins[1] as i64,
        ],
    }
}

fn write_json_v1(path: &Path, value: &impl Serialize) {
    let bytes = serde_json::to_vec_pretty(value).expect("report must encode");
    fs::write(path, bytes).expect("report must write");
}

#[test]
fn deterministic_antithetic_rng_is_symmetric_and_repeatable() {
    let mut first = SplitMix64V1::new_v1(2_026_080_3);
    let mut second = SplitMix64V1::new_v1(2_026_080_3);
    for _ in 0..128 {
        assert_eq!(
            first.normal_approx_v1().to_bits(),
            second.normal_approx_v1().to_bits()
        );
    }
}

#[test]
#[ignore = "bounded response-oracle throughput screen, run explicitly"]
fn response_oracle_throughput_v1() {
    let pool_root = PathBuf::from(required_or_default_v1(
        "RESPONSE_ORACLE_POOL_ROOT",
        DEFAULT_POOL_ROOT_V1,
    ));
    let evidence_root = PathBuf::from(required_or_default_v1(
        "RESPONSE_ORACLE_EVIDENCE_ROOT",
        DEFAULT_EVIDENCE_ROOT_V1,
    ));
    let games = std::env::var("RESPONSE_ORACLE_GAMES")
        .unwrap_or_else(|_| "64".to_owned())
        .parse::<u64>()
        .expect("RESPONSE_ORACLE_GAMES must be u64");
    assert!(games > 0 && games % 2 == 0);
    fs::create_dir_all(&evidence_root).expect("evidence root must create");
    let authorities = load_authorities_v1(&pool_root);
    let zero = [0.0_f32; NATIVE_ACTION_KIND_RESIDUAL_DIM_V1];
    let (first_result, first) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        1_270_001,
        games,
    );
    let (repeat_result, repeat) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        1_270_001,
        games,
    );
    let repeat_identical = first_result.rollout().episodes == repeat_result.rollout().episodes
        && first_result.episode_bindings() == repeat_result.episode_bindings();
    assert!(
        repeat_identical,
        "the repeated native panel must be identical"
    );
    let games_per_second =
        (first.games + repeat.games) as f64 / (first.elapsed_seconds + repeat.elapsed_seconds);
    let report = ThroughputReportV1 {
        schema: "mtg-kernel-response-oracle-throughput/v1",
        pool_root: pool_root.display().to_string(),
        candidate_generation: CANDIDATE_GENERATION_V1,
        parameter_count: NATIVE_ACTION_KIND_RESIDUAL_DIM_V1,
        first,
        repeat,
        repeat_identical,
        games_per_second,
    };
    write_json_v1(&evidence_root.join("throughput.json"), &report);
    println!("{}", serde_json::to_string(&report).unwrap());

    // Keep the pure promoted2 authority live in this preflight so the formal
    // target panel cannot discover a loader or identity failure after search.
    assert_eq!(authorities.promoted2.pool().size, 4);
}

#[test]
#[ignore = "bounded state-conditional response-oracle throughput screen, run explicitly"]
fn state_conditional_response_oracle_throughput_v1() {
    let pool_root = PathBuf::from(required_or_default_v1(
        "STATE_RESPONSE_ORACLE_POOL_ROOT",
        DEFAULT_POOL_ROOT_V1,
    ));
    let evidence_root = PathBuf::from(required_or_default_v1(
        "STATE_RESPONSE_ORACLE_EVIDENCE_ROOT",
        DEFAULT_STATE_CONDITIONAL_EVIDENCE_ROOT_V1,
    ));
    let games = std::env::var("STATE_RESPONSE_ORACLE_GAMES")
        .unwrap_or_else(|_| "16".to_owned())
        .parse::<u64>()
        .expect("STATE_RESPONSE_ORACLE_GAMES must be u64");
    assert!(games > 0 && games % 2 == 0);
    fs::create_dir_all(&evidence_root).expect("evidence root must create");
    let authorities = load_authorities_v1(&pool_root);
    let zero = [0.0_f32; NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1];
    let (first_result, first) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        1_300_001,
        games,
    );
    let (repeat_result, repeat) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        1_300_001,
        games,
    );
    let repeat_identical = first_result.rollout().episodes == repeat_result.rollout().episodes
        && first_result.episode_bindings() == repeat_result.episode_bindings();
    assert!(repeat_identical, "repeated native panel must be identical");

    let mut probe_parameters = zero;
    probe_parameters[0] = -0.20;
    probe_parameters[2 * 8] = 0.20;
    probe_parameters[14 * 8] = 0.20;
    probe_parameters[15 * 8] = 0.20;
    let (_, probe) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        probe_parameters,
        1_300_001,
        games,
    );
    let probe_trajectory_differs = probe.trajectory_set_sha256 != first.trajectory_set_sha256;
    assert!(
        probe_trajectory_differs,
        "semantic probe must activate on a real native panel"
    );
    let games_per_second = (first.games + repeat.games + probe.games) as f64
        / (first.elapsed_seconds + repeat.elapsed_seconds + probe.elapsed_seconds);
    let report = StateConditionalThroughputReportV1 {
        schema: "mtg-kernel-state-conditional-response-oracle-throughput/v1",
        pool_root: pool_root.display().to_string(),
        candidate_generation: CANDIDATE_GENERATION_V1,
        parameter_count: NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1,
        first,
        repeat,
        probe,
        repeat_identical,
        probe_trajectory_differs,
        games_per_second,
    };
    write_json_v1(&evidence_root.join("throughput.json"), &report);
    println!("{}", serde_json::to_string(&report).unwrap());
    assert_eq!(authorities.promoted2.pool().size, 4);
}

#[test]
#[ignore = "formal bounded terminal-outcome response-oracle screen, run explicitly"]
fn response_oracle_cem_formal_v1() {
    const POPULATION: usize = 20;
    const ELITE_COUNT: usize = 5;
    const GENERATIONS: usize = 5;
    const GAMES_PER_CANDIDATE: u64 = 128;
    const ANCHOR_GAMES: u64 = 256;
    const FRESH_PANEL_GAMES: u64 = 512;
    const RNG_SEED: u64 = 20_260_803;
    const INITIAL_SIGMA: f64 = 0.35;
    const MINIMUM_SIGMA: f64 = 0.08;
    const MAXIMUM_SIGMA: f64 = 0.50;
    const PARAMETER_ABS_CAP: f64 = 1.50;
    const DEVELOPMENT_SEED_BASE: u64 = 1_281_001;
    const DEVELOPMENT_SEED_STRIDE: u64 = 10_000;
    const ANCHOR_SEED: u64 = 1_289_001;
    const FRESH_POOL3_SEED: u64 = 1_290_001;
    const FRESH_PROMOTED2_SEED: u64 = 1_291_001;

    let started = Instant::now();
    let pool_root = PathBuf::from(required_or_default_v1(
        "RESPONSE_ORACLE_POOL_ROOT",
        DEFAULT_POOL_ROOT_V1,
    ));
    let evidence_root = PathBuf::from(required_or_default_v1(
        "RESPONSE_ORACLE_EVIDENCE_ROOT",
        DEFAULT_EVIDENCE_ROOT_V1,
    ));
    fs::create_dir_all(&evidence_root).expect("evidence root must create");
    let formal_path = evidence_root.join("formal.json");
    assert!(!formal_path.exists(), "formal report path must be fresh");
    let authorities = load_authorities_v1(&pool_root);
    let zero = [0.0_f32; NATIVE_ACTION_KIND_RESIDUAL_DIM_V1];
    let (_, development_parent) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        ANCHOR_SEED,
        ANCHOR_GAMES,
    );

    let mut rng = SplitMix64V1::new_v1(RNG_SEED);
    let mut mean = zero;
    let mut sigma = INITIAL_SIGMA;
    let mut selected_parameters = zero;
    let mut selected_anchor = development_parent.clone();
    let mut selected_fitness = fitness_v1(&selected_anchor);
    let mut selected_l2 = 0.0_f64;
    let mut generation_reports = Vec::with_capacity(GENERATIONS);

    for generation in 0..GENERATIONS {
        let generation_seed =
            DEVELOPMENT_SEED_BASE + u64::try_from(generation).unwrap() * DEVELOPMENT_SEED_STRIDE;
        let sigma_before = sigma;
        let mut candidate_parameters = Vec::with_capacity(POPULATION);
        for _ in 0..(POPULATION / 2) {
            let mut positive = mean;
            let mut negative = mean;
            for parameter_index in 0..NATIVE_ACTION_KIND_RESIDUAL_DIM_V1 {
                let direction = rng.normal_approx_v1();
                positive[parameter_index] = (f64::from(mean[parameter_index]) + sigma * direction)
                    .clamp(-PARAMETER_ABS_CAP, PARAMETER_ABS_CAP)
                    as f32;
                negative[parameter_index] = (f64::from(mean[parameter_index]) - sigma * direction)
                    .clamp(-PARAMETER_ABS_CAP, PARAMETER_ABS_CAP)
                    as f32;
            }
            candidate_parameters.push(positive);
            candidate_parameters.push(negative);
        }
        assert_eq!(candidate_parameters.len(), POPULATION);

        let mut candidates = Vec::with_capacity(POPULATION);
        for (candidate_index, parameters) in candidate_parameters.into_iter().enumerate() {
            let (_, evaluation) = evaluate_v1(
                &authorities,
                Arc::clone(&authorities.mixture),
                parameters,
                generation_seed,
                GAMES_PER_CANDIDATE,
            );
            candidates.push(OracleCandidateReportV1 {
                candidate_index,
                parameters: parameters.to_vec(),
                fitness: fitness_v1(&evaluation),
                l2_squared: l2_squared_v1(&parameters),
                evaluation,
            });
        }
        let mut order: Vec<usize> = (0..POPULATION).collect();
        order.sort_by(|left, right| {
            candidates[*right]
                .fitness
                .cmp(&candidates[*left].fitness)
                .then_with(|| {
                    candidates[*left]
                        .l2_squared
                        .total_cmp(&candidates[*right].l2_squared)
                })
                .then_with(|| left.cmp(right))
        });
        let elite_indices = order[..ELITE_COUNT].to_vec();
        let weight_total = (ELITE_COUNT * (ELITE_COUNT + 1) / 2) as f64;
        let mut updated_mean = [0.0_f32; NATIVE_ACTION_KIND_RESIDUAL_DIM_V1];
        for (rank, &candidate_index) in elite_indices.iter().enumerate() {
            let weight = (ELITE_COUNT - rank) as f64 / weight_total;
            for (destination, source) in updated_mean
                .iter_mut()
                .zip(candidates[candidate_index].parameters.iter().copied())
            {
                *destination += (weight * f64::from(source)) as f32;
            }
        }
        let mut variance_sum = 0.0_f64;
        for &candidate_index in &elite_indices {
            for (value, center) in candidates[candidate_index]
                .parameters
                .iter()
                .zip(updated_mean)
            {
                variance_sum += (f64::from(*value) - f64::from(center)).powi(2);
            }
        }
        let measured_sigma =
            (variance_sum / (ELITE_COUNT * NATIVE_ACTION_KIND_RESIDUAL_DIM_V1) as f64).sqrt();
        sigma = (0.70 * sigma + 0.30 * measured_sigma).clamp(MINIMUM_SIGMA, MAXIMUM_SIGMA);
        mean = updated_mean;

        let (_, anchor) = evaluate_v1(
            &authorities,
            Arc::clone(&authorities.mixture),
            mean,
            ANCHOR_SEED,
            ANCHOR_GAMES,
        );
        let anchor_fitness = fitness_v1(&anchor);
        let anchor_l2 = l2_squared_v1(&mean);
        if candidate_is_better_v1(anchor_fitness, anchor_l2, selected_fitness, selected_l2) {
            selected_parameters = mean;
            selected_anchor = anchor.clone();
            selected_fitness = anchor_fitness;
            selected_l2 = anchor_l2;
        }
        generation_reports.push(OracleGenerationReportV1 {
            generation,
            evaluation_base_seed: generation_seed,
            sigma_before,
            sigma_after: sigma,
            candidates,
            elite_candidate_indices: elite_indices,
            updated_mean: mean.to_vec(),
            anchor,
        });
        write_json_v1(
            &evidence_root.join("progress.json"),
            &OracleProgressV1 {
                schema: "mtg-kernel-response-oracle-cem-progress/v1",
                completed_generations: generation + 1,
                total_generations: GENERATIONS,
                latest_anchor_fitness: anchor_fitness,
                best_anchor_fitness: selected_fitness,
                sigma,
                elapsed_seconds: started.elapsed().as_secs_f64(),
            },
        );
        println!(
            "RESPONSE_ORACLE generation={}/{} anchor_fitness={} best_anchor_fitness={} sigma={:.6}",
            generation + 1,
            GENERATIONS,
            anchor_fitness,
            selected_fitness,
            sigma
        );
    }

    let (pool3_parent_result, pool3_parent) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        FRESH_POOL3_SEED,
        FRESH_PANEL_GAMES,
    );
    let (pool3_candidate_result, pool3_candidate) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        selected_parameters,
        FRESH_POOL3_SEED,
        FRESH_PANEL_GAMES,
    );
    let pool3_paired = paired_comparison_v1(
        &pool3_parent_result,
        &pool3_candidate_result,
        &pool3_parent,
        &pool3_candidate,
    );
    let fresh_pool3 = OracleFreshPanelV1 {
        opponent: "Pool3-40-20-20-20",
        parent: pool3_parent,
        candidate: pool3_candidate,
        paired: pool3_paired,
    };

    let (promoted2_parent_result, promoted2_parent) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.promoted2),
        zero,
        FRESH_PROMOTED2_SEED,
        FRESH_PANEL_GAMES,
    );
    let (promoted2_candidate_result, promoted2_candidate) = evaluate_v1(
        &authorities,
        Arc::clone(&authorities.promoted2),
        selected_parameters,
        FRESH_PROMOTED2_SEED,
        FRESH_PANEL_GAMES,
    );
    let promoted2_paired = paired_comparison_v1(
        &promoted2_parent_result,
        &promoted2_candidate_result,
        &promoted2_parent,
        &promoted2_candidate,
    );
    let fresh_promoted2 = OracleFreshPanelV1 {
        opponent: "promoted2-generation384",
        parent: promoted2_parent,
        candidate: promoted2_candidate,
        paired: promoted2_paired,
    };

    let pool3_paired_gain =
        fresh_pool3.paired.gains >= fresh_pool3.paired.losses.saturating_add(12);
    let promoted2_paired_gain =
        fresh_promoted2.paired.gains >= fresh_promoted2.paired.losses.saturating_add(12);
    let pool3_seat_floor = fresh_pool3
        .paired
        .candidate_minus_parent_seat_wins
        .iter()
        .all(|value| *value >= -4);
    let promoted2_seat_floor = fresh_promoted2
        .paired
        .candidate_minus_parent_seat_wins
        .iter()
        .all(|value| *value >= -4);
    let gates = OracleFormalGatesV1 {
        pool3_paired_gain,
        promoted2_paired_gain,
        pool3_seat_floor,
        promoted2_seat_floor,
        pass: pool3_paired_gain
            && promoted2_paired_gain
            && pool3_seat_floor
            && promoted2_seat_floor,
    };
    let report = OracleFormalReportV1 {
        schema: "mtg-kernel-response-oracle-cem-formal/v1",
        pool_root: pool_root.display().to_string(),
        candidate_generation: CANDIDATE_GENERATION_V1,
        parameter_count: NATIVE_ACTION_KIND_RESIDUAL_DIM_V1,
        optimizer: "five-generation antithetic cross-entropy method; rank-weighted elite mean; terminal reward only",
        population: POPULATION,
        elite_count: ELITE_COUNT,
        generations: GENERATIONS,
        games_per_candidate: GAMES_PER_CANDIDATE,
        anchor_games: ANCHOR_GAMES,
        fresh_panel_games: FRESH_PANEL_GAMES,
        rng_seed: RNG_SEED,
        initial_sigma: INITIAL_SIGMA,
        development_parent,
        generation_reports,
        selected_parameters: selected_parameters.to_vec(),
        selected_anchor,
        fresh_pool3,
        fresh_promoted2,
        gates,
        elapsed_seconds: started.elapsed().as_secs_f64(),
    };
    write_json_v1(&formal_path, &report);
    println!("{}", serde_json::to_string(&report.gates).unwrap());
}

#[test]
#[ignore = "formal bounded state-conditional terminal-outcome oracle, run explicitly"]
fn state_conditional_response_oracle_cem_formal_v1() {
    const POPULATION: usize = 40;
    const ELITE_COUNT: usize = 10;
    const GENERATIONS: usize = 8;
    const GAMES_PER_CANDIDATE: u64 = 96;
    const SELECTOR_GAMES_PER_PANEL: u64 = 256;
    const FRESH_PANEL_GAMES: u64 = 512;
    const RNG_SEED: u64 = 202_608_031;
    const INITIAL_SIGMA: f64 = 0.12;
    const MINIMUM_SIGMA: f64 = 0.04;
    const MAXIMUM_SIGMA: f64 = 0.20;
    const PARAMETER_ABS_CAP: f64 = 0.50;
    const DEVELOPMENT_SEED_BASE: u64 = 1_301_001;
    const DEVELOPMENT_SEED_STRIDE: u64 = 10_000;
    const SELECTOR_A_SEED: u64 = 1_390_001;
    const SELECTOR_B_SEED: u64 = 1_391_001;
    const FRESH_POOL3_SEED: u64 = 1_392_001;
    const FRESH_PROMOTED2_SEED: u64 = 1_393_001;

    let started = Instant::now();
    let pool_root = PathBuf::from(required_or_default_v1(
        "STATE_RESPONSE_ORACLE_POOL_ROOT",
        DEFAULT_POOL_ROOT_V1,
    ));
    let evidence_root = PathBuf::from(required_or_default_v1(
        "STATE_RESPONSE_ORACLE_EVIDENCE_ROOT",
        DEFAULT_STATE_CONDITIONAL_EVIDENCE_ROOT_V1,
    ));
    fs::create_dir_all(&evidence_root).expect("evidence root must create");
    let formal_path = evidence_root.join("formal.json");
    assert!(!formal_path.exists(), "formal report path must be fresh");
    let authorities = load_authorities_v1(&pool_root);
    let zero = [0.0_f32; NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1];

    let mut rng = SplitMix64V1::new_v1(RNG_SEED);
    let mut mean = zero;
    let mut sigma = INITIAL_SIGMA;
    let mut generation_means = Vec::with_capacity(GENERATIONS);
    let mut generation_reports = Vec::with_capacity(GENERATIONS);

    for generation in 0..GENERATIONS {
        let generation_seed =
            DEVELOPMENT_SEED_BASE + u64::try_from(generation).unwrap() * DEVELOPMENT_SEED_STRIDE;
        let sigma_before = sigma;
        let mut candidate_parameters = Vec::with_capacity(POPULATION);
        for _ in 0..(POPULATION / 2) {
            let mut positive = mean;
            let mut negative = mean;
            for parameter_index in 0..NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1 {
                let direction = rng.normal_approx_v1();
                positive[parameter_index] = (f64::from(mean[parameter_index]) + sigma * direction)
                    .clamp(-PARAMETER_ABS_CAP, PARAMETER_ABS_CAP)
                    as f32;
                negative[parameter_index] = (f64::from(mean[parameter_index]) - sigma * direction)
                    .clamp(-PARAMETER_ABS_CAP, PARAMETER_ABS_CAP)
                    as f32;
            }
            candidate_parameters.push(positive);
            candidate_parameters.push(negative);
        }
        assert_eq!(candidate_parameters.len(), POPULATION);

        let mut candidates = Vec::with_capacity(POPULATION);
        for (candidate_index, parameters) in candidate_parameters.into_iter().enumerate() {
            let (_, evaluation) = evaluate_state_conditional_v1(
                &authorities,
                Arc::clone(&authorities.mixture),
                parameters,
                generation_seed,
                GAMES_PER_CANDIDATE,
            );
            candidates.push(StateConditionalCandidateReportV1 {
                candidate_index,
                parameters: parameters.to_vec(),
                fitness: fitness_v1(&evaluation),
                l2_squared: slice_l2_squared_v1(&parameters),
                evaluation,
            });
        }
        let mut order: Vec<usize> = (0..POPULATION).collect();
        order.sort_by(|left, right| {
            candidates[*right]
                .fitness
                .cmp(&candidates[*left].fitness)
                .then_with(|| {
                    candidates[*left]
                        .l2_squared
                        .total_cmp(&candidates[*right].l2_squared)
                })
                .then_with(|| left.cmp(right))
        });
        let elite_indices = order[..ELITE_COUNT].to_vec();
        let weight_total = (ELITE_COUNT * (ELITE_COUNT + 1) / 2) as f64;
        let mut updated_mean = [0.0_f32; NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1];
        for (rank, &candidate_index) in elite_indices.iter().enumerate() {
            let weight = (ELITE_COUNT - rank) as f64 / weight_total;
            for (destination, source) in updated_mean
                .iter_mut()
                .zip(candidates[candidate_index].parameters.iter().copied())
            {
                *destination += (weight * f64::from(source)) as f32;
            }
        }
        let mut variance_sum = 0.0_f64;
        for &candidate_index in &elite_indices {
            for (value, center) in candidates[candidate_index]
                .parameters
                .iter()
                .zip(updated_mean)
            {
                variance_sum += (f64::from(*value) - f64::from(center)).powi(2);
            }
        }
        let measured_sigma =
            (variance_sum / (ELITE_COUNT * NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1) as f64).sqrt();
        sigma = (0.70 * sigma + 0.30 * measured_sigma).clamp(MINIMUM_SIGMA, MAXIMUM_SIGMA);
        mean = updated_mean;
        generation_means.push(mean);
        let latest_generation_best_fitness = candidates[order[0]].fitness;
        generation_reports.push(StateConditionalGenerationReportV1 {
            generation,
            evaluation_base_seed: generation_seed,
            sigma_before,
            sigma_after: sigma,
            candidates,
            elite_candidate_indices: elite_indices,
            updated_mean: mean.to_vec(),
        });
        write_json_v1(
            &evidence_root.join("progress.json"),
            &StateConditionalProgressV1 {
                schema: "mtg-kernel-state-conditional-response-oracle-progress/v1",
                completed_generations: generation + 1,
                total_generations: GENERATIONS,
                latest_generation_best_fitness,
                sigma,
                elapsed_seconds: started.elapsed().as_secs_f64(),
            },
        );
        println!(
            "STATE_RESPONSE_ORACLE generation={}/{} best_fitness={} sigma={:.6}",
            generation + 1,
            GENERATIONS,
            latest_generation_best_fitness,
            sigma
        );
    }

    let mut policy_parameters = Vec::with_capacity(GENERATIONS + 1);
    policy_parameters.push(zero);
    policy_parameters.extend(generation_means);
    let mut selector_reports = Vec::with_capacity(policy_parameters.len());
    for (policy_index, parameters) in policy_parameters.iter().copied().enumerate() {
        let (_, panel_a) = evaluate_state_conditional_v1(
            &authorities,
            Arc::clone(&authorities.mixture),
            parameters,
            SELECTOR_A_SEED,
            SELECTOR_GAMES_PER_PANEL,
        );
        let (_, panel_b) = evaluate_state_conditional_v1(
            &authorities,
            Arc::clone(&authorities.mixture),
            parameters,
            SELECTOR_B_SEED,
            SELECTOR_GAMES_PER_PANEL,
        );
        let fitness_a = fitness_v1(&panel_a);
        let fitness_b = fitness_v1(&panel_b);
        selector_reports.push(StateConditionalSelectorReportV1 {
            policy_index,
            parameters: parameters.to_vec(),
            l2_squared: slice_l2_squared_v1(&parameters),
            panel_a,
            panel_b,
            worst_fitness: fitness_a.min(fitness_b),
            summed_fitness: fitness_a + fitness_b,
        });
        println!(
            "STATE_RESPONSE_ORACLE selector={}/{} worst_fitness={} summed_fitness={}",
            policy_index + 1,
            GENERATIONS + 1,
            fitness_a.min(fitness_b),
            fitness_a + fitness_b
        );
    }
    let mut selected_policy_index = 0_usize;
    for candidate_index in 1..selector_reports.len() {
        let candidate = &selector_reports[candidate_index];
        let selected = &selector_reports[selected_policy_index];
        let is_better = candidate.worst_fitness > selected.worst_fitness
            || (candidate.worst_fitness == selected.worst_fitness
                && (candidate.summed_fitness > selected.summed_fitness
                    || (candidate.summed_fitness == selected.summed_fitness
                        && (candidate.l2_squared < selected.l2_squared
                            || (candidate.l2_squared == selected.l2_squared
                                && candidate.policy_index < selected.policy_index)))));
        if is_better {
            selected_policy_index = candidate_index;
        }
    }
    let selected_parameters = policy_parameters[selected_policy_index];

    let (pool3_parent_result, pool3_parent) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        zero,
        FRESH_POOL3_SEED,
        FRESH_PANEL_GAMES,
    );
    let (pool3_candidate_result, pool3_candidate) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.mixture),
        selected_parameters,
        FRESH_POOL3_SEED,
        FRESH_PANEL_GAMES,
    );
    let fresh_pool3 = OracleFreshPanelV1 {
        opponent: "Pool3-40-20-20-20",
        paired: paired_comparison_v1(
            &pool3_parent_result,
            &pool3_candidate_result,
            &pool3_parent,
            &pool3_candidate,
        ),
        parent: pool3_parent,
        candidate: pool3_candidate,
    };

    let (promoted2_parent_result, promoted2_parent) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.promoted2),
        zero,
        FRESH_PROMOTED2_SEED,
        FRESH_PANEL_GAMES,
    );
    let (promoted2_candidate_result, promoted2_candidate) = evaluate_state_conditional_v1(
        &authorities,
        Arc::clone(&authorities.promoted2),
        selected_parameters,
        FRESH_PROMOTED2_SEED,
        FRESH_PANEL_GAMES,
    );
    let fresh_promoted2 = OracleFreshPanelV1 {
        opponent: "promoted2-generation384",
        paired: paired_comparison_v1(
            &promoted2_parent_result,
            &promoted2_candidate_result,
            &promoted2_parent,
            &promoted2_candidate,
        ),
        parent: promoted2_parent,
        candidate: promoted2_candidate,
    };

    let pool3_paired_gain =
        fresh_pool3.paired.gains >= fresh_pool3.paired.losses.saturating_add(12);
    let promoted2_paired_gain =
        fresh_promoted2.paired.gains >= fresh_promoted2.paired.losses.saturating_add(12);
    let pool3_seat_floor = fresh_pool3
        .paired
        .candidate_minus_parent_seat_wins
        .iter()
        .all(|value| *value >= -4);
    let promoted2_seat_floor = fresh_promoted2
        .paired
        .candidate_minus_parent_seat_wins
        .iter()
        .all(|value| *value >= -4);
    let gates = OracleFormalGatesV1 {
        pool3_paired_gain,
        promoted2_paired_gain,
        pool3_seat_floor,
        promoted2_seat_floor,
        pass: pool3_paired_gain
            && promoted2_paired_gain
            && pool3_seat_floor
            && promoted2_seat_floor,
    };
    let report = StateConditionalFormalReportV1 {
        schema: "mtg-kernel-state-conditional-response-oracle-formal/v1",
        pool_root: pool_root.display().to_string(),
        candidate_generation: CANDIDATE_GENERATION_V1,
        parameter_count: NATIVE_STATE_CONDITIONAL_RESIDUAL_DIM_V1,
        optimizer: "eight-generation antithetic cross-entropy method; two-panel terminal selector; terminal reward only",
        population: POPULATION,
        elite_count: ELITE_COUNT,
        generations: GENERATIONS,
        games_per_candidate: GAMES_PER_CANDIDATE,
        selector_games_per_panel: SELECTOR_GAMES_PER_PANEL,
        fresh_panel_games: FRESH_PANEL_GAMES,
        rng_seed: RNG_SEED,
        initial_sigma: INITIAL_SIGMA,
        generation_reports,
        selector_reports,
        selected_policy_index,
        selected_parameters: selected_parameters.to_vec(),
        fresh_pool3,
        fresh_promoted2,
        gates,
        elapsed_seconds: started.elapsed().as_secs_f64(),
    };
    write_json_v1(&formal_path, &report);
    println!("{}", serde_json::to_string(&report.gates).unwrap());
}
