//! Binary columnar trajectory container v1 (`MTGTRAJ1`, issue #910):
//! compact little-endian binary encoding for [`ExpandedTrajectoryV1`] with an
//! uncompressed front metadata header for O(1) schedule validation and an
//! optionally zstd1-compressed, sparse-float columnar decisions payload.
//!
//! # Layout (all integers little-endian)
//!
//! ```text
//! offset  0   magic             [u8; 8]   "MTGTRAJ1"
//! offset  8   version           u32       1
//! offset 12   flags             u32       bit 0: OPPONENT_TENSORS_OMITTED
//!                                         bit 1: PAYLOAD_ZSTD1
//!                                         bits 2..31: reserved (must be 0)
//! offset 16   header_json_len   u64       byte length of canonical JSON header
//!                                         (all ExpandedTrajectoryV1 fields except `decisions`)
//! offset 24   body_raw_len      u64       uncompressed byte length of binary decisions body
//! offset 32   body_stored_len   u64       stored byte length of binary decisions body
//! offset 40   body_sha256       [u8; 32]  SHA-256 of uncompressed binary decisions body
//! offset 72   header_json       [u8; header_json_len]
//! offset 72+H body_bytes        [u8; body_stored_len]
//! ```
//!
//! # Key properties
//! - **Auto-detection:** Magic `MTGTRAJ1` never collides with JSON (`{`) or
//!   `record_envelope_v1` zstd (`28 b5 2f fd`). Every reader accepts legacy
//!   JSON, zstd1-sealed JSON, and `MTGTRAJ1` seamlessly.
//! - **O(1) schedule validation:** `header_json` (~1–2 KiB) sits uncompressed
//!   at offset 72 even when `PAYLOAD_ZSTD1` is set, so collection validation and
//!   pooled merge check episode/sampler/math/contract without decompressing or
//!   parsing decision tensors.
//! - **Bit-exact float encoding:** Float-bit arrays (`Vec<u32>`) preserve every
//!   bit including `-0.0` (`0x8000_0000`) and NaN payloads; arrays with >50%
//!   exact `+0.0` (`0x0000_0000`) bits use sparse `(index, bits)` encoding.
//! - **Compact integer indices:** `Vec<i64>` index arrays store `i32` LE when
//!   all elements fit in `i32` (4 bytes vs 8 bytes), falling back to `i64` LE
//!   losslessly otherwise.
//! - **Opt-in default-off wire:** [`TrajectoryRecordFormatV1::Json`] is the
//!   default and is never serialized into configs, commands, or receipts, so
//!   all existing run identities and archived files keep their exact bytes.

use super::trajectory_parse_ahead_v1::CollectionTrajectoryFieldsV1;
use super::{
    ensure, err, no_state_digest_v1, DecisionRecordV1, ExpandedEpisodeV1, ExpandedSeatBehaviorV1,
    ExpandedTrajectoryV1, SearchPlayedRowV1, TensorBitsV1, MAX_FILE_BYTES,
};
use crate::rl_session::pauper_london_v1::PauperOpeningRecordV1;
use crate::rl_session::RlSessionTerminalV1;
use crate::sideboard_play_policy_v1::PlayPolicyOriginV1;
use crate::training_math_v1::TrainingMathV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Container magic bytes (never collides with JSON `{` or zstd `28 b5 2f fd`).
pub(crate) const MAGIC_V1: &[u8; 8] = b"MTGTRAJ1";
const CONTAINER_VERSION_V1: u32 = 1;
const FIXED_HEADER_BYTES_V1: usize = 72;
/// Maximum allowed header JSON size (4 MiB — real headers are ~1–2 KiB).
const MAX_HEADER_JSON_BYTES_V1: usize = 4 * 1024 * 1024;
const FLAG_OPPONENT_TENSORS_OMITTED_V1: u32 = 1 << 0;
const FLAG_PAYLOAD_ZSTD1_V1: u32 = 1 << 1;
const KNOWN_FLAGS_MASK_V1: u32 = FLAG_OPPONENT_TENSORS_OMITTED_V1 | FLAG_PAYLOAD_ZSTD1_V1;

