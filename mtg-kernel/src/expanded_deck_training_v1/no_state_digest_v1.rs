//! The no-state-digest input contract (throughput lane design,
//! collab `LANES/throughput-architecture-20261020/DESIGN.md`).
//!
//! A converted model is the original checkpoint without the 96 state-digest
//! inputs (SHA-512 of the canonical state JSON). Its state encoder is
//! `W_new = concat(W_old[:, ..123], W_old[:, 219..])`; every other parameter
//! is unchanged. The runtime keeps the 1,499-wide layout with those 96
//! columns at zero and gives zero digest features, which computes the same
//! function, so the tensorizer no longer hashes the state.
//!
//! A converted source keeps the ordinary source shape: its `play_import` is a
//! [`NoStateDigestImportV1`] naming the original import and a pinned
//! [`NoStateDigestConversionV1`]. `MTG_KERNEL_NO_STATE_DIGEST_CHECK=1` keeps
//! the unconverted model and compares every forward with it, bit for bit.
//!
//! Training (lane `no-digest-training-20261023`, DESIGN.md v2) keeps the
//! padded layout: the digest columns of `state_encoder.0.weight` and of both
//! Adam moments stay +0.0, and every scored row's digest inputs are +0.0
//! bits. Backward then gives those columns signed-zero terms that leave +0.0
//! accumulators at +0.0, Adam's exact-zero shortcut keeps W, m and v there,
//! and the kept parameters follow the 1,403-wide model's arithmetic. Every
//! invariant is checked and refused, never repaired. One admission rule,
//! [`initialize_v1`], serves inference, opponents, training, the update's
//! readback and the driver: at run start it converts the base checkpoint's
//! full optimizer state, and a continuation admits only a checkpoint of the
//! same conversion. Only an ordinary import or a fresh-initialization source
//! trains; a converted BO3, registry-evolution, registry-transfer or
//! terminal-teacher source keeps the inference-only install ([`load_v1`]).
//! Records name the contract in an `input_contract` member
//! ([`InputContractRecordV1`]), absent from every unconverted record.

use super::{
    ensure, err, hex, identity_valid, initialize_from_source_bytes_v1,
    load_expanded_inference_any_math_v1, ordinary_checkpoint_schema_v1, publish_json_bytes_v1,
    read_ordinary_checkpoint_v1, read_pinned, read_pinned_bytes, restore_checkpoint_state_v1, sha,
    CheckpointInputExpectationV1, DecisionRecordV1, ExpandedCheckpointV1,
    ExpandedInferenceIdentityV1, ExpandedModelSourceV1, ParameterBitsV1, PinnedFileV1,
};
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::native_flat_tensorizer_v3::{FEATURE_CONTRACT_DIGEST_V3, FEATURE_ENCODING_DIGEST_V3};
use crate::native_flat_tensorizer_v4::{FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4};
use crate::native_policy_train_step_v1::{
    columns_are_positive_zero_v1, NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1,
};
use crate::sideboard_play_policy_v1::{
    no_negative_zero_v1, FreshLineageGenerationV1, FrozenPlayPolicyV1,
};
use crate::training_math_v1::TrainingMathV1;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub const IMPORT_SCHEMA_V1: &str = "mtg-kernel-no-state-digest-import/v1";
pub const CONVERSION_SCHEMA_V1: &str = "mtg-kernel-no-state-digest-conversion/v1";
pub const CONTRACT_V1: &str = "no-state-digest-v1";
/// Input columns of `state_encoder.0.weight` that read the state digest:
/// after the 123 explicit state features, before the 1,280 pooled objects.
pub const STATE_DIGEST_COLUMNS_V1: std::ops::Range<usize> = 123..219;
/// The original state encoder's input width.
pub const STATE_ENCODER_INPUT_V1: usize = 1_499;
const STATE_ENCODER_WEIGHT: &str = "state_encoder.0.weight";
const STATE_ENCODER_BIAS: &str = "state_encoder.0.bias";

/// A converted model's `play_import`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoStateDigestImportV1 {
    pub schema: String,
    pub base_play_import: PinnedFileV1,
    pub conversion: PinnedFileV1,
}

/// One side of a conversion: the model's parameter manifest and contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoStateDigestModelV1 {
    pub model_parameter_sha256: String,
    pub feature_contract_digest: String,
}

/// What a conversion did, pinned by the converted import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoStateDigestConversionV1 {
    pub schema: String,
    pub contract: String,
    /// `[start, end)` of the removed `state_encoder.0.weight` input columns.
    pub state_digest_columns: [usize; 2],
    pub base_play_import: PinnedFileV1,
    pub base_checkpoint: Option<PinnedFileV1>,
    pub base: NoStateDigestModelV1,
    /// The runtime model: the base with the digest columns at zero.
    pub converted: NoStateDigestModelV1,
    /// The base checkpoint in the 1,403-wide layout (parameters and both
    /// Adam moments with the columns removed), for later training work.
    /// Inference does not read it.
    pub converted_checkpoint: Option<PinnedFileV1>,
}

/// The converted model's feature contract digest.
pub fn converted_contract_digest_v1(base: &str) -> String {
    sha(format!("{CONTRACT_V1}|{base}").as_bytes())
}

pub(super) fn check_enabled_v1() -> bool {
    #[cfg(test)]
    if let Some(check) = CHECK_OVERRIDE_FOR_TEST_V1.with(std::cell::Cell::get) {
        return check;
    }
    std::env::var("MTG_KERNEL_NO_STATE_DIGEST_CHECK").is_ok_and(|value| value == "1")
}

