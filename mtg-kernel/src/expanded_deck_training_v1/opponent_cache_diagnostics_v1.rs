//! Diagnostic counters for the opponent caches and the verified-bytes memo
//! (#599 milestone 2): misses by cache and reason, hit latency, retained and
//! duplicated bytes, evictions, and how opponent work splits into file
//! reads, byte verification, checkpoint decoding and the rest of a load.
//!
//! Counting is always compiled. It is relaxed atomic additions and clock
//! reads on paths that already read, hash or parse whole files, never inside
//! a forward. No counter is read by any decision, so no output, receipt,
//! wire byte or error changes. Counters are cumulative from process start.
//! Under parallel collection, how the counts split between workers depends
//! on timing, and so do some totals (two workers can miss one source at
//! once). For that reason the counters are kept out of every receipt and
//! leave the process only through the opt-in diagnostics file
//! (`cache_diagnostics_v1`), as the sections [`sections_v1`] returns.
//!
//! The verified-bytes memo keeps its own counters (`VerifiedBytesStatsV1`),
//! so a test's private memo counts only that test's reads. This module holds
//! the process-wide ones ([`counters_v1`]): the process opponent cache
//! (`ordered_update_preparation`), the collection opponent caches
//! (`OpponentCacheV1`, one per collector), the collection boundary's pin
//! checks, file reads and checkpoint decodes, plus the per-thread work
//! totals that attribute reads, verification and decoding to one load. Unit
//! tests run in parallel threads, so there each test thread keeps its own
//! process-wide counters, as `loaded_source_profile_v1` does.

use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// One cumulative counter.
#[derive(Default)]
pub(crate) struct CountV1(AtomicU64);

impl CountV1 {
    #[cfg(test)]
    pub(crate) const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub(crate) fn add(&self, amount: u64) {
        self.incremented_to(amount);
    }

    fn incremented_to(&self, amount: u64) -> u64 {
        self.0
            .fetch_add(amount, Ordering::Relaxed)
            .wrapping_add(amount)
    }

    pub(crate) fn add_usize(&self, amount: usize) {
        self.add(amount as u64);
    }

    pub(crate) fn add_elapsed(&self, started: Instant) {
        self.add(nanos_since_v1(started));
    }

    /// Raises the counter to `value` if it is lower: a peak.
    pub(crate) fn raise(&self, value: u64) {
        self.0.fetch_max(value, Ordering::Relaxed);
    }

    /// Lowers a level counter, never below zero.
    pub(crate) fn sub(&self, amount: u64) {
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(amount))
            });
    }

    pub(crate) fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    /// Nanoseconds as seconds, for a time counter.
    pub(crate) fn seconds(&self) -> f64 {
        self.get() as f64 * 1e-9
    }
}

