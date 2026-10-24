//! Process-wide cache of loaded ordinary fresh-initialization sources.
//!
//! A native training run loads each new checkpoint four times: the update's
//! round-trip check, update validation, the next collection and the next
//! update. Each load reads and hashes the same pinned files and then parses
//! the 20 MB checkpoint and rebuilds the same policy and optimizer state.
//! The rebuild is a pure function of the verified bytes, so this cache keeps
//! the first load's result and hands out exact duplicates of it.
//!
//! A hit still performs every file check of a full load, in the same order
//! and with the same errors: the descriptor bytes must equal the cached
//! bytes, and the initialization manifest, the parameter payload and the
//! checkpoint are each read and compared with their SHA-256 pins by the same
//! readers. Only the parses and checks that are pure functions of those bytes
//! are skipped; they passed when the entry was created, so the first failure a
//! hit can report is the one a full load would report. The update's input
//! binding is checked on every call against the inputs the checkpoint records.
//!
//! Only fresh-initialization descriptors are cached, and only when the
//! checkpoint names no further file (a phase-1 archive-distillation
//! checkpoint reads its parent), and converted (`no_state_digest_v1`)
//! sources over such a base: a hit reads the conversion record, the base
//! import and every pin of the base, the artifact or continuation checkpoint
//! in the full converted load's order (`reverify_converted_v1`), and must
//! match the full load's per-forward check setting. Every other source loads
//! in full as before.
//! Duplicates come from `FrozenPlayPolicyV1::duplicate_loaded_v1`, which only
//! accepts a policy in its as-loaded state, and from cloning the train state.

use super::fresh_initialization_source::{self, ExpandedFreshInitializationSourceV1};
use super::{
    ensure, no_state_digest_v1, read_pinned_bytes, CheckpointInputExpectationV1,
    ExpandedCheckpointV1, ExpandedModelSourceV1, FrozenPlayPolicyV1, NativePolicyValueTrainStateV1,
    PinnedFileV1,
};
use std::collections::VecDeque;
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// A native run needs one entry (the newest checkpoint); a second keeps the
/// run's initial source or an alternating caller from evicting it. Converted
/// (`no_state_digest_v1`) and fresh entries share the bound, so either kind
/// can evict the other; a converted load also looks up its base's entry.
const LOADED_SOURCE_CACHE_ENTRIES_V1: usize = 2;

pub(super) struct RecordedInputsV1 {
    pub(super) trajectories: Vec<PinnedFileV1>,
    pub(super) learning_rate_bits: u32,
    pub(super) value_coefficient_bits: u32,
}

/// What a hit must reread, by kind of source.
enum KindV1 {
    /// A fresh-initialization descriptor.
    Fresh(ExpandedFreshInitializationSourceV1),
    /// A converted source over a fresh-initialization base. `run_start`: its
    /// checkpoint is the conversion's base checkpoint. `check`: the
    /// per-forward check setting it loaded under.
    Converted { run_start: bool, check: bool },
}

struct LoadedSourceV1 {
    source: ExpandedModelSourceV1,
    descriptor_bytes: Vec<u8>,
    kind: KindV1,
    policy: FrozenPlayPolicyV1,
    state: NativePolicyValueTrainStateV1,
    /// The checkpoint's recorded update inputs; `None` without a checkpoint.
    inputs: Option<RecordedInputsV1>,
}

type EntriesV1 = VecDeque<Arc<LoadedSourceV1>>;

/// Least recently used first. `None` if the lock is poisoned, which callers
/// treat as a miss.
#[cfg(not(test))]
fn with_entries_v1<R>(f: impl FnOnce(&mut EntriesV1) -> R) -> Option<R> {
    use std::sync::{Mutex, OnceLock};
    static ENTRIES: OnceLock<Mutex<EntriesV1>> = OnceLock::new();
    let mut entries = ENTRIES.get_or_init(Default::default).lock().ok()?;
    Some(f(&mut entries))
}

/// Unit tests run in parallel threads, so each test thread keeps its own
/// entries: one test's loads never evict another's.
#[cfg(test)]
fn with_entries_v1<R>(f: impl FnOnce(&mut EntriesV1) -> R) -> Option<R> {
    thread_local! {
        static ENTRIES: std::cell::RefCell<EntriesV1> = Default::default();
    }
    ENTRIES.with(|entries| Some(f(&mut entries.borrow_mut())))
}

#[cfg(test)]
thread_local! {
    static MATCHES_FOR_TEST_V1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static BYPASS_FOR_TEST_V1: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether an entry holds this exact converted continuation (`source`,
/// with its checkpoint pin, and its import bytes). A continuation whose
/// checkpoint names another file is never kept, so a held one names none.
pub(super) fn holds_converted_continuation_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
) -> bool {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return false;
    }
    with_entries_v1(|entries| {
        entries.iter().any(|entry| {
            entry.source == *source
                && entry.descriptor_bytes == descriptor_bytes
                && matches!(
                    entry.kind,
                    KindV1::Converted {
                        run_start: false,
                        ..
                    }
                )
        })
    })
    .unwrap_or(false)
}

