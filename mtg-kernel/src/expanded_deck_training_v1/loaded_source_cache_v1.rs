//! Process-wide cache of loaded ordinary fresh-initialization sources.
//!
//! A native training run loads each checkpoint across several roles: the
//! update's round-trip check, update validation, collection and opponent
//! seats. Each load reads and verifies the same pinned files; parsing a 15 to
//! 20 MB checkpoint and rebuilding its policy or optimizer state is a pure
//! function of those verified bytes, so this cache keeps a verified template
//! and serves only what each caller role actually needs:
//! - [`serve_v1`] duplicates the policy and clones the owned train state for
//!   callers that mutate or inspect `NativePolicyValueTrainStateV1` directly;
//! - [`serve_policy_v1`] duplicates the policy and clones its validated
//!   [`ExpandedInferenceIdentityV1`], copying zero train-state or Adam bytes;
//! - [`serve_identity_v1`] clones the validated [`ExpandedInferenceIdentityV1`]
//!   alone after input-binding verification, copying neither train-state nor
//!   policy buffers.
//!
//! Callers that only need inference/validation or that move the single owned
//! train state into `CarriedLearnerV1` remember policy-only entries
//! (`state: None`), so keeping an entry also copies zero train-state bytes.
//!
//! A hit still performs every file check of a full load, in the same order
//! and with the same errors: the descriptor bytes must equal the cached
//! bytes, and the initialization manifest, the parameter payload and the
//! checkpoint are each checked against their SHA-256 pins by the same readers.
//! Only the parses and checks that are pure functions of those bytes are
//! skipped; they passed when the entry was created, so the first failure a hit
//! can report is the one a full load would report. The update's input binding
//! is checked on every call against the inputs the checkpoint records.
//!
//! Only fresh-initialization descriptors are cached, and only when the
//! checkpoint names no further file (a phase-1 archive-distillation
//! checkpoint reads its parent), and converted (`no_state_digest_v1`)
//! sources over such a base: a hit checks the conversion record, the base
//! import and every pin of the base, the artifact or continuation checkpoint
//! in the full converted load's order (`reverify_converted_v1`), and must
//! match the full load's per-forward check setting. Every other source loads
//! in full as before.
//! Duplicates come from `FrozenPlayPolicyV1::duplicate_loaded_v1`, which only
//! accepts a policy in its as-loaded state, and (for state-serving hits only)
//! from cloning the train state.
//! Lookups, hits by serving mode, misses, kept entries and the bytes each copy
//! are counted by caller role (`loaded_source_profile_v1`); counting changes
//! no result.

use super::fresh_initialization_source::{self, ExpandedFreshInitializationSourceV1};
use super::loaded_source_profile_v1 as profile;
use super::{
    check_pinned_v1, ensure, inference_identity_v1, no_state_digest_v1,
    CheckpointInputExpectationV1, ExpandedCheckpointV1, ExpandedInferenceIdentityV1,
    ExpandedModelSourceV1, FrozenPlayPolicyV1, NativePolicyValueTrainStateV1, PinnedFileV1,
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

/// What a cached hit or carried learner must reread, by kind of source.
#[derive(Clone)]
pub(super) enum VerifiableSourceKindV1 {
    /// A fresh-initialization descriptor.
    Fresh(ExpandedFreshInitializationSourceV1),
    /// A converted source over a fresh-initialization base. `run_start`: its
    /// checkpoint is the conversion's base checkpoint. `check`: the
    /// per-forward check setting it loaded under.
    Converted { run_start: bool, check: bool },
    /// A FlatV7 or FlatV8 source whose checkpoint has that identity, over a
    /// fresh-initialization import (`initialize_from_source_bytes_v1`'s
    /// flat branches): served only by the flat checkpoint lookups.
    FlatCheckpoint(ExpandedFreshInitializationSourceV1),
}

impl VerifiableSourceKindV1 {
    pub(super) fn matches_current_setting_v1(&self) -> bool {
        match self {
            Self::Fresh(_) | Self::FlatCheckpoint(_) => true,
            Self::Converted { check, .. } => *check == no_state_digest_v1::check_enabled_v1(),
        }
    }

    fn same_variant_v1(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Fresh(_), Self::Fresh(_)) => true,
            (
                Self::Converted {
                    run_start: a_start,
                    check: a_check,
                },
                Self::Converted {
                    run_start: b_start,
                    check: b_check,
                },
            ) => a_start == b_start && a_check == b_check,
            (Self::FlatCheckpoint(_), Self::FlatCheckpoint(_)) => true,
            _ => false,
        }
    }

    /// Runs every file check of the full load for `source`, in its exact order
    /// and with its exact errors.
    pub(super) fn reverify_v1(
        &self,
        source: &ExpandedModelSourceV1,
        descriptor_bytes: &[u8],
    ) -> Result<(), String> {
        match self {
            Self::Fresh(descriptor) => {
                fresh_initialization_source::reverify_input_files_v1(descriptor)?;
                if let Some(pin) = &source.checkpoint {
                    check_pinned_v1(pin)?;
                }
                Ok(())
            }
            Self::Converted { .. } => {
                no_state_digest_v1::reverify_converted_v1(source, descriptor_bytes)
            }
            Self::FlatCheckpoint(descriptor) => {
                let pin = source
                    .checkpoint
                    .as_ref()
                    .ok_or("a flat checkpoint source requires a checkpoint")?;
                check_pinned_v1(pin)?;
                fresh_initialization_source::reverify_input_files_v1(descriptor)?;
                check_pinned_v1(pin)
            }
        }
    }
}

