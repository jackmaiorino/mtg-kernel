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
//! (default 160; 0 turns the memo off). An entry is charged its bytes (for
//! bytes shared with their publisher, the whole allocation it keeps alive,
//! which can exceed the bytes), its
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

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

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

/// What an entry for `path`, `sha256` and `bytes_len` bytes of memory kept
/// alive for its bytes is charged.
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

pub(crate) struct VerifiedBytesCacheV1 {
    /// Bound on the held entries' charges; 0 turns the cache off.
    budget: usize,
    /// Bound on the number of held entries.
    entry_limit: usize,
    held: Mutex<HeldV1>,
    /// Resident reads (module docs): set once by a run, never cleared.
    resident: AtomicBool,
    /// Runs admitted to this cache (`enter_run_v1`); the lock also orders
    /// every change of `resident`.
    runs: Mutex<RunCountsV1>,
    /// Digests computed by `digest_matches_v1`, so tests can tell a
    /// comparison from a fallback.
    #[cfg(test)]
    hashed: std::sync::atomic::AtomicUsize,
}

/// Runs admitted to a cache (`enter_run_v1`).
#[derive(Default)]
struct RunCountsV1 {
    /// In flight without resident reads.
    reread: usize,
    /// Admitted with resident reads and not accepted yet (`accept_v1`).
    resident_pending: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RunReadsKindV1 {
    Reread,
    ResidentPending,
    Resident,
}

/// A run's admission to a cache (`enter_run_v1`), held until the run
/// returns.
pub(crate) struct RunReadsV1<'a> {
    cache: &'a VerifiedBytesCacheV1,
    kind: RunReadsKindV1,
}

impl RunReadsV1<'_> {
    /// The run's arguments are accepted: a run admitted with resident reads
    /// turns them on, for this cache and for good. An admission does not turn
    /// them on by itself, so until this call the run reads in the cache's
    /// current mode: rereads for the first resident run of a process, unless
    /// another resident run is accepted meanwhile (which this run's config
    /// allows as well).
    pub(crate) fn accept_v1(&mut self) {
        if self.kind == RunReadsKindV1::ResidentPending {
            let mut runs = self
                .cache
                .runs
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.cache.resident.store(true, Ordering::SeqCst);
            runs.resident_pending -= 1;
            self.kind = RunReadsKindV1::Resident;
        }
    }
}

impl Drop for RunReadsV1<'_> {
    fn drop(&mut self) {
        let mut runs = self
            .cache
            .runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match self.kind {
            RunReadsKindV1::Reread => runs.reread -= 1,
            // Rejected before it was accepted: the mode stays as it was.
            RunReadsKindV1::ResidentPending => runs.resident_pending -= 1,
            RunReadsKindV1::Resident => {}
        }
    }
}