const DECISION_FLAG_HAS_TENSOR_V1: u8 = 1 << 0;
const DECISION_FLAG_HAS_SAMPLER_ID_V1: u8 = 1 << 1;
const KNOWN_DECISION_FLAGS_MASK_V1: u8 =
    DECISION_FLAG_HAS_TENSOR_V1 | DECISION_FLAG_HAS_SAMPLER_ID_V1;

const U32_ENC_DENSE_V1: u8 = 0;
const U32_ENC_SPARSE_ZERO_V1: u8 = 1;

const I64_ENC_EMPTY_V1: u8 = 0;
const I64_ENC_I32_V1: u8 = 1;
const I64_ENC_I64_V1: u8 = 2;

const ZSTD1_LEVEL_V1: i32 = 1;

/// Opt-in format for published trajectory records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryRecordFormatV1 {
    /// Legacy JSON (`ExpandedTrajectoryV1`); default and never serialized.
    #[default]
    Json,
    /// `MTGTRAJ1` uncompressed binary columnar container (all seats' tensors).
    BinaryV1,
    /// `MTGTRAJ1` binary container with zstd level-1 compressed decisions body
    /// and uncompressed front metadata header (all seats' tensors).
    BinaryZstd1,
    /// `MTGTRAJ1` with zstd level-1 body and opponent `TensorBitsV1` omitted
    /// (opponent action/logit/value/metadata retained; learner tensors exact).
    BinaryZstd1LearnerOnly,
}

impl TrajectoryRecordFormatV1 {
    /// Serde skip predicate: `Json` is omitted so existing configs keep their exact bytes.
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }

    pub fn is_binary(&self) -> bool {
        !self.is_json()
    }

    pub fn omits_opponent_tensors(&self) -> bool {
        matches!(self, Self::BinaryZstd1LearnerOnly)
    }

    pub fn uses_zstd1_payload(&self) -> bool {
        matches!(self, Self::BinaryZstd1 | Self::BinaryZstd1LearnerOnly)
    }
}

/// Whether `bytes` begins with the `MTGTRAJ1` binary container magic.
pub(crate) fn is_container_v1(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC_V1.len() && &bytes[..MAGIC_V1.len()] == MAGIC_V1
}

#[derive(Serialize)]
struct TrajectoryHeaderBorrowedV1<'a> {
    schema: &'a str,
    feature_contract_digest: &'a str,
    feature_encoding_digest: &'a str,
    card_db_hash: &'a str,
    source_import: &'a PlayPolicyOriginV1,
    behavior_state_sha256: &'a str,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seat_behaviors: Option<&'a [ExpandedSeatBehaviorV1; 2]>,
    episode: &'a ExpandedEpisodeV1,
    configuration_sha256: &'a [String; 2],
    terminal: &'a RlSessionTerminalV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    learner_sampler: Option<&'a str>,
    #[serde(default, skip_serializing_if = "TrainingMathV1::is_platform_libm")]
    training_math: TrainingMathV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_contract: Option<&'a no_state_digest_v1::InputContractRecordV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opening_record: Option<&'a PauperOpeningRecordV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_played: Option<&'a [SearchPlayedRowV1]>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    opponent_tensors_omitted: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrajectoryHeaderOwnedV1 {
    schema: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    card_db_hash: String,
    source_import: PlayPolicyOriginV1,
    behavior_state_sha256: String,
    #[serde(default)]
    seat_behaviors: Option<[ExpandedSeatBehaviorV1; 2]>,
    episode: ExpandedEpisodeV1,
    configuration_sha256: [String; 2],
    terminal: RlSessionTerminalV1,
    #[serde(default)]
    learner_sampler: Option<String>,
    #[serde(default)]
    training_math: TrainingMathV1,
    #[serde(
        default,
        deserialize_with = "no_state_digest_v1::deserialize_present_input_contract_v1"
    )]
    input_contract: Option<no_state_digest_v1::InputContractRecordV1>,
    #[serde(default)]
    opening_record: Option<PauperOpeningRecordV1>,
    #[serde(default)]
    search_played: Option<Vec<SearchPlayedRowV1>>,
    #[serde(default)]
    opponent_tensors_omitted: bool,
}

