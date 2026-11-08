//! In unit tests each test thread keeps its own counters (`counters_v1`), so
//! these tests count exactly their own calls. What a cache holds is shared
//! by every test, so only what a test itself put there is checked exactly.

use super::super::fresh_initialization_source::SOURCE_SCHEMA;
use super::super::{
    ordered_update_preparation, sha, write_synthetic_fresh_source_with_parameters_v1,
    ExpandedModelSourceV1, PinnedFileV1, VerifiedFilesV1,
};
use super::*;
use crate::sideboard_play_policy_v1::{FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V4};
use std::fs;

#[test]
fn counts_add_keep_peaks_and_never_fall_below_zero() {
    let count = CountV1::new();
    count.add(3);
    count.add_usize(2);
    assert_eq!(count.get(), 5);
    count.raise(4);
    assert_eq!(count.get(), 5);
    count.raise(9);
    assert_eq!(count.get(), 9);
    count.sub(4);
    assert_eq!(count.get(), 5);
    count.sub(7);
    assert_eq!(count.get(), 0);
    count.add(1_500_000_000);
    assert_eq!(count.seconds(), 1.5);
}

#[test]
fn a_mark_attributes_only_this_threads_work_after_it() {
    add_thread_work_v1(WorkV1::Read, 1_000);
    let split = SplitV1::default();
    let mark = WorkMarkV1::now_v1();
    add_thread_work_v1(WorkV1::Read, 5);
    add_thread_work_v1(WorkV1::Verify, 7);
    add_thread_work_v1(WorkV1::Decode, 11);
    // Another thread's work is not this call's.
    std::thread::spawn(|| add_thread_work_v1(WorkV1::Read, 1_000_000))
        .join()
        .unwrap();
    mark.finish_v1(&split);
    assert_eq!(
        (split.read.get(), split.verify.get(), split.decode.get()),
        (5, 7, 11)
    );
    let report = split.report_v1();
    assert_eq!(report["seconds"], split.total.seconds());
    let other = split.total.get().saturating_sub(23) as f64 * 1e-9;
    assert_eq!(report["other_seconds"], other);
    // A second call adds to the same split.
    let mark = WorkMarkV1::now_v1();
    add_thread_work_v1(WorkV1::Decode, 2);
    mark.finish_v1(&split);
    assert_eq!(split.decode.get(), 13);
}

#[test]
fn file_bytes_count_each_path_and_content_once() {
    let import = Arc::new(b"import".to_vec());
    let memo: Vec<HeldFileV1> = vec![
        (PathBuf::from("/held/import.json"), Arc::clone(&import)),
        (PathBuf::from("/held/a.ckpt"), Arc::new(b"aaaa".to_vec())),
    ];
    // Two opponent entries each keep their own copy of a shared import; the
    // first also keeps the checkpoint the memo holds.
    let first: VerifiedFilesV1 = vec![
        (PathBuf::from("/held/import.json"), b"import".to_vec()),
        (PathBuf::from("/held/a.ckpt"), b"aaaa".to_vec()),
    ];
    let second: VerifiedFilesV1 = vec![
        (PathBuf::from("/held/import.json"), b"import".to_vec()),
        (PathBuf::from("/held/b.ckpt"), b"bbbbb".to_vec()),
        // Equal bytes under another path are another file.
        (PathBuf::from("/held/c.ckpt"), b"bbbbb".to_vec()),
    ];
    // Other bytes under a held path are other content.
    let changed: VerifiedFilesV1 = vec![(PathBuf::from("/held/a.ckpt"), b"AAAA".to_vec())];
    let bytes = |holders: &[&dyn HeldFilesV1]| {
        let counted = file_bytes_v1(holders);
        (counted.total, counted.unique)
    };
    assert_eq!(bytes(&[&memo]), (10, 10));
    assert_eq!(bytes(&[&first, &second]), (26, 20));
    assert_eq!(bytes(&[&memo, &first, &second]), (36, 20));
    assert_eq!(bytes(&[&memo, &first, &second, &changed]), (40, 24));
    // The same shared allocation twice is one content.
    let again: Vec<HeldFileV1> = vec![(PathBuf::from("/held/import.json"), import)];
    assert_eq!(bytes(&[&memo, &again]), (16, 10));
    assert_eq!(bytes(&[]), (0, 0));
}

#[test]
fn every_section_reports_an_object() {
    let sections = sections_v1();
    let names: Vec<&str> = sections.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "opponent_cache",
            "verified_bytes",
            "retained_files",
            "io",
            "process"
        ]
    );
    for (name, section) in &sections {
        assert!(section.is_object(), "{name}");
    }
    let process = &sections[4].1;
    if cfg!(target_os = "linux") {
        let peak = process["peak_rss_bytes"].as_u64().unwrap();
        let current = process["rss_bytes"].as_u64().unwrap();
        assert!(peak >= current && current > 0);
    }
}

