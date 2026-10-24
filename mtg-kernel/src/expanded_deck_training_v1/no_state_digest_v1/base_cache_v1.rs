//! Process-wide cache of verified converted bases.
//!
//! Every converted load reads its conversion record, its base import, the
//! base's fresh-initialization inputs and the 20 MB base checkpoint, rebuilds
//! the base policy, installs the contract (hashing the base and the converted
//! parameters) and probes the base checkpoint for an archive-distillation
//! parent; a run start also reads its 1,403-wide artifact. Every converted
//! loaded-source hit and every collection boundary check hashes those files
//! again and probes again. All of that is a pure function of the files'
//! bytes.
//!
//! This cache keeps, per converted import, the bytes of those files as a full
//! load verified them against their pins, the base checkpoint's archive
//! status as the full probe parsed it, the artifact's bytes when a run start
//! read it, and the base policy with the contract installed and the
//! conversion bound, as a full load left it just before restoring a
//! continuation (or, at a run start, after installing). A hit rereads each
//! file and compares its bytes with the verified bytes, which proves its pin
//! without hashing (the comparison durable publication makes of a published
//! file), and then serves the archive status the probe derived from those
//! same bytes. Any difference, unreadable file or other doubt is a miss, and
//! the caller runs the full load, the full reads or the full probe, which
//! report exactly their own errors in their own order. Only
//! fresh-initialization bases are kept. A base whose checkpoint names an
//! archive parent is kept for its status alone: its loads stay full, since
//! they read that parent. An entry matches only the per-forward check
//! setting its policy was installed under.

use super::{
    names_another_file_v1, read_pinned_bytes, ExpandedModelSourceV1, FrozenPlayPolicyV1,
    NoStateDigestConversionV1, NoStateDigestImportV1, PinnedFileV1,
};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

/// Enough for a run's learner and its fixed converted opponents, each its
/// own conversion, not to evict one another. Six resident entries is the
/// enforced bound. An entry normally holds its base files' bytes (about
/// 30 MB for a fresh base), a run start's artifact (about 20 MB) and one
/// installed policy (about 6 MB, twice that with the per-forward check
/// model), so the expected payload is about 6 x 60 MB = 360 MB per process
/// (2.9 GB for eight concurrent runs). It is an estimate, not a limit:
/// larger valid checkpoints make larger entries, and an evicted entry stays
/// alive while a caller still holds its `Arc`.
const BASE_CACHE_ENTRIES_V1: usize = 6;

/// Files in read order, each with its bytes as verified against its pin.
type VerifiedFilesV1 = Vec<(PathBuf, Vec<u8>)>;

pub(super) struct VerifiedBaseV1 {
    source: ExpandedModelSourceV1,
    import_bytes: Vec<u8>,
    check: bool,
    /// The base load's files in its read order (conversion record, base
    /// import, initialization manifest, parameter payload, base checkpoint),
    /// with the bytes verified against their pins.
    files: VerifiedFilesV1,
    /// Whether the base checkpoint names an archive-distillation parent, as
    /// the full probe parsed it from the verified bytes.
    archive: bool,
    /// The 1,403-wide artifact's verified bytes, when a run start read it.
    artifact: Option<(PathBuf, Vec<u8>)>,
    /// The base policy, contract installed and conversion bound.
    template: FrozenPlayPolicyV1,
}

impl VerifiedBaseV1 {
    /// Whether every base file still holds exactly its verified bytes.
    fn files_unchanged_v1(&self) -> bool {
        self.files
            .iter()
            .all(|(path, bytes)| holds_bytes_v1(path, bytes))
    }

    /// Whether the base checkpoint names an archive parent.
    pub(super) fn archive_v1(&self) -> bool {
        self.archive
    }

