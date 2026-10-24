//! Every test uses its own cache, never the process cache, so none depends
//! on what other tests hold or on `MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB`.

use super::super::fresh_initialization_source::read_actual_in_v1;
use super::super::{publish_json_bytes_in_v1, read_pinned_bytes_in_v1, sha, PinnedFileV1};
use super::{charge_v1, VerifiedBytesCacheV1};
use std::fs;
use std::path::{Path, PathBuf};

fn held_entries_v1(cache: &VerifiedBytesCacheV1) -> (Vec<String>, usize) {
    let held = cache.held.lock().unwrap();
    (
        held.entries
            .iter()
            .map(|entry| entry.sha256.clone())
            .collect(),
        held.charged,
    )
}

#[test]
fn held_bytes_are_keyed_by_path_and_digest() {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let (a, b) = (Path::new("/memo/a"), Path::new("/memo/b"));
    assert!(cache.held_v1(a, "x").is_none());
    cache.record_verified_v1(a, "x", b"alpha");
    assert_eq!(cache.held_v1(a, "x").unwrap().as_slice(), b"alpha");
    assert!(cache.held_v1(a, "y").is_none());
    assert!(cache.held_v1(b, "x").is_none());
    // A zero budget turns the cache off, and a zero entry limit holds nothing.
    let off = VerifiedBytesCacheV1::new_v1(0, 16);
    off.record_verified_v1(a, "x", b"");
    assert!(off.held_v1(a, "x").is_none());
    assert_eq!(held_entries_v1(&off), (vec![], 0));
    let none = VerifiedBytesCacheV1::new_v1(1 << 20, 0);
    none.record_verified_v1(a, "x", b"alpha");
    assert!(none.held_v1(a, "x").is_none());
}

#[test]
fn least_recently_used_entries_are_dropped_before_new_bytes_are_copied_in() {
    let path = Path::new("/memo/lru");
    // Every entry here is charged `overhead` plus its byte count.
    let overhead = charge_v1(path, "a", 0);
    let cache = VerifiedBytesCacheV1::new_v1(2 * overhead + 10, 16);
    cache.record_verified_v1(path, "a", b"aaaa");
    cache.record_verified_v1(path, "b", b"bbbb");
    assert_eq!(
        held_entries_v1(&cache),
        (vec!["a".into(), "b".into()], 2 * overhead + 8)
    );
    // A hit makes "a" the most recently used, so "b" is dropped for "c".
    assert!(cache.held_v1(path, "a").is_some());
    cache.record_verified_v1(path, "c", b"cccccc");
    assert_eq!(
        held_entries_v1(&cache),
        (vec!["a".into(), "c".into()], 2 * overhead + 10)
    );
    // Re-recording a held pin moves it to the back without double charging.
    cache.record_verified_v1(path, "a", b"aaaa");
    assert_eq!(
        held_entries_v1(&cache),
        (vec!["c".into(), "a".into()], 2 * overhead + 10)
    );
    // An entry charged more than the whole budget is not held, and nothing
    // is dropped for it.
    cache.record_verified_v1(path, "e", &vec![b'e'; overhead + 11]);
    assert_eq!(
        held_entries_v1(&cache),
        (vec!["c".into(), "a".into()], 2 * overhead + 10)
    );
    assert!(cache.held_v1(path, "e").is_none());
}

#[test]
fn empty_files_are_charged_and_counted() {
    let paths: Vec<PathBuf> = (0..6)
        .map(|i| PathBuf::from(format!("/memo/{i}")))
        .collect();
    let overhead = charge_v1(&paths[0], "x", 0);
    assert!(overhead > 0);
    // The charge bounds empty entries: three fit.
    let charged = VerifiedBytesCacheV1::new_v1(3 * overhead, 16);
    for path in &paths {
        charged.record_verified_v1(path, "x", b"");
    }
    assert_eq!(
        held_entries_v1(&charged),
        (vec![String::from("x"); 3], 3 * overhead)
    );
    assert!(charged.held_v1(&paths[2], "x").is_none());
    assert!(charged.held_v1(&paths[5], "x").is_some());
    // So does the entry limit, whatever the budget.
    let counted = VerifiedBytesCacheV1::new_v1(1 << 20, 2);
    for path in &paths {
        counted.record_verified_v1(path, "x", b"");
    }
    assert_eq!(
        held_entries_v1(&counted),
        (vec![String::from("x"); 2], 2 * overhead)
    );
    assert!(counted.held_v1(&paths[3], "x").is_none());
    assert!(counted.held_v1(&paths[4], "x").is_some());
}

#[test]
fn digest_matches_compares_held_bytes_and_hashes_otherwise() {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let path = Path::new("/memo/digest");
    let pin = sha(b"alpha");
    assert!(cache.digest_matches_v1(path, &pin, b"alpha", sha));
    assert_eq!(cache.hashed_for_test_v1(), 1);
    // Held: compared, not hashed.
    assert!(cache.digest_matches_v1(path, &pin, b"alpha", |_| unreachable!()));
    assert_eq!(cache.hashed_for_test_v1(), 1);
    // Different bytes of the same length: hashed, and rejected.
    assert!(!cache.digest_matches_v1(path, &pin, b"alphb", sha));
    assert_eq!(cache.hashed_for_test_v1(), 2);
    // A failed digest records nothing.
    assert!(!cache.digest_matches_v1(path, "0", b"alpha", sha));
    assert!(cache.held_v1(path, "0").is_none());
}

