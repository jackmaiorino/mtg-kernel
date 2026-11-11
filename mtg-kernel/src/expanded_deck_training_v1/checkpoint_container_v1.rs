//! Checkpoint container v1 (bookkeeping part 2, design revision 6): one file
//! holding a train state as a small canonical JSON document plus aligned
//! little-endian `f32` bit sections, instead of a 20 MB JSON document of
//! decimal integers.
//!
//! Layout, every integer little-endian:
//!
//! ```text
//! offset 0   magic             8 bytes  "MTGCKPT\0"
//! offset 8   container_version u32      = 1
//! offset 12  reserved          u32      = 0
//! offset 16  document_length   u64      canonical serde_json of the checkpoint
//!                                       with `parameters`, `first_moments` and
//!                                       `second_moments` empty
//! offset 24  table_length      u64      canonical serde_json of the section table
//! offset 32  document, then table, then zero padding to the data start, the
//!            first multiple of 256 after the table
//! data       for each of parameters, first moments and second moments, and
//!            each tensor in order: element_count x 4 bytes of f32 bits, each
//!            tensor starting at a multiple of 256 from the data start, zero
//!            padded between tensors; the file ends after the last tensor
//! ```
//!
//! The document keeps every field of [`ExpandedCheckpointV1`] with its name,
//! type and meaning (`schema`, `state_sha256` included), so probes read it as
//! they read a v1 checkpoint. A state has exactly one encoding: [`decode_v1`]
//! re-encodes what it decoded and refuses unless the bytes are equal.
//! [`export_v1`] reproduces the v1 JSON bytes `publish_json` writes for the
//! same checkpoint.
//!
//! # Tensors where they are held
//!
//! [`ExpandedCheckpointV1`] is generic over its tensors ([`TensorBitsV1`]):
//! a decoded checkpoint owns [`ParameterBitsV1`]s, and an update publishes a
//! checkpoint of [`ParameterBitsRefV1`]s, which borrow the live train state's
//! tensors (`TrainTensorsV1`) instead of copying every value's bits. Either
//! way each format
//! writes the same bytes: the v1 JSON through the same derived serialization,
//! the container through one encoder. The encoder writes into a sink
//! ([`SinkV1`]): a buffer, or a comparison with existing bytes, which is how
//! [`decode_v1`] checks its re-encoding without holding a copy of it.
//!
//! # Default, identity and config bytes (the default switch)
//!
//! The 16-run acceptance measurement accepted v2, so an update writes the
//! container by default wherever the writer's scope gate ([`admit_writer_v1`])
//! admits its checkpoint, and v1 JSON everywhere else. The selection is
//! [`CheckpointRecordFormatV1`]:
//!
//! - **Absent (`Auto`)**: v2 inside the gate, v1 outside it. Never
//!   serialized, so every run config, update command, `run.json` config
//!   member and run identity written before the switch keeps its bytes, and
//!   the same file still means the same run. (The rest of `run.json` carries
//!   build metadata and differs by build, as before.) What changes is the
//!   checkpoint file an in-scope default run publishes, and so the pins that
//!   carry its hash.
//! - **`"v1"`**: always v1 JSON. Serialized when explicit, so it no longer
//!   shares bytes with absence (before the switch it did, because absence
//!   meant v1). A config that needs the old bytes and output states `"v1"`.
//! - **`"v2"`**: always the container; outside the gate it is refused by name.
//!
//! The gate is applied to the checkpoint about to be written, so the default
//! cannot disagree with it. A run cannot resume across builds (`run.json`
//! pins the implementation digest), so the switch never changes a format
//! mid-run. The Rust readers of section 4.1 accept either format; the
//! no-state-digest conversion and the registry-transfer paths refuse a
//! container by name, and Python tools read one only through
//! `python/tools/checkpoint_container_v1.py`.

use super::{ExpandedCheckpointV1, ParameterBitsV1};
use crate::native_policy_value_net_v1::{NativeNamedParameterV1, NativeNamedTensorRefV1};
use serde::{Deserialize, Serialize, Serializer};
use std::borrow::Cow;

/// The container's first eight bytes.
pub(crate) const MAGIC_V1: &[u8; 8] = b"MTGCKPT\0";
const CONTAINER_VERSION_V1: u32 = 1;
const HEADER_BYTES_V1: usize = 32;
const ALIGN_V1: usize = 256;
/// The section table's `state_format`.
pub(crate) const STATE_FORMAT_V1: &str = "mtgts-sections-v1";
const SECTION_NAMES_V1: [&str; 3] = ["parameters", "first_moments", "second_moments"];

