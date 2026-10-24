use super::super::fresh_initialization_source::read_actual;
use super::super::{publish_json_bytes_v1, read_pinned_bytes, sha, PinnedFileV1};
use super::{held_in_v1, held_v1, record_in_v1, CacheV1};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn held_entries_v1(cache: &Mutex<CacheV1>) -> (Vec<String>, usize) {
    let cache = cache.lock().unwrap();
    (
        cache
            .entries
            .iter()
            .map(|entry| entry.sha256.clone())
            .collect(),
        cache.held_bytes,
    )
}

#[test]
fn held_bytes_are_keyed_by_path_and_digest() {
    let cache = Mutex::new(CacheV1::default());
    let (a, b) = (Path::new("/memo/a"), Path::new("/memo/b"));
    assert!(held_in_v1(&cache, 64, a, "x").is_none());
    record_in_v1(&cache, 64, a, "x", b"alpha");
    assert_eq!(held_in_v1(&cache, 64, a, "x").unwrap().as_slice(), b"alpha");
    assert!(held_in_v1(&cache, 64, a, "y").is_none());
    assert!(held_in_v1(&cache, 64, b, "x").is_none());
    // A zero budget turns the memo off even with bytes held, and holds nothing.
    assert!(held_in_v1(&cache, 0, a, "x").is_none());
    record_in_v1(&cache, 0, b, "z", b"");
    assert_eq!(held_entries_v1(&cache), (vec!["x".into()], 5));
}

#[test]
fn least_recently_used_entries_are_dropped_before_new_bytes_are_copied_in() {
    let cache = Mutex::new(CacheV1::default());
    let path = Path::new("/memo/lru");
    record_in_v1(&cache, 10, path, "a", b"aaaa");
    record_in_v1(&cache, 10, path, "b", b"bbbb");
    assert_eq!(held_entries_v1(&cache), (vec!["a".into(), "b".into()], 8));
    // A hit makes "a" the most recently used, so "b" is dropped for "c".
    assert!(held_in_v1(&cache, 10, path, "a").is_some());
    record_in_v1(&cache, 10, path, "c", b"cccccc");
    assert_eq!(held_entries_v1(&cache), (vec!["a".into(), "c".into()], 10));
    // Re-recording a held pin moves it to the back without double counting.
    record_in_v1(&cache, 10, path, "a", b"aaaa");
    assert_eq!(held_entries_v1(&cache), (vec!["c".into(), "a".into()], 10));
    // Bytes larger than the whole budget are not held, and nothing is dropped.
    record_in_v1(&cache, 10, path, "e", b"eeeeeeeeeee");
    assert_eq!(held_entries_v1(&cache), (vec!["c".into(), "a".into()], 10));
    assert!(held_in_v1(&cache, 10, path, "e").is_none());
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
fn pinned_reads_return_held_bytes_and_fail_as_before() {
    let bytes = br#"{"verified":"original"}"#.to_vec();
    let pin = pinned_file_v1("reads", &bytes);
    assert_eq!(read_pinned_bytes(&pin).unwrap(), bytes);
    // The hashed read recorded the bytes; the next reads compare with them.
    assert_eq!(held_v1(&pin.path, &pin.sha256).unwrap().as_slice(), bytes);
    for _ in 0..2 {
        assert_eq!(read_pinned_bytes(&pin).unwrap(), bytes);
    }
    // Same length, different content: the comparison fails, the hash runs,
    // and the read fails with the error it had without the memo.
    fs::write(&pin.path, br#"{"verified":"changed!"}"#).unwrap();
    assert_eq!(
        read_pinned_bytes(&pin).unwrap_err(),
        "input file SHA differs"
    );
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(read_pinned_bytes(&pin).unwrap(), bytes);
    // A pin the bytes never matched fails every time and is never held.
    let wrong = PinnedFileV1 {
        path: pin.path.clone(),
        sha256: "0".repeat(64),
    };
    for _ in 0..2 {
        assert_eq!(
            read_pinned_bytes(&wrong).unwrap_err(),
            "input file SHA differs"
        );
    }
    assert!(held_v1(&wrong.path, &wrong.sha256).is_none());
    // A deleted file fails at its first read, as before.
    fs::remove_file(&pin.path).unwrap();
    assert_eq!(
        read_pinned_bytes(&pin).unwrap_err(),
        fs::metadata(&pin.path).unwrap_err().to_string()
    );
    fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
}

#[test]
fn fresh_input_reads_return_held_bytes_and_fail_as_before() {
    const CAP: usize = 1024;
    const DIFFERS: &str = "fresh input bytes differ from their size bound or SHA256 pin";
    let bytes = br#"{"fresh":"original"}"#.to_vec();
    let pin = pinned_file_v1("fresh", &bytes);
    for _ in 0..3 {
        assert_eq!(read_actual(&pin, CAP).unwrap(), bytes);
    }
    assert!(held_v1(&pin.path, &pin.sha256).is_some());
    fs::write(&pin.path, br#"{"fresh":"changed!"}"#).unwrap();
    assert_eq!(read_actual(&pin, CAP).unwrap_err(), DIFFERS);
    // Held bytes never relax the size bound, which is checked first.
    fs::write(&pin.path, &bytes).unwrap();
    assert_eq!(
        read_actual(&pin, bytes.len() - 1).unwrap_err(),
        "fresh input is not a bounded regular file"
    );
    assert_eq!(read_actual(&pin, CAP).unwrap(), bytes);
    fs::remove_file(&pin.path).unwrap();
    assert_eq!(
        read_actual(&pin, CAP).unwrap_err(),
        fs::File::open(&pin.path).unwrap_err().to_string()
    );
    fs::remove_dir_all(pin.path.parent().unwrap()).unwrap();
}

#[test]
fn publication_records_its_bytes_only_after_success() {
    let directory = temp_directory_v1("publish");
    let bytes = br#"{"published":1}"#.to_vec();
    let pin = publish_json_bytes_v1(&directory, "document.json", &bytes).unwrap();
    assert_eq!(pin.sha256, sha(&bytes));
    assert_eq!(held_v1(&pin.path, &pin.sha256).unwrap().as_slice(), bytes);
    assert_eq!(read_pinned_bytes(&pin).unwrap(), bytes);
    // A second publication under the same name fails; its bytes are not held.
    let other = br#"{"published":2}"#.to_vec();
    assert!(publish_json_bytes_v1(&directory, "document.json", &other).is_err());
    assert!(held_v1(&pin.path, &sha(&other)).is_none());
    // So does a publication into a missing directory.
    let missing = directory.join("missing");
    assert!(publish_json_bytes_v1(&missing, "document.json", &other).is_err());
    assert!(held_v1(&missing.join("document.json"), &sha(&other)).is_none());
    // A published file changed afterwards fails its read as before.
    fs::write(&pin.path, br#"{"published":3}"#).unwrap();
    assert_eq!(
        read_pinned_bytes(&pin).unwrap_err(),
        "input file SHA differs"
    );
    fs::remove_dir_all(directory).unwrap();
}