#[cfg(test)]
thread_local! {
    /// Test-only: this thread's loads use this per-forward check setting
    /// instead of the process environment, which other tests change.
    static CHECK_OVERRIDE_FOR_TEST_V1: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Test-only: runs `body` with this thread's per-forward check set to `check`.
#[cfg(test)]
pub(super) fn with_check_for_test_v1<R>(check: bool, body: impl FnOnce() -> R) -> R {
    let previous = CHECK_OVERRIDE_FOR_TEST_V1.with(|cell| cell.replace(Some(check)));
    let result = body();
    CHECK_OVERRIDE_FOR_TEST_V1.with(|cell| cell.set(previous));
    result
}

fn validate_conversion_v1(conversion: &NoStateDigestConversionV1) -> Result<(), String> {
    ensure(
        conversion.schema == CONVERSION_SCHEMA_V1
            && conversion.contract == CONTRACT_V1
            && conversion.state_digest_columns
                == [STATE_DIGEST_COLUMNS_V1.start, STATE_DIGEST_COLUMNS_V1.end]
            && conversion.converted.feature_contract_digest
                == converted_contract_digest_v1(&conversion.base.feature_contract_digest),
        "unsupported no-state-digest conversion",
    )
}

/// Installs the contract on a loaded base policy and checks the result.
fn install_v1(
    policy: &mut FrozenPlayPolicyV1,
    conversion: &NoStateDigestConversionV1,
    check: bool,
) -> Result<(), String> {
    ensure(
        policy.input_contract_v1().is_none(),
        "a converted model cannot be converted again",
    )?;
    let base = policy.actual_model_identity_v1();
    ensure(
        base.model_parameter_sha256 == conversion.base.model_parameter_sha256
            && base.feature_contract_digest == conversion.base.feature_contract_digest,
        "the base model differs from the conversion record",
    )?;
    policy.apply_no_state_digest_v1(
        conversion.converted.feature_contract_digest.clone(),
        STATE_DIGEST_COLUMNS_V1,
        STATE_ENCODER_INPUT_V1,
        check,
    )?;
    ensure(
        policy.actual_model_identity_v1().model_parameter_sha256
            == conversion.converted.model_parameter_sha256,
        "the converted parameters differ from the conversion record",
    )
}

/// The `input_contract` member of a converted lineage's training records
/// (trajectories, collection and update receipts, checkpoints, `run.json`).
/// Absent from every unconverted record, which keeps its bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputContractRecordV1 {
    pub contract: String,
    /// `[start, end)` of the zeroed `state_encoder.0.weight` input columns.
    pub state_digest_columns: [usize; 2],
    /// The conversion record the lineage started from. Two conversions of
    /// one import share the converted digest; this pin tells them apart.
    pub conversion: PinnedFileV1,
}

impl InputContractRecordV1 {
    fn of_conversion_v1(conversion: PinnedFileV1) -> Self {
        Self {
            contract: CONTRACT_V1.into(),
            state_digest_columns: [STATE_DIGEST_COLUMNS_V1.start, STATE_DIGEST_COLUMNS_V1.end],
            conversion,
        }
    }
}

/// Absence keeps the original records' wire shape; an explicitly present
/// `null` is not absence and stays invalid, as the strict readers refused
/// the then unknown member before the contract existed.
pub(super) fn deserialize_present_input_contract_v1<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<InputContractRecordV1>, D::Error> {
    InputContractRecordV1::deserialize(deserializer).map(Some)
}

/// The contract a validated record or model runs under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EffectiveContractV1 {
    None,
    NoStateDigest {
        converted_digest: String,
        conversion: PinnedFileV1,
    },
}

/// A validated feature identity: the base generation and its digests, which
/// dispatch replay, and the contract its inputs follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedContractV1 {
    pub(crate) generation: FreshLineageGenerationV1,
    pub(crate) base_contract_digest: &'static str,
    pub(crate) base_encoding_digest: &'static str,
    pub(crate) effective: EffectiveContractV1,
}

impl ResolvedContractV1 {
    /// The converted digest under the contract, None otherwise.
    pub(crate) fn input_contract_digest_v1(&self) -> Option<&str> {
        match &self.effective {
            EffectiveContractV1::None => None,
            EffectiveContractV1::NoStateDigest {
                converted_digest, ..
            } => Some(converted_digest),
        }
    }

    fn conversion_v1(&self) -> Option<&PinnedFileV1> {
        match &self.effective {
            EffectiveContractV1::None => None,
            EffectiveContractV1::NoStateDigest { conversion, .. } => Some(conversion),
        }
    }

    /// Requires the record's contract to be exactly `policy`'s: the same
    /// converted digest and conversion, or neither.
    pub(crate) fn require_policy_contract_v1(
        &self,
        policy: &FrozenPlayPolicyV1,
    ) -> Result<(), String> {
        ensure(
            self.input_contract_digest_v1() == policy.input_contract_v1()
                && self.conversion_v1() == policy.input_contract_conversion_v1(),
            "record and model no-state-digest contracts differ",
        )
    }
}

/// The one resolver of a stored `(contract, encoding, cards)` identity and
/// its `input_contract` member. Without the member it admits exactly the V3
/// and V4 pairs, as [`identity_valid`] does; with it, exactly a converted
/// digest `converted_contract_digest_v1(base)` beside its own base
/// generation's encoding digest. The marker and the digest are checked
/// together, so neither passes without the other.
pub(crate) fn resolve_contract_v1(
    contract: &str,
    encoding: &str,
    cards: &str,
    input_contract: Option<&InputContractRecordV1>,
) -> Result<ResolvedContractV1, String> {
    let generations = [
        (
            FreshLineageGenerationV1::V3,
            FEATURE_CONTRACT_DIGEST_V3,
            FEATURE_ENCODING_DIGEST_V3,
        ),
        (
            FreshLineageGenerationV1::V4,
            FEATURE_CONTRACT_DIGEST_V4,
            FEATURE_ENCODING_DIGEST_V4,
        ),
    ];
    let Some(record) = input_contract else {
        identity_valid(contract, encoding, cards)?;
        let (generation, base_contract_digest, base_encoding_digest) = generations
            .into_iter()
            .find(|(_, base, _)| *base == contract)
            .ok_or("successor feature identity differs")?;
        return Ok(ResolvedContractV1 {
            generation,
            base_contract_digest,
            base_encoding_digest,
            effective: EffectiveContractV1::None,
        });
    };
    ensure(
        record.contract == CONTRACT_V1
            && record.state_digest_columns
                == [STATE_DIGEST_COLUMNS_V1.start, STATE_DIGEST_COLUMNS_V1.end],
        "unsupported no-state-digest input contract",
    )?;
    let (generation, base_contract_digest, base_encoding_digest) = generations
        .into_iter()
        .find(|(_, base, base_encoding)| {
            *base_encoding == encoding && converted_contract_digest_v1(base) == contract
        })
        .ok_or("successor feature identity differs from its no-state-digest contract")?;
    ensure(
        cards == format!("{KERNEL_CARDDB_HASH:016x}"),
        "successor card database differs",
    )?;
    Ok(ResolvedContractV1 {
        generation,
        base_contract_digest,
        base_encoding_digest,
        effective: EffectiveContractV1::NoStateDigest {
            converted_digest: contract.to_owned(),
            conversion: record.conversion.clone(),
        },
    })
}

