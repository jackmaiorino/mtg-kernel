//! Environment-only capacity diagnostic for the partial flat action slice.
//!
//! This is not training throughput, a production-state encoder gate, an
//! XMage comparison, or science-ready evidence. Encode/hash phases repeat
//! independently generated valid Rally session states according to a frozen
//! Rally legal-action-width histogram; consume/combined phases walk live,
//! independently generated Rally/Rally sessions and report their actual shape
//! histograms separately.

use mtg_kernel::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, FlatActionCoreV1,
    FlatActionDecisionDiagnosticV1, FlatActionDecisionSliceBuffersV1, FlatActionObjectV1,
    FlatActionRefV1, CANONICAL_RALLY_DECK_ID,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Instant;

const HISTOGRAM_BYTES: &[u8] =
    include_bytes!("../../data/rally_all_policy_legal_action_width_histogram_v1.json");
const HISTOGRAM_FILE_SHA256: &str =
    "d9471ee78ee8b656040d1920118f962f4b239e55603220e3679b1d11b847e579";
const AGGREGATE_RECORD_SHA256: &str =
    "33490dc1fbf21555cc469595beadbda70c30092ac95cac297bc6f0e48ef18f7c";
const UPSTREAM_RAW_ARTIFACT_SHA256: &str =
    "682198c7e169a67a2c885dd8362db0c67c329b8cb1e6390f4fbc905c3f9bd7ee";
const UPSTREAM_SOURCE_COMMIT: &str = "d71dca82dfe36292328ecbc4962a0d6764d9ca5c";
const HISTOGRAM_SOURCE_COMMIT: &str = "ba00eade5a0b25fc848a865182e6da61ee26e510";
const MAX_PHYSICAL_DECISIONS: u64 = 10_000;
const MAX_POLICY_STEPS: u64 = 1_280_000;
const MAX_ACTION_ROWS: usize = 256;
const MAX_REF_ROWS: usize = 1_024;
const MAX_OBJECT_ROWS: usize = 512;
const MAX_REPORTED_SHAPE: usize = 511;

struct CountingAllocator;

static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATION_EVENTS: AtomicU64 = AtomicU64::new(0);
static ALLOCATION_REQUESTED_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_EVENTS.fetch_add(1, Ordering::Relaxed);
            ALLOCATION_REQUESTED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_EVENTS.fetch_add(1, Ordering::Relaxed);
            ALLOCATION_REQUESTED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_EVENTS.fetch_add(1, Ordering::Relaxed);
            ALLOCATION_REQUESTED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[derive(Debug)]
struct Config {
    git_commit: String,
    shape_repeats: u64,
    live_decisions_per_worker: u64,
    fixture_search_games: u64,
}

impl Config {
    fn parse() -> Result<Self, String> {
        let mut git_commit = None;
        let mut shape_repeats = 32_u64;
        let mut live_decisions_per_worker = 32_768_u64;
        let mut fixture_search_games = 4_096_u64;
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| format!("{argument} requires a value"))
            };
            match argument.as_str() {
                "--git-commit" => git_commit = Some(value()?),
                "--shape-repeats" => shape_repeats = parse_positive(&value()?, "--shape-repeats")?,
                "--live-decisions-per-worker" => {
                    live_decisions_per_worker =
                        parse_positive(&value()?, "--live-decisions-per-worker")?
                }
                "--fixture-search-games" => {
                    fixture_search_games = parse_positive(&value()?, "--fixture-search-games")?
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }
        let git_commit = git_commit.ok_or_else(|| "--git-commit is required".to_string())?;
        if git_commit.len() != 40
            || !git_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("--git-commit must be exactly 40 lowercase hexadecimal characters".into());
        }
        Ok(Self {
            git_commit,
            shape_repeats,
            live_decisions_per_worker,
            fixture_search_games,
        })
    }
}

