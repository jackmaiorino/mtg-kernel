//! Launcher-level, hash-chained baseline checkpoint chain for
//! terminal_reinforce_value/v4.
//!
//! Contract: `docs/native_trainer_terminal_reinforce_value_v4_candidate_v1.md`
//! section 5 ("Persistence architecture"). Pinned deviation from the
//! contract's own prose (flagged prominently, per this deliverable's task
//! order): section 5's "Baseline-state persistence" text describes the
//! baseline joining the Store's checkpoint publication directly as a v4
//! sibling schema. This module instead keeps the arm's v3 Store completely
//! untouched -- no v3 wire struct, publish schedule, resume read set, or
//! leaf grammar is modified -- and persists the baseline as an independent,
//! launcher-owned, append-only, hash-chained stream of records living in its
//! own directory, one record per Store checkpoint boundary. Each record
//! embeds (by hash binding to an adjacent file, not byte-exact JSON nesting)
//! a full `native_training_store_checkpoint_v4` checkpoint-manifest v4
//! authority, whose `core_state_sha256` input is the paired v3 Store
//! checkpoint's own unchanged core train-state hash for that generation.
//! This lets an independent validator re-derive the exact baseline
//! trajectory from durable evidence alone, exactly as the Store validator
//! recomputes the v3 loss, without this module ever writing into Store
//! territory.
//!
//! Embedding choice (contract-authorized either/or, decided here): the
//! record is a small canonical-JSON envelope carrying `manifest_sha256`
//! rather than the manifest's own JSON nested inline. Byte-exact nesting
//! would require re-serializing a `serde_json::Value` through the canonical
//! codec and trusting that round trip to reproduce the manifest's own
//! canonical bytes exactly; binding by hash to a same-generation adjacent
//! file (`baseline-<gen>.manifest.json`, itself decoded by
//! `native_training_store_checkpoint_v4`'s own strict decoder) is simpler,
//! avoids that risk entirely, and is still fully tamper-evident.
//!
//! Chain layout in the caller-owned directory `dir`:
//!
//! ```text
//! baseline-00000000.record.json     baseline-00000000.manifest.json
//! baseline-00000004.record.json     baseline-00000004.manifest.json
//! ...
//! ```
//!
//! Each record is a canonical-JSON `deny_unknown_fields` envelope: schema
//! `mtg-kernel-baseline-chain-record/v1`, `store_generation_index`,
//! `previous_record_sha256` (`null` only for the genesis record),
//! `run_sha256`, and `checkpoint_manifest_sha256` (the adjacent manifest
//! file's own `checkpoint_manifest_sha256`). Publication uses the durable
//! create-new (no-replace) move primitives from `durable_move_publication_v2`
//! directly, so the chain is append-only by construction: any attempt to
//! publish over an existing generation fails closed.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPathSegmentV1, CanonicalJsonNullPolicyV1,
};
use crate::durable_move_publication_v2::publish_immutable_file_by_move_v2;
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
    DurablePublicationErrorKindV1, DurablePublicationErrorV1,
};
use crate::native_policy_baseline_state_v4::NativeBaselineStateV4;
use crate::native_training_store_checkpoint_v4::{
    build_checkpoint_manifest_v4, decode_checkpoint_manifest_v4, CheckpointManifestPartsV4,
    CheckpointManifestV4ErrorKind,
};
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::Path;

/// Exact schema string for one chain record envelope.
pub(crate) const BASELINE_CHAIN_RECORD_SCHEMA_V4: &str = "mtg-kernel-baseline-chain-record/v1";

const RECORD_NAME_PREFIX_V4: &str = "baseline-";
const RECORD_NAME_SUFFIX_V4: &str = ".record.json";
const MANIFEST_NAME_SUFFIX_V4: &str = ".manifest.json";
const STAGE_SUFFIX_V4: &str = ".stage-v4";
const GENERATION_DIGITS_V4: usize = 8;
/// Mirrors `native_training_store_layout_v2::NATIVE_TRAINING_STORE_MAX_UPDATE_INDEX_V2`:
/// the same eight-digit fixed-width bound, kept independent of the Store's
/// own layout module since this chain owns its own grammar.
const MAX_GENERATION_INDEX_V4: u64 = 99_999_999;

const PREVIOUS_RECORD_SHA256_NULL_PATH_V4: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "previous_record_sha256",
    )];

fn record_null_policy_v4() -> CanonicalJsonNullPolicyV1 {
    CanonicalJsonNullPolicyV1::AllowOnly(&[PREVIOUS_RECORD_SHA256_NULL_PATH_V4])
}

/// One chain record envelope. `previous_record_sha256` is `None` only for
/// the genesis record (the first record ever published into an empty
/// chain directory).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct BaselineChainRecordWireV4 {
    schema: String,
    run_sha256: String,
    store_generation_index: u64,
    previous_record_sha256: Option<String>,
    checkpoint_manifest_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BaselineChainErrorKindV4 {
    Io,
    InvalidGeneration,
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidSchema,
    InvalidDigest,
    RunIdentityMismatch,
    GenerationMismatch,
    NotGenesis,
    ExpectedGenesis,
    BrokenHashChain,
    ManifestBindingMismatch,
    ManifestDecode(CheckpointManifestV4ErrorKind),
    ManifestMissing,
    StoreCheckpointMissingForGeneration,
    StalePreviousRecord,
    NonMonotonicGeneration,
    UnsortedStoreCheckpoints,
    EmptyStoreCheckpoints,
    ChainAheadOfStore,
    GapOrTamper,
    Publication(DurablePublicationErrorKindV1),
}

