//! Process-wide memo of file bytes already verified against a SHA-256 pin.
//!
//! A native training run reads the same pinned files several times per
//! update and hashes them at every read: the learner checkpoint (about 20 MB)
//! about five times, the fresh parameter payload (about 5 MB) four or more
//! times, and every trajectory (about 6 MB) up to three times, though this
//! process wrote the checkpoint and the trajectories itself and verified the
//! payload at its first read.
//!
//! Every read still happens in full, with its metadata and size checks. When
//! this process holds bytes for the same path whose SHA-256 is the pinned one,
//! the bytes read are compared with them in full instead of hashed: equal
//! bytes have that digest. Any difference, or no held bytes, falls back to
//! hashing, so every failure reads exactly as before.
//!
//! Bytes enter the memo only with their digest established: a read that
//! hashed them and matched its pin (`digest_matches_v1`), or a durable
//! publication (`publish_json_bytes_v1`), whose receipt digest is computed
//! from the published bytes and whose final file it recaptured byte for
//! byte. Every caller's digest is the lowercase hex SHA-256 of the bytes, so
//! an entry recorded by one caller proves the pin of every other.
//!
//! Readers take a cache handle: production passes the process cache
//! (`global_v1`), and tests pass their own.
//!
//! `MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB` sets the process cache's budget
//! (default 160; 0 turns the memo off). An entry is charged its bytes, its
//! path, its digest and `ENTRY_OVERHEAD_BYTES_V1`, so empty files count too,
//! and at most `ENTRY_LIMIT_V1` entries are held. Least recently used entries
//! are dropped before a new one is copied in, so held entries never exceed
//! either bound, and an entry whose charge exceeds the whole budget is not
//! held. A dropped entry's bytes live on only while a reader that took it
//! before the drop finishes its comparison (at most one file per concurrent
//! reader).
//!
//! Resident reads (`verified_reads: "resident"`, an opt-in run setting) go
//! one step further for a process that writes and rereads its own records at
//! memory-bandwidth cost. With the mode on, a reader whose pin this process
//! holds verified bytes for still checks the file's metadata (a regular file
//! of exactly the held length) but takes the held bytes instead of reading
//! the file again: every byte a reader returns still has the pinned SHA-256,
//! established when it entered the memo, so every output is unchanged. What
//! the mode gives up is noticing that a file this process already verified
//! was rewritten with the same length later in the same process; a new
//! process (a segment, a resume, an audit) reads and verifies every file
//! again. The other process-cache comparisons (`holds_bytes_v1`) and durable
//! publication follow the same switch. The mode only ever turns on, never off.
//!
//! Each cache counts its lookups, hits, misses by reason, digests, records,
//! copies, evictions and resident serves (`VerifiedBytesStatsV1`), for the
//! diagnostics file (`opponent_cache_diagnostics_v1`); no count is read by
//! any decision.

use super::opponent_cache_diagnostics_v1::{
    add_thread_work_v1, nanos_since_v1, CountV1, HeldFileV1, WorkV1,
};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Instant;

#[cfg(test)]
mod tests;

const DEFAULT_BUDGET_MIB_V1: usize = 160;
/// Charged per entry beside its bytes, path and digest: the entry, its `Arc`
/// and the allocations' bookkeeping, rounded up.
const ENTRY_OVERHEAD_BYTES_V1: usize = 256;
/// A run holds a few dozen pins per update; this only stops tiny files from
/// accumulating entries.
const ENTRY_LIMIT_V1: usize = 4096;

/// The process cache every production reader uses.
pub(crate) fn global_v1() -> &'static VerifiedBytesCacheV1 {
    static CACHE: OnceLock<VerifiedBytesCacheV1> = OnceLock::new();
    CACHE.get_or_init(|| {
        let budget = std::env::var("MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_BUDGET_MIB_V1)
            .saturating_mul(1 << 20);
        VerifiedBytesCacheV1::new_v1(budget, ENTRY_LIMIT_V1)
    })
}

