//! Every test uses its own cache, never the process cache, so none depends
//! on what other tests hold or on `MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB`.

use super::super::fresh_initialization_source::read_actual_in_v1;
use super::super::no_state_digest_v1::holds_bytes_in_v1;
use super::super::{
    check_pinned_in_v1, holds_pinned_in_v1, publish_json_bytes_in_v1, read_pinned_bytes_in_v1, sha,
    PinnedFileV1, PinnedOpponentFileV1,
};
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

fn temp_directory_v1(label: &str) -> crate::test_temp_dir_v1::TempTestDir {
    crate::test_temp_dir_v1::TempTestDir::new_v1(&format!("verified-bytes-cache-{label}"))
}

fn pinned_file_v1(
    label: &str,
    bytes: &[u8],
) -> (crate::test_temp_dir_v1::TempTestDir, PinnedFileV1) {
    let directory = temp_directory_v1(label);
    let path = directory.join("input.json");
    fs::write(&path, bytes).unwrap();
    (
        directory,
        PinnedFileV1 {
            path,
            sha256: sha(bytes),
        },
    )
}

#[test]
fn pinned_reads_compare_held_bytes_and_fail_as_before() {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let bytes = br#"{"verified":"original"}"#.to_vec();
    let (_dir, pin) = pinned_file_v1("reads", &bytes);
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
}

#[test]
fn fresh_input_reads_compare_held_bytes_and_fail_as_before() {
    const CAP: usize = 1024;
    const DIFFERS: &str = "fresh input bytes differ from their size bound or SHA256 pin";
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    let bytes = br#"{"fresh":"original"}"#.to_vec();
    let (_dir, pin) = pinned_file_v1("fresh", &bytes);
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
}

fn resident_cache_v1() -> VerifiedBytesCacheV1 {
    let cache = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    assert!(!cache.resident_v1());
    cache.enable_resident_for_test_v1();
    assert!(cache.resident_v1());
    cache
}

#[test]
fn resident_bytes_need_the_mode_the_pin_and_the_exact_length() {
    let path = Path::new("/memo/resident");
    let off = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    off.record_verified_v1(path, "x", b"alpha");
    assert!(off.resident_bytes_v1(path, "x", 5).is_none());
    let cache = resident_cache_v1();
    assert!(cache.resident_bytes_v1(path, "x", 5).is_none());
    cache.record_verified_v1(path, "x", b"alpha");
    assert_eq!(
        cache.resident_bytes_v1(path, "x", 5).unwrap().as_slice(),
        b"alpha"
    );
    for (sha256, length) in [("x", 4), ("x", 6), ("y", 5)] {
        assert!(cache.resident_bytes_v1(path, sha256, length).is_none());
    }
    assert!(cache
        .resident_bytes_v1(Path::new("/memo/other"), "x", 5)
        .is_none());
}

