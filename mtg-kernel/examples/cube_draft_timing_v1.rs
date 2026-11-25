//! Cost of the cube draft environment v1 (single thread): drafts (draft plus
//! deck build, uniform pickers) per core-second, and full pods (draft,
//! build, 28 uniform-random games) per core-hour, with game end counts by
//! kind. The pickers and the deck rule are fixtures; nothing here trains.
//!
//! `cargo run --release -p mtg-kernel --features eternal-fixtures --example
//! cube_draft_timing_v1 -- --seed 1 --drafts 100 --pods 1`
mtg_kernel::fast_global_allocator!();

use mtg_kernel::cube_draft_v1::{
    play_pod_round_robin, pod_seed, uniform_draft_and_build, CubePool, POD_GAMES,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Instant;

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);

fn arg(args: &[String], name: &str, default: u64) -> u64 {
    match args.iter().position(|a| a == name) {
        Some(at) => args
            .get(at + 1)
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("{name} needs an unsigned integer")),
        None => default,
    }
}

fn per(count: u64, seconds: f64, scale: f64) -> f64 {
    if seconds > 0.0 {
        count as f64 / seconds * scale
    } else {
        0.0
    }
}

fn run(seed: u64, drafts: u64, pods: u64) {
    let cube = std::fs::read_to_string(CUBE_LIST).expect("cube list");
    let gaps = std::fs::read_to_string(GAP_RECORD).expect("gap record");
    let pool = CubePool::from_json_texts(&cube, &gaps, None).expect("draftable pool");

    // Draft `index` (and pod `index`) uses the draft seed `seed + index`.
    let started = Instant::now();
    for index in 0..drafts {
        let draft_seed = seed.wrapping_add(index);
        std::hint::black_box(uniform_draft_and_build(&pool, draft_seed).expect("draft and build"));
    }
    let draft_seconds = started.elapsed().as_secs_f64();

    let mut ends: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut decisions = 0u64;
    let started = Instant::now();
    for index in 0..pods {
        let draft_seed = seed.wrapping_add(index);
        let (_, decks) = uniform_draft_and_build(&pool, draft_seed).expect("draft and build");
        let result = play_pod_round_robin(&decks, pod_seed(draft_seed));
        for (kind, count) in result.end_counts() {
            *ends.entry(kind).or_default() += count;
        }
        decisions += result.games.iter().map(|game| game.decisions).sum::<u64>();
    }
    let pod_seconds = started.elapsed().as_secs_f64();

    println!(
        "{}",
        json!({
            "tool": "cube_draft_timing_v1",
            "seed": seed,
            "threads": 1,
            "pool_cards": pool.card_count(),
            "drafts": drafts,
            "draft_seconds": draft_seconds,
            "drafts_per_core_second": per(drafts, draft_seconds, 1.0),
            "pods": pods,
            "games": pods * POD_GAMES as u64,
            "pod_seconds": pod_seconds,
            "pods_per_core_hour": per(pods, pod_seconds, 3600.0),
            "game_ends": ends,
            "decisions": decisions
        })
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed = arg(&args, "--seed", 20_261_122);
    let drafts = arg(&args, "--drafts", 100);
    let pods = arg(&args, "--pods", 1);
    // One worker thread with a large stack, as the engine's binaries use;
    // the main thread only waits, so the work stays single-threaded.
    std::thread::Builder::new()
        .name("cube-draft-timing-v1".to_owned())
        .stack_size(256 << 20)
        .spawn(move || run(seed, drafts, pods))
        .expect("spawn worker")
        .join()
        .expect("worker");
}
