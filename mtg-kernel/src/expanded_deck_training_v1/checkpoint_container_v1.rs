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

use super::{ExpandedCheckpointV1, ParameterBitsV1};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// The container's first eight bytes.
pub(crate) const MAGIC_V1: &[u8; 8] = b"MTGCKPT\0";
const CONTAINER_VERSION_V1: u32 = 1;
const HEADER_BYTES_V1: usize = 32;
const ALIGN_V1: usize = 256;
/// The section table's `state_format`.
pub(crate) const STATE_FORMAT_V1: &str = "mtgts-sections-v1";
const SECTION_NAMES_V1: [&str; 3] = ["parameters", "first_moments", "second_moments"];

/// How an update publishes its checkpoint: v1 JSON (the default) or the
/// container. A run config and an update command omit the field for v1, so
/// every existing config, command and run identity keeps its bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointRecordFormatV1 {
    #[default]
    V1,
    V2,
}

impl CheckpointRecordFormatV1 {
    /// The serde skip predicate: an explicit `"v1"` serializes like absence.
    pub fn is_v1(&self) -> bool {
        matches!(self, Self::V1)
    }
}

/// The v2 writer's scope gate (design 2.1): a whitelist of checkpoint kinds
/// that have passed a v2 identity replay. Today that is the ordinary fresh
/// checkpoint under the V4, FlatV7 or FlatV8 identity, with the terminal or
/// GAE loss, the platform's math, and none of the optional continuation,
/// transfer, distillation or input-contract members. Anything else is
/// refused by name and keeps writing v1 JSON.
pub(crate) fn admit_writer_v1(saved: &ExpandedCheckpointV1) -> Result<(), String> {
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

fn sections_of_v1(saved: &ExpandedCheckpointV1) -> [&[ParameterBitsV1]; 3] {
    [
        &saved.parameters,
        &saved.first_moments,
        &saved.second_moments,
    ]
}

/// `saved` with its three tensor sections empty: the container's document.
/// The destructuring names every field, so a field added to the checkpoint
/// fails to compile here until the container carries it.
fn document_of_v1(saved: &ExpandedCheckpointV1) -> ExpandedCheckpointV1 {
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
        parameters: Vec::new(),
        first_moments: Vec::new(),
        second_moments: Vec::new(),
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

/// Encodes `saved` as a container. Refuses a tensor whose value count is not
/// the product of its shape.
pub(crate) fn encode_v1(saved: &ExpandedCheckpointV1) -> Result<Vec<u8>, String> {
    let document = serde_json::to_vec(&document_of_v1(saved)).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut offset = 0usize;
    for (section, tensors) in SECTION_NAMES_V1.iter().zip(sections_of_v1(saved)) {
        for (ordinal, tensor) in tensors.iter().enumerate() {
            let count = element_count_v1(&tensor.shape)?;
            if count != tensor.values.len() {
                return Err("checkpoint container tensor length differs from its shape".into());
            }
            let byte_length = count
                .checked_mul(4)
                .ok_or("checkpoint container tensor size overflows")?;
            entries.push(SectionEntryV1 {
                section: (*section).to_owned(),
                ordinal,
                name: tensor.name.clone(),
                shape: tensor.shape.clone(),
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
    let table = SectionTableV1 {
        state_format: STATE_FORMAT_V1.to_owned(),
        sections: entries,
    };
    let table = serde_json::to_vec(&table).map_err(|e| e.to_string())?;
    let data_start = align_up_v1(HEADER_BYTES_V1 + document.len() + table.len())?;
    let total = data_start
        .checked_add(data_length)
        .ok_or("checkpoint container size overflows")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(MAGIC_V1);
    bytes.extend_from_slice(&CONTAINER_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(document.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(table.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&document);
    bytes.extend_from_slice(&table);
    bytes.resize(data_start, 0);
    for tensors in sections_of_v1(saved) {
        for tensor in tensors {
            let start = align_up_v1(bytes.len() - data_start)? + data_start;
            bytes.resize(start, 0);
            for value in &tensor.values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    debug_assert_eq!(bytes.len(), total);
    Ok(bytes)
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

/// The container's JSON document (the checkpoint with its three tensor
/// sections empty), for probes that read only metadata fields. Checks the
/// header only; a load goes through [`decode_v1`].
pub(crate) fn document_v1(bytes: &[u8]) -> Result<&[u8], String> {
    let header = header_v1(bytes)?;
    Ok(&bytes[header.document])
}

/// `bytes` themselves for a v1 JSON checkpoint, or the container's JSON
/// document: the view a metadata probe parses.
pub(crate) fn metadata_view_v1(bytes: &[u8]) -> Result<&[u8], String> {
    if is_container_v1(bytes) {
        document_v1(bytes)
    } else {
        Ok(bytes)
    }
}

/// Decodes a container into the checkpoint it encodes. Refuses anything that
/// is not the canonical encoding of what it decodes to: bad magic, version or
/// lengths, a document that is not a checkpoint with empty tensor sections, a
/// section table out of order, misaligned, overlapping or not covering the
/// file, nonzero padding, or any byte that re-encoding does not reproduce.
pub(crate) fn decode_v1(bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    let header = header_v1(bytes)?;
    let mut saved: ExpandedCheckpointV1 =
        serde_json::from_slice(&bytes[header.document.clone()]).map_err(|e| e.to_string())?;
    if !(saved.parameters.is_empty()
        && saved.first_moments.is_empty()
        && saved.second_moments.is_empty())
    {
        return Err("checkpoint container document carries tensor values".into());
    }
    let table: SectionTableV1 =
        serde_json::from_slice(&bytes[header.table.clone()]).map_err(|e| e.to_string())?;
    if table.state_format != STATE_FORMAT_V1 {
        return Err("checkpoint container state format differs".into());
    }
    let data = &bytes[header.data_start..];
    let mut expected_offset = 0usize;
    let mut next_section = 0usize;
    let mut next_ordinal = 0usize;
    for entry in &table.sections {
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
        let raw = data
            .get(expected_offset..end)
            .ok_or("checkpoint container is truncated")?;
        let values = raw
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("four bytes")))
            .collect();
        let tensor = ParameterBitsV1 {
            name: entry.name.clone(),
            shape: entry.shape.clone(),
            values,
        };
        match next_section {
            0 => saved.parameters.push(tensor),
            1 => saved.first_moments.push(tensor),
            _ => saved.second_moments.push(tensor),
        }
        next_ordinal += 1;
        expected_offset = align_up_v1(end)?;
    }
    // Canonical form: one encoding per state. This also checks every padding
    // byte and that the file ends after the last tensor.
    if encode_v1(&saved)? != bytes {
        return Err("checkpoint container is not in canonical form".into());
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
pub(crate) fn json_document_v1(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if is_container_v1(bytes) {
        export_v1(bytes).map(Cow::Owned)
    } else {
        Ok(Cow::Borrowed(bytes))
    }
}

/// The checkpoint either format encodes (design 4.1, interface 3).
pub(crate) fn parse_checkpoint_v1(bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    if is_container_v1(bytes) {
        decode_v1(bytes)
    } else {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
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