pub(crate) fn nanos_since_v1(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// The work a thread accumulates, so that a caller can attribute the reads,
/// verification and decoding done inside one of its calls to that call.
#[derive(Clone, Copy)]
pub(crate) enum WorkV1 {
    /// Opening, stat and reading pinned files (`read_pinned_bytes`, the
    /// fresh-source reader).
    Read = 0,
    /// Hashing, or comparing with held bytes, in the verified-bytes memo.
    Verify = 1,
    /// Decoding a checkpoint (`read_checkpoint_v1`), either format.
    Decode = 2,
}

const WORK_KINDS_V1: usize = 3;

thread_local! {
    static THREAD_WORK_NANOS_V1: [Cell<u64>; WORK_KINDS_V1] =
        const { [Cell::new(0), Cell::new(0), Cell::new(0)] };
}

/// Adds `nanos` of `kind` to this thread's total.
pub(crate) fn add_thread_work_v1(kind: WorkV1, nanos: u64) {
    THREAD_WORK_NANOS_V1.with(|work| {
        let cell = &work[kind as usize];
        cell.set(cell.get().saturating_add(nanos));
    });
}

fn thread_work_v1() -> [u64; WORK_KINDS_V1] {
    THREAD_WORK_NANOS_V1.with(|work| [work[0].get(), work[1].get(), work[2].get()])
}

/// A start time and this thread's work totals at that time.
pub(crate) struct WorkMarkV1 {
    started: Instant,
    work: [u64; WORK_KINDS_V1],
}

impl WorkMarkV1 {
    pub(crate) fn now_v1() -> Self {
        Self {
            started: Instant::now(),
            work: thread_work_v1(),
        }
    }

    /// Adds the elapsed time since the mark to `split.total` and this
    /// thread's reads, verification and decoding since the mark to their
    /// counters; what is left of the elapsed time is the rest of the call.
    pub(crate) fn finish_v1(&self, split: &SplitV1) {
        let elapsed = nanos_since_v1(self.started);
        let now = thread_work_v1();
        split.total.add(elapsed);
        split.read.add(now[0].saturating_sub(self.work[0]));
        split.verify.add(now[1].saturating_sub(self.work[1]));
        split.decode.add(now[2].saturating_sub(self.work[2]));
    }
}

/// Elapsed time of one kind of call, and the part of it spent reading,
/// verifying and decoding on the calling thread.
#[derive(Default)]
pub(crate) struct SplitV1 {
    pub(crate) total: CountV1,
    pub(crate) read: CountV1,
    pub(crate) verify: CountV1,
    pub(crate) decode: CountV1,
}

impl SplitV1 {
    fn report_v1(&self) -> Value {
        let total = self.total.get();
        let parts = self.read.get() + self.verify.get() + self.decode.get();
        json!({
            "seconds": self.total.seconds(),
            "read_seconds": self.read.seconds(),
            "verify_seconds": self.verify.seconds(),
            "decode_seconds": self.decode.seconds(),
            "other_seconds": total.saturating_sub(parts) as f64 * 1e-9,
        })
    }
}

/// The process-wide counters.
#[derive(Default)]
pub(crate) struct ProcessCountersV1 {
    pub(crate) file_reads: FileReadsV1,
    pub(crate) checkpoint_decodes: CheckpointDecodesV1,
    pub(crate) process_opponent: ProcessOpponentStatsV1,
    pub(crate) collection_opponent: CollectionOpponentStatsV1,
    pub(crate) boundary: BoundaryStatsV1,
}

/// This process's counters.
#[cfg(not(test))]
pub(crate) fn counters_v1() -> &'static ProcessCountersV1 {
    static COUNTERS: std::sync::OnceLock<ProcessCountersV1> = std::sync::OnceLock::new();
    COUNTERS.get_or_init(ProcessCountersV1::default)
}

/// This test thread's counters.
#[cfg(test)]
pub(crate) fn counters_v1() -> &'static ProcessCountersV1 {
    thread_local! {
        static COUNTERS: &'static ProcessCountersV1 =
            Box::leak(Box::new(ProcessCountersV1::default()));
    }
    COUNTERS.with(|counters| *counters)
}

/// Pinned file reads (`read_pinned_bytes`, the fresh-source reader): count,
/// bytes and time, over every caller. A process opponent cache hit's rereads
/// (`holds_bytes_v1`) are its revalidation, not counted here.
#[derive(Default)]
pub(crate) struct FileReadsV1 {
    pub(crate) reads: CountV1,
    pub(crate) bytes: CountV1,
    pub(crate) nanos: CountV1,
}

/// Records one pinned file read of `bytes` bytes that began at `started`.
pub(crate) fn note_file_read_v1(bytes: usize, started: Instant) {
    let nanos = nanos_since_v1(started);
    let reads = &counters_v1().file_reads;
    reads.reads.add(1);
    reads.bytes.add_usize(bytes);
    reads.nanos.add(nanos);
    add_thread_work_v1(WorkV1::Read, nanos);
}

/// Checkpoint decodes (`read_checkpoint_v1`, either format), over every
/// caller: learner loads, validation and opponent loads alike.
#[derive(Default)]
pub(crate) struct CheckpointDecodesV1 {
    pub(crate) decodes: CountV1,
    pub(crate) bytes: CountV1,
    pub(crate) nanos: CountV1,
}

/// Records one checkpoint decode of `bytes` record bytes that began at
/// `started`.
pub(crate) fn note_checkpoint_decode_v1(bytes: usize, started: Instant) {
    let nanos = nanos_since_v1(started);
    let decodes = &counters_v1().checkpoint_decodes;
    decodes.decodes.add(1);
    decodes.bytes.add_usize(bytes);
    decodes.nanos.add(nanos);
    add_thread_work_v1(WorkV1::Decode, nanos);
}