/// How an update publishes its checkpoint.
///
/// - `Auto` (the default, and what an absent field means): the container
///   (`V2`) for a checkpoint inside the writer's scope gate, v1 JSON for every
///   other kind. The choice is a pure function of the checkpoint about to be
///   written (`admit_writer_v1`), so it is deterministic.
/// - `V1`: always v1 JSON.
/// - `V2`: always the container; a checkpoint outside the gate is refused by
///   name.
///
/// Wire rule (design 4.2, revised for the default switch): `Auto` is never
/// serialized, so every config, command and run identity written before the
/// switch keeps its bytes. An explicit `"v1"` or `"v2"` is serialized, so an
/// explicit `"v1"` no longer serializes like absence (before the switch it
/// did, because absence meant v1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointRecordFormatV1 {
    #[default]
    Auto,
    V1,
    V2,
}

impl CheckpointRecordFormatV1 {
    /// The serde skip predicate: absence and an explicit `"auto"` share bytes.
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Whether the container was explicitly demanded (never true for `Auto`).
    pub fn is_v2(&self) -> bool {
        matches!(self, Self::V2)
    }

    /// The format an update writes for `saved`: `V1` or `V2`, never `Auto`.
    /// An explicit `V2` outside the scope gate is an error; `Auto` falls back
    /// to `V1` there.
    pub(crate) fn resolve_v1<T>(self, saved: &ExpandedCheckpointV1<T>) -> Result<Self, String> {
        match self {
            Self::V1 => Ok(Self::V1),
            Self::V2 => admit_writer_v1(saved).map(|()| Self::V2),
            Self::Auto => Ok(if admit_writer_v1(saved).is_ok() {
                Self::V2
            } else {
                Self::V1
            }),
        }
    }
}

/// The v2 writer's scope gate (design 2.1): a whitelist of checkpoint kinds
/// that have passed a v2 identity replay. Today that is the ordinary fresh
/// checkpoint under the V4, FlatV7 or FlatV8 identity, with the terminal or
/// GAE loss, the platform's math, and none of the optional continuation,
/// transfer, distillation or input-contract members. Anything else is
/// refused by name and keeps writing v1 JSON.
pub(crate) fn admit_writer_v1<T>(saved: &ExpandedCheckpointV1<T>) -> Result<(), String> {
    use crate::native_flat_tensorizer_v4::{
        FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4,
    };
    use crate::native_flat_tensorizer_v7::{
        FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7,
    };
    use crate::native_flat_tensorizer_v8::{
        FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8,
    };
    let digests = (
        saved.feature_contract_digest.as_str(),
        saved.feature_encoding_digest.as_str(),
    );
    let identity = [
        (FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4),
        (FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7),
        (FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8),
    ]
    .contains(&digests);
    let loss = saved.loss_identity == "terminal_reinforce_value/v3"
        || saved.loss_identity == super::GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1;
    let plain = saved.e_supported.is_none()
        && saved.soft_q.is_none()
        && saved.search_targets.is_none()
        && saved.ex1.is_none()
        && saved.exploration.is_none()
        && saved.registry_transfer.is_none()
        && saved.archive_distillation.is_none()
        && saved.input_contract.is_none()
        && saved.training_math.is_platform_libm();
    if saved.schema == super::FRESH_CHECKPOINT_SCHEMA && identity && loss && plain {
        Ok(())
    } else {
        Err("the v2 checkpoint writer admits only ordinary fresh V4, FlatV7 or FlatV8 checkpoints with the terminal or GAE loss; this kind has no v2 identity replay and keeps record_format v1".into())
    }
}

/// One tensor's values as the codec reads them: their `f32` bit patterns
/// themselves (a decoded checkpoint), or the `f32` values whose `to_bits`
/// they are (a snapshot being published).
#[derive(Clone, Copy, Debug)]
pub(crate) enum TensorValuesV1<'a> {
    Bits(&'a [u32]),
    Floats(&'a [f32]),
}

impl TensorValuesV1<'_> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Bits(bits) => bits.len(),
            Self::Floats(floats) => floats.len(),
        }
    }
}