/// What an entry for `path`, `sha256` and `bytes_len` bytes is charged.
fn charge_v1(path: &Path, sha256: &str, bytes_len: usize) -> usize {
    bytes_len
        .saturating_add(path.as_os_str().len())
        .saturating_add(sha256.len())
        .saturating_add(ENTRY_OVERHEAD_BYTES_V1)
}

struct EntryV1 {
    path: PathBuf,
    sha256: String,
    bytes: Arc<Vec<u8>>,
    charge: usize,
}

#[derive(Default)]
struct HeldV1 {
    /// Least recently used first; a hit or a re-record moves to the back.
    entries: VecDeque<EntryV1>,
    /// The sum of the entries' charges.
    charged: usize,
}

/// A cache's diagnostic counters, cumulative from its creation.
#[derive(Default)]
struct VerifiedBytesStatsV1 {
    /// `held_v1` calls, from every caller, and those that found bytes.
    lookups: CountV1,
    lookup_hits: CountV1,
    /// `digest_matches_v1` calls answered by comparison with held bytes,
    /// and their bytes; the time of every lookup and comparison.
    compared_hits: CountV1,
    compared_hit_bytes: CountV1,
    compare_nanos: CountV1,
    /// `digest_matches_v1` misses, by reason: the cache is off, it holds no
    /// bytes for the pin, or the held bytes differ from the bytes read.
    misses_disabled: CountV1,
    misses_absent: CountV1,
    misses_differ: CountV1,
    /// The digests those misses computed, their bytes and time, and the
    /// digests that differed from the pin.
    hashed: CountV1,
    hashed_bytes: CountV1,
    hash_nanos: CountV1,
    hash_mismatches: CountV1,
    /// Entries held and their bytes, and the bytes copied for them (a
    /// shared record keeps the caller's allocation); entries refused
    /// (charged above the budget, or the cache off); re-records of a held
    /// pin; entries dropped for the bounds and their bytes; the peak charge.
    records: CountV1,
    recorded_bytes: CountV1,
    copied_bytes: CountV1,
    records_refused: CountV1,
    rerecords: CountV1,
    evictions: CountV1,
    evicted_bytes: CountV1,
    peak_charged: CountV1,
    /// With resident reads on, reads served from held bytes instead of the
    /// file, and their bytes.
    resident_hits: CountV1,
    resident_hit_bytes: CountV1,
}

pub(crate) struct VerifiedBytesCacheV1 {
    /// Bound on the held entries' charges; 0 turns the cache off.
    budget: usize,
    /// Bound on the number of held entries.
    entry_limit: usize,
    held: Mutex<HeldV1>,
    /// Resident reads (module docs): set once by a run, never cleared.
    resident: AtomicBool,
    stats: VerifiedBytesStatsV1,
    /// Digests computed by `digest_matches_v1`, so tests can tell a
    /// comparison from a fallback.
    #[cfg(test)]
    hashed: std::sync::atomic::AtomicUsize,
}