impl VerifiedBytesCacheV1 {
    pub(crate) fn new_v1(budget: usize, entry_limit: usize) -> Self {
        Self {
            budget,
            entry_limit,
            held: Mutex::new(HeldV1::default()),
            resident: AtomicBool::new(false),
            runs: Mutex::new(RunCountsV1::default()),
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
        if self
            .held_v1(path, sha256)
            .is_some_and(|held| held.as_slice() == bytes)
        {
            return true;
        }
        #[cfg(test)]
        self.hashed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let matches = digest(bytes) == sha256;
        if matches {
            self.record_verified_v1(path, sha256, bytes);
        }
        matches
    }

    /// The bytes held for `path` under `sha256`, whose SHA-256 is `sha256`.
    /// Turns resident reads on for a test's private cache without an
    /// admission. Everything else goes through `enter_run_v1`.
    #[cfg(test)]
    pub(crate) fn enable_resident_for_test_v1(&self) {
        self.resident.store(true, Ordering::SeqCst);
    }

    /// Whether resident reads are on.
    pub(crate) fn resident_v1(&self) -> bool {
        self.resident.load(Ordering::SeqCst)
    }

    /// Admits a run to this cache for the life of the returned guard, before
    /// the run's first pinned read. Resident reads belong to the cache, which
    /// a process shares, and never turn off, so one cache does not serve both
    /// modes. A run without them is refused once they are on, and while a run
    /// that will turn them on is admitted (it would read resident under a
    /// config that says otherwise). A run with them is refused while a run
    /// without them is in flight (that run's reads would change under it), and
    /// turns them on when its arguments are accepted (`RunReadsV1::accept_v1`),
    /// not before, so a rejected invocation leaves the mode alone. Readers
    /// that are never admitted take the mode the cache is in.
    pub(crate) fn enter_run_v1(&self, resident: bool) -> Result<RunReadsV1<'_>, String> {
        let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
        let kind = if resident {
            if runs.reread > 0 {
                return Err(
                    "resident verified reads cannot turn on while a run without them is in \
                     flight in this process"
                        .to_owned(),
                );
            }
            if self.resident_v1() {
                RunReadsKindV1::Resident
            } else {
                runs.resident_pending += 1;
                RunReadsKindV1::ResidentPending
            }
        } else {
            if self.resident_v1() || runs.resident_pending > 0 {
                return Err(
                    "this process reads, or is about to read, with resident verified reads; \
                     a run without them needs its own process"
                        .to_owned(),
                );
            }
            runs.reread += 1;
            RunReadsKindV1::Reread
        };
        Ok(RunReadsV1 { cache: self, kind })
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
        self.held_v1(path, sha256)
            .filter(|held| held.len() as u64 == file_length)
    }

    pub(crate) fn held_v1(&self, path: &Path, sha256: &str) -> Option<Arc<Vec<u8>>> {
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
        Some(bytes)
    }

    /// Holds `bytes` for `path`; the caller has established that their
    /// SHA-256 is `sha256`. The least recently used entries are dropped until
    /// the new entry fits both bounds, then `bytes` are copied in; an entry
    /// whose charge exceeds the whole budget is not held.
    pub(crate) fn record_verified_v1(&self, path: &Path, sha256: &str, bytes: &[u8]) {
        self.record_with_v1(path, sha256, bytes.len(), bytes.len(), || {
            Arc::new(bytes.to_vec())
        });
    }

    /// [`Self::record_verified_v1`] keeping the caller's allocation instead
    /// of a copy (the bytes are immutable once shared): a publisher that
    /// still hands its bytes to other readers records them without copying.
    /// The entry keeps the whole allocation alive, so it is charged the
    /// allocation's capacity, not only the bytes in use: a buffer with spare
    /// capacity (a compressor's worst-case allocation, a grown `Vec`) counts
    /// in full against the budget.
    pub(crate) fn record_verified_shared_v1(&self, path: &Path, sha256: &str, bytes: Arc<Vec<u8>>) {
        self.record_with_v1(path, sha256, bytes.len(), bytes.capacity(), || bytes);
    }

    /// Both recorders: the bounds, then the evictions, then `bytes()` (a copy
    /// or the caller's allocation), so nothing is copied for an entry that is
    /// not held and nothing beyond the budget is held while copying.
    /// `length` is the bytes' length and `allocated` the memory the entry
    /// keeps alive for them (the same for a copy).
    fn record_with_v1(
        &self,
        path: &Path,
        sha256: &str,
        length: usize,
        allocated: usize,
        bytes: impl FnOnce() -> Arc<Vec<u8>>,
    ) {
        let charge = charge_v1(path, sha256, allocated.max(length));
        if self.entry_limit == 0 || charge > self.budget {
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
            }
        }
        while held.charged + charge > self.budget || held.entries.len() >= self.entry_limit {
            let Some(oldest) = held.entries.pop_front() else {
                break;
            };
            held.charged -= oldest.charge;
        }
        held.charged += charge;
        held.entries.push_back(EntryV1 {
            path: path.to_path_buf(),
            sha256: sha256.to_owned(),
            bytes: bytes(),
            charge,
        });
    }

    #[cfg(test)]
    pub(crate) fn hashed_for_test_v1(&self) -> usize {
        self.hashed.load(std::sync::atomic::Ordering::Relaxed)
    }
}