/// A checkpoint tensor: its name, its shape and its values' bit patterns,
/// wherever they are held. The codec writes, compares and serializes every
/// implementation the same way, so a [`ParameterBitsV1`] and a
/// [`ParameterBitsRefV1`] of the same tensor publish the same bytes.
pub(crate) trait TensorBitsV1 {
    fn name_v1(&self) -> &str;
    fn shape_v1(&self) -> &[usize];
    fn values_v1(&self) -> TensorValuesV1<'_>;
}

impl TensorBitsV1 for ParameterBitsV1 {
    fn name_v1(&self) -> &str {
        &self.name
    }

    fn shape_v1(&self) -> &[usize] {
        &self.shape
    }

    fn values_v1(&self) -> TensorValuesV1<'_> {
        TensorValuesV1::Bits(&self.values)
    }
}

impl TensorBitsV1 for NativeNamedParameterV1 {
    fn name_v1(&self) -> &str {
        self.name
    }

    fn shape_v1(&self) -> &[usize] {
        &self.shape
    }

    fn values_v1(&self) -> TensorValuesV1<'_> {
        TensorValuesV1::Floats(&self.values)
    }
}

/// A live train state's tensor (`TrainTensorsV1`), read where the state holds
/// it.
impl TensorBitsV1 for NativeNamedTensorRefV1<'_> {
    fn name_v1(&self) -> &str {
        self.name
    }

    fn shape_v1(&self) -> &[usize] {
        &self.shape
    }

    fn values_v1(&self) -> TensorValuesV1<'_> {
        TensorValuesV1::Floats(self.values)
    }
}

/// A tensor as a checkpoint records it, borrowed from a live train state or a
/// snapshot: what `ParameterBitsV1::from_native` copies, without the copy.
/// Its serialization is that `ParameterBitsV1`'s, member for member and value
/// for value.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename = "ParameterBitsV1")]
pub(crate) struct ParameterBitsRefV1<'a> {
    name: &'static str,
    shape: &'a [usize],
    #[serde(serialize_with = "serialize_bits_v1")]
    values: &'a [f32],
}

/// `ParameterBitsV1`'s members, which [`ParameterBitsRefV1`] repeats in
/// order: a member added there fails to compile here until the view has it.
const _: fn(ParameterBitsV1) = |bits| {
    let ParameterBitsV1 {
        name: _,
        shape: _,
        values: _,
    } = bits;
};

/// A snapshot's tensor, as a checkpoint records it.
impl<'a> From<&'a NativeNamedParameterV1> for ParameterBitsRefV1<'a> {
    fn from(tensor: &'a NativeNamedParameterV1) -> Self {
        Self {
            name: tensor.name,
            shape: &tensor.shape,
            values: &tensor.values,
        }
    }
}

/// A live train state's tensor, as a checkpoint records it: the values stay
/// in the state.
impl<'a> From<&'a NativeNamedTensorRefV1<'_>> for ParameterBitsRefV1<'a> {
    fn from(tensor: &'a NativeNamedTensorRefV1<'_>) -> Self {
        Self {
            name: tensor.name,
            shape: &tensor.shape,
            values: tensor.values,
        }
    }
}

impl TensorBitsV1 for ParameterBitsRefV1<'_> {
    fn name_v1(&self) -> &str {
        self.name
    }

    fn shape_v1(&self) -> &[usize] {
        self.shape
    }

    fn values_v1(&self) -> TensorValuesV1<'_> {
        TensorValuesV1::Floats(self.values)
    }
}

/// The values' bit patterns as the sequence of `u32`s a `Vec<u32>` writes.
fn serialize_bits_v1<S: Serializer>(values: &&[f32], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(values.iter().map(|value| value.to_bits()))
}

/// Whether two values hold the same bit patterns. Branch-free within a block
/// so the comparison runs at memory speed.
fn same_bits_v1<L: Copy, R: Copy>(
    left: &[L],
    right: &[R],
    left_bits: impl Fn(L) -> u32,
    right_bits: impl Fn(R) -> u32,
) -> bool {
    const BLOCK: usize = 1024;
    left.len() == right.len()
        && left
            .chunks(BLOCK)
            .zip(right.chunks(BLOCK))
            .all(|(left, right)| {
                left.iter().zip(right).fold(true, |same, (&l, &r)| {
                    same & (left_bits(l) == right_bits(r))
                })
            })
}