fn parse_positive(value: &str, option: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{option} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{option} must be positive"));
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct WidthBin {
    width: u32,
    policy_decision_count: u64,
}

#[derive(Debug, Deserialize)]
struct HistogramEnvelope {
    schema_version: String,
    aggregate_record_sha256: String,
    record: HistogramRecord,
}

#[derive(Debug, Deserialize)]
struct HistogramRecord {
    sample_decisions: u64,
    source_artifact_sha256: String,
    source_head: String,
    provenance_class: String,
    performance_gate_valid: bool,
    legal_action_width_histogram: Vec<WidthBin>,
}

fn load_histogram() -> Result<(HistogramEnvelope, String), String> {
    let file_sha = hex_lower(&Sha256::digest(HISTOGRAM_BYTES));
    if file_sha != HISTOGRAM_FILE_SHA256 {
        return Err(format!(
            "histogram file SHA-256 mismatch: expected {HISTOGRAM_FILE_SHA256}, got {file_sha}"
        ));
    }
    let envelope: HistogramEnvelope =
        serde_json::from_slice(HISTOGRAM_BYTES).map_err(|error| error.to_string())?;
    if envelope.schema_version != "kernel_rally_all_policy_legal_action_width_histogram/v1"
        || envelope.aggregate_record_sha256 != AGGREGATE_RECORD_SHA256
        || envelope.record.source_artifact_sha256 != UPSTREAM_RAW_ARTIFACT_SHA256
        || envelope.record.source_head != UPSTREAM_SOURCE_COMMIT
        || envelope.record.sample_decisions != 2_048
        || envelope.record.performance_gate_valid
        || envelope.record.provenance_class
            != "deterministic_workload_shape_only_not_performance_evidence"
        || envelope
            .record
            .legal_action_width_histogram
            .iter()
            .map(|bin| bin.policy_decision_count)
            .sum::<u64>()
            != envelope.record.sample_decisions
    {
        return Err("histogram provenance or shape contract mismatch".into());
    }
    Ok((envelope, file_sha))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

#[derive(Clone)]
struct WidthFixture {
    width: u32,
    decision: FastActorDecisionV1,
    shape: FlatActionDecisionDiagnosticV1,
    session: FastActorSessionV1,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn rally_decks() -> [String; 2] {
    [
        CANONICAL_RALLY_DECK_ID.to_string(),
        CANONICAL_RALLY_DECK_ID.to_string(),
    ]
}

fn collect_width_fixtures(
    histogram: &[WidthBin],
    max_games: u64,
) -> Result<Vec<WidthFixture>, String> {
    let mut fixtures = BTreeMap::new();
    let mut target_widths = BTreeMap::new();
    for bin in histogram {
        target_widths.insert(bin.width, ());
    }
    for game in 0..max_games {
        let episode_id = 9_100_000_u64.wrapping_add(game);
        let mut seed_state = 0x5eed_5eed_a11c_e123_u64 ^ game;
        let env_seed = splitmix64(&mut seed_state);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            MAX_PHYSICAL_DECISIONS,
            MAX_POLICY_STEPS,
            rally_decks(),
        )
        .map_err(|error| error.to_string())?;
        for decision_index in 0..MAX_POLICY_STEPS {
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                break;
            };
            if target_widths.contains_key(&decision.legal_action_count)
                && !fixtures.contains_key(&decision.legal_action_count)
            {
                let shape = session
                    .diagnostic_current_flat_action_shape_v1()
                    .ok_or_else(|| "live decision has no flat-action shape".to_string())?;
                fixtures.insert(
                    decision.legal_action_count,
                    WidthFixture {
                        width: decision.legal_action_count,
                        decision,
                        shape,
                        session: session.clone(),
                    },
                );
                if fixtures.len() == target_widths.len() {
                    return Ok(fixtures.into_values().collect());
                }
            }
            let binding = session
                .diagnostic_current_flat_action_binding_v1()
                .ok_or_else(|| "live decision has no cached binding".to_string())?;
            let selection = (splitmix64(&mut seed_state)
                ^ decision_index.wrapping_mul(0xd134_2543_de82_ef95))
                % u64::from(decision.legal_action_count);
            if matches!(
                session
                    .consume_current_flat_action_slice_v1(binding, selection as u32)
                    .map_err(|error| error.to_string())?,
                FastActorResponseV1::Terminal(_)
            ) {
                break;
            }
        }
    }
    let missing: Vec<_> = target_widths
        .keys()
        .filter(|width| !fixtures.contains_key(width))
        .copied()
        .collect();
    Err(format!(
        "fixture search exhausted {max_games} games; missing legal widths {missing:?}"
    ))
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    EncodeOnly,
    HashOnly,
    CacheRebuildOnly,
    ConsumeOnly,
    Combined,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::EncodeOnly => "encode_only_synthetic_state_rally_width_shaped",
            Self::HashOnly => "hash_only_cached_rows_rally_width_shaped",
            Self::CacheRebuildOnly => "cache_rebuild_only_synthetic_state_rally_width_shaped",
            Self::ConsumeOnly => "consume_only_live_rally",
            Self::Combined => "encode_consume_live_rally",
        }
    }

    fn uses_shape_workload(self) -> bool {
        matches!(
            self,
            Self::EncodeOnly | Self::HashOnly | Self::CacheRebuildOnly
        )
    }
}

