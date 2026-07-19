//! Allocation and warmed-time census for one production V2 tensor decision.
//!
//! This is an environment-only tensorizer microprofile. It is not trainer
//! throughput, an XMage comparison, or science-ready performance evidence.

use mtg_kernel::native_flat_tensorizer_diagnostic_v1::NativeFlatTensorizerDiagnosticFixtureV1;
use serde::Serialize;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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

#[derive(Serialize)]
struct Record {
    schema_version: &'static str,
    claim_scope: &'static str,
    fixture: &'static str,
    timing_iterations: u64,
    elapsed_nanoseconds: u128,
    nanoseconds_per_decision: f64,
    allocation_census_iterations: u64,
    allocation_events: u64,
    requested_bytes: u64,
    allocation_events_per_decision: f64,
    requested_bytes_per_decision: f64,
    object_rows: usize,
    edge_rows: usize,
    action_rows: usize,
    action_ref_rows: usize,
    checksum_hex: String,
}

fn parse_iterations() -> Result<(u64, u64), String> {
    let mut timing = 10_000_u64;
    let mut allocations = 1_000_u64;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{argument} requires a positive integer"))?;
        let parsed = value
            .parse::<u64>()
            .map_err(|_| format!("{argument} requires a positive integer"))?;
        if parsed == 0 {
            return Err(format!("{argument} requires a positive integer"));
        }
        match argument.as_str() {
            "--timing-iterations" => timing = parsed,
            "--allocation-iterations" => allocations = parsed,
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    Ok((timing, allocations))
}

fn run() -> Result<Record, String> {
    let (timing_iterations, allocation_iterations) = parse_iterations()?;
    let mut fixture = NativeFlatTensorizerDiagnosticFixtureV1::production_burn_combat_v1()?;
    fixture.run_v1(128)?;
    let timing = fixture.run_v1(timing_iterations)?;

    ALLOCATION_EVENTS.store(0, Ordering::SeqCst);
    ALLOCATION_REQUESTED_BYTES.store(0, Ordering::SeqCst);
    TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
    let allocation = fixture.run_v1(allocation_iterations);
    TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
    let allocation = allocation?;
    let allocation_events = ALLOCATION_EVENTS.load(Ordering::SeqCst);
    let requested_bytes = ALLOCATION_REQUESTED_BYTES.load(Ordering::SeqCst);

    Ok(Record {
        schema_version: "kernel_native_flat_tensorizer_diagnostic/v1",
        claim_scope:
            "environment_only_production_tensorizer_microprofile_not_trainer_or_xmage_speedup",
        fixture: "python_full_features_v2/burn-mirror-combat-production-replay",
        timing_iterations,
        elapsed_nanoseconds: timing.elapsed_nanoseconds,
        nanoseconds_per_decision: timing.elapsed_nanoseconds as f64 / timing.iterations as f64,
        allocation_census_iterations: allocation_iterations,
        allocation_events,
        requested_bytes,
        allocation_events_per_decision: allocation_events as f64 / allocation_iterations as f64,
        requested_bytes_per_decision: requested_bytes as f64 / allocation_iterations as f64,
        object_rows: allocation.object_rows,
        edge_rows: allocation.edge_rows,
        action_rows: allocation.action_rows,
        action_ref_rows: allocation.action_ref_rows,
        checksum_hex: format!("{:016x}", timing.checksum),
    })
}

fn main() {
    match run() {
        Ok(record) => println!("{}", serde_json::to_string_pretty(&record).unwrap()),
        Err(error) => {
            eprintln!("native flat tensorizer diagnostic failed: {error}");
            std::process::exit(2);
        }
    }
}