impl VerifiedBytesCacheV1 {
    pub(crate) fn new_v1(budget: usize, entry_limit: usize) -> Self {
        Self {
            budget,
            entry_limit,
            held: Mutex::new(HeldV1::default()),
            resident: AtomicBool::new(false),
            stats: VerifiedBytesStatsV1::default(),
            #[cfg(test)]
            hashed: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Whether `bytes`, just read from `path`, have the SHA-256 `sha256`:
    /// equal to held bytes for that pin, or else `digest(bytes) == sha256`,
    /// recording them on a match. The comparison runs outside the lock.
    pub(crate) fn digest_matches_v1(
        &self,
        path: &Path,
        sha256: &str,
        bytes: &[u8],
        digest: impl FnOnce(&[u8]) -> String,
    ) -> bool {
        let started = Instant::now();
        let held = self.held_v1(path, sha256);
        let equal = held.as_ref().is_some_and(|held| held.as_slice() == bytes);
        let compared = nanos_since_v1(started);
        self.stats.compare_nanos.add(compared);
        add_thread_work_v1(WorkV1::Verify, compared);
        if equal {
            self.stats.compared_hits.add(1);
            self.stats.compared_hit_bytes.add_usize(bytes.len());
            return true;
        }
        match (self.budget, held) {
            (0, _) => self.stats.misses_disabled.add(1),
            (_, None) => self.stats.misses_absent.add(1),
            (_, Some(_)) => self.stats.misses_differ.add(1),
        }
        #[cfg(test)]
        self.hashed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let started = Instant::now();
        let matches = digest(bytes) == sha256;
        let hashed = nanos_since_v1(started);
        self.stats.hashed.add(1);
        self.stats.hashed_bytes.add_usize(bytes.len());
        self.stats.hash_nanos.add(hashed);
        add_thread_work_v1(WorkV1::Verify, hashed);
        if matches {
            self.record_verified_v1(path, sha256, bytes);
        } else {
            self.stats.hash_mismatches.add(1);
        }
        matches
    }

    /// The bytes held for `path` under `sha256`, whose SHA-256 is `sha256`.
    /// Turns resident reads on for this cache (module docs). Irreversible.
    pub(crate) fn enable_resident_v1(&self) {
        self.resident.store(true, Ordering::SeqCst);
    }

    /// Whether resident reads are on.
    pub(crate) fn resident_v1(&self) -> bool {
        self.resident.load(Ordering::SeqCst)
    }

    /// With resident reads on, the bytes held for `path` and `sha256` when
    /// they are exactly `file_length` long: a reader that has checked the
    /// file's metadata returns them instead of reading the file. `None`
    /// (mode off, nothing held, a different length) means read as before.
    pub(crate) fn resident_bytes_v1(
        &self,
        path: &Path,
        sha256: &str,
        file_length: u64,
    ) -> Option<Arc<Vec<u8>>> {
        if !self.resident_v1() {
            return None;
        }
        let served = self
            .held_v1(path, sha256)
            .filter(|held| held.len() as u64 == file_length);
        if let Some(held) = &served {
            self.stats.resident_hits.add(1);
            self.stats.resident_hit_bytes.add_usize(held.len());
        }
        served
    }

    pub(crate) fn held_v1(&self, path: &Path, sha256: &str) -> Option<Arc<Vec<u8>>> {
        self.stats.lookups.add(1);
        if self.budget == 0 {
            return None;
        }
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let index = held
            .entries
            .iter()
            .position(|entry| entry.sha256 == sha256 && entry.path == path)?;
        let entry = held.entries.remove(index)?;
        let bytes = Arc::clone(&entry.bytes);
        held.entries.push_back(entry);
        self.stats.lookup_hits.add(1);
        Some(bytes)
    }

    /// Holds `bytes` for `path`; the caller has established that their
    /// SHA-256 is `sha256`. The least recently used entries are dropped until
    /// the new entry fits both bounds, then `bytes` are copied in; an entry
    /// whose charge exceeds the whole budget is not held.
    pub(crate) fn record_verified_v1(&self, path: &Path, sha256: &str, bytes: &[u8]) {
        self.record_with_v1(path, sha256, bytes.len(), || {
            self.stats.copied_bytes.add_usize(bytes.len());
            Arc::new(bytes.to_vec())
        });
    }

    /// [`Self::record_verified_v1`] keeping the caller's allocation instead
    /// of a copy (the bytes are immutable once shared): a publisher that
    /// still hands its bytes to other readers records them without copying.
    pub(crate) fn record_verified_shared_v1(&self, path: &Path, sha256: &str, bytes: Arc<Vec<u8>>) {
        self.record_with_v1(path, sha256, bytes.len(), || bytes);
    }

    /// Both recorders: the bounds, then the evictions, then `bytes()` (a copy
    /// or the caller's allocation), so nothing is copied for an entry that is
    /// not held and nothing beyond the budget is held while copying.
    fn record_with_v1(
        &self,
        path: &Path,
        sha256: &str,
        length: usize,
        bytes: impl FnOnce() -> Arc<Vec<u8>>,
    ) {
        let charge = charge_v1(path, sha256, length);
        if self.entry_limit == 0 || charge > self.budget {
            self.stats.records_refused.add(1);
            return;
        }
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = held
            .entries
            .iter()
            .position(|entry| entry.sha256 == sha256 && entry.path == path)
        {
            if let Some(entry) = held.entries.remove(index) {
                held.charged -= entry.charge;
                self.stats.rerecords.add(1);
            }
        }
        while held.charged + charge > self.budget || held.entries.len() >= self.entry_limit {
            let Some(oldest) = held.entries.pop_front() else {
                break;
            };
            held.charged -= oldest.charge;
            self.stats.evictions.add(1);
            self.stats.evicted_bytes.add_usize(oldest.bytes.len());
        }
        held.charged += charge;
        held.entries.push_back(EntryV1 {
            path: path.to_path_buf(),
            sha256: sha256.to_owned(),
            bytes: bytes(),
            charge,
        });
        self.stats.records.add(1);
        self.stats.recorded_bytes.add_usize(length);
        self.stats.peak_charged.raise(held.charged as u64);
    }

    /// Every held entry's path and bytes, shared, not copied
    /// (`opponent_cache_diagnostics_v1`).
    pub(crate) fn held_files_v1(&self) -> Vec<HeldFileV1> {
        let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.entries
            .iter()
            .map(|entry| (entry.path.clone(), Arc::clone(&entry.bytes)))
            .collect()
    }

    /// The `verified_bytes` diagnostics section: the bounds, what is held
    /// now, and the counters.
    pub(crate) fn diagnostics_v1(&self) -> Value {
        let (entries, charged, held_bytes) = {
            let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            let bytes: usize = held.entries.iter().map(|entry| entry.bytes.len()).sum();
            (held.entries.len(), held.charged, bytes)
        };
        let stats = &self.stats;
        json!({
            "budget_bytes": self.budget,
            "resident": self.resident_v1(),
            "entry_limit": self.entry_limit,
            "entries": entries,
            "charged_bytes": charged,
            "held_bytes": held_bytes,
            "peak_charged_bytes": stats.peak_charged.get(),
            "lookups": stats.lookups.get(),
            "lookup_hits": stats.lookup_hits.get(),
            "compared_hits": stats.compared_hits.get(),
            "compared_hit_bytes": stats.compared_hit_bytes.get(),
            "compare_seconds": stats.compare_nanos.seconds(),
            "misses": {
                "disabled": stats.misses_disabled.get(),
                "absent": stats.misses_absent.get(),
                "differ": stats.misses_differ.get(),
            },
            "hashed": stats.hashed.get(),
            "hashed_bytes": stats.hashed_bytes.get(),
            "hash_seconds": stats.hash_nanos.seconds(),
            "hash_mismatches": stats.hash_mismatches.get(),
            "records": stats.records.get(),
            "recorded_bytes": stats.recorded_bytes.get(),
            "copied_bytes": stats.copied_bytes.get(),
            "records_refused": stats.records_refused.get(),
            "rerecords": stats.rerecords.get(),
            "evictions": stats.evictions.get(),
            "evicted_bytes": stats.evicted_bytes.get(),
            "resident_hits": stats.resident_hits.get(),
            "resident_hit_bytes": stats.resident_hit_bytes.get(),
        })
    }

    #[cfg(test)]
    pub(crate) fn hashed_for_test_v1(&self) -> usize {
        self.hashed.load(std::sync::atomic::Ordering::Relaxed)
    }
}
