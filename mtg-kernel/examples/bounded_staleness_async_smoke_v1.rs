//! Bounded-staleness async inference: throughput self-check.
//!
//! Runs the matched A/B harness (`mtg_kernel::bounded_staleness_async_
//! harness_v1::run_matched_ab_v1`) at a small, bounded-duration scale and
//! prints an honest sync-vs-async throughput and staleness report. This is
//! the "bounded smoke measurement (a few minutes)" the implementation report
//! calls for, not a validation campaign: see `HARNESS_LIMITATIONS_NOTE_V1` in
//! the printed output for exactly what this run does and does not prove.
//!
//! Usage: `cargo run --release --example bounded_staleness_async_smoke_v1
//! -- [--updates N] [--batch-episodes N] [--max-staleness N]
//! [--train-step-ms N] [--workers N] [--sessions-per-worker N]
//! [--broker-batch-target N]`

use mtg_kernel::bounded_staleness_async_harness_v1::{run_matched_ab_v1, ArmReportV1, HarnessConfigV1};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
struct Args {
    updates: u64,
    batch_episodes: u64,
    max_staleness: u32,
    train_step_ms: u64,
    workers: usize,
    sessions_per_worker: usize,
    broker_batch_target: usize,
}

fn parse_args() -> Result<Args, String> {
    // Defaults reproduce the run reported in the implementation report: on
    // the reference host, this takes roughly 110 seconds total for both
    // arms in --release. --train-step-ms 100 is a synthetic stand-in for
    // the trainer's real non-rollout wall (gradient step + Store commit),
    // not a measurement of it; see HARNESS_LIMITATIONS_NOTE_V1.
    let mut parsed = Args {
        updates: 400,
        batch_episodes: 64,
        max_staleness: 3,
        train_step_ms: 100,
        workers: 8,
        sessions_per_worker: 8,
        broker_batch_target: 64,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut index = 0usize;
    while index < args.len() {
        let target: Option<&mut dyn FromArg> = match args[index].as_str() {
            "--updates" => Some(&mut parsed.updates),
            "--batch-episodes" => Some(&mut parsed.batch_episodes),
            "--max-staleness" => Some(&mut parsed.max_staleness),
            "--train-step-ms" => Some(&mut parsed.train_step_ms),
            "--workers" => Some(&mut parsed.workers),
            "--sessions-per-worker" => Some(&mut parsed.sessions_per_worker),
            "--broker-batch-target" => Some(&mut parsed.broker_batch_target),
            other => return Err(format!("unknown argument {other}")),
        };
        if let Some(target) = target {
            index += 1;
            let raw = args.get(index).ok_or("option requires a value")?;
            target.assign(raw)?;
        }
        index += 1;
    }
    if parsed.updates == 0 || parsed.batch_episodes == 0 || parsed.max_staleness == 0 {
        return Err("--updates, --batch-episodes, and --max-staleness must be positive".into());
    }
    Ok(parsed)
}

// Small local trait so `parse_args` can hold a homogeneous list of `&mut`
// references into heterogeneously-typed `Args` fields, matching the pattern
// `async_rollout_v2_diagnostic.rs` uses for its own integer options.
trait FromArg {
    fn assign(&mut self, raw: &str) -> Result<(), String>;
}

impl FromArg for u64 {
    fn assign(&mut self, raw: &str) -> Result<(), String> {
        *self = raw.parse().map_err(|_| "expected an integer".to_string())?;
        Ok(())
    }
}

impl FromArg for u32 {
    fn assign(&mut self, raw: &str) -> Result<(), String> {
        *self = raw.parse().map_err(|_| "expected an integer".to_string())?;
        Ok(())
    }
}

impl FromArg for usize {
    fn assign(&mut self, raw: &str) -> Result<(), String> {
        *self = raw.parse().map_err(|_| "expected an integer".to_string())?;
        Ok(())
    }
}

fn print_arm(report: &ArmReportV1) {
    println!("  arm: {}", report.arm_name);
    println!("    updates:            {}", report.total_updates);
    println!("    episodes:           {}", report.total_episodes);
    println!("    wall time:          {:?}", report.wall_time);
    println!("    episodes/sec:       {:.2}", report.episodes_per_second);
    if let Some(max_staleness) = report.max_observed_staleness {
        println!("    max observed staleness (updates): {max_staleness}");
    }
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(2);
        }
    };

    let config = HarnessConfigV1 {
        deck_ids: ["Rally".to_string(), "Rally".to_string()],
        base_environment_seed: 0xB0DE_D57A_1EEE_0001,
        opponent_policy_seed: 0xB0DE_D57A_1EEE_0002,
        learner_policy_seed: 0xB0DE_D57A_1EEE_0003,
        batch_episodes: args.batch_episodes,
        total_updates: args.updates,
        max_staleness_updates: args.max_staleness,
        worker_count: args.workers,
        sessions_per_worker: args.sessions_per_worker,
        broker_batch_target: args.broker_batch_target,
        max_physical_decisions: 4_096,
        max_policy_steps: 524_288,
        scheduler_timeout: Duration::from_secs(60),
        synthetic_train_step: Duration::from_millis(args.train_step_ms),
    };

    println!("bounded-staleness async inference: throughput self-check");
    println!(
        "config: updates={} batch_episodes={} max_staleness={} \
         synthetic_train_step={:?} workers={} sessions_per_worker={} broker_batch_target={}",
        config.total_updates,
        config.batch_episodes,
        config.max_staleness_updates,
        config.synthetic_train_step,
        config.worker_count,
        config.sessions_per_worker,
        config.broker_batch_target,
    );
    println!();

    let report = run_matched_ab_v1(&config).expect("harness run failed");

    print_arm(&report.sync);
    println!();
    print_arm(&report.bounded_staleness_async);
    println!();
    println!("speedup (sync wall / async wall): {:.3}x", report.speedup_ratio);
    println!(
        "reward curves identical between arms (expected under the stand-in \
         scorer): {}",
        report.reward_curves_match
    );
    println!();
    println!("LIMITATIONS: {}", report.limitations);
}