pub(super) fn classify_fresh_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    saved: Option<&ExpandedCheckpointV1>,
) -> Option<(VerifiableSourceKindV1, Option<RecordedInputsV1>)> {
    if source.checkpoint.is_some() != saved.is_some()
        || saved.is_some_and(|saved| saved.archive_distillation.is_some())
        || policy.input_contract_v1().is_some()
    {
        return None;
    }
    let descriptor = fresh_initialization_source::parse_source_v1(descriptor_bytes).ok()?;
    let inputs = saved.map(|saved| RecordedInputsV1 {
        trajectories: saved.trajectories.clone(),
        learning_rate_bits: saved.learning_rate_bits,
        value_coefficient_bits: saved.value_coefficient_bits,
    });
    Some((VerifiableSourceKindV1::Fresh(descriptor), inputs))
}

pub(super) fn classify_converted_v1(
    load: &no_state_digest_v1::ConvertedLoadV1,
) -> Option<(VerifiableSourceKindV1, Option<RecordedInputsV1>)> {
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
        no_state_digest_v1::ConvertedLoadKindV1::NotCached => return None,
    };
    fresh_initialization_source::parse_source_v1(&load.base_bytes).ok()?;
    Some((
        VerifiableSourceKindV1::Converted {
            run_start,
            check: load.check,
        },
        inputs,
    ))
}

pub(super) fn classify_flat_checkpoint_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    saved: &ExpandedCheckpointV1,
) -> Option<(VerifiableSourceKindV1, Option<RecordedInputsV1>)> {
    if source.checkpoint.is_none()
        || saved.archive_distillation.is_some()
        || policy.input_contract_v1().is_some()
    {
        return None;
    }
    let descriptor = fresh_initialization_source::parse_source_v1(descriptor_bytes).ok()?;
    let inputs = Some(RecordedInputsV1 {
        trajectories: saved.trajectories.clone(),
        learning_rate_bits: saved.learning_rate_bits,
        value_coefficient_bits: saved.value_coefficient_bits,
    });
    Some((VerifiableSourceKindV1::FlatCheckpoint(descriptor), inputs))
}

struct LoadedSourceV1 {
    source: ExpandedModelSourceV1,
    descriptor_bytes: Vec<u8>,
    kind: VerifiableSourceKindV1,
    policy: FrozenPlayPolicyV1,
    identity: ExpandedInferenceIdentityV1,
    /// Owned train state when remembered by a state-requiring caller; `None`
    /// when remembered for policy/identity serving only.
    state: Option<NativePolicyValueTrainStateV1>,
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

fn find_verified_entry_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
    require_state: bool,
) -> Result<Option<Arc<LoadedSourceV1>>, String> {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return Ok(None);
    }
    profile::record_lookup_v1();
    let entry = with_entries_v1(|entries| {
        let index = entries.iter().position(|entry| {
            entry.source == *source
                && entry.descriptor_bytes == descriptor_bytes
                && (!require_state || entry.state.is_some())
                && match entry.kind {
                    VerifiableSourceKindV1::Fresh(_) => true,
                    // A converted run start's update inputs bind the base
                    // checkpoint inside the base load; it is served only
                    // without them. Either kind matches only the current
                    // per-forward check setting.
                    VerifiableSourceKindV1::Converted { run_start, check } => {
                        !(run_start && inputs.is_some())
                            && check == no_state_digest_v1::check_enabled_v1()
                    }
                    VerifiableSourceKindV1::FlatCheckpoint(_) => false,
                }
        })?;
        let entry = entries.remove(index)?;
        entries.push_back(Arc::clone(&entry));
        Some(entry)
    })
    .flatten();
    let Some(entry) = entry else {
        profile::record_miss_v1();
        return Ok(None);
    };
    #[cfg(test)]
    MATCHES_FOR_TEST_V1.with(|matches| matches.set(matches.get() + 1));
    if matches!(entry.kind, VerifiableSourceKindV1::FlatCheckpoint(_)) {
        profile::record_miss_v1();
        return Ok(None);
    }
    entry.kind.reverify_v1(source, descriptor_bytes)?;
    Ok(Some(entry))
}

