//! Engine ceiling probe for the GPU design note (measurement only): games and
//! decisions per second on a run config's real episodes, per thread and
//! across threads, for three per-decision workloads:
//!
//! - `uniform`: the engine and its decision surface only (a seeded uniform
//!   choice among the legal actions);
//! - `policy`: the net-8 fixture policy's V3 tensorization (with the 96
//!   state-digest features), CPU forward and sampling, as collection does
//!   minus records and publication;
//! - `policy-no-digest`: the same under the no-state-digest input contract.
//!
//! Run (one claim of the cores measured):
//!   MTG_KERNEL_CEILING_CONFIG=<run config.json> MTG_KERNEL_CEILING_THREADS=1,6
//!   cargo test --release --lib engine_ceiling_probe -- --ignored --nocapture

use super::*;
use crate::state::SplitMix64;
use std::time::Instant;

fn episodes_from_config() -> Vec<ExpandedEpisodeV1> {
    let path = std::env::var("MTG_KERNEL_CEILING_CONFIG").expect("set MTG_KERNEL_CEILING_CONFIG");
    let config: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    config["iterations"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|iteration| iteration["episodes"].as_array().unwrap().iter())
        .map(|scheduled| serde_json::from_value(scheduled["episode"].clone()).unwrap())
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Workload {
    Uniform,
    Policy,
    PolicyNoDigest,
}

/// Plays `episode` to its terminal; returns the decision count, or None when
/// the fixture policy cannot encode a decision of this deck pool (the game is
/// then left out of the rates and counted).
fn play(episode: &ExpandedEpisodeV1, workload: Workload, policy: &mut FrozenPlayPolicyV1) -> Option<u64> {
    let configs = episode.configurations().unwrap();
    let mut session = episode_session_v1(episode, &configs).unwrap();
    restore_reset_cut_v1(&mut session, episode).unwrap();
    policy.reset_sampling_v1(paired_policy_seeds_v1(episode.seed));
    let mut rng = SplitMix64::seed(episode.seed);
    let mut decisions = 0u64;
    while let FastActorResponseV1::Decision(d) = session.current_response() {
        let selected = match workload {
            Workload::Uniform => (rng.next_u64() % u64::from(d.legal_action_count)) as u32,
            Workload::Policy | Workload::PolicyNoDigest => {
                policy.select_with_training_tensor_v3(&session).ok()?.0
            }
        };
        session.step(d.episode_id, d.step, selected).unwrap();
        decisions += 1;
        assert!(decisions < 1_000_000, "decision cap");
    }
    Some(decisions)
}

#[test]
#[ignore = "timing only: set MTG_KERNEL_CEILING_CONFIG"]
fn engine_ceiling_probe_v1() {
    let episodes = episodes_from_config();
    let threads: Vec<usize> = std::env::var("MTG_KERNEL_CEILING_THREADS")
        .unwrap_or_else(|_| "1".into())
        .split(',')
        .map(|t| t.parse().unwrap())
        .collect();
    let games: usize = std::env::var("MTG_KERNEL_CEILING_GAMES_PER_THREAD")
        .map_or(20, |g| g.parse().unwrap());
    let mut template = FrozenPlayPolicyV1::training_fixture_v3();
    let mut no_digest = FrozenPlayPolicyV1::training_fixture_v3();
    no_digest
        .apply_no_state_digest_v1("converted".into(), 123..219, 1_499, false)
        .unwrap();
    for (name, workload) in [
        ("uniform", Workload::Uniform),
        ("policy", Workload::Policy),
        ("policy-no-digest", Workload::PolicyNoDigest),
    ] {
        for &count in &threads {
            let base = if workload == Workload::PolicyNoDigest { &mut no_digest } else { &mut template };
            let forks: Vec<FrozenPlayPolicyV1> =
                (0..count).map(|_| base.fork_for_collection_v3().unwrap()).collect();
            let started = Instant::now();
            let (decisions, completed): (u64, u64) = std::thread::scope(|s| {
                let handles: Vec<_> = forks
                    .into_iter()
                    .enumerate()
                    .map(|(t, mut policy)| {
                        let episodes = &episodes;
                        s.spawn(move || {
                            (0..games)
                                .filter_map(|g| {
                                    let episode = &episodes[(t * games + g) % episodes.len()];
                                    play(episode, workload, &mut policy)
                                })
                                .fold((0u64, 0u64), |(d, n), x| (d + x, n + 1))
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .fold((0, 0), |(d, n), (x, m)| (d + x, n + m))
            });
            let seconds = started.elapsed().as_secs_f64();
            // Skipped games ran partway, so the rates are slight overestimates.
            let skipped = (count * games) as u64 - completed;
            let total_games = completed as f64;
            println!(
                "{{\"probe\":\"engine_ceiling\",\"workload\":\"{name}\",\"threads\":{count},\"games\":{total_games},\"decisions\":{decisions},\"seconds\":{seconds:.3},\"games_per_s\":{:.2},\"decisions_per_s\":{:.0},\"games_per_s_per_thread\":{:.2},\"decisions_per_game\":{:.1},\"skipped_games\":{skipped}}}",
                total_games / seconds,
                decisions as f64 / seconds,
                total_games / seconds / count as f64,
                decisions as f64 / total_games
            );
        }
    }
    let _ = &mut template;
}