struct ParsedPrefixV1<'a> {
    flags: u32,
    header_json: &'a [u8],
    body_raw_len: usize,
    body_stored: &'a [u8],
    body_sha256: [u8; 32],
}

fn parse_prefix_v1(
    bytes: &[u8],
    max_uncompressed_bytes: u64,
) -> Result<ParsedPrefixV1<'_>, String> {
    ensure(
        bytes.len() >= FIXED_HEADER_BYTES_V1 && is_container_v1(bytes),
        "invalid trajectory container header",
    )?;
    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
    ensure(
        version == CONTAINER_VERSION_V1,
        "unsupported trajectory container version",
    )?;
    let flags = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes"));
    ensure(
        (flags & !KNOWN_FLAGS_MASK_V1) == 0,
        "unknown trajectory container flags",
    )?;
    let header_len_u64 = u64::from_le_bytes(bytes[16..24].try_into().expect("8 bytes"));
    let body_raw_len_u64 = u64::from_le_bytes(bytes[24..32].try_into().expect("8 bytes"));
    let body_stored_len_u64 = u64::from_le_bytes(bytes[32..40].try_into().expect("8 bytes"));
    let body_sha256: [u8; 32] = bytes[40..72].try_into().expect("32 bytes");

    let header_len =
        usize::try_from(header_len_u64).map_err(|_| "trajectory container header overflow")?;
    ensure(
        header_len > 0 && header_len <= MAX_HEADER_JSON_BYTES_V1,
        "trajectory container header length out of bounds",
    )?;
    let body_raw_len = usize::try_from(body_raw_len_u64)
        .map_err(|_| "trajectory container body raw length overflow")?;
    let body_stored_len = usize::try_from(body_stored_len_u64)
        .map_err(|_| "trajectory container body stored length overflow")?;
    let total_raw = header_len_u64
        .checked_add(body_raw_len_u64)
        .and_then(|v| v.checked_add(FIXED_HEADER_BYTES_V1 as u64))
        .ok_or("trajectory container size overflow")?;
    ensure(
        total_raw <= max_uncompressed_bytes,
        "trajectory container exceeds maximum allowed size",
    )?;
    if (flags & FLAG_PAYLOAD_ZSTD1_V1) == 0 {
        ensure(
            body_stored_len == body_raw_len,
            "uncompressed trajectory container stored/raw length mismatch",
        )?;
    }
    let header_end = FIXED_HEADER_BYTES_V1
        .checked_add(header_len)
        .ok_or("trajectory container offset overflow")?;
    let body_end = header_end
        .checked_add(body_stored_len)
        .ok_or("trajectory container offset overflow")?;
    ensure(
        body_end == bytes.len(),
        "trajectory container byte length does not match framing",
    )?;
    Ok(ParsedPrefixV1 {
        flags,
        header_json: &bytes[FIXED_HEADER_BYTES_V1..header_end],
        body_raw_len,
        body_stored: &bytes[header_end..body_end],
        body_sha256,
    })
}

