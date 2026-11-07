//! Record envelope v1: an opt-in zstd frame around the exact bytes of a
//! published trajectory or checkpoint, to cut the bytes an ordinary training
//! run writes per update (a W5 C update wrote about 47 MB: ten trajectory
//! JSON files of about 32 MB and a 15 MB checkpoint container).
//!
//! # Format
//!
//! A sealed file is exactly one zstd frame (level 1, single-threaded, with
//! the content size and the content checksum in the frame) whose content is
//! the bytes the unsealed writer publishes for the same record. Nothing else
//! changes: the record's JSON or container bytes, every parse of them and
//! every state they restore are the unsealed ones. A sealed file starts with
//! the zstd magic `28 b5 2f fd`; a JSON record starts with `{` and a
//! checkpoint container with `MTGCKPT`, so the two never collide, and readers
//! accept either without being told which one to expect.
//!
//! # Pins and determinism
//!
//! A pin binds the bytes on disk (the sealed bytes). The writer is
//! deterministic for a given build (the zstd library is pinned by
//! `Cargo.lock`), so a replay or recollection with the same build reproduces
//! the same pins. Model states, `after_state_sha256` and every tensor are
//! unaffected; the state digest never covers a file's bytes.
//!
//! # Opt-in and wire
//!
//! [`RecordEnvelopeV1::None`] is the default and is never serialized, so
//! every existing run config, command and run identity keeps its bytes and
//! publishes exactly what it did before. `"zstd1"` is serialized when chosen.
//!
//! # Readers
//!
//! [`open_v1`] returns the record's bytes: the input itself when it is not
//! sealed, else the frame's content after checking that the input is one
//! whole frame with a declared content size within the caller's bound, that
//! the decompressed length equals it, and that the content is not itself
//! sealed. Readers verify pins against the sealed bytes first, as before,
//! and open only where they parse, so byte comparisons and caches keep
//! working on the bytes that were pinned.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// The first four bytes of every zstd frame (little-endian 0xFD2FB528).
pub(crate) const ZSTD_MAGIC_V1: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
/// The compression level of `zstd1`. Part of the format: a different level
/// writes different (equally valid) bytes and so different pins.
const ZSTD1_LEVEL_V1: i32 = 1;

/// How a run publishes its trajectories and checkpoints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordEnvelopeV1 {
    /// The record's bytes as they are (the default; never serialized).
    #[default]
    None,
    /// One zstd frame (level 1, content size and checksum) of the record's
    /// bytes.
    Zstd1,
}

impl RecordEnvelopeV1 {
    /// The serde skip predicate: absence and an explicit `"none"` share bytes.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The bytes to publish for a record whose unsealed bytes are `bytes`.
    /// `Zstd1` refuses input that already looks sealed (a record is sealed
    /// once) and checks that the frame opens to exactly `bytes`.
    pub(crate) fn seal_v1<'a>(self, bytes: &'a [u8]) -> Result<Cow<'a, [u8]>, String> {
        match self {
            Self::None => Ok(Cow::Borrowed(bytes)),
            Self::Zstd1 => {
                if is_sealed_v1(bytes) {
                    return Err("a record envelope seals unsealed record bytes only".into());
                }
                let mut compressor =
                    zstd::bulk::Compressor::new(ZSTD1_LEVEL_V1).map_err(|e| e.to_string())?;
                compressor
                    .set_parameter(zstd::stream::raw::CParameter::ChecksumFlag(true))
                    .map_err(|e| e.to_string())?;
                compressor
                    .set_parameter(zstd::stream::raw::CParameter::ContentSizeFlag(true))
                    .map_err(|e| e.to_string())?;
                let sealed = compressor.compress(bytes).map_err(|e| e.to_string())?;
                if open_v1(&sealed, bytes.len() as u64)?.as_ref() != bytes {
                    return Err("record envelope round trip differs".into());
                }
                Ok(Cow::Owned(sealed))
            }
        }
    }

    /// [`Self::seal_v1`] for owned record bytes: `None` returns them as they
    /// are, without a copy.
    pub(crate) fn seal_owned_v1(self, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
        match self {
            Self::None => Ok(bytes),
            Self::Zstd1 => self.seal_v1(&bytes).map(Cow::into_owned),
        }
    }
}

/// Whether `bytes` start with the zstd frame magic.
pub(crate) fn is_sealed_v1(bytes: &[u8]) -> bool {
    bytes.starts_with(&ZSTD_MAGIC_V1)
}

/// The record bytes of `bytes`: `bytes` themselves when unsealed, else the
/// content of their single zstd frame, at most `cap` bytes long.
pub(crate) fn open_v1(bytes: &[u8], cap: u64) -> Result<Cow<'_, [u8]>, String> {
    if !is_sealed_v1(bytes) {
        return Ok(Cow::Borrowed(bytes));
    }
    let length = sealed_content_length_v1(bytes, cap)?;
    let frame = zstd::zstd_safe::find_frame_compressed_size(bytes)
        .map_err(|_| "record envelope frame is malformed".to_string())?;
    if frame != bytes.len() {
        return Err("a record envelope is exactly one zstd frame".into());
    }
    let content = zstd::bulk::decompress(bytes, length).map_err(|e| e.to_string())?;
    if content.len() != length {
        return Err("record envelope content length differs from its frame".into());
    }
    if is_sealed_v1(&content) {
        return Err("a record envelope holds unsealed record bytes only".into());
    }
    Ok(Cow::Owned(content))
}