/// Test-only: lookups on this thread that found an entry.
#[cfg(test)]
pub(super) fn matches_for_test_v1() -> u64 {
    MATCHES_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: whether this thread's caches are switched off, which also
/// switches off the converted base cache (`no_state_digest_v1::base_cache_v1`).
#[cfg(test)]
pub(super) fn bypassed_now_for_test_v1() -> bool {
    BYPASS_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: empties this thread's entries.
#[cfg(test)]
pub(super) fn clear_for_test_v1() {
    with_entries_v1(VecDeque::clear);
}

/// Test-only: runs `load` on this thread with the cache switched off.
#[cfg(test)]
pub(super) fn bypassed_for_test_v1<T>(load: impl FnOnce() -> T) -> T {
    BYPASS_FOR_TEST_V1.with(|bypass| bypass.set(true));
    let result = load();
    BYPASS_FOR_TEST_V1.with(|bypass| bypass.set(false));
    result
}

/// The cached result for these descriptor bytes, after the full loader's file
/// checks and input binding, or `None` when the source must load in full.
pub(super) fn lookup_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return Ok(None);
    }
    let entry = with_entries_v1(|entries| {
        let index = entries.iter().position(|entry| {
            entry.source == *source
                && entry.descriptor_bytes == descriptor_bytes
                && match entry.kind {
                    KindV1::Fresh(_) => true,
                    // A converted run start's update inputs bind the base
                    // checkpoint inside the base load; it is served only
                    // without them. Either kind matches only the current
                    // per-forward check setting.
                    KindV1::Converted { run_start, check } => {
                        !(run_start && inputs.is_some())
                            && check == no_state_digest_v1::check_enabled_v1()
                    }
                }
        })?;
        let entry = entries.remove(index)?;
        entries.push_back(Arc::clone(&entry));
        Some(entry)
    })
    .flatten();
    let Some(entry) = entry else {
        return Ok(None);
    };
    #[cfg(test)]
    MATCHES_FOR_TEST_V1.with(|matches| matches.set(matches.get() + 1));
    match &entry.kind {
        // The full loader's reads, in its order: the initialization manifest
        // and the parameter payload, then the checkpoint.
        KindV1::Fresh(descriptor) => {
            fresh_initialization_source::reverify_input_files_v1(descriptor)?;
            if let Some(pin) = &source.checkpoint {
                read_pinned_bytes(pin)?;
            }
        }
        KindV1::Converted { .. } => {
            no_state_digest_v1::reverify_converted_v1(source, descriptor_bytes)?;
        }
    }
    match (&entry.inputs, inputs) {
        (Some(recorded), Some(expected)) => expected.validate_v1(
            &recorded.trajectories,
            recorded.learning_rate_bits,
            recorded.value_coefficient_bits,
        )?,
        (None, expected) => ensure(expected.is_none(), "update requires checkpoint")?,
        (Some(_), None) => {}
    }
    let Ok(policy) = entry.policy.duplicate_loaded_v1() else {
        return Ok(None);
    };
    Ok(Some((policy, entry.state.clone())))
}

/// Keep a full load's result. `saved` is the checkpoint it restored, if any.
/// Sources this cache cannot serve exactly are left out.
pub(super) fn remember_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    saved: Option<&ExpandedCheckpointV1>,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    // A converted (no-state-digest) policy is kept only by
    // `remember_converted_v1`.
    if source.checkpoint.is_some() != saved.is_some()
        || saved.is_some_and(|saved| saved.archive_distillation.is_some())
        || policy.input_contract_v1().is_some()
    {
        return;
    }
    let Ok(descriptor) = fresh_initialization_source::parse_source_v1(descriptor_bytes) else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind: KindV1::Fresh(descriptor),
        policy: template,
        state: state.clone(),
        inputs: saved.map(|saved| RecordedInputsV1 {
            trajectories: saved.trajectories.clone(),
            learning_rate_bits: saved.learning_rate_bits,
            value_coefficient_bits: saved.value_coefficient_bits,
        }),
    }));
}

/// Keep a full converted load's result (`no_state_digest_v1::initialize_training_v1`)
/// when its base is a fresh-initialization source and its checkpoint names
/// no further file. `descriptor_bytes` are the converted import's. Never
/// inlined: its policy template stays out of the loader's frame.
#[inline(never)]
pub(super) fn remember_converted_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    load: &no_state_digest_v1::ConvertedLoadV1,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    let (run_start, inputs) = match &load.kind {
        no_state_digest_v1::ConvertedLoadKindV1::RunStart => (true, None),
        no_state_digest_v1::ConvertedLoadKindV1::Continuation(inputs) => (
            false,
            Some(RecordedInputsV1 {
                trajectories: inputs.trajectories.clone(),
                learning_rate_bits: inputs.learning_rate_bits,
                value_coefficient_bits: inputs.value_coefficient_bits,
            }),
        ),
        no_state_digest_v1::ConvertedLoadKindV1::NotCached => return,
    };
    if fresh_initialization_source::parse_source_v1(&load.base_bytes).is_err() {
        return;
    }
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind: KindV1::Converted {
            run_start,
            check: load.check,
        },
        policy: template,
        state: state.clone(),
        inputs,
    }));
}

fn insert_v1(entry: Arc<LoadedSourceV1>) {
    with_entries_v1(|entries| {
        // Concurrent loads of one source keep a single entry.
        entries.retain(|cached| {
            !(cached.source == entry.source && cached.descriptor_bytes == entry.descriptor_bytes)
        });
        while entries.len() >= LOADED_SOURCE_CACHE_ENTRIES_V1 {
            entries.pop_front();
        }
        entries.push_back(entry);
    });
}