fn same_values_v1(left: TensorValuesV1<'_>, right: TensorValuesV1<'_>) -> bool {
    use TensorValuesV1::{Bits, Floats};
    match (left, right) {
        (Bits(left), Bits(right)) => left == right,
        (Floats(left), Floats(right)) => same_bits_v1(left, right, f32::to_bits, f32::to_bits),
        (Floats(floats), Bits(bits)) | (Bits(bits), Floats(floats)) => {
            same_bits_v1(floats, bits, f32::to_bits, |bits| bits)
        }
    }
}

/// Whether two tensor lists hold the same tensors: the same count and, in
/// order, the same names, the same shapes and the same value bits (`to_bits`,
/// so a signed zero or a NaN payload counts), compared where each is held.
/// For a list of snapshot tensors and a list of `ParameterBitsV1`, this is
/// whether `ParameterBitsV1::from_native` of the first equals the second.
pub(crate) fn same_tensors_v1<L: TensorBitsV1, R: TensorBitsV1>(left: &[L], right: &[R]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name_v1() == right.name_v1()
                && left.shape_v1() == right.shape_v1()
                && same_values_v1(left.values_v1(), right.values_v1())
        })
}

/// Whether `bytes` carry the container magic. A v1 JSON checkpoint starts
/// with `{`, so the two never collide.
pub(crate) fn is_container_v1(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC_V1)
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionTableV1 {
    state_format: String,
    sections: Vec<SectionEntryV1>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionEntryV1 {
    section: String,
    ordinal: usize,
    name: String,
    shape: Vec<usize>,
    /// Bytes from the data start.
    offset: u64,
    byte_length: u64,
}

fn align_up_v1(value: usize) -> Result<usize, String> {
    value
        .div_ceil(ALIGN_V1)
        .checked_mul(ALIGN_V1)
        .ok_or_else(|| "checkpoint container size overflows".to_owned())
}

fn element_count_v1(shape: &[usize]) -> Result<usize, String> {
    shape
        .iter()
        .try_fold(1usize, |acc, &dim| acc.checked_mul(dim))
        .ok_or_else(|| "checkpoint container tensor shape overflows".to_owned())
}

fn sections_of_v1<T>(saved: &ExpandedCheckpointV1<T>) -> [&[T]; 3] {
    [
        &saved.parameters,
        &saved.first_moments,
        &saved.second_moments,
    ]
}

/// `saved` with its three tensor sections empty: the container's document.
fn document_of_v1<T>(saved: &ExpandedCheckpointV1<T>) -> ExpandedCheckpointV1 {
    with_tensors_v1(saved, [Vec::new(), Vec::new(), Vec::new()])
}

/// `saved`'s members with `tensors` as its parameters, first moments and
/// second moments. The destructuring names every field, so a field added to
/// the checkpoint fails to compile here until the container carries it.
pub(crate) fn with_tensors_v1<T, U>(
    saved: &ExpandedCheckpointV1<T>,
    [parameters, first_moments, second_moments]: [Vec<U>; 3],
) -> ExpandedCheckpointV1<U> {
    let ExpandedCheckpointV1 {
        schema,
        feature_contract_digest,
        feature_encoding_digest,
        card_db_hash,
        source_import,
        state_sha256,
        e_supported,
        soft_q,
        search_targets,
        ex1,
        adam_step,
        scorer_bias_anchor_bits,
        parameters: _,
        first_moments: _,
        second_moments: _,
        trajectories,
        loss_identity,
        gamma_bits,
        gae_lambda_bits,
        entropy_coefficient_bits,
        exploration,
        learning_rate_bits,
        value_coefficient_bits,
        registry_transfer,
        archive_distillation,
        training_math,
        input_contract,
    } = saved;
    ExpandedCheckpointV1 {
        schema: schema.clone(),
        feature_contract_digest: feature_contract_digest.clone(),
        feature_encoding_digest: feature_encoding_digest.clone(),
        card_db_hash: card_db_hash.clone(),
        source_import: source_import.clone(),
        state_sha256: state_sha256.clone(),
        e_supported: e_supported.clone(),
        soft_q: soft_q.clone(),
        search_targets: search_targets.clone(),
        ex1: ex1.clone(),
        adam_step: *adam_step,
        scorer_bias_anchor_bits: *scorer_bias_anchor_bits,
        parameters,
        first_moments,
        second_moments,
        trajectories: trajectories.clone(),
        loss_identity: loss_identity.clone(),
        gamma_bits: *gamma_bits,
        gae_lambda_bits: *gae_lambda_bits,
        entropy_coefficient_bits: *entropy_coefficient_bits,
        exploration: *exploration,
        learning_rate_bits: *learning_rate_bits,
        value_coefficient_bits: *value_coefficient_bits,
        registry_transfer: registry_transfer.clone(),
        archive_distillation: archive_distillation.clone(),
        training_math: *training_math,
        input_contract: input_contract.clone(),
    }
}

/// Where an encoding goes as it is produced: appended to a buffer, or
/// compared with bytes that claim to be that encoding ([`CompareSinkV1`]).
trait SinkV1 {
    fn put_v1(&mut self, bytes: &[u8]);
    fn zeros_v1(&mut self, count: usize);
    /// Bytes produced so far.
    fn len_v1(&self) -> usize;
}

impl SinkV1 for Vec<u8> {
    fn put_v1(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }

    fn zeros_v1(&mut self, count: usize) {
        self.resize(self.len() + count, 0);
    }

    fn len_v1(&self) -> usize {
        self.len()
    }
}

/// Compares an encoding with `expected` as it is produced: `same` stays true
/// exactly while every byte so far equals `expected`'s byte at its offset.
struct CompareSinkV1<'a> {
    expected: &'a [u8],
    at: usize,
    same: bool,
}

