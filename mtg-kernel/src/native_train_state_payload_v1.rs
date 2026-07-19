//! Deterministic headerless payload codec for the complete native train state.
//!
//! This module implements only the raw three-section payload. It does not
//! define checkpoint JSON, filesystem publication, recovery, or CLI behavior.
//! Decode constructs and validates a private owned snapshot before returning;
//! no caller-owned live train state is accepted or mutated here.

use crate::common_model_snapshot_v1::{
    PARAMETER_ELEMENT_COUNT_V1, PARAMETER_TENSOR_COUNT_V1, PAYLOAD_BYTE_COUNT_V1,
};
use crate::native_policy_train_step_v1::{
    native_train_state_parameter_layout_v1, NativePolicyTrainErrorV1,
    NativePolicyValueTrainSnapshotV1,
};
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1: &str =
    "mtg_kernel_native_train_state_payload/v1";
pub(crate) const NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1: &str = "ordered-three-section-f32le/v1";
pub(crate) const NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1: usize = PAYLOAD_BYTE_COUNT_V1;
pub(crate) const NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1: usize =
    NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1 * 3;

pub(crate) const NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1: [NativeTrainStatePayloadSectionLayoutV1;
    3] = [
    NativeTrainStatePayloadSectionLayoutV1 {
        name: "parameters",
        offset_bytes: 0,
        byte_count: NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
    },
    NativeTrainStatePayloadSectionLayoutV1 {
        name: "first_moments",
        offset_bytes: NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
        byte_count: NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
    },
    NativeTrainStatePayloadSectionLayoutV1 {
        name: "second_moments",
        offset_bytes: NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1 * 2,
        byte_count: NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
    },
];