#[derive(Clone)]
struct WorkerStats {
    decisions: u64,
    invalids: u64,
    terminals: u64,
    checksum: u64,
    action_widths: [u64; MAX_REPORTED_SHAPE + 1],
    arena_objects: [u64; MAX_REPORTED_SHAPE + 1],
    refs: [u64; MAX_REPORTED_SHAPE + 1],
    referenced_objects: [u64; MAX_REPORTED_SHAPE + 1],
}

impl Default for WorkerStats {
    fn default() -> Self {
        Self {
            decisions: 0,
            invalids: 0,
            terminals: 0,
            checksum: 0,
            action_widths: [0; MAX_REPORTED_SHAPE + 1],
            arena_objects: [0; MAX_REPORTED_SHAPE + 1],
            refs: [0; MAX_REPORTED_SHAPE + 1],
            referenced_objects: [0; MAX_REPORTED_SHAPE + 1],
        }
    }
}

impl WorkerStats {
    fn observe(&mut self, shape: FlatActionDecisionDiagnosticV1) {
        increment_shape(&mut self.action_widths, u64::from(shape.action_count));
        increment_shape(&mut self.arena_objects, u64::from(shape.arena_object_count));
        increment_shape(&mut self.refs, u64::from(shape.ref_count));
        increment_shape(
            &mut self.referenced_objects,
            u64::from(shape.referenced_object_count),
        );
    }

    fn merge(&mut self, other: &Self) {
        self.decisions = self.decisions.saturating_add(other.decisions);
        self.invalids = self.invalids.saturating_add(other.invalids);
        self.terminals = self.terminals.saturating_add(other.terminals);
        self.checksum ^= other.checksum;
        for index in 0..=MAX_REPORTED_SHAPE {
            self.action_widths[index] =
                self.action_widths[index].saturating_add(other.action_widths[index]);
            self.arena_objects[index] =
                self.arena_objects[index].saturating_add(other.arena_objects[index]);
            self.refs[index] = self.refs[index].saturating_add(other.refs[index]);
            self.referenced_objects[index] =
                self.referenced_objects[index].saturating_add(other.referenced_objects[index]);
        }
    }
}

fn increment_shape(histogram: &mut [u64; MAX_REPORTED_SHAPE + 1], value: u64) {
    let index = usize::try_from(value)
        .ok()
        .filter(|index| *index <= MAX_REPORTED_SHAPE)
        .unwrap_or(MAX_REPORTED_SHAPE);
    histogram[index] = histogram[index].saturating_add(1);
}

fn commitment_checksum(commitment: [u8; 16]) -> u64 {
    let mut first = [0_u8; 8];
    first.copy_from_slice(&commitment[..8]);
    u64::from_le_bytes(first)
}

