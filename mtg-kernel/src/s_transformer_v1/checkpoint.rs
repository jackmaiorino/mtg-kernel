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

use super::{
    LayoutV1, SConfigV1, SModelV1, STrainStateV1, S_ARCHITECTURE_V1, S_TRAIN_STATE_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};

const MAGIC_V1: &[u8; 8] = b"MTGCKPT\0";
const CONTAINER_VERSION_V1: u32 = 1;
const HEADER_BYTES_V1: usize = 32;
const ALIGN_V1: usize = 256;
const STATE_FORMAT_V1: &str = "mtgts-sections-v1";
const SECTION_NAMES_V1: [&str; 3] = ["parameters", "first_moments", "second_moments"];

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

fn table_of_v1(layout: &LayoutV1) -> SectionTableV1 {
    let mut sections = Vec::new();
    let mut cursor = 0usize;
    for section in SECTION_NAMES_V1 {
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
