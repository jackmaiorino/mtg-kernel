//! Parse-ahead for an external collection (`--collect-external`).
//!
//! An external collector publishes the iteration's trajectories in another
//! process, so nothing reaches this process's parse-ahead store and
//! collection validation and the update would each open and parse every
//! file after the collector exits. While the collector runs, a watcher reads
//! each `episode-NNNN.json` that appears under its final name in the
//! collection's output directory (a file is published there whole, by a
//! rename) and offers its bytes to the store (`offer_v1`), whose helpers then
//! make the two parses the readers would make, as for files a local
//! collection publishes. After the collector exits successfully, the files
//! the watcher has not offered yet are offered too.
//!
//! The feed changes only which reads find a parse ready, never what a read
//! returns: each reader still reads and verifies its own file and takes a
//! result only for bytes equal to its own, so a file read early, torn,
//! replaced or never offered is a miss and is parsed as before. Both parses
//! are still made (the fields as a `Value`, the trajectory as a struct); the
//! feed moves them earlier and onto helpers, it does not merge them.
//!
//! Bounds: only the schedule's file names are read (one per slot, at most
//! three attempts each), one file at a time while the collector runs and on
//! at most `workers` threads after it, each file within the per-file bound;
//! the store's retention bounds what is held, and at most `workers` helpers
//! (the chain's collection allocation) are started for it. Any failure of the
//! watcher (an unreadable file, a panic) only leaves files to their readers;
//! the collector's outcome and its errors are reported exactly as before, and
//! the watcher is joined before the collection returns on every path.

use crate::expanded_deck_training_v1::{
    offer_parse_ahead_v1, parse_ahead_retains_v1, start_parse_ahead_helpers_v1,
};
use std::collections::BTreeMap;
use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// How often the watcher looks for new files.
const POLL_V1: Duration = Duration::from_millis(20);
/// Reads of one file before the watcher leaves it to its readers.
const ATTEMPTS_V1: u8 = 3;
/// The per-file bound (`expanded_deck_training_v1::MAX_FILE_BYTES`).
const MAX_FILE_BYTES_V1: u64 = 512 * 1024 * 1024;

/// The schedule slot of a final trajectory name (`episode-{slot:04}.json`
/// with `slot < slots`), None for any other name.
fn slot_of_v1(name: &str, slots: usize) -> Option<usize> {
    let digits = name.strip_prefix("episode-")?.strip_suffix(".json")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let slot: usize = digits.parse().ok()?;
    (slot < slots && format!("episode-{slot:04}.json") == name).then_some(slot)
}

/// What the watcher knows of each slot: offered, or the reads that failed.
#[derive(Default)]
struct FeedV1 {
    slots: BTreeMap<usize, u8>,
    offered: usize,
}

impl FeedV1 {
    fn pending_v1(&self, slot: usize) -> bool {
        self.slots
            .get(&slot)
            .is_none_or(|&tries| tries < ATTEMPTS_V1)
    }

    fn record_v1(&mut self, slot: usize, offered: bool) {
        let tries = self.slots.entry(slot).or_insert(0);
        if offered {
            *tries = ATTEMPTS_V1;
            self.offered += 1;
        } else {
            *tries = tries.saturating_add(1);
        }
    }
}

/// The final trajectory files in `directory` not yet offered or given up.
fn pending_files_v1(directory: &Path, slots: usize, feed: &FeedV1) -> Vec<(usize, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<(usize, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let slot = slot_of_v1(entry.file_name().to_str()?, slots)?;
            feed.pending_v1(slot).then(|| (slot, entry.path()))
        })
        .collect();
    files.sort();
    files
}