impl BaselineChainErrorKindV4 {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Io => "native_baseline_chain_v4_io",
            Self::InvalidGeneration => "native_baseline_chain_v4_invalid_generation",
            Self::CanonicalJson(kind) => kind.code(),
            Self::InvalidSchema => "native_baseline_chain_v4_invalid_schema",
            Self::InvalidDigest => "native_baseline_chain_v4_invalid_digest",
            Self::RunIdentityMismatch => "native_baseline_chain_v4_run_identity_mismatch",
            Self::GenerationMismatch => "native_baseline_chain_v4_generation_mismatch",
            Self::NotGenesis => "native_baseline_chain_v4_not_genesis",
            Self::ExpectedGenesis => "native_baseline_chain_v4_expected_genesis",
            Self::BrokenHashChain => "native_baseline_chain_v4_broken_hash_chain",
            Self::ManifestBindingMismatch => "native_baseline_chain_v4_manifest_binding_mismatch",
            Self::ManifestDecode(kind) => kind.code(),
            Self::ManifestMissing => "native_baseline_chain_v4_manifest_missing",
            Self::StoreCheckpointMissingForGeneration => {
                "native_baseline_chain_v4_store_checkpoint_missing_for_generation"
            }
            Self::StalePreviousRecord => "native_baseline_chain_v4_stale_previous_record",
            Self::NonMonotonicGeneration => "native_baseline_chain_v4_non_monotonic_generation",
            Self::UnsortedStoreCheckpoints => "native_baseline_chain_v4_unsorted_store_checkpoints",
            Self::EmptyStoreCheckpoints => "native_baseline_chain_v4_empty_store_checkpoints",
            Self::ChainAheadOfStore => "native_baseline_chain_v4_chain_ahead_of_store",
            Self::GapOrTamper => "native_baseline_chain_v4_gap_or_tamper",
            Self::Publication(kind) => publication_error_code_v4(kind),
        }
    }
}

const fn publication_error_code_v4(kind: DurablePublicationErrorKindV1) -> &'static str {
    match kind {
        DurablePublicationErrorKindV1::InvalidParent => {
            "native_baseline_chain_v4_pub_invalid_parent"
        }
        DurablePublicationErrorKindV1::ParentChanged => {
            "native_baseline_chain_v4_pub_parent_changed"
        }
        DurablePublicationErrorKindV1::InvalidChildName => {
            "native_baseline_chain_v4_pub_invalid_child_name"
        }
        DurablePublicationErrorKindV1::InputContentMismatch => {
            "native_baseline_chain_v4_pub_input_content_mismatch"
        }
        DurablePublicationErrorKindV1::StageCollision => {
            "native_baseline_chain_v4_pub_stage_collision"
        }
        DurablePublicationErrorKindV1::StageCreate => "native_baseline_chain_v4_pub_stage_create",
        DurablePublicationErrorKindV1::StageWrite => "native_baseline_chain_v4_pub_stage_write",
        DurablePublicationErrorKindV1::StageSync => "native_baseline_chain_v4_pub_stage_sync",
        DurablePublicationErrorKindV1::StageVerification => {
            "native_baseline_chain_v4_pub_stage_verification"
        }
        DurablePublicationErrorKindV1::FinalCollision => {
            "native_baseline_chain_v4_pub_final_collision"
        }
        DurablePublicationErrorKindV1::FinalPublish => "native_baseline_chain_v4_pub_final_publish",
        DurablePublicationErrorKindV1::ParentNamespaceSync => {
            "native_baseline_chain_v4_pub_parent_namespace_sync"
        }
        DurablePublicationErrorKindV1::FinalVerification => {
            "native_baseline_chain_v4_pub_final_verification"
        }
        DurablePublicationErrorKindV1::StageCleanup => "native_baseline_chain_v4_pub_stage_cleanup",
        DurablePublicationErrorKindV1::UnsupportedPlatform => {
            "native_baseline_chain_v4_pub_unsupported_platform"
        }
        #[cfg(test)]
        DurablePublicationErrorKindV1::InjectedFault => {
            "native_baseline_chain_v4_pub_injected_fault"
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BaselineChainErrorV4 {
    kind: BaselineChainErrorKindV4,
}

impl BaselineChainErrorV4 {
    const fn new(kind: BaselineChainErrorKindV4) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind(self) -> BaselineChainErrorKindV4 {
        self.kind
    }
}

impl Display for BaselineChainErrorV4 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.kind.code())
    }
}

impl Error for BaselineChainErrorV4 {}

impl From<CanonicalJsonErrorV1> for BaselineChainErrorV4 {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(BaselineChainErrorKindV4::CanonicalJson(error.kind()))
    }
}

fn io_err_v4(_: io::Error) -> BaselineChainErrorV4 {
    BaselineChainErrorV4::new(BaselineChainErrorKindV4::Io)
}

fn publication_err_v4(error: DurablePublicationErrorV1) -> BaselineChainErrorV4 {
    BaselineChainErrorV4::new(BaselineChainErrorKindV4::Publication(error.kind()))
}

type Result<T> = std::result::Result<T, BaselineChainErrorV4>;

fn validate_hex_digest_v4(value: &str) -> Result<()> {
    parse_lower_hex_raw32_v1(value)
        .map_err(|_| BaselineChainErrorV4::new(BaselineChainErrorKindV4::InvalidDigest))?;
    Ok(())
}

