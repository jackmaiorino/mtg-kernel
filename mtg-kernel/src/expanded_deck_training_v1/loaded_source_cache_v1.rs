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
    check_pinned_v1, ensure, no_state_digest_v1, CheckpointInputExpectationV1,
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
    /// A FlatV7 or FlatV8 source whose checkpoint has that identity, over a
    /// fresh-initialization import (`initialize_from_source_bytes_v1`'s
    /// flat branches): served only by [`lookup_flat_checkpoint_v1`].
    FlatCheckpoint(ExpandedFreshInitializationSourceV1),
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
    static FLAT_MATCHES_FOR_TEST_V1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
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
                    KindV1::FlatCheckpoint(_) => false,
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
                check_pinned_v1(pin)?;
            }
        }
        KindV1::Converted { .. } => {
            no_state_digest_v1::reverify_converted_v1(source, descriptor_bytes)?;
        }
        KindV1::FlatCheckpoint(_) => return Ok(None),
    }
    serve_v1(&entry, inputs)
}

/// The cached result for a FlatV7 or FlatV8 source whose checkpoint has that
/// identity, after the full flat loader's file checks and input binding, or
/// `None` when the source must load in full.
///
/// The full loader reads the checkpoint (for its feature-contract probe),
/// loads the import alone (the initialization manifest and the parameter
/// payload), reads the checkpoint again and restores it. A hit makes the same
/// reads through the same readers, in that order, so a changed or missing
/// file fails exactly as it would fail the full load; only the probe, the
/// parse and the restore are skipped, which are pure functions of those
/// verified bytes and passed when the entry was created.
pub(super) fn lookup_flat_checkpoint_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return Ok(None);
    }
    let Some(pin) = &source.checkpoint else {
        return Ok(None);
    };
    let entry = with_entries_v1(|entries| {
        let index = entries.iter().position(|entry| {
            entry.source == *source
                && entry.descriptor_bytes == descriptor_bytes
                && matches!(entry.kind, KindV1::FlatCheckpoint(_))
        })?;
        let entry = entries.remove(index)?;
        entries.push_back(Arc::clone(&entry));
        Some(entry)
    })
    .flatten();
    let Some(entry) = entry else {
        return Ok(None);
    };
    let KindV1::FlatCheckpoint(descriptor) = &entry.kind else {
        return Ok(None);
    };
    #[cfg(test)]
    FLAT_MATCHES_FOR_TEST_V1.with(|matches| matches.set(matches.get() + 1));
    check_pinned_v1(pin)?;
    fresh_initialization_source::reverify_input_files_v1(descriptor)?;
    check_pinned_v1(pin)?;
    serve_v1(&entry, inputs)
}