    /// Whether `pin` is the artifact a run start verified and it still holds
    /// exactly those bytes.
    pub(super) fn artifact_unchanged_v1(&self, pin: &PinnedFileV1) -> bool {
        self.artifact
            .as_ref()
            .is_some_and(|(path, bytes)| *path == pin.path && holds_bytes_v1(path, bytes))
    }

    /// The base import's bytes.
    pub(super) fn base_bytes_v1(&self) -> &[u8] {
        &self.files[1].1
    }

    /// A private duplicate of the installed base policy, for a base without
    /// an archive parent; `None` when its loads must stay full.
    pub(super) fn template_v1(&self) -> Option<Result<FrozenPlayPolicyV1, String>> {
        (!self.archive).then(|| self.template.duplicate_loaded_v1())
    }
}

/// Whether `path` is a regular file of exactly `expected`'s length holding
/// exactly those bytes (also the process opponent cache's comparison). Metadata comes first and the read is capped at one
/// byte past that length, so an oversized or non-regular replacement (a
/// directory, a pipe) is a miss without being read; the original reader
/// then reports it.
pub(in crate::expanded_deck_training_v1) fn holds_bytes_v1(
    path: &std::path::Path,
    expected: &[u8],
) -> bool {
    use std::io::Read;
    let regular = |metadata: std::io::Result<std::fs::Metadata>| {
        metadata.is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected.len() as u64)
    };
    // Checked before opening, which would block on a pipe, and again on the
    // opened file.
    if !regular(std::fs::metadata(path)) {
        return false;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    if !regular(file.metadata()) {
        return false;
    }
    let mut read = Vec::with_capacity(expected.len() + 1);
    if file
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut read)
        .is_err()
    {
        return false;
    }
    #[cfg(test)]
    BYTES_READ_FOR_TEST_V1.with(|total| total.set(total.get() + read.len() as u64));
    read == expected
}

type EntriesV1 = VecDeque<Arc<VerifiedBaseV1>>;

#[cfg(not(test))]
fn with_entries_v1<R>(f: impl FnOnce(&mut EntriesV1) -> R) -> Option<R> {
    use std::sync::{Mutex, OnceLock};
    static ENTRIES: OnceLock<Mutex<EntriesV1>> = OnceLock::new();
    let mut entries = ENTRIES.get_or_init(Default::default).lock().ok()?;
    Some(f(&mut entries))
}

/// Unit tests run in parallel threads, so each test thread keeps its own
/// entries, as the loaded-source cache does.
#[cfg(test)]
fn with_entries_v1<R>(f: impl FnOnce(&mut EntriesV1) -> R) -> Option<R> {
    thread_local! {
        static ENTRIES: std::cell::RefCell<EntriesV1> = Default::default();
    }
    ENTRIES.with(|entries| Some(f(&mut entries.borrow_mut())))
}

