//! Followed runs (`follow_v1`): the continuation check, the spool, and that
//! a followed process publishes what fresh processes publish, also when it
//! crashes at any publication of either run.

use super::*;
use crate::native_expanded_training_run_v1::follow_v1::{
    check_continuation_v1, done_name_v1, follow_v1, FollowEndV1, FollowErrorV1, FollowNextV1,
    SpoolV1, NEXT_NAME, STOP_NAME,
};

/// Documents that carry timing telemetry, or the digest of one.
const TIMED: [&str; 5] = [
    "collection.json",
    "update.json",
    "update-input.json",
    "complete.json",
    "completion.json",
];

/// A chain of two segments of two iterations each under `chain`: segment 0
/// from the fresh source, segment 1 the continuation from segment 0's last
/// checkpoint (`checkpoint`, None until segment 0 has run).
fn segment_v1(
    base: &NativeExpandedTrainingRunV1,
    chain: &Path,
    segment: usize,
    checkpoint: Option<PinnedFileV1>,
) -> NativeExpandedTrainingRunV1 {
    let mut config = base.clone();
    config.output_directory = chain.join(format!("seg-{segment:05}"));
    for (index, iteration) in config.iterations.iter_mut().enumerate() {
        for (slot, scheduled) in iteration.episodes.iter_mut().enumerate() {
            scheduled.episode.id = format!("seg-{segment}-iter-{index}-{slot}");
            scheduled.episode.seed += 10 * segment as u64;
        }
    }
    if segment > 0 {
        config.initial_source.checkpoint = checkpoint;
    }
    config
}

fn checkpoint_pin_v1(result: &Value) -> PinnedFileV1 {
    serde_json::from_value(result["source"]["checkpoint"].clone()).unwrap()
}

fn chain_base_v1(label: &str) -> (NativeExpandedTrainingRunV1, PathBuf) {
    let base = parallel_two_iteration_config_v1(label);
    base.validate_v1().unwrap();
    let chain = base.output_directory.with_file_name("chain");
    (base, chain)
}

/// Runs `config` on a new thread, which starts with empty thread-local
/// loaded-source entries (the test build's cache), as a new process would.
fn fresh_run_v1(config: &NativeExpandedTrainingRunV1) -> Result<Value, String> {
    let config = config.clone();
    std::thread::spawn(move || run_native_expanded_training_v1(&config, None))
        .join()
        .unwrap()
}

/// A followed process over segments `from..to` of the chain on `base`, each
/// later segment built from the previous run's last checkpoint as the driver
/// builds it (`from > 0` starts from segment `from - 1`'s completion), with its
/// done records in `spool`.
fn followed_v1(
    base: &NativeExpandedTrainingRunV1,
    chain: &Path,
    from: usize,
    to: usize,
    spool: &Path,
    before_next: &mut dyn FnMut(&NativeExpandedTrainingRunV1),
) -> Result<(FollowEndV1, Vec<Value>), FollowErrorV1> {
    fs::create_dir_all(spool).unwrap();
    let spool = SpoolV1::open_v1(spool).unwrap();
    let start = (from > 0).then(|| {
        let completion = chain
            .join(format!("seg-{:05}", from - 1))
            .join("completion.json");
        checkpoint_pin_v1(&read_json(&completion, SMALL_CAP).unwrap())
    });
    let results = std::cell::RefCell::new(Vec::<Value>::new());
    let ended = follow_v1(
        segment_v1(base, chain, from, start),
        |completed| {
            let segment = from + completed;
            if segment >= to {
                return Ok(FollowNextV1::Stop);
            }
            let previous = checkpoint_pin_v1(results.borrow().last().unwrap());
            let config = segment_v1(base, chain, segment, Some(previous));
            before_next(&config);
            Ok(FollowNextV1::Config(Box::new(config)))
        },
        |index, config, result| {
            results.borrow_mut().push(result.clone());
            spool.publish_done_v1(index, config, result)
        },
    )?;
    Ok((ended, results.into_inner()))
}