/// Encodes `trajectory` into `MTGTRAJ1` binary container format.
pub(super) fn encode_v1(
    trajectory: &ExpandedTrajectoryV1,
    format: TrajectoryRecordFormatV1,
) -> Result<Vec<u8>, String> {
    ensure(
        format.is_binary(),
        "trajectory_container_v1::encode_v1 requires a binary format",
    )?;
    let omit_opponent = format.omits_opponent_tensors() || trajectory.opponent_tensors_omitted;
    let learner_seat = trajectory.episode.learner_seat;
    let header = TrajectoryHeaderBorrowedV1 {
        schema: &trajectory.schema,
        feature_contract_digest: &trajectory.feature_contract_digest,
        feature_encoding_digest: &trajectory.feature_encoding_digest,
        card_db_hash: &trajectory.card_db_hash,
        source_import: &trajectory.source_import,
        behavior_state_sha256: &trajectory.behavior_state_sha256,
        seat_behaviors: trajectory.seat_behaviors.as_ref(),
        episode: &trajectory.episode,
        configuration_sha256: &trajectory.configuration_sha256,
        terminal: &trajectory.terminal,
        learner_sampler: trajectory.learner_sampler.as_deref(),
        training_math: trajectory.training_math,
        input_contract: trajectory.input_contract.as_ref(),
        opening_record: trajectory.opening_record.as_ref(),
        search_played: trajectory.search_played.as_deref(),
        opponent_tensors_omitted: omit_opponent,
    };
    let header_json = serde_json::to_vec(&header).map_err(err)?;
    ensure(
        header_json.len() <= MAX_HEADER_JSON_BYTES_V1,
        "trajectory container header exceeds limit",
    )?;

    let body_raw = encode_decisions_body_v1(&trajectory.decisions, learner_seat, omit_opponent)?;
    let body_sha256: [u8; 32] = Sha256::digest(&body_raw).into();

    let mut flags = 0u32;
    if omit_opponent {
        flags |= FLAG_OPPONENT_TENSORS_OMITTED_V1;
    }
    let body_stored = if format.uses_zstd1_payload() {
        flags |= FLAG_PAYLOAD_ZSTD1_V1;
        let mut compressor = zstd::bulk::Compressor::new(ZSTD1_LEVEL_V1).map_err(err)?;
        compressor
            .set_parameter(zstd::stream::raw::CParameter::ChecksumFlag(true))
            .map_err(err)?;
        compressor
            .set_parameter(zstd::stream::raw::CParameter::ContentSizeFlag(true))
            .map_err(err)?;
        compressor.compress(&body_raw).map_err(err)?
    } else {
        body_raw.clone()
    };

    let total_len = FIXED_HEADER_BYTES_V1
        .checked_add(header_json.len())
        .and_then(|v| v.checked_add(body_stored.len()))
        .ok_or("trajectory container encoded size overflow")?;
    ensure(
        total_len as u64 <= MAX_FILE_BYTES,
        "trajectory container exceeds file size limit",
    )?;

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(MAGIC_V1);
    out.extend_from_slice(&CONTAINER_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&(header_json.len() as u64).to_le_bytes());
    out.extend_from_slice(&(body_raw.len() as u64).to_le_bytes());
    out.extend_from_slice(&(body_stored.len() as u64).to_le_bytes());
    out.extend_from_slice(&body_sha256);
    out.extend_from_slice(&header_json);
    out.extend_from_slice(&body_stored);
    debug_assert_eq!(out.len(), total_len);
    Ok(out)
}

/// O(1) header-only validation read: verifies container framing and parses ONLY
/// the small uncompressed front JSON header without touching or decompressing
/// the binary decisions payload.
pub(crate) fn read_collection_fields_v1(
    bytes: &[u8],
    max_uncompressed_bytes: u64,
) -> Result<CollectionTrajectoryFieldsV1, String> {
    let prefix = parse_prefix_v1(bytes, max_uncompressed_bytes)?;
    CollectionTrajectoryFieldsV1::parse_json_v1(prefix.header_json)
}

