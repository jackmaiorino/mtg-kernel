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
            slots,
        },
    );
    bytes
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
    offer_v1(key, bytes.clone());
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
    offer_v1(key, not_a_trajectory.clone());
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
    offer_v1(key, not_json.clone());
    wait_until_parsed_for_test_v1(key);
    assert_eq!(take_collection_fields_v1(key, &not_json), None);
    assert!(take_trajectory_v1(key, &not_json).is_none());
    assert_eq!(served_for_test_v1(key), [false, false]);
}

#[test]
fn a_queued_parse_is_left_to_its_reader_and_a_running_one_is_awaited() {
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
