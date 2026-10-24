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
//! `MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB` sets the held bytes (default 160;
//! 0 turns the memo off). Least recently used entries are dropped before a
//! new one is copied in, so held entries never exceed the budget, and bytes
//! larger than the whole budget are not held. A dropped entry's bytes live on
//! only while a reader that took it before the drop finishes its comparison
//! (at most one file per concurrent reader).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

#[cfg(test)]
mod tests;

const DEFAULT_BUDGET_MIB_V1: usize = 160;

fn budget_bytes_v1() -> usize {
    static BUDGET: OnceLock<usize> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_BUDGET_MIB_V1)
            .saturating_mul(1 << 20)
    })
}

struct EntryV1 {
    path: PathBuf,
    sha256: String,
    bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
struct CacheV1 {
    /// Least recently used first; a hit or a re-record moves to the back.
    entries: VecDeque<EntryV1>,
    held_bytes: usize,
}

fn cache_v1() -> &'static Mutex<CacheV1> {
    static CACHE: OnceLock<Mutex<CacheV1>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(CacheV1::default()))
}

/// Whether `bytes`, just read from `path`, have the SHA-256 `sha256`: equal
/// to held bytes for that pin, or else `digest(bytes) == sha256`, recording
/// them on a match. The comparison runs outside the lock.
pub(crate) fn digest_matches_v1(
    path: &Path,
    sha256: &str,
    bytes: &[u8],
    digest: impl FnOnce(&[u8]) -> String,
) -> bool {
    if held_v1(path, sha256).is_some_and(|held| held.as_slice() == bytes) {
        return true;
    }
    let matches = digest(bytes) == sha256;
    if matches {
        record_verified_v1(path, sha256, bytes);
    }
    matches
}

/// The bytes held for `path` under `sha256`, whose SHA-256 is `sha256`.
pub(crate) fn held_v1(path: &Path, sha256: &str) -> Option<Arc<Vec<u8>>> {
    held_in_v1(cache_v1(), budget_bytes_v1(), path, sha256)
}

/// Holds `bytes` for `path`; the caller has established that their SHA-256
/// is `sha256`.
pub(crate) fn record_verified_v1(path: &Path, sha256: &str, bytes: &[u8]) {
    record_in_v1(cache_v1(), budget_bytes_v1(), path, sha256, bytes);
}

fn held_in_v1(
    cache: &Mutex<CacheV1>,
    budget: usize,
    path: &Path,
    sha256: &str,
) -> Option<Arc<Vec<u8>>> {
    if budget == 0 {
        return None;
    }
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    let index = cache
        .entries
        .iter()
        .position(|entry| entry.sha256 == sha256 && entry.path == path)?;
    let entry = cache.entries.remove(index)?;
    let bytes = Arc::clone(&entry.bytes);
    cache.entries.push_back(entry);
    Some(bytes)
}

/// The least recently used entries are dropped until `bytes` fit, then
/// `bytes` are copied in; bytes larger than the whole budget are not held.
fn record_in_v1(cache: &Mutex<CacheV1>, budget: usize, path: &Path, sha256: &str, bytes: &[u8]) {
    if budget == 0 || bytes.len() > budget {
        return;
    }
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(index) = cache
        .entries
        .iter()
        .position(|entry| entry.sha256 == sha256 && entry.path == path)
    {
        if let Some(entry) = cache.entries.remove(index) {
            cache.held_bytes -= entry.bytes.len();
        }
    }
    while cache.held_bytes + bytes.len() > budget {
        let Some(oldest) = cache.entries.pop_front() else {
            break;
        };
        cache.held_bytes -= oldest.bytes.len();
    }
    cache.held_bytes += bytes.len();
    cache.entries.push_back(EntryV1 {
        path: path.to_path_buf(),
        sha256: sha256.to_owned(),
        bytes: Arc::new(bytes.to_vec()),
    });
}