/// The process opponent cache (`ordered_update_preparation`).
#[derive(Default)]
pub(crate) struct ProcessOpponentStatsV1 {
    /// Lookups, and their outcome. A hit serves a fork of the held policy;
    /// `revalidated` hits first recheck every held pin (a prepared update),
    /// `trusted` ones follow the collection boundary's pin check.
    pub(crate) lookups: CountV1,
    pub(crate) hits_revalidated: CountV1,
    pub(crate) hits_trusted: CountV1,
    /// A hit's whole time: lookup, pin checks and fork. Its maximum, and the
    /// pin checks and forks on their own.
    pub(crate) hit_nanos: CountV1,
    pub(crate) hit_max_nanos: CountV1,
    /// Payload covered by successful hit pin checks, including resident checks.
    pub(crate) revalidated_payload_bytes: CountV1,
    pub(crate) revalidation_nanos: CountV1,
    pub(crate) fork_nanos: CountV1,
    /// Forks served, by hits and by kept full loads, and the bytes they
    /// copied (`policy_bytes_v1`); the model itself is shared.
    pub(crate) forks: CountV1,
    pub(crate) forked_policy_bytes: CountV1,
    /// Misses, by reason: nothing held for the source; held only under the
    /// other per-forward check setting; a held file that changed or could
    /// not be read again.
    pub(crate) misses_absent: CountV1,
    pub(crate) misses_check_setting: CountV1,
    pub(crate) misses_revalidation: CountV1,
    /// Full loads after a miss, split into reads, verification, decoding
    /// and the rest; then rereading the load's files for the entry.
    pub(crate) full_load: SplitV1,
    pub(crate) full_loads: CountV1,
    pub(crate) capture: SplitV1,
    pub(crate) captured_bytes: CountV1,
    /// What became of a full load: kept; not kept, because the cache could
    /// not reverify its files exactly or its policy cannot be forked.
    pub(crate) kept: CountV1,
    pub(crate) not_kept_unverifiable: CountV1,
    pub(crate) not_kept_unforkable: CountV1,
    /// Entries dropped for the bound, with their held file bytes, and
    /// entries replaced by a concurrent load of the same source.
    pub(crate) evictions: CountV1,
    pub(crate) evicted_file_bytes: CountV1,
    pub(crate) replaced: CountV1,
}

/// The collection opponent caches (`OpponentCacheV1`), summed over every
/// one: each collector's, and the S learner's and the replay's.
#[derive(Default)]
pub(crate) struct CollectionOpponentStatsV1 {
    /// Loads served by the collector's own entry, and their time.
    pub(crate) hits: CountV1,
    pub(crate) hit_nanos: CountV1,
    /// Loads that missed it: through the process opponent cache (a source
    /// the boundary verified), or a full load of an unverified source; their
    /// time includes the process cache's.
    pub(crate) misses_verified: CountV1,
    pub(crate) misses_unverified: CountV1,
    pub(crate) miss_nanos: CountV1,
    /// Entries inserted (misses and current-self forks), and dropped for the
    /// bound.
    pub(crate) inserts: CountV1,
    pub(crate) evictions: CountV1,
    /// Policies held now by every live collection cache, the bytes of
    /// their own copies (`policy_bytes_v1`), and the peaks of both. A
    /// policy's model is not counted: a fork shares its model with the
    /// process cache's entry, counted there, and only a source loaded in
    /// full (`misses_unverified`) holds a model of its own.
    pub(crate) live_entries: CountV1,
    pub(crate) peak_live_entries: CountV1,
    pub(crate) live_policy_bytes: CountV1,
    pub(crate) peak_live_policy_bytes: CountV1,
}

impl CollectionOpponentStatsV1 {
    /// One policy of `bytes` owned bytes now held by a collection cache.
    pub(crate) fn held_v1(&self, bytes: usize) {
        self.inserts.add(1);
        self.peak_live_entries
            .raise(self.live_entries.incremented_to(1));
        self.peak_live_policy_bytes
            .raise(self.live_policy_bytes.incremented_to(bytes as u64));
    }

    /// One held policy of `bytes` owned bytes dropped.
    pub(crate) fn released_v1(&self, bytes: usize) {
        self.live_entries.sub(1);
        self.live_policy_bytes.sub(bytes as u64);
    }
}

/// The bytes a policy's own copies hold, which each fork and each
/// collection cache entry duplicates: its embedding table and any boxed
/// no-state-digest check model (`FrozenPlayPolicyV1::owned_f32_len_v1`),
/// four bytes each. Its model is shared with its forks (`model_bytes_v1`).
pub(crate) fn policy_bytes_v1(
    policy: &crate::sideboard_play_policy_v1::FrozenPlayPolicyV1,
) -> usize {
    policy.owned_f32_len_v1() * std::mem::size_of::<f32>()
}

/// The model `policy` shares with its forks: an identity that tells live
/// models apart, and its bytes, with each linear layer's transposed weight
/// (`FrozenPlayPolicyV1::shared_model_f32_v1`).
pub(crate) fn model_bytes_v1(
    policy: &crate::sideboard_play_policy_v1::FrozenPlayPolicyV1,
) -> (usize, usize) {
    let (identity, values) = policy.shared_model_f32_v1();
    (identity, values * std::mem::size_of::<f32>())
}