/// Reads a regular file within the per-file bound, and offers its bytes
/// under their SHA-256. False when the file could not be read whole.
fn offer_file_v1(path: &Path) -> bool {
    let read = || -> std::io::Result<Option<Vec<u8>>> {
        let file = std::fs::File::open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES_V1 {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        file.take(meta.len() + 1).read_to_end(&mut bytes)?;
        Ok((bytes.len() as u64 == meta.len()).then_some(bytes))
    };
    let Ok(Some(mut bytes)) = read() else {
        return false;
    };
    // The store charges an entry its buffer's capacity.
    bytes.shrink_to_fit();
    offer_parse_ahead_v1(&super::digest(&bytes), path, bytes);
    true
}

/// Offers `files` on up to `workers` threads, recording each outcome.
fn offer_all_v1(files: Vec<(usize, PathBuf)>, workers: usize, feed: &Mutex<FeedV1>) {
    let next = Mutex::new(files.into_iter());
    let work = || loop {
        let Some((slot, path)) = next.lock().ok().and_then(|mut files| files.next()) else {
            return;
        };
        let offered = offer_file_v1(&path);
        if let Ok(mut feed) = feed.lock() {
            feed.record_v1(slot, offered);
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..workers.max(1) {
            if std::thread::Builder::new()
                .name("external-feed".into())
                .spawn_scoped(scope, work)
                .is_err()
            {
                break;
            }
        }
        work();
    });
}

/// Runs `collect` (the external collector) while feeding the trajectories it
/// publishes in `directory` to the parse-ahead store, and returns its result
/// unchanged. `slots` is the schedule's episode count, `workers` the chain's
/// collection allocation; `succeeded` says whether the collector published
/// its collection, after which the files not offered yet are offered.
pub(super) fn collect_fed_v1<T>(
    directory: &Path,
    slots: usize,
    workers: usize,
    collect: impl FnOnce() -> T,
    succeeded: impl FnOnce(&T) -> bool,
) -> T {
    if !parse_ahead_retains_v1() || slots == 0 {
        return collect();
    }
    let workers = workers.max(1);
    let stop = AtomicBool::new(false);
    let feed = Mutex::new(FeedV1::default());
    let result = std::thread::scope(|scope| {
        let watcher = std::thread::Builder::new()
            .name("external-feed".into())
            .spawn_scoped(scope, || {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    while !stop.load(Ordering::Relaxed) {
                        let files = match feed.lock() {
                            Ok(feed) => pending_files_v1(directory, slots, &feed),
                            Err(_) => return,
                        };
                        for (slot, path) in files {
                            if stop.load(Ordering::Relaxed) {
                                return;
                            }
                            let offered = offer_file_v1(&path);
                            if let Ok(mut feed) = feed.lock() {
                                feed.record_v1(slot, offered);
                            }
                            start_parse_ahead_helpers_v1(workers);
                        }
                        std::thread::sleep(POLL_V1);
                    }
                }));
            });
        // Stops the watcher on every exit from the collector, unwinding too.
        struct StopV1<'a>(&'a AtomicBool);
        impl Drop for StopV1<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let result = {
            let _stop = StopV1(&stop);
            collect()
        };
        if let Ok(watcher) = watcher {
            let _ = watcher.join();
        }
        result
    });
    if succeeded(&result) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let files = match feed.lock() {
                Ok(feed) => pending_files_v1(directory, slots, &feed),
                Err(_) => return,
            };
            offer_all_v1(files, workers, &feed);
            start_parse_ahead_helpers_v1(workers);
        }));
    }
    if let Ok(feed) = feed.lock() {
        eprintln!(
            "external collection feed: {} of {} trajectories offered for parse-ahead",
            feed.offered, slots
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_schedules_final_names_are_read() {
        assert_eq!(slot_of_v1("episode-0000.json", 2), Some(0));
        assert_eq!(slot_of_v1("episode-0001.json", 2), Some(1));
        assert_eq!(slot_of_v1("episode-12345.json", 20_000), Some(12_345));
        for name in [
            "episode-0002.json",
            "episode-1.json",
            "episode-00001.json",
            ".episode-0000.json.stage",
            "episode-0000.json.tmp",
            "episode-+001.json",
            "episode-.json",
            "collection.json",
            "ex1-0000.json",
        ] {
            assert_eq!(slot_of_v1(name, 2), None, "{name}");
        }
    }

    #[test]
    fn a_file_is_read_at_most_three_times() {
        let mut feed = FeedV1::default();
        for _ in 0..ATTEMPTS_V1 {
            assert!(feed.pending_v1(4));
            feed.record_v1(4, false);
        }
        assert!(!feed.pending_v1(4));
        feed.record_v1(5, true);
        assert!(!feed.pending_v1(5));
        assert_eq!(feed.offered, 1);
    }
}
