use super::super::tests::replay_fixture;
use super::*;
use std::time::Duration;

/// An entry no helper will touch: nothing is queued for it.
fn insert_unqueued(key: &str, bytes: &[u8], slots: [SlotV1; 2]) -> Arc<Vec<u8>> {
    let mut state = lock_v1(shared_v1());
    let bytes = Arc::new(bytes.to_vec());
    state.retained_bytes += bytes.len();
    state.order.push_back(key.to_owned());
    state.entries.insert(
        key.to_owned(),
        EntryV1 {
            bytes: Arc::clone(&bytes),
            path: test_path(key),
            charge: bytes.len(),
            slots,
        },
    );
    bytes
}

/// A file path of its own for each test entry.
fn test_path(key: &str) -> PathBuf {
    std::env::temp_dir()
        .join("parse-ahead-unit-tests")
        .join(key)
        .join("episode-0000.json")
}

fn trajectory_bytes(id: &str) -> Vec<u8> {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v3();
    let (mut trajectory, _, _) = replay_fixture(&mut policy, None, 0, &[0, 1]);
    trajectory.episode.id = id.to_owned();
    serde_json::to_vec(&trajectory).unwrap()
}

#[test]
fn readers_receive_exactly_their_own_parse_once() {
    let key = "parse-ahead-test-exact";
    let bytes = trajectory_bytes(key);
    let _retention = default_retention_for_test_v1();
    offer_v1(key, &test_path(key), bytes.clone());
    wait_until_parsed_for_test_v1(key);

    // Other bytes under the same key are a miss and leave the entry intact.
    let mut other = bytes.clone();
    other.push(b' ');
    assert_eq!(take_collection_fields_v1(key, &other), None);
    assert!(take_trajectory_v1(key, &other).is_none());

    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        take_collection_fields_v1(key, &bytes),
        Some(CollectionTrajectoryFieldsV1::of_document_v1(&document))
    );
    let expected: ExpandedTrajectoryV1 = serde_json::from_slice(&bytes).unwrap();
    let served = take_trajectory_v1(key, &bytes).unwrap();
    assert_eq!(
        serde_json::to_vec(&served).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
    assert_eq!(served_for_test_v1(key), [true, true]);

    // Each result is handed out once; the entry is then gone.
    assert_eq!(take_collection_fields_v1(key, &bytes), None);
    assert!(take_trajectory_v1(key, &bytes).is_none());
    assert!(!lock_v1(shared_v1()).entries.contains_key(key));
}

#[test]
fn failed_parses_are_not_kept() {
    let key = "parse-ahead-test-failed";
    // Valid JSON that is not a trajectory, and bytes that are not JSON.
    let not_a_trajectory = br#"{"episode":{"id":"x"},"learner_sampler":7}"#.to_vec();
    let _retention = default_retention_for_test_v1();
    offer_v1(key, &test_path(key), not_a_trajectory.clone());
    wait_until_parsed_for_test_v1(key);
    assert_eq!(
        take_collection_fields_v1(key, &not_a_trajectory),
        Some(CollectionTrajectoryFieldsV1 {
            episode: json!({"id": "x"}),
            learner_sampler: None,
            training_math: None,
            input_contract: None,
        })
    );
    assert!(take_trajectory_v1(key, &not_a_trajectory).is_none());

    let key = "parse-ahead-test-not-json";
    let not_json = b"{\"episode\":".to_vec();
    offer_v1(key, &test_path(key), not_json.clone());
    wait_until_parsed_for_test_v1(key);
    assert_eq!(take_collection_fields_v1(key, &not_json), None);
    assert!(take_trajectory_v1(key, &not_json).is_none());
    assert_eq!(served_for_test_v1(key), [false, false]);
}

#[test]
fn a_queued_parse_is_left_to_its_reader_and_a_running_one_is_awaited() {
    let _retention = default_retention_for_test_v1();
    let key = "parse-ahead-test-queued";
    let bytes = br#"{"episode":null}"#.to_vec();
    let stored = insert_unqueued(key, &bytes, [SlotV1::Queued, SlotV1::Running]);
    // The queued fields parse is withdrawn: the reader parses itself.
    assert_eq!(take_collection_fields_v1(key, &bytes), None);

    // A reader of the running parse waits for its result.
    let reader = thread::spawn({
        let bytes = bytes.clone();
        move || take_v1(key, &bytes, KindV1::Trajectory).is_some()
    });
    thread::sleep(Duration::from_millis(20));
    {
        let shared = shared_v1();
        let mut state = lock_v1(shared);
        state.finish_v1(
            key,
            KindV1::Trajectory,
            &stored,
            Some(ParsedV1::CollectionFields(CollectionTrajectoryFieldsV1 {
                episode: Value::Null,
                learner_sampler: None,
                training_math: None,
                input_contract: None,
            })),
        );
        shared.changed.notify_all();
    }
    assert!(reader.join().unwrap());
    assert!(!lock_v1(shared_v1()).entries.contains_key(key));
}