fn run_shape_worker(
    phase: Phase,
    repeats: u64,
    fixtures: &mut [WidthFixture],
    histogram: &[WidthBin],
) -> WorkerStats {
    let mut stats = WorkerStats::default();
    let mut actions = [FlatActionCoreV1::default(); MAX_ACTION_ROWS];
    let mut refs = [FlatActionRefV1::default(); MAX_REF_ROWS];
    let mut objects = [FlatActionObjectV1::default(); MAX_OBJECT_ROWS];
    for _ in 0..repeats {
        for (fixture, bin) in fixtures.iter_mut().zip(histogram) {
            debug_assert_eq!(fixture.width, bin.width);
            for _ in 0..bin.policy_decision_count {
                let result = match phase {
                    Phase::EncodeOnly => fixture
                        .session
                        .encode_current_flat_action_slice_v1(
                            fixture.decision,
                            &mut FlatActionDecisionSliceBuffersV1 {
                                actions: &mut actions,
                                refs: &mut refs,
                                objects: &mut objects,
                            },
                        )
                        .map(|encoded| encoded.binding.candidate_order_commitment),
                    Phase::HashOnly => fixture
                        .session
                        .diagnostic_recompute_flat_action_commitment_v1(),
                    Phase::CacheRebuildOnly => fixture
                        .session
                        .diagnostic_rebuild_current_flat_action_cache_v1(),
                    _ => unreachable!(),
                };
                match result {
                    Ok(commitment) => {
                        stats.decisions += 1;
                        stats.observe(fixture.shape);
                        stats.checksum ^= commitment_checksum(black_box(commitment));
                    }
                    Err(_) => stats.invalids += 1,
                }
            }
        }
    }
    black_box((&actions, &refs, &objects));
    stats
}

fn new_live_session(worker: usize, generation: u64) -> Result<FastActorSessionV1, String> {
    let episode_id = 9_200_000_u64
        .wrapping_add((worker as u64) << 32)
        .wrapping_add(generation);
    let mut seed = 0xa17e_c7ed_5eed_0000_u64 ^ episode_id;
    FastActorSessionV1::reset_with_decks_and_limits(
        episode_id,
        splitmix64(&mut seed),
        MAX_PHYSICAL_DECISIONS,
        MAX_POLICY_STEPS,
        rally_decks(),
    )
    .map_err(|error| error.to_string())
}

fn run_live_worker(phase: Phase, worker: usize, decisions: u64) -> WorkerStats {
    let mut stats = WorkerStats::default();
    let mut generation = 0_u64;
    let mut rng = 0x7c15_9e37_79b9_5eed_u64 ^ worker as u64;
    let mut session = new_live_session(worker, generation).expect("Rally session reset");
    let mut actions = [FlatActionCoreV1::default(); MAX_ACTION_ROWS];
    let mut refs = [FlatActionRefV1::default(); MAX_REF_ROWS];
    let mut objects = [FlatActionObjectV1::default(); MAX_OBJECT_ROWS];
    while stats.decisions + stats.invalids < decisions {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            generation = generation.wrapping_add(1);
            session = new_live_session(worker, generation).expect("Rally session reset");
            continue;
        };
        let Some(shape) = session.diagnostic_current_flat_action_shape_v1() else {
            stats.invalids += 1;
            break;
        };
        let binding = match phase {
            Phase::Combined => session
                .encode_current_flat_action_slice_v1(
                    decision,
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut actions,
                        refs: &mut refs,
                        objects: &mut objects,
                    },
                )
                .map(|encoded| encoded.binding),
            Phase::ConsumeOnly => session.diagnostic_current_flat_action_binding_v1().ok_or(
                mtg_kernel::rl_session::FlatActionDecisionSliceErrorV1::CorruptCurrentBinding,
            ),
            _ => unreachable!(),
        };
        let Ok(binding) = binding else {
            stats.invalids += 1;
            break;
        };
        let selected = (splitmix64(&mut rng) % u64::from(decision.legal_action_count)) as u32;
        match session.consume_current_flat_action_slice_v1(binding, selected) {
            Ok(response) => {
                stats.decisions += 1;
                stats.observe(shape);
                stats.checksum ^= commitment_checksum(binding.candidate_order_commitment)
                    .rotate_left(selected & 63);
                if matches!(response, FastActorResponseV1::Terminal(_)) {
                    stats.terminals += 1;
                    generation = generation.wrapping_add(1);
                    session = new_live_session(worker, generation).expect("Rally session reset");
                }
            }
            Err(_) => {
                stats.invalids += 1;
                break;
            }
        }
    }
    black_box((&actions, &refs, &objects));
    stats
}