/// Decodes an `MTGTRAJ1` container into [`ExpandedTrajectoryV1`], verifying
/// framing, zstd checksum (if compressed), exact uncompressed length, and
/// `body_sha256`.
pub(super) fn decode_v1(
    bytes: &[u8],
    max_uncompressed_bytes: u64,
) -> Result<ExpandedTrajectoryV1, String> {
    let prefix = parse_prefix_v1(bytes, max_uncompressed_bytes)?;
    let header: TrajectoryHeaderOwnedV1 =
        serde_json::from_slice(prefix.header_json).map_err(err)?;
    let flag_omit = (prefix.flags & FLAG_OPPONENT_TENSORS_OMITTED_V1) != 0;
    ensure(
        header.opponent_tensors_omitted == flag_omit,
        "trajectory container opponent-tensor flag disagrees with header",
    )?;

    let body_owned;
    let body_raw: &[u8] = if (prefix.flags & FLAG_PAYLOAD_ZSTD1_V1) != 0 {
        let mut decompressor = zstd::bulk::Decompressor::new().map_err(err)?;
        body_owned = decompressor
            .decompress(prefix.body_stored, prefix.body_raw_len)
            .map_err(err)?;
        ensure(
            body_owned.len() == prefix.body_raw_len,
            "trajectory container decompressed body length differs",
        )?;
        &body_owned
    } else {
        prefix.body_stored
    };

    let actual_sha: [u8; 32] = Sha256::digest(body_raw).into();
    ensure(
        actual_sha == prefix.body_sha256,
        "trajectory container body SHA-256 mismatch",
    )?;

    let decisions = decode_decisions_body_v1(
        body_raw,
        header.episode.learner_seat,
        header.opponent_tensors_omitted,
    )?;

    Ok(ExpandedTrajectoryV1 {
        schema: header.schema,
        feature_contract_digest: header.feature_contract_digest,
        feature_encoding_digest: header.feature_encoding_digest,
        card_db_hash: header.card_db_hash,
        source_import: header.source_import,
        behavior_state_sha256: header.behavior_state_sha256,
        seat_behaviors: header.seat_behaviors,
        episode: header.episode,
        configuration_sha256: header.configuration_sha256,
        decisions,
        terminal: header.terminal,
        learner_sampler: header.learner_sampler,
        training_math: header.training_math,
        input_contract: header.input_contract,
        opening_record: header.opening_record,
        search_played: header.search_played,
        opponent_tensors_omitted: header.opponent_tensors_omitted,
    })
}

/// Declared uncompressed record byte length for batch admission checks
/// (`MTGTRAJ1` header + uncompressed body, or `record_envelope_v1::record_length_v1`
/// for legacy JSON / zstd1-sealed records).
pub(crate) fn record_length_v1(bytes: &[u8], max_uncompressed_bytes: u64) -> Result<u64, String> {
    if is_container_v1(bytes) {
        let prefix = parse_prefix_v1(bytes, max_uncompressed_bytes)?;
        let total = (FIXED_HEADER_BYTES_V1 as u64)
            .checked_add(prefix.header_json.len() as u64)
            .and_then(|v| v.checked_add(prefix.body_raw_len as u64))
            .ok_or("trajectory container size overflow")?;
        ensure(
            total <= max_uncompressed_bytes,
            "trajectory container exceeds maximum allowed size",
        )?;
        Ok(total)
    } else {
        super::record_envelope_v1::record_length_v1(bytes, max_uncompressed_bytes)
    }
}

/// Auto-detecting reader for any trajectory record (legacy JSON, `zstd1`-sealed
/// JSON, `MTGTRAJ1`, or `zstd1`-sealed `MTGTRAJ1`).
pub(super) fn decode_any_v1(
    bytes: &[u8],
    max_uncompressed_bytes: u64,
) -> Result<ExpandedTrajectoryV1, String> {
    if is_container_v1(bytes) {
        return decode_v1(bytes, max_uncompressed_bytes);
    }
    let opened = super::record_envelope_v1::open_v1(bytes, max_uncompressed_bytes)?;
    if is_container_v1(&opened) {
        decode_v1(&opened, max_uncompressed_bytes)
    } else {
        serde_json::from_slice(&opened).map_err(err)
    }
}