const _: [(); 4_923_976] = [(); NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1];
const _: [(); 14_771_928] = [(); NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainStatePayloadSectionLayoutV1 {
    pub(crate) name: &'static str,
    pub(crate) offset_bytes: usize,
    pub(crate) byte_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainStatePayloadDigestsV1 {
    pub(crate) payload_sha256: [u8; 32],
    pub(crate) parameters_sha256: [u8; 32],
    pub(crate) first_moments_sha256: [u8; 32],
    pub(crate) second_moments_sha256: [u8; 32],
    pub(crate) model_parameter_sha256: [u8; 32],
    pub(crate) native_state_sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeEncodedTrainStatePayloadV1 {
    pub(crate) bytes: Vec<u8>,
    pub(crate) digests: NativeTrainStatePayloadDigestsV1,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeDecodedTrainStatePayloadV1 {
    pub(crate) snapshot: NativePolicyValueTrainSnapshotV1,
    pub(crate) digests: NativeTrainStatePayloadDigestsV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeTrainStatePayloadDigestFieldV1 {
    Payload,
    Parameters,
    FirstMoments,
    SecondMoments,
    ModelParameters,
    NativeState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeTrainStatePayloadErrorV1 {
    ExactLength { expected: usize, actual: usize },
    LayoutInvariant(&'static str),
    TrainState(NativePolicyTrainErrorV1),
    DigestMismatch(NativeTrainStatePayloadDigestFieldV1),
}

impl Display for NativeTrainStatePayloadErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native train-state payload v1 error: {self:?}")
    }
}

impl Error for NativeTrainStatePayloadErrorV1 {}

impl From<NativePolicyTrainErrorV1> for NativeTrainStatePayloadErrorV1 {
    fn from(value: NativePolicyTrainErrorV1) -> Self {
        Self::TrainState(value)
    }
}

/// Encodes a validated complete train snapshot into the exact headerless
/// parameters/first-moments/second-moments f32le layout.
pub(crate) fn encode_native_train_state_payload_v1(
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> Result<NativeEncodedTrainStatePayloadV1, NativeTrainStatePayloadErrorV1> {
    // This call is the authoritative structural and semantic precondition. It
    // covers the frozen manifest, finiteness, nonnegative second moments,
    // positive-zero padding/gauge moments, anchor, and bounded Adam step.
    let native_state_sha256 = snapshot.state_sha256_v1()?;

    let mut bytes = Vec::with_capacity(NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1);
    encode_section_v1(&mut bytes, &snapshot.parameters);
    encode_section_v1(&mut bytes, &snapshot.first_moments);
    encode_section_v1(&mut bytes, &snapshot.second_moments);
    require_exact_payload_length_v1(&bytes)?;

    let digests = payload_digests_v1(snapshot, &bytes, native_state_sha256)?;
    Ok(NativeEncodedTrainStatePayloadV1 { bytes, digests })
}

/// Decodes the exact headerless payload into a fully validated private owned
/// snapshot. `adam_step` and the scorer-bias anchor are checkpoint fields and
/// therefore deliberately remain out of the raw payload.
pub(crate) fn decode_native_train_state_payload_v1(
    bytes: &[u8],
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
) -> Result<NativeDecodedTrainStatePayloadV1, NativeTrainStatePayloadErrorV1> {
    require_exact_payload_length_v1(bytes)?;
    decode_native_train_state_payload_inner_v1(
        bytes,
        adam_step,
        scorer_bias_anchor_bits,
        raw_payload_digests_v1(bytes),
    )
}

/// As above, while also requiring every raw and semantic digest declared by a
/// checkpoint to match before the owned candidate is returned.
pub(crate) fn decode_native_train_state_payload_verified_v1(
    bytes: &[u8],
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    expected: &NativeTrainStatePayloadDigestsV1,
) -> Result<NativeDecodedTrainStatePayloadV1, NativeTrainStatePayloadErrorV1> {
    require_exact_payload_length_v1(bytes)?;
    let raw = raw_payload_digests_v1(bytes);
    require_digest_v1(
        raw.payload_sha256,
        expected.payload_sha256,
        NativeTrainStatePayloadDigestFieldV1::Payload,
    )?;
    require_digest_v1(
        raw.parameters_sha256,
        expected.parameters_sha256,
        NativeTrainStatePayloadDigestFieldV1::Parameters,
    )?;
    require_digest_v1(
        raw.first_moments_sha256,
        expected.first_moments_sha256,
        NativeTrainStatePayloadDigestFieldV1::FirstMoments,
    )?;
    require_digest_v1(
        raw.second_moments_sha256,
        expected.second_moments_sha256,
        NativeTrainStatePayloadDigestFieldV1::SecondMoments,
    )?;

    let decoded =
        decode_native_train_state_payload_inner_v1(bytes, adam_step, scorer_bias_anchor_bits, raw)?;
    require_digest_v1(
        decoded.digests.model_parameter_sha256,
        expected.model_parameter_sha256,
        NativeTrainStatePayloadDigestFieldV1::ModelParameters,
    )?;
    require_digest_v1(
        decoded.digests.native_state_sha256,
        expected.native_state_sha256,
        NativeTrainStatePayloadDigestFieldV1::NativeState,
    )?;
    Ok(decoded)
}

#[derive(Clone, Copy)]
struct RawPayloadDigestsV1 {
    payload_sha256: [u8; 32],
    parameters_sha256: [u8; 32],
    first_moments_sha256: [u8; 32],
    second_moments_sha256: [u8; 32],
}

fn decode_native_train_state_payload_inner_v1(
    bytes: &[u8],
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    raw: RawPayloadDigestsV1,
) -> Result<NativeDecodedTrainStatePayloadV1, NativeTrainStatePayloadErrorV1> {
    let parameters = decode_section_v1(section_bytes_v1(bytes, 0)?)?;
    let first_moments = decode_section_v1(section_bytes_v1(bytes, 1)?)?;
    let second_moments = decode_section_v1(section_bytes_v1(bytes, 2)?)?;
    let snapshot = NativePolicyValueTrainSnapshotV1 {
        adam_step,
        scorer_bias_anchor_bits,
        parameters,
        first_moments,
        second_moments,
    };

    // Validation happens on this function-local candidate; no partial result
    // can escape on any structural or semantic failure.
    let native_state_sha256 = snapshot.state_sha256_v1()?;
    let model_parameter_sha256 =
        model_parameter_sha256_v1(&snapshot.parameters, section_bytes_v1(bytes, 0)?)?;
    let digests = NativeTrainStatePayloadDigestsV1 {
        payload_sha256: raw.payload_sha256,
        parameters_sha256: raw.parameters_sha256,
        first_moments_sha256: raw.first_moments_sha256,
        second_moments_sha256: raw.second_moments_sha256,
        model_parameter_sha256,
        native_state_sha256,
    };
    Ok(NativeDecodedTrainStatePayloadV1 { snapshot, digests })
}

fn encode_section_v1(output: &mut Vec<u8>, tensors: &[NativeNamedParameterV1]) {
    for tensor in tensors {
        for value in &tensor.values {
            output.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
}

fn decode_section_v1(
    section: &[u8],
) -> Result<Vec<NativeNamedParameterV1>, NativeTrainStatePayloadErrorV1> {
    if section.len() != NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1 {
        return Err(NativeTrainStatePayloadErrorV1::ExactLength {
            expected: NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1,
            actual: section.len(),
        });
    }

    let mut tensors = Vec::with_capacity(PARAMETER_TENSOR_COUNT_V1);
    let mut cursor = 0usize;
    let mut element_count = 0usize;
    for (name, shape) in native_train_state_parameter_layout_v1() {
        let tensor_elements = shape
            .iter()
            .try_fold(1usize, |product, dimension| product.checked_mul(*dimension));
        let tensor_elements = tensor_elements.ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("tensor element-count overflow"),
        )?;
        let tensor_bytes = tensor_elements.checked_mul(4).ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("tensor byte-count overflow"),
        )?;
        let end = cursor.checked_add(tensor_bytes).ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("tensor byte-end overflow"),
        )?;
        let raw =
            section
                .get(cursor..end)
                .ok_or(NativeTrainStatePayloadErrorV1::LayoutInvariant(
                    "tensor outside section",
                ))?;
        let values = raw
            .chunks_exact(4)
            .map(|word| f32::from_bits(u32::from_le_bytes([word[0], word[1], word[2], word[3]])))
            .collect();
        tensors.push(NativeNamedParameterV1 {
            name,
            shape: shape.to_vec(),
            values,
        });
        cursor = end;
        element_count = element_count.checked_add(tensor_elements).ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("section element-count overflow"),
        )?;
    }
    if cursor != section.len()
        || element_count != PARAMETER_ELEMENT_COUNT_V1
        || tensors.len() != PARAMETER_TENSOR_COUNT_V1
    {
        return Err(NativeTrainStatePayloadErrorV1::LayoutInvariant(
            "frozen section layout mismatch",
        ));
    }
    Ok(tensors)
}

fn payload_digests_v1(
    snapshot: &NativePolicyValueTrainSnapshotV1,
    bytes: &[u8],
    native_state_sha256: [u8; 32],
) -> Result<NativeTrainStatePayloadDigestsV1, NativeTrainStatePayloadErrorV1> {
    let raw = raw_payload_digests_v1(bytes);
    Ok(NativeTrainStatePayloadDigestsV1 {
        payload_sha256: raw.payload_sha256,
        parameters_sha256: raw.parameters_sha256,
        first_moments_sha256: raw.first_moments_sha256,
        second_moments_sha256: raw.second_moments_sha256,
        model_parameter_sha256: model_parameter_sha256_v1(
            &snapshot.parameters,
            section_bytes_v1(bytes, 0)?,
        )?,
        native_state_sha256,
    })
}

fn raw_payload_digests_v1(bytes: &[u8]) -> RawPayloadDigestsV1 {
    RawPayloadDigestsV1 {
        payload_sha256: sha256_v1(bytes),
        parameters_sha256: sha256_v1(&bytes[section_range_v1(0)]),
        first_moments_sha256: sha256_v1(&bytes[section_range_v1(1)]),
        second_moments_sha256: sha256_v1(&bytes[section_range_v1(2)]),
    }
}

fn model_parameter_sha256_v1(
    parameters: &[NativeNamedParameterV1],
    parameter_section: &[u8],
) -> Result<[u8; 32], NativeTrainStatePayloadErrorV1> {
    if parameters.len() != PARAMETER_TENSOR_COUNT_V1
        || parameter_section.len() != NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1
    {
        return Err(NativeTrainStatePayloadErrorV1::LayoutInvariant(
            "model parameter stream input layout",
        ));
    }

    let mut hasher = Sha256::new();
    let mut cursor = 0usize;
    for (parameter, (expected_name, expected_shape)) in parameters
        .iter()
        .zip(native_train_state_parameter_layout_v1())
    {
        if parameter.name != expected_name || parameter.shape.as_slice() != expected_shape {
            return Err(NativeTrainStatePayloadErrorV1::LayoutInvariant(
                "model parameter stream named layout",
            ));
        }
        let name_len = u32::try_from(parameter.name.len()).map_err(|_| {
            NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter name length overflow")
        })?;
        let rank = u32::try_from(parameter.shape.len()).map_err(|_| {
            NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter rank overflow")
        })?;
        let element_count = u64::try_from(parameter.values.len()).map_err(|_| {
            NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter element-count overflow")
        })?;
        let byte_count = parameter.values.len().checked_mul(4).ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter byte-count overflow"),
        )?;
        let end = cursor.checked_add(byte_count).ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter byte-end overflow"),
        )?;
        let raw = parameter_section.get(cursor..end).ok_or(
            NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter stream outside section"),
        )?;

        hasher.update(name_len.to_be_bytes());
        hasher.update(parameter.name.as_bytes());
        hasher.update(rank.to_be_bytes());
        for dimension in &parameter.shape {
            let dimension = u64::try_from(*dimension).map_err(|_| {
                NativeTrainStatePayloadErrorV1::LayoutInvariant("parameter dimension overflow")
            })?;
            hasher.update(dimension.to_be_bytes());
        }
        hasher.update(element_count.to_be_bytes());
        hasher.update(raw);
        cursor = end;
    }
    if cursor != parameter_section.len() {
        return Err(NativeTrainStatePayloadErrorV1::LayoutInvariant(
            "model parameter stream trailing bytes",
        ));
    }
    Ok(finalize_sha256_v1(hasher))
}

fn require_exact_payload_length_v1(bytes: &[u8]) -> Result<(), NativeTrainStatePayloadErrorV1> {
    if bytes.len() != NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 {
        return Err(NativeTrainStatePayloadErrorV1::ExactLength {
            expected: NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1,
            actual: bytes.len(),
        });
    }
    Ok(())
}

fn section_range_v1(index: usize) -> std::ops::Range<usize> {
    let layout = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[index];
    layout.offset_bytes..layout.offset_bytes + layout.byte_count
}

fn section_bytes_v1(bytes: &[u8], index: usize) -> Result<&[u8], NativeTrainStatePayloadErrorV1> {
    bytes
        .get(section_range_v1(index))
        .ok_or(NativeTrainStatePayloadErrorV1::LayoutInvariant(
            "section outside payload",
        ))
}

fn require_digest_v1(
    actual: [u8; 32],
    expected: [u8; 32],
    field: NativeTrainStatePayloadDigestFieldV1,
) -> Result<(), NativeTrainStatePayloadErrorV1> {
    if actual != expected {
        return Err(NativeTrainStatePayloadErrorV1::DigestMismatch(field));
    }
    Ok(())
}

fn sha256_v1(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    finalize_sha256_v1(hasher)
}

fn finalize_sha256_v1(hasher: Sha256) -> [u8; 32] {
    let digest = hasher.finalize();
    let mut output = [0u8; 32];
    output.copy_from_slice(&digest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_train_step_v1::{
        NativePolicyTrainErrorV1, NativePolicyValueTrainStateV1,
    };
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn snapshot_with_distinct_moments_v1() -> (NativePolicyValueTrainSnapshotV1, String) {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let model_parameter_sha256 = model.parameter_manifest_sha256_v1();
        let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let mut snapshot = state.snapshot_v1().unwrap();
        snapshot.first_moments[1].values[0] = 0.25;
        snapshot.second_moments[1].values[0] = 0.5;
        snapshot.state_sha256_v1().unwrap();
        (snapshot, model_parameter_sha256)
    }

    fn hex_v1(digest: [u8; 32]) -> String {
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn tensor_offset_v1(name: &str) -> usize {
        let mut offset = 0usize;
        for (candidate, shape) in native_train_state_parameter_layout_v1() {
            if candidate == name {
                return offset;
            }
            offset += shape.iter().product::<usize>() * 4;
        }
        panic!("unknown tensor {name}");
    }

    fn assert_expected_digest_mismatch_v1(
        encoded: &NativeEncodedTrainStatePayloadV1,
        snapshot: &NativePolicyValueTrainSnapshotV1,
        field: NativeTrainStatePayloadDigestFieldV1,
        mutate: impl FnOnce(&mut NativeTrainStatePayloadDigestsV1),
    ) {
        let mut expected = encoded.digests;
        mutate(&mut expected);
        assert_eq!(
            decode_native_train_state_payload_verified_v1(
                &encoded.bytes,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
                &expected,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::DigestMismatch(field)
        );
    }

    #[test]
    fn exact_payload_roundtrips_deterministically_with_all_hashes() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1, 14_771_928);
        assert_eq!(NATIVE_TRAIN_STATE_SECTION_BYTE_COUNT_V1, 4_923_976);
        assert_eq!(
            NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1,
            [
                NativeTrainStatePayloadSectionLayoutV1 {
                    name: "parameters",
                    offset_bytes: 0,
                    byte_count: 4_923_976,
                },
                NativeTrainStatePayloadSectionLayoutV1 {
                    name: "first_moments",
                    offset_bytes: 4_923_976,
                    byte_count: 4_923_976,
                },
                NativeTrainStatePayloadSectionLayoutV1 {
                    name: "second_moments",
                    offset_bytes: 9_847_952,
                    byte_count: 4_923_976,
                },
            ]
        );

        let (snapshot, model_parameter_sha256) = snapshot_with_distinct_moments_v1();
        let encoded = encode_native_train_state_payload_v1(&snapshot).unwrap();
        let repeated = encode_native_train_state_payload_v1(&snapshot).unwrap();
        assert_eq!(encoded, repeated);
        assert_eq!(encoded.bytes.len(), 14_771_928);
        let tensor_offset = tensor_offset_v1("object_encoder.0.weight");
        let first_moment_offset =
            NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[1].offset_bytes + tensor_offset;
        let second_moment_offset =
            NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[2].offset_bytes + tensor_offset;
        assert_eq!(
            &encoded.bytes[first_moment_offset..first_moment_offset + 4],
            &[0x00, 0x00, 0x80, 0x3e]
        );
        assert_eq!(
            &encoded.bytes[second_moment_offset..second_moment_offset + 4],
            &[0x00, 0x00, 0x00, 0x3f]
        );
        assert_eq!(encoded.digests.payload_sha256, sha256_v1(&encoded.bytes));
        assert_eq!(
            encoded.digests.parameters_sha256,
            sha256_v1(&encoded.bytes[section_range_v1(0)])
        );
        assert_eq!(
            encoded.digests.first_moments_sha256,
            sha256_v1(&encoded.bytes[section_range_v1(1)])
        );
        assert_eq!(
            encoded.digests.second_moments_sha256,
            sha256_v1(&encoded.bytes[section_range_v1(2)])
        );
        assert_eq!(
            hex_v1(encoded.digests.model_parameter_sha256),
            model_parameter_sha256
        );
        assert_eq!(
            encoded.digests.native_state_sha256,
            snapshot.state_sha256_v1().unwrap()
        );

        let decoded = decode_native_train_state_payload_v1(
            &encoded.bytes,
            snapshot.adam_step,
            snapshot.scorer_bias_anchor_bits,
        )
        .unwrap();
        assert_eq!(decoded.snapshot, snapshot);
        assert_eq!(decoded.digests, encoded.digests);
        assert_eq!(
            decode_native_train_state_payload_verified_v1(
                &encoded.bytes,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
                &encoded.digests,
            )
            .unwrap(),
            decoded
        );
    }

    #[test]
    fn truncation_and_semantic_corruption_fail_before_candidate_return() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (snapshot, _) = snapshot_with_distinct_moments_v1();

        let mut invalid_snapshot = snapshot.clone();
        invalid_snapshot.second_moments[1].values[0] = -f32::EPSILON;
        assert_eq!(
            encode_native_train_state_payload_v1(&invalid_snapshot).unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::OptimizerState)
        );

        let encoded = encode_native_train_state_payload_v1(&snapshot).unwrap();

        assert_eq!(
            decode_native_train_state_payload_v1(
                &encoded.bytes[..encoded.bytes.len() - 1],
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::ExactLength {
                expected: 14_771_928,
                actual: 14_771_927,
            }
        );
        let mut oversized = encoded.bytes.clone();
        oversized.push(0);
        assert!(matches!(
            decode_native_train_state_payload_v1(
                &oversized,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
            ),
            Err(NativeTrainStatePayloadErrorV1::ExactLength {
                expected: 14_771_928,
                actual: 14_771_929
            })
        ));
        drop(oversized);

        let mut nonfinite_parameter = encoded.bytes.clone();
        nonfinite_parameter[..4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert_eq!(
            decode_native_train_state_payload_v1(
                &nonfinite_parameter,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::ParameterManifest)
        );
        drop(nonfinite_parameter);

        let mut negative_zero_padding_moment = encoded.bytes.clone();
        let first_offset = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[1].offset_bytes;
        negative_zero_padding_moment[first_offset..first_offset + 4]
            .copy_from_slice(&(-0.0f32).to_bits().to_le_bytes());
        assert_eq!(
            decode_native_train_state_payload_v1(
                &negative_zero_padding_moment,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::OptimizerState)
        );
        drop(negative_zero_padding_moment);

        let mut negative_second_moment = encoded.bytes.clone();
        let second_offset = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[2].offset_bytes
            + tensor_offset_v1("object_encoder.0.weight");
        negative_second_moment[second_offset..second_offset + 4]
            .copy_from_slice(&(-f32::EPSILON).to_bits().to_le_bytes());
        assert_eq!(
            decode_native_train_state_payload_v1(
                &negative_second_moment,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::OptimizerState)
        );
        drop(negative_second_moment);

        let mut nonzero_gauge_moment = encoded.bytes.clone();
        let gauge_offset = NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1[1].offset_bytes
            + tensor_offset_v1("scorer.2.bias");
        nonzero_gauge_moment[gauge_offset..gauge_offset + 4]
            .copy_from_slice(&1.0f32.to_bits().to_le_bytes());
        assert_eq!(
            decode_native_train_state_payload_v1(
                &nonzero_gauge_moment,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::OptimizerState)
        );

        assert_eq!(
            decode_native_train_state_payload_v1(
                &encoded.bytes,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits ^ 1,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::GaugeAnchor)
        );
        assert_eq!(
            decode_native_train_state_payload_v1(
                &encoded.bytes,
                i32::MAX as u64 + 1,
                snapshot.scorer_bias_anchor_bits,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::TrainState(NativePolicyTrainErrorV1::AdamStepOverflow)
        );
    }

    #[test]
    fn verified_decode_rejects_finite_corruption_and_section_reordering() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (snapshot, _) = snapshot_with_distinct_moments_v1();
        let encoded = encode_native_train_state_payload_v1(&snapshot).unwrap();

        assert_expected_digest_mismatch_v1(
            &encoded,
            &snapshot,
            NativeTrainStatePayloadDigestFieldV1::Payload,
            |digests| digests.payload_sha256[0] ^= 1,
        );
        assert_expected_digest_mismatch_v1(
            &encoded,
            &snapshot,
            NativeTrainStatePayloadDigestFieldV1::Parameters,
            |digests| digests.parameters_sha256[0] ^= 1,
        );
        assert_expected_digest_mismatch_v1(
            &encoded,
            &snapshot,
            NativeTrainStatePayloadDigestFieldV1::FirstMoments,
            |digests| digests.first_moments_sha256[0] ^= 1,
        );
        assert_expected_digest_mismatch_v1(
            &encoded,
            &snapshot,
            NativeTrainStatePayloadDigestFieldV1::SecondMoments,
            |digests| digests.second_moments_sha256[0] ^= 1,
        );
        assert_expected_digest_mismatch_v1(
            &encoded,
            &snapshot,
            NativeTrainStatePayloadDigestFieldV1::ModelParameters,
            |digests| digests.model_parameter_sha256[0] ^= 1,
        );
        assert_expected_digest_mismatch_v1(
            &encoded,
            &snapshot,
            NativeTrainStatePayloadDigestFieldV1::NativeState,
            |digests| digests.native_state_sha256[0] ^= 1,
        );

        let mut finite_corruption = encoded.bytes.clone();
        let parameter_offset = tensor_offset_v1("object_encoder.0.weight");
        let original = u32::from_le_bytes(
            finite_corruption[parameter_offset..parameter_offset + 4]
                .try_into()
                .unwrap(),
        );
        finite_corruption[parameter_offset..parameter_offset + 4]
            .copy_from_slice(&(original ^ 1).to_le_bytes());
        let corrupted = decode_native_train_state_payload_v1(
            &finite_corruption,
            snapshot.adam_step,
            snapshot.scorer_bias_anchor_bits,
        )
        .unwrap();
        assert_ne!(corrupted.digests, encoded.digests);
        assert_eq!(
            decode_native_train_state_payload_verified_v1(
                &finite_corruption,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
                &encoded.digests,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::DigestMismatch(
                NativeTrainStatePayloadDigestFieldV1::Payload
            )
        );
        drop(finite_corruption);

        let mut reordered = encoded.bytes.clone();
        let first = reordered[section_range_v1(1)].to_vec();
        let second = reordered[section_range_v1(2)].to_vec();
        reordered[section_range_v1(1)].copy_from_slice(&second);
        reordered[section_range_v1(2)].copy_from_slice(&first);
        let decoded = decode_native_train_state_payload_v1(
            &reordered,
            snapshot.adam_step,
            snapshot.scorer_bias_anchor_bits,
        )
        .unwrap();
        assert_eq!(decoded.snapshot.first_moments[1].values[0], 0.5);
        assert_eq!(decoded.snapshot.second_moments[1].values[0], 0.25);
        assert_eq!(
            decode_native_train_state_payload_verified_v1(
                &reordered,
                snapshot.adam_step,
                snapshot.scorer_bias_anchor_bits,
                &encoded.digests,
            )
            .unwrap_err(),
            NativeTrainStatePayloadErrorV1::DigestMismatch(
                NativeTrainStatePayloadDigestFieldV1::Payload
            )
        );
    }
}