/// Every stored row of a trajectory under the contract, opponent rows
/// included, holds +0.0 (bits 0; -0.0 is refused too) in its state-digest
/// inputs. With zero weight columns a nonzero input would still reproduce
/// the recorded forward, but not the weight gradient's zero.
pub(super) fn check_trajectory_rows_v1(decisions: &[DecisionRecordV1]) -> Result<(), String> {
    ensure(
        decisions.iter().all(|row| {
            row.tensor
                .state
                .get(STATE_DIGEST_COLUMNS_V1)
                .is_some_and(|digest| digest.iter().all(|bits| *bits == 0))
        }),
        "no-state-digest contract: a stored state-digest input is not +0.0",
    )
}

/// The `input_contract` member `policy`'s training records carry: None for
/// the original inputs. A converted model without a bound conversion (an
/// in-memory conversion) writes no training records.
pub(crate) fn policy_contract_record_v1(
    policy: &FrozenPlayPolicyV1,
) -> Result<Option<InputContractRecordV1>, String> {
    if policy.input_contract_v1().is_none() {
        return Ok(None);
    }
    let conversion = policy
        .input_contract_conversion_v1()
        .ok_or("a converted model without a pinned conversion writes no training records")?;
    Ok(Some(InputContractRecordV1::of_conversion_v1(
        conversion.clone(),
    )))
}

/// Whether `source`'s `play_import` is a [`NoStateDigestImportV1`]. A read
/// or parse failure counts as no: the loader reports it as before.
pub(crate) fn is_converted_source_v1(source: &ExpandedModelSourceV1) -> bool {
    fs::read(&source.play_import.path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|probe| probe.get("schema").and_then(Value::as_str) == Some(IMPORT_SCHEMA_V1))
}

/// The feature contract digest `policy`'s records carry: the converted
/// digest under the contract, its base generation's otherwise.
pub(crate) fn effective_contract_digest_v1(policy: &FrozenPlayPolicyV1) -> String {
    policy.input_contract_v1().map_or_else(
        || {
            policy
                .feature_identity_v1()
                .feature_contract_digest
                .to_owned()
        },
        str::to_owned,
    )
}

/// The inference branch for a [`NoStateDigestImportV1`] `play_import`. A
/// base that trains (`base_trains_v1`) loads through the one admission rule
/// of [`initialize_v1`], whose receipt describes the converted optimizer
/// state. Any other base (BO3, registry-evolution, registry-transfer or
/// terminal-teacher inference sources) keeps the inference-only install
/// that predates training: the base loads through its own inference loader
/// at the conversion's base checkpoint, the contract is installed on the
/// policy alone, and no train state or training record exists for it.
/// Never inlined: its policy locals must not enlarge the ordinary loader's
/// frame, which several binaries still reach on a 1 MiB main thread.
#[inline(never)]
pub(super) fn load_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let (import, conversion) = read_import_v1(bytes)?;
    if base_trains_v1(&read_pinned_bytes(&import.base_play_import)?)? {
        // The loaded-source cache serves it exactly as it serves training
        // loads.
        let (policy, state, _) =
            match super::loaded_source_cache_v1::lookup_v1(source, bytes, None)? {
                Some((policy, state)) => (policy, state, None),
                None => initialize_training_v1(source, bytes, None)?,
            };
        let identity = super::inference_identity_v1(source, &policy, &state)?;
        return Ok((policy, identity));
    }
    ensure(
        conversion.base_checkpoint == source.checkpoint,
        "the conversion record names another base model",
    )?;
    let base = ExpandedModelSourceV1 {
        play_import: import.base_play_import,
        feature_transfer: source.feature_transfer.clone(),
        checkpoint: source.checkpoint.clone(),
    };
    let (mut policy, mut identity) = load_expanded_inference_any_math_v1(&base)?;
    install_v1(&mut policy, &conversion, check_enabled_v1())?;
    identity.model = policy.actual_model_identity_v1();
    Ok((policy, identity))
}

/// A converted import and its validated conversion record, which must name
/// the same base import.
fn read_import_v1(
    bytes: &[u8],
) -> Result<(NoStateDigestImportV1, NoStateDigestConversionV1), String> {
    let import: NoStateDigestImportV1 = serde_json::from_slice(bytes).map_err(err)?;
    ensure(
        import.schema == IMPORT_SCHEMA_V1,
        "unsupported no-state-digest import",
    )?;
    let conversion: NoStateDigestConversionV1 = read_pinned(&import.conversion)?;
    validate_conversion_v1(&conversion)?;
    ensure(
        conversion.base_play_import == import.base_play_import,
        "the conversion record names another base model",
    )?;
    Ok((import, conversion))
}

/// Whether a base import trains under the contract: an ordinary import (no
/// schema) or a fresh-initialization source. Every other base is
/// inference-only.
fn base_trains_v1(base_bytes: &[u8]) -> Result<bool, String> {
    let probe: Value = serde_json::from_slice(base_bytes).map_err(err)?;
    Ok(match probe.get("schema") {
        None => true,
        Some(schema) => schema.as_str() == Some(super::fresh_initialization_source::SOURCE_SCHEMA),
    })
}

/// [`initialize_v1`] in the shape of the ordinary training loader, which
/// returns it directly: no registry-transfer context.
#[inline(never)]
pub(super) fn initialize_training_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<
    (
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        Option<super::TransferContextV1>,
    ),
    String,