impl SinkV1 for CompareSinkV1<'_> {
    fn put_v1(&mut self, bytes: &[u8]) {
        let end = self.at.saturating_add(bytes.len());
        self.same = self.same && self.expected.get(self.at..end) == Some(bytes);
        self.at = end;
    }

    fn zeros_v1(&mut self, count: usize) {
        let end = self.at.saturating_add(count);
        self.same = self.same && self.expected.get(self.at..end).is_some_and(all_zero_v1);
        self.at = end;
    }

    fn len_v1(&self) -> usize {
        self.at
    }
}

/// Writes each value's little-endian bit pattern, a few thousand at a time.
fn put_values_v1(sink: &mut impl SinkV1, values: TensorValuesV1<'_>) {
    fn put<V: Copy>(sink: &mut impl SinkV1, values: &[V], bits: impl Fn(V) -> u32) {
        const CHUNK_VALUES: usize = 4096;
        let mut buffer = [0u8; CHUNK_VALUES * 4];
        for chunk in values.chunks(CHUNK_VALUES) {
            for (slot, &value) in buffer.chunks_exact_mut(4).zip(chunk) {
                slot.copy_from_slice(&bits(value).to_le_bytes());
            }
            sink.put_v1(&buffer[..chunk.len() * 4]);
        }
    }
    match values {
        TensorValuesV1::Bits(bits) => put(sink, bits, |bits| bits),
        TensorValuesV1::Floats(floats) => put(sink, floats, f32::to_bits),
    }
}

/// Everything of a container's encoding but its tensor values: the document,
/// the table, where each tensor starts and the total length.
struct FrameV1 {
    document: Vec<u8>,
    table: Vec<u8>,
    data_start: usize,
    /// Each tensor's offset from the data start, in section order.
    offsets: Vec<usize>,
    total: usize,
}

/// The frame of `saved`'s container. Refuses a tensor whose value count is
/// not the product of its shape; every refusal of [`encode_v1`] is made here,
/// before anything is written.
fn frame_v1<T: TensorBitsV1>(saved: &ExpandedCheckpointV1<T>) -> Result<FrameV1, String> {
    let document = serde_json::to_vec(&document_of_v1(saved)).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut offset = 0usize;
    for (section, tensors) in SECTION_NAMES_V1.iter().zip(sections_of_v1(saved)) {
        for (ordinal, tensor) in tensors.iter().enumerate() {
            let count = element_count_v1(tensor.shape_v1())?;
            if count != tensor.values_v1().len() {
                return Err("checkpoint container tensor length differs from its shape".into());
            }
            let byte_length = count
                .checked_mul(4)
                .ok_or("checkpoint container tensor size overflows")?;
            entries.push(SectionEntryV1 {
                section: (*section).to_owned(),
                ordinal,
                name: tensor.name_v1().to_owned(),
                shape: tensor.shape_v1().to_vec(),
                offset: offset as u64,
                byte_length: byte_length as u64,
            });
            offset = align_up_v1(
                offset
                    .checked_add(byte_length)
                    .ok_or("checkpoint container size overflows")?,
            )?;
        }
    }
    let data_length = entries
        .last()
        .map_or(0, |last| (last.offset + last.byte_length) as usize);
    let offsets = entries.iter().map(|entry| entry.offset as usize).collect();
    let table = SectionTableV1 {
        state_format: STATE_FORMAT_V1.to_owned(),
        sections: entries,
    };
    let table = serde_json::to_vec(&table).map_err(|e| e.to_string())?;
    let data_start = align_up_v1(HEADER_BYTES_V1 + document.len() + table.len())?;
    let total = data_start
        .checked_add(data_length)
        .ok_or("checkpoint container size overflows")?;
    Ok(FrameV1 {
        document,
        table,
        data_start,
        offsets,
        total,
    })
}