/// The collection boundary's pin check of a collection's opponent sources
/// (`verify_collection_opponent_sources_v1`).
#[derive(Default)]
pub(crate) struct BoundaryStatsV1 {
    pub(crate) checks: CountV1,
    /// Distinct sources checked, and those the process cache may serve.
    pub(crate) sources: CountV1,
    pub(crate) cacheable_sources: CountV1,
    pub(crate) split: SplitV1,
}

/// The `opponent_cache` section: the process opponent cache, with what it
/// holds now, the collection caches and the collection boundary.
pub(crate) fn opponent_cache_section_v1() -> Value {
    let counters = counters_v1();
    let process = &counters.process_opponent;
    let collection = &counters.collection_opponent;
    let boundary = &counters.boundary;
    let held = super::ordered_update_preparation::held_v1();
    let files = file_bytes_v1(&held.holders_v1());
    json!({
        "process": {
            "entry_limit": held.entry_limit,
            "entries": held.files.len(),
            "file_bytes": files.total,
            "unique_file_bytes": files.unique,
            "duplicate_file_bytes": files.total - files.unique,
            "policy_bytes": held.policy_bytes,
            "models": held.models,
            "model_bytes": held.model_bytes,
            "lookups": process.lookups.get(),
            "hits": {
                "revalidated": process.hits_revalidated.get(),
                "trusted": process.hits_trusted.get(),
            },
            "hit_seconds": process.hit_nanos.seconds(),
            "hit_max_seconds": process.hit_max_nanos.seconds(),
            "revalidated_payload_bytes": process.revalidated_payload_bytes.get(),
            "revalidation_seconds": process.revalidation_nanos.seconds(),
            "fork_seconds": process.fork_nanos.seconds(),
            "forks": process.forks.get(),
            "forked_policy_bytes": process.forked_policy_bytes.get(),
            "misses": {
                "absent": process.misses_absent.get(),
                "check_setting": process.misses_check_setting.get(),
                "revalidation": process.misses_revalidation.get(),
            },
            "full_loads": process.full_loads.get(),
            "full_load": process.full_load.report_v1(),
            "capture": process.capture.report_v1(),
            "captured_bytes": process.captured_bytes.get(),
            "kept": process.kept.get(),
            "not_kept": {
                "unverifiable": process.not_kept_unverifiable.get(),
                "unforkable": process.not_kept_unforkable.get(),
            },
            "evictions": process.evictions.get(),
            "evicted_file_bytes": process.evicted_file_bytes.get(),
            "replaced": process.replaced.get(),
        },
        "collection": {
            "entry_limit": super::OPPONENT_CACHE_CAPACITY_V1,
            "hits": collection.hits.get(),
            "hit_seconds": collection.hit_nanos.seconds(),
            "misses": {
                "verified": collection.misses_verified.get(),
                "unverified": collection.misses_unverified.get(),
            },
            "miss_seconds": collection.miss_nanos.seconds(),
            "inserts": collection.inserts.get(),
            "evictions": collection.evictions.get(),
            "live_entries": collection.live_entries.get(),
            "peak_live_entries": collection.peak_live_entries.get(),
            "live_policy_bytes": collection.live_policy_bytes.get(),
            "peak_live_policy_bytes": collection.peak_live_policy_bytes.get(),
        },
        "boundary": {
            "checks": boundary.checks.get(),
            "sources": boundary.sources.get(),
            "cacheable_sources": boundary.cacheable_sources.get(),
            "time": boundary.split.report_v1(),
        },
    })
}

/// The `verified_bytes` section: the process memo's counters and what it
/// holds now.
pub(crate) fn verified_bytes_section_v1() -> Value {
    super::verified_bytes_cache_v1::global_v1().diagnostics_v1()
}

/// The `retained_files` section: file bytes the memo and the process
/// opponent cache hold now, and how much of it is the same content held
/// more than once, within the opponent cache (a shared import kept by every
/// entry) or as a second copy in the other cache.
pub(crate) fn retained_files_section_v1() -> Value {
    let memo = super::verified_bytes_cache_v1::global_v1().held_files_v1();
    let held = super::ordered_update_preparation::held_v1();
    let opponent = held.holders_v1();
    let mut both: Vec<&dyn HeldFilesV1> = vec![&memo];
    both.extend(opponent.iter().copied());
    let memo_files = file_bytes_v1(&[&memo]);
    let opponent_files = file_bytes_v1(&opponent);
    let all = file_bytes_v1(&both);
    json!({
        "memo_bytes": memo_files.total,
        "opponent_cache_bytes": opponent_files.total,
        "total_bytes": all.total,
        "unique_bytes": all.unique,
        "duplicate_bytes": all.total - all.unique,
        // Opponent cache content of which the memo holds a separate copy.
        "opponent_cache_bytes_also_in_memo":
            memo_files.unique + opponent_files.unique - all.unique,
    })
}

