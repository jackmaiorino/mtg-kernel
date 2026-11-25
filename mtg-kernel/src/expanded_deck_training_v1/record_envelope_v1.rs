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
    /// once) and checks, in every build and before the frame is returned for
    /// publication, that the frame opens to exactly `bytes`.
    pub(crate) fn seal_v1(self, bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
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
                // The compressor allocates for the worst case (the bound for
                // the unsealed length) and returns the frame in that
                // allocation. The frame is kept for a long time (the
                // verified-bytes cache and the parse-ahead store share it), so
                // it is copied into an allocation of its own size and the
                // large one is freed. (Shrinking in place is not enough: the
                // allocator may keep a block that is still at least half
                // used.)
                let sealed = compressor.compress(bytes).map_err(|e| e.to_string())?;
                let sealed = if sealed.capacity() > sealed.len() {
                    sealed.as_slice().to_vec()
                } else {
                    sealed
                };
                check_round_trip_v1(&sealed, bytes)?;
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

/// The self-check of a new frame: `open_v1(sealed, bytes.len())` is exactly
/// `bytes`, with `open_v1`'s error when it fails. A frame that streams to
/// exactly `bytes` ([`round_trip_streams_v1`]) passes without the record's
/// second copy; any other frame gets [`reference_round_trip_v1`], the
/// previous check itself, so every failure keeps its previous error.
fn check_round_trip_v1(sealed: &[u8], bytes: &[u8]) -> Result<(), String> {
    if round_trip_streams_v1(sealed, bytes) {
        return Ok(());
    }
    reference_round_trip_v1(sealed, bytes)
}

/// The previous self-check: the whole frame opened with [`open_v1`] and
/// compared with `bytes`.
fn reference_round_trip_v1(sealed: &[u8], bytes: &[u8]) -> Result<(), String> {
    if open_v1(sealed, bytes.len() as u64)?.as_ref() != bytes {
        return Err("record envelope round trip differs".into());
    }
    Ok(())
}

/// The decompressed bytes compared at a time by [`round_trip_streams_v1`].
const ROUND_TRIP_CHUNK_BYTES_V1: usize = 128 << 10;

/// Whether `open_v1(sealed, bytes.len())` certainly returns exactly `bytes`,
/// decided by decompressing `sealed` in chunks and comparing each with
/// `bytes` as it comes, so the record is never held twice. `false` means
/// only "not shown here": the caller then runs the previous check.
///
/// `true` requires everything `open_v1` and the comparison require:
/// - `sealed` starts with the zstd magic and `bytes` does not (so the
///   content, equal to `bytes`, is not itself sealed);
/// - the frame declares its content size, at most `bytes.len()`
///   (`sealed_content_length_v1`, as `open_v1` checks it);
/// - the decoder reports no error (a bad block, or a content checksum or
///   declared size that the content does not match) and finishes the frame;
/// - the frame is all of `sealed` (no trailing or second frame);
/// - the content is `bytes`, byte for byte and in length.
///
/// The decoder verifies the content checksum and the declared content size
/// against what it wrote, so a finished frame whose content is `bytes` has
/// declared `bytes.len()`, and `open_v1`'s one-shot decompression of the same
/// valid frame gives the same content.
fn round_trip_streams_v1(sealed: &[u8], bytes: &[u8]) -> bool {
    use zstd::zstd_safe::{DCtx, InBuffer, OutBuffer};
    if !is_sealed_v1(sealed) || is_sealed_v1(bytes) {
        return false;
    }
    if sealed_content_length_v1(sealed, bytes.len() as u64).is_err() {
        return false;
    }
    let mut context = DCtx::create();
    let mut chunk = vec![0u8; ROUND_TRIP_CHUNK_BYTES_V1];
    let mut input = InBuffer::around(sealed);
    let mut compared = 0usize;
    loop {
        let mut output = OutBuffer::around(&mut chunk[..]);
        let Ok(remaining) = context.decompress_stream(&mut output, &mut input) else {
            return false;
        };
        let written = output.pos();
        let produced = &chunk[..written];
        let Some(expected) = bytes.get(compared..compared + produced.len()) else {
            return false;
        };
        if produced != expected {
            return false;
        }
        compared += produced.len();
        if remaining == 0 {
            break;
        }
        // Unfinished, with room left in the output: the decoder has used all
        // of the input and needs more, so the frame is incomplete.
        if produced.len() < ROUND_TRIP_CHUNK_BYTES_V1 {
            return false;
        }
    }
    input.pos() == sealed.len() && compared == bytes.len()
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

/// The longest zstd frame header: magic, descriptor, window, a 4-byte
/// dictionary id and an 8-byte content size.
const FRAME_HEADER_MAX_BYTES_V1: usize = 18;

/// The error of a batch refused by [`admit_batch_v1`]: the update's own
/// batch-bound error, so both checks read the same.
pub(crate) const BATCH_BOUND_ERROR_V1: &str =
    "trajectory batch exceeds 512 MiB CPU ingestion bound";

/// The batch-bound error for a bound of `bound` bytes: for the default bound
/// exactly [`BATCH_BOUND_ERROR_V1`], for a configured whole number of MiB the
/// same text with that number.
pub(crate) fn batch_bound_error_v1(bound: u64) -> String {
    if bound == super::MAX_BATCH_BYTES {
        BATCH_BOUND_ERROR_V1.into()
    } else if bound.is_multiple_of(1 << 20) {
        format!(
            "trajectory batch exceeds {} MiB CPU ingestion bound",
            bound >> 20
        )
    } else {
        format!("trajectory batch exceeds {bound}-byte CPU ingestion bound")
    }
}

/// What [`admit_batch_v1`] saw of one file: its declared content size when
/// it was a sealed regular file with a readable frame header, else nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AdmittedV1(Option<u64>);

impl AdmittedV1 {
    /// Checks a file's verified bytes against what admission saw of it: a
    /// sealed file must be the sealed file of the same declared size that
    /// admission counted, so the admitted total bounds what is opened even
    /// if a file changed between admission and its pinned read. Plain bytes
    /// never expand and pass as before.
    pub(crate) fn check_v1(self, verified: &[u8]) -> Result<(), String> {
        if !is_sealed_v1(verified) {
            return Ok(());
        }
        match self.0 {
            Some(declared) if record_length_v1(verified, u64::MAX).ok() == Some(declared) => Ok(()),
            _ => Err("a sealed record differs from the one its batch admitted".into()),
        }
    }
}

/// Refuses, before any file of the batch is opened, a batch that holds a
/// sealed file and whose record lengths, in order, pass `bound`. A file's
/// record length is its sealed frame's declared content size, read from the
/// frame header in its first bytes, else its length on disk. Only regular
/// files are opened, as the pinned readers check. Nothing is verified here:
/// a file that cannot be read or measured counts as empty and fails its own
/// pinned read later, exactly as before. Returns what was seen of each file,
/// which its reader checks against the verified bytes before opening them
/// ([`AdmittedV1::check_v1`]). A batch without a sealed file is always
/// admitted, so plain runs keep their checks and errors.
pub(crate) fn admit_batch_v1<'a>(
    paths: impl IntoIterator<Item = &'a std::path::Path>,
    bound: u64,
) -> Result<Vec<AdmittedV1>, String> {
    use std::io::Read;
    let mut total = 0u64;
    let mut sealed = false;
    let mut admitted = Vec::new();
    for path in paths {
        let mut seen = AdmittedV1(None);
        let length = (|| -> Option<u64> {
            if !std::fs::metadata(path).ok()?.is_file() {
                return None;
            }
            let mut file = std::fs::File::open(path).ok()?;
            let mut header = [0u8; FRAME_HEADER_MAX_BYTES_V1];
            let mut filled = 0;
            while filled < header.len() {
                match file.read(&mut header[filled..]).ok()? {
                    0 => break,
                    read => filled += read,
                }
            }
            if is_sealed_v1(&header[..filled]) {
                sealed = true;
                let declared = zstd::zstd_safe::get_frame_content_size(&header[..filled]);
                let declared = (declared != zstd::zstd_safe::CONTENTSIZE_ERROR
                    && declared != zstd::zstd_safe::CONTENTSIZE_UNKNOWN)
                    .then_some(declared);
                seen = AdmittedV1(declared);
                declared
            } else {
                file.metadata().ok().map(|metadata| metadata.len())
            }
        })()
        .unwrap_or(0);
        total = total.saturating_add(length);
        admitted.push(seen);
    }
    if sealed && total > bound {
        return Err(batch_bound_error_v1(bound));
    }
    Ok(admitted)
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
        // The compressor's worst-case allocation (more than the record's
        // length) is not kept: the frame's buffer has little capacity
        // beyond its bytes. (A `Vec` reports the same capacity after an
        // in-place shrink that the allocator did not honor, so this cannot
        // tell the copy from `shrink_to_fit`; a trainer process's peak
        // memory is the measurement that does.)
        assert!(
            sealed.capacity() <= sealed.len() + sealed.len() / 8 + 64,
            "capacity {} for {} bytes",
            sealed.capacity(),
            sealed.len()
        );
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

    /// A frame header declaring `declared` content bytes, followed by one
    /// empty last raw block: it measures as `declared` and never opens.
    fn declared_only_frame(declared: u64) -> Vec<u8> {
        let mut frame = ZSTD_MAGIC_V1.to_vec();
        frame.push(0xe0); // 8-byte content size, single segment, no checksum or dictionary
        frame.extend_from_slice(&declared.to_le_bytes());
        frame.extend_from_slice(&[0x01, 0x00, 0x00]);
        frame
    }

    #[test]
    fn a_batch_is_admitted_in_order_from_its_declared_sizes_before_any_is_opened() {
        let directory = std::env::temp_dir().join(format!(
            "record-envelope-admit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let path = directory.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        let big = write("big.json", &declared_only_frame(300 << 20));
        assert_eq!(
            record_length_v1(&std::fs::read(&big).unwrap(), u64::MAX).unwrap(),
            300 << 20
        );
        assert!(open_v1(&std::fs::read(&big).unwrap(), u64::MAX).is_err());
        let plain = write("plain.json", &[b' '; 1000]);
        let missing = directory.join("missing.json");
        let bound = 512u64 << 20;
        let seen = admit_batch_v1(
            [
                big.as_path(),
                plain.as_path(),
                missing.as_path(),
                directory.as_path(),
            ],
            bound,
        )
        .unwrap();
        assert_eq!(
            seen,
            [
                AdmittedV1(Some(300 << 20)),
                AdmittedV1(None),
                AdmittedV1(None),
                AdmittedV1(None)
            ]
        );
        // A reader opens a sealed file only if admission counted that file.
        let big_bytes = std::fs::read(&big).unwrap();
        seen[0].check_v1(&big_bytes).unwrap();
        seen[1].check_v1(&[b' '; 1000]).unwrap();
        assert!(seen[1].check_v1(&big_bytes).is_err());
        assert!(AdmittedV1(Some(1)).check_v1(&big_bytes).is_err());
        assert_eq!(
            admit_batch_v1([big.as_path(), big.as_path()], bound).unwrap_err(),
            BATCH_BOUND_ERROR_V1
        );
        // The same total with no sealed file is admitted: plain batches keep
        // their own checks.
        admit_batch_v1([plain.as_path(); 3], 2000).unwrap();
        assert!(admit_batch_v1([plain.as_path(), big.as_path()], (300 << 20) + 999).is_err());
        // A raised bound (`max_trajectory_batch_mebibytes`) admits the pair
        // and names itself when exceeded.
        admit_batch_v1([big.as_path(), big.as_path()], 600 << 20).unwrap();
        assert_eq!(
            admit_batch_v1([big.as_path(), big.as_path(), big.as_path()], 600 << 20).unwrap_err(),
            "trajectory batch exceeds 600 MiB CPU ingestion bound"
        );
        assert_eq!(batch_bound_error_v1(bound), BATCH_BOUND_ERROR_V1);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    /// A frame of `bytes` written as `seal_v1` writes it, without its
    /// self-check; `checksum: false` drops the content checksum.
    fn frame_v1(bytes: &[u8], checksum: bool) -> Vec<u8> {
        let mut compressor = zstd::bulk::Compressor::new(ZSTD1_LEVEL_V1).unwrap();
        compressor
            .set_parameter(zstd::stream::raw::CParameter::ChecksumFlag(checksum))
            .unwrap();
        compressor
            .set_parameter(zstd::stream::raw::CParameter::ContentSizeFlag(true))
            .unwrap();
        compressor.compress(bytes).unwrap()
    }

    /// The self-check against the previous one: the same result and error;
    /// and the streamed check never accepts what the previous one refuses.
    fn assert_same_round_trip_v1(sealed: &[u8], bytes: &[u8]) -> Result<(), String> {
        let reference = reference_round_trip_v1(sealed, bytes);
        assert_eq!(check_round_trip_v1(sealed, bytes), reference);
        if reference.is_err() {
            assert!(
                !round_trip_streams_v1(sealed, bytes),
                "streamed acceptance of a refused frame: {reference:?}"
            );
        }
        reference
    }

    /// `length` bytes of JSON-like text with some repetition.
    fn text_v1(length: usize) -> Vec<u8> {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                b"0123456789,[]{}\"a"[(state % 17) as usize]
            })
            .collect()
    }

    #[test]
    fn the_streamed_round_trip_check_is_the_previous_check_v1() {
        let chunk = ROUND_TRIP_CHUNK_BYTES_V1;
        for length in [0, 1, 100, chunk - 1, chunk, chunk + 1, 3 * chunk + 7] {
            let bytes = text_v1(length);
            let sealed = frame_v1(&bytes, true);
            // A genuine frame is accepted by the streamed check itself.
            assert!(round_trip_streams_v1(&sealed, &bytes), "length {length}");
            assert_eq!(assert_same_round_trip_v1(&sealed, &bytes), Ok(()));
            // Content that differs at its first, middle or last byte, or is
            // one byte longer or shorter.
            if let Some(last) = length.checked_sub(1) {
                for at in [0, length / 2, last] {
                    let mut other = bytes.clone();
                    other[at] ^= 1;
                    assert!(assert_same_round_trip_v1(&sealed, &other).is_err());
                }
                assert!(assert_same_round_trip_v1(&sealed, &bytes[..last]).is_err());
            }
            let mut longer = bytes.clone();
            longer.push(b'0');
            assert!(assert_same_round_trip_v1(&sealed, &longer).is_err());
        }

        let bytes = record();
        let sealed = frame_v1(&bytes, true);
        // Every truncation (including one missing only its checksum) and
        // every single-bit corruption at two bit positions of every byte.
        for end in 0..sealed.len() {
            assert!(assert_same_round_trip_v1(&sealed[..end], &bytes).is_err());
        }
        for at in 0..sealed.len() {
            for mask in [0x01, 0x80] {
                let mut corrupt = sealed.clone();
                corrupt[at] ^= mask;
                let _ = assert_same_round_trip_v1(&corrupt, &bytes);
            }
        }
        // A trailing byte, and a second frame.
        let mut trailing = sealed.clone();
        trailing.push(0);
        assert!(assert_same_round_trip_v1(&trailing, &bytes).is_err());
        let mut two = sealed.clone();
        two.extend_from_slice(&sealed);
        assert!(assert_same_round_trip_v1(&two, &bytes).is_err());
        // A frame without a checksum opens, as before.
        let unchecked = frame_v1(&bytes, false);
        assert_eq!(assert_same_round_trip_v1(&unchecked, &bytes), Ok(()));
        // A frame without a declared content size is refused.
        let mut encoder = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
        std::io::Write::write_all(&mut encoder, &bytes).unwrap();
        let streamed = encoder.finish().unwrap();
        assert!(assert_same_round_trip_v1(&streamed, &bytes)
            .unwrap_err()
            .contains("declares its content size"));
        // A frame whose content is itself sealed is refused.
        let double = frame_v1(&sealed, true);
        assert!(assert_same_round_trip_v1(&double, &sealed)
            .unwrap_err()
            .contains("unsealed"));
        // Bytes that are not a frame pass only when they are the record.
        assert_eq!(assert_same_round_trip_v1(&bytes, &bytes), Ok(()));
        assert!(assert_same_round_trip_v1(&bytes, &bytes[1..]).is_err());
        // An empty skippable frame decodes to nothing but is not sealed.
        let skippable = [0x50, 0x2a, 0x4d, 0x18, 0, 0, 0, 0];
        assert!(assert_same_round_trip_v1(&skippable, b"").is_err());
        // `seal_v1` returns the frame it checked.
        assert_eq!(
            RecordEnvelopeV1::Zstd1.seal_v1(&bytes).unwrap().as_ref(),
            sealed.as_slice()
        );
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