#[test]
fn a_claim_released_by_unwinding_wakes_its_readers() {
    let _retention = default_retention_for_test_v1();
    let key = "parse-ahead-test-unwind";
    let bytes = br#"{}"#.to_vec();
    let claim = ClaimV1 {
        shared: shared_v1(),
        key: key.to_owned(),
        kind: KindV1::Trajectory,
        bytes: insert_unqueued(key, &bytes, [SlotV1::Gone, SlotV1::Running]),
    };
    let reader = thread::spawn({
        let bytes = bytes.clone();
        move || take_trajectory_v1(key, &bytes).is_none()
    });
    thread::sleep(Duration::from_millis(20));
    drop(claim);
    assert!(reader.join().unwrap());
    assert!(!lock_v1(shared_v1()).entries.contains_key(key));
}

/// An entry is charged its record length, or its buffer's capacity when the
/// buffer keeps more than that alive.
#[test]
fn an_entry_is_never_charged_less_than_the_capacity_it_keeps_alive() {
    assert_eq!(retained_charge_v1(100, 100), 100);
    assert_eq!(retained_charge_v1(100, 4096), 4096);
    // A sealed record: the record length exceeds the frame's buffer.
    assert_eq!(retained_charge_v1(4096, 100), 4096);

    // Through the store's own insertion, under one lock so that no other
    // test's entries can displace it in between: a short plain record in a
    // roomy buffer, as a serializer can leave it, is charged the buffer.
    let _retention = default_retention_for_test_v1();
    let key = "parse-ahead-test-roomy";
    let mut roomy = Vec::with_capacity(64 * 1024);
    roomy.extend_from_slice(b"{}");
    let capacity = roomy.capacity();
    assert!(capacity >= 64 * 1024);
    let record_length = roomy.len();
    let mut state = lock_v1(shared_v1());
    let limit = DEFAULT_RETAINED_MEBIBYTES_V1 << 20;
    assert!(insert_locked_v1(
        &mut state,
        key,
        (test_path(key), Arc::new(roomy), record_length),
        [SlotV1::Gone, SlotV1::Gone],
        limit
    ));
    assert_eq!(state.entries[key].charge, capacity);
    assert!(state.retained_bytes >= capacity);
    // The same pin again is not held twice.
    assert!(!insert_locked_v1(
        &mut state,
        key,
        (test_path(key), Arc::new(Vec::new()), 0),
        [SlotV1::Gone, SlotV1::Gone],
        limit
    ));
    // Dropping the entry gives back exactly what it was charged.
    let held = state.retained_bytes;
    state.remove_v1(key);
    assert_eq!(held - state.retained_bytes, capacity);
    assert!(!state.entries.contains_key(key));
}

/// An entry for `record_length` record bytes in a buffer of `capacity`.
fn sized(key: &str, record_length: usize, capacity: usize) -> (PathBuf, Arc<Vec<u8>>, usize) {
    let mut bytes = Vec::with_capacity(capacity);
    bytes.resize(capacity.min(record_length), b' ');
    (test_path(key), Arc::new(bytes), record_length)
}

/// The retention bound decides only what is held: zero holds nothing, a
/// bound drops the oldest entries first, a large one holds all, and an entry
/// is charged its buffer's capacity when that exceeds its record length, so
/// a batch that fits its record-byte bound can still overflow a retention of
/// the same size.
#[test]
fn the_retention_bound_decides_what_is_held() {
    let held = |state: &StateV1| -> Vec<String> { state.order.iter().cloned().collect() };
    let gone = || [SlotV1::Gone, SlotV1::Gone];

    let mut state = StateV1::default();
    assert!(!insert_locked_v1(
        &mut state,
        "a",
        sized("a", 10, 10),
        gone(),
        0
    ));
    assert!(state.entries.is_empty());
    assert_eq!(state.retained_bytes, 0);

    let mut state = StateV1::default();
    for key in ["a", "b", "c"] {
        assert!(insert_locked_v1(
            &mut state,
            key,
            sized(key, 40, 40),
            gone(),
            100
        ));
    }
    assert_eq!(held(&state), ["b", "c"]);
    assert_eq!(state.retained_bytes, 80);

    let mut state = StateV1::default();
    for key in ["a", "b", "c"] {
        assert!(insert_locked_v1(
            &mut state,
            key,
            sized(key, 40, 40),
            gone(),
            1 << 30
        ));
    }
    assert_eq!(held(&state), ["a", "b", "c"]);
    assert_eq!(state.retained_bytes, 120);

    // Two records of 40 bytes fit a bound of 100 by record bytes, but the
    // second buffer's spare capacity makes it 64: the first is dropped.
    let mut state = StateV1::default();
    assert!(insert_locked_v1(
        &mut state,
        "a",
        sized("a", 40, 40),
        gone(),
        100
    ));
    assert!(insert_locked_v1(
        &mut state,
        "b",
        sized("b", 40, 64),
        gone(),
        100
    ));
    assert_eq!(held(&state), ["b"]);
    assert_eq!(state.retained_bytes, 64);
    // A single entry above the bound is never held and drops nothing.
    assert!(!insert_locked_v1(
        &mut state,
        "c",
        sized("c", 101, 101),
        gone(),
        100
    ));
    assert_eq!(held(&state), ["b"]);
}