#[cfg(test)]
thread_local! {
    static HITS_FOR_TEST_V1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static BYTES_READ_FOR_TEST_V1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Test-only: bytes this thread's comparisons read.
#[cfg(test)]
pub(super) fn bytes_read_for_test_v1() -> u64 {
    BYTES_READ_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: base files this thread found unchanged for an entry.
#[cfg(test)]
pub(super) fn hits_for_test_v1() -> u64 {
    HITS_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: empties this thread's entries.
#[cfg(test)]
pub(super) fn clear_for_test_v1() {
    with_entries_v1(VecDeque::clear);
}

fn bypassed_v1() -> bool {
    #[cfg(test)]
    if super::super::loaded_source_cache_v1::bypassed_now_for_test_v1() {
        return true;
    }
    false
}

/// The entry of a converted source's import, under the current per-forward
/// check setting, whose base files all still hold their verified bytes.
/// `None` sends the caller to its full reads.
pub(super) fn lookup_v1(
    source: &ExpandedModelSourceV1,
    import_bytes: &[u8],
) -> Option<Arc<VerifiedBaseV1>> {
    let entry = find_v1(source, import_bytes)?;
    #[cfg(test)]
    HITS_FOR_TEST_V1.with(|hits| hits.set(hits.get() + 1));
    Some(entry)
}

/// [`lookup_v1`] without its test count.
fn find_v1(source: &ExpandedModelSourceV1, import_bytes: &[u8]) -> Option<Arc<VerifiedBaseV1>> {
    if bypassed_v1() {
        return None;
    }
    let check = super::check_enabled_v1();
    let entry = with_entries_v1(|entries| {
        let index = entries.iter().position(|entry| {
            entry.source.play_import == source.play_import
                && entry.source.feature_transfer == source.feature_transfer
                && entry.import_bytes == import_bytes
                && entry.check == check
        })?;
        let entry = entries.remove(index)?;
        entries.push_back(Arc::clone(&entry));
        Some(entry)
    })
    .flatten()?;
    if !entry.files_unchanged_v1() {
        return None;
    }
    Some(entry)
}

/// Keep a full converted load's installed base policy (`template`: before a
/// continuation was restored, or after a run start installed) with its base
/// files' bytes, read again and verified against their pins, the base
/// checkpoint's archive status probed from those bytes and, at a run start,
/// the artifact's verified bytes. A base that is not a fresh-initialization
/// source, or any file that no longer verifies, is not kept. Never inlined:
/// its policy stays out of the loader's frame.
#[inline(never)]
pub(super) fn remember_v1(
    source: &ExpandedModelSourceV1,
    import_bytes: &[u8],
    import: &NoStateDigestImportV1,
    conversion: &NoStateDigestConversionV1,
    run_start: bool,
    template: FrozenPlayPolicyV1,
) {
    // A base whose loads stay full (an archive parent) would otherwise be
    // read again at every load; an unchanged entry is kept as it is.
    if bypassed_v1() || find_v1(source, import_bytes).is_some() {
        return;
    }
    type ReadV1 = (VerifiedFilesV1, bool, Option<(PathBuf, Vec<u8>)>);
    let read = || -> Result<Option<ReadV1>, String> {
        let base_bytes = read_pinned_bytes(&import.base_play_import)?;
        let Ok(descriptor) =
            super::super::fresh_initialization_source::parse_source_v1(&base_bytes)
        else {
            return Ok(None);
        };
        let mut files = vec![
            (
                import.conversion.path.clone(),
                read_pinned_bytes(&import.conversion)?,
            ),
            (import.base_play_import.path.clone(), base_bytes),
            (
                descriptor.initialization.path.clone(),
                read_pinned_bytes(&descriptor.initialization)?,
            ),
            (
                descriptor.parameters.path.clone(),
                read_pinned_bytes(&descriptor.parameters)?,
            ),
        ];
        let mut archive = false;
        if let Some(pin) = &conversion.base_checkpoint {
            let bytes = read_pinned_bytes(pin)?;
            archive = names_another_file_v1(&bytes)?;
            files.push((pin.path.clone(), bytes));
        }
        let artifact = match (&conversion.converted_checkpoint, run_start) {
            (Some(pin), true) => Some((pin.path.clone(), read_pinned_bytes(pin)?)),
            _ => None,
        };
        Ok(Some((files, archive, artifact)))
    };
    let Ok(Some((files, archive, artifact))) = read() else {
        return;
    };
    let entry = Arc::new(VerifiedBaseV1 {
        source: source.clone(),
        import_bytes: import_bytes.to_vec(),
        check: template.no_state_digest_check_v1(),
        files,
        archive,
        artifact,
        template,
    });
    with_entries_v1(|entries| {
        entries.retain(|cached| {
            !(cached.source.play_import == entry.source.play_import
                && cached.source.feature_transfer == entry.source.feature_transfer
                && cached.import_bytes == entry.import_bytes
                && cached.check == entry.check)
        });
        while entries.len() >= BASE_CACHE_ENTRIES_V1 {
            entries.pop_front();
        }
        entries.push_back(entry);
    });
}