> {
    let (policy, state, load) = initialize_v1(source, bytes, inputs)?;
    super::loaded_source_cache_v1::remember_converted_v1(source, bytes, &policy, &state, &load);
    Ok((policy, state, None))
}

/// What a converted full load restored, for the loaded-source cache: a run
/// start, a continuation with its checkpoint's recorded update inputs, or a
/// load the cache does not serve (its base or continuation checkpoint names
/// another file). The base bytes and whether the policy holds the
/// per-forward check model come with it.
pub(super) struct ConvertedLoadV1 {
    pub(super) kind: ConvertedLoadKindV1,
    pub(super) base_bytes: Vec<u8>,
    pub(super) check: bool,
}

pub(super) enum ConvertedLoadKindV1 {
    RunStart,
    Continuation(super::loaded_source_cache_v1::RecordedInputsV1),
    NotCached,
}

/// The reads of a full converted load of `source`, in its order and with
/// its errors, for a loaded-source cache hit: the import's conversion
/// record, the base import, the base's own fresh-initialization inputs and
/// checkpoint (as the base's cache hit reads them), then the 1,403-wide
/// artifact at run start or the continuation checkpoint. Every semantic
/// check of a full load is a pure function of these verified bytes.
pub(super) fn reverify_converted_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<(), String> {
    let (import, conversion) = read_import_v1(bytes)?;
    // Base files that still hold the bytes a full load verified pass every
    // base read below; otherwise those reads run and report as before.
    if base_cache_v1::lookup_v1(source, bytes).is_none() {
        let base_bytes = read_pinned_bytes(&import.base_play_import)?;
        ensure(
            base_trains_v1(&base_bytes)?,
            "this no-state-digest source's base is inference-only and does not train",
        )?;
        let descriptor = super::fresh_initialization_source::parse_source_v1(&base_bytes)?;
        super::fresh_initialization_source::reverify_input_files_v1(&descriptor)?;
        if let Some(pin) = &conversion.base_checkpoint {
            read_pinned_bytes(pin)?;
        }
    }
    if source.checkpoint == conversion.base_checkpoint {
        if let Some(pin) = &conversion.converted_checkpoint {
            read_pinned_bytes(pin)?;
        }
    } else {
        read_pinned_bytes(
            source
                .checkpoint
                .as_ref()
                .ok_or("a no-state-digest continuation requires a checkpoint")?,
        )?;
    }
    Ok(())
}

