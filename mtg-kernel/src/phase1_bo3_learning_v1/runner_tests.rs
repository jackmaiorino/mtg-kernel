//! Focused scheduler/IO units. These do not manufacture a clean runtime or
//! claim native game parity; the real executable schedule is qualified by root.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

fn scratch(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "phase1-bo3-runner-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn phase1_bo3_runner_orders_worker_results_and_bounds_dimensions() {
    let serial = ordered_jobs(1, 16, |slot| Ok((slot, slot * slot))).unwrap();
    let parallel = ordered_jobs(4, 16, |slot| {
        if slot % 4 == 0 {
            thread::yield_now();
        }
        Ok((slot, slot * slot))
    })
    .unwrap();
    assert!(serial.failure.is_none());
    assert!(parallel.failure.is_none());
    assert_eq!(serial.outputs, parallel.outputs);
    assert_eq!(
        parallel
            .timings
            .iter()
            .map(|t| t.completed_slots)
            .sum::<usize>(),
        16
    );
    for (workers, jobs) in [(0, 1), (2, 1), (33, 33), (1, 0), (1, 33)] {
        assert!(ordered_jobs(workers, jobs, |_| Ok(())).is_err());
    }
}

#[test]
fn phase1_bo3_runner_cancels_dispatch_and_joins_active_jobs_on_error_or_panic() {
    for panic_job in [false, true] {
        let active = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Barrier::new(2));
        let report = ordered_jobs(2, 32, |slot| {
            struct Active<'a>(&'a AtomicUsize);
            impl Drop for Active<'_> {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
            active.fetch_add(1, Ordering::SeqCst);
            let _active = Active(&active);
            if slot < 2 {
                started.wait();
            }
            if slot == 0 {
                if panic_job {
                    panic!("bounded intentional worker panic");
                }
                return Err("bounded intentional slot failure".into());
            }
            thread::sleep(Duration::from_millis(10));
            finished.fetch_add(1, Ordering::SeqCst);
            Ok(slot)
        })
        .unwrap();
        assert!(report.failure.as_ref().unwrap().contains("slot 0"));
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(finished.load(Ordering::SeqCst) >= 1);
        assert!(
            report
                .timings
                .iter()
                .map(|t| t.assigned_slots)
                .sum::<usize>()
                < 32
        );
        assert_eq!(
            report.timings.iter().map(|t| t.failed_slots).sum::<usize>(),
            1
        );
    }
    let calls = AtomicUsize::new(0);
    let report = ordered_jobs(1, 8, |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>("stop".into())
    })
    .unwrap();
    assert!(report.failure.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn phase1_bo3_runner_prefix_and_no_update_counts_are_distinct() {
    assert_eq!(prefix_length(&[true, true, false, false]).unwrap(), 2);
    assert_eq!(prefix_length(&[false, false]).unwrap(), 0);
    assert!(prefix_length(&[true, false, true]).is_err());
    // Two attempted matches, no Adam step. The next attempted batch may update.
    validate_count_delta((0, 0, 0), (0, 1, 2), false, 2).unwrap();
    validate_count_delta((0, 1, 2), (1, 2, 5), true, 3).unwrap();
    assert!(validate_count_delta((0, 0, 0), (1, 1, 2), false, 2).is_err());
    assert!(validate_count_delta((0, 0, 0), (0, 0, 2), false, 2).is_err());
    assert!(validate_count_delta((1, 2, 5), (2, 3, 7), true, 3).is_err());
    assert!(validate_count_delta((u64::MAX, 0, 0), (0, 1, 1), true, 1).is_err());
    assert!(!valid_attempt_name("attempt-../escape"));
    assert!(!valid_update_attempt_name("update-attempt-0000000"));
}

#[test]
fn phase1_bo3_runner_strict_parser_and_serialized_bounds() {
    assert!(
        NativeBo3TrainingRunV1::from_json_v1(r#"{"schema":"x","schema":"y"}"#)
            .unwrap_err()
            .contains("duplicate JSON object key")
    );
    assert!(
        NativeBo3TrainingRunV1::from_json_v1(&" ".repeat(MAX_NATIVE_BO3_RUN_REQUEST_BYTES_V1 + 1))
            .unwrap_err()
            .contains("exceeds 16 MiB")
    );
    let item = vec!["exact", "bytes"];
    let expected = serde_json::to_vec(&item).unwrap();
    assert_eq!(
        bounded_json(&item, expected.len() as u64).unwrap(),
        expected
    );
    assert!(bounded_json(&item, expected.len() as u64 - 1).is_err());
}

#[test]
fn phase1_bo3_runner_intent_precedes_payload_and_corrupt_final_is_preserved() {
    let root = scratch("publication");
    let request = publish_bytes(&root, "request.json", b"opaque IO-unit request").unwrap();
    let payload = b"opaque IO-unit payload, not a game capture".to_vec();
    let attempt = publish_slot_bytes(&root, &request, PlayerSeatV1::P1, payload.clone()).unwrap();
    let intent: SlotIntent = read_json(&root.join("intent.json"), SMALL).unwrap();
    assert_eq!(intent.request, request);
    assert_eq!(intent.result, attempt.result);
    assert_eq!(fs::read(&attempt.result.path).unwrap(), payload);
    let mut verified = VerifiedFiles::default();
    verified
        .verify(&attempt.result, payload.len() as u64)
        .unwrap();
    assert!(
        verified
            .verify(&attempt.result, payload.len() as u64 - 1)
            .is_err()
    );
    fs::write(&attempt.result.path, b"corrupt final").unwrap();
    assert!(
        VerifiedFiles::default()
            .verify(&attempt.result, INPUT)
            .is_err()
    );
    assert!(publish_slot_bytes(&root, &request, PlayerSeatV1::P1, payload).is_err());
    assert_eq!(fs::read(&attempt.result.path).unwrap(), b"corrupt final");
    // An orphan stage never gets removed or overwritten by a later attempt.
    let abandoned = root.join("attempt-000000");
    fs::create_dir(&abandoned).unwrap();
    fs::write(abandoned.join(".result.json.stage-interrupted"), b"partial").unwrap();
    let next = new_attempt(
        &root,
        "attempt-",
        &attempt_directories(&root, "attempt-").unwrap(),
    )
    .unwrap();
    assert_eq!(next.file_name().unwrap(), "attempt-000001");
    assert_eq!(
        fs::read(abandoned.join(".result.json.stage-interrupted")).unwrap(),
        b"partial"
    );
}