fn fixed_generation_v4(generation: u64) -> Result<String> {
    if generation > MAX_GENERATION_INDEX_V4 {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::InvalidGeneration,
        ));
    }
    Ok(format!(
        "{generation:0width$}",
        width = GENERATION_DIGITS_V4
    ))
}

fn parse_fixed_generation_v4(text: &str) -> Option<u64> {
    if text.len() != GENERATION_DIGITS_V4 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok()
}

fn record_final_name_v4(generation: u64) -> Result<String> {
    Ok(format!(
        "{RECORD_NAME_PREFIX_V4}{}{RECORD_NAME_SUFFIX_V4}",
        fixed_generation_v4(generation)?
    ))
}

fn manifest_final_name_v4(generation: u64) -> Result<String> {
    Ok(format!(
        "{RECORD_NAME_PREFIX_V4}{}{MANIFEST_NAME_SUFFIX_V4}",
        fixed_generation_v4(generation)?
    ))
}

fn stage_name_v4(final_name: &str) -> String {
    format!(".{final_name}{STAGE_SUFFIX_V4}")
}

fn encode_record_v4(record: &BaselineChainRecordWireV4) -> Result<Vec<u8>> {
    Ok(to_canonical_json_bytes_v1(record, record_null_policy_v4())?)
}

/// Decodes one record envelope: canonical-JSON round trip and
/// `deny_unknown_fields` are enforced by `from_canonical_json_bytes_v1`
/// itself; this adds the schema check and digest-format validation for
/// every declared hex field.
fn decode_record_v4(bytes: &[u8]) -> Result<BaselineChainRecordWireV4> {
    let wire: BaselineChainRecordWireV4 =
        from_canonical_json_bytes_v1(bytes, record_null_policy_v4())?;
    if wire.schema != BASELINE_CHAIN_RECORD_SCHEMA_V4 {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::InvalidSchema,
        ));
    }
    validate_hex_digest_v4(&wire.run_sha256)?;
    validate_hex_digest_v4(&wire.checkpoint_manifest_sha256)?;
    if let Some(previous) = &wire.previous_record_sha256 {
        validate_hex_digest_v4(previous)?;
    }
    Ok(wire)
}

/// Lists every recognized record generation present in `dir`, ascending. A
/// directory that does not exist yet is treated as an empty chain (the
/// pre-first-publish state), not an error. Entries other than
/// `baseline-<8 digits>.record.json` are ignored (adjacent manifest files,
/// stage debris, or unrelated content).
fn list_record_generations_v4(dir: &Path) -> Result<Vec<u64>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_err_v4(error)),
    };
    let mut generations = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_err_v4)?;
        let file_type = entry.file_type().map_err(io_err_v4)?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(stem) = name.strip_prefix(RECORD_NAME_PREFIX_V4) else {
            continue;
        };
        let Some(index_text) = stem.strip_suffix(RECORD_NAME_SUFFIX_V4) else {
            continue;
        };
        let generation = parse_fixed_generation_v4(index_text).ok_or_else(|| {
            BaselineChainErrorV4::new(BaselineChainErrorKindV4::InvalidGeneration)
        })?;
        generations.push(generation);
    }
    generations.sort_unstable();
    Ok(generations)
}

fn read_record_bytes_v4(dir: &Path, generation: u64) -> Result<Vec<u8>> {
    let name = record_final_name_v4(generation)?;
    fs::read(dir.join(name)).map_err(io_err_v4)
}

fn read_manifest_bytes_v4(dir: &Path, generation: u64) -> Result<Vec<u8>> {
    let name = manifest_final_name_v4(generation)?;
    fs::read(dir.join(name))
        .map_err(|_| BaselineChainErrorV4::new(BaselineChainErrorKindV4::ManifestMissing))
}

fn store_core_hash_lookup_v4(
    store_checkpoints: &[(u64, [u8; 32])],
    generation: u64,
) -> Option<[u8; 32]> {
    store_checkpoints
        .binary_search_by_key(&generation, |(candidate, _)| *candidate)
        .ok()
        .map(|index| store_checkpoints[index].1)
}

/// Caller-supplied chain-linkage assertion for one new record: the
/// compare-and-swap guard. The caller states what it believes the current
/// chain tip's own record sha is (or `None` for a genesis publish, i.e. an
/// empty chain directory); the publisher verifies this belief against the
/// actual directory contents before publishing, so a stale caller view
/// fails closed rather than silently forking the chain.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BaselineChainRecordPartsV4 {
    pub(crate) expected_previous_record_sha256: Option<[u8; 32]>,
}

