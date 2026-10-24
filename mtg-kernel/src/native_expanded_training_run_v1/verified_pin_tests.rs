//! `verify_pin` and `read_pin_bounded_bytes` with the verified-bytes memo
//! (`verified_bytes_cache_v1`): held bytes replace only the hash, and every
//! failure reads as before.

use super::{digest, read_pin_bounded_bytes, verify_pin, ARTIFACT_CAP};
use crate::expanded_deck_training_v1::verified_bytes_cache_v1::{held_v1, record_verified_v1};
use crate::expanded_deck_training_v1::PinnedFileV1;
use std::fs;

const DIFFERS: &str = "pinned artifact hash differs";

/// Several 64 KiB chunks of `verify_pin`'s loop, the last one partial.
fn artifact_bytes_v1() -> Vec<u8> {
    (0..200_000usize).map(|i| (i * 31 % 251) as u8).collect()
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
fn verify_pin_compares_held_bytes_and_fails_as_before() {
    let bytes = artifact_bytes_v1();
    let pin = pinned_artifact_v1("compare", &bytes);
    // Nothing held: the file is hashed, and `verify_pin` records nothing.
    verify_pin(&pin).unwrap();
    assert!(held_v1(&pin.path, &pin.sha256).is_none());
    // The bounded read hashes and records; `verify_pin` then compares.
    assert_eq!(read_pin_bounded_bytes(&pin, ARTIFACT_CAP).unwrap(), bytes);
    assert_eq!(held_v1(&pin.path, &pin.sha256).unwrap().as_slice(), bytes);
    verify_pin(&pin).unwrap();
    assert_eq!(read_pin_bounded_bytes(&pin, ARTIFACT_CAP).unwrap(), bytes);
    // Same length, one byte changed after the first chunks.
    let mut changed = bytes.clone();
    changed[150_000] ^= 1;
    let longer = [&bytes[..], &b"x"[..]].concat();
    for file in [
        &changed[..],
        &longer[..],
        &bytes[..bytes.len() - 1],
        &b""[..],
    ] {
        fs::write(&pin.path, file).unwrap();
        assert_eq!(verify_pin(&pin).unwrap_err(), DIFFERS);
        assert_eq!(
            read_pin_bounded_bytes(&pin, ARTIFACT_CAP).unwrap_err(),
            DIFFERS
        );
    }
    // Held bytes never relax the bound, which is checked first.
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(
        read_pin_bounded_bytes(&pin, bytes.len() as u64 - 1).unwrap_err(),
        "artifact is not a bounded regular file"
    );
    verify_pin(&pin).unwrap();
    fs::remove_file(&pin.path).unwrap();
    let missing = fs::File::open(&pin.path).unwrap_err().to_string();
    assert_eq!(verify_pin(&pin).unwrap_err(), missing);
    assert_eq!(
        read_pin_bounded_bytes(&pin, ARTIFACT_CAP).unwrap_err(),
        missing
    );
    fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
}

/// The memo never holds bytes whose digest is not their key. Recording such
/// bytes here against the file's true pin shows that, from the first chunk
/// that differs from the held bytes (or at the end), the digest covers
/// exactly the file's bytes: the matched held prefix, then the rest of the
/// file. The verification passes only because the file matches its pin.
#[test]
fn verify_pin_hashes_exactly_the_file_after_a_held_prefix() {
    let bytes = artifact_bytes_v1();
    let flipped = |at: usize| {
        let mut held = bytes.clone();
        held[at] ^= 1;
        held
    };
    for (label, held) in [
        ("late", flipped(150_000)),
        ("first", flipped(0)),
        ("longer", [&bytes[..], &b"tail"[..]].concat()),
        ("shorter", bytes[..100_000].to_vec()),
        ("chunk", bytes[..64 * 1024].to_vec()),
        ("empty", Vec::new()),
    ] {
        let pin = pinned_artifact_v1(label, &bytes);
        record_verified_v1(&pin.path, &pin.sha256, &held);
        verify_pin(&pin).unwrap();
        fs::write(&pin.path, &bytes[1..]).unwrap();
        assert_eq!(verify_pin(&pin).unwrap_err(), DIFFERS);
        fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
    }
}
