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

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
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

pub(crate) struct VerifiedBytesCacheV1 {
    /// Bound on the held entries' charges; 0 turns the cache off.
    budget: usize,
    /// Bound on the number of held entries.
    entry_limit: usize,
    held: Mutex<HeldV1>,
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
        let charge = charge_v1(path, sha256, bytes.len());
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
            bytes: Arc::new(bytes.to_vec()),
            charge,
        });
    }

    #[cfg(test)]
    pub(crate) fn hashed_for_test_v1(&self) -> usize {
        self.hashed.load(std::sync::atomic::Ordering::Relaxed)
    }
}