/// Builds the v4 checkpoint manifest from `manifest_parts`, then durably
/// publishes it and its binding record into `dir` (created if absent) via
/// the create-new (no-replace) move primitives, in that order: an orphaned
/// manifest file with no matching record is harmless and ignored by every
/// read path here, while the reverse would leave a record whose manifest is
/// missing. Fails closed if either final name already exists (append-only),
/// or if `manifest_parts.generation_index` is not strictly greater than the
/// chain's current tip generation, or if `record_parts` does not agree with
/// the chain's actual current tip. Returns the published record's own
/// SHA-256 (over its canonical bytes), the value the *next* publish must
/// supply as `expected_previous_record_sha256`.
pub(crate) fn publish_baseline_record_v4(
    dir: &Path,
    record_parts: BaselineChainRecordPartsV4,
    manifest_parts: CheckpointManifestPartsV4,
) -> Result<[u8; 32]> {
    let manifest = build_checkpoint_manifest_v4(manifest_parts).map_err(|error| {
        BaselineChainErrorV4::new(BaselineChainErrorKindV4::ManifestDecode(error.kind()))
    })?;
    let generation_index = manifest.generation_index();
    let record_final_name = record_final_name_v4(generation_index)?;
    let manifest_final_name = manifest_final_name_v4(generation_index)?;

    let existing = list_record_generations_v4(dir)?;
    let previous_record_sha256 = match existing.last() {
        None => {
            if record_parts.expected_previous_record_sha256.is_some() {
                return Err(BaselineChainErrorV4::new(
                    BaselineChainErrorKindV4::ExpectedGenesis,
                ));
            }
            None
        }
        Some(&tip_generation) => {
            if generation_index <= tip_generation {
                return Err(BaselineChainErrorV4::new(
                    BaselineChainErrorKindV4::NonMonotonicGeneration,
                ));
            }
            let tip_bytes = read_record_bytes_v4(dir, tip_generation)?;
            let tip_sha256 = sha256_v1(&tip_bytes);
            if record_parts.expected_previous_record_sha256 != Some(tip_sha256) {
                return Err(BaselineChainErrorV4::new(
                    BaselineChainErrorKindV4::StalePreviousRecord,
                ));
            }
            Some(lower_hex_raw32_v1(tip_sha256))
        }
    };

    let record_wire = BaselineChainRecordWireV4 {
        schema: BASELINE_CHAIN_RECORD_SCHEMA_V4.to_owned(),
        run_sha256: manifest.run_sha256().to_owned(),
        store_generation_index: generation_index,
        previous_record_sha256,
        checkpoint_manifest_sha256: lower_hex_raw32_v1(manifest.checkpoint_manifest_sha256()),
    };
    let record_bytes = encode_record_v4(&record_wire)?;

    fs::create_dir_all(dir).map_err(io_err_v4)?;
    let parent = capture_existing_publication_parent_v1(dir).map_err(publication_err_v4)?;

    let manifest_bytes = manifest.canonical_bytes();
    let manifest_expectation =
        DurableFileExpectationV1::from_bytes(manifest_bytes).map_err(publication_err_v4)?;
    publish_immutable_file_by_move_v2(
        &parent,
        stage_name_v4(&manifest_final_name),
        &manifest_final_name,
        manifest_bytes,
        manifest_expectation,
    )
    .map_err(publication_err_v4)?;

    let record_expectation =
        DurableFileExpectationV1::from_bytes(&record_bytes).map_err(publication_err_v4)?;
    publish_immutable_file_by_move_v2(
        &parent,
        stage_name_v4(&record_final_name),
        &record_final_name,
        &record_bytes,
        record_expectation,
    )
    .map_err(publication_err_v4)?;

    Ok(sha256_v1(&record_bytes))
}

/// The outcome of walking the chain against the Store's own checkpoint
/// generation list. Only the two forward-tolerant outcomes are represented
/// here; every other relationship (the chain leading the store, a gap, or a
/// tampered record) is a hard [`BaselineChainErrorV4`] from
/// [`resume_baseline_chain_v4`] or [`baseline_chain_recovery_verdict_v4`],
/// never a variant of this type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BaselineChainResumeVerdictV4 {
    /// The chain's newest record's generation equals the Store's newest
    /// checkpoint's generation.
    Clean,
    /// The Store has published exactly one checkpoint boundary beyond the
    /// chain's newest record (the crash window between a Store checkpoint
    /// commit and this launcher's paired baseline-record publish). Resume
    /// falls back to the chain's own tip: the older, still-fully-committed
    /// baseline state.
    StoreAheadByOneBoundary,
}

/// The newest agreeing (generation, baseline state) pair recovered from the
/// chain, plus the verdict that produced it. `generation_index` is `None`
/// only when the chain has no records at all yet (the pre-genesis crash
/// window, when the verdict must be
/// [`BaselineChainResumeVerdictV4::StoreAheadByOneBoundary`]); the paired
/// baseline state is then exactly [`NativeBaselineStateV4::empty_v4`].
#[derive(Clone, Debug)]
pub(crate) struct BaselineChainResumeV4 {
    verdict: BaselineChainResumeVerdictV4,
    generation_index: Option<u64>,
    baseline_state: NativeBaselineStateV4,
}

impl BaselineChainResumeV4 {
    pub(crate) const fn verdict(&self) -> BaselineChainResumeVerdictV4 {
        self.verdict
    }

    pub(crate) const fn generation_index(&self) -> Option<u64> {
        self.generation_index
    }

    pub(crate) fn baseline_state(&self) -> NativeBaselineStateV4 {
        self.baseline_state.clone()
    }
}

/// The pure recovery rule (contract section 5's recovery requirement,
/// exposed standalone so it can be tested without any filesystem access).
/// Both slices are the caller's own generation sequences: `chain_generations`
/// ascending as validated by the chain walk, `store_generations` ascending
/// as validated from the caller-supplied Store checkpoint list. Neither is
/// mutated or re-sorted; an unsorted or duplicate-bearing `store_generations`
/// is itself a caller error at the outer `resume_baseline_chain_v4` layer,
/// not silently repaired here.
pub(crate) fn baseline_chain_recovery_verdict_v4(
    chain_generations: &[u64],
    store_generations: &[u64],
) -> Result<BaselineChainResumeVerdictV4> {
    if store_generations.is_empty() {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::EmptyStoreCheckpoints,
        ));
    }
    if chain_generations.len() > store_generations.len() {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::ChainAheadOfStore,
        ));
    }
    if let Some(&chain_tip) = chain_generations.last() {
        // Safe: store_generations was just proven non-empty above.
        let store_tip = *store_generations
            .last()
            .expect("non-empty store generations");
        if chain_tip > store_tip {
            return Err(BaselineChainErrorV4::new(
                BaselineChainErrorKindV4::ChainAheadOfStore,
            ));
        }
    }
    if chain_generations != &store_generations[..chain_generations.len()] {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::GapOrTamper,
        ));
    }
    match store_generations.len() - chain_generations.len() {
        0 => Ok(BaselineChainResumeVerdictV4::Clean),
        1 => Ok(BaselineChainResumeVerdictV4::StoreAheadByOneBoundary),
        _ => Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::GapOrTamper,
        )),
    }
}