#[test]
fn resident_pinned_reads_reuse_verified_bytes_and_keep_the_metadata_checks() {
    let cache = resident_cache_v1();
    let bytes = br#"{"verified":"original"}"#.to_vec();
    let (_dir, pin) = pinned_file_v1("resident-reads", &bytes);
    // The first read verifies in full; later reads and checks reuse its bytes.
    assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
    assert_eq!(cache.hashed_for_test_v1(), 1);
    // The mode's contract: a same-length rewrite later in this process is not
    // reread, and the verified bytes, which have the pinned digest, are served.
    fs::write(&pin.path, br#"{"verified":"changed!"}"#).unwrap();
    for _ in 0..2 {
        assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
        check_pinned_in_v1(&cache, &pin).unwrap();
        assert_eq!(read_actual_in_v1(&cache, &pin, 1024).unwrap(), bytes);
    }
    assert_eq!(cache.hashed_for_test_v1(), 1);
    // A changed length is read in full and fails as before, as does a pin
    // the cache holds nothing for.
    fs::write(&pin.path, br#"{"verified":"longer, changed"}"#).unwrap();
    assert_eq!(
        read_pinned_bytes_in_v1(&cache, &pin).unwrap_err(),
        "input file SHA differs"
    );
    assert_eq!(
        check_pinned_in_v1(&cache, &pin).unwrap_err(),
        "input file SHA differs"
    );
    let wrong = PinnedFileV1 {
        path: pin.path.clone(),
        sha256: "0".repeat(64),
    };
    assert_eq!(
        check_pinned_in_v1(&cache, &wrong).unwrap_err(),
        "input file SHA differs"
    );
    // The size bound and a deleted file still fail first.
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(
        read_actual_in_v1(&cache, &pin, bytes.len() - 1).unwrap_err(),
        "fresh input is not a bounded regular file"
    );
    fs::remove_file(&pin.path).unwrap();
    let missing = fs::metadata(&pin.path).unwrap_err().to_string();
    assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap_err(), missing);
    assert_eq!(check_pinned_in_v1(&cache, &pin).unwrap_err(), missing);
    fs::create_dir(&pin.path).unwrap();
    assert_eq!(
        read_pinned_bytes_in_v1(&cache, &pin).unwrap_err(),
        "input is not a bounded regular file"
    );
    assert_eq!(
        check_pinned_in_v1(&cache, &pin).unwrap_err(),
        "input is not a bounded regular file"
    );
}

#[test]
fn resident_publication_holds_its_bytes_and_later_reads_reuse_them() {
    let cache = resident_cache_v1();
    let directory = temp_directory_v1("resident-publish");
    let bytes = br#"{"published":1}"#.to_vec();
    let pin = publish_json_bytes_in_v1(&cache, &directory, "document.json", &bytes).unwrap();
    assert_eq!(pin.sha256, sha(&bytes));
    assert_eq!(fs::read(&pin.path).unwrap(), bytes);
    assert_eq!(read_pinned_bytes_in_v1(&cache, &pin).unwrap(), bytes);
    check_pinned_in_v1(&cache, &pin).unwrap();
    assert_eq!(cache.hashed_for_test_v1(), 0);
    // A second publication under the same name still fails, unheld.
    let other = br#"{"published":2}"#.to_vec();
    assert!(publish_json_bytes_in_v1(&cache, &directory, "document.json", &other).is_err());
    assert!(cache.held_v1(&pin.path, &sha(&other)).is_none());
    assert_eq!(fs::read(&pin.path).unwrap(), bytes);
}

#[test]
fn resident_cache_comparisons_check_metadata_without_reading() {
    let bytes = br#"{"cached":"original"}"#.to_vec();
    let (_dir, pin) = pinned_file_v1("resident-holds", &bytes);
    for resident in [false, true] {
        assert!(holds_bytes_in_v1(&pin.path, &bytes, resident));
    }
    fs::write(&pin.path, br#"{"cached":"changed!"}"#).unwrap();
    assert!(!holds_bytes_in_v1(&pin.path, &bytes, false));
    assert!(holds_bytes_in_v1(&pin.path, &bytes, true));
    fs::write(&pin.path, br#"{"cached":"longer, changed"}"#).unwrap();
    for resident in [false, true] {
        assert!(!holds_bytes_in_v1(&pin.path, &bytes, resident));
    }
    fs::remove_file(&pin.path).unwrap();
    for resident in [false, true] {
        assert!(!holds_bytes_in_v1(&pin.path, &bytes, resident));
    }
    fs::create_dir(&pin.path).unwrap();
    for resident in [false, true] {
        assert!(!holds_bytes_in_v1(&pin.path, &bytes, resident));
    }
}

#[test]
fn diagnostics_count_misses_by_reason_records_and_evictions() {
    let path = Path::new("/memo/diagnostics");
    let overhead = charge_v1(path, &sha(b"aaaa"), 0);
    let cache = VerifiedBytesCacheV1::new_v1(2 * overhead + 8, 16);
    let (a, b, c) = (sha(b"aaaa"), sha(b"bbbb"), sha(b"cccc"));
    // Absent: hashed and recorded. Held and equal: compared only.
    assert!(cache.digest_matches_v1(path, &a, b"aaaa", sha));
    assert!(cache.digest_matches_v1(path, &a, b"aaaa", sha));
    // Held but different: hashed, and the digest differs from the pin.
    assert!(!cache.digest_matches_v1(path, &a, b"aaab", sha));
    assert!(cache.digest_matches_v1(path, &b, b"bbbb", sha));
    // A third entry is over the budget: "a", the least recently used, goes.
    assert!(cache.digest_matches_v1(path, &c, b"cccc", sha));
    // A re-record of a held pin is counted as one.
    cache.record_verified_v1(path, &c, b"cccc");
    let report = cache.diagnostics_v1();
    let expected = serde_json::json!({
        "budget_bytes": 2 * overhead + 8,
        "entry_limit": 16,
        "entries": 2,
        "charged_bytes": 2 * overhead + 8,
        "held_bytes": 8,
        "peak_charged_bytes": 2 * overhead + 8,
        "lookups": 5,
        "lookup_hits": 2,
        "compared_hits": 1,
        "compared_hit_bytes": 4,
        "misses": {"disabled": 0, "absent": 3, "differ": 1},
        "hashed": 4,
        "hashed_bytes": 16,
        "hash_mismatches": 1,
        "records": 4,
        "recorded_bytes": 16,
        "copied_bytes": 16,
        "records_refused": 0,
        "rerecords": 1,
        "evictions": 1,
        "evicted_bytes": 4,
        "resident": false,
        "resident_hits": 0,
        "resident_hit_bytes": 0,
    });
    let mut timed = report.clone();
    for key in ["compare_seconds", "hash_seconds"] {
        assert!(timed[key].as_f64().unwrap() >= 0.0);
        timed.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(timed, expected);
    // The held files are the held entries' own bytes, not copies.
    let files = cache.held_files_v1();
    assert_eq!(files.len(), 2);
    for (held_path, bytes) in &files {
        assert_eq!(held_path, path);
        let sha256 = sha(bytes);
        assert!(std::sync::Arc::ptr_eq(
            bytes,
            &cache.held_v1(path, &sha256).unwrap()
        ));
    }
    // An off cache counts every lookup as a miss and refuses every record.
    let off = VerifiedBytesCacheV1::new_v1(0, 16);
    assert!(off.digest_matches_v1(path, &a, b"aaaa", sha));
    let report = off.diagnostics_v1();
    assert_eq!(report["misses"]["disabled"], 1);
    assert_eq!(report["hashed"], 1);
    assert_eq!(report["records"], 0);
    assert_eq!(report["records_refused"], 1);
}

#[test]
fn diagnostics_count_shared_records_and_resident_serves() {
    let path = Path::new("/memo/diagnostics-resident");
    let cache = resident_cache_v1();
    // A shared record holds the caller's allocation: nothing is copied.
    let shared = std::sync::Arc::new(b"shared".to_vec());
    cache.record_verified_shared_v1(path, "s", std::sync::Arc::clone(&shared));
    cache.record_verified_v1(path, "c", b"copied!");
    assert!(std::sync::Arc::ptr_eq(
        &cache.resident_bytes_v1(path, "s", 6).unwrap(),
        &shared
    ));
    // A different length is read as before, and is not a resident serve.
    assert!(cache.resident_bytes_v1(path, "c", 6).is_none());
    assert!(cache.resident_bytes_v1(path, "c", 7).is_some());
    let report = cache.diagnostics_v1();
    assert_eq!(report["resident"], true);
    let counts: Vec<u64> = [
        "records",
        "recorded_bytes",
        "copied_bytes",
        "resident_hits",
        "resident_hit_bytes",
        "lookups",
        "lookup_hits",
    ]
    .iter()
    .map(|key| report[*key].as_u64().unwrap())
    .collect();
    assert_eq!(counts, [2, 13, 7, 2, 13, 3, 3]);
    assert_eq!(
        VerifiedBytesCacheV1::new_v1(1 << 20, 16).diagnostics_v1()["resident"],
        false
    );
}

#[test]
fn stream_digest_matches_and_holds_pinned_enforce_strict_and_resident_contracts() {
    let payload: Vec<u8> = (0..(150 * 1024)).map(|i| (i * 31 + 7) as u8).collect();
    let (_dir, pin) = pinned_file_v1("stream-verify", &payload);
    let pinned = PinnedOpponentFileV1 {
        pin: pin.clone(),
        len: payload.len() as u64,
    };

    // Unheld streaming check hashes in 64 KiB chunks without inserting into or
    // churning the verified-bytes memo.
    let unheld = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    check_pinned_in_v1(&unheld, &pin).unwrap();
    assert_eq!(unheld.hashed_for_test_v1(), 1);
    assert_eq!(unheld.diagnostics_v1()["records"], 0);
    assert!(holds_pinned_in_v1(&unheld, &pinned, false));

    // Held streaming check compares chunks against the held allocation without
    // hashing again.
    let held = VerifiedBytesCacheV1::new_v1(1 << 20, 16);
    held.record_verified_v1(&pin.path, &pin.sha256, &payload);
    check_pinned_in_v1(&held, &pin).unwrap();
    assert_eq!(held.hashed_for_test_v1(), 0);
    assert_eq!(held.diagnostics_v1()["compared_hits"], 1);
    assert!(holds_pinned_in_v1(&held, &pinned, false));

    // Strict mode rejects same-length mutations, truncations, lengthening,
    // directories, and missing files whether bytes are held or unheld.
    let mut same_len_mutated = payload.clone();
    same_len_mutated[90 * 1024] ^= 0x55;
    fs::write(&pin.path, &same_len_mutated).unwrap();
    assert_eq!(
        check_pinned_in_v1(&unheld, &pin).unwrap_err(),
        "input file SHA differs"
    );
    assert_eq!(
        check_pinned_in_v1(&held, &pin).unwrap_err(),
        "input file SHA differs"
    );
    assert!(!holds_pinned_in_v1(&unheld, &pinned, false));
    assert!(!holds_pinned_in_v1(&held, &pinned, false));
    // Resident mode keeps its metadata-only contract for same-length rewrites.
    assert!(holds_pinned_in_v1(&held, &pinned, true));

    // Truncated and lengthened files fail both strict and resident checks.
    fs::write(&pin.path, &payload[..payload.len() - 1]).unwrap();
    assert!(!holds_pinned_in_v1(&held, &pinned, false));
    assert!(!holds_pinned_in_v1(&held, &pinned, true));
    let mut longer = payload.clone();
    longer.push(0);
    fs::write(&pin.path, &longer).unwrap();
    assert!(!holds_pinned_in_v1(&held, &pinned, false));
    assert!(!holds_pinned_in_v1(&held, &pinned, true));
    assert!(!held
        .stream_digest_matches_v1(
            std::io::Cursor::new(&longer),
            payload.len() as u64,
            0,
            &pin.path,
            &pin.sha256,
        )
        .unwrap());

    // Non-regular directory and missing file replacements fail closed.
    fs::remove_file(&pin.path).unwrap();
    assert!(!holds_pinned_in_v1(&held, &pinned, false));
    assert!(!holds_pinned_in_v1(&held, &pinned, true));
    fs::create_dir(&pin.path).unwrap();
    assert_eq!(
        check_pinned_in_v1(&unheld, &pin).unwrap_err(),
        "input is not a bounded regular file"
    );
    assert!(!holds_pinned_in_v1(&held, &pinned, false));
    assert!(!holds_pinned_in_v1(&held, &pinned, true));
    fs::remove_dir(&pin.path).unwrap();
    fs::write(&pin.path, &payload).unwrap();
    assert!(holds_pinned_in_v1(&held, &pinned, false));
}

/// A shared record keeps the publisher's whole allocation alive, so its
/// spare capacity counts against the budget: two shared buffers of four bytes
/// each with a large allocation do not both fit a budget that two four-byte
/// copies fit.
#[test]
fn a_shared_record_is_charged_the_capacity_it_keeps_alive() {
    let path = Path::new("/memo/capacity");
    let overhead = charge_v1(path, "a", 0);
    let roomy = |fill: u8| {
        let mut bytes = Vec::with_capacity(4096);
        bytes.extend_from_slice(&[fill; 4]);
        assert!(bytes.capacity() >= 4096);
        std::sync::Arc::new(bytes)
    };
    // Copies are charged their bytes: both fit.
    let copies = VerifiedBytesCacheV1::new_v1(2 * overhead + 4096, 16);
    copies.record_verified_v1(path, "a", &[1; 4]);
    copies.record_verified_v1(path, "b", &[2; 4]);
    assert_eq!(
        held_entries_v1(&copies),
        (vec!["a".into(), "b".into()], 2 * overhead + 8)
    );
    // The same bytes shared with a 4,096 byte allocation each: the second
    // drops the first, and the charge is the capacity.
    let shared = VerifiedBytesCacheV1::new_v1(2 * overhead + 4096, 16);
    let first = roomy(1);
    let second = roomy(2);
    let second_charge = overhead + second.capacity();
    shared.record_verified_shared_v1(path, "a", first);
    shared.record_verified_shared_v1(path, "b", second);
    assert_eq!(held_entries_v1(&shared), (vec!["b".into()], second_charge));
    assert_eq!(shared.diagnostics_v1()["evictions"], 1);
    // A shared buffer whose allocation exceeds the whole budget is not held,
    // however few bytes it has.
    let small = VerifiedBytesCacheV1::new_v1(overhead + 64, 16);
    small.record_verified_shared_v1(path, "a", roomy(3));
    assert_eq!(held_entries_v1(&small), (vec![], 0));
    assert_eq!(small.diagnostics_v1()["records_refused"], 1);
    // Recording a held pin again replaces its charge: a roomy buffer by a
    // copy, and back, leaves exactly the last one's charge.
    let again = VerifiedBytesCacheV1::new_v1(2 * overhead + 8192, 16);
    let wide = roomy(5);
    let wide_charge = overhead + wide.capacity();
    again.record_verified_shared_v1(path, "a", std::sync::Arc::clone(&wide));
    assert_eq!(held_entries_v1(&again), (vec!["a".into()], wide_charge));
    again.record_verified_v1(path, "a", &[5; 4]);
    assert_eq!(held_entries_v1(&again), (vec!["a".into()], overhead + 4));
    again.record_verified_shared_v1(path, "a", wide);
    assert_eq!(held_entries_v1(&again), (vec!["a".into()], wide_charge));
    // A shared buffer with no spare capacity is charged as a copy is.
    let exact = VerifiedBytesCacheV1::new_v1(2 * overhead + 4096, 16);
    let mut tight = vec![4u8; 4];
    tight.shrink_to_fit();
    let tight_capacity = tight.capacity();
    exact.record_verified_shared_v1(path, "a", std::sync::Arc::new(tight));
    assert_eq!(
        held_entries_v1(&exact),
        (vec!["a".into()], overhead + tight_capacity)
    );
}

/// One cache does not serve both read modes: resident reads do not turn on
/// under a run without them, an admitted resident run already keeps such runs
/// out, and a rejected one leaves the mode alone.
#[test]
fn a_cache_serves_one_read_mode() {
    let cache = VerifiedBytesCacheV1::new_v1(4096, 16);
    let first = cache.enter_run_v1(false).unwrap();
    let second = cache.enter_run_v1(false).unwrap();
    let refused = cache.enter_run_v1(true).err().unwrap();
    assert!(refused.contains("in flight"), "{refused}");
    drop(first);
    assert!(cache.enter_run_v1(true).is_err());
    drop(second);

    // Admitted and not accepted: the mode is still off, and a run without
    // resident reads is refused already.
    let pending = cache.enter_run_v1(true).unwrap();
    let beside = cache.enter_run_v1(true).unwrap();
    assert!(!cache.resident_v1());
    let refused = cache.enter_run_v1(false).err().unwrap();
    assert!(refused.contains("needs its own process"), "{refused}");
    // Both rejected before acceptance: nothing turned on, rereads run again.
    drop(pending);
    assert!(cache.enter_run_v1(false).is_err());
    drop(beside);
    assert!(!cache.resident_v1());
    drop(cache.enter_run_v1(false).unwrap());

    // Accepted: on, for good.
    let mut resident = cache.enter_run_v1(true).unwrap();
    assert!(!cache.resident_v1());
    resident.accept_v1();
    assert!(cache.resident_v1());
    resident.accept_v1();
    assert!(cache.enter_run_v1(false).is_err());
    drop(resident);
    assert!(cache.resident_v1());
    assert!(cache.enter_run_v1(false).is_err());
    // A later resident run is admitted, and its acceptance changes nothing.
    let mut later = cache.enter_run_v1(true).unwrap();
    later.accept_v1();
    drop(later);
    assert!(cache.resident_v1());
    assert!(cache.enter_run_v1(false).is_err());
}

/// Across threads: while one thread's run without resident reads is in
/// flight, another thread's resident run is refused and the mode stays off;
/// once that run has returned, the resident run is admitted.
#[test]
fn a_run_in_flight_on_another_thread_keeps_resident_reads_off() {
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let wait = Duration::from_secs(60);
    let cache = VerifiedBytesCacheV1::new_v1(4096, 16);
    let (admitted, is_admitted) = channel();
    let (release, released) = channel::<()>();
    std::thread::scope(|scope| {
        let shared = &cache;
        let run = scope.spawn(move || {
            let run = shared.enter_run_v1(false).unwrap();
            admitted.send(()).unwrap();
            // Held across what would be the run's reads.
            released.recv_timeout(wait).unwrap();
            assert!(!shared.resident_v1());
            drop(run);
        });
        is_admitted.recv_timeout(wait).unwrap();
        let refused = cache.enter_run_v1(true).err().unwrap();
        assert!(refused.contains("in flight"), "{refused}");
        assert!(!cache.resident_v1());
        release.send(()).unwrap();
        run.join().unwrap();
    });
    let mut resident = cache.enter_run_v1(true).unwrap();
    resident.accept_v1();
    assert!(cache.resident_v1());
    assert!(cache.enter_run_v1(false).is_err());
}