/// The collection boundary's byte check of a converted opponent before the
/// process opponent cache may serve it (`verify_collection_opponent_sources_v1`):
/// the conversion record, the base import and its own pins, the base
/// checkpoint and, at run start, the 1,403-wide artifact. `false` for an
/// inference-only base, which keeps its full load.
pub(super) fn verify_source_pins_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<bool, String> {
    let (import, conversion) = read_import_v1(bytes)?;
    // A verified base (a trainable fresh base without an archive parent)
    // whose files still hold their verified bytes passes the base checks.
    if base_cache_v1::lookup_v1(source, bytes).is_none() {
        let base_bytes = read_pinned_bytes(&import.base_play_import)?;
        if !base_trains_v1(&base_bytes)? {
            return Ok(false);
        }
        if !super::verify_import_pins_v1(&base_bytes)? {
            return Ok(false);
        }
        // A base or continuation checkpoint that names a further file (a
        // phase-1 archive-distillation parent) keeps its full load, which
        // reads that file; the cache would not.
        if let Some(pin) = &conversion.base_checkpoint {
            if names_another_file_v1(&read_pinned_bytes(pin)?)? {
                return Ok(false);
            }
        }
    }
    if source.checkpoint == conversion.base_checkpoint {
        if let Some(pin) = &conversion.converted_checkpoint {
            read_pinned_bytes(pin)?;
        }
    } else if let Some(pin) = &source.checkpoint {
        if names_another_file_v1(&read_pinned_bytes(pin)?)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether checkpoint bytes name a further file a full load reads: a
/// phase-1 archive-distillation parent. Neither cache serves such a
/// converted source, so it always loads in full.
fn names_another_file_v1(checkpoint: &[u8]) -> Result<bool, String> {
    #[derive(Deserialize)]
    struct ArchiveProbeV1 {
        #[serde(default, deserialize_with = "super::present_value_v1")]
        archive_distillation: Option<Value>,
    }
    let probe: ArchiveProbeV1 = serde_json::from_slice(checkpoint).map_err(err)?;
    Ok(probe.archive_distillation.is_some())
}

/// The training loader of a [`NoStateDigestImportV1`] source, and its one
/// admission rule. The base import loads through the ordinary path with the
/// conversion's pinned base checkpoint and full optimizer state. A source
/// whose checkpoint is that base checkpoint starts a run: the base state is
/// converted ([`convert_state_v1`]). Any other checkpoint must continue this
/// exact conversion ([`continue_state_v1`]). Either way the returned policy
/// and state carry the contract, the policy names the conversion, and the
/// returned [`ConvertedLoadV1`] says what was restored, for the cache.
#[inline(never)]
fn initialize_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
) -> Result<
    (
        FrozenPlayPolicyV1,
        NativePolicyValueTrainStateV1,
        ConvertedLoadV1,
    ),
    String,
> {
    let (import, conversion) = read_import_v1(bytes)?;
    let run_start = source.checkpoint == conversion.base_checkpoint;
    // A continuation over a verified base whose files still hold their
    // verified bytes starts from its installed base policy: the base load,
    // contract installation and archive probe are pure functions of those
    // bytes, and every continuation check below runs as in a full load.
    if !run_start {
        if let Some(base) = base_cache_v1::lookup_v1(source, bytes) {
            let mut policy = base.template_v1()?;
            let (mut state, kind, _) = continue_state_v1(
                source,
                &mut policy,
                &conversion,
                &import.conversion,
                inputs,
                None,
            )?;
            state
                .install_no_state_digest_v1(STATE_DIGEST_COLUMNS_V1)
                .map_err(err)?;
            let check = policy.no_state_digest_check_v1();
            return Ok((
                policy,
                state,
                ConvertedLoadV1 {
                    kind,
                    base_bytes: base.base_bytes_v1().to_vec(),
                    check,
                },
            ));
        }
    }
    let base = ExpandedModelSourceV1 {
        play_import: import.base_play_import.clone(),
        feature_transfer: source.feature_transfer.clone(),
        checkpoint: conversion.base_checkpoint.clone(),
    };
    let base_bytes = read_pinned_bytes(&base.play_import)?;
    ensure(
        base_trains_v1(&base_bytes)?,
        "this no-state-digest source's base is inference-only and does not train",
    )?;
    let (mut policy, base_state, transfer) =
        initialize_from_source_bytes_v1(&base, &base_bytes, inputs.filter(|_| run_start))?;
    ensure(
        transfer.is_none(),
        "registry-transfer sources do not train under the no-state-digest contract",
    )?;
    let check = check_enabled_v1();
    let (mut state, mut kind, template) = if run_start {
        (
            convert_state_v1(&mut policy, base_state, &conversion, check)?,
            ConvertedLoadKindV1::RunStart,
            None,
        )
    } else {
        continue_state_v1(
            source,
            &mut policy,
            &conversion,
            &import.conversion,
            inputs,
            Some(check),
        )?
    };
    if run_start {
        policy.bind_input_contract_conversion_v1(import.conversion.clone())?;
    }
    state
        .install_no_state_digest_v1(STATE_DIGEST_COLUMNS_V1)
        .map_err(err)?;
    // A base checkpoint that names a further file keeps every load full.
    if let Some(pin) = &conversion.base_checkpoint {
        if names_another_file_v1(&read_pinned_bytes(pin)?)? {
            kind = ConvertedLoadKindV1::NotCached;
        }
    }
    if let Some(template) = template {
        base_cache_v1::remember_v1(source, bytes, &import, &conversion, template);
    }
    // The cache's tag is the check model the policy actually holds.
    let check = policy.no_state_digest_check_v1();
    Ok((
        policy,
        state,
        ConvertedLoadV1 {
            kind,
            base_bytes,
            check,
        },
    ))
}

/// The base state with the digest columns of W, m and v at +0.0, checked
/// against the conversion record and its 1,403-wide checkpoint before the
/// contract is installed on `policy`. The base model, still installed when
/// the contract takes it, becomes the check model.
fn convert_state_v1(
    policy: &mut FrozenPlayPolicyV1,
    base_state: NativePolicyValueTrainStateV1,
    conversion: &NoStateDigestConversionV1,
    check: bool,
) -> Result<NativePolicyValueTrainStateV1, String> {
    let base = base_state.snapshot_v1().map_err(err)?;
    let mut candidate = base.clone();
    for list in [
        &mut candidate.parameters,
        &mut candidate.first_moments,
        &mut candidate.second_moments,
    ] {
        let weight = list
            .iter_mut()
            .find(|parameter| parameter.name == STATE_ENCODER_WEIGHT)
            .ok_or("state encoder weight is missing")?;
        ensure(
            weight.shape
                == [
                    weight.values.len() / STATE_ENCODER_INPUT_V1,
                    STATE_ENCODER_INPUT_V1,
                ]
                && weight.values.len().is_multiple_of(STATE_ENCODER_INPUT_V1),
            "state encoder width differs",
        )?;
        for row in weight.values.chunks_exact_mut(STATE_ENCODER_INPUT_V1) {
            row[STATE_DIGEST_COLUMNS_V1].fill(0.0);
        }
    }
    ensure(
        candidate
            .parameters
            .iter()
            .find(|parameter| parameter.name == STATE_ENCODER_BIAS)
            .is_some_and(|bias| no_negative_zero_v1(&bias.values)),
        "a state encoder bias of -0.0 has no exact no-state-digest conversion",
    )?;
    let mut converted =
        NativePolicyValueTrainStateV1::from_snapshot_v1(base_state.model_v1().clone(), &candidate)
            .map_err(err)?;
    converted.set_training_math_v1(base_state.training_math_v1());
    ensure(
        converted.model_v1().parameter_manifest_sha256_v1()
            == conversion.converted.model_parameter_sha256,
        "the converted parameters differ from the conversion record",
    )?;
    ensure(
        converted.adam_step_v1() == base.adam_step
            && converted.scorer_bias_anchor_f32_bits_v1() == base.scorer_bias_anchor_bits
            && converted.training_math_v1() == base_state.training_math_v1(),
        "the conversion changed the Adam step, gauge anchor or training math",
    )?;
    match (
        &conversion.base_checkpoint,
        &conversion.converted_checkpoint,
    ) {
        (None, None) => {}
        (Some(_), Some(pin)) => check_converted_checkpoint_v1(
            pin,
            &ConvertedCheckpointExpectationV1 {
                schema: ordinary_checkpoint_schema_v1(policy.identity_v1()),
                base_state_sha256: hex(&base.state_sha256_v1().map_err(err)?),
                converted_contract: &conversion.converted.feature_contract_digest,
                encoding: policy.feature_identity_v1().feature_encoding_digest,
                training_math: base_state.training_math_v1(),
            },
            &candidate,
        )?,
        _ => return Err("the conversion record's checkpoints differ in presence".into()),
    }
    install_v1(policy, conversion, check)?;
    Ok(converted)
}

/// A trained checkpoint of this conversion: its contract member names
/// exactly `conversion_pin` and the derived digest beside the base encoding,
/// and its stored W/m/v digest columns are already +0.0 bits, checked before
/// anything could zero them. The base model (the conversion's pinned base
/// checkpoint) must still be the recorded one; it supplies the check model's
/// original digest columns. A trained checkpoint is never compared with the
/// initial converted parameter digest.
///
/// `install` carries the per-forward check setting when the base policy is
/// still uninstalled; then its installed duplicate is handed back for the
/// base cache. `None`: the policy comes installed and bound from that cache.
#[allow(
    clippy::type_complexity,
    reason = "the state, its cache kind and the installed base"
)]
fn continue_state_v1(
    source: &ExpandedModelSourceV1,
    policy: &mut FrozenPlayPolicyV1,
    conversion: &NoStateDigestConversionV1,
    conversion_pin: &PinnedFileV1,
    inputs: Option<&CheckpointInputExpectationV1<'_>>,
    install: Option<bool>,
) -> Result<
    (
        NativePolicyValueTrainStateV1,
        ConvertedLoadKindV1,
        Option<FrozenPlayPolicyV1>,
    ),
    String,
