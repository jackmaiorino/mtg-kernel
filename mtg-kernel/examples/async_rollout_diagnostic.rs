//! Environment-only diagnostic for the asynchronous Rust rollout prototype.
//!
//! This excludes observations, encoding, inference, learning, persistence,
//! Python, IPC, and XMage. It is not end-to-end training throughput.

use mtg_kernel::async_rollout::{
    run_seeded_uniform_async_rollout_v1, AsyncRolloutConfigV1, AsyncRolloutResultV1,
};
use mtg_kernel::rl::PlayerSeatV1;

const ENVIRONMENT_SEED: u64 = 0xE070_D1A6_0000_0001;
const OPPONENT_POLICY_SEED: u64 = 0x0AA0_D1A6_0000_0001;
const LEARNER_POLICY_SEED: u64 = 0x1EA2_D1A6_0000_0001;
const WARMUP_FIRST_EPISODE_ID: u64 = 1_000_000;
const MEASURE_FIRST_EPISODE_ID: u64 = 2_000_000;
const WARMUP_GAMES: u64 = 1_024;
const DEFAULT_GAMES: u64 = 16_384;
const EXPECTED_POLICY_STEPS: u64 = 4_127_303;
const EXPECTED_PHYSICAL_DECISIONS: u64 = 3_617_616;
const EXPECTED_LEARNER_ACTIONS: u64 = 2_065_302;

#[derive(Debug, Clone, Copy)]
struct Args {
    workers: usize,
    games: u64,
    runs: usize,
    matrix: bool,
    warmup: bool,
    broker_timing: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut parsed = Args {
        workers: 16,
        games: DEFAULT_GAMES,
        runs: 1,
        matrix: false,
        warmup: true,
        broker_timing: true,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--workers" => {
                index += 1;
                parsed.workers = args
                    .get(index)
                    .ok_or("--workers requires a value")?
                    .parse()
                    .map_err(|_| "--workers must be an integer")?;
            }
            "--games" => {
                index += 1;
                parsed.games = args
                    .get(index)
                    .ok_or("--games requires a value")?
                    .parse()
                    .map_err(|_| "--games must be an integer")?;
            }
            "--runs" => {
                index += 1;
                parsed.runs = args
                    .get(index)
                    .ok_or("--runs requires a value")?
                    .parse()
                    .map_err(|_| "--runs must be an integer")?;
            }
            "--matrix" => parsed.matrix = true,
            "--no-warmup" => parsed.warmup = false,
            "--no-broker-timing" => parsed.broker_timing = false,
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }
    if parsed.games == 0 || parsed.runs == 0 {
        return Err("--games and --runs must be positive".into());
    }
    Ok(parsed)
}

fn config(
    workers: usize,
    first_episode_id: u64,
    episode_count: u64,
    broker_timing: bool,
) -> AsyncRolloutConfigV1 {
    AsyncRolloutConfigV1 {
        deck_ids: ["Rally".to_string(), "Rally".to_string()],
        learner_seat: PlayerSeatV1::P0,
        environment_seed: ENVIRONMENT_SEED,
        opponent_policy_seed: OPPONENT_POLICY_SEED,
        learner_policy_seed: LEARNER_POLICY_SEED,
        max_physical_decisions: 4_096,
        max_policy_steps: 524_288,
        worker_count: workers,
        first_episode_id,
        episode_count,
        measure_broker_service_time: broker_timing,
    }
}

fn verify_known(result: &AsyncRolloutResultV1, games: u64) {
    assert_eq!(result.episodes.len(), games as usize);
    assert!(result.all_natural());
    if games == DEFAULT_GAMES {
        assert_eq!(result.policy_step_count, EXPECTED_POLICY_STEPS);
        assert_eq!(result.physical_decision_count, EXPECTED_PHYSICAL_DECISIONS);
        assert_eq!(
            result.metrics.learner_action_count,
            EXPECTED_LEARNER_ACTIONS
        );
    }
}

fn print_result(run: usize, workers: usize, result: &AsyncRolloutResultV1) {
    println!(
        "run={run} workers={workers} natural_games={} games_per_second={:.3} total_wall_ns={} policy_steps={} physical_decisions={} learner_actions={} ready_snapshots={} mean_ready_width={:.3} max_ready_width={} mean_broker_service_ns={:.1}",
        result.episodes.len(),
        result.games_per_second(),
        result.metrics.total_elapsed_ns,
        result.policy_step_count,
        result.physical_decision_count,
        result.metrics.learner_action_count,
        result.metrics.ready_snapshot_count,
        result.metrics.mean_ready_width(),
        result.metrics.max_ready_width,
        result.metrics.mean_broker_service_ns(),
    );
}

fn run_one(args: Args, workers: usize, run: usize) -> AsyncRolloutResultV1 {
    if args.warmup {
        let warmup = run_seeded_uniform_async_rollout_v1(config(
            workers,
            WARMUP_FIRST_EPISODE_ID,
            WARMUP_GAMES,
            false,
        ))
        .expect("asynchronous warmup succeeds");
        assert!(warmup.all_natural());
    }
    let result = run_seeded_uniform_async_rollout_v1(config(
        workers,
        MEASURE_FIRST_EPISODE_ID,
        args.games,
        args.broker_timing,
    ))
    .expect("asynchronous measurement succeeds");
    verify_known(&result, args.games);
    print_result(run, workers, &result);
    result
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run this diagnostic with cargo run --release");
        std::process::exit(2);
    }
    let args = parse_args().unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!("usage: async_rollout_diagnostic [--workers 1|16] [--games N] [--runs N] [--matrix] [--no-warmup] [--no-broker-timing]");
        std::process::exit(2);
    });
    println!("async_rollout/environment_only/v1");
    println!(
        "WARNING: excludes encoding, inference, learning, persistence, Python, IPC, and XMage"
    );
    println!("timing_scope=full_executor_api_including_validation_allocation_spawn_join_canonicalization_and_aggregate_accounting");

    if args.matrix {
        let n1 = run_one(args, 1, 1);
        let n16 = run_one(args, 16, 1);
        assert_eq!(n1.episodes, n16.episodes);
        println!("n1_n16_episode_parity=PASS");
        return;
    }
    for run in 1..=args.runs {
        let _ = run_one(args, args.workers, run);
    }
}