fn temp_directory_v1(label: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "verified-bytes-cache-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn pinned_file_v1(label: &str, bytes: &[u8]) -> PinnedFileV1 {
    let path = temp_directory_v1(label).join("input.json");
    fs::write(&path, bytes).unwrap();
    PinnedFileV1 {
        path,
        sha256: sha(bytes),
    }
}

#[test]
fn pinned_reads_compare_held_bytes_and_fail_as_before() {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let bytes = br#"{"verified":"original"}"#.to_vec();
    let pin = pinned_file_v1("reads", &bytes);
    assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 1);
    // The hashed read recorded the bytes; the next reads compare with them.
    assert_eq!(
        cache.held_v1(&pin.path, &pin.sha256).unwrap().as_slice(),
        bytes
    );
    for _ in 0..2 {
        assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
    }
    assert_eq!(cache.hashed_for_test_v1(), 1);
    // Same length, different content: the comparison fails, the hash runs,
    // and the read fails with the error it had without the cache.
    fs::write(&pin.path, br#"{"verified":"changed!"}"#).unwrap();
    assert_eq!(
        read_pinned_bytes_in_v1(&cache, &pin).unwrap_err(),
        "input file SHA differs"
    );
    assert_eq!(cache.hashed_for_test_v1(), 2);
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 2);
    // A pin the bytes never matched is hashed and fails every time.
    let wrong = PinnedFileV1 {
        path: pin.path.clone(),
        sha256: "0".repeat(64),
    };
    for hashed in [3, 4] {
        assert_eq!(
            read_pinned_bytes_in_v1(&cache, &wrong).unwrap_err(),
            "input file SHA differs"
        );
        assert_eq!(cache.hashed_for_test_v1(), hashed);
    }
    // A deleted file fails at its first read, as before.
    fs::remove_file(&pin.path).unwrap();
    assert_eq!(
        read_pinned_bytes_in_v1(&cache, &pin).unwrap_err(),
        fs::metadata(&pin.path).unwrap_err().to_string()
    );
    fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
}

#[test]
fn fresh_input_reads_compare_held_bytes_and_fail_as_before() {
    const CAP: usize = 1024;
    const DIFFERS: &str = "fresh input bytes differ from their size bound or SHA256 pin";
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let bytes = br#"{"fresh":"original"}"#.to_vec();
    let pin = pinned_file_v1("fresh", &bytes);
    for _ in 0..3 {
        assert_eq!(read_actual_in_v1(&cache, &pin, CAP).unwrap(), bytes);
    }
    assert_eq!(cache.hashed_for_test_v1(), 1);
    fs::write(&pin.path, br#"{"fresh":"changed!"}"#).unwrap();
    assert_eq!(read_actual_in_v1(&cache, &pin, CAP).unwrap_err(), DIFFERS);
    assert_eq!(cache.hashed_for_test_v1(), 2);
    // Held bytes never relax the size bound, which is checked first.
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(
        read_actual_in_v1(&cache, &pin, bytes.len() - 1).unwrap_err(),
        "fresh input is not a bounded regular file"
    );
    assert_eq!(read_actual_in_v1(&cache, &pin, CAP).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 2);
    fs::remove_file(&pin.path).unwrap();
    assert_eq!(
        read_actual_in_v1(&cache, &pin, CAP).unwrap_err(),
        fs::File::open(&pin.path).unwrap_err().to_string()
    );
    fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
}

#[test]
fn publication_records_its_bytes_only_after_success() {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let directory = temp_directory_v1("publish");
    let bytes = br#"{"published":1}"#.to_vec();
    let pin = publish_json_bytes_in_v1(&cache, &directory, "document.json", &bytes).unwrap();
    assert_eq!(pin.sha256, sha(&bytes));
    assert_eq!(
        cache.held_v1(&pin.path, &pin.sha256).unwrap().as_slice(),
        bytes
    );
    assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 0);
    // A second publication under the same name fails; its bytes are not held.
    let other = br#"{"published":2}"#.to_vec();
    assert!(publish_json_bytes_in_v1(&cache, &directory, "document.json", &other).is_err());
    assert!(cache.held_v1(&pin.path, &sha(&other)).is_none());
    // So does a publication into a missing directory.
    let missing = directory.join("missing");
    assert!(publish_json_bytes_in_v1(&cache, &missing, "document.json", &other).is_err());
    assert!(cache
        .held_v1(&missing.join("document.json"), &sha(&other))
        .is_none());
    assert_eq!(held_entries_v1(&cache).0, vec![pin.sha256.clone()]);
    // A published file changed afterwards fails its read as before.
    fs::write(&pin.path, br#"{"published":3}"#).unwrap();
    assert_eq!(
        read_pinned_bytes_in_v1(&cache, &pin).unwrap_err(),
        "input file SHA differs"
    );
    assert_eq!(cache.hashed_for_test_v1(), 1);
    fs::remove_dir_all(directory).unwrap();
}