/// Every non-timed file present in both trees has the same bytes, and the
/// same files exist apart from staging debris and the run lock.
fn assert_trees_match_v1(case: &str, got: &Path, want: &Path, same_files: bool) {
    let wanted = files_under_v1(want);
    let found = files_under_v1(got);
    for (path, bytes) in &found {
        let name = path.file_name().unwrap().to_str().unwrap();
        if name.starts_with('.') && name.contains(".stage") {
            continue;
        }
        let expected = wanted
            .get(path)
            .unwrap_or_else(|| panic!("{case}: unexpected file {}", path.display()));
        if name != "run.lock" && !TIMED.contains(&name) {
            assert!(bytes == expected, "{case}: {} differs", path.display());
        }
    }
    if same_files {
        for path in wanted.keys() {
            let name = path.file_name().unwrap().to_str().unwrap();
            if name.starts_with('.') && name.contains(".stage") {
                continue;
            }
            assert!(
                found.contains_key(path),
                "{case}: {} missing",
                path.display()
            );
        }
    }
}

/// The states and schedule bindings of every iteration receipt of a run.
fn receipt_states_v1(result: &Value) -> Vec<(Value, Value, Value)> {
    (0..result["iterations"].as_array().unwrap().len())
        .map(|index| {
            let receipt = iteration_receipt_v1(result, index);
            (
                receipt["source"].clone(),
                receipt["episodes_sha256"].clone(),
                receipt["output_identity"].clone(),
            )
        })
        .collect()
}

#[test]
fn the_continuation_check_admits_only_the_next_segment_of_the_same_run() {
    let (base, chain) = chain_base_v1("follow-check");
    let first = segment_v1(&base, &chain, 0, None);
    let pin = PinnedFileV1 {
        path: chain.join("seg-00000/iterations/000001/attempt-000000/update/checkpoint.json"),
        sha256: "b".repeat(64),
    };
    let result = json!({"complete":true, "source":{"checkpoint":pin},
        "actual_identity":{"checkpoint_sha256":pin.sha256}});
    let next = segment_v1(&base, &chain, 1, Some(pin.clone()));
    check_continuation_v1(&first, &result, &next).unwrap();
    // The segment after a followed segment: its source already has a checkpoint.
    let later_pin = PinnedFileV1 {
        path: chain.join("seg-00001/iterations/000001/attempt-000000/update/checkpoint.json"),
        sha256: "c".repeat(64),
    };
    let later_result = json!({"complete":true, "source":{"checkpoint":later_pin},
        "actual_identity":{"checkpoint_sha256":later_pin.sha256}});
    let later = segment_v1(&base, &chain, 2, Some(later_pin.clone()));
    check_continuation_v1(&next, &later_result, &later).unwrap();
    // A pin of the run before the previous one is not a continuation.
    assert!(check_continuation_v1(&next, &later_result, &next.clone()).is_err());
    let refused = |what: &str, previous_result: &Value, candidate: NativeExpandedTrainingRunV1| {
        let error = check_continuation_v1(&first, previous_result, &candidate).expect_err(what);
        assert!(!error.is_empty(), "{what}");
    };
    let changes: Vec<(&str, fn(&mut NativeExpandedTrainingRunV1))> = vec![
        ("learning rate", |c| c.learning_rate *= 2.0),
        ("value coefficient", |c| c.value_coefficient *= 2.0),
        ("collection workers", |c| c.collection_workers = 3),
        ("preparation workers", |c| c.preparation_workers = 3),
        ("record envelope", |c| {
            c.record_envelope = crate::expanded_deck_training_v1::RecordEnvelopeV1::Zstd1
        }),
        ("verified reads", |c| {
            c.verified_reads = VerifiedReadsV1::Resident
        }),
        ("record format", |c| {
            c.record_format = crate::expanded_deck_training_v1::CheckpointRecordFormatV1::V1
        }),
        ("process cpu", |c| c.record_process_cpu = true),
        ("opponents", |c| {
            c.opponents.push(NamedExpandedOpponentV1 {
                id: "extra".into(),
                source: c.initial_source.clone(),
            })
        }),
        ("play import", |c| {
            c.initial_source.play_import.sha256 = "d".repeat(64)
        }),
        ("training math", |c| {
            c.training_math = TrainingMathV1::PortableV1
        }),
        ("non-natural fraction", |c| {
            c.max_non_natural_episode_fraction = 0.25
        }),
        ("schema", |c| c.schema.push('x')),
    ];
    for (what, change) in changes {
        let mut candidate = next.clone();
        change(&mut candidate);
        refused(what, &result, candidate);
    }
    let mut same_directory = next.clone();
    same_directory.output_directory = first.output_directory.clone();
    refused("same directory", &result, same_directory);
    let mut no_checkpoint = next.clone();
    no_checkpoint.initial_source.checkpoint = None;
    refused("no checkpoint", &result, no_checkpoint);
    let mut wrong_digest = next.clone();
    wrong_digest
        .initial_source
        .checkpoint
        .as_mut()
        .unwrap()
        .sha256 = "e".repeat(64);
    refused("wrong checkpoint digest", &result, wrong_digest);
    let mut wrong_path = next.clone();
    wrong_path.initial_source.checkpoint.as_mut().unwrap().path = chain.join("copy.json");
    refused("wrong checkpoint path", &result, wrong_path);
    let mut incomplete = result.clone();
    incomplete["complete"] = json!(false);
    refused("incomplete previous run", &incomplete, next.clone());
    let mut identity = result.clone();
    identity["actual_identity"]["checkpoint_sha256"] = json!("f".repeat(64));
    refused(
        "identity differs from the source pin",
        &identity,
        next.clone(),
    );
}

