//! Process-wide cache of verified converted bases.
//!
//! Every converted continuation load reads its conversion record, its base
//! import, the base's fresh-initialization inputs and the 20 MB base
//! checkpoint, rebuilds the base policy, installs the contract (hashing the
//! base and the converted parameters) and probes the base checkpoint for an
//! archive-distillation parent. Every converted loaded-source hit and every
//! collection boundary check hashes the base files again. All of that is a
//! pure function of those files' bytes.
//!
//! This cache keeps, per converted import, the bytes of those files as a full
//! load verified them against their pins, and the base policy with the
//! contract installed and the conversion bound, as a full load left it just
//! before restoring the continuation. A hit rereads each file and compares
//! its bytes with the verified bytes, which proves its pin without hashing
//! (the comparison durable publication makes of a published file). Any
//! difference, unreadable file or other doubt is a miss, and the caller runs
//! the full load or the full reads, which report exactly their own errors in
//! their own order. Only fresh-initialization bases without an archive
//! parent are kept, and an entry matches only the per-forward check setting
//! its policy was installed under.

use super::{
    names_another_file_v1, read_pinned_bytes, ExpandedModelSourceV1, FrozenPlayPolicyV1,
    NoStateDigestConversionV1, NoStateDigestImportV1,
};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

/// A run needs one converted import; a second serves an alternating caller.
/// Each entry holds its base files' bytes (about 30 MB for a fresh base).
const BASE_CACHE_ENTRIES_V1: usize = 2;

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
    /// The base policy, contract installed and conversion bound.
    template: FrozenPlayPolicyV1,
}

impl VerifiedBaseV1 {
    /// Whether every base file still holds exactly its verified bytes.
    pub(super) fn files_unchanged_v1(&self) -> bool {
        self.files
            .iter()
            .all(|(path, bytes)| std::fs::read(path).is_ok_and(|read| read == *bytes))
    }

    /// The base import's bytes.
    pub(super) fn base_bytes_v1(&self) -> &[u8] {
        &self.files[1].1
    }

    /// A private duplicate of the installed base policy.
    pub(super) fn template_v1(&self) -> Result<FrozenPlayPolicyV1, String> {
        self.template.duplicate_loaded_v1()
    }
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
}

/// Test-only: base files this thread found unchanged for an entry.
#[cfg(test)]
pub(super) fn hits_for_test_v1() -> u64 {
    HITS_FOR_TEST_V1.with(std::cell::Cell::get)
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
    #[cfg(test)]
    HITS_FOR_TEST_V1.with(|hits| hits.set(hits.get() + 1));
    Some(entry)
}

/// Keep a full converted load's installed base policy (`template`, taken just
/// before the continuation was restored) with its base files' bytes, read
/// again and verified against their pins. A base that is not a
/// fresh-initialization source, whose checkpoint names an archive parent, or
/// any file that no longer verifies is not kept. Never inlined: its policy
/// stays out of the loader's frame.
#[inline(never)]
pub(super) fn remember_v1(
    source: &ExpandedModelSourceV1,
    import_bytes: &[u8],
    import: &NoStateDigestImportV1,
    conversion: &NoStateDigestConversionV1,
    template: FrozenPlayPolicyV1,
) {
    if bypassed_v1() {
        return;
    }
    let read = || -> Result<Option<VerifiedFilesV1>, String> {
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
        if let Some(pin) = &conversion.base_checkpoint {
            let bytes = read_pinned_bytes(pin)?;
            if names_another_file_v1(&bytes)? {
                return Ok(None);
            }
            files.push((pin.path.clone(), bytes));
        }
        Ok(Some(files))
    };
    let Ok(Some(files)) = read() else {
        return;
    };
    let entry = Arc::new(VerifiedBaseV1 {
        source: source.clone(),
        import_bytes: import_bytes.to_vec(),
        check: template.no_state_digest_check_v1(),
        files,
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