/// File reads and checkpoint decodes over every caller.
pub(crate) fn io_section_v1() -> Value {
    let counters = counters_v1();
    let (reads, decodes) = (&counters.file_reads, &counters.checkpoint_decodes);
    json!({
        "file_reads": {
            "reads": reads.reads.get(),
            "bytes": reads.bytes.get(),
            "seconds": reads.nanos.seconds(),
        },
        "checkpoint_decodes": {
            "decodes": decodes.decodes.get(),
            "bytes": decodes.bytes.get(),
            "seconds": decodes.nanos.seconds(),
        },
    })
}

/// The `process` section: peak and current resident memory and CPU time,
/// where the system reports them (`null` otherwise).
pub(crate) fn process_section_v1() -> Value {
    let (peak, current) = resident_bytes_v1();
    json!({
        "peak_rss_bytes": peak,
        "rss_bytes": current,
        "cpu_seconds": crate::regret_census_v1::stage4a::process_cpu::seconds().ok(),
    })
}

/// Peak and current resident set size in bytes.
#[cfg(target_os = "linux")]
fn resident_bytes_v1() -> (Option<u64>, Option<u64>) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return (None, None);
    };
    let kib = |key: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|rest| rest.trim().strip_suffix("kB"))
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|value| value * 1024)
    };
    (kib("VmHWM:"), kib("VmRSS:"))
}

/// Peak working set in bytes; the current one is not read.
#[cfg(windows)]
fn resident_bytes_v1() -> (Option<u64>, Option<u64>) {
    (
        crate::kernel_native_search_calibration_runner_v1::peak_working_set_bytes_v1(),
        None,
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
fn resident_bytes_v1() -> (Option<u64>, Option<u64>) {
    (None, None)
}

/// Every diagnostics section of this module, by name, for the diagnostics
/// file (`cache_diagnostics_v1`).
pub(crate) fn sections_v1() -> Vec<(&'static str, Value)> {
    vec![
        ("opponent_cache", opponent_cache_section_v1()),
        ("verified_bytes", verified_bytes_section_v1()),
        ("retained_files", retained_files_section_v1()),
        ("io", io_section_v1()),
        ("process", process_section_v1()),
    ]
}

/// Total bytes of a set of held files, and of their distinct contents: two
/// files are the same content when they have the same path and equal bytes.
pub(crate) struct FileBytesV1 {
    pub(crate) total: u64,
    pub(crate) unique: u64,
}

pub(crate) type HeldFileV1 = (PathBuf, Arc<Vec<u8>>);

/// The files of several holders, each a list of `(path, bytes)` pairs.
pub(crate) fn file_bytes_v1(holders: &[&dyn HeldFilesV1]) -> FileBytesV1 {
    let mut total = 0u64;
    let mut unique = 0u64;
    let mut distinct: HashMap<&Path, Vec<&[u8]>> = HashMap::new();
    for holder in holders {
        for (path, bytes) in holder.files_v1() {
            total += bytes.len() as u64;
            let seen = distinct.entry(path).or_default();
            if !seen
                .iter()
                .any(|held| held.as_ptr() == bytes.as_ptr() || *held == bytes)
            {
                unique += bytes.len() as u64;
                seen.push(bytes);
            }
        }
    }
    FileBytesV1 { total, unique }
}

/// A holder of files: the memo's entries or one opponent cache entry's.
pub(crate) trait HeldFilesV1 {
    fn files_v1(&self) -> Box<dyn Iterator<Item = (&Path, &[u8])> + '_>;
}

impl HeldFilesV1 for Vec<HeldFileV1> {
    fn files_v1(&self) -> Box<dyn Iterator<Item = (&Path, &[u8])> + '_> {
        Box::new(
            self.iter()
                .map(|(path, bytes)| (path.as_path(), bytes.as_slice())),
        )
    }
}

impl HeldFilesV1 for super::VerifiedFilesV1 {
    fn files_v1(&self) -> Box<dyn Iterator<Item = (&Path, &[u8])> + '_> {
        Box::new(
            self.iter()
                .map(|(path, bytes)| (path.as_path(), bytes.as_slice())),
        )
    }
}

#[cfg(test)]
mod tests;