/// Writes `saved`'s container, framed by `frame` (its own [`frame_v1`]),
/// into `sink`: the header, the document, the table, zero padding to the
/// data start, then each tensor at its offset with zero padding between.
fn write_v1<T: TensorBitsV1>(
    saved: &ExpandedCheckpointV1<T>,
    frame: &FrameV1,
    sink: &mut impl SinkV1,
) {
    sink.put_v1(MAGIC_V1);
    sink.put_v1(&CONTAINER_VERSION_V1.to_le_bytes());
    sink.put_v1(&0u32.to_le_bytes());
    sink.put_v1(&(frame.document.len() as u64).to_le_bytes());
    sink.put_v1(&(frame.table.len() as u64).to_le_bytes());
    sink.put_v1(&frame.document);
    sink.put_v1(&frame.table);
    let tensors = sections_of_v1(saved).into_iter().flatten();
    for (tensor, offset) in tensors.zip(&frame.offsets) {
        sink.zeros_v1(frame.data_start + offset - sink.len_v1());
        put_values_v1(sink, tensor.values_v1());
    }
    if frame.offsets.is_empty() {
        sink.zeros_v1(frame.data_start - sink.len_v1());
    }
}

/// Encodes `saved` as a container. Refuses a tensor whose value count is not
/// the product of its shape.
pub(crate) fn encode_v1<T: TensorBitsV1>(
    saved: &ExpandedCheckpointV1<T>,
) -> Result<Vec<u8>, String> {
    let frame = frame_v1(saved)?;
    let mut bytes = Vec::with_capacity(frame.total);
    write_v1(saved, &frame, &mut bytes);
    debug_assert_eq!(bytes.len(), frame.total);
    Ok(bytes)
}

/// Whether `bytes` are exactly [`encode_v1`]'s encoding of `saved`: the
/// encoding is compared with them as it is produced, never held. Refuses
/// what [`encode_v1`] refuses.
fn encodes_to_v1<T: TensorBitsV1>(
    saved: &ExpandedCheckpointV1<T>,
    bytes: &[u8],
) -> Result<bool, String> {
    let frame = frame_v1(saved)?;
    if frame.total != bytes.len() {
        return Ok(false);
    }
    let mut sink = CompareSinkV1 {
        expected: bytes,
        at: 0,
        same: true,
    };
    write_v1(saved, &frame, &mut sink);
    Ok(sink.same && sink.at == bytes.len())
}

struct HeaderV1 {
    document: std::ops::Range<usize>,
    table: std::ops::Range<usize>,
    data_start: usize,
}

fn read_u32_v1(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

fn read_u64_v1(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"))
}

fn header_v1(bytes: &[u8]) -> Result<HeaderV1, String> {
    if bytes.len() < HEADER_BYTES_V1 || !is_container_v1(bytes) {
        return Err("not a checkpoint container".into());
    }
    if read_u32_v1(bytes, 8) != CONTAINER_VERSION_V1 || read_u32_v1(bytes, 12) != 0 {
        return Err("checkpoint container version differs".into());
    }
    let document_length = usize::try_from(read_u64_v1(bytes, 16))
        .map_err(|_| "checkpoint container document length overflows")?;
    let table_length = usize::try_from(read_u64_v1(bytes, 24))
        .map_err(|_| "checkpoint container table length overflows")?;
    let document_end = HEADER_BYTES_V1
        .checked_add(document_length)
        .ok_or("checkpoint container document length overflows")?;
    let table_end = document_end
        .checked_add(table_length)
        .ok_or("checkpoint container table length overflows")?;
    let data_start = align_up_v1(table_end)?;
    if data_start > bytes.len() {
        return Err("checkpoint container is truncated".into());
    }
    Ok(HeaderV1 {
        document: HEADER_BYTES_V1..document_end,
        table: document_end..table_end,
        data_start,
    })
}

/// A container whose structure has been checked: everything except the
/// tensor values themselves, which are data.
struct CheckedV1 {
    document: std::ops::Range<usize>,
    saved: ExpandedCheckpointV1,
    sections: Vec<(SectionEntryV1, std::ops::Range<usize>)>,
}

fn all_zero_v1(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0)
}