#[derive(Debug, Serialize)]
struct ShapeCount {
    value: usize,
    count: u64,
}

fn nonzero_histogram(histogram: &[u64]) -> Vec<ShapeCount> {
    histogram
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, count)| *count != 0)
        .map(|(value, count)| ShapeCount { value, count })
        .collect()
}

fn capture_start_after_workers_ready<T>(
    ready_barrier: &Barrier,
    start_barrier: &Barrier,
    capture_start: impl FnOnce() -> T,
) -> T {
    ready_barrier.wait();
    let start = capture_start();
    start_barrier.wait();
    start
}

fn capture_end_after_workers_finish<T>(
    finish_barrier: &Barrier,
    capture_end: impl FnOnce() -> T,
) -> T {
    finish_barrier.wait();
    capture_end()
}

#[derive(Debug, Serialize)]
struct PhaseRecord {
    phase: &'static str,
    workers: usize,
    common_window_seconds: f64,
    decisions: u64,
    decisions_per_second: f64,
    invalids: u64,
    natural_terminals: u64,
    allocation_census_scope: &'static str,
    allocation_census_decisions: u64,
    allocation_events: u64,
    allocation_requested_bytes: u64,
    allocation_events_per_decision: f64,
    requested_bytes_per_decision: f64,
    nanoseconds_per_decision: f64,
    checksum: String,
    action_width_histogram: Vec<ShapeCount>,
    arena_object_count_histogram: Vec<ShapeCount>,
    action_ref_count_histogram: Vec<ShapeCount>,
    referenced_object_count_histogram: Vec<ShapeCount>,
}

fn run_phase(
    phase: Phase,
    workers: usize,
    config: &Config,
    fixtures: Arc<Vec<WidthFixture>>,
    histogram: Arc<Vec<WidthBin>>,
) -> PhaseRecord {
    let ready_barrier = Arc::new(Barrier::new(workers + 1));
    let start_barrier = Arc::new(Barrier::new(workers + 1));
    let finish_barrier = Arc::new(Barrier::new(workers + 1));
    let mut aggregate = WorkerStats::default();
    let elapsed = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let ready_barrier = Arc::clone(&ready_barrier);
            let start_barrier = Arc::clone(&start_barrier);
            let finish_barrier = Arc::clone(&finish_barrier);
            let fixtures = Arc::clone(&fixtures);
            let histogram = Arc::clone(&histogram);
            handles.push(scope.spawn(move || {
                let mut local_fixtures = phase
                    .uses_shape_workload()
                    .then(|| fixtures.as_ref().clone());
                // All allocations and first-use initialization happen before
                // the common measurement window.
                if phase.uses_shape_workload() {
                    black_box(run_shape_worker(
                        phase,
                        1,
                        local_fixtures.as_deref_mut().unwrap(),
                        &histogram,
                    ));
                } else {
                    black_box(run_live_worker(phase, worker, 256));
                }
                ready_barrier.wait();
                start_barrier.wait();
                let stats = if phase.uses_shape_workload() {
                    run_shape_worker(
                        phase,
                        config.shape_repeats,
                        local_fixtures.as_deref_mut().unwrap(),
                        &histogram,
                    )
                } else {
                    run_live_worker(phase, worker, config.live_decisions_per_worker)
                };
                finish_barrier.wait();
                stats
            }));
        }
        TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
        let start = capture_start_after_workers_ready(
            ready_barrier.as_ref(),
            start_barrier.as_ref(),
            Instant::now,
        );
        let elapsed = capture_end_after_workers_finish(finish_barrier.as_ref(), || start.elapsed());
        for handle in handles {
            aggregate.merge(&handle.join().expect("diagnostic worker panicked"));
        }
        elapsed
    });
    let (allocation_census_decisions, allocation_events, allocation_requested_bytes) =
        run_allocation_census(
            phase,
            workers,
            Arc::clone(&fixtures),
            Arc::clone(&histogram),
        );
    let seconds = elapsed.as_secs_f64();
    let decisions = aggregate.decisions;
    PhaseRecord {
        phase: phase.label(),
        workers,
        common_window_seconds: seconds,
        decisions,
        decisions_per_second: decisions as f64 / seconds,
        invalids: aggregate.invalids,
        natural_terminals: aggregate.terminals,
        allocation_census_scope: if phase.uses_shape_workload() {
            "untimed_sequential_after_one_untracked_full_shape_warmup_per_worker"
        } else {
            "untimed_sequential_live_walk_including_initial_session_reset_per_worker"
        },
        allocation_census_decisions,
        allocation_events,
        allocation_requested_bytes,
        allocation_events_per_decision: allocation_events as f64
            / allocation_census_decisions.max(1) as f64,
        requested_bytes_per_decision: allocation_requested_bytes as f64
            / allocation_census_decisions.max(1) as f64,
        nanoseconds_per_decision: seconds * 1e9 / decisions.max(1) as f64,
        checksum: format!("{:016x}", aggregate.checksum),
        action_width_histogram: nonzero_histogram(&aggregate.action_widths),
        arena_object_count_histogram: nonzero_histogram(&aggregate.arena_objects),
        action_ref_count_histogram: nonzero_histogram(&aggregate.refs),
        referenced_object_count_histogram: nonzero_histogram(&aggregate.referenced_objects),
    }
}

