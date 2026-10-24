//! `verify_pin` and `read_pin_bounded_bytes` with the verified-bytes memo
//! (`verified_bytes_cache_v1`): held bytes replace only the hash, and every
//! failure reads as before. Every test uses its own cache or passes held
//! bytes directly, never the process cache.

use super::{
    digest, read_pin_bounded_bytes_in_v1, verify_pin_in_v1, verify_pin_stream_v1, ARTIFACT_CAP,
};
use crate::expanded_deck_training_v1::verified_bytes_cache_v1::VerifiedBytesCacheV1;
use crate::expanded_deck_training_v1::PinnedFileV1;
use std::fs;
use std::io::Read;

const DIFFERS: &str = "pinned artifact hash differs";

/// Several 64 KiB chunks of `verify_pin`'s loop, the last one partial.
fn artifact_bytes_v1() -> Vec<u8> {
    (0..200_000usize).map(|i| (i * 31 % 251) as u8).collect()
}

/// A reader returning at most `step` bytes per read, so chunks fall at
/// other offsets than a file's.
struct StepReaderV1<'a> {
    bytes: &'a [u8],
    step: usize,
}

impl Read for StepReaderV1<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.step.min(buffer.len()).min(self.bytes.len());
        buffer[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Ok(count)
    }
}

/// `verify_pin_stream_v1` over `file` with `held`, read whole and in
/// 1,000-byte steps; both must agree.
fn stream_v1(file: &[u8], held: Option<&[u8]>, sha256: &str) -> Result<bool, String> {
    let whole = verify_pin_stream_v1(&mut &file[..], held, sha256);
    let stepped = verify_pin_stream_v1(
        &mut StepReaderV1 {
            bytes: file,
            step: 1000,
        },
        held,
        sha256,
    );
    assert_eq!(whole, stepped);
    whole
}

#[test]
fn stream_compares_equal_held_bytes_and_hashes_everything_else() {
    let bytes = artifact_bytes_v1();
    let pin = digest(&bytes);
    // Nothing held, or held bytes equal to the file: hashed, or compared.
    assert_eq!(stream_v1(&bytes, None, &pin), Ok(true));
    assert_eq!(stream_v1(&bytes, Some(&bytes[..]), &pin), Ok(false));
    // The memo never holds bytes whose digest is not their pin. Holding such
    // bytes here against the file's true pin shows that, from the first chunk
    // that differs from them (or at the end), the hash covers exactly the
    // file's bytes: the matched held prefix, then the rest of the file. The
    // verification passes only because the file matches its pin, and it
    // reports that it hashed.
    let flipped = |at: usize| {
        let mut held = bytes.clone();
        held[at] ^= 1;
        held
    };
    for held in [
        flipped(150_000),
        flipped(0),
        flipped(bytes.len() - 1),
        [&bytes[..], &b"tail"[..]].concat(),
        bytes[..100_000].to_vec(),
        bytes[..64 * 1024].to_vec(),
        Vec::new(),
    ] {
        assert_eq!(stream_v1(&bytes, Some(&held[..]), &pin), Ok(true));
    }
    // A file that differs from its pin fails, whatever is held.
    let mut changed = bytes.clone();
    changed[150_000] ^= 1;
    let longer = [&bytes[..], &b"x"[..]].concat();
    for file in [
        &changed[..],
        &longer[..],
        &bytes[..bytes.len() - 1],
        &b""[..],
    ] {
        for held in [None, Some(&bytes[..])] {
            assert_eq!(stream_v1(file, held, &pin), Err(DIFFERS.to_string()));
        }
    }
}

fn pinned_artifact_v1(label: &str, bytes: &[u8]) -> PinnedFileV1 {
    let directory = std::env::temp_dir().join(format!(
        "verified-pin-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("artifact.json");
    fs::write(&path, bytes).unwrap();
    PinnedFileV1 {
        path,
        sha256: digest(bytes),
    }
}

#[test]
fn pinned_artifact_reads_compare_held_bytes_and_fail_as_before() {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let bytes = artifact_bytes_v1();
    let pin = pinned_artifact_v1("compare", &bytes);
    // Nothing held: `verify_pin` hashes and records nothing.
    verify_pin_in_v1(&cache, &pin).unwrap();
    assert!(cache.held_v1(&pin.path, &pin.sha256).is_none());
    // The bounded read hashes and records; later reads compare.
    let read = |cap| read_pin_bounded_bytes_in_v1(&cache, &pin, cap);
    assert_eq!(read(ARTIFACT_CAP).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 1);
    assert_eq!(
        cache.held_v1(&pin.path, &pin.sha256).unwrap().as_slice(),
        bytes
    );
    assert_eq!(read(ARTIFACT_CAP).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 1);
    verify_pin_in_v1(&cache, &pin).unwrap();
    // Changed, longer, shorter and empty files fail as before, hashed.
    let mut changed = bytes.clone();
    changed[150_000] ^= 1;
    let longer = [&bytes[..], &b"x"[..]].concat();
    for (hashed, file) in [
        (2, &changed[..]),
        (3, &longer[..]),
        (4, &bytes[..bytes.len() - 1]),
        (5, &b""[..]),
    ] {
        fs::write(&pin.path, file).unwrap();
        assert_eq!(verify_pin_in_v1(&cache, &pin).unwrap_err(), DIFFERS);
        assert_eq!(read(ARTIFACT_CAP).unwrap_err(), DIFFERS);
        assert_eq!(cache.hashed_for_test_v1(), hashed);
    }
    // Held bytes never relax the bound, which is checked first.
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(
        read(bytes.len() as u64 - 1).unwrap_err(),
        "artifact is not a bounded regular file"
    );
    verify_pin_in_v1(&cache, &pin).unwrap();
    fs::remove_file(&pin.path).unwrap();
    let missing = fs::File::open(&pin.path).unwrap_err().to_string();
    assert_eq!(verify_pin_in_v1(&cache, &pin).unwrap_err(), missing);
    assert_eq!(read(ARTIFACT_CAP).unwrap_err(), missing);
    fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
}