/// A hit's input binding, then exact duplicates of the entry's policy and
/// state.
fn serve_v1(
    entry: &LoadedSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
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
pub(super) fn remember_v1<T>(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    saved: Option<&ExpandedCheckpointV1<T>>,
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

/// Keep a full flat load's result (`initialize_from_source_bytes_v1`'s FlatV7
/// or FlatV8 branch with a checkpoint of that identity): `saved` is the
/// checkpoint it restored. Only an import that is a fresh-initialization
/// descriptor, and a checkpoint that names no further file, are kept. Never
/// inlined: its policy template stays out of the loader's frame.
#[inline(never)]
pub(super) fn remember_flat_checkpoint_v1<T>(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    saved: &ExpandedCheckpointV1<T>,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    if source.checkpoint.is_none()
        || saved.archive_distillation.is_some()
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
        kind: KindV1::FlatCheckpoint(descriptor),
        policy: template,
        state: state.clone(),
        inputs: Some(RecordedInputsV1 {
            trajectories: saved.trajectories.clone(),
            learning_rate_bits: saved.learning_rate_bits,
            value_coefficient_bits: saved.value_coefficient_bits,
        }),
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

/// What a learner template ([`LearnerTemplateV1`]) restores: the branch of
/// `initialize_from_source_bytes_v1` its sources take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateKindV1 {
    /// The ordinary branch: a fresh-initialization descriptor under its own
    /// feature identity (fresh V4, or V3).
    Ordinary,
    /// The FlatV7 branch with a FlatV7 checkpoint.
    FlatV7,
    /// The FlatV8 branch.
    FlatV8,
}

/// A learner's policy exactly as the full loader
/// (`initialize_from_source_bytes_v1`) holds it right before it restores a
/// checkpoint: the play import loaded under its feature identity and, for
/// FlatV7 and FlatV8, given that identity. It is a pure function of the
/// descriptor bytes, their initialization manifest and parameter payload, and
/// the feature transfer, so it serves every checkpoint of one import
/// (`collection_tail_v1`, the persistent collection state). It is never
/// restored itself: each restore starts from an exact duplicate of it
/// (`FrozenPlayPolicyV1::duplicate_loaded_v1`).
///
/// Only fresh-initialization descriptors have a template, as only they have
/// loaded-source cache entries; every other source loads in full.
pub(super) struct LearnerTemplateV1 {
    play_import: PinnedFileV1,
    feature_transfer: crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3,
    descriptor_bytes: Vec<u8>,
    descriptor: ExpandedFreshInitializationSourceV1,
    kind: TemplateKindV1,
    policy: FrozenPlayPolicyV1,
}

#[cfg(test)]
thread_local! {
    static TEMPLATE_RESTORES_FOR_TEST_V1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Test-only: restores this thread made from a template.
#[cfg(test)]
pub(super) fn template_restores_for_test_v1() -> u64 {
    TEMPLATE_RESTORES_FOR_TEST_V1.with(std::cell::Cell::get)
}

impl LearnerTemplateV1 {
    /// The template of `source`'s import (`bytes` are its verified
    /// descriptor bytes), or `None` when the source has none or any step of
    /// building it fails. Nothing here is an error: a source without a
    /// template loads in full, and that load reports its own errors.
    pub(super) fn build_v1(source: &ExpandedModelSourceV1, bytes: &[u8]) -> Option<Self> {
        if source.s_learner.is_some() {
            return None;
        }
        let kind = if super::flat_v8_requested_v1(source).ok()? {
            TemplateKindV1::FlatV8
        } else if super::flat_v7_transfer_requested_v1(source).ok()? {
            TemplateKindV1::FlatV7
        } else {
            TemplateKindV1::Ordinary
        };
        // Only a fresh-initialization descriptor; the probe the loader makes.
        let probe: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        if probe.get("schema").and_then(serde_json::Value::as_str)
            != Some(fresh_initialization_source::SOURCE_SCHEMA)
        {
            return None;
        }
        let descriptor = fresh_initialization_source::parse_source_v1(bytes).ok()?;
        // The source the loader loads the import from: the flat branches load
        // it under its V4 identity, the ordinary branch under its own.
        let import_source = ExpandedModelSourceV1 {
            feature_transfer: match kind {
                TemplateKindV1::Ordinary => source.feature_transfer.clone(),
                TemplateKindV1::FlatV7 | TemplateKindV1::FlatV8 => {
                    crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3 {
                        expected_feature_contract_digest: super::FEATURE_CONTRACT_DIGEST_V4.into(),
                        expected_feature_encoding_digest: super::FEATURE_ENCODING_DIGEST_V4.into(),
                    }
                }
            },
            checkpoint: None,
            ..source.clone()
        };
        let policy = super::load_ordinary_policy_v1(&import_source, bytes).ok()?;
        let policy = match kind {
            TemplateKindV1::Ordinary => policy,
            TemplateKindV1::FlatV7 => policy.into_flat_v7_transfer_v1().ok()?,
            TemplateKindV1::FlatV8 => policy.into_flat_v8_v1().ok()?,
        };
        // The template must be duplicable, as every restore duplicates it.
        policy.duplicate_loaded_v1().ok()?;
        Some(Self {
            play_import: source.play_import.clone(),
            feature_transfer: source.feature_transfer.clone(),
            descriptor_bytes: bytes.to_vec(),
            descriptor,
            kind,
            policy,
        })
    }

    /// Whether this template is `source`'s import's, with these descriptor
    /// bytes: the same pinned import, the same feature transfer and the same
    /// bytes. Its checkpoint may be any.
    pub(super) fn serves_v1(&self, source: &ExpandedModelSourceV1, bytes: &[u8]) -> bool {
        source.s_learner.is_none()
            && source.play_import == self.play_import
            && source.feature_transfer == self.feature_transfer
            && bytes == self.descriptor_bytes.as_slice()
    }
}

/// `initialize_from_source_bytes_v1(source, bytes, None)` for a source with a
/// checkpoint whose import `template` serves, through the template:
/// `Ok(None)` when the template does not apply (no checkpoint, another
/// import, a FlatV7 source whose checkpoint is a V4 one, a FlatV8 source
/// whose checkpoint lacks the FlatV8 identity), and the caller loads in full.
///
/// Otherwise it is the full loader with only its import load replaced by an
/// exact duplicate of the template. Every other step is the loader's own, in
/// its order: the same entry checks, the same loaded-source cache lookup (a
/// hit is served as the loader serves it), the same checkpoint probe, the
/// same reads of the import's initialization manifest and parameter payload
/// (`reverify_input_files_v1`, as a cache hit makes them, with the full
/// load's errors), the same read of the checkpoint record, its parse, the
/// loader's restore (`restore_checkpoint_state_v1`) and the same cache entry.
/// So the policy and state are bit for bit the full load's, and a changed,
/// missing or unreadable file fails exactly as it fails the full load. The
/// one difference: a flat full load also keeps its import alone in the
/// loaded-source cache, which a restore does not need; that changes which
/// later loads hit, never what they return.
pub(super) fn initialize_with_template_v1(
    template: &LearnerTemplateV1,
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
    // The loader's entry checks, in its order.
    ensure(
        source.s_learner.is_none(),
        super::s_learner_v1::S_LEARNER_NET8_REFUSAL_V1,
    )?;
    let flat_v8 = super::flat_v8_requested_v1(source)?;
    let flat_v7 = !flat_v8 && super::flat_v7_transfer_requested_v1(source)?;
    let kind = match (flat_v8, flat_v7) {
        (true, _) => TemplateKindV1::FlatV8,
        (false, true) => TemplateKindV1::FlatV7,
        (false, false) => TemplateKindV1::Ordinary,
    };
    let Some(pin) = &source.checkpoint else {
        return Ok(None);
    };
    if kind != template.kind || !template.serves_v1(source, bytes) {
        return Ok(None);
    }
    match kind {
        TemplateKindV1::FlatV7 | TemplateKindV1::FlatV8 => {
            if let Some(hit) = lookup_flat_checkpoint_v1(source, bytes, None)? {
                return Ok(Some(hit));
            }
            let identity = if kind == TemplateKindV1::FlatV8 {
                crate::native_flat_tensorizer_v8::FEATURE_CONTRACT_DIGEST_V8
            } else {
                crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7
            };
            // The loader's probe of the checkpoint; any other identity takes
            // the loader's other way (the V4-to-V7 transfer) or its refusal.
            if super::checkpoint_feature_contract_v1(pin)? != identity {
                return Ok(None);
            }
        }
        TemplateKindV1::Ordinary => {
            if let Some(hit) = lookup_v1(source, bytes, None)? {
                return Ok(Some(hit));
            }
        }
    }
    // The import load's reads; everything else it does is a pure function of
    // these verified bytes, done once for the template.
    fresh_initialization_source::reverify_input_files_v1(&template.descriptor)?;
    let Ok(mut policy) = template.policy.duplicate_loaded_v1() else {
        return Ok(None);
    };
    let model = policy.training_model_v3();
    let record = super::read_checkpoint_record_v1(pin)?;
    let saved = super::parse_ordinary_checkpoint_borrowed_v1(&record, policy.identity_v1())?;
    let state = super::restore_checkpoint_state_v1(&saved, &mut policy, model)?;
    match kind {
        TemplateKindV1::Ordinary => remember_v1(source, bytes, &policy, &state, Some(&saved)),
        TemplateKindV1::FlatV7 | TemplateKindV1::FlatV8 => {
            remember_flat_checkpoint_v1(source, bytes, &policy, &state, &saved)
        }
    }
    #[cfg(test)]
    TEMPLATE_RESTORES_FOR_TEST_V1.with(|restores| restores.set(restores.get() + 1));
    Ok(Some((policy, state)))
}
