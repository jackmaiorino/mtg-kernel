//! Checked binary digest framing shared by pure Native Training Store records.
//!
//! This module owns no record schema and performs no filesystem I/O.  It only
//! implements the frozen `ATOM(tag, payload)` framing, strict lowercase raw32
//! conversion, and SHA-256 helpers used by typed record validators.

use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTrainingStoreDigestErrorV1 {
    AtomTagLength,
    AtomPayloadLength,
    InvalidRaw32,
}

impl Display for NativeTrainingStoreDigestErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            Self::AtomTagLength => "native_training_store_atom_tag_length",
            Self::AtomPayloadLength => "native_training_store_atom_payload_length",
            Self::InvalidRaw32 => "native_training_store_invalid_raw32",
        };
        formatter.write_str(code)
    }
}

impl Error for NativeTrainingStoreDigestErrorV1 {}

pub(crate) struct NativeTrainingStoreAtomSha256V1 {
    hasher: Sha256,
}

impl NativeTrainingStoreAtomSha256V1 {
    pub(crate) fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    pub(crate) fn atom(
        &mut self,
        tag: &str,
        payload: &[u8],
    ) -> Result<(), NativeTrainingStoreDigestErrorV1> {
        let tag_length = u32::try_from(tag.len())
            .map_err(|_| NativeTrainingStoreDigestErrorV1::AtomTagLength)?;
        let payload_length = u64::try_from(payload.len())
            .map_err(|_| NativeTrainingStoreDigestErrorV1::AtomPayloadLength)?;
        self.hasher.update(tag_length.to_be_bytes());
        self.hasher.update(tag.as_bytes());
        self.hasher.update(payload_length.to_be_bytes());
        self.hasher.update(payload);
        Ok(())
    }

    pub(crate) fn finalize(self) -> [u8; 32] {
        finalize_sha256_v1(self.hasher)
    }
}

pub(crate) fn sha256_v1(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    finalize_sha256_v1(hasher)
}

pub(crate) fn lower_hex_raw32_v1(raw: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in raw {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

pub(crate) fn parse_lower_hex_raw32_v1(
    encoded: &str,
) -> Result<[u8; 32], NativeTrainingStoreDigestErrorV1> {
    if encoded.len() != 64 {
        return Err(NativeTrainingStoreDigestErrorV1::InvalidRaw32);
    }
    let bytes = encoded.as_bytes();
    let mut raw = [0_u8; 32];
    for (index, output) in raw.iter_mut().enumerate() {
        let high = decode_lower_hex_nibble_v1(bytes[index * 2])?;
        let low = decode_lower_hex_nibble_v1(bytes[index * 2 + 1])?;
        *output = (high << 4) | low;
    }
    Ok(raw)
}

fn decode_lower_hex_nibble_v1(byte: u8) -> Result<u8, NativeTrainingStoreDigestErrorV1> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(NativeTrainingStoreDigestErrorV1::InvalidRaw32),
    }
}

fn finalize_sha256_v1(hasher: Sha256) -> [u8; 32] {
    let digest = hasher.finalize();
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw32_roundtrip_is_strictly_lowercase() {
        let raw = [0xab; 32];
        let encoded = lower_hex_raw32_v1(raw);
        assert_eq!(encoded, "ab".repeat(32));
        assert_eq!(parse_lower_hex_raw32_v1(&encoded).unwrap(), raw);
        assert_eq!(
            parse_lower_hex_raw32_v1(&"AB".repeat(32)).unwrap_err(),
            NativeTrainingStoreDigestErrorV1::InvalidRaw32
        );
        assert_eq!(
            parse_lower_hex_raw32_v1(&"0".repeat(63)).unwrap_err(),
            NativeTrainingStoreDigestErrorV1::InvalidRaw32
        );
    }

    #[test]
    fn atom_framing_matches_an_independent_byte_reference() {
        let mut framed = Vec::new();
        framed.extend_from_slice(&6_u32.to_be_bytes());
        framed.extend_from_slice(b"domain");
        framed.extend_from_slice(&3_u64.to_be_bytes());
        framed.extend_from_slice(b"abc");

        let mut production = NativeTrainingStoreAtomSha256V1::new();
        production.atom("domain", b"abc").unwrap();
        assert_eq!(production.finalize(), sha256_v1(&framed));
    }
}