/// Auto-detecting collection-fields reader: O(1) header-only for `MTGTRAJ1`,
/// zero-DOM JSON field visitor for JSON records.
pub(crate) fn read_any_collection_fields_v1(
    bytes: &[u8],
    max_uncompressed_bytes: u64,
) -> Result<CollectionTrajectoryFieldsV1, String> {
    if is_container_v1(bytes) {
        return read_collection_fields_v1(bytes, max_uncompressed_bytes);
    }
    let opened = super::record_envelope_v1::open_v1(bytes, max_uncompressed_bytes)?;
    if is_container_v1(&opened) {
        read_collection_fields_v1(&opened, max_uncompressed_bytes)
    } else {
        CollectionTrajectoryFieldsV1::parse_json_v1(&opened)
    }
}

fn encode_decisions_body_v1(
    decisions: &[DecisionRecordV1],
    learner_seat: u8,
    omit_opponent_tensors: bool,
) -> Result<Vec<u8>, String> {
    let count =
        u32::try_from(decisions.len()).map_err(|_| "trajectory decision count exceeds u32")?;
    let mut buf = Vec::with_capacity(decisions.len().saturating_mul(256) + 4);
    buf.extend_from_slice(&count.to_le_bytes());
    for d in decisions {
        let is_learner = d.actor == learner_seat;
        let include_tensor = if omit_opponent_tensors && !is_learner {
            false
        } else {
            !d.tensor.is_empty_v1()
        };
        if omit_opponent_tensors && is_learner {
            ensure(
                include_tensor,
                "learner decision cannot have empty tensor in learner-only container",
            )?;
        }
        let mut dflags = 0u8;
        if include_tensor {
            dflags |= DECISION_FLAG_HAS_TENSOR_V1;
        }
        if d.sampler_identity.is_some() {
            dflags |= DECISION_FLAG_HAS_SAMPLER_ID_V1;
        }
        buf.extend_from_slice(&d.step.to_le_bytes());
        buf.extend_from_slice(&d.physical_decision_id.to_le_bytes());
        buf.extend_from_slice(&d.substep_index.to_le_bytes());
        buf.extend_from_slice(&d.substep_count.to_le_bytes());
        buf.push(d.actor);
        buf.push(dflags);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&d.selected.to_le_bytes());
        buf.extend_from_slice(&d.value.to_le_bytes());
        let logits_len =
            u32::try_from(d.logits.len()).map_err(|_| "decision logits count exceeds u32")?;
        buf.extend_from_slice(&logits_len.to_le_bytes());
        for &logit in &d.logits {
            buf.extend_from_slice(&logit.to_le_bytes());
        }
        if let Some(id) = &d.sampler_identity {
            let id_bytes = id.as_bytes();
            let id_len =
                u32::try_from(id_bytes.len()).map_err(|_| "sampler_identity length exceeds u32")?;
            buf.extend_from_slice(&id_len.to_le_bytes());
            buf.extend_from_slice(id_bytes);
        }
        if include_tensor {
            encode_u32_vec_v1(&mut buf, &d.tensor.state)?;
            encode_u32_vec_v1(&mut buf, &d.tensor.object_features)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.object_card_ids)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.object_groups)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.object_node_ids)?;
            encode_u32_vec_v1(&mut buf, &d.tensor.edge_features)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.edge_source_indices)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.edge_target_indices)?;
            encode_u32_vec_v1(&mut buf, &d.tensor.action_features)?;
            encode_u32_vec_v1(&mut buf, &d.tensor.action_ref_features)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.action_ref_card_ids)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.action_ref_action_indices)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.action_ref_node_indices)?;
            encode_i64_vec_v1(&mut buf, &d.tensor.pairing_node_indices)?;
        }
    }
    Ok(buf)
}