> {
    let pin = source
        .checkpoint
        .as_ref()
        .ok_or("a no-state-digest continuation requires a checkpoint")?;
    let saved = read_ordinary_checkpoint_v1(pin, policy.identity_v1())?;
    if let Some(inputs) = inputs {
        inputs.validate_v1(
            &saved.trajectories,
            saved.learning_rate_bits,
            saved.value_coefficient_bits,
        )?;
    }
    let record = saved
        .input_contract
        .as_ref()
        .ok_or("an unconverted checkpoint does not continue a no-state-digest source")?;
    ensure(
        record.conversion == *conversion_pin,
        "the checkpoint belongs to another no-state-digest conversion",
    )?;
    let resolved = resolve_contract_v1(
        &saved.feature_contract_digest,
        &saved.feature_encoding_digest,
        &saved.card_db_hash,
        Some(record),
    )?;
    ensure(
        resolved.input_contract_digest_v1()
            == Some(conversion.converted.feature_contract_digest.as_str())
            && resolved.base_contract_digest == conversion.base.feature_contract_digest,
        "the checkpoint's no-state-digest contract differs from the conversion's",
    )?;
    check_saved_columns_v1(&saved)?;
    let template = match install {
        Some(check) => {
            install_v1(policy, conversion, check)?;
            policy.bind_input_contract_conversion_v1(conversion_pin.clone())?;
            policy.duplicate_loaded_v1().ok()
        }
        None => None,
    };
    let model = policy.training_model_v3();
    let state = restore_checkpoint_state_v1(&saved, policy, model)?;
    // A checkpoint that names another file is never cached, as for an
    // ordinary source.
    let kind = if saved.archive_distillation.is_some() {
        ConvertedLoadKindV1::NotCached
    } else {
        ConvertedLoadKindV1::Continuation(super::loaded_source_cache_v1::RecordedInputsV1 {
            trajectories: saved.trajectories.clone(),
            learning_rate_bits: saved.learning_rate_bits,
            value_coefficient_bits: saved.value_coefficient_bits,
        })
    };
    Ok((state, kind, template))
}

/// The stored bits of a checkpoint's W, m and v digest columns are +0.0 and
/// its state encoder bias has no -0.0.
fn check_saved_columns_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
    for (list, what) in [
        (&saved.parameters, "parameter"),
        (&saved.first_moments, "first moment"),
        (&saved.second_moments, "second moment"),
    ] {
        let weight = list
            .iter()
            .find(|parameter| parameter.name == STATE_ENCODER_WEIGHT)
            .ok_or("state encoder weight is missing")?;
        let values: Vec<f32> = weight
            .values
            .iter()
            .map(|bits| f32::from_bits(*bits))
            .collect();
        ensure(
            columns_are_positive_zero_v1(&values, STATE_ENCODER_INPUT_V1, STATE_DIGEST_COLUMNS_V1),
            &format!("no-state-digest contract: a stored state-digest {what} column is not +0.0"),
        )?;
    }
    let bias = saved
        .parameters
        .iter()
        .find(|parameter| parameter.name == STATE_ENCODER_BIAS)
        .ok_or("state encoder bias is missing")?;
    ensure(
        bias.values.iter().all(|bits| *bits != 0x8000_0000),
        "a state encoder bias of -0.0 has no exact no-state-digest conversion",
    )
}

/// The members of the 1,403-wide conversion artifact
/// ([`converted_checkpoint_bytes_v1`]) its comparison reads. It is not a
/// resumable checkpoint: it records `source_state_sha256`, not
/// `state_sha256`, and the narrower state encoder.
#[derive(Deserialize)]
struct ConvertedCheckpointViewV1 {
    schema: String,
    source_state_sha256: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    parameters: Vec<ParameterBitsV1>,
    first_moments: Vec<ParameterBitsV1>,
    second_moments: Vec<ParameterBitsV1>,
    input_contract: ConvertedCheckpointContractV1,
    #[serde(default)]
    training_math: TrainingMathV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConvertedCheckpointContractV1 {
    contract: String,
    state_digest_columns: [usize; 2],
}

/// What the conversion's 1,403-wide checkpoint must record: the base
/// checkpoint's schema and state, the converted contract beside the base
/// encoding, and the base optimizer's training math.
struct ConvertedCheckpointExpectationV1<'a> {
    schema: &'static str,
    base_state_sha256: String,
    converted_contract: &'a str,
    encoding: &'static str,
    training_math: TrainingMathV1,
}

