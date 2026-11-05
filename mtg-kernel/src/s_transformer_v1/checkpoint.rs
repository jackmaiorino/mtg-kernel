//! S's binary train-state checkpoint, in the framing of checkpoint container
//! v1 (mtg-kernel#468, `expanded_deck_training_v1/checkpoint_container_v1`):
//!
//! ```text
//! offset 0   magic             8 bytes  "MTGCKPT\0"
//! offset 8   container_version u32      = 1
//! offset 12  reserved          u32      = 0
//! offset 16  document_length   u64
//! offset 24  table_length      u64
//! offset 32  document, then table, then zero padding to the data start, the
//!            first multiple of 256 after the table
//! data       parameters, first moments, second moments: each tensor's f32
//!            bits little-endian, each starting at a multiple of 256 from the
//!            data start, zero padded between tensors
//! ```
//!
//! The section table has #468's schema (`mtgts-sections-v1`). The document is
//! S's own (`s-transformer-train-state-v1`): architecture, config, Adam step
//! and the state hash. Until #468 lands this module is the interim writer; a
//! #468 reader would admit this document schema as a new kind. A state has
//! one encoding: the decoder re-encodes and refuses unless the bytes match,
//! and refuses unless the recomputed state hash equals the document's.
//!
//! **Weights snapshot** (W5 design section 2.2, ruling 2): the same framing
//! with the parameters section alone and the document schema
//! `w5-weights-snapshot/v1:s-transformer-train-state-v1`, which carries the
//! state hash it was cut from, the parameter hash and the S checkpoint record
//! it was cut from. [`decode_checkpoint_v1`] refuses it, so a snapshot is
//! never a train state; [`decode_weights_snapshot_v1`] admits only its
//! canonical encoding.

use super::{
    LayoutV1, SConfigV1, SModelV1, STrainStateV1, S_ARCHITECTURE_V1, S_TRAIN_STATE_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAGIC_V1: &[u8; 8] = b"MTGCKPT\0";
const CONTAINER_VERSION_V1: u32 = 1;
const HEADER_BYTES_V1: usize = 32;
const ALIGN_V1: usize = 256;
const STATE_FORMAT_V1: &str = "mtgts-sections-v1";
const SECTION_NAMES_V1: [&str; 3] = ["parameters", "first_moments", "second_moments"];
/// A weights snapshot's document schema.
pub const S_WEIGHTS_SNAPSHOT_SCHEMA_V1: &str =
    "w5-weights-snapshot/v1:s-transformer-train-state-v1";
const SNAPSHOT_SECTIONS_V1: [&str; 1] = ["parameters"];

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentV1 {
    schema: String,
    architecture: String,
    config: SConfigV1,
    adam_step: u64,
    state_sha256: String,
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

fn align_up_v1(value: usize) -> usize {
    value.div_ceil(ALIGN_V1) * ALIGN_V1
}

/// A weights snapshot's document.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotDocumentV1 {
    schema: String,
    architecture: String,
    config: SConfigV1,
    adam_step: u64,
    /// The full state the snapshot was cut from.
    cut_from_state_sha256: String,
    parameters_sha256: String,
    /// The S checkpoint record the snapshot was cut from.
    record: Value,
}

/// A decoded weights snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct SWeightsSnapshotV1 {
    pub model: SModelV1,
    pub adam_step: u64,
    pub cut_from_state_sha256: String,
    pub record: Value,
}

fn table_of_v1(layout: &LayoutV1) -> SectionTableV1 {
    table_of_sections_v1(layout, &SECTION_NAMES_V1)
}

fn table_of_sections_v1(layout: &LayoutV1, names: &[&str]) -> SectionTableV1 {
    let mut sections = Vec::new();
    let mut cursor = 0usize;
    for &section in names {
        for (ordinal, spec) in layout.tensors.iter().enumerate() {
            cursor = align_up_v1(cursor);
            sections.push(SectionEntryV1 {
                section: section.to_owned(),
                ordinal,
                name: spec.name.clone(),
                shape: spec.shape.clone(),
                offset: cursor as u64,
                byte_length: (spec.len * 4) as u64,
            });
            cursor += spec.len * 4;
        }
    }
    SectionTableV1 {
        state_format: STATE_FORMAT_V1.to_owned(),
        sections,
    }
}