#[test]
fn the_spool_reads_stop_first_then_one_whole_next_config() {
    let (base, chain) = chain_base_v1("follow-spool");
    let _ = fs::remove_dir_all(&chain);
    let spool_dir = chain.join("spool");
    fs::create_dir_all(&spool_dir).unwrap();
    assert!(SpoolV1::open_v1(Path::new("relative/spool")).is_err());
    assert!(SpoolV1::open_v1(&chain.join("absent")).is_err());
    let spool = SpoolV1::open_v1(&spool_dir).unwrap();
    let config = segment_v1(&base, &chain, 0, None);
    let bytes = serde_json::to_vec_pretty(&config).unwrap();
    fs::write(spool_dir.join(NEXT_NAME), &bytes).unwrap();
    match spool.wait_v1().unwrap() {
        FollowNextV1::Config(read) => assert_eq!(value(&*read).unwrap(), value(&config).unwrap()),
        _ => panic!("expected the next config"),
    }
    assert!(
        !spool_dir.join(NEXT_NAME).exists(),
        "a taken config is removed"
    );
    fs::write(spool_dir.join(NEXT_NAME), &bytes).unwrap();
    fs::write(spool_dir.join(STOP_NAME), b"").unwrap();
    assert!(matches!(spool.wait_v1().unwrap(), FollowNextV1::Stop));
    assert!(
        spool_dir.join(NEXT_NAME).exists(),
        "STOP leaves a pending config unread"
    );
    fs::remove_file(spool_dir.join(STOP_NAME)).unwrap();
    fs::remove_file(spool_dir.join(NEXT_NAME)).unwrap();
    fs::create_dir(spool_dir.join(NEXT_NAME)).unwrap();
    assert!(
        spool.wait_v1().is_err(),
        "a next.json that is not a file is an error"
    );
    fs::remove_dir(spool_dir.join(NEXT_NAME)).unwrap();
    fs::write(spool_dir.join(NEXT_NAME), b"{\"schema\":").unwrap();
    assert!(spool.wait_v1().is_err(), "an unparsable config is an error");
    fs::remove_dir_all(&chain).unwrap();
}