/// Every kept bit of the converted parameters and both moments, with every
/// tensor's exact shape (the state encoder weight and its moments 1,403
/// wide), the Adam step, gauge anchor, training math and the artifact's
/// identity, against the conversion's 1,403-wide checkpoint, which names the
/// base state it was cut from.
fn check_converted_checkpoint_v1(
    pin: &PinnedFileV1,
    expected: &ConvertedCheckpointExpectationV1<'_>,
    candidate: &NativePolicyValueTrainSnapshotV1,
) -> Result<(), String> {
    let view: ConvertedCheckpointViewV1 = read_pinned(pin)?;
    ensure(
        view.schema == expected.schema
            && view.feature_encoding_digest == expected.encoding
            && view.card_db_hash == format!("{KERNEL_CARDDB_HASH:016x}"),
        "the converted checkpoint's schema, encoding or card database differs",
    )?;
    ensure(
        view.source_state_sha256 == expected.base_state_sha256
            && view.feature_contract_digest == expected.converted_contract
            && view.input_contract.contract == CONTRACT_V1
            && view.input_contract.state_digest_columns
                == [STATE_DIGEST_COLUMNS_V1.start, STATE_DIGEST_COLUMNS_V1.end]
            && view.adam_step == candidate.adam_step
            && view.scorer_bias_anchor_bits == candidate.scorer_bias_anchor_bits
            && view.training_math == expected.training_math,
        "the converted checkpoint names another base state or optimizer",
    )?;
    for (stored, converted) in [
        (&view.parameters, &candidate.parameters),
        (&view.first_moments, &candidate.first_moments),
        (&view.second_moments, &candidate.second_moments),
    ] {
        ensure(
            stored.len() == converted.len(),
            "converted checkpoint tensor count differs",
        )?;
        for (stored, converted) in stored.iter().zip(converted) {
            let mut shape = converted.shape.clone();
            if converted.name == STATE_ENCODER_WEIGHT {
                shape = vec![
                    converted.values.len() / STATE_ENCODER_INPUT_V1,
                    STATE_ENCODER_INPUT_V1 - STATE_DIGEST_COLUMNS_V1.len(),
                ];
            }
            ensure(
                stored.shape == shape,
                "converted checkpoint tensor shape differs",
            )?;
            let kept: Vec<u32> = if converted.name == STATE_ENCODER_WEIGHT {
                converted
                    .values
                    .chunks_exact(STATE_ENCODER_INPUT_V1)
                    .flat_map(|row| {
                        row[..STATE_DIGEST_COLUMNS_V1.start]
                            .iter()
                            .chain(&row[STATE_DIGEST_COLUMNS_V1.end..])
                            .map(|value| value.to_bits())
                    })
                    .collect()
            } else {
                converted
                    .values
                    .iter()
                    .map(|value| value.to_bits())
                    .collect()
            };
            ensure(
                stored.name == converted.name && stored.values == kept,
                "converted checkpoint differs from the converted state",
            )?;
        }
    }
    Ok(())
}

/// The four files a conversion writes.
#[derive(Clone, Debug, Serialize)]
pub struct NoStateDigestOutputsV1 {
    pub source: PinnedFileV1,
    pub play_import: PinnedFileV1,
    pub conversion: PinnedFileV1,
    pub converted_checkpoint: Option<PinnedFileV1>,
    pub record: NoStateDigestConversionV1,
}

/// Durable publication that never replaces an existing file, read back
/// through the bounded pinned reader.
fn write_pinned_v1(directory: &Path, name: &str, bytes: &[u8]) -> Result<PinnedFileV1, String> {
    let pin = publish_json_bytes_v1(directory, name, bytes)?;
    ensure(pin.sha256 == sha(bytes), "published bytes differ")?;
    read_pinned_bytes(&pin)?;
    Ok(pin)
}

/// Drops `columns` from every row of a `[rows, width]` JSON tensor entry.
fn drop_columns_v1(entry: &mut Value, columns: std::ops::Range<usize>) -> Result<(), String> {
    let shape: Vec<usize> = serde_json::from_value(entry["shape"].clone()).map_err(err)?;
    let &[rows, width] = shape.as_slice() else {
        return Err("state encoder weight is not a matrix".into());
    };
    ensure(
        width == STATE_ENCODER_INPUT_V1,
        "state encoder width differs",
    )?;
    let values = entry["values"].as_array().ok_or("tensor values missing")?;
    ensure(values.len() == rows * width, "tensor size differs")?;
    let kept: Vec<Value> = values
        .chunks_exact(width)
        .flat_map(|row| {
            row[..columns.start]
                .iter()
                .chain(&row[columns.end..])
                .cloned()
        })
        .collect();
    entry["shape"] = Value::from(vec![rows, width - columns.len()]);
    entry["values"] = Value::Array(kept);
    Ok(())
}

/// The base checkpoint in the 1,403-wide layout, checked against the
/// installed (zero-column) runtime parameters bit for bit.
fn converted_checkpoint_bytes_v1(
    checkpoint: &[u8],
    converted_contract: &str,
    runtime_weight: &[f32],
) -> Result<Vec<u8>, String> {
    let mut value: Value = serde_json::from_slice(checkpoint).map_err(err)?;
    let source_state = value
        .get("state_sha256")
        .cloned()
        .ok_or("checkpoint has no state digest")?;
    for list in ["parameters", "first_moments", "second_moments"] {
        let entries = value[list]
            .as_array_mut()
            .ok_or("checkpoint tensor list missing")?;
        let mut found = 0;
        for entry in entries.iter_mut() {
            if entry["name"] == STATE_ENCODER_WEIGHT {
                drop_columns_v1(entry, STATE_DIGEST_COLUMNS_V1)?;
                found += 1;
            }
        }
        ensure(found == 1, "state encoder tensor not found exactly once")?;
    }
    // The kept parameter columns are the runtime's nonzero columns.
    let kept_runtime: Vec<u32> = runtime_weight
        .chunks_exact(STATE_ENCODER_INPUT_V1)
        .flat_map(|row| {
            row[..STATE_DIGEST_COLUMNS_V1.start]
                .iter()
                .chain(&row[STATE_DIGEST_COLUMNS_V1.end..])
                .map(|x| x.to_bits())
        })
        .collect();
    let weight = value["parameters"]
        .as_array()
        .and_then(|entries| entries.iter().find(|e| e["name"] == STATE_ENCODER_WEIGHT))
        .ok_or("state encoder parameters missing")?;
    let stored: Vec<u32> = serde_json::from_value(weight["values"].clone()).map_err(err)?;
    ensure(
        stored == kept_runtime,
        "converted checkpoint differs from the runtime parameters",
    )?;
    let object = value.as_object_mut().ok_or("checkpoint is not an object")?;
    object.remove("state_sha256");
    object.insert("source_state_sha256".into(), source_state);
    object.insert(
        "feature_contract_digest".into(),
        Value::from(converted_contract),
    );
    object.insert(
        "input_contract".into(),
        serde_json::json!({
            "contract": CONTRACT_V1,
            "state_digest_columns": [STATE_DIGEST_COLUMNS_V1.start, STATE_DIGEST_COLUMNS_V1.end],
        }),
    );
    serde_json::to_vec(&value).map_err(err)
}