/// The process setting admits 0..=16384 MiB once; the same value again is
/// accepted, another is refused.
#[test]
fn the_retention_is_configured_once_within_its_range() {
    let _hold = retention_for_test_v1();
    assert!(configure_retained_mebibytes_v1(MAX_RETAINED_MEBIBYTES_V1 + 1).is_err());
    configure_retained_mebibytes_v1(DEFAULT_RETAINED_MEBIBYTES_V1).unwrap();
    configure_retained_mebibytes_v1(DEFAULT_RETAINED_MEBIBYTES_V1).unwrap();
    assert!(configure_retained_mebibytes_v1(0).is_err());
    assert_eq!(retained_limit_v1(), DEFAULT_RETAINED_MEBIBYTES_V1 << 20);
}

/// Lowering the bound in force drops the oldest entries above it at once,
/// and zero leaves nothing held for a reader.
#[test]
fn a_lower_bound_drops_the_oldest_entries_at_once() {
    let hold = retention_for_test_v1();
    let key = "parse-ahead-test-lowered";
    let bytes = br#"{"episode":null}"#.to_vec();
    offer_v1(key, &test_path(key), bytes.clone());
    assert!(lock_v1(shared_v1()).entries.contains_key(key));
    hold.set_v1(0);
    assert!(!retains_v1());
    assert!(!lock_v1(shared_v1()).entries.contains_key(key));
    assert_eq!(take_collection_fields_v1(key, &bytes), None);
    // Nothing is held while the bound is zero.
    offer_v1(key, &test_path(key), bytes.clone());
    assert!(!lock_v1(shared_v1()).entries.contains_key(key));
    drop(hold);
    assert!(retains_v1());
}

/// Releasing a directory drops exactly its entries, queued and running parses
/// included: a reader waiting on a running parse parses itself, a parse that
/// finishes afterwards fills nothing (not even a later entry for the same
/// bytes), and entries elsewhere stay.
#[test]
fn released_entries_are_dropped_and_late_parses_never_refill_them() {
    let _retention = default_retention_for_test_v1();
    let key = "parse-ahead-test-released";
    let other = "parse-ahead-test-kept";
    let bytes = br#"{"episode":null}"#.to_vec();
    let stored = insert_unqueued(key, &bytes, [SlotV1::Queued, SlotV1::Running]);
    insert_unqueued(other, &bytes, [SlotV1::Gone, SlotV1::Gone]);
    lock_v1(shared_v1()).queues[0].push_back(key.to_owned());
    let reader = thread::spawn({
        let bytes = bytes.clone();
        move || take_v1(key, &bytes, KindV1::Trajectory).is_none()
    });
    thread::sleep(Duration::from_millis(20));
    let directory = test_path(key).parent().unwrap().to_path_buf();
    release_under_v1(&directory);
    assert!(reader.join().unwrap());
    {
        let state = lock_v1(shared_v1());
        assert!(!state.entries.contains_key(key));
        assert!(state.entries.contains_key(other));
        assert!(!state.queues[0].iter().any(|queued| queued == key));
    }
    // The same bytes offered again are another entry; the old parse that
    // finishes now leaves it queued.
    let again = insert_unqueued(key, &bytes, [SlotV1::Queued, SlotV1::Queued]);
    assert!(!Arc::ptr_eq(&again, &stored));
    {
        let shared = shared_v1();
        let mut state = lock_v1(shared);
        state.finish_v1(
            key,
            KindV1::CollectionFields,
            &stored,
            Some(ParsedV1::CollectionFields(CollectionTrajectoryFieldsV1 {
                episode: json!("stale"),
                learner_sampler: None,
                training_math: None,
                input_contract: None,
            })),
        );
        assert!(matches!(
            state.entries[key].slots[KindV1::CollectionFields as usize],
            SlotV1::Queued
        ));
    }
    release_under_v1(&directory);
    release_under_v1(test_path(other).parent().unwrap());
    let state = lock_v1(shared_v1());
    assert!(!state.entries.contains_key(key) && !state.entries.contains_key(other));
}