/// The record length of `bytes` without decompressing: `bytes.len()` when
/// unsealed, else the frame's declared content size (at most `cap`).
pub(crate) fn record_length_v1(bytes: &[u8], cap: u64) -> Result<u64, String> {
    if is_sealed_v1(bytes) {
        sealed_content_length_v1(bytes, cap).map(|length| length as u64)
    } else {
        Ok(bytes.len() as u64)
    }
}

fn sealed_content_length_v1(bytes: &[u8], cap: u64) -> Result<usize, String> {
    let declared = zstd::zstd_safe::get_frame_content_size(bytes);
    if declared == zstd::zstd_safe::CONTENTSIZE_ERROR
        || declared == zstd::zstd_safe::CONTENTSIZE_UNKNOWN
    {
        return Err("a record envelope declares its content size".into());
    }
    if declared > cap {
        return Err("record envelope content exceeds its size limit".into());
    }
    usize::try_from(declared).map_err(|_| "record envelope content size overflows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Vec<u8> {
        let mut bytes = b"{\"decisions\":[".to_vec();
        for index in 0..4000u32 {
            bytes.extend_from_slice(format!("{},0,0,1065353216,", index % 7).as_bytes());
        }
        bytes.extend_from_slice(b"0]}");
        bytes
    }

    #[test]
    fn none_publishes_the_record_bytes_and_is_never_serialized() {
        let bytes = record();
        assert!(matches!(
            RecordEnvelopeV1::None.seal_v1(&bytes).unwrap(),
            Cow::Borrowed(b) if b == bytes.as_slice()
        ));
        assert!(RecordEnvelopeV1::default().is_none());
        assert_eq!(
            serde_json::to_string(&RecordEnvelopeV1::Zstd1).unwrap(),
            "\"zstd1\""
        );
        assert_eq!(
            serde_json::from_str::<RecordEnvelopeV1>("\"none\"").unwrap(),
            RecordEnvelopeV1::None
        );
        assert!(serde_json::from_str::<RecordEnvelopeV1>("\"zstd\"").is_err());
    }

    #[test]
    fn zstd1_seals_deterministically_and_opens_to_the_record_bytes() {
        let bytes = record();
        let sealed = RecordEnvelopeV1::Zstd1
            .seal_v1(&bytes)
            .unwrap()
            .into_owned();
        assert!(is_sealed_v1(&sealed));
        assert!(sealed.len() < bytes.len() / 4);
        assert_eq!(
            RecordEnvelopeV1::Zstd1.seal_v1(&bytes).unwrap().as_ref(),
            sealed.as_slice()
        );
        assert_eq!(
            open_v1(&sealed, bytes.len() as u64).unwrap().as_ref(),
            bytes.as_slice()
        );
        assert_eq!(
            record_length_v1(&sealed, u64::MAX).unwrap(),
            bytes.len() as u64
        );
        // Unsealed input opens to itself, borrowed.
        assert!(matches!(open_v1(&bytes, 0).unwrap(), Cow::Borrowed(_)));
        assert_eq!(record_length_v1(&bytes, 0).unwrap(), bytes.len() as u64);
        // A checkpoint container or JSON never looks sealed.
        assert!(!is_sealed_v1(b"MTGCKPT\0"));
        assert!(!is_sealed_v1(b"{}"));
    }

    #[test]
    fn open_refuses_anything_but_one_bounded_frame_of_unsealed_bytes() {
        let bytes = record();
        let sealed = RecordEnvelopeV1::Zstd1
            .seal_v1(&bytes)
            .unwrap()
            .into_owned();
        assert!(open_v1(&sealed, bytes.len() as u64 - 1)
            .unwrap_err()
            .contains("size limit"));
        let mut two = sealed.clone();
        two.extend_from_slice(&sealed);
        assert!(open_v1(&two, u64::MAX).unwrap_err().contains("exactly one"));
        let mut trailing = sealed.clone();
        trailing.push(0);
        assert!(open_v1(&trailing, u64::MAX).is_err());
        let truncated = &sealed[..sealed.len() - 1];
        assert!(open_v1(truncated, u64::MAX).is_err());
        let mut flipped = sealed.clone();
        let middle = flipped.len() / 2;
        flipped[middle] ^= 0x40;
        assert!(open_v1(&flipped, u64::MAX).is_err());
        // Sealing twice is refused, and so is a frame of sealed bytes.
        assert!(RecordEnvelopeV1::Zstd1.seal_v1(&sealed).is_err());
        let double = zstd::bulk::compress(&sealed, 1).unwrap();
        assert!(open_v1(&double, u64::MAX).unwrap_err().contains("unsealed"));
        // A frame without a declared content size is refused.
        let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
        std::io::Write::write_all(&mut encoder, &bytes).unwrap();
        let streamed = encoder.finish().unwrap();
        assert!(open_v1(&streamed, u64::MAX)
            .unwrap_err()
            .contains("declares its content size"));
    }
}