/// A followed process over two segments publishes the files, bytes and states
/// that two fresh processes publish (timed documents compared by their
/// states), records each done, and STOP ends it; a refused continuation ends
/// it before any of the refused config's work.
#[test]
fn a_followed_chain_publishes_the_trees_and_states_of_fresh_processes() {
    let (base, chain) = chain_base_v1("follow-identity");
    let fresh_chain = chain.with_file_name("chain-fresh");
    let _ = fs::remove_dir_all(&chain);
    let _ = fs::remove_dir_all(&fresh_chain);
    let first = segment_v1(&base, &chain, 0, None);
    let fresh_first = fresh_run_v1(&first).unwrap();
    let second = segment_v1(&base, &chain, 1, Some(checkpoint_pin_v1(&fresh_first)));
    let fresh_second = fresh_run_v1(&second).unwrap();
    fs::rename(&chain, &fresh_chain).unwrap();

    let spool = chain.join("spool");
    let (ended, results) = followed_v1(&base, &chain, 0, 2, &spool, &mut |_| {}).unwrap();
    assert_eq!(ended, FollowEndV1::Stopped);
    assert_eq!(results.len(), 2);
    for (followed, fresh) in results.iter().zip([&fresh_first, &fresh_second]) {
        assert_eq!(followed["actual_identity"], fresh["actual_identity"]);
        assert_eq!(followed["source"], fresh["source"]);
        assert_eq!(receipt_states_v1(followed), receipt_states_v1(fresh));
    }
    for (index, result) in results.iter().enumerate() {
        let done = read_json(&spool.join(done_name_v1(index)), SMALL_CAP).unwrap();
        assert_eq!(done["index"], index);
        assert_eq!(done["source"], result["source"]);
    }
    fs::remove_dir_all(&spool).unwrap();
    assert_trees_match_v1("followed against fresh", &chain, &fresh_chain, true);

    // A refused next config: nothing of it runs.
    fs::remove_dir_all(&chain).unwrap();
    let mut changed = base.clone();
    changed.learning_rate *= 2.0;
    fs::create_dir_all(&spool).unwrap();
    let handle = SpoolV1::open_v1(&spool).unwrap();
    let mut handed = false;
    let error = follow_v1(
        first.clone(),
        |_| {
            if handed {
                return Ok(FollowNextV1::Stop);
            }
            handed = true;
            Ok(FollowNextV1::Config(Box::new(segment_v1(
                &changed,
                &chain,
                1,
                Some(checkpoint_pin_v1(&fresh_first)),
            ))))
        },
        |index, config, result| handle.publish_done_v1(index, config, result),
    )
    .unwrap_err();
    assert!(matches!(error, FollowErrorV1::Refused(_)), "{error:?}");
    assert!(
        !second.output_directory.exists(),
        "a refused config created its run"
    );
    fs::remove_dir_all(&chain).unwrap();
    fs::remove_dir_all(&fresh_chain).unwrap();
}

/// The followed-run counterpart of
/// `a_crash_after_the_checkpoint_resumes_in_a_new_attempt_with_the_same_states`:
/// a crash after the first checkpoint of the second segment of a followed
/// process, then a new process on that segment, reaches the uninterrupted
/// chain's states.
#[test]
fn a_crash_in_a_followed_segment_resumes_in_a_new_process_with_the_same_states() {
    let (base, chain) = chain_base_v1("follow-crash-point");
    let _ = fs::remove_dir_all(&chain);
    let first = segment_v1(&base, &chain, 0, None);
    let reference_first = fresh_run_v1(&first).unwrap();
    let second = segment_v1(&base, &chain, 1, Some(checkpoint_pin_v1(&reference_first)));
    let reference_second = fresh_run_v1(&second).unwrap();
    fs::remove_dir_all(&chain).unwrap();
    let spool = chain.join("spool");
    let error = followed_v1(&base, &chain, 0, 2, &spool, &mut |_| {
        crate::expanded_deck_training_v1::arm_crash_point_for_test_v1()
    })
    .unwrap_err();
    match error {
        FollowErrorV1::Run(message) => assert!(message.contains("crash point"), "{message}"),
        other => panic!("{other:?}"),
    }
    assert!(first.output_directory.join("completion.json").exists());
    assert!(!second.output_directory.join("completion.json").exists());
    assert!(spool.join(done_name_v1(0)).exists());
    assert!(!spool.join(done_name_v1(1)).exists());
    let resumed = fresh_run_v1(&second).unwrap();
    assert_eq!(
        resumed["actual_identity"],
        reference_second["actual_identity"]
    );
    assert_eq!(
        receipt_states_v1(&resumed),
        receipt_states_v1(&reference_second)
    );
    fs::remove_dir_all(&chain).unwrap();
}