fn find_verified_flat_entry_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    require_state: bool,
) -> Result<Option<Arc<LoadedSourceV1>>, String> {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return Ok(None);
    }
    if source.checkpoint.is_none() {
        return Ok(None);
    }
    profile::record_lookup_v1();
    let entry = with_entries_v1(|entries| {
        let index = entries.iter().position(|entry| {
            entry.source == *source
                && entry.descriptor_bytes == descriptor_bytes
                && (!require_state || entry.state.is_some())
                && matches!(entry.kind, VerifiableSourceKindV1::FlatCheckpoint(_))
        })?;
        let entry = entries.remove(index)?;
        entries.push_back(Arc::clone(&entry));
        Some(entry)
    })
    .flatten();
    let Some(entry) = entry else {
        profile::record_miss_v1();
        return Ok(None);
    };
    if !matches!(entry.kind, VerifiableSourceKindV1::FlatCheckpoint(_)) {
        profile::record_miss_v1();
        return Ok(None);
    }
    #[cfg(test)]
    FLAT_MATCHES_FOR_TEST_V1.with(|matches| matches.set(matches.get() + 1));
    entry.kind.reverify_v1(source, descriptor_bytes)?;
    Ok(Some(entry))
}

/// The cached policy and mutable train state for these descriptor bytes, after
/// the full loader's file checks and input binding, or `None` when the source
/// must load in full.
#[cfg(test)]
pub(super) fn lookup_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
    Ok(lookup_with_kind_v1(source, descriptor_bytes, inputs)?
        .map(|(policy, state, _, _)| (policy, state)))
}

/// [`lookup_v1`] also returning the entry's validated identity and
/// reverification kind so carried-state callers need not rehash the state.
#[allow(clippy::type_complexity)]
pub(super) fn lookup_with_kind_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<
    Option<(
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        ExpandedInferenceIdentityV1,
        VerifiableSourceKindV1,
    )>,
    String,
> {
    let Some(entry) = find_verified_entry_v1(source, descriptor_bytes, inputs, true)? else {
        return Ok(None);
    };
    Ok(serve_v1(&entry, inputs)?
        .map(|(policy, state)| (policy, state, entry.identity.clone(), entry.kind.clone())))
}

/// The cached policy and validated inference identity for these descriptor
/// bytes, copying zero train-state or Adam bytes.
pub(super) fn lookup_policy_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1)>, String> {
    let Some(entry) = find_verified_entry_v1(source, descriptor_bytes, inputs, false)? else {
        return Ok(None);
    };
    serve_policy_v1(&entry, inputs)
}

/// The cached validated inference identity alone for these descriptor bytes,
/// copying zero train-state or policy bytes.
pub(super) fn lookup_identity_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<ExpandedInferenceIdentityV1>, String> {
    let Some(entry) = find_verified_entry_v1(source, descriptor_bytes, inputs, false)? else {
        return Ok(None);
    };
    serve_identity_v1(&entry, inputs)
}

/// The cached policy and mutable train state for a FlatV7 or FlatV8 source
/// whose checkpoint has that identity, after the full flat loader's file checks
/// and input binding, or `None` when the source must load in full.
#[cfg(test)]
pub(super) fn lookup_flat_checkpoint_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
    Ok(
        lookup_flat_checkpoint_with_kind_v1(source, descriptor_bytes, inputs)?
            .map(|(policy, state, _, _)| (policy, state)),
    )
}

