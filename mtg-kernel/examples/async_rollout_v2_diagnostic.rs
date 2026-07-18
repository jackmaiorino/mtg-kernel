//! Environment-only diagnostic for the multi-session rollout scheduler.
//!
//! This excludes observations, encoding, inference, learning, persistence,
//! Python, IPC, and XMage.  It measures the full scheduler API and its
//! timer-free batch widths; it is not end-to-end training throughput.

use mtg_kernel::async_rollout::{run_seeded_uniform_async_rollout_v1, AsyncRolloutConfigV1};
use mtg_kernel::async_rollout_v2::{
    run_seeded_uniform_async_rollout_v2, AsyncRolloutConfigV2, AsyncRolloutResultV2,
};
use mtg_kernel::rl::PlayerSeatV1;
use std::fmt::Write as _;
use std::time::Duration;

const ENVIRONMENT_SEED: u64 = 0xE070_D1A6_0000_0001;
const OPPONENT_POLICY_SEED: u64 = 0x0AA0_D1A6_0000_0001;
const LEARNER_POLICY_SEED: u64 = 0x1EA2_D1A6_0000_0001;
const WARMUP_FIRST_EPISODE_ID: u64 = 1_000_000;
const MEASURE_FIRST_EPISODE_ID: u64 = 2_000_000;
const PARITY_FIRST_EPISODE_ID: u64 = 3_000_000;
const WARMUP_GAMES: u64 = 1_024;
const PARITY_GAMES: u64 = 256;
const DEFAULT_GAMES: u64 = 16_384;
const EXPECTED_POLICY_STEPS: u64 = 4_127_303;
const EXPECTED_PHYSICAL_DECISIONS: u64 = 3_617_616;
const EXPECTED_LEARNER_ACTIONS: u64 = 2_065_302;