/// The followed-run counterpart of
/// `a_crash_at_any_publication_resumes_to_the_uninterrupted_result`: a crash
/// just before or just after any publication of either segment of a followed
/// process (the done records included) leaves only files the uninterrupted
/// followed chain publishes, with the same bytes apart from timing telemetry,
/// plus staging debris (and that chain's tree equals two fresh processes'
/// tree in the same way:
/// `a_followed_chain_publishes_the_trees_and_states_of_fresh_processes`); and a
/// new followed process from the first segment without completion.json, as
/// the driver starts one, reaches the uninterrupted chain's final states.
/// Every case runs in the same directories, so documents that name them
/// compare equal.
#[test]
#[ignore = "slow: one crashed followed chain and its resume per crash point; run before shipping a follow change"]
fn a_crash_at_any_publication_of_a_followed_chain_resumes_to_the_uninterrupted_result() {
    use crate::durable_publication_v1::crash_points_for_test_v1::{
        with_crash_point_v1, CrashPointV1,
    };
    let (base, chain) = chain_base_v1("follow-crash-points");
    let reference_chain = chain.with_file_name("chain-uninterrupted");
    let _ = fs::remove_dir_all(&chain);
    let _ = fs::remove_dir_all(&reference_chain);
    let spool = chain.join("spool");
    let ((ended, reference), publications, crashed) = with_crash_point_v1(&chain, None, || {
        followed_v1(&base, &chain, 0, 2, &spool, &mut |_| {}).unwrap()
    });
    assert!(!crashed);
    assert_eq!(ended, FollowEndV1::Stopped);
    assert_eq!(reference.len(), 2);
    fs::rename(&chain, &reference_chain).unwrap();
    // Per segment: two iterations of ten publications, its plan and
    // completion; and one done record.
    assert!(publications >= 2 * 23, "{publications} publications");
    for index in 0..publications {
        for point in [CrashPointV1::BeforeFinal, CrashPointV1::AfterFinal] {
            let case = format!("publication {index} {point:?}");
            let (crashed_run, _, crashed) =
                with_crash_point_v1(&chain, Some((index, point)), || {
                    followed_v1(&base, &chain, 0, 2, &spool, &mut |_| {})
                });
            assert!(crashed, "{case}: never reached");
            match crashed_run {
                Err(FollowErrorV1::Run(error)) => {
                    assert!(error.contains("injected crash"), "{case}: {error}")
                }
                other => panic!("{case}: {:?}", other.map(|(ended, _)| ended)),
            }
            assert_trees_match_v1(&case, &chain, &reference_chain, false);
            // The driver's restart: a new followed process from the first
            // segment without completion.json (the last one when both have
            // one: a crash at the last done record).
            let unfinished = (0..2)
                .find(|segment| {
                    !chain
                        .join(format!("seg-{segment:05}"))
                        .join("completion.json")
                        .exists()
                })
                .unwrap_or(1);
            fs::remove_dir_all(&spool).unwrap();
            let (ended, resumed) = followed_v1(&base, &chain, unfinished, 2, &spool, &mut |_| {})
                .unwrap_or_else(|error| panic!("{case}: resume failed: {error:?}"));
            assert_eq!(ended, FollowEndV1::Stopped, "{case}");
            let last = resumed.last().unwrap();
            for field in ["state_sha256", "adam_step"] {
                assert_eq!(
                    last["actual_identity"][field], reference[1]["actual_identity"][field],
                    "{case}: {field}"
                );
            }
            fs::remove_dir_all(&chain).unwrap();
        }
    }
    fs::remove_dir_all(&reference_chain).unwrap();
}
