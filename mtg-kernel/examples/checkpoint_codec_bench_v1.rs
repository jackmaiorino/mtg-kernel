//! Opt-in checkpoint codec benchmark (mtg-kernel#596, milestone 1).
//!
//! Measurement only; `docs/checkpoint_codec_bench_v1.md` has the commands,
//! the receipt schema and how to compare two builds A-B-B-A.
//!
//! ```text
//! cargo run --release --locked -p mtg-kernel --features checkpoint-codec-bench-v1 \
//!     --example checkpoint_codec_bench_v1 -- generate --fixture /abs/fixture
//! cargo run --release --locked -p mtg-kernel --features checkpoint-codec-bench-v1 \
//!     --example checkpoint_codec_bench_v1 -- measure --fixture /abs/fixture \
//!     --work /abs/work --public-pair python/tools/tests/fixtures/checkpoint_container_v1 \
//!     --out receipt.json
//! ```
//!
//! The global allocator is the production one (`FastAllocatorV1`, with
//! `retain_freed_memory_v1`) wrapped in relaxed atomic counters, so each
//! sample can report the bytes it allocated and its heap peak. The counters
//! cost the same in every build this binary is compiled from.

use mtg_kernel::expanded_deck_training_v1::checkpoint_codec_bench_v1::{
    generate_fixture_v1, measure_v1, peak_rss_bytes_v1, AllocationCountersV1, AllocationProbeV1,
    GenerateConfigV1, MeasureConfigV1, SectionV1,
};
use mtg_kernel::expanded_deck_training_v1::ExpandedModelSourceV1;
use serde_json::Value;
use std::alloc::{GlobalAlloc, Layout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct CountingAllocatorV1;

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);

impl CountingAllocatorV1 {
    fn grew(size: usize) {
        let size = size as u64;
        ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
        PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
    }

    fn shrank(size: usize) {
        LIVE_BYTES.fetch_sub(size as u64, Ordering::Relaxed);
    }
}