fn run_allocation_census(
    phase: Phase,
    workers: usize,
    fixtures: Arc<Vec<WidthFixture>>,
    histogram: Arc<Vec<WidthBin>>,
) -> (u64, u64, u64) {
    let mut fixture_sets: Vec<_> = if phase.uses_shape_workload() {
        (0..workers).map(|_| fixtures.as_ref().clone()).collect()
    } else {
        Vec::new()
    };
    if phase.uses_shape_workload() {
        for fixture_set in &mut fixture_sets {
            black_box(run_shape_worker(phase, 1, fixture_set, &histogram));
        }
    }
    ALLOCATION_EVENTS.store(0, Ordering::SeqCst);
    ALLOCATION_REQUESTED_BYTES.store(0, Ordering::SeqCst);
    TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
    let mut decisions = 0_u64;
    if phase.uses_shape_workload() {
        for fixture_set in &mut fixture_sets {
            let stats = run_shape_worker(phase, 1, fixture_set, &histogram);
            decisions = decisions.saturating_add(stats.decisions);
        }
    } else {
        for worker in 0..workers {
            let stats = run_live_worker(phase, worker.wrapping_add(10_000), 2_048);
            decisions = decisions.saturating_add(stats.decisions);
        }
    }
    TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
    (
        decisions,
        ALLOCATION_EVENTS.load(Ordering::SeqCst),
        ALLOCATION_REQUESTED_BYTES.load(Ordering::SeqCst),
    )
}

#[derive(Debug, Serialize)]
struct FixtureRecord {
    width: u32,
    episode_id: u64,
    environment_revision: u64,
    arena_object_count: u32,
    action_ref_count: u32,
    referenced_object_count: u16,
}

