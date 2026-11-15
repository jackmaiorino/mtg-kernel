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
        w_learner: None,
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
            stats.revalidated_payload_bytes.get(),
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
    // The entry's pinned dependencies are held without retaining duplicate
    // file bytes, and reported in the sections.
    let held = ordered_update_preparation::held_v1();
    let entry = held
        .pins
        .iter()
        .find(|pins| {
            pins.iter()
                .any(|file| file.pin.path == source.play_import.path)
        })
        .expect("the source is held");
    assert_eq!(entry.iter().map(|file| file.len).sum::<u64>(), on_disk);
    assert!(held.policy_bytes > 0);
    assert!(held.models >= 1 && held.models <= held.entries && held.model_bytes > 0);
    assert!(held.charged_bytes >= held.policy_bytes + held.model_bytes + held.metadata_bytes);
    let section = opponent_cache_section_v1();
    assert_eq!(section["process"]["lookups"], stats.lookups.get());
    assert_eq!(section["process"]["forks"], stats.forks.get());
    // The three forks copied the same policy's own bytes; the model is
    // shared, not copied.
    let forked = stats.forked_policy_bytes.get() - forked_before;
    assert!(forked > 0 && forked.is_multiple_of(3));
    assert!(section["process"]["entries"].as_u64().unwrap() >= 1);
    assert!(section["process"]["budget_bytes"].as_u64().unwrap() > 0);
    assert!(section["process"]["charged_bytes"].as_u64().unwrap() >= held.charged_bytes as u64);
    assert_eq!(section["process"]["file_bytes"], 0);
    let retained = retained_files_section_v1();
    assert_eq!(retained["opponent_cache_bytes"], 0);
    fs::remove_dir_all(&root).unwrap();
}

/// Every net-8 parameter's name, shape and exact f32 bits.
fn parameter_bits_v1(
    opponent: &super::super::LoadedOpponentV1,
) -> Vec<(&'static str, Vec<usize>, Vec<u32>)> {
    opponent
        .policy
        .training_parameters_v3()
        .into_iter()
        .map(|parameter| {
            (
                parameter.name,
                parameter.shape,
                parameter
                    .values
                    .iter()
                    .map(|value| value.to_bits())
                    .collect(),
            )
        })
        .collect()
}

/// Two loads of one source agree on their behavior record and on every
/// parameter bit.
fn assert_same_opponent_v1(
    left: &super::super::LoadedOpponentV1,
    right: &super::super::LoadedOpponentV1,
) {
    assert_eq!(left.behavior, right.behavior);
    assert_eq!(parameter_bits_v1(left), parameter_bits_v1(right));
}

fn fresh_source_variant_v1(
    root: &Path,
    label: &str,
    delta: f32,
) -> (ExpandedModelSourceV1, Vec<PathBuf>) {
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let sub = root.join(label);
    let descriptor =
        write_synthetic_fresh_source_with_parameters_v1(&sub.join("source"), identity, |params| {
            params[0].values[0] += delta;
        });
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = sub.join("descriptor.json");
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
        w_learner: None,
    };
    (source, read)
}

#[test]
fn process_and_collection_opponent_caches_enforce_byte_budgets_lru_and_oversized_passthrough() {
    on_large_stack_v1(
        process_and_collection_opponent_caches_enforce_byte_budgets_lru_and_oversized_passthrough_v1,
    );
}

