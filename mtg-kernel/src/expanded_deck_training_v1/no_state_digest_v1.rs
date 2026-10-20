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
//! [`NoStateDigestConversionV1`]. The loader validates the original model
//! exactly as before, installs the contract and checks the result against the
//! conversion record. Training readers refuse the import schema, so nothing
//! trains under this contract yet. `MTG_KERNEL_NO_STATE_DIGEST_CHECK=1` keeps
//! the unconverted model and compares every forward with it, bit for bit.

use super::{
    ensure, err, load_expanded_inference_any_math_v1, publish_json_bytes_v1, read_pinned,
    read_pinned_bytes, sha, ExpandedInferenceIdentityV1, ExpandedModelSourceV1, PinnedFileV1,
};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
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

fn check_enabled_v1() -> bool {
    std::env::var("MTG_KERNEL_NO_STATE_DIGEST_CHECK").is_ok_and(|value| value == "1")
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

/// The loader branch for a [`NoStateDigestImportV1`] `play_import`. Never
/// inlined: its policy locals must not enlarge the ordinary loader's frame,
/// which several binaries still reach on a 1 MiB main thread.
#[inline(never)]
pub(super) fn load_v1(
    source: &ExpandedModelSourceV1,
    bytes: &[u8],
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let import: NoStateDigestImportV1 = serde_json::from_slice(bytes).map_err(err)?;
    ensure(
        import.schema == IMPORT_SCHEMA_V1,
        "unsupported no-state-digest import",
    )?;
    let conversion: NoStateDigestConversionV1 = read_pinned(&import.conversion)?;
    validate_conversion_v1(&conversion)?;
    ensure(
        conversion.base_play_import == import.base_play_import
            && conversion.base_checkpoint == source.checkpoint,
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