/// Checks a container's structure without reading tensor values: header,
/// canonical document (a checkpoint with empty tensor lists) and table,
/// sections in order, aligned, contiguous and inside the file, every padding
/// byte zero, and the file ending after the last tensor. Together with the
/// values this is exactly the canonical encoding [`encode_v1`] writes.
fn check_structure_v1(bytes: &[u8]) -> Result<CheckedV1, String> {
    let header = header_v1(bytes)?;
    let document = &bytes[header.document.clone()];
    let saved: ExpandedCheckpointV1 =
        serde_json::from_slice(document).map_err(|e| e.to_string())?;
    if !(saved.parameters.is_empty()
        && saved.first_moments.is_empty()
        && saved.second_moments.is_empty())
    {
        return Err("checkpoint container document carries tensor values".into());
    }
    if serde_json::to_vec(&saved).map_err(|e| e.to_string())? != document {
        return Err("checkpoint container is not in canonical form".into());
    }
    let table_bytes = &bytes[header.table.clone()];
    let table: SectionTableV1 = serde_json::from_slice(table_bytes).map_err(|e| e.to_string())?;
    if table.state_format != STATE_FORMAT_V1 {
        return Err("checkpoint container state format differs".into());
    }
    if serde_json::to_vec(&table).map_err(|e| e.to_string())? != table_bytes {
        return Err("checkpoint container is not in canonical form".into());
    }
    if !all_zero_v1(&bytes[header.table.end..header.data_start]) {
        return Err("checkpoint container is not in canonical form".into());
    }
    let data = &bytes[header.data_start..];
    let mut expected_offset = 0usize;
    let mut data_end = 0usize;
    let mut next_section = 0usize;
    let mut next_ordinal = 0usize;
    let mut sections = Vec::with_capacity(table.sections.len());
    for entry in table.sections {
        while SECTION_NAMES_V1.get(next_section) != Some(&entry.section.as_str()) {
            next_section += 1;
            next_ordinal = 0;
            if next_section >= SECTION_NAMES_V1.len() {
                return Err("checkpoint container sections are out of order".into());
            }
        }
        let count = element_count_v1(&entry.shape)?;
        let byte_length = count
            .checked_mul(4)
            .ok_or("checkpoint container tensor size overflows")?;
        if entry.ordinal != next_ordinal
            || entry.offset != expected_offset as u64
            || entry.byte_length != byte_length as u64
        {
            return Err("checkpoint container section layout differs".into());
        }
        let end = expected_offset
            .checked_add(byte_length)
            .ok_or("checkpoint container size overflows")?;
        if end > data.len() || !all_zero_v1(&data[data_end..expected_offset]) {
            return Err("checkpoint container section layout differs".into());
        }
        sections.push((entry, expected_offset..end));
        next_ordinal += 1;
        data_end = end;
        expected_offset = align_up_v1(end)?;
    }
    if data_end != data.len() {
        return Err("checkpoint container length differs from its sections".into());
    }
    let shift = header.data_start;
    Ok(CheckedV1 {
        document: header.document,
        saved,
        sections: sections
            .into_iter()
            .map(|(entry, range)| (entry, range.start + shift..range.end + shift))
            .collect(),
    })
}

/// The container's JSON document (the checkpoint with its three tensor
/// lists empty), for probes that read only metadata fields. The whole
/// structure is checked first, so a malformed or truncated container is
/// refused here as it is by a full load.
pub(crate) fn document_v1(bytes: &[u8]) -> Result<&[u8], String> {
    let checked = check_structure_v1(bytes)?;
    Ok(&bytes[checked.document])
}