/// Walks every record in `dir` in generation order, verifies the hash
/// chain, decodes each embedded manifest with its generation's own core
/// hash from `store_checkpoints` (the composed-hash recomputation inside
/// `decode_checkpoint_manifest_v4` then makes tampering with any persisted
/// baseline cell, or with the declared core hash, fail closed), verifies
/// run identity throughout, and returns the newest agreeing pair plus the
/// recovery verdict. `store_checkpoints` must be strictly ascending by
/// generation with no duplicates; it is never sorted or repaired here.
pub(crate) fn resume_baseline_chain_v4(
    dir: &Path,
    expected_run_sha256: &str,
    store_checkpoints: &[(u64, [u8; 32])],
) -> Result<BaselineChainResumeV4> {
    validate_hex_digest_v4(expected_run_sha256)?;
    if store_checkpoints.is_empty() {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::EmptyStoreCheckpoints,
        ));
    }
    if store_checkpoints
        .windows(2)
        .any(|window| window[0].0 >= window[1].0)
    {
        return Err(BaselineChainErrorV4::new(
            BaselineChainErrorKindV4::UnsortedStoreCheckpoints,
        ));
    }

    let chain_generations = list_record_generations_v4(dir)?;

    // The coarse-grained relationship between the chain's and the Store's
    // generation sequences is decided BEFORE any manifest is read: this
    // guarantees that once this call proceeds past it, every generation the
    // walk below looks up is provably present in `store_checkpoints` (the
    // verdict rule requires `chain_generations` to be an exact prefix of
    // `store_generations`), and it gives `ChainAheadOfStore`/`GapOrTamper`
    // priority over a merely-incidental missing-store-checkpoint lookup
    // failure.
    let store_generations: Vec<u64> = store_checkpoints.iter().map(|&(g, _)| g).collect();
    let verdict = baseline_chain_recovery_verdict_v4(&chain_generations, &store_generations)?;

    let mut previous_record_bytes: Option<Vec<u8>> = None;
    let mut newest: Option<(u64, NativeBaselineStateV4)> = None;
    for &generation in &chain_generations {
        let record_bytes = read_record_bytes_v4(dir, generation)?;
        let record = decode_record_v4(&record_bytes)?;
        if record.store_generation_index != generation {
            return Err(BaselineChainErrorV4::new(
                BaselineChainErrorKindV4::GenerationMismatch,
            ));
        }
        if record.run_sha256 != expected_run_sha256 {
            return Err(BaselineChainErrorV4::new(
                BaselineChainErrorKindV4::RunIdentityMismatch,
            ));
        }
        match &previous_record_bytes {
            None => {
                if record.previous_record_sha256.is_some() {
                    return Err(BaselineChainErrorV4::new(
                        BaselineChainErrorKindV4::NotGenesis,
                    ));
                }
            }
            Some(previous_bytes) => {
                let expected_previous_hex = lower_hex_raw32_v1(sha256_v1(previous_bytes));
                if record.previous_record_sha256.as_deref() != Some(expected_previous_hex.as_str())
                {
                    return Err(BaselineChainErrorV4::new(
                        BaselineChainErrorKindV4::BrokenHashChain,
                    ));
                }
            }
        }

        let core_state_sha256 = store_core_hash_lookup_v4(store_checkpoints, generation)
            .ok_or_else(|| {
                BaselineChainErrorV4::new(
                    BaselineChainErrorKindV4::StoreCheckpointMissingForGeneration,
                )
            })?;
        let manifest_bytes = read_manifest_bytes_v4(dir, generation)?;
        let manifest_sha256_hex = lower_hex_raw32_v1(sha256_v1(&manifest_bytes));
        if manifest_sha256_hex != record.checkpoint_manifest_sha256 {
            return Err(BaselineChainErrorV4::new(
                BaselineChainErrorKindV4::ManifestBindingMismatch,
            ));
        }
        let manifest =
            decode_checkpoint_manifest_v4(&manifest_bytes, core_state_sha256).map_err(|error| {
                BaselineChainErrorV4::new(BaselineChainErrorKindV4::ManifestDecode(error.kind()))
            })?;
        if manifest.generation_index() != generation {
            return Err(BaselineChainErrorV4::new(
                BaselineChainErrorKindV4::GenerationMismatch,
            ));
        }
        if manifest.run_sha256() != expected_run_sha256 {
            return Err(BaselineChainErrorV4::new(
                BaselineChainErrorKindV4::RunIdentityMismatch,
            ));
        }

        newest = Some((generation, manifest.baseline_state()));
        previous_record_bytes = Some(record_bytes);
    }

    let (generation_index, baseline_state) = match newest {
        Some((generation, state)) => (Some(generation), state),
        None => (None, NativeBaselineStateV4::empty_v4()),
    };

    Ok(BaselineChainResumeV4 {
        verdict,
        generation_index,
        baseline_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_baseline_state_v4::{
        BaselineCellKeyV4, BaselineObservationV4, BaselineRoleV4,
    };
    use crate::native_training_store_checkpoint_v3::{
        CheckpointLearnerSeatCountersV3, CheckpointOutcomeCountsV3,
        CheckpointOutcomesByLearnerSeatV3, CheckpointPayloadBindingV1,
        CheckpointPayloadSectionBindingV1, CheckpointProgressV3,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIRECTORY_ORDINAL_V4: AtomicU64 = AtomicU64::new(0);

    struct TestChainDirV4 {
        path: std::path::PathBuf,
    }

    impl TestChainDirV4 {
        fn new(label: &str) -> Self {
            let ordinal = TEST_DIRECTORY_ORDINAL_V4.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mtg-kernel-baseline-chain-v4-{}-{}-{}",
                std::process::id(),
                label,
                ordinal
            ));
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestChainDirV4 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn fake_digest_v4(tag: u8) -> [u8; 32] {
        [tag; 32]
    }

    fn fake_hex_digest_v4(tag: u8) -> String {
        format!("{tag:02x}").repeat(32)
    }

    fn sample_progress_v4() -> CheckpointProgressV3 {
        let zero_outcomes = CheckpointOutcomeCountsV3 {
            win: 0,
            loss: 0,
            draw: 0,
        };
        let zero_seat = CheckpointLearnerSeatCountersV3 { p0: 0, p1: 0 };
        CheckpointProgressV3 {
            batch_episodes: 64,
            checkpoint_segment_updates: 4,
            next_episode_index: 0,
            successful_update_count: 0,
            completed_episode_count: 0,
            outcomes_by_learner_seat: CheckpointOutcomesByLearnerSeatV3 {
                p0: zero_outcomes,
                p1: zero_outcomes,
            },
            learner_policy_steps_by_seat: zero_seat,
            learner_physical_decisions_by_seat: zero_seat,
        }
    }

    fn sample_payload_v4() -> CheckpointPayloadBindingV1 {
        let section = |index: u8| CheckpointPayloadSectionBindingV1 {
            name: format!("section-{index}"),
            offset_bytes: u64::from(index) * 1000,
            byte_count: 1000,
            sha256: fake_hex_digest_v4(index),
        };
        CheckpointPayloadBindingV1 {
            schema: "test-native-train-state-payload-schema/v1".to_owned(),
            encoding: "f32le".to_owned(),
            byte_count: 3000,
            sha256: fake_hex_digest_v4(9),
            sections: [section(0), section(1), section(2)],
        }
    }

    fn sample_manifest_parts_v4(
        run_sha256: String,
        generation_index: u64,
        core_state_sha256: [u8; 32],
        baseline: NativeBaselineStateV4,
    ) -> CheckpointManifestPartsV4 {
        CheckpointManifestPartsV4 {
            run_sha256,
            identity_bundle_sha256: fake_hex_digest_v4(11),
            segment_ordinal: 0,
            generation_index,
            batch_episodes: 64,
            checkpoint_segment_updates: 4,
            progress: sample_progress_v4(),
            adam_step: generation_index,
            scorer_bias_anchor_f32_bits: 0,
            parameter_layout_sha256: fake_hex_digest_v4(12),
            parameter_tensor_count: 33,
            parameter_element_count: 1_230_994,
            model_parameter_sha256: [0x42; 32],
            core_state_sha256,
            payload: sample_payload_v4(),
            baseline,
        }
    }

    fn baseline_at_boundary_v4(
        previous: &NativeBaselineStateV4,
        tag: u8,
        residual: f64,
    ) -> NativeBaselineStateV4 {
        previous
            .apply_update_v4(&[BaselineObservationV4 {
                key: BaselineCellKeyV4::new_v4(fake_hex_digest_v4(tag), BaselineRoleV4::P0)
                    .expect("key"),
                residual_sum_f64: residual,
                decision_count: 40,
                episode_count: 20,
            }])
            .expect("apply update")
    }

    fn run_sha256_v4() -> String {
        fake_hex_digest_v4(0x10)
    }

    #[test]
    fn publish_and_resume_round_trip_across_three_boundaries_v4() {
        let dir = TestChainDirV4::new("round-trip");
        let run_sha256 = run_sha256_v4();

        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0xa0);
        let sha_0 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        let baseline_4 = baseline_at_boundary_v4(&baseline_0, 1, 20.0);
        let core_4 = fake_digest_v4(0xa4);
        let sha_4 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_0),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 4, core_4, baseline_4.clone()),
        )
        .expect("publish gen 4");

        let baseline_8 = baseline_at_boundary_v4(&baseline_4, 2, -12.0);
        let core_8 = fake_digest_v4(0xa8);
        let _sha_8 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_4),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 8, core_8, baseline_8.clone()),
        )
        .expect("publish gen 8");

        let store_checkpoints = [(0, core_0), (4, core_4), (8, core_8)];
        let resumed = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect("resume clean");
        assert_eq!(resumed.verdict(), BaselineChainResumeVerdictV4::Clean);
        assert_eq!(resumed.generation_index(), Some(8));
        assert_eq!(resumed.baseline_state(), baseline_8);
    }

    #[test]
    fn store_ahead_by_one_boundary_resumes_at_chain_tip_v4() {
        let dir = TestChainDirV4::new("store-ahead-one");
        let run_sha256 = run_sha256_v4();

        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0xb0);
        let sha_0 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        let baseline_4 = baseline_at_boundary_v4(&baseline_0, 1, 20.0);
        let core_4 = fake_digest_v4(0xb4);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_0),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 4, core_4, baseline_4.clone()),
        )
        .expect("publish gen 4");

        // The Store already committed checkpoint 8, but the launcher crashed
        // before this module's paired baseline record for generation 8 was
        // published: no baseline-00000008.* files exist in `dir`.
        let core_8 = fake_digest_v4(0xb8);
        let store_checkpoints = [(0, core_0), (4, core_4), (8, core_8)];
        let resumed = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect("resume store-ahead-by-one");
        assert_eq!(
            resumed.verdict(),
            BaselineChainResumeVerdictV4::StoreAheadByOneBoundary
        );
        assert_eq!(resumed.generation_index(), Some(4));
        assert_eq!(resumed.baseline_state(), baseline_4);
    }

    #[test]
    fn pre_genesis_crash_resumes_empty_baseline_v4() {
        // No baseline record has ever been published (the directory is not
        // even created yet), but the Store already has its genesis
        // checkpoint. The general recovery rule folds this into
        // store-ahead-by-one-boundary with an empty fallback baseline.
        let dir = TestChainDirV4::new("pre-genesis");
        let run_sha256 = run_sha256_v4();
        let core_0 = fake_digest_v4(0xc0);
        let store_checkpoints = [(0, core_0)];
        let resumed = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect("resume pre-genesis");
        assert_eq!(
            resumed.verdict(),
            BaselineChainResumeVerdictV4::StoreAheadByOneBoundary
        );
        assert_eq!(resumed.generation_index(), None);
        assert_eq!(resumed.baseline_state(), NativeBaselineStateV4::empty_v4());
    }

    #[test]
    fn chain_ahead_of_store_fails_v4() {
        let dir = TestChainDirV4::new("chain-ahead");
        let run_sha256 = run_sha256_v4();

        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0xd0);
        let sha_0 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        let baseline_4 = baseline_at_boundary_v4(&baseline_0, 1, 20.0);
        let core_4 = fake_digest_v4(0xd4);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_0),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 4, core_4, baseline_4),
        )
        .expect("publish gen 4");

        // The Store's own view has not reached generation 4 at all.
        let store_checkpoints = [(0, core_0)];
        let error = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect_err("chain ahead must fail closed");
        assert_eq!(error.kind(), BaselineChainErrorKindV4::ChainAheadOfStore);
    }

    #[test]
    fn tampered_middle_record_manifest_binding_fails_v4() {
        let dir = TestChainDirV4::new("tampered-binding");
        let run_sha256 = run_sha256_v4();

        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0xe0);
        let sha_0 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        let baseline_4 = baseline_at_boundary_v4(&baseline_0, 1, 20.0);
        let core_4 = fake_digest_v4(0xe4);
        let sha_4 = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_0),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 4, core_4, baseline_4.clone()),
        )
        .expect("publish gen 4 (middle record)");

        let baseline_8 = baseline_at_boundary_v4(&baseline_4, 2, -12.0);
        let core_8 = fake_digest_v4(0xe8);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_4),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 8, core_8, baseline_8),
        )
        .expect("publish gen 8");

        // Corrupt the middle record's declared manifest binding directly on
        // disk (simulating post-publication tampering), leaving the hash
        // chain and canonical form otherwise intact.
        let record_bytes = read_record_bytes_v4(dir.path(), 4).expect("read middle record");
        let mut wire = decode_record_v4(&record_bytes).expect("decode middle record");
        wire.checkpoint_manifest_sha256 = fake_hex_digest_v4(0xff);
        let tampered = encode_record_v4(&wire).expect("re-encode tampered record");
        fs::write(dir.path().join("baseline-00000004.record.json"), tampered)
            .expect("overwrite middle record");

        let store_checkpoints = [(0, core_0), (4, core_4), (8, core_8)];
        let error = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect_err("tampered middle record must fail closed");
        assert_eq!(
            error.kind(),
            BaselineChainErrorKindV4::ManifestBindingMismatch
        );
    }

    #[test]
    fn tampered_previous_record_hash_breaks_the_chain_v4() {
        let dir = TestChainDirV4::new("tampered-chain");
        let run_sha256 = run_sha256_v4();

        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0xf0);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        let baseline_4 = baseline_at_boundary_v4(&baseline_0, 1, 20.0);
        let core_4 = fake_digest_v4(0xf4);
        // Deliberately supply a wrong "expected previous" is rejected by the
        // publisher itself (a separate append-only guard); to exercise the
        // resume-side hash-chain check, tamper the on-disk bytes directly
        // instead, after a legitimate publish.
        let sha_0_actual = sha256_v1(&read_record_bytes_v4(dir.path(), 0).expect("read genesis"));
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(sha_0_actual),
            },
            sample_manifest_parts_v4(run_sha256.clone(), 4, core_4, baseline_4),
        )
        .expect("publish gen 4");

        let record_bytes = read_record_bytes_v4(dir.path(), 4).expect("read gen 4 record");
        let mut wire = decode_record_v4(&record_bytes).expect("decode gen 4 record");
        wire.previous_record_sha256 = Some(fake_hex_digest_v4(0x99));
        let tampered = encode_record_v4(&wire).expect("re-encode tampered record");
        fs::write(dir.path().join("baseline-00000004.record.json"), tampered)
            .expect("overwrite gen 4 record");

        let store_checkpoints = [(0, core_0), (4, core_4)];
        let error = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect_err("broken hash chain must fail closed");
        assert_eq!(error.kind(), BaselineChainErrorKindV4::BrokenHashChain);
    }

    #[test]
    fn wrong_core_hash_fails_composed_recomputation_v4() {
        let dir = TestChainDirV4::new("wrong-core-hash");
        let run_sha256 = run_sha256_v4();
        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0x11);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0),
        )
        .expect("publish genesis");

        // Resume with a Store view whose core hash for generation 0 differs
        // from what was published: the composed-hash recomputation inside
        // `decode_checkpoint_manifest_v4` must catch it.
        let wrong_core_0 = fake_digest_v4(0x12);
        let store_checkpoints = [(0, wrong_core_0)];
        let error = resume_baseline_chain_v4(dir.path(), &run_sha256, &store_checkpoints)
            .expect_err("wrong core hash must fail closed");
        assert_eq!(
            error.kind(),
            BaselineChainErrorKindV4::ManifestDecode(
                CheckpointManifestV4ErrorKind::ComposedStateDigestMismatch
            )
        );
    }

    #[test]
    fn append_only_violation_fails_v4() {
        let dir = TestChainDirV4::new("append-only");
        let run_sha256 = run_sha256_v4();
        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0x21);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        // Re-publishing at the same (already-taken) generation fails closed,
        // whether or not the caller believed it had the right parent.
        let error = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0),
        )
        .expect_err("re-publishing generation 0 must fail closed");
        assert_eq!(
            error.kind(),
            BaselineChainErrorKindV4::NonMonotonicGeneration
        );
    }

    #[test]
    fn stale_previous_record_assertion_fails_v4() {
        let dir = TestChainDirV4::new("stale-previous");
        let run_sha256 = run_sha256_v4();
        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0x31);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish genesis");

        let baseline_4 = baseline_at_boundary_v4(&baseline_0, 1, 5.0);
        let core_4 = fake_digest_v4(0x34);
        let error = publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4 {
                expected_previous_record_sha256: Some(fake_digest_v4(0xaa)),
            },
            sample_manifest_parts_v4(run_sha256, 4, core_4, baseline_4),
        )
        .expect_err("stale expected-previous must fail closed");
        assert_eq!(error.kind(), BaselineChainErrorKindV4::StalePreviousRecord);
    }

    #[test]
    fn run_identity_mismatch_fails_v4() {
        let dir = TestChainDirV4::new("run-identity");
        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0x41);
        publish_baseline_record_v4(
            dir.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256_v4(), 0, core_0, baseline_0),
        )
        .expect("publish genesis");

        let different_run = fake_hex_digest_v4(0x77);
        let store_checkpoints = [(0, core_0)];
        let error = resume_baseline_chain_v4(dir.path(), &different_run, &store_checkpoints)
            .expect_err("run identity mismatch must fail closed");
        assert_eq!(error.kind(), BaselineChainErrorKindV4::RunIdentityMismatch);
    }

    #[test]
    fn record_bytes_are_deterministic_v4() {
        let dir_a = TestChainDirV4::new("determinism-a");
        let dir_b = TestChainDirV4::new("determinism-b");
        let run_sha256 = run_sha256_v4();
        let baseline_0 = NativeBaselineStateV4::empty_v4();
        let core_0 = fake_digest_v4(0x51);

        let sha_a = publish_baseline_record_v4(
            dir_a.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256.clone(), 0, core_0, baseline_0.clone()),
        )
        .expect("publish a");
        let sha_b = publish_baseline_record_v4(
            dir_b.path(),
            BaselineChainRecordPartsV4::default(),
            sample_manifest_parts_v4(run_sha256, 0, core_0, baseline_0),
        )
        .expect("publish b");

        assert_eq!(sha_a, sha_b);
        let bytes_a = read_record_bytes_v4(dir_a.path(), 0).expect("read a");
        let bytes_b = read_record_bytes_v4(dir_b.path(), 0).expect("read b");
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn recovery_rule_matrix_v4() {
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0, 4, 8], &[0, 4, 8]).unwrap(),
            BaselineChainResumeVerdictV4::Clean
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0, 4], &[0, 4, 8]).unwrap(),
            BaselineChainResumeVerdictV4::StoreAheadByOneBoundary
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[], &[0]).unwrap(),
            BaselineChainResumeVerdictV4::StoreAheadByOneBoundary
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0], &[0, 4, 8])
                .unwrap_err()
                .kind(),
            BaselineChainErrorKindV4::GapOrTamper
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0, 4, 8, 12], &[0, 4, 8])
                .unwrap_err()
                .kind(),
            BaselineChainErrorKindV4::ChainAheadOfStore
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0, 8], &[0, 4])
                .unwrap_err()
                .kind(),
            BaselineChainErrorKindV4::ChainAheadOfStore
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0, 8], &[0, 4, 8])
                .unwrap_err()
                .kind(),
            BaselineChainErrorKindV4::GapOrTamper
        );
        assert_eq!(
            baseline_chain_recovery_verdict_v4(&[0], &[])
                .unwrap_err()
                .kind(),
            BaselineChainErrorKindV4::EmptyStoreCheckpoints
        );
    }

    #[test]
    fn record_schema_constant_is_stable_v4() {
        assert_eq!(
            BASELINE_CHAIN_RECORD_SCHEMA_V4,
            "mtg-kernel-baseline-chain-record/v1"
        );
    }
}