#[derive(Debug, Serialize)]
struct DiagnosticRecord {
    schema_version: &'static str,
    claim_scope: &'static str,
    git_commit: String,
    histogram_source_commit: &'static str,
    histogram_file_sha256: String,
    aggregate_record_sha256: &'static str,
    upstream_raw_artifact_sha256: &'static str,
    upstream_source_commit: &'static str,
    source_histogram: Vec<WidthBin>,
    fixture_generation: &'static str,
    fixtures: Vec<FixtureRecord>,
    shape_repeats_per_worker: u64,
    live_decisions_per_worker: u64,
    phases: Vec<PhaseRecord>,
    advisory_context: [&'static str; 7],
}

fn main() {
    let result = run();
    match result {
        Ok(record) => println!("{}", serde_json::to_string_pretty(&record).unwrap()),
        Err(error) => {
            eprintln!("flat action encoder diagnostic failed: {error}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<DiagnosticRecord, String> {
    let config = Config::parse()?;
    let (envelope, histogram_file_sha256) = load_histogram()?;
    let fixtures = collect_width_fixtures(
        &envelope.record.legal_action_width_histogram,
        config.fixture_search_games,
    )?;
    if fixtures
        .iter()
        .zip(&envelope.record.legal_action_width_histogram)
        .any(|(fixture, bin)| fixture.width != bin.width)
    {
        return Err("generated fixture widths do not match the bound histogram".into());
    }
    let fixture_records = fixtures
        .iter()
        .map(|fixture| FixtureRecord {
            width: fixture.width,
            episode_id: fixture.decision.episode_id,
            environment_revision: fixture.decision.environment_revision,
            arena_object_count: fixture.shape.arena_object_count,
            action_ref_count: fixture.shape.ref_count,
            referenced_object_count: fixture.shape.referenced_object_count,
        })
        .collect();
    let fixtures = Arc::new(fixtures);
    let histogram = Arc::new(envelope.record.legal_action_width_histogram.clone());
    let mut phases = Vec::new();
    for workers in [1_usize, 16] {
        for phase in [
            Phase::EncodeOnly,
            Phase::HashOnly,
            Phase::CacheRebuildOnly,
            Phase::ConsumeOnly,
            Phase::Combined,
        ] {
            phases.push(run_phase(
                phase,
                workers,
                &config,
                Arc::clone(&fixtures),
                Arc::clone(&histogram),
            ));
        }
    }
    Ok(DiagnosticRecord {
        schema_version: "kernel_flat_action_encoder_diagnostic/v1",
        claim_scope: "noncanonical_environment_only_capacity_diagnostic_not_training_or_xmage_speedup",
        git_commit: config.git_commit,
        histogram_source_commit: HISTOGRAM_SOURCE_COMMIT,
        histogram_file_sha256,
        aggregate_record_sha256: AGGREGATE_RECORD_SHA256,
        upstream_raw_artifact_sha256: UPSTREAM_RAW_ARTIFACT_SHA256,
        upstream_source_commit: UPSTREAM_SOURCE_COMMIT,
        source_histogram: histogram.as_ref().clone(),
        fixture_generation: "independent_deterministic_valid_rally_sessions_repeated_by_bound_width_not_upstream_state_snapshots",
        fixtures: fixture_records,
        shape_repeats_per_worker: config.shape_repeats,
        live_decisions_per_worker: config.live_decisions_per_worker,
        phases,
        advisory_context: [
            "rollout_demand_context_is_approximately_573000_learner_decisions_per_second",
            "aggregate_combined_2500000_decisions_per_second_is_a_continuation_floor_not_a_frozen_gate",
            "hash_only_quantifies_the_once_per_new_decision_v1_sha_over_cached_rows",
            "cache_rebuild_only_attributes_refs_only_resolution_hash_and_steady_state_cache_allocations",
            "consume_only_includes_environment_transition_and_next_decision_cache_construction",
            "allocation_counts_use_a_separate_untimed_sequential_census_to_avoid_perturbing_parallel_rates",
            "a_clean_source_audit_and_true_production_state_encoder_gate_remain_required",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_histogram_identity_and_counts_are_exact() {
        let (envelope, file_sha) = load_histogram().unwrap();
        assert_eq!(file_sha, HISTOGRAM_FILE_SHA256);
        assert_eq!(
            envelope.record.legal_action_width_histogram,
            vec![
                WidthBin {
                    width: 2,
                    policy_decision_count: 844,
                },
                WidthBin {
                    width: 3,
                    policy_decision_count: 249,
                },
                WidthBin {
                    width: 4,
                    policy_decision_count: 202,
                },
                WidthBin {
                    width: 5,
                    policy_decision_count: 179,
                },
                WidthBin {
                    width: 6,
                    policy_decision_count: 139,
                },
                WidthBin {
                    width: 7,
                    policy_decision_count: 141,
                },
                WidthBin {
                    width: 8,
                    policy_decision_count: 126,
                },
                WidthBin {
                    width: 9,
                    policy_decision_count: 82,
                },
                WidthBin {
                    width: 10,
                    policy_decision_count: 46,
                },
                WidthBin {
                    width: 11,
                    policy_decision_count: 25,
                },
                WidthBin {
                    width: 12,
                    policy_decision_count: 11,
                },
                WidthBin {
                    width: 13,
                    policy_decision_count: 4,
                },
            ]
        );
    }

    #[test]
    fn generated_width_fixtures_are_deterministic_and_shape_bound() {
        let (envelope, _) = load_histogram().unwrap();
        let fixtures =
            collect_width_fixtures(&envelope.record.legal_action_width_histogram, 8).unwrap();
        let observed: Vec<_> = fixtures
            .iter()
            .map(|fixture| {
                (
                    fixture.width,
                    fixture.decision.episode_id,
                    fixture.decision.environment_revision,
                    fixture.shape.arena_object_count,
                    fixture.shape.ref_count,
                    fixture.shape.referenced_object_count,
                )
            })
            .collect();
        assert_eq!(
            observed,
            vec![
                (2, 9_100_000, 0, 120, 1, 1),
                (3, 9_100_000, 20, 121, 2, 2),
                (4, 9_100_000, 18, 121, 6, 3),
                (5, 9_100_000, 24, 121, 4, 4),
                (6, 9_100_000, 15, 120, 5, 5),
                (7, 9_100_000, 2, 120, 6, 6),
                (8, 9_100_000, 1, 120, 7, 7),
                (9, 9_100_000, 4, 120, 8, 8),
                (10, 9_100_001, 172, 128, 9, 9),
                (11, 9_100_001, 171, 128, 10, 10),
                (12, 9_100_002, 285, 128, 11, 11),
                (13, 9_100_007, 177, 127, 12, 12),
            ]
        );
    }

    #[test]
    fn common_window_excludes_preparation_and_uses_a_common_finish() {
        const WORKERS: usize = 4;
        let ready_barrier = Arc::new(Barrier::new(WORKERS + 1));
        let start_barrier = Arc::new(Barrier::new(WORKERS + 1));
        let finish_barrier = Arc::new(Barrier::new(WORKERS + 1));
        let prepared: Arc<Vec<_>> =
            Arc::new((0..WORKERS).map(|_| AtomicBool::new(false)).collect());
        let measured: Arc<Vec<_>> =
            Arc::new((0..WORKERS).map(|_| AtomicBool::new(false)).collect());
        let start_captured = Arc::new(AtomicBool::new(false));

        std::thread::scope(|scope| {
            for worker in 0..WORKERS {
                let ready_barrier = Arc::clone(&ready_barrier);
                let start_barrier = Arc::clone(&start_barrier);
                let finish_barrier = Arc::clone(&finish_barrier);
                let prepared = Arc::clone(&prepared);
                let measured = Arc::clone(&measured);
                let start_captured = Arc::clone(&start_captured);
                scope.spawn(move || {
                    prepared[worker].store(true, Ordering::SeqCst);
                    ready_barrier.wait();
                    start_barrier.wait();
                    assert!(start_captured.load(Ordering::SeqCst));
                    measured[worker].store(true, Ordering::SeqCst);
                    finish_barrier.wait();
                });
            }

            capture_start_after_workers_ready(
                ready_barrier.as_ref(),
                start_barrier.as_ref(),
                || {
                    assert!(prepared.iter().all(|flag| flag.load(Ordering::SeqCst)));
                    assert!(measured.iter().all(|flag| !flag.load(Ordering::SeqCst)));
                    start_captured.store(true, Ordering::SeqCst);
                },
            );
            capture_end_after_workers_finish(finish_barrier.as_ref(), || {
                assert!(measured.iter().all(|flag| flag.load(Ordering::SeqCst)));
            });
        });
    }
}