/// `bytes` themselves for a v1 JSON checkpoint, or the container's JSON
/// document: the view a metadata probe parses. A sealed checkpoint
/// (`record_envelope_v1`) is opened first.
pub(crate) fn metadata_view_v1(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    match open_record_v1(bytes)? {
        Cow::Borrowed(bytes) if is_container_v1(bytes) => document_v1(bytes).map(Cow::Borrowed),
        Cow::Borrowed(bytes) => Ok(Cow::Borrowed(bytes)),
        Cow::Owned(bytes) if is_container_v1(&bytes) => {
            Ok(Cow::Owned(document_v1(&bytes)?.to_vec()))
        }
        Cow::Owned(bytes) => Ok(Cow::Owned(bytes)),
    }
}

/// The record bytes of a checkpoint file's bytes: a sealed file
/// (`record_envelope_v1`) opened, anything else as it is.
fn open_record_v1(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    super::record_envelope_v1::open_v1(bytes, super::MAX_FILE_BYTES)
}

/// Decodes a container into the checkpoint it encodes. Refuses anything that
/// is not the canonical encoding of what it decodes to (see
/// [`check_structure_v1`]); as a second guard the decoded checkpoint is
/// re-encoded and compared byte for byte, as the re-encoding is produced
/// ([`encodes_to_v1`]), without holding a copy of it.
pub(crate) fn decode_v1(bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    let saved = decode_sections_v1(bytes)?;
    if !encodes_to_v1(&saved, bytes)? {
        return Err("checkpoint container is not in canonical form".into());
    }
    Ok(saved)
}

/// The checkpoint a container's checked structure and tensor values make,
/// before [`decode_v1`]'s second guard.
fn decode_sections_v1(bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    let CheckedV1 {
        mut saved,
        sections,
        ..
    } = check_structure_v1(bytes)?;
    for (entry, range) in sections {
        let values = bytes[range]
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("four bytes")))
            .collect();
        let tensor = ParameterBitsV1 {
            name: entry.name,
            shape: entry.shape,
            values,
        };
        match entry.section.as_str() {
            "parameters" => saved.parameters.push(tensor),
            "first_moments" => saved.first_moments.push(tensor),
            _ => saved.second_moments.push(tensor),
        }
    }
    Ok(saved)
}

/// The v1 JSON bytes of the checkpoint a container encodes: exactly what
/// `publish_json` writes for that checkpoint.
pub(crate) fn export_v1(bytes: &[u8]) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&decode_v1(bytes)?).map_err(|e| e.to_string())
}

/// `bytes` themselves for a v1 JSON checkpoint, or the exported v1 JSON of a
/// container: the JSON-compatibility view (design 4.1, interface 2).
/// A sealed file (`record_envelope_v1`) is opened first, so this is also the
/// JSON view of any sealed JSON record.
pub(crate) fn json_document_v1(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    let record = open_record_v1(bytes)?;
    if is_container_v1(&record) {
        export_v1(&record).map(Cow::Owned)
    } else {
        Ok(record)
    }
}

/// The checkpoint either format encodes (design 4.1, interface 3), sealed
/// (`record_envelope_v1`) or not.
pub(crate) fn parse_checkpoint_v1(bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    let record = open_record_v1(bytes)?;
    if is_container_v1(&record) {
        decode_v1(&record)
    } else {
        serde_json::from_slice(&record).map_err(|e| e.to_string())
    }
}

/// Converter admission and round trip (design section 5): `v1` must be the
/// canonical serialization of the checkpoint it parses to, and the container
/// must decode to that checkpoint and export to exactly `v1`. Returns the
/// container bytes.
pub(crate) fn convert_v1_bytes(v1: &[u8]) -> Result<Vec<u8>, String> {
    if is_container_v1(v1) {
        return Err("the input is already a checkpoint container".into());
    }
    let saved: ExpandedCheckpointV1 = serde_json::from_slice(v1).map_err(|e| e.to_string())?;
    if serde_json::to_vec(&saved).map_err(|e| e.to_string())? != v1 {
        return Err("v1 checkpoint is not in canonical form; keep it as v1".into());
    }
    let container = encode_v1(&saved)?;
    if export_v1(&container)? != v1 {
        return Err("checkpoint container does not export to its v1 checkpoint".into());
    }
    Ok(container)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod update_tests;