/// Converts the ordinary source at `source_path` into `out_dir`: the
/// conversion record, the converted import and source, and (when the source
/// has a checkpoint) the 1,403-wide checkpoint. The converted source is then
/// loaded back and must install the same model.
pub fn convert_no_state_digest_v1(
    source_path: &Path,
    out_dir: &Path,
) -> Result<NoStateDigestOutputsV1, String> {
    let source: ExpandedModelSourceV1 =
        serde_json::from_slice(&fs::read(source_path).map_err(err)?).map_err(err)?;
    let (mut policy, _) = super::load_expanded_inference_v1(&source)?;
    ensure(
        policy.input_contract_v1().is_none(),
        "the source is already converted",
    )?;
    let base = policy.actual_model_identity_v1();
    let converted_contract = converted_contract_digest_v1(&base.feature_contract_digest);
    policy.apply_no_state_digest_v1(
        converted_contract.clone(),
        STATE_DIGEST_COLUMNS_V1,
        STATE_ENCODER_INPUT_V1,
        false,
    )?;
    let converted = policy.actual_model_identity_v1();
    fs::create_dir_all(out_dir).map_err(err)?;
    let converted_checkpoint = match &source.checkpoint {
        None => None,
        Some(pin) => {
            let runtime = policy
                .training_parameters_v3()
                .into_iter()
                .find(|p| p.name == STATE_ENCODER_WEIGHT)
                .ok_or("state encoder weight is missing")?;
            let bytes = converted_checkpoint_bytes_v1(
                &read_pinned_bytes(pin)?,
                &converted_contract,
                &runtime.values,
            )?;
            Some(write_pinned_v1(
                out_dir,
                "checkpoint-no-state-digest.json",
                &bytes,
            )?)
        }
    };
    let record = NoStateDigestConversionV1 {
        schema: CONVERSION_SCHEMA_V1.into(),
        contract: CONTRACT_V1.into(),
        state_digest_columns: [STATE_DIGEST_COLUMNS_V1.start, STATE_DIGEST_COLUMNS_V1.end],
        base_play_import: source.play_import.clone(),
        base_checkpoint: source.checkpoint.clone(),
        base: NoStateDigestModelV1 {
            model_parameter_sha256: base.model_parameter_sha256,
            feature_contract_digest: base.feature_contract_digest,
        },
        converted: NoStateDigestModelV1 {
            model_parameter_sha256: converted.model_parameter_sha256,
            feature_contract_digest: converted.feature_contract_digest,
        },
        converted_checkpoint: converted_checkpoint.clone(),
    };
    let conversion = write_pinned_v1(
        out_dir,
        "conversion.json",
        &serde_json::to_vec_pretty(&record).map_err(err)?,
    )?;
    let import = NoStateDigestImportV1 {
        schema: IMPORT_SCHEMA_V1.into(),
        base_play_import: source.play_import.clone(),
        conversion: conversion.clone(),
    };
    let play_import = write_pinned_v1(
        out_dir,
        "import.json",
        &serde_json::to_vec_pretty(&import).map_err(err)?,
    )?;
    let converted_source = ExpandedModelSourceV1 {
        play_import: play_import.clone(),
        feature_transfer: source.feature_transfer.clone(),
        checkpoint: source.checkpoint.clone(),
    };
    let source_pin = write_pinned_v1(
        out_dir,
        "source.json",
        &serde_json::to_vec_pretty(&converted_source).map_err(err)?,
    )?;
    // Round trip: the converted source installs exactly the recorded model.
    let (reloaded, identity) = super::load_expanded_inference_v1(&converted_source)?;
    ensure(
        reloaded.input_contract_v1() == Some(record.converted.feature_contract_digest.as_str())
            && identity.model.model_parameter_sha256 == record.converted.model_parameter_sha256,
        "the converted source does not reload as recorded",
    )?;
    Ok(NoStateDigestOutputsV1 {
        source: source_pin,
        play_import,
        conversion,
        converted_checkpoint,
        record,
    })
}

mod base_cache_v1;

#[cfg(test)]
mod training_tests;

#[cfg(test)]
mod tests {
    use crate::sideboard_play_policy_v1::no_negative_zero_v1;

    /// The native first-layer order: start at the bias, add inputs in order.
    fn first_layer(bias: f32, weights: &[f32], inputs: &[f32]) -> f32 {
        weights
            .iter()
            .zip(inputs)
            .fold(bias, |sum, (w, x)| sum + x * w)
    }

    #[test]
    fn zeroed_columns_equal_removed_columns_unless_the_bias_is_negative_zero() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (kept_head, digest, kept_tail) = (5, 4, 6);
        for case in 0..20_000 {
            let mut value = |zero_odds: u64| {
                let r = next();
                if r % zero_odds == 0 {
                    if r & 1 == 0 {
                        0.0
                    } else {
                        -0.0
                    }
                } else {
                    (r >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
                }
            };
            let bias = if case % 3 == 0 { 0.0 } else { value(4) };
            let mut weights: Vec<f32> = (0..kept_head + digest + kept_tail)
                .map(|_| value(4))
                .collect();
            let mut inputs: Vec<f32> = (0..weights.len()).map(|_| value(2)).collect();
            inputs[kept_head..kept_head + digest].fill(0.0);
            let old = first_layer(bias, &weights, &inputs);
            let removed = first_layer(
                bias,
                &[&weights[..kept_head], &weights[kept_head + digest..]].concat(),
                &[&inputs[..kept_head], &inputs[kept_head + digest..]].concat(),
            );
            weights[kept_head..kept_head + digest].fill(0.0);
            let zeroed = first_layer(bias, &weights, &inputs);
            if no_negative_zero_v1(&[bias]) {
                assert_eq!(zeroed.to_bits(), removed.to_bits(), "case {case}");
                assert_eq!(zeroed.to_bits(), old.to_bits(), "case {case}");
            }
        }
        // The excluded case: a -0.0 bias with nothing else nonzero.
        // -0.0 + (-1.0 * 0.0) stays -0.0; adding the zeroed column's +0.0
        // turns it into +0.0.
        let removed = first_layer(-0.0, &[-1.0], &[0.0]);
        let zeroed = first_layer(-0.0, &[-1.0, 0.0], &[0.0, 0.0]);
        assert_ne!(zeroed.to_bits(), removed.to_bits());
        assert!(!no_negative_zero_v1(&[-0.0]));
    }
}