/// Encodes a train state as a container.
pub fn encode_checkpoint_v1(state: &STrainStateV1) -> Vec<u8> {
    let layout = state.model.layout_v1();
    let document = serde_json::to_vec(&DocumentV1 {
        schema: S_TRAIN_STATE_SCHEMA_V1.to_owned(),
        architecture: S_ARCHITECTURE_V1.to_owned(),
        config: layout.config,
        adam_step: state.adam_step,
        state_sha256: state.state_sha256_v1(),
    })
    .expect("document serializes");
    let table = table_of_v1(layout);
    let table_bytes = serde_json::to_vec(&table).expect("table serializes");
    let data_start = align_up_v1(HEADER_BYTES_V1 + document.len() + table_bytes.len());
    let mut out =
        Vec::with_capacity(data_start + 3 * layout.total * 4 + 3 * layout.tensors.len() * ALIGN_V1);
    out.extend_from_slice(MAGIC_V1);
    out.extend_from_slice(&CONTAINER_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(document.len() as u64).to_le_bytes());
    out.extend_from_slice(&(table_bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(&document);
    out.extend_from_slice(&table_bytes);
    out.resize(data_start, 0);
    let data = [
        state.model.values_v1(),
        state.first.as_slice(),
        state.second.as_slice(),
    ];
    for entry in &table.sections {
        let section = SECTION_NAMES_V1
            .iter()
            .position(|name| *name == entry.section)
            .expect("known section");
        let spec = &layout.tensors[entry.ordinal];
        out.resize(data_start + entry.offset as usize, 0);
        for value in &data[section][spec.offset..spec.offset + spec.len] {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    out
}

/// The container bytes of `document`, `table` and each table entry's
/// tensor from `data` (indexed like `names`).
fn frame_v1(
    layout: &LayoutV1,
    document: &[u8],
    table: &SectionTableV1,
    names: &[&str],
    data: &[&[f32]],
) -> Vec<u8> {
    let table_bytes = serde_json::to_vec(table).expect("table serializes");
    let data_start = align_up_v1(HEADER_BYTES_V1 + document.len() + table_bytes.len());
    let mut out = Vec::with_capacity(
        data_start + names.len() * (layout.total * 4 + layout.tensors.len() * ALIGN_V1),
    );
    out.extend_from_slice(MAGIC_V1);
    out.extend_from_slice(&CONTAINER_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(document.len() as u64).to_le_bytes());
    out.extend_from_slice(&(table_bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(document);
    out.extend_from_slice(&table_bytes);
    out.resize(data_start, 0);
    for entry in &table.sections {
        let section = names
            .iter()
            .position(|name| *name == entry.section)
            .expect("known section");
        let spec = &layout.tensors[entry.ordinal];
        out.resize(data_start + entry.offset as usize, 0);
        for value in &data[section][spec.offset..spec.offset + spec.len] {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    out
}

fn snapshot_document_v1(
    config: SConfigV1,
    adam_step: u64,
    cut_from_state_sha256: &str,
    parameters_sha256: &str,
    record: &Value,
) -> Vec<u8> {
    serde_json::to_vec(&SnapshotDocumentV1 {
        schema: S_WEIGHTS_SNAPSHOT_SCHEMA_V1.to_owned(),
        architecture: S_ARCHITECTURE_V1.to_owned(),
        config,
        adam_step,
        cut_from_state_sha256: cut_from_state_sha256.to_owned(),
        parameters_sha256: parameters_sha256.to_owned(),
        record: record.clone(),
    })
    .expect("document serializes")
}

/// Encodes the weights snapshot of `state`, cut from the S checkpoint
/// `record` (whose state it is).
pub fn encode_weights_snapshot_v1(state: &STrainStateV1, record: &Value) -> Vec<u8> {
    let layout = state.model.layout_v1();
    let document = snapshot_document_v1(
        layout.config,
        state.adam_step,
        &state.state_sha256_v1(),
        &state.model.parameter_sha256_v1(),
        record,
    );
    let table = table_of_sections_v1(layout, &SNAPSHOT_SECTIONS_V1);
    frame_v1(
        layout,
        &document,
        &table,
        &SNAPSHOT_SECTIONS_V1,
        &[state.model.values_v1()],
    )
}

/// The container's document and table bytes after the magic, version and
/// extent checks, and its data start.
fn split_v1(bytes: &[u8]) -> Result<(&[u8], &[u8], usize), String> {
    if bytes.len() < HEADER_BYTES_V1 || &bytes[..8] != MAGIC_V1 {
        return Err("not an S checkpoint container".into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
    let reserved = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION_V1 || reserved != 0 {
        return Err("S checkpoint container version or reserved field differs".into());
    }
    let document_length = usize::try_from(read_u64_v1(bytes, 16)?).map_err(|e| e.to_string())?;
    let table_length = usize::try_from(read_u64_v1(bytes, 24)?).map_err(|e| e.to_string())?;
    let document_end = HEADER_BYTES_V1
        .checked_add(document_length)
        .ok_or("S checkpoint document length overflows")?;
    let table_end = document_end
        .checked_add(table_length)
        .ok_or("S checkpoint table length overflows")?;
    if table_end > bytes.len() {
        return Err("S checkpoint is truncated".into());
    }
    Ok((
        &bytes[HEADER_BYTES_V1..document_end],
        &bytes[document_end..table_end],
        align_up_v1(table_end),
    ))
}

/// Decodes a weights snapshot written by [`encode_weights_snapshot_v1`],
/// refusing any other encoding and any parameters whose hash differs.
pub fn decode_weights_snapshot_v1(bytes: &[u8]) -> Result<SWeightsSnapshotV1, String> {
    let (document_bytes, table_bytes, data_start) = split_v1(bytes)?;
    let document: SnapshotDocumentV1 = serde_json::from_slice(document_bytes)
        .map_err(|e| format!("not an S weights snapshot: {e}"))?;
    if document.schema != S_WEIGHTS_SNAPSHOT_SCHEMA_V1 || document.architecture != S_ARCHITECTURE_V1
    {
        return Err("not an S weights snapshot".into());
    }
    let layout = LayoutV1::of_v1(document.config)?;
    let table: SectionTableV1 = serde_json::from_slice(table_bytes).map_err(|e| e.to_string())?;
    if table != table_of_sections_v1(&layout, &SNAPSHOT_SECTIONS_V1) {
        return Err("S weights snapshot section table differs from the config's layout".into());
    }
    let last = table
        .sections
        .last()
        .ok_or("S weights snapshot has no sections")?;
    let data_end = (data_start as u64)
        .checked_add(last.offset)
        .and_then(|end| end.checked_add(last.byte_length))
        .ok_or("S weights snapshot extent overflows")?;
    if data_end != bytes.len() as u64 {
        return Err("S weights snapshot length differs from its section table".into());
    }
    let mut values = vec![0.0f32; layout.total];
    for entry in &table.sections {
        let spec = &layout.tensors[entry.ordinal];
        let start = data_start + entry.offset as usize;
        let raw = bytes
            .get(start..start + spec.len * 4)
            .ok_or("S weights snapshot tensor is truncated")?;
        for (slot, chunk) in values[spec.offset..spec.offset + spec.len]
            .iter_mut()
            .zip(raw.chunks_exact(4))
        {
            *slot = f32::from_bits(u32::from_le_bytes(chunk.try_into().expect("4 bytes")));
        }
    }
    let model = SModelV1::from_values_v1(document.config, values)?;
    if model.parameter_sha256_v1() != document.parameters_sha256 {
        return Err("S weights snapshot parameter hash differs from its document".into());
    }
    // One encoding per snapshot.
    let canonical = frame_v1(
        &layout,
        &snapshot_document_v1(
            document.config,
            document.adam_step,
            &document.cut_from_state_sha256,
            &document.parameters_sha256,
            &document.record,
        ),
        &table,
        &SNAPSHOT_SECTIONS_V1,
        &[model.values_v1()],
    );
    if canonical != bytes {
        return Err("S weights snapshot is not the canonical encoding of its parameters".into());
    }
    Ok(SWeightsSnapshotV1 {
        model,
        adam_step: document.adam_step,
        cut_from_state_sha256: document.cut_from_state_sha256,
        record: document.record,
    })
}

fn read_u64_v1(bytes: &[u8], at: usize) -> Result<u64, String> {
    bytes
        .get(at..at + 8)
        .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")))
        .ok_or_else(|| "S checkpoint header is truncated".to_owned())
}

/// Decodes a container written by [`encode_checkpoint_v1`], refusing any
/// other encoding of the state and any state whose hash differs.
pub fn decode_checkpoint_v1(bytes: &[u8]) -> Result<STrainStateV1, String> {
    if bytes.len() < HEADER_BYTES_V1 || &bytes[..8] != MAGIC_V1 {
        return Err("not an S checkpoint container".into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
    let reserved = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION_V1 || reserved != 0 {
        return Err("S checkpoint container version or reserved field differs".into());
    }
    let document_length = usize::try_from(read_u64_v1(bytes, 16)?).map_err(|e| e.to_string())?;
    let table_length = usize::try_from(read_u64_v1(bytes, 24)?).map_err(|e| e.to_string())?;
    let document_end = HEADER_BYTES_V1
        .checked_add(document_length)
        .ok_or("S checkpoint document length overflows")?;
    let table_end = document_end
        .checked_add(table_length)
        .ok_or("S checkpoint table length overflows")?;
    if table_end > bytes.len() {
        return Err("S checkpoint is truncated".into());
    }
    let document: DocumentV1 =
        serde_json::from_slice(&bytes[HEADER_BYTES_V1..document_end]).map_err(|e| e.to_string())?;
    if document.schema != S_TRAIN_STATE_SCHEMA_V1 || document.architecture != S_ARCHITECTURE_V1 {
        return Err("S checkpoint schema or architecture differs".into());
    }
    let layout = LayoutV1::of_v1(document.config)?;
    let table: SectionTableV1 =
        serde_json::from_slice(&bytes[document_end..table_end]).map_err(|e| e.to_string())?;
    if table != table_of_v1(&layout) {
        return Err("S checkpoint section table differs from the config's layout".into());
    }
    let data_start = align_up_v1(table_end);
    // The canonical file ends exactly after its last tensor: check the
    // extent before allocating anything the config sizes.
    let last = table
        .sections
        .last()
        .ok_or("S checkpoint has no sections")?;
    let data_end = (data_start as u64)
        .checked_add(last.offset)
        .and_then(|end| end.checked_add(last.byte_length))
        .ok_or("S checkpoint extent overflows")?;
    if data_end != bytes.len() as u64 {
        return Err("S checkpoint length differs from its section table".into());
    }
    let mut data = [
        vec![0.0f32; layout.total],
        vec![0.0f32; layout.total],
        vec![0.0f32; layout.total],
    ];
    for entry in &table.sections {
        let section = SECTION_NAMES_V1
            .iter()
            .position(|name| *name == entry.section)
            .ok_or("unknown S checkpoint section")?;
        let spec = &layout.tensors[entry.ordinal];
        let start = data_start + entry.offset as usize;
        let end = start + spec.len * 4;
        let raw = bytes
            .get(start..end)
            .ok_or("S checkpoint tensor is truncated")?;
        for (slot, chunk) in data[section][spec.offset..spec.offset + spec.len]
            .iter_mut()
            .zip(raw.chunks_exact(4))
        {
            *slot = f32::from_bits(u32::from_le_bytes(chunk.try_into().expect("4 bytes")));
        }
    }
    let [values, first, second] = data;
    let model = SModelV1::from_values_v1(document.config, values)?;
    let state = STrainStateV1::from_parts_v1(model, first, second, document.adam_step)?;
    if state.state_sha256_v1() != document.state_sha256 {
        return Err("S checkpoint state hash differs from its document".into());
    }
    if encode_checkpoint_v1(&state) != bytes {
        return Err("S checkpoint is not the canonical encoding of its state".into());
    }
    Ok(state)
}