/// [`lookup_flat_checkpoint_v1`] also returning the entry's validated identity
/// and reverification kind.
#[allow(clippy::type_complexity)]
pub(super) fn lookup_flat_checkpoint_with_kind_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<
    Option<(
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        ExpandedInferenceIdentityV1,
        VerifiableSourceKindV1,
    )>,
    String,
> {
    let Some(entry) = find_verified_flat_entry_v1(source, descriptor_bytes, true)? else {
        return Ok(None);
    };
    Ok(serve_v1(&entry, inputs)?
        .map(|(policy, state)| (policy, state, entry.identity.clone(), entry.kind.clone())))
}

/// Policy-only lookup for a FlatV7 or FlatV8 checkpoint source, copying zero
/// train-state bytes.
pub(super) fn lookup_flat_checkpoint_policy_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1)>, String> {
    let Some(entry) = find_verified_flat_entry_v1(source, descriptor_bytes, false)? else {
        return Ok(None);
    };
    serve_policy_v1(&entry, inputs)
}

/// Identity-only lookup for a FlatV7 or FlatV8 checkpoint source, copying
/// neither train-state nor policy buffers.
pub(super) fn lookup_flat_checkpoint_identity_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<ExpandedInferenceIdentityV1>, String> {
    let Some(entry) = find_verified_flat_entry_v1(source, descriptor_bytes, false)? else {
        return Ok(None);
    };
    serve_identity_v1(&entry, inputs)
}

fn validate_inputs_v1(
    entry: &LoadedSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<(), String> {
    match (&entry.inputs, inputs) {
        (Some(recorded), Some(expected)) => expected.validate_v1(
            &recorded.trajectories,
            recorded.learning_rate_bits,
            recorded.value_coefficient_bits,
        ),
        (None, expected) => ensure(expected.is_none(), "update requires checkpoint"),
        (Some(_), None) => Ok(()),
    }
}

/// A hit's input binding, then exact duplicates of the entry's policy and
/// mutable train state.
fn serve_v1(
    entry: &LoadedSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1)>, String> {
    validate_inputs_v1(entry, inputs)?;
    let Some(state) = entry.state.as_ref() else {
        profile::record_miss_v1();
        return Ok(None);
    };
    let Ok(policy) = entry.policy.duplicate_loaded_v1() else {
        profile::record_miss_v1();
        return Ok(None);
    };
    let state = state.clone();
    let (state_bytes, policy_bytes) = (
        state_copied_bytes_v1(&state),
        policy_copied_bytes_v1(&policy),
    );
    profile::state_served_v1(state_bytes, policy_bytes);
    Ok(Some((policy, state)))
}

/// A hit's input binding, then an exact duplicate of the entry's policy and
/// validated identity without cloning `NativePolicyValueTrainStateV1`.
fn serve_policy_v1(
    entry: &LoadedSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1)>, String> {
    validate_inputs_v1(entry, inputs)?;
    let Ok(policy) = entry.policy.duplicate_loaded_v1() else {
        profile::record_miss_v1();
        return Ok(None);
    };
    let policy_bytes = policy_copied_bytes_v1(&policy);
    profile::policy_served_v1(policy_bytes);
    Ok(Some((policy, entry.identity.clone())))
}

/// A hit's input binding, then the entry's validated identity alone without
/// cloning either `NativePolicyValueTrainStateV1` or `FrozenPlayPolicyV1`.
fn serve_identity_v1(
    entry: &LoadedSourceV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<Option<ExpandedInferenceIdentityV1>, String> {
    validate_inputs_v1(entry, inputs)?;
    profile::identity_served_v1();
    Ok(Some(entry.identity.clone()))
}

pub(super) fn state_copied_bytes_v1(state: &NativePolicyValueTrainStateV1) -> u64 {
    profile::f32_bytes_v1(state.owned_f32_len_v1())
}

pub(super) fn policy_copied_bytes_v1(policy: &FrozenPlayPolicyV1) -> u64 {
    profile::f32_bytes_v1(policy.owned_f32_len_v1())
}

/// Keep a full load's result, including its mutable train state. `saved` is
/// the checkpoint it restored, if any.
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
    let Some((kind, inputs)) = classify_fresh_v1(source, descriptor_bytes, policy, saved) else {
        return;
    };
    let Ok(identity) = inference_identity_v1(source, policy, state) else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind,
        policy: template,
        identity,
        state: Some(state.clone()),
        inputs,
    }));
}