#[derive(Debug, Clone, Copy)]
struct Args {
    workers: usize,
    sessions_per_worker: usize,
    batch_target: usize,
    games: u64,
    runs: usize,
    scheduler_timeout_ms: u64,
    warmup: bool,
    parity: bool,
    broker_timing: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut parsed = Args {
        workers: 16,
        sessions_per_worker: 64,
        batch_target: 1_024,
        games: DEFAULT_GAMES,
        runs: 1,
        scheduler_timeout_ms: 300_000,
        warmup: true,
        parity: true,
        broker_timing: true,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut index = 0usize;
    while index < args.len() {
        let target = match args[index].as_str() {
            "--workers" => Some(&mut parsed.workers),
            "--sessions-per-worker" => Some(&mut parsed.sessions_per_worker),
            "--batch-target" => Some(&mut parsed.batch_target),
            "--runs" => Some(&mut parsed.runs),
            "--games" => {
                index += 1;
                parsed.games = args
                    .get(index)
                    .ok_or("--games requires a value")?
                    .parse()
                    .map_err(|_| "--games must be an integer")?;
                None
            }
            "--scheduler-timeout-ms" => {
                index += 1;
                parsed.scheduler_timeout_ms = args
                    .get(index)
                    .ok_or("--scheduler-timeout-ms requires a value")?
                    .parse()
                    .map_err(|_| "--scheduler-timeout-ms must be an integer")?;
                None
            }
            "--no-warmup" => {
                parsed.warmup = false;
                None
            }
            "--no-parity" => {
                parsed.parity = false;
                None
            }
            "--no-broker-timing" => {
                parsed.broker_timing = false;
                None
            }
            other => return Err(format!("unknown argument {other}")),
        };
        if let Some(target) = target {
            index += 1;
            *target = args
                .get(index)
                .ok_or("integer option requires a value")?
                .parse()
                .map_err(|_| "option value must be an integer")?;
        }
        index += 1;
    }
    if parsed.games == 0 || parsed.runs == 0 || parsed.scheduler_timeout_ms == 0 {
        return Err("--games, --runs, and --scheduler-timeout-ms must be positive".into());
    }
    Ok(parsed)
}

fn v2_config(
    args: Args,
    first_episode_id: u64,
    episode_count: u64,
    broker_timing: bool,
) -> AsyncRolloutConfigV2 {
    AsyncRolloutConfigV2 {
        deck_ids: ["Rally".to_string(), "Rally".to_string()],
        learner_seat: PlayerSeatV1::P0,
        environment_seed: ENVIRONMENT_SEED,
        opponent_policy_seed: OPPONENT_POLICY_SEED,
        learner_policy_seed: LEARNER_POLICY_SEED,
        max_physical_decisions: 4_096,
        max_policy_steps: 524_288,
        worker_count: args.workers,
        sessions_per_worker: args.sessions_per_worker,
        broker_batch_target: args.batch_target,
        first_episode_id,
        episode_count,
        scheduler_timeout: Duration::from_millis(args.scheduler_timeout_ms),
        measure_broker_service_time: broker_timing,
    }
}

fn v1_config(args: Args, first_episode_id: u64, episode_count: u64) -> AsyncRolloutConfigV1 {
    AsyncRolloutConfigV1 {
        deck_ids: ["Rally".to_string(), "Rally".to_string()],
        learner_seat: PlayerSeatV1::P0,
        environment_seed: ENVIRONMENT_SEED,
        opponent_policy_seed: OPPONENT_POLICY_SEED,
        learner_policy_seed: LEARNER_POLICY_SEED,
        max_physical_decisions: 4_096,
        max_policy_steps: 524_288,
        worker_count: args.workers,
        first_episode_id,
        episode_count,
        measure_broker_service_time: false,
    }
}

fn verify_known(result: &AsyncRolloutResultV2, games: u64) {
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

fn digest_hex(digest: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn run_one(args: Args, run: usize) {
    if args.warmup {
        let warmup = run_seeded_uniform_async_rollout_v2(v2_config(
            args,
            WARMUP_FIRST_EPISODE_ID,
            WARMUP_GAMES,
            false,
        ))
        .expect("multi-session warmup succeeds");
        assert!(warmup.all_natural());
    }
    let result = run_seeded_uniform_async_rollout_v2(v2_config(
        args,
        MEASURE_FIRST_EPISODE_ID,
        args.games,
        args.broker_timing,
    ))
    .expect("multi-session measurement succeeds");
    verify_known(&result, args.games);
    println!(
        "run={run} workers={} sessions_per_worker={} logical_lanes={} batch_target={} natural_games={} games_per_second={:.3} total_wall_ns={} policy_steps={} physical_decisions={} learner_actions={} complete_rounds={} batches={} mean_batch_width={:.3} max_batch_width={} full_target_chunks={} short_round_chunks={} worker_park_attempts={} broker_park_attempts={} batch_membership_digest_v2={} mean_broker_service_ns={:.1}",
        args.workers,
        args.sessions_per_worker,
        args.workers * args.sessions_per_worker,
        args.batch_target,
        result.episodes.len(),
        result.games_per_second(),
        result.metrics.total_elapsed_ns,
        result.policy_step_count,
        result.physical_decision_count,
        result.metrics.learner_action_count,
        result.metrics.complete_round_count,
        result.metrics.batch_publication_count,
        result.metrics.mean_batch_width(),
        result.metrics.max_batch_width,
        result.metrics.target_flush_count,
        result.metrics.quiescent_flush_count,
        result.metrics.worker_park_attempt_count,
        result.metrics.broker_park_attempt_count,
        digest_hex(result.metrics.batch_membership_digest),
        result.metrics.mean_broker_service_ns(),
    );
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run this diagnostic with cargo run --release");
        std::process::exit(2);
    }
    let args = parse_args().unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!(
            "usage: async_rollout_v2_diagnostic [--workers N] [--sessions-per-worker N] [--batch-target N] [--games N] [--runs N] [--scheduler-timeout-ms N] [--no-warmup] [--no-parity] [--no-broker-timing]"
        );
        std::process::exit(2);
    });
    println!("async_rollout/environment_only/v2");
    println!(
        "WARNING: excludes observations, encoding, inference, learning, persistence, Python, IPC, and XMage"
    );
    println!("batching=complete_global_quiescent_round_stable_sort_deterministic_target_chunks");
    println!("deadline=cooperative_scheduler_timeout; hard_kill_of_nonreturning_engine_call_requires_future_process_isolation");
    println!("timing_scope=full_executor_api_including_validation_allocation_spawn_join_canonicalization_and_aggregate_accounting");

    if args.parity {
        let v1 = run_seeded_uniform_async_rollout_v1(v1_config(
            args,
            PARITY_FIRST_EPISODE_ID,
            PARITY_GAMES,
        ))
        .expect("v1 parity schedule succeeds");
        let v2 = run_seeded_uniform_async_rollout_v2(v2_config(
            args,
            PARITY_FIRST_EPISODE_ID,
            PARITY_GAMES,
            false,
        ))
        .expect("v2 parity schedule succeeds");
        assert_eq!(v2.episodes, v1.episodes);
        assert_eq!(v2.policy_step_count, v1.policy_step_count);
        assert_eq!(v2.physical_decision_count, v1.physical_decision_count);
        println!("v1_v2_episode_terminal_trace_parity=PASS games={PARITY_GAMES}");
    }
    for run in 1..=args.runs {
        run_one(args, run);
    }
}