/// A V4 fresh-initialization source without a checkpoint, under `root`,
/// and the paths of the files a full load of it reads.
fn fresh_source_v1(root: &Path) -> (ExpandedModelSourceV1, Vec<PathBuf>) {
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor =
        write_synthetic_fresh_source_with_parameters_v1(&root.join("source"), identity, |_| {});
    assert_eq!(descriptor.schema, SOURCE_SCHEMA);
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = root.join("descriptor.json");
    fs::write(&path, &bytes).unwrap();
    let read = vec![
        path.clone(),
        descriptor.initialization.path.clone(),
        descriptor.parameters.path.clone(),
    ];
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    (source, read)
}

/// Runs `body` on a 64 MiB thread, whose counters are its own: unoptimized
/// loaders keep several policies in a frame.
fn on_large_stack_v1(body: fn()) {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(body)
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn the_process_opponent_cache_counts_a_full_load_and_its_hits() {
    on_large_stack_v1(the_process_opponent_cache_counts_a_full_load_and_its_hits_v1);
}

fn the_process_opponent_cache_counts_a_full_load_and_its_hits_v1() {
    let root = std::env::temp_dir().join(format!(
        "opponent-cache-diagnostics-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let (source, read) = fresh_source_v1(&root);
    let on_disk: u64 = read
        .iter()
        .map(|path| fs::metadata(path).unwrap().len())
        .sum();
    let counters = counters_v1();
    let stats = &counters.process_opponent;
    let count = || {
        [
            stats.lookups.get(),
            stats.misses_absent.get(),
            stats.misses_check_setting.get(),
            stats.misses_revalidation.get(),
            stats.full_loads.get(),
            stats.kept.get(),
            stats.hits_revalidated.get(),
            stats.hits_trusted.get(),
            stats.captured_bytes.get(),
            stats.revalidated_bytes.get(),
            stats.forks.get(),
        ]
    };
    let before = count();
    let reads_before = counters.file_reads.bytes.get();
    let forked_before = stats.forked_policy_bytes.get();
    let load = |revalidate| {
        ordered_update_preparation::load_opponent_cached_v1(&source, revalidate)
            .unwrap()
            .1
    };
    // A full load, then a prepared update's hit (every file reread) and a
    // collection's hit (after the boundary's check).
    assert!(!load(true));
    assert!(load(true));
    assert!(load(false));
    let now = count();
    let rose: Vec<u64> = (0..now.len())
        .map(|index| now[index] - before[index])
        .collect();
    // Its insertion can evict another test's entry from the shared cache,
    // so evictions are not checked here. Each of the three loads served a
    // fork.
    assert_eq!(rose, [3, 1, 0, 0, 1, 1, 1, 1, on_disk, on_disk, 3]);
    // The full load read the three files, and the capture read them again.
    assert!(counters.file_reads.bytes.get() - reads_before >= 2 * on_disk);
    for split in [&stats.full_load, &stats.capture] {
        let parts = split.read.get() + split.verify.get() + split.decode.get();
        assert!(split.read.get() > 0 && parts <= split.total.get());
    }
    // No checkpoint: nothing decoded.
    assert_eq!(stats.capture.decode.get(), 0);
    assert!(stats.hit_max_nanos.get() > 0 && stats.hit_nanos.get() >= stats.hit_max_nanos.get());
    // The entry's files are held, and reported in the sections.
    let held = ordered_update_preparation::held_v1();
    let entry = held
        .files
        .iter()
        .find(|files| {
            files
                .iter()
                .any(|(path, _)| *path == source.play_import.path)
        })
        .expect("the source is held");
    assert_eq!(
        entry
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .sum::<u64>(),
        on_disk
    );
    assert!(held.policy_bytes > 0);
    assert!(held.models >= 1 && held.models <= held.files.len() && held.model_bytes > 0);
    let section = opponent_cache_section_v1();
    assert_eq!(section["process"]["lookups"], stats.lookups.get());
    assert_eq!(section["process"]["forks"], stats.forks.get());
    // The three forks copied the same policy's own bytes; the model is
    // shared, not copied.
    let forked = stats.forked_policy_bytes.get() - forked_before;
    assert!(forked > 0 && forked.is_multiple_of(3));
    assert!(section["process"]["entries"].as_u64().unwrap() >= 1);
    assert!(section["process"]["file_bytes"].as_u64().unwrap() >= on_disk);
    let retained = retained_files_section_v1();
    assert!(retained["opponent_cache_bytes"].as_u64().unwrap() >= on_disk);
    fs::remove_dir_all(&root).unwrap();
}