// SAFETY: every call is forwarded unchanged to the production allocator;
// the counters only observe sizes.
unsafe impl GlobalAlloc for CountingAllocatorV1 {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { mtg_kernel::FastAllocatorV1.alloc(layout) };
        if !pointer.is_null() {
            Self::grew(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { mtg_kernel::FastAllocatorV1.alloc_zeroed(layout) };
        if !pointer.is_null() {
            Self::grew(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { mtg_kernel::FastAllocatorV1.dealloc(pointer, layout) };
        Self::shrank(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { mtg_kernel::FastAllocatorV1.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            // A reallocation counts as one allocation of its new size.
            Self::shrank(layout.size());
            Self::grew(new_size);
        }
        moved
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocatorV1 = CountingAllocatorV1;

struct ProbeV1;

impl AllocationProbeV1 for ProbeV1 {
    fn read_v1(&self) -> AllocationCountersV1 {
        AllocationCountersV1 {
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
            peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
        }
    }

    fn reset_peak_v1(&self) {
        PEAK_LIVE_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}

const USAGE: &str = "usage:
  checkpoint_codec_bench_v1 generate --fixture DIR [--seed N] [--games-per-update N]
      [--warm-updates N] [--s-warm-steps N] [--s-groups N] [--out FILE]
  checkpoint_codec_bench_v1 measure --fixture DIR --work DIR --out FILE
      [--sections stages,updates,public-pair,s] [--reps N] [--warmup N]
      [--update-reps N] [--update-warmup N] [--preparation-workers N]
      [--public-pair DIR] [--extra-state LABEL=SOURCE.json]...
      [--max-wall-seconds N] [--label TEXT]
DIR and FILE paths are made absolute; the fixture and work directories must not exist yet.";

fn main() {
    // SAFETY: first statement of main; no other thread exists yet.
    unsafe { mtg_kernel::retain_freed_memory_v1() };
    mtg_kernel::opt_out_of_power_throttling_v1();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A large stack, so unoptimized smoke builds do not overflow in the
    // update; untouched stack pages are never resident.
    let outcome = std::thread::Builder::new()
        .name("checkpoint-codec-bench".into())
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run(&args))
        .map_err(|error| error.to_string())
        .and_then(|handle| {
            handle
                .join()
                .unwrap_or_else(|_| Err("the benchmark thread panicked".into()))
        });
    if let Err(error) = outcome {
        eprintln!("checkpoint_codec_bench_v1: {error}");
        std::process::exit(1);
    }
}

struct ArgsV1 {
    values: Vec<(String, String)>,
}

impl ArgsV1 {
    fn parse_v1(args: &[String]) -> Result<Self, String> {
        let mut values = Vec::new();
        let mut index = 0;
        while index < args.len() {
            let name = args[index]
                .strip_prefix("--")
                .ok_or_else(|| format!("unexpected argument {:?}\n{USAGE}", args[index]))?
                .to_owned();
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("--{name} needs a value\n{USAGE}"))?;
            values.push((name, value.clone()));
            index += 2;
        }
        Ok(Self { values })
    }

    fn take_all_v1(&mut self, name: &str) -> Vec<String> {
        let (taken, kept) = std::mem::take(&mut self.values)
            .into_iter()
            .partition(|(key, _)| key == name);
        self.values = kept;
        taken.into_iter().map(|(_, value)| value).collect()
    }

    fn take_v1(&mut self, name: &str) -> Result<Option<String>, String> {
        let mut all = self.take_all_v1(name);
        match all.len() {
            0 => Ok(None),
            1 => Ok(all.pop()),
            _ => Err(format!("--{name} is given more than once")),
        }
    }

    fn number_v1<T: std::str::FromStr>(&mut self, name: &str) -> Result<Option<T>, String> {
        self.take_v1(name)?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("--{name} {value:?} is not a number"))
            })
            .transpose()
    }

    fn path_v1(&mut self, name: &str) -> Result<Option<PathBuf>, String> {
        self.take_v1(name)?
            .map(|value| absolute_v1(Path::new(&value)))
            .transpose()
    }

    fn finish_v1(self) -> Result<(), String> {
        match self.values.first() {
            Some((name, _)) => Err(format!("unknown option --{name}\n{USAGE}")),
            None => Ok(()),
        }
    }
}

fn absolute_v1(path: &Path) -> Result<PathBuf, String> {
    std::path::absolute(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_json_v1(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn run(args: &[String]) -> Result<(), String> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    match command.as_str() {
        "generate" => generate(rest),
        "measure" => measure(rest),
        _ => Err(USAGE.into()),
    }
}

fn generate(args: &[String]) -> Result<(), String> {
    let mut args = ArgsV1::parse_v1(args)?;
    let fixture = args.path_v1("fixture")?.ok_or("generate needs --fixture")?;
    let mut config = GenerateConfigV1::defaults_v1(fixture);
    if let Some(seed) = args.number_v1("seed")? {
        config.seed = seed;
    }
    if let Some(games) = args.number_v1("games-per-update")? {
        config.games_per_update = games;
    }
    if let Some(updates) = args.number_v1("warm-updates")? {
        config.warm_updates = updates;
    }
    if let Some(steps) = args.number_v1("s-warm-steps")? {
        config.s_warm_steps = steps;
    }
    if let Some(groups) = args.number_v1("s-groups")? {
        config.s_groups_per_step = groups;
    }
    let out = args.path_v1("out")?;
    args.finish_v1()?;
    let started = std::time::Instant::now();
    let manifest = generate_fixture_v1(&config)?;
    if let Some(out) = out {
        write_json_v1(&out, &manifest)?;
    }
    println!(
        "fixture {} generated in {:.1} s: {} ordinary states, {} S states",
        config.fixture_directory.display(),
        started.elapsed().as_secs_f64(),
        manifest["states"].as_array().map_or(0, Vec::len),
        manifest["s_states"].as_array().map_or(0, Vec::len),
    );
    Ok(())
}

fn measure(args: &[String]) -> Result<(), String> {
    let mut args = ArgsV1::parse_v1(args)?;
    let fixture = args.path_v1("fixture")?.ok_or("measure needs --fixture")?;
    let work = args.path_v1("work")?.ok_or("measure needs --work")?;
    let out = args.path_v1("out")?.ok_or("measure needs --out")?;
    let mut config = MeasureConfigV1::defaults_v1(fixture, work);
    if let Some(sections) = args.take_v1("sections")? {
        config.sections = sections
            .split(',')
            .map(SectionV1::parse_v1)
            .collect::<Result<_, _>>()?;
    }
    if let Some(reps) = args.number_v1("reps")? {
        config.reps = reps;
    }
    if let Some(warmup) = args.number_v1("warmup")? {
        config.warmup = warmup;
    }
    if let Some(reps) = args.number_v1("update-reps")? {
        config.update_reps = reps;
    }
    if let Some(warmup) = args.number_v1("update-warmup")? {
        config.update_warmup = warmup;
    }
    if let Some(workers) = args.number_v1("preparation-workers")? {
        config.preparation_workers = workers;
    }
    config.public_pair = args.path_v1("public-pair")?;
    if config.public_pair.is_none() {
        config
            .sections
            .retain(|section| *section != SectionV1::PublicPair);
    }
    for extra in args.take_all_v1("extra-state") {
        let (label, path) = extra
            .split_once('=')
            .ok_or("--extra-state takes LABEL=SOURCE.json")?;
        let path = absolute_v1(Path::new(path))?;
        let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let source: ExpandedModelSourceV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        config.extra_states.push((label.to_owned(), source));
    }
    config.max_wall_seconds = args.number_v1("max-wall-seconds")?;
    config.label = args.take_v1("label")?.unwrap_or_default();
    args.finish_v1()?;
    let mut receipt = measure_v1(&config, Some(&ProbeV1))?;
    receipt["process_peak_rss_bytes"] = peak_rss_bytes_v1().into();
    receipt["argv"] = std::env::args().collect::<Vec<_>>().into();
    write_json_v1(&out, &receipt)?;
    print_summary_v1(&receipt);
    println!("receipt: {}", out.display());
    Ok(())
}

fn print_summary_v1(receipt: &Value) {
    let mib = |value: f64| value / (1024.0 * 1024.0);
    let median = |series: &Value, field: &str| series["summary"][field]["median"].as_f64();
    let show = |value: Option<f64>, scale: fn(f64) -> f64| {
        value.map_or_else(|| "-".to_owned(), |value| format!("{:.3}", scale(value)))
    };
    println!(
        "{:<12} {:<10} {:<9} {:<24} {:>4} {:>11} {:>11} {:>11} {:>9} {:>11}",
        "section",
        "state",
        "cell",
        "stage",
        "n",
        "wall ms",
        "cpu ms",
        "alloc MiB",
        "allocs",
        "peak MiB"
    );
    for series in receipt["series"].as_array().into_iter().flatten() {
        println!(
            "{:<12} {:<10} {:<9} {:<24} {:>4} {:>11} {:>11} {:>11} {:>9} {:>11}",
            series["section"].as_str().unwrap_or("?"),
            series["state"].as_str().unwrap_or("?"),
            series["cell"].as_str().unwrap_or("?"),
            series["stage"].as_str().unwrap_or("?"),
            series["summary"]["n"].as_u64().unwrap_or(0),
            show(median(series, "wall_ns"), |ns| ns / 1e6),
            show(median(series, "cpu_ns"), |ns| ns / 1e6),
            show(median(series, "allocated_bytes"), mib),
            show(median(series, "allocations"), |count| count),
            show(median(series, "peak_heap_bytes"), mib),
        );
    }
    for check in receipt["checks"].as_array().into_iter().flatten() {
        println!(
            "check {}: {}",
            check["state"].as_str().unwrap_or("?"),
            if check["passed"] == true {
                "passed"
            } else {
                "FAILED"
            }
        );
    }
}