fn process_and_collection_opponent_caches_enforce_byte_budgets_lru_and_oversized_passthrough_v1() {
    use super::super::loaded_source_profile_v1::LoadRoleV1;
    use super::super::{collection_entry_charge_v1, OpponentCacheRunOptionsV1, OpponentCacheV1};
    use ordered_update_preparation::ProcessOpponentCacheV1;

    // Validate bounded config options.
    assert!(OpponentCacheRunOptionsV1 {
        entries: Some(64),
        budget_mib: Some(768),
        collection_entries: Some(16),
        collection_budget_mib: Some(256),
    }
    .validate_v1()
    .is_ok());
    assert!(OpponentCacheRunOptionsV1 {
        entries: Some(0),
        budget_mib: Some(0),
        collection_entries: Some(0),
        collection_budget_mib: Some(0),
    }
    .validate_v1()
    .is_ok());
    for bad in [
        OpponentCacheRunOptionsV1 {
            entries: Some(257),
            ..Default::default()
        },
        OpponentCacheRunOptionsV1 {
            budget_mib: Some(16_385),
            ..Default::default()
        },
        OpponentCacheRunOptionsV1 {
            collection_entries: Some(129),
            ..Default::default()
        },
        OpponentCacheRunOptionsV1 {
            collection_budget_mib: Some(16_385),
            ..Default::default()
        },
    ] {
        assert!(bad.validate_v1().is_err());
    }

    let root = std::env::temp_dir().join(format!(
        "opponent-cache-budgets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let (s0, s0_files) = fresh_source_variant_v1(&root, "m0", 0.0);
    let (s1, _) = fresh_source_variant_v1(&root, "m1", 0.125);
    let (s2, _) = fresh_source_variant_v1(&root, "m2", 0.25);

    // Measure one standalone entry's exact byte charge.
    let probe_cache = ProcessOpponentCacheV1::new_v1(8, 256 << 20);
    let (first_loaded, hit) = probe_cache
        .load_v1(&s0, true, LoadRoleV1::Opponent)
        .unwrap();
    assert!(!hit);
    let single = probe_cache.held_v1();
    assert_eq!(single.entries, 1);
    assert_eq!(single.holders_v1().len(), 0);
    assert_eq!(
        single.charged_bytes,
        single.policy_bytes + single.model_bytes + single.metadata_bytes
    );
    let single_charge = single.charged_bytes;
    assert!(single_charge > single.policy_bytes && single.model_bytes > 0);

    // Byte-budget LRU eviction with a generous entry limit (8 entries, budget
    // sized for at most 2 entries).
    let two_budget = 2 * single_charge + 4096;
    let bounded = ProcessOpponentCacheV1::new_v1(8, two_budget);
    assert!(!bounded.load_v1(&s0, true, LoadRoleV1::Opponent).unwrap().1);
    assert!(!bounded.load_v1(&s1, true, LoadRoleV1::Opponent).unwrap().1);
    assert_eq!(bounded.held_v1().entries, 2);
    assert!(bounded.held_v1().charged_bytes <= two_budget);
    // Touch s0 so s1 becomes least-recently-used; loading s2 must evict s1.
    assert!(bounded.load_v1(&s0, false, LoadRoleV1::Opponent).unwrap().1);
    assert!(!bounded.load_v1(&s2, true, LoadRoleV1::Opponent).unwrap().1);
    assert_eq!(bounded.held_v1().entries, 2);
    assert!(bounded.held_v1().charged_bytes <= two_budget);
    assert!(bounded.holds_source_v1(&s0));
    assert!(!bounded.holds_source_v1(&s1));
    assert!(bounded.holds_source_v1(&s2));

    // Re-loading the evicted s1 is a full miss that evicts the new LRU entry
    // (s0), and its policy matches a never-evicted reference load and hit bit
    // for bit.
    let reference = ProcessOpponentCacheV1::new_v1(8, 256 << 20);
    let (reference_s1, hit) = reference.load_v1(&s1, true, LoadRoleV1::Opponent).unwrap();
    assert!(!hit);
    let (reference_s1_hit, hit) = reference.load_v1(&s1, true, LoadRoleV1::Opponent).unwrap();
    assert!(hit);
    let evictions_before = counters_v1().process_opponent.evictions.get();
    let (reloaded_s1, hit) = bounded.load_v1(&s1, true, LoadRoleV1::Opponent).unwrap();
    assert!(!hit);
    assert_eq!(
        counters_v1().process_opponent.evictions.get() - evictions_before,
        1
    );
    assert!(!bounded.holds_source_v1(&s0));
    assert!(bounded.holds_source_v1(&s1));
    assert!(bounded.holds_source_v1(&s2));
    assert_same_opponent_v1(&reloaded_s1, &reference_s1);
    assert_same_opponent_v1(&reloaded_s1, &reference_s1_hit);
    // The comparison distinguishes members: s0 and s1 differ in a parameter.
    assert_ne!(
        parameter_bits_v1(&first_loaded),
        parameter_bits_v1(&reference_s1)
    );

    // Oversized-entry pass-through and zero-budget / zero-entry caches still
    // serve valid forks without retaining entries or evicting smaller entries.
    let counters = counters_v1();
    let refused_before = counters.process_opponent.not_kept_budget.get();
    for (entries, budget) in [(8, single_charge - 1), (8, 0), (0, 256 << 20)] {
        let tiny = ProcessOpponentCacheV1::new_v1(entries, budget);
        let (loaded, hit) = tiny.load_v1(&s0, true, LoadRoleV1::Opponent).unwrap();
        assert!(!hit);
        assert_eq!(loaded.behavior.identity, first_loaded.behavior.identity);
        assert_eq!(tiny.held_v1().entries, 0);
        assert_eq!(tiny.held_v1().charged_bytes, 0);
    }
    assert_eq!(
        counters.process_opponent.not_kept_budget.get() - refused_before,
        3
    );

    // Reconfiguring a live cache downward evicts oldest entries until within bounds.
    bounded.configure_v1(None, Some(0));
    assert_eq!(bounded.held_v1().entries, 0);
    assert_eq!(bounded.held_v1().charged_bytes, 0);

    // Strict revalidation rejects same-length mutation, truncation, lengthening,
    // directory replacement, and deletion of any pinned file.
    let strict_cache = ProcessOpponentCacheV1::new_v1(4, 256 << 20);
    assert!(
        !strict_cache
            .load_v1(&s0, true, LoadRoleV1::Opponent)
            .unwrap()
            .1
    );
    assert!(
        strict_cache
            .load_v1(&s0, true, LoadRoleV1::Opponent)
            .unwrap()
            .1
    );
    let param_path = &s0_files[2];
    let original_params = fs::read(param_path).unwrap();
    let mut same_len_bad = original_params.clone();
    same_len_bad[original_params.len() / 2] ^= 0x01;
    fs::write(param_path, &same_len_bad).unwrap();
    assert_eq!(
        strict_cache
            .load_v1(&s0, true, LoadRoleV1::Opponent)
            .err()
            .unwrap(),
        "fresh input file SHA differs"
    );
    fs::write(param_path, &original_params[..original_params.len() - 4]).unwrap();
    assert!(strict_cache
        .load_v1(&s0, true, LoadRoleV1::Opponent)
        .is_err());
    let mut longer = original_params.clone();
    longer.extend_from_slice(&[0u8; 16]);
    fs::write(param_path, &longer).unwrap();
    assert!(strict_cache
        .load_v1(&s0, true, LoadRoleV1::Opponent)
        .is_err());
    fs::remove_file(param_path).unwrap();
    assert!(strict_cache
        .load_v1(&s0, true, LoadRoleV1::Opponent)
        .is_err());
    fs::create_dir(param_path).unwrap();
    assert!(strict_cache
        .load_v1(&s0, true, LoadRoleV1::Opponent)
        .is_err());
    fs::remove_dir(param_path).unwrap();
    fs::write(param_path, &original_params).unwrap();
    assert!(
        strict_cache
            .load_v1(&s0, true, LoadRoleV1::Opponent)
            .unwrap()
            .1
    );

    // Collection OpponentCacheV1 enforces both byte-budget LRU eviction and
    // oversized-entry transient borrows without leaking live_entries.
    let entry_charge = collection_entry_charge_v1(&first_loaded);
    let live_before = counters.collection_opponent.live_entries.get();
    let col_refused_before = counters.collection_opponent.not_kept_budget.get();
    {
        let mut col = OpponentCacheV1::with_limits_v1(8, 2 * entry_charge + 512);
        assert_eq!(col.load(&s0).unwrap().behavior.source, s0);
        assert_eq!(col.load(&s1).unwrap().behavior.source, s1);
        assert_eq!(col.load(&s0).unwrap().behavior.source, s0);
        assert_eq!(col.load(&s2).unwrap().behavior.source, s2);
        assert!(col.contains(&s0));
        assert!(!col.contains(&s1));
        assert!(col.contains(&s2));
        assert!(col.charged_bytes <= col.budget_bytes);
        // Re-loading the evicted s1 matches the never-evicted reference.
        let reloaded = col.load(&s1).unwrap();
        assert_same_opponent_v1(reloaded, &reference_s1);
        assert!(col.contains(&s1));
        assert!(!col.contains(&s0));

        let mut oversized_col = OpponentCacheV1::with_limits_v1(8, entry_charge - 1);
        assert_eq!(oversized_col.load(&s0).unwrap().behavior.source, s0);
        assert!(!oversized_col.contains(&s0));
        assert_eq!(oversized_col.charged_bytes, 0);

        let mut zero_col = OpponentCacheV1::with_limits_v1(0, 256 << 20);
        assert_eq!(zero_col.load(&s0).unwrap().behavior.source, s0);
        assert!(!zero_col.contains(&s0));
    }
    assert_eq!(counters.collection_opponent.live_entries.get(), live_before);
    assert_eq!(
        counters.collection_opponent.not_kept_budget.get() - col_refused_before,
        2
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_process_opponent_cache_counts_checkpoint_decodes() {
    on_large_stack_v1(the_process_opponent_cache_counts_checkpoint_decodes_v1);
}

fn the_process_opponent_cache_counts_checkpoint_decodes_v1() {
    let root = std::env::temp_dir().join(format!(
        "opponent-cache-decode-diagnostics-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let (import, _) = fresh_source_v1(&root);
    let member = super::super::loaded_source_cache_v1::bypassed_for_test_v1(|| {
        super::super::pool_fixture_v1::write_members_v1(&root.join("pool"), &import, 1)
    })
    .unwrap()
    .pop()
    .unwrap();
    let source = member.source;
    let checkpoint_len = fs::metadata(&source.checkpoint.as_ref().unwrap().path)
        .unwrap()
        .len();
    let counters = counters_v1();
    let stats = &counters.process_opponent;
    let decodes_before = counters.checkpoint_decodes.decodes.get();
    let decode_bytes_before = counters.checkpoint_decodes.bytes.get();
    let full_decode_before = stats.full_load.decode.get();
    let capture_decode_before = stats.capture.decode.get();
    let load = |revalidate| {
        ordered_update_preparation::load_opponent_cached_v1(&source, revalidate)
            .unwrap()
            .1
    };
    assert!(!load(true));
    assert_eq!(
        counters.checkpoint_decodes.decodes.get() - decodes_before,
        1
    );
    assert_eq!(
        counters.checkpoint_decodes.bytes.get() - decode_bytes_before,
        checkpoint_len
    );
    assert!(stats.full_load.decode.get() > full_decode_before);
    assert_eq!(stats.capture.decode.get(), capture_decode_before);
    for split in [&stats.full_load, &stats.capture] {
        let parts = split.read.get() + split.verify.get() + split.decode.get();
        assert!(split.read.get() > 0 && parts <= split.total.get());
    }
    assert!(load(true));
    assert!(load(false));
    assert_eq!(
        counters.checkpoint_decodes.decodes.get() - decodes_before,
        1
    );
    assert_eq!(
        counters.checkpoint_decodes.bytes.get() - decode_bytes_before,
        checkpoint_len
    );
    fs::remove_dir_all(&root).unwrap();
}