/// Keep a full load's policy and validated identity without cloning its train
/// state (`kept_state_bytes == 0`).
pub(super) fn remember_policy_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    identity: &ExpandedInferenceIdentityV1,
    saved: Option<&ExpandedCheckpointV1>,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    let Some((kind, inputs)) = classify_fresh_v1(source, descriptor_bytes, policy, saved) else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind,
        policy: template,
        identity: identity.clone(),
        state: None,
        inputs,
    }));
}

/// Keep a full converted load's result (`no_state_digest_v1::initialize_training_v1`)
/// when its base is a fresh-initialization source and its checkpoint names
/// no further file. Never inlined: its policy template stays out of the
/// loader's frame.
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
    let Some((kind, inputs)) = classify_converted_v1(load) else {
        return;
    };
    let Ok(identity) = inference_identity_v1(source, policy, state) else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind,
        policy: template,
        identity,
        state: Some(state.clone()),
        inputs,
    }));
}

/// Keep a converted load's policy and validated identity without cloning its
/// train state. Never inlined: its policy template stays out of the loader's
/// frame.
#[inline(never)]
pub(super) fn remember_converted_policy_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    identity: &ExpandedInferenceIdentityV1,
    load: &no_state_digest_v1::ConvertedLoadV1,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    let Some((kind, inputs)) = classify_converted_v1(load) else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind,
        policy: template,
        identity: identity.clone(),
        state: None,
        inputs,
    }));
}

/// Keep a full flat load's result (`initialize_from_source_bytes_v1`'s FlatV7
/// or FlatV8 branch with a checkpoint of that identity). Never inlined: its
/// policy template stays out of the loader's frame.
#[inline(never)]
pub(super) fn remember_flat_checkpoint_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    state: &NativePolicyValueTrainStateV1,
    saved: &ExpandedCheckpointV1,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    let Some((kind, inputs)) = classify_flat_checkpoint_v1(source, descriptor_bytes, policy, saved)
    else {
        return;
    };
    let Ok(identity) = inference_identity_v1(source, policy, state) else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind,
        policy: template,
        identity,
        state: Some(state.clone()),
        inputs,
    }));
}

/// Keep a flat checkpoint load's policy and validated identity without cloning
/// its train state. Never inlined: its policy template stays out of the
/// loader's frame.
#[inline(never)]
pub(super) fn remember_flat_checkpoint_policy_v1(
    source: &ExpandedModelSourceV1,
    descriptor_bytes: &[u8],
    policy: &FrozenPlayPolicyV1,
    identity: &ExpandedInferenceIdentityV1,
    saved: &ExpandedCheckpointV1,
) {
    #[cfg(test)]
    if BYPASS_FOR_TEST_V1.with(std::cell::Cell::get) {
        return;
    }
    let Some((kind, inputs)) = classify_flat_checkpoint_v1(source, descriptor_bytes, policy, saved)
    else {
        return;
    };
    let Ok(template) = policy.duplicate_loaded_v1() else {
        return;
    };
    insert_v1(Arc::new(LoadedSourceV1 {
        source: source.clone(),
        descriptor_bytes: descriptor_bytes.to_vec(),
        kind,
        policy: template,
        identity: identity.clone(),
        state: None,
        inputs,
    }));
}

fn insert_v1(entry: Arc<LoadedSourceV1>) {
    let state_bytes = entry.state.as_ref().map_or(0, state_copied_bytes_v1);
    let policy_bytes = policy_copied_bytes_v1(&entry.policy);
    let inserted = with_entries_v1(|entries| {
        if entry.state.is_none()
            && entries.iter().any(|cached| {
                cached.source == entry.source
                    && cached.descriptor_bytes == entry.descriptor_bytes
                    && cached.state.is_some()
                    && cached.kind.same_variant_v1(&entry.kind)
            })
        {
            return false;
        }
        // Concurrent loads of one source keep a single entry.
        entries.retain(|cached| {
            !(cached.source == entry.source && cached.descriptor_bytes == entry.descriptor_bytes)
        });
        while entries.len() >= LOADED_SOURCE_CACHE_ENTRIES_V1 {
            entries.pop_front();
        }
        entries.push_back(entry);
        true
    });
    if inserted == Some(true) {
        profile::record_kept_v1(state_bytes, policy_bytes);
    }
}