fn encode_u32_vec_v1(buf: &mut Vec<u8>, values: &[u32]) -> Result<(), String> {
    let len = u32::try_from(values.len()).map_err(|_| "tensor u32 vector exceeds u32")?;
    buf.extend_from_slice(&len.to_le_bytes());
    let nnz = values.iter().filter(|&&v| v != 0).count();
    if !values.is_empty() && nnz.saturating_mul(2) < values.len() {
        buf.push(U32_ENC_SPARSE_ZERO_V1);
        buf.extend_from_slice(&(nnz as u32).to_le_bytes());
        for (idx, &v) in values.iter().enumerate() {
            if v != 0 {
                buf.extend_from_slice(&(idx as u32).to_le_bytes());
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
    } else {
        buf.push(U32_ENC_DENSE_V1);
        for &v in values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(())
}

fn encode_i64_vec_v1(buf: &mut Vec<u8>, values: &[i64]) -> Result<(), String> {
    let len = u32::try_from(values.len()).map_err(|_| "tensor i64 vector exceeds u32")?;
    buf.extend_from_slice(&len.to_le_bytes());
    if values.is_empty() {
        buf.push(I64_ENC_EMPTY_V1);
        return Ok(());
    }
    if values.iter().all(|&v| i32::try_from(v).is_ok()) {
        buf.push(I64_ENC_I32_V1);
        for &v in values {
            buf.extend_from_slice(&(v as i32).to_le_bytes());
        }
    } else {
        buf.push(I64_ENC_I64_V1);
        for &v in values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(())
}

struct CursorV1<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> CursorV1<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or("trajectory container cursor overflow")?;
        ensure(end <= self.data.len(), "trajectory container truncated")?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("2")))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
}

fn decode_u32_vec_v1(cur: &mut CursorV1<'_>) -> Result<Vec<u32>, String> {
    let len = cur.read_u32()? as usize;
    let enc = cur.read_u8()?;
    match enc {
        U32_ENC_DENSE_V1 => {
            let byte_len = len
                .checked_mul(4)
                .ok_or("trajectory container u32 array length overflow")?;
            let raw = cur.take(byte_len)?;
            let mut out = Vec::with_capacity(len);
            for chunk in raw.chunks_exact(4) {
                out.push(u32::from_le_bytes(chunk.try_into().expect("4")));
            }
            Ok(out)
        }
        U32_ENC_SPARSE_ZERO_V1 => {
            let nnz = cur.read_u32()? as usize;
            ensure(nnz <= len, "trajectory container sparse nnz exceeds length")?;
            let byte_len = nnz
                .checked_mul(8)
                .ok_or("trajectory container sparse array length overflow")?;
            let raw = cur.take(byte_len)?;
            let mut out = vec![0u32; len];
            let mut prev_idx: Option<usize> = None;
            for chunk in raw.chunks_exact(8) {
                let idx = u32::from_le_bytes(chunk[0..4].try_into().expect("4")) as usize;
                let bits = u32::from_le_bytes(chunk[4..8].try_into().expect("4"));
                ensure(idx < len, "trajectory container sparse index out of bounds")?;
                ensure(
                    bits != 0,
                    "trajectory container sparse encoding contains zero value",
                )?;
                if let Some(prev) = prev_idx {
                    ensure(
                        idx > prev,
                        "trajectory container sparse indices not strictly ascending",
                    )?;
                }
                prev_idx = Some(idx);
                out[idx] = bits;
            }
            Ok(out)
        }
        _ => Err("unknown trajectory container u32 encoding".into()),
    }
}

fn decode_i64_vec_v1(cur: &mut CursorV1<'_>) -> Result<Vec<i64>, String> {
    let len = cur.read_u32()? as usize;
    let enc = cur.read_u8()?;
    match enc {
        I64_ENC_EMPTY_V1 => {
            ensure(
                len == 0,
                "trajectory container non-empty array marked empty",
            )?;
            Ok(Vec::new())
        }
        I64_ENC_I32_V1 => {
            let byte_len = len
                .checked_mul(4)
                .ok_or("trajectory container i32 array length overflow")?;
            let raw = cur.take(byte_len)?;
            let mut out = Vec::with_capacity(len);
            for chunk in raw.chunks_exact(4) {
                out.push(i64::from(i32::from_le_bytes(chunk.try_into().expect("4"))));
            }
            Ok(out)
        }
        I64_ENC_I64_V1 => {
            let byte_len = len
                .checked_mul(8)
                .ok_or("trajectory container i64 array length overflow")?;
            let raw = cur.take(byte_len)?;
            let mut out = Vec::with_capacity(len);
            for chunk in raw.chunks_exact(8) {
                out.push(i64::from_le_bytes(chunk.try_into().expect("8")));
            }
            Ok(out)
        }
        _ => Err("unknown trajectory container i64 encoding".into()),
    }
}

fn decode_decisions_body_v1(
    body: &[u8],
    learner_seat: u8,
    opponent_tensors_omitted: bool,
) -> Result<Vec<DecisionRecordV1>, String> {
    let mut cur = CursorV1::new(body);
    let count = cur.read_u32()? as usize;
    let mut decisions = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let step = cur.read_u64()?;
        let physical_decision_id = cur.read_u64()?;
        let substep_index = cur.read_u32()?;
        let substep_count = cur.read_u32()?;
        let actor = cur.read_u8()?;
        let dflags = cur.read_u8()?;
        ensure(
            (dflags & !KNOWN_DECISION_FLAGS_MASK_V1) == 0,
            "unknown trajectory decision flags",
        )?;
        let reserved = cur.read_u16()?;
        ensure(
            reserved == 0,
            "non-zero reserved bytes in trajectory decision",
        )?;
        let selected = cur.read_u32()?;
        let value = cur.read_u32()?;
        let logits_len = cur.read_u32()? as usize;
        let logits_bytes = cur.take(
            logits_len
                .checked_mul(4)
                .ok_or("trajectory container logits overflow")?,
        )?;
        let mut logits = Vec::with_capacity(logits_len);
        for chunk in logits_bytes.chunks_exact(4) {
            logits.push(u32::from_le_bytes(chunk.try_into().expect("4")));
        }
        let sampler_identity = if (dflags & DECISION_FLAG_HAS_SAMPLER_ID_V1) != 0 {
            let id_len = cur.read_u32()? as usize;
            let raw = cur.take(id_len)?;
            let s = std::str::from_utf8(raw)
                .map_err(|_| "invalid UTF-8 in trajectory sampler_identity")?;
            Some(s.to_owned())
        } else {
            None
        };
        let has_tensor = (dflags & DECISION_FLAG_HAS_TENSOR_V1) != 0;
        if opponent_tensors_omitted {
            let is_learner = actor == learner_seat;
            ensure(
                has_tensor == is_learner,
                "learner-only trajectory tensor presence violates seat contract",
            )?;
        }
        let tensor = if has_tensor {
            TensorBitsV1 {
                state: decode_u32_vec_v1(&mut cur)?,
                object_features: decode_u32_vec_v1(&mut cur)?,
                object_card_ids: decode_i64_vec_v1(&mut cur)?,
                object_groups: decode_i64_vec_v1(&mut cur)?,
                object_node_ids: decode_i64_vec_v1(&mut cur)?,
                edge_features: decode_u32_vec_v1(&mut cur)?,
                edge_source_indices: decode_i64_vec_v1(&mut cur)?,
                edge_target_indices: decode_i64_vec_v1(&mut cur)?,
                action_features: decode_u32_vec_v1(&mut cur)?,
                action_ref_features: decode_u32_vec_v1(&mut cur)?,
                action_ref_card_ids: decode_i64_vec_v1(&mut cur)?,
                action_ref_action_indices: decode_i64_vec_v1(&mut cur)?,
                action_ref_node_indices: decode_i64_vec_v1(&mut cur)?,
                pairing_node_indices: decode_i64_vec_v1(&mut cur)?,
            }
        } else {
            TensorBitsV1::empty_v1()
        };
        decisions.push(DecisionRecordV1 {
            step,
            physical_decision_id,
            substep_index,
            substep_count,
            actor,
            selected,
            logits,
            value,
            tensor,
            sampler_identity,
        });
    }
    ensure(
        cur.pos == body.len(),
        "trajectory container has trailing bytes in decisions body",
    )?;
    Ok(decisions)
}

#[cfg(test)]
mod tests;
